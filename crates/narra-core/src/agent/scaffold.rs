//! `project_scaffold` (M16 slice 9, D-044): an AI-generated project base.
//!
//! The model proposes a small set of new files as strict JSON; the task
//! validates every path and size *before* anything can be written. The files
//! are not change proposals: `proposal::apply` requires an existing target,
//! so new files go through [`write_plan`], which is create-only and rolls back
//! what it created if a write fails. The UI must show the plan for review
//! before calling it.
//!
//! Safety rules (D-044):
//! - only relative paths under `bible/`, `structure/`, `research/`, `notes/`
//!   or `screenplay/`, with `.md`/`.fountain` extensions;
//! - never `.narra/`, `project.yaml`, `agents.yaml`, `AGENTS.md`,
//!   `.gitignore`, the default screenplay or the seeded notes;
//! - at most 12 files of 16 KB each; duplicate paths are rejected;
//! - an existing file is never overwritten.

use std::collections::BTreeSet;
use std::path::{Component, Path};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

use super::adapter::{ModelAdapter, ModelRequest};
use super::roles::Role;
use super::{AgentError, ContextSource, TaskEvent, TaskExit, TaskOptions, TaskOutput};
use crate::document;
use crate::manifest::ProjectType;

/// Human-readable task kind.
pub const KIND: &str = "project_scaffold";
/// Upper bounds for a generated base.
pub const MAX_FILES: usize = 12;
pub const MAX_FILE_BYTES: usize = 16 * 1024;
pub const MAX_PATH_CHARS: usize = 120;
/// Output budget for the plan. 2k truncated multi-file plans mid-JSON; the
/// plan is capped at 12 files, so give the model enough room to finish the
/// object instead of returning a parse error.
const SCAFFOLD_MAX_TOKENS: u32 = 8_000;

/// The only top-level folders a generated file may live in.
pub const ALLOWED_ROOTS: [&str; 5] = ["bible", "structure", "research", "notes", "screenplay"];

/// Paths the base project already owns (or app-managed state) and that the
/// model must never try to replace.
pub const RESERVED_PATHS: [&str; 7] = [
    "project.yaml",
    "AGENTS.md",
    "agents.yaml",
    ".gitignore",
    "screenplay/main.fountain",
    "notes/decisions.md",
    "notes/open-questions.md",
];

const BASE_SYSTEM: &str = "Eres un arquitecto de proyectos de guion. \
     Propones una base de trabajo en archivos Markdown/Fountain para que una \
     persona empiece a escribir. Respondes SIEMPRE con un único objeto JSON \
     válido, sin texto fuera del JSON.";

const RULES: &str = "Devuelve SOLO un objeto JSON con esta forma:\n\
{\"files\":[{\"path\":\"bible/premise.md\",\"content\":\"...\"}]}\n\
- Entre 1 y 12 archivos.\n\
- Rutas relativas bajo bible/, structure/, research/, notes/ o screenplay/; \
extensión .md o .fountain.\n\
- Nunca .narra/, project.yaml, agents.yaml, AGENTS.md, .gitignore, \
screenplay/main.fountain, notes/decisions.md ni notes/open-questions.md.\n\
- Contenido en el idioma del proyecto; no inventes datos personales.\n\
- Sé conciso: prioriza entre 3 y 6 archivos y no repitas contenido.\n\
- El contenido debe ser una cadena JSON válida: escapa los saltos de línea \
como \\n y las comillas como \\\".";

/// One proposed file. Paths are validated before this type is exposed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScaffoldFile {
    pub path: String,
    pub content: String,
}

/// A reviewed set of new files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScaffoldPlan {
    pub files: Vec<ScaffoldFile>,
}

/// Validation/write failures. Messages name the path and never echo content.
#[derive(Debug, Error)]
pub enum ScaffoldError {
    #[error("scaffold: {0}")]
    Parse(String),
    #[error("scaffold: the plan has no files")]
    Empty,
    #[error("scaffold: more than {MAX_FILES} files")]
    TooMany,
    #[error("scaffold: invalid path `{path}`: {reason}")]
    InvalidPath { path: String, reason: String },
    #[error("scaffold: `{path}` is reserved and cannot be generated")]
    Reserved { path: String },
    #[error("scaffold: duplicate path `{path}`")]
    Duplicate { path: String },
    #[error("scaffold: `{path}` is larger than {MAX_FILE_BYTES} bytes")]
    TooLarge { path: String },
    #[error("scaffold: `{path}` must not be empty")]
    EmptyFile { path: String },
    #[error("scaffold: could not write `{path}`: {error}")]
    Write { path: String, error: String },
}

