//! `scene_review`: read-only scene review grounded in project material (M6).
//!
//! The acceptance it serves (`DEVELOPMENT_PLAN.md`, Milestone 6): the user
//! can ask for a scene review, the task grounds it in the project's own
//! material — bible, structure, research, and notes (decisions and open
//! questions) — and *shows every source it read*; it never writes a file,
//! never creates a proposal and never touches canon (`DATA_MODEL.md` §1:
//! files are authoritative, agents propose, users apply).
//!
//! Context rules (`AGENTS.md`): scoped, read-only, every context item
//! becomes a [`ContextSource`], and long documents are truncated with the
//! truncation stated in the source detail (nothing is silently dropped).

use std::path::Path;
use std::time::Instant;

use serde_json::json;

use super::adapter::{ModelAdapter, ModelRequest};
use super::scene::{SceneLookup, scene_block};
use super::{AgentError, ContextSource, TaskEvent, TaskExit, TaskOptions, TaskOutput};
use crate::project::open_project;

/// Hard cap on model output tokens for a review.
const REVIEW_MAX_TOKENS: u32 = 1400;
/// Directories whose documents ground a scene review (`DATA_MODEL.md` §2:
/// bible, structure, research; `notes/` holds decisions and open questions).
const MATERIAL_DIRS: &[&str] = &["bible", "structure", "research", "notes"];
/// Upper bound on material documents put into context. Paths come sorted
/// from `list_documents`, so the selection is deterministic.
const MAX_MATERIAL_DOCS: usize = 12;
/// Max characters taken from each material document: a bible can grow large,
/// and the source reports the truncation instead of hiding it.
const MAX_CHARS_PER_DOC: usize = 2000;
/// Longest focus shown in the context source detail (display only).
const MAX_FOCUS_CHARS: usize = 160;

/// The `scene_review` task: input is explicit and serializable — project
/// root, 1-based scene number, and an optional reviewer focus (empty string
/// means a general review).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneReviewTask {
    pub project_root: String,
    pub scene_number: usize,
    /// Optional question/focus for the review; empty = general review.
    pub focus: String,
}

impl SceneReviewTask {
    pub fn new(
        project_root: impl Into<String>,
        scene_number: usize,
        focus: impl Into<String>,
    ) -> Self {
        Self {
            project_root: project_root.into(),
            scene_number,
            focus: focus.into(),
        }
    }

    /// Human-readable task kind for events and provenance.
    pub const KIND: &'static str = "scene_review";

    /// Run the task headless: no window, no UI framework. `emit` receives
    /// UI-neutral events in order; the final [`TaskOutput`] carries both the
    /// machine payload and the human summary. Read-only by construction:
    /// there is no proposal step and no write path in this task.
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
        let script = project.read_document(&screenplay_path)?;
        let block = match scene_block(&script, self.scene_number) {
            SceneLookup::Found(block) => block,
            SceneLookup::NoScenes => {
                return Err(AgentError::InvalidInput(
                    "the screenplay has no scenes to review",
                ));
            }
            SceneLookup::OutOfRange { total } => {
                return Err(AgentError::SceneNumber {
                    number: self.scene_number,
                    total,
                });
            }
        };

        let mut sources = vec![
            ContextSource {
                kind: "manifest".to_string(),
                location: "project.yaml".to_string(),
                detail: format!("Título «{}» e idioma primario", manifest.title),
            },
            ContextSource {
                kind: "scene".to_string(),
                location: format!(
                    "{screenplay_path}:L{}-L{}",
                    block.start_line, block.end_line
                ),
                detail: format!(
                    "«{}» ({} líneas)",
                    block.heading,
                    block.text.lines().count()
                ),
            },
        ];
        let focus = self.focus.trim();
        if !focus.is_empty() {
            sources.push(ContextSource {
                kind: "focus".to_string(),
                location: "task input".to_string(),
                detail: truncate_chars(focus, MAX_FOCUS_CHARS),
            });
        }

