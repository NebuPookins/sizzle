use crate::commands::metadata::{ProjectTag, MetadataStore};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannedProject {
    pub name: String,
    pub path: String,
    pub readme_files: Vec<String>,
    pub detected_tags: Vec<ProjectTag>,
}

const SKIP_DIRS: &[&str] = &[
    "node_modules", ".git", "target", "dist", "build", "__pycache__",
    "vendor", "venv", ".venv", ".cache", ".npm", ".cargo", "out",
];

fn is_path_within_root(root: &str, candidate: &str) -> bool {
    let root = Path::new(root);
    let candidate = Path::new(candidate);
    candidate == root || candidate.starts_with(root)
}

fn should_skip_by_ignore_roots(dir: &str, ignore_roots: &[String]) -> bool {
    ignore_roots.iter().any(|r| is_path_within_root(r, dir))
}

fn is_under_manual_root(dir: &str, manual_roots: &HashSet<String>) -> bool {
    manual_roots.iter().any(|r| {
        if dir == r { return false; }
        is_path_within_root(r, dir)
    })
}

fn collect_readme_files(_dir: &Path, entries: &[fs::DirEntry]) -> Vec<String> {
    entries.iter()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter(|e| e.file_name().to_string_lossy().to_lowercase().starts_with("readme"))
        .map(|e| e.path().to_string_lossy().to_string())
        .collect()
}

fn try_read_dir(dir: &Path) -> Option<Vec<fs::DirEntry>> {
    match fs::read_dir(dir) {
        Ok(entries) => Some(entries.filter_map(|e| e.ok()).collect()),
        Err(_) => None,
    }
}

fn scan_dir(
    dir: &Path,
    ignore_roots: &[String],
    manual_roots: &HashSet<String>,
    results: &mut Vec<ScannedProject>,
    tags: &TagDetector,
) {
    let dir_str = dir.to_string_lossy().to_string();
    if should_skip_by_ignore_roots(&dir_str, ignore_roots) { return; }
    if is_under_manual_root(&dir_str, manual_roots) { return; }

    let entries = match try_read_dir(dir) {
        Some(e) => e,
        None => return,
    };

    let dir_str = dir.to_string_lossy().to_string();
    if manual_roots.contains(&dir_str) || is_project_root(dir) {
        let readme_files = collect_readme_files(dir, &entries);
        results.push(ScannedProject {
            name: dir.file_name().unwrap().to_string_lossy().to_string(),
            path: dir_str,
            readme_files: readme_files.iter().map(|f| Path::new(f).to_string_lossy().to_string()).collect(),
            detected_tags: tags.detect(dir),
        });
        return;
    }

    for entry in &entries {
        let Ok(file_type) = entry.file_type() else { continue };
        if !file_type.is_dir() { continue; }
        let name = entry.file_name().to_string_lossy().to_string();
        if SKIP_DIRS.contains(&name.as_str()) { continue; }
        if name.starts_with('.') { continue; }
        scan_dir(&entry.path(), ignore_roots, manual_roots, results, tags);
    }
}