/// Parse the model's strict JSON, tolerating a Markdown fence, surrounding
/// prose, raw newlines inside strings and trailing commas. Models routinely
/// return a valid-looking plan with unescaped control characters in long
/// Markdown bodies; a plain `serde_json` parse would reject the whole plan.
pub fn parse_plan(text: &str) -> Result<ScaffoldPlan, ScaffoldError> {
    let value = parse_json_object(text)?;
    let object = value
        .as_object()
        .ok_or_else(|| ScaffoldError::Parse("top level must be an object".to_string()))?;
    if let Some(key) = object.keys().find(|key| key.as_str() != "files") {
        return Err(ScaffoldError::Parse(format!(
            "unknown top-level key `{key}`"
        )));
    }
    let files = object
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| ScaffoldError::Parse("missing `files` array".to_string()))?;
    let mut plan = ScaffoldPlan {
        files: Vec::with_capacity(files.len()),
    };
    for file in files {
        let object = file
            .as_object()
            .ok_or_else(|| ScaffoldError::Parse("each file must be an object".to_string()))?;
        if let Some(key) = object
            .keys()
            .find(|key| key.as_str() != "path" && key.as_str() != "content")
        {
            return Err(ScaffoldError::Parse(format!(
                "unknown file key `{key}` (expected path/content)"
            )));
        }
        let path = object
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| ScaffoldError::Parse("every file needs a string `path`".to_string()))?;
        let content = object
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| ScaffoldError::Parse("every file needs string `content`".to_string()))?;
        plan.files.push(ScaffoldFile {
            path: path.to_string(),
            content: content.to_string(),
        });
    }
    validate_plan(&plan)?;
    Ok(plan)
}

/// Find and decode the first usable JSON object in the model's answer.
/// Balanced braces inside strings are ignored; prose before/after the object
/// is skipped. Each candidate is tried as-is and then with a conservative
/// repair (unescaped control characters and trailing commas).
fn parse_json_object(text: &str) -> Result<Value, ScaffoldError> {
    let mut last_error: Option<String> = None;
    for candidate in balanced_json_objects(text) {
        let repaired = repair_json_text(&candidate);
        for attempt in [candidate.as_str(), repaired.as_str()] {
            match serde_json::from_str::<Value>(attempt) {
                Ok(value) if value.get("files").is_some_and(Value::is_array) => {
                    return Ok(value);
                }
                Ok(value) if value.is_object() => {
                    last_error = Some("missing `files` array".to_string());
                }
                Ok(_) => {
                    last_error = Some("top level must be an object".to_string());
                }
                Err(error) => last_error = Some(error.to_string()),
            }
        }
    }
    Err(ScaffoldError::Parse(
        last_error.unwrap_or_else(|| "no JSON object found".to_string()),
    ))
}

/// Extract balanced `{...}` candidates. The scanner tracks JSON strings so a
/// brace in a Markdown body or a quoted prompt does not end the object.
fn balanced_json_objects(text: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    for (start, _) in text.match_indices('{') {
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        for (offset, ch) in text[start..].char_indices() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    in_string = false;
                }
                continue;
            }
            match ch {
                '"' => in_string = true,
                '{' => depth += 1,
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        let end = start + offset + ch.len_utf8();
                        candidates.push(text[start..end].to_string());
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    candidates
}

/// Repair the two common model-JSON defects without touching the strings'
/// meaning: escape raw control characters inside strings and drop trailing
/// commas before `}` or `]`.
fn repair_json_text(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 16);
    let mut in_string = false;
    let mut escaped = false;
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if in_string {
            if escaped {
                out.push(ch);
                escaped = false;
            } else if ch == '\\' {
                out.push(ch);
                escaped = true;
            } else if ch == '"' {
                in_string = false;
                out.push(ch);
            } else {
                match ch {
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    control if control < ' ' => {
                        out.push_str(&format!("\\u{:04x}", control as u32));
                    }
                    _ => out.push(ch),
                }
            }
        } else if ch == '"' {
            in_string = true;
            out.push(ch);
        } else if ch == ',' {
            let mut next = index + 1;
            while next < chars.len() && chars[next].is_whitespace() {
                next += 1;
            }
            if next >= chars.len() || !matches!(chars[next], '}' | ']') {
                out.push(ch);
            }
            // A comma directly before a closing delimiter is dropped.
        } else {
            out.push(ch);
        }
        index += 1;
    }
    out
}

