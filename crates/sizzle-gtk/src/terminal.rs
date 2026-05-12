use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gtk4::gdk;
use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use gtk4::{DrawingArea, EventControllerFocus, EventControllerKey, EventControllerScroll, EventControllerScrollFlags, GestureClick, GestureDrag, Popover, Scrollbar};

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::tty;
use alacritty_terminal::vte::ansi::{Color, NamedColor};

pub const CELL_W: f64 = 8.0;
pub const CELL_H: f64 = 17.0;
const FONT_PT: f64 = 11.0;
const SEL_BG: (f64, f64, f64) = (0.30, 0.30, 0.65);
const SEL_FG: (f64, f64, f64) = (0.95, 0.95, 1.0);

pub struct TermSize {
    pub cols: usize,
    pub rows: usize,
}

impl Dimensions for TermSize {
    fn total_lines(&self) -> usize { self.rows }
    fn screen_lines(&self) -> usize { self.rows }
    fn columns(&self) -> usize { self.cols }
}

#[derive(Clone)]
pub struct DirtyFlag {
    pub dirty: Arc<AtomicBool>,
    on_exit: Arc<Mutex<Option<Box<dyn Fn() + Send>>>>,
    write_fn: Arc<Mutex<Option<Box<dyn Fn(Vec<u8>) + Send>>>>,
}

impl DirtyFlag {
    pub fn new() -> Self {
        Self {
            dirty: Arc::new(AtomicBool::new(true)),
            on_exit: Arc::new(Mutex::new(None)),
            write_fn: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_write_fn<F: Fn(Vec<u8>) + Send + 'static>(&self, f: F) {
        *self.write_fn.lock().unwrap() = Some(Box::new(f));
    }
}

impl EventListener for DirtyFlag {
    fn send_event(&self, event: Event) {
        self.dirty.store(true, Ordering::Release);
        match event {
            Event::ChildExit(_) => {
                if let Some(cb) = self.on_exit.lock().unwrap().take() {
                    cb();
                }
            }
            Event::PtyWrite(text) => {
                if let Some(ref wf) = *self.write_fn.lock().unwrap() {
                    wf(text.into_bytes());
                }
            }
            _ => {}
        }
    }
}

#[derive(Clone)]
pub struct TerminalWidget {
    pub container: gtk4::Box,
    da: DrawingArea,
    term: Arc<FairMutex<Term<DirtyFlag>>>,
    sender: EventLoopSender,
    dirty: Arc<AtomicBool>,
    cols: Arc<AtomicUsize>,
    rows: Arc<AtomicUsize>,
    on_exit: Arc<Mutex<Option<Box<dyn Fn() + Send>>>>,
    adjustment: gtk4::Adjustment,
    focused: Arc<AtomicBool>,
}

impl TerminalWidget {
    pub fn new(working_dir: Option<&str>, command: Option<String>) -> Self {
        let cols = 80usize;
        let rows = 24usize;

        let dirty_flag = DirtyFlag::new();
        let dirty = dirty_flag.dirty.clone();

        let win_size = WindowSize {
            num_cols: cols as u16,
            num_lines: rows as u16,
            cell_width: CELL_W as u16,
            cell_height: CELL_H as u16,
        };

        let term = Arc::new(FairMutex::new(Term::new(
            Config::default(),
            &TermSize { cols, rows },
            dirty_flag.clone(),
        )));

        let shell_bin = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
        let cmd = command.unwrap_or(shell_bin);

        let pty_opts = tty::Options {
            shell: Some(tty::Shell::new(cmd, vec![])),
            working_directory: working_dir.map(std::path::PathBuf::from),
            drain_on_exit: false,
            env: Default::default(),
        };

        let pty = tty::new(&pty_opts, win_size, 0).expect("PTY failed");
        let ev_loop = EventLoop::new(term.clone(), dirty_flag.clone(), pty, false, false)
            .expect("EventLoop failed");
        let sender = ev_loop.channel();
        dirty_flag.set_write_fn({
            let sender = sender.clone();
            move |bytes| { let _ = sender.send(Msg::Input(bytes.into())); }
        });
        ev_loop.spawn();

        let da = DrawingArea::new();
        da.set_hexpand(true);
        da.set_vexpand(true);
        da.set_focusable(true);

        // Shared adjustment between scrollbar and internal scrolling
        let adjustment = gtk4::Adjustment::new(0.0, 0.0, rows as f64, 1.0, 3.0, rows as f64);

        let scrollbar = Scrollbar::new(gtk4::Orientation::Vertical, Some(&adjustment));
        scrollbar.set_valign(gtk4::Align::Fill);

        let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        container.set_hexpand(true);
        container.set_vexpand(true);
        container.append(&da);
        container.append(&scrollbar);

        let cols_a = Arc::new(AtomicUsize::new(cols));
        let rows_a = Arc::new(AtomicUsize::new(rows));

        let widget = Self { container, da, term, sender, dirty, cols: cols_a, rows: rows_a, on_exit: dirty_flag.on_exit.clone(), adjustment, focused: Arc::new(AtomicBool::new(false)) };
        widget.setup_draw();
        widget.setup_keyboard();
        widget.setup_context_menu();
        widget.setup_selection();
        widget.setup_click_to_focus();
        widget.setup_scroll();
        widget.setup_redraw_timer();
        widget.setup_resize();
        widget
    }

