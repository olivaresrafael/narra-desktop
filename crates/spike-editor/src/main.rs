//! Spike A — slice 2: is GPUI a viable *editing* surface?
//!
//! Slice 1 answered rendering/navigation; this slice adds a real editing core:
//! a multi-line buffer (`buffer.rs`), keyboard editing (typing, movement,
//! selection, undo/redo, clipboard), IME plumbing through GPUI's input handler,
//! and the same instrumentation as slice 1 so typing latency can be measured
//! against the approved thresholds (PERFORMANCE_BASELINES.md §6).
//!
//! Still NOT answered (slice 3 / Spike B): IME/selection on wrapped lines,
//! full visual-row navigation chain (limited by test infrastructure focus loss),
//! accessibility, large-file strategy. IME composition and click-to-caret ARE
//! now verified end-to-end against the live macOS input context and window
//! server, including one-undo-per-session grouping and wrapped-line caret
//! placement with visual-row navigation.
//!
//! Modes:
//!   spike-editor --fixture <path>                 interactive
//!       cmd-g / cmd-shift-g  next/previous scene
//!       arrows, shift-arrows, home/end, cmd-a     movement and selection
//!       cmd-z / cmd-shift-z                       undo / redo
//!       cmd-c / cmd-x / cmd-v                     clipboard
//!       cmd-f                                     find bar (type to search)
//!       enter / cmd-g / cmd-shift-g / cmd-alt-f / cmd-enter / cmd-alt-enter
//!           next match / next+prev match / focus replacement /
//!           replace match / replace all
//!       escape                                     close find bar
//!       cmd-d                                     diff view (toggle)
//!       cmd-g / cmd-shift-g (diff open)           next/previous hunk
//!       cmd-q                                     quit (prints the JSON report)
//!   spike-editor --fixture <path> --steps 60      automated scroll probe (slice 1)
//!   spike-editor --fixture <path> --find-steps 60 automated find probe (slice 3)
//!   spike-editor --fixture <path> --diff-steps 30 automated diff probe (M1-3b)
//!   spike-editor --report-after-ms <ms>           print the report and exit
//!   spike-editor --dump-after-ms <ms>             print a buffer dump and exit
//!       (used by the scripted typing verification: text, enter, backspace,
//!        selection and undo are observable without trusting a screenshot)
//!   spike-editor --project <root>                 M6 slice 1: Narra project
//!       (mutually exclusive with --fixture) opens a real project through
//!       narra-core; the left panel lists its documents (hidden state like
//!       .git/.narra excluded), clicking one loads it into the buffer, ⌘S
//!       saves through the boundary-checked document IO, and switching
//!       documents with unsaved changes is refused explicitly — the status
//!       bar shows the document, a dirty marker and the last outcome.
//!
//! Find bar note (spike scope): one focus handle serves both the document
//! and the find fields — while the bar is open, text input, backspace and
//! enter are routed to the active find field instead of the buffer (the
//! documented single-focus compromise; per-field focus/IME is production
//! work, not a spike question). Arrow keys still move the document caret,
//! which doubles as a scroll preview while the bar is open.
//!
//! Output: JSON lines on stdout starting with `{"kind":"spike-a`.

mod buffer;

use std::cell::RefCell;
use std::env;
use std::fs;
use std::ops::Range;
use std::path::Path;
use std::process::ExitCode;
use std::rc::Rc;
use std::time::{Duration, Instant};

use buffer::{Buffer, Position};
use gpui::{
    App, Application, Bounds, ClipboardItem, Context, ElementInputHandler, EntityInputHandler,
    FocusHandle, Focusable, KeyBinding, ListAlignment, ListState, MouseButton, MouseDownEvent,
    Pixels, Point, Rgba, ShapedLine, TextRun, Timer, UTF16Selection, Window, WindowBounds,
    WindowOptions, actions, canvas, div, list, point, prelude::*, px, rgb, rgba, size,
};
use narra_core::{DocumentError, Project, ProjectError, open_project};
use unicode_segmentation::UnicodeSegmentation;

actions!(
    spike_editor,
    [
        // windows and navigation
        Quit,
        NextScene,
        PrevScene,
        // caret movement
        MoveLeft,
        MoveRight,
        MoveUp,
        MoveDown,
        MoveHome,
        MoveEnd,
        // selection
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectHome,
        SelectEnd,
        SelectAll,
        // editing
        Backspace,
        DeleteText,
        InsertNewline,
        Save,
        Undo,
        Redo,
        CopyText,
        CutText,
        PasteText,
        // find / replace (slice 3)
        ToggleFind,
        FocusReplace,
        CloseFind,
        ReplaceCurrent,
        ReplaceAll,
        // diff view (slice 3, M1-3b)
        ToggleDiff,
    ]
);

/// Baseline line height. Wrapping (slice 3) makes non-hot rows variable-height
/// (`list` measures each); this constant still sizes the caret, the hot-row
/// single-line fallback, and the list overdraw.
const LINE_HEIGHT: f32 = 18.0;

/// Explicit so that cursor x-positions (shaped manually) match rendered rows.
/// 14 px == `text_sm` (0.875 rem), which slice 1 used.
const TEXT_SIZE: f32 = 14.0;
const FONT_FAMILY: &str = "Menlo";
/// `px_2` on every row.
const ROW_PADDING_X: f32 = 8.0;

/// Height of the status strip above the buffer list; the only chrome between
/// the window's content top and the list (the find bar sits below it when
/// open). Mouse hit-testing subtracts nothing — item bounds are already in
/// window coordinates — but clicks on this strip must be ignored.
const STATUS_BAR_H: f32 = 26.0;

/// Gaps longer than this are treated as idle time (user reading) and excluded
/// from frame statistics, so interactive sessions do not pollute the samples.
const IDLE_GAP_MS: f64 = 500.0;

const BG: u32 = 0x1e1e1e;
const STATUS_BG: u32 = 0x2d2d2d;
const STATUS_FG: u32 = 0xa3a3a3;
const DEFAULT_FG: u32 = 0xd4d4d4;
const SCENE_FG: u32 = 0x4ade80;
const CUE_FG: u32 = 0x7dd3fc;
const TRANSITION_FG: u32 = 0xfbbf24;
const NOTE_FG: u32 = 0xc4b5fd;
const SECTION_FG: u32 = 0xf9a8d4;
const CURSOR_FG: u32 = 0xffffff;
const SELECTION_BG: u32 = 0x3311ff30;
/// Find-match highlight (all matches) and the current match — slice 3.
const MATCH_BG: u32 = 0x3366cc40;
const MATCH_CURRENT_BG: u32 = 0xeab30855;
/// Find bar background (distinct from the status bar) and text colors.
const FIND_BG: u32 = 0x252526;
const FIND_FG: u32 = 0xd4d4d4;
const FIND_DIM_FG: u32 = 0x808080;
/// Explorer panel (M6 slice 1): width and colors. The panel renders only in
/// `--project` mode, so fixture-mode geometry — the verified spike driver —
/// stays byte-for-byte unchanged.
const EXPLORER_W: f32 = 220.0;
const EXPLORER_BG: u32 = 0x252526;
const EXPLORER_BORDER: u32 = 0x3c3c3c;
const DOC_SELECTED_BG: u32 = 0x37373d;
/// Diff view colors (slice 3, M1-3b): added/removed row backgrounds and the
/// line-label gutter text.
const DIFF_ADD_BG: u32 = 0x16321f;
const DIFF_ADD_LABEL: u32 = 0x7ee08a;
const DIFF_REMOVE_BG: u32 = 0x3a1a1f;
const DIFF_REMOVE_LABEL: u32 = 0xe08a95;
const DIFF_CONTEXT_LABEL: u32 = 0x606060;
/// Which find field owns keystrokes while the bar is open (single-focus
/// spike compromise — see the module header).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum FindField {
    #[default]
    Query,
    Replacement,
}

/// Find/replace state (slice 3). `matches` are document byte ranges from
/// `Buffer::find_all`, recomputed on every query change (the measured
/// operation) and after edits that touch the document.
#[derive(Debug, Default)]
struct FindState {
    open: bool,
    field: FindField,
    query: String,
    replacement: String,
    matches: Vec<Range<usize>>,
    /// Index of the highlighted current match in `matches` (0 when empty).
    current: usize,
    /// Replace-all counter for the dump/report (verified evidence).
    replaced_total: usize,
}

/// One row of the unified diff view: the label and the line text.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DiffRow {
    /// Unified-diff label: " ", "-", "+".
    label: char,
    text: String,
}

/// Diff view state (slice 3, M1-3b). The base is a snapshot of the file as
/// loaded, so the diff stays meaningful while the buffer keeps changing —
/// the same model the M4 proposal surface needs (canonical file vs. edited
/// buffer). Rows are recomputed on open and on the explicit refresh.
#[derive(Debug, Default)]
struct DiffState {
    open: bool,
    /// Snapshot of the loaded file, one entry per line (no newline).
    base_lines: Vec<String>,
    rows: Vec<DiffRow>,
    /// Highlighted cursor row in `rows` (0 when empty).
    cursor: usize,
    /// Cumulative counters for the dump/report.
    added_total: usize,
    removed_total: usize,
}

fn is_scene_heading(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with("INT. ")
        || line.starts_with("EXT. ")
        || line.starts_with("I/E. ")
        || line.starts_with("EXT./INT. ")
}

fn is_character_cue(line: &str) -> bool {
    let line = line.trim_end();
    !line.is_empty()
        && line.len() <= 40
        && line.chars().any(|c| c.is_alphabetic())
        && line.chars().all(|c| !c.is_alphabetic() || c.is_uppercase())
        && !line.ends_with(':')
}

fn line_color(line: &str) -> Rgba {
    if is_scene_heading(line) {
        rgb(SCENE_FG)
    } else if line.starts_with('#') {
        rgb(SECTION_FG)
    } else if line.starts_with("[[") {
        rgb(NOTE_FG)
    } else if line.ends_with("TO:") || line.starts_with("FUNDIDO") {
        rgb(TRANSITION_FG)
    } else if is_character_cue(line) {
        rgb(CUE_FG)
    } else {
        rgb(DEFAULT_FG)
    }
}