fn is_project_root(dir: &Path) -> bool {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries.filter_map(|e| e.ok()).collect::<Vec<_>>(),
        Err(_) => return false,
    };

    let entry_names: Vec<String> = entries.iter()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    // Has .git directory
    if entry_names.iter().any(|n| n == ".git") { return true; }

    // Has README or AGENTS.md
    if entry_names.iter().any(|n| {
        let lower = n.to_lowercase();
        lower.starts_with("readme") || lower == "agents.md"
    }) { return true; }

    // Known manifest files
    const MANIFESTS: &[&str] = &[
        "Cargo.toml", "package.json", "go.mod", "pyproject.toml", "setup.py",
        "pom.xml", "build.gradle", "Makefile", "CMakeLists.txt", "meson.build",
        "mix.exs", "composer.json", "Gemfile", "pubspec.yaml", "build.sbt",
        "project.clj", "deps.edn", "flex-config.xml", "air-app.xml",
    ];
    if entry_names.iter().any(|n| MANIFESTS.contains(&n.as_str())) { return true; }

    // Simple static web app
    const WEB_ENTRIES: &[&str] = &["index.html", "index.htm"];
    const WEB_ASSET_EXTS: &[&str] = &[".js", ".jsx", ".ts", ".tsx", ".css", ".scss", ".sass", ".less"];
    let has_html = entry_names.iter().any(|n| WEB_ENTRIES.contains(&n.to_lowercase().as_str()));
    let has_asset = entry_names.iter().any(|n| {
        let ext = Path::new(n).extension().and_then(|e| e.to_str()).unwrap_or("");
        let ext = format!(".{}", ext.to_lowercase());
        WEB_ASSET_EXTS.contains(&ext.as_str())
    });
    if has_html && has_asset { return true; }

    // >= 3 source files
    const SOURCE_EXTS: &[&str] = &[
        ".rs", ".ts", ".tsx", ".js", ".jsx", ".py", ".java", ".kt", ".go",
        ".c", ".cpp", ".cc", ".cxx", ".h", ".hpp", ".cs", ".rb", ".swift",
        ".scala", ".clj", ".ex", ".exs", ".hs", ".ml", ".elm", ".dart",
        ".lua", ".php", ".r", ".jl", ".asm", ".z80", ".as", ".mxml",
    ];
    let source_count = entries.iter().filter(|e| {
        e.file_type().map(|t| t.is_file()).unwrap_or(false)
    }).filter(|e| {
        let ext = Path::new(&e.file_name().to_string_lossy().to_string())
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{}", e.to_lowercase()))
            .unwrap_or_default();
        SOURCE_EXTS.contains(&ext.as_str())
    }).count();

    source_count >= 3
}

// --- Tag detection ---

const MAX_SAMPLED_FILES: usize = 6000;

const EXTENSION_LANGUAGES: &[(&str, &str)] = &[
    (".ts", "TypeScript"), (".tsx", "TypeScript"), (".mts", "TypeScript"), (".cts", "TypeScript"),
    (".js", "JavaScript"), (".jsx", "JavaScript"), (".mjs", "JavaScript"), (".cjs", "JavaScript"),
    (".as", "ActionScript"), (".mxml", "ActionScript"),
    (".py", "Python"), (".go", "Go"), (".rs", "Rust"),
    (".java", "Java"), (".kt", "Kotlin"), (".kts", "Kotlin"),
    (".c", "C"), (".h", "C"), (".cpp", "C++"), (".cc", "C++"), (".cxx", "C++"), (".hpp", "C++"),
    (".cs", "C#"), (".rb", "Ruby"), (".php", "PHP"), (".swift", "Swift"),
    (".scala", "Scala"), (".dart", "Dart"), (".ex", "Elixir"), (".exs", "Elixir"),
    (".clj", "Clojure"), (".hs", "Haskell"), (".ml", "OCaml"), (".gd", "GDScript"),
    (".lua", "Lua"), (".r", "R"), (".jl", "Julia"), (".d", "D"),
    (".asm", "Z80 Assembly"), (".z80", "Z80 Assembly"),
];

fn ext_to_language(ext: &str) -> Option<&'static str> {
    EXTENSION_LANGUAGES.iter().find(|(e, _)| *e == ext).map(|(_, l)| *l)
}

struct FrameworkRule {
    tag: &'static str,
    package_names: &'static [&'static str],
    files: &'static [&'static str],
    manifests: &'static [&'static str],
}

