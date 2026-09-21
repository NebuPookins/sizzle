use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::Path;

const MAX_TEXT_PREVIEW_BYTES: u64 = 2 * 1024 * 1024;
const MAX_MEDIA_PREVIEW_BYTES: u64 = 30 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSystemEntry {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveTreeNode {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<ArchiveTreeNode>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePreview {
    pub kind: String, // text, media, archive, unsupported, tooLarge, error
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive_tree: Option<Vec<ArchiveTreeNode>>,
}

/// Collapse `..`/`.` components purely lexically, without touching the
/// filesystem. `Path::file_name` returns `None` for a path ending in `..`, so
/// this must run before any walk that relies on `file_name`/`parent`.
fn normalize_lexically(path: &Path) -> std::path::PathBuf {
    let mut result = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => { result.pop(); }
            std::path::Component::CurDir => {}
            other => result.push(other.as_os_str()),
        }
    }
    result
}

/// Resolve `candidate` to an absolute, symlink-free path even when it (or
/// part of it) doesn't exist yet, by canonicalizing the nearest existing
/// ancestor and appending the remaining (already `..`/`.`-free) components on
/// top of it. `Path::canonicalize` alone fails on nonexistent paths, and a
/// raw fallback to the un-resolved path lets `..` segments defeat a
/// `starts_with` prefix check (e.g. `root/../../etc/passwd` still starts with
/// `root`). `symlink_metadata` (rather than `metadata`) is used for the
/// existence probe so a dangling symlink counts as "existing" and forces
/// `canonicalize` to fail closed, instead of being treated as a missing path
/// whose name would then be appended un-resolved.
fn resolve_lexically(path: &Path) -> Option<std::path::PathBuf> {
    let normalized = normalize_lexically(path);
    let mut existing: &Path = &normalized;
    let mut tail: Vec<&std::ffi::OsStr> = Vec::new();
    while fs::symlink_metadata(existing).is_err() {
        tail.push(existing.file_name()?);
        existing = existing.parent()?;
    }
    let mut resolved = existing.canonicalize().ok()?;
    for name in tail.into_iter().rev() {
        resolved.push(name);
    }
    Some(resolved)
}

fn is_within_root(root: &str, candidate: &str) -> bool {
    let Some(root) = resolve_lexically(Path::new(root)) else { return false };
    let Some(candidate) = resolve_lexically(Path::new(candidate)) else { return false };
    candidate == root || candidate.starts_with(&root)
}

pub fn list_directory(
    project_path: String,
    directory_path: Option<String>,
) -> Vec<FileSystemEntry> {
    let root = Path::new(&project_path);
    let target = directory_path
        .as_ref()
        .map(|p| Path::new(p).to_path_buf())
        .unwrap_or_else(|| root.to_path_buf());

    if !is_within_root(&project_path, &target.to_string_lossy()) {
        return vec![];
    }

    let Ok(meta) = fs::metadata(&target) else { return vec![] };
    if !meta.is_dir() { return vec![]; }

    let Ok(entries) = fs::read_dir(&target) else { return vec![] };

    let mut result: Vec<FileSystemEntry> = entries
        .filter_map(|e| e.ok())
        .map(|e| {
            let path = e.path();
            FileSystemEntry {
                name: e.file_name().to_string_lossy().to_string(),
                path: path.to_string_lossy().to_string(),
                is_directory: e.file_type().map(|t| t.is_dir()).unwrap_or(false),
            }
        })
        .collect();

    result.sort_by(|a, b| {
        if a.is_directory != b.is_directory {
            return if a.is_directory { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater };
        }
        a.name.to_lowercase().cmp(&b.name.to_lowercase())
    });

    result
}

const TEXT_EXTENSIONS: &[&str] = &[
    ".md", ".markdown", ".txt", ".rst", ".json", ".jsonc", ".yml", ".yaml", ".toml", ".ini",
    ".conf", ".config", ".xml", ".csv", ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".css",
    ".scss", ".sass", ".less", ".html", ".htm", ".sh", ".bash", ".zsh", ".fish", ".env",
    ".gitignore", ".gitattributes", ".npmrc", ".editorconfig", ".py", ".java", ".go", ".rs",
    ".c", ".cc", ".cpp", ".h", ".hpp", ".sql", ".graphql", ".proto", ".log", ".lock",
];

