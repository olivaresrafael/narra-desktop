//! Agent review panel state (M6 slice 3): a pure reducer from [`TaskEvent`]s
//! and task results to what the panel shows — no GPUI, no window, no IO.
//! The UI layer only renders this state and forwards messages, so the
//! behavior under test is exactly the behavior that ships (`AGENTS.md`:
//! core services stay independent of UI framework state).

use std::time::Instant;

use narra_core::agent::{AgentError, ContextSource, TaskEvent, TaskOutput};

/// Review-panel status. `Idle` = no review has been requested this session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    Idle,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Messages flowing from the background task thread into the panel state.
pub enum AgentMessage {
    Event(TaskEvent),
    Done(Result<TaskOutput, AgentError>),
}

/// Scroll by `delta` lines, clamped to the content: a view never shows a
/// negative offset nor scrolls past the end (delta may overshoot either
/// way on purpose — callers pass raw key steps).
pub fn clamp_scroll(scroll: usize, delta: i64, total_lines: usize, visible: usize) -> usize {
    let max = total_lines.saturating_sub(visible);
    (scroll as i64 + delta).clamp(0, max as i64) as usize
}

/// Panel-visible state of one review task.
#[derive(Debug)]
pub struct AgentState {
    pub status: AgentStatus,
    /// Scene the running/last review targeted (0 = none yet).
    pub scene_number: usize,
    /// Reviewer focus for the running/last review (empty = general review).
    /// Kept in the state (not just the bar) so the panel and the dump can
    /// show what was asked, and so ⌘R reuses the last committed focus.
    pub focus: String,
    /// Last progress stage shown while running (cleared on terminal states).
    pub stage: String,
    /// Context sources the task reported — the user can audit exactly what
    /// informed the review (PRODUCT_SPEC: show the context used).
    pub sources: Vec<ContextSource>,
    /// Review text, full. The panel shows a scrolled window; the dump prints
    /// everything, so verification never depends on pixels.
    pub output: String,
    pub summary: String,
    /// Error text when `Failed` (or the cancel note when `Cancelled`).
    pub error: String,
    /// When the run began, for the elapsed time in status bar and dump.
    pub started: Option<Instant>,
    /// Panel scroll: first review line shown (counted in lines, not pixels).
    pub scroll: usize,
}

impl AgentState {
    pub fn new() -> Self {
        Self {
            status: AgentStatus::Idle,
            scene_number: 0,
            focus: String::new(),
            stage: String::new(),
            sources: Vec::new(),
            output: String::new(),
            summary: String::new(),
            error: String::new(),
            started: None,
            scroll: 0,
        }
    }

    /// Test-only convenience: the binary matches on `status` directly (and
    /// clippy keeps the binary free of unused accessors).
    #[cfg(test)]
    pub fn is_running(&self) -> bool {
        self.status == AgentStatus::Running
    }

    /// The panel renders in every state but `Idle` (a failed review stays
    /// visible with its error until the next run).
    pub fn is_open(&self) -> bool {
        self.status != AgentStatus::Idle
    }

    /// Start a new run: everything from the previous one is cleared. The
    /// focus is an input (what the user asked to review), so it is set here
    /// rather than cleared.
    pub fn begin(&mut self, scene_number: usize, focus: String) {
        self.status = AgentStatus::Running;
        self.scene_number = scene_number;
        self.focus = focus;
        self.stage = String::new();
        self.sources = Vec::new();
        self.output = String::new();
        self.summary = String::new();
        self.error = String::new();
        self.started = Some(Instant::now());
        self.scroll = 0;
    }

