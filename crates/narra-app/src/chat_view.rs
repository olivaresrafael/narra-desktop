//! Chat workspace state (M12, D-035).
//!
//! Pure reducer for the right-hand chat: no GPUI, no IO. The core
//! [`narra_core::agent::ChatTask`] produces events; this state folds them into
//! messages, streaming text, sources and proposal cards. The app renders it
//! and the future CLI can reuse the same task contract.
//!
//! Modes are enforced in the core (Plan/Review get no tools; Build only gets
//! proposal tools). This state never writes canonical files.

use narra_core::agent::{
    AgentError, ChatActivity, ChatActivityKind, ChatAttachment, ChatMode, ChatProposalRef,
    ChatThread, ContextSource, TaskEvent, TaskOutput,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatStatus {
    Idle,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl ChatStatus {
    pub fn key(self) -> &'static str {
        match self {
            ChatStatus::Idle => "idle",
            ChatStatus::Running => "running",
            ChatStatus::Completed => "completed",
            ChatStatus::Failed => "failed",
            ChatStatus::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalCardStatus {
    Ready,
    Applied,
    Rejected,
    Failed,
}

/// One proposal card inside the conversation. Diff rows are loaded lazily by
/// the app from the persisted record, so the reducer stays IO-free.
#[derive(Debug, Clone)]
pub struct ChatProposalView {
    pub id: String,
    pub target: String,
    pub hunks: usize,
    /// True when the proposal creates a new file instead of editing one.
    pub is_new: bool,
    pub diff_rows: Vec<(char, String)>,
    pub selected: Vec<bool>,
    pub cursor: usize,
    pub armed: bool,
    pub status: ProposalCardStatus,
    pub error: String,
}

impl ChatProposalView {
    pub fn new(id: String, target: String, hunks: usize) -> Self {
        Self {
            id,
            target,
            hunks,
            is_new: false,
            diff_rows: Vec::new(),
            selected: vec![true; hunks],
            cursor: 0,
            armed: false,
            status: ProposalCardStatus::Ready,
            error: String::new(),
        }
    }

    pub fn from_ref(reference: &ChatProposalRef) -> Self {
        Self::new(
            reference.id.clone(),
            reference.target.clone(),
            reference.hunks,
        )
    }

    pub fn all_selected(&self) -> bool {
        !self.selected.is_empty() && self.selected.iter().all(|selected| *selected)
    }

    pub fn selected_hunks(&self) -> Vec<usize> {
        self.selected
            .iter()
            .enumerate()
            .filter(|(_, selected)| **selected)
            .map(|(index, _)| index)
            .collect()
    }

    pub fn toggle(&mut self) {
        if let Some(selected) = self.selected.get_mut(self.cursor) {
            *selected = !*selected;
            self.armed = false;
        }
    }

    pub fn move_cursor(&mut self, delta: isize) {
        let max = self.hunks.saturating_sub(1) as isize;
        self.cursor = (self.cursor as isize + delta).clamp(0, max) as usize;
    }

    pub fn status_key(&self) -> &'static str {
        match self.status {
            ProposalCardStatus::Ready => "ready",
            ProposalCardStatus::Applied => "applied",
            ProposalCardStatus::Rejected => "rejected",
            ProposalCardStatus::Failed => "failed",
        }
    }
}

/// The whole chat panel state.
#[derive(Debug)]
pub struct ChatUiState {
    pub thread: ChatThread,
    pub input: String,
    pub focused: bool,
    pub status: ChatStatus,
    pub stage: String,
    pub error: String,
    pub sources: Vec<narra_core::agent::ContextSource>,
    pub streaming: String,
    pub proposals: Vec<ChatProposalView>,
    pub attachments: Vec<ChatAttachment>,
    /// Token usage accumulated across this chat session, when the provider
    /// reports it. Cost is calculated by the app from optional settings.
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub usage_reported: bool,
    /// Estimated cost for the persisted thread, in micro-USD.
    pub cost_micros: Option<u64>,
}

impl ChatUiState {
    pub fn new(mode: ChatMode) -> Self {
        Self {
            thread: ChatThread::new(mode),
            input: String::new(),
            focused: false,
            status: ChatStatus::Idle,
            stage: String::new(),
            error: String::new(),
            sources: Vec::new(),
            streaming: String::new(),
            proposals: Vec::new(),
            attachments: Vec::new(),
            input_tokens: 0,
            output_tokens: 0,
            usage_reported: false,
            cost_micros: None,
        }
    }

    pub fn from_thread(thread: ChatThread) -> Self {
        let input_tokens = thread.usage.input_tokens;
        let output_tokens = thread.usage.output_tokens;
        let cost_micros = thread.usage.cost_micros;
        let proposals = thread
            .proposals
            .iter()
            .map(ChatProposalView::from_ref)
            .collect();
        Self {
            thread,
            input: String::new(),
            focused: false,
            status: ChatStatus::Idle,
            stage: String::new(),
            error: String::new(),
            sources: Vec::new(),
            streaming: String::new(),
            proposals,
            attachments: Vec::new(),
            input_tokens,
            output_tokens,
            usage_reported: input_tokens > 0 || output_tokens > 0,
            cost_micros,
        }
    }

    pub fn mode(&self) -> ChatMode {
        self.thread.mode
    }

    pub fn set_mode(&mut self, mode: ChatMode) {
        if self.thread.mode != mode {
            self.thread.mode = mode;
            self.thread.touch();
        }
    }

    pub fn set_input(&mut self, input: impl Into<String>) {
        self.input = input.into();
    }

    pub fn push_input(&mut self, text: &str) {
        let cleaned: String = text
            .chars()
            .filter(|character| !character.is_control() || *character == '\n')
            .collect();
        self.input.push_str(&cleaned);
    }

    pub fn backspace(&mut self) {
        if let Some(last) = self.input.chars().last() {
            let len = self.input.len();
            self.input.truncate(len - last.len_utf8());
        }
    }

    pub fn newline(&mut self) {
        self.input.push('\n');
    }

    pub fn can_send(&self) -> bool {
        self.status != ChatStatus::Running && !self.input.trim().is_empty()
    }

    pub fn begin_run(&mut self, instruction: String, attachments: Vec<ChatAttachment>) {
        self.thread.push_user(instruction);
        self.attachments = attachments;
        self.status = ChatStatus::Running;
        self.stage = String::new();
        self.error.clear();
        self.sources.clear();
        self.streaming.clear();
    }

    pub fn apply_event(&mut self, event: &TaskEvent) {
        match event {
            TaskEvent::Started { .. } => {
                self.stage = "started".to_string();
            }
            TaskEvent::Progress { stage, .. } => {
                self.stage = stage.clone();
            }
            TaskEvent::ContextCollected { sources } => {
                let run = self.thread.current_run();
                for source in sources.iter().skip(self.sources.len()) {
                    if source.kind != "tool" {
                        continue;
                    }
                    let (kind, target) = activity_from_source(source);
                    self.thread.push_activity(ChatActivity {
                        run,
                        kind,
                        target,
                        detail: source.detail.clone(),
                        proposal_id: None,
                    });
                }
                self.sources = sources.clone();
            }
            TaskEvent::ModelResponded { .. } => {
                self.stage = "responded".to_string();
            }
            TaskEvent::ModelDelta { text } => {
                self.streaming.push_str(text);
            }
            TaskEvent::ProposalCreated { id, target, hunks } => {
                if !self.proposals.iter().any(|proposal| proposal.id == *id) {
                    self.proposals
                        .push(ChatProposalView::new(id.clone(), target.clone(), *hunks));
                }
                self.thread
                    .push_proposal(id.clone(), target.clone(), *hunks);
                if !self
                    .thread
                    .activities
                    .iter()
                    .any(|activity| activity.proposal_id.as_deref() == Some(id.as_str()))
                {
                    let run = self.thread.current_run();
                    self.thread.push_activity(ChatActivity {
                        run,
                        kind: ChatActivityKind::Edit,
                        target: target.clone(),
                        detail: String::new(),
                        proposal_id: Some(id.clone()),
                    });
                }
            }
            TaskEvent::ToolExecuted {
                tool,
                allowed,
                outcome,
                ..
            } => {
                self.stage = format!("tool:{tool}");
                if !allowed || outcome.starts_with("Failed") {
                    let run = self.thread.current_run();
                    self.thread.push_activity(ChatActivity {
                        run,
                        kind: ChatActivityKind::Error,
                        target: tool.clone(),
                        detail: outcome.clone(),
                        proposal_id: None,
                    });
                }
            }
            TaskEvent::Completed { .. } => {
                self.stage.clear();
            }
            TaskEvent::Failed { error } => {
                self.status = ChatStatus::Failed;
                self.error = error.clone();
                self.stage.clear();
            }
            TaskEvent::Cancelled => {
                self.status = ChatStatus::Cancelled;
                self.error = "chat cancelled".to_string();
                self.stage.clear();
            }
        }
    }

    /// Fold the terminal result. On success the assistant text is appended
    /// once (streaming is transient) and proposal references are persisted in
    /// the thread.
    /// Fold the terminal result and, when manual prices are configured,
    /// accumulate an estimated cost in the persisted thread (micro-USD).
    pub fn finish_with_prices(
        &mut self,
        result: Result<TaskOutput, AgentError>,
        input_price_per_million: f64,
        output_price_per_million: f64,
    ) {
        match result {
            Ok(output) => {
                let text = output.data["final_text"]
                    .as_str()
                    .unwrap_or(output.summary.as_str())
                    .to_string();
                if !text.trim().is_empty() {
                    self.thread.push_assistant(text);
                }
                if let Some(proposals) = output.data["proposals"].as_array() {
                    for proposal in proposals {
                        let (Some(id), Some(target)) = (
                            proposal.get("id").and_then(|value| value.as_str()),
                            proposal.get("target").and_then(|value| value.as_str()),
                        ) else {
                            continue;
                        };
                        let hunks = proposal
                            .get("hunks")
                            .and_then(|value| value.as_u64())
                            .unwrap_or(0) as usize;
                        self.thread.push_proposal(id, target, hunks);
                        if !self.proposals.iter().any(|view| view.id == id) {
                            self.proposals.push(ChatProposalView::new(
                                id.to_string(),
                                target.to_string(),
                                hunks,
                            ));
                        }
                    }
                }
                if output.data["usage_reported"].as_bool().unwrap_or(false) {
                    let run_input = output.data["input_tokens"].as_u64().unwrap_or(0) as u32;
                    let run_output = output.data["output_tokens"].as_u64().unwrap_or(0) as u32;
                    self.input_tokens = self.input_tokens.saturating_add(run_input);
                    self.output_tokens = self.output_tokens.saturating_add(run_output);
                    self.thread.usage.input_tokens =
                        self.thread.usage.input_tokens.saturating_add(run_input);
                    self.thread.usage.output_tokens =
                        self.thread.usage.output_tokens.saturating_add(run_output);
                    if input_price_per_million > 0.0 || output_price_per_million > 0.0 {
                        let cost = (f64::from(run_input) / 1_000_000.0) * input_price_per_million
                            + (f64::from(run_output) / 1_000_000.0) * output_price_per_million;
                        let micros = (cost * 1_000_000.0).round().max(0.0) as u64;
                        self.thread.usage.cost_micros = Some(
                            self.thread
                                .usage
                                .cost_micros
                                .unwrap_or(0)
                                .saturating_add(micros),
                        );
                        self.cost_micros = self.thread.usage.cost_micros;
                    }
                    self.usage_reported = true;
                }
                self.status = ChatStatus::Completed;
                self.streaming.clear();
                self.stage.clear();
                self.attachments.clear();
            }
            Err(AgentError::Cancelled) => {
                self.status = ChatStatus::Cancelled;
                self.error = "chat cancelled".to_string();
                self.streaming.clear();
                self.stage.clear();
                self.attachments.clear();
            }
            Err(error) => {
                self.status = ChatStatus::Failed;
                self.error = error.to_string();
                self.streaming.clear();
                self.stage.clear();
                self.attachments.clear();
            }
        }
    }

    pub fn clear_error(&mut self) {
        self.error.clear();
    }

    pub fn add_attachment(&mut self, attachment: ChatAttachment) {
        let duplicate = self.attachments.iter().any(|existing| {
            existing.kind == attachment.kind
                && existing.location == attachment.location
                && existing.label == attachment.label
        });
        if !duplicate {
            self.attachments.push(attachment);
        }
    }

    pub fn remove_attachment(&mut self, index: usize) {
        if index < self.attachments.len() {
            self.attachments.remove(index);
        }
    }

    pub fn proposal_mut(&mut self, id: &str) -> Option<&mut ChatProposalView> {
        self.proposals.iter_mut().find(|proposal| proposal.id == id)
    }

    pub fn proposal(&self, id: &str) -> Option<&ChatProposalView> {
        self.proposals.iter().find(|proposal| proposal.id == id)
    }

    pub fn dump_json(&self) -> String {
        let messages = self
            .thread
            .messages
            .iter()
            .map(|turn| {
                format!(
                    "{{\"role\":\"{}\",\"content\":\"{}\"}}",
                    match turn.role {
                        narra_core::agent::ChatTurnRole::User => "user",
                        narra_core::agent::ChatTurnRole::Assistant => "assistant",
                    },
                    json_escape(&turn.content)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let proposals = self
            .proposals
            .iter()
            .map(|proposal| {
                format!(
                    "{{\"id\":\"{}\",\"target\":\"{}\",\"hunks\":{},\"new\":{},\"status\":\"{}\",\"armed\":{},\"selected\":{}}}",
                    json_escape(&proposal.id),
                    json_escape(&proposal.target),
                    proposal.hunks,
                    proposal.is_new,
                    proposal.status_key(),
                    proposal.armed,
                    proposal.selected_hunks().len()
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let activities = self
            .thread
            .activities
            .iter()
            .map(|activity| {
                format!(
                    "{{\"run\":{},\"kind\":\"{}\",\"target\":\"{}\",\"proposal\":\"{}\"}}",
                    activity.run,
                    activity.kind.key(),
                    json_escape(&activity.target),
                    json_escape(activity.proposal_id.as_deref().unwrap_or(""))
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"thread\":\"{}\",\"title\":\"{}\",\"mode\":\"{}\",\"status\":\"{}\",\
             \"input\":\"{}\",\"focused\":{},\"streaming\":\"{}\",\"error\":\"{}\",\
             \"sources\":{},\"attachments\":{},\"usage\":{{\"input\":{},\"output\":{},\"reported\":{},\"cost_micros\":{}}},\
             \"activities\":[{}],\"messages\":[{}],\"proposals\":[{}]}}",
            json_escape(&self.thread.id),
            json_escape(&self.thread.title),
            self.thread.mode.key(),
            self.status.key(),
            json_escape(&self.input),
            self.focused,
            json_escape(&self.streaming),
            json_escape(&self.error),
            self.sources.len(),
            self.attachments.len(),
            self.input_tokens,
            self.output_tokens,
            self.usage_reported,
            self.cost_micros
                .map(|value| value.to_string())
                .unwrap_or_else(|| "null".to_string()),
            activities,
            messages,
            proposals
        )
    }
}

fn activity_from_source(source: &ContextSource) -> (ChatActivityKind, String) {
    let location = source.location.as_str();
    if let Some(query) = location.strip_prefix("búsqueda: ") {
        (ChatActivityKind::Search, query.to_string())
    } else if location == "project" {
        (ChatActivityKind::List, "documents".to_string())
    } else if let Some(skill) = location.strip_prefix("skill: ") {
        (ChatActivityKind::Skill, skill.to_string())
    } else if let Some(scene) = location.strip_prefix("escena ") {
        (ChatActivityKind::Scene, scene.to_string())
    } else {
        (ChatActivityKind::Read, location.to_string())
    }
}

fn json_escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use narra_core::agent::{ContextSource, TaskExit};

    fn output(text: &str, proposals: serde_json::Value) -> TaskOutput {
        TaskOutput {
            exit: TaskExit::Success,
            summary: text.to_string(),
            data: serde_json::json!({
                "final_text": text,
                "proposals": proposals,
            }),
            sources: Vec::new(),
        }
    }

    #[test]
    fn finish_accumulates_provider_usage_when_reported() {
        let mut chat = ChatUiState::new(ChatMode::Plan);
        chat.begin_run("hola".to_string(), Vec::new());
        let mut task = output("ok", serde_json::json!([]));
        task.data["input_tokens"] = serde_json::json!(12);
        task.data["output_tokens"] = serde_json::json!(7);
        task.data["usage_reported"] = serde_json::json!(true);
        chat.finish_with_prices(Ok(task), 0.0, 0.0);
        assert_eq!(chat.input_tokens, 12);
        assert_eq!(chat.output_tokens, 7);
        assert!(chat.usage_reported);
    }

    #[test]
    fn finish_persists_usage_and_estimated_cost_in_the_thread() {
        let mut chat = ChatUiState::new(ChatMode::Plan);
        chat.begin_run("hola".to_string(), Vec::new());
        let mut task = output("ok", serde_json::json!([]));
        task.data["input_tokens"] = serde_json::json!(12);
        task.data["output_tokens"] = serde_json::json!(7);
        task.data["usage_reported"] = serde_json::json!(true);
        chat.finish_with_prices(Ok(task), 1000.0, 2000.0);
        assert_eq!(chat.thread.usage.input_tokens, 12);
        assert_eq!(chat.thread.usage.output_tokens, 7);
        assert_eq!(chat.thread.usage.cost_micros, Some(26_000));
        assert_eq!(chat.cost_micros, Some(26_000));

        let restored = ChatUiState::from_thread(chat.thread.clone());
        assert_eq!(restored.input_tokens, 12);
        assert_eq!(restored.output_tokens, 7);
        assert_eq!(restored.cost_micros, Some(26_000));
    }

    #[test]
    fn streaming_events_are_transient_and_final_text_is_appended_once() {
        let mut chat = ChatUiState::new(ChatMode::Plan);
        chat.set_input("hola");
        chat.begin_run("hola".to_string(), Vec::new());
        assert_eq!(chat.status, ChatStatus::Running);
        chat.apply_event(&TaskEvent::ModelDelta {
            text: "par".to_string(),
        });
        chat.apply_event(&TaskEvent::ModelDelta {
            text: "cial".to_string(),
        });
        assert_eq!(chat.streaming, "parcial");
        chat.finish_with_prices(Ok(output("parcial", serde_json::json!([]))), 0.0, 0.0);
        assert_eq!(chat.status, ChatStatus::Completed);
        assert!(chat.streaming.is_empty());
        assert_eq!(chat.thread.messages.len(), 2);
        assert_eq!(
            chat.thread
                .messages
                .last()
                .map(|turn| turn.content.as_str()),
            Some("parcial")
        );
    }

    #[test]
    fn proposal_events_persist_refs_and_build_cards() {
        let mut chat = ChatUiState::new(ChatMode::Build);
        chat.begin_run("reescribe".to_string(), Vec::new());
        chat.apply_event(&TaskEvent::ProposalCreated {
            id: "p1".to_string(),
            target: "screenplay/main.fountain".to_string(),
            hunks: 2,
        });
        assert_eq!(chat.proposals.len(), 1);
        assert_eq!(chat.proposals[0].selected, vec![true, true]);
        assert_eq!(chat.thread.proposals.len(), 1);
        chat.finish_with_prices(
            Ok(output(
                "lista",
                serde_json::json!([{"id":"p1","target":"screenplay/main.fountain","hunks":2}]),
            )),
            0.0,
            0.0,
        );
        assert_eq!(chat.thread.proposals.len(), 1, "deduplicated");
        assert_eq!(chat.thread.messages.len(), 2);
    }

    #[test]
    fn cancelled_and_failed_are_distinct_and_clear_streaming() {
        let mut chat = ChatUiState::new(ChatMode::Review);
        chat.begin_run("revisa".to_string(), Vec::new());
        chat.apply_event(&TaskEvent::ModelDelta {
            text: "texto".to_string(),
        });
        chat.finish_with_prices(Err(AgentError::Cancelled), 0.0, 0.0);
        assert_eq!(chat.status, ChatStatus::Cancelled);
        assert!(chat.streaming.is_empty());

        chat.begin_run("otra".to_string(), Vec::new());
        chat.finish_with_prices(Err(AgentError::Adapter("sin red".to_string())), 0.0, 0.0);
        assert_eq!(chat.status, ChatStatus::Failed);
        assert!(chat.error.contains("sin red"));
    }

    #[test]
    fn attachments_are_deduplicated_and_context_sources_are_kept() {
        let mut chat = ChatUiState::new(ChatMode::Plan);
        let attachment = ChatAttachment::new("scene", "Escena 1", "main.fountain", "INT.");
        chat.add_attachment(attachment.clone());
        chat.add_attachment(attachment);
        assert_eq!(chat.attachments.len(), 1);
        chat.apply_event(&TaskEvent::ContextCollected {
            sources: vec![ContextSource {
                kind: "scene".to_string(),
                location: "main.fountain".to_string(),
                detail: "8 líneas".to_string(),
            }],
        });
        assert_eq!(chat.sources.len(), 1);
    }

    #[test]
    fn tool_sources_and_proposals_become_ordered_inline_activities() {
        let mut chat = ChatUiState::new(ChatMode::Build);
        chat.begin_run("mejora la ficha".to_string(), Vec::new());
        chat.apply_event(&TaskEvent::ContextCollected {
            sources: vec![
                ContextSource {
                    kind: "manifest".to_string(),
                    location: "project.yaml".to_string(),
                    detail: "manifest".to_string(),
                },
                ContextSource {
                    kind: "tool".to_string(),
                    location: "bible/characters/marta.md".to_string(),
                    detail: "documento leído por la herramienta".to_string(),
                },
            ],
        });
        chat.apply_event(&TaskEvent::ContextCollected {
            sources: vec![
                ContextSource {
                    kind: "manifest".to_string(),
                    location: "project.yaml".to_string(),
                    detail: "manifest".to_string(),
                },
                ContextSource {
                    kind: "tool".to_string(),
                    location: "bible/characters/marta.md".to_string(),
                    detail: "documento leído por la herramienta".to_string(),
                },
                ContextSource {
                    kind: "tool".to_string(),
                    location: "búsqueda: faro".to_string(),
                    detail: "resultados".to_string(),
                },
            ],
        });
        chat.apply_event(&TaskEvent::ProposalCreated {
            id: "p1".to_string(),
            target: "bible/characters/marta.md".to_string(),
            hunks: 1,
        });

        let kinds: Vec<ChatActivityKind> = chat
            .thread
            .activities
            .iter()
            .map(|activity| activity.kind)
            .collect();
        assert_eq!(
            kinds,
            vec![
                ChatActivityKind::Read,
                ChatActivityKind::Search,
                ChatActivityKind::Edit
            ]
        );
        assert_eq!(chat.thread.activities[0].run, 0);
        assert_eq!(chat.thread.activities[2].proposal_id.as_deref(), Some("p1"));
        assert_eq!(chat.thread.activities[1].target, "faro");
    }

    #[test]
    fn thread_round_trip_preserves_mode_messages_and_proposals() {
        let mut chat = ChatUiState::new(ChatMode::Build);
        chat.begin_run("hola".to_string(), Vec::new());
        chat.finish_with_prices(
            Ok(output(
                "respuesta",
                serde_json::json!([{"id":"p1","target":"x","hunks":1}]),
            )),
            0.0,
            0.0,
        );
        let restored = ChatUiState::from_thread(chat.thread.clone());
        assert_eq!(restored.mode(), ChatMode::Build);
        assert_eq!(restored.thread.messages.len(), 2);
        assert_eq!(restored.proposals.len(), 1);
        assert_eq!(restored.proposals[0].id, "p1");
    }
}