    fn setup_draw(&self) {
        let term_draw = self.term.clone();
        let cols = self.cols.clone();
        let rows = self.rows.clone();
        let focused = self.focused.clone();
        self.da.set_draw_func(move |_da, cr, _w, _h| {
            draw_term(&term_draw, cr, cols.load(Ordering::Relaxed), rows.load(Ordering::Relaxed), focused.load(Ordering::Acquire));
        });
    }

    fn setup_keyboard(&self) {
        let sender = self.sender.clone();
        let term = self.term.clone();
        let adj = self.adjustment.clone();
        let da = self.da.clone();
        let ctrl = EventControllerKey::new();
        ctrl.connect_key_pressed(move |_, kv, _, mods| {
            let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
            let shift = mods.contains(gdk::ModifierType::SHIFT_MASK);

            // Paste
            if kv == gdk::Key::v && ctrl && shift {
                paste_from_clipboard(sender.clone(), term.clone());
                return glib::Propagation::Stop;
            }

            // Copy selection
            if (kv == gdk::Key::c && ctrl && shift) || (kv == gdk::Key::Insert && ctrl) {
                copy_selection(term.clone());
                return glib::Propagation::Stop;
            }

            // Jump to bottom on any key press if scrolled back
            {
                let t = term.lock();
                if t.grid().display_offset() > 0 {
                    let hist = t.grid().total_lines().saturating_sub(t.grid().screen_lines()) as f64;
                    drop(t);
                    term.lock().scroll_display(Scroll::Bottom);
                    adj.set_value(hist);
                    da.queue_draw();
                }
            }

            // Clear selection on any other key press
            {
                let mut t = term.lock();
                if t.selection.is_some() {
                    t.selection = None;
                    da.queue_draw();
                }
            }

            let bytes = key_to_bytes(kv, mods);
            if !bytes.is_empty() {
                let _ = sender.send(Msg::Input(bytes.into()));
            }
            glib::Propagation::Stop
        });
        self.da.add_controller(ctrl);
    }

    fn setup_context_menu(&self) {
        let sender = self.sender.clone();
        let term = self.term.clone();
        let container = self.container.clone();

        let gesture = GestureClick::new();
        gesture.set_button(3);
        gesture.connect_pressed(move |_, _, x, y| {
            let popover = Popover::new();
            popover.set_has_arrow(false);

            let copy_btn = gtk4::Button::with_label("Copy");
            copy_btn.set_has_frame(false);
            copy_btn.add_css_class("menu");
            {
                let t = term.clone();
                let popover = popover.clone();
                copy_btn.connect_clicked(move |_| {
                    copy_selection(t.clone());
                    popover.popdown();
                });
            }

            let paste_btn = gtk4::Button::with_label("Paste");
            paste_btn.set_has_frame(false);
            paste_btn.add_css_class("menu");
            let s = sender.clone();
            let t = term.clone();
            paste_btn.connect_clicked({
                let popover = popover.clone();
                move |_| {
                    paste_from_clipboard(s.clone(), t.clone());
                    popover.popdown();
                }
            });

            let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            vbox.add_css_class("menu");
            vbox.append(&copy_btn);
            vbox.append(&paste_btn);

            popover.set_child(Some(&vbox));
            let rect = gdk::Rectangle::new(x as i32, y as i32, 1, 1);
            popover.set_pointing_to(Some(&rect));
            popover.set_parent(&container);
            popover.popup();
        });
        self.container.add_controller(gesture);
    }

