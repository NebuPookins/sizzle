//! GTK4 kanban board widget.
//!
//! A horizontally scrollable board with columns of cards grouped by project.
//! Cards can be created, edited, duplicated, deleted, and dragged between columns.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, DropDown, Entry, GestureClick, Label, Popover,
    ScrolledWindow, StringList, TextView,
    CheckButton, Window,
};
use gtk4::gdk::DragAction;

use sizzle_core::{
    kanban::{KanbanBoard, KanbanCard, KanbanColumn},
    MetadataStore, ScannedProject,
};

// ── Public widget ────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct KanbanBoardWidget {
    /// The outermost scroll container.  Add this to your layout.
    pub container: ScrolledWindow,
    columns_box: GtkBox,
    store: Arc<MetadataStore>,
    projects: Rc<RefCell<Vec<ScannedProject>>>,
    board: Rc<RefCell<KanbanBoard>>,
    on_launch_agent: RefCell<Option<Rc<dyn Fn(String, String, String)>>>,
}

impl KanbanBoardWidget {
    pub fn new(
        store: Arc<MetadataStore>,
        projects: Vec<ScannedProject>,
        parent_window: &Window,
    ) -> Self {
        let board = Rc::new(RefCell::new(store.get_kanban_board()));

        let columns_box = GtkBox::new(gtk4::Orientation::Horizontal, 8);
        columns_box.set_margin_start(12);
        columns_box.set_margin_end(12);
        columns_box.set_margin_top(12);
        columns_box.set_margin_bottom(12);

        let container = ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Automatic)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .child(&columns_box)
            .build();
        container.add_css_class("kanban-board");

        let widget = Self {
            container,
            columns_box,
            store,
            projects: Rc::new(RefCell::new(projects)),
            board,
            on_launch_agent: RefCell::new(None),
        };

