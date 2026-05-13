pub mod markdown;
pub mod terminal;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, Button, CssProvider, DrawingArea, Entry,
    GestureClick, HeaderBar, Label, ListBox, ListBoxRow, Notebook, Orientation, Paned, Picture,
    Popover, ScrolledWindow, Stack, StackTransitionType, TextView, WrapMode,
};

use sizzle_core::{scan_projects, AgentPreset, MetadataStore, ScanSettings, ScannedProject};

// ── App state ─────────────────────────────────────────────────────────────

struct ProjectWidgets {
    git_view: TextView,
    agent_terminal: Option<terminal::TerminalWidget>,
    shell_terminal: Option<terminal::TerminalWidget>,
}

#[derive(Clone)]
struct ShellTab {
    id: usize,
    label: String,
    terminal: terminal::TerminalWidget,
}

#[derive(Clone)]
struct ShellTabContext {
    path: String,
    agent: terminal::TerminalWidget,
    shell_stack: Stack,
    tab_box: GtkBox,
    state: State,
    pending_exits: Arc<Mutex<Vec<String>>>,
    shells: Rc<RefCell<Vec<ShellTab>>>,
    active_shell_id: Rc<Cell<usize>>,
    next_shell_id: Rc<Cell<usize>>,
}

struct AppState {
    store: Arc<MetadataStore>,
    projects: Vec<ScannedProject>,
    project_widgets: HashMap<String, ProjectWidgets>,
    project_stack: Stack,
    git_stack: Stack,
    list_box: ListBox,
    active_terminals: HashMap<String, usize>,
    last_terminal_activity: HashMap<String, Vec<Arc<Mutex<Option<Instant>>>>>,
    pending_exits: Arc<Mutex<Vec<String>>>,
    focus_memory: HashMap<String, String>,
    main_window: gtk4::Window,
}

type State = Rc<RefCell<AppState>>;

// ── Entry ──────────────────────────────────────────────────────────────────

pub fn run() {
    env_logger::init();
    let app = Application::builder()
        .application_id("net.nebupookins.sizzle")
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(build_ui);
    install_app_icon();
    app.run();
}

/// Install the app icon to the user's XDG icon theme directory on first run so
/// `WindowExt::set_icon_name` can resolve it.
fn install_app_icon() {
    use std::path::PathBuf;

    let svg_data = include_bytes!("../../../assets/icons/app-icon.svg");
    let icon_name = "net.nebupookins.sizzle";
    let data_home = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(home).join(".local/share")
        });
    let icon_dir = data_home.join("icons/hicolor/scalable/apps");
    let icon_path = icon_dir.join(format!("{icon_name}.svg"));
    if icon_path.exists() {
        return;
    }
    std::fs::create_dir_all(&icon_dir).ok();
    let _ = std::fs::write(&icon_path, svg_data);
}

// ── Config dir ────────────────────────────────────────────────────────────

fn config_dir() -> PathBuf {
    std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .map(|h| PathBuf::from(h).join(".config"))
                .unwrap_or_else(|_| PathBuf::from("/tmp"))
        })
        .join("sizzle")
}

// ── UI construction ────────────────────────────────────────────────────────

