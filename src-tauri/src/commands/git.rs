use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitFileChange {
    pub status: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orig_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: i32,
    pub behind: i32,
    pub staged: Vec<GitFileChange>,
    pub unstaged: Vec<GitFileChange>,
    pub untracked: Vec<String>,
    pub is_detached: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRepositoryInfo {
    pub is_git_repo: bool,
    pub github_url: Option<String>,
}

fn parse_git_status(stdout: &str) -> GitStatus {
    let mut branch: Option<String> = None;
    let mut upstream: Option<String> = None;
    let mut ahead = 0i32;
    let mut behind = 0i32;
    let mut is_detached = false;
    let mut staged: Vec<GitFileChange> = Vec::new();
    let mut unstaged: Vec<GitFileChange> = Vec::new();
    let mut untracked: Vec<String> = Vec::new();

    for line in stdout.lines() {
        if line.starts_with("## ") {
            let branch_line = &line[3..];
            if branch_line.starts_with("HEAD (no branch)") {
                is_detached = true;
                continue;
            }
            if let Some(rest) = branch_line.strip_prefix("No commits yet on ") {
                branch = Some(rest.trim().to_string());
                continue;
            }
            if let Some(dot_idx) = branch_line.find("...") {
                branch = Some(branch_line[..dot_idx].to_string());
                let rest = &branch_line[dot_idx + 3..];
                if let Some(bracket_idx) = rest.find(" [") {
                    upstream = Some(rest[..bracket_idx].to_string());
                    let bracket_content = &rest[bracket_idx + 2..];
                    if let Some(end) = bracket_content.rfind(']') {
                        let content = &bracket_content[..end];
                        if let Some(a) = content.split("ahead ").nth(1).and_then(|s| s.split_whitespace().next()) {
                            ahead = a.parse().unwrap_or(0);
                        }
                        if let Some(b) = content.split("behind ").nth(1).and_then(|s| s.split_whitespace().next()) {
                            behind = b.parse().unwrap_or(0);
                        }
                    }
                } else {
                    upstream = Some(rest.to_string());
                }
            } else {
                branch = Some(branch_line.to_string());
            }
            continue;
        }

        if line.len() < 3 { continue; }
        let x = line.as_bytes()[0] as char;
        let y = line.as_bytes()[1] as char;
        let raw_path = &line[3..];

        if x == '?' && y == '?' {
            untracked.push(raw_path.to_string());
            continue;
        }
        if x == '!' && y == '!' { continue; }

        if x != ' ' {
            let mut file_path = raw_path.to_string();
            let mut orig_path: Option<String> = None;
            if x == 'R' || x == 'C' {
                if let Some(arrow_idx) = raw_path.find(" -> ") {
                    orig_path = Some(raw_path[..arrow_idx].to_string());
                    file_path = raw_path[arrow_idx + 4..].to_string();
                }
            }
            staged.push(GitFileChange {
                status: x.to_string(),
                path: file_path,
                orig_path,
            });
        }

        if y != ' ' {
            let path = if let Some(tab_idx) = raw_path.find('\t') {
                raw_path[..tab_idx].to_string()
            } else {
                raw_path.to_string()
            };
            unstaged.push(GitFileChange {
                status: y.to_string(),
                path,
                orig_path: None,
            });
        }
    }

    GitStatus { branch, upstream, ahead, behind, staged, unstaged, untracked, is_detached }
}

#[tauri::command]
pub fn get_git_status(project_path: String) -> Option<GitStatus> {
    let output = Command::new("git")
        .args(["status", "--porcelain", "-b"])
        .current_dir(&project_path)
        .output()
        .ok()?;

    if !output.status.success() { return None; }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Some(parse_git_status(&stdout))
}

fn find_git_dir(start_path: &Path) -> Option<String> {
    let mut current = Some(start_path);
    while let Some(dir) = current {
        let dot_git = dir.join(".git");
        if dot_git.exists() {
            if dot_git.is_dir() {
                return Some(dot_git.to_string_lossy().to_string());
            }
            // submodule or worktree — read the file
            if let Ok(content) = fs::read_to_string(&dot_git) {
                if let Some(line) = content.lines().next() {
                    if let Some(gitdir) = line.strip_prefix("gitdir: ") {
                        let resolved = dir.join(gitdir.trim());
                        return Some(resolved.to_string_lossy().to_string());
                    }
                }
            }
            return None;
        }
        let parent = dir.parent()?;
        if parent == dir { return None; }
        current = Some(parent);
    }
    None
}

fn parse_git_config(content: &str) -> std::collections::HashMap<String, std::collections::HashMap<String, String>> {
    let mut sections: std::collections::HashMap<String, std::collections::HashMap<String, String>> = std::collections::HashMap::new();
    let mut current_section: Option<String> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') { continue; }

        if let Some(captured) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            current_section = Some(captured.trim().to_string());
            sections.entry(current_section.clone().unwrap()).or_default();
            continue;
        }

        let Some(ref section) = current_section else { continue };
        if let Some(eq_idx) = trimmed.find('=') {
            let key = trimmed[..eq_idx].trim().to_string();
            let value = trimmed[eq_idx + 1..].trim().to_string();
            sections.get_mut(section).unwrap().insert(key, value);
        }
    }

    sections
}

