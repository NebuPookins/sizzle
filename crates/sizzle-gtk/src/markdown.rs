use std::borrow::Cow;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::SystemTime;

use gtk4::gdk::Display;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Button, Entry, EventControllerKey, Label, Orientation, Overlay};
use gtk4::{ScrolledWindow, TextBuffer, TextTag, TextView, WrapMode};
use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use sizzle_core;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use unicode_width::UnicodeWidthStr;

pub struct CodeHighlighter {
    syntax_set: SyntaxSet,
    theme: Theme,
}

impl CodeHighlighter {
    pub fn new() -> Self {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let ts = ThemeSet::load_defaults();
        let theme = ts
            .themes
            .get("base16-ocean.dark")
            .or_else(|| ts.themes.get("Solarized (dark)"))
            .cloned()
            .unwrap_or_else(|| ts.themes.values().next().unwrap().clone());

        Self { syntax_set, theme }
    }

    pub fn highlight(&self, lang: &str, code: &str) -> Vec<Vec<(syntect::highlighting::Color, String)>> {
        let lang_clean = lang.trim().split_whitespace().next().unwrap_or("");
        let syntax = self
            .syntax_set
            .find_syntax_by_token(lang_clean)
            .or_else(|| self.syntax_set.find_syntax_by_extension(lang_clean))
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let mut highlighter = HighlightLines::new(syntax, &self.theme);
        let mut lines_output = Vec::new();

        let cleaned_code = code.strip_suffix('\n').unwrap_or(code);
        for line in cleaned_code.split('\n') {
            let line_with_nl = format!("{}\n", line);
            if let Ok(ranges) = highlighter.highlight_line(&line_with_nl, &self.syntax_set) {
                let line_tokens = ranges
                    .into_iter()
                    .map(|(style, text)| {
                        let t = if text.ends_with('\n') {
                            &text[..text.len() - 1]
                        } else {
                            text
                        };
                        (style.foreground, t.to_string())
                    })
                    .filter(|(_, t)| !t.is_empty())
                    .collect();
                lines_output.push(line_tokens);
            } else {
                lines_output.push(vec![(
                    syntect::highlighting::Color {
                        r: 212,
                        g: 212,
                        b: 212,
                        a: 255,
                    },
                    line.to_string(),
                )]);
            }
        }
        lines_output
    }
}

#[derive(Default)]
pub struct SearchState {
    matches: Vec<(i32, i32)>, // (start_offset, end_offset)
    active_index: Option<usize>,
}

#[derive(Clone)]
pub struct MarkdownView {
    pub container: Overlay,
    scroll: ScrolledWindow,
    view: TextView,
    source: Rc<RefCell<String>>,
    /// (path, last-known mtime). `None` mtime means "never synced" so the
    /// next `check_and_reload` will always re-read.
    file_state: Rc<RefCell<Option<(String, Option<SystemTime>)>>>,
    highlighter: Rc<CodeHighlighter>,
    search_box: GtkBox,
    search_entry: Entry,
    search_count_lbl: Label,
    search_state: Rc<RefCell<SearchState>>,
}

