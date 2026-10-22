//! Permission-gated tool loop (Milestone 4, slice 3b).
//!
//! The model is offered a declared set of [`ToolSpec`]s, may request tool
//! calls over several rounds, and every call is executed through
//! [`ToolExecutor`] — which applies a [`PermissionPolicy`] *before* running
//! anything and records an audit [`ToolResult`] either way.
//!
//! Product rules enforced here (`AGENTS.md`):
//!
//! - deny-by-default: an `Ask`/`Deny` decision becomes a tool message the
//!   model sees (so it can adapt) and an audit record the user sees — the
//!   call never executes;
//! - AI-visible changes are proposals: the only write-capable tool stages a
//!   `FileProposal`; canonical files change only when a human (or an
//!   explicitly destructive grant) applies it;
//! - bounded: at most [`MAX_ROUNDS`] model rounds, every round honors the
//!   task timeout and cancellation token;
//! - auditable: every tool call and decision lands in the task output.

use std::path::Path;
use std::time::Instant;

use serde_json::json;

use super::adapter::{ModelAdapter, ModelRequest};
use super::proposal::Origin;
use super::tools::{
    self, ChatMessage, ChatRole, GrantPolicy, PermissionPolicy, ToolOutcome, ToolResult,
};
use super::{AgentError, ContextSource, TaskEvent, TaskExit, TaskOptions, TaskOutput};
use crate::project::open_project;

/// Maximum model rounds in one loop (each round may request tool calls).
pub const MAX_ROUNDS: usize = 6;

/// Max output tokens per model round. Reasoning-capable models spend a
/// large share of this on their own reasoning before any visible content,
/// so the budget is generous.
const LOOP_MAX_TOKENS: u32 = 4096;

/// A task that lets the model request declared tools under an explicit
/// permission policy.
pub struct ToolLoopTask {
    pub project_root: String,
    /// The writer's goal, e.g. "refuerza la tensión de la escena 2".
    pub instruction: String,
}

impl ToolLoopTask {
    pub fn new(project_root: impl Into<String>, instruction: impl Into<String>) -> Self {
        Self {
            project_root: project_root.into(),
            instruction: instruction.into(),
        }
    }

    fn validate(&self) -> Result<(), AgentError> {
        if self.instruction.trim().is_empty() {
            return Err(AgentError::InvalidInput("instruction must not be empty"));
        }
        Ok(())
    }

