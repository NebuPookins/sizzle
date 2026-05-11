// Terminal prototype: GTK4 + alacritty_terminal
//
// Validates: PTY spawn → terminal parsing → Cairo/Pango rendering → keyboard input

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use gtk4::gdk;
use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, DrawingArea, EventControllerKey};

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, Msg};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::tty;
use alacritty_terminal::vte::ansi::{Color, NamedColor};

const COLS: usize = 100;
const ROWS: usize = 40;
const CELL_W: f64 = 8.0;
const CELL_H: f64 = 17.0;
const FONT_PT: f64 = 11.0;

struct TermSize {
    cols: usize,
    rows: usize,
}

impl Dimensions for TermSize {
    fn total_lines(&self) -> usize { self.rows }
    fn screen_lines(&self) -> usize { self.rows }
    fn columns(&self) -> usize { self.cols }
}

#[derive(Clone)]
struct DirtyFlag(Arc<AtomicBool>);

impl EventListener for DirtyFlag {
    fn send_event(&self, _: Event) {
        self.0.store(true, Ordering::Release);
    }
}

fn win_size() -> WindowSize {
    WindowSize {
        num_cols: COLS as u16,
        num_lines: ROWS as u16,
        cell_width: CELL_W as u16,
        cell_height: CELL_H as u16,
    }
}

fn main() {
    env_logger::init();
    let app = Application::builder()
        .application_id("com.sizzle.term-proto")
        .build();
    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Sizzle Term Proto")
        .default_width((COLS as f64 * CELL_W) as i32)
        .default_height((ROWS as f64 * CELL_H) as i32)
        .build();

    let da = DrawingArea::new();
    da.set_hexpand(true);
    da.set_vexpand(true);
    window.set_child(Some(&da));

    // ── Terminal + PTY ────────────────────────────────────────────────────
    let dirty = DirtyFlag(Arc::new(AtomicBool::new(false)));

    let term = Arc::new(FairMutex::new(Term::new(
        Config::default(),
        &TermSize { cols: COLS, rows: ROWS },
        dirty.clone(),
    )));

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
    let pty_opts = tty::Options {
        shell: Some(tty::Shell::new(shell, vec![])),
        working_directory: None,
        drain_on_exit: false,
        env: Default::default(),
    };
    let pty = tty::new(&pty_opts, win_size(), 0).expect("PTY failed");

    let ev_loop = EventLoop::new(term.clone(), dirty.clone(), pty, false, false)
        .expect("EventLoop failed");
    let notifier = ev_loop.channel();
    ev_loop.spawn();

    // ── Draw ─────────────────────────────────────────────────────────────
    let term_draw = term.clone();
    da.set_draw_func(move |_da, cr, _w, _h| {
        let term = term_draw.lock();
        let content = term.renderable_content();
        let colors = content.colors;
        let cursor_pt = content.cursor.point;

        // Clear background
        cr.set_source_rgb(0.08, 0.08, 0.08);
        cr.paint().ok();

        // Pango font
        let pctx = pangocairo::functions::create_context(cr);
        let layout = pango::Layout::new(&pctx);
        let font = pango::FontDescription::from_string(&format!("Monospace {FONT_PT}"));
        layout.set_font_description(Some(&font));

        for ic in content.display_iter {
            let col = ic.point.column.0;
            let row = ic.point.line.0;
            if row < 0 { continue; }
            let (row, col) = (row as usize, col);
            if row >= ROWS || col >= COLS { continue; }

            let x = col as f64 * CELL_W;
            let y = row as f64 * CELL_H;

            let cell = &ic.cell;
            let on_cursor = ic.point == cursor_pt;

            let (br, bg, bb) = if on_cursor {
                (0.85, 0.85, 0.85)
            } else {
                resolve(cell.bg, false, colors)
            };

            cr.set_source_rgb(br, bg, bb);
            cr.rectangle(x, y, CELL_W, CELL_H);
            cr.fill().ok();

            let ch = cell.c;
            if ch == ' ' || ch == '\0' { continue; }

            let (fr, fg, fb) = if on_cursor {
                (0.08, 0.08, 0.08)
            } else {
                resolve(cell.fg, true, colors)
            };

            let flags = cell.flags;
            let bold = flags.contains(alacritty_terminal::term::cell::Flags::BOLD);
            let italic = flags.contains(alacritty_terminal::term::cell::Flags::ITALIC);

            let mut f = font.clone();
            if bold   { f.set_weight(pango::Weight::Bold); }
            if italic { f.set_style(pango::Style::Italic); }
            layout.set_font_description(Some(&f));
            layout.set_text(&ch.to_string());

            cr.set_source_rgb(fr, fg, fb);
            cr.move_to(x, y);
            pangocairo::functions::show_layout(cr, &layout);

            // Reset font
            if bold || italic {
                layout.set_font_description(Some(&font));
            }
        }
    });

    // ── Keyboard ─────────────────────────────────────────────────────────
    let key_notifier = notifier.clone();
    let key_ctrl = EventControllerKey::new();
    key_ctrl.connect_key_pressed(move |_, kv, _, mods| {
        let bytes = key_to_bytes(kv, mods);
        if !bytes.is_empty() {
            let _ = key_notifier.send(Msg::Input(bytes.into()));
        }
        glib::Propagation::Stop
    });
    da.add_controller(key_ctrl);
    da.set_focusable(true);
    da.grab_focus();

    // ── Redraw timer (16 ms ≈ 60 fps) ───────────────────────────────────
    let dirty_flag = dirty.0.clone();
    let da_weak = da.downgrade();
    glib::timeout_add_local(Duration::from_millis(16), move || {
        if dirty_flag.swap(false, Ordering::AcqRel) {
            if let Some(da) = da_weak.upgrade() {
                da.queue_draw();
            }
        }
        glib::ControlFlow::Continue
    });

    window.present();
}