impl MarkdownView {
    pub fn new() -> Self {
        let view = TextView::new();
        view.set_editable(false);
        view.set_cursor_visible(false);
        view.set_wrap_mode(WrapMode::Word);
        view.set_top_margin(12);
        view.set_bottom_margin(12);
        view.set_left_margin(16);
        view.set_right_margin(16);

        // Dark theme with a scoped CSS class
        view.add_css_class("sizzle-md-view");
        if let Some(display) = Display::default() {
            let provider = gtk4::CssProvider::new();
            provider.load_from_data(
                ".sizzle-md-view { background: #1e1e1e; color: #d4d4d4; }
                 .sizzle-md-view text { background: #1e1e1e; color: #d4d4d4; }
                 .sizzle-md-view text selection { background: #264f78; }
                 .sizzle-md-search {
                     background-color: #252526;
                     border: 1px solid #454545;
                     border-radius: 6px;
                     padding: 4px 8px;
                     margin-top: 8px;
                     margin-end: 16px;
                     box-shadow: 0 4px 8px rgba(0, 0, 0, 0.4);
                 }
                 .sizzle-md-search entry {
                     background-color: #1e1e1e;
                     color: #d4d4d4;
                     border: 1px solid #3c3c3c;
                     border-radius: 4px;
                     min-height: 24px;
                     padding: 2px 6px;
                 }
                 .sizzle-md-search entry:focus {
                     border-color: #007acc;
                 }
                 .sizzle-md-search label {
                     color: #cccccc;
                     font-size: 12px;
                 }
                 .sizzle-md-search button {
                     background-color: transparent;
                     color: #cccccc;
                     border: none;
                     border-radius: 4px;
                     min-width: 24px;
                     min-height: 24px;
                     padding: 0;
                 }
                 .sizzle-md-search button:hover {
                     background-color: #333333;
                     color: #ffffff;
                 }",
            );
            gtk4::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }

        view.set_pixels_above_lines(2);
        view.set_pixels_below_lines(2);

        setup_tags(&view.buffer());

        let scroll = ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .build();
        scroll.set_child(Some(&view));

        // Search Overlay UI construction
        let search_entry = Entry::builder()
            .placeholder_text("Find…")
            .width_chars(15)
            .build();

        let search_count_lbl = Label::builder()
            .label("")
            .margin_start(6)
            .margin_end(6)
            .build();

        let prev_btn = Button::builder()
            .label("▲")
            .tooltip_text("Previous Match (Shift+Enter)")
            .build();

        let next_btn = Button::builder()
            .label("▼")
            .tooltip_text("Next Match (Enter)")
            .build();

        let close_btn = Button::builder()
            .label("✕")
            .tooltip_text("Close (Escape)")
            .build();

        let search_box = GtkBox::new(Orientation::Horizontal, 4);
        search_box.add_css_class("sizzle-md-search");
        search_box.set_halign(gtk4::Align::End);
        search_box.set_valign(gtk4::Align::Start);
        search_box.set_visible(false);

        search_box.append(&search_entry);
        search_box.append(&search_count_lbl);
        search_box.append(&prev_btn);
        search_box.append(&next_btn);
        search_box.append(&close_btn);

        let container = Overlay::builder()
            .hexpand(true)
            .vexpand(true)
            .child(&scroll)
            .build();
        container.add_overlay(&search_box);

        let search_state = Rc::new(RefCell::new(SearchState::default()));

        let mv = Self {
            container,
            scroll,
            view,
            source: Rc::new(RefCell::new(String::new())),
            file_state: Rc::new(RefCell::new(None)),
            highlighter: Rc::new(CodeHighlighter::new()),
            search_box,
            search_entry,
            search_count_lbl,
            search_state,
        };

        mv.setup_search_handlers(&prev_btn, &next_btn, &close_btn);
        mv
    }

    fn setup_search_handlers(&self, prev_btn: &Button, next_btn: &Button, close_btn: &Button) {
        let mv = self.clone();
        self.search_entry.connect_changed(move |_| {
            mv.perform_search();
        });

        let mv = self.clone();
        prev_btn.connect_clicked(move |_| {
            mv.previous_match();
        });

        let mv = self.clone();
        next_btn.connect_clicked(move |_| {
            mv.next_match();
        });

        let mv = self.clone();
        close_btn.connect_clicked(move |_| {
            mv.close_search();
        });

        // Key controller on search Entry
        let entry_key = EventControllerKey::new();
        let mv = self.clone();
        entry_key.connect_key_pressed(move |_, keyval, _, state| {
            if keyval == gtk4::gdk::Key::Escape {
                mv.close_search();
                return glib::Propagation::Stop;
            }
            if keyval == gtk4::gdk::Key::Return || keyval == gtk4::gdk::Key::KP_Enter {
                if state.contains(gtk4::gdk::ModifierType::SHIFT_MASK) {
                    mv.previous_match();
                } else {
                    mv.next_match();
                }
                return glib::Propagation::Stop;
            }
            if keyval == gtk4::gdk::Key::f && state.contains(gtk4::gdk::ModifierType::CONTROL_MASK) {
                mv.search_entry.grab_focus();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        self.search_entry.add_controller(entry_key);

        // Key controller on TextView
        let view_key = EventControllerKey::new();
        let mv = self.clone();
        view_key.connect_key_pressed(move |_, keyval, _, state| {
            if keyval == gtk4::gdk::Key::f && state.contains(gtk4::gdk::ModifierType::CONTROL_MASK) {
                mv.open_search();
                return glib::Propagation::Stop;
            }
            if keyval == gtk4::gdk::Key::Escape {
                if mv.search_box.is_visible() {
                    mv.close_search();
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
        self.view.add_controller(view_key);
    }

    /// Clear all search highlights from the text buffer.
    pub fn clear_search_highlights(&self) {
        let buf = self.view.buffer();
        buf.remove_tag_by_name("search_highlight_all", &buf.start_iter(), &buf.end_iter());
        buf.remove_tag_by_name("search_highlight_active", &buf.start_iter(), &buf.end_iter());
    }

    /// Open or re-focus the search bar overlay.
    pub fn open_search(&self) {
        self.search_box.set_visible(true);
        self.search_entry.grab_focus();
        if !self.search_entry.text().is_empty() {
            self.perform_search();
        }
    }

    /// Close the search bar overlay and clear highlights.
    pub fn close_search(&self) {
        self.search_box.set_visible(false);
        self.clear_search_highlights();
        *self.search_state.borrow_mut() = SearchState::default();
        self.search_count_lbl.set_text("");
        self.view.grab_focus();
    }

    /// Perform a search based on current search entry query.
    pub fn perform_search(&self) {
        let query = self.search_entry.text().to_string();
        self.clear_search_highlights();

        if query.is_empty() {
            self.search_count_lbl.set_text("");
            *self.search_state.borrow_mut() = SearchState::default();
            return;
        }

        let buf = self.view.buffer();
        let text = buf.text(&buf.start_iter(), &buf.end_iter(), false);
        let matches = find_substring_matches(&text, &query);

        if matches.is_empty() {
            self.search_count_lbl.set_text("No results");
            *self.search_state.borrow_mut() = SearchState::default();
            return;
        }

        for &(start, end) in &matches {
            let start_iter = buf.iter_at_offset(start);
            let end_iter = buf.iter_at_offset(end);
            buf.apply_tag_by_name("search_highlight_all", &start_iter, &end_iter);
        }

        let active_idx = self.determine_active_match_on_type(&matches);

        *self.search_state.borrow_mut() = SearchState {
            matches,
            active_index: Some(active_idx),
        };

        self.update_active_match_highlight();
    }

    fn determine_active_match_on_type(&self, matches: &[(i32, i32)]) -> usize {
        let (v_start, v_end) = self.get_visible_offset_range();

        // If any match is inside the visible viewport, keep the viewport stationary and pick the first visible match.
        if let Some(idx) = matches.iter().position(|&(start, end)| {
            (start >= v_start && start <= v_end) || (end >= v_start && end <= v_end) || (start <= v_start && end >= v_end)
        }) {
            return idx;
        }

        // Otherwise pick the first match at or below the viewport top, falling back to the first match overall.
        let idx = matches.iter().position(|&(start, _)| start >= v_start).unwrap_or(0);
        self.scroll_to_offset(matches[idx].0);
        idx
    }

    fn get_visible_offset_range(&self) -> (i32, i32) {
        let adj = self.scroll.vadjustment();
        let top_y = adj.value() as i32;
        let bottom_y = top_y + adj.page_size() as i32;

        let (top_iter, _) = self.view.line_at_y(top_y);
        let (mut bottom_iter, _) = self.view.line_at_y(bottom_y);

        let top_offset = top_iter.offset();
        let mut bottom_offset = bottom_iter.offset();
        if !bottom_iter.is_end() {
            bottom_iter.forward_to_line_end();
            bottom_offset = bottom_iter.offset();
        }

        (top_offset, bottom_offset)
    }

    fn update_active_match_highlight(&self) {
        let st = self.search_state.borrow();
        let buf = self.view.buffer();
        buf.remove_tag_by_name("search_highlight_active", &buf.start_iter(), &buf.end_iter());

        if let Some(idx) = st.active_index {
            if idx < st.matches.len() {
                let (start, end) = st.matches[idx];
                let start_iter = buf.iter_at_offset(start);
                let end_iter = buf.iter_at_offset(end);
                buf.apply_tag_by_name("search_highlight_active", &start_iter, &end_iter);
                self.search_count_lbl.set_text(&format!("{}/{}", idx + 1, st.matches.len()));
            }
        }
    }

    /// Select the next search match (with wrap-around) and scroll to it.
    pub fn next_match(&self) {
        self.step_match(1);
    }

    /// Select the previous search match (with wrap-around) and scroll to it.
    pub fn previous_match(&self) {
        self.step_match(-1);
    }

    /// Advance the active match by `delta` (wrapping), then refresh the highlight and scroll.
    fn step_match(&self, delta: isize) {
        {
            let mut st = self.search_state.borrow_mut();
            if st.matches.is_empty() {
                return;
            }
            let len = st.matches.len() as isize;
            st.active_index = Some(match st.active_index {
                Some(cur) => (cur as isize + delta).rem_euclid(len) as usize,
                None => {
                    if delta > 0 {
                        0
                    } else {
                        len as usize - 1
                    }
                }
            });
        }

        self.update_active_match_highlight();
        self.scroll_to_active_match();
    }

    fn scroll_to_active_match(&self) {
        let st = self.search_state.borrow();
        if let Some(idx) = st.active_index {
            if idx < st.matches.len() {
                self.scroll_to_offset(st.matches[idx].0);
            }
        }
    }

    /// Scroll the view so the given buffer character offset is visible.
    fn scroll_to_offset(&self, offset: i32) {
        let mut iter = self.view.buffer().iter_at_offset(offset);
        self.view.scroll_to_iter(&mut iter, 0.1, true, 0.0, 0.2);
    }

    /// Render markdown (view mode). Stores the source for later editing.
    pub fn render(&self, markdown: &str) {
        *self.source.borrow_mut() = markdown.to_string();
        self.render_from_source();
    }

    fn render_from_source(&self) {
        let markdown = self.source.borrow().clone();
        let buf = self.view.buffer();
        buf.set_text("");

        let opts = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
        let parser = Parser::new_ext(&markdown, opts);

        let mut ctx = RenderCtx {
            buf: &buf,
            active_tags: Vec::new(),
            list_depth: 0,
            ordered_counter: Vec::new(),
            in_code_block: false,
            code_block_lang: String::new(),
            code_block_buf: String::new(),
            highlighter: &self.highlighter,
            table_state: None,
        };

        for event in parser {
            ctx.handle(event);
        }
    }

    /// Toggle between view (stylized, read-only) and edit (raw text, editable) mode.
    pub fn set_editable(&self, editable: bool) {
        self.view.set_editable(editable);
        self.view.set_cursor_visible(editable);
        if editable {
            let raw = self.source.borrow().clone();
            self.view.buffer().set_text(&raw);
        } else {
            self.render_from_source();
        }
    }

    /// Return the current buffer text (what the user sees / has typed).
    pub fn get_buffer_text(&self) -> String {
        let buf = self.view.buffer();
        let start = buf.start_iter();
        let end = buf.end_iter();
        buf.text(&start, &end, false).to_string()
    }

    pub fn view(&self) -> &TextView {
        &self.view
    }

    /// Update the stored markdown source (e.g. after a successful save).
    pub fn set_source(&self, text: &str) {
        *self.source.borrow_mut() = text.to_string();
    }

    /// Record the file path this view is displaying, so we can check for
    /// external changes on tab-switch.
    pub fn set_file_path(&self, path: &str) {
        let mtime = std::fs::metadata(path)
            .ok()
            .and_then(|m| m.modified().ok());
        *self.file_state.borrow_mut() = Some((path.to_string(), mtime));
    }

    /// Re-read the underlying file if its modification time has changed since
    /// the last render. Does nothing when the view is in edit mode (so we
    /// don't clobber the user's unsaved edits).
    pub fn check_and_reload(&self) {
        let state = self.file_state.borrow().clone();
        let (path, last_mtime) = match state.as_ref() {
            Some(s) => s,
            None => return,
        };
        if self.view.is_editable() {
            return;
        }
        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(_) => {
                *self.file_state.borrow_mut() = None;
                return;
            }
        };
        let mtime = match metadata.modified() {
            Ok(t) => t,
            Err(_) => return,
        };
        if let Some(last) = last_mtime {
            if mtime == *last {
                return;
            }
        }
        match sizzle_core::files::read_markdown_file(path.clone()) {
            Some(content) => {
                self.render(&content);
                *self.file_state.borrow_mut() = Some((path.clone(), Some(mtime)));
            }
            None => {
                *self.file_state.borrow_mut() = None;
            }
        }
    }
}

// ── Tag setup ─────────────────────────────────────────────────────────────

fn setup_tags(buf: &TextBuffer) {
    add_heading(buf, "h1", 1.8);
    add_heading(buf, "h2", 1.5);
    add_heading(buf, "h3", 1.25);
    add_heading(buf, "h4", 1.1);
    add_heading(buf, "h5", 1.0);
    add_heading(buf, "h6", 1.0);

    // weight/style/strikethrough are not string properties.
    int_tag(buf, "bold", "weight", 700_i32);
    bool_tag(buf, "strike", "strikethrough", true);
    // pango::Style must be passed as the enum, not as a raw gint.
    {
        let tag = TextTag::new(Some("italic"));
        tag.set_property("style", gtk4::pango::Style::Italic);
        buf.tag_table().add(&tag);
    }

    multi_str_tag(buf, "code_inline", &[("family", "Monospace"), ("foreground", "#ce9178")]);
    multi_str_tag(buf, "code_block", &[("family", "Monospace"), ("foreground", "#d4d4d4"), ("background", "#2d2d2d")]);
    multi_str_tag(buf, "table_box", &[("family", "Monospace"), ("foreground", "#6e7681"), ("background", "#252526")]);
    {
        let tag = TextTag::new(Some("table_header"));
        tag.set_property("family", "Monospace");
        tag.set_property("weight", 700_i32);
        tag.set_property("foreground", "#569cd6");
        tag.set_property("background", "#2d2d2d");
        buf.tag_table().add(&tag);
    }
    multi_str_tag(buf, "table_cell", &[("family", "Monospace"), ("foreground", "#d4d4d4"), ("background", "#252526")]);
    str_tag(buf, "blockquote", "foreground", "#b0b0b0");
    str_tag(buf, "link", "foreground", "#8be9fd");

    // Search highlights
    multi_str_tag(buf, "search_highlight_all", &[("background", "#615100"), ("foreground", "#ffffff")]);
    multi_str_tag(buf, "search_highlight_active", &[("background", "#d96b00"), ("foreground", "#ffffff")]);
}

fn add_heading(buf: &TextBuffer, name: &str, scale: f64) {
    let tag = TextTag::new(Some(name));
    tag.set_property("scale", scale);
    tag.set_property("weight", 700_i32);
    buf.tag_table().add(&tag);
}

fn int_tag(buf: &TextBuffer, name: &str, prop: &str, val: i32) {
    let tag = TextTag::new(Some(name));
    tag.set_property(prop, val);
    buf.tag_table().add(&tag);
}

fn bool_tag(buf: &TextBuffer, name: &str, prop: &str, val: bool) {
    let tag = TextTag::new(Some(name));
    tag.set_property(prop, val);
    buf.tag_table().add(&tag);
}

fn str_tag(buf: &TextBuffer, name: &str, prop: &str, val: &str) {
    let tag = TextTag::new(Some(name));
    tag.set_property(prop, val);
    buf.tag_table().add(&tag);
}

fn multi_str_tag(buf: &TextBuffer, name: &str, props: &[(&str, &str)]) {
    let tag = TextTag::new(Some(name));
    for (prop, val) in props {
        tag.set_property(prop, val);
    }
    buf.tag_table().add(&tag);
}

// ── Table data structures ──────────────────────────────────────────────────

#[derive(Clone, Debug)]
struct CellSegment {
    text: String,
    tags: Vec<String>,
}

#[derive(Clone, Debug, Default)]
struct TableCellData {
    segments: Vec<CellSegment>,
}

impl TableCellData {
    fn display_width(&self) -> usize {
        self.segments.iter().map(|s| s.text.width()).sum()
    }
}

#[derive(Clone, Debug, Default)]
struct TableRowData {
    cells: Vec<TableCellData>,
}

#[derive(Clone, Debug)]
struct TableBuildingState {
    alignments: Vec<Alignment>,
    headers: Option<TableRowData>,
    rows: Vec<TableRowData>,
    current_row: Option<TableRowData>,
    current_cell: Option<TableCellData>,
    in_head: bool,
}

// ── Render context ────────────────────────────────────────────────────────

struct RenderCtx<'a> {
    buf: &'a TextBuffer,
    active_tags: Vec<String>,
    list_depth: usize,
    ordered_counter: Vec<u64>,
    in_code_block: bool,
    code_block_lang: String,
    code_block_buf: String,
    highlighter: &'a CodeHighlighter,
    table_state: Option<TableBuildingState>,
}

impl<'a> RenderCtx<'a> {
    fn insert(&mut self, text: &str) {
        if let Some(ref mut table) = self.table_state {
            if let Some(ref mut cell) = table.current_cell {
                cell.segments.push(CellSegment {
                    text: text.to_string(),
                    tags: self.active_tags.clone(),
                });
                return;
            }
        }
        if self.in_code_block {
            self.code_block_buf.push_str(text);
            return;
        }
        let mut iter = self.buf.end_iter();
        let tag_names: Vec<&str> = self.active_tags.iter().map(|s| s.as_str()).collect();
        if tag_names.is_empty() {
            self.buf.insert(&mut iter, text);
        } else {
            self.buf.insert_with_tags_by_name(&mut iter, text, &tag_names);
        }
    }

    fn insert_direct(&self, text: &str, tag_names: &[&str]) {
        let mut iter = self.buf.end_iter();
        if tag_names.is_empty() {
            self.buf.insert(&mut iter, text);
        } else {
            self.buf.insert_with_tags_by_name(&mut iter, text, tag_names);
        }
    }

    fn push(&mut self, tag: &str) {
        self.active_tags.push(tag.to_string());
    }

    fn pop(&mut self) {
        self.active_tags.pop();
    }

    fn handle(&mut self, event: Event) {
        match event {
            Event::Start(tag) => self.start_tag(tag),
            Event::End(tag) => self.end_tag(tag),
            Event::Text(text) => {
                self.insert(&text);
            }
            Event::Code(text) => {
                self.push("code_inline");
                self.insert(&text);
                self.pop();
            }
            Event::SoftBreak => self.insert(" "),
            Event::HardBreak => self.insert("\n"),
            Event::Rule => self.insert("\n──────────────────────────────────────\n\n"),
            _ => {}
        }
    }

    fn start_tag(&mut self, tag: Tag) {
        match tag {
            Tag::Heading { level, .. } => {
                self.push(heading_tag(level));
            }
            Tag::Paragraph => {}
            Tag::Strong => self.push("bold"),
            Tag::Emphasis => self.push("italic"),
            Tag::Strikethrough => self.push("strike"),
            Tag::Link { dest_url, .. } => {
                self.push("link");
                let _ = dest_url;
            }
            Tag::CodeBlock(kind) => {
                self.in_code_block = true;
                self.code_block_lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                self.code_block_buf.clear();
                self.push("code_block");
            }
            Tag::BlockQuote(_) => {
                self.push("blockquote");
                self.insert("┃ ");
            }
            Tag::List(start) => {
                self.list_depth += 1;
                if let Some(n) = start {
                    self.ordered_counter.push(n);
                } else {
                    self.ordered_counter.push(0);
                }
            }
            Tag::Item => {
                let indent = " ".repeat(self.list_depth - 1);
                let is_ordered = self.ordered_counter.last().copied().unwrap_or(0) > 0;
                if is_ordered {
                    let n = self.ordered_counter.last_mut().unwrap();
                    let bullet = format!("{}{}. ", indent, n);
                    *n += 1;
                    self.insert(&bullet);
                } else {
                    self.insert(&format!("{}• ", indent));
                }
            }
            Tag::Table(alignments) => {
                self.table_state = Some(TableBuildingState {
                    alignments,
                    headers: None,
                    rows: Vec::new(),
                    current_row: None,
                    current_cell: None,
                    in_head: false,
                });
            }
            Tag::TableHead => {
                if let Some(ref mut table) = self.table_state {
                    table.in_head = true;
                }
            }
            Tag::TableRow => {
                if let Some(ref mut table) = self.table_state {
                    table.current_row = Some(TableRowData::default());
                }
            }
            Tag::TableCell => {
                if let Some(ref mut table) = self.table_state {
                    table.current_cell = Some(TableCellData::default());
                }
            }
            _ => {}
        }
    }

    fn end_tag(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Heading(_) => {
                self.insert("\n\n");
                self.pop();
            }
            TagEnd::Paragraph => self.insert("\n\n"),
            TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough | TagEnd::Link => {
                self.pop();
            }
            TagEnd::CodeBlock => {
                self.in_code_block = false;
                let code_text = std::mem::take(&mut self.code_block_buf);
                let lang = std::mem::take(&mut self.code_block_lang);

                self.insert_direct("\n", &["code_block"]);

                let lines = self.highlighter.highlight(&lang, &code_text);
                for line_tokens in lines {
                    if line_tokens.is_empty() {
                        self.insert_direct("\n", &["code_block"]);
                        continue;
                    }
                    for (color, text) in line_tokens {
                        let tag_name = format!("syn_{:02x}{:02x}{:02x}", color.r, color.g, color.b);
                        if self.buf.tag_table().lookup(&tag_name).is_none() {
                            let tag = TextTag::new(Some(&tag_name));
                            let color_hex = format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b);
                            tag.set_property("foreground", &color_hex);
                            tag.set_property("family", "Monospace");
                            self.buf.tag_table().add(&tag);
                        }
                        self.insert_direct(&text, &["code_block", &tag_name]);
                    }
                    self.insert_direct("\n", &["code_block"]);
                }
                self.insert_direct("\n", &[]);
                self.pop();
            }
            TagEnd::BlockQuote(_) => {
                self.insert("\n\n");
                self.pop();
            }
            TagEnd::List(_) => {
                self.list_depth -= 1;
                self.ordered_counter.pop();
                if self.list_depth == 0 {
                    self.insert("\n");
                }
            }
            TagEnd::Item => self.insert("\n"),
            TagEnd::TableHead => {
                if let Some(ref mut table) = self.table_state {
                    table.in_head = false;
                }
            }
            TagEnd::TableRow => {
                if let Some(ref mut table) = self.table_state {
                    if let Some(row) = table.current_row.take() {
                        if table.in_head {
                            table.headers = Some(row);
                        } else {
                            table.rows.push(row);
                        }
                    }
                }
            }
            TagEnd::TableCell => {
                if let Some(ref mut table) = self.table_state {
                    if let Some(cell) = table.current_cell.take() {
                        if let Some(ref mut row) = table.current_row {
                            row.cells.push(cell);
                        }
                    }
                }
            }
            TagEnd::Table => {
                if let Some(table) = self.table_state.take() {
                    self.render_table(table);
                }
            }
            _ => {}
        }
    }

    fn render_table(&mut self, table: TableBuildingState) {
        let num_cols = table
            .alignments
            .len()
            .max(table.headers.as_ref().map_or(0, |h| h.cells.len()))
            .max(table.rows.iter().map(|r| r.cells.len()).max().unwrap_or(0));

        if num_cols == 0 {
            return;
        }

        let mut col_widths = vec![3usize; num_cols];
        if let Some(ref h) = table.headers {
            for (i, cell) in h.cells.iter().enumerate() {
                if i < num_cols {
                    col_widths[i] = col_widths[i].max(cell.display_width());
                }
            }
        }
        for row in &table.rows {
            for (i, cell) in row.cells.iter().enumerate() {
                if i < num_cols {
                    col_widths[i] = col_widths[i].max(cell.display_width());
                }
            }
        }

        // Top border: ┌───┬───┐
        let mut top_border = String::from("┌");
        for (i, &w) in col_widths.iter().enumerate() {
            top_border.push_str(&"─".repeat(w + 2));
            if i < num_cols - 1 {
                top_border.push('┬');
            } else {
                top_border.push_str("┐\n");
            }
        }
        self.insert_direct(&top_border, &["table_box"]);

        // Header row if present
        if let Some(ref header_row) = table.headers {
            self.render_table_row(header_row, &col_widths, &table.alignments, true);

            // Separator: ├───┼───┤
            let mut sep = String::from("├");
            for (i, &w) in col_widths.iter().enumerate() {
                sep.push_str(&"─".repeat(w + 2));
                if i < num_cols - 1 {
                    sep.push('┼');
                } else {
                    sep.push_str("┤\n");
                }
            }
            self.insert_direct(&sep, &["table_box"]);
        }

        // Body rows
        for row in &table.rows {
            self.render_table_row(row, &col_widths, &table.alignments, false);
        }

        // Bottom border: └───┴───┘
        let mut bot_border = String::from("└");
        for (i, &w) in col_widths.iter().enumerate() {
            bot_border.push_str(&"─".repeat(w + 2));
            if i < num_cols - 1 {
                bot_border.push('┴');
            } else {
                bot_border.push_str("┘\n\n");
            }
        }
        self.insert_direct(&bot_border, &["table_box"]);
    }

    fn render_table_row(
        &mut self,
        row: &TableRowData,
        col_widths: &[usize],
        alignments: &[Alignment],
        is_header: bool,
    ) {
        let cell_bg_tag = if is_header { "table_header" } else { "table_cell" };

        self.insert_direct("│", &["table_box"]);

        for (i, &w) in col_widths.iter().enumerate() {
            let align = alignments.get(i).copied().unwrap_or(Alignment::None);
            let cell = row.cells.get(i);
            let cell_w = cell.map_or(0, |c| c.display_width());
            let pad = w.saturating_sub(cell_w);

            let (left_pad, right_pad) = match align {
                Alignment::Right => (pad, 0),
                Alignment::Center => (pad / 2, pad - (pad / 2)),
                _ => (0, pad),
            };

            // Left padding
            let l_str = format!(" {}", " ".repeat(left_pad));
            self.insert_direct(&l_str, &[cell_bg_tag]);

            // Cell content
            if let Some(c) = cell {
                for seg in &c.segments {
                    let mut seg_tags = vec![cell_bg_tag];
                    let seg_tag_refs: Vec<&str> = seg.tags.iter().map(|s| s.as_str()).collect();
                    seg_tags.extend(seg_tag_refs);
                    self.insert_direct(&seg.text, &seg_tags);
                }
            }

            // Right padding
            let r_str = format!("{} ", " ".repeat(right_pad));
            self.insert_direct(&r_str, &[cell_bg_tag]);

            if i < col_widths.len() - 1 {
                self.insert_direct("│", &["table_box"]);
            } else {
                self.insert_direct("│\n", &["table_box"]);
            }
        }
    }
}

fn heading_tag(level: HeadingLevel) -> &'static str {
    match level {
        HeadingLevel::H1 => "h1",
        HeadingLevel::H2 => "h2",
        HeadingLevel::H3 => "h3",
        HeadingLevel::H4 => "h4",
        HeadingLevel::H5 => "h5",
        HeadingLevel::H6 => "h6",
    }
}

/// Find substring matches with vim-style smart case:
/// If `query` contains zero uppercase characters, search is case-insensitive.
/// If `query` contains at least one uppercase character, search is case-sensitive.
/// Returns character offset pairs `(start_char_offset, end_char_offset)`.
fn find_substring_matches(text: &str, query: &str) -> Vec<(i32, i32)> {
    if query.is_empty() || text.is_empty() {
        return Vec::new();
    }

    let is_case_sensitive = query.chars().any(|c| c.is_uppercase());
    let (haystack, needle): (Cow<'_, str>, Cow<'_, str>) = if is_case_sensitive {
        (Cow::Borrowed(text), Cow::Borrowed(query))
    } else {
        (Cow::Owned(text.to_lowercase()), Cow::Owned(query.to_lowercase()))
    };

    let mut matches = Vec::new();
    let mut search_start_byte = 0;

    while let Some(byte_idx) = haystack[search_start_byte..].find(needle.as_ref()) {
        let abs_start_byte = search_start_byte + byte_idx;
        let abs_end_byte = abs_start_byte + needle.len();

        let start_char_offset = text[..abs_start_byte].chars().count() as i32;
        let end_char_offset = text[..abs_end_byte].chars().count() as i32;

        matches.push((start_char_offset, end_char_offset));

        // `needle` is non-empty (empty `query` returned early), so this advances at least one byte.
        search_start_byte = abs_start_byte + needle.len();
        if search_start_byte >= haystack.len() {
            break;
        }
    }

    matches
}
