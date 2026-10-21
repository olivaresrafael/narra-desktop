//! Spike A — slice 2: is GPUI a viable *editing* surface?
//!
//! Slice 1 answered rendering/navigation; this slice adds a real editing core:
//! a multi-line buffer (`buffer.rs`), keyboard editing (typing, movement,
//! selection, undo/redo, clipboard), IME plumbing through GPUI's input handler,
//! and the same instrumentation as slice 1 so typing latency can be measured
//! against the approved thresholds (PERFORMANCE_BASELINES.md §6).
//!
//! Still NOT answered (slice 3 / Spike B): text wrapping, mouse click-to-caret,
//! full IME verification with a live composition, large-file strategy.
//!
//! Modes:
//!   spike-editor --fixture <path>                 interactive
//!       cmd-g / cmd-shift-g  next/previous scene
//!       arrows, shift-arrows, home/end, cmd-a     movement and selection
//!       cmd-z / cmd-shift-z                       undo / redo
//!       cmd-c / cmd-x / cmd-v                     clipboard
//!       cmd-q                                     quit (prints the JSON report)
//!   spike-editor --fixture <path> --steps 60      automated scroll probe (slice 1)
//!   spike-editor --report-after-ms <ms>           print the report and exit
//!   spike-editor --dump-after-ms <ms>             print a buffer dump and exit
//!       (used by the scripted typing verification: text, enter, backspace,
//!        selection and undo are observable without trusting a screenshot)
//!
//! Output: JSON lines on stdout starting with `{"kind":"spike-a`.

mod buffer;

use std::cell::RefCell;
use std::env;
use std::fs;
use std::ops::Range;
use std::process::ExitCode;
use std::rc::Rc;
use std::time::{Duration, Instant};

use buffer::{Buffer, Position};
use gpui::{
    App, Application, Bounds, ClipboardItem, Context, ElementInputHandler, EntityInputHandler,
    FocusHandle, Focusable, KeyBinding, Pixels, Point, Rgba, ScrollStrategy, ShapedLine, TextRun,
    Timer, UTF16Selection, UniformListScrollHandle, Window, WindowBounds, WindowOptions, actions,
    canvas, div, point, prelude::*, px, rgb, rgba, size, uniform_list,
};

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
        Undo,
        Redo,
        CopyText,
        CutText,
        PasteText,
    ]
);

/// Rows are fixed-height so `uniform_list` can virtualize them. Wrapping is
/// still slice 3 work; long lines are clipped (documented limitation).
const LINE_HEIGHT: f32 = 18.0;

/// Explicit so that cursor x-positions (shaped manually) match rendered rows.
/// 14 px == `text_sm` (0.875 rem), which slice 1 used.
const TEXT_SIZE: f32 = 14.0;
const FONT_FAMILY: &str = "Menlo";
/// `px_2` on every row.
const ROW_PADDING_X: f32 = 8.0;

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
    frame_gaps_ms: Vec<f64>,
    keystroke_to_frame_ms: Vec<f64>,
    renders: usize,
    edits: usize,
    probe_steps: usize,
}

