//! `scene_edit`: the first write-proposing agent task (M4 slice 2b).
//!
//! Reads one scene **read-only**, asks the model to rewrite it following an
//! explicit instruction, and turns the answer into a reviewable
//! [`FileProposal`] persisted under `.narra/proposals/`. The canonical
//! screenplay is never written by this task: the proposal is the copy, and
//! only an explicit [`proposal::apply`] (with hunk selection) touches the
//! file — refusing when its content no longer matches the proposal base
//! (`DATA_MODEL.md` §98 stale rule).

use std::path::Path;
use std::time::Instant;

use serde_json::json;

use super::adapter::{ModelAdapter, ModelRequest};
use super::proposal::{self, FileProposal, Origin};
use super::{AgentError, ContextSource, TaskEvent, TaskExit, TaskOptions, TaskOutput};
use crate::fountain;
use crate::git::{self, OpOptions};
use crate::project::open_project;

/// Hard cap on model output tokens for a scene rewrite.
const SCENE_EDIT_MAX_TOKENS: u32 = 4000;
/// Longest instruction shown in a context source detail (display only).
const MAX_INSTRUCTION_CHARS: usize = 160;

/// The `scene_edit` task: input is explicit and serializable — project root,
/// 1-based scene number and the writer's instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneEditTask {
    pub project_root: String,
    /// 1-based scene number as counted by the derived scene index.
    pub scene_number: usize,
    pub instruction: String,
}

impl SceneEditTask {
    pub fn new(
        project_root: impl Into<String>,
        scene_number: usize,
        instruction: impl Into<String>,
    ) -> Self {
        Self {
            project_root: project_root.into(),
            scene_number,
            instruction: instruction.into(),
        }
    }

    /// Human-readable task kind for events and provenance.
    pub const KIND: &'static str = "scene_edit";

    /// Rebuild the task that produced `proposal` so a stale proposal can be
    /// renewed by re-running the same request against the current content.
    ///
    /// Only tasks whose input round-trips through [`Origin::task_input`] are
    /// supported; anything else is an explicit error, never a guess.
    pub fn from_proposal(
        project_root: impl Into<String>,
        proposal: &FileProposal,
    ) -> Result<Self, AgentError> {
        if proposal.origin.task != Self::KIND {
            return Err(AgentError::InvalidInput(
                "renew is only available for scene_edit proposals",
            ));
        }
        let scene_number =
            proposal.origin.task_input["scene_number"]
                .as_u64()
                .ok_or(AgentError::InvalidInput(
                    "proposal is missing its scene_number input",
                ))? as usize;
        let instruction =
            proposal.origin.task_input["instruction"]
                .as_str()
                .ok_or(AgentError::InvalidInput(
                    "proposal is missing its instruction input",
                ))?;
        Ok(Self::new(project_root, scene_number, instruction))
    }

    /// Run the task headless: no window, no UI framework. `emit` receives
    /// UI-neutral events in order; the final [`TaskOutput`] carries both the
    /// machine payload and the human summary.
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

        // --- Read-only context -------------------------------------------
        self.check_cancel(options)?;
        emit(TaskEvent::Progress {
            stage: "open_project".to_string(),
            detail: "Leyendo project.yaml".to_string(),
        });
        let project = open_project(root)?;
        let manifest = project.manifest();
        let screenplay_path = manifest.default_screenplay_path.clone();

        emit(TaskEvent::Progress {
            stage: "collect_context".to_string(),
            detail: format!("Extrayendo la escena {}", self.scene_number),
        });
        let base = project.read_document(&screenplay_path)?;
        let scenes = fountain::parse(&base)
            .iter()
            .filter(|element| element.kind == fountain::ElementKind::SceneHeading)
            .map(|element| (element.line_number, element.text.clone()))
            .collect::<Vec<_>>();
        if scenes.is_empty() {
            return Err(AgentError::InvalidInput(
                "the screenplay has no scenes to edit",
            ));
        }
        if self.scene_number > scenes.len() {
            return Err(AgentError::SceneNumber {
                number: self.scene_number,
                total: scenes.len(),
            });
        }

        // Scene region: its heading line up to (excluding) the next heading.
        let base_lines: Vec<&str> = base.lines().collect();
        let (heading_line, heading) = &scenes[self.scene_number - 1];
        let start = heading_line - 1;
        let end = scenes
            .get(self.scene_number)
            .map(|(line, _)| line - 1)
            .unwrap_or(base_lines.len());
        // The blank line(s) separating this scene from the next are part of
        // the *separator*, not of the scene: the model reply is trimmed, so
        // they stay outside the replaced region and can never be lost.
        let mut region_end = end;
        while region_end > start && base_lines[region_end - 1].is_empty() {
            region_end -= 1;
        }
        let scene_text = base_lines[start..region_end].join("\n");

