use portable_pty::{ChildKiller, MasterPty, PtySize, native_pty_system, CommandBuilder};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};

const HISTORY_LIMIT: usize = 200_000;
const READ_BUFFER_SIZE: usize = 64 * 1024;
const EMIT_INTERVAL_MS: u64 = 16;
const EMIT_QUEUE_CHUNKS: usize = 4;
const MAX_EMIT_BATCH_BYTES: usize = 64 * 1024;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PtyDataEvent {
    pub id: String,
    pub data: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PtyExitEvent {
    pub id: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PtyCreateResult {
    pub replay: String,
    pub exit_code: Option<i32>,
}

pub(crate) struct PtySession {
    writer: Box<dyn Write + Send>,
    child_killer: Box<dyn ChildKiller + Send>,
    master_pty: Box<dyn MasterPty + Send>,
    history: Arc<Mutex<String>>,
    exit_code: Arc<Mutex<Option<i32>>>,
    attached: Arc<AtomicBool>,
}

pub struct PtyRegistry {
    pub(crate) ptys: HashMap<String, PtySession>,
}

impl PtyRegistry {
    pub fn new() -> Self {
        Self { ptys: HashMap::new() }
    }

    pub fn create(
        &mut self,
        id: &str,
        cwd: &str,
        command: &str,
        args: &[String],
        app_handle: AppHandle,
    ) -> PtyCreateResult {
        if let Some(existing) = self.ptys.get(id) {
            existing.attached.store(true, Ordering::Release);
            let replay = existing.history.lock().unwrap().clone();
            let exit_code = *existing.exit_code.lock().unwrap();
            return PtyCreateResult {
                replay,
                exit_code,
            };
        }

        let pty_system = native_pty_system();

        let pair = match pty_system.openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        }) {
            Ok(p) => p,
            Err(e) => {
                return PtyCreateResult {
                    replay: format!("\r\n\x1b[91m[Failed to create PTY: {}]\x1b[0m\r\n", e),
                    exit_code: Some(1),
                };
            }
        };

        let mut cmd = CommandBuilder::new(command);
        cmd.args(args);
        cmd.cwd(cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");

        let child = match pair.slave.spawn_command(cmd) {
            Ok(c) => c,
            Err(e) => {
                return PtyCreateResult {
                    replay: format!("\r\n\x1b[91m[Failed to spawn: {}]\x1b[0m\r\n", e),
                    exit_code: Some(1),
                };
            }
        };

        let reader = match pair.master.try_clone_reader() {
            Ok(r) => r,
            Err(e) => {
                return PtyCreateResult {
                    replay: format!("\r\n\x1b[91m[Failed to get PTY reader: {}]\x1b[0m\r\n", e),
                    exit_code: Some(1),
                };
            }
        };

        let writer = match pair.master.take_writer() {
            Ok(w) => w,
            Err(e) => {
                return PtyCreateResult {
                    replay: format!("\r\n\x1b[91m[Failed to get PTY writer: {}]\x1b[0m\r\n", e),
                    exit_code: Some(1),
                };
            }
        };

        let history = Arc::new(Mutex::new(String::new()));
        let exit_code = Arc::new(Mutex::new(None));
        let attached = Arc::new(AtomicBool::new(true));

        let session = PtySession {
            writer,
            child_killer: child,
            master_pty: pair.master,
            history: history.clone(),
            exit_code: exit_code.clone(),
            attached: attached.clone(),
        };

        self.ptys.insert(id.to_string(), session);

        // Reader thread
        let id_clone = id.to_string();
        let app_clone = app_handle.clone();

        thread::spawn(move || {
            // Clones for use after catch_unwind
            let exit_app = app_clone.clone();
            let exit_id = id_clone.clone();

            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                move || {
                    let (output_tx, output_rx) = sync_channel::<String>(EMIT_QUEUE_CHUNKS);
                    let emit_app = app_clone.clone();
                    let emit_id = id_clone.clone();
                    let emit_attached = attached.clone();
                    let emitter = thread::spawn(move || {
                        emit_pty_data(emit_app, emit_id, emit_attached, output_rx);
                    });

                    let mut reader = reader;
                    let mut buf = [0u8; READ_BUFFER_SIZE];

                    loop {
                        let n = match reader.read(&mut buf) {
                            Ok(0) => break,
                            Ok(n) => n,
                            Err(_) => break,
                        };

                        let data =
                            String::from_utf8_lossy(&buf[..n]).to_string();

                        {
                            let mut hist = history.lock().unwrap();
                            hist.push_str(&data);
                            trim_front_to(&mut hist, HISTORY_LIMIT);
                        }

                        if !attached.load(Ordering::Acquire) {
                            continue;
                        }

                        if output_tx.send(data).is_err() {
                            break;
                        }
                    }

                    drop(output_tx);
                    let _ = emitter.join();
                },
            ));

            let code = if result.is_err() { -1 } else { 0 };
            *exit_code.lock().unwrap() = Some(code);
            let _ = exit_app.emit(
                "pty:exit",
                PtyExitEvent { id: exit_id, exit_code: code },
            );
        });

        PtyCreateResult {
            replay: String::new(),
            exit_code: None,
        }
    }

    pub fn write(&mut self, id: &str, data: &str) -> Result<(), String> {
        match self.ptys.get_mut(id) {
            Some(session) => session.writer.write_all(data.as_bytes()).map_err(|e| e.to_string()),
            None => Err(format!("PTY {} not found", id)),
        }
    }

    pub fn resize(&mut self, id: &str, cols: u16, rows: u16) -> Result<(), String> {
        match self.ptys.get_mut(id) {
            Some(session) => {
                session.master_pty.resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                }).map_err(|e| e.to_string())
            }
            None => Err(format!("PTY {} not found", id)),
        }
    }

    pub fn detach(&mut self, id: &str) -> Result<(), String> {
        if let Some(session) = self.ptys.get(id) {
            session.attached.store(false, Ordering::Release);
        }
        Ok(())
    }

    pub fn kill(&mut self, id: &str) -> Result<(), String> {
        match self.ptys.remove(id) {
            Some(mut session) => {
                session.child_killer.kill().map_err(|e| e.to_string())?;
                Ok(())
            }
            None => Err(format!("PTY {} not found", id)),
        }
    }
}