    fn setup_selection(&self) {
        let term = self.term.clone();
        let da = self.da.clone();
        let dirty = self.dirty.clone();

        let start_state: Rc<RefCell<Option<(f64, f64, usize)>>> = Rc::new(RefCell::new(None));

        let drag = GestureDrag::new();
        {
            let t = term.clone();
            let d = da.clone();
            let dirty = dirty.clone();
            let start = start_state.clone();
            drag.connect_drag_begin(move |_, start_x, start_y| {
                let mut term = t.lock();
                let doff = term.grid().display_offset();
                let col = (start_x.max(0.0) / CELL_W) as usize;
                let screen_row = (start_y.max(0.0) / CELL_H) as i32;
                let grid_line = Line(screen_row - doff as i32);
                term.selection = Some(Selection::new(
                    SelectionType::Simple,
                    Point::new(grid_line, Column(col)),
                    Side::Left,
                ));
                *start.borrow_mut() = Some((start_x, start_y, doff));
                dirty.store(true, Ordering::Release);
                d.queue_draw();
            });
        }
        {
            let t = term.clone();
            let d = da.clone();
            let dirty = dirty.clone();
            let start = start_state.clone();
            drag.connect_drag_update(move |_, offset_x, offset_y| {
                let (sx, sy, doff) = match *start.borrow() {
                    Some(v) => v,
                    None => return,
                };
                let cur_x = sx + offset_x;
                let cur_y = sy + offset_y;
                let col = (cur_x.max(0.0) / CELL_W) as usize;
                let screen_row = (cur_y.max(0.0) / CELL_H) as i32;
                let grid_line = Line(screen_row - doff as i32);
                let mut term = t.lock();
                if let Some(ref mut sel) = term.selection {
                    sel.update(Point::new(grid_line, Column(col)), Side::Right);
                    dirty.store(true, Ordering::Release);
                    d.queue_draw();
                }
            });
        }
        {
            let t = term.clone();
            let d = da.clone();
            let dirty = dirty.clone();
            drag.connect_drag_end(move |_, _, _| {
                let mut term = t.lock();
                let should_clear = term.selection.as_ref().map_or(false, |s| s.is_empty());
                if should_clear {
                    term.selection = None;
                }
                dirty.store(true, Ordering::Release);
                d.queue_draw();
            });
        }
        self.da.add_controller(drag);
    }

    fn setup_click_to_focus(&self) {
        let da = self.da.clone();
        let gesture = GestureClick::new();
        gesture.connect_pressed(move |_, _, _, _| {
            da.grab_focus();
        });
        self.da.add_controller(gesture);
    }

    fn setup_scroll(&self) {
        let term = self.term.clone();
        let adj = self.adjustment.clone();


        // Mouse wheel / touchpad scroll
        let scroll_ctrl = EventControllerScroll::new(EventControllerScrollFlags::VERTICAL);
        let t = term.clone();
        let a = adj.clone();
        let da = self.da.clone();
        scroll_ctrl.connect_scroll(move |_ctrl, _dx, dy| {
            let delta = -(dy.round() as i32);
            if delta != 0 {
                // Clear selection on scroll
                t.lock().selection = None;
                t.lock().scroll_display(Scroll::Delta(delta));
                let (doff, history) = {
                    let locked = t.lock();
                    let g = locked.grid();
                    (g.display_offset(), g.total_lines().saturating_sub(g.screen_lines()) as f64)
                };
                // gtk_value = history - display_offset, so
                //   at bottom (doff=0): gtk = history
                //   at top   (doff=hist): gtk = 0
                let gtk_value = (history - doff as f64).max(0.0);
                a.set_value(gtk_value);
                da.queue_draw();
            }
            glib::Propagation::Stop
        });
        self.da.add_controller(scroll_ctrl);

        // Scrollbar drag / click — scroll to absolute position
        let t = term.clone();
        let da_sb = self.da.clone();
        adj.connect_value_changed(move |adj| {
            let gtk_val = adj.value() as usize;
            let (doff, total, screen) = {
                let locked = t.lock();
                let g = locked.grid();
                (g.display_offset(), g.total_lines(), g.screen_lines())
            };
            let hist = total.saturating_sub(screen);
            // doff = history - gtk_value, clamp to valid range
            let target_doff = hist.saturating_sub(gtk_val);
            if target_doff != doff {
                t.lock().selection = None;
                t.lock().scroll_display(Scroll::Bottom);
                t.lock().scroll_display(Scroll::Delta(target_doff as i32));
                da_sb.queue_draw();
            }
        });
    }