        let sources = vec![
            ContextSource {
                kind: "manifest".to_string(),
                location: "project.yaml".to_string(),
                detail: format!("Título «{}» e idioma primario", manifest.title),
            },
            ContextSource {
                kind: "scene".to_string(),
                location: format!("{screenplay_path}:L{}-L{region_end}", start + 1),
                detail: format!("«{heading}» ({} líneas)", region_end - start),
            },
            ContextSource {
                kind: "instruction".to_string(),
                location: "task input".to_string(),
                detail: truncate_chars(&self.instruction, MAX_INSTRUCTION_CHARS),
            },
        ];
        emit(TaskEvent::ContextCollected {
            sources: sources.clone(),
        });

        // --- Model call ----------------------------------------------------
        self.check_cancel(options)?;
        emit(TaskEvent::Progress {
            stage: "call_model".to_string(),
            detail: format!("Reescribiendo «{heading}»"),
        });
        let request = ModelRequest {
            system: "Eres un editor de guion profesional. Respondes en el \
                     idioma primario del proyecto. Devuelves exclusivamente \
                     el texto solicitado, sin comentarios, sin explicaciones \
                     y sin bloques de código."
                .to_string(),
            prompt: format!(
                "PROYECTO: {}\n\
                 INSTRUCCIÓN:\n{}\n\n\
                 ESCENA {} (líneas {}-L{} de {}):\n{}\n\n\
                 Reescribe la escena completa siguiendo la instrucción. \
                 Devuelve SOLO la escena reescrita: empezando por su \
                 encabezado, conservando el formato Fountain, con una línea \
                 en blanco entre el encabezado y el resto de la escena.",
                manifest.title,
                self.instruction,
                self.scene_number,
                start + 1,
                region_end,
                screenplay_path,
                scene_text,
            ),
            max_output_tokens: SCENE_EDIT_MAX_TOKENS,
        };
        let remaining = options
            .timeout
            .checked_sub(started.elapsed())
            .ok_or(AgentError::Timeout(options.timeout))?;
        let adapter_options = TaskOptions {
            timeout: remaining,
            cancel: options.cancel.clone(),
        };
        let response = run_model(adapter, &request, &adapter_options)?;
        emit(TaskEvent::ModelResponded {
            model: response.model.clone(),
            output_chars: response.text.chars().count(),
        });

        // --- Proposal (canonical file untouched) ---------------------------
        self.check_cancel(options)?;
        emit(TaskEvent::Progress {
            stage: "create_proposal".to_string(),
            detail: "Generando la propuesta de edición".to_string(),
        });
        let rewrite = sanitize_response(&response.text);
        if rewrite.is_empty() {
            return Err(AgentError::Adapter(
                "the model returned an empty rewrite".to_string(),
            ));
        }

        let mut proposed_lines = base_lines.clone();
        proposed_lines.splice(start..region_end, rewrite.lines());
        let mut proposed = proposed_lines.join("\n");
        if base.ends_with('\n') {
            proposed.push('\n');
        }

        let origin = Origin {
            task: Self::KIND.to_string(),
            adapter: adapter.name().to_string(),
            model: response.model.clone(),
            task_input: json!({
                "scene_number": self.scene_number,
                "instruction": self.instruction,
            }),
        };
        let record = FileProposal::new(
            screenplay_path.clone(),
            &base,
            &proposed,
            self.instruction.clone(),
            origin,
            base_revision(root, options),
        )?;
        proposal::save(root, &record)?;

        emit(TaskEvent::ProposalCreated {
            id: record.id.clone(),
            target: record.target.clone(),
            hunks: record.hunks.len(),
        });

        let data = json!({
            "proposal_id": record.id,
            "target": record.target,
            "state": record.state,
            "scene_number": self.scene_number,
            "scene_heading": heading,
            "scene_range": format!("L{}-L{region_end}", start + 1),
            "hunks": record.hunks.len(),
            "base_revision": record.base_revision,
            "base_sha256": record.base_sha256,
            "adapter": adapter.name(),
            "model": response.model,
            "usage": {
                "input_tokens": response.input_tokens,
                "output_tokens": response.output_tokens,
            },
            "elapsed_ms": started.elapsed().as_millis() as u64,
        });

        let summary = format!(
            "Propuesta {} lista para revisión: {} cambios en {} (escena {}).",
            &record.id[..8],
            record.hunks.len(),
            record.target,
            self.scene_number,
        );
        emit(TaskEvent::Completed {
            summary: summary.clone(),
        });

