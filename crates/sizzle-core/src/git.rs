use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;

/// Line-level change estimate for a single file, from `git diff --numstat`.
/// `added`/`deleted` are the number of lines added and removed respectively.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffStat {
    pub added: u32,
    pub deleted: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitFileChange {
    pub status: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orig_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<DiffStat>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitRemoteInfo {
    pub is_github: bool,
    pub web_url: Option<String>,
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

    // `-z` output is NUL-separated and unquoted: the `## ...` branch header
    // comes first, then one `XY path` record per file. Rename/copy records
    // carry the source path as the immediately-following record (destination
    // first, then source).
    let mut fields = stdout.split('\0');

    if let Some(header) = fields.next().and_then(|h| h.strip_prefix("## ")) {
        if header.starts_with("HEAD (no branch)") {
            is_detached = true;
        } else if let Some(rest) = header.strip_prefix("No commits yet on ") {
            branch = Some(rest.trim().to_string());
        } else if let Some(dot_idx) = header.find("...") {
            branch = Some(header[..dot_idx].to_string());
            let rest = &header[dot_idx + 3..];
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
            branch = Some(header.to_string());
        }
    }

    while let Some(entry) = fields.next() {
        if entry.len() < 3 { continue; }
        let x = entry.as_bytes()[0] as char;
        let y = entry.as_bytes()[1] as char;
        let path = &entry[3..];

        if x == '?' && y == '?' {
            untracked.push(path.to_string());
            continue;
        }
        if x == '!' && y == '!' { continue; }

        // A rename/copy (`R`/`C`) is followed by a record holding the source
        // path; `path` itself is already the destination.
        let orig_path = if x == 'R' || x == 'C' {
            fields.next().filter(|s| !s.is_empty()).map(String::from)
        } else {
            None
        };

        if x != ' ' {
            staged.push(GitFileChange {
                status: x.to_string(),
                path: path.to_string(),
                orig_path,
                diff: None,
            });
        }
        if y != ' ' {
            unstaged.push(GitFileChange {
                status: y.to_string(),
                path: path.to_string(),
                orig_path: None,
                diff: None,
            });
        }
    }

    GitStatus { branch, upstream, ahead, behind, staged, unstaged, untracked, is_detached }
}

pub fn get_git_status(project_path: String) -> Option<GitStatus> {
    let output = Command::new("git")
        .args(["status", "--porcelain", "-b", "-z"])
        .current_dir(&project_path)
        .output()
        .ok()?;

    if !output.status.success() { return None; }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut status = parse_git_status(&stdout);

    // Attach line-change estimates (`+N -M`) to each staged/unstaged file. The
    // numstat spawn is skipped when there is nothing to attach, so a clean repo
    // pays only for the single `git status` above (this runs on a 5s UI timer).
    if !status.staged.is_empty() {
        attach_diffs(&mut status.staged, numstat(&project_path, DiffScope::Staged));
    }
    if !status.unstaged.is_empty() {
        attach_diffs(&mut status.unstaged, numstat(&project_path, DiffScope::Unstaged));
    }

    Some(status)
}

/// Copy a [`DiffStat`] onto each file change, keyed by the file's new path.
fn attach_diffs(files: &mut [GitFileChange], stats: std::collections::HashMap<String, DiffStat>) {
    for f in files {
        f.diff = stats.get(&f.path).copied();
    }
}

/// Which side of the index `numstat` should report on.
enum DiffScope {
    Staged,
    Unstaged,
}

/// Run `git diff --numstat -z` and return a map of new-path → [`DiffStat`].
/// Binary files (whose counts are `-`) and rows with no line change (pure
/// renames / mode changes) are omitted, as are any records that fail to parse.
fn numstat(project_path: &str, scope: DiffScope) -> std::collections::HashMap<String, DiffStat> {
    let mut cmd = Command::new("git");
    cmd.arg("diff").arg("--numstat").arg("-z");
    if matches!(scope, DiffScope::Staged) {
        cmd.arg("--cached");
    }
    let output = cmd
        .current_dir(project_path)
        .output()
        .ok()
        .filter(|o| o.status.success());
    let Some(output) = output else {
        return std::collections::HashMap::new();
    };

    parse_numstat(&String::from_utf8_lossy(&output.stdout))
}

/// Parse `git diff --numstat -z` output into a map of new-path → [`DiffStat`].
/// In `-z` mode each record is `<added>\t<deleted>\t<path>\0`, with paths
/// unquoted. A rename is `<added>\t<deleted>\t\0<source>\0<dest>\0`: an empty
/// path followed by the source and destination as two further records.
fn parse_numstat(stdout: &str) -> std::collections::HashMap<String, DiffStat> {
    let mut map = std::collections::HashMap::new();
    let mut fields = stdout.split('\0');
    while let Some(field) = fields.next() {
        if field.is_empty() {
            continue;
        }
        let mut parts = field.splitn(3, '\t');
        let added = parts.next().unwrap_or("");
        let deleted = parts.next().unwrap_or("");
        let path = parts.next().unwrap_or("");
        let path = if path.is_empty() {
            // Rename: skip the source record, key by the destination.
            fields.next();
            fields.next().unwrap_or("")
        } else {
            path
        };
        if path.is_empty() {
            continue;
        }
        let (Ok(added), Ok(deleted)) = (added.parse(), deleted.parse()) else {
            continue;
        };
        // A `0 0` row is a pure rename or mode change — nothing to estimate.
        if added == 0 && deleted == 0 {
            continue;
        }
        map.insert(path.to_string(), DiffStat { added, deleted });
    }
    map
}

fn find_git_dir(start_path: &Path) -> Option<String> {
    let mut current = Some(start_path);
    while let Some(dir) = current {
        let dot_git = dir.join(".git");
        if dot_git.exists() {
            if dot_git.is_dir() {
                return Some(dot_git.to_string_lossy().to_string());
            }
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

    if let Some(captured) = trimmed.strip_prefix("git@github.com:") {
        let repo = captured.strip_suffix(".git").unwrap_or(captured);
        return Some(format!("https://github.com/{}", repo));
    }

    if let Some(captured) = trimmed.strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("http://github.com/"))
    {
        let repo = captured.strip_suffix(".git").unwrap_or(captured).trim_end_matches('/');
        return Some(format!("https://github.com/{}", repo));
    }

    if let Some(captured) = trimmed.strip_prefix("ssh://git@github.com/") {
        let repo = captured.strip_suffix(".git").unwrap_or(captured).trim_end_matches('/');
        return Some(format!("https://github.com/{}", repo));
    }

    None
}

/// Parse a git remote URL to determine if it is a GitHub remote and derive a web URL if possible.
pub fn parse_remote_url(url: &str) -> (bool, Option<String>) {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return (false, None);
    }

    if let Some(gh_url) = normalize_github_url(trimmed) {
        return (true, Some(gh_url));
    }

    // Generic SSH: git@host:owner/repo.git or user@host:owner/repo.git
    if let Some(idx) = trimmed.find('@') {
        let after_at = &trimmed[idx + 1..];
        if let Some(colon_idx) = after_at.find(':') {
            let host = &after_at[..colon_idx];
            let path = &after_at[colon_idx + 1..];
            if !host.is_empty() && !path.is_empty() && !host.contains('/') {
                let repo_path = path.strip_suffix(".git").unwrap_or(path).trim_matches('/');
                let is_github = host.eq_ignore_ascii_case("github.com");
                let web_url = format!("https://{}/{}", host, repo_path);
                return (is_github, Some(web_url));
            }
        }
    }

    // Generic ssh://
    if let Some(captured) = trimmed.strip_prefix("ssh://") {
        let sans_user = if let Some(at_idx) = captured.find('@') {
            &captured[at_idx + 1..]
        } else {
            captured
        };
        if let Some(slash_idx) = sans_user.find('/') {
            let host = &sans_user[..slash_idx];
            let path = &sans_user[slash_idx + 1..];
            let repo_path = path.strip_suffix(".git").unwrap_or(path).trim_matches('/');
            let is_github = host.eq_ignore_ascii_case("github.com");
            let web_url = format!("https://{}/{}", host, repo_path);
            return (is_github, Some(web_url));
        }
    }

    // Generic http:// or https://
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        let scheme = if trimmed.starts_with("https://") { "https" } else { "http" };
        let rest = &trimmed[scheme.len() + 3..];
        let sans_auth = if let Some(at_idx) = rest.find('@') {
            &rest[at_idx + 1..]
        } else {
            rest
        };
        let repo_path = sans_auth.strip_suffix(".git").unwrap_or(sans_auth).trim_end_matches('/');
        let host = repo_path.split('/').next().unwrap_or("");
        let is_github = host.eq_ignore_ascii_case("github.com");
        let web_url = format!("{}://{}", scheme, repo_path);
        return (is_github, Some(web_url));
    }

    (false, None)
}

