use gtk4::prelude::*;
use gtk4::{ScrolledWindow, TextBuffer, TextTag, TextView, WrapMode};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

#[derive(Clone)]
pub struct MarkdownView {
    pub scroll: ScrolledWindow,
    view: TextView,
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

        setup_tags(&view.buffer());

        let scroll = ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .build();
        scroll.set_child(Some(&view));

        Self { scroll, view }
    }

    pub fn render(&self, markdown: &str) {
        let buf = self.view.buffer();
        buf.set_text("");

        let opts = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
        let parser = Parser::new_ext(markdown, opts);

        let mut ctx = RenderCtx {
            buf: &buf,
            active_tags: Vec::new(),
            list_depth: 0,
            ordered_counter: Vec::new(),
            in_code_block: false,
        };

        for event in parser {
            ctx.handle(event);
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
    int_tag(buf, "bold",  "weight", 700_i32);
    bool_tag(buf, "strike", "strikethrough", true);
    // pango::Style must be passed as the enum, not as a raw gint.
    {
        let tag = TextTag::new(Some("italic"));
        tag.set_property("style", gtk4::pango::Style::Italic);
        buf.tag_table().add(&tag);
    }

    str_tag(buf, "code_inline", "family",     "Monospace");
    str_tag(buf, "code_block",  "family",     "Monospace");
    str_tag(buf, "blockquote",  "foreground", "#888888");
    str_tag(buf, "link",        "foreground", "#8be9fd");
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

// ── Render context ────────────────────────────────────────────────────────

struct RenderCtx<'a> {
    buf: &'a TextBuffer,
    active_tags: Vec<String>,
    list_depth: usize,
    ordered_counter: Vec<u64>,
    in_code_block: bool,
}

impl<'a> RenderCtx<'a> {
    fn insert(&self, text: &str) {
        let mut iter = self.buf.end_iter();
        let tag_names: Vec<&str> = self.active_tags.iter().map(|s| s.as_str()).collect();
        if tag_names.is_empty() {
            self.buf.insert(&mut iter, text);
        } else {
            self.buf.insert_with_tags_by_name(&mut iter, text, &tag_names);
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
                if self.in_code_block {
                    self.insert(&text);
                } else {
                    self.insert(&text);
                }
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
                let _ = dest_url; // URL not displayed inline
            }
            Tag::CodeBlock(_) => {
                self.in_code_block = true;
                self.push("code_block");
                self.insert("\n");
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
                let indent = "  ".repeat(self.list_depth - 1);
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
            Tag::Table(_) | Tag::TableHead | Tag::TableRow | Tag::TableCell => {}
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
                self.insert("\n\n");
                self.pop();
                self.in_code_block = false;
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
            TagEnd::Table | TagEnd::TableHead | TagEnd::TableRow | TagEnd::TableCell => {}
            _ => {}
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
