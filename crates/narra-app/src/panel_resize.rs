//! Side-panel width limits and drag math (M16 slice 3, D-042).
//!
//! The explorer and the right-hand chat/review panel are resized with the
//! mouse. The widths live in the app's view state; the limits and the fit
//! rules live here so they can be tested without GPUI. Width persistence is
//! the open part of D-042 and is not handled by this module.

/// Default width of the explorer panel.
pub const EXPLORER_DEFAULT_W: f32 = 250.0;
/// Default width of the right-hand chat/review panel.
pub const RIGHT_PANEL_DEFAULT_W: f32 = 420.0;

pub const EXPLORER_MIN_W: f32 = 180.0;
pub const EXPLORER_MAX_W: f32 = 420.0;
pub const RIGHT_PANEL_MIN_W: f32 = 300.0;
pub const RIGHT_PANEL_MAX_W: f32 = 640.0;

/// The document column never gets narrower than this while dragging or when
/// the window shrinks.
pub const EDITOR_MIN_W: f32 = 320.0;

/// Width of one drag handle between a side panel and the document column.
pub const SPLITTER_W: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Explorer,
    RightPanel,
}

/// The side-panel state a drag or a fit depends on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Panels {
    pub explorer_open: bool,
    pub right_open: bool,
    pub explorer_w: f32,
    pub right_w: f32,
}

fn splitter_total(panels: Panels) -> f32 {
    (usize::from(panels.explorer_open) + usize::from(panels.right_open)) as f32 * SPLITTER_W
}

fn limits(target: Target) -> (f32, f32) {
    match target {
        Target::Explorer => (EXPLORER_MIN_W, EXPLORER_MAX_W),
        Target::RightPanel => (RIGHT_PANEL_MIN_W, RIGHT_PANEL_MAX_W),
    }
}

/// Width after a drag. `delta` is the pointer movement from the start; the
/// explorer grows to the right and the right panel grows to the left. The
/// result respects the panel limits and keeps the document column at
/// [`EDITOR_MIN_W`], so a drag can never squeeze the editor below its minimum.
pub fn drag_width(
    target: Target,
    start_w: f32,
    delta: f32,
    viewport_w: f32,
    panels: Panels,
) -> f32 {
    let other_w = match target {
        Target::Explorer if panels.right_open => panels.right_w,
        Target::RightPanel if panels.explorer_open => panels.explorer_w,
        _ => 0.0,
    };
    let available = (viewport_w - splitter_total(panels) - other_w - EDITOR_MIN_W).max(0.0);
    let (min, max) = limits(target);
    // A window narrower than the minimums cannot satisfy everything; the
    // panel keeps its minimum and the editor gives way, never the reverse.
    let max = max.min(available).max(min);
    let proposed = match target {
        Target::Explorer => start_w + delta,
        Target::RightPanel => start_w - delta,
    };
    proposed.clamp(min, max)
}

/// Widths to display: clamp to the limits, then shrink the right panel first
/// (and the explorer after it) so the document column keeps
/// [`EDITOR_MIN_W`] when the window gets narrower.
pub fn fit(viewport_w: f32, panels: Panels) -> (f32, f32) {
    let mut explorer = if panels.explorer_open {
        panels.explorer_w.clamp(EXPLORER_MIN_W, EXPLORER_MAX_W)
    } else {
        0.0
    };
    let mut right = if panels.right_open {
        panels.right_w.clamp(RIGHT_PANEL_MIN_W, RIGHT_PANEL_MAX_W)
    } else {
        0.0
    };
    let available = (viewport_w - splitter_total(panels) - EDITOR_MIN_W).max(0.0);
    let mut excess = (explorer + right - available).max(0.0);
    if excess > 0.0 {
        let reducible = (right
            - if panels.right_open {
                RIGHT_PANEL_MIN_W
            } else {
                0.0
            })
        .max(0.0);
        let take = excess.min(reducible);
        right -= take;
        excess -= take;
    }
    if excess > 0.0 {
        let reducible = (explorer
            - if panels.explorer_open {
                EXPLORER_MIN_W
            } else {
                0.0
            })
        .max(0.0);
        explorer -= excess.min(reducible);
    }
    (explorer, right)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn both() -> Panels {
        Panels {
            explorer_open: true,
            right_open: true,
            explorer_w: EXPLORER_DEFAULT_W,
            right_w: RIGHT_PANEL_DEFAULT_W,
        }
    }

    #[test]
    fn drag_clamps_to_the_limits_and_keeps_the_editor_minimum() {
        // 1100 - 12 handles - 420 right panel - 320 editor = 348.
        assert_eq!(
            drag_width(Target::Explorer, 250.0, 1000.0, 1100.0, both()),
            348.0
        );
        assert_eq!(
            drag_width(Target::Explorer, 250.0, -1000.0, 1100.0, both()),
            EXPLORER_MIN_W
        );
        // 1100 - 12 handles - 250 explorer - 320 editor = 518.
        assert_eq!(
            drag_width(Target::RightPanel, 420.0, -1000.0, 1100.0, both()),
            518.0
        );
        assert_eq!(
            drag_width(Target::RightPanel, 420.0, 1000.0, 1100.0, both()),
            RIGHT_PANEL_MIN_W
        );
    }

    #[test]
    fn drag_stops_before_the_editor_minimum_on_a_narrow_window() {
        // 900 - 12 - 250 - 320 = 318, so the right panel cannot reach 420.
        assert_eq!(
            drag_width(Target::RightPanel, 420.0, -1000.0, 900.0, both()),
            318.0
        );
        // 900 - 12 - 420 - 320 = 148, below the explorer minimum.
        assert_eq!(
            drag_width(Target::Explorer, 250.0, 1000.0, 900.0, both()),
            EXPLORER_MIN_W
        );
    }

    #[test]
    fn fit_keeps_the_defaults_when_the_window_fits() {
        assert_eq!(fit(1100.0, both()), (250.0, 420.0));
        assert_eq!(
            fit(
                1100.0,
                Panels {
                    right_open: false,
                    right_w: 0.0,
                    ..both()
                }
            ),
            (250.0, 0.0)
        );
        assert_eq!(
            fit(
                1100.0,
                Panels {
                    explorer_open: false,
                    explorer_w: 0.0,
                    ..both()
                }
            ),
            (0.0, 420.0)
        );
    }

    #[test]
    fn fit_shrinks_the_right_panel_first_to_protect_the_editor() {
        // 900 - 12 - 320 = 568 available for panels; 250 + 420 = 670, so
        // 102 come off the right panel.
        assert_eq!(fit(900.0, both()), (250.0, 318.0));
    }

    #[test]
    fn fit_never_goes_below_the_panel_minimums() {
        assert_eq!(fit(500.0, both()), (EXPLORER_MIN_W, RIGHT_PANEL_MIN_W));
    }
}
