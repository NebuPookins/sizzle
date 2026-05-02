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

fn is_within_root(root: &str, candidate: &str) -> bool {
    let root = Path::new(root).canonicalize().unwrap_or_else(|_| Path::new(root).to_path_buf());
    let candidate = Path::new(candidate).canonicalize().unwrap_or_else(|_| Path::new(candidate).to_path_buf());
    candidate == root || candidate.starts_with(&root)
}

#[tauri::command]
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

    // Build tree from flat map (clone nodes to avoid borrow conflicts)
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

    // Deduplicate children
    for node in node_by_path.values_mut() {
        if let Some(ref mut children) = node.children {
            children.sort_by(|a, b| a.path.cmp(&b.path));
            children.dedup_by_key(|c| c.path.clone());
        }
    }

    // Re-sort the tree
    sort_archive_tree(&mut roots);

    // Deduplicate roots
    let mut seen_paths = std::collections::HashSet::new();
    roots.retain(|node| seen_paths.insert(node.path.clone()));

    Ok(roots)
}

#[tauri::command]
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

    // ZIP archives
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

    // Known media types
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

    // Text extensions
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

    // Check if it is likely text even without known extension
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

// Markdown helpers

#[tauri::command]
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

#[tauri::command]
pub fn read_markdown_file(file_path: String) -> Option<String> {
    fs::read_to_string(&file_path).ok()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDetail {
    pub markdown_files: Vec<String>,
    pub is_git_repo: bool,
    pub github_url: Option<String>,
}

#[tauri::command]
pub fn get_project_detail(project_path: String) -> ProjectDetail {
    let markdown_files = get_markdown_files(project_path.clone());
    let repo_info = super::git::get_project_repository_info(project_path);
    ProjectDetail {
        markdown_files,
        is_git_repo: repo_info.is_git_repo,
        github_url: repo_info.github_url,
    }
}
