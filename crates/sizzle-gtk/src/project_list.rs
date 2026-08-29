//! A project list paired with a search-cache index that the type system
//! guarantees stays in sync: `projects` and `search_cache` are private to
//! this module, so the only way to (re)populate them is `ProjectList::new`,
//! which builds both together. There is no way to update one without the
//! other, which makes a stale search cache unrepresentable.

use std::collections::HashMap;
use sizzle_core::ScannedProject;

/// The first detected tag of a project, or the empty string when it has none.
/// The canonical definition of "a project's tag": used both to populate the
/// search cache below and by `populate_list` (in `lib.rs`) to render the tag
/// badge, so the two stay in sync by construction.
pub(crate) fn primary_tag(project: &ScannedProject) -> &str {
    project
        .detected_tags
        .first()
        .map(|t| t.name.as_str())
        .unwrap_or("")
}

/// Cached, pre-lowercased fields used by `apply_search_filter`. See
/// `ProjectList::search_cache`.
pub struct SearchEntry {
    name: String,
    name_lower: String,
    tag: String,
    tag_lower: String,
}

impl SearchEntry {
    /// Substring match against the project's name/tag. When `case_sensitive`
    /// is true the original-cased name/tag are searched; otherwise the cached
    /// lowercased fields are used. The caller computes `case_sensitive` once
    /// per query (smart-case: any uppercase in the query), not once per row.
    pub fn matches(&self, query: &str, query_lower: &str, case_sensitive: bool) -> bool {
        if case_sensitive {
            self.name.contains(query) || self.tag.contains(query)
        } else {
            self.name_lower.contains(query_lower) || self.tag_lower.contains(query_lower)
        }
    }
}

fn build_search_cache(projects: &[ScannedProject]) -> HashMap<String, SearchEntry> {
    projects
        .iter()
        .map(|p| {
            let tag = primary_tag(p).to_string();
            let tag_lower = tag.to_lowercase();
            (
                p.path.clone(),
                SearchEntry {
                    name_lower: p.name.to_lowercase(),
                    name: p.name.clone(),
                    tag,
                    tag_lower,
                },
            )
        })
        .collect()
}

pub struct ProjectList {
    projects: Vec<ScannedProject>,
    search_cache: HashMap<String, SearchEntry>,
}

impl ProjectList {
    pub fn new(projects: Vec<ScannedProject>) -> Self {
        let search_cache = build_search_cache(&projects);
        Self { projects, search_cache }
    }

    pub fn projects(&self) -> &[ScannedProject] {
        &self.projects
    }

    pub fn search_cache(&self) -> &HashMap<String, SearchEntry> {
        &self.search_cache
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, tag: &str) -> SearchEntry {
        SearchEntry {
            name: name.to_string(),
            name_lower: name.to_lowercase(),
            tag: tag.to_string(),
            tag_lower: tag.to_lowercase(),
        }
    }

    #[test]
    fn lowercase_query_matches_regardless_of_name_case() {
        let e = entry("MyProject", "Rust");
        assert!(e.matches("project", &"project".to_lowercase(), false));
    }

    #[test]
    fn case_sensitive_query_uses_original_case() {
        let e = entry("myproject", "Rust");
        assert!(!e.matches("Project", &"project".to_lowercase(), true));
        assert!(e.matches("project", &"project".to_lowercase(), false));
    }

    #[test]
    fn matches_on_tag_as_well_as_name() {
        let e = entry("widgets", "Rust");
        assert!(e.matches("rust", &"rust".to_lowercase(), false));
    }

    #[test]
    fn no_match_when_neither_name_nor_tag_contains_query() {
        let e = entry("widgets", "Rust");
        assert!(!e.matches("python", &"python".to_lowercase(), false));
    }
}
