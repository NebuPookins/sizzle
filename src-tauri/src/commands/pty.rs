use portable_pty::{ChildKiller, MasterPty, PtySize, native_pty_system, CommandBuilder};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use tauri::{AppHandle, Emitter, State};

const HISTORY_LIMIT: usize = 200_000;

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
    history: String,
    exit_code: Option<i32>,
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
            return PtyCreateResult {
                replay: existing.history.clone(),
                exit_code: existing.exit_code,
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

        let session = PtySession {
            writer,
            child_killer: child,
            master_pty: pair.master,
            history: String::new(),
            exit_code: None,
        };

        self.ptys.insert(id.to_string(), session);

        // Reader thread
        let id_clone = id.to_string();
        let app_clone = app_handle.clone();
        let history = Arc::new(Mutex::new(String::new()));
        let history_clone = history.clone();

        thread::spawn(move || {
            let mut reader = reader;
            let mut buf = [0u8; 4096];

            loop {
                let n = match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(_) => break,
                };

                let data = String::from_utf8_lossy(&buf[..n]).to_string();

                {
                    let mut hist = history_clone.lock().unwrap();
                    hist.push_str(&data);
                    if hist.len() > HISTORY_LIMIT {
                        let remove = hist.len() - HISTORY_LIMIT;
                        *hist = hist.split_off(remove);
                    }
                }

                let payload = PtyDataEvent {
                    id: id_clone.clone(),
                    data,
                };
                if app_clone.emit("pty:data", payload).is_err() {
                    break;
                }
            }

            // Emit exit
            let exit_payload = PtyExitEvent {
                id: id_clone.clone(),
                exit_code: 0,
            };
            let _ = app_clone.emit("pty:exit", exit_payload);
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

    pub fn detach(&mut self, _id: &str) -> Result<(), String> {
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
