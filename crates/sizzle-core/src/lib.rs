pub mod metadata;
pub mod scanner;
pub mod git;
pub mod files;

pub use metadata::{
    MetadataStore, ProjectMeta, ProjectTag, ProjectTagOverride, ProjectMarker,
    ScanSettings, AgentPreset,
};
pub use scanner::{ScannedProject, TagDetector, scan_projects};
