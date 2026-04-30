use std::path::Path;

fn encode_project_path_for_claude(project_path: &str) -> String {
    project_path.replace('/', "-")
}

#[tauri::command]
pub fn claude_has_session(project_path: String) -> bool {
    let home = dirs_fallback();
    let encoded = encode_project_path_for_claude(&project_path);
    let session_dir = Path::new(&home).join(".claude").join("projects").join(&encoded);

    if !session_dir.exists() { return false; }

    let Ok(entries) = std::fs::read_dir(&session_dir) else { return false };
    entries.flatten().any(|e| {
        e.file_name().to_string_lossy().ends_with(".jsonl")
    })
}

fn dirs_fallback() -> String {
    std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/home/unknown".to_string())
}