fn emit_pty_data(app_handle: AppHandle, id: String, attached: Arc<AtomicBool>, rx: Receiver<String>) {
    let emit_interval = Duration::from_millis(EMIT_INTERVAL_MS);
    let mut pending = String::new();
    let mut dropped_bytes = 0usize;
    let mut next_emit = Instant::now() + emit_interval;

    loop {
        let timeout = next_emit.saturating_duration_since(Instant::now());
        let disconnected = match rx.recv_timeout(timeout) {
            Ok(data) => {
                append_to_emit_buffer(&mut pending, &mut dropped_bytes, &data);
                false
            }
            Err(RecvTimeoutError::Timeout) => false,
            Err(RecvTimeoutError::Disconnected) => true,
        };

        for _ in 0..EMIT_QUEUE_CHUNKS {
            match rx.try_recv() {
                Ok(data) => append_to_emit_buffer(&mut pending, &mut dropped_bytes, &data),
                Err(_) => break,
            }
        }

        if disconnected || Instant::now() >= next_emit {
            if !flush_emit_buffer(&app_handle, &id, &attached, &mut pending, &mut dropped_bytes) {
                break;
            }
            next_emit = Instant::now() + emit_interval;
        }

        if disconnected {
            break;
        }
    }
}

fn append_to_emit_buffer(pending: &mut String, dropped_bytes: &mut usize, data: &str) {
    pending.push_str(data);
    if pending.len() > MAX_EMIT_BATCH_BYTES {
        let before = pending.len();
        trim_front_to(pending, MAX_EMIT_BATCH_BYTES);
        *dropped_bytes += before.saturating_sub(pending.len());
    }
}

fn flush_emit_buffer(
    app_handle: &AppHandle,
    id: &str,
    attached: &AtomicBool,
    pending: &mut String,
    dropped_bytes: &mut usize,
) -> bool {
    if !attached.load(Ordering::Acquire) {
        pending.clear();
        *dropped_bytes = 0;
        return true;
    }

    if pending.is_empty() && *dropped_bytes == 0 {
        return true;
    }

    let mut data = String::new();
    if *dropped_bytes > 0 {
        data.push_str(&format!(
            "\r\n\x1b[33m[sizzle skipped {} bytes of terminal output]\x1b[0m\r\n",
            *dropped_bytes
        ));
        *dropped_bytes = 0;
    }
    data.push_str(pending);
    pending.clear();

    app_handle
        .emit(
            "pty:data",
            PtyDataEvent {
                id: id.to_string(),
                data,
            },
        )
        .is_ok()
}