const FRAMEWORK_RULES: &[FrameworkRule] = &[
    FrameworkRule { tag: "React", package_names: &["react"], files: &[], manifests: &[] },
    FrameworkRule { tag: "Next.js", package_names: &["next"], files: &["next.config.js", "next.config.mjs", "next.config.ts"], manifests: &[] },
    FrameworkRule { tag: "Vue", package_names: &["vue"], files: &[], manifests: &[] },
    FrameworkRule { tag: "Angular", package_names: &["@angular/core", "@angular/cli"], files: &[], manifests: &[] },
    FrameworkRule { tag: "Svelte", package_names: &["svelte"], files: &[], manifests: &[] },
    FrameworkRule { tag: "SvelteKit", package_names: &["@sveltejs/kit"], files: &[], manifests: &[] },
    FrameworkRule { tag: "Electron", package_names: &["electron", "electron-vite"], files: &[], manifests: &[] },
    FrameworkRule { tag: "Express", package_names: &["express"], files: &[], manifests: &[] },
    FrameworkRule { tag: "Django", package_names: &[], files: &["manage.py"], manifests: &["pyproject.toml", "requirements.txt"] },
    FrameworkRule { tag: "Flask", package_names: &[], files: &[], manifests: &["requirements.txt", "pyproject.toml"] },
    FrameworkRule { tag: "Ruby on Rails", package_names: &[], files: &["config.ru"], manifests: &["Gemfile"] },
    FrameworkRule { tag: "Godot", package_names: &[], files: &["project.godot"], manifests: &[] },
];

fn read_package_json(dir: &Path) -> HashSet<String> {
    let pkg_path = dir.join("package.json");
    let Ok(content) = fs::read_to_string(&pkg_path) else { return HashSet::new() };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else { return HashSet::new() };
    let mut names = HashSet::new();
    if let Some(deps) = json.get("dependencies").and_then(|v| v.as_object()) {
        for key in deps.keys() { names.insert(key.clone()); }
    }
    if let Some(deps) = json.get("devDependencies").and_then(|v| v.as_object()) {
        for key in deps.keys() { names.insert(key.clone()); }
    }
    if let Some(deps) = json.get("peerDependencies").and_then(|v| v.as_object()) {
        for key in deps.keys() { names.insert(key.clone()); }
    }
    names
}

fn has_text_in_file(path: &Path, pattern: &str) -> bool {
    let Ok(content) = fs::read_to_string(path) else { return false };
    content.contains(pattern)
}

pub struct TagDetector;

impl TagDetector {
    pub fn detect(&self, root_dir: &Path) -> Vec<ProjectTag> {
        let mut scores: HashMap<String, f64> = HashMap::new();
        let walks = walk_project(root_dir);

        if walks.total_source_files > 0 {
            for (ext, count) in &walks.extension_counts {
                if let Some(lang) = ext_to_language(ext) {
                    let score = *count as f64 / walks.total_source_files as f64;
                    *scores.entry(lang.to_string()).or_insert(0.0) += score;
                }
            }
        }

        // Manifest signals
        const MANIFEST_SIGNALS: &[(&str, &str, f64)] = &[
            ("Cargo.toml", "Rust", 0.9), ("go.mod", "Go", 0.9),
            ("pyproject.toml", "Python", 0.75), ("requirements.txt", "Python", 0.6),
            ("setup.py", "Python", 0.65), ("pom.xml", "Java", 0.85),
            ("build.gradle", "Java", 0.8), ("build.gradle.kts", "Kotlin", 0.8),
            ("Gemfile", "Ruby", 0.85), ("composer.json", "PHP", 0.8),
            ("pubspec.yaml", "Dart", 0.85), ("mix.exs", "Elixir", 0.85),
            ("project.clj", "Clojure", 0.85), ("deps.edn", "Clojure", 0.7),
            ("dub.json", "D", 0.85), ("dub.sdl", "D", 0.85),
        ];
        for (file, tag, score) in MANIFEST_SIGNALS {
            if root_dir.join(file).exists() {
                *scores.entry(tag.to_string()).or_insert(0.0) += score;
            }
        }

        // Framework signals
        let package_names = read_package_json(root_dir);
        for rule in FRAMEWORK_RULES {
            let mut value = 0.0;

            if !rule.package_names.is_empty() && rule.package_names.iter().any(|n| package_names.contains(*n)) {
                value += 0.9;
            }

            if !rule.files.is_empty() && rule.files.iter().any(|f| {
                walks.file_names.contains(*f) || root_dir.join(f).exists()
            }) {
                value += 0.5;
            }

            if !rule.manifests.is_empty() && rule.manifests.iter().any(|m| root_dir.join(m).exists()) {
                match rule.tag {
                    "Django" if has_text_in_file(&root_dir.join("requirements.txt"), "django") => value += 0.8,
                    "Flask" if has_text_in_file(&root_dir.join("requirements.txt"), "flask") => value += 0.8,
                    "Ruby on Rails" if has_text_in_file(&root_dir.join("Gemfile"), "rails") => value += 0.8,
                    _ => {}
                }
            }

            if value > 0.0 {
                *scores.entry(rule.tag.to_string()).or_insert(0.0) += value;
            }
        }

        let total_score: f64 = scores.values().sum();
        if total_score <= 0.0 { return vec![]; }

        let mut result: Vec<ProjectTag> = scores.into_iter()
            .map(|(name, score)| ProjectTag {
                name,
                score: (score / total_score).clamp(0.0, 1.0),
            })
            .collect();

        result.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
            .then(a.name.cmp(&b.name)));