fn normalize_github_url(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if trimmed.is_empty() { return None; }

    // git@github.com:user/repo.git
    if let Some(captured) = trimmed.strip_prefix("git@github.com:") {
        let repo = captured.strip_suffix(".git").unwrap_or(captured);
        return Some(format!("https://github.com/{}", repo));
    }

    // https://github.com/user/repo.git
    if let Some(captured) = trimmed.strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("http://github.com/"))
    {
        let repo = captured.strip_suffix(".git").unwrap_or(captured).trim_end_matches('/');
        return Some(format!("https://github.com/{}", repo));
    }

    // ssh://git@github.com/user/repo.git
    if let Some(captured) = trimmed.strip_prefix("ssh://git@github.com/") {
        let repo = captured.strip_suffix(".git").unwrap_or(captured).trim_end_matches('/');
        return Some(format!("https://github.com/{}", repo));
    }

    None
}

#[tauri::command]
pub fn get_project_repository_info(project_path: String) -> ProjectRepositoryInfo {
    let dir = Path::new(&project_path);
    let git_dir = match find_git_dir(dir) {
        Some(d) => d,
        None => return ProjectRepositoryInfo { is_git_repo: false, github_url: None },
    };

    let config_path = Path::new(&git_dir).join("config");
    let head_path = Path::new(&git_dir).join("HEAD");

    let Ok(config_content) = fs::read_to_string(&config_path) else {
        return ProjectRepositoryInfo { is_git_repo: true, github_url: None };
    };

    let config = parse_git_config(&config_content);
    let head_content = fs::read_to_string(&head_path).unwrap_or_default();
    let head_match = head_content.trim().strip_prefix("ref: refs/heads/");
    let current_branch = head_match.map(|s| s.trim().to_string());

    // Find branch remote and origin
    let branch_remote = current_branch.as_ref()
        .and_then(|b| config.get(&format!("branch \"{}\"", b)))
        .and_then(|s| s.get("remote").cloned());

    // Collect all remote names in order
    let mut remote_names = Vec::new();
    if let Some(ref br) = branch_remote {
        remote_names.push(br.clone());
    }
    remote_names.push("origin".to_string());
    for key in config.keys() {
        if let Some(name) = key.strip_prefix("remote \"").and_then(|s| s.strip_suffix('"')) {
            if !remote_names.contains(&name.to_string()) {
                remote_names.push(name.to_string());
            }
        }
    }

    for remote_name in &remote_names {
        let section_key = format!("remote \"{}\"", remote_name);
        if let Some(remote_config) = config.get(&section_key) {
            if let Some(url) = remote_config.get("url") {
                if let Some(gh_url) = normalize_github_url(url) {
                    return ProjectRepositoryInfo { is_git_repo: true, github_url: Some(gh_url) };
                }
            }
        }
    }

    ProjectRepositoryInfo { is_git_repo: true, github_url: None }
}