        Ok(TaskOutput {
            exit: TaskExit::Success,
            summary,
            data,
            sources,
        })
    }

    fn validate(&self) -> Result<(), AgentError> {
        if self.project_root.trim().is_empty() {
            return Err(AgentError::InvalidInput("project root must not be empty"));
        }
        if self.scene_number == 0 {
            return Err(AgentError::InvalidInput("scene numbers start at 1"));
        }
        if self.instruction.trim().is_empty() {
            return Err(AgentError::InvalidInput("instruction must not be empty"));
        }
        Ok(())
    }

    fn check_cancel(&self, options: &TaskOptions) -> Result<(), AgentError> {
        if options.cancel.is_cancelled() {
            Err(AgentError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Model call isolated so the timeout slice and cancellation map cleanly.
fn run_model(
    adapter: &dyn ModelAdapter,
    request: &ModelRequest,
    options: &TaskOptions,
) -> Result<crate::agent::ModelResponse, AgentError> {
    if options.cancel.is_cancelled() {
        return Err(AgentError::Cancelled);
    }
    let started = Instant::now();
    let response = adapter.complete(request, &options.cancel)?;
    if started.elapsed() > options.timeout {
        return Err(AgentError::Timeout(options.timeout));
    }
    Ok(response)
}

/// Strip a wrapping Markdown code fence (a common model habit) and stray
/// whitespace. Anything else is taken verbatim: the diff shows exactly what
/// the model answered.
fn sanitize_response(text: &str) -> String {
    let mut lines: Vec<&str> = text.trim().lines().collect();
    let is_fence = |line: &&str| line.trim_start().starts_with("```");
    if lines.first().is_some_and(is_fence) {
        lines.remove(0);
    }
    if lines.last().is_some_and(is_fence) {
        lines.pop();
    }
    lines.join("\n").trim().to_string()
}

/// Git HEAD oid when the project is a repository — provenance only; staleness
/// is decided by content hash, not by the revision.
fn base_revision(root: &Path, options: &TaskOptions) -> Option<String> {
    let git_options = OpOptions::default()
        .timeout(options.timeout)
        .cancel(options.cancel.clone());
    let repository = git::Repository::open(root, git_options.clone()).ok()?;
    repository
        .log(1, git_options)
        .ok()?
        .first()
        .map(|commit| commit.id.clone())
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max).collect();
    format!("{truncated}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_wrapping_code_fences() {
        let fenced = "```fountain\nINT. TALLER - NOCHE\n\nELENA\nHola.\n```";
        assert_eq!(
            sanitize_response(fenced),
            "INT. TALLER - NOCHE\n\nELENA\nHola."
        );
    }

    #[test]
    fn sanitize_keeps_plain_text_untouched() {
        assert_eq!(
            sanitize_response("  INT. CALLE - DÍA  \n"),
            "INT. CALLE - DÍA"
        );
    }

    #[test]
    fn truncate_respects_char_boundaries() {
        let text = "áéíóú".repeat(10);
        let truncated = truncate_chars(&text, 10);
        assert_eq!(truncated.chars().count(), 11); // 10 + ellipsis
        assert!(truncated.ends_with('…'));
        assert_eq!(truncate_chars("corto", 10), "corto");
    }

    #[test]
    fn validate_rejects_bad_input() {
        let blank = SceneEditTask::new("   ", 1, "instrucción");
        assert!(matches!(blank.validate(), Err(AgentError::InvalidInput(_))));
        let zero = SceneEditTask::new("/tmp/p", 0, "instrucción");
        assert!(matches!(zero.validate(), Err(AgentError::InvalidInput(_))));
        let no_instruction = SceneEditTask::new("/tmp/p", 2, "  ");
        assert!(matches!(
            no_instruction.validate(),
            Err(AgentError::InvalidInput(_))
        ));
        assert!(
            SceneEditTask::new("/tmp/p", 2, "instrucción")
                .validate()
                .is_ok()
        );
    }

    #[test]
    fn from_proposal_round_trips_the_task_input() {
        let origin = Origin {
            task: "scene_edit".into(),
            adapter: "mock".into(),
            model: "mock-1".into(),
            task_input: json!({"scene_number": 3, "instruction": "más tensión"}),
        };
        let proposal = FileProposal::new(
            "screenplay/main.fountain",
            "INT. A - DÍA\n\nHola.\n",
            "INT. A - DÍA\n\nBuenos días.\n",
            "más tensión",
            origin,
            None,
        )
        .expect("proposal");
        let task = SceneEditTask::from_proposal("/tmp/p", &proposal).expect("renew");
        assert_eq!(task.scene_number, 3);
        assert_eq!(task.instruction, "más tensión");
        assert_eq!(task.project_root, "/tmp/p");
    }

    #[test]
    fn from_proposal_refuses_foreign_task_origins() {
        let origin = Origin {
            task: "project_brief".into(),
            adapter: "mock".into(),
            model: "mock-1".into(),
            task_input: json!({}),
        };
        let proposal = FileProposal::new(
            "screenplay/main.fountain",
            "INT. A - DÍA\n\nHola.\n",
            "INT. A - DÍA\n\nBuenos días.\n",
            "r",
            origin,
            None,
        )
        .expect("proposal");
        let error = SceneEditTask::from_proposal("/tmp/p", &proposal).expect_err("refused");
        assert!(matches!(error, AgentError::InvalidInput(_)), "{error:?}");
    }
}