        result
    }
}

struct WalkResult {
    extension_counts: HashMap<String, usize>,
    file_names: HashSet<String>,
    total_source_files: usize,
}

fn walk_project(root_dir: &Path) -> WalkResult {
    let mut extension_counts: HashMap<String, usize> = HashMap::new();
    let mut file_names = HashSet::new();
    let mut total_source_files = 0usize;
    let mut sampled = 0usize;
    let mut stack = vec![root_dir.to_path_buf()];

    while let Some(current) = stack.pop() {
        if sampled >= MAX_SAMPLED_FILES { break; }
        let Ok(entries) = fs::read_dir(&current) else { continue };

        for entry in entries.flatten() {
            if sampled >= MAX_SAMPLED_FILES { break; }
            let Ok(file_type) = entry.file_type() else { continue };

            if file_type.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if SKIP_DIRS.contains(&name.as_str()) || name.starts_with('.') { continue; }
                stack.push(entry.path());
                continue;
            }

            if !file_type.is_file() { continue; }

            sampled += 1;
            let fname = entry.file_name().to_string_lossy().to_string();
            file_names.insert(fname.clone());

            let ext = Path::new(&fname)
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| format!(".{}", e.to_lowercase()))
                .unwrap_or_default();

            if ext.is_empty() { continue; }
            if ext_to_language(&ext).is_none() { continue; }

            total_source_files += 1;
            *extension_counts.entry(ext).or_insert(0) += 1;
        }
    }

    WalkResult { extension_counts, file_names, total_source_files }
}

// Tauri commands

#[tauri::command]
pub fn scan_projects(state: State<'_, MetadataStore>) -> Vec<ScannedProject> {
    let settings = state.get_scan_settings();
    log::info!("[sizzle] scan_projects: {} scan_roots", settings.scan_roots.len());
    if settings.scan_roots.is_empty() {
        log::info!("[sizzle]   scan_roots empty, returning empty list");
        return vec![];
    }

    let mut results = Vec::new();
    let tag_detector = TagDetector;

    let manual_roots: HashSet<String> = settings.manual_project_roots.iter()
        .filter(|r| !should_skip_by_ignore_roots(r, &settings.ignore_roots))
        .cloned()
        .collect();

    for manual_root in &manual_roots {
        let dir = Path::new(manual_root);
        if let Some(entries) = try_read_dir(dir) {
            let readme_files = collect_readme_files(dir, &entries);
            results.push(ScannedProject {
                name: dir.file_name().unwrap().to_string_lossy().to_string(),
                path: manual_root.clone(),
                readme_files: readme_files.iter().map(|f| f.to_string()).collect(),
                detected_tags: tag_detector.detect(dir),
            });
        }
    }

    for root_dir in &settings.scan_roots {
        scan_dir(Path::new(root_dir), &settings.ignore_roots, &manual_roots, &mut results, &tag_detector);
    }

    // Deduplicate
    let mut seen = HashSet::new();
    results.retain(|p| seen.insert(p.path.clone()));

    log::info!("[sizzle]   scan_projects returning {} projects", results.len());
    results
}

#[tauri::command]
pub fn rescan_project_tags(project_path: String) -> Vec<ProjectTag> {
    let detector = TagDetector;
    detector.detect(Path::new(&project_path))
}