/// Trims the front of a string so it's at most `limit` bytes,
/// ensuring the split falls on a UTF-8 char boundary to avoid panics.
fn trim_front_to(s: &mut String, limit: usize) {
    if s.len() > limit {
        let remove = s.len() - limit;
        let split_at = s.floor_char_boundary(remove);
        *s = s.split_off(split_at);
    }
}

// Tauri commands

#[tauri::command]
pub fn pty_create(
    app: AppHandle,
    state: State<'_, Mutex<PtyRegistry>>,
    id: String,
    cwd: String,
    command: String,
    args: Vec<String>,
) -> PtyCreateResult {
    let mut registry = state.lock().unwrap();
    registry.create(&id, &cwd, &command, &args, app)
}

#[tauri::command]
pub fn pty_write(
    state: State<'_, Mutex<PtyRegistry>>,
    id: String,
    data: String,
) -> Result<(), String> {
    let mut registry = state.lock().unwrap();
    registry.write(&id, &data)
}

#[tauri::command]
pub fn pty_resize(
    state: State<'_, Mutex<PtyRegistry>>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let mut registry = state.lock().unwrap();
    registry.resize(&id, cols, rows)
}

#[tauri::command]
pub fn pty_detach(
    state: State<'_, Mutex<PtyRegistry>>,
    id: String,
) -> Result<(), String> {
    let mut registry = state.lock().unwrap();
    registry.detach(&id)
}

#[tauri::command]
pub fn pty_kill(
    state: State<'_, Mutex<PtyRegistry>>,
    id: String,
) -> Result<(), String> {
    let mut registry = state.lock().unwrap();
    registry.kill(&id)
}