    /// Fold one message into the panel state.
    pub fn apply(&mut self, message: AgentMessage) {
        match message {
            AgentMessage::Event(event) => match event {
                TaskEvent::Started { .. } => self.stage = "tarea iniciada".to_string(),
                TaskEvent::Progress { stage, detail } => {
                    self.stage = format!("{stage}: {detail}");
                }
                TaskEvent::ContextCollected { sources } => {
                    self.sources = sources;
                    self.stage = "contexto recogido".to_string();
                }
                TaskEvent::ModelResponded {
                    model,
                    output_chars,
                } => {
                    self.stage = format!("respondió {model} ({output_chars} caracteres)");
                }
                TaskEvent::ModelDelta { text } => self.output.push_str(&text),
                TaskEvent::Completed { summary } => self.summary = summary,
                TaskEvent::Failed { error } => {
                    self.status = AgentStatus::Failed;
                    self.error = error;
                    self.stage.clear();
                }
                TaskEvent::Cancelled => {
                    self.status = AgentStatus::Cancelled;
                    self.error = "revisión cancelada".to_string();
                    self.stage.clear();
                }
                // Proposals and tool calls belong to other tasks (scene_edit,
                // tool_loop); this panel reviews scenes and ignores them.
                TaskEvent::ProposalCreated { .. } | TaskEvent::ToolExecuted { .. } => {}
            },
            AgentMessage::Done(result) => match result {
                Ok(output) => {
                    self.status = AgentStatus::Completed;
                    if self.sources.is_empty() {
                        self.sources = output.sources;
                    }
                    if !output.summary.is_empty() {
                        self.summary = output.summary;
                    }
                    if let Some(review) = output.data["review"].as_str() {
                        self.output = review.to_string();
                    }
                    self.stage.clear();
                }
                Err(AgentError::Cancelled) => {
                    self.status = AgentStatus::Cancelled;
                    self.error = "revisión cancelada".to_string();
                    self.stage.clear();
                }
                Err(error) => {
                    self.status = AgentStatus::Failed;
                    self.error = error.to_string();
                    self.stage.clear();
                }
            },
        }
    }

    /// Scroll by `delta` lines, clamped to the content (see
    /// [`clamp_scroll`]): the panel never shows a negative offset nor
    /// scrolls past the end.
    pub fn scroll_lines(&mut self, delta: i64, total_lines: usize, visible: usize) {
        self.scroll = clamp_scroll(self.scroll, delta, total_lines, visible);
    }

    /// Elapsed milliseconds since the run began, or `None` before any run.
    pub fn elapsed_ms(&self) -> Option<u128> {
        self.started.map(|started| started.elapsed().as_millis())
    }

    /// Stable machine-readable status for the dump (English keys; the
    /// rendered labels are localizable separately).
    pub fn status_key(&self) -> &'static str {
        match self.status {
            AgentStatus::Idle => "idle",
            AgentStatus::Running => "running",
            AgentStatus::Completed => "completed",
            AgentStatus::Failed => "failed",
            AgentStatus::Cancelled => "cancelled",
        }
    }
}

// --- Proposal surface (M6 slice 4) ------------------------------------------

/// Lifecycle of the proposal the workspace is working on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposeStatus {
    Idle,
    /// `scene_edit` is running on a worker thread.
    Running,
    /// Proposal persisted, ready for review (apply/reject enabled).
    Ready,
    /// Applied to the canonical file through the core (confirmed twice).
    Applied,
    /// Rejected: the record says so, no canonical file was touched.
    Rejected,
    Failed,
    Cancelled,
}

/// What the proposal worker thread sends back: the task output plus the
/// diff rows pre-rendered from the persisted record (so the UI never
/// recomputes diffs and the panel shows exactly what would be applied).
pub struct ProposeResult {
    pub output: TaskOutput,
    pub diff_rows: Vec<(char, String)>,
}

/// Messages flowing from the proposal worker thread into the view state.
pub enum ProposeMessage {
    Event(TaskEvent),
    Done(Result<ProposeResult, AgentError>),
}

/// Panel-visible state of the proposal flow.
#[derive(Debug)]
pub struct ProposeView {
    pub status: ProposeStatus,
    pub instruction: String,
    pub scene_number: usize,
    /// Persisted proposal id (empty until `ProposalCreated`/`Done`).
    pub id: String,
    pub target: String,
    pub hunks: usize,
    /// Unified-diff-style rows: ('-', removed), ('+', added), ('#', header).
    pub diff_rows: Vec<(char, String)>,
    pub stage: String,
    pub summary: String,
    pub error: String,
    /// Two-step apply confirmation: the first cmd-shift-enter arms, the
    /// second one applies (writing the canonical file is never one keystroke).
    pub armed_apply: bool,
    pub scroll: usize,
    pub started: Option<Instant>,
}

