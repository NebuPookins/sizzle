pub mod metadata;
pub mod scanner;
pub mod git;
pub mod files;
pub mod kanban;

pub use metadata::{
    MetadataStore, ProjectMeta, ProjectTag, ProjectTagOverride, ProjectMarker,
    ScanSettings, AgentPreset,
};
pub use scanner::{ScannedProject, TagDetector, scan_projects};
pub use kanban::{KanbanBoard, KanbanColumn, KanbanCard, AgentBlock};