        // Project material: only the narrative-context directories, capped
        // and truncated, each one listed exactly as it was read.
        let mut material: Vec<(String, String)> = Vec::new();
        for path in project.list_documents()? {
            let in_material = MATERIAL_DIRS
                .iter()
                .any(|dir| path.starts_with(&format!("{dir}/")));
            if !in_material {
                continue;
            }
            if material.len() >= MAX_MATERIAL_DOCS {
                break;
            }
            // A listed document must be readable: report the error instead
            // of silently reviewing against less material than shown.
            let text = project.read_document(&path)?;
            let original_chars = text.chars().count();
            let (text, truncated) = if original_chars > MAX_CHARS_PER_DOC {
                (truncate_chars(&text, MAX_CHARS_PER_DOC), true)
            } else {
                (text, false)
            };
            let kind = path.split('/').next().unwrap_or("material").to_string();
            sources.push(ContextSource {
                kind,
                location: path.clone(),
                detail: if truncated {
                    format!(
                        "{original_chars} caracteres (truncados a {MAX_CHARS_PER_DOC} para el contexto)"
                    )
                } else {
                    format!("{original_chars} caracteres")
                },
            });
            material.push((path, text));
        }

        emit(TaskEvent::ContextCollected {
            sources: sources.clone(),
        });

        // --- Model call ----------------------------------------------------
        self.check_cancel(options)?;
        emit(TaskEvent::Progress {
            stage: "call_model".to_string(),
            detail: format!("Revisando «{}»", block.heading),
        });
        let mut prompt = format!(
            "PROYECTO: {} (idioma {})\nFOCO: {}\n\nMATERIAL DEL PROYECTO:\n",
            manifest.title,
            manifest.primary_language,
            if focus.is_empty() {
                "revisión general".to_string()
            } else {
                focus.to_string()
            },
        );
        if material.is_empty() {
            prompt.push_str("(sin documentos de material en el proyecto)\n");
        }
        for (path, text) in &material {
            prompt.push_str(&format!("\n--- {path} ---\n{text}\n"));
        }
        prompt.push_str(&format!(
            "\nESCENA {} ({} L{}-L{}):\n{}\n\n\
             Revisa esta escena usando el material anterior. Responde en el idioma \
             primario del proyecto y con esta estructura: (1) lectura de la escena; \
             (2) continuidad con el material del proyecto, citando la fuente al \
             señalar cada contradicción; (3) problemas de escritura; (4) sugerencias \
             concretas. Es una revisión de solo lectura: no propongas cambios de \
             archivos ni pidas ejecutar herramientas.",
            self.scene_number, screenplay_path, block.start_line, block.end_line, block.text,
        ));
        let request = ModelRequest {
            system: "Eres un editor de guion profesional. Respondes en el idioma \
                     primario del proyecto, en prosa clara y concreta. Tu trabajo es \
                     revisar y fundamentar la revisión en el material entregado."
                .to_string(),
            prompt,
            max_output_tokens: REVIEW_MAX_TOKENS,
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
        let response = run_model(adapter, &request, &adapter_options, &mut emit)?;

        emit(TaskEvent::ModelResponded {
            model: response.model.clone(),
            output_chars: response.text.chars().count(),
        });

        // --- Output (no writes anywhere in this task) ----------------------
        let summary = format!(
            "Revisión de la escena {} «{}» con {} documento(s) de material, por {}.",
            self.scene_number,
            block.heading,
            material.len(),
            adapter.name()
        );
        let data = json!({
            "project_title": manifest.title,
            "scene_number": self.scene_number,
            "scene_heading": block.heading,
            "scene_range": format!("L{}-L{}", block.start_line, block.end_line),
            "focus": focus,
            "material_docs": material.iter().map(|(path, _)| path).collect::<Vec<_>>(),
            "adapter": adapter.name(),
            "model": response.model,
            "usage": {
                "input_tokens": response.input_tokens,
                "output_tokens": response.output_tokens,
            },
            "review": response.text,
            "elapsed_ms": started.elapsed().as_millis() as u64,
        });
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
/// When `options.stream` is set, each chunk is forwarded to `emit` as a
/// [`TaskEvent::ModelDelta`] before the full response is returned.
fn run_model(
    adapter: &dyn ModelAdapter,
    request: &ModelRequest,
    options: &TaskOptions,
    emit: &mut dyn FnMut(TaskEvent),
) -> Result<crate::agent::ModelResponse, AgentError> {
    if options.cancel.is_cancelled() {
        return Err(AgentError::Cancelled);
    }
    let started = Instant::now();
    let response = if options.stream {
        adapter.complete_streaming(request, &options.cancel, &mut |chunk| {
            emit(TaskEvent::ModelDelta {
                text: chunk.to_string(),
            });
        })?
    } else {
        adapter.complete(request, &options.cancel)?
    };
    if started.elapsed() > options.timeout {
        return Err(AgentError::Timeout(options.timeout));
    }
    Ok(response)
}

/// Display truncation on a char boundary with an ellipsis (sources and
/// focus detail only — the full file stays on disk, authoritative).
fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max).collect();
    format!("{truncated}…")
}
