//! UI-neutral agent task contracts (Milestone 4, Spikes C + F).
//!
//! Everything here is independent of any UI framework and of any specific
//! model provider: tasks take explicit serializable input, emit
//! [`TaskEvent`]s that any client (desktop, future CLI) can render, and
//! return machine-readable output alongside a human summary.
//!
//! Design rules (`AGENTS.md`):
//! - core services stay independent of UI framework state;
//! - provider-specific code lives behind [`adapter::ModelAdapter`];
//! - long-running work is cancellable and timeout-bounded;
//! - AI-visible context is scoped, read-only and *shown* to the user
//!   (every context item becomes a [`ContextSource`]).

pub mod adapter;
pub mod brief;
pub mod chat;
pub mod mock;
pub mod openai_compat;
pub mod proposal;
pub mod roles;
mod scene;
pub mod scene_edit;
pub mod scene_review;
pub mod tool_loop;
pub mod tools;

pub use adapter::{ModelAdapter, ModelRequest, ModelResponse};
pub use brief::ProjectBriefTask;
pub use chat::{
    ChatAttachment, ChatMode, ChatProposalRef, ChatTask, ChatThread, ChatTurn, ChatTurnRole,
};
pub use mock::MockAdapter;
pub use openai_compat::{
    LOCAL_DEFAULT_BASE_URL, OpenAiCompatAdapter, OpenAiCompatConfig, fetch_model_ids,
};
pub use proposal::{ApplyOutcome, FileProposal, ProposalState};
pub use roles::{Role, RoleOrigin, RolesError};
pub use scene_edit::SceneEditTask;
pub use scene_review::SceneReviewTask;
pub use tool_loop::ToolLoopTask;
pub use tools::{
    ChatMessage, ChatRole, GrantPolicy, PermissionDecision, PermissionPolicy, ToolCall,
    ToolOutcome, ToolPermission, ToolResult, ToolSpec,
};

use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::cancel::CancellationToken;

/// Options shared by every agent task.
#[derive(Debug, Clone)]
pub struct TaskOptions {
    pub timeout: Duration,
    pub cancel: CancellationToken,
    /// Deliver text to the UI as the model generates it (Spike C streaming).
    /// Tasks emit [`TaskEvent::ModelDelta`] per chunk when set; the final
    /// [`TaskOutput`] is identical either way.
    pub stream: bool,
}

impl Default for TaskOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(120),
            cancel: CancellationToken::new(),
            stream: false,
        }
    }
}

/// A UI-neutral progress/status event. Serialized as JSONL by the harness
/// (Spike F); the desktop UI will map the same events to its own surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum TaskEvent {
    Started {
        task: String,
        project: String,
    },
    Progress {
        stage: String,
        detail: String,
    },
    /// Which context the task actually read — shown to the user, not hidden.
    ContextCollected {
        sources: Vec<ContextSource>,
    },
    ModelResponded {
        model: String,
        output_chars: usize,
    },
    /// One incremental text chunk while streaming (Spike C). Only emitted
    /// when `TaskOptions::stream` is set; never carries credentials.
    ModelDelta {
        text: String,
    },
    /// A reviewable edit proposal was persisted; canonical files untouched.
    ProposalCreated {
        id: String,
        target: String,
        hunks: usize,
    },
    /// One tool call was processed: the policy ruled and the outcome is
    /// recorded (`allowed: false` means the call never executed).
    ToolExecuted {
        id: String,
        tool: String,
        allowed: bool,
        outcome: String,
    },
    Completed {
        summary: String,
    },
    Failed {
        error: String,
    },
    Cancelled,
}

/// One item of context fed to the model, with enough detail for a human to
/// audit exactly what the agent saw (`PRODUCT_SPEC.md`: show context used).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextSource {
    /// e.g. "manifest", "screenplay", "git_status".
    pub kind: String,
    /// Human-readable location: project-relative path, "git status", …
    pub location: String,
    /// Short description of what was taken from it.
    pub detail: String,
}

/// Defined exit status for a task (Spike F). The harness maps these to
/// process exit codes 0 / 1 / 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskExit {
    Success,
    Failure,
    Cancelled,
}

impl TaskExit {
    /// Process exit code convention: 0 ok, 1 failure, 2 cancelled.
    pub fn exit_code(self) -> i32 {
        match self {
            TaskExit::Success => 0,
            TaskExit::Failure => 1,
            TaskExit::Cancelled => 2,
        }
    }
}

/// Task result: machine-readable payload plus a human summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskOutput {
    pub exit: TaskExit,
    /// Human-readable summary (English for now; UI-localizable separately).
    pub summary: String,
    /// Machine-readable payload for programmatic clients.
    pub data: serde_json::Value,
    /// Context shown to the user; also emitted as a `ContextCollected` event.
    pub sources: Vec<ContextSource>,
}

/// Errors surfaced by agent tasks. Messages must never contain secrets.
#[derive(Debug, Error)]
pub enum AgentError {
    #[error(transparent)]
    Project(#[from] crate::project::ProjectError),
    #[error("git: {0}")]
    Git(#[from] crate::git::GitError),
    #[error("model adapter: {0}")]
    Adapter(String),
    #[error("cancelled")]
    Cancelled,
    #[error("timed out after {0:?}")]
    Timeout(Duration),
    #[error("invalid task input: {0}")]
    InvalidInput(&'static str),
    #[error(
        "invalid task input: scene {number} does not exist (the screenplay has {total} scenes)"
    )]
    SceneNumber { number: usize, total: usize },
    #[error("proposal: {0}")]
    Proposal(#[from] proposal::ProposalError),
    #[error("role `{role}` cannot run task `{task}`: its permission profile does not grant it")]
    RoleDenied { role: String, task: String },
    #[error("roles: {0}")]
    Roles(#[from] roles::RolesError),
}
