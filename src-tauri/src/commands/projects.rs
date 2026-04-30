use crate::commands::metadata::MetadataStore;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::fs;
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveRenameResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub changes: Vec<String>,
}

fn path_to_claude_dir(project_path: &str) -> String {
    project_path.replace('/', "-")
}

fn is_under_scan_root(target_path: &str, scan_roots: &[String]) -> bool {
    let target = Path::new(target_path).canonicalize().ok();
    let target = match target { Some(t) => t, None => return false };

    scan_roots.iter().any(|root| {
        if let Ok(normalized) = Path::new(root).canonicalize() {
            target == normalized || target.starts_with(&normalized)
        } else {
            let normalized = Path::new(root);
            target == *normalized || target.starts_with(normalized)
        }
    })
}

fn dirs_home() -> String {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/home/unknown".to_string())
}

#[tauri::command]
pub fn move_rename_project(
    state: State<'_, MetadataStore>,
    old_path: String,
    new_path: String,
) -> MoveRenameResult {
    let mut changes: Vec<String> = Vec::new();

    // Move directory
    if let Err(e) = fs::rename(&old_path, &new_path) {
        return MoveRenameResult {
            success: false,
            error: Some(e.to_string()),
            changes,
        };
    }
    changes.push(format!("Moved directory:\n  {}\n  → {}", old_path, new_path));

    // Update metadata
    state.rename_project_metadata(&old_path, &new_path);

    // Update settings paths
    let settings = state.get_scan_settings();
    let new_manual: Vec<String> = settings.manual_project_roots.iter()
        .map(|r| if r == &old_path { new_path.clone() } else { r.clone() })
        .collect();
    let new_ignore: Vec<String> = settings.ignore_roots.iter()
        .map(|r| if r == &old_path { new_path.clone() } else { r.clone() })
        .collect();
    let new_scan: Vec<String> = settings.scan_roots.iter()
        .map(|r| if r == &old_path { new_path.clone() } else { r.clone() })
        .collect();

    let will_be_visible = is_under_scan_root(&new_path, &new_scan) || new_manual.contains(&new_path);
    let mut final_manual = new_manual;
    if !will_be_visible {
        final_manual.push(new_path.clone());
    }

    state.set_scan_settings(&crate::commands::metadata::ScanSettings {
        scan_roots: new_scan,
        ignore_roots: new_ignore,
        manual_project_roots: final_manual,
    });

    // Move Claude project data
    let home = dirs_home();
    let claude_projects_dir = Path::new(&home).join(".claude").join("projects");
    let old_claude = claude_projects_dir.join(path_to_claude_dir(&old_path));
    let new_claude = claude_projects_dir.join(path_to_claude_dir(&new_path));

    if old_claude.exists() {
        if let Err(e) = fs::rename(&old_claude, &new_claude) {
            changes.push(format!("Failed to move Claude data: {}", e));
        } else {
            changes.push(format!("Moved Claude project data:\n  {}\n  → {}", old_claude.display(), new_claude.display()));
        }
    }

    // Update Codex config
    let codex_config = Path::new(&home).join(".codex").join("config.toml");
    if codex_config.exists() {
        if let Ok(content) = fs::read_to_string(&codex_config) {
            let old_key = format!("[projects.\"{}\"]", old_path);
            let new_key = format!("[projects.\"{}\"]", new_path);
            if content.contains(&old_key) {
                let updated = content.replace(&old_key, &new_key);
                fs::write(&codex_config, &updated).ok();
                changes.push(format!("Updated Codex config ({}):\n  replaced \"{}\" with \"{}\"", codex_config.display(), old_path, new_path));
            }
        }
    }

    MoveRenameResult { success: true, error: None, changes }
}
