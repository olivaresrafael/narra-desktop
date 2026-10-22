//! `project_brief`: the first agent task (Spike C deliverable).
//!
//! Reads scoped, read-only project context (manifest, derived scene index,
//! Git status), shows every source it used, and asks the model for a short
//! project brief. No canonical file is ever modified by this task.

use std::path::Path;
use std::time::Instant;

use serde_json::json;

use super::adapter::{ModelAdapter, ModelRequest};
use super::{AgentError, ContextSource, TaskEvent, TaskExit, TaskOptions, TaskOutput};
use crate::git::{self, OpOptions};
use crate::manifest::ProjectType;
use crate::project::open_project;

/// Max scene headings included in the model context; the rest are counted.
const MAX_SCENES_IN_CONTEXT: usize = 30;
/// Hard cap on model output tokens for the brief.
const BRIEF_MAX_TOKENS: u32 = 800;

/// The `project_brief` task: input is explicit and serializable — a project
/// root path — and the run is driven by [`ProjectBriefTask::run`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectBriefTask {
    pub project_root: String,
}

impl ProjectBriefTask {
    pub fn new(project_root: impl Into<String>) -> Self {
        Self {
            project_root: project_root.into(),
        }
    }

    /// Human-readable task kind for events and provenance.
    pub const KIND: &'static str = "project_brief";

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
        if self.project_root.trim().is_empty() {
            return Err(AgentError::InvalidInput("project root must not be empty"));
        }

        emit(TaskEvent::Started {
            task: Self::KIND.to_string(),
            project: self.project_root.clone(),
        });

        self.check_cancel(options)?;
        emit(TaskEvent::Progress {
            stage: "open_project".to_string(),
            detail: "Leyendo project.yaml".to_string(),
        });
        let project = open_project(root)?;

        // --- Scoped, read-only context ----------------------------------
        self.check_cancel(options)?;
        emit(TaskEvent::Progress {
            stage: "collect_context".to_string(),
            detail: "Recopilando contexto de solo lectura".to_string(),
        });

        let manifest = project.manifest();
        let mut sources = vec![ContextSource {
            kind: "manifest".to_string(),
            location: "project.yaml".to_string(),
            detail: format!(
                "Título, tipo ({}) e idioma primario",
                project_type_label(manifest.project_type)
            ),
        }];

        let index = project.rebuild_index()?;
        let total_scenes = index.scenes.len();
        let shown_scenes = &index.scenes[..total_scenes.min(MAX_SCENES_IN_CONTEXT)];
        sources.push(ContextSource {
            kind: "scene_index".to_string(),
            location: manifest.default_screenplay_path.clone(),
            detail: format!(
                "{total_scenes} escenas (índice derivado); se envían las primeras {}",
                shown_scenes.len()
            ),
        });

        let git_summary = self.git_summary(root, options, &mut sources)?;

        let context = format!(
            "PROYECTO: {}\nIDIOMA: {}\nTIPO: {}\nESCENAS TOTALES: {total_scenes}\n\
             PRIMERAS ESCENAS:\n{}\nESTADO GIT:\n{}\n",
            manifest.title,
            manifest.primary_language,
            project_type_label(manifest.project_type),
            shown_scenes
                .iter()
                .map(|scene| format!("  L{} {}", scene.line_number, scene.heading))
                .collect::<Vec<_>>()
                .join("\n"),
            git_summary,
        );

        emit(TaskEvent::ContextCollected {
            sources: sources.clone(),
        });

        // --- Model call --------------------------------------------------
        self.check_cancel(options)?;
        let request = ModelRequest {
            system: "Eres un asistente de guion. Respondes en el idioma \
                     primario del proyecto, en prosa breve y concreta."
                .to_string(),
            prompt: format!(
                "{context}\nEscribe un resumen de una página como máximo \
                 sobre este proyecto: premisa, escala y estado actual."
            ),
            max_output_tokens: BRIEF_MAX_TOKENS,
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

        let data = json!({
            "project_title": manifest.title,
            "primary_language": manifest.primary_language,
            "project_type": project_type_label(manifest.project_type),
            "total_scenes": total_scenes,
            "git": git_summary,
            "adapter": adapter.name(),
            "model": response.model,
            "usage": {
                "input_tokens": response.input_tokens,
                "output_tokens": response.output_tokens,
            },
            "brief": response.text,
            "elapsed_ms": started.elapsed().as_millis() as u64,
        });

        let summary = format!(
            "Resumen de «{}» generado por {} ({total_scenes} escenas).",
            manifest.title,
            adapter.name()
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

    fn check_cancel(&self, options: &TaskOptions) -> Result<(), AgentError> {
        if options.cancel.is_cancelled() {
            Err(AgentError::Cancelled)
        } else {
            Ok(())
        }
    }

    /// One-line Git summary; a project without Git is fine, not an error.
    fn git_summary(
        &self,
        root: &Path,
        options: &TaskOptions,
        sources: &mut Vec<ContextSource>,
    ) -> Result<String, AgentError> {
        let git_options = OpOptions::default()
            .timeout(options.timeout)
            .cancel(options.cancel.clone());
        match git::Repository::open(root, git_options.clone()) {
            Ok(repository) => {
                let (branch, entries) = repository.status(git_options)?;
                let branch = branch
                    .and_then(|info| info.head)
                    .unwrap_or_else(|| "(sin commits)".to_string());
                let dirty = entries.len();
                sources.push(ContextSource {
                    kind: "git_status".to_string(),
                    location: "repositorio local".to_string(),
                    detail: format!("Rama {branch}, {dirty} entradas con cambios"),
                });
                Ok(format!("rama {branch}, {dirty} archivos con cambios"))
            }
            Err(_) => {
                sources.push(ContextSource {
                    kind: "git_status".to_string(),
                    location: "repositorio local".to_string(),
                    detail: "Sin repositorio Git; se omite el estado".to_string(),
                });
                Ok("sin repositorio Git".to_string())
            }
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
    // Adapters are trusted to honor the token; the task-level timeout is
    // enforced around the call boundary here (adapters also bound their own
    // requests). A real provider adapter will use the deadline internally.
    let started = Instant::now();
    let response = adapter.complete(request, &options.cancel)?;
    if started.elapsed() > options.timeout {
        return Err(AgentError::Timeout(options.timeout));
    }
    Ok(response)
}

fn project_type_label(kind: ProjectType) -> &'static str {
    match kind {
        ProjectType::Feature => "largometraje",
        ProjectType::Series => "serie",
        ProjectType::WebSeries => "webserie",
        ProjectType::Short => "corto",
        ProjectType::Microdrama => "microdrama",
        ProjectType::Other => "otro",
    }
}