        widget.refresh_board(&parent_window);
        widget
    }

    /// Update the internal project list (e.g. after a rescan).
    pub fn set_projects(&self, projects: Vec<ScannedProject>, parent_window: &Window) {
        *self.projects.borrow_mut() = projects;
        self.refresh_board(parent_window);
    }

    /// Register a callback for launching an agent from a card.
    /// Parameters: (project_path, working_directory, agent_label).
    pub fn set_on_launch_agent<F: Fn(String, String, String) + 'static>(&self, f: F) {
        *self.on_launch_agent.borrow_mut() = Some(Rc::new(f));
    }

    // ── Board construction ──────────────────────────────────────────────────

    fn refresh_board(&self, parent_window: &Window) {
        // Remove all existing column widgets, unparenting popovers first to
        // avoid GTK warnings about buttons being finalized with popover children.
        while let Some(child) = self.columns_box.first_child() {
            // Walk the widget tree looking for popovers to unparent.
            fn unparent_popovers(w: &gtk4::Widget) {
                if let Some(first) = w.first_child() {
                    let mut c = Some(first.clone());
                    while let Some(child) = c {
                        c = child.next_sibling();
                        if child.is::<Popover>() {
                            child.unparent();
                        } else {
                            unparent_popovers(&child);
                        }
                    }
                }
            }
            unparent_popovers(&child);
            self.columns_box.remove(&child);
        }

        let board = self.board.borrow();
        let columns = board.columns_sorted();

        for col in columns {
            let col_widget = self.build_column_widget(&col, &board, parent_window);
            self.columns_box.append(&col_widget);
        }

        // Add column button at the end.
        let add_col_btn = Button::builder()
            .label("＋ Add Column")
            .has_frame(false)
            .margin_start(4)
            .margin_end(12)
            .valign(gtk4::Align::Start)
            .build();
        add_col_btn.add_css_class("kanban-add-col-btn");

        let widget_self = self.clone();
        let pw = parent_window.clone();
        add_col_btn.connect_clicked(move |_| {
            widget_self.prompt_add_column(&pw);
        });

        self.columns_box.append(&add_col_btn);
    }

    fn build_column_widget(
        &self,
        col: &KanbanColumn,
        board: &KanbanBoard,
        parent_window: &Window,
    ) -> GtkBox {
        let col_id = col.id.clone();
        let cards = board.cards_in_column(&col_id);
        let card_count = cards.len();
        let wip_text = match col.wip_limit {
            Some(limit) => format!("  {}/{}", card_count, limit),
            None => format!("  {}", card_count),
        };

        // ── Column header ───────────────────────────────────────────────────
        let name_lbl = Label::builder()
            .label(&col.name)
            .halign(gtk4::Align::Start)
            .css_classes(["kanban-col-title"])
            .build();

        let wip_lbl = Label::builder()
            .label(&wip_text)
            .css_classes(["kanban-col-wip"])
            .build();
        if let Some(limit) = col.wip_limit {
            if card_count > limit as usize {
                wip_lbl.add_css_class("kanban-wip-over");
            }
        }

        let col_menu_btn = Button::builder()
            .label("⋮")
            .has_frame(false)
            .css_classes(["kanban-col-menu"])
            .build();

        let header = GtkBox::new(gtk4::Orientation::Horizontal, 4);
        header.append(&name_lbl);
        header.append(&wip_lbl);

        let spacer = GtkBox::new(gtk4::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        header.append(&spacer);
        header.append(&col_menu_btn);
        header.set_margin_start(8);
        header.set_margin_end(8);
        header.set_margin_top(8);
        header.set_margin_bottom(4);
        header.add_css_class("kanban-col-header");

        // ── Column settings popover ─────────────────────────────────────────
        self.attach_column_menu(col_menu_btn, &col_id, &col.name, parent_window);

        // ── Card list ───────────────────────────────────────────────────────
        let card_list = GtkBox::new(gtk4::Orientation::Vertical, 4);
        card_list.set_margin_start(6);
        card_list.set_margin_end(6);
        card_list.set_margin_bottom(4);
        card_list.set_hexpand(true);
        card_list.set_vexpand(true);

        // Group cards by project.
        let groups = board.cards_in_column_grouped(&col_id);

        let drop_target = gtk4::DropTarget::new(glib::Type::STRING, DragAction::MOVE);
        let drop_col_id = col_id.clone();
        let drop_self = self.clone();
        let dpw = parent_window.clone();
        drop_target.connect_drop(move |_drop, value, _x, _y| {
            let card_id: Option<String> = value.get().ok().and_then(|s: String| {
                if s.is_empty() { None } else { Some(s) }
            });
            if let Some(cid) = card_id {
                drop_self.move_card_to_column(&cid, &drop_col_id, &dpw);
                return true;
            }
            false
        });
        card_list.add_controller(drop_target);

        for (_project_path, _project_cards) in &groups {
            for card in _project_cards {
                let card_widget = self.build_card_widget(card, parent_window);
                card_list.append(&card_widget);
            }
        }

        let card_scroll = ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .vexpand(true)
            .hexpand(true)
            .min_content_width(220)
            .child(&card_list)
            .build();
        card_scroll.add_css_class("kanban-card-scroll");

        // ── Add card button ─────────────────────────────────────────────────
        let add_btn = Button::builder()
            .label("＋ Add Card")
            .has_frame(false)
            .css_classes(["kanban-add-card-btn"])
            .build();

        let widget_self = self.clone();
        let pw = parent_window.clone();
        let add_col_id = col_id.clone();
        add_btn.connect_clicked(move |_| {
            widget_self.show_card_dialog(None, &add_col_id, &pw);
        });

        // ── Column container ────────────────────────────────────────────────
        let column_box = GtkBox::new(gtk4::Orientation::Vertical, 0);
        column_box.add_css_class("kanban-column");
        column_box.set_size_request(240, -1);
        column_box.append(&header);
        column_box.append(&card_scroll);
        column_box.append(&add_btn);

        column_box
    }

    fn build_card_widget(
        &self,
        card: &KanbanCard,
        parent_window: &Window,
    ) -> GtkBox {
        let card_id = card.id.clone();
        let project_name = self.lookup_project_name(&card.project_path);
        let title = if card.title.is_empty() {
            "(untitled)"
        } else {
            &card.title
        };

        // ── Labels ──────────────────────────────────────────────────────────
        let title_lbl = Label::builder()
            .label(title)
            .halign(gtk4::Align::Start)
            .wrap(true)
            .xalign(0.0)
            .css_classes(["kanban-card-title"])
            .build();

        let meta_parts: Vec<String> = {
            let mut parts = Vec::new();
            if !project_name.is_empty() {
                parts.push(project_name.clone());
            }
            if let Some(ref agent) = card.assigned_agent {
                parts.push(agent.clone());
            }
            parts
        };
        let meta_text = meta_parts.join("  ·  ");
        let meta_lbl = Label::builder()
            .label(&meta_text)
            .halign(gtk4::Align::Start)
            .xalign(0.0)
            .css_classes(["kanban-card-meta"])
            .build();

        let inner = GtkBox::new(gtk4::Orientation::Vertical, 2);
        inner.set_margin_start(6);
        inner.set_margin_end(6);
        inner.set_margin_top(6);
        inner.set_margin_bottom(6);
        inner.append(&title_lbl);
        inner.append(&meta_lbl);

        let card_box = GtkBox::new(gtk4::Orientation::Vertical, 0);
        card_box.add_css_class("kanban-card");
        card_box.append(&inner);

        // ── Block indicator ─────────────────────────────────────────────────
        if let Some(ref agent) = card.assigned_agent {
            if self.board.borrow().is_agent_blocked(agent).is_some() {
                let block_lbl = Label::builder()
                    .label("⏸ blocked")
                    .halign(gtk4::Align::Start)
                    .css_classes(["kanban-card-blocked"])
                    .build();
                inner.append(&block_lbl);
            }
        }

        // ── Drag source ─────────────────────────────────────────────────────
        let drag_source = gtk4::DragSource::new();
        drag_source.set_actions(DragAction::MOVE);
        let dnd_card_id = card_id.clone();
        drag_source.connect_prepare(move |_, _, _| {
            let provider = gdk::ContentProvider::for_value(&dnd_card_id.to_value());
            Some(provider)
        });
        card_box.add_controller(drag_source);

        // ── Drop target for reordering (within column) ──────────────────────
        // We accept drops on individual cards to support inserting before the
        // dropped-on card, but the column-level handler already handles basic
        // column moves. For v1, within-column reorder is handled by the column
        // drop target (cards go to end of column).

        // ── Right-click context menu ────────────────────────────────────────
        self.attach_card_context_menu(&card_box, &card_id, &card.column_id, parent_window);

        card_box
    }

    // ── Card dialog (create / edit) ──────────────────────────────────────────

    fn show_card_dialog(
        &self,
        edit_card_id: Option<&str>,
        default_column_id: &str,
        parent_window: &Window,
    ) {
        let edit_card_id_owned = edit_card_id.map(|s| s.to_string());
        let is_edit = edit_card_id_owned.is_some();
        let existing = edit_card_id_owned.as_deref().and_then(|id| {
            let b = self.board.borrow();
            b.cards.iter().find(|c| c.id == id).cloned()
        });

        let dialog = Window::builder()
            .title(if is_edit { "Edit Card" } else { "New Card" })
            .modal(true)
            .transient_for(parent_window)
            .default_width(420)
            .build();

        // ── Title ───────────────────────────────────────────────────────────
        let title_entry = Entry::builder()
            .placeholder_text("Card title…")
            .build();
        if let Some(ref c) = existing {
            title_entry.set_text(&c.title);
        }
        let title_row = GtkBox::new(gtk4::Orientation::Vertical, 2);
        title_row.set_margin_start(12);
        title_row.set_margin_end(12);
        title_row.set_margin_top(12);
        let title_lbl = Label::builder()
            .label("Title *")
            .halign(gtk4::Align::Start)
            .css_classes(["dialog-field-label"])
            .build();
        let title_req = Label::builder()
            .label("required")
            .halign(gtk4::Align::Start)
            .css_classes(["dialog-field-required"])
            .build();
        let title_header = GtkBox::new(gtk4::Orientation::Horizontal, 4);
        title_header.append(&title_lbl);
        title_header.append(&title_req);
        title_row.append(&title_header);
        title_row.append(&title_entry);

        // ── Description ─────────────────────────────────────────────────────
        let desc_view = TextView::new();
        desc_view.set_wrap_mode(gtk4::WrapMode::Word);
        desc_view.set_top_margin(4);
        desc_view.set_left_margin(6);
        desc_view.set_right_margin(6);
        desc_view.set_vexpand(false);
        desc_view.add_css_class("dialog-description");
        let desc_buf = desc_view.buffer();
        if let Some(ref c) = existing {
            desc_buf.set_text(&c.description);
        }
        // Auto-grow: estimate wrapped lines via character count heuristic.
        {
            let text = desc_buf.text(&desc_buf.start_iter(), &desc_buf.end_iter(), false);
            let n_paras = text.chars().filter(|&c| c == '\n').count().max(1);
            let height = ((n_paras + text.len() / 40).max(3).min(15) * 22) as i32;
            desc_view.set_height_request(height);
        }
        let desc_auto = desc_view.clone();
        desc_buf.connect_changed(move |buf| {
            let t = buf.text(&buf.start_iter(), &buf.end_iter(), false);
            let paras = t.chars().filter(|&c| c == '\n').count().max(1);
            let h = ((paras + t.len() / 40).max(3).min(15) * 22) as i32;
            desc_auto.set_height_request(h);
        });
        let desc_row = GtkBox::new(gtk4::Orientation::Vertical, 2);
        desc_row.set_margin_start(12);
        desc_row.set_margin_end(12);
        desc_row.set_margin_top(8);
        let desc_lbl = Label::builder()
            .label("Description")
            .halign(gtk4::Align::Start)
            .css_classes(["dialog-field-label"])
            .build();
        desc_row.append(&desc_lbl);
        desc_row.append(&desc_view);

        // ── Project (searchable) ────────────────────────────────────────────
        let project_names: Vec<String> = {
            let projs = self.projects.borrow();
            let mut names: Vec<String> = projs.iter().map(|p| p.name.clone()).collect();
            names.sort();
            names
        };
        let project_strings: Vec<&str> = std::iter::once("None")
            .chain(project_names.iter().map(|s| s.as_str()))
            .collect();
        let project_list = StringList::new(&project_strings);
        let project_dropdown = DropDown::builder()
            .model(&project_list)
            .enable_search(true)
            .build();
        project_dropdown.set_expression(Some(&gtk4::PropertyExpression::new(
            gtk4::StringObject::static_type(),
            gtk4::Expression::NONE,
            "string",
        )));
        if let Some(ref c) = existing {
            if let Some(ref pp) = c.project_path {
                if let Some(idx) = project_names.iter().position(|n| {
                    self.projects.borrow().iter().any(|p| p.path == *pp && p.name == *n)
                }) {
                    project_dropdown.set_selected((idx + 1) as u32);
                }
            }
        }
        let proj_row = GtkBox::new(gtk4::Orientation::Vertical, 2);
        proj_row.set_margin_start(12);
        proj_row.set_margin_end(12);
        proj_row.set_margin_top(8);
        let proj_lbl = Label::builder()
            .label("Project")
            .halign(gtk4::Align::Start)
            .css_classes(["dialog-field-label"])
            .build();
        proj_row.append(&proj_lbl);
        proj_row.append(&project_dropdown);

        // ── Agent ───────────────────────────────────────────────────────────
        let agent_names: Vec<String> = {
            let presets = self.store.get_agent_presets();
            let mut names = vec!["None".to_string(), "Claude".to_string(), "Codex".to_string()];
            for p in &presets {
                names.push(p.label.clone());
            }
            names
        };
        let agent_strs: Vec<&str> = agent_names.iter().map(|s| s.as_str()).collect();
        let agent_list = StringList::new(&agent_strs);
        let agent_dropdown = DropDown::builder()
            .model(&agent_list)
            .build();
        if let Some(ref c) = existing {
            if let Some(ref aa) = c.assigned_agent {
                if let Some(idx) = agent_names.iter().position(|n| n == aa) {
                    agent_dropdown.set_selected(idx as u32);
                }
            }
        }
        let agent_row = GtkBox::new(gtk4::Orientation::Vertical, 2);
        agent_row.set_margin_start(12);
        agent_row.set_margin_end(12);
        agent_row.set_margin_top(8);
        let agent_lbl = Label::builder()
            .label("Assigned Agent")
            .halign(gtk4::Align::Start)
            .css_classes(["dialog-field-label"])
            .build();
        agent_row.append(&agent_lbl);
        agent_row.append(&agent_dropdown);

        // ── Git worktree checkbox ───────────────────────────────────────────
        let wt_checkbtn = CheckButton::builder()
            .label("Create git worktree")
            .active(false)
            .build();
        let wt_row = GtkBox::new(gtk4::Orientation::Horizontal, 4);
        wt_row.set_margin_start(12);
        wt_row.set_margin_end(12);
        wt_row.set_margin_top(10);
        wt_row.append(&wt_checkbtn);

        // Worktree checkbox: auto-enabled for new cards, shows path if existing.
        let projs = self.projects.borrow();
        let checked_out = existing.as_ref().and_then(|c| c.worktree_path.as_ref()).is_some();
        wt_checkbtn.set_active(checked_out || existing.is_none());
        if existing.as_ref().and_then(|c| c.worktree_path.as_ref()).is_some() {
            // Worktree already exists, show path.
            if let Some(ref c) = existing {
                if let Some(ref wt) = c.worktree_path {
                    let wt_lbl = Label::builder()
                        .label(&format!("(worktree: {})", wt))
                        .css_classes(["dialog-field-label"])
                        .build();
                    wt_row.append(&wt_lbl);
                }
            }
        }
        drop(projs);

        // ── Buttons ─────────────────────────────────────────────────────────
        let cancel_btn = Button::builder()
            .label("Cancel")
            .css_classes(["dialog-btn"])
            .build();
        let ok_btn = Button::builder()
            .label(if is_edit { "Save" } else { "Create" })
            .css_classes(["dialog-btn", "suggested-action"])
            .sensitive(is_edit) // disabled in create mode until title is entered
            .build();

        // Gray out Create until title is non-empty.
        let ok_btn_state = ok_btn.clone();
        title_entry.connect_changed(move |e| {
            let has_title = !e.text().trim().is_empty();
            ok_btn_state.set_sensitive(has_title);
        });

        let btn_box = GtkBox::new(gtk4::Orientation::Horizontal, 8);
        btn_box.set_halign(gtk4::Align::End);
        btn_box.set_margin_start(12);
        btn_box.set_margin_end(12);
        btn_box.set_margin_top(12);
        btn_box.set_margin_bottom(12);
        btn_box.append(&cancel_btn);
        btn_box.append(&ok_btn);

        // ── Layout ─────────────────────────────────────────────────────────-
        let content = GtkBox::new(gtk4::Orientation::Vertical, 0);
        content.append(&title_row);
        content.append(&desc_row);
        content.append(&proj_row);
        content.append(&agent_row);
        content.append(&wt_row);
        content.append(&btn_box);
        dialog.set_child(Some(&content));

        let dialog_clone = dialog.clone();
        cancel_btn.connect_clicked(move |_| dialog_clone.close());

        let self2 = self.clone();
        let dialog_close = dialog.clone();
        let default_col_id = default_column_id.to_string();
        ok_btn.connect_clicked(move |_| {
            let title = title_entry.text().trim().to_string();
            if title.is_empty() {
                return; // title required
            }

            let description = desc_buf
                .text(&desc_buf.start_iter(), &desc_buf.end_iter(), false)
                .to_string();
            let proj_idx = project_dropdown.selected() as usize;
            let project_path = if proj_idx == 0 {
                None
            } else {
                let projs = self2.projects.borrow();
                let name = project_names.get(proj_idx - 1).cloned();
                name.and_then(|n| projs.iter().find(|p| p.name == n).map(|p| p.path.clone()))
            };

            let agent_idx = agent_dropdown.selected() as usize;
            let assigned_agent = if agent_idx == 0 {
                None
            } else {
                agent_names.get(agent_idx).cloned()
            };

            let create_wt = wt_checkbtn.is_active() && project_path.is_some();
            let worktree_path = if create_wt && project_path.is_some() {
                // Build a worktree path from the project dir and branch name.
                let slug = slugify(&title);
                let base = std::path::Path::new(project_path.as_ref().unwrap());
                let worktree_dir = base.join(".sizzle-worktrees").join(&slug);
                let worktree_dir_str = worktree_dir.to_string_lossy().to_string();

                // Attempt to create the branch and worktree.
                let branch_name = format!("card/{}/{}", slug, &uuid::Uuid::new_v4().to_string()[..8]);
                let repo_dir = project_path.as_ref().unwrap();
                let git_result = std::process::Command::new("git")
                    .args(["-C", repo_dir, "worktree", "add"])
                    .arg(&worktree_dir_str)
                    .arg("-b")
                    .arg(&branch_name)
                    .output();
                match git_result {
                    Ok(out) if out.status.success() => {
                        log::info!("[kanban] Created worktree at {}", worktree_dir_str);
                        Some(worktree_dir_str)
                    }
                    Ok(out) => {
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        log::warn!("[kanban] git worktree add failed: {}", stderr);
                        None
                    }
                    Err(e) => {
                        log::warn!("[kanban] git worktree add error: {}", e);
                        None
                    }
                }
            } else {
                None
            };

            if is_edit {
                if let Some(ref cid) = edit_card_id_owned {
                    let mut board = self2.board.borrow_mut();
                    if let Some(card) = board.get_card_mut(cid) {
                        card.title = title;
                        card.description = description;
                        if let Some(pp) = project_path {
                            card.project_path = Some(pp);
                        }
                        card.assigned_agent = assigned_agent;
                        if let Some(wt) = worktree_path {
                            card.worktree_path = Some(wt);
                        }
                    }
                }
            } else {
                let new_card = KanbanCard {
                    id: uuid::Uuid::new_v4().to_string(),
                    title,
                    description,
                    project_path,
                    assigned_agent,
                    worktree_path,
                    column_id: default_col_id.clone(),
                    position: {
                        let board = self2.board.borrow();
                        let count = board.cards_in_column(&default_col_id).len();
                        count as f64
                    },
                };
                self2.board.borrow_mut().cards.push(new_card);
            }

            self2.save_and_refresh(&dialog_close);
            dialog_close.close();
        });

        dialog.present();
    }

    // ── Context menu for cards ──────────────────────────────────────────────

    fn attach_card_context_menu(
        &self,
        card_widget: &GtkBox,
        card_id: &str,
        column_id: &str,
        parent_window: &Window,
    ) {
        let card_id = card_id.to_string();
        let column_id = column_id.to_string();

        let gesture = GestureClick::new();
        gesture.set_button(3);

        let popover = Popover::builder()
            .has_arrow(false)
            .build();
        let vbox = GtkBox::new(gtk4::Orientation::Vertical, 0);

        // Edit
        let edit_btn = Button::builder()
            .label("Edit")
            .has_frame(false)
            .halign(gtk4::Align::Start)
            .css_classes(["context-menu-item"])
            .build();

        // Duplicate
        let dup_btn = Button::builder()
            .label("Duplicate")
            .has_frame(false)
            .halign(gtk4::Align::Start)
            .css_classes(["context-menu-item"])
            .build();

        // Delete
        let del_btn = Button::builder()
            .label("Delete")
            .has_frame(false)
            .halign(gtk4::Align::Start)
            .css_classes(["context-menu-item"])
            .build();

        vbox.append(&edit_btn);
        vbox.append(&dup_btn);

        // Move to submenu
        let move_lbl = Label::builder()
            .label("Move to column ›")
            .halign(gtk4::Align::Start)
            .margin_start(4)
            .margin_end(4)
            .margin_top(2)
            .margin_bottom(2)
            .css_classes(["context-menu-item"])
            .build();
        vbox.append(&move_lbl);

        // Separator
        vbox.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

        // Run with Agent
        let run_lbl = Label::builder()
            .label("Run with Agent")
            .halign(gtk4::Align::Start)
            .margin_start(4)
            .margin_end(4)
            .margin_top(2)
            .margin_bottom(2)
            .css_classes(["context-menu-item"])
            .build();
        vbox.append(&run_lbl);

        vbox.append(&del_btn);

        popover.set_child(Some(&vbox));
        popover.set_parent(card_widget);

        let popover_close = popover.clone();
        edit_btn.connect_clicked(move |_| {
            popover_close.popdown();
            // Re-show card dialog in edit mode.
            // We need self reference here — using self clones captured earlier.
        });
        // We need to capture self for edit/dup/delete/move callbacks.
        // Let's do this differently — connect these in a block with self clones.

        // We'll rebuild these connections to properly capture self.
        // Actually, the closures need self. Let me use a different approach:
        // capture self_clone and card_id in a block.

        // Drop initial gesture/popover — we need to rewire the connections
        // properly after building the menu. For now, just popdown and show edit.
        // Actually popover is already set up. Let me fix the callbacks.

        // Unfortunately we can't easily fix these closures now since they're already
        // captured in the closure. Let me rebuild the whole function.
        // For now disable the dummy handlers and just popdown.
        drop(edit_btn);
        drop(dup_btn);
        drop(del_btn);

        // Actually, the best approach is to move the popover into a block where we
        // have proper self captures. Let me redo this entire helper.
        // Skip the previous setup and re-implement properly.
        popover.unparent();

        // Re-do the context menu properly.
        let gesture2 = GestureClick::new();
        gesture2.set_button(3);
        let popover2 = Popover::builder()
            .has_arrow(false)
            .build();
        let vbox2 = GtkBox::new(gtk4::Orientation::Vertical, 0);

        let edit_btn2 = Button::builder()
            .label("Edit")
            .has_frame(false)
            .halign(gtk4::Align::Start)
            .css_classes(["context-menu-item"])
            .build();
        let dup_btn2 = Button::builder()
            .label("Duplicate")
            .has_frame(false)
            .halign(gtk4::Align::Start)
            .css_classes(["context-menu-item"])
            .build();
        let run_btn2 = Button::builder()
            .label("Run with Agent")
            .has_frame(false)
            .halign(gtk4::Align::Start)
            .css_classes(["context-menu-item"])
            .build();
        let del_btn2 = Button::builder()
            .label("Delete")
            .has_frame(false)
            .halign(gtk4::Align::Start)
            .css_classes(["context-menu-item"])
            .build();

        vbox2.append(&edit_btn2);
        vbox2.append(&dup_btn2);
        vbox2.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
        vbox2.append(&run_btn2);
        vbox2.append(&del_btn2);

        popover2.set_child(Some(&vbox2));
        popover2.set_parent(card_widget);

        let gesture_close = popover2.clone();
        gesture2.connect_pressed(move |_, _, _, _| {
            gesture_close.popup();
        });
        card_widget.add_controller(gesture2);

        // Edit
        let self_edit = self.clone();
        let cid = card_id.clone();
        let pw = parent_window.clone();
        let pop_close = popover2.clone();
        edit_btn2.connect_clicked(move |_| {
            pop_close.popdown();
            self_edit.show_card_dialog(Some(&cid), &column_id, &pw);
        });

        // Duplicate
        let self_dup = self.clone();
        let cid_dup = card_id.clone();
        let pw_dup = parent_window.clone();
        let pop_close2 = popover2.clone();
        dup_btn2.connect_clicked(move |_| {
            pop_close2.popdown();
            let mut board = self_dup.board.borrow_mut();
            if let Some(card) = board.get_card(&cid_dup).cloned() {
                let new_card = KanbanCard {
                    id: uuid::Uuid::new_v4().to_string(),
                    title: format!("{} (copy)", card.title),
                    position: board.cards_in_column(&card.column_id).len() as f64,
                    ..card
                };
                board.cards.push(new_card);
                drop(board);
                self_dup.save_and_refresh(&pw_dup);
            }
        });

        // Run with Agent
        let self_run = self.clone();
        let cid_run = card_id.clone();
        let _pw_run = parent_window.clone();
        let pop_close_run = popover2.clone();
        run_btn2.connect_clicked(move |_| {
            pop_close_run.popdown();
            let board = self_run.board.borrow();
            if let Some(card) = board.get_card(&cid_run) {
                let proj_path = card.project_path.clone();
                let working_dir = card.worktree_path.clone()
                    .or_else(|| card.project_path.clone());
                log::info!("[kanban] Run with Agent: proj_path={:?}, working_dir={:?}, agent={:?}",
                    proj_path, working_dir, card.assigned_agent);
                if let Some(ref agent) = card.assigned_agent {
                    if let Some(ref cb) = *self_run.on_launch_agent.borrow() {
                        if let (Some(pp), Some(wd)) = (proj_path, working_dir) {
                            cb(pp, wd, agent.clone());
                        }
                    }
                }
            }
        });

        // Delete
        let self_del = self.clone();
        let cid_del = card_id.clone();
        let pw_del = parent_window.clone();
        let pop_close3 = popover2.clone();
        del_btn2.connect_clicked(move |_| {
            pop_close3.popdown();
            let self_del2 = self_del.clone();
            let cid_del2 = cid_del.clone();
            let pw_del2 = pw_del.clone();

            // Look up the card and check worktree status.
            enum WtStatus { NotFound, Present { has_changes: bool, merged: Option<bool> } }
            let worktree_info: Option<(String, WtStatus)> = {
                let board = self_del2.board.borrow();
                board.get_card(&cid_del2).and_then(|card| {
                    card.worktree_path.as_ref().map(|wt| {
                        let p = Path::new(wt);
                        if p.exists() {
                            let has_changes =
                                sizzle_core::git::worktree_has_uncommitted_changes(wt);
                            let merged =
                                sizzle_core::git::is_latest_commit_merged_elsewhere(wt);
                            (wt.clone(), WtStatus::Present { has_changes, merged })
                        } else {
                            (wt.clone(), WtStatus::NotFound)
                        }
                    })
                })
            };

            let confirm_win = Window::builder()
                .title("Delete Card")
                .modal(true)
                .transient_for(&pw_del2)
                .default_width(420)
                .build();

            let vbox2 = GtkBox::new(gtk4::Orientation::Vertical, 8);
            vbox2.set_margin_start(12);
            vbox2.set_margin_end(12);
            vbox2.set_margin_top(12);
            vbox2.set_margin_bottom(12);

            let confirm_label = Label::builder()
                .label("Delete this card?")
                .wrap(true)
                .build();
            vbox2.append(&confirm_label);

            let warn_label = Label::builder()
                .label("This action cannot be undone.")
                .wrap(true)
                .build();
            vbox2.append(&warn_label);

            let remove_worktree_check = CheckButton::builder()
                .label("Also remove the git worktree")
                .active(true)
                .build();

            if let Some((ref wt_path, ref wt_status)) = worktree_info {
                let info_frame = GtkBox::new(gtk4::Orientation::Vertical, 4);
                info_frame.set_margin_top(8);
                info_frame.set_margin_bottom(8);

                match wt_status {
                    WtStatus::NotFound => {
                        let pl = Label::builder()
                            .label(format!("Git worktree (directory not found):\n{}", wt_path))
                            .wrap(true)
                            .xalign(0.0)
                            .build();
                        info_frame.append(&pl);
                    }
                    WtStatus::Present { has_changes, merged } => {
                        let pl = Label::builder()
                            .label(format!("Git worktree:\n{}", wt_path))
                            .wrap(true)
                            .xalign(0.0)
                            .build();
                        info_frame.append(&pl);

                        let cl = Label::builder()
                            .label(if *has_changes {
                                "⚠ Has uncommitted changes"
                            } else {
                                "✓ No uncommitted changes"
                            })
                            .xalign(0.0)
                            .build();
                        info_frame.append(&cl);

                        let ml = Label::builder()
                            .label(match merged {
                                Some(true) =>
                                    "✓ Latest commit is merged into another branch",
                                Some(false) =>
                                    "⚠ Latest commit is NOT merged into any other branch",
                                None =>
                                    "? Could not determine if commits are merged elsewhere",
                            })
                            .xalign(0.0)
                            .wrap(true)
                            .build();
                        info_frame.append(&ml);
                    }
                }
                vbox2.append(&info_frame);

                if matches!(wt_status, WtStatus::Present { .. }) {
                    vbox2.append(&remove_worktree_check);
                }
            } else {
                let no_wt_label = Label::builder()
                    .label("No git worktree associated with this card.")
                    .xalign(0.0)
                    .margin_top(8)
                    .margin_bottom(8)
                    .build();
                vbox2.append(&no_wt_label);
            }

            let yes_btn = Button::builder()
                .label("Delete")
                .css_classes(["suggested-action"])
                .build();
            let no_btn = Button::builder()
                .label("Cancel")
                .build();
            let btn_box2 = GtkBox::new(gtk4::Orientation::Horizontal, 8);
            btn_box2.set_halign(gtk4::Align::End);
            btn_box2.append(&no_btn);
            btn_box2.append(&yes_btn);
            vbox2.append(&btn_box2);

            confirm_win.set_child(Some(&vbox2));

            let confirm_win2 = confirm_win.clone();
            yes_btn.connect_clicked(move |_| {
                // Optionally remove the git worktree if the directory still exists.
                if remove_worktree_check.is_active() {
                    if let Some((ref wt_path, WtStatus::Present { .. })) = worktree_info {
                        let _ = std::process::Command::new("git")
                            .args(["worktree", "remove", "--force"])
                            .arg(wt_path)
                            .output();
                        let _ = std::fs::remove_dir_all(wt_path);
                    }
                }
                self_del2.board.borrow_mut().cards.retain(|c| c.id != cid_del2);
                self_del2.save_and_refresh(&pw_del2);
                confirm_win2.close();
            });
            let confirm_win3 = confirm_win.clone();
            no_btn.connect_clicked(move |_| {
                confirm_win3.close();
            });
            confirm_win.present();
        });
    }

    // ── Column management ───────────────────────────────────────────────────

    fn attach_column_menu(
        &self,
        menu_btn: Button,
        col_id: &str,
        col_name: &str,
        parent_window: &Window,
    ) {
        let col_id = col_id.to_string();
        let col_name = col_name.to_string();

        let popover = Popover::builder()
            .has_arrow(false)
            .build();
        let vbox = GtkBox::new(gtk4::Orientation::Vertical, 0);

        let rename_btn = Button::builder()
            .label("Rename")
            .has_frame(false)
            .halign(gtk4::Align::Start)
            .css_classes(["context-menu-item"])
            .build();
        vbox.append(&rename_btn);

        // Move left / right
        let left_btn = Button::builder()
            .label("Move Left")
            .has_frame(false)
            .halign(gtk4::Align::Start)
            .css_classes(["context-menu-item"])
            .build();
        vbox.append(&left_btn);

        let right_btn = Button::builder()
            .label("Move Right")
            .has_frame(false)
            .halign(gtk4::Align::Start)
            .css_classes(["context-menu-item"])
            .build();
        vbox.append(&right_btn);

        vbox.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

        // Set WIP limit
        let wip_btn = Button::builder()
            .label("Set WIP limit…")
            .has_frame(false)
            .halign(gtk4::Align::Start)
            .css_classes(["context-menu-item"])
            .build();
        vbox.append(&wip_btn);

        vbox.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

        let del_btn = Button::builder()
            .label("Delete Column")
            .has_frame(false)
            .halign(gtk4::Align::Start)
            .css_classes(["context-menu-item"])
            .build();
        vbox.append(&del_btn);

        popover.set_child(Some(&vbox));
        popover.set_parent(&menu_btn);

        let pop_close = popover.clone();
        menu_btn.connect_clicked(move |_| {
            pop_close.popup();
        });

        // Rename
        let self_rn = self.clone();
        let col_id_rn = col_id.clone();
        let pw_rn = parent_window.clone();
        let pop_close_rn = popover.clone();
        rename_btn.connect_clicked(move |_| {
            pop_close_rn.popdown();
            self_rn.prompt_rename_column(&col_id_rn, &col_name, &pw_rn);
        });

        // Move left
        let self_left = self.clone();
        let col_id_left = col_id.clone();
        let pw_left = parent_window.clone();
        let pop_close_left = popover.clone();
        left_btn.connect_clicked(move |_| {
            pop_close_left.popdown();
            self_left.reorder_column(&col_id_left, -1, &pw_left);
        });

        // Move right
        let self_right = self.clone();
        let col_id_right = col_id.clone();
        let pw_right = parent_window.clone();
        let pop_close_right = popover.clone();
        right_btn.connect_clicked(move |_| {
            pop_close_right.popdown();
            self_right.reorder_column(&col_id_right, 1, &pw_right);
        });

        // WIP limit
        let self_wip = self.clone();
        let col_id_wip = col_id.clone();
        let pw_wip = parent_window.clone();
        let pop_close_wip = popover.clone();
        wip_btn.connect_clicked(move |_| {
            pop_close_wip.popdown();
            self_wip.prompt_wip_limit(&col_id_wip, &pw_wip);
        });

        // Delete
        let self_del = self.clone();
        let col_id_del = col_id.clone();
        let pw_del = parent_window.clone();
        let pop_close_del = popover.clone();
        del_btn.connect_clicked(move |_| {
            pop_close_del.popdown();
            self_del.try_delete_column(&col_id_del, &pw_del);
        });
    }

    fn prompt_rename_column(&self, col_id: &str, current_name: &str, parent_window: &Window) {
        let col_id = col_id.to_string();

        let dialog = Window::builder()
            .title("Rename Column")
            .modal(true)
            .transient_for(parent_window)
            .default_width(300)
            .build();

        let entry = Entry::builder()
            .text(current_name)
            .build();
        entry.activate();

        let cancel_btn = Button::builder()
            .label("Cancel")
            .build();
        let ok_btn = Button::builder()
            .label("Rename")
            .css_classes(["suggested-action"])
            .build();

        let btn_box = GtkBox::new(gtk4::Orientation::Horizontal, 8);
        btn_box.set_halign(gtk4::Align::End);
        btn_box.append(&cancel_btn);
        btn_box.append(&ok_btn);

        let content = GtkBox::new(gtk4::Orientation::Vertical, 8);
        content.set_margin_start(12);
        content.set_margin_end(12);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.append(&entry);
        content.append(&btn_box);
        dialog.set_child(Some(&content));

        let dialog_close = dialog.clone();
        cancel_btn.connect_clicked(move |_| dialog_close.close());

        let self_rn = self.clone();
        let pw = parent_window.clone();
        let dialog_rn = dialog.clone();
        ok_btn.connect_clicked(move |_| {
            let new_name = entry.text().trim().to_string();
            if !new_name.is_empty() {
                let mut board = self_rn.board.borrow_mut();
                if let Some(col) = board.get_column_mut(&col_id) {
                    col.name = new_name;
                }
                drop(board);
                self_rn.save_and_refresh(&pw);
            }
            dialog_rn.close();
        });
    }

    fn reorder_column(&self, col_id: &str, direction: i32, parent_window: &Window) {
        let mut board = self.board.borrow_mut();
        let idx = board.columns.iter().position(|c| c.id == col_id);
        if let Some(i) = idx {
            let new_pos = if direction < 0 {
                if i == 0 { return; }
                i - 1
            } else {
                if i >= board.columns.len() - 1 { return; }
                i + 1
            };
            // Swap positions.
            let current_pos = board.columns[i].position;
            board.columns[i].position = board.columns[new_pos].position;
            board.columns[new_pos].position = current_pos;
        } else {
            return;
        }
        drop(board);
        self.save_and_refresh(parent_window);
    }

    fn prompt_wip_limit(&self, col_id: &str, parent_window: &Window) {
        let col_id = col_id.to_string();
        let current_limit = {
            let board = self.board.borrow();
            board.get_column(&col_id).and_then(|c| c.wip_limit).map(|l| l.to_string()).unwrap_or_default()
        };

        let dialog = Window::builder()
            .title("WIP Limit")
            .modal(true)
            .transient_for(parent_window)
            .default_width(260)
            .build();

        let entry = Entry::builder()
            .placeholder_text("Max cards (leave empty for no limit)")
            .text(&current_limit)
            .build();

        let cancel_btn = Button::builder()
            .label("Cancel")
            .build();
        let ok_btn = Button::builder()
            .label("Set")
            .css_classes(["suggested-action"])
            .build();

        let btn_box = GtkBox::new(gtk4::Orientation::Horizontal, 8);
        btn_box.set_halign(gtk4::Align::End);
        btn_box.append(&cancel_btn);
        btn_box.append(&ok_btn);

        let content = GtkBox::new(gtk4::Orientation::Vertical, 8);
        content.set_margin_start(12);
        content.set_margin_end(12);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.append(&entry);
        content.append(&btn_box);
        dialog.set_child(Some(&content));

        let dialog_close = dialog.clone();
        cancel_btn.connect_clicked(move |_| dialog_close.close());

        let self_wip = self.clone();
        let pw = parent_window.clone();
        let col_id_wip = col_id.clone();
        let dialog_wip = dialog.clone();
        ok_btn.connect_clicked(move |_| {
            let text = entry.text().trim().to_string();
            let limit = if text.is_empty() {
                None
            } else {
                text.parse::<u32>().ok()
            };
            let mut board = self_wip.board.borrow_mut();
            if let Some(col) = board.get_column_mut(&col_id_wip) {
                col.wip_limit = limit;
            }
            drop(board);
            self_wip.save_and_refresh(&pw);
            dialog_wip.close();
        });
    }

    fn prompt_add_column(&self, parent_window: &Window) {
        let dialog = Window::builder()
            .title("Add Column")
            .modal(true)
            .transient_for(parent_window)
            .default_width(300)
            .build();

        let entry = Entry::builder()
            .placeholder_text("Column name…")
            .build();
        entry.activate();

        let cancel_btn = Button::builder()
            .label("Cancel")
            .build();
        let ok_btn = Button::builder()
            .label("Add")
            .css_classes(["suggested-action"])
            .build();

        let btn_box = GtkBox::new(gtk4::Orientation::Horizontal, 8);
        btn_box.set_halign(gtk4::Align::End);
        btn_box.append(&cancel_btn);
        btn_box.append(&ok_btn);

        let content = GtkBox::new(gtk4::Orientation::Vertical, 8);
        content.set_margin_start(12);
        content.set_margin_end(12);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.append(&entry);
        content.append(&btn_box);
        dialog.set_child(Some(&content));

        let dialog_close = dialog.clone();
        cancel_btn.connect_clicked(move |_| dialog_close.close());

        let self_ac = self.clone();
        let pw = parent_window.clone();
        let dialog_ac = dialog.clone();
        ok_btn.connect_clicked(move |_| {
            let name = entry.text().trim().to_string();
            if !name.is_empty() {
                let mut board = self_ac.board.borrow_mut();
                let max_pos = board.columns.iter().map(|c| c.position).max().unwrap_or(0);
                board.columns.push(KanbanColumn {
                    id: uuid::Uuid::new_v4().to_string(),
                    name,
                    position: max_pos + 1,
                    wip_limit: None,
                });
                drop(board);
                self_ac.save_and_refresh(&pw);
            }
            dialog_ac.close();
        });

        dialog.present();
    }

    fn try_delete_column(&self, col_id: &str, parent_window: &Window) {
        let col_id = col_id.to_string();

        let column_count;
        let has_cards;
        {
            let board = self.board.borrow();
            column_count = board.columns.len();
            has_cards = !board.cards_in_column(&col_id).is_empty();
        }

        if column_count <= 1 {
            let msg_win = Window::builder()
                .title("Cannot Delete")
                .modal(true)
                .transient_for(parent_window)
                .default_width(280)
                .build();
            let msg_lbl = Label::builder()
                .label("Cannot delete the last column.")
                .margin_start(12)
                .margin_end(12)
                .margin_top(12)
                .build();
            let ok_btn2 = Button::builder()
                .label("OK")
                .css_classes(["suggested-action"])
                .build();
            let btn_box2 = GtkBox::new(gtk4::Orientation::Horizontal, 8);
            btn_box2.set_halign(gtk4::Align::End);
            btn_box2.set_margin_start(12);
            btn_box2.set_margin_end(12);
            btn_box2.set_margin_bottom(12);
            btn_box2.append(&ok_btn2);
            let vbox2 = GtkBox::new(gtk4::Orientation::Vertical, 0);
            vbox2.append(&msg_lbl);
            vbox2.append(&btn_box2);
            msg_win.set_child(Some(&vbox2));
            let msg_win2 = msg_win.clone();
            ok_btn2.connect_clicked(move |_| msg_win2.close());
            msg_win.present();
            return;
        }

        if has_cards {
            // Prompt for destination column.
            self.show_delete_column_with_cards_dialog(&col_id, parent_window);
        } else {
            let mut board = self.board.borrow_mut();
            board.columns.retain(|c| c.id != col_id);
            drop(board);
            self.save_and_refresh(parent_window);
        }
    }

    fn show_delete_column_with_cards_dialog(&self, col_id: &str, parent_window: &Window) {
        let col_id = col_id.to_string();
        let other_columns: Vec<(String, String)> = {
            let board = self.board.borrow();
            board.columns.iter()
                .filter(|c| c.id != col_id)
                .map(|c| (c.id.clone(), c.name.clone()))
                .collect()
        };

        let dialog = Window::builder()
            .title("Move Cards Before Deleting")
            .modal(true)
            .transient_for(parent_window)
            .default_width(350)
            .build();

        let lbl = Label::builder()
            .label("This column has cards. Where should they go?")
            .halign(gtk4::Align::Start)
            .wrap(true)
            .build();

        let dest_names: Vec<String> = other_columns.iter().map(|(_, n)| n.clone()).collect();
        let dest_strs: Vec<&str> = dest_names.iter().map(|s| s.as_str()).collect();
        let dest_list = StringList::new(&dest_strs);
        let dest_dropdown = DropDown::builder()
            .model(&dest_list)
            .selected(0)
            .build();

        let cancel_btn = Button::builder()
            .label("Cancel")
            .build();
        let del_btn = Button::builder()
            .label("Delete & Move")
            .css_classes(["suggested-action"])
            .build();

        let btn_box = GtkBox::new(gtk4::Orientation::Horizontal, 8);
        btn_box.set_halign(gtk4::Align::End);
        btn_box.append(&cancel_btn);
        btn_box.append(&del_btn);

        let content = GtkBox::new(gtk4::Orientation::Vertical, 8);
        content.set_margin_start(12);
        content.set_margin_end(12);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.append(&lbl);
        content.append(&dest_dropdown);
        content.append(&btn_box);
        dialog.set_child(Some(&content));

        let dialog_close = dialog.clone();
        cancel_btn.connect_clicked(move |_| dialog_close.close());

        let self_dd = self.clone();
        let pw = parent_window.clone();
        let dialog_dd = dialog.clone();
        del_btn.connect_clicked(move |_| {
            let dest_idx = dest_dropdown.selected() as usize;
            if dest_idx < other_columns.len() {
                let dest_id = other_columns[dest_idx].0.clone();
                let mut board = self_dd.board.borrow_mut();
                // Move all cards from the deleted column to the destination.
                let start_pos = board.cards_in_column(&dest_id).len() as f64;
                let mut i = 0u32;
                for card in &mut board.cards {
                    if card.column_id == col_id {
                        card.column_id = dest_id.clone();
                        card.position = start_pos + i as f64;
                        i += 1;
                    }
                }
                board.columns.retain(|c| c.id != col_id);
                drop(board);
                self_dd.save_and_refresh(&pw);
            }
            dialog_dd.close();
        });

        dialog.present();
    }

    // ── Card movement (drag-and-drop) ────────────────────────────────────────

    fn move_card_to_column(&self, card_id: &str, target_column_id: &str, parent_window: &Window) {
        let mut board = self.board.borrow_mut();
        let new_pos = board.cards_in_column(target_column_id).len() as f64;
        if let Some(card) = board.get_card_mut(card_id) {
            card.column_id = target_column_id.to_string();
            card.position = new_pos;
        }
        drop(board);
        self.save_and_refresh(parent_window);
    }

    // ── Persistence ──────────────────────────────────────────────────────────

    fn save_and_refresh(&self, parent_window: &Window) {
        let board = self.board.borrow();
        self.store.set_kanban_board(&board);
        drop(board);
        self.refresh_board(parent_window);
    }

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn lookup_project_name(&self, project_path: &Option<String>) -> String {
        let projs = self.projects.borrow();
        match project_path {
            Some(path) => projs
                .iter()
                .find(|p| p.path == *path)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "(project not found)".to_string()),
            None => String::new(),
        }
    }
}

// ── Slugify helper ──────────────────────────────────────────────────────────

fn slugify(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .filter(|c| *c != '\0')
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}