impl Metrics {
    fn new(
        started: Instant,
        fixture: &str,
        fixture_read_ms: f64,
        lines: usize,
        scenes: usize,
        probe_steps: usize,
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
            frame_gaps_ms: Vec::new(),
            keystroke_to_frame_ms: Vec::new(),
            renders: 0,
            edits: 0,
            probe_steps,
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
        format!(
            concat!(
                "{{\"kind\":\"spike-a-slice2\",",
                "\"fixture\":\"{}\",\"lines\":{},\"scenes\":{},\"probe_steps\":{},",
                "\"fixture_read_ms\":{:.3},\"cold_start_ms\":{:.3},\"rss_mb\":{:.1},",
                "\"renders\":{},\"edits\":{},",
                "\"frame_ms\":{{\"count\":{},\"p50\":{:.3},\"p95\":{:.3},\"max\":{:.3}}},",
                "\"steady_frame_ms\":{{\"count\":{},\"p50\":{:.3},\"p95\":{:.3},\"max\":{:.3}}},",
                "\"keystroke_to_frame_ms\":{{\"count\":{},\"p50\":{:.3},\"p95\":{:.3},\"max\":{:.3}}}",
                "}}"
            ),
            self.fixture,
            self.lines,
            self.scenes,
            self.probe_steps,
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
        )
    }
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// One reversible edit. `range` is where `new_text` currently sits in the
/// document; undo replaces it back with `old_text`.
#[derive(Clone)]
struct Edit {
    range: Range<usize>,
    new_text: String,
    old_text: String,
}

struct Editor {
    buffer: Buffer,
    /// Selection as a sorted doc-offset range; empty == caret only.
    selected: Range<usize>,
    /// When true the caret sits at `selected.start` instead of `selected.end`.
    reversed: bool,
    /// IME composition range (doc offsets).
    marked: Option<Range<usize>>,
    undo_stack: Vec<Edit>,
    redo_stack: Vec<Edit>,

    scene_lines: Vec<usize>,
    scenes_dirty: bool,
    current_scene: usize,

    scroll: UniformListScrollHandle,
    focus_handle: FocusHandle,
    /// Bounds of the list container (captured from the canvas paint hook);
    /// needed for caret visibility and point → document hit-testing.
    last_bounds: Option<Bounds<Pixels>>,

    metrics: Rc<RefCell<Metrics>>,
    probe_left: usize,
    probe_cursor: usize,
    probe_scheduled: bool,
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

