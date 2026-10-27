//! Runtime visual tokens and editor geometry (M10 S4b).
//!
//! The MVP started with hard-coded dark colors. Settings now need a real
//! theme switch and a font-size preference, so every view reads its colors
//! and metrics here instead of from constants. The app is single-window, so a
//! small process-wide palette keeps the existing rendering code unchanged in
//! shape while making Dark/Light/System real.
//!
//! `System` follows the macOS window appearance reported by GPUI; the
//! workspace sets it on every render, so changing the OS appearance is
//! reflected without restarting.

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU16, Ordering};

/// Persisted appearance preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    #[default]
    System,
    Dark,
    Light,
}

impl ThemeMode {
    pub const ALL: [ThemeMode; 3] = [ThemeMode::System, ThemeMode::Dark, ThemeMode::Light];

    pub fn key(self) -> &'static str {
        match self {
            ThemeMode::System => "system",
            ThemeMode::Dark => "dark",
            ThemeMode::Light => "light",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.key() == value)
    }

    pub fn i18n_key(self) -> &'static str {
        match self {
            ThemeMode::System => "settings.theme.system",
            ThemeMode::Dark => "settings.theme.dark",
            ThemeMode::Light => "settings.theme.light",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Palette {
    bg: u32,
    status_bg: u32,
    status_fg: u32,
    default_fg: u32,
    scene_fg: u32,
    cue_fg: u32,
    transition_fg: u32,
    note_fg: u32,
    section_fg: u32,
    cursor_fg: u32,
    selection_bg: u32,
    match_bg: u32,
    match_current_bg: u32,
    find_bg: u32,
    find_fg: u32,
    find_dim_fg: u32,
    explorer_bg: u32,
    explorer_border: u32,
    doc_selected_bg: u32,
    dirty_fg: u32,
    git_modified_fg: u32,
    git_added_fg: u32,
    git_deleted_fg: u32,
    git_renamed_fg: u32,
    git_conflict_fg: u32,
    gutter_fg: u32,
    active_line_bg: u32,
    agent_err_fg: u32,
    diff_add_bg: u32,
    diff_add_label: u32,
    diff_remove_bg: u32,
    diff_remove_label: u32,
    diff_context_label: u32,
    mention_character_bg: u32,
    mention_location_bg: u32,
    mention_object_bg: u32,
}

const DARK: Palette = Palette {
    bg: 0x1e1e1e,
    status_bg: 0x2d2d2d,
    status_fg: 0xa3a3a3,
    default_fg: 0xd4d4d4,
    scene_fg: 0x4ade80,
    cue_fg: 0x7dd3fc,
    transition_fg: 0xfbbf24,
    note_fg: 0xc4b5fd,
    section_fg: 0xf9a8d4,
    cursor_fg: 0xffffff,
    selection_bg: 0x3311ff30,
    match_bg: 0x3366cc40,
    match_current_bg: 0xeab30855,
    find_bg: 0x252526,
    find_fg: 0xd4d4d4,
    find_dim_fg: 0x808080,
    explorer_bg: 0x252526,
    explorer_border: 0x3c3c3c,
    doc_selected_bg: 0x37373d,
    dirty_fg: 0xfbbf24,
    git_modified_fg: 0xfbbf24,
    git_added_fg: 0x4ade80,
    git_deleted_fg: 0xf87171,
    git_renamed_fg: 0x7dd3fc,
    git_conflict_fg: 0xfb923c,
    gutter_fg: 0x6a6a6a,
    active_line_bg: 0x2a2a2e,
    agent_err_fg: 0xf48771,
    diff_add_bg: 0x16321f,
    diff_add_label: 0x7ee08a,
    diff_remove_bg: 0x3a1a1f,
    diff_remove_label: 0xe08a95,
    diff_context_label: 0x606060,
    mention_character_bg: 0x3b82f644,
    mention_location_bg: 0x22c55e44,
    mention_object_bg: 0xf59e0b44,
};

const LIGHT: Palette = Palette {
    bg: 0xfdfdfa,
    status_bg: 0xe9e9e3,
    status_fg: 0x5f5f59,
    default_fg: 0x242421,
    scene_fg: 0x15803d,
    cue_fg: 0x0369a1,
    transition_fg: 0xb45309,
    note_fg: 0x7c3aed,
    section_fg: 0xbe185d,
    cursor_fg: 0x111111,
    selection_bg: 0x99ccff66,
    match_bg: 0xffef9a99,
    match_current_bg: 0xffc94dbb,
    find_bg: 0xf1f1ea,
    find_fg: 0x1f1f1c,
    find_dim_fg: 0x6b6b64,
    explorer_bg: 0xf3f3ec,
    explorer_border: 0xd2d2c8,
    doc_selected_bg: 0xdbeafe,
    dirty_fg: 0xb45309,
    git_modified_fg: 0xb45309,
    git_added_fg: 0x15803d,
    git_deleted_fg: 0xdc2626,
    git_renamed_fg: 0x0369a1,
    git_conflict_fg: 0xc2410c,
    gutter_fg: 0x9a9a90,
    active_line_bg: 0xf0f0e8,
    agent_err_fg: 0xb91c1c,
    diff_add_bg: 0xe6f6ea,
    diff_add_label: 0x166534,
    diff_remove_bg: 0xfde8e8,
    diff_remove_label: 0x991b1b,
    diff_context_label: 0x8a8a80,
    mention_character_bg: 0x3b82f633,
    mention_location_bg: 0x22c55e33,
    mention_object_bg: 0xf59e0b44,
};

const SYSTEM: u8 = 0;
const DARK_MODE: u8 = 1;
const LIGHT_MODE: u8 = 2;

static MODE: AtomicU8 = AtomicU8::new(SYSTEM);
static SYSTEM_DARK: AtomicBool = AtomicBool::new(true);
static FONT_SIZE: AtomicU16 = AtomicU16::new(14);
static LINE_NUMBERS: AtomicBool = AtomicBool::new(true);

pub fn set_mode(mode: ThemeMode) {
    MODE.store(
        match mode {
            ThemeMode::System => SYSTEM,
            ThemeMode::Dark => DARK_MODE,
            ThemeMode::Light => LIGHT_MODE,
        },
        Ordering::Relaxed,
    );
}

/// The OS appearance, provided by the workspace on render.
pub fn set_system_dark(dark: bool) {
    SYSTEM_DARK.store(dark, Ordering::Relaxed);
}

pub fn is_dark() -> bool {
    match MODE.load(Ordering::Relaxed) {
        DARK_MODE => true,
        LIGHT_MODE => false,
        _ => SYSTEM_DARK.load(Ordering::Relaxed),
    }
}

/// Effective mode for the dump: `system` is resolved to the palette in use.
pub fn resolved_key() -> &'static str {
    if is_dark() { "dark" } else { "light" }
}