    fn setup_redraw_timer(&self) {
        let dirty_flag = self.dirty.clone();
        let da_weak = self.da.downgrade();
        let term = self.term.clone();
        let adj = self.adjustment.clone();
        let rows = self.rows.clone();
        glib::timeout_add_local(Duration::from_millis(16), move || {
            // Read terminal state under lock, then drop before touching GTK
            // (which can fire value-changed → lock the same term = deadlock)
            let (doff, history) = {
                let t = term.lock();
                let total = t.grid().total_lines();
                let screen = t.grid().screen_lines();
                let h = total.saturating_sub(screen) as f64;
                (t.grid().display_offset(), h)
            };
            let rows_f = rows.load(Ordering::Relaxed) as f64;
            // GTK adjustment: value=0 at top, value=upper-page_size at bottom.
            // display_offset=0 (bottom) ↔ gtk_value = history
            // display_offset=history (top) ↔ gtk_value = 0
            let gtk_value = (history - doff as f64).max(0.0);
            let cur_upper = adj.upper();
            let cur_page = adj.page_size();
            let target_upper = history + rows_f;
            if (cur_upper - target_upper).abs() > 0.5 || (cur_page - rows_f).abs() > 0.5 {
                adj.configure(gtk_value, 0.0, target_upper, 1.0, 3.0, rows_f);
            } else {
                adj.set_value(gtk_value);
            }

            if dirty_flag.swap(false, Ordering::AcqRel) {
                if let Some(da) = da_weak.upgrade() {
                    da.queue_draw();
                }
            }
            glib::ControlFlow::Continue
        });
    }

    fn setup_resize(&self) {
        let term = self.term.clone();
        let sender = self.sender.clone();
        let cols = self.cols.clone();
        let rows = self.rows.clone();
        self.da.connect_resize(move |_da, w, h| {
            let new_cols = ((w as f64 / CELL_W) as usize).max(2);
            let new_rows = ((h as f64 / CELL_H) as usize).max(2);
            let old_cols = cols.swap(new_cols, Ordering::Relaxed);
            let old_rows = rows.swap(new_rows, Ordering::Relaxed);
            if new_cols != old_cols || new_rows != old_rows {
                term.lock().resize(TermSize { cols: new_cols, rows: new_rows });
                let _ = sender.send(Msg::Resize(WindowSize {
                    num_cols: new_cols as u16,
                    num_lines: new_rows as u16,
                    cell_width: CELL_W as u16,
                    cell_height: CELL_H as u16,
                }));
            }
        });
    }

    pub fn focus(&self) {
        self.da.grab_focus();
    }

    /// Register a callback that fires (on the alacritty event-loop thread)
    /// when the child process exits. Use `glib::idle_add` to dispatch to the
    /// GTK main thread from within the callback.
    pub fn set_on_exit<F: Fn() + Send + 'static>(&self, cb: F) {
        *self.on_exit.lock().unwrap() = Some(Box::new(cb));
    }

    /// Mark this terminal as focused or unfocused, causing a visual dim when unfocused.
    pub fn set_focused(&self, focused: bool) {
        self.focused.store(focused, Ordering::Release);
        self.da.queue_draw();
    }

    /// Returns whether this terminal currently has focus.
    #[allow(dead_code)]
    pub fn is_focused(&self) -> bool {
        self.focused.load(Ordering::Acquire)
    }

    /// Connect a callback for when this terminal gains focus.
    pub fn connect_focus_in<F: Fn() + 'static>(&self, f: F) {
        let focus_ctrl = EventControllerFocus::new();
        focus_ctrl.connect_enter(move |_| {
            f();
        });
        self.da.add_controller(focus_ctrl);
    }
}

