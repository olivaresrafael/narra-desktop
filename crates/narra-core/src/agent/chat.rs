//! Conversational agent sessions (M12, D-035).
//!
//! A chat is a UI-neutral task plus a small persisted thread. The task
//! enforces the mode contract:
//!
//! - `Plan` and `Review` never receive tools and cannot change the project;
//! - `Build` receives the read/propose tools; `apply_proposal` stays denied,
//!   so canonical files still require an explicit human apply from a
//!   reviewable proposal;
//! - every context item is shown as a [`ContextSource`];
//! - the whole run is cancelable and timeout-bounded.
//!
//! Threads are ordinary JSON under `<project>/.narra/chats/`, workflow state
//! rather than canon. They never contain credentials. The desktop renders
//! them; the future CLI can build the same [`ChatTask`] headless.

use std::fs;
use std::path::Path;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::json;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

use super::adapter::{ModelAdapter, ModelRequest};
use super::proposal;
use super::tools::{
    self, ChatMessage, ChatRole, GrantPolicy, ToolExecutor, ToolOutcome, ToolResult,
};
use super::{AgentError, ContextSource, TaskEvent, TaskExit, TaskOptions, TaskOutput};
use crate::document;
use crate::project::open_project;

/// Chat modes exposed by the composer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChatMode {
    #[default]
    Plan,
    Build,
    Review,
}

impl ChatMode {
    pub const ALL: [ChatMode; 3] = [ChatMode::Plan, ChatMode::Build, ChatMode::Review];

    pub fn key(self) -> &'static str {
        match self {
            ChatMode::Plan => "plan",
            ChatMode::Build => "build",
            ChatMode::Review => "review",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.key() == value)
    }

    /// Tools available to this mode. `Plan`/`Review` are read-only by
    /// construction (no tools), not by prompt wording alone.
    pub fn tools(self) -> Vec<tools::ToolSpec> {
        match self {
            ChatMode::Build => tools::tool_specs(),
            ChatMode::Plan | ChatMode::Review => Vec::new(),
        }
    }

    fn system_prompt(self) -> &'static str {
        match self {
            ChatMode::Plan => {
                "Eres un colaborador de guion en modo PLAN. Trabajas en solo \
                 lectura: analiza, propone ideas, estructura y preguntas de \
                 continuidad, pero no afirmes haber cambiado archivos. \
                 Responde en el idioma del usuario, con claridad y sin \
                 inventar canon del proyecto."
            }
            ChatMode::Build => {
                "Eres un colaborador de guion en modo BUILD. Puedes usar \
                 read_scene para leer escenas y propose_scene_edit para \
                 proponer una reescritura. Una propuesta NO se aplica: queda \
                 para revisión humana. Nunca digas que aplicaste un cambio ni \
                 escribas archivos canónicos. Si falta información, pídela o \
                 lee la escena con la herramienta. Responde en el idioma del \
                 usuario."
            }
            ChatMode::Review => {
                "Eres un editor de guion en modo REVIEW. Haz una crítica \
                 concreta de estructura, ritmo, personajes y continuidad, \
                 apoyándote solo en el contexto entregado. No propongas \
                 archivos ni afirmes haber modificado nada. Responde en el \
                 idioma del usuario."
            }
        }
    }
}

/// One context item explicitly attached by the writer (current scene,
/// selection, document, …). The UI supplies the content; the core caps it and
/// reports the truncation in a source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatAttachment {
    /// Stable kind key: `scene`, `selection`, `document`, `bible`, `note`.
    pub kind: String,
    /// Human-readable label shown as a chip, e.g. `Escena 4`.
    pub label: String,
    /// Project-relative location when applicable.
    pub location: String,
    pub content: String,
}

impl ChatAttachment {
    pub fn new(
        kind: impl Into<String>,
        label: impl Into<String>,
        location: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            kind: kind.into(),
            label: label.into(),
            location: location.into(),
            content: content.into(),
        }
    }
}

/// Persisted conversation role. Tool calls are audited in the task output;
/// the thread stores the human-visible conversation only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatTurnRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatTurn {
    pub role: ChatTurnRole,
    pub content: String,
}

impl ChatTurn {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ChatTurnRole::User,
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: ChatTurnRole::Assistant,
            content: content.into(),
        }
    }
}

/// A proposal created during a chat, persisted so the card survives a
/// restart. The diff itself stays in the proposal record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatProposalRef {
    pub id: String,
    pub target: String,
    pub hunks: usize,
}