fn build_ui(app: &Application) {
    let store = Arc::new(MetadataStore::new(config_dir()));
    let settings = store.get_scan_settings();
    let projects = if settings.scan_roots.is_empty() {
        vec![]
    } else {
        scan_projects(&settings)
    };

    let header = HeaderBar::new();

    let search = Entry::builder()
        .placeholder_text("Search projects…")
        .margin_start(6)
        .margin_end(6)
        .margin_top(6)
        .margin_bottom(4)
        .build();

    let list_box = ListBox::new();
    list_box.set_selection_mode(gtk4::SelectionMode::Single);

    let scroll = ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .build();
    scroll.set_child(Some(&list_box));

    // ── Settings button opens a modal window ─────────────────────────────────
    let settings_btn = Button::with_label("⚙ Settings");
    settings_btn.set_has_frame(false);
    settings_btn.set_margin_start(6);
    settings_btn.set_margin_top(4);
    settings_btn.set_margin_bottom(6);

    let scan_btn = Button::with_label("Add folder…");
    scan_btn.set_margin_end(6);
    scan_btn.set_margin_top(4);
    scan_btn.set_margin_bottom(6);

    let btn_row = GtkBox::new(Orientation::Horizontal, 0);
    btn_row.append(&settings_btn);
    btn_row.append(&scan_btn);

    // ── Memory breakdown widget ────────────────────────────────────────────
    let mem_total_lbl = Label::builder()
        .label("Memory: –")
        .halign(gtk4::Align::Start)
        .margin_start(8)
        .margin_end(8)
        .margin_top(2)
        .build();
    mem_total_lbl.add_css_class("caption");
    let mem_app_lbl = Label::builder()
        .label("")
        .halign(gtk4::Align::Start)
        .margin_start(8)
        .margin_end(8)
        .build();
    mem_app_lbl.add_css_class("caption");
    let mem_agent_lbl = Label::builder()
        .label("")
        .halign(gtk4::Align::Start)
        .margin_start(8)
        .margin_end(8)
        .margin_bottom(4)
        .build();
    mem_agent_lbl.add_css_class("caption");
    let mem_vbox = GtkBox::new(Orientation::Vertical, 0);
    mem_vbox.append(&mem_total_lbl);
    mem_vbox.append(&mem_app_lbl);
    mem_vbox.append(&mem_agent_lbl);

    // Hide the memory box until we have data
    let mem_sep = gtk4::Separator::new(Orientation::Horizontal);
    mem_sep.set_margin_start(4);
    mem_sep.set_margin_end(4);
    mem_vbox.set_visible(false);
    mem_sep.set_visible(false);

    let left = GtkBox::new(Orientation::Vertical, 0);
    left.append(&search);
    left.append(&scroll);
    left.append(&btn_row);
    left.append(&mem_sep);
    left.append(&mem_vbox);
    left.set_size_request(240, -1);

    let project_stack = Stack::builder()
        .transition_type(StackTransitionType::None)
        .hexpand(true)
        .vexpand(true)
        .build();

    let placeholder = Label::new(Some(
        "No project selected.\nClick \"Add folder…\" to configure a scan root.",
    ));
    placeholder.set_justify(gtk4::Justification::Center);
    project_stack.add_named(&placeholder, Some("__placeholder__"));
    project_stack.set_visible_child_name("__placeholder__");

    let git_stack = Stack::new();

    let inner_paned = Paned::new(Orientation::Horizontal);
    inner_paned.set_start_child(Some(&project_stack));
    inner_paned.set_end_child(Some(&git_stack));
    inner_paned.set_position(700);
    inner_paned.set_shrink_start_child(false);

    let outer_paned = Paned::new(Orientation::Horizontal);
    outer_paned.set_start_child(Some(&left));
    outer_paned.set_end_child(Some(&inner_paned));
    outer_paned.set_position(260);
    outer_paned.set_shrink_start_child(false);
    outer_paned.set_shrink_end_child(false);

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Sizzle")
        .default_width(1200)
        .default_height(750)
        .child(&outer_paned)
        .build();
    window.set_titlebar(Some(&header));
    window.set_icon_name(Some("net.nebupookins.sizzle"));

    let state = Rc::new(RefCell::new(AppState {
        store: store.clone(),
        projects,
        project_widgets: HashMap::new(),
        project_stack: project_stack.clone(),
        git_stack: git_stack.clone(),
        list_box: list_box.clone(),
        active_terminals: HashMap::new(),
        last_terminal_activity: HashMap::new(),
        pending_exits: Arc::new(Mutex::new(Vec::new())),
        focus_memory: HashMap::new(),
        main_window: window.clone().upcast(),
    }));

    populate_list(&state);

    {
        let state = state.clone();
        search.connect_changed(move |entry| {
            let query = entry.text();
            let query_lower = query.to_lowercase();
            let case_sensitive = query.chars().any(|c| c.is_uppercase());
            let st = state.borrow();
            let mut i = 0;
            while let Some(row) = st.list_box.row_at_index(i) {
                let path = row.widget_name();
                // Match against project data directly instead of navigating widget children,
                // which fails when a dot DrawingArea is prepended for active projects.
                let visible = if query.is_empty() {
                    true
                } else {
                    st.projects
                        .iter()
                        .find(|p| p.path == path)
                        .map_or(false, |p| {
                            let tag = p
                                .detected_tags
                                .first()
                                .map(|t| t.name.as_str())
                                .unwrap_or("");
                            let label = if tag.is_empty() {
                                p.name.clone()
                            } else {
                                format!("{} [{}]", p.name, tag)
                            };
                            if case_sensitive {
                                label.contains(query.as_str())
                            } else {
                                label.to_lowercase().contains(&query_lower as &str)
                            }
                        })
                };
                row.set_visible(visible);
                i += 1;
            }
        });
    }

    {
        let state = state.clone();
        list_box.connect_row_activated(move |_, row| {
            let path = row.widget_name().to_string();
            select_project(&state, &path);
        });
    }

    {
        let state = state.clone();
        let window_weak = window.downgrade();
        settings_btn.connect_clicked(move |_| {
            let Some(win) = window_weak.upgrade() else {
                return;
            };
            show_settings_window(&win, &state);
        });
    }

    {
        let state = state.clone();
        let window_weak = window.downgrade();
        scan_btn.connect_clicked(move |_| {
            let Some(win) = window_weak.upgrade() else {
                return;
            };
            pick_folder_and_scan(&state, &win);
        });
    }

    if store.get_scan_settings().scan_roots.is_empty() {
        let state = state.clone();
        let window_weak = window.downgrade();
        glib::idle_add_local_once(move || {
            let Some(win) = window_weak.upgrade() else {
                return;
            };
            pick_folder_and_scan(&state, &win);
        });
    }

    let state_for_exits = state.clone();
    glib::timeout_add_local(Duration::from_secs(2), move || {
        // Process exited terminals
        {
            let s = state_for_exits.borrow();
            let exits = std::mem::take(&mut *s.pending_exits.lock().unwrap());
            if !exits.is_empty() {
                drop(s);
                let mut s = state_for_exits.borrow_mut();
                for path in exits {
                    if let Some(n) = s.active_terminals.get_mut(&path) {
                        if *n <= 1 {
                            s.active_terminals.remove(&path);
                            s.last_terminal_activity.remove(&path);
                        } else {
                            *n -= 1;
                        }
                    }
                }
                drop(s);
                populate_list(&state_for_exits);
            }
        }

        // Refresh status dots (yellow/green) while terminals are active
        {
            let s = state_for_exits.borrow();
            if !s.active_terminals.is_empty() {
                drop(s);
                populate_list(&state_for_exits);
            }
        }

        if let Some((app_kb, agent_kb)) = read_mem_breakdown() {
            let total_mb = (app_kb + agent_kb) / 1024;
            let app_mb = app_kb / 1024;
            let agent_mb = agent_kb / 1024;
            mem_total_lbl.set_text(&format!("Memory   Total: {} MB", total_mb));
            mem_app_lbl.set_text(&format!("App      {} MB", app_mb));
            mem_agent_lbl.set_text(&format!("Agents   {} MB", agent_mb));
            mem_sep.set_visible(true);
            mem_vbox.set_visible(true);
        }
        glib::ControlFlow::Continue
    });

    {
        let state = state.clone();
        glib::timeout_add_local(Duration::from_secs(5), move || {
            let st = state.borrow();
            if let Some(name) = st.project_stack.visible_child_name() {
                let path = name.to_string();
                if path != "__placeholder__" {
                    if let Some(pw) = st.project_widgets.get(&path) {
                        update_git_status(&path, &pw.git_view);
                    }
                }
            }
            glib::ControlFlow::Continue
        });
    }

    // ── Dark background for the git status pane ──────────────────────────
    let css_provider = CssProvider::new();
    css_provider.load_from_data(
        ".git-pane { background: #1e1e2e; }
         .git-pane textview text { background: #1e1e2e; color: #cdd6f4; }
         .git-pane label { color: #cdd6f4; }
         .terminal-middle-bar {
             background: #20242b;
             border-top: 1px solid #3a404b;
             border-bottom: 1px solid #101216;
             padding: 2px 6px;
             min-height: 30px;
         }
         .terminal-grip { color: #8b949e; padding: 0 6px; }
         button.shell-tab {
             border-radius: 4px;
             padding: 2px 8px;
             background: transparent;
             color: #c9d1d9;
         }
         button.shell-tab-active {
             background: #3a404b;
             color: #ffffff;
         }
         button.shell-tab-close {
             min-width: 18px;
             min-height: 18px;
             padding: 0;
         }
         popover {
             background: #2a2e38;
             border: 1px solid #3a404b;
             border-radius: 6px;
         }
         popover label, popover button {
             color: #cdd6f4;
         }
         popover button:hover {
             background: #3a404b;
         }",
    );
    if let Some(display) = gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &css_provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    window.present();
}

// ── Populate the left-pane list ────────────────────────────────────────────

fn marker_sort_key(marker: Option<&str>) -> u8 {
    match marker {
        Some("favorite") => 0,
        Some("ignored") => 2,
        _ => 1,
    }
}

fn format_relative_time(last_ms: i64) -> String {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let secs = ((now_ms - last_ms) / 1000).max(0);
    match secs {
        s if s < 60 => "just now".to_string(),
        s if s < 120 => "1 minute ago".to_string(),
        s if s < 3_600 => format!("{} minutes ago", s / 60),
        s if s < 7_200 => "1 hour ago".to_string(),
        s if s < 86_400 => format!("{} hours ago", s / 3_600),
        s if s < 172_800 => "yesterday".to_string(),
        s if s < 2_592_000 => format!("{} days ago", s / 86_400),
        s if s < 5_184_000 => "1 month ago".to_string(),
        s => format!("{} months ago", s / 2_592_000),
    }
}

fn populate_list(state: &State) {
    use std::cmp::Reverse;

    let st = state.borrow();
    while let Some(row) = st.list_box.row_at_index(0) {
        st.list_box.remove(&row);
    }

    let all_meta = st.store.get_all_metadata();

    let mut sorted: Vec<&ScannedProject> = st.projects.iter().collect();
    sorted.sort_by_key(|p| {
        let is_active = st.active_terminals.contains_key(&p.path);
        let meta = all_meta.get(&p.path);
        let marker_key = marker_sort_key(meta.and_then(|m| m.marker.as_deref()));
        let time_key = Reverse(meta.and_then(|m| m.last_launched));
        let name_key = p.name.to_lowercase();
        (!is_active, marker_key, time_key, name_key)
    });

    for project in sorted {
        let marker = all_meta
            .get(&project.path)
            .and_then(|m| m.marker.as_deref())
            .unwrap_or("");
        let is_ignored = marker == "ignored";

        let tag = project
            .detected_tags
            .first()
            .map(|t| t.name.as_str())
            .unwrap_or("");
        let name_text = if tag.is_empty() {
            project.name.clone()
        } else {
            format!("{} [{}]", project.name, tag)
        };

        let last_active = all_meta
            .get(&project.path)
            .and_then(|m| m.last_launched)
            .map(format_relative_time)
            .unwrap_or_else(|| "never".to_string());

        let name_lbl = Label::builder()
            .label(&name_text)
            .halign(gtk4::Align::Start)
            .margin_start(8)
            .margin_top(4)
            .margin_bottom(0)
            .build();

        let time_lbl = Label::builder()
            .label(&last_active)
            .halign(gtk4::Align::Start)
            .margin_start(8)
            .margin_top(0)
            .margin_bottom(4)
            .build();
        time_lbl.add_css_class("caption");

        let info_box = GtkBox::new(Orientation::Vertical, 0);
        info_box.set_hexpand(true);
        info_box.append(&name_lbl);
        info_box.append(&time_lbl);

        if is_ignored {
            info_box.set_opacity(0.45);
        }

        let (icon, tooltip, marker_opacity) = match marker {
            "favorite" => ("★", "Mark as ignored", 1.0),
            "ignored" => ("🗑", "Reset to default", 1.0),
            _ => ("☆", "Mark as favourite", 0.35),
        };
        let marker_btn = Button::with_label(icon);
        marker_btn.set_has_frame(false);
        marker_btn.set_opacity(marker_opacity);
        marker_btn.set_tooltip_text(Some(tooltip));
        marker_btn.set_valign(gtk4::Align::Center);
        marker_btn.set_margin_end(4);

        let row_box = GtkBox::new(Orientation::Horizontal, 0);

        // Status dot: yellow if recent terminal activity, green if idle, hidden if inactive
        if st.active_terminals.contains_key(&project.path) {
            let has_recent_activity = st
                .last_terminal_activity
                .get(&project.path)
                .map(|activities| {
                    activities.iter().any(|a| {
                        a.lock()
                            .unwrap()
                            .map(|t| t.elapsed() < Duration::from_secs(5))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);

            let dot = DrawingArea::new();
            dot.set_size_request(8, 8);
            dot.set_valign(gtk4::Align::Center);
            dot.set_margin_start(8);
            dot.set_draw_func(move |_, cr, w, h| {
                let r = (w.min(h) as f64 / 2.0).min(4.0);
                if has_recent_activity {
                    cr.set_source_rgb(0.95, 0.76, 0.0); // yellow/gold
                } else {
                    cr.set_source_rgb(0.31, 0.98, 0.48); // #50fa7b green
                }
                cr.arc(
                    w as f64 / 2.0,
                    h as f64 / 2.0,
                    r - 0.5,
                    0.0,
                    2.0 * std::f64::consts::PI,
                );
                cr.fill().ok();
            });
            row_box.append(&dot);
        }

        row_box.append(&info_box);
        row_box.append(&marker_btn);

        let row = ListBoxRow::new();
        row.set_widget_name(&project.path);
        row.set_child(Some(&row_box));
        st.list_box.append(&row);

        // ── Right-click context menu ────────────────────────────────────
        {
            let gesture = GestureClick::new();
            gesture.set_button(3);

            let context_popover = Popover::builder()
                .has_arrow(false)
                .build();
            let popover_vbox = GtkBox::new(Orientation::Vertical, 0);

            let move_btn = Button::with_label("Move/Rename Project");
            move_btn.set_has_frame(false);
            move_btn.set_halign(gtk4::Align::Start);
            move_btn.set_margin_start(4);
            move_btn.set_margin_end(4);
            move_btn.set_margin_top(2);
            move_btn.set_margin_bottom(2);
            popover_vbox.append(&move_btn);

            let ignore_btn = Button::with_label("Add to Ignored Roots");
            ignore_btn.set_has_frame(false);
            ignore_btn.set_halign(gtk4::Align::Start);
            ignore_btn.set_margin_start(4);
            ignore_btn.set_margin_end(4);
            ignore_btn.set_margin_top(2);
            ignore_btn.set_margin_bottom(2);
            popover_vbox.append(&ignore_btn);

            let is_favorite = all_meta
                .get(&project.path)
                .and_then(|m| m.marker.as_deref())
                == Some("favorite");
            if is_favorite {
                let sep = gtk4::Separator::new(Orientation::Horizontal);
                popover_vbox.append(&sep);
                let unmark_btn = Button::with_label("Remove from Favorites");
                unmark_btn.set_has_frame(false);
                unmark_btn.set_halign(gtk4::Align::Start);
                unmark_btn.set_margin_start(4);
                unmark_btn.set_margin_end(4);
                unmark_btn.set_margin_top(2);
                unmark_btn.set_margin_bottom(2);
                popover_vbox.append(&unmark_btn);

                let unmark_path = project.path.clone();
                let unmark_state = state.clone();
                let popover_close = context_popover.clone();
                unmark_btn.connect_clicked(move |_| {
                    popover_close.popdown();
                    unmark_state.borrow().store.set_project_marker(&unmark_path, None);
                    populate_list(&unmark_state);
                });
            }

            context_popover.set_child(Some(&popover_vbox));

            let popover_gesture = context_popover.clone();
            gesture.connect_pressed(move |_, _, _, _| {
                popover_gesture.popup();
            });
            context_popover.set_parent(&row);
            row.add_controller(gesture);

            let path_mr = project.path.clone();
            let state_mr = state.clone();
            let popover_close = context_popover.clone();
            move_btn.connect_clicked(move |_| {
                popover_close.popdown();
                show_move_rename_dialog(&state_mr, &path_mr);
            });

            let path_ig = project.path.clone();
            let state_ig = state.clone();
            let popover_close2 = context_popover.clone();
            ignore_btn.connect_clicked(move |_| {
                popover_close2.popdown();
                add_to_ignored_roots(&state_ig, &path_ig);
            });
        }

        {
            let path = project.path.clone();
            let state = state.clone();
            marker_btn.connect_clicked(move |_| {
                {
                    let st = state.borrow();
                    let cur = st.store.get_metadata(&path).marker;
                    let next = match cur.as_deref() {
                        None | Some("") => Some("favorite".to_string()),
                        Some("favorite") => Some("ignored".to_string()),
                        Some("ignored") => None,
                        _ => Some("favorite".to_string()),
                    };
                    st.store.set_project_marker(&path, next);
                }
                populate_list(&state);
            });
        }
    }
}

// ── Switch to a project ────────────────────────────────────────────────────

fn select_project(state: &State, path: &str) {
    let path = path.to_string();

    let found = {
        let st = state.borrow();
        st.projects.iter().any(|p| p.path == path)
    };
    if !found {
        return;
    }

    {
        let mut st = state.borrow_mut();
        if !st.project_widgets.contains_key(&path) {
            let presets = st.store.get_agent_presets();

            // ── Notebook with markdown + explorer tabs ────────────────────
            let notebook = Notebook::new();
            notebook.set_hexpand(true);
            notebook.set_vexpand(true);

            let md_files = sizzle_core::files::get_markdown_files(path.clone());
            for md_path in &md_files {
                let tab_name = std::path::Path::new(md_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(md_path)
                    .to_string();
                let mv = markdown::MarkdownView::new();
                match sizzle_core::files::read_markdown_file(md_path.clone()) {
                    Some(content) => mv.render(&content),
                    None => mv.render("*Failed to read file.*"),
                }

                // ── Toolbar: Edit / Save / Cancel buttons ─────────────────
                let edit_btn = Button::with_label("Edit");
                edit_btn.set_has_frame(false);
                edit_btn.set_margin_start(4);
                edit_btn.set_margin_top(4);
                edit_btn.set_margin_bottom(4);

                let save_btn = Button::with_label("Save");
                save_btn.set_has_frame(false);
                save_btn.set_visible(false);

                let cancel_btn = Button::with_label("Cancel");
                cancel_btn.set_has_frame(false);
                cancel_btn.set_visible(false);

                let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
                spacer.set_hexpand(true);

                let toolbar = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
                toolbar.set_margin_start(8);
                toolbar.set_margin_end(8);
                toolbar.append(&spacer);
                toolbar.append(&edit_btn);
                toolbar.append(&save_btn);
                toolbar.append(&cancel_btn);

                let mv_edit = mv.clone();
                let eb = edit_btn.clone();
                let sb = save_btn.clone();
                let cb = cancel_btn.clone();
                edit_btn.connect_clicked(move |_| {
                    mv_edit.set_editable(true);
                    eb.set_visible(false);
                    sb.set_visible(true);
                    cb.set_visible(true);
                });

                let mv_cancel = mv.clone();
                let eb2 = edit_btn.clone();
                let sb2 = save_btn.clone();
                let cb2 = cancel_btn.clone();
                cancel_btn.connect_clicked(move |_| {
                    mv_cancel.set_editable(false);
                    eb2.set_visible(true);
                    sb2.set_visible(false);
                    cb2.set_visible(false);
                });

                let fp = md_path.clone();
                let mv_save = mv.clone();
                let eb3 = edit_btn.clone();
                let sb3 = save_btn.clone();
                let cb3 = cancel_btn.clone();
                save_btn.connect_clicked(move |_| {
                    let text = mv_save.get_buffer_text();
                    mv_save.set_source(&text);
                    if sizzle_core::files::write_markdown_file(fp.clone(), text).is_ok() {
                        mv_save.set_editable(false);
                        eb3.set_visible(true);
                        sb3.set_visible(false);
                        cb3.set_visible(false);
                    }
                });

                let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                container.append(&toolbar);
                container.append(&mv.scroll);

                notebook.append_page(&container, Some(&Label::new(Some(&tab_name))));
            }

            let explorer = build_explorer_tab(&path);
            notebook.append_page(&explorer, Some(&Label::new(Some("Explorer"))));

            // Open to first markdown tab; fall back to Explorer
            notebook.set_current_page(Some(0));

            // ── Launch buttons ─────────────────────────────────────────────
            let btn_box = GtkBox::new(Orientation::Horizontal, 4);
            btn_box.set_margin_start(8);
            btn_box.set_margin_end(8);
            btn_box.set_margin_top(4);
            btn_box.set_margin_bottom(4);
            btn_box.set_halign(gtk4::Align::End);

            let claude_btn = Button::with_label("Launch Claude");
            let codex_btn = Button::with_label("Launch Codex");
            let shell_btn = Button::with_label("Shell");
            btn_box.append(&claude_btn);
            btn_box.append(&codex_btn);
            btn_box.append(&shell_btn);

            // Collect preset buttons before connecting callbacks
            let mut preset_btns: Vec<(Button, String, String)> = Vec::new();
            for preset in &presets {
                let btn = Button::with_label(&preset.label);
                btn_box.append(&btn);
                preset_btns.push((btn, preset.label.clone(), preset.command.clone()));
            }

            let spacer = GtkBox::new(Orientation::Horizontal, 0);
            spacer.set_hexpand(true);

            let top_bar = GtkBox::new(Orientation::Horizontal, 0);
            top_bar.append(&spacer);
            top_bar.append(&btn_box);

            // ── Git status strip (right pane) ─────────────────────────
            let git_view = make_git_view();
            git_view.add_css_class("git-pane");
            let git_scroll = ScrolledWindow::builder()
                .hscrollbar_policy(gtk4::PolicyType::Automatic)
                .vscrollbar_policy(gtk4::PolicyType::Automatic)
                .hexpand(true)
                .vexpand(true)
                .build();
            git_scroll.add_css_class("git-pane");
            git_scroll.set_child(Some(&git_view));

            let git_container = GtkBox::new(Orientation::Vertical, 0);
            git_container.add_css_class("git-pane");
            let git_header = Label::builder()
                .label("Git Status")
                .halign(gtk4::Align::Start)
                .margin_start(8)
                .margin_top(6)
                .margin_bottom(2)
                .build();
            git_header.add_css_class("git-pane");
            git_container.append(&git_header);
            git_container.append(&git_scroll);

            st.git_stack.add_named(&git_container, Some(&path));

            let project_box = GtkBox::new(Orientation::Vertical, 0);
            project_box.append(&top_bar);
            project_box.append(&notebook);

            // ── Connect launch buttons ─────────────────────────────────────
            {
                let nb = notebook.clone();
                let p = path.clone();
                let st = state.clone();
                claude_btn.connect_clicked(move |_| {
                    launch_terminals(&p, "Claude", Some("claude".to_string()), &nb, &st);
                });
            }
            {
                let nb = notebook.clone();
                let p = path.clone();
                let st = state.clone();
                codex_btn.connect_clicked(move |_| {
                    launch_terminals(&p, "Codex", Some("codex".to_string()), &nb, &st);
                });
            }
            {
                let nb = notebook.clone();
                let p = path.clone();
                let st = state.clone();
                shell_btn.connect_clicked(move |_| {
                    launch_terminals(&p, "Shell", None, &nb, &st);
                });
            }
            for (btn, label, cmd) in preset_btns {
                let nb = notebook.clone();
                let p = path.clone();
                let st = state.clone();
                btn.connect_clicked(move |_| {
                    launch_terminals(&p, &label, Some(cmd.clone()), &nb, &st);
                });
            }

            st.project_stack.add_named(&project_box, Some(&path));
            st.project_widgets.insert(
                path.clone(),
                ProjectWidgets {
                    git_view,
                    agent_terminal: None,
                    shell_terminal: None,
                },
            );
        }
    }

    // Phase 1: immutable borrow to set visible child + read focus memory
    let focus_key = {
        let st = state.borrow();
        st.project_stack.set_visible_child_name(&path);
        st.git_stack.set_visible_child_name(&path);
        if let Some(pw) = st.project_widgets.get(&path) {
            update_git_status(&path, &pw.git_view);
        }
        st.store.set_last_launched(&path);
        st.focus_memory.get(&path).cloned()
    };

    // Phase 2: restore focus memory
    // Important: clone the terminal out first, then call focus() outside the borrow,
    // because focus() triggers the focus-in callback which also borrows state.
    if let Some(key) = focus_key {
        let terminal =
            state
                .borrow()
                .project_widgets
                .get(&path)
                .and_then(|pw| match key.as_str() {
                    "shell" => pw.shell_terminal.clone(),
                    _ => pw.agent_terminal.clone(),
                });
        if let Some(t) = terminal {
            t.focus();
        }
    }
}

// ── Explorer tab ──────────────────────────────────────────────────────────

fn build_explorer_tab(project_root: &str) -> Paned {
    // ── Left: nav bar + file list ──────────────────────────────────────────
    let path_lbl = Label::builder()
        .halign(gtk4::Align::Start)
        .hexpand(true)
        .ellipsize(gtk4::pango::EllipsizeMode::Start)
        .margin_start(6)
        .margin_end(6)
        .margin_top(4)
        .margin_bottom(4)
        .build();

    let back_btn = Button::builder()
        .label("↑")
        .tooltip_text("Go up")
        .has_frame(false)
        .sensitive(false)
        .margin_start(4)
        .margin_top(4)
        .margin_bottom(4)
        .build();

    let nav_bar = GtkBox::new(Orientation::Horizontal, 0);
    nav_bar.append(&back_btn);
    nav_bar.append(&path_lbl);

    let file_list = ListBox::new();
    file_list.set_selection_mode(gtk4::SelectionMode::Single);

    let list_scroll = ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .build();
    list_scroll.set_child(Some(&file_list));

    let left = GtkBox::new(Orientation::Vertical, 0);
    left.append(&nav_bar);
    left.append(&list_scroll);
    left.set_size_request(240, -1);

    // ── Right: content viewer ──────────────────────────────────────────────
    let content_stack = Stack::new();
    content_stack.set_hexpand(true);
    content_stack.set_vexpand(true);

    let placeholder = Label::builder()
        .label("Select a file to preview")
        .halign(gtk4::Align::Center)
        .valign(gtk4::Align::Center)
        .build();
    content_stack.add_named(&placeholder, Some("placeholder"));

    let msg_lbl = Label::builder()
        .halign(gtk4::Align::Center)
        .valign(gtk4::Align::Center)
        .build();
    content_stack.add_named(&msg_lbl, Some("message"));

    let text_view = TextView::new();
    text_view.set_editable(false);
    text_view.set_monospace(true);
    text_view.set_wrap_mode(WrapMode::None);
    text_view.set_top_margin(8);
    text_view.set_bottom_margin(8);
    text_view.set_left_margin(8);
    text_view.set_right_margin(8);
    let text_scroll = ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Automatic)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .hexpand(true)
        .vexpand(true)
        .child(&text_view)
        .build();
    content_stack.add_named(&text_scroll, Some("text"));

    let md_view = markdown::MarkdownView::new();
    let current_md_path: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

    // ── Explorer toolbar for markdown editing ──────────────────────────
    let md_edit_btn = Button::with_label("Edit");
    md_edit_btn.set_has_frame(false);
    md_edit_btn.set_margin_start(4);
    md_edit_btn.set_margin_top(4);
    md_edit_btn.set_margin_bottom(4);
    let md_save_btn = Button::with_label("Save");
    md_save_btn.set_has_frame(false);
    md_save_btn.set_visible(false);
    let md_cancel_btn = Button::with_label("Cancel");
    md_cancel_btn.set_has_frame(false);
    md_cancel_btn.set_visible(false);

    let md_toolbar_spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    md_toolbar_spacer.set_hexpand(true);

    let md_toolbar = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    md_toolbar.set_margin_start(8);
    md_toolbar.set_margin_end(8);
    md_toolbar.append(&md_toolbar_spacer);
    md_toolbar.append(&md_edit_btn);
    md_toolbar.append(&md_save_btn);
    md_toolbar.append(&md_cancel_btn);

    let md_container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    md_container.append(&md_toolbar);
    md_container.append(&md_view.scroll);
    content_stack.add_named(&md_container, Some("markdown"));

    let image_view = gtk4::Picture::new();
    image_view.set_hexpand(true);
    image_view.set_vexpand(true);
    let image_scroll = ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Automatic)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .hexpand(true)
        .vexpand(true)
        .child(&image_view)
        .build();
    content_stack.add_named(&image_scroll, Some("image"));

    // Wire up explorer markdown edit/save/cancel
    {
        let mv = md_view.clone();
        let eb = md_edit_btn.clone();
        let sb = md_save_btn.clone();
        let cb = md_cancel_btn.clone();
        md_edit_btn.connect_clicked(move |_| {
            mv.set_editable(true);
            eb.set_visible(false);
            sb.set_visible(true);
            cb.set_visible(true);
        });
    }
    {
        let mv = md_view.clone();
        let eb2 = md_edit_btn.clone();
        let sb2 = md_save_btn.clone();
        let cb2 = md_cancel_btn.clone();
        md_cancel_btn.connect_clicked(move |_| {
            mv.set_editable(false);
            eb2.set_visible(true);
            sb2.set_visible(false);
            cb2.set_visible(false);
        });
    }
    {
        let mv = md_view.clone();
        let eb3 = md_edit_btn.clone();
        let sb3 = md_save_btn.clone();
        let cb3 = md_cancel_btn.clone();
        let path = current_md_path.clone();
        md_save_btn.connect_clicked(move |_| {
            let text = mv.get_buffer_text();
            let file_path = path.borrow().clone();
            if let Some(fp) = file_path {
                mv.set_source(&text);
                if sizzle_core::files::write_markdown_file(fp, text).is_ok() {
                    mv.set_editable(false);
                    eb3.set_visible(true);
                    sb3.set_visible(false);
                    cb3.set_visible(false);
                }
            }
        });
    }

    content_stack.set_visible_child_name("placeholder");

    // ── Paned ──────────────────────────────────────────────────────────────
    let paned = Paned::new(Orientation::Horizontal);
    paned.set_start_child(Some(&left));
    paned.set_end_child(Some(&content_stack));
    paned.set_position(240);
    paned.set_shrink_start_child(false);
    paned.set_shrink_end_child(false);

    // ── Shared state: current directory ───────────────────────────────────
    let current_dir: Rc<RefCell<String>> = Rc::new(RefCell::new(project_root.to_string()));
    let project_root = project_root.to_string();

    explorer_load_dir(
        &file_list,
        &path_lbl,
        &back_btn,
        &project_root,
        project_root.clone(),
        &current_dir,
    );

    // ── Row activated (connected once) ────────────────────────────────────
    {
        let fl = file_list.clone();
        let pl = path_lbl.clone();
        let bb = back_btn.clone();
        let cs = content_stack.clone();
        let tv = text_view.clone();
        let mv = md_view.clone();
        let ml = msg_lbl.clone();
        let pr = project_root.clone();
        let cd = current_dir.clone();
        let md_path = current_md_path.clone();
        let eb = md_edit_btn.clone();
        let sb = md_save_btn.clone();
        let cb = md_cancel_btn.clone();
        let iv = image_view.clone();

        file_list.connect_row_activated(move |_, row| {
            let path = row.widget_name().to_string();
            if std::fs::metadata(&path)
                .map(|m| m.is_dir())
                .unwrap_or(false)
            {
                explorer_load_dir(&fl, &pl, &bb, &pr, path, &cd);
            } else {
                explorer_show_file(&cs, &tv, &mv, &ml, &iv, &pr, &path);
                // Track the current file for markdown editing
                if cs.visible_child_name().as_deref() == Some("markdown") {
                    *md_path.borrow_mut() = Some(path);
                    // Reset toolbar to view mode when switching files
                    mv.set_editable(false);
                    eb.set_visible(true);
                    sb.set_visible(false);
                    cb.set_visible(false);
                }
            }
        });
    }

    // ── Back button ────────────────────────────────────────────────────────
    {
        let fl = file_list.clone();
        let pl = path_lbl.clone();
        let bb = back_btn.clone();
        let pr = project_root.clone();
        let cd = current_dir.clone();

        back_btn.connect_clicked(move |_| {
            let cur = cd.borrow().clone();
            let parent = std::path::Path::new(&cur)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .filter(|p| p.starts_with(&pr))
                .unwrap_or_else(|| pr.clone());
            explorer_load_dir(&fl, &pl, &bb, &pr, parent, &cd);
        });
    }

    paned
}

fn explorer_load_dir(
    file_list: &ListBox,
    path_lbl: &Label,
    back_btn: &Button,
    project_root: &str,
    dir: String,
    current_dir: &Rc<RefCell<String>>,
) {
    *current_dir.borrow_mut() = dir.clone();

    let rel = dir
        .strip_prefix(project_root)
        .unwrap_or(&dir)
        .trim_start_matches('/');
    path_lbl.set_text(if rel.is_empty() { "/" } else { rel });

    back_btn.set_sensitive(&dir != project_root);

    while let Some(row) = file_list.row_at_index(0) {
        file_list.remove(&row);
    }

    for entry in sizzle_core::files::list_directory(project_root.to_string(), Some(dir)) {
        let prefix = if entry.is_directory { "📁 " } else { "  " };
        let lbl = Label::builder()
            .label(&format!("{}{}", prefix, entry.name))
            .halign(gtk4::Align::Start)
            .margin_start(8)
            .margin_top(4)
            .margin_bottom(4)
            .build();
        let row = ListBoxRow::new();
        row.set_child(Some(&lbl));
        row.set_widget_name(&entry.path);
        file_list.append(&row);
    }
}

fn explorer_show_file(
    content_stack: &Stack,
    text_view: &TextView,
    md_view: &markdown::MarkdownView,
    msg_lbl: &Label,
    image_view: &Picture,
    project_root: &str,
    file_path: &str,
) {
    let preview = sizzle_core::files::preview_file(project_root.to_string(), file_path.to_string());
    match preview.kind.as_str() {
        "text" => {
            let content = preview.content.unwrap_or_default();
            let ext = std::path::Path::new(file_path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if matches!(ext.as_str(), "md" | "markdown" | "txt" | "rst") {
                md_view.render(&content);
                content_stack.set_visible_child_name("markdown");
            } else {
                text_view.buffer().set_text(&content);
                content_stack.set_visible_child_name("text");
            }
        }
        "media" => {
            if preview
                .mime_type
                .as_deref()
                .map_or(false, |m| m.starts_with("image/"))
            {
                image_view.set_filename(Some(file_path));
                content_stack.set_visible_child_name("image");
            } else if let Some(mime) = preview.mime_type {
                msg_lbl.set_text(&format!("Preview not available for {}", mime));
                content_stack.set_visible_child_name("message");
            } else {
                msg_lbl.set_text("Cannot preview this file type");
                content_stack.set_visible_child_name("message");
            }
        }
        "tooLarge" => {
            let mb = preview.size.unwrap_or(0) as f64 / (1024.0 * 1024.0);
            msg_lbl.set_text(&format!("File too large to preview ({:.1} MB)", mb));
            content_stack.set_visible_child_name("message");
        }
        _ => {
            let msg = preview
                .message
                .unwrap_or_else(|| format!("Cannot preview this file ({})", preview.kind));
            msg_lbl.set_text(&msg);
            content_stack.set_visible_child_name("message");
        }
    }
}

// ── Launch a vertically split terminal pair ────────────────────────────────

fn launch_terminals(
    path: &str,
    tab_label: &str,
    agent_cmd: Option<String>,
    notebook: &Notebook,
    state: &State,
) {
    let agent = terminal::TerminalWidget::new(Some(path), agent_cmd);
    let pending_exits = {
        let mut st = state.borrow_mut();
        *st.active_terminals.entry(path.to_string()).or_insert(0) += 1;
        st.last_terminal_activity
            .entry(path.to_string())
            .or_default()
            .push(agent.last_activity());
        st.pending_exits.clone()
    };

    {
        let pe = pending_exits.clone();
        let p = path.to_string();
        agent.set_on_exit(move || {
            pe.lock().unwrap().push(p.clone());
        });
    }

    let shell_stack = Stack::builder()
        .transition_type(StackTransitionType::None)
        .hexpand(true)
        .vexpand(true)
        .build();

    let middle_bar = GtkBox::new(Orientation::Horizontal, 4);
    middle_bar.add_css_class("terminal-middle-bar");
    middle_bar.set_cursor_from_name(Some("ns-resize"));

    let bottom = GtkBox::new(Orientation::Vertical, 0);
    bottom.set_hexpand(true);
    bottom.set_vexpand(true);
    bottom.append(&middle_bar);
    bottom.append(&shell_stack);

    let vpaned = Paned::new(Orientation::Vertical);
    vpaned.set_start_child(Some(&agent.container));
    vpaned.set_end_child(Some(&bottom));
    vpaned.set_position(300);
    vpaned.set_shrink_start_child(false);
    vpaned.set_shrink_end_child(false);
    vpaned.set_vexpand(true);

    {
        let paned = vpaned.clone();
        let drag_start = Rc::new(Cell::new(0));
        let drag = gtk4::GestureDrag::new();
        {
            let drag_start = drag_start.clone();
            let paned = paned.clone();
            drag.connect_drag_begin(move |_, _, _| {
                drag_start.set(paned.position());
            });
        }
        {
            let drag_start = drag_start.clone();
            let paned = paned.clone();
            drag.connect_drag_update(move |_, _, offset_y| {
                let next = drag_start.get() + offset_y.round() as i32;
                paned.set_position(next.max(80));
            });
        }
        middle_bar.add_controller(drag);
    }

    let ctx = ShellTabContext {
        path: path.to_string(),
        agent: agent.clone(),
        shell_stack,
        tab_box: middle_bar,
        state: state.clone(),
        pending_exits: pending_exits.clone(),
        shells: Rc::new(RefCell::new(Vec::new())),
        active_shell_id: Rc::new(Cell::new(0)),
        next_shell_id: Rc::new(Cell::new(1)),
    };

    {
        let ctx = ctx.clone();
        agent.connect_focus_in(move || {
            activate_agent_terminal(&ctx);
        });
    }

    let shell = add_shell_tab(&ctx, false);
    activate_agent_terminal(&ctx);

    // Tab header: label + close button
    let tab_lbl = Label::new(Some(tab_label));
    let close_btn = Button::builder()
        .icon_name("window-close-symbolic")
        .has_frame(false)
        .focus_on_click(false)
        .tooltip_text("Close tab")
        .build();
    let tab_header = GtkBox::new(Orientation::Horizontal, 4);
    tab_header.append(&tab_lbl);
    tab_header.append(&close_btn);

    let tab_idx = notebook.append_page(&vpaned, Some(&tab_header));
    notebook.set_current_page(Some(tab_idx));

    // Close button removes the terminal tab and shuts down its active PTYs.
    let nb = notebook.clone();
    let page_widget = vpaned.clone();
    let ctx_close = ctx.clone();
    close_btn.connect_clicked(move |_| {
        shutdown_terminal_tab(&ctx_close);
        if let Some(page_num) = nb.page_num(&page_widget) {
            nb.remove_page(Some(page_num));
        }
    });

    agent.focus();

    // Store terminal references for focus memory restoration when switching back.
    if let Some(pw) = state.borrow_mut().project_widgets.get_mut(path) {
        pw.agent_terminal = Some(agent);
        pw.shell_terminal = Some(shell);
    }
}

fn shell_child_name(id: usize) -> String {
    format!("shell-{id}")
}

fn add_shell_tab(ctx: &ShellTabContext, focus: bool) -> terminal::TerminalWidget {
    let id = ctx.next_shell_id.get();
    ctx.next_shell_id.set(id + 1);

    let shell = terminal::TerminalWidget::new(Some(&ctx.path), None);
    let child_name = shell_child_name(id);
    ctx.shell_stack
        .add_named(&shell.container, Some(&child_name));

    {
        let mut st = ctx.state.borrow_mut();
        *st.active_terminals.entry(ctx.path.clone()).or_insert(0) += 1;
        st.last_terminal_activity
            .entry(ctx.path.clone())
            .or_default()
            .push(shell.last_activity());
    }

    {
        let pe = ctx.pending_exits.clone();
        let p = ctx.path.clone();
        shell.set_on_exit(move || {
            pe.lock().unwrap().push(p.clone());
        });
    }

    ctx.shells.borrow_mut().push(ShellTab {
        id,
        label: format!("Shell {id}"),
        terminal: shell.clone(),
    });

    wire_shell_terminal(ctx, &shell, id);
    activate_shell_tab(ctx, id, focus);
    populate_list(&ctx.state);
    shell
}

fn wire_shell_terminal(ctx: &ShellTabContext, shell: &terminal::TerminalWidget, shell_id: usize) {
    {
        let ctx = ctx.clone();
        shell.connect_focus_in(move || {
            activate_shell_tab(&ctx, shell_id, false);
        });
    }
    {
        let ctx = ctx.clone();
        shell.connect_key_pressed_capture(move |kv, mods| {
            let is_ctrl_w = kv == gdk::Key::w && mods.contains(gdk::ModifierType::CONTROL_MASK);
            let can_close = ctx.shells.borrow().len() > 1;
            if is_ctrl_w && can_close {
                close_shell_tab(&ctx, shell_id, true);
                true
            } else {
                false
            }
        });
    }
}

fn activate_agent_terminal(ctx: &ShellTabContext) {
    ctx.agent.set_focused(true);
    for shell in ctx.shells.borrow().iter() {
        shell.terminal.set_focused(false);
    }
    ctx.state
        .borrow_mut()
        .focus_memory
        .insert(ctx.path.clone(), "agent".to_string());
}

fn activate_shell_tab(ctx: &ShellTabContext, shell_id: usize, grab_focus: bool) {
    let changed = ctx.active_shell_id.get() != shell_id;
    ctx.active_shell_id.set(shell_id);
    ctx.shell_stack
        .set_visible_child_name(&shell_child_name(shell_id));

    let mut active_terminal = None;
    for shell in ctx.shells.borrow().iter() {
        let is_active = shell.id == shell_id;
        shell.terminal.set_focused(is_active);
        if is_active {
            active_terminal = Some(shell.terminal.clone());
        }
    }
    ctx.agent.set_focused(false);

    {
        let mut st = ctx.state.borrow_mut();
        st.focus_memory
            .insert(ctx.path.clone(), "shell".to_string());
        if let Some(pw) = st.project_widgets.get_mut(&ctx.path) {
            pw.shell_terminal = active_terminal.clone();
        }
    }

    if changed {
        refresh_shell_tab_bar(ctx);
    }
    if grab_focus {
        if let Some(terminal) = active_terminal {
            terminal.focus();
        }
    }
}

fn close_shell_tab(ctx: &ShellTabContext, shell_id: usize, refocus: bool) -> bool {
    let (removed, was_active, next_active_id) = {
        let mut shells = ctx.shells.borrow_mut();
        if shells.len() <= 1 {
            return false;
        }
        let Some(index) = shells.iter().position(|shell| shell.id == shell_id) else {
            return false;
        };
        let was_active = ctx.active_shell_id.get() == shell_id;
        let removed = shells.remove(index);
        let next_active_id = if was_active {
            shells
                .get(index)
                .or_else(|| shells.last())
                .map(|shell| shell.id)
        } else {
            Some(ctx.active_shell_id.get())
        };
        (removed, was_active, next_active_id)
    };

    ctx.shell_stack.remove(&removed.terminal.container);
    removed.terminal.shutdown();
    ctx.pending_exits.lock().unwrap().push(ctx.path.clone());

    if was_active {
        if let Some(next_id) = next_active_id {
            activate_shell_tab(ctx, next_id, refocus);
        }
    } else {
        refresh_shell_tab_bar(ctx);
    }
    true
}

fn refresh_shell_tab_bar(ctx: &ShellTabContext) {
    while let Some(child) = ctx.tab_box.first_child() {
        ctx.tab_box.remove(&child);
    }

    let grip = Label::new(Some("|||"));
    grip.add_css_class("terminal-grip");
    grip.set_tooltip_text(Some("Drag to resize terminals"));
    ctx.tab_box.append(&grip);

    let shells = ctx.shells.borrow().clone();
    let shell_count = shells.len();
    let active_id = ctx.active_shell_id.get();

    for shell in shells {
        let tab_btn = Button::with_label(&shell.label);
        tab_btn.set_has_frame(false);
        tab_btn.set_focus_on_click(false);
        tab_btn.add_css_class("shell-tab");
        if shell.id == active_id {
            tab_btn.add_css_class("shell-tab-active");
        }
        {
            let ctx = ctx.clone();
            let shell_id = shell.id;
            tab_btn.connect_clicked(move |_| {
                activate_shell_tab(&ctx, shell_id, true);
            });
        }
        ctx.tab_box.append(&tab_btn);

        if shell_count > 1 {
            let close_btn = Button::builder()
                .icon_name("window-close-symbolic")
                .has_frame(false)
                .focus_on_click(false)
                .tooltip_text("Close shell")
                .build();
            close_btn.add_css_class("shell-tab-close");
            {
                let ctx = ctx.clone();
                let shell_id = shell.id;
                close_btn.connect_clicked(move |_| {
                    close_shell_tab(&ctx, shell_id, true);
                });
            }
            ctx.tab_box.append(&close_btn);
        }
    }

    let spacer = GtkBox::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    ctx.tab_box.append(&spacer);

    let add_btn = Button::builder()
        .icon_name("list-add-symbolic")
        .has_frame(false)
        .focus_on_click(false)
        .tooltip_text("New shell")
        .build();
    {
        let ctx = ctx.clone();
        add_btn.connect_clicked(move |_| {
            add_shell_tab(&ctx, true);
        });
    }
    ctx.tab_box.append(&add_btn);
}

fn shutdown_terminal_tab(ctx: &ShellTabContext) {
    ctx.agent.shutdown();
    ctx.pending_exits.lock().unwrap().push(ctx.path.clone());

    let shells = ctx.shells.borrow().clone();
    for shell in shells {
        shell.terminal.shutdown();
        ctx.pending_exits.lock().unwrap().push(ctx.path.clone());
    }
}

// ── Git status view ────────────────────────────────────────────────────────

fn make_git_view() -> TextView {
    let view = TextView::new();
    view.set_editable(false);
    view.set_cursor_visible(false);
    view.set_monospace(true);
    view.set_wrap_mode(WrapMode::None);
    view.set_top_margin(4);
    view.set_bottom_margin(4);
    view.set_left_margin(8);
    view.set_right_margin(8);

    let buf = view.buffer();
    add_git_tag(&buf, "green", "foreground", "#50fa7b");
    add_git_tag(&buf, "red", "foreground", "#ff5555");
    add_git_tag(&buf, "yellow", "foreground", "#f1fa8c");
    add_git_tag(&buf, "dim", "foreground", "#888888");

    view
}

fn add_git_tag(buf: &gtk4::TextBuffer, name: &str, prop: &str, val: &str) {
    let tag = gtk4::TextTag::new(Some(name));
    tag.set_property(prop, val);
    buf.tag_table().add(&tag);
}

fn update_git_status(path: &str, view: &TextView) {
    let buf = view.buffer();
    buf.set_text("");

    let status = match sizzle_core::git::get_git_status(path.to_string()) {
        Some(s) => s,
        None => {
            git_insert(&buf, "(not a git repo)", "dim");
            return;
        }
    };

    let branch = status.branch.as_deref().unwrap_or("(detached HEAD)");
    git_insert(&buf, &format!("branch: {}", branch), "dim");
    if status.ahead > 0 {
        git_insert(&buf, &format!("  ↑{}", status.ahead), "green");
    }
    if status.behind > 0 {
        git_insert(&buf, &format!("  ↓{}", status.behind), "red");
    }
    git_insert(&buf, "\n", "dim");

    for f in &status.staged {
        git_insert(&buf, &format!("S {} {}\n", f.status, f.path), "green");
    }
    for f in &status.unstaged {
        let color = if f.status == "D" { "red" } else { "yellow" };
        git_insert(&buf, &format!("  {} {}\n", f.status, f.path), color);
    }
    for f in &status.untracked {
        git_insert(&buf, &format!("? {}\n", f), "dim");
    }

    if status.staged.is_empty() && status.unstaged.is_empty() && status.untracked.is_empty() {
        git_insert(&buf, "  clean\n", "dim");
    }
}

fn git_insert(buf: &gtk4::TextBuffer, text: &str, tag: &str) {
    let mut end = buf.end_iter();
    buf.insert_with_tags_by_name(&mut end, text, &[tag]);
}

// ── Folder picker + rescan ─────────────────────────────────────────────────

fn pick_folder_and_scan(state: &State, window: &ApplicationWindow) {
    let dialog = gtk4::FileDialog::builder()
        .title("Select Projects Folder")
        .build();

    let state = state.clone();
    dialog.select_folder(Some(window), gtk4::gio::Cancellable::NONE, move |result| {
        let Ok(file) = result else { return };
        let Some(path) = file.path() else { return };
        let path_str = path.to_string_lossy().to_string();

        let mut settings = {
            let st = state.borrow();
            st.store.get_scan_settings()
        };
        if !settings.scan_roots.contains(&path_str) {
            settings.scan_roots.push(path_str);
        }

        let new_projects = scan_projects(&settings);
        {
            let mut st = state.borrow_mut();
            st.store.set_scan_settings(&settings);
            st.projects = new_projects;
        }
        populate_list(&state);
    });
}

// ── Settings modal window ──────────────────────────────────────────────────

fn show_settings_window(parent: &ApplicationWindow, state: &State) {
    let win = ApplicationWindow::builder()
        .application(parent.application().as_ref().unwrap())
        .title("Settings")
        .modal(true)
        .transient_for(parent)
        .default_width(550)
        .default_height(450)
        .build();

    let notebook = Notebook::new();

    build_settings_path_tab(
        &notebook,
        parent,
        state,
        "Scan Roots",
        "Add scan folder…",
        |s| &s.scan_roots,
        |s, p| {
            if !s.scan_roots.contains(&p) {
                s.scan_roots.push(p);
            }
        },
        |s, p| {
            s.scan_roots.retain(|x| x != p);
        },
    );

    build_settings_path_tab(
        &notebook,
        parent,
        state,
        "Ignore Paths",
        "Add ignore path…",
        |s| &s.ignore_roots,
        |s, p| {
            if !s.ignore_roots.contains(&p) {
                s.ignore_roots.push(p);
            }
        },
        |s, p| {
            s.ignore_roots.retain(|x| x != p);
        },
    );

    build_settings_path_tab(
        &notebook,
        parent,
        state,
        "Manual Projects",
        "Add manual project…",
        |s| &s.manual_project_roots,
        |s, p| {
            if !s.manual_project_roots.contains(&p) {
                s.manual_project_roots.push(p);
            }
        },
        |s, p| {
            s.manual_project_roots.retain(|x| x != p);
        },
    );

    build_agent_presets_tab(&notebook, parent, state);

    win.set_child(Some(&notebook));
    win.present();
}

fn build_settings_path_tab(
    notebook: &Notebook,
    parent: &ApplicationWindow,
    state: &State,
    tab_label: &str,
    add_btn_label: &str,
    get_items: fn(&ScanSettings) -> &[String],
    add_item: fn(&mut ScanSettings, String),
    remove_item: fn(&mut ScanSettings, &str),
) {
    let state = state.clone();
    let list_box = ListBox::new();

    /// Populate the ListBox from the current scan settings.
    fn populate(
        list_box: &ListBox,
        state: &State,
        get_items: fn(&ScanSettings) -> &[String],
        remove_item: fn(&mut ScanSettings, &str),
    ) {
        while let Some(row) = list_box.row_at_index(0) {
            list_box.remove(&row);
        }
        let settings = state.borrow().store.get_scan_settings();
        let items = get_items(&settings).to_vec();
        for item in items {
            let lbl = Label::builder()
                .label(&item)
                .halign(gtk4::Align::Start)
                .hexpand(true)
                .margin_start(8)
                .margin_top(4)
                .margin_bottom(4)
                .ellipsize(gtk4::pango::EllipsizeMode::Middle)
                .build();
            let remove_btn = Button::builder()
                .label("✕")
                .has_frame(false)
                .tooltip_text("Remove")
                .build();
            let hbox = GtkBox::new(Orientation::Horizontal, 0);
            hbox.append(&lbl);
            hbox.append(&remove_btn);
            let row = ListBoxRow::new();
            row.set_child(Some(&hbox));
            list_box.append(&row);

            let state = state.clone();
            let list_box = list_box.clone();
            let item = item.clone();
            remove_btn.connect_clicked(move |_| {
                let mut settings = state.borrow().store.get_scan_settings();
                remove_item(&mut settings, &item);
                let projects = scan_projects(&settings);
                {
                    let mut st = state.borrow_mut();
                    st.store.set_scan_settings(&settings);
                    st.projects = projects;
                }
                populate_list(&state);
                populate(&list_box, &state, get_items, remove_item);
            });
        }
    }

    populate(&list_box, &state, get_items, remove_item);

    let scroll = ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .build();
    scroll.set_child(Some(&list_box));

    let add_btn_label = add_btn_label.to_string();
    let add_btn = Button::with_label(&add_btn_label);
    {
        let state = state.clone();
        let list_box = list_box.clone();
        let parent_win = parent.clone();
        let add_btn_label = add_btn_label.clone();
        add_btn.connect_clicked(move |_| {
            let dialog = gtk4::FileDialog::builder().title(&add_btn_label).build();
            let state = state.clone();
            let list_box = list_box.clone();
            dialog.select_folder(
                Some(&parent_win),
                gtk4::gio::Cancellable::NONE,
                move |result| {
                    let Ok(file) = result else { return };
                    let Some(path) = file.path() else { return };
                    let path_str = path.to_string_lossy().to_string();

                    let mut settings = state.borrow().store.get_scan_settings();
                    add_item(&mut settings, path_str);
                    let projects = scan_projects(&settings);
                    {
                        let mut st = state.borrow_mut();
                        st.store.set_scan_settings(&settings);
                        st.projects = projects;
                    }
                    populate_list(&state);
                    populate(&list_box, &state, get_items, remove_item);
                },
            );
        });
    }

    let vbox = GtkBox::new(Orientation::Vertical, 8);
    vbox.set_margin_start(12);
    vbox.set_margin_end(12);
    vbox.set_margin_top(12);
    vbox.set_margin_bottom(12);
    vbox.append(&scroll);
    vbox.append(&add_btn);

    notebook.append_page(&vbox, Some(&Label::new(Some(tab_label))));
}

// ── Agent Presets tab ──────────────────────────────────────────────────────

fn refresh_presets_listbox(parent: &ApplicationWindow, list_box: &ListBox, state: &State) {
    while let Some(row) = list_box.row_at_index(0) {
        list_box.remove(&row);
    }
    let presets = state.borrow().store.get_agent_presets();
    for (i, preset) in presets.iter().enumerate() {
        let label = preset.label.clone();
        let command = preset.command.clone();

        let lbl = Label::builder()
            .label(&format!("{} → {}", preset.label, preset.command))
            .halign(gtk4::Align::Start)
            .hexpand(true)
            .margin_start(8)
            .margin_top(4)
            .margin_bottom(4)
            .build();

        let edit_btn = Button::builder()
            .label("✎")
            .has_frame(false)
            .tooltip_text("Edit")
            .build();
        let remove_btn = Button::builder()
            .label("✕")
            .has_frame(false)
            .tooltip_text("Remove")
            .build();

        let hbox = GtkBox::new(Orientation::Horizontal, 0);
        hbox.append(&lbl);
        hbox.append(&edit_btn);
        hbox.append(&remove_btn);

        let row = ListBoxRow::new();
        row.set_child(Some(&hbox));
        list_box.append(&row);

        // Owned clones for edit button closure
        let state_e = state.clone();
        let list_box_e = list_box.clone();
        let parent_e = parent.clone();
        let edit_label = label.clone();
        let edit_command = command.clone();
        edit_btn.connect_clicked(move |_| {
            show_agent_preset_dialog(
                &parent_e,
                &state_e,
                &list_box_e,
                Some((edit_label.clone(), edit_command.clone(), i)),
            );
        });

        // Separate owned clones for remove button closure
        let state_r = state.clone();
        let list_box_r = list_box.clone();
        let parent_r = parent.clone();
        remove_btn.connect_clicked(move |_| {
            let mut presets = state_r.borrow().store.get_agent_presets();
            presets.remove(i);
            state_r.borrow().store.set_agent_presets(presets);
            refresh_presets_listbox(&parent_r, &list_box_r, &state_r);
        });
    }
}

fn build_agent_presets_tab(notebook: &Notebook, parent: &ApplicationWindow, state: &State) {
    let state = state.clone();
    let list_box = ListBox::new();
    refresh_presets_listbox(parent, &list_box, &state);

    let scroll = ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .build();
    scroll.set_child(Some(&list_box));

    let add_btn = Button::with_label("Add preset…");
    {
        let state = state.clone();
        let list_box = list_box.clone();
        let parent = parent.clone();
        add_btn.connect_clicked(move |_| {
            show_agent_preset_dialog(&parent, &state, &list_box, None);
        });
    }

    let vbox = GtkBox::new(Orientation::Vertical, 8);
    vbox.set_margin_start(12);
    vbox.set_margin_end(12);
    vbox.set_margin_top(12);
    vbox.set_margin_bottom(12);
    vbox.append(&scroll);
    vbox.append(&add_btn);

    notebook.append_page(&vbox, Some(&Label::new(Some("Agent Presets"))));
}

fn show_agent_preset_dialog(
    parent: &ApplicationWindow,
    state: &State,
    list_box: &ListBox,
    existing: Option<(String, String, usize)>,
) {
    let win = ApplicationWindow::builder()
        .application(parent.application().as_ref().unwrap())
        .title("Agent Preset")
        .modal(true)
        .transient_for(parent)
        .default_width(400)
        .build();

    let label_entry = Entry::builder()
        .placeholder_text("Label (e.g. DeepSeek)")
        .build();
    let cmd_entry = Entry::builder()
        .placeholder_text("Command (e.g. /usr/bin/deepseek)")
        .build();

    if let Some((ref label, ref cmd, _)) = existing {
        label_entry.set_text(label);
        cmd_entry.set_text(cmd);
    }

    let save_btn = Button::with_label("Save");
    let cancel_btn = Button::with_label("Cancel");

    let btn_box = GtkBox::new(Orientation::Horizontal, 8);
    btn_box.set_halign(gtk4::Align::End);
    btn_box.append(&cancel_btn);
    btn_box.append(&save_btn);

    let vbox = GtkBox::new(Orientation::Vertical, 8);
    vbox.set_margin_start(12);
    vbox.set_margin_end(12);
    vbox.set_margin_top(12);
    vbox.set_margin_bottom(12);
    vbox.append(&label_entry);
    vbox.append(&cmd_entry);
    vbox.append(&btn_box);

    win.set_child(Some(&vbox));

    let win_weak = win.downgrade();
    cancel_btn.connect_clicked(move |_| {
        if let Some(w) = win_weak.upgrade() {
            w.close();
        }
    });

    let state = state.clone();
    let list_box = list_box.clone();
    let parent = parent.clone();
    let edit_idx = existing.as_ref().map(|(_, _, i)| *i);
    let win_weak2 = win.downgrade();
    save_btn.connect_clicked(move |_| {
        let label = label_entry.text().trim().to_string();
        let command = cmd_entry.text().trim().to_string();
        if label.is_empty() || command.is_empty() {
            return;
        }

        let mut presets = state.borrow().store.get_agent_presets();
        if let Some(idx) = edit_idx {
            if idx < presets.len() {
                presets[idx] = AgentPreset { label, command };
            }
        } else {
            presets.push(AgentPreset { label, command });
        }
        state.borrow().store.set_agent_presets(presets);
        refresh_presets_listbox(&parent, &list_box, &state);

        if let Some(w) = win_weak2.upgrade() {
            w.close();
        }
    });

    win.present();
}

// ── Memory usage ──────────────────────────────────────────────────────────

fn process_rss_kb(pid: u32) -> Option<u64> {
    let content = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    for line in content.lines() {
        if line.starts_with("VmRSS:") {
            let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
            return Some(kb);
        }
    }
    None
}

fn process_cmdline(pid: u32) -> String {
    let raw = match std::fs::read(format!("/proc/{pid}/cmdline")) {
        Ok(r) => r,
        Err(_) => return String::new(),
    };
    raw.split(|&b| b == 0)
        .filter_map(|s| std::str::from_utf8(s).ok())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Walk /proc to find all descendants of the current process and sum their RSS,
/// categorized as "app" (main process + non-agent children) and "LLM agents"
/// (claude, codex processes spawned by terminals).
fn read_mem_breakdown() -> Option<(u64, u64)> {
    let current_pid = std::process::id();
    let mut all_pids = Vec::new();
    let entries = std::fs::read_dir("/proc").ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if let Some(pid_str) = name.to_str() {
            if let Ok(pid) = pid_str.parse::<u32>() {
                all_pids.push(pid);
            }
        }
    }

    // Build parent→children map
    let mut children: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
    for &pid in &all_pids {
        let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status")) else {
            continue;
        };
        for line in status.lines() {
            if line.starts_with("PPid:") {
                if let Some(ppid_str) = line.split_whitespace().nth(1) {
                    if let Ok(ppid) = ppid_str.parse::<u32>() {
                        children.entry(ppid).or_default().push(pid);
                    }
                }
                break;
            }
        }
    }

    // Collect all descendants of the current process
    let mut descendants = Vec::new();
    let mut stack = vec![current_pid];
    while let Some(pid) = stack.pop() {
        if let Some(kids) = children.get(&pid) {
            for &child in kids {
                descendants.push(child);
                stack.push(child);
            }
        }
    }

    let mut app_kb = match process_rss_kb(current_pid) {
        Some(kb) => kb,
        None => return None,
    };
    let mut agent_kb = 0u64;

    for &pid in &descendants {
        let Some(rss) = process_rss_kb(pid) else {
            continue;
        };
        let cmdline = process_cmdline(pid);
        let lower = cmdline.to_lowercase();
        if lower.contains("claude") || lower.contains("codex") {
            agent_kb += rss;
        } else {
            app_kb += rss;
        }
    }

    Some((app_kb, agent_kb))
}

// ── Move/Rename Project ──────────────────────────────────────────────────

fn path_is_under_any_root(path: &str, roots: &[String]) -> bool {
    let p = std::path::Path::new(path);
    roots.iter().any(|r| p.starts_with(r))
}

fn mangle_claude_path(path: &str) -> String {
    path.replace('/', "-")
}

// ── Trait for individual move/rename operations ─────────────────────────

trait MoveOp {
    /// Side-effect-free descriptions (used during dry run). Returns an empty
    /// vec if this operation would do nothing (preconditions not met).
    fn describe(&self) -> Vec<String>;
    /// Perform the operation for real. Returns human-readable results or an error.
    fn execute(&self) -> Result<Vec<String>, String>;
}

// ── 1. Move the directory on disk ───────────────────────────────────────

struct MoveDirOp {
    old: String,
    new: String,
}

impl MoveOp for MoveDirOp {
    fn describe(&self) -> Vec<String> {
        vec![format!("Move project directory:\n     {}\n  -> {}", self.old, self.new)]
    }

    fn execute(&self) -> Result<Vec<String>, String> {
        log::info!("MoveDirOp: {} -> {}", self.old, self.new);
        std::fs::rename(&self.old, &self.new)
            .map_err(|e| {
                log::error!("MoveDirOp failed: {} -> {}: {}", self.old, self.new, e);
                e.to_string()
            })?;
        Ok(vec![format!(
            "Moved project directory:\n  {}\n  -> {}",
            self.old, self.new
        )])
    }
}

// ── 2. Update metadata key in db.json ───────────────────────────────────

struct UpdateMetadataOp {
    state: State,
    old: String,
    new: String,
    db_path: PathBuf,
}

impl MoveOp for UpdateMetadataOp {
    fn describe(&self) -> Vec<String> {
        vec![format!(
            "Update project metadata ({}):\n     {}\n  -> {}",
            self.db_path.display(),
            self.old,
            self.new
        )]
    }

    fn execute(&self) -> Result<Vec<String>, String> {
        log::info!("UpdateMetadataOp: {} -> {}", self.old, self.new);
        self.state
            .borrow()
            .store
            .rename_project_metadata(&self.old, &self.new);
        Ok(vec!["Updated project metadata.".to_string()])
    }
}

// ── 3. Add destination to manual project roots ─────────────────────────

struct AddManualRootOp {
    state: State,
    old: String,
    new: String,
}

impl AddManualRootOp {
    fn should_apply(&self) -> bool {
        let settings = self.state.borrow().store.get_scan_settings();
        let was_under_scan_root = path_is_under_any_root(&self.old, &settings.scan_roots);
        let is_under_scan_root = path_is_under_any_root(&self.new, &settings.scan_roots);
        was_under_scan_root && !is_under_scan_root
    }
}

impl MoveOp for AddManualRootOp {
    fn describe(&self) -> Vec<String> {
        if !self.should_apply() {
            return vec![];
        }
        vec![format!("Add to manual project roots:\n     {}", self.new)]
    }

    fn execute(&self) -> Result<Vec<String>, String> {
        if !self.should_apply() {
            return Ok(vec![]);
        }
        log::info!("AddManualRootOp: {}", self.new);
        let mut settings = self.state.borrow().store.get_scan_settings();
        if !settings.manual_project_roots.contains(&self.new) {
            settings.manual_project_roots.push(self.new.clone());
            self.state.borrow().store.set_scan_settings(&settings);
        }
        Ok(vec!["Added destination to manual project roots.".to_string()])
    }
}

// ── 3b. Update existing manual project root entry ──────────────────────

struct UpdateManualRootOp {
    state: State,
    old: String,
    new: String,
}

impl UpdateManualRootOp {
    fn should_apply(&self) -> bool {
        let settings = self.state.borrow().store.get_scan_settings();
        let was_under_scan_root = path_is_under_any_root(&self.old, &settings.scan_roots);
        let was_manual = settings.manual_project_roots.iter().any(|r| r == &self.old);
        !was_under_scan_root && was_manual
    }
}

impl MoveOp for UpdateManualRootOp {
    fn describe(&self) -> Vec<String> {
        if !self.should_apply() {
            return vec![];
        }
        vec![format!(
            "Update manual project root:\n     {}\n  -> {}",
            self.old, self.new
        )]
    }

    fn execute(&self) -> Result<Vec<String>, String> {
        if !self.should_apply() {
            return Ok(vec![]);
        }
        log::info!("UpdateManualRootOp: {} -> {}", self.old, self.new);
        let mut settings = self.state.borrow().store.get_scan_settings();
        if let Some(pos) = settings.manual_project_roots.iter().position(|p| p == &self.old) {
            settings.manual_project_roots[pos] = self.new.clone();
            self.state.borrow().store.set_scan_settings(&settings);
        }
        Ok(vec!["Updated manual project root path.".to_string()])
    }
}

// ── 4. Rename Claude project data directory ────────────────────────────

struct MoveClaudeDirOp {
    old_claude: PathBuf,
    new_claude: PathBuf,
}

impl MoveClaudeDirOp {
    fn should_apply(&self) -> bool {
        let claude_dir = self.old_claude
            .parent()
            .and_then(|p| p.parent())
            .expect("~/.claude/projects path");
        claude_dir.exists() && self.old_claude.exists()
    }
}

impl MoveOp for MoveClaudeDirOp {
    fn describe(&self) -> Vec<String> {
        if !self.should_apply() {
            return vec![];
        }
        vec![format!(
            "Move Claude project data:\n     {}\n  -> {}",
            self.old_claude.display(),
            self.new_claude.display()
        )]
    }

    fn execute(&self) -> Result<Vec<String>, String> {
        if !self.should_apply() {
            return Ok(vec![]);
        }
        log::info!("MoveClaudeDirOp: {} -> {}", self.old_claude.display(), self.new_claude.display());
        std::fs::rename(&self.old_claude, &self.new_claude)
            .map_err(|e| {
                log::error!("MoveClaudeDirOp failed: {} -> {}: {}", self.old_claude.display(), self.new_claude.display(), e);
                e.to_string()
            })?;
        Ok(vec![format!(
            "Moved Claude project data:\n  {}\n  -> {}",
            self.old_claude.display(),
            self.new_claude.display()
        )])
    }
}

// ── 5. Update Codex config ─────────────────────────────────────────────

struct UpdateCodexConfigOp {
    config_path: PathBuf,
    old: String,
    new: String,
}

impl MoveOp for UpdateCodexConfigOp {
    fn describe(&self) -> Vec<String> {
        if !self.config_path.exists() {
            return vec![];
        }
        vec![format!(
            "Update Codex config ({}):\n     {}\n  -> {}",
            self.config_path.display(),
            self.old,
            self.new
        )]
    }

    fn execute(&self) -> Result<Vec<String>, String> {
        if !self.config_path.exists() {
            return Ok(vec![]);
        }
        log::info!("UpdateCodexConfigOp: {} -> {} in {}", self.old, self.new, self.config_path.display());
        let content = std::fs::read_to_string(&self.config_path)
            .map_err(|e| format!("Read error ({}): {}", self.config_path.display(), e))?;
        let updated = content.replace(&self.old, &self.new);
        if updated != content {
            std::fs::write(&self.config_path, &updated)
                .map_err(|e| format!("Write error ({}): {}", self.config_path.display(), e))?;
        }
        Ok(vec!["Updated Codex config with new path.".to_string()])
    }
}

// ── Planning — build the operation list ─────────────────────────────────

fn plan_move_operations(state: &State, old_path: &str, new_path: &str) -> Vec<Box<dyn MoveOp>> {
    let home = std::env::var("HOME").unwrap_or_default();

    vec![
        Box::new(MoveDirOp {
            old: old_path.to_string(),
            new: new_path.to_string(),
        }),
        Box::new(UpdateMetadataOp {
            state: state.clone(),
            old: old_path.to_string(),
            new: new_path.to_string(),
            db_path: config_dir().join("db.json"),
        }),
        Box::new(AddManualRootOp {
            state: state.clone(),
            old: old_path.to_string(),
            new: new_path.to_string(),
        }),
        Box::new(UpdateManualRootOp {
            state: state.clone(),
            old: old_path.to_string(),
            new: new_path.to_string(),
        }),
        Box::new(MoveClaudeDirOp {
            old_claude: {
                let dir = PathBuf::from(&home).join(".claude/projects");
                dir.join(mangle_claude_path(old_path))
            },
            new_claude: {
                let dir = PathBuf::from(&home).join(".claude/projects");
                dir.join(mangle_claude_path(new_path))
            },
        }),
        Box::new(UpdateCodexConfigOp {
            config_path: PathBuf::from(&home).join(".codex/config.toml"),
            old: old_path.to_string(),
            new: new_path.to_string(),
        }),
    ]
}

// ── Execution helpers ──────────────────────────────────────────────────

fn run_dry_run(ops: &[Box<dyn MoveOp>]) -> Vec<String> {
    ops.iter().flat_map(|op| op.describe()).collect()
}

fn run_execute(
    ops: &[Box<dyn MoveOp>],
    parent: &gtk4::Window,
) -> Result<Vec<String>, String> {
    let mut results = Vec::new();
    for op in ops {
        let descriptions = op.describe();
        match op.execute() {
            Ok(mut lines) => results.append(&mut lines),
            Err(e) => {
                log::error!(
                    "Failed to move project. Operation: {}. Error: {}",
                    descriptions.join("\n"),
                    e
                );
                let err_dialog = gtk4::AlertDialog::builder()
                    .message("Failed to move project")
                    .detail(&e)
                    .build();
                err_dialog.show(Some(parent));
                return Err(e);
            }
        }
    }
    Ok(results)
}

// ── Dialog: initial path entry ─────────────────────────────────────────

fn show_move_rename_dialog(state: &State, old_path: &str) {
    let main_window = state.borrow().main_window.clone();

    let win = ApplicationWindow::builder()
        .application(main_window.application().as_ref().unwrap())
        .title("Move/Rename Project")
        .modal(true)
        .transient_for(&main_window)
        .default_width(500)
        .build();

    let entry = Entry::builder()
        .text(old_path)
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .build();

    let warning_lbl = Label::builder()
        .label("")
        .margin_start(12)
        .margin_end(12)
        .margin_top(4)
        .build();

    let error_lbl = Label::builder()
        .label("")
        .margin_start(12)
        .margin_end(12)
        .margin_top(4)
        .build();

    let ok_btn = Button::with_label("Review Changes…");
    let cancel_btn = Button::with_label("Cancel");

    let btn_box = GtkBox::new(Orientation::Horizontal, 8);
    btn_box.set_halign(gtk4::Align::End);
    btn_box.set_margin_start(12);
    btn_box.set_margin_end(12);
    btn_box.set_margin_bottom(12);
    btn_box.append(&cancel_btn);
    btn_box.append(&ok_btn);

    let vbox = GtkBox::new(Orientation::Vertical, 0);
    vbox.append(&entry);
    vbox.append(&warning_lbl);
    vbox.append(&error_lbl);
    vbox.append(&btn_box);

    win.set_child(Some(&vbox));

    // Dynamic validation: check warnings and errors as the user types
    {
        let state = state.clone();
        let warn = warning_lbl.clone();
        let err = error_lbl.clone();
        let btn = ok_btn.clone();
        let old = old_path.to_string();
        entry.connect_changed(move |entry| {
            let raw = entry.text().to_string();
            let trimmed = raw.trim().to_string();

            // Reset
            warn.set_text("");
            err.set_text("");
            btn.set_sensitive(true);

            if trimmed.is_empty() || trimmed == old {
                return;
            }

            // Check ignore roots warning
            let settings = state.borrow().store.get_scan_settings();
            if path_is_under_any_root(&trimmed, &settings.ignore_roots) {
                warn.set_text(
                    "Warning: destination is under an ignored root — the project may be hidden.",
                );
            }

            // Check if destination exists and is non-empty or a file
            let dest = std::path::Path::new(&trimmed);
            if dest.exists() {
                if dest.is_file() {
                    err.set_text("Error: destination is a file, not a directory.");
                    btn.set_sensitive(false);
                } else if dest.is_dir() {
                    if dest.read_dir().map(|mut it| it.next().is_some()).unwrap_or(false) {
                        err.set_text("Error: destination is a non-empty directory.");
                        btn.set_sensitive(false);
                    }
                }
            }
        });
    }

    // Cancel
    let win_weak = win.downgrade();
    cancel_btn.connect_clicked(move |_| {
        if let Some(w) = win_weak.upgrade() {
            w.close();
        }
    });

    // Review — dry run, then show confirmation
    let entry_ok = entry.clone();
    let state_ok = state.clone();
    let old_path_owned = old_path.to_string();
    let win_weak2 = win.downgrade();
    let mw = main_window.clone();
    ok_btn.connect_clicked(move |_| {
        let raw = entry_ok.text().to_string();
        let new_path = raw.trim().to_string();
        if new_path.is_empty() || new_path == old_path_owned {
            return;
        }

        // Dry run: plan operations without performing them
        let ops = plan_move_operations(&state_ok, &old_path_owned, &new_path);
        let planned = run_dry_run(&ops);

        // Close the entry dialog
        if let Some(w) = win_weak2.upgrade() {
            w.close();
        }

        // Show confirmation
        show_move_rename_confirmation(&state_ok, &old_path_owned, &new_path, planned, &mw);
    });

    win.present();
}

// ── Dialog: confirmation with planned changes ──────────────────────────

fn show_move_rename_confirmation(
    state: &State,
    old_path: &str,
    new_path: &str,
    planned: Vec<String>,
    parent: &gtk4::Window,
) {
    let content = if planned.is_empty() {
        "No changes to make.".to_string()
    } else {
        planned.join("\n\n")
    };

    let buf = gtk4::TextBuffer::new(None);
    buf.set_text(&content);

    let text_view = TextView::builder()
        .editable(false)
        .monospace(true)
        .hexpand(true)
        .vexpand(true)
        .margin_start(8)
        .margin_end(8)
        .margin_top(8)
        .margin_bottom(8)
        .build();
    text_view.set_buffer(Some(&buf));

    let scroll = ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Automatic)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .hexpand(true)
        .vexpand(true)
        .child(&text_view)
        .build();

    let cancel_btn = Button::with_label("Cancel");
    let confirm_btn = Button::with_label("Confirm");

    let btn_box = GtkBox::new(Orientation::Horizontal, 8);
    btn_box.set_halign(gtk4::Align::End);
    btn_box.set_margin_start(12);
    btn_box.set_margin_end(12);
    btn_box.set_margin_bottom(12);
    btn_box.append(&cancel_btn);
    btn_box.append(&confirm_btn);

    let vbox = GtkBox::new(Orientation::Vertical, 0);
    vbox.append(&scroll);
    vbox.append(&btn_box);

    let win = ApplicationWindow::builder()
        .application(parent.application().as_ref().unwrap())
        .title("Move/Rename — Planned Changes")
        .modal(true)
        .transient_for(parent)
        .default_width(500)
        .default_height(400)
        .child(&vbox)
        .build();

    let weak = win.downgrade();
    cancel_btn.connect_clicked(move |_| {
        if let Some(w) = weak.upgrade() {
            w.close();
        }
    });

    let win_weak = win.downgrade();
    let state_c = state.clone();
    let old_path_c = old_path.to_string();
    let new_path_c = new_path.to_string();
    let mw_c = parent.clone();
    confirm_btn.connect_clicked(move |_| {
        if let Some(w) = win_weak.upgrade() {
            w.close();
        }
        let ops = plan_move_operations(&state_c, &old_path_c, &new_path_c);
        match run_execute(&ops, &mw_c) {
            Ok(results) => {
                let summary = results.join("\n");
                let detail = gtk4::AlertDialog::builder()
                    .message("Project moved successfully")
                    .detail(&summary)
                    .build();
                detail.show(Some(&mw_c));
            }
            Err(_) => {} // run_execute already showed an error dialog
        }
        // Rescan and refresh
        let settings = state_c.borrow().store.get_scan_settings();
        let new_projects = sizzle_core::scan_projects(&settings);
        {
            let mut st = state_c.borrow_mut();
            st.projects = new_projects;
        }
        populate_list(&state_c);
    });

    win.present();
}

// ── Add to Ignored Roots ─────────────────────────────────────────────────

fn add_to_ignored_roots(state: &State, path: &str) {
    let mut settings = state.borrow().store.get_scan_settings();
    if !settings.ignore_roots.contains(&path.to_string()) {
        settings.ignore_roots.push(path.to_string());
    }
    let new_projects = sizzle_core::scan_projects(&settings);
    {
        let mut st = state.borrow_mut();
        st.store.set_scan_settings(&settings);
        st.projects = new_projects;
    }
    populate_list(state);
}