const MEDIA_MIME_BY_EXT: &[(&str, &str)] = &[
    (".png", "image/png"), (".jpg", "image/jpeg"), (".jpeg", "image/jpeg"),
    (".gif", "image/gif"), (".webp", "image/webp"), (".bmp", "image/bmp"),
    (".svg", "image/svg+xml"), (".ico", "image/x-icon"),
    (".mp4", "video/mp4"), (".webm", "video/webm"), (".ogg", "video/ogg"),
    (".mov", "video/quicktime"), (".m4v", "video/x-m4v"),
    (".mp3", "audio/mpeg"), (".wav", "audio/wav"), (".flac", "audio/flac"),
    (".m4a", "audio/mp4"), (".aac", "audio/aac"), (".oga", "audio/ogg"),
    (".opus", "audio/opus"), (".pdf", "application/pdf"),
];

fn mime_for_ext(ext: &str) -> Option<&'static str> {
    MEDIA_MIME_BY_EXT.iter().find(|(e, _)| *e == ext).map(|(_, m)| *m)
}

fn is_likely_text(buf: &[u8]) -> bool {
    let probe = buf.len().min(4096);
    !buf[..probe].contains(&0)
}

fn normalize_archive_path(entry_path: &str) -> String {
    entry_path
        .replace('\\', "/")
        .split('/')
        .filter(|s| !s.is_empty() && *s != "." && *s != "..")
        .collect::<Vec<_>>()
        .join("/")
}

fn sort_archive_tree(nodes: &mut [ArchiveTreeNode]) {
    nodes.sort_by(|a, b| {
        if a.is_directory != b.is_directory {
            return if a.is_directory { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater };
        }
        a.name.cmp(&b.name)
    });
    for node in nodes {
        if let Some(ref mut children) = node.children {
            sort_archive_tree(children);
        }
    }
}

fn read_zip_archive(file_path: &str) -> Result<Vec<ArchiveTreeNode>, String> {
    let file = fs::File::open(file_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    let mut roots: Vec<ArchiveTreeNode> = Vec::new();
    let mut node_by_path: HashMap<String, ArchiveTreeNode> = HashMap::new();

    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let raw_name = entry.name().to_string();
        let normalized = normalize_archive_path(&raw_name);
        if normalized.is_empty() { continue; }

        let is_dir = entry.is_dir() || raw_name.ends_with('/');
        let segments: Vec<&str> = normalized.split('/').collect();

        let mut current_path = String::new();
        for (idx, segment) in segments.iter().enumerate() {
            if !current_path.is_empty() { current_path.push('/'); }
            current_path.push_str(segment);
            let is_last = idx == segments.len() - 1;

            if !node_by_path.contains_key(&current_path) {
                let children = if is_last && !is_dir { None } else { Some(Vec::new()) };
                let node = ArchiveTreeNode {
                    name: segment.to_string(),
                    path: current_path.clone(),
                    is_directory: is_dir || !is_last,
                    children,
                };
                node_by_path.insert(current_path.clone(), node);
            } else if is_dir {
                if let Some(existing) = node_by_path.get_mut(&current_path) {
                    if !existing.is_directory {
                        existing.is_directory = true;
                        existing.children = Some(Vec::new());
                    }
                }
            }
        }
    }

    let all_paths: Vec<String> = node_by_path.keys().cloned().collect();
    for path_str in &all_paths {
        let node = node_by_path.get(path_str).unwrap().clone();
        let is_top = !path_str.contains('/');
        if is_top {
            roots.push(node);
        } else {
            let parent_path = path_str.rsplit_once('/').map(|(p, _)| p.to_string()).unwrap();
            if let Some(parent) = node_by_path.get_mut(&parent_path) {
                parent.children.get_or_insert(Vec::new()).push(node);
            } else {
                roots.push(node);
            }
        }
    }

    for node in node_by_path.values_mut() {
        if let Some(ref mut children) = node.children {
            children.sort_by(|a, b| a.path.cmp(&b.path));
            children.dedup_by_key(|c| c.path.clone());
        }
    }

    sort_archive_tree(&mut roots);

    let mut seen_paths = std::collections::HashSet::new();
    roots.retain(|node| seen_paths.insert(node.path.clone()));

    Ok(roots)
}