pub fn get_git_remote_info(project_path: &str) -> Option<GitRemoteInfo> {
    let dir = Path::new(project_path);
    let git_dir = find_git_dir(dir)?;

    let config_path = Path::new(&git_dir).join("config");
    let head_path = Path::new(&git_dir).join("HEAD");

    let Ok(config_content) = fs::read_to_string(&config_path) else {
        return None;
    };

    let config = parse_git_config(&config_content);
    let head_content = fs::read_to_string(&head_path).unwrap_or_default();
    let head_match = head_content.trim().strip_prefix("ref: refs/heads/");
    let current_branch = head_match.map(|s| s.trim().to_string());

    let branch_remote = current_branch.as_ref()
        .and_then(|b| config.get(&format!("branch \"{}\"", b)))
        .and_then(|s| s.get("remote").cloned());

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

    if remote_names.is_empty() {
        return None;
    }

    let mut first_remote_info: Option<GitRemoteInfo> = None;

    for remote_name in &remote_names {
        let section_key = format!("remote \"{}\"", remote_name);
        if let Some(remote_config) = config.get(&section_key) {
            if let Some(url) = remote_config.get("url") {
                let (is_github, web_url) = parse_remote_url(url);
                let info = GitRemoteInfo { is_github, web_url };
                if is_github {
                    return Some(info);
                }
                if first_remote_info.is_none() {
                    first_remote_info = Some(info);
                }
            }
        }
    }

    first_remote_info
}