fn draw_term(
    term: &Arc<FairMutex<Term<DirtyFlag>>>,
    cr: &gtk4::cairo::Context,
    cols: usize,
    rows: usize,
    focused: bool,
) {
    let term = term.lock();
    let content = term.renderable_content();
    let colors = content.colors;
    let cursor_pt = content.cursor.point;
    let display_offset = content.display_offset;
    let selection = content.selection;
    let cursor_shape = content.cursor.shape;

    cr.set_source_rgb(0.08, 0.08, 0.08);
    cr.paint().ok();

    let pctx = pangocairo::functions::create_context(cr);
    let layout = pango::Layout::new(&pctx);
    let base_font = pango::FontDescription::from_string(&format!("Monospace {FONT_PT}"));
    layout.set_font_description(Some(&base_font));

    for ic in content.display_iter {
        let col = ic.point.column.0;
        let row = ic.point.line.0;
        // Map grid line to screen row using display_offset
        let screen_row = row + display_offset as i32;
        if screen_row < 0 || screen_row >= rows as i32 { continue; }
        if col >= cols { continue; }

        let x = col as f64 * CELL_W;
        let y = screen_row as f64 * CELL_H;
        let cell = &ic.cell;
        // Hide cursor when scrolled back
        let on_cursor = ic.point == cursor_pt && display_offset == 0;

        let is_selected = !on_cursor && selection.as_ref().map_or(false, |sel| {
            sel.contains_cell(&ic, ic.point, cursor_shape)
        });

        let (br, bg, bb) = if on_cursor { (0.85, 0.85, 0.85) }
            else if is_selected { SEL_BG }
            else { resolve(cell.bg, false, colors) };
        cr.set_source_rgb(br, bg, bb);
        cr.rectangle(x, y, CELL_W, CELL_H);
        cr.fill().ok();

        let ch = cell.c;
        if ch == ' ' || ch == '\0' { continue; }

        let (fr, fg, fb) = if on_cursor { (0.08, 0.08, 0.08) }
            else if is_selected { SEL_FG }
            else { resolve(cell.fg, true, colors) };

        let flags = cell.flags;
        let bold = flags.contains(alacritty_terminal::term::cell::Flags::BOLD);
        let italic = flags.contains(alacritty_terminal::term::cell::Flags::ITALIC);

        let mut f = base_font.clone();
        if bold { f.set_weight(pango::Weight::Bold); }
        if italic { f.set_style(pango::Style::Italic); }
        layout.set_font_description(Some(&f));
        layout.set_text(&ch.to_string());

        cr.set_source_rgb(fr, fg, fb);
        cr.move_to(x, y);
        pangocairo::functions::show_layout(cr, &layout);

        if bold || italic { layout.set_font_description(Some(&base_font)); }
    }

    // Dim unfocused terminals with a semi-transparent overlay
    if !focused {
        let w = cols as f64 * CELL_W;
        let h = rows as f64 * CELL_H;
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.35);
        cr.rectangle(0.0, 0.0, w, h);
        cr.fill().ok();
    }
}

fn resolve(
    color: Color,
    is_fg: bool,
    colors: &alacritty_terminal::term::color::Colors,
) -> (f64, f64, f64) {
    match color {
        Color::Named(nc) => {
            if let Some(rgb) = colors[nc] { return rgb_f(rgb); }
            named_fb(nc, is_fg)
        }
        Color::Spec(rgb) => rgb_f(rgb),
        Color::Indexed(idx) => {
            if let Some(rgb) = colors[idx as usize] { return rgb_f(rgb); }
            idx_fb(idx)
        }
    }
}

fn rgb_f(rgb: alacritty_terminal::vte::ansi::Rgb) -> (f64, f64, f64) {
    (rgb.r as f64 / 255.0, rgb.g as f64 / 255.0, rgb.b as f64 / 255.0)
}

