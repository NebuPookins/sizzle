use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTag {
    pub name: String,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTagOverride {
    pub tags: Vec<ProjectTag>,
    #[serde(rename = "primaryTag")]
    pub primary_tag: Option<String>,
}

pub type ProjectMarker = Option<String>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMeta {
    pub last_launched: Option<i64>,
    pub tag_override: Option<ProjectTagOverride>,
    pub marker: ProjectMarker,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSettings {
    pub scan_roots: Vec<String>,
    pub ignore_roots: Vec<String>,
    pub manual_project_roots: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DB {
    projects: HashMap<String, ProjectMeta>,
    #[serde(default)]
    scan_settings: Option<ScanSettings>,
}

pub struct MetadataStore {
    config_dir: PathBuf,
    db_path: PathBuf,
    tmp_path: PathBuf,
    cache: Mutex<Option<DB>>,
}

impl MetadataStore {
    pub fn new(config_dir: PathBuf) -> Self {
        let db_path = config_dir.join("db.json");
        let tmp_path = config_dir.join("db.json.tmp");
        log::info!("[sizzle] MetadataStore db_path: {:?}", db_path);
        Self {
            config_dir,
            db_path,
            tmp_path,
            cache: Mutex::new(None),
        }
    }

    fn ensure_dir(&self) {
        fs::create_dir_all(&self.config_dir).ok();
    }

    fn read_db(&self) -> DB {
        let mut cache = self.cache.lock().unwrap();
        if let Some(ref db) = *cache {
            return db.clone();
        }
        self.ensure_dir();
        let db = match fs::read_to_string(&self.db_path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|e| {
                log::info!("[sizzle] Failed to parse db.json: {}. First 500 chars: {}", e, &raw[..raw.len().min(500)]);
                DB {
                    projects: HashMap::new(),
                    scan_settings: None,
                }
            }),
            Err(e) => {
                log::info!("[sizzle] Failed to read db.json: {}", e);
                DB {
                    projects: HashMap::new(),
                    scan_settings: None,
                }
            },
        };
        log::info!("[sizzle] read_db: {} projects, scan_settings: {}",
            db.projects.len(),
            if db.scan_settings.is_some() { "present" } else { "none" }
        );
        *cache = Some(db.clone());
        db
    }

    fn write_db(&self, db: &DB) {
        self.ensure_dir();
        let json = serde_json::to_string_pretty(db).unwrap();
        fs::write(&self.tmp_path, &json).ok();
        fs::rename(&self.tmp_path, &self.db_path).ok();
        let mut cache = self.cache.lock().unwrap();
        *cache = Some(db.clone());
    }

    fn normalize_root_path(root: &str) -> String {
        Path::new(root.trim()).canonicalize().unwrap_or_else(|_| PathBuf::from(root.trim()))
            .to_string_lossy()
            .to_string()
    }

    fn sanitize_root_list(values: &[String]) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        for v in values {
            let trimmed = v.trim().to_string();
            if trimmed.is_empty() { continue; }
            let normalized = Self::normalize_root_path(&trimmed);
            if normalized.is_empty() { continue; }
            if seen.insert(normalized.clone()) {
                result.push(normalized);
            }
        }
        result
    }

    fn normalize_marker(marker: &ProjectMarker) -> ProjectMarker {
        match marker.as_deref() {
            Some("favorite") | Some("ignored") => marker.clone(),
            _ => None,
        }
    }

    fn normalize_tag_name(name: &str) -> String {
        name.trim().split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn normalize_tag_override(override_val: &ProjectTagOverride) -> ProjectTagOverride {
        let mut tag_map: HashMap<String, f64> = HashMap::new();
        for tag in &override_val.tags {
            let name = Self::normalize_tag_name(&tag.name);
            if name.is_empty() { continue; }
            let score = if tag.score.is_finite() && tag.score > 0.0 { tag.score } else { 0.0 };
            let entry = tag_map.entry(name).or_insert(0.0);
            *entry = entry.max(score);
        }

        let mut tags: Vec<ProjectTag> = tag_map
            .into_iter()
            .map(|(name, score)| ProjectTag { name, score })
            .collect();

        tags.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
            .then(a.name.cmp(&b.name)));

        let total_score: f64 = tags.iter().map(|t| t.score).sum();
        if total_score > 0.0 {
            for tag in &mut tags {
                tag.score /= total_score;
            }
        } else if !tags.is_empty() {
            let equal = 1.0 / tags.len() as f64;
            for tag in &mut tags {
                tag.score = equal;
            }
        }

        let primary_tag = override_val.primary_tag.as_ref()
            .map(|t| Self::normalize_tag_name(t))
            .filter(|t| !t.is_empty())
            .and_then(|t| {
                if tags.iter().any(|tag| tag.name == t) { Some(t) } else { None }
            })
            .or_else(|| tags.first().map(|t| t.name.clone()));

        ProjectTagOverride { tags, primary_tag }
    }

    pub fn get_metadata(&self, project_path: &str) -> ProjectMeta {
        let db = self.read_db();
        db.projects.get(project_path).cloned().unwrap_or(ProjectMeta {
            last_launched: None,
            tag_override: None,
            marker: None,
        })
    }

    pub fn set_last_launched(&self, project_path: &str) {
        let mut db = self.read_db();
        {
            let entry = db.projects.entry(project_path.to_string()).or_insert(ProjectMeta {
                last_launched: None,
                tag_override: None,
                marker: None,
            });
            entry.last_launched = Some(chrono::Utc::now().timestamp_millis());
        }
        self.write_db(&db);
    }

    pub fn get_all_metadata(&self) -> HashMap<String, ProjectMeta> {
        let db = self.read_db();
        log::info!("[sizzle] get_all_metadata: {} entries in db", db.projects.len());
        let mut changed = false;
        let mut result = db.projects.clone();

        let before = result.len();
        result.retain(|path, _| Path::new(path).exists());
        let retained = result.len();
        if retained != before {
            log::info!("[sizzle]   removed {} non-existent paths", before - retained);
        }

        for meta in result.values_mut() {
            if meta.tag_override.is_none() {
                // ensure it's None, not a weird value
            }
            let normalized = Self::normalize_marker(&meta.marker);
            if meta.marker != normalized {
                meta.marker = normalized;
                changed = true;
            }
        }

        if changed {
            let mut db = self.read_db();
            db.projects = result.clone();
            self.write_db(&db);
        }

        result
    }

    #[allow(dead_code)]
    fn trim_missing(&self, db: &mut DB) -> bool {
        let before = db.projects.len();
        db.projects.retain(|path, _| Path::new(path).exists());
        db.projects.len() != before
    }

    pub fn set_tag_override(&self, project_path: &str, override_val: Option<ProjectTagOverride>) -> ProjectMeta {
        let mut db = self.read_db();
        let result = {
            let entry = db.projects.entry(project_path.to_string()).or_insert(ProjectMeta {
                last_launched: None,
                tag_override: None,
                marker: None,
            });
            entry.tag_override = override_val.map(|o| Self::normalize_tag_override(&o));
            entry.marker = Self::normalize_marker(&entry.marker);
            entry.clone()
        };
        self.write_db(&db);
        result
    }

    pub fn set_project_marker(&self, project_path: &str, marker: ProjectMarker) -> ProjectMeta {
        let mut db = self.read_db();
        let result = {
            let entry = db.projects.entry(project_path.to_string()).or_insert(ProjectMeta {
                last_launched: None,
                tag_override: None,
                marker: None,
            });
            entry.marker = Self::normalize_marker(&marker);
            entry.clone()
        };
        self.write_db(&db);
        result
    }

    pub fn rename_project_metadata(&self, old_path: &str, new_path: &str) {
        let mut db = self.read_db();
        if let Some(meta) = db.projects.remove(old_path) {
            db.projects.insert(new_path.to_string(), meta);
            self.write_db(&db);
        }
    }

    pub fn get_scan_settings(&self) -> ScanSettings {
        let db = self.read_db();
        let settings = db.scan_settings.clone().unwrap_or(ScanSettings {
            scan_roots: vec![],
            ignore_roots: vec![],
            manual_project_roots: vec![],
        });

        log::info!("[sizzle] get_scan_settings: {} scan_roots, {} ignore_roots, {} manual_roots",
            settings.scan_roots.len(),
            settings.ignore_roots.len(),
            settings.manual_project_roots.len(),
        );
        if !settings.scan_roots.is_empty() {
            log::info!("[sizzle]   first scan_root: {:?}", settings.scan_roots[0]);
        }

        let normalized = ScanSettings {
            scan_roots: Self::sanitize_root_list(&settings.scan_roots),
            ignore_roots: Self::sanitize_root_list(&settings.ignore_roots),
            manual_project_roots: Self::sanitize_root_list(&settings.manual_project_roots),
        };

        // Write back if normalized differs
        if normalized != settings {
            let mut db = self.read_db();
            db.scan_settings = Some(normalized.clone());
            self.write_db(&db);
        }

        normalized
    }

    pub fn set_scan_settings(&self, settings: &ScanSettings) -> ScanSettings {
        let normalized = ScanSettings {
            scan_roots: Self::sanitize_root_list(&settings.scan_roots),
            ignore_roots: Self::sanitize_root_list(&settings.ignore_roots),
            manual_project_roots: Self::sanitize_root_list(&settings.manual_project_roots),
        };
        let mut db = self.read_db();
        db.scan_settings = Some(normalized.clone());
        self.write_db(&db);
        normalized
    }
}

// Tauri commands

#[tauri::command]
pub fn get_metadata(state: State<'_, MetadataStore>, project_path: String) -> ProjectMeta {
    state.get_metadata(&project_path)
}

#[tauri::command]
pub fn get_all_metadata(state: State<'_, MetadataStore>) -> HashMap<String, ProjectMeta> {
    state.get_all_metadata()
}

#[tauri::command]
pub fn set_last_launched(state: State<'_, MetadataStore>, project_path: String) {
    state.set_last_launched(&project_path);
}

#[tauri::command]
pub fn set_tag_override(
    state: State<'_, MetadataStore>,
    project_path: String,
    override_val: Option<ProjectTagOverride>,
) -> ProjectMeta {
    state.set_tag_override(&project_path, override_val)
}

#[tauri::command]
pub fn set_project_marker(
    state: State<'_, MetadataStore>,
    project_path: String,
    marker: ProjectMarker,
) -> ProjectMeta {
    state.set_project_marker(&project_path, marker)
}

#[tauri::command]
pub fn get_scan_settings(state: State<'_, MetadataStore>) -> ScanSettings {
    state.get_scan_settings()
}

#[tauri::command]
pub fn set_scan_settings(
    state: State<'_, MetadataStore>,
    settings: ScanSettings,
) -> ScanSettings {
    state.set_scan_settings(&settings)
}

#[tauri::command]
pub fn add_ignore_root(state: State<'_, MetadataStore>, root_path: String) -> ScanSettings {
    let mut current = state.get_scan_settings();
    current.ignore_roots.push(root_path);
    state.set_scan_settings(&current)
}