// ── Color resolution ──────────────────────────────────────────────────────

fn resolve(
    color: Color,
    is_fg: bool,
    colors: &alacritty_terminal::term::color::Colors,
) -> (f64, f64, f64) {
    match color {
        Color::Named(nc) => {
            if let Some(rgb) = colors[nc] {
                return rgb_f(rgb);
            }
            named_fb(nc, is_fg)
        }
        Color::Spec(rgb) => rgb_f(rgb),
        Color::Indexed(idx) => {
            if let Some(rgb) = colors[idx as usize] {
                return rgb_f(rgb);
            }
            idx_fb(idx)
        }
    }
}

fn rgb_f(rgb: alacritty_terminal::vte::ansi::Rgb) -> (f64, f64, f64) {
    (rgb.r as f64 / 255.0, rgb.g as f64 / 255.0, rgb.b as f64 / 255.0)
}

fn named_fb(nc: NamedColor, is_fg: bool) -> (f64, f64, f64) {
    match nc {
        NamedColor::Black        => (0.07, 0.07, 0.07),
        NamedColor::Red          => (0.80, 0.12, 0.12),
        NamedColor::Green        => (0.16, 0.70, 0.16),
        NamedColor::Yellow       => (0.85, 0.76, 0.0),
        NamedColor::Blue         => (0.22, 0.44, 0.80),
        NamedColor::Magenta      => (0.76, 0.18, 0.76),
        NamedColor::Cyan         => (0.08, 0.68, 0.74),
        NamedColor::White        => (0.75, 0.75, 0.75),
        NamedColor::BrightBlack  => (0.40, 0.40, 0.40),
        NamedColor::BrightRed    => (1.0,  0.33, 0.33),
        NamedColor::BrightGreen  => (0.33, 0.94, 0.33),
        NamedColor::BrightYellow => (1.0,  1.0,  0.33),
        NamedColor::BrightBlue   => (0.45, 0.62, 1.0),
        NamedColor::BrightMagenta=> (1.0,  0.45, 1.0),
        NamedColor::BrightCyan   => (0.33, 0.90, 1.0),
        NamedColor::BrightWhite  => (1.0,  1.0,  1.0),
        NamedColor::Foreground | NamedColor::BrightForeground | NamedColor::DimForeground =>
            if is_fg { (0.90, 0.90, 0.90) } else { (0.07, 0.07, 0.07) },
        NamedColor::Background =>
            if is_fg { (0.90, 0.90, 0.90) } else { (0.07, 0.07, 0.07) },
        NamedColor::Cursor => (0.90, 0.90, 0.90),
        _ => if is_fg { (0.90, 0.90, 0.90) } else { (0.07, 0.07, 0.07) },
    }
}

