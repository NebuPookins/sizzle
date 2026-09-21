use std::cell::RefCell;
use std::rc::Rc;
use std::time::SystemTime;

use gtk4::gdk::Display;
use gtk4::prelude::*;
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

#[derive(Clone)]
pub struct MarkdownView {
    pub scroll: ScrolledWindow,
    view: TextView,
    source: Rc<RefCell<String>>,
    /// (path, last-known mtime). `None` mtime means "never synced" so the
    /// next `check_and_reload` will always re-read.
    file_state: Rc<RefCell<Option<(String, Option<SystemTime>)>>>,
    highlighter: Rc<CodeHighlighter>,
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
                 .sizzle-md-view text selection { background: #264f78; }",
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

        Self {
            scroll,
            view,
            source: Rc::new(RefCell::new(String::new())),
            file_state: Rc::new(RefCell::new(None)),
            highlighter: Rc::new(CodeHighlighter::new()),
        }
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