/// A persisted chat thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatThread {
    pub schema_version: u32,
    pub id: String,
    pub title: String,
    pub mode: ChatMode,
    pub updated_at: String,
    pub messages: Vec<ChatTurn>,
    #[serde(default)]
    pub proposals: Vec<ChatProposalRef>,
}

pub const CHAT_SCHEMA_VERSION: u32 = 1;
pub const CHATS_DIR: &str = ".narra/chats";
const MAX_TITLE_CHARS: usize = 60;

impl ChatThread {
    pub fn new(mode: ChatMode) -> Self {
        Self {
            schema_version: CHAT_SCHEMA_VERSION,
            id: Uuid::new_v4().to_string(),
            title: String::new(),
            mode,
            updated_at: now_rfc3339(),
            messages: Vec::new(),
            proposals: Vec::new(),
        }
    }

    pub fn push_proposal(
        &mut self,
        id: impl Into<String>,
        target: impl Into<String>,
        hunks: usize,
    ) {
        let id = id.into();
        if !self.proposals.iter().any(|proposal| proposal.id == id) {
            self.proposals.push(ChatProposalRef {
                id,
                target: target.into(),
                hunks,
            });
            self.touch();
        }
    }

    pub fn push_user(&mut self, content: impl Into<String>) {
        let content = content.into();
        if self.title.is_empty() {
            self.title = truncate(content.trim(), MAX_TITLE_CHARS);
        }
        self.messages.push(ChatTurn::user(content));
        self.touch();
    }

    pub fn push_assistant(&mut self, content: impl Into<String>) {
        self.messages.push(ChatTurn::assistant(content));
        self.touch();
    }

    pub fn touch(&mut self) {
        self.updated_at = now_rfc3339();
    }

    fn relative_path(&self) -> Result<String, String> {
        Uuid::parse_str(&self.id).map_err(|_| format!("invalid chat id `{}`", self.id))?;
        Ok(format!("{CHATS_DIR}/{}.json", self.id))
    }

    /// Persist atomically under the project's workflow state. The id is
    /// validated as a UUID, so a crafted id cannot escape the directory.
    pub fn save(&self, project_root: &Path) -> Result<(), String> {
        let path = self.relative_path()?;
        let text = serde_json::to_string_pretty(self)
            .map_err(|error| format!("could not encode chat: {error}"))?;
        document::write_document(project_root, &path, &text).map_err(|error| error.to_string())
    }

    pub fn load(project_root: &Path, id: &str) -> Result<Self, String> {
        Uuid::parse_str(id).map_err(|_| format!("invalid chat id `{id}`"))?;
        let path = format!("{CHATS_DIR}/{id}.json");
        let text =
            document::read_document(project_root, &path).map_err(|error| error.to_string())?;
        let thread: Self =
            serde_json::from_str(&text).map_err(|error| format!("invalid chat record: {error}"))?;
        if thread.schema_version != CHAT_SCHEMA_VERSION {
            return Err(format!(
                "unsupported chat schema_version {} (expected {CHAT_SCHEMA_VERSION})",
                thread.schema_version
            ));
        }
        Ok(thread)
    }

    /// Most recently updated first; invalid records are skipped rather than
    /// making the picker unusable.
    pub fn list(project_root: &Path) -> Vec<Self> {
        let dir = project_root.join(CHATS_DIR);
        let Ok(entries) = fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut threads: Vec<Self> = entries
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
            .filter_map(|entry| fs::read_to_string(entry.path()).ok())
            .filter_map(|text| serde_json::from_str::<Self>(&text).ok())
            .filter(|thread| thread.schema_version == CHAT_SCHEMA_VERSION)
            .collect();
        threads.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        threads
    }
}

struct BuildPrompt {
    system: String,
    prompt: String,
    sources: Vec<ContextSource>,
}

/// Explicit chat input. A fresh task is built for each user turn; prior
/// conversation and attachments are inputs, never hidden global state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatTask {
    pub project_root: String,
    pub mode: ChatMode,
    pub history: Vec<ChatTurn>,
    pub instruction: String,
    pub attachments: Vec<ChatAttachment>,
}

impl ChatTask {
    pub const KIND: &'static str = "chat";

    pub fn new(
        project_root: impl Into<String>,
        mode: ChatMode,
        history: Vec<ChatTurn>,
        instruction: impl Into<String>,
    ) -> Self {
        Self {
            project_root: project_root.into(),
            mode,
            history,
            instruction: instruction.into(),
            attachments: Vec::new(),
        }
    }

