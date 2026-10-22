//! UI-neutral application and agent core.
//!
//! This crate is the boundary that keeps product logic independent of the desktop
//! UI framework (D-005) so the same services can later be invoked by a CLI.
//!
//! Delivered so far:
//! - project lifecycle, `project.yaml` validation and safe document IO
//!   (Milestone 2, Spike G-1); on-disk names are English (D-017).
//!
//! What belongs here next:
//! - local Git operations (Milestone 3, Spike D)
//! - agent task orchestration, permissions and proposals (Milestone 4, Spike C/F)
//!
//! Rules for anything added here:
//! - no UI framework types in public APIs; commands are explicit inputs and
//!   structured outputs, progress travels over UI-neutral events;
//! - long-running work is cancellable and bounded by timeouts;
//! - no secrets in errors, logs or events.

pub mod document;
pub mod manifest;
pub mod project;

pub use document::{DocumentError, read_document, resolve_within, write_document};
pub use manifest::{
    MANIFEST_SCHEMA_VERSION, Manifest, ManifestError, ProjectType, parse_manifest,
    serialize_manifest,
};
pub use project::{
    DEFAULT_SCREENPLAY, MANIFEST_FILE, Project, ProjectError, ProjectIndex, SceneRef,
    TEMPLATE_DIRS, create_project, open_project,
};

/// Marker for the core boundary, so the crate is exercised by CI from day one.
pub const CORE_BOUNDARY: &str = "application-core";

#[cfg(test)]
mod tests {
    use super::CORE_BOUNDARY;

    #[test]
    fn core_boundary_is_present() {
        assert_eq!(CORE_BOUNDARY, "application-core");
    }
}
