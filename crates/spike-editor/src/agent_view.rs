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

/// Panel-visible state of one review task.
#[derive(Debug)]
pub struct AgentState {
    pub status: AgentStatus,
    /// Scene the running/last review targeted (0 = none yet).
    pub scene_number: usize,
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
            stage: String::new(),
            sources: Vec::new(),
            output: String::new(),
            summary: String::new(),
            error: String::new(),
            started: None,
            scroll: 0,
        }
    }

    pub fn is_running(&self) -> bool {
        self.status == AgentStatus::Running
    }

    /// The panel renders in every state but `Idle` (a failed review stays
    /// visible with its error until the next run).
    pub fn is_open(&self) -> bool {
        self.status != AgentStatus::Idle
    }

    /// Start a new run: everything from the previous one is cleared.
    pub fn begin(&mut self, scene_number: usize) {
        self.status = AgentStatus::Running;
        self.scene_number = scene_number;
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

    /// Scroll by `delta` lines, clamped to the content: a view never shows a
    /// negative offset nor scrolls past the end (delta may overshoot either
    /// way on purpose — callers pass raw key steps).
    pub fn scroll_lines(&mut self, delta: i64, total_lines: usize, visible: usize) {
        let max = total_lines.saturating_sub(visible);
        self.scroll = (self.scroll as i64 + delta).clamp(0, max as i64) as usize;
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
        state.begin(2);
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
        state.begin(1);
        state.apply(AgentMessage::Done(Err(AgentError::Cancelled)));
        assert_eq!(state.status, AgentStatus::Cancelled);
        assert!(!state.error.is_empty());
        assert!(state.stage.is_empty());
    }

    #[test]
    fn adapter_error_marks_the_state_failed_with_the_message() {
        let mut state = AgentState::new();
        state.begin(1);
        state.apply(AgentMessage::Done(Err(AgentError::Adapter(
            "sin conexión".to_string(),
        ))));
        assert_eq!(state.status, AgentStatus::Failed);
        assert!(state.error.contains("sin conexión"), "{}", state.error);
    }

    #[test]
    fn begin_clears_everything_from_the_previous_run() {
        let mut state = AgentState::new();
        state.begin(1);
        state.apply(AgentMessage::Event(TaskEvent::ContextCollected {
            sources: vec![source("notes", "notes/decisions.md")],
        }));
        state.apply(AgentMessage::Done(Ok(task_output("review previa"))));
        state.scroll = 4;

        state.begin(3);
        assert_eq!(state.status, AgentStatus::Running);
        assert_eq!(state.scene_number, 3);
        assert!(state.sources.is_empty());
        assert!(state.output.is_empty());
        assert!(state.error.is_empty());
        assert_eq!(state.scroll, 0);
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
        state.begin(1);
        // A task that skipped the event (defensive): sources still shown.
        state.apply(AgentMessage::Done(Ok(task_output("r"))));
        assert_eq!(state.sources.len(), 1);
        assert_eq!(state.sources[0].location, "screenplay/main.fountain:L1-L3");
    }
}