impl ProposeView {
    pub fn new() -> Self {
        Self {
            status: ProposeStatus::Idle,
            instruction: String::new(),
            scene_number: 0,
            id: String::new(),
            target: String::new(),
            hunks: 0,
            diff_rows: Vec::new(),
            stage: String::new(),
            summary: String::new(),
            error: String::new(),
            armed_apply: false,
            scroll: 0,
            started: None,
        }
    }

    /// Test-only convenience: the binary matches on `status` directly.
    #[cfg(test)]
    pub fn is_running(&self) -> bool {
        self.status == ProposeStatus::Running
    }

    pub fn is_open(&self) -> bool {
        self.status != ProposeStatus::Idle
    }

    /// Whether apply is offered (a reviewable proposal, not yet decided).
    pub fn can_apply(&self) -> bool {
        self.status == ProposeStatus::Ready
    }

    /// Start a new run: everything from the previous proposal is cleared.
    pub fn begin(&mut self, scene_number: usize, instruction: String) {
        self.status = ProposeStatus::Running;
        self.instruction = instruction;
        self.scene_number = scene_number;
        self.id = String::new();
        self.target = String::new();
        self.hunks = 0;
        self.diff_rows = Vec::new();
        self.stage = String::new();
        self.summary = String::new();
        self.error = String::new();
        self.armed_apply = false;
        self.scroll = 0;
        self.started = Some(Instant::now());
    }

    /// Fold one message into the view state.
    pub fn apply(&mut self, message: ProposeMessage) {
        match message {
            ProposeMessage::Event(event) => match event {
                TaskEvent::Started { .. } => self.stage = "tarea iniciada".to_string(),
                TaskEvent::Progress { stage, detail } => {
                    self.stage = format!("{stage}: {detail}");
                }
                TaskEvent::ModelResponded {
                    model,
                    output_chars,
                } => {
                    self.stage = format!("respondió {model} ({output_chars} caracteres)");
                }
                TaskEvent::ProposalCreated { id, target, hunks } => {
                    self.id = id;
                    self.target = target;
                    self.hunks = hunks;
                    self.stage = "propuesta creada (grabada en .narra/proposals)".to_string();
                }
                TaskEvent::Completed { summary } => self.summary = summary,
                TaskEvent::Failed { error } => {
                    self.status = ProposeStatus::Failed;
                    self.error = error;
                    self.stage.clear();
                }
                TaskEvent::Cancelled => {
                    self.status = ProposeStatus::Cancelled;
                    self.error = "propuesta cancelada".to_string();
                    self.stage.clear();
                }
                // Reviews don't create proposals and the tool loop isn't
                // wired into this surface: nothing else belongs here.
                TaskEvent::ContextCollected { .. }
                | TaskEvent::ModelDelta { .. }
                | TaskEvent::ToolExecuted { .. } => {}
            },
            ProposeMessage::Done(result) => match result {
                Ok(result) => {
                    self.status = ProposeStatus::Ready;
                    if self.id.is_empty()
                        && let Some(id) = result.output.data["proposal_id"].as_str()
                    {
                        self.id = id.to_string();
                    }
                    if self.target.is_empty()
                        && let Some(target) = result.output.data["target"].as_str()
                    {
                        self.target = target.to_string();
                    }
                    if self.hunks == 0 {
                        self.hunks = result.output.data["hunks"].as_u64().unwrap_or(0) as usize;
                    }
                    self.diff_rows = result.diff_rows;
                    if !result.output.summary.is_empty() {
                        self.summary = result.output.summary;
                    }
                    self.stage.clear();
                }
                Err(AgentError::Cancelled) => {
                    self.status = ProposeStatus::Cancelled;
                    self.error = "propuesta cancelada".to_string();
                    self.stage.clear();
                }
                Err(error) => {
                    self.status = ProposeStatus::Failed;
                    self.error = error.to_string();
                    self.stage.clear();
                }
            },
        }
    }

    pub fn scroll_lines(&mut self, delta: i64, total_lines: usize, visible: usize) {
        self.scroll = clamp_scroll(self.scroll, delta, total_lines, visible);
    }

