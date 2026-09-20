pub mod markdown;
pub mod terminal;
pub mod kanban;
pub(crate) mod timer;
mod project_list;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use gtk4::gdk;
use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, Button, CssProvider, DrawingArea, Entry,
    EventControllerKey, GestureClick, HeaderBar, Image, Label, ListBox, ListBoxRow, Notebook,
    Orientation, Paned, Picture, Popover, ScrolledWindow, Stack, StackTransitionType, TextView,
    WrapMode,
};

use sizzle_core::{scan_projects, AgentPreset, MetadataStore, ProjectMeta, ScanSettings, ScannedProject};
use crate::kanban::KanbanBoardWidget;
use crate::timer::TimerSlot;

// ── App state ─────────────────────────────────────────────────────────────

enum TabPageMeta {
    /// A markdown file tab or the Explorer preview pane — has a MarkdownView
    /// for file-change detection; git status uses the project root.
    Markdown { md_view: markdown::MarkdownView },
    /// A terminal tab (agent or shell) — may have a custom git worktree path;
    /// never has a markdown view.
    Terminal { git_path: Option<String> },
}

impl TabPageMeta {
    fn git_path(&self) -> Option<&str> {
        match self {
            TabPageMeta::Markdown { .. } => None,
            TabPageMeta::Terminal { git_path } => git_path.as_deref(),
        }
    }
}

struct ProjectWidgets {
    git_view: TextView,
    /// All terminal widgets for this project (agent + all shells across all
    /// tabs).  Used by `populate_list` to determine whether the green dot
    /// should be drawn — checking [`terminal::TerminalWidget::is_alive`]
    /// directly eliminates the possibility of counter drift.
    terminals: Vec<terminal::TerminalHandle>,
    /// The terminal that was most recently focused, restored when switching
    /// back to the project.
    focus_terminal: Option<terminal::TerminalHandle>,
    /// The notebook where agent/shell terminal tabs are added.
    notebook: Notebook,
    /// Maps each notebook page (child widget) to per-tab metadata: git
    /// worktree path and optional MarkdownView for file-change detection.
    tab_meta: HashMap<gtk4::glib::Object, TabPageMeta>,
}

#[derive(Clone)]
struct ShellTab {
    id: usize,
    label: String,
    terminal: terminal::TerminalWidget,
}

#[derive(Clone)]
struct ShellTabContext {
    working_dir: String,
    agent: terminal::TerminalHandle,
    agent_terminal: Rc<RefCell<Option<terminal::TerminalWidget>>>,
    shell_stack: Stack,
    tab_box: GtkBox,
    /// Per-project widget handle — focus handlers use this instead of global
    /// `State` so they cannot cause `RefCell` re-entrancy panics when GTK
    /// signals fire during a global state borrow.
    pw: Rc<RefCell<ProjectWidgets>>,
    /// Signals the 2-second timer to call `populate_list` when a terminal
    /// exits, so the left-pane green dot updates immediately.
    repopulate: Arc<AtomicBool>,
    shells: Rc<RefCell<Vec<ShellTab>>>,
    active_shell_id: Rc<Cell<usize>>,
    next_shell_id: Rc<Cell<usize>>,
}

struct AppState {
    store: Arc<MetadataStore>,
    /// The scanned projects, paired with a search-cache index that's always
    /// kept in sync with it. See `project_list::ProjectList`.
    project_list: ProjectList,
    project_widgets: HashMap<String, Rc<RefCell<ProjectWidgets>>>,
    project_stack: Stack,
    list_box: ListBox,
    /// Set to `true` when a terminal's child process exits or is shut down,
    /// triggering the 2-second timer to call `populate_list` so the left-pane
    /// dots reflect the new state.  Read-and-reset on every timer tick.
    repopulate: Arc<AtomicBool>,
    main_window: gtk4::Window,
    /// DrawingArea widgets for the green/yellow status dots, kept so the
    /// 2-second timer can redraw them in-place (for the yellow → green
    /// transition) without recreating the entire list.
    status_dots: Rc<RefCell<Vec<DrawingArea>>>,
    /// Kanban board widget, created once in build_ui.
    kanban_board: Option<KanbanBoardWidget>,
    /// File monitors watching scan root directories. Kept alive to prevent
    /// them from being dropped (which would stop monitoring).
    file_monitors: Vec<gtk4::gio::FileMonitor>,
    /// Pending debounce rescan timer. Canceled when a new change arrives
    /// before the previous timer fires.
    rescan_timer_id: TimerSlot,
    /// Current search query, stored so `populate_list` can re-apply the
    /// active filter after rebuilding the project list.
    search_query: String,
    /// Golden "M" badges for the git-dirty indicator, keyed by project path.
    /// The badge's own visibility is the single source of truth for its dirty
    /// state: `drain_git_results` toggles it in place, and a list rebuild reads
    /// it back to preserve the prior state.
    git_badges: Rc<RefCell<HashMap<String, Label>>>,
    /// Git remote buttons for projects, showing GitHub/globe icons.
    git_remote_btns: Rc<RefCell<HashMap<String, RemoteButtonState>>>,
    /// Worker → UI channel for completed git-status refresh results.
    git_result_tx: mpsc::Sender<HashMap<String, GitRefreshResult>>,
    git_result_rx: mpsc::Receiver<HashMap<String, GitRefreshResult>>,
    /// Set while a git-status worker is running, to avoid overlapping workers.
    git_refresh_in_progress: Arc<AtomicBool>,
}

#[derive(Clone, Debug)]
struct GitRefreshResult {
    dirty: bool,
    remote_info: Option<sizzle_core::git::GitRemoteInfo>,
}

#[derive(Clone)]
struct RemoteButtonState {
    button: Button,
    url_holder: Rc<RefCell<Option<String>>>,
    remote_info: Rc<RefCell<Option<sizzle_core::git::GitRemoteInfo>>>,
}

