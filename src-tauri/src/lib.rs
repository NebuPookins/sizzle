mod commands;

use commands::metadata::MetadataStore;
use commands::pty::PtyRegistry;
use std::path::PathBuf;
use std::sync::Mutex;

fn get_config_dir() -> PathBuf {
    let arg_config = std::env::args()
        .find(|a| a.starts_with("--sizzle-config-dir="))
        .and_then(|a| a.split('=').nth(1).map(|s| s.to_string()));

    if let Some(dir) = arg_config {
        PathBuf::from(dir)
    } else {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| "/home/unknown".to_string());
        PathBuf::from(home).join(".config").join("sizzle")
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_target(false)
        .init();
    let config_dir = get_config_dir();
    log::info!("config_dir: {:?}", config_dir);
    let metadata_store = MetadataStore::new(config_dir);
    let pty_registry = PtyRegistry::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(metadata_store)
        .manage(Mutex::new(pty_registry))
        .invoke_handler(tauri::generate_handler![
            commands::scan_projects,
            commands::rescan_project_tags,
            commands::get_scan_settings,
            commands::set_scan_settings,
            commands::add_ignore_root,
            commands::get_markdown_files,
            commands::read_markdown_file,
            commands::list_directory,
            commands::preview_file,
            commands::get_project_repository_info,
            commands::get_git_status,
            commands::get_metadata,
            commands::get_all_metadata,
            commands::set_last_launched,
            commands::set_tag_override,
            commands::set_project_marker,
            commands::move_rename_project,
            commands::claude_has_session,
            commands::pty_create,
            commands::pty_write,
            commands::pty_resize,
            commands::pty_detach,
            commands::pty_kill,
        ])
        .run(tauri::generate_context!())
        .expect("error while running sizzle");
}