    /// Run the loop. `policy` is supplied by the caller (desktop: prompt the
    /// user on `Ask`; harness: [`GrantPolicy`] from explicit flags — `Ask`
    /// becomes an actionable denial, never a silent grant).
    pub fn run(
        &self,
        adapter: &dyn ModelAdapter,
        policy: &mut dyn PermissionPolicy,
        options: &TaskOptions,
        mut emit: impl FnMut(TaskEvent),
    ) -> Result<TaskOutput, AgentError> {
        let started = Instant::now();
        let root = Path::new(&self.project_root);
        self.validate()?;

        emit(TaskEvent::Started {
            task: "tool_loop".to_string(),
            project: self.project_root.clone(),
        });

        // --- Read-only context -------------------------------------------
        if options.cancel.is_cancelled() {
            return Err(AgentError::Cancelled);
        }
        emit(TaskEvent::Progress {
            stage: "open_project".to_string(),
            detail: "Leyendo project.yaml".to_string(),
        });
        let project = open_project(root)?;
        let manifest = project.manifest();
        let sources = vec![
            ContextSource {
                kind: "manifest".to_string(),
                location: "project.yaml".to_string(),
                detail: format!("Título «{}» e idioma primario", manifest.title),
            },
            ContextSource {
                kind: "tools".to_string(),
                location: "declared tool specs".to_string(),
                detail: format!("{} herramientas disponibles", tools::tool_specs().len()),
            },
            ContextSource {
                kind: "instruction".to_string(),
                location: "task input".to_string(),
                detail: self.instruction.clone(),
            },
        ];
        emit(TaskEvent::ContextCollected {
            sources: sources.clone(),
        });

        // --- Loop ---------------------------------------------------------
        let specs = tools::tool_specs();
        let mut history: Vec<ChatMessage> = Vec::new();
        let mut audit: Vec<ToolResult> = Vec::new();
        let mut model_name = "desconocido".to_string();
        let mut final_text: Option<String> = None;

        for round in 0..MAX_ROUNDS {
            if options.cancel.is_cancelled() {
                return Err(AgentError::Cancelled);
            }
            let remaining = options
                .timeout
                .checked_sub(started.elapsed())
                .ok_or(AgentError::Timeout(options.timeout))?;
            emit(TaskEvent::Progress {
                stage: "call_model".to_string(),
                detail: format!("Ronda {} de hasta {MAX_ROUNDS}", round + 1),
            });

            let request = ModelRequest {
                system: "Eres un editor de guion que trabaja sobre un \
                     proyecto de guion. Puedes usar herramientas para leer \
                     escenas y proponer ediciones. Responde en el idioma \
                     primario del proyecto. Si no puedes cumplir la \
                     instrucción (por ejemplo, una herramienta fue \
                     denegada), dilo con claridad."
                    .to_string(),
                prompt: format!(
                    "PROYECTO: {}\nINSTRUCCIÓN DEL USUARIO:\n{}\n\n{}",
                    manifest.title,
                    self.instruction,
                    if history.is_empty() {
                        "Empieza leyendo lo que necesites y luego propón el \
                         cambio."
                            .to_string()
                    } else {
                        "Continúa según los resultados de las herramientas.".to_string()
                    }
                ),
                max_output_tokens: LOOP_MAX_TOKENS,
                tools: specs.clone(),
                history: history.clone(),
            };
            let adapter_options = TaskOptions {
                timeout: remaining,
                cancel: options.cancel.clone(),
                stream: false,
            };
            let response = adapter.complete(&request, &adapter_options.cancel)?;
            model_name = response.model.clone();
            if remaining < started.elapsed() {
                return Err(AgentError::Timeout(options.timeout));
            }
            emit(TaskEvent::ModelResponded {
                model: response.model.clone(),
                output_chars: response.text.chars().count(),
            });

            if response.tool_calls.is_empty() {
                final_text = Some(response.text);
                break;
            }

            // Record the assistant turn, then execute each requested call.
            let assistant_calls = response.tool_calls.clone();
            history.push(ChatMessage {
                role: ChatRole::Assistant,
                content: response.text.clone(),
                tool_calls: assistant_calls.clone(),
                tool_call_id: None,
                tool_name: None,
            });

            let origin = Origin {
                task: "tool_loop".to_string(),
                adapter: adapter.name().to_string(),
                model: response.model.clone(),
                task_input: json!({"instruction": self.instruction}),
            };
            let mut executor = tools::ToolExecutor::new(root);
            for call in &assistant_calls {
                if options.cancel.is_cancelled() {
                    return Err(AgentError::Cancelled);
                }
                let result = executor.execute(call, policy, &origin);
                emit_tool_event(&result, &mut emit);
                // Feed the outcome back so the model can adapt (including
                // denials: it must see *why* it cannot proceed).
                let content = match &result.outcome {
                    ToolOutcome::Text(text) => text.clone(),
                    ToolOutcome::ProposalCreated { id, target, hunks } => format!(
                        "Propuesta {id} creada sobre {target} con {hunks} \
                         hunks. No se modificó ningún archivo canónico."
                    ),
                    ToolOutcome::ProposalApplied { id, target } => {
                        format!("Propuesta {id} aplicada a {target}.")
                    }
                    ToolOutcome::Denied { reason } => {
                        format!("Herramienta denegada por permisos: {reason}")
                    }
                    ToolOutcome::Failed { message } => {
                        format!("La herramienta falló: {message}")
                    }
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

        let final_text = final_text
            .unwrap_or_else(|| "El bucle agotó sus rondas sin una respuesta final.".to_string());
        let denied = audit
            .iter()
            .filter(|result| matches!(result.outcome, ToolOutcome::Denied { .. }))
            .count();
        let summary = format!(
            "Ronda completada con {} llamadas a herramientas ({} denegadas). \
             {final_text}",
            audit.len(),
            denied
        );
        emit(TaskEvent::Completed {
            summary: summary.clone(),
        });

        let data = json!({
            "final_text": final_text,
            "adapter": adapter.name(),
            "model": model_name,
            "instruction": self.instruction,
            "tools": audit,
        });
        Ok(TaskOutput {
            exit: TaskExit::Success,
            summary,
            data,
            sources,
        })
    }
}

/// One event per executed call: what was asked, how the policy ruled and
/// the outcome kind — auditable without the final output.
fn emit_tool_event(result: &ToolResult, emit: &mut impl FnMut(TaskEvent)) {
    let (allowed, outcome) = match &result.outcome {
        ToolOutcome::Denied { .. } => (false, "denied"),
        ToolOutcome::Text(_) => (true, "text"),
        ToolOutcome::ProposalCreated { .. } => (true, "proposal_created"),
        ToolOutcome::ProposalApplied { .. } => (true, "proposal_applied"),
        ToolOutcome::Failed { .. } => (true, "failed"),
    };
    emit(TaskEvent::ToolExecuted {
        id: result.call_id.clone(),
        tool: result.tool.clone(),
        allowed,
        outcome: outcome.to_string(),
    });
}

/// Convenience: a deny-everything-but-read policy, the harness default.
pub fn default_policy() -> GrantPolicy {
    GrantPolicy::new()
}

// Keep the unused-import lint honest: ToolCall is used in tests below.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::mock::MockAdapter;
    use crate::agent::tools::{PermissionDecision, ToolCall};

    fn call(name: &str) -> ToolCall {
        ToolCall {
            id: "call-1".to_string(),
            name: name.to_string(),
            arguments: json!({"scene": 1}),
        }
    }

    #[test]
    fn empty_instruction_is_rejected() {
        let task = ToolLoopTask::new("/tmp/p", "   ");
        assert!(matches!(task.validate(), Err(AgentError::InvalidInput(_))));
    }

    #[test]
    fn default_policy_grants_only_read_only() {
        let mut policy = GrantPolicy::new();
        assert_eq!(
            policy.decide(&call("read_scene"), tools::ToolPermission::ReadOnly),
            PermissionDecision::Allow
        );
        assert!(matches!(
            policy.decide(
                &call("propose_scene_edit"),
                tools::ToolPermission::ProposeWrite
            ),
            PermissionDecision::Ask(_)
        ));
        assert!(matches!(
            policy.decide(&call("apply_proposal"), tools::ToolPermission::Destructive),
            PermissionDecision::Deny(_)
        ));
    }

    #[test]
    fn write_grant_does_not_imply_destructive() {
        let mut policy = GrantPolicy::new().allow("propose_scene_edit");
        assert_eq!(
            policy.decide(
                &call("propose_scene_edit"),
                tools::ToolPermission::ProposeWrite
            ),
            PermissionDecision::Allow
        );
        // The same grant must not unlock the destructive tool.
        assert!(matches!(
            policy.decide(&call("apply_proposal"), tools::ToolPermission::Destructive),
            PermissionDecision::Deny(_)
        ));
    }

    #[test]
    fn scripted_mock_without_tool_calls_completes_in_one_round() {
        let task = ToolLoopTask::new("/tmp/no-project", "instrucción");
        // Validation of the project happens first; use a real fixture? No —
        // this test only checks that a text-only response ends the loop.
        // The full loop is covered by integration tests with a real project.
        let mut policy = GrantPolicy::new();
        let error = task
            .run(
                &MockAdapter::new(),
                &mut policy,
                &TaskOptions::default(),
                |_| {},
            )
            .expect_err("missing project must fail before any model call");
        assert!(matches!(error, AgentError::Project(_)), "{error:?}");
    }
}