    pub fn with_attachments(mut self, attachments: Vec<ChatAttachment>) -> Self {
        self.attachments = attachments;
        self
    }

    fn validate(&self) -> Result<(), AgentError> {
        if self.instruction.trim().is_empty() {
            return Err(AgentError::InvalidInput("instruction must not be empty"));
        }
        Ok(())
    }

    /// Run one turn. `emit` receives streaming deltas in Plan/Review and
    /// tool/proposal events in Build.
    pub fn run(
        &self,
        adapter: &dyn ModelAdapter,
        options: &TaskOptions,
        mut emit: impl FnMut(TaskEvent),
    ) -> Result<TaskOutput, AgentError> {
        let started = Instant::now();
        let root = Path::new(&self.project_root);
        self.validate()?;
        emit(TaskEvent::Started {
            task: Self::KIND.to_string(),
            project: self.project_root.clone(),
        });

        if options.cancel.is_cancelled() {
            return Err(AgentError::Cancelled);
        }
        emit(TaskEvent::Progress {
            stage: "open_project".to_string(),
            detail: "Leyendo project.yaml".to_string(),
        });
        let project = open_project(root)?;
        let manifest = project.manifest();

        let mut sources = vec![ContextSource {
            kind: "manifest".to_string(),
            location: "project.yaml".to_string(),
            detail: format!("Título «{}» e idioma primario", manifest.title),
        }];
        let mut context = String::new();
        for attachment in &self.attachments {
            let chars = attachment.content.chars().count();
            let (text, truncated) = if chars > 8_000 {
                (truncate(&attachment.content, 8_000), true)
            } else {
                (attachment.content.clone(), false)
            };
            sources.push(ContextSource {
                kind: attachment.kind.clone(),
                location: attachment.location.clone(),
                detail: if truncated {
                    format!(
                        "{} — {chars} caracteres (truncados a 8000)",
                        attachment.label
                    )
                } else {
                    format!("{} — {chars} caracteres", attachment.label)
                },
            });
            context.push_str(&format!(
                "\n[{}: {}]\n{}\n",
                attachment.kind, attachment.location, text
            ));
        }
        if context.is_empty() {
            context.push_str("(sin adjuntos; usa solo la conversación)\n");
        }
        emit(TaskEvent::ContextCollected {
            sources: sources.clone(),
        });

        let transcript = transcript(&self.history);
        let prompt = format!(
            "PROYECTO: {}\nIDIOMA PRIMARIO: {}\nMODO: {}\n\n\
             CONVERSACIÓN PREVIA:\n{}\n\n\
             CONTEXTO ADJUNTO:\n{}\n\n\
             NUEVO MENSAJE DEL USUARIO:\n{}",
            manifest.title,
            manifest.primary_language,
            self.mode.key(),
            transcript,
            context,
            self.instruction
        );
        let system = self.mode.system_prompt().to_string();

        if self.mode == ChatMode::Build {
            return self.run_build(
                root,
                adapter,
                options,
                BuildPrompt {
                    system,
                    prompt,
                    sources,
                },
                emit,
            );
        }

        let request = ModelRequest {
            system,
            prompt,
            max_output_tokens: 4096,
            tools: Vec::new(),
            history: Vec::new(),
        };
        let remaining = options
            .timeout
            .checked_sub(started.elapsed())
            .ok_or(AgentError::Timeout(options.timeout))?;
        let adapter_options = TaskOptions {
            timeout: remaining,
            cancel: options.cancel.clone(),
            stream: options.stream,
        };
        let mut streamed = String::new();
        let response = if options.stream {
            adapter.complete_streaming(&request, &adapter_options.cancel, &mut |delta| {
                streamed.push_str(delta);
                emit(TaskEvent::ModelDelta {
                    text: delta.to_string(),
                });
            })?
        } else {
            adapter.complete(&request, &adapter_options.cancel)?
        };
        emit(TaskEvent::ModelResponded {
            model: response.model.clone(),
            output_chars: response.text.chars().count(),
        });
        if options.cancel.is_cancelled() {
            return Err(AgentError::Cancelled);
        }
        let final_text = response.text;
        emit(TaskEvent::Completed {
            summary: final_text.clone(),
        });
        Ok(TaskOutput {
            exit: TaskExit::Success,
            summary: final_text.clone(),
            data: json!({
                "mode": self.mode.key(),
                "final_text": final_text,
                "adapter": adapter.name(),
                "model": response.model,
                "input_tokens": response.input_tokens,
                "output_tokens": response.output_tokens,
                "tools": [],
                "sources": sources,
            }),
            sources,
        })
    }