/// Whether a repository has modified tracked files (staged or unstaged),
/// excluding untracked files. Returns `false` for non-repositories or on error.
///
/// Kept separate from [`get_git_status`] on purpose: `--untracked-files=no`
/// skips the untracked-file walk that dominates `git status` cost, and the list
/// badge only needs a boolean rather than the full parsed status. With untracked
/// files suppressed, a non-empty porcelain output is exactly "has a tracked
/// change", so there is no need to parse the output.
pub fn has_modified_tracked_files(project_path: &str) -> bool {
    // Fail fast for non-repositories: `find_git_dir` is a few filesystem checks
    // versus spawning a `git` process that would just report "not a repository".
    if find_git_dir(Path::new(project_path)).is_none() {
        return false;
    }
    // The `-- .` pathspec scopes the check to the project directory, so a
    // project nested inside a larger repo reports only its own files instead of
    // the whole parent repo's dirty state.
    let output = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=no", "--", "."])
        .current_dir(project_path)
        .output();
    matches!(output, Ok(out) if out.status.success() && !out.stdout.is_empty())
}

/// Check if a worktree directory has uncommitted changes.
pub fn worktree_has_uncommitted_changes(worktree_path: &str) -> bool {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(worktree_path)
        .output();
    matches!(output, Ok(out) if out.status.success() && !out.stdout.is_empty())
}

/// Check if the latest commit on the given worktree is merged into any other branch.
/// Returns `None` if the git command fails (e.g. no commits yet).
/// Returns `Some(list)` containing branch names (excluding the current branch)
/// that contain the commit. An empty list means the commit is not merged anywhere.
pub fn is_latest_commit_merged_elsewhere(worktree_path: &str) -> Option<Vec<String>> {
    let head_output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(worktree_path)
        .output()
        .ok()?;
    if !head_output.status.success() {
        return None;
    }
    let head_commit = String::from_utf8_lossy(&head_output.stdout).trim().to_string();

    let branch_output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(worktree_path)
        .output()
        .ok()?;
    if !branch_output.status.success() {
        return None;
    }
    let current_branch = String::from_utf8_lossy(&branch_output.stdout).trim().to_string();

    let contains_output = Command::new("git")
        .args(["branch", "--contains", &head_commit, "--format", "%(refname:short)"])
        .current_dir(worktree_path)
        .output()
        .ok()?;
    if !contains_output.status.success() {
        return None;
    }

    let contains_stdout = String::from_utf8_lossy(&contains_output.stdout);
    let merged_branches: Vec<String> = contains_stdout
        .lines()
        .map(|b| b.trim().to_string())
        .filter(|b| !b.is_empty() && *b != current_branch)
        .collect();

    Some(merged_branches)
}