pub fn preview_file(project_path: String, file_path: String) -> FilePreview {
    if !is_within_root(&project_path, &file_path) {
        return FilePreview {
            kind: "error".to_string(),
            content: None,
            mime_type: None,
            size: None,
            message: Some("Path is outside project root.".to_string()),
            archive_tree: None,
        };
    }

    let path = Path::new(&file_path);
    let Ok(meta) = fs::metadata(path) else {
        return FilePreview {
            kind: "error".to_string(),
            content: None,
            mime_type: None,
            size: None,
            message: Some("Failed to access file.".to_string()),
            archive_tree: None,
        };
    };

    if !meta.is_file() {
        return FilePreview {
            kind: "unsupported".to_string(),
            content: None,
            mime_type: None,
            size: None,
            message: Some("Not a file.".to_string()),
            archive_tree: None,
        };
    }

    let size = meta.len();
    let ext = path.extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e.to_lowercase()))
        .unwrap_or_default();

    if ext == ".zip" {
        match read_zip_archive(&file_path) {
            Ok(tree) => {
                return FilePreview {
                    kind: "archive".to_string(),
                    content: None,
                    mime_type: None,
                    size: Some(size),
                    message: None,
                    archive_tree: Some(tree),
                };
            }
            Err(e) => {
                return FilePreview {
                    kind: "error".to_string(),
                    content: None,
                    mime_type: None,
                    size: None,
                    message: Some(format!("Failed to read archive: {}", e)),
                    archive_tree: None,
                };
            }
        }
    }

    if let Some(mime) = mime_for_ext(&ext) {
        if size > MAX_MEDIA_PREVIEW_BYTES {
            return FilePreview {
                kind: "tooLarge".to_string(),
                content: None,
                mime_type: None,
                size: Some(size),
                message: Some("File is too large for media preview.".to_string()),
                archive_tree: None,
            };
        }
        let Ok(mut file) = fs::File::open(path) else {
            return FilePreview {
                kind: "error".to_string(),
                content: None,
                mime_type: None,
                size: None,
                message: Some("Failed to read file.".to_string()),
                archive_tree: None,
            };
        };
        let mut buf = Vec::new();
        if file.read_to_end(&mut buf).is_ok() {
            let encoded = base64::engine::general_purpose::STANDARD.encode(&buf);
            return FilePreview {
                kind: "media".to_string(),
                content: Some(encoded),
                mime_type: Some(mime.to_string()),
                size: Some(size),
                message: None,
                archive_tree: None,
            };
        }
    }

    if TEXT_EXTENSIONS.contains(&ext.as_str()) {
        if size > MAX_TEXT_PREVIEW_BYTES {
            return FilePreview {
                kind: "tooLarge".to_string(),
                content: None,
                mime_type: None,
                size: Some(size),
                message: Some("File is too large for text preview.".to_string()),
                archive_tree: None,
            };
        }
        match fs::read_to_string(path) {
            Ok(content) => {
                return FilePreview {
                    kind: "text".to_string(),
                    content: Some(content),
                    mime_type: None,
                    size: Some(size),
                    message: None,
                    archive_tree: None,
                };
            }
            Err(_) => {}
        }
    }

    if size <= MAX_TEXT_PREVIEW_BYTES {
        if let Ok(mut file) = fs::File::open(path) {
            let mut buf = Vec::new();
            if file.read_to_end(&mut buf).is_ok() && is_likely_text(&buf) {
                if let Ok(content) = String::from_utf8(buf) {
                    return FilePreview {
                        kind: "text".to_string(),
                        content: Some(content),
                        mime_type: None,
                        size: Some(size),
                        message: None,
                        archive_tree: None,
                    };
                }
            }
        }
    }

    FilePreview {
        kind: "unsupported".to_string(),
        content: None,
        mime_type: None,
        size: Some(size),
        message: Some("Unsupported file format.".to_string()),
        archive_tree: None,
    }
}