fn palette() -> &'static Palette {
    if is_dark() { &DARK } else { &LIGHT }
}

macro_rules! color_getter {
    ($name:ident, $field:ident) => {
        pub fn $name() -> u32 {
            palette().$field
        }
    };
}

color_getter!(bg, bg);
color_getter!(status_bg, status_bg);
color_getter!(status_fg, status_fg);
color_getter!(default_fg, default_fg);
color_getter!(scene_fg, scene_fg);
color_getter!(cue_fg, cue_fg);
color_getter!(transition_fg, transition_fg);
color_getter!(note_fg, note_fg);
color_getter!(section_fg, section_fg);
color_getter!(cursor_fg, cursor_fg);
color_getter!(selection_bg, selection_bg);
color_getter!(match_bg, match_bg);
color_getter!(match_current_bg, match_current_bg);
color_getter!(find_bg, find_bg);
color_getter!(find_fg, find_fg);
color_getter!(find_dim_fg, find_dim_fg);
color_getter!(explorer_bg, explorer_bg);
color_getter!(explorer_border, explorer_border);
color_getter!(doc_selected_bg, doc_selected_bg);
color_getter!(dirty_fg, dirty_fg);
color_getter!(git_modified_fg, git_modified_fg);
color_getter!(git_added_fg, git_added_fg);
color_getter!(git_deleted_fg, git_deleted_fg);
color_getter!(git_renamed_fg, git_renamed_fg);
color_getter!(git_conflict_fg, git_conflict_fg);
color_getter!(gutter_fg, gutter_fg);
color_getter!(active_line_bg, active_line_bg);
color_getter!(agent_err_fg, agent_err_fg);
color_getter!(diff_add_bg, diff_add_bg);
color_getter!(diff_add_label, diff_add_label);
color_getter!(diff_remove_bg, diff_remove_bg);
color_getter!(diff_remove_label, diff_remove_label);
color_getter!(diff_context_label, diff_context_label);
color_getter!(mention_character_bg, mention_character_bg);
color_getter!(mention_location_bg, mention_location_bg);
color_getter!(mention_object_bg, mention_object_bg);

/// Editor font size in points (12–24, clamped).
pub fn set_font_size(size: u16) {
    FONT_SIZE.store(size.clamp(12, 24), Ordering::Relaxed);
}

pub fn font_size() -> u16 {
    FONT_SIZE.load(Ordering::Relaxed)
}

pub fn text_size() -> f32 {
    f32::from(font_size())
}

/// Menlo needs ~1.286× its size for a comfortable line box; 14 pt → 18 px,
/// the baseline the original spike measured.
pub fn line_height() -> f32 {
    (text_size() * 9.0 / 7.0).round().max(14.0)
}

pub fn set_line_numbers(show: bool) {
    LINE_NUMBERS.store(show, Ordering::Relaxed);
}

pub fn line_numbers() -> bool {
    LINE_NUMBERS.load(Ordering::Relaxed)
}

/// Fixed gutter width when line numbers are visible; zero otherwise, so all
/// click/wrap math shifts with the preference instead of leaving a blank gap.
pub fn gutter_width() -> f32 {
    if line_numbers() { 44.0 } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn themes_have_distinct_surfaces_and_text() {
        set_system_dark(false);
        set_mode(ThemeMode::Dark);
        assert!(is_dark());
        assert_eq!(bg(), DARK.bg);
        set_mode(ThemeMode::Light);
        assert!(!is_dark());
        assert_eq!(bg(), LIGHT.bg);
        assert_ne!(DARK.default_fg, LIGHT.default_fg);
        set_mode(ThemeMode::System);
        set_system_dark(true);
        assert!(is_dark(), "system follows the OS appearance");
        set_system_dark(false);
        assert!(!is_dark());
    }

    #[test]
    fn font_size_is_clamped_and_line_height_tracks_it() {
        set_font_size(14);
        assert_eq!(text_size(), 14.0);
        assert_eq!(line_height(), 18.0);
        set_font_size(99);
        assert_eq!(font_size(), 24);
        set_font_size(1);
        assert_eq!(font_size(), 12);
        set_font_size(14);
    }

    #[test]
    fn line_numbers_zero_the_gutter() {
        set_line_numbers(true);
        assert_eq!(gutter_width(), 44.0);
        set_line_numbers(false);
        assert_eq!(gutter_width(), 0.0);
        set_line_numbers(true);
    }
}