    /// Stable machine-readable status for the dump.
    pub fn status_key(&self) -> &'static str {
        match self.status {
            ProposeStatus::Idle => "idle",
            ProposeStatus::Running => "running",
            ProposeStatus::Ready => "ready",
            ProposeStatus::Applied => "applied",
            ProposeStatus::Rejected => "rejected",
            ProposeStatus::Failed => "failed",
            ProposeStatus::Cancelled => "cancelled",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use narra_core::agent::TaskExit;

    fn source(kind: &str, location: &str) -> ContextSource {
        ContextSource {
            kind: kind.to_string(),
            location: location.to_string(),
            detail: "detalle".to_string(),
        }
    }

    fn task_output(review: &str) -> TaskOutput {
        TaskOutput {
            exit: TaskExit::Success,
            summary: "resumen".to_string(),
            data: serde_json::json!({ "review": review }),
            sources: vec![source("scene", "screenplay/main.fountain:L1-L3")],
        }
    }

    #[test]
    fn flow_events_then_done_completes_with_output_and_sources() {
        let mut state = AgentState::new();
        state.begin(2, String::new());
        assert_eq!(state.status, AgentStatus::Running);
        assert_eq!(state.scene_number, 2);

        state.apply(AgentMessage::Event(TaskEvent::Started {
            task: "scene_review".to_string(),
            project: "/tmp/p".to_string(),
        }));
        state.apply(AgentMessage::Event(TaskEvent::Progress {
            stage: "call_model".to_string(),
            detail: "Revisando".to_string(),
        }));
        assert!(state.is_running());
        assert!(state.stage.contains("call_model"));

        let sources = vec![source("bible", "bible/elena.md")];
        state.apply(AgentMessage::Event(TaskEvent::ContextCollected {
            sources: sources.clone(),
        }));
        assert_eq!(state.sources, sources);

        state.apply(AgentMessage::Done(Ok(task_output("1) Lectura."))));
        assert_eq!(state.status, AgentStatus::Completed);
        assert_eq!(state.output, "1) Lectura.");
        assert_eq!(state.summary, "resumen");
        assert!(state.stage.is_empty(), "no stale progress on completion");
        assert!(state.elapsed_ms().is_some());
        assert!(state.is_open() && !state.is_running());
    }

    #[test]
    fn cancelled_result_marks_the_state_cancelled() {
        let mut state = AgentState::new();
        state.begin(1, String::new());
        state.apply(AgentMessage::Done(Err(AgentError::Cancelled)));
        assert_eq!(state.status, AgentStatus::Cancelled);
        assert!(!state.error.is_empty());
        assert!(state.stage.is_empty());
    }

    #[test]
    fn adapter_error_marks_the_state_failed_with_the_message() {
        let mut state = AgentState::new();
        state.begin(1, String::new());
        state.apply(AgentMessage::Done(Err(AgentError::Adapter(
            "sin conexión".to_string(),
        ))));
        assert_eq!(state.status, AgentStatus::Failed);
        assert!(state.error.contains("sin conexión"), "{}", state.error);
    }

    #[test]
    fn begin_clears_everything_from_the_previous_run() {
        let mut state = AgentState::new();
        state.begin(1, "diálogos tensos".to_string());
        state.apply(AgentMessage::Event(TaskEvent::ContextCollected {
            sources: vec![source("notes", "notes/decisions.md")],
        }));
        state.apply(AgentMessage::Done(Ok(task_output("review previa"))));
        state.scroll = 4;

        state.begin(3, String::new());
        assert_eq!(state.status, AgentStatus::Running);
        assert_eq!(state.scene_number, 3);
        assert!(state.sources.is_empty());
        assert!(state.output.is_empty());
        assert!(state.error.is_empty());
        assert_eq!(state.scroll, 0);
        // The focus is an input, not run output: a new run without focus
        // replaces the previous one (no stale focus is ever shown).
        assert!(state.focus.is_empty());
    }

    #[test]
    fn begin_sets_the_focus_for_the_run() {
        let mut state = AgentState::new();
        state.begin(2, "coherencia del arco de Marta".to_string());
        assert_eq!(state.focus, "coherencia del arco de Marta");
        assert_eq!(state.scene_number, 2);
        // A general review (no focus) replaces a focused one.
        state.begin(2, String::new());
        assert!(state.focus.is_empty());
    }

    #[test]
    fn scroll_clamps_to_the_content_window() {
        let mut state = AgentState::new();
        // 30 lines, 10 visible: max offset is 20.
        state.scroll_lines(5, 30, 10);
        assert_eq!(state.scroll, 5);
        state.scroll_lines(100, 30, 10);
        assert_eq!(state.scroll, 20);
        state.scroll_lines(-100, 30, 10);
        assert_eq!(state.scroll, 0);
        // Short content: no scrolling at all.
        state.scroll_lines(10, 4, 10);
        assert_eq!(state.scroll, 0);
    }

    #[test]
    fn task_output_without_event_sources_fills_them_from_the_result() {
        let mut state = AgentState::new();
        state.begin(1, String::new());
        // A task that skipped the event (defensive): sources still shown.
        state.apply(AgentMessage::Done(Ok(task_output("r"))));
        assert_eq!(state.sources.len(), 1);
        assert_eq!(state.sources[0].location, "screenplay/main.fountain:L1-L3");
    }

    // --- Proposal surface (M6 slice 4) ------------------------------------

    fn propose_result(diff_rows: Vec<(char, String)>) -> ProposeResult {
        ProposeResult {
            output: TaskOutput {
                exit: TaskExit::Success,
                summary: "Propuesta ab12 lista".to_string(),
                data: serde_json::json!({
                    "proposal_id": "abcd-1234",
                    "target": "screenplay/main.fountain",
                    "hunks": 1,
                }),
                sources: Vec::new(),
            },
            diff_rows,
        }
    }

    #[test]
    fn proposal_flow_reaches_ready_with_diff_and_record_data() {
        let mut view = ProposeView::new();
        view.begin(2, "haz el diálogo más tenso".to_string());
        assert_eq!(view.status, ProposeStatus::Running);
        assert_eq!(view.scene_number, 2);
        assert_eq!(view.instruction, "haz el diálogo más tenso");

        view.apply(ProposeMessage::Event(TaskEvent::ProposalCreated {
            id: "abcd-1234".to_string(),
            target: "screenplay/main.fountain".to_string(),
            hunks: 1,
        }));
        assert!(view.is_running());
        assert_eq!(view.id, "abcd-1234", "id known before the task ends");

        let rows = vec![('#', "@@ L1 @@".to_string()), ('-', "viejo".to_string())];
        view.apply(ProposeMessage::Done(Ok(propose_result(rows.clone()))));
        assert_eq!(view.status, ProposeStatus::Ready);
        assert_eq!(view.id, "abcd-1234");
        assert_eq!(view.target, "screenplay/main.fountain");
        assert_eq!(view.hunks, 1);
        assert_eq!(view.diff_rows, rows);
        assert!(view.can_apply());
        assert!(!view.armed_apply, "apply is never armed implicitly");
        assert!(view.stage.is_empty());
    }

    #[test]
    fn proposal_cancelled_and_failed_are_distinct() {
        let mut view = ProposeView::new();
        view.begin(1, "x".to_string());
        view.apply(ProposeMessage::Done(Err(AgentError::Cancelled)));
        assert_eq!(view.status, ProposeStatus::Cancelled);

        view.begin(1, "x".to_string());
        view.apply(ProposeMessage::Done(Err(AgentError::Adapter(
            "sin modelo".into(),
        ))));
        assert_eq!(view.status, ProposeStatus::Failed);
        assert!(view.error.contains("sin modelo"), "{}", view.error);
        assert!(!view.can_apply(), "a failed run cannot be applied");
    }

    #[test]
    fn begin_clears_the_previous_proposal() {
        let mut view = ProposeView::new();
        view.begin(1, "primera".to_string());
        view.apply(ProposeMessage::Done(Ok(propose_result(vec![(
            '+',
            "n".into(),
        )]))));
        view.armed_apply = true;
        view.scroll = 3;

        view.begin(4, "segunda".to_string());
        assert_eq!(view.status, ProposeStatus::Running);
        assert!(view.id.is_empty());
        assert!(view.diff_rows.is_empty());
        assert!(!view.armed_apply);
        assert_eq!(view.scroll, 0);
        assert_eq!(view.instruction, "segunda");
    }
}