/// The macOS input context can deliver `\r` (or `\r\n`) for a return key.
/// The buffer stores `\n` exclusively.
fn normalize_newlines(text: &str) -> String {
    if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text.to_string()
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// Shape one line with the row's font so caret/selection positions can be
/// computed in pixel space. Runs only for rows that carry caret/selection/IME
/// state, plus hit-testing (mouse/IME), not for every visible row.
fn shape_line(window: &Window, text: &str) -> ShapedLine {
    let run = TextRun {
        len: text.len(),
        font: gpui::font(FONT_FAMILY),
        color: rgb(DEFAULT_FG).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window
        .text_system()
        .shape_line(text.to_string().into(), px(TEXT_SIZE), &[run], None)
}

fn x_for_index(window: &Window, text: &str, col: usize) -> Pixels {
    shape_line(window, text).x_for_index(col)
}

struct Metrics {
    started: Instant,
    fixture: String,
    fixture_read_ms: f64,
    lines: usize,
    scenes: usize,
    first_render: Option<Instant>,
    last_render: Option<Instant>,
    pending_keystroke: Option<Instant>,
    /// A find query was just submitted; the next render closes the
    /// "query submitted → results rendered" sample (slice 3 baseline).
    pending_search: Option<Instant>,
    frame_gaps_ms: Vec<f64>,
    keystroke_to_frame_ms: Vec<f64>,
    /// Time spent inside `Buffer::find_all` per submission — the cost of
    /// the search itself, separate from the frame that renders it.
    search_compute_ms: Vec<f64>,
    /// Query submitted → next render pass (same method as
    /// keystroke_to_frame: input → start of render, no vsync term).
    search_to_frame_ms: Vec<f64>,
    /// Wall time of each replace-all over the whole document.
    replace_all_ms: Vec<f64>,
    /// A diff open/refresh was just requested; the next render closes the
    /// "diff requested → diff rendered" sample (slice 3, M1-3b baseline).
    pending_diff: Option<Instant>,
    /// Time spent computing the unified diff (base → current buffer).
    diff_compute_ms: Vec<f64>,
    /// Diff open/refresh requested → next render pass (same method as
    /// keystroke_to_frame: input → start of render, no vsync term).
    diff_render_ms: Vec<f64>,
    renders: usize,
    edits: usize,
    searches: usize,
    diff_opens: usize,
    probe_steps: usize,
    find_probe_steps: usize,
    diff_probe_steps: usize,
}

/// The three probe step counts. Grouped so `Metrics::new` stays under the
/// clippy argument limit instead of carrying an `#[allow]`.
struct ProbeSteps {
    scroll: usize,
    find: usize,
    diff: usize,
}

impl Metrics {
    fn new(
        started: Instant,
        fixture: &str,
        fixture_read_ms: f64,
        lines: usize,
        scenes: usize,
        probe: ProbeSteps,
    ) -> Self {
        Metrics {
            started,
            fixture: fixture.to_string(),
            fixture_read_ms,
            lines,
            scenes,
            first_render: None,
            last_render: None,
            pending_keystroke: None,
            pending_search: None,
            pending_diff: None,
            frame_gaps_ms: Vec::new(),
            keystroke_to_frame_ms: Vec::new(),
            search_compute_ms: Vec::new(),
            search_to_frame_ms: Vec::new(),
            replace_all_ms: Vec::new(),
            diff_compute_ms: Vec::new(),
            diff_render_ms: Vec::new(),
            renders: 0,
            edits: 0,
            searches: 0,
            diff_opens: 0,
            probe_steps: probe.scroll,
            find_probe_steps: probe.find,
            diff_probe_steps: probe.diff,
        }
    }

    fn on_render(&mut self, now: Instant) {
        self.renders += 1;
        if self.first_render.is_none() {
            self.first_render = Some(now);
        }
        if let Some(pending) = self.pending_keystroke.take() {
            self.keystroke_to_frame_ms.push(duration_ms(now - pending));
        }
        if let Some(pending) = self.pending_search.take() {
            self.search_to_frame_ms.push(duration_ms(now - pending));
        }
        if let Some(pending) = self.pending_diff.take() {
            self.diff_render_ms.push(duration_ms(now - pending));
        }
        if let Some(last) = self.last_render {
            let gap = duration_ms(now - last);
            if gap < IDLE_GAP_MS {
                self.frame_gaps_ms.push(gap);
            }
        }
        self.last_render = Some(now);
    }

    fn mark_keystroke(&mut self, now: Instant) {
        if self.pending_keystroke.is_none() {
            self.pending_keystroke = Some(now);
        }
    }

    /// A query was submitted; the next render closes the sample. The latest
    /// submission wins if several arrive before a frame (interactive typing
    /// bursts); the automated probe spaces steps by frames, so every step
    /// yields exactly one sample.
    fn mark_search(&mut self, now: Instant) {
        self.pending_search = Some(now);
        self.searches += 1;
    }

    /// A diff open/refresh was just requested; the next render closes the
    /// "requested → rendered" sample (same contract as `mark_search`).
    fn mark_diff(&mut self, now: Instant) {
        self.pending_diff = Some(now);
        self.diff_opens += 1;
    }

    fn cold_start_ms(&self) -> f64 {
        self.first_render
            .map(|first| duration_ms(first - self.started))
            .unwrap_or(0.0)
    }

    fn stats(samples: &[f64]) -> (usize, f64, f64, f64) {
        if samples.is_empty() {
            return (0, 0.0, 0.0, 0.0);
        }
        let mut sorted = samples.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        (
            sorted.len(),
            percentile(&sorted, 0.5),
            percentile(&sorted, 0.95),
            sorted[sorted.len() - 1],
        )
    }

    /// Steady state: the first frames pay one-time costs (font loading, first
    /// layout, GPU warm-up). Spike reports quote both views.
    fn steady(&self) -> &[f64] {
        const WARMUP_SAMPLES: usize = 10;
        if self.frame_gaps_ms.len() > WARMUP_SAMPLES {
            &self.frame_gaps_ms[WARMUP_SAMPLES..]
        } else {
            &[]
        }
    }

    /// Resident memory in MB, sampled through `ps` (spike-only convenience,
    /// no dependency added for a metric we could add properly later).
    fn rss_mb() -> f64 {
        let pid = std::process::id().to_string();
        std::process::Command::new("ps")
            .args(["-p", &pid, "-o", "rss="])
            .output()
            .ok()
            .and_then(|out| {
                String::from_utf8_lossy(&out.stdout)
                    .trim()
                    .parse::<f64>()
                    .ok()
            })
            .map(|kb| kb / 1024.0)
            .unwrap_or(0.0)
    }

    fn report(&self) -> String {
        let (frame_count, frame_p50, frame_p95, frame_max) = Self::stats(&self.frame_gaps_ms);
        let (steady_count, steady_p50, steady_p95, steady_max) = Self::stats(self.steady());
        let (key_count, key_p50, key_p95, key_max) = Self::stats(&self.keystroke_to_frame_ms);
        let (search_count, search_p50, search_p95, search_max) =
            Self::stats(&self.search_to_frame_ms);
        let (compute_count, compute_p50, compute_p95, compute_max) =
            Self::stats(&self.search_compute_ms);
        let (repl_count, repl_p50, repl_p95, repl_max) = Self::stats(&self.replace_all_ms);
        let (dcompute_count, dcompute_p50, dcompute_p95, dcompute_max) =
            Self::stats(&self.diff_compute_ms);
        let (drender_count, drender_p50, drender_p95, drender_max) =
            Self::stats(&self.diff_render_ms);
        format!(
            concat!(
                "{{\"kind\":\"spike-a-slice2\",",
                "\"fixture\":\"{}\",\"lines\":{},\"scenes\":{},\"probe_steps\":{},",
                "\"find_probe_steps\":{},\"searches\":{},",
                "\"diff_probe_steps\":{},\"diff_opens\":{},",
                "\"fixture_read_ms\":{:.3},\"cold_start_ms\":{:.3},\"rss_mb\":{:.1},",
                "\"renders\":{},\"edits\":{},",
                "\"frame_ms\":{{\"count\":{},\"p50\":{:.3},\"p95\":{:.3},\"max\":{:.3}}},",
                "\"steady_frame_ms\":{{\"count\":{},\"p50\":{:.3},\"p95\":{:.3},\"max\":{:.3}}},",
                "\"keystroke_to_frame_ms\":{{\"count\":{},\"p50\":{:.3},\"p95\":{:.3},\"max\":{:.3}}},",
                "\"search_to_frame_ms\":{{\"count\":{},\"p50\":{:.3},\"p95\":{:.3},\"max\":{:.3}}},",
                "\"search_compute_ms\":{{\"count\":{},\"p50\":{:.3},\"p95\":{:.3},\"max\":{:.3}}},",
                "\"replace_all_ms\":{{\"count\":{},\"p50\":{:.3},\"p95\":{:.3},\"max\":{:.3}}},",
                "\"diff_compute_ms\":{{\"count\":{},\"p50\":{:.3},\"p95\":{:.3},\"max\":{:.3}}},",
                "\"diff_render_ms\":{{\"count\":{},\"p50\":{:.3},\"p95\":{:.3},\"max\":{:.3}}}",
                "}}"
            ),
            self.fixture,
            self.lines,
            self.scenes,
            self.probe_steps,
            self.find_probe_steps,
            self.searches,
            self.diff_probe_steps,
            self.diff_opens,
            self.fixture_read_ms,
            self.cold_start_ms(),
            Self::rss_mb(),
            self.renders,
            self.edits,
            frame_count,
            frame_p50,
            frame_p95,
            frame_max,
            steady_count,
            steady_p50,
            steady_p95,
            steady_max,
            key_count,
            key_p50,
            key_p95,
            key_max,
            search_count,
            search_p50,
            search_p95,
            search_max,
            compute_count,
            compute_p50,
            compute_p95,
            compute_max,
            repl_count,
            repl_p50,
            repl_p95,
            repl_max,
            dcompute_count,
            dcompute_p50,
            dcompute_p95,
            dcompute_max,
            drender_count,
            drender_p50,
            drender_p95,
            drender_max,
        )
    }
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// One reversible edit. `range` is where `new_text` currently sits in the
/// document; undo replaces it back with `old_text`. `composition` marks the
/// entry that was opened by an IME composition session, so later marked
/// updates merge into it instead of stacking one undo step per keystroke.
#[derive(Clone)]
struct Edit {
    range: Range<usize>,
    new_text: String,
    old_text: String,
    composition: bool,
}

struct Editor {
    buffer: Buffer,
    /// Selection as a sorted doc-offset range; empty == caret only.
    selected: Range<usize>,
    /// When true the caret sits at `selected.start` instead of `selected.end`.
    reversed: bool,
    /// IME composition range (doc offsets).
    marked: Option<Range<usize>>,
    /// True from the first `setMarkedText` of a composition session until
    /// its commit (`insertText`) or cancellation (`unmarkText`): edits made
    /// inside this window merge into one undo entry (Spike B undo grouping).
    composing: bool,
    /// Debug for mouse verification (Spike B click-to-caret): last left-click
    /// position in window coordinates, the resolved line index and its bounds,
    /// and the doc offset (or None if the click was ignored / above the list).
    /// (click_x, click_y, line, item_top, item_h, offset)
    #[allow(clippy::type_complexity)]
    last_click: Option<(f32, f32, Option<usize>, Option<f32>, f32, Option<usize>)>,
    undo_stack: Vec<Edit>,
    redo_stack: Vec<Edit>,

    scene_lines: Vec<usize>,
    scenes_dirty: bool,
    current_scene: usize,

    list_state: ListState,
    /// Scroll state for the diff view (its own list, sized to the rows).
    diff_list_state: ListState,
    focus_handle: FocusHandle,
    /// Wrap width for non-hot rows, refreshed from the viewport each frame and
    /// read by the `list` render callback (which reaches the editor via the
    /// entity, not through the render-time `&mut self`).
    wrap_width: Pixels,

    metrics: Rc<RefCell<Metrics>>,
    probe_left: usize,
    probe_cursor: usize,
    probe_scheduled: bool,

    /// Automated find probe (`--find-steps`): same functions as the
    /// interactive find bar, driven by the probe timer.
    find_probe_left: usize,
    find_probe_cursor: usize,

    /// Automated diff probe (`--diff-steps`): toggles the diff view through
    /// the same functions as cmd-d, so the measured path is the shipped one.
    diff_probe_left: usize,
    /// Total diff steps requested (detects the seeding first step).
    diff_probe_total: usize,

    /// Find/replace bar (slice 3). While `open`, keystrokes route here
    /// instead of the buffer (single-focus spike compromise).
    find: FindState,

    /// Unified diff view (slice 3, M1-3b). While `open`, it renders instead
    /// of the buffer rows and the probe/diff navigation takes over.
    diff: DiffState,

    /// Open Narra project (M6 slice 1). `None` in `--fixture` spike mode,
    /// which keeps the verified spike surface unchanged.
    project: Option<Project>,
    /// Documents offered by the explorer panel (project-relative paths).
    explorer: Vec<String>,
    /// Document this buffer represents: project-relative path in project
    /// mode, fixture path in spike mode. The status bar shows it.
    doc_path: String,
    /// File text as last loaded or saved. The dirty flag compares the edit
    /// generation instead of scanning the document, because the status bar
    /// reads it every frame (O(1), no per-frame allocation).
    base_text: String,
    /// Value of `metrics.edits` at the last load/save: `edits != saved_edits`
    /// means the buffer diverged (every mutation path bumps `metrics.edits`).
    saved_edits: usize,
    /// Last user-visible outcome (save/open/blocked switch). Display only.
    notice: Option<String>,
}

impl Focusable for Editor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Editor {
    fn caret(&self) -> usize {
        if self.reversed {
            self.selected.start
        } else {
            self.selected.end
        }
    }

    fn set_caret(&mut self, target: usize, extend: bool) {
        if extend {
            if self.reversed {
                self.selected.start = target;
                if self.selected.start > self.selected.end {
                    let (start, end) = (self.selected.end, self.selected.start);
                    self.selected = start..end;
                    self.reversed = false;
                }
            } else {
                self.selected.end = target;
                if self.selected.end < self.selected.start {
                    let (start, end) = (self.selected.end, self.selected.start);
                    self.selected = start..end;
                    self.reversed = true;
                }
            }
        } else {
            self.selected = target..target;
            self.reversed = false;
        }
    }

    /// Doc offset under a window-coordinate point (mouse hit-test), or `None`
    /// when the point is above the list (status/find bar chrome). Item bounds
    /// come from `ListState::bounds_for_item` — window coordinates with scroll
    /// already applied, `Some` only for laid-out items — so the scan walks from
    /// the top and keeps the nearest measured row at or above the point.
    /// Measured rows are contiguous around the viewport, so the walk is bounded
    /// by the scroll position; it runs once per click, never per frame.
    fn offset_for_point(&self, position: Point<Pixels>, window: &Window) -> Option<usize> {
        let mut best: Option<(usize, Bounds<Pixels>)> = None;
        for ix in 0..self.buffer.line_count() {
            let Some(bounds) = self.list_state.bounds_for_item(ix) else {
                continue;
            };
            if bounds.top() > position.y {
                break;
            }
            best = Some((ix, bounds));
            if position.y < bounds.bottom() {
                break;
            }
        }
        let (line, bounds) = best?;
        // Below the last measured row: clamp to that row's content end (the
        // last line's end is the document end).
        if position.y >= bounds.bottom() {
            return Some(self.buffer.line_content_range(line).end);
        }
        let text = &self.buffer.lines()[line];
        let start = self.buffer.line_content_range(line).start;
        if text.is_empty() {
            return Some(start);
        }
        let shaped = shape_line(window, text);
        let glyphs: Vec<(usize, f32)> = shaped
            .runs
            .iter()
            .flat_map(|run| {
                run.glyphs
                    .iter()
                    .map(|g| (g.index, f32::from(g.position.x)))
            })
            .collect();
        let wrap_width = f32::from((bounds.size.width - px(ROW_PADDING_X * 2.0)).max(px(0.0)));
        let rows = wrap_row_starts(&glyphs, wrap_width);
        let visual_row =
            (((position.y - bounds.top()) / px(LINE_HEIGHT)).floor() as usize).min(rows.len() - 1);
        // Glyph x positions are absolute on the un-wrapped line, but wrapping
        // restarts each visual row at x = 0: shift the probe by the x of the
        // row's first glyph.
        let row_start_x = glyphs
            .iter()
            .find(|(index, _)| *index >= rows[visual_row])
            .map(|(_, x)| *x)
            .unwrap_or(0.0);
        let x = f32::from(position.x - bounds.left() - px(ROW_PADDING_X)) + row_start_x;
        let col = shaped.closest_index_for_x(px(x)).min(text.len());
        // Snap down to a grapheme boundary: a combining mark's glyph index can
        // land inside one.
        let offset = start + col;
        let snapped = self.buffer.prev_boundary(offset);
        let offset = if self.buffer.next_boundary(snapped) == offset {
            offset
        } else {
            snapped
        };
        Some(offset.min(self.buffer.doc_len()))
    }

    /// Click-to-caret (Spike B): a plain click moves the caret, shift-click
    /// extends the selection from the active end (`set_caret` handles the
    /// crossing flip). The diff view is read-only, so clicks there do nothing.
    fn on_buffer_click(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Record debug for every left click (even on diff), so the dump shows
        // what coordinates the handler received and what it resolved.
        if event.button == MouseButton::Left {
            self.last_click = Some((
                f32::from(event.position.x),
                f32::from(event.position.y),
                None,
                None,
                0.0,
                None,
            ));
        }
        if self.diff.open || event.button != MouseButton::Left {
            return;
        }
        let Some(offset) = self.offset_for_point(event.position, window) else {
            return;
        };
        // Update debug with resolved line and bounds.
        let pos = self.buffer.position_at(offset);
        let line = pos.line;
        let bounds = self.list_state.bounds_for_item(line);
        let (item_top, item_h) = match bounds {
            Some(b) => (Some(f32::from(b.top())), f32::from(b.size.height)),
            None => (None, 0.0),
        };
        self.last_click = Some((
            f32::from(event.position.x),
            f32::from(event.position.y),
            Some(line),
            item_top,
            item_h,
            Some(offset),
        ));
        self.set_caret(offset, event.modifiers.shift);
        // A click re-targets typing (and IME composition) at the buffer.
        window.focus(&self.focus_handle(cx));
        cx.notify();
    }

    /// Keep the `list` item count in sync with the buffer after an edit that
    /// replaced `deleted` (the removed text) with `inserted` starting at
    /// `start_offset`. Uses `splice`, not `reset`, so the scroll position is
    /// preserved across keystrokes (reset would jump to the top).
    fn splice_for_edit(&self, start_offset: usize, deleted: &str, inserted: &str) {
        let start_line = self.buffer.position_at(start_offset).line;
        let old_span = deleted.matches('\n').count() + 1;
        let new_span = inserted.matches('\n').count() + 1;
        self.list_state
            .splice(start_line..start_line + old_span, new_span);
    }

    fn clamp(&self, range: Range<usize>) -> Range<usize> {
        let len = self.buffer.doc_len();
        let start = range.start.min(len);
        let end = range.end.min(len).max(start);
        start..end
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.buffer.utf16_to_offset(range.start)..self.buffer.utf16_to_offset(range.end)
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.buffer.offset_to_utf16(range.start)..self.buffer.offset_to_utf16(range.end)
    }

    /// Apply an edit, record undo state, move the caret after the inserted
    /// text and invalidate the derived scene index.
    fn apply_edit(&mut self, range: Range<usize>, text: &str) -> Range<usize> {
        let start = range.start;
        let (inserted, deleted) = self.buffer.replace(range.clone(), text);
        self.splice_for_edit(start, &deleted, text);
        self.metrics.borrow_mut().edits += 1;
        // Undo bookkeeping goes through `record_edit` so IME composition
        // updates collapse into the single open entry (one undo per session).
        record_edit(
            &mut self.undo_stack,
            self.composing,
            range,
            inserted.clone(),
            text,
            deleted,
        );
        const MAX_UNDO: usize = 5000;
        if self.undo_stack.len() > MAX_UNDO {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
        self.selected = inserted.end..inserted.end;
        self.reversed = false;
        self.marked = None;
        self.scenes_dirty = true;
        // Offsets shifted: keep find matches valid (silent — edits are not
        // query submissions and must not pollute the search baseline).
        if self.find.open {
            self.refresh_matches(false);
        }
        inserted
    }

    fn after_history_edit(&mut self, caret: usize) {
        self.metrics.borrow_mut().edits += 1;
        self.selected = caret..caret;
        self.reversed = false;
        self.marked = None;
        self.composing = false;
        self.scenes_dirty = true;
        if self.find.open {
            self.refresh_matches(false);
        }
    }

    fn undo(&mut self) {
        let Some(edit) = self.undo_stack.pop() else {
            return;
        };
        let inserted = edit.range.start..edit.range.start + edit.new_text.len();
        let start = inserted.start;
        let (restored, deleted) = self.buffer.replace(inserted, &edit.old_text);
        self.splice_for_edit(start, &deleted, &edit.old_text);
        self.redo_stack.push(Edit {
            range: restored.clone(),
            new_text: edit.old_text.clone(),
            old_text: deleted,
            composition: false,
        });
        self.after_history_edit(restored.end);
    }

    fn redo(&mut self) {
        let Some(edit) = self.redo_stack.pop() else {
            return;
        };
        let current = edit.range.start..edit.range.start + edit.new_text.len();
        let start = current.start;
        let (restored, deleted) = self.buffer.replace(current, &edit.old_text);
        self.splice_for_edit(start, &deleted, &edit.old_text);
        self.undo_stack.push(Edit {
            range: restored.clone(),
            new_text: edit.old_text.clone(),
            old_text: deleted,
            composition: false,
        });
        self.after_history_edit(restored.end);
    }

    fn refresh_scenes(&mut self) {
        if !self.scenes_dirty {
            return;
        }
        self.scene_lines = self
            .buffer
            .lines()
            .iter()
            .enumerate()
            .filter(|(_, line)| is_scene_heading(line))
            .map(|(ix, _)| ix)
            .collect();
        self.scenes_dirty = false;
    }

    fn jump(&mut self, direction: i64) {
        self.refresh_scenes();
        if self.scene_lines.is_empty() {
            return;
        }
        let len = self.scene_lines.len() as i64;
        let next = (self.current_scene as i64 + direction).rem_euclid(len);
        self.current_scene = next as usize;
        let line = self.scene_lines[self.current_scene];
        self.list_state.scroll_to_reveal_item(line);
        let offset = self.buffer.offset_at(Position { line, col: 0 });
        self.selected = offset..offset;
        self.reversed = false;
    }

    /// One probe step: jump to the next scene and request a frame.
    ///
    /// This runs from a timer callback, NOT from inside `render()`: calling
    /// `cx.notify()` during render only produced 2-3 frames and then stalled
    /// (observed in both debug and release, 2026-10-21).
    fn advance_probe(&mut self, cx: &mut Context<Self>) {
        self.refresh_scenes();
        if self.scene_lines.is_empty() {
            return;
        }
        let target_line = self.scene_lines[self.probe_cursor % self.scene_lines.len()];
        self.list_state.scroll_to_reveal_item(target_line);
        self.probe_cursor += 1;
        self.probe_left = self.probe_left.saturating_sub(1);

        // Quit only when no probe work remains (scroll, find and diff probes
        // may be combined in one run).
        let finished =
            self.probe_left == 0 && self.find_probe_left == 0 && self.diff_probe_left == 0;
        cx.notify();
        if finished {
            let report = self.metrics.borrow().report();
            println!("{report}");
            cx.defer(|cx| cx.quit());
        }
    }

    /// The diff probe exercises the shipped cmd-d path. The first step makes
    /// one large real edit (replace-all over the whole document) so there is
    /// a substantial change to diff; every step after that opens the diff
    /// (recompute + render) and then closes it, yielding one
    /// "requested → rendered" sample per open.
    fn advance_diff_probe(&mut self, cx: &mut Context<Self>) {
        if self.diff_probe_left == self.diff_probe_total {
            // Seed a large, real change once — the acceptance metric is
            // "open a diff of a large change", so the diff under measurement
            // must actually be big. Reuse the shipped find + replace-all
            // machinery (query a common word, swap every match) rather than a
            // bespoke edit, so the seeded change is produced by the same code
            // a user drives. `INT.` matches ~778 scene headings.
            self.find.query = "INT.".to_string();
            self.find.replacement = "CENA.".to_string();
            self.refresh_matches(false);
            self.replace_all(cx);
        }

        if self.diff.open {
            self.close_diff(cx);
        } else {
            self.open_diff(cx);
        }

        self.diff_probe_left = self.diff_probe_left.saturating_sub(1);
        let finished =
            self.probe_left == 0 && self.find_probe_left == 0 && self.diff_probe_left == 0;
        cx.notify();
        if finished {
            let report = self.metrics.borrow().report();
            println!("{report}");
            cx.defer(|cx| cx.quit());
        }
    }

    fn schedule_probe_step(&mut self, window: &Window, cx: &Context<Self>) {
        let has_probe = self.probe_left > 0 || self.find_probe_left > 0 || self.diff_probe_left > 0;
        if !has_probe || self.probe_scheduled {
            return;
        }
        self.probe_scheduled = true;
        let entity = cx.entity();
        // 1 ms is the measurement floor: observed frame gap = 1 ms + real
        // render/present time. Documented alongside the baseline.
        window
            .spawn(cx, async move |cx| {
                Timer::after(Duration::from_millis(1)).await;
                cx.update(|_, cx| {
                    entity.update(cx, |view, cx| {
                        view.probe_scheduled = false;
                        if view.diff_probe_left > 0 {
                            view.advance_diff_probe(cx);
                        } else if view.find_probe_left > 0 {
                            view.advance_find_probe(cx);
                        } else {
                            view.advance_probe(cx);
                        }
                    });
                })
                .ok();
            })
            .detach();
    }

    fn ensure_caret_visible(&mut self) {
        let line = self.buffer.position_at(self.caret()).line;
        self.list_state.scroll_to_reveal_item(line);
    }

    // ---- caret movement -------------------------------------------------

    fn move_horiz(&mut self, direction: i64, extend: bool) {
        if !extend && !self.selected.is_empty() {
            let target = if direction < 0 {
                self.selected.start
            } else {
                self.selected.end
            };
            self.set_caret(target, false);
            return;
        }
        let caret = self.caret();
        let target = if direction < 0 {
            self.buffer.prev_boundary(caret)
        } else {
            self.buffer.next_boundary(caret)
        };
        self.set_caret(target, extend);
    }

    /// Returns (visual_row, total_visual_rows, caret_x) for the caret position.
    /// visual_row is 0-based index of the visual row containing the caret.
    /// total_visual_rows is the number of visual rows the line wraps to.
    /// caret_x is the x-offset within the visual row where the caret sits.
    fn caret_visual_info(&self, window: &Window, caret: usize) -> (usize, usize, f32) {
        let pos = self.buffer.position_at(caret);
        let line = pos.line;
        let text = &self.buffer.lines()[line];
        let caret_col = pos.col;
        let wrap_w = f32::from(self.wrap_width);
        let (visual_row, caret_x) = caret_visual_row_and_x(window, text, caret_col, wrap_w);
        let starts = visual_row_starts(window, text, wrap_w);
        let total_visual_rows = starts.len();
        (visual_row, total_visual_rows, caret_x)
    }

    /// Returns the document offset for a given visual row and x-offset within a line.
    fn offset_in_visual_row(
        &self,
        caret: usize,
        target_visual_row: usize,
        caret_x: f32,
        window: &Window,
    ) -> usize {
        let pos = self.buffer.position_at(caret);
        let line = pos.line;
        let text = &self.buffer.lines()[line];
        let wrap_w = f32::from(self.wrap_width);
        let starts = visual_row_starts(window, text, wrap_w);
        if target_visual_row >= starts.len() {
            return self.buffer.line_content_range(line).end;
        }
        let row_start = starts[target_visual_row];
        let row_end = starts
            .get(target_visual_row + 1)
            .copied()
            .unwrap_or(text.len());
        let row_text = &text[row_start..row_end];
        // Find the byte offset within the visual row that corresponds to caret_x
        let shaped = shape_line(window, row_text);
        let glyphs: Vec<(usize, f32)> = shaped
            .runs
            .iter()
            .flat_map(|run| {
                run.glyphs
                    .iter()
                    .map(|g| (g.index, f32::from(g.position.x)))
            })
            .collect();
        // caret_x is relative to the visual row start; we need absolute x
        let row_start_x = glyphs.first().map(|(_, x)| *x).unwrap_or(0.0);
        let absolute_x = caret_x + row_start_x;
        let col = glyphs
            .iter()
            .rev()
            .find(|(_, gx)| *gx <= absolute_x)
            .map(|(ix, _)| *ix)
            .unwrap_or(row_text.len())
            .min(row_text.len());
        (self.buffer.line_content_range(line).start + starts[target_visual_row] + col)
            .min(self.buffer.doc_len())
    }

    fn move_vert(&mut self, delta: i64, extend: bool, window: &Window) {
        let caret = self.caret();
        let (visual_row, total_visual_rows, caret_x) = self.caret_visual_info(window, caret);

        // If on a wrapped line (multiple visual rows), navigate visual rows first
        if total_visual_rows > 1 {
            if delta < 0 {
                // Up
                if visual_row > 0 {
                    // Move to previous visual row on same line
                    let target = self.offset_in_visual_row(caret, visual_row - 1, caret_x, window);
                    self.set_caret(target, extend);
                    return;
                }
                // At visual row 0: fall through to previous logical line
            } else if delta > 0 {
                // Down
                if visual_row < total_visual_rows - 1 {
                    // Move to next visual row on same line
                    let target = self.offset_in_visual_row(caret, visual_row + 1, caret_x, window);
                    self.set_caret(target, extend);
                    return;
                }
                // At last visual row: fall through to next logical line
            }
        }
        // Not on a wrapped line, or at boundary of wrapped line: use logical line navigation
        let target = self.buffer.vertical(caret, delta);
        self.set_caret(target, extend);
    }

    fn move_line_edge(&mut self, to_start: bool, extend: bool) {
        let caret = self.caret();
        let target = if to_start {
            self.buffer.line_start(caret)
        } else {
            self.buffer.line_end(caret)
        };
        self.set_caret(target, extend);
    }

    fn select_all(&mut self) {
        self.selected = 0..self.buffer.doc_len();
        self.reversed = false;
    }

    // ---- action handlers -------------------------------------------------

    fn next_scene(&mut self, _: &NextScene, _window: &mut Window, cx: &mut Context<Self>) {
        // With the find bar open, cmd-g/shift-g navigate matches (same
        // shortcuts the scene jump uses when it is closed); with the diff
        // view open they walk its hunks.
        if self.diff.open {
            self.diff_move_cursor(1, cx);
            return;
        }
        if self.find.open {
            self.next_match(cx);
            return;
        }
        self.jump(1);
        cx.notify();
    }

    fn prev_scene(&mut self, _: &PrevScene, _window: &mut Window, cx: &mut Context<Self>) {
        if self.diff.open {
            self.diff_move_cursor(-1, cx);
            return;
        }
        if self.find.open {
            self.prev_match(cx);
            return;
        }
        self.jump(-1);
        cx.notify();
    }

    fn on_left(&mut self, _: &MoveLeft, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_horiz(-1, false);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_right(&mut self, _: &MoveRight, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_horiz(1, false);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_up(&mut self, _: &MoveUp, window: &mut Window, cx: &mut Context<Self>) {
        self.move_vert(-1, false, window);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_down(&mut self, _: &MoveDown, window: &mut Window, cx: &mut Context<Self>) {
        self.move_vert(1, false, window);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_home(&mut self, _: &MoveHome, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_line_edge(true, false);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_end(&mut self, _: &MoveEnd, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_line_edge(false, false);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_select_left(&mut self, _: &SelectLeft, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_horiz(-1, true);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_select_right(&mut self, _: &SelectRight, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_horiz(1, true);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_select_up(&mut self, _: &SelectUp, window: &mut Window, cx: &mut Context<Self>) {
        self.move_vert(-1, true, window);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_select_down(&mut self, _: &SelectDown, window: &mut Window, cx: &mut Context<Self>) {
        self.move_vert(1, true, window);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_select_home(&mut self, _: &SelectHome, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_line_edge(true, true);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_select_end(&mut self, _: &SelectEnd, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_line_edge(false, true);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_select_all(&mut self, _: &SelectAll, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_all();
        cx.notify();
    }

    fn on_backspace(&mut self, _: &Backspace, _window: &mut Window, cx: &mut Context<Self>) {
        if self.find.open {
            self.find_backspace();
            cx.notify();
            return;
        }
        if self.selected.is_empty() {
            let caret = self.caret();
            let previous = self.buffer.prev_boundary(caret);
            if previous < caret {
                self.apply_edit(previous..caret, "");
            }
        } else {
            let range = self.selected.clone();
            self.apply_edit(range, "");
        }
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_delete(&mut self, _: &DeleteText, _window: &mut Window, cx: &mut Context<Self>) {
        if self.selected.is_empty() {
            let caret = self.caret();
            let next = self.buffer.next_boundary(caret);
            if next > caret {
                self.apply_edit(caret..next, "");
            }
        } else {
            let range = self.selected.clone();
            self.apply_edit(range, "");
        }
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_insert_newline(
        &mut self,
        _: &InsertNewline,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // While the find bar owns keystrokes, enter means "next match" in the
        // query field and "replace current + advance" in the replacement
        // field (the routing note in the module header).
        if self.find.open {
            match self.find.field {
                FindField::Query => self.next_match(cx),
                FindField::Replacement => {
                    self.replace_current(cx);
                    self.next_match(cx);
                }
            }
            return;
        }
        let caret = self.caret();
        self.apply_edit(caret..caret, "\n");
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_undo(&mut self, _: &Undo, _window: &mut Window, cx: &mut Context<Self>) {
        self.undo();
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_redo(&mut self, _: &Redo, _window: &mut Window, cx: &mut Context<Self>) {
        self.redo();
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_copy(&mut self, _: &CopyText, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected.is_empty() {
            let text = self.buffer.slice(self.selected.clone());
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn on_cut(&mut self, _: &CutText, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected.is_empty() {
            let text = self.buffer.slice(self.selected.clone());
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            let range = self.selected.clone();
            self.apply_edit(range, "");
            cx.notify();
        }
    }

    fn on_paste(&mut self, _: &PasteText, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    // ---- find / replace (Spike A slice 3) -------------------------------

    /// Open the find bar (cmd-f) or focus its query field if it is already
    /// open; cmd-alt-f opens it with the replacement field focused. A
    /// non-single-line selection prefills the query on first open.
    fn open_find(&mut self, with_replacement: bool, cx: &mut Context<Self>) {
        let selection = if self.selected.is_empty() {
            String::new()
        } else {
            self.buffer.slice(self.selected.clone())
        };
        let first_open = !self.find.open;
        self.find.open = true;
        self.find.field = if with_replacement {
            FindField::Replacement
        } else {
            FindField::Query
        };
        if first_open && !selection.is_empty() && !selection.contains('\n') {
            self.find.query = selection;
        }
        self.refresh_matches(true);
        cx.notify();
    }

    fn close_find(&mut self, cx: &mut Context<Self>) {
        self.find.open = false;
        self.find.matches.clear();
        self.find.current = 0;
        cx.notify();
    }

    /// Recompute `find.matches` from `find.query`. `measured` records the
    /// baseline sample (compute time + query→next frame); silent refreshes
    /// after buffer edits keep the baseline free of maintenance noise.
    /// An empty query clears matches without recording anything.
    fn refresh_matches(&mut self, measured: bool) {
        if self.find.query.is_empty() {
            self.find.matches.clear();
            self.find.current = 0;
            return;
        }
        let start = Instant::now();
        self.find.matches = self.buffer.find_all(&self.find.query);
        if measured {
            let mut metrics = self.metrics.borrow_mut();
            metrics.search_compute_ms.push(duration_ms(start.elapsed()));
            metrics.mark_search(Instant::now());
        }
        if self.find.current >= self.find.matches.len() {
            self.find.current = 0;
        }
    }

    /// Reveal and select the match at `index` (wraps).
    fn goto_match(&mut self, index: usize) {
        if self.find.matches.is_empty() {
            return;
        }
        self.find.current = index % self.find.matches.len();
        let range = self.find.matches[self.find.current].clone();
        let line = self.buffer.position_at(range.start).line;
        self.selected = range;
        self.reversed = false;
        self.marked = None;
        self.list_state.scroll_to_reveal_item(line);
    }

    fn next_match(&mut self, cx: &mut Context<Self>) {
        if self.find.matches.is_empty() {
            return;
        }
        let next = (self.find.current + 1) % self.find.matches.len();
        self.goto_match(next);
        cx.notify();
    }

    fn prev_match(&mut self, cx: &mut Context<Self>) {
        if self.find.matches.is_empty() {
            return;
        }
        let len = self.find.matches.len();
        let prev = (self.find.current + len - 1) % len;
        self.goto_match(prev);
        cx.notify();
    }

    /// Keystrokes typed while the bar is open go to the active field
    /// (single-focus spike compromise). Newlines are dropped: fields are
    /// single-line and `Buffer::find_all` matches nothing for `\n` queries.
    fn find_input(&mut self, text: &str) {
        let cleaned: String = text
            .chars()
            .filter(|c| !c.is_control() || *c == ' ')
            .collect();
        match self.find.field {
            FindField::Query => {
                self.find.query.push_str(&cleaned);
                self.refresh_matches(true);
            }
            FindField::Replacement => self.find.replacement.push_str(&cleaned),
        }
    }

    /// Grapheme-aware backspace for the active find field (same standard as
    /// the buffer).
    fn find_backspace(&mut self) {
        let field = match self.find.field {
            FindField::Query => &mut self.find.query,
            FindField::Replacement => &mut self.find.replacement,
        };
        let new_len = field
            .graphemes(true)
            .next_back()
            .map(|g| field.len() - g.len());
        if let Some(len) = new_len {
            field.truncate(len);
        }
        if self.find.field == FindField::Query {
            self.refresh_matches(true);
        }
    }

    /// Replace the current match (cmd-enter, or enter in the replacement
    /// field) and keep the visual position near where the edit happened.
    fn replace_current(&mut self, cx: &mut Context<Self>) {
        if self.find.matches.is_empty() {
            return;
        }
        let range = self.find.matches[self.find.current].clone();
        let anchor = range.start;
        let replacement = self.find.replacement.clone();
        // apply_edit pushes one undo entry and silently refreshes matches.
        self.apply_edit(range, &replacement);
        self.find.replaced_total += 1;
        if !self.find.matches.is_empty() {
            let index = self
                .find
                .matches
                .iter()
                .position(|m| m.start >= anchor)
                .unwrap_or(0);
            self.goto_match(index);
        }
        cx.notify();
    }

    /// Replace every match in a single document-wide edit (one undo entry),
    /// measured as one operation into `replace_all_ms`.
    fn replace_all(&mut self, cx: &mut Context<Self>) {
        if self.find.matches.is_empty() {
            return;
        }
        let start = Instant::now();
        let count = self.find.matches.len();
        let mut new_text = String::with_capacity(self.buffer.doc_len());
        let mut last = 0usize;
        for range in &self.find.matches {
            new_text.push_str(&self.buffer.slice(last..range.start));
            new_text.push_str(&self.find.replacement);
            last = range.end;
        }
        new_text.push_str(&self.buffer.slice(last..self.buffer.doc_len()));
        let doc_len = self.buffer.doc_len();
        self.apply_edit(0..doc_len, &new_text);
        self.find.replaced_total += count;
        self.metrics
            .borrow_mut()
            .replace_all_ms
            .push(duration_ms(start.elapsed()));
        cx.notify();
    }

    // ---- diff view (slice 3, M1-3b) --------------------------------------

    /// Recompute the unified diff rows: `diff.base_lines` (the file as
    /// loaded) vs. the current buffer. The compute time is recorded into
    /// `diff_compute_ms`; the caller marks the render sample with
    /// `mark_diff` so "requested → rendered" is captured separately. The
    /// correctness contract this surface owes the M4 proposal view is not
    /// "the diff is empty after an edit" (an edit must show), but that the
    /// diff **reconstructs the edited document from the base** — pinned by
    /// `diff_reconstructs_edited_document`.
    fn recompute_diff(&mut self) {
        let start = Instant::now();
        let base = self.diff.base_lines.join("\n");
        let current = self.buffer.lines().join("\n");
        let d = similar::TextDiff::from_lines(&base, &current);
        let mut rows = Vec::new();
        let mut added = 0usize;
        let mut removed = 0usize;
        for change in d.iter_all_changes() {
            let label = match change.tag() {
                similar::ChangeTag::Delete => {
                    removed += 1;
                    '-'
                }
                similar::ChangeTag::Insert => {
                    added += 1;
                    '+'
                }
                similar::ChangeTag::Equal => ' ',
            };
            rows.push(DiffRow {
                label,
                text: change.value().trim_end_matches('\n').to_string(),
            });
        }
        self.diff.rows = rows;
        self.diff.added_total += added;
        self.diff.removed_total += removed;
        // Park the cursor on the first hunk so opening the diff shows the
        // change, not the unchanged head of the file.
        self.diff.cursor = self
            .diff
            .rows
            .iter()
            .position(|row| row.label != ' ')
            .unwrap_or(0);
        self.metrics
            .borrow_mut()
            .diff_compute_ms
            .push(duration_ms(start.elapsed()));
    }

    /// Open the diff view (cmd-d). The base is seeded from the loaded file
    /// the first time; the cursor and scroll reset to the first hunk.
    fn open_diff(&mut self, cx: &mut Context<Self>) {
        if !self.diff.open {
            self.diff.open = true;
        }
        self.recompute_diff();
        self.diff_list_state = ListState::new(
            self.diff.rows.len(),
            ListAlignment::Top,
            px(LINE_HEIGHT * 2.0),
        );
        let cursor = self.diff.cursor;
        self.diff_list_state.scroll_to_reveal_item(cursor);
        self.metrics.borrow_mut().mark_diff(Instant::now());
        cx.notify();
    }

    fn close_diff(&mut self, cx: &mut Context<Self>) {
        self.diff.open = false;
        cx.notify();
    }

    /// Move the diff cursor to the next/previous **hunk** (cmd-g/shift-g).
    /// A modified line renders as an adjacent delete+insert pair, so a plain
    /// row step would park between them; instead, leave the current block of
    /// consecutive change rows and land on the next one, wrapping.
    fn diff_move_cursor(&mut self, delta: isize, cx: &mut Context<Self>) {
        let len = self.diff.rows.len();
        if len == 0 {
            return;
        }
        let step = if delta >= 0 { 1isize } else { -1 };
        let is_change = |ix: usize| self.diff.rows[ix].label != ' ';
        let mut ix = self.diff.cursor;
        // Leave the current change block (the +/− pair of one hunk)…
        let mut guard = 0usize;
        while is_change(ix) {
            ix = (ix as isize + step).rem_euclid(len as isize) as usize;
            guard += 1;
            if guard >= len {
                return; // the whole view is one change block: nowhere to go
            }
        }
        // …then walk through context rows to the next change block.
        guard = 0;
        while !is_change(ix) {
            ix = (ix as isize + step).rem_euclid(len as isize) as usize;
            guard += 1;
            if guard >= len {
                return; // no changes at all (empty diff): stay put
            }
        }
        self.diff.cursor = ix;
        self.diff_list_state.scroll_to_reveal_item(self.diff.cursor);
        cx.notify();
    }

    /// The diff view row: label gutter + line text, colored by tag.
    fn diff_row(&self, ix: usize) -> gpui::Div {
        let Some(row) = self.diff.rows.get(ix) else {
            return div();
        };
        let (label_color, bg) = match row.label {
            '+' => (rgb(DIFF_ADD_LABEL), Some(DIFF_ADD_BG)),
            '-' => (rgb(DIFF_REMOVE_LABEL), Some(DIFF_REMOVE_BG)),
            _ => (rgb(DIFF_CONTEXT_LABEL), None),
        };
        let mut element = div()
            .px_2()
            .h(px(LINE_HEIGHT))
            .flex()
            .items_center()
            .whitespace_nowrap()
            .font_family(FONT_FAMILY)
            .text_size(px(TEXT_SIZE));
        if ix == self.diff.cursor {
            element = element.bg(rgba(SELECTION_BG));
        } else if let Some(color) = bg {
            element = element.bg(rgb(color));
        }
        element
            .child(
                div()
                    .w(px(16.0))
                    .text_color(label_color)
                    .child(row.label.to_string()),
            )
            .child(div().text_color(rgb(STATUS_FG)).child(row.text.clone()))
    }

    // ---- find bar actions ------------------------------------------------

    fn on_toggle_find(&mut self, _: &ToggleFind, _window: &mut Window, cx: &mut Context<Self>) {
        if self.find.open && self.find.field == FindField::Query {
            self.close_find(cx);
        } else {
            self.open_find(false, cx);
        }
    }

    fn on_focus_replace(&mut self, _: &FocusReplace, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_find(true, cx);
    }

    fn on_close_find(&mut self, _: &CloseFind, _window: &mut Window, cx: &mut Context<Self>) {
        if self.find.open {
            self.close_find(cx);
        }
    }

    fn on_replace_current(
        &mut self,
        _: &ReplaceCurrent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.find.open {
            self.replace_current(cx);
        }
    }

    fn on_replace_all(&mut self, _: &ReplaceAll, _window: &mut Window, cx: &mut Context<Self>) {
        if self.find.open {
            self.replace_all(cx);
        }
    }

    // ---- diff view actions (slice 3, M1-3b) ------------------------------

    fn on_toggle_diff(&mut self, _: &ToggleDiff, _window: &mut Window, cx: &mut Context<Self>) {
        if self.diff.open {
            self.close_diff(cx);
        } else {
            self.open_diff(cx);
        }
    }

    // ---- find probe (slice 3 measurement) ---------------------------------

    /// Each probe step submits a real query from a fixed list against the
    /// whole document and reveals the next match — the exact functions the
    /// interactive bar calls, so the measured path is the shipped path.
    fn advance_find_probe(&mut self, cx: &mut Context<Self>) {
        const QUERIES: [&str; 5] = ["noche", "INT.", "toda la noche", "habitación", "escena"];
        self.find.open = true;
        self.find.field = FindField::Query;
        let query = QUERIES[self.find_probe_cursor % QUERIES.len()];
        self.find.query = query.to_string();
        self.refresh_matches(true);
        if !self.find.matches.is_empty() {
            self.find.current = self.find_probe_cursor % self.find.matches.len();
            self.goto_match(self.find.current);
        }
        self.find_probe_cursor += 1;
        self.find_probe_left = self.find_probe_left.saturating_sub(1);

        let finished = self.find_probe_left == 0 && self.probe_left == 0;
        cx.notify();
        if finished {
            let report = self.metrics.borrow().report();
            println!("{report}");
            cx.defer(|cx| cx.quit());
        }
    }

    // ---- project workspace (M6 slice 1) ----------------------------------

    /// Full buffer text. Round-trips what was loaded: `Buffer::new` splits
    /// on `\n` and `slice(0..doc_len)` joins the same lines back.
    fn full_text(&self) -> String {
        self.buffer.slice(0..self.buffer.doc_len())
    }

    /// True when the buffer diverged from the last loaded/saved text.
    /// Generation-based (O(1)): every mutation path bumps `metrics.edits`.
    fn dirty(&self) -> bool {
        self.metrics.borrow().edits != self.saved_edits
    }

    /// Replace the buffer with a freshly loaded document, resetting all
    /// per-document state (selection, undo, find, diff, scene cache). Scroll
    /// starts at the top: this is an explicit document switch, not a jump the
    /// user must recover from. `base_text` becomes the new diff/save base.
    fn load_document(&mut self, path: String, text: String) {
        self.buffer = Buffer::new(&text);
        self.base_text = text.clone();
        self.doc_path = path;
        self.selected = 0..0;
        self.reversed = false;
        self.marked = None;
        self.composing = false;
        self.last_click = None;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.scene_lines.clear();
        self.scenes_dirty = true;
        self.current_scene = 0;
        self.list_state = ListState::new(
            self.buffer.line_count(),
            ListAlignment::Top,
            px(LINE_HEIGHT * 2.0),
        );
        self.find = FindState::default();
        self.diff = DiffState {
            base_lines: text.split('\n').map(str::to_string).collect(),
            ..DiffState::default()
        };
        self.diff_list_state = ListState::new(0, ListAlignment::Top, px(LINE_HEIGHT * 2.0));
        self.saved_edits = self.metrics.borrow().edits;
        self.notice = None;
    }

    /// Explorer click: load the selected document. Unsaved changes block the
    /// switch explicitly (save with ⌘S first) — editor state is never dropped
    /// silently, which is the M6 acceptance rule.
    fn select_doc(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.project.clone() else {
            return;
        };
        let Some(path) = self.explorer.get(ix).cloned() else {
            return;
        };
        if path == self.doc_path {
            return;
        }
        if self.dirty() {
            self.notice = Some(
                "cambios sin guardar: guarda con ⌘S antes de cambiar de documento".to_string(),
            );
            cx.notify();
            return;
        }
        match project.read_document(&path) {
            Ok(text) => {
                self.load_document(path, text);
                // A click re-targets typing (and IME composition) at the buffer.
                window.focus(&self.focus_handle(cx));
            }
            Err(error) => self.notice = Some(format!("no se pudo abrir {path}: {error}")),
        }
        cx.notify();
    }

    /// ⌘S: write the buffer through the boundary-checked project IO. The
    /// canonical file changes only here, on an explicit save — never by an
    /// agent action and never as a side effect of switching documents.
    fn on_save(&mut self, _: &Save, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.project.clone() else {
            self.notice = Some("modo fixture: no hay proyecto abierto (usa --project)".to_string());
            cx.notify();
            return;
        };
        let text = self.full_text();
        let edits = self.metrics.borrow().edits;
        if text == self.base_text {
            // Converged back (e.g. undo to the original): clear the dirty
            // flag without touching the file.
            self.saved_edits = edits;
            self.notice = Some("sin cambios que guardar".to_string());
            cx.notify();
            return;
        }
        match project.write_document(&self.doc_path, &text) {
            Ok(()) => {
                self.base_text = text.clone();
                self.saved_edits = edits;
                // The saved file becomes the diff base (same model as the M4
                // proposal surface: canonical file vs. edited buffer).
                self.diff.base_lines = text.split('\n').map(str::to_string).collect();
                if self.diff.open {
                    // Recompute and resize the rows list in one step.
                    self.open_diff(cx);
                } else {
                    // Keep the dump honest; opening the diff rebuilds rows.
                    self.recompute_diff();
                }
                self.notice = Some(format!("guardado: {}", self.doc_path));
            }
            Err(error) => {
                self.notice = Some(format!("error al guardar {}: {error}", self.doc_path));
            }
        }
        cx.notify();
    }

    /// Explorer panel (M6 slice 1): one row per project document. Clicking
    /// loads it (`select_doc`); the active document is highlighted to mirror
    /// `doc_path`. Long paths clip at the panel edge (status bar shows the
    /// full path).
    fn explorer_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let title = self
            .project
            .as_ref()
            .map(|project| project.manifest().title.clone())
            .unwrap_or_default();
        let mut panel = div()
            .id("explorer")
            .w(px(EXPLORER_W))
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(EXPLORER_BG))
            .border_r_1()
            .border_color(rgb(EXPLORER_BORDER))
            .child(
                div()
                    .px_2()
                    .py_1()
                    .text_sm()
                    .text_color(rgb(STATUS_FG))
                    .child(title),
            );
        for (ix, path) in self.explorer.iter().enumerate() {
            let selected = *path == self.doc_path;
            panel = panel.child(
                div()
                    .id(("doc", ix))
                    .px_2()
                    .py_1()
                    .text_sm()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .when(selected, |row| row.bg(rgb(DOC_SELECTED_BG)))
                    .text_color(rgb(if selected { DEFAULT_FG } else { STATUS_FG }))
                    .cursor_pointer()
                    .child(path.clone())
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.select_doc(ix, window, cx)),
                    ),
            );
        }
        panel
    }

    fn status_text(&self) -> String {
        let metrics = self.metrics.borrow();
        let (_, p50, p95, max) = Metrics::stats(&metrics.frame_gaps_ms);
        let caret = self.buffer.position_at(self.caret());
        // M6 slice 1: show the document (not just the run's fixture path) with
        // a dirty marker, plus the last outcome (save/open/blocked switch).
        let document = format!("{}{}", self.doc_path, if self.dirty() { " •" } else { "" });
        let notice = self
            .notice
            .as_ref()
            .map(|text| format!(" · {text}"))
            .unwrap_or_default();
        format!(
            "{} · L{}:C{} · ediciones {} · tecla→frame {} · {} líneas · {} escenas · frames {} · gap ms p50 {:.2} / p95 {:.2} / max {:.2} · probe {}{}",
            document,
            caret.line + 1,
            caret.col,
            metrics.edits,
            metrics.keystroke_to_frame_ms.len(),
            metrics.lines,
            metrics.scenes,
            metrics.renders,
            p50,
            p95,
            max,
            self.probe_left,
            notice,
        )
    }

    /// The find/replace bar (slice 3). Rendered above the document when
    /// open; it shows the two fields, which one owns keystrokes (a trailing
    /// caret marker), the match counter and the key hints. The fields are
    /// display-only: keystrokes are routed here by the input handler
    /// (single-focus spike compromise, see the module header).
    fn find_bar(&self) -> gpui::Div {
        let total = self.find.matches.len();
        let counter = if self.find.query.is_empty() {
            String::new()
        } else if total == 0 {
            "0/0".to_string()
        } else {
            format!("{}/{}", self.find.current + 1, total)
        };
        let marker = |active: bool| if active { "▌" } else { "" };
        let field = |label: &str, value: String, active: bool| {
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(div().text_color(rgb(FIND_DIM_FG)).child(label.to_string()))
                .child(
                    div()
                        .text_color(rgb(if active { FIND_FG } else { FIND_DIM_FG }))
                        .child(format!("{value}{}", marker(active))),
                )
        };
        div()
            .w_full()
            .flex()
            .items_center()
            .gap_3()
            .px_2()
            .h(px(STATUS_BAR_H))
            .bg(rgb(FIND_BG))
            .text_sm()
            .font_family(FONT_FAMILY)
            .whitespace_nowrap()
            .child(field(
                "Buscar:",
                self.find.query.clone(),
                self.find.field == FindField::Query,
            ))
            .child(field(
                "Reemplazar:",
                self.find.replacement.clone(),
                self.find.field == FindField::Replacement,
            ))
            .child(div().text_color(rgb(FIND_FG)).child(counter))
            .child(
                div().text_color(rgb(FIND_DIM_FG)).child(
                    "enter siguiente · cmd-enter reemplaza · cmd-alt-enter todo · esc cierra",
                ),
            )
    }

    /// Render a line that has only a caret (no selection, no IME, no matches)
    /// with soft-wrapping, placing the caret on the correct visual row.
    /// Returns a flex column where each visual row is a flex child; the row
    /// containing the caret gets a cursor element at the correct x offset.
    fn render_caret_only_wrapped_row(
        &self,
        window: &Window,
        base: gpui::Div,
        text: &str,
        doc: Range<usize>,
        caret: usize,
    ) -> gpui::Div {
        let caret_col = (caret - doc.start).min(text.len());
        let wrap_w = f32::from(self.wrap_width);
        let (visual_row, caret_x) = caret_visual_row_and_x(window, text, caret_col, wrap_w);
        let starts = visual_row_starts(window, text, wrap_w);

        // Build visual rows as a flex column.
        let mut col = base.flex().flex_col().w(self.wrap_width);
        for (row_idx, &start) in starts.iter().enumerate() {
            let end = starts.get(row_idx + 1).copied().unwrap_or(text.len());
            let segment_text = &text[start..end];
            let mut row_div = div()
                .font_family(FONT_FAMILY)
                .text_size(px(TEXT_SIZE))
                .text_color(line_color(text))
                .h(px(LINE_HEIGHT))
                .flex()
                .items_center();

            if row_idx == visual_row {
                // This visual row contains the caret: split at caret_x.
                // We render as: text before caret | cursor | text after caret.
                // For simplicity, we place cursor at the x offset using a
                // zero-width element with margin-left.
                let before = &segment_text[..caret_col
                    .saturating_sub(starts[visual_row])
                    .min(segment_text.len())];
                let after = &segment_text[caret_col
                    .saturating_sub(starts[visual_row])
                    .min(segment_text.len())..];
                if !before.is_empty() {
                    row_div = row_div.child(before.to_string());
                }
                row_div = row_div.child(cursor_element_at(caret_x));
                if !after.is_empty() {
                    row_div = row_div.child(after.to_string());
                }
            } else {
                row_div = row_div.child(segment_text.to_string());
            }
            col = col.child(row_div);
        }
        col
    }

    /// One row of the virtualized list.
    ///
    /// Non-hot rows (no caret/selection/IME) wrap at `wrap_width` and take
    /// whatever height the wrap needs; `list` measures each, so virtualization
    /// is preserved with variable heights. Rows carrying caret/selection/IME
    /// state keep the slice-2 single-line representation (fixed height, no
    /// wrap) so the caret stays exact — the documented spike compromise for
    /// wrapped editing.
    fn row(&self, ix: usize, window: &Window) -> gpui::Div {
        let text = &self.buffer.lines()[ix];

        let doc = self.buffer.line_content_range(ix);
        let caret = self.caret();
        let selection = if self.selected.is_empty() {
            None
        } else {
            Some(self.selected.clone())
        };
        let marked = self.marked.clone().filter(|range| !range.is_empty());

        let intersects = |range: &Range<usize>| range.end > doc.start && range.start < doc.end;
        let selection_here = selection.as_ref().filter(|range| intersects(range));
        let marked_here = marked.as_ref().filter(|range| intersects(range));
        let caret_here = caret >= doc.start && caret <= doc.end;

        // Find matches intersecting this line. `matches` is sorted and
        // non-overlapping, so a binary search finds the first candidate and
        // the scan stops at the first match past the line (bounded work per
        // visible row even with hundreds of matches).
        let matches_here: Vec<(Range<usize>, bool)> =
            if self.find.open && !self.find.matches.is_empty() {
                let first = self.find.matches.partition_point(|m| m.end <= doc.start);
                self.find.matches[first..]
                    .iter()
                    .enumerate()
                    .take_while(|(_, m)| m.start < doc.end)
                    .map(|(offset, m)| (m.clone(), first + offset == self.find.current))
                    .collect()
            } else {
                Vec::new()
            };

        let base = div()
            .px_2()
            .font_family(FONT_FAMILY)
            .text_size(px(TEXT_SIZE))
            .text_color(line_color(text));

        // Cold row: no caret/selection/IME/matches -> simple wrapped div.
        if selection_here.is_none()
            && marked_here.is_none()
            && !caret_here
            && matches_here.is_empty()
        {
            return base.w(self.wrap_width).child(text.clone());
        }

        // Caret-only (no selection, no IME, no matches): wrap the line and
        // place the caret on the correct visual row using a flex column of
        // visual rows. This is the B-3 wrapped-caret placement.
        if selection_here.is_none()
            && marked_here.is_none()
            && matches_here.is_empty()
            && caret_here
        {
            return self.render_caret_only_wrapped_row(window, base, text, doc, caret);
        }

        let mut row = base
            .h(px(LINE_HEIGHT))
            .flex()
            .items_center()
            .whitespace_nowrap();

        let len = text.len();
        let mut cuts: Vec<usize> = vec![0, len];
        if let Some(selection) = selection_here {
            cuts.push((selection.start - doc.start).min(len));
            cuts.push((selection.end - doc.start).min(len));
        }
        if let Some(marked) = marked_here {
            cuts.push((marked.start - doc.start).min(len));
            cuts.push((marked.end - doc.start).min(len));
        }
        for (range, _) in &matches_here {
            cuts.push((range.start - doc.start).min(len));
            cuts.push((range.end - doc.start).min(len));
        }
        let caret_col = (caret - doc.start).min(len);
        if caret_here {
            cuts.push(caret_col);
        }
        cuts.sort_unstable();
        cuts.dedup();

        for pair in cuts.windows(2) {
            let (start, end) = (pair[0], pair[1]);
            if caret_here && start == caret_col {
                row = row.child(cursor_element());
            }
            if start == end {
                continue;
            }
            let selected = selection_here.is_some_and(|selection| {
                start >= selection.start - doc.start && end <= selection.end - doc.start
            });
            let composing = marked_here.is_some_and(|marked| {
                start >= marked.start - doc.start && end <= marked.end - doc.start
            });
            let match_state = matches_here.iter().find(|(range, _)| {
                start >= range.start - doc.start && end <= range.end - doc.start
            });
            let mut segment = div().whitespace_nowrap();
            if selected {
                segment = segment.bg(rgba(SELECTION_BG));
            } else if let Some((_, is_current)) = match_state {
                // Selection wins over highlight; the current match is louder
                // than the rest.
                segment = segment.bg(rgba(if *is_current {
                    MATCH_CURRENT_BG
                } else {
                    MATCH_BG
                }));
            }
            if composing {
                segment = segment.underline();
            }
            row = row.child(segment.child(text[start..end].to_string()));
        }
        if caret_here && caret_col == len {
            row = row.child(cursor_element());
        }
        row
    }

    /// JSON snapshot used by the scripted typing verification
    /// (`--dump-after-ms`): objective evidence of what the buffer contains
    /// after a driven sequence of keystrokes.
    fn dump_json(&self) -> String {
        let caret = self.caret();
        let position = self.buffer.position_at(caret);
        let len = self.buffer.doc_len();

        let mut start = caret.saturating_sub(64);
        while start < caret && !self.is_boundary(start) {
            start += 1;
        }
        let mut end = (caret + 64).min(len);
        while end > caret && !self.is_boundary(end) {
            end -= 1;
        }
        let around = escape_json(&self.buffer.slice(start..end));

        // The diff counts are the ground truth the M4 proposal surface
        // leans on: every row must be a real change (no phantom EOF line)
        // and `buffer_hash` lets an external check compare the rendered
        // buffer against an independently computed expectation.
        let diff_lines = self.diff.rows.len();
        let diff_added = self.diff.rows.iter().filter(|row| row.label == '+').count();
        let diff_removed = self.diff.rows.iter().filter(|row| row.label == '-').count();
        let buffer_hash = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            for line in self.buffer.lines() {
                line.hash(&mut hasher);
            }
            format!("{:016x}", hasher.finish())
        };

        // Composition state (Spike B): `marked` is the live setMarkedText
        // range in doc byte offsets (null when no composition is open) and
        // `composing` is the undo-group session flag. Together they let an
        // external check observe a composition mid-flight and confirm the
        // session closed after commit.
        let marked_dump = match &self.marked {
            Some(range) => format!("[{},{}]", range.start, range.end),
            None => "null".to_string(),
        };

        // Click debug (Spike B): the last left-click's window coordinates,
        // resolved line, item bounds, and doc offset. `null` when no click
        // has been recorded or the click was ignored.
        let (click_x, click_y, click_line, click_top, click_h, click_offset) =
            self.last_click.unwrap_or((0.0, 0.0, None, None, 0.0, None));
        let click_line_dump = click_line
            .map(|l| l.to_string())
            .unwrap_or("null".to_string());
        let click_top_dump = click_top
            .map(|t| format!("{:.1}", t))
            .unwrap_or("null".to_string());
        let click_h_dump = format!("{:.1}", click_h);
        let click_offset_dump = click_offset
            .map(|o| o.to_string())
            .unwrap_or("null".to_string());

        // M6 slice 1: which document the buffer represents, whether it
        // diverged from the last load/save, the last user-visible outcome
        // (save/open/blocked switch) and the explorer's document list — all
        // observable in the dump without trusting a screenshot.
        let notice_dump = match &self.notice {
            Some(text) => format!("\"{}\"", escape_json(text)),
            None => "null".to_string(),
        };
        let explorer_dump = self
            .explorer
            .iter()
            .map(|path| format!("\"{}\"", escape_json(path)))
            .collect::<Vec<_>>()
            .join(",");

        format!(
            concat!(
                "{{\"kind\":\"spike-a-buffer-dump\",",
                "\"lines\":{},\"doc_len\":{},\"edits\":{},",
                "\"caret_line\":{},\"caret_col\":{},\"selection\":[{},{}],",
                "\"scenes\":{},\"undo\":{},\"redo\":{},\"around\":\"{}\",",
                "\"find\":{{\"open\":{},\"query\":\"{}\",\"matches\":{},",
                "\"current\":{},\"replaced\":{}}},",
                "\"diff\":{{\"open\":{},\"lines\":{},\"added\":{},",
                "\"removed\":{},\"cursor\":{},\"buffer_hash\":\"{}\"}},",
                "\"ime\":{{\"marked\":{},\"composing\":{}}},",
                "\"click\":{{\"x\":{},\"y\":{},\"line\":{},\"top\":{},\"item_h\":{},\"offset\":{}}},",
                "\"document\":\"{}\",\"dirty\":{},\"notice\":{},\"explorer\":[{}]}}"
            ),
            self.buffer.line_count(),
            len,
            self.metrics.borrow().edits,
            position.line + 1,
            position.col,
            self.selected.start,
            self.selected.end,
            self.scene_lines.len(),
            self.undo_stack.len(),
            self.redo_stack.len(),
            around,
            self.find.open,
            escape_json(&self.find.query),
            self.find.matches.len(),
            self.find.current,
            self.find.replaced_total,
            self.diff.open,
            diff_lines,
            diff_added,
            diff_removed,
            self.diff.cursor,
            buffer_hash,
            marked_dump,
            self.composing,
            click_x,
            click_y,
            click_line_dump,
            click_top_dump,
            click_h_dump,
            click_offset_dump,
            escape_json(&self.doc_path),
            self.dirty(),
            notice_dump,
            explorer_dump,
        )
    }

    fn is_boundary(&self, offset: usize) -> bool {
        let position = self.buffer.position_at(offset);
        self.buffer.lines()[position.line].is_char_boundary(position.col)
    }
}

fn cursor_element() -> gpui::Div {
    // The negative right margin keeps the cursor's flow width at zero so the
    // text after the caret is not pushed sideways.
    div().w(px(2.0)).mr(px(-2.0)).h_full().bg(rgb(CURSOR_FG))
}

/// Cursor element positioned at a specific x offset within a visual row
/// (used for wrapped-line caret placement, B-3). The x offset is from the
/// start of the visual row; we use `ml` (margin-left) to position it.
fn cursor_element_at(x: f32) -> gpui::Div {
    div()
        .w(px(2.0))
        .mr(px(-2.0))
        .h_full()
        .ml(px(x))
        .bg(rgb(CURSOR_FG))
}

fn escape_json(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

/// Undo bookkeeping for one applied edit (Spike B undo grouping). While an
/// IME composition session is open (`composing`), consecutive edits that
/// replace the composition's own range merge into the single open entry, so
/// one composition = one undo step no matter how many marked updates the IME
/// sends. A range that does not line up opens a fresh entry instead of
/// corrupting the merge.
fn record_edit(
    undo_stack: &mut Vec<Edit>,
    composing: bool,
    replaced: Range<usize>,
    inserted: Range<usize>,
    new_text: &str,
    old_text: String,
) {
    if composing
        && let Some(prev) = undo_stack.last_mut()
        && prev.composition
        && prev.range == replaced
    {
        prev.new_text = new_text.to_string();
        prev.range = inserted;
        return;
    }
    undo_stack.push(Edit {
        range: inserted,
        new_text: new_text.to_string(),
        old_text,
        composition: composing,
    });
}

/// Byte offsets inside `text` for a UTF-16 range expressed **relative to
/// `text`** — what `setMarkedText:selectedRange:` delivers (the selection
/// lives inside the marked string, not in document coordinates; treating it
/// as document UTF-16 drifts as soon as the document head is non-ASCII).
/// Offsets past the end clamp to `text.len()`, and offsets inside a
/// surrogate pair clamp to the enclosing character boundary.
fn utf16_to_byte_in(text: &str, utf16: Range<usize>) -> Range<usize> {
    let mut units = 0usize;
    let mut start = None;
    let mut end = None;
    for (byte, ch) in text.char_indices() {
        if start.is_none() && units >= utf16.start {
            start = Some(byte);
        }
        if end.is_none() && units >= utf16.end {
            end = Some(byte);
        }
        units += ch.len_utf16();
    }
    start.unwrap_or(text.len())..end.unwrap_or(text.len())
}

/// Byte columns where each visual (soft-wrapped) row of a line starts,
/// derived from shaped glyph positions crossing `wrap_width`. Glyph x
/// positions are absolute on the un-wrapped line, so a glyph past the wrap
/// width marks the start of the next visual row. Word-wrap edges can differ
/// by a word between GPUI's div layout and this approximation; the spike
/// accepts and documents that (mouse caret lands within the same row).
fn wrap_row_starts(glyphs: &[(usize, f32)], wrap_width: f32) -> Vec<usize> {
    let mut starts = vec![0usize];
    let mut current_row_start_x = 0.0;
    for &(index, x) in glyphs {
        // When the glyph's absolute x minus the current row's start x
        // exceeds the wrap width, we've wrapped to a new visual row.
        if x - current_row_start_x >= wrap_width {
            starts.push(index);
            current_row_start_x = x;
        }
    }
    starts
}

/// Compute the visual (soft-wrapped) row boundaries for a line of text,
/// given the available content width. Returns a vec of byte offsets within
/// `line` where each visual row starts (first is always 0). Uses shaped
/// glyph x-positions to find wrap points at `wrap_width`.
fn visual_row_starts(window: &Window, line: &str, wrap_width: f32) -> Vec<usize> {
    let shaped = shape_line(window, line);
    let glyphs: Vec<(usize, f32)> = shaped
        .runs
        .iter()
        .flat_map(|run| {
            run.glyphs
                .iter()
                .map(|g| (g.index, f32::from(g.position.x)))
        })
        .collect();
    wrap_row_starts(&glyphs, wrap_width)
}

/// Given a line, caret column (byte offset within the line), and the
/// visual row starts, returns the visual row index (0-based) containing
/// the caret, and the x-offset within that visual row for cursor rendering.
fn caret_visual_row_and_x(
    window: &Window,
    line: &str,
    caret_col: usize,
    wrap_width: f32,
) -> (usize, f32) {
    let starts = visual_row_starts(window, line, wrap_width);
    let row = starts
        .iter()
        .rposition(|&start| start <= caret_col)
        .unwrap_or(0);
    // x within the visual row: compute via shaped glyphs
    let shaped = shape_line(window, line);
    let glyphs: Vec<(usize, f32)> = shaped
        .runs
        .iter()
        .flat_map(|run| {
            run.glyphs
                .iter()
                .map(|g| (g.index, f32::from(g.position.x)))
        })
        .collect();
    let row_start_x = glyphs
        .iter()
        .find(|(ix, _)| *ix >= starts[row])
        .map(|(_, x)| *x)
        .unwrap_or(0.0);
    let caret_x = glyphs
        .iter()
        .find(|(ix, _)| *ix >= caret_col)
        .map(|(_, x)| *x)
        .unwrap_or_else(|| {
            // Past end: use line width
            shaped
                .runs
                .last()
                .map(|r| {
                    r.glyphs
                        .last()
                        .map(|g| f32::from(g.position.x))
                        .unwrap_or(0.0)
                })
                .unwrap_or(0.0)
        });
    (row, caret_x - row_start_x)
}

impl EntityInputHandler for Editor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.clamp(self.range_from_utf16(&range_utf16));
        adjusted_range.replace(self.range_to_utf16(&range));
        Some(self.buffer.slice(range))
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected),
            reversed: self.reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked.as_ref().map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // The system calls this after commit (insertText) or when it drops
        // the composition; the buffer text was already settled by the
        // preceding call, so this only ends the session.
        self.marked = None;
        self.composing = false;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Find bar owns keystrokes while open: typing, IME commits and
        // pastes land in the active field, never in the buffer.
        if self.find.open {
            self.find_input(new_text);
            cx.notify();
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or(self.marked.clone())
            .unwrap_or_else(|| self.selected.clone());
        let text = normalize_newlines(new_text);
        let range = {
            let mut range = self.clamp(range);
            if range.end < range.start {
                range = range.start..range.start;
            }
            range
        };
        self.apply_edit(range, &text);
        // `insertText` is both plain typing and composition commit; either
        // way no marked session survives it (the system's later unmarkText
        // becomes a no-op). This also guarantees a stale `composing` flag
        // can never swallow the next keystroke into an old entry.
        self.composing = false;
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // IME while the find bar is open: commit into the active field with
        // no marked state (the documented single-focus compromise — live
        // composition inside the bar is production work).
        if self.find.open {
            self.find_input(new_text);
            cx.notify();
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or(self.marked.clone())
            .unwrap_or_else(|| self.selected.clone());
        let text = normalize_newlines(new_text);
        let range = self.clamp(range);
        // First update of a session opens the undo group; subsequent marked
        // updates merge into that entry inside `record_edit`.
        self.composing = true;
        let inserted = self.apply_edit(range, &text);
        self.marked = (!text.is_empty()).then_some(inserted.clone());
        if text.is_empty() {
            // An empty setMarkedText is the IME removing the composition
            // (cancellation): the session ends here even if the system's
            // unmarkText never follows, so the stale flag can never swallow
            // the next keystroke into an old entry.
            self.composing = false;
        }
        // `selectedRange` from setMarkedText is relative to the *marked
        // string*, not to the document: convert it inside `text` (treating it
        // as document UTF-16 drifts as soon as the document head is
        // non-ASCII) and anchor the result at the inserted span.
        let selection = new_selected_range_utf16
            .as_ref()
            .map(|utf16| {
                let local = utf16_to_byte_in(&text, utf16.clone());
                (inserted.start + local.start)..(inserted.start + local.end)
            })
            .unwrap_or_else(|| inserted.end..inserted.end);
        self.selected = self.clamp(selection);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _bounds: Bounds<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.clamp(self.range_from_utf16(&range_utf16));
        let start = self.buffer.position_at(range.start);
        let end = self.buffer.position_at(range.end);
        // `bounds_for_item` already accounts for scroll. Only rendered (visible)
        // items report bounds, which is fine: IME positions over visible text.
        let start_bounds = self.list_state.bounds_for_item(start.line)?;
        let end_bounds = self.list_state.bounds_for_item(end.line);
        let x_start = x_for_index(window, &self.buffer.lines()[start.line], start.col);
        let x_end = x_for_index(window, &self.buffer.lines()[end.line], end.col);
        let top_left = point(start_bounds.left() + x_start, start_bounds.top());
        let bottom_right = match end_bounds {
            Some(bounds) => point(bounds.left() + x_end, bounds.bottom()),
            None => point(start_bounds.left() + x_end, start_bounds.bottom()),
        };
        Some(Bounds::from_corners(top_left, bottom_right))
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        // NSTextInputClient's `characterIndexForPoint:` uses the same mapping
        // as a mouse click; the index crosses the Cocoa boundary as UTF-16,
        // like every other range this handler reports.
        let offset = self.offset_for_point(position, window)?;
        Some(self.buffer.offset_to_utf16(offset))
    }
}

impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        self.metrics.borrow_mut().on_render(now);

        // The probe is driven by a timer scheduled from here, never by
        // notifying during render (see `advance_probe`).
        self.schedule_probe_step(window, cx);

        // Wrap width for non-hot rows, refreshed from the viewport each frame.
        // The `list` render callback reads it back through the entity. With
        // the explorer panel open (project mode) the document area is narrower
        // by exactly the panel width, so wrapped rows never hide under it.
        let explorer_w = if self.project.is_some() {
            EXPLORER_W
        } else {
            0.0
        };
        self.wrap_width =
            (window.viewport_size().width - px(explorer_w + ROW_PADDING_X * 2.0)).max(px(200.0));

        let status = self.status_text();
        let focus = self.focus_handle.clone();
        let entity = cx.entity();

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::next_scene))
            .on_action(cx.listener(Self::prev_scene))
            .on_action(cx.listener(Self::on_left))
            .on_action(cx.listener(Self::on_right))
            .on_action(cx.listener(Self::on_up))
            .on_action(cx.listener(Self::on_down))
            .on_action(cx.listener(Self::on_home))
            .on_action(cx.listener(Self::on_end))
            .on_action(cx.listener(Self::on_select_left))
            .on_action(cx.listener(Self::on_select_right))
            .on_action(cx.listener(Self::on_select_up))
            .on_action(cx.listener(Self::on_select_down))
            .on_action(cx.listener(Self::on_select_home))
            .on_action(cx.listener(Self::on_select_end))
            .on_action(cx.listener(Self::on_select_all))
            .on_action(cx.listener(Self::on_backspace))
            .on_action(cx.listener(Self::on_delete))
            .on_action(cx.listener(Self::on_insert_newline))
            .on_action(cx.listener(Self::on_undo))
            .on_action(cx.listener(Self::on_redo))
            .on_action(cx.listener(Self::on_copy))
            .on_action(cx.listener(Self::on_cut))
            .on_action(cx.listener(Self::on_paste))
            .on_action(cx.listener(Self::on_save))
            .on_action(cx.listener(Self::on_toggle_find))
            .on_action(cx.listener(Self::on_focus_replace))
            .on_action(cx.listener(Self::on_close_find))
            .on_action(cx.listener(Self::on_replace_current))
            .on_action(cx.listener(Self::on_replace_all))
            .on_action(cx.listener(Self::on_toggle_diff))
            .child(
                div()
                    .id("status")
                    .h(px(26.0))
                    .w_full()
                    .flex()
                    .items_center()
                    .px_2()
                    .bg(rgb(STATUS_BG))
                    .text_sm()
                    .whitespace_nowrap()
                    .text_color(rgb(STATUS_FG))
                    .child(status),
            )
            .when(self.find.open, |bar| bar.child(self.find_bar()))
            .child(
                div()
                    .flex_1()
                    .flex()
                    // Body row (M6 slice 1): explorer panel (project mode
                    // only) + document area. Fixture mode has no panel, so
                    // the verified full-width spike geometry is unchanged.
                    .when(self.project.is_some(), |body| {
                        body.child(self.explorer_panel(cx))
                    })
                    .child(
                        div()
                            .flex_1()
                            .relative()
                            // Click-to-caret for the buffer list (the diff list is
                            // read-only and ignores clicks in the handler).
                            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_buffer_click))
                            // The diff view replaces the buffer rows while open; the
                            // buffer list keeps its own (idle) scroll state so
                            // closing the diff restores the caret position.
                            .when(self.diff.open, |host| {
                                host.child(
                                    list(self.diff_list_state.clone(), {
                                        let entity = entity.clone();
                                        move |ix: usize, _window: &mut Window, cx: &mut App| {
                                            entity.read(cx).diff_row(ix).into_any_element()
                                        }
                                    })
                                    .size_full(),
                                )
                            })
                            .when(!self.diff.open, |host| {
                                host.child(
                                    list(self.list_state.clone(), {
                                        let entity = entity.clone();
                                        move |ix: usize, window: &mut Window, cx: &mut App| {
                                            entity.read(cx).row(ix, window).into_any_element()
                                        }
                                    })
                                    .size_full(),
                                )
                            })
                            // Invisible overlay that registers the platform input
                            // handler for this frame: typing and IME flow into
                            // `EntityInputHandler` while the root has focus.
                            .child(
                                canvas(
                                    |_bounds: Bounds<Pixels>,
                                     _window: &mut Window,
                                     _cx: &mut App| (),
                                    {
                                        let entity = entity.clone();
                                        let focus = focus.clone();
                                        move |bounds: Bounds<Pixels>,
                                              _: (),
                                              window: &mut Window,
                                              cx: &mut App| {
                                            window.handle_input(
                                                &focus,
                                                ElementInputHandler::new(bounds, entity.clone()),
                                                cx,
                                            );
                                        }
                                    },
                                )
                                .absolute()
                                .size_full(),
                            ),
                    ),
            )
    }
}

struct Args {
    fixture: String,
    /// Narra project root (`--project`); mutually exclusive with an explicit
    /// `--fixture`. The default fixture stays valid when neither is given.
    project: Option<String>,
    steps: usize,
    find_steps: usize,
    diff_steps: usize,
    report_after_ms: Option<u64>,
    dump_after_ms: Option<u64>,
}

fn parse_args() -> Result<Args, String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut fixture = "fixtures/screenplay-1000.fountain".to_string();
    let mut fixture_given = false;
    let mut project: Option<String> = None;
    let mut steps = 0usize;
    let mut find_steps = 0usize;
    let mut diff_steps = 0usize;
    let mut report_after_ms = None;
    let mut dump_after_ms = None;

    let mut i = 0;
    while i < args.len() {
        let key = args[i].as_str();
        let value = args.get(i + 1).ok_or(format!("{key} requiere un valor"))?;
        match key {
            "--fixture" => {
                fixture = value.clone();
                fixture_given = true;
            }
            // M6 slice 1: open a real Narra project through narra-core
            // (explorer panel, ⌘S save). Mutually exclusive with --fixture so
            // the verified spike driver can never run against a project by
            // accident (and vice versa).
            "--project" => project = Some(value.clone()),
            "--steps" => {
                steps = value
                    .parse()
                    .map_err(|_| format!("--steps inválido: {value}"))?
            }
            "--find-steps" => {
                find_steps = value
                    .parse()
                    .map_err(|_| format!("--find-steps inválido: {value}"))?
            }
            "--diff-steps" => {
                diff_steps = value
                    .parse()
                    .map_err(|_| format!("--diff-steps inválido: {value}"))?
            }
            "--report-after-ms" => {
                report_after_ms = Some(
                    value
                        .parse()
                        .map_err(|_| format!("--report-after-ms inválido: {value}"))?,
                )
            }
            "--dump-after-ms" => {
                dump_after_ms = Some(
                    value
                        .parse()
                        .map_err(|_| format!("--dump-after-ms inválido: {value}"))?,
                )
            }
            other => return Err(format!("argumento desconocido: {other}")),
        }
        i += 2;
    }

    if project.is_some() && fixture_given {
        return Err("usa --fixture o --project, no ambos".to_string());
    }

    Ok(Args {
        fixture,
        project,
        steps,
        find_steps,
        diff_steps,
        report_after_ms,
        dump_after_ms,
    })
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let started = Instant::now();

    // M6 slice 1: `--project` goes through narra-core (validated manifest,
    // boundary-checked IO); `--fixture` is the untouched spike path.
    let project = match &args.project {
        Some(root) => Some(
            open_project(Path::new(root))
                .map_err(|error| format!("no se pudo abrir el proyecto {root}: {error}"))?,
        ),
        None => None,
    };

    let (content, doc_path) = match &project {
        Some(project) => {
            let path = project.manifest().default_screenplay_path.clone();
            let text = match project.read_document(&path) {
                Ok(text) => text,
                // A project created without a screenplay opens as a blank
                // document; ⌘S creates it through the same safe IO.
                Err(ProjectError::Document(DocumentError::Io { source, .. }))
                    if source.kind() == std::io::ErrorKind::NotFound =>
                {
                    String::new()
                }
                Err(error) => return Err(format!("no se pudo leer {path}: {error}")),
            };
            (text, path)
        }
        None => (
            fs::read_to_string(&args.fixture)
                .map_err(|e| format!("no se pudo leer {}: {e}", args.fixture))?,
            args.fixture.clone(),
        ),
    };
    let fixture_read_ms = duration_ms(started.elapsed());

    // The explorer's document list comes from the core: sorted, hidden state
    // (.git/.narra/dot-files) excluded, symlinks not followed.
    let explorer = match &project {
        Some(project) => project
            .list_documents()
            .map_err(|error| format!("no se pudo listar el proyecto: {error}"))?,
        None => Vec::new(),
    };

    let scene_count = content
        .lines()
        .filter(|line| is_scene_heading(line))
        .count();
    // The scene gate is fixture-specific (the spike measures scene
    // navigation); a real project may legitimately start with a blank
    // screenplay, so it must not block opening the workspace.
    if scene_count == 0 && project.is_none() {
        return Err(format!(
            "no se detectaron escenas en {} (¿es un Fountain?)",
            args.fixture
        ));
    }

    // Fixture mode must keep reporting exactly `args.fixture` (existing
    // scripts parse it); project mode reports the project root.
    let metrics_label = args.project.clone().unwrap_or_else(|| args.fixture.clone());

    let metrics = Rc::new(RefCell::new(Metrics::new(
        started,
        &metrics_label,
        fixture_read_ms,
        content.lines().count(),
        scene_count,
        ProbeSteps {
            scroll: args.steps,
            find: args.find_steps,
            diff: args.diff_steps,
        },
    )));

    let steps = args.steps;
    let find_steps = args.find_steps;
    let diff_steps = args.diff_steps;
    let report_after_ms = args.report_after_ms;
    let dump_after_ms = args.dump_after_ms;
    Application::new().run(move |cx: &mut App| {
        cx.activate(true);
        cx.bind_keys([
            // navigation and quit
            KeyBinding::new("cmd-g", NextScene, None),
            KeyBinding::new("cmd-shift-g", PrevScene, None),
            KeyBinding::new("cmd-q", Quit, None),
            // caret movement
            KeyBinding::new("left", MoveLeft, None),
            KeyBinding::new("right", MoveRight, None),
            KeyBinding::new("up", MoveUp, None),
            KeyBinding::new("down", MoveDown, None),
            KeyBinding::new("home", MoveHome, None),
            KeyBinding::new("end", MoveEnd, None),
            // selection
            KeyBinding::new("shift-left", SelectLeft, None),
            KeyBinding::new("shift-right", SelectRight, None),
            KeyBinding::new("shift-up", SelectUp, None),
            KeyBinding::new("shift-down", SelectDown, None),
            KeyBinding::new("shift-home", SelectHome, None),
            KeyBinding::new("shift-end", SelectEnd, None),
            KeyBinding::new("cmd-a", SelectAll, None),
            // editing
            KeyBinding::new("backspace", Backspace, None),
            KeyBinding::new("delete", DeleteText, None),
            KeyBinding::new("enter", InsertNewline, None),
            KeyBinding::new("cmd-z", Undo, None),
            KeyBinding::new("cmd-shift-z", Redo, None),
            KeyBinding::new("cmd-c", CopyText, None),
            KeyBinding::new("cmd-x", CutText, None),
            KeyBinding::new("cmd-v", PasteText, None),
            // save through the project IO (M6 slice 1; no-op notice in
            // fixture mode, where there is no project to write to)
            KeyBinding::new("cmd-s", Save, None),
            // find / replace (slice 3)
            KeyBinding::new("cmd-f", ToggleFind, None),
            KeyBinding::new("cmd-alt-f", FocusReplace, None),
            KeyBinding::new("escape", CloseFind, None),
            KeyBinding::new("cmd-enter", ReplaceCurrent, None),
            KeyBinding::new("cmd-alt-enter", ReplaceAll, None),
            // diff view (slice 3, M1-3b): cmd-d toggles, cmd-g/shift-g walk
            // hunks while it is open.
            KeyBinding::new("cmd-d", ToggleDiff, None),
        ]);

        cx.observe_keystrokes({
            let metrics = metrics.clone();
            move |_, _, _| metrics.borrow_mut().mark_keystroke(Instant::now())
        })
        .detach();

        cx.on_action({
            let metrics = metrics.clone();
            move |_: &Quit, cx| {
                println!("{}", metrics.borrow().report());
                cx.quit();
            }
        });

        let buffer = Buffer::new(&content);
        let line_count = buffer.line_count();
        // Seed the diff base with the same representation `Buffer::new`
        // uses (`split('\n')`, keeping the final empty line), so the base
        // matches the buffer byte for byte at load and every diff row after
        // that is a real change — `str::lines` here would fake a one-line
        // change at EOF on every diff.
        let base_lines: Vec<String> = content.split('\n').map(str::to_string).collect();
        let entity = cx.new(|cx| Editor {
            buffer,
            project,
            explorer,
            doc_path,
            base_text: content.clone(),
            saved_edits: 0,
            notice: None,
            selected: 0..0,
            reversed: false,
            marked: None,
            composing: false,
            last_click: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            scene_lines: Vec::new(),
            scenes_dirty: true,
            current_scene: 0,
            list_state: ListState::new(line_count, ListAlignment::Top, px(LINE_HEIGHT * 2.0)),
            diff_list_state: ListState::new(0, ListAlignment::Top, px(LINE_HEIGHT * 2.0)),
            focus_handle: cx.focus_handle(),
            wrap_width: px(800.0),
            metrics: metrics.clone(),
            probe_left: steps,
            probe_cursor: 1,
            probe_scheduled: false,
            find_probe_left: find_steps,
            find_probe_cursor: 0,
            diff_probe_left: diff_steps,
            diff_probe_total: diff_steps,
            find: FindState::default(),
            diff: DiffState {
                base_lines,
                ..DiffState::default()
            },
        });

        let bounds = Bounds::centered(None, size(px(1100.0), px(780.0)), cx);
        let window = match cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, _| entity.clone(),
        ) {
            Ok(window) => window,
            Err(err) => {
                eprintln!("no se pudo abrir la ventana: {err}");
                std::process::exit(1);
            }
        };

        window
            .update(cx, |view, window, cx| {
                window.focus(&view.focus_handle(cx));
            })
            .ok();

        // Timed self-report: lets an automated driver capture keystroke
        // metrics without relying on the window keeping focus.
        if let Some(ms) = report_after_ms {
            let metrics = metrics.clone();
            cx.spawn(async move |cx| {
                Timer::after(Duration::from_millis(ms)).await;
                println!("{}", metrics.borrow().report());
                cx.update(|cx| cx.quit()).ok();
            })
            .detach();
        }

        // Timed buffer dump: scripted keystroke verification.
        if let Some(ms) = dump_after_ms {
            cx.spawn(async move |cx| {
                Timer::after(Duration::from_millis(ms)).await;
                cx.update(|cx| {
                    entity.update(cx, |view, _| println!("{}", view.dump_json()));
                    cx.quit();
                })
                .ok();
            })
            .detach();
        }
    });

    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            eprintln!(
                "uso: spike-editor [--fixture <ruta> | --project <ruta>] [--steps <n>] [--report-after-ms <ms>] [--dump-after-ms <ms>]"
            );
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_edit_pushes_plain_edits_separately() {
        let mut stack = Vec::new();
        record_edit(&mut stack, false, 0..0, 0..1, "a", String::new());
        record_edit(&mut stack, false, 1..1, 1..2, "b", String::new());
        assert_eq!(stack.len(), 2);
        assert!(!stack[0].composition && !stack[1].composition);
        assert_eq!(stack[1].new_text, "b");
    }

    #[test]
    fn record_edit_merges_composition_updates_into_one_entry() {
        let mut stack = Vec::new();
        // setMarkedText inserts the dead-key base at the caret (5..5).
        record_edit(&mut stack, true, 5..5, 5..6, "e", String::new());
        // The IME updates the marked span (5..6) with the composed char (two
        // UTF-8 bytes), then commits it over the same span.
        record_edit(&mut stack, true, 5..6, 5..7, "\u{e9}", String::new());
        record_edit(&mut stack, true, 5..7, 5..7, "\u{e9}", String::new());
        assert_eq!(stack.len(), 1);
        let merged = &stack[0];
        assert!(merged.composition);
        assert_eq!(merged.range, 5..7);
        assert_eq!(merged.new_text, "\u{e9}");
        // One undo restores the full pre-composition state.
        assert_eq!(merged.old_text, "");
    }

    #[test]
    fn record_edit_keeps_pre_composition_text_for_undo() {
        let mut stack = Vec::new();
        // Composition over a selection: the marked span started as "old".
        record_edit(&mut stack, true, 5..8, 5..6, "e", "old".into());
        record_edit(&mut stack, true, 5..6, 5..7, "\u{e9}", String::new());
        assert_eq!(stack.len(), 1);
        assert_eq!(stack[0].old_text, "old");
        assert_eq!(stack[0].new_text, "\u{e9}");
        assert_eq!(stack[0].range, 5..7);
    }

    #[test]
    fn record_edit_does_not_merge_across_ranges() {
        let mut stack = Vec::new();
        record_edit(&mut stack, true, 0..0, 0..1, "x", String::new());
        // A composition edit at a different span opens a fresh entry rather
        // than corrupting the first one.
        record_edit(&mut stack, true, 9..9, 9..10, "y", String::new());
        assert_eq!(stack.len(), 2);
        assert_eq!(stack[0].new_text, "x");
        assert_eq!(stack[1].new_text, "y");
    }

    #[test]
    fn record_edit_stops_merging_after_session_ends() {
        let mut stack = Vec::new();
        record_edit(&mut stack, true, 0..0, 0..1, "x", String::new());
        // After unmark/commit cleared `composing`, the next edit must not
        // merge even though the last entry is still flagged as composition.
        record_edit(&mut stack, false, 1..1, 1..2, "y", String::new());
        assert_eq!(stack.len(), 2);
        assert_eq!(stack[1].new_text, "y");
    }

    #[test]
    fn wrap_row_starts_splits_at_the_wrap_width() {
        assert_eq!(wrap_row_starts(&[], 100.0), vec![0]);
        // Everything fits on one row.
        assert_eq!(wrap_row_starts(&[(0, 0.0), (3, 10.0)], 100.0), vec![0]);
        // Crossing glyphs open second visual row at wrap boundary.
        // Glyphs at x=0, 10, 20, 30 with wrap_width=15:
        // - x=0: row 0 starts at 0
        // - x=10: still row 0 (10-0 < 15)
        // - x=20: wraps (20-0 >= 15), new row at index 6
        // - x=30: same row (30-20 < 15)
        let glyphs = [(0, 0.0), (3, 10.0), (6, 20.0), (9, 30.0)];
        assert_eq!(wrap_row_starts(&glyphs, 15.0), vec![0, 6]);
        // A repeated glyph index (combining mark) does not open a new row.
        let glyphs = [(0, 0.0), (0, 8.0), (2, 20.0)];
        assert_eq!(wrap_row_starts(&glyphs, 10.0), vec![0, 2]);
    }

    // visual_row_starts tests require a Window, so they're integration-tested
    // via the live click verification (B-2) and the wrapped-caret live run below.

    #[test]
    fn utf16_to_byte_in_maps_relative_offsets() {
        assert_eq!(utf16_to_byte_in("abc", 0..0), 0..0);
        assert_eq!(utf16_to_byte_in("abc", 1..3), 1..3);
        // Decomposed e-acute = two chars (two UTF-16 units) in one
        // grapheme: offset 2 lands past both, at byte 3.
        assert_eq!(utf16_to_byte_in("e\u{301}", 2..2), 3..3);
        // Four units: a(1) + emoji(2) + b(1). Offsets count units, not bytes.
        assert_eq!(utf16_to_byte_in("a\u{1f600}b", 3..4), 5..6);
        // An offset inside a surrogate pair clamps to the char boundary.
        assert_eq!(utf16_to_byte_in("a\u{1f600}b", 2..4), 5..6);
        // Offsets past the end clamp to the text length.
        assert_eq!(utf16_to_byte_in("ab", 5..9), 2..2);
    }
}