pub fn get_project_repository_info(project_path: String) -> ProjectRepositoryInfo {
    let is_git_repo = find_git_dir(Path::new(&project_path)).is_some();
    let remote_info = get_git_remote_info(&project_path);
    let github_url = remote_info.and_then(|r| if r.is_github { r.web_url } else { None });
    ProjectRepositoryInfo { is_git_repo, github_url }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_unquotes_paths_with_spaces() {
        let status = parse_git_status("## main\0 M my file.txt\0");
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(status.staged.len(), 0);
        assert_eq!(status.unstaged.len(), 1);
        assert_eq!(status.unstaged[0].status, "M");
        assert_eq!(status.unstaged[0].path, "my file.txt");
    }

    #[test]
    fn status_rename_with_unstaged_edit_uses_destination_path() {
        // `RM dest\0source\0` — destination first, then source.
        let status = parse_git_status("## main\0RM renamed.txt\0a => b.txt\0");
        assert_eq!(status.staged.len(), 1);
        assert_eq!(status.staged[0].status, "R");
        assert_eq!(status.staged[0].path, "renamed.txt");
        assert_eq!(status.staged[0].orig_path.as_deref(), Some("a => b.txt"));
        assert_eq!(status.unstaged.len(), 1);
        assert_eq!(status.unstaged[0].status, "M");
        assert_eq!(status.unstaged[0].path, "renamed.txt");
    }

    #[test]
    fn status_pure_rename() {
        let status = parse_git_status("## main\0R  dest.txt\0src.txt\0");
        assert_eq!(status.staged.len(), 1);
        assert_eq!(status.staged[0].path, "dest.txt");
        assert_eq!(status.staged[0].orig_path.as_deref(), Some("src.txt"));
        assert_eq!(status.unstaged.len(), 0);
    }

    #[test]
    fn status_untracked_path_with_spaces() {
        let status = parse_git_status("## main\0?? my new file\0");
        assert_eq!(status.untracked, vec!["my new file"]);
    }

    #[test]
    fn numstat_keys_by_unquoted_path() {
        let stats = parse_numstat("3\t2\tmy file.txt\0");
        assert_eq!(stats.get("my file.txt").map(|d| (d.added, d.deleted)), Some((3, 2)));
    }

    #[test]
    fn numstat_keys_rename_by_destination() {
        // `1\t0\t\0source\0dest\0` — source first, then destination.
        let stats = parse_numstat("1\t0\t\0ORIGINAL.txt\0DEST.txt\0");
        assert_eq!(stats.get("DEST.txt").map(|d| (d.added, d.deleted)), Some((1, 0)));
        assert!(!stats.contains_key("ORIGINAL.txt"));
    }

    #[test]
    fn numstat_skips_binary_and_zero_change_rows() {
        let stats = parse_numstat("-\t-\tbin.dat\00\t0\tmode.txt\01\t0\tok.txt\0");
        assert_eq!(stats.len(), 1);
        assert!(stats.contains_key("ok.txt"));
    }

    #[test]
    fn test_parse_remote_url_github() {
        let (is_gh, url) = parse_remote_url("git@github.com:user/repo.git");
        assert!(is_gh);
        assert_eq!(url.as_deref(), Some("https://github.com/user/repo"));

        let (is_gh, url) = parse_remote_url("https://github.com/user/repo.git");
        assert!(is_gh);
        assert_eq!(url.as_deref(), Some("https://github.com/user/repo"));
    }

    #[test]
    fn test_parse_remote_url_gitlab() {
        let (is_gh, url) = parse_remote_url("git@gitlab.com:org/project.git");
        assert!(!is_gh);
        assert_eq!(url.as_deref(), Some("https://gitlab.com/org/project"));

        let (is_gh, url) = parse_remote_url("https://gitlab.com/org/project.git");
        assert!(!is_gh);
        assert_eq!(url.as_deref(), Some("https://gitlab.com/org/project"));
    }
}