    fn scroll_y(&self) -> Pixels {
        self.scroll.0.borrow().base_handle.offset().y
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
        let (inserted, deleted) = self.buffer.replace(range, text);
        self.metrics.borrow_mut().edits += 1;
        self.undo_stack.push(Edit {
            range: inserted.clone(),
            new_text: text.to_string(),
            old_text: deleted,
        });
        const MAX_UNDO: usize = 5000;
        if self.undo_stack.len() > MAX_UNDO {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
        self.selected = inserted.end..inserted.end;
        self.reversed = false;
        self.marked = None;
        self.scenes_dirty = true;
        inserted
    }

    fn after_history_edit(&mut self, caret: usize) {
        self.metrics.borrow_mut().edits += 1;
        self.selected = caret..caret;
        self.reversed = false;
        self.marked = None;
        self.scenes_dirty = true;
    }

    fn undo(&mut self) {
        let Some(edit) = self.undo_stack.pop() else {
            return;
        };
        let inserted = edit.range.start..edit.range.start + edit.new_text.len();
        let (restored, deleted) = self.buffer.replace(inserted, &edit.old_text);
        self.redo_stack.push(Edit {
            range: restored.clone(),
            new_text: edit.old_text.clone(),
            old_text: deleted,
        });
        self.after_history_edit(restored.end);
    }

    fn redo(&mut self) {
        let Some(edit) = self.redo_stack.pop() else {
            return;
        };
        let current = edit.range.start..edit.range.start + edit.new_text.len();
        let (restored, deleted) = self.buffer.replace(current, &edit.old_text);
        self.undo_stack.push(Edit {
            range: restored.clone(),
            new_text: edit.old_text.clone(),
            old_text: deleted,
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
        self.scroll.scroll_to_item(line, ScrollStrategy::Top);
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
        self.scroll.scroll_to_item(target_line, ScrollStrategy::Top);
        self.probe_cursor += 1;
        self.probe_left = self.probe_left.saturating_sub(1);

        let finished = self.probe_left == 0;
        cx.notify();
        if finished {
            let report = self.metrics.borrow().report();
            println!("{report}");
            cx.defer(|cx| cx.quit());
        }
    }

    fn schedule_probe_step(&mut self, window: &Window, cx: &Context<Self>) {
        if self.probe_left == 0 || self.probe_scheduled {
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
                        view.advance_probe(cx);
                    });
                })
                .ok();
            })
            .detach();
    }

    fn ensure_caret_visible(&mut self) {
        let Some(bounds) = self.last_bounds else {
            return;
        };
        let line = self.buffer.position_at(self.caret()).line;
        let scroll = self.scroll_y();
        let top = px(line as f32 * LINE_HEIGHT);
        let bottom = top + px(LINE_HEIGHT);
        let viewport = bounds.size.height;
        if top < scroll {
            self.scroll.scroll_to_item(line, ScrollStrategy::Top);
        } else if bottom > scroll + viewport {
            self.scroll.scroll_to_item(line, ScrollStrategy::Bottom);
        }
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

    fn move_vert(&mut self, delta: i64, extend: bool) {
        let target = self.buffer.vertical(self.caret(), delta);
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
        self.jump(1);
        cx.notify();
    }

    fn prev_scene(&mut self, _: &PrevScene, _window: &mut Window, cx: &mut Context<Self>) {
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

    fn on_up(&mut self, _: &MoveUp, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_vert(-1, false);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_down(&mut self, _: &MoveDown, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_vert(1, false);
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

    fn on_select_up(&mut self, _: &SelectUp, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_vert(-1, true);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_select_down(&mut self, _: &SelectDown, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_vert(1, true);
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

    fn status_text(&self) -> String {
        let metrics = self.metrics.borrow();
        let (_, p50, p95, max) = Metrics::stats(&metrics.frame_gaps_ms);
        let caret = self.buffer.position_at(self.caret());
        format!(
            "{} · L{}:C{} · ediciones {} · tecla→frame {} · {} líneas · {} escenas · frames {} · gap ms p50 {:.2} / p95 {:.2} / max {:.2} · probe {}",
            metrics.fixture,
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
        )
    }

    /// One row of the virtualized list. Rows carrying caret/selection/IME
    /// state are split into positioned segments; everything else stays a
    /// single text child (the slice-1 fast path).
    fn row(&self, ix: usize) -> gpui::Div {
        let text = &self.buffer.lines()[ix];
        let mut row = div()
            .h(px(LINE_HEIGHT))
            .px_2()
            .flex()
            .items_center()
            .whitespace_nowrap()
            .font_family(FONT_FAMILY)
            .text_size(px(TEXT_SIZE))
            .text_color(line_color(text));

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

        if selection_here.is_none() && marked_here.is_none() && !caret_here {
            return row.child(text.clone());
        }

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
            let mut segment = div().whitespace_nowrap();
            if selected {
                segment = segment.bg(rgba(SELECTION_BG));
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

        format!(
            concat!(
                "{{\"kind\":\"spike-a-buffer-dump\",",
                "\"lines\":{},\"doc_len\":{},\"edits\":{},",
                "\"caret_line\":{},\"caret_col\":{},\"selection\":[{},{}],",
                "\"scenes\":{},\"undo\":{},\"redo\":{},\"around\":\"{}\"}}"
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
        self.marked = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or(self.marked.clone())
            .unwrap_or_else(|| self.selected.clone());
        let text = normalize_newlines(new_text);
        let range = self.clamp(range);
        let inserted = self.apply_edit(range.clone(), &text);
        self.marked = (!text.is_empty()).then_some(inserted.clone());
        self.selected = new_selected_range_utf16
            .as_ref()
            .map(|utf16| {
                let local = self.clamp(self.range_from_utf16(utf16));
                (range.start + local.start)..(range.start + local.end)
            })
            .unwrap_or_else(|| inserted.end..inserted.end);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.clamp(self.range_from_utf16(&range_utf16));
        let start = self.buffer.position_at(range.start);
        let end = self.buffer.position_at(range.end);
        let scroll = self.scroll_y();
        let x_start = x_for_index(window, &self.buffer.lines()[start.line], start.col);
        let x_end = x_for_index(window, &self.buffer.lines()[end.line], end.col);
        let top = bounds.top() + px(start.line as f32 * LINE_HEIGHT) - scroll;
        let bottom = bounds.top() + px(end.line as f32 * LINE_HEIGHT + LINE_HEIGHT) - scroll;
        Some(Bounds::from_corners(
            point(bounds.left() + x_start, top),
            point(bounds.left() + x_end, bottom),
        ))
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        let scroll = self.scroll_y();
        let line = ((position.y - bounds.top() + scroll) / px(LINE_HEIGHT))
            .floor()
            .clamp(0.0, (self.buffer.line_count() - 1) as f32) as usize;
        let text = &self.buffer.lines()[line];
        let x = position.x - bounds.left() - px(ROW_PADDING_X);
        let x = if x < Pixels::ZERO { Pixels::ZERO } else { x };
        let col = shape_line(window, text)
            .index_for_x(x)
            .unwrap_or(0)
            .min(text.len());
        let offset = self.buffer.offset_at(Position { line, col });
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

        let line_count = self.buffer.line_count();
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
            .child(
                div()
                    .flex_1()
                    .relative()
                    .child(
                        uniform_list(
                            "lines",
                            line_count,
                            cx.processor(|this, range: Range<usize>, _window, _cx| {
                                range.map(|ix| this.row(ix)).collect::<Vec<_>>()
                            }),
                        )
                        .size_full(),
                    )
                    // Invisible overlay that registers the platform input
                    // handler for this frame: typing and IME flow into
                    // `EntityInputHandler` while the root has focus.
                    .child(
                        canvas(
                            {
                                let entity = entity.clone();
                                move |bounds: Bounds<Pixels>, _window: &mut Window, cx: &mut App| {
                                    entity.update(cx, |view, _| view.last_bounds = Some(bounds));
                                }
                            },
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
            )
    }
}

struct Args {
    fixture: String,
    steps: usize,
    report_after_ms: Option<u64>,
    dump_after_ms: Option<u64>,
}

fn parse_args() -> Result<Args, String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut fixture = "fixtures/screenplay-1000.fountain".to_string();
    let mut steps = 0usize;
    let mut report_after_ms = None;
    let mut dump_after_ms = None;

    let mut i = 0;
    while i < args.len() {
        let key = args[i].as_str();
        let value = args.get(i + 1).ok_or(format!("{key} requiere un valor"))?;
        match key {
            "--fixture" => fixture = value.clone(),
            "--steps" => {
                steps = value
                    .parse()
                    .map_err(|_| format!("--steps inválido: {value}"))?
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

    Ok(Args {
        fixture,
        steps,
        report_after_ms,
        dump_after_ms,
    })
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let started = Instant::now();

    let content = fs::read_to_string(&args.fixture)
        .map_err(|e| format!("no se pudo leer {}: {e}", args.fixture))?;
    let fixture_read_ms = duration_ms(started.elapsed());

    let scene_count = content
        .lines()
        .filter(|line| is_scene_heading(line))
        .count();
    if scene_count == 0 {
        return Err(format!(
            "no se detectaron escenas en {} (¿es un Fountain?)",
            args.fixture
        ));
    }

    let metrics = Rc::new(RefCell::new(Metrics::new(
        started,
        &args.fixture,
        fixture_read_ms,
        content.lines().count(),
        scene_count,
        args.steps,
    )));

    let steps = args.steps;
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

        let entity = cx.new(|cx| Editor {
            buffer: Buffer::new(&content),
            selected: 0..0,
            reversed: false,
            marked: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            scene_lines: Vec::new(),
            scenes_dirty: true,
            current_scene: 0,
            scroll: UniformListScrollHandle::new(),
            focus_handle: cx.focus_handle(),
            last_bounds: None,
            metrics: metrics.clone(),
            probe_left: steps,
            probe_cursor: 1,
            probe_scheduled: false,
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
                "uso: spike-editor [--fixture <ruta>] [--steps <n>] [--report-after-ms <ms>] [--dump-after-ms <ms>]"
            );
            ExitCode::FAILURE
        }
    }
}