fn named_fb(nc: NamedColor, is_fg: bool) -> (f64, f64, f64) {
    match nc {
        NamedColor::Black         => (0.07, 0.07, 0.07),
        NamedColor::Red           => (0.80, 0.12, 0.12),
        NamedColor::Green         => (0.16, 0.70, 0.16),
        NamedColor::Yellow        => (0.85, 0.76, 0.0),
        NamedColor::Blue          => (0.22, 0.44, 0.80),
        NamedColor::Magenta       => (0.76, 0.18, 0.76),
        NamedColor::Cyan          => (0.08, 0.68, 0.74),
        NamedColor::White         => (0.75, 0.75, 0.75),
        NamedColor::BrightBlack   => (0.40, 0.40, 0.40),
        NamedColor::BrightRed     => (1.0,  0.33, 0.33),
        NamedColor::BrightGreen   => (0.33, 0.94, 0.33),
        NamedColor::BrightYellow  => (1.0,  1.0,  0.33),
        NamedColor::BrightBlue    => (0.45, 0.62, 1.0),
        NamedColor::BrightMagenta => (1.0,  0.45, 1.0),
        NamedColor::BrightCyan    => (0.33, 0.90, 1.0),
        NamedColor::BrightWhite   => (1.0,  1.0,  1.0),
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
        named_fb([
            NamedColor::Black, NamedColor::Red, NamedColor::Green, NamedColor::Yellow,
            NamedColor::Blue, NamedColor::Magenta, NamedColor::Cyan, NamedColor::White,
            NamedColor::BrightBlack, NamedColor::BrightRed, NamedColor::BrightGreen,
            NamedColor::BrightYellow, NamedColor::BrightBlue, NamedColor::BrightMagenta,
            NamedColor::BrightCyan, NamedColor::BrightWhite,
        ][idx as usize], false)
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

pub fn key_to_bytes(kv: gdk::Key, mods: gdk::ModifierType) -> Vec<u8> {
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
        gdk::Key::F5  => b"\x1b[15~".to_vec(),
        gdk::Key::F6  => b"\x1b[17~".to_vec(),
        gdk::Key::F7  => b"\x1b[18~".to_vec(),
        gdk::Key::F8  => b"\x1b[19~".to_vec(),
        gdk::Key::F9  => b"\x1b[20~".to_vec(),
        gdk::Key::F10 => b"\x1b[21~".to_vec(),
        gdk::Key::F11 => b"\x1b[23~".to_vec(),
        gdk::Key::F12 => b"\x1b[24~".to_vec(),
        _ => kv.to_unicode().map(|c| {
            let mut buf = [0u8; 4];
            c.encode_utf8(&mut buf).as_bytes().to_vec()
        }).unwrap_or_default(),
    }
}

fn copy_selection(term: Arc<FairMutex<Term<DirtyFlag>>>) {
    let text = term.lock().selection_to_string();
    if let Some(text) = text {
        if let Some(display) = gdk::Display::default() {
            display.clipboard().set_text(&text);
        }
    }
}

fn paste_from_clipboard(sender: EventLoopSender, term: Arc<FairMutex<Term<DirtyFlag>>>) {
    let display = match gdk::Display::default() {
        Some(d) => d,
        None => return,
    };
    let clipboard = display.clipboard();
    clipboard.read_text_async(None::<&gtk4::gio::Cancellable>, move |result| {
        let text = match result {
            Ok(Some(t)) => t,
            _ => return,
        };
        let use_bracketed = {
            let t = term.lock();
            t.mode().contains(TermMode::BRACKETED_PASTE)
        };
        let mut data = Vec::new();
        if use_bracketed {
            data.extend_from_slice(b"\x1b[200~");
        }
        let text = text.replace("\r\n", "\n").replace("\n", "\r");
        data.extend_from_slice(text.as_bytes());
        if use_bracketed {
            data.extend_from_slice(b"\x1b[201~");
        }
        let _ = sender.send(Msg::Input(data.into()));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    #[test]
    fn pty_write_forwards_to_write_fn() {
        let flag = DirtyFlag::new();
        let written = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
        flag.set_write_fn({
            let w = Arc::clone(&written);
            move |bytes| { w.lock().unwrap().push(bytes); }
        });

        flag.send_event(Event::PtyWrite("hello".to_string()));
        let guard = written.lock().unwrap();
        assert_eq!(guard.len(), 1);
        assert_eq!(guard[0], b"hello");
    }

    #[test]
    fn child_exit_does_not_forward() {
        let flag = DirtyFlag::new();
        // If write_fn is called, the test fails
        flag.set_write_fn(|_| panic!("write_fn must not be called for ChildExit"));
        flag.send_event(Event::ChildExit(std::process::ExitStatus::from_raw(0)));
    }

    #[test]
    fn any_event_sets_dirty_flag() {
        let flag = DirtyFlag::new();
        flag.dirty.store(false, Ordering::Release);
        flag.send_event(Event::PtyWrite("test".to_string()));
        assert!(flag.dirty.load(Ordering::Acquire));
    }

    #[test]
    fn pty_write_without_write_fn_does_not_panic() {
        let flag = DirtyFlag::new();
        // No write_fn set — should be a no-op, not a panic
        flag.send_event(Event::PtyWrite("test".to_string()));
    }
}
