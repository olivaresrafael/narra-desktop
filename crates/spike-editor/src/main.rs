//! Spike A — slice 1: does GPUI give us a usable editing surface foundation?
//!
//! Questions answered by this binary (TECHNICAL_SPIKES.md, Spike A):
//! - open and render a 1,000-scene / 18k-line Fountain fixture, virtualized;
//! - scroll stability while jumping scene to scene;
//! - cold start on the reference machine (D-019);
//! - keystroke → next frame latency for navigation keystrokes.
//!
//! Explicitly NOT answered yet (slice 2): typing latency with a text input,
//! IME, selection correctness, text wrapping, undo/redo.
//!
//! Modes:
//!   spike-editor --fixture <path>               interactive (cmd-g next scene, cmd-shift-g prev, cmd-q quit)
//!   spike-editor --fixture <path> --steps 60    automated probe: 60 scene jumps, then prints JSON and exits
//!
//! The JSON report is a single line on stdout beginning with `{"kind":"spike-a`.

use std::cell::RefCell;
use std::env;
use std::fs;
use std::process::ExitCode;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    App, Application, Bounds, Context, FocusHandle, Focusable, KeyBinding, Rgba, ScrollStrategy,
    Timer, UniformListScrollHandle, Window, WindowBounds, WindowOptions, actions, div, prelude::*,
    px, rgb, size, uniform_list,
};

actions!(spike_editor, [Quit, NextScene, PrevScene]);

/// Rows are fixed-height so `uniform_list` can virtualize them. Text longer
/// than the viewport is clipped for now; wrapping is slice 2 work (Spike B).
const LINE_HEIGHT: f32 = 18.0;

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

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
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
                "{{\"kind\":\"spike-a-slice1\",",
                "\"fixture\":\"{}\",\"lines\":{},\"scenes\":{},\"probe_steps\":{},",
                "\"fixture_read_ms\":{:.3},\"cold_start_ms\":{:.3},\"rss_mb\":{:.1},\"renders\":{},",
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

struct Editor {
    lines: Vec<String>,
    scene_lines: Vec<usize>,
    current_scene: usize,
    scroll: UniformListScrollHandle,
    focus_handle: FocusHandle,
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
    fn jump(&mut self, direction: i64) {
        if self.scene_lines.is_empty() {
            return;
        }
        let len = self.scene_lines.len() as i64;
        let next = (self.current_scene as i64 + direction).rem_euclid(len);
        self.current_scene = next as usize;
        let line = self.scene_lines[self.current_scene];
        self.scroll.scroll_to_item(line, ScrollStrategy::Top);
    }

    /// One probe step: jump to the next scene and request a frame.
    ///
    /// This runs from a timer callback, NOT from inside `render()`: calling
    /// `cx.notify()` during render only produced 2-3 frames and then stalled
    /// (observed in both debug and release, 2026-10-21).
    fn advance_probe(&mut self, cx: &mut Context<Self>) {
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

    fn next_scene(&mut self, _: &NextScene, _window: &mut Window, cx: &mut Context<Self>) {
        self.jump(1);
        cx.notify();
    }

    fn prev_scene(&mut self, _: &PrevScene, _window: &mut Window, cx: &mut Context<Self>) {
        self.jump(-1);
        cx.notify();
    }

    fn status_text(&self) -> String {
        let metrics = self.metrics.borrow();
        let (_, p50, p95, max) = Metrics::stats(&metrics.frame_gaps_ms);
        format!(
            "{} · {} líneas · {} escenas · frames {} · gap ms p50 {:.2} / p95 {:.2} / max {:.2} · tecla→frame {} · probe {} · cmd-g sig. escena · cmd-q salir",
            metrics.fixture,
            metrics.lines,
            metrics.scenes,
            metrics.renders,
            p50,
            p95,
            max,
            metrics.keystroke_to_frame_ms.len(),
            self.probe_left,
        )
    }
}

impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        self.metrics.borrow_mut().on_render(now);

        // The probe is driven by a timer scheduled from here, never by
        // notifying during render (see `advance_probe`).
        self.schedule_probe_step(window, cx);

        let line_count = self.lines.len();
        let status = self.status_text();

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::next_scene))
            .on_action(cx.listener(Self::prev_scene))
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
                div().flex_1().child(
                    uniform_list(
                        "lines",
                        line_count,
                        cx.processor(|this, range: std::ops::Range<usize>, _window, _cx| {
                            let mut rows = Vec::with_capacity(range.end - range.start);
                            for ix in range {
                                let text = &this.lines[ix];
                                rows.push(
                                    div()
                                        .id(ix)
                                        .h(px(LINE_HEIGHT))
                                        .px_2()
                                        .flex()
                                        .items_center()
                                        .whitespace_nowrap()
                                        .font_family("Menlo")
                                        .text_sm()
                                        .text_color(line_color(text))
                                        .child(text.clone()),
                                );
                            }
                            rows
                        }),
                    )
                    .size_full(),
                ),
            )
    }
}

struct Args {
    fixture: String,
    steps: usize,
    report_after_ms: Option<u64>,
}

fn parse_args() -> Result<Args, String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut fixture = "fixtures/screenplay-1000.fountain".to_string();
    let mut steps = 0usize;
    let mut report_after_ms = None;

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
            other => return Err(format!("argumento desconocido: {other}")),
        }
        i += 2;
    }

    Ok(Args {
        fixture,
        steps,
        report_after_ms,
    })
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let started = Instant::now();

    let content = fs::read_to_string(&args.fixture)
        .map_err(|e| format!("no se pudo leer {}: {e}", args.fixture))?;
    let fixture_read_ms = duration_ms(started.elapsed());

    let lines: Vec<String> = content.lines().map(str::to_string).collect();
    let scene_lines: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| is_scene_heading(line))
        .map(|(ix, _)| ix)
        .collect();
    if scene_lines.is_empty() {
        return Err(format!(
            "no se detectaron escenas en {} (¿es un Fountain?)",
            args.fixture
        ));
    }

    let metrics = Rc::new(RefCell::new(Metrics::new(
        started,
        &args.fixture,
        fixture_read_ms,
        lines.len(),
        scene_lines.len(),
        args.steps,
    )));

    let steps = args.steps;
    let report_after_ms = args.report_after_ms;
    Application::new().run(move |cx: &mut App| {
        cx.activate(true);
        cx.bind_keys([
            KeyBinding::new("cmd-g", NextScene, None),
            KeyBinding::new("cmd-shift-g", PrevScene, None),
            KeyBinding::new("cmd-q", Quit, None),
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

        let bounds = Bounds::centered(None, size(px(1100.0), px(780.0)), cx);
        let window = match cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|cx| Editor {
                    lines,
                    scene_lines,
                    current_scene: 0,
                    scroll: UniformListScrollHandle::new(),
                    focus_handle: cx.focus_handle(),
                    metrics: metrics.clone(),
                    probe_left: steps,
                    probe_cursor: 1,
                    probe_scheduled: false,
                })
            },
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
    });

    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            eprintln!(
                "uso: spike-editor [--fixture <ruta>] [--steps <n>] [--report-after-ms <ms>]"
            );
            ExitCode::FAILURE
        }
    }
}