pub fn get_markdown_files(project_path: String) -> Vec<String> {
    let dir = Path::new(&project_path);
    let Ok(entries) = fs::read_dir(dir) else { return vec![] };

    let mut files: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().map(|t| t.is_file()).unwrap_or(false)
        })
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|name| {
            let lower = name.to_lowercase();
            lower.ends_with(".md") || lower.ends_with(".txt") || lower.ends_with(".rst")
        })
        .collect();

    files.sort_by(|a, b| {
        let a_is_readme = a.to_lowercase().starts_with("readme");
        let b_is_readme = b.to_lowercase().starts_with("readme");
        if a_is_readme && !b_is_readme { return std::cmp::Ordering::Less; }
        if !a_is_readme && b_is_readme { return std::cmp::Ordering::Greater; }
        a.cmp(b)
    });

    files.into_iter()
        .map(|f| dir.join(f).to_string_lossy().to_string())
        .collect()
}

pub fn read_markdown_file(file_path: String) -> Option<String> {
    fs::read_to_string(&file_path).ok()
}

pub fn write_markdown_file(project_path: String, file_path: String, content: String) -> Result<(), String> {
    if !is_within_root(&project_path, &file_path) {
        return Err("Path is outside project root.".to_string());
    }
    fs::write(&file_path, &content).map_err(|e| format!("Failed to write file: {}", e))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDetail {
    pub markdown_files: Vec<String>,
    pub is_git_repo: bool,
    pub github_url: Option<String>,
}

pub fn get_project_detail(project_path: String) -> ProjectDetail {
    let markdown_files = get_markdown_files(project_path.clone());
    let repo_info = crate::git::get_project_repository_info(project_path);
    ProjectDetail {
        markdown_files,
        is_git_repo: repo_info.is_git_repo,
        github_url: repo_info.github_url,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("sizzle-files-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn write_markdown_file_rejects_traversal_to_a_nonexistent_path() {
        let root = test_root();
        let root_str = root.to_string_lossy().to_string();
        let escape_target = root.parent().unwrap().join(format!("sizzle-escape-{}.md", uuid::Uuid::new_v4()));
        let traversal_path = root.join("..").join(escape_target.file_name().unwrap());

        let result = write_markdown_file(
            root_str,
            traversal_path.to_string_lossy().to_string(),
            "malicious".to_string(),
        );

        assert!(result.is_err());
        assert!(!escape_target.exists());

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn write_markdown_file_allows_a_new_file_inside_the_root() {
        let root = test_root();
        let root_str = root.to_string_lossy().to_string();
        let target = root.join("NEW.md");

        let result = write_markdown_file(
            root_str,
            target.to_string_lossy().to_string(),
            "hello".to_string(),
        );

        assert!(result.is_ok());
        assert_eq!(fs::read_to_string(&target).unwrap(), "hello");

        fs::remove_dir_all(&root).unwrap();
    }

    /// `Path::file_name` returns `None` for a path ending in `..`, which
    /// previously made `resolve_lexically` bail out (rejecting the write)
    /// once the walk-up reached a `..` component after crossing a
    /// nonexistent leaf file. The subdirectory itself must exist, since the
    /// OS can't resolve `..` through a directory that isn't there.
    #[test]
    fn write_markdown_file_allows_dot_dot_back_out_of_an_existing_subdirectory() {
        let root = test_root();
        fs::create_dir_all(root.join("subdir")).unwrap();
        let root_str = root.to_string_lossy().to_string();
        let real_target = root.join("real.md");
        let traversal_path = root.join("subdir").join("..").join("real.md");

        let result = write_markdown_file(
            root_str,
            traversal_path.to_string_lossy().to_string(),
            "hello".to_string(),
        );

        assert!(result.is_ok());
        assert_eq!(fs::read_to_string(&real_target).unwrap(), "hello");

        fs::remove_dir_all(&root).unwrap();
    }

    /// A dangling symlink must be treated as "existing" (via
    /// `symlink_metadata`) so containment fails closed instead of treating
    /// the symlink's name as an un-resolved path component.
    #[test]
    fn write_markdown_file_rejects_through_a_dangling_symlink() {
        let root = test_root();
        let root_str = root.to_string_lossy().to_string();
        let link = root.join("dangling");
        std::os::unix::fs::symlink(root.join("does-not-exist"), &link).unwrap();
        let traversal_path = link.join("new.md");

        let result = write_markdown_file(
            root_str,
            traversal_path.to_string_lossy().to_string(),
            "malicious".to_string(),
        );

        assert!(result.is_err());

        fs::remove_dir_all(&root).unwrap();
    }
}
