//! UI-neutral application and agent core.
//!
//! This crate is the boundary that keeps product logic independent of the desktop
//! UI framework (D-005) so the same services can later be invoked by a CLI.
//!
//! Milestone 0 ships it as an empty placeholder on purpose: the editor proof of
//! concept (Milestone 1) gates feature work. What belongs here later:
//!
//! - project lifecycle and manifest validation (Milestone 2, Spike G-1)
//! - local Git operations (Milestone 3, Spike D)
//! - agent task orchestration, permissions and proposals (Milestone 4, Spike C/F)
//!
//! Rules for anything added here:
//! - no UI framework types in public APIs; commands are explicit inputs and
//!   structured outputs, progress travels over UI-neutral events;
//! - long-running work is cancellable and bounded by timeouts;
//! - no secrets in errors, logs or events.

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