/// Validate a plan without touching the filesystem.
pub fn validate_plan(plan: &ScaffoldPlan) -> Result<(), ScaffoldError> {
    if plan.files.is_empty() {
        return Err(ScaffoldError::Empty);
    }
    if plan.files.len() > MAX_FILES {
        return Err(ScaffoldError::TooMany);
    }
    let mut seen = BTreeSet::new();
    for file in &plan.files {
        validate_path(&file.path)?;
        if file.content.trim().is_empty() {
            return Err(ScaffoldError::EmptyFile {
                path: file.path.clone(),
            });
        }
        if file.content.len() > MAX_FILE_BYTES {
            return Err(ScaffoldError::TooLarge {
                path: file.path.clone(),
            });
        }
        if !seen.insert(file.path.as_str()) {
            return Err(ScaffoldError::Duplicate {
                path: file.path.clone(),
            });
        }
    }
    Ok(())
}

/// Validate one relative path (D-044 allowlist). `Path::components` rejects
/// `..`, absolute roots and Windows prefixes before any file is touched.
pub fn validate_path(path: &str) -> Result<(), ScaffoldError> {
    let invalid = |reason: &str| ScaffoldError::InvalidPath {
        path: path.to_string(),
        reason: reason.to_string(),
    };
    if path.is_empty() || path.chars().count() > MAX_PATH_CHARS {
        return Err(invalid("empty or too long"));
    }
    if path.trim() != path || path.ends_with('/') {
        return Err(invalid("no surrounding whitespace or trailing slash"));
    }
    if path.starts_with('/') || path.contains('\\') {
        return Err(invalid("only forward-slash relative paths"));
    }
    if RESERVED_PATHS.contains(&path) {
        return Err(ScaffoldError::Reserved {
            path: path.to_string(),
        });
    }
    let mut names = Vec::new();
    for component in Path::new(path).components() {
        let Component::Normal(name) = component else {
            return Err(invalid("`..`, `.` and absolute paths are not allowed"));
        };
        let name = name
            .to_str()
            .ok_or_else(|| invalid("the path must be UTF-8"))?;
        if name.is_empty() || name.starts_with('.') {
            return Err(invalid("hidden names are not allowed"));
        }
        names.push(name.to_string());
    }
    if names.len() < 2 {
        return Err(invalid(
            "the path must name a file inside an allowed folder",
        ));
    }
    if !ALLOWED_ROOTS.contains(&names[0].as_str()) {
        return Err(invalid(
            "the top-level folder must be bible, structure, research, notes or screenplay",
        ));
    }
    let file = names.last().expect("length checked above");
    if file.eq_ignore_ascii_case("AGENTS.md")
        || file.eq_ignore_ascii_case("agents.yaml")
        || file.eq_ignore_ascii_case("project.yaml")
    {
        return Err(ScaffoldError::Reserved {
            path: path.to_string(),
        });
    }
    let extension = Path::new(file)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("");
    if !extension.eq_ignore_ascii_case("md") && !extension.eq_ignore_ascii_case("fountain") {
        return Err(invalid("only .md and .fountain files are allowed"));
    }
    Ok(())
}

/// Write a reviewed plan with create-only semantics. If any write fails, the
/// files created earlier in this call are removed and the error names the
/// failing path. Existing files are never overwritten.
pub fn write_plan(root: &Path, plan: &ScaffoldPlan) -> Result<(), ScaffoldError> {
    validate_plan(plan)?;
    let mut created = Vec::new();
    for file in &plan.files {
        match document::create_document(root, &file.path, &file.content) {
            Ok(()) => created.push(file.path.clone()),
            Err(error) => {
                for path in created.iter().rev() {
                    let _ = document::delete_document(root, path);
                }
                return Err(ScaffoldError::Write {
                    path: file.path.clone(),
                    error: error.to_string(),
                });
            }
        }
    }
    Ok(())
}