    fn run_build(
        &self,
        root: &Path,
        adapter: &dyn ModelAdapter,
        options: &TaskOptions,
        input: BuildPrompt,
        mut emit: impl FnMut(TaskEvent),
    ) -> Result<TaskOutput, AgentError> {
        let BuildPrompt {
            system,
            prompt,
            sources,
        } = input;
        let started = Instant::now();
        let specs = self.mode.tools();
        let mut history: Vec<ChatMessage> = Vec::new();
        let mut audit: Vec<ToolResult> = Vec::new();
        let mut proposals = Vec::new();
        let mut model_name = "desconocido".to_string();
        let mut final_text: Option<String> = None;
        let mut policy = GrantPolicy::new().allow("propose_scene_edit");

        for round in 0..6 {
            if options.cancel.is_cancelled() {
                return Err(AgentError::Cancelled);
            }
            let _remaining = options
                .timeout
                .checked_sub(started.elapsed())
                .ok_or(AgentError::Timeout(options.timeout))?;
            emit(TaskEvent::Progress {
                stage: "call_model".to_string(),
                detail: format!("Ronda {} de 6", round + 1),
            });
            let request = ModelRequest {
                system: system.clone(),
                prompt: if history.is_empty() {
                    format!(
                        "{prompt}\n\nUsa read_scene si necesitas leer una escena y \
                         propose_scene_edit para proponer el cambio. La propuesta \
                         queda para revisión humana."
                    )
                } else {
                    format!("{prompt}\n\nContinúa según los resultados de las herramientas.")
                },
                max_output_tokens: 4096,
                tools: specs.clone(),
                history: history.clone(),
            };
            let response = adapter.complete(&request, &options.cancel)?;
            if started.elapsed() > options.timeout {
                return Err(AgentError::Timeout(options.timeout));
            }
            model_name = response.model.clone();
            emit(TaskEvent::ModelResponded {
                model: response.model.clone(),
                output_chars: response.text.chars().count(),
            });

            if response.tool_calls.is_empty() {
                final_text = Some(response.text);
                break;
            }

            let calls = response.tool_calls.clone();
            history.push(ChatMessage {
                role: ChatRole::Assistant,
                content: response.text.clone(),
                tool_calls: calls.clone(),
                tool_call_id: None,
                tool_name: None,
            });
            let origin = proposal::Origin {
                task: Self::KIND.to_string(),
                adapter: adapter.name().to_string(),
                model: response.model.clone(),
                task_input: json!({
                    "mode": self.mode.key(),
                    "instruction": self.instruction,
                    "history_len": self.history.len(),
                }),
            };
            let mut executor = ToolExecutor::new(root);
            for call in &calls {
                if options.cancel.is_cancelled() {
                    return Err(AgentError::Cancelled);
                }
                let result = executor.execute(call, &mut policy, &origin);
                if let ToolOutcome::ProposalCreated { id, target, hunks } = &result.outcome {
                    proposals.push(json!({"id": id, "target": target, "hunks": hunks}));
                    emit(TaskEvent::ProposalCreated {
                        id: id.clone(),
                        target: target.clone(),
                        hunks: *hunks,
                    });
                }
                emit(TaskEvent::ToolExecuted {
                    id: result.call_id.clone(),
                    tool: result.tool.clone(),
                    allowed: result.succeeded(),
                    outcome: format!("{:?}", result.outcome),
                });
                let content = match &result.outcome {
                    ToolOutcome::Text(text) => text.clone(),
                    ToolOutcome::ProposalCreated { id, target, hunks } => format!(
                        "Propuesta {id} creada sobre {target} con {hunks} hunks. \
                         No se modificó ningún archivo canónico."
                    ),
                    ToolOutcome::ProposalApplied { id, target } => {
                        format!("Propuesta {id} aplicada a {target}.")
                    }
                    ToolOutcome::Denied { reason } => format!("Denegado: {reason}"),
                    ToolOutcome::Failed { message } => format!("Falló: {message}"),
                };
                history.push(ChatMessage {
                    role: ChatRole::Tool,
                    content,
                    tool_calls: Vec::new(),
                    tool_call_id: Some(call.id.clone()),
                    tool_name: Some(call.name.clone()),
                });
                audit.push(result);
            }
        }

        if options.cancel.is_cancelled() {
            return Err(AgentError::Cancelled);
        }
        let final_text = final_text.unwrap_or_else(|| {
            "El agente agotó las rondas sin una respuesta final. Revisa las \
             propuestas creadas o reformula la petición."
                .to_string()
        });
        emit(TaskEvent::Completed {
            summary: final_text.clone(),
        });
        Ok(TaskOutput {
            exit: TaskExit::Success,
            summary: final_text.clone(),
            data: json!({
                "mode": self.mode.key(),
                "final_text": final_text,
                "adapter": adapter.name(),
                "model": model_name,
                "tools": audit,
                "proposals": proposals,
                "sources": sources.clone(),
            }),
            sources,
        })
    }
}

