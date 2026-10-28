//! Chat transcript scrollbar geometry (M16 slice 4, D-042).
//!
//! GPUI 0.2.2 reserves scrollbar space and exposes scroll state through
//! [`gpui::ScrollHandle`], but paints no scrollbar for scroll containers, so
//! the app draws its own. This module owns the pure geometry and the
//! sticky-bottom rule so they can be tested without a window.

/// Width of the scrollbar gutter on the right edge of the transcript.
pub const WIDTH: f32 = 10.0;
/// Width of the thumb inside the gutter.
pub const THUMB_W: f32 = 4.0;
/// The thumb never gets shorter than this, so long transcripts stay grabbable.
pub const THUMB_MIN_H: f32 = 24.0;
/// A position within this distance of the bottom counts as "at the bottom".
pub const BOTTOM_EPSILON: f32 = 2.0;

/// `(top, height)` of the thumb inside a track of `viewport_h`, or `None`
/// when the content does not overflow.
pub fn thumb(viewport_h: f32, max_offset: f32, offset_y: f32) -> Option<(f32, f32)> {
    if viewport_h <= 0.0 || max_offset <= 0.5 {
        return None;
    }
    let content_h = viewport_h + max_offset;
    let thumb_h =
        (viewport_h * viewport_h / content_h).clamp(THUMB_MIN_H.min(viewport_h), viewport_h);
    let scrolled = (-offset_y).clamp(0.0, max_offset);
    let travel = (viewport_h - thumb_h).max(0.0);
    let top = (scrolled / max_offset) * travel;
    Some((top, thumb_h))
}

/// True when the reader is at the bottom. `chat_follow_stream` uses it to
/// decide whether new text follows (sticky bottom) or the view stays where the
/// reader left it.
pub fn at_bottom(offset_y: f32, max_offset: f32) -> bool {
    max_offset <= 0.5 || offset_y <= -max_offset + BOTTOM_EPSILON
}

/// Offset after dragging the thumb by `delta_y` pixels. Dragging down moves
/// the content down (the offset becomes more negative); the result is clamped
/// to the scroll range.
pub fn drag_offset(start_offset: f32, delta_y: f32, viewport_h: f32, max_offset: f32) -> f32 {
    if max_offset <= 0.0 || viewport_h <= 0.0 {
        return 0.0;
    }
    let thumb_h = thumb(viewport_h, max_offset, start_offset)
        .map(|(_, height)| height)
        .unwrap_or(viewport_h);
    let travel = (viewport_h - thumb_h).max(1.0);
    (start_offset - delta_y * max_offset / travel).clamp(-max_offset, 0.0)
}

/// Offset that centers the thumb under a click at `pointer_y` inside the
/// track. Clicks near the top/bottom jump to the start/end.
pub fn offset_for_pointer(pointer_y: f32, viewport_h: f32, max_offset: f32) -> f32 {
    if max_offset <= 0.0 || viewport_h <= 0.0 {
        return 0.0;
    }
    let thumb_h = thumb(viewport_h, max_offset, 0.0)
        .map(|(_, height)| height)
        .unwrap_or(viewport_h);
    let travel = (viewport_h - thumb_h).max(1.0);
    let ratio = ((pointer_y - thumb_h / 2.0) / travel).clamp(0.0, 1.0);
    -ratio * max_offset
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumb_is_absent_without_overflow() {
        assert_eq!(thumb(500.0, 0.0, 0.0), None);
        assert_eq!(thumb(0.0, 100.0, 0.0), None);
    }

    #[test]
    fn thumb_tracks_the_offset() {
        // 500 viewport + 1500 hidden = 2000 content; 500²/2000 = 125.
        assert_eq!(thumb(500.0, 1500.0, 0.0), Some((0.0, 125.0)));
        assert_eq!(thumb(500.0, 1500.0, -750.0), Some((187.5, 125.0)));
        assert_eq!(thumb(500.0, 1500.0, -1500.0), Some((375.0, 125.0)));
    }

    #[test]
    fn thumb_keeps_a_minimum_height() {
        let (_, height) = thumb(300.0, 100_000.0, 0.0).expect("overflow");
        assert_eq!(height, THUMB_MIN_H);
    }

    #[test]
    fn at_bottom_tolerates_a_couple_of_pixels() {
        assert!(at_bottom(0.0, 0.0), "no overflow is at the bottom");
        assert!(at_bottom(-100.0, 100.0));
        assert!(at_bottom(-98.0, 100.0), "within the epsilon");
        assert!(!at_bottom(-97.0, 100.0));
        assert!(!at_bottom(0.0, 100.0));
    }

    #[test]
    fn drag_offset_moves_and_clamps() {
        // travel = 500 - 125 = 375.
        assert_eq!(drag_offset(0.0, 375.0, 500.0, 1500.0), -1500.0);
        assert_eq!(drag_offset(-1500.0, -375.0, 500.0, 1500.0), 0.0);
        assert_eq!(drag_offset(-750.0, 1000.0, 500.0, 1500.0), -1500.0);
        assert_eq!(drag_offset(0.0, -1000.0, 500.0, 1500.0), 0.0);
    }

    #[test]
    fn pointer_maps_to_the_track() {
        // Thumb height 125; travel 375; the thumb center starts at 62.5.
        assert_eq!(offset_for_pointer(0.0, 500.0, 1500.0), 0.0);
        assert_eq!(offset_for_pointer(500.0, 500.0, 1500.0), -1500.0);
        assert_eq!(offset_for_pointer(250.0, 500.0, 1500.0), -750.0);
    }
}