/// The `project_scaffold` task: explicit dialog input in, a validated plan
/// out. It reads no project content and writes nothing; the UI reviews the
/// plan and calls [`write_plan`] only after confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectScaffoldTask {
    pub project_root: String,
    pub title: String,
    pub project_type: ProjectType,
    pub language: String,
    pub prompt: String,
    /// Human-readable type label for custom packages; built-ins fall back to
    /// the enum label.
    pub type_label: Option<String>,
    pub role: Option<Role>,
}

impl ProjectScaffoldTask {
    pub const KIND: &'static str = KIND;

    pub fn new(
        project_root: impl Into<String>,
        title: impl Into<String>,
        project_type: ProjectType,
        language: impl Into<String>,
        prompt: impl Into<String>,
    ) -> Self {
        Self {
            project_root: project_root.into(),
            title: title.into(),
            project_type,
            language: language.into(),
            prompt: prompt.into(),
            type_label: None,
            role: None,
        }
    }

    pub fn with_type_label(mut self, label: impl Into<String>) -> Self {
        self.type_label = Some(label.into());
        self
    }

    pub fn with_role(mut self, role: Role) -> Self {
        self.role = Some(role);
        self
    }

    pub fn run(
        &self,
        adapter: &dyn ModelAdapter,
        options: &TaskOptions,
        mut emit: impl FnMut(TaskEvent),
    ) -> Result<TaskOutput, AgentError> {
        let started = Instant::now();
        if self.project_root.trim().is_empty() {
            return Err(AgentError::InvalidInput("project root must not be empty"));
        }
        if self.title.trim().is_empty() {
            return Err(AgentError::InvalidInput("project title must not be empty"));
        }
        if self.language.trim().is_empty() {
            return Err(AgentError::InvalidInput(
                "project language must not be empty",
            ));
        }
        if self.prompt.trim().is_empty() {
            return Err(AgentError::InvalidInput(
                "scaffold prompt must not be empty",
            ));
        }
        if let Some(role) = &self.role
            && !role.allows(Self::KIND)
        {
            return Err(AgentError::RoleDenied {
                role: role.id.clone(),
                task: Self::KIND.to_string(),
            });
        }
        if options.cancel.is_cancelled() {
            return Err(AgentError::Cancelled);
        }

        emit(TaskEvent::Started {
            task: Self::KIND.to_string(),
            project: self.project_root.clone(),
        });
        emit(TaskEvent::Progress {
            stage: "plan".to_string(),
            detail: "Pidiendo una base de proyecto al modelo".to_string(),
        });

        let sources = vec![ContextSource {
            kind: "dialog".to_string(),
            location: "new_project".to_string(),
            detail: "Título, tipo, idioma y prompt del diálogo".to_string(),
        }];
        emit(TaskEvent::ContextCollected {
            sources: sources.clone(),
        });

        let system = self.role.as_ref().map_or_else(
            || BASE_SYSTEM.to_string(),
            |role| role.system_with(BASE_SYSTEM),
        );
        let prompt = format!(
            "{RULES}\n\nPROYECTO: {}\nTIPO: {}\nIDIOMA: {}\nPETICIÓN DEL USUARIO:\n{}\n",
            self.title.trim(),
            self.type_label
                .as_deref()
                .unwrap_or_else(|| project_type_label(self.project_type)),
            self.language.trim(),
            self.prompt.trim()
        );
        let request = ModelRequest {
            system,
            prompt,
            max_output_tokens: SCAFFOLD_MAX_TOKENS,
            tools: Vec::new(),
            history: Vec::new(),
        };
        if started.elapsed() >= options.timeout {
            return Err(AgentError::Timeout(options.timeout));
        }
        let response = if options.stream {
            adapter.complete_streaming(&request, &options.cancel, &mut |chunk| {
                emit(TaskEvent::ModelDelta {
                    text: chunk.to_string(),
                });
            })?
        } else {
            adapter.complete(&request, &options.cancel)?
        };
        if options.cancel.is_cancelled() {
            return Err(AgentError::Cancelled);
        }
        if started.elapsed() > options.timeout {
            return Err(AgentError::Timeout(options.timeout));
        }

        emit(TaskEvent::ModelResponded {
            model: response.model.clone(),
            output_chars: response.text.chars().count(),
        });
        let plan =
            parse_plan(&response.text).map_err(|error| AgentError::Adapter(error.to_string()))?;
        let files = plan.files.clone();
        let data = json!({
            "plan": plan,
            "files": files,
            "adapter": adapter.name(),
            "model": response.model,
            "role": self.role.as_ref().map(|role| role.id.as_str()),
            "role_source": self.role.as_ref().map(|role| role.origin_key()),
            "usage": {
                "input_tokens": response.input_tokens,
                "output_tokens": response.output_tokens,
            },
            "elapsed_ms": started.elapsed().as_millis() as u64,
        });
        let summary = format!(
            "Base del proyecto propuesta: {} archivos (revisión previa).",
            data["plan"]["files"]
                .as_array()
                .map(Vec::len)
                .unwrap_or_default()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{ModelResponse, roles};
    use crate::cancel::CancellationToken;
    use std::time::Duration;

    fn plan_text(path: &str) -> String {
        format!(
            "{{\"files\":[{{\"path\":\"{path}\",\"content\":\"# Documento\\n\\nTexto.\\n\"}}]}}"
        )
    }

    fn options() -> TaskOptions {
        TaskOptions {
            timeout: Duration::from_secs(5),
            cancel: CancellationToken::new(),
            stream: false,
        }
    }

    struct Fixed(String);

    impl ModelAdapter for Fixed {
        fn name(&self) -> &str {
            "fixed"
        }

        fn complete(
            &self,
            _: &ModelRequest,
            _: &CancellationToken,
        ) -> Result<ModelResponse, AgentError> {
            Ok(ModelResponse {
                text: self.0.clone(),
                model: "fixed".to_string(),
                tool_calls: Vec::new(),
                input_tokens: None,
                output_tokens: None,
            })
        }
    }

    #[test]
    fn valid_plan_parses_and_tolerates_a_fence() {
        let text = plan_text("bible/premise.md");
        let fenced = format!("```json\n{text}\n```");
        let plan = parse_plan(&fenced).expect("valid plan");
        assert_eq!(plan.files.len(), 1);
        assert_eq!(plan.files[0].path, "bible/premise.md");
    }

    #[test]
    fn parses_raw_newlines_braces_in_prose_and_trailing_commas() {
        // Models often emit literal newlines inside Markdown content instead
        // of JSON `\n` escapes; the repair must preserve the text.
        let raw =
            "{\"files\":[{\"path\":\"bible/a.md\",\"content\":\"# A\nline with } and { braces\"}]}";
        let plan = parse_plan(raw).expect("raw newlines");
        assert_eq!(plan.files[0].content, "# A\nline with } and { braces");

        // Prose before the real object may contain its own braces or even a
        // different JSON object without `files`.
        let prose = "{\"note\":\"ejemplo\"}. Aquí está el plan {no es JSON}. {\"files\":[{\"path\":\"notes/a.md\",\"content\":\"x\"}]} Gracias.";
        let plan = parse_plan(prose).expect("prose");
        assert_eq!(plan.files[0].path, "notes/a.md");

        // A trailing comma before a closing delimiter is tolerated.
        let trailing = "{\"files\":[{\"path\":\"bible/a.md\",\"content\":\"x\",},],}";
        let plan = parse_plan(trailing).expect("trailing commas");
        assert_eq!(plan.files[0].content, "x");
    }

    #[test]
    fn rejects_traversal_absolute_reserved_and_unknown_paths() {
        for path in [
            "../outside.md",
            "/tmp/outside.md",
            ".narra/agents/x/agent.yaml",
            "project.yaml",
            "bible/../../outside.md",
            "bible/characters/.hidden.md",
            "bible/characters",
            "bible/characters/marta.txt",
            "assets/logo.md",
            "bible/AGENTS.md",
        ] {
            let error = parse_plan(&plan_text(path)).expect_err(path);
            assert!(
                matches!(
                    error,
                    ScaffoldError::InvalidPath { .. } | ScaffoldError::Reserved { .. }
                ),
                "{path}: {error}"
            );
        }
    }

    #[test]
    fn rejects_empty_duplicate_oversized_and_too_many_files() {
        let empty = parse_plan("{\"files\":[]}").expect_err("empty");
        assert!(matches!(empty, ScaffoldError::Empty), "{empty}");

        let duplicate = r#"{"files":[{"path":"bible/a.md","content":"a"},{"path":"bible/a.md","content":"b"}]}"#;
        let error = parse_plan(duplicate).expect_err("duplicate");
        assert!(matches!(error, ScaffoldError::Duplicate { .. }), "{error}");

        let big = "x".repeat(MAX_FILE_BYTES + 1);
        let oversized =
            format!("{{\"files\":[{{\"path\":\"bible/a.md\",\"content\":\"{big}\"}}]}}");
        let error = parse_plan(&oversized).expect_err("oversized");
        assert!(matches!(error, ScaffoldError::TooLarge { .. }), "{error}");

        let files: Vec<String> = (0..=MAX_FILES)
            .map(|index| format!("{{\"path\":\"bible/f{index}.md\",\"content\":\"x\"}}"))
            .collect();
        let too_many = format!("{{\"files\":[{}]}}", files.join(","));
        let error = parse_plan(&too_many).expect_err("too many");
        assert!(matches!(error, ScaffoldError::TooMany), "{error}");
    }

    #[test]
    fn write_plan_is_create_only_and_rolls_back() {
        let dir = tempfile::tempdir().expect("tempdir");
        document::create_document(dir.path(), "bible/existing.md", "keep").expect("existing file");
        let plan = ScaffoldPlan {
            files: vec![
                ScaffoldFile {
                    path: "bible/ok.md".to_string(),
                    content: "# Ok\n".to_string(),
                },
                ScaffoldFile {
                    path: "bible/existing.md".to_string(),
                    content: "# Replace\n".to_string(),
                },
            ],
        };
        let error = write_plan(dir.path(), &plan).expect_err("existing target");
        assert!(matches!(error, ScaffoldError::Write { .. }), "{error}");
        assert!(!dir.path().join("bible/ok.md").exists());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("bible/existing.md")).expect("read"),
            "keep"
        );
    }