fn transcript(history: &[ChatTurn]) -> String {
    if history.is_empty() {
        return "(sin mensajes previos)".to_string();
    }
    history
        .iter()
        .rev()
        .take(20)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|turn| {
            let role = match turn.role {
                ChatTurnRole::User => "Usuario",
                ChatTurnRole::Assistant => "Asistente",
            };
            format!("{role}: {}", truncate(&turn.content, 4_000))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn truncate(text: &str, max_chars: usize) -> String {
    let mut out: String = text.chars().take(max_chars).collect();
    if text.chars().count() > max_chars {
        out.push('…');
    }
    out
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::ProjectType;
    use crate::project::create_project;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn project() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "narra-chat-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("temp dir");
        create_project(&root, "Chat", ProjectType::Feature, "es").expect("project");
        root
    }

    #[test]
    fn modes_encode_tools_and_permissions_by_construction() {
        assert!(ChatMode::Plan.tools().is_empty());
        assert!(ChatMode::Review.tools().is_empty());
        let build = ChatMode::Build.tools();
        assert!(build.iter().any(|tool| tool.name == "read_scene"));
        assert!(build.iter().any(|tool| tool.name == "propose_scene_edit"));
        assert!(build.iter().any(|tool| tool.name == "apply_proposal"));
        for mode in ChatMode::ALL {
            assert_eq!(ChatMode::parse(mode.key()), Some(mode));
        }
    }

    #[test]
    fn thread_round_trips_under_narra_chats_without_secrets() {
        let root = project();
        let mut thread = ChatThread::new(ChatMode::Plan);
        thread.push_user("Resume la escena 1");
        thread.push_assistant("La escena plantea el conflicto.");
        thread.push_proposal("proposal-1", "screenplay/main.fountain", 2);
        thread.save(&root).expect("save");
        let path = root.join(CHATS_DIR).join(format!("{}.json", thread.id));
        assert!(path.is_file());
        let text = fs::read_to_string(&path).expect("read");
        assert!(!text.contains("api_key"));
        assert!(!text.contains("sk-"));
        let loaded = ChatThread::load(&root, &thread.id).expect("load");
        assert_eq!(loaded, thread);
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.messages[0].role, ChatTurnRole::User);
        assert_eq!(loaded.proposals.len(), 1);
        assert_eq!(loaded.proposals[0].hunks, 2);
    }

    #[test]
    fn list_is_most_recent_first_and_invalid_records_are_skipped() {
        let root = project();
        let mut first = ChatThread::new(ChatMode::Plan);
        first.push_user("primero");
        first.save(&root).expect("save first");
        std::thread::sleep(std::time::Duration::from_millis(5));
        let mut second = ChatThread::new(ChatMode::Build);
        second.push_user("segundo");
        second.save(&root).expect("save second");
        let listed = ChatThread::list(&root);
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, second.id);
        let invalid = root.join(CHATS_DIR).join("not-json.json");
        fs::write(&invalid, "{").expect("invalid");
        assert_eq!(ChatThread::list(&root).len(), 2);
    }

    #[test]
    fn title_comes_from_the_first_user_turn_and_ids_cannot_escape() {
        let mut thread = ChatThread::new(ChatMode::Plan);
        thread.push_user("  Una pregunta larga sobre la continuidad del acto uno  ");
        assert!(thread.title.starts_with("Una pregunta"));
        assert!(thread.title.chars().count() <= MAX_TITLE_CHARS + 1);
        thread.id = "../escape".to_string();
        assert!(thread.relative_path().is_err());
        assert!(ChatThread::load(&project(), "../escape").is_err());
    }

    #[test]
    fn transcript_keeps_recent_turns_and_caps_long_messages() {
        let history: Vec<ChatTurn> = (0..25)
            .map(|index| ChatTurn::user(format!("turno {index}")))
            .collect();
        let text = transcript(&history);
        assert!(text.contains("turno 24"));
        assert!(!text.contains("turno 0"));
        let long = transcript(&[ChatTurn::user("x".repeat(10_000))]);
        assert!(long.chars().count() < 5_000);
    }

    #[test]
    fn plan_chat_runs_without_tools_and_reports_context() {
        let root = project();
        let adapter = crate::agent::MockAdapter::new();
        let task = ChatTask::new(root.to_string_lossy(), ChatMode::Plan, Vec::new(), "Resume")
            .with_attachments(vec![ChatAttachment::new(
                "scene",
                "Escena 1",
                "screenplay/main.fountain",
                "INT. CASA - DÍA\nMarta entra.",
            )]);
        let output = task
            .run(&adapter, &TaskOptions::default(), |_event| {})
            .expect("chat");
        assert_eq!(output.exit, TaskExit::Success);
        assert!(!output.summary.is_empty());
        assert_eq!(output.sources.len(), 2);
        assert_eq!(output.data["mode"], "plan");
        assert!(output.data["tools"].as_array().is_some_and(Vec::is_empty));
    }

    #[test]
    fn build_chat_can_read_a_scene_but_never_applies_a_proposal() {
        let root = project();
        fs::write(
            root.join("screenplay/main.fountain"),
            "INT. CASA - DÍA\nMarta entra.\n",
        )
        .expect("screenplay");
        let adapter = crate::agent::MockAdapter::scripted(vec![
            crate::agent::ModelResponse {
                text: String::new(),
                model: "mock".to_string(),
                tool_calls: vec![crate::agent::ToolCall {
                    id: "read-1".to_string(),
                    name: "read_scene".to_string(),
                    arguments: json!({"scene": 1}),
                }],
                input_tokens: None,
                output_tokens: None,
            },
            crate::agent::ModelResponse {
                text: "La escena presenta a Marta.".to_string(),
                model: "mock".to_string(),
                tool_calls: Vec::new(),
                input_tokens: None,
                output_tokens: None,
            },
        ]);
        let task = ChatTask::new(
            root.to_string_lossy(),
            ChatMode::Build,
            Vec::new(),
            "Lee la escena",
        );
        let output = task
            .run(&adapter, &TaskOptions::default(), |_event| {})
            .expect("chat build");
        assert_eq!(output.data["final_text"], "La escena presenta a Marta.");
        assert_eq!(output.data["tools"].as_array().map(Vec::len), Some(1));
        assert!(
            output.data["proposals"]
                .as_array()
                .is_some_and(Vec::is_empty)
        );
    }

    #[test]
    fn build_chat_stages_a_proposal_but_leaves_the_screenplay_untouched() {
        let root = project();
        let screenplay = "INT. CASA - DÍA\n\nMarta entra.\n";
        fs::write(root.join("screenplay/main.fountain"), screenplay).expect("screenplay");
        let adapter = crate::agent::MockAdapter::scripted(vec![
            crate::agent::ModelResponse {
                text: String::new(),
                model: "mock".to_string(),
                tool_calls: vec![crate::agent::ToolCall {
                    id: "propose-1".to_string(),
                    name: "propose_scene_edit".to_string(),
                    arguments: json!({
                        "scene": 1,
                        "instruction": "haz que salga",
                        "rewritten_scene": "INT. CASA - DÍA\n\nMarta sale.\n"
                    }),
                }],
                input_tokens: None,
                output_tokens: None,
            },
            crate::agent::ModelResponse {
                text: "Propuse el cambio.".to_string(),
                model: "mock".to_string(),
                tool_calls: Vec::new(),
                input_tokens: None,
                output_tokens: None,
            },
        ]);
        let task = ChatTask::new(
            root.to_string_lossy(),
            ChatMode::Build,
            Vec::new(),
            "Haz que Marta salga",
        );
        let output = task
            .run(&adapter, &TaskOptions::default(), |_event| {})
            .expect("chat build");
        let proposals = output.data["proposals"].as_array().expect("proposals");
        assert_eq!(proposals.len(), 1);
        assert!(
            output.data["tools"]
                .as_array()
                .is_some_and(|tools| !tools.is_empty())
        );
        assert_eq!(
            fs::read_to_string(root.join("screenplay/main.fountain")).expect("read"),
            screenplay,
            "proposal must not write the canonical screenplay"
        );
        assert!(
            root.join(".narra/proposals").is_dir(),
            "proposal record should be persisted"
        );
    }
}