#[tauri::command]
pub fn pty_list_sessions(
    state: State<'_, Mutex<PtyRegistry>>,
) -> Vec<String> {
    let registry = state.lock().unwrap();
    registry
        .ptys
        .iter()
        .filter(|(_, session)| session.exit_code.lock().unwrap().is_none())
        .map(|(id, _)| id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_front_to_ascii_only() {
        let mut s = "abcdefghij".to_string();
        trim_front_to(&mut s, 5);
        assert_eq!(s, "fghij");
    }

    #[test]
    fn trim_front_to_under_limit() {
        let mut s = "hello".to_string();
        trim_front_to(&mut s, 10);
        assert_eq!(s, "hello");
    }

    #[test]
    fn trim_front_to_empty_string() {
        let mut s = String::new();
        trim_front_to(&mut s, 10);
        assert_eq!(s, "");
    }

    #[test]
    fn trim_front_to_limit_zero() {
        let mut s = "hello".to_string();
        trim_front_to(&mut s, 0);
        assert_eq!(s, "");
    }

    #[test]
    fn trim_front_to_exactly_at_limit() {
        let mut s = "hello".to_string();
        trim_front_to(&mut s, 5);
        assert_eq!(s, "hello");
    }

    #[test]
    fn trim_front_to_multibyte_exact_boundary() {
        // Each € is 3 bytes.
        // "aaaa" (4) + "€€€€€" (15) = 19 bytes
        let mut s = "aaaa€€€€€".to_string();
        // remove = 19 - 15 = 4, byte 4 is start of first € → on boundary
        trim_front_to(&mut s, 15);
        assert_eq!(s, "€€€€€");
    }

    #[test]
    fn trim_front_to_multibyte_in_mid_char() {
        // "aaa" (3) + "€€€€€" (15) = 18 bytes
        let mut s = "aaa€€€€€".to_string();
        // remove = 18 - 14 = 4, byte 4 is inside first € (bytes 3-5)
        // floor_char_boundary(4) = 3, keeps [3..) = "€€€€€" (15 bytes)
        trim_front_to(&mut s, 14);
        assert_eq!(s, "€€€€€");
    }

    #[test]
    fn trim_front_to_4byte_chars() {
        // "a" + 4 × '𐍈' (U+10348, 4 bytes each) = 1 + 16 = 17 bytes
        let mut s = "a\u{10348}\u{10348}\u{10348}\u{10348}".to_string();
        // remove = 17 - 12 = 5, byte 5 is start of second '𐍈' → on boundary
        trim_front_to(&mut s, 12);
        assert_eq!(s.len(), 12);
        // Should keep the last 3 '𐍈' chars
        assert_eq!(s, "\u{10348}\u{10348}\u{10348}");
    }

    #[test]
    fn trim_front_to_4byte_chars_mid() {
        // "a" + 4 × '𐍈' = 1 + 16 = 17 bytes
        let mut s = "a\u{10348}\u{10348}\u{10348}\u{10348}".to_string();
        // remove = 17 - 9 = 8, byte 8 is in third '𐍈' (bytes 5-8)
        // floor_char_boundary(8) = 5, keeps [5..) = last 3 '𐍈' (12 bytes)
        trim_front_to(&mut s, 9);
        assert_eq!(s.len(), 12);
        assert_eq!(s, "\u{10348}\u{10348}\u{10348}");
    }

    #[test]
    fn trim_front_to_mixed_chars_boundary_stress() {
        // Mix of 1, 2, 3, and 4 byte chars to stress the boundary logic.
        // 'a' (1) + '©' (2, U+00A9) + '€' (3) + '𐍈' (4) = 10 bytes
        let mut s = "a©€\u{10348}".to_string();
        // remove = 10 - 5 = 5, byte 5 is in '€' (bytes 3-5)
        // floor_char_boundary(5) = 3, keeps [3..) = "€𐍈" (7 bytes)
        trim_front_to(&mut s, 5);
        assert_eq!(s, "€\u{10348}");
    }

    #[test]
    fn trim_front_to_keeps_newest_data() {
        // Verify we keep the suffix (newest data), not the prefix (oldest)
        // "old_data_new_data" is 17 bytes; remove = 8, byte 8 is '_' (on boundary)
        let mut s = "old_data_new_data".to_string();
        trim_front_to(&mut s, 9);
        assert_eq!(s, "_new_data");
    }

    #[test]
    fn trim_front_to_panic_regression() {
        // Build a string where remove falls in the middle of a multi-byte char.
        // Head: "€€€€" = 4 × 3 = 12 bytes
        // Body: 'a' × (HISTORY_LIMIT - 5) = 199,995 bytes
        // Tail: "€" = 3 bytes
        // Total: 12 + 199,995 + 3 = 200,010
        // remove = 200,010 - 200,000 = 10, which is in the 4th € (bytes 9-11)
        // Before the fix, split_off(10) would panic.
        // With the fix, floor_char_boundary(10) = 9, keeping [9..200010).
        let head = "€€€€".to_string();
        let body = "a".repeat(HISTORY_LIMIT - 5);
        let tail = "€".to_string();
        let mut s = format!("{}{}{}", head, body, tail);
        assert_eq!(s.len(), HISTORY_LIMIT + 10);

        trim_front_to(&mut s, HISTORY_LIMIT);

        // Should not panic, string should be slightly over limit (partial char kept)
        assert!(s.len() > HISTORY_LIMIT);
        assert!(s.len() <= HISTORY_LIMIT + 10);
        assert!(s.starts_with('€'));
        assert!(s.ends_with('€'));
    }

    #[test]
    fn catch_unwind_catches_panic() {
        // Verify that catch_unwind with AssertUnwindSafe catches a panic
        // as expected — this validates the pattern used in the reader thread.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            panic!("simulated panic");
        }));
        assert!(result.is_err());
    }

    #[test]
    fn append_to_emit_buffer_keeps_recent_output_under_cap() {
        let mut pending = "old".repeat(MAX_EMIT_BATCH_BYTES / 3).to_string();
        let mut dropped = 0usize;
        let newest = "new-data";

        append_to_emit_buffer(&mut pending, &mut dropped, newest);

        assert!(pending.len() <= MAX_EMIT_BATCH_BYTES);
        assert!(pending.ends_with(newest));
        assert!(dropped > 0);
    }

    #[test]
    fn append_to_emit_buffer_tracks_multibyte_trims_without_panicking() {
        let mut pending = "€".repeat((MAX_EMIT_BATCH_BYTES / 3) + 10);
        let mut dropped = 0usize;

        append_to_emit_buffer(&mut pending, &mut dropped, "tail");

        assert!(pending.len() <= MAX_EMIT_BATCH_BYTES + 3);
        assert!(pending.ends_with("tail"));
        assert!(pending.is_char_boundary(0));
        assert!(dropped > 0);
    }
}