    #[test]
    fn write_plan_creates_the_reviewed_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let plan = ScaffoldPlan {
            files: vec![
                ScaffoldFile {
                    path: "bible/premise.md".to_string(),
                    content: "# Premisa\n".to_string(),
                },
                ScaffoldFile {
                    path: "structure/outline.md".to_string(),
                    content: "# Escaleta\n".to_string(),
                },
            ],
        };
        write_plan(dir.path(), &plan).expect("write");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("bible/premise.md")).expect("read"),
            "# Premisa\n"
        );
        assert!(dir.path().join("structure/outline.md").exists());
    }

    #[test]
    fn task_returns_a_validated_plan_from_the_model() {
        let dir = tempfile::tempdir().expect("tempdir");
        let task = ProjectScaffoldTask::new(
            dir.path().to_string_lossy(),
            "Luces de Invierno",
            ProjectType::Feature,
            "es",
            "Una historia de invierno.",
        );
        let output = task
            .run(&Fixed(plan_text("bible/premise.md")), &options(), |_| {})
            .expect("scaffold");
        assert_eq!(
            output.data["plan"]["files"].as_array().map(Vec::len),
            Some(1)
        );
        assert!(output.summary.contains("1 archivos"));
    }

    #[test]
    fn task_rejects_prose_and_roles_without_the_permission() {
        let dir = tempfile::tempdir().expect("tempdir");
        let task = ProjectScaffoldTask::new(
            dir.path().to_string_lossy(),
            "Proyecto",
            ProjectType::Short,
            "es",
            "Una escena.",
        );
        let error = task
            .run(&Fixed("no soy JSON".to_string()), &options(), |_| {})
            .expect_err("prose");
        assert!(matches!(error, AgentError::Adapter(_)), "{error}");

        let researcher = roles::builtin_roles()
            .into_iter()
            .find(|role| role.id == "researcher")
            .expect("researcher");
        let error = task
            .clone()
            .with_role(researcher)
            .run(&Fixed("{}".to_string()), &options(), |_| {})
            .expect_err("role denied");
        assert!(matches!(error, AgentError::RoleDenied { .. }), "{error}");
    }
}