fn idx_fb(idx: u8) -> (f64, f64, f64) {
    if idx < 16 {
        named_fb(
            [NamedColor::Black, NamedColor::Red, NamedColor::Green, NamedColor::Yellow,
             NamedColor::Blue, NamedColor::Magenta, NamedColor::Cyan, NamedColor::White,
             NamedColor::BrightBlack, NamedColor::BrightRed, NamedColor::BrightGreen,
             NamedColor::BrightYellow, NamedColor::BrightBlue, NamedColor::BrightMagenta,
             NamedColor::BrightCyan, NamedColor::BrightWhite][idx as usize],
            false,
        )
    } else if idx < 232 {
        let i = idx - 16;
        let b = i % 6; let g = (i / 6) % 6; let r = i / 36;
        let v = |x: u8| if x == 0 { 0.0 } else { (55.0 + x as f64 * 40.0) / 255.0 };
        (v(r), v(g), v(b))
    } else {
        let x = (idx - 232) as f64 / 23.0 * 0.88 + 0.03;
        (x, x, x)
    }
}

// ── Key translation ──────────────────────────────────────────────────────

fn key_to_bytes(kv: gdk::Key, mods: gdk::ModifierType) -> Vec<u8> {
    let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
    let shift = mods.contains(gdk::ModifierType::SHIFT_MASK);

    if ctrl {
        if let Some(c) = kv.to_unicode() {
            let b = c as u8;
            if b.is_ascii_alphabetic() { return vec![b & 0x1f]; }
        }
        match kv {
            gdk::Key::bracketleft  => return vec![0x1b],
            gdk::Key::backslash    => return vec![0x1c],
            gdk::Key::bracketright => return vec![0x1d],
            gdk::Key::grave        => return vec![0x00],
            _ => {}
        }
    }

    match kv {
        gdk::Key::Return | gdk::Key::KP_Enter => vec![b'\r'],
        gdk::Key::BackSpace => vec![0x7f],
        gdk::Key::Tab       => if shift { b"\x1b[Z".to_vec() } else { vec![b'\t'] },
        gdk::Key::Escape    => vec![0x1b],
        gdk::Key::Up        => b"\x1b[A".to_vec(),
        gdk::Key::Down      => b"\x1b[B".to_vec(),
        gdk::Key::Right     => b"\x1b[C".to_vec(),
        gdk::Key::Left      => b"\x1b[D".to_vec(),
        gdk::Key::Home      => b"\x1b[H".to_vec(),
        gdk::Key::End       => b"\x1b[F".to_vec(),
        gdk::Key::Page_Up   => b"\x1b[5~".to_vec(),
        gdk::Key::Page_Down => b"\x1b[6~".to_vec(),
        gdk::Key::Insert    => b"\x1b[2~".to_vec(),
        gdk::Key::Delete    => b"\x1b[3~".to_vec(),
        gdk::Key::F1  => b"\x1bOP".to_vec(),
        gdk::Key::F2  => b"\x1bOQ".to_vec(),
        gdk::Key::F3  => b"\x1bOR".to_vec(),
        gdk::Key::F4  => b"\x1bOS".to_vec(),
        _ => kv.to_unicode().map(|c| {
            let mut buf = [0u8; 4];
            c.encode_utf8(&mut buf).as_bytes().to_vec()
        }).unwrap_or_default(),
    }
}