use project_list::{ProjectList, SearchEntry};

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

    let data_home = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(home).join(".local/share")
        });
    let icon_dir = data_home.join("icons/hicolor/scalable/apps");
    std::fs::create_dir_all(&icon_dir).ok();

    let app_svg = include_bytes!("../../../assets/icons/app-icon.svg");
    let app_icon_path = icon_dir.join("net.nebupookins.sizzle.svg");
    if !app_icon_path.exists() {
        let _ = std::fs::write(&app_icon_path, app_svg);
    }

    let github_svg = include_bytes!("../../../assets/icons/github.svg");
    let github_icon_path = icon_dir.join("sizzle-github.svg");
    let _ = std::fs::write(&github_icon_path, github_svg);
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
        .build();

    let search_box = GtkBox::new(Orientation::Horizontal, 0);
    search_box.add_css_class("app-sidebar-search");
    search_box.append(&search);

    let list_box = ListBox::new();
    list_box.set_selection_mode(gtk4::SelectionMode::Single);
    list_box.add_css_class("project-list");

    let scroll = ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .build();
    scroll.set_child(Some(&list_box));

    // ── Settings button opens a modal window ─────────────────────────────────
    let settings_btn = Button::with_label("Settings");
    settings_btn.set_has_frame(false);
    settings_btn.set_hexpand(true);
    settings_btn.add_css_class("settings-btn");

    let btn_row = GtkBox::new(Orientation::Horizontal, 0);
    btn_row.append(&settings_btn);

    // ── Memory breakdown widget ────────────────────────────────────────────
    let mem_total_lbl = Label::builder()
        .label("Memory: –")
        .halign(gtk4::Align::Start)
        .build();
    mem_total_lbl.add_css_class("mem-value");
    let mem_app_lbl = Label::builder()
        .label("")
        .halign(gtk4::Align::Start)
        .build();
    mem_app_lbl.add_css_class("mem-sublabel");
    let mem_agent_lbl = Label::builder()
        .label("")
        .halign(gtk4::Align::Start)
        .build();
    mem_agent_lbl.add_css_class("mem-sublabel");
    let mem_terminal_lbl = Label::builder()
        .label("")
        .halign(gtk4::Align::Start)
        .build();
    mem_terminal_lbl.add_css_class("mem-sublabel");
    let mem_vbox = GtkBox::new(Orientation::Vertical, 0);
    mem_vbox.append(&mem_total_lbl);
    mem_vbox.append(&mem_app_lbl);
    mem_vbox.append(&mem_agent_lbl);
    mem_vbox.append(&mem_terminal_lbl);

    // Hide the memory box until we have data
    let mem_sep = gtk4::Separator::new(Orientation::Horizontal);
    mem_sep.set_margin_start(4);
    mem_sep.set_margin_end(4);
    mem_vbox.set_tooltip_text(Some("Click for per-project memory breakdown"));
    mem_vbox.set_cursor_from_name(Some("pointer"));
    mem_vbox.set_visible(false);
    mem_sep.set_visible(false);

    let left = GtkBox::new(Orientation::Vertical, 0);
    left.add_css_class("app-sidebar");
    left.append(&search_box);
    left.append(&scroll);

    let sidebar_footer = GtkBox::new(Orientation::Vertical, 0);
    sidebar_footer.add_css_class("app-sidebar-footer");
    sidebar_footer.append(&btn_row);
    sidebar_footer.append(&mem_sep);
    sidebar_footer.append(&mem_vbox);
    left.append(&sidebar_footer);
    left.set_size_request(10, -1);

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

    let outer_paned = Paned::new(Orientation::Horizontal);
    outer_paned.set_start_child(Some(&left));
    outer_paned.set_end_child(Some(&project_stack));
    outer_paned.set_position(240);
    outer_paned.set_shrink_start_child(true);
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

    // ── Kanban board (added to project_stack before the placeholder) ────────
    let window_widget: gtk4::Window = window.clone().upcast();
    let kanban_board = KanbanBoardWidget::new(store.clone(), projects.clone(), &window_widget);

    let kanban_container = kanban_board.container.clone();
    kanban_container.set_hexpand(true);
    kanban_container.set_vexpand(true);
    project_stack.add_named(&kanban_container, Some("__kanban__"));

    let (git_result_tx, git_result_rx) = mpsc::channel();

    let state = Rc::new(RefCell::new(AppState {
        store: store.clone(),
        project_list: ProjectList::new(projects),
        project_widgets: HashMap::new(),
        project_stack: project_stack.clone(),
        list_box: list_box.clone(),
        repopulate: Arc::new(AtomicBool::new(false)),
        main_window: window.clone().upcast(),
        status_dots: Rc::new(RefCell::new(Vec::new())),
        kanban_board: Some(kanban_board),
        file_monitors: Vec::new(),
        rescan_timer_id: TimerSlot::default(),
        search_query: String::new(),
        git_badges: Rc::new(RefCell::new(HashMap::new())),
        git_remote_btns: Rc::new(RefCell::new(HashMap::new())),
        git_result_tx,
        git_result_rx,
        git_refresh_in_progress: Arc::new(AtomicBool::new(false)),
    }));

    {
        let mem_popover = Popover::new();
        mem_popover.set_parent(&mem_vbox);

        let mem_breakdown_view = TextView::new();
        mem_breakdown_view.set_editable(false);
        mem_breakdown_view.set_cursor_visible(false);
        mem_breakdown_view.set_monospace(true);
        mem_breakdown_view.set_wrap_mode(WrapMode::None);

        let mem_scroll = ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Automatic)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .min_content_width(560)
            .min_content_height(280)
            .build();
        mem_scroll.set_child(Some(&mem_breakdown_view));
        mem_popover.set_child(Some(&mem_scroll));

        let state = state.clone();
        let mem_popover = mem_popover.clone();
        let mem_breakdown_view = mem_breakdown_view.clone();
        let click = GestureClick::new();
        click.connect_pressed(move |_, _, _, _| {
            let text = {
                let st = state.borrow();
                format_project_mem_breakdown(&st)
            };
            mem_breakdown_view.buffer().set_text(&text);
            mem_popover.popup();
        });
        mem_vbox.add_controller(click);
    }

    // Set up kanban agent launch callback.
    state.borrow().kanban_board.as_ref().unwrap().set_on_launch_agent({
        let state = state.clone();
        move |project_path, working_dir, agent_label, card_id| {
            log::info!("[kanban] Launch callback: project_path={}, working_dir={}, agent={}, card_id={}",
                project_path, working_dir, agent_label, card_id);
            // Ensure the project is initialized so its widgets exist.
            select_project(&state, &project_path);

            let (agent_cmd, tab_label) = {
                let st = state.borrow();
                let cmd = match agent_label.as_str() {
                    "Claude" => Some("claude".to_string()),
                    "Codex" => Some("codex".to_string()),
                    _ => st.store.get_agent_presets().iter()
                        .find(|p| p.label == agent_label)
                        .map(|p| p.command.clone()),
                };
                (cmd, agent_label)
            };

            if let Some(cmd) = agent_cmd {
                let notebook = state.borrow().project_widgets.get(&project_path)
                    .map(|pw| pw.borrow().notebook.clone());
                if let Some(nb) = notebook {
                    log::info!("[kanban] Launching terminal: working_dir={}", working_dir);
                    let (agent, paned) = launch_terminals(
                        &project_path, &working_dir, &tab_label, Some(cmd), &nb, &state,
                    );
                    // Register the session with the kanban board and refresh
                    // so the green dot appears on the card.
                    if let Some(kb) = state.borrow().kanban_board.as_ref() {
                        kb.register_active_session(&card_id, &agent, &paned, &project_path);
                        kb.refresh_self();
                    }
                } else {
                    log::warn!("[kanban] No project widget found for path: {}", project_path);
                }
            }
        }
    });

    // Set up kanban focus-existing-session callback.
    state.borrow().kanban_board.as_ref().unwrap().set_on_focus_session({
        let state = state.clone();
        move |project_path, card_id| {
            log::info!("[kanban] Focus session: project_path={}, card_id={}", project_path, card_id);
            if project_path.is_empty() { return; }
            select_project(&state, &project_path);
            state.borrow().store.set_last_launched(&project_path);
            // Extract notebook and terminal outside the borrow scope, because
            // calling set_current_page / focus() triggers focus-enter signals
            // that also borrow state (via activate_agent_terminal), causing
            // RefCell::borrow_mut to panic.
            let Some((notebook, page_num, terminal)) = ({
                let st = state.borrow();
                st.kanban_board.as_ref().and_then(|kb| {
                    kb.get_active_session_info(&card_id).and_then(|(_pp, paned, terminal)| {
                        let pw = st.project_widgets.get(&_pp)?;
                        let pw = pw.borrow();
                        pw.notebook.page_num(&paned)
                            .map(|pn| (pw.notebook.clone(), pn, terminal.clone()))
                    })
                })
            }) else {
                return;
            };
            notebook.set_current_page(Some(page_num));
            terminal.focus();
        }
    });

    populate_list(&state);
    refresh_git_statuses(&state);
    setup_file_monitors(&state);
    start_poll_timer(&state);

    {
        let state = state.clone();
        search.connect_changed(move |entry| {
            state.borrow_mut().search_query = entry.text().to_string();
            apply_search_filter(&state.borrow());
        });
    }

    {
        let state = state.clone();
        list_box.connect_row_activated(move |_, row| {
            let path = row.widget_name().to_string();
            if path == "__separator__" {
                return;
            }
            if path == "__kanban__" {
                let st = state.borrow();
                st.project_stack.set_visible_child_name("__kanban__");
                st.main_window.set_title(Some("Sizzle"));
            } else {
                select_project(&state, &path);
                state.borrow().store.set_last_launched(&path);
            }
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

    // Auto-prompt to pick a folder if no scan roots are configured
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

    let timer_state = state.clone();
    glib::timeout_add_local(Duration::from_secs(2), move || {
        // Rebuild the left-pane list when a terminal has exited (child died
        // naturally or was shut down), so the green dot is removed.
        if timer_state
            .borrow()
            .repopulate
            .swap(false, Ordering::Acquire)
        {
            populate_list(&timer_state);
        }

        // Kanban: clean up dead sessions and refresh board if needed.
        // Runs on every tick so that card green dots update promptly when
        // agent terminals exit (even those not launched via kanban).
        if let Some(ref kb) = timer_state.borrow().kanban_board {
            if kb.cleanup_dead_sessions() {
                kb.refresh_self();
            }
        }

        // Refresh status dots (yellow/green) while any terminal is alive —
        // queue a redraw on each dot DrawingArea in-place rather than
        // recreating all list rows (which was the source of a severe memory
        // leak).
        {
            let s = timer_state.borrow();
            let has_alive = s
                .project_widgets
                .values()
                .any(|pw| pw.borrow().terminals.iter().any(|t| t.is_alive()));
            if has_alive {
                let dots = s.status_dots.borrow();
                for dot in dots.iter() {
                    dot.queue_draw();
                }
            }
        }

        // Apply any completed git-status refresh and toggle the "M" badges.
        drain_git_results(&timer_state);

        if let Some(mem) = read_mem_breakdown() {
            let total_mb = mem.total_kb() / 1024;
            let app_mb = mem.sizzle_kb / 1024;
            let agent_mb = mem.agent_kb / 1024;
            let terminal_mb = mem.terminal_app_kb / 1024;
            mem_total_lbl.set_text(&format!("Memory   Total: {} MB", total_mb));
            mem_app_lbl.set_text(&format!("Sizzle   {} MB", app_mb));
            mem_agent_lbl.set_text(&format!("Agents   {} MB", agent_mb));
            mem_terminal_lbl.set_text(&format!("Child    {} MB", terminal_mb));
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
                        let pw = pw.borrow();
                        update_git_status_for_current_tab(&pw, &path);
                    }
                }
            }
            glib::ControlFlow::Continue
        });
    }

    // ── Full dark theme CSS ──────────────────────────────────────────────
    let css_provider = CssProvider::new();
    css_provider.load_from_data(
        "window {
             background-color: #0d0b14;
             color: #ede8f8;
         }
         headerbar {
             background-color: #1d1933;
             border-bottom: 1px solid #2e2952;
             color: #ede8f8;
             min-height: 38px;
             padding: 0 8px;
             box-shadow: none;
         }
         .app-sidebar {
             background-color: #100d1c;
         }
         .app-sidebar-search {
             padding: 8px 10px;
             border-bottom: 1px solid #2e2952;
             background-color: #100d1c;
         }
         .app-sidebar-search entry {
             background-color: #1d1933;
             border: 1px solid #2e2952;
             border-radius: 6px;
             color: #ede8f8;
             min-height: 30px;
             padding: 4px 8px;
             caret-color: #ede8f8;
         }
         .app-sidebar-search entry:focus {
             border-color: #ff5533;
         }
         entry placeholder {
             color: #514b6f;
         }
         .project-list {
             background-color: #100d1c;
         }
         .project-list > row {
             background-color: transparent;
             border-left: 2px solid transparent;
             padding: 0;
             border-radius: 0;
         }
         .project-list > row:hover {
             background-color: #231e3e;
         }
         .project-list > row:selected,
         .project-list > row:active {
             background-color: #2c2554;
             border-left: 2px solid #ff5533;
             color: #ede8f8;
         }
         .project-list > row:selected * {
             color: inherit;
         }
         .project-name {
             font-size: 13px;
             font-weight: 500;
             color: #ede8f8;
         }
         .project-time {
             font-size: 11px;
             color: #514b6f;
         }
         .project-time-running {
             font-size: 11px;
             color: #00ccee;
         }
         .project-tag {
             font-size: 9px;
             font-weight: 700;
             color: #514b6f;
             background-color: #2e2952;
             border-radius: 3px;
             padding: 1px 5px;
         }
         .project-ignored {
             opacity: 0.35;
         }
         .marker-favorite {
             color: #ffd700;
         }
         .marker-ignored {
             color: #ff4444;
         }
         .marker-neutral {
             color: #514b6f;
         }
         .git-dirty {
             color: #ffd700;
             font-size: 10px;
             font-weight: 700;
         }
         .git-remote-btn {
             padding: 0 2px;
             min-width: 0;
             min-height: 0;
             margin-start: 2px;
             color: #9088b8;
         }
         .git-remote-btn:hover {
             color: #ede8f8;
         }
         .app-sidebar-footer {
             border-top: 1px solid #2e2952;
             background-color: #100d1c;
             padding: 10px 12px;
         }
         .mem-value {
             font-size: 11px;
             color: #9088b8;
             font-family: \"JetBrains Mono\", Monospace;
         }
         .mem-sublabel {
             font-size: 10px;
             color: #514b6f;
         }
         button.settings-btn {
             background-color: #1d1933;
             border: 1px solid #2e2952;
             border-radius: 6px;
             color: #9088b8;
             font-size: 12px;
             padding: 5px 10px;
             margin-top: 4px;
         }
         button.settings-btn:hover {
             background-color: #231e3e;
             color: #ede8f8;
         }
         .launch-toolbar {
             background-color: #1d1933;
             border-bottom: 1px solid #2e2952;
             padding: 0 12px;
             min-height: 38px;
         }
         button.launch-btn {
             background-color: #231e3e;
             border: 1px solid #2e2952;
             border-radius: 5px;
             color: #9088b8;
             font-size: 12px;
             font-weight: 500;
             padding: 4px 12px;
             margin: 0 3px;
         }
         button.launch-btn:hover {
             background-color: #2c2554;
             color: #ede8f8;
         }
         button.launch-claude {
             background-color: #2c2554;
             color: #a6e3a1;
             border-color: rgba(255,85,51,0.3);
         }
         button.launch-codex {
             color: #89dceb;
         }
         notebook > header {
             background-color: #100d1c;
             border-bottom: 1px solid #2e2952;
             padding: 0;
             min-height: 35px;
         }
         notebook > header > tabs {
             margin: 0;
             padding: 0;
         }
         notebook > header > tabs > tab {
             background-color: transparent;
             color: #514b6f;
             font-size: 12px;
             padding: 0 14px;
             border-right: 1px solid #2e2952;
             border-bottom: 2px solid transparent;
             min-height: 35px;
             margin: 0;
             border-radius: 0;
         }
         notebook > header > tabs > tab:checked {
             background-color: #14112a;
             color: #ede8f8;
             font-weight: 500;
             border-bottom: 2px solid #ff5533;
         }
         notebook > header > tabs > tab:hover:not(:checked) {
             background-color: #1d1933;
             color: #9088b8;
         }
         notebook > header > tabs > tab:checked reorder-placeholder {
             background-color: transparent;
         }
         notebook > header.top > tabs > tab {
             box-shadow: none;
         }
         .content-area {
             background-color: #14112a;
         }
         .git-pane {
             background-color: #100d1c;
             border-left: 1px solid #2e2952;
         }
         .git-pane-header {
             background-color: #1d1933;
             border-bottom: 1px solid #2e2952;
             min-height: 36px;
             padding: 0 14px;
         }
         .git-pane-header-label {
             font-size: 10px;
             font-weight: 700;
             letter-spacing: 1px;
             color: #514b6f;
         }
         .git-pane textview {
             background-color: #100d1c;
             color: #9088b8;
             font-family: \"JetBrains Mono\", Monospace;
             font-size: 11px;
             padding: 12px;
         }
         .git-pane textview text {
             background-color: #100d1c;
         }
         .git-modified  { color: #f5a62a; }
         .git-branch    { color: #00ccee; }
         .git-untracked { color: #514b6f; }
         .terminal-area {
             background-color: #08070d;
         }
         .terminal-middle-bar {
             background-color: #1d1933;
             border-top: 1px solid #2e2952;
             border-bottom: 1px solid #2e2952;
             padding: 0 10px;
             min-height: 33px;
         }
         .terminal-grip {
             color: #514b6f;
             padding: 0 4px;
             opacity: 0.5;
         }
         button.shell-tab {
             background-color: transparent;
             border: none;
             border-radius: 4px;
             color: #9088b8;
             font-size: 11px;
             font-weight: 500;
             padding: 2px 10px;
             min-height: 22px;
         }
         button.shell-tab-active {
             background-color: #2c2554;
             color: #ede8f8;
         }
         button.shell-tab:hover:not(.shell-tab-active) {
             background-color: #231e3e;
         }
         button.shell-tab-close {
             min-width: 18px;
             min-height: 18px;
             padding: 0;
             border-radius: 3px;
             background-color: transparent;
             color: #514b6f;
             font-size: 10px;
             border: none;
         }
         button.shell-tab-close:hover {
             background-color: #231e3e;
             color: #ede8f8;
         }
         button.shell-tab-add {
             background-color: transparent;
             border: 1px solid #2e2952;
             border-radius: 4px;
             color: #514b6f;
             min-width: 22px;
             min-height: 22px;
             padding: 0;
             font-size: 14px;
             border-style: dashed;
         }
         button.shell-tab-add:hover {
             background-color: #231e3e;
             color: #9088b8;
         }
         popover {
             background-color: #1d1933;
             border: 1px solid #2e2952;
             border-radius: 6px;
         }
         popover > contents {
             background-color: #1d1933;
             border-radius: 6px;
             padding: 4px;
         }
         popover label {
             color: #9088b8;
         }
         popover button {
             background-color: transparent;
             color: #9088b8;
             font-size: 12px;
             padding: 5px 10px;
             border-radius: 4px;
             border: none;
         }
         popover button:hover {
             background-color: #231e3e;
             color: #ede8f8;
         }
         scrollbar {
             background-color: transparent;
             border: none;
             padding: 0;
         }
         scrollbar.vertical {
             margin-left: 1px;
         }
         scrollbar slider {
             background-color: rgba(255,255,255,0.08);
             border-radius: 3px;
             min-width: 5px;
             min-height: 20px;
             border: none;
         }
         scrollbar slider:hover {
             background-color: rgba(255,255,255,0.15);
         }
         scrollbar trough {
             background-color: transparent;
             border: none;
         }
         separator {
             background-color: #2e2952;
             min-height: 1px;
             min-width: 1px;
         }
         .caption {
             font-size: 11px;
             color: #514b6f;
         }
         .markdown-view, .markdown-view text {
             background-color: #14112a;
             color: #ede8f8;
             font-size: 13px;
         }
         .markdown-edit-btn {
             background-color: #1d1933;
             border: 1px solid #2e2952;
             border-radius: 5px;
             color: #9088b8;
             font-size: 12px;
             padding: 4px 14px;
         }
         .markdown-edit-btn:hover {
             background-color: #231e3e;
             color: #ede8f8;
         }
         .explorer-file-list {
             background-color: #100d1c;
             border-right: 1px solid #2e2952;
         }
         .explorer-file-list listbox {
             background: transparent;
         }
         .explorer-file-list row {
             background-color: transparent;
             padding: 3px 10px;
             font-size: 12px;
             color: #ede8f8;
             font-family: \"JetBrains Mono\", Monospace;
         }
         .explorer-file-list row:hover {
             background-color: #231e3e;
         }
         .explorer-file-list row:selected {
             background-color: #2c2554;
             color: #ede8f8;
         }
         .explorer-dir-row {
             color: #9088b8;
             font-family: Cantarell, sans-serif;
         }
         .explorer-path-label {
             font-size: 11px;
             color: #514b6f;
             font-family: \"JetBrains Mono\", Monospace;
             padding: 8px 14px 4px;
         }
         .explorer-content textview {
             background-color: #14112a;
             color: #cdd6f4;
             font-family: \"JetBrains Mono\", Monospace;
             font-size: 11px;
             padding: 16px 20px;
         }
         .explorer-content textview text {
             background-color: #14112a;
         }
         dialog {
             background-color: #1d1933;
             color: #ede8f8;
         }
         dialog entry {
             background-color: #14112a;
             border: 1px solid #2e2952;
             border-radius: 5px;
             color: #ede8f8;
             padding: 6px 10px;
             caret-color: #ede8f8;
         }
         dialog button {
             background-color: #231e3e;
             border: 1px solid #2e2952;
             border-radius: 5px;
             color: #9088b8;
             padding: 6px 16px;
         }
         dialog button:hover {
             background-color: #2c2554;
             color: #ede8f8;
         }
         dialog button.suggested-action {
             background-color: #ff5533;
             border-color: #ff5533;
             color: #ffffff;
             font-weight: 600;
         }
         dialog button.suggested-action:hover {
             background-color: #e04420;
         }
         /* ── Root containers — kill grey ──────────────────────────── */
         paned {
             background-color: #0d0b14;
             background: #0d0b14;
         }
         paned separator {
             background-color: #2e2952;
             min-width: 1px;
             min-height: 1px;
         }
         stack {
             background: transparent;
         }
         notebook {
             background: transparent;
         }
         notebook stack {
             background: transparent;
         }
         scrolledwindow {
             background-color: transparent;
             border: none;
         }
         scrolledwindow viewport {
             background: transparent;
         }
         .app-sidebar box {
             background: transparent;
         }
         .content-area scrolledwindow {
             background-color: transparent;
         }
         .content-area stack {
             background: transparent;
         }
         /* ── Kanban board ────────────────────────────────────────── */
         .kanban-board {
             background-color: #14112a;
         }
         .kanban-board scrollbar.horizontal {
             min-height: 8px;
         }
         .kanban-column {
             background-color: #1d1933;
             border: 1px solid #2e2952;
             border-radius: 8px;
             min-width: 240px;
         }
         .kanban-col-header {
             border-bottom: 1px solid #2e2952;
         }
         .kanban-col-title {
             font-size: 13px;
             font-weight: 600;
             color: #ede8f8;
         }
         .kanban-col-wip {
             font-size: 11px;
             color: #514b6f;
         }
         .kanban-wip-over {
             color: #ff5533;
             font-weight: 700;
         }
         .kanban-col-menu {
             color: #514b6f;
             font-size: 14px;
             padding: 0 4px;
             min-width: 20px;
             min-height: 20px;
             border-radius: 4px;
         }
         .kanban-col-menu:hover {
             background-color: #2c2554;
             color: #ede8f8;
         }
         .kanban-card-scroll {
             min-height: 80px;
         }
         .kanban-card {
             background-color: #231e3e;
             border: 1px solid #2e2952;
             border-radius: 6px;
             margin: 2px 0;
         }
         .kanban-card:hover {
             border-color: #ff5533;
         }
         .kanban-card-title {
             font-size: 12px;
             font-weight: 500;
             color: #ede8f8;
         }
         .kanban-card-meta {
             font-size: 10px;
             color: #514b6f;
         }
         .kanban-card-blocked {
             font-size: 10px;
             color: #f5a62a;
             margin-top: 2px;
         }
         .kanban-card-ready {
             font-size: 10px;
             color: #50fa7b;
             margin-top: 2px;
         }
         .kanban-card-active {
             font-size: 10px;
             color: #50fa7b;
         }
         .kanban-add-card-btn {
             background-color: transparent;
             color: #514b6f;
             font-size: 12px;
             padding: 6px 8px;
             margin: 4px 6px;
             border: 1px dashed #2e2952;
             border-radius: 5px;
         }
         .kanban-add-card-btn:hover {
             background-color: #231e3e;
             color: #9088b8;
             border-style: solid;
         }
         .kanban-add-col-btn {
             background-color: #1d1933;
             color: #514b6f;
             font-size: 12px;
             padding: 8px 16px;
             border: 1px dashed #2e2952;
             border-radius: 8px;
             min-width: 220px;
         }
         .kanban-add-col-btn:hover {
             background-color: #231e3e;
             color: #9088b8;
             border-style: solid;
         }
         .kanban-sidebar-row {
             border-bottom: 1px solid #2e2952;
         }
         .kanban-sidebar-row:selected {
             border-left: 2px solid #00ccee !important;
         }
         .kanban-sidebar-icon {
             font-size: 16px;
             color: #00ccee;
         }
         .kanban-sidebar-text {
             font-size: 13px;
             font-weight: 600;
             color: #89dceb;
         }
         .context-menu-item {
             padding: 4px 10px;
             font-size: 12px;
             border-radius: 4px;
         }
         .context-menu-item:hover {
             background-color: #2c2554;
         }
         .dialog-field-label {
             font-size: 11px;
             font-weight: 600;
             color: #9088b8;
         }
         .dialog-field-required {
             font-size: 9px;
             font-weight: 400;
             color: #ff5533;
             font-style: italic;
         }
         .dialog-btn {
             padding: 6px 16px;
             font-size: 12px;
             border-radius: 5px;
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

pub(crate) fn marker_sort_key(marker: Option<&str>) -> u8 {
    match marker {
        Some("favorite") => 0,
        Some("ignored") => 2,
        _ => 1,
    }
}

/// Shared sort key for consistent project ordering across the UI
/// (left panel, kanban board dropdown, etc.).
pub(crate) fn project_sort_key(
    project: &ScannedProject,
    meta: Option<&ProjectMeta>,
    has_active_terminal: bool,
) -> (bool, u8, std::cmp::Reverse<Option<i64>>, String) {
    let marker_key = marker_sort_key(meta.and_then(|m| m.marker.as_deref()));
    let time_key = std::cmp::Reverse(meta.and_then(|m| m.last_launched));
    let name_key = project.name.to_lowercase();
    (!has_active_terminal, marker_key, time_key, name_key)
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

/// Apply the current search filter text to all rows in the project list.
/// Sets each row visible or hidden based on whether the project name or
/// tag matches the query.  Called both when the user types in the search
/// entry and after `populate_list` rebuilds the list (so the filter
/// survives rescans).
fn apply_search_filter(st: &AppState) {
    let query = st.search_query.as_str();
    let query_lower = query.to_lowercase();
    let case_sensitive = query.chars().any(|c| c.is_uppercase());

    // `search_cache` holds the pre-lowercased name/tag for every project
    // (rebuilt only when the project list changes), so matching here is a
    // single O(1) map lookup per row plus a substring check — no
    // to_lowercase() or uppercase-scan calls on the hot path of every
    // keystroke.
    let matches = |entry: &SearchEntry| entry.matches(query, &query_lower, case_sensitive);

    let mut child = st.list_box.first_child();
    while let Some(row) = child {
        let path = row.widget_name();
        let visible = match path.as_str() {
            // Always show kanban entry and its separator.
            "__kanban__" | "__separator__" => true,
            _ if query.is_empty() => true,
            _ => st
                .project_list
                .search_cache()
                .get(path.as_str())
                .map_or(false, &matches),
        };
        row.set_visible(visible);
        child = row.next_sibling();
    }
}

fn populate_list(state: &State) {
    // Clean up dead terminal references before rebuilding, so the per-project
    // vec doesn't grow unbounded when tabs are closed or processes exit.
    for pw in state.borrow().project_widgets.values() {
        pw.borrow_mut().terminals.retain(|t| t.is_alive());
    }

    let st = state.borrow();
    // Clone the Rc so we can mutate the inner vec without needing &mut AppState.
    let dots_rc = st.status_dots.clone();
    dots_rc.borrow_mut().clear();
    // Take the previous badges so a rebuild preserves each project's last-known
    // dirty state until the next background refresh lands.
    let git_badges_rc = st.git_badges.clone();
    let prev_git_badges = git_badges_rc.replace(HashMap::new());

    let git_remote_btns_rc = st.git_remote_btns.clone();
    let prev_git_remote_btns = git_remote_btns_rc.replace(HashMap::new());

    while let Some(row) = st.list_box.row_at_index(0) {
        // Break the ref cycle created by context_popover.set_parent(&row).
        // GTK4's set_parent makes the child hold a strong ref to its parent,
        // so we must explicitly unparent popovers before dropping the row,
        // otherwise the row and popover keep each other alive indefinitely.
        // We iterate through first_child/last_child to catch popovers that may
        // not appear in the first_child chain in all GTK4 versions.
        if let Some(first) = row.first_child() {
            let mut c = Some(first.clone());
            let last = row.last_child();
            while let Some(child) = c {
                c = child.next_sibling();
                if child.is::<Popover>() {
                    child.unparent();
                }
            }
            if let Some(l) = last {
                if Some(&l) != row.last_child().as_ref() {
                    // Re-check from the end if the list was modified by unparent
                    let mut c = Some(l.clone());
                    while let Some(child) = c {
                        c = child.prev_sibling();
                        if child.is::<Popover>() {
                            child.unparent();
                        }
                    }
                }
            }
        }
        st.list_box.remove(&row);
    }

    // ── "Kanban Board" entry at the top of the list ─────────────────────────
    {
        let kanban_icon = Label::builder()
            .label("▦")
            .css_classes(["kanban-sidebar-icon"])
            .build();
        let kanban_label = Label::builder()
            .label("Kanban Board")
            .halign(gtk4::Align::Start)
            .css_classes(["kanban-sidebar-text"])
            .build();

        let row_box = GtkBox::new(Orientation::Horizontal, 6);
        row_box.set_margin_start(14);
        row_box.set_margin_top(8);
        row_box.set_margin_bottom(6);
        row_box.append(&kanban_icon);
        row_box.append(&kanban_label);

        let row = ListBoxRow::new();
        row.set_widget_name("__kanban__");
        row.add_css_class("kanban-sidebar-row");
        row.set_child(Some(&row_box));
        st.list_box.append(&row);

        // Separator after kanban entry
        let sep = gtk4::Separator::new(Orientation::Horizontal);
        let sep_row = ListBoxRow::new();
        sep_row.set_widget_name("__separator__");
        sep_row.set_child(Some(&sep));
        sep_row.set_selectable(false);
        sep_row.set_focusable(false);
        st.list_box.append(&sep_row);
    }

    let all_meta = st.store.get_all_metadata();

    let mut sorted: Vec<&ScannedProject> = st.project_list.projects().iter().collect();
    sorted.sort_by_key(|p| {
        let is_active = st
            .project_widgets
            .get(&p.path)
            .map_or(false, |pw| pw.borrow().terminals.iter().any(|t| t.is_alive()));
        let meta = all_meta.get(&p.path);
        project_sort_key(p, meta, is_active)
    });

    for project in sorted {
        let meta = all_meta.get(&project.path);
        let marker = meta.and_then(|m| m.marker.as_deref()).unwrap_or("");
        let is_ignored = marker == "ignored";

        let tag = project_list::primary_tag(project);

        let is_running = st
            .project_widgets
            .get(&project.path)
            .map_or(false, |pw| pw.borrow().terminals.iter().any(|t| t.is_alive()));

        let last_active = meta
            .and_then(|m| m.last_launched)
            .map(format_relative_time)
            .unwrap_or_else(|| "never".to_string());

        let last_provider = meta.and_then(|m| m.last_provider.as_deref());

        let name_lbl = Label::builder()
            .label(&project.name)
            .halign(gtk4::Align::Start)
            .ellipsize(pango::EllipsizeMode::End)
            .single_line_mode(true)
            .build();
        name_lbl.add_css_class("project-name");

        let name_row = GtkBox::new(Orientation::Horizontal, 6);

        name_lbl.set_margin_start(14);
        name_row.append(&name_lbl);

        // Git-dirty badge: a golden "M" shown when the project has modified
        // tracked files. The previous flag is carried over on rebuild; the
        // background refresh updates it in place via `drain_git_results`.
        let prev_dirty = prev_git_badges
            .get(&project.path)
            .map(|b| b.is_visible())
            .unwrap_or(false);
        let git_badge = Label::builder().label("M").build();
        git_badge.add_css_class("git-dirty");
        git_badge.set_margin_start(6);
        git_badge.set_tooltip_text(Some("Modified tracked files"));
        git_badge.set_visible(prev_dirty);
        git_badges_rc
            .borrow_mut()
            .insert(project.path.clone(), git_badge.clone());
        name_row.append(&git_badge);

        // Git remote icon button
        let prev_remote_info = prev_git_remote_btns
            .get(&project.path)
            .and_then(|st| st.remote_info.borrow().clone());

        let url_holder = Rc::new(RefCell::new(prev_remote_info.as_ref().and_then(|r| r.web_url.clone())));
        let remote_info_holder = Rc::new(RefCell::new(prev_remote_info.clone()));

        let remote_btn = Button::builder()
            .has_frame(false)
            .visible(false)
            .build();
        remote_btn.add_css_class("git-remote-btn");

        {
            let url_holder = url_holder.clone();
            remote_btn.connect_clicked(move |_| {
                if let Some(ref url) = *url_holder.borrow() {
                    let _ = gtk4::gio::AppInfo::launch_default_for_uri(url, None::<&gtk4::gio::AppLaunchContext>);
                }
            });
        }

        update_remote_button(&remote_btn, &url_holder, prev_remote_info.as_ref());

        git_remote_btns_rc.borrow_mut().insert(
            project.path.clone(),
            RemoteButtonState {
                button: remote_btn.clone(),
                url_holder,
                remote_info: remote_info_holder,
            },
        );
        name_row.append(&remote_btn);

        // Tag badge
        if !tag.is_empty() {
            let tag_lbl = Label::builder()
                .label(&tag.to_uppercase())
                .build();
            tag_lbl.add_css_class("project-tag");
            name_row.append(&tag_lbl);
        }

        let status = if is_running { "running" } else { last_active.as_str() };
        let last_display = match last_provider {
            Some(p) => format!("{} · {}", p, status),
            None if is_running => "claude · running".to_string(),
            None => status.to_string(),
        };
        let time_lbl = Label::builder()
            .label(&last_display)
            .halign(gtk4::Align::Start)
            .margin_start(14)
            .ellipsize(pango::EllipsizeMode::End)
            .single_line_mode(true)
            .build();
        if is_running {
            time_lbl.add_css_class("project-time-running");
        } else {
            time_lbl.add_css_class("project-time");
        }

        let info_box = GtkBox::new(Orientation::Vertical, 2);
        info_box.set_hexpand(true);
        info_box.append(&name_row);
        info_box.append(&time_lbl);

        if is_ignored {
            info_box.add_css_class("project-ignored");
        }

        let (icon, tooltip, css_class, marker_opacity) = match marker {
            "favorite" => ("★", "Mark as ignored", "marker-favorite", 1.0),
            "ignored" => ("🗑", "Reset to default", "marker-ignored", 1.0),
            _ => ("☆", "Mark as favourite", "marker-neutral", 0.35),
        };
        let marker_btn = Button::with_label(icon);
        marker_btn.set_has_frame(false);
        marker_btn.add_css_class(css_class);
        marker_btn.set_opacity(marker_opacity);
        marker_btn.set_tooltip_text(Some(tooltip));
        marker_btn.set_valign(gtk4::Align::Center);
        marker_btn.set_margin_end(4);

        let row_box = GtkBox::new(Orientation::Horizontal, 0);

        // Status dot: yellow if recent terminal activity, green if idle, hidden if
        // no terminals are alive.  The draw function captures live `Arc` refs so
        // `queue_draw()` gives a fresh colour without recreating the entire list.
        if let Some(pw) = st.project_widgets.get(&project.path) {
            let pw = pw.borrow();
            if pw.terminals.iter().any(|t| t.is_alive()) {
                let activity_refs: Vec<Arc<Mutex<Option<Instant>>>> =
                    pw.terminals.iter().map(|t| t.last_activity()).collect();

                let dot = DrawingArea::new();
                dot.set_size_request(8, 8);
                dot.set_valign(gtk4::Align::Center);
                dot.set_margin_start(8);
                dot.set_draw_func(move |_, cr, w, h| {
                    let has_recent = activity_refs.iter().any(|a| {
                        a.lock()
                            .unwrap()
                            .map(|t| t.elapsed() < Duration::from_secs(5))
                            .unwrap_or(false)
                    });
                    let r = (w.min(h) as f64 / 2.0).min(4.0);
                    if has_recent {
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
                dots_rc.borrow_mut().push(dot.clone());
                row_box.append(&dot);
            }
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
    // Re-apply any active search filter after rebuilding the list, so the
    // filter survives rescans triggered by the file monitor or poll timer.
    // Rows are already visible after a rebuild, so skip the pass when idle.
    if !st.search_query.is_empty() {
        apply_search_filter(&st);
    }
    // st (the immutable borrow) is still alive at this point but no longer used.
    // Drop it so we can re-borrow state for the kanban update.
    drop(st);
    if let Some(kb) = state.borrow().kanban_board.as_ref() {
        let projects = state.borrow().project_list.projects().to_vec();
        let window = state.borrow().main_window.clone();
        kb.set_projects(projects, &window);
    }
}

// ── Switch to a project ────────────────────────────────────────────────────

/// Open a project's view (build it if needed), switch to it, and restore focus.
/// Launch metadata is recorded by the caller: `set_last_launched` for a plain
/// open, `launch_terminals` (via `set_launch_metadata`) for an agent launch.
fn select_project(state: &State, path: &str) {
    let path = path.to_string();

    let found = {
        let st = state.borrow();
        st.project_list.projects().iter().any(|p| p.path == path)
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
            let mut tab_meta: HashMap<gtk4::glib::Object, TabPageMeta> = HashMap::new();
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
                edit_btn.add_css_class("markdown-edit-btn");

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
                let project_path_save = path.clone();
                let mv_save = mv.clone();
                let eb3 = edit_btn.clone();
                let sb3 = save_btn.clone();
                let cb3 = cancel_btn.clone();
                save_btn.connect_clicked(move |_| {
                    let text = mv_save.get_buffer_text();
                    mv_save.set_source(&text);
                    if sizzle_core::files::write_markdown_file(project_path_save.clone(), fp.clone(), text).is_ok() {
                        mv_save.set_editable(false);
                        eb3.set_visible(true);
                        sb3.set_visible(false);
                        cb3.set_visible(false);
                    }
                });

                // Ctrl+S to save
                {
                    let mv_saver = mv.clone();
                    let fp = md_path.clone();
                    let project_path_save = path.clone();
                    let eb4 = edit_btn.clone();
                    let sb4 = save_btn.clone();
                    let cb4 = cancel_btn.clone();
                    let ctrl_key = EventControllerKey::new();
                    ctrl_key.connect_key_pressed(move |_, kv, _, mods| {
                        if kv == gdk::Key::s && mods.contains(gdk::ModifierType::CONTROL_MASK) {
                            let text = mv_saver.get_buffer_text();
                            mv_saver.set_source(&text);
                            if sizzle_core::files::write_markdown_file(project_path_save.clone(), fp.clone(), text).is_ok() {
                                mv_saver.set_editable(false);
                                eb4.set_visible(true);
                                sb4.set_visible(false);
                                cb4.set_visible(false);
                            }
                            return glib::Propagation::Stop;
                        }
                        glib::Propagation::Proceed
                    });
                    mv.view().add_controller(ctrl_key);
                }

                mv.set_file_path(md_path);

                let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                container.append(&toolbar);
                container.append(&mv.scroll);

                notebook.append_page(&container, Some(&Label::new(Some(&tab_name))));
                tab_meta.insert(
                    container.clone().upcast::<gtk4::glib::Object>(),
                    TabPageMeta::Markdown { md_view: mv.clone() },
                );
            }

            let (explorer, explorer_md) = build_explorer_tab(&path);
            notebook.append_page(&explorer, Some(&Label::new(Some("Explorer"))));
            tab_meta.insert(
                explorer.clone().upcast::<gtk4::glib::Object>(),
                TabPageMeta::Markdown { md_view: explorer_md },
            );

            // ── Launch buttons ─────────────────────────────────────────────
            let btn_box = GtkBox::new(Orientation::Horizontal, 4);
            btn_box.set_margin_start(8);
            btn_box.set_margin_end(8);
            btn_box.set_margin_top(4);
            btn_box.set_margin_bottom(4);
            btn_box.set_halign(gtk4::Align::End);

            let claude_btn = Button::with_label("◉ Claude");
            claude_btn.add_css_class("launch-btn");
            claude_btn.add_css_class("launch-claude");
            let codex_btn = Button::with_label("⬡ Codex");
            codex_btn.add_css_class("launch-btn");
            codex_btn.add_css_class("launch-codex");
            let shell_btn = Button::with_label("$ Shell");
            shell_btn.add_css_class("launch-btn");
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
            top_bar.add_css_class("launch-toolbar");
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
                .label("GIT STATUS")
                .halign(gtk4::Align::Start)
                .build();
            git_header.add_css_class("git-pane-header-label");
            let git_header_box = GtkBox::new(Orientation::Horizontal, 0);
            git_header_box.set_valign(gtk4::Align::Center);
            git_header_box.add_css_class("git-pane-header");
            git_header_box.append(&git_header);
            git_container.append(&git_header_box);
            git_container.append(&git_scroll);

            let project_box = GtkBox::new(Orientation::Vertical, 0);
            project_box.add_css_class("content-area");
            project_box.append(&top_bar);
            project_box.append(&notebook);

            // ── Connect launch buttons ─────────────────────────────────────
            {
                let nb = notebook.clone();
                let p = path.clone();
                let st = state.clone();
                claude_btn.connect_clicked(move |_| {
                    let _ = launch_terminals(&p, &p, "Claude", Some("claude".to_string()), &nb, &st);
                });
            }
            {
                let nb = notebook.clone();
                let p = path.clone();
                let st = state.clone();
                codex_btn.connect_clicked(move |_| {
                    let _ = launch_terminals(&p, &p, "Codex", Some("codex".to_string()), &nb, &st);
                });
            }
            {
                let nb = notebook.clone();
                let p = path.clone();
                let st = state.clone();
                shell_btn.connect_clicked(move |_| {
                    let _ = launch_terminals(&p, &p, "Shell", None, &nb, &st);
                });
            }
            for (btn, label, cmd) in preset_btns {
                let nb = notebook.clone();
                let p = path.clone();
                let st = state.clone();
                btn.connect_clicked(move |_| {
                    let _ = launch_terminals(&p, &p, &label, Some(cmd.clone()), &nb, &st);
                });
            }

            // Wrap project content + git status in a horizontal Paned so the
            // git panel is only shown when a project is active, not when
            // viewing the kanban board or placeholder.
            let project_paned = Paned::new(Orientation::Horizontal);
            project_paned.set_start_child(Some(&project_box));
            project_paned.set_end_child(Some(&git_container));
            project_paned.set_position(700);
            project_paned.set_shrink_start_child(false);

            st.project_stack.add_named(&project_paned, Some(&path));
            st.project_widgets.insert(
                path.clone(),
                Rc::new(RefCell::new(ProjectWidgets {
                    git_view,
                    terminals: Vec::new(),
                    focus_terminal: None,
                    notebook: notebook.clone(),
                    tab_meta,
                })),
            );

            // Connect switch-page signal: update git panel when the user
            // switches to a different editor tab.
            {
                let sp_state = state.clone();
                let sp_path = path.clone();
                notebook.connect_switch_page(move |_nb, page_child, _page_num| {
                    let st = sp_state.borrow();
                    if let Some(pw) = st.project_widgets.get(&sp_path) {
                        let pw = pw.borrow();
                        let key = page_child.upcast_ref::<gtk4::glib::Object>();
                        if let Some(meta) = pw.tab_meta.get(key) {
                            let git_path = meta.git_path().unwrap_or(&sp_path);
                            update_git_status(git_path, &pw.git_view);
                            if let TabPageMeta::Markdown { md_view } = meta {
                                md_view.check_and_reload();
                            }
                        }
                    }
                });
            }

            // Clean up tab_meta entries when a page is removed.
            {
                let rp_state = state.clone();
                let rp_path = path.clone();
                notebook.connect_page_removed(move |_nb, child, _page_num| {
                    if let Some(st) = rp_state.try_borrow().ok() {
                        if let Some(pw) = st.project_widgets.get(&rp_path) {
                            let key: &gtk4::glib::Object = child.upcast_ref();
                            pw.borrow_mut().tab_meta.remove(key);
                        }
                    }
                });
            }

            // Open to first markdown tab; fall back to Explorer
            notebook.set_current_page(Some(0));
        }
    }

    // Restore the terminal that was last focused in this project.
    // Important: clone the terminal out first, then call focus() outside the
    // borrow, because focus() triggers the focus-in callback which also borrows
    // state.
    let terminal = {
        let st = state.borrow();
        st.project_stack.set_visible_child_name(&path);
        if let Some(pw) = st.project_widgets.get(&path) {
            let pw = pw.borrow();
            update_git_status_for_current_tab(&pw, &path);
        }
        if let Some(project) = st.project_list.projects().iter().find(|p| p.path == path) {
            st.main_window
                .set_title(Some(&format!("Sizzle – {}", project.name)));
        }
        st.project_widgets
            .get(&path)
            .and_then(|pw| pw.borrow().focus_terminal.clone())
    };
    if let Some(t) = terminal {
        t.focus();
    }
}

// ── Explorer tab ──────────────────────────────────────────────────────────

fn build_explorer_tab(project_root: &str) -> (Paned, markdown::MarkdownView) {
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

    path_lbl.add_css_class("explorer-path-label");
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
    list_scroll.add_css_class("explorer-file-list");
    file_list.add_css_class("explorer-file-list");
    list_scroll.set_child(Some(&file_list));

    let explorer_left = GtkBox::new(Orientation::Vertical, 0);
    explorer_left.add_css_class("explorer-file-list");
    explorer_left.append(&nav_bar);
    explorer_left.append(&list_scroll);
    explorer_left.set_size_request(10, -1);

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
    md_edit_btn.add_css_class("markdown-edit-btn");
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
        let project_path_save = project_root.to_string();
        md_save_btn.connect_clicked(move |_| {
            let text = mv.get_buffer_text();
            let file_path = path.borrow().clone();
            if let Some(fp) = file_path {
                mv.set_source(&text);
                if sizzle_core::files::write_markdown_file(project_path_save.clone(), fp, text).is_ok() {
                    mv.set_editable(false);
                    eb3.set_visible(true);
                    sb3.set_visible(false);
                    cb3.set_visible(false);
                }
            }
        });
    }

    // Ctrl+S to save in explorer
    {
        let mv = md_view.clone();
        let path = current_md_path.clone();
        let project_path_save = project_root.to_string();
        let eb = md_edit_btn.clone();
        let sb = md_save_btn.clone();
        let cb = md_cancel_btn.clone();
        let ctrl_key = EventControllerKey::new();
        ctrl_key.connect_key_pressed(move |_, kv, _, mods| {
            if kv == gdk::Key::s && mods.contains(gdk::ModifierType::CONTROL_MASK) {
                let text = mv.get_buffer_text();
                let file_path = path.borrow().clone();
                if let Some(fp) = file_path {
                    mv.set_source(&text);
                    if sizzle_core::files::write_markdown_file(project_path_save.clone(), fp, text).is_ok() {
                        mv.set_editable(false);
                        eb.set_visible(true);
                        sb.set_visible(false);
                        cb.set_visible(false);
                    }
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        md_view.view().add_controller(ctrl_key);
    }

    content_stack.set_visible_child_name("placeholder");

    let explorer_content = GtkBox::new(Orientation::Vertical, 0);
    explorer_content.add_css_class("explorer-content");
    explorer_content.append(&content_stack);

    // ── Paned ──────────────────────────────────────────────────────────────
    let paned = Paned::new(Orientation::Horizontal);
    paned.set_start_child(Some(&explorer_left));
    paned.set_end_child(Some(&explorer_content));
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

    (paned, md_view)
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
        if entry.is_directory {
            row.add_css_class("explorer-dir-row");
        }
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
                md_view.set_file_path(file_path);
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
    project_path: &str,
    working_dir: &str,
    tab_label: &str,
    agent_cmd: Option<String>,
    notebook: &Notebook,
    state: &State,
) -> (terminal::TerminalWidget, gtk4::Paned) {
    // Record the launch (last_launched + last_provider) in one write. The
    // provider is the button's label (`tab_label`), never the command line, so
    // a custom preset like "DeepSeek" → `claude-deepseek` shows as "DeepSeek".
    state.borrow().store.set_launch_metadata(project_path, tab_label);

    let agent = terminal::TerminalWidget::new(Some(working_dir), agent_cmd);
    let agent_handle = agent.handle();
    let repopulate = state.borrow().repopulate.clone();
    // Register the agent terminal so `populate_list` can draw a green dot.
    if let Some(pw) = state.borrow().project_widgets.get(project_path) {
        pw.borrow_mut().terminals.push(agent_handle.clone());
    }

    {
        let repopulate = repopulate.clone();
        agent.set_on_exit(move || {
            repopulate.store(true, Ordering::Release);
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
    bottom.add_css_class("terminal-area");
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

    // Extract the per-project handle so focus handlers borrow this RefCell
    // instead of global State, preventing RefCell re-entrancy panics.
    let pw = state.borrow().project_widgets.get(project_path)
        .cloned()
        .expect("project_widgets entry must exist to launch terminals");

    let ctx = ShellTabContext {
        working_dir: working_dir.to_string(),
        agent: agent_handle.clone(),
        agent_terminal: Rc::new(RefCell::new(Some(agent.clone()))),
        shell_stack,
        tab_box: middle_bar,
        pw,
        repopulate: repopulate.clone(),
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

    let _shell = add_shell_tab(&ctx, false);
    populate_list(state);
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
    // Record the git path for this tab so the switch-page handler shows the
    // correct git status when this tab is selected.
    let tab_git_path = if project_path != working_dir {
        Some(working_dir.to_string())
    } else {
        None
    };
    if let Some(pw) = state.borrow().project_widgets.get(project_path) {
        pw.borrow_mut().tab_meta.insert(
            vpaned.clone().upcast::<gtk4::glib::Object>(),
            TabPageMeta::Terminal { git_path: tab_git_path },
        );
    }
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

    // Store the focused terminal for focus memory restoration when switching back.
    if let Some(pw) = state.borrow().project_widgets.get(project_path) {
        pw.borrow_mut().focus_terminal = Some(agent_handle);
    }

    (agent, vpaned)
}

fn shell_child_name(id: usize) -> String {
    format!("shell-{id}")
}

fn add_shell_tab(ctx: &ShellTabContext, focus: bool) -> terminal::TerminalWidget {
    let id = ctx.next_shell_id.get();
    ctx.next_shell_id.set(id + 1);

    let shell = terminal::TerminalWidget::new(Some(&ctx.working_dir), None);
    let shell_handle = shell.handle();
    let child_name = shell_child_name(id);
    ctx.shell_stack
        .add_named(&shell.container, Some(&child_name));

    // Register the shell terminal so `populate_list` can draw a green dot.
    ctx.pw.borrow_mut().terminals.push(shell_handle);

    {
        let repopulate = ctx.repopulate.clone();
        shell.set_on_exit(move || {
            repopulate.store(true, Ordering::Release);
        });
    }

    ctx.shells.borrow_mut().push(ShellTab {
        id,
        label: format!("Shell {id}"),
        terminal: shell.clone(),
    });

    wire_shell_terminal(ctx, &shell, id);
    activate_shell_tab(ctx, id, focus);
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
        let shell_owned = shell.clone();
        shell.connect_key_pressed_capture(move |kv, mods| {
            let is_ctrl_w = kv == gdk::Key::w && mods.contains(gdk::ModifierType::CONTROL_MASK);
            if is_ctrl_w && !shell_owned.is_alive() {
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
    ctx.pw.borrow_mut().focus_terminal = Some(ctx.agent.clone());
}

fn activate_shell_tab(ctx: &ShellTabContext, shell_id: usize, grab_focus: bool) {
    let changed = ctx.active_shell_id.get() != shell_id;
    ctx.active_shell_id.set(shell_id);
    ctx.shell_stack
        .set_visible_child_name(&shell_child_name(shell_id));

    let mut active_terminal = None;
    let mut focus_terminal = None;
    for shell in ctx.shells.borrow().iter() {
        let is_active = shell.id == shell_id;
        shell.terminal.set_focused(is_active);
        if is_active {
            active_terminal = Some(shell.terminal.handle());
            focus_terminal = Some(shell.terminal.clone());
        }
    }
    ctx.agent.set_focused(false);

    ctx.pw.borrow_mut().focus_terminal = active_terminal.clone();

    if changed {
        refresh_shell_tab_bar(ctx);
    }
    if grab_focus {
        if let Some(terminal) = focus_terminal {
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
    ctx.repopulate.store(true, Ordering::Release);

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

    let add_btn = Button::builder()
        .label("+")
        .has_frame(false)
        .focus_on_click(false)
        .tooltip_text("New shell")
        .build();
    add_btn.add_css_class("shell-tab-add");
    {
        let ctx = ctx.clone();
        add_btn.connect_clicked(move |_| {
            add_shell_tab(&ctx, true);
        });
    }
    ctx.tab_box.append(&add_btn);

    let spacer = GtkBox::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    ctx.tab_box.append(&spacer);

    let grip = Label::new(Some("⋮⋮"));
    grip.add_css_class("terminal-grip");
    grip.set_tooltip_text(Some("Drag to resize terminals"));
    ctx.tab_box.append(&grip);
}

fn shutdown_terminal_tab(ctx: &ShellTabContext) {
    if let Some(agent) = ctx.agent_terminal.borrow_mut().take() {
        agent.shutdown();
    }
    // Agent set_on_exit already sets repopulate; the explicit signal here
    // covers the case where shutdown() doesn't trigger a ChildExit event.
    ctx.repopulate.store(true, Ordering::Release);

    let shells = {
        let mut shells = ctx.shells.borrow_mut();
        std::mem::take(&mut *shells)
    };
    for shell in shells {
        shell.terminal.shutdown();
    }

    while let Some(child) = ctx.tab_box.first_child() {
        ctx.tab_box.remove(&child);
    }

    let mut pw = ctx.pw.borrow_mut();
    pw.focus_terminal = None;
    pw.terminals.retain(|terminal| terminal.is_alive());
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

/// Update the git panel to show the status for the currently selected notebook
/// tab's associated git path.  Falls back to `project_path` when the tab has no
/// specific worktree path set.
fn update_git_status_for_current_tab(pw: &ProjectWidgets, project_path: &str) {
    let git_path = pw.notebook.current_page()
        .and_then(|page| pw.notebook.nth_page(Some(page)))
        .and_then(|c| {
            let key: &gtk4::glib::Object = c.upcast_ref();
            pw.tab_meta.get(key).and_then(|m| m.git_path().map(String::from))
        })
        .unwrap_or_else(|| project_path.to_string());
    update_git_status(&git_path, &pw.git_view);
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
        insert_change(&buf, f, "S ", "green");
    }
    for f in &status.unstaged {
        let color = if f.status == "D" { "red" } else { "yellow" };
        insert_change(&buf, f, "  ", color);
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

/// Insert one changed-file row: its status letter + path (prefixed and colored
/// per its section), followed by a `+N -M` line-change estimate when available.
fn insert_change(buf: &gtk4::TextBuffer, f: &sizzle_core::git::GitFileChange, prefix: &str, color: &str) {
    git_insert(buf, &format!("{}{} {}", prefix, f.status, f.path), color);
    insert_diff_stat(buf, f.diff);
    git_insert(buf, "\n", "dim");
}

/// Append a `+N -M` line-change estimate for a file, or nothing when there is
/// no stat (binary, untracked, or a pure rename/mode change).
fn insert_diff_stat(buf: &gtk4::TextBuffer, diff: Option<sizzle_core::git::DiffStat>) {
    let Some(diff) = diff else { return };
    git_insert(buf, &format!(" +{}", diff.added), "green");
    git_insert(buf, &format!(" -{}", diff.deleted), "red");
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
        apply_scan_settings(&state, &settings);
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
                apply_scan_settings(&state, &settings);
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
                    apply_scan_settings(&state, &settings);
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

fn process_status(pid: u32) -> Option<(u32, u64)> {
    let content = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let mut ppid = None;
    let mut rss_kb = None;
    for line in content.lines() {
        if line.starts_with("PPid:") {
            ppid = line
                .split_whitespace()
                .nth(1)
                .and_then(|value| value.parse::<u32>().ok());
        } else if line.starts_with("VmRSS:") {
            rss_kb = line
                .split_whitespace()
                .nth(1)
                .and_then(|value| value.parse::<u64>().ok());
        }

        if ppid.is_some() && rss_kb.is_some() {
            break;
        }
    }
    Some((ppid?, rss_kb.unwrap_or(0)))
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

struct ProcessSnapshot {
    parent: HashMap<u32, u32>,
    children: HashMap<u32, Vec<u32>>,
    rss_kb: HashMap<u32, u64>,
    cmdline: HashMap<u32, String>,
}

impl ProcessSnapshot {
    fn read() -> Option<Self> {
        let mut parent = HashMap::new();
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut rss_kb = HashMap::new();
        let mut cmdline = HashMap::new();

        let entries = std::fs::read_dir("/proc").ok()?;
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(pid_str) = name.to_str() else {
                continue;
            };
            let Ok(pid) = pid_str.parse::<u32>() else {
                continue;
            };
            let Some((ppid, rss)) = process_status(pid) else {
                continue;
            };

            parent.insert(pid, ppid);
            children.entry(ppid).or_default().push(pid);
            rss_kb.insert(pid, rss);
            cmdline.insert(pid, process_cmdline(pid));
        }

        Some(Self {
            parent,
            children,
            rss_kb,
            cmdline,
        })
    }

    fn rss_kb(&self, pid: u32) -> Option<u64> {
        self.rss_kb.get(&pid).copied()
    }

    fn cmdline(&self, pid: u32) -> &str {
        self.cmdline.get(&pid).map(|s| s.as_str()).unwrap_or("")
    }
}

#[derive(Clone, Copy)]
struct MemBreakdown {
    sizzle_kb: u64,
    agent_kb: u64,
    terminal_app_kb: u64,
}

impl MemBreakdown {
    fn total_kb(&self) -> u64 {
        self.sizzle_kb + self.agent_kb + self.terminal_app_kb
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DescendantMemBucket {
    Agent,
    TerminalApp,
}

fn is_agent_process(cmdline: &str) -> bool {
    let lower = cmdline.to_lowercase();
    lower.contains("claude") || lower.contains("codex")
}

fn accumulate_descendant_memory(
    snapshot: &ProcessSnapshot,
    root_pid: u32,
    root_bucket: DescendantMemBucket,
    agent_kb: &mut u64,
    terminal_app_kb: &mut u64,
) {
    let mut stack = vec![(root_pid, root_bucket)];

    while let Some((pid, inherited_bucket)) = stack.pop() {
        let bucket = if inherited_bucket == DescendantMemBucket::Agent
            || is_agent_process(snapshot.cmdline(pid))
        {
            DescendantMemBucket::Agent
        } else {
            DescendantMemBucket::TerminalApp
        };

        let rss = snapshot.rss_kb(pid).unwrap_or(0);
        match bucket {
            DescendantMemBucket::Agent => *agent_kb += rss,
            DescendantMemBucket::TerminalApp => *terminal_app_kb += rss,
        }

        if let Some(kids) = snapshot.children.get(&pid) {
            for &child in kids {
                stack.push((child, bucket));
            }
        }
    }
}

/// Walk /proc to find descendants of the current process and sum their RSS.
/// The Sizzle process itself is kept separate from anything launched inside a
/// terminal pane. Agent subtrees are also separated from other terminal apps
/// such as dev servers, browsers, editors, and build tools.
fn read_mem_breakdown() -> Option<MemBreakdown> {
    let current_pid = std::process::id();
    let snapshot = ProcessSnapshot::read()?;
    let sizzle_kb = snapshot.rss_kb(current_pid)?;
    let mut agent_kb = 0u64;
    let mut terminal_app_kb = 0u64;

    if let Some(kids) = snapshot.children.get(&current_pid) {
        for &child in kids {
            accumulate_descendant_memory(
                &snapshot,
                child,
                DescendantMemBucket::TerminalApp,
                &mut agent_kb,
                &mut terminal_app_kb,
            );
        }
    }

    Some(MemBreakdown {
        sizzle_kb,
        agent_kb,
        terminal_app_kb,
    })
}

struct ProjectMemBreakdown {
    name: String,
    agent_kb: u64,
    terminal_app_kb: u64,
}

impl ProjectMemBreakdown {
    fn total_kb(&self) -> u64 {
        self.agent_kb + self.terminal_app_kb
    }
}

fn read_project_mem_breakdowns(state: &AppState) -> Option<Vec<ProjectMemBreakdown>> {
    let current_pid = std::process::id();
    let project_roots = state
        .project_widgets
        .iter()
        .map(|(path, pw)| {
            let name = state
                .project_list
                .projects()
                .iter()
                .find(|project| project.path == *path)
                .map(|project| project.name.clone())
                .unwrap_or_else(|| {
                    std::path::Path::new(path)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or(path)
                        .to_string()
                });
            let pids = pw
                .borrow()
                .terminals
                .iter()
                .filter(|terminal| terminal.is_alive())
                .map(|terminal| terminal.child_pid())
                .collect::<Vec<_>>();
            (name, pids)
        })
        .collect::<Vec<_>>();

    let snapshot = ProcessSnapshot::read()?;
    let mut rows = Vec::new();

    for (name, pids) in project_roots {
        let mut seen = HashSet::new();
        let mut agent_kb = 0u64;
        let mut terminal_app_kb = 0u64;

        for pid in pids {
            if !seen.insert(pid) || snapshot.parent.get(&pid).copied() != Some(current_pid) {
                continue;
            }
            accumulate_descendant_memory(
                &snapshot,
                pid,
                DescendantMemBucket::TerminalApp,
                &mut agent_kb,
                &mut terminal_app_kb,
            );
        }

        if agent_kb + terminal_app_kb > 0 {
            rows.push(ProjectMemBreakdown {
                name,
                agent_kb,
                terminal_app_kb,
            });
        }
    }

    rows.sort_by(|a, b| {
        b.total_kb()
            .cmp(&a.total_kb())
            .then_with(|| a.name.cmp(&b.name))
    });
    Some(rows)
}

fn format_project_mem_breakdown(state: &AppState) -> String {
    let Some(rows) = read_project_mem_breakdowns(state) else {
        return "Per-project memory unavailable.\n/proc could not be read.".to_string();
    };

    if rows.is_empty() {
        return "No active terminal child processes are currently attached to projects."
            .to_string();
    }

    let total_agent_kb: u64 = rows.iter().map(|row| row.agent_kb).sum();
    let total_terminal_kb: u64 = rows.iter().map(|row| row.terminal_app_kb).sum();
    let total_kb = total_agent_kb + total_terminal_kb;

    let mut output = String::new();
    output.push_str("Per-project child memory (RSS)\n\n");
    output.push_str(&format!(
        "{:<30} {:>9} {:>9} {:>10}\n",
        "Project", "Total", "Agents", "Child apps"
    ));
    output.push_str(&format!(
        "{:<30} {:>9} {:>9} {:>10}\n",
        "------------------------------", "---------", "---------", "----------"
    ));
    output.push_str(&format!(
        "{:<30} {:>9} {:>9} {:>10}\n",
        "All projects",
        mem_mb(total_kb),
        mem_mb(total_agent_kb),
        mem_mb(total_terminal_kb)
    ));

    for row in rows {
        output.push_str(&format!(
            "{:<30} {:>9} {:>9} {:>10}\n",
            truncate_label(&row.name, 30),
            mem_mb(row.total_kb()),
            mem_mb(row.agent_kb),
            mem_mb(row.terminal_app_kb)
        ));
    }

    output.push_str("\nChild apps are terminal-launched non-agent processes.");
    output
}

fn mem_mb(kb: u64) -> String {
    format!("{} MB", kb / 1024)
}

fn truncate_label(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    if max_chars <= 3 {
        return value.chars().take(max_chars).collect();
    }
    let mut truncated = value.chars().take(max_chars - 3).collect::<String>();
    truncated.push_str("...");
    truncated
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
        perform_rescan(&state_c);
    });

    win.present();
}

// ── Add to Ignored Roots ─────────────────────────────────────────────────

fn add_to_ignored_roots(state: &State, path: &str) {
    let mut settings = state.borrow().store.get_scan_settings();
    if !settings.ignore_roots.contains(&path.to_string()) {
        settings.ignore_roots.push(path.to_string());
    }
    state.borrow_mut().store.set_scan_settings(&settings);
    perform_rescan(state);
}

// ── Filesystem watching ────────────────────────────────────────────────────

/// Tear down any existing file monitors and create new ones for every
/// scan root in the current settings. Non-existent roots are silently
/// skipped (logged at warn level).
fn setup_file_monitors(state: &State) {
    state.borrow_mut().file_monitors.clear();

    let settings = state.borrow().store.get_scan_settings();

    log::info!(
        "[sizzle] Setting up file monitors for {} scan root(s)",
        settings.scan_roots.len()
    );

    for root in &settings.scan_roots {
        let path = std::path::Path::new(root);
        if !path.exists() || !path.is_dir() {
            log::warn!("[sizzle] Cannot watch non-existent scan root: {}", root);
            continue;
        }

        let file = gtk4::gio::File::for_path(root);
        match file.monitor(
            gtk4::gio::FileMonitorFlags::NONE,
            gtk4::gio::Cancellable::NONE,
        ) {
            Ok(monitor) => {
                // Capture a weak reference so the monitor's signal handler does
                // not form a strong Rc cycle with AppState (which owns the
                // monitor through `file_monitors`).
                let weak = Rc::downgrade(state);
                monitor.connect_changed(
                    move |_monitor, child, _other_file, event_type| {
                        log::debug!(
                            "[sizzle] File monitor event: {:?} on {}",
                            event_type,
                            child.uri()
                        );
                        if let Some(state) = weak.upgrade() {
                            debounce_rescan(&state);
                        }
                    },
                );
                state.borrow_mut().file_monitors.push(monitor);
                log::info!("[sizzle] File monitor set up for: {}", root);
            }
            Err(e) => {
                log::warn!(
                    "[sizzle] Failed to monitor scan root {}: {}",
                    root,
                    e
                );
            }
        }
    }
}

/// Persist new scan settings, rescan projects, and rebuild the file monitors
/// to match the new scan roots.
fn apply_scan_settings(state: &State, settings: &ScanSettings) {
    state.borrow_mut().store.set_scan_settings(settings);
    perform_rescan(state);
    setup_file_monitors(state);
}

/// Replace the current project list and rebuild the left-panel list.
fn apply_projects(state: &State, new_projects: Vec<ScannedProject>) {
    state.borrow_mut().project_list = ProjectList::new(new_projects);
    populate_list(state);
    refresh_git_statuses(state);
}

fn update_remote_button(
    btn: &Button,
    url_holder: &Rc<RefCell<Option<String>>>,
    remote_info: Option<&sizzle_core::git::GitRemoteInfo>,
) {
    match remote_info {
        Some(info) => {
            *url_holder.borrow_mut() = info.web_url.clone();
            if info.is_github {
                let img = Image::builder()
                    .icon_name("sizzle-github")
                    .pixel_size(12)
                    .build();
                btn.set_child(Some(&img));
                btn.set_tooltip_text(info.web_url.as_deref().or(Some("GitHub remote repository")));
            } else {
                let lbl = Label::builder().label("🌐").build();
                btn.set_child(Some(&lbl));
                btn.set_tooltip_text(info.web_url.as_deref().or(Some("Git remote repository")));
            }
            btn.set_visible(true);
        }
        None => {
            *url_holder.borrow_mut() = None;
            btn.set_visible(false);
        }
    }
}

/// Recompute the "modified tracked files" state and git remote info for every project on a
/// background thread, then hand the results back to the UI thread (via the
/// mpsc channel drained by `drain_git_results`). The in-progress flag prevents
/// overlapping workers. Blocking git calls never run on the GTK thread.
fn refresh_git_statuses(state: &State) {
    let st = state.borrow();
    if st.git_refresh_in_progress.swap(true, Ordering::AcqRel) {
        return;
    }
    let paths: Vec<String> = st.project_list.projects().iter().map(|p| p.path.clone()).collect();
    let tx = st.git_result_tx.clone();
    let in_progress = st.git_refresh_in_progress.clone();
    std::thread::spawn(move || {
        let results: HashMap<String, GitRefreshResult> = paths
            .into_iter()
            .map(|p| {
                let dirty = sizzle_core::git::has_modified_tracked_files(&p);
                let remote_info = sizzle_core::git::get_git_remote_info(&p);
                (p, GitRefreshResult { dirty, remote_info })
            })
            .collect();
        let _ = tx.send(results);
        in_progress.store(false, Ordering::Release);
    });
}

/// Apply completed git-status refreshes on the UI thread: update each badge's
/// flag and toggle its visibility in place. Called from the 2-second timer.
fn drain_git_results(state: &State) {
    let st = state.borrow();
    while let Ok(results) = st.git_result_rx.try_recv() {
        for (path, res) in results {
            if let Some(badge) = st.git_badges.borrow().get(&path) {
                badge.set_visible(res.dirty);
            }
            if let Some(remote_state) = st.git_remote_btns.borrow().get(&path) {
                *remote_state.remote_info.borrow_mut() = res.remote_info.clone();
                update_remote_button(&remote_state.button, &remote_state.url_holder, res.remote_info.as_ref());
            }
        }
    }
}

/// Execute a full rescan: read current settings, scan for projects, update
/// state, and repopulate the left-panel list.
fn perform_rescan(state: &State) {
    log::info!("[sizzle] perform_rescan: starting scan");
    let settings = state.borrow().store.get_scan_settings();
    let new_projects = scan_projects(&settings);
    log::info!("[sizzle] perform_rescan: found {} project(s)", new_projects.len());
    apply_projects(state, new_projects);
}

/// Schedule a debounced rescan. If a rescan is already pending, cancel the
/// existing timer and start a fresh one. This ensures that rapid filesystem
/// changes (e.g., during a git clone) produce at most one rescan, occurring
/// 2 seconds after the last change event.
fn debounce_rescan(state: &State) {
    log::debug!("[sizzle] debounce_rescan: scheduling rescan in 2s");
    let state_c = state.clone();
    let timer = state_c.borrow().rescan_timer_id.clone();
    timer.schedule(Duration::from_secs(2), move || {
        log::debug!("[sizzle] debounce timer fired, running rescan");
        perform_rescan(&state_c);
    });
}

/// A deterministic, order-independent snapshot of the UI-relevant fields of a
/// project. The poll timer uses this to decide whether a rescan produced any
/// change worth rebuilding the list for — comparing only paths would miss
/// metadata changes (new tags, renamed README) inside an existing project.
/// Tag scores (`f64`) are deliberately excluded.
fn project_signature(p: &ScannedProject) -> (String, String, Vec<String>, Vec<String>) {
    let mut readmes = p.readme_files.clone();
    readmes.sort();
    let mut tags: Vec<String> = p.detected_tags.iter().map(|t| t.name.clone()).collect();
    tags.sort();
    (p.path.clone(), p.name.clone(), readmes, tags)
}

/// Start a periodic poll timer that rescans for project changes every 15
/// seconds.  This catches cases the file monitor misses — the file monitor
/// only watches immediate children of each scan root, so creating a
/// directory under a scan root and *later* adding `.git` inside it (or
/// adding files to an existing directory to make it look like a project)
/// won't trigger a file-monitor event.  The poll timer ensures these
/// changes are eventually picked up.
///
/// The poll skips the UI rebuild if a rescan yields no project changes.
fn start_poll_timer(state: &State) {
    let state_c = state.clone();
    let _ = glib::timeout_add_local(Duration::from_secs(15), move || {
        // If the projects are unchanged (same name/path/tags/readmes), skip
        // the expensive populate_list call.
        let settings = state_c.borrow().store.get_scan_settings();
        let new_projects = scan_projects(&settings);
        let (changed, old_count) = {
            let st = state_c.borrow();
            let old: HashSet<_> = st.project_list.projects().iter().map(project_signature).collect();
            let new: HashSet<_> = new_projects.iter().map(project_signature).collect();
            (old != new, old.len())
        };
        if changed {
            log::info!(
                "[sizzle] Poll detected project changes ({} → {} projects), updating",
                old_count,
                new_projects.len()
            );
            apply_projects(&state_c, new_projects);
        } else {
            // Project set unchanged, but tracked files may have been edited in
            // place — refresh the git-dirty indicator (≤15s latency; the
            // in-progress flag prevents overlapping workers).
            refresh_git_statuses(&state_c);
        }
        glib::ControlFlow::Continue
    });
    log::info!("[sizzle] Poll timer started (15s interval)");
}
