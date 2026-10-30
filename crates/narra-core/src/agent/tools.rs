//! Agent tools with explicit permissions (Milestone 4, slice 3b).
//!
//! Spike C evidence: "permission enforcement and sandboxing". Product rules
//! (`AGENTS.md`):
//!
//! - least privilege: tools are classified [`ToolPermission::ReadOnly`],
//!   [`ToolPermission::ProposeWrite`] or [`ToolPermission::Destructive`];
//! - a policy decides *before* execution, deny-by-default for anything that
//!   can change state; the headless harness cannot "ask", so an undecided
//!   request is an actionable denial, never a silent grant;
//! - AI-visible changes are proposals, never canonical writes: the only
//!   write-capable tool stages a [`FileProposal`] for review;
//! - destructive operations (applying a proposal to a canonical file)
//!   require a **separate, explicit** grant that a blanket read/write grant
//!   can never imply;
//! - every call — allowed, denied or failed — is recorded in the returned
//!   [`ToolResult`] so the run is auditable.
//!
//! Execution is sandboxed to the project: every tool reads or writes through
//! boundary-checked document IO (no path traversal, no writes outside
//! `<project>/.narra/` + the reviewed canonical apply).

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::proposal::{self, FileProposal, Origin};
use super::skill::Skill;
use crate::fountain;

/// Maximum characters returned by `read_document` in one call.
const READ_DOCUMENT_MAX_CHARS: usize = 8_000;

/// A document the read-only tools may open: project-relative, non-hidden,
/// narrative text only. The manifest, agent config and coding-agent
/// instructions stay out; the brief already carries the manifest and the
/// dedicated `.narra/instructions.md`.
fn is_tool_document_path(path: &str) -> bool {
    if !super::context::is_text_path(path)
        || path.starts_with('/')
        || path.contains('\\')
        || path == "project.yaml"
        || path == "agents.yaml"
        || path == "AGENTS.md"
        || path == "exports"
        || path.starts_with("exports/")
    {
        return false;
    }
    !path
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == ".." || part.starts_with('.'))
}

/// How much a tool is allowed to do. Every tool carries exactly one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolPermission {
    /// Reads project content; cannot change anything.
    ReadOnly,
    /// Proposes a change as a reviewable [`FileProposal`]; never writes
    /// canonical files by itself.
    ProposeWrite,
    /// Writes canonical content (applies a reviewed proposal). Requires its
    /// own explicit grant.
    Destructive,
}

/// Declared capability of one tool, in the provider-neutral shape the
/// OpenAI-compatible adapter maps to its wire `tools` format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema for the arguments (OpenAI function-calling convention).
    pub parameters: Value,
    pub permission: ToolPermission,
}

/// A tool invocation, either requested by the model (provider wire format)
/// or constructed directly by a client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Provider correlation id (echoed back to the provider).
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// Prior conversation turns needed for a tool loop: the model's tool
/// requests and the tool results fed back, in order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Always [`ChatRole::Tool`] or [`ChatRole::Assistant`] in a loop; the
    /// adapter's own system/user turns are built from the request itself.
    pub role: ChatRole,
    /// Assistant turn text (may be empty when the model only requested
    /// tools).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub content: String,
    /// Tool calls the assistant requested in this turn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// For tool results: the `ToolCall::id` this answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Tool name for result messages (display/audit).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
}

/// Role of a [`ChatMessage`] turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    Assistant,
    Tool,
}

/// The policy's answer for one call. `Ask` is the desktop UI's prompt
/// opportunity; headless clients map it to an actionable denial.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionDecision {
    Allow,
    /// Needs explicit user confirmation; the string tells the user what to
    /// grant.
    Ask(String),
    /// Refused outright with an actionable reason.
    Deny(String),
}

/// Decides whether a call may execute, before any execution happens.
pub trait PermissionPolicy {
    fn decide(&mut self, call: &ToolCall, permission: ToolPermission) -> PermissionDecision;
}

/// Deny-by-default policy (`AGENTS.md`: explicit permissions).
///
/// - read-only tools are allowed (they cannot change anything);
/// - [`ToolPermission::ProposeWrite`] tools are `Ask` unless explicitly
///   granted;
/// - [`ToolPermission::Destructive`] tools are denied unless granted **in
///   the destructive set** — a write grant never implies them.
#[derive(Debug, Clone, Default)]
pub struct GrantPolicy {
    granted: BTreeSet<String>,
    destructive: BTreeSet<String>,
}

impl GrantPolicy {
    /// Policy with nothing granted beyond read-only.
    pub fn new() -> Self {
        Self::default()
    }

    /// Explicitly grant a tool by name (propose-write level).
    pub fn allow(mut self, tool: impl Into<String>) -> Self {
        self.granted.insert(tool.into());
        self
    }

    /// Explicitly grant a **destructive** tool by name. Separate from
    /// [`GrantPolicy::allow`] by design: a blanket write grant can never
    /// silently imply canonical writes.
    pub fn allow_destructive(mut self, tool: impl Into<String>) -> Self {
        self.destructive.insert(tool.into());
        self
    }
}

impl PermissionPolicy for GrantPolicy {
    fn decide(&mut self, call: &ToolCall, permission: ToolPermission) -> PermissionDecision {
        match permission {
            ToolPermission::ReadOnly => PermissionDecision::Allow,
            ToolPermission::ProposeWrite => {
                if self.granted.contains(&call.name) {
                    PermissionDecision::Allow
                } else {
                    PermissionDecision::Ask(format!(
                        "la herramienta «{}» propone cambios en el guion; \
                         confirma su uso (grant: allow «{}»)",
                        call.name, call.name
                    ))
                }
            }
            ToolPermission::Destructive => {
                if self.destructive.contains(&call.name) {
                    PermissionDecision::Allow
                } else {
                    PermissionDecision::Deny(format!(
                        "«{}» escribe archivos canónicos; requiere aprobación \
                         destructiva explícita (grant: allow_destructive «{}»)",
                        call.name, call.name
                    ))
                }
            }
        }
    }
}

/// Outcome of one executed (or refused) call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ToolOutcome {
    /// Plain text result (read-only tools).
    Text(String),
    /// A reviewable proposal was staged; canonical files untouched.
    ProposalCreated {
        id: String,
        target: String,
        hunks: usize,
    },
    /// A proposal was applied to its canonical target.
    ProposalApplied { id: String, target: String },
    /// The policy refused the call; nothing ran.
    Denied { reason: String },
    /// The call was allowed but failed; nothing partial was left behind.
    Failed { message: String },
}

/// Audit record of one call: what was asked, how the policy ruled and what
/// happened (`DATA_MODEL.md`: task output records tool actions).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    pub call_id: String,
    pub tool: String,
    pub permission: ToolPermission,
    pub decision: PermissionDecision,
    pub outcome: ToolOutcome,
    /// Wall-clock milliseconds the call took (0 when denied before running).
    pub elapsed_ms: u128,
}

impl ToolResult {
    pub fn succeeded(&self) -> bool {
        matches!(
            self.outcome,
            ToolOutcome::Text(_) | ToolOutcome::ProposalCreated { .. }
        )
    }
}

/// Executes tool calls against one project, applying a [`PermissionPolicy`]
/// first and recording an audit [`ToolResult`] for every call.
pub struct ToolExecutor<'a> {
    root: &'a Path,
    skills: Vec<Skill>,
}

/// Tools the executor understands, with their permission class and schema.
pub fn tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "list_documents".to_string(),
            description: "Lista los documentos narrativos del proyecto, en \
                          orden alfabético, con su tamaño en bytes."
                .to_string(),
            parameters: json!({"type": "object", "properties": {}}),
            permission: ToolPermission::ReadOnly,
        },
        ToolSpec {
            name: "search_project".to_string(),
            description: "Busca un texto literal en todos los documentos del \
                          proyecto y devuelve documento, línea, columna y una \
                          vista previa."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {"query": {"type": "string"}},
                "required": ["query"]
            }),
            permission: ToolPermission::ReadOnly,
        },
        ToolSpec {
            name: "read_document".to_string(),
            description: "Lee un documento del proyecto, opcionalmente un \
                          rango de líneas 1-based. La salida está acotada; usa \
                          start_line/end_line para continuar un archivo largo."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "start_line": {"type": "integer", "minimum": 1},
                    "end_line": {"type": "integer", "minimum": 1}
                },
                "required": ["path"]
            }),
            permission: ToolPermission::ReadOnly,
        },
        ToolSpec {
            name: "load_skill".to_string(),
            description: "Carga el cuerpo de una skill de confianza por su id. \
                          El prompt solo muestra el catálogo; usa esta \
                          herramienta cuando la tarea encaje con una skill."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {"id": {"type": "string"}},
                "required": ["id"]
            }),
            permission: ToolPermission::ReadOnly,
        },
        ToolSpec {
            name: "read_scene".to_string(),
            description: "Devuelve el texto de una escena del guion principal \
                          (1-based, en orden del archivo)."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {"scene": {"type": "integer", "minimum": 1}},
                "required": ["scene"]
            }),
            permission: ToolPermission::ReadOnly,
        },
        ToolSpec {
            name: "propose_scene_edit".to_string(),
            description: "Propone una reescritura de una escena. La \
                          modificación NO se aplica: se guarda como propuesta \
                          para revisión humana."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "scene": {"type": "integer", "minimum": 1},
                    "instruction": {"type": "string"},
                    "rewritten_scene": {"type": "string"}
                },
                "required": ["scene", "instruction", "rewritten_scene"]
            }),
            permission: ToolPermission::ProposeWrite,
        },
        ToolSpec {
            name: "apply_proposal".to_string(),
            description: "Aplica una propuesta revisada a su archivo \
                          canónico. Operación destructiva: solo con \
                          aprobación explícita del usuario."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {"id": {"type": "string"}},
                "required": ["id"]
            }),
            permission: ToolPermission::Destructive,
        },
    ]
}

/// Read-only subset used by Plan and Review.
pub fn read_only_specs() -> Vec<ToolSpec> {
    tool_specs()
        .into_iter()
        .filter(|spec| spec.permission == ToolPermission::ReadOnly)
        .collect()
}

fn permission_of(name: &str) -> Option<ToolPermission> {
    tool_specs()
        .into_iter()
        .find(|spec| spec.name == name)
        .map(|spec| spec.permission)
}

impl<'a> ToolExecutor<'a> {
    pub fn new(root: &'a Path) -> Self {
        Self {
            root,
            skills: Vec::new(),
        }
    }

    /// Make the trusted skills available to `load_skill`. The caller is
    /// responsible for passing only active/trusted skills.
    pub fn with_skills(mut self, skills: Vec<Skill>) -> Self {
        self.skills = skills;
        self
    }

    /// Run one call: the policy rules first, then the tool executes (or the
    /// refusal is recorded). Always returns a result — never panics, never
    /// leaves a partial write.
    pub fn execute(
        &mut self,
        call: &ToolCall,
        policy: &mut dyn PermissionPolicy,
        origin: &Origin,
    ) -> ToolResult {
        let Some(permission) = permission_of(&call.name) else {
            // Unknown tool: refuse with an actionable message listing names.
            let known = tool_specs()
                .into_iter()
                .map(|spec| spec.name)
                .collect::<Vec<_>>()
                .join(", ");
            return ToolResult {
                call_id: call.id.clone(),
                tool: call.name.clone(),
                permission: ToolPermission::ReadOnly,
                decision: PermissionDecision::Deny("herramienta desconocida".to_string()),
                outcome: ToolOutcome::Denied {
                    reason: format!(
                        "herramienta desconocida «{}»; disponibles: {known}",
                        call.name
                    ),
                },
                elapsed_ms: 0,
            };
        };

        let decision = policy.decide(call, permission);
        let started = std::time::Instant::now();
        let outcome = match &decision {
            PermissionDecision::Allow => self.dispatch(call, permission, origin),
            PermissionDecision::Ask(reason) | PermissionDecision::Deny(reason) => {
                ToolOutcome::Denied {
                    reason: reason.clone(),
                }
            }
        };
        ToolResult {
            call_id: call.id.clone(),
            tool: call.name.clone(),
            permission,
            decision,
            outcome,
            elapsed_ms: started.elapsed().as_millis(),
        }
    }

    fn dispatch(
        &mut self,
        call: &ToolCall,
        permission: ToolPermission,
        origin: &Origin,
    ) -> ToolOutcome {
        debug_assert!(matches!(
            permission,
            ToolPermission::ReadOnly | ToolPermission::ProposeWrite | ToolPermission::Destructive
        ));
        match call.name.as_str() {
            "list_documents" => self.list_documents(),
            "search_project" => self.search_project(call),
            "read_document" => self.read_document(call),
            "load_skill" => self.load_skill(call),
            "read_scene" => self.read_scene(call),
            "propose_scene_edit" => self.propose_scene_edit(call, origin),
            "apply_proposal" => self.apply_proposal(call),
            other => ToolOutcome::Failed {
                // Unreachable while `permission_of` and `dispatch` agree;
                // kept as an explicit error rather than an unreachable!().
                message: format!("sin implementación para «{other}»"),
            },
        }
    }

    /// Read-only: list the project documents the assistant may open. Hidden
    /// state and exports are already excluded by `Project::list_documents`.
    fn list_documents(&self) -> ToolOutcome {
        let project = match crate::project::open_project(self.root) {
            Ok(project) => project,
            Err(error) => {
                return ToolOutcome::Failed {
                    message: error.to_string(),
                };
            }
        };
        let documents = match project.list_documents() {
            Ok(documents) => documents,
            Err(error) => {
                return ToolOutcome::Failed {
                    message: error.to_string(),
                };
            }
        };
        let shown = documents.len().min(200);
        let mut out = String::from("Documentos del proyecto:\n");
        for path in documents.iter().take(shown) {
            let size = std::fs::metadata(project.root().join(path))
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            out.push_str(&format!("- {path} ({size} B)\n"));
        }
        if documents.len() > shown {
            out.push_str(&format!(
                "- … {} documentos más no listados\n",
                documents.len() - shown
            ));
        }
        ToolOutcome::Text(out)
    }

    /// Read-only: literal project search, filtered to documents the read tool
    /// may open. The search core already bounds the scan and hit count.
    fn search_project(&self, call: &ToolCall) -> ToolOutcome {
        let Some(query) = call.arguments.get("query").and_then(Value::as_str) else {
            return ToolOutcome::Failed {
                message: "argumento «query» ausente".to_string(),
            };
        };
        if query.trim().is_empty() {
            return ToolOutcome::Failed {
                message: "la búsqueda no puede estar vacía".to_string(),
            };
        }
        let project = match crate::project::open_project(self.root) {
            Ok(project) => project,
            Err(error) => {
                return ToolOutcome::Failed {
                    message: error.to_string(),
                };
            }
        };
        let report = match crate::search::search(&project, query) {
            Ok(report) => report,
            Err(error) => {
                return ToolOutcome::Failed {
                    message: error.to_string(),
                };
            }
        };
        let hits: Vec<_> = report
            .hits
            .iter()
            .filter(|hit| is_tool_document_path(&hit.document))
            .take(80)
            .collect();
        if hits.is_empty() {
            return ToolOutcome::Text(format!(
                "Sin coincidencias para «{}» en los documentos del proyecto.",
                query.trim()
            ));
        }
        let mut out = format!(
            "Coincidencias para «{}» ({} mostradas de {}):\n",
            query.trim(),
            hits.len(),
            report.hits.len()
        );
        for hit in hits {
            out.push_str(&format!(
                "- {}:{}:{} — {}\n",
                hit.document, hit.line, hit.column, hit.preview
            ));
        }
        if report.truncated {
            out.push_str("- … la búsqueda alcanzó el límite de resultados\n");
        }
        ToolOutcome::Text(out)
    }

    /// Read-only: read one text document, optionally a 1-based line range.
    /// The result is capped so a large file cannot flood the model context.
    fn read_document(&self, call: &ToolCall) -> ToolOutcome {
        let Some(path) = call.arguments.get("path").and_then(Value::as_str) else {
            return ToolOutcome::Failed {
                message: "argumento «path» ausente".to_string(),
            };
        };
        if !is_tool_document_path(path) {
            return ToolOutcome::Failed {
                message: format!(
                    "«{path}» no es un documento narrativo legible; usa \
                     list_documents para ver las rutas disponibles"
                ),
            };
        }
        let start = call
            .arguments
            .get("start_line")
            .and_then(Value::as_u64)
            .map(|value| value.max(1) as usize);
        let end = call
            .arguments
            .get("end_line")
            .and_then(Value::as_u64)
            .map(|value| value.max(1) as usize);
        if let (Some(start), Some(end)) = (start, end)
            && end < start
        {
            return ToolOutcome::Failed {
                message: "«end_line» no puede ser menor que «start_line»".to_string(),
            };
        }
        let project = match crate::project::open_project(self.root) {
            Ok(project) => project,
            Err(error) => {
                return ToolOutcome::Failed {
                    message: error.to_string(),
                };
            }
        };
        let content = match project.read_document(path) {
            Ok(content) => content,
            Err(error) => {
                return ToolOutcome::Failed {
                    message: error.to_string(),
                };
            }
        };
        let lines: Vec<&str> = content.lines().collect();
        let total = lines.len();
        if total == 0 {
            return ToolOutcome::Text(format!("{path} está vacío."));
        }
        let first = start.unwrap_or(1);
        if first > total {
            return ToolOutcome::Failed {
                message: format!("«{path}» solo tiene {total} líneas"),
            };
        }
        let last = end.unwrap_or(total).min(total);
        let mut out = format!("{path} (líneas {first}–{last} de {total}):\n");
        let mut used = 0usize;
        for (index, line) in lines[first - 1..last].iter().enumerate() {
            let remaining = READ_DOCUMENT_MAX_CHARS.saturating_sub(used);
            if remaining == 0 {
                out.push_str("… (salida truncada; pide otro rango)\n");
                break;
            }
            let numbered = format!("{}: {line}\n", first + index);
            let chunk: String = numbered.chars().take(remaining).collect();
            used += chunk.chars().count();
            out.push_str(&chunk);
            if chunk.chars().count() < numbered.chars().count() {
                out.push_str("… (salida truncada; pide otro rango)\n");
                break;
            }
        }
        ToolOutcome::Text(out)
    }

    /// Read-only: return a trusted skill body. The catalog is supplied by
    /// the caller, so untrusted or changed skills never reach this tool.
    fn load_skill(&self, call: &ToolCall) -> ToolOutcome {
        let Some(id) = call.arguments.get("id").and_then(Value::as_str) else {
            return ToolOutcome::Failed {
                message: "argumento «id» ausente".to_string(),
            };
        };
        let Some(skill) = self.skills.iter().find(|skill| skill.id == id) else {
            return ToolOutcome::Failed {
                message: format!("skill «{id}» no disponible o no confiada"),
            };
        };
        ToolOutcome::Text(format!(
            "# Skill: {}\n\n{}\n\n{}",
            skill.name, skill.description, skill.body
        ))
    }

    /// Read-only: extract one scene, exactly like the context retrieval the
    /// `scene_edit` task shows to the user.
    fn read_scene(&self, call: &ToolCall) -> ToolOutcome {
        let scene_number = match call.arguments.get("scene").and_then(Value::as_u64) {
            Some(n) if n >= 1 => n as usize,
            _ => {
                return ToolOutcome::Failed {
                    message: "argumento «scene» ausente o inválido (entero ≥ 1)".to_string(),
                };
            }
        };
        match self.scene_text(scene_number) {
            Ok(text) => ToolOutcome::Text(text),
            Err(message) => ToolOutcome::Failed { message },
        }
    }

    /// Propose-write: stages a [`FileProposal`] from the model's rewritten
    /// text. Canonical files are untouched; the record is reviewable and
    /// renewable (`SceneEditTask::from_proposal` round-trips).
    fn propose_scene_edit(&self, call: &ToolCall, origin: &Origin) -> ToolOutcome {
        let Some(scene_number) = call.arguments.get("scene").and_then(Value::as_u64) else {
            return ToolOutcome::Failed {
                message: "argumento «scene» ausente o inválido (entero ≥ 1)".to_string(),
            };
        };
        let scene_number = scene_number as usize;
        let Some(instruction) = call.arguments.get("instruction").and_then(Value::as_str) else {
            return ToolOutcome::Failed {
                message: "argumento «instruction» ausente".to_string(),
            };
        };
        let Some(rewritten) = call
            .arguments
            .get("rewritten_scene")
            .and_then(Value::as_str)
        else {
            return ToolOutcome::Failed {
                message: "argumento «rewritten_scene» ausente".to_string(),
            };
        };
        if instruction.trim().is_empty() {
            return ToolOutcome::Failed {
                message: "la instrucción no puede estar vacía".to_string(),
            };
        }

        // Load base content with the same boundary-checked IO the task uses.
        let project = match crate::project::open_project(self.root) {
            Ok(project) => project,
            Err(error) => {
                return ToolOutcome::Failed {
                    message: error.to_string(),
                };
            }
        };
        let screenplay_path = project.manifest().default_screenplay_path.clone();
        let base = match project.read_document(&screenplay_path) {
            Ok(base) => base,
            Err(error) => {
                return ToolOutcome::Failed {
                    message: error.to_string(),
                };
            }
        };
        let region = match scene_region(&base, scene_number) {
            Ok(region) => region,
            Err(message) => {
                return ToolOutcome::Failed { message };
            }
        };
        // The rewritten text replaces the trimmed scene region, mirroring
        // `SceneEditTask`: the blank separator stays outside the region.
        let proposed = rewrite_region(&base, &region, rewritten);
        let origin = Origin {
            task: "tool_loop".to_string(),
            adapter: origin.adapter.clone(),
            model: origin.model.clone(),
            task_input: json!({
                "scene_number": scene_number,
                "instruction": instruction,
            }),
        };
        match FileProposal::new(
            screenplay_path.clone(),
            &base,
            &proposed,
            instruction,
            origin,
            head_oid(self.root),
        ) {
            Ok(record) => {
                let hunks = record.hunks.len();
                let target = record.target.clone();
                let id = record.id.clone();
                match proposal::save(self.root, &record) {
                    Ok(()) => ToolOutcome::ProposalCreated { id, target, hunks },
                    Err(error) => ToolOutcome::Failed {
                        message: error.to_string(),
                    },
                }
            }
            Err(error) => ToolOutcome::Failed {
                message: error.to_string(),
            },
        }
    }

    /// Destructive: applies a reviewed proposal to its canonical file.
    fn apply_proposal(&self, call: &ToolCall) -> ToolOutcome {
        let Some(id) = call.arguments.get("id").and_then(Value::as_str) else {
            return ToolOutcome::Failed {
                message: "argumento «id» ausente".to_string(),
            };
        };
        match proposal::apply(self.root, id, None) {
            Ok(outcome) => ToolOutcome::ProposalApplied {
                id: outcome.proposal.id,
                target: outcome.proposal.target,
            },
            Err(error) => ToolOutcome::Failed {
                message: error.to_string(),
            },
        }
    }

    fn scene_text(&self, scene_number: usize) -> Result<String, String> {
        let project = crate::project::open_project(self.root).map_err(|error| error.to_string())?;
        let screenplay_path = project.manifest().default_screenplay_path.clone();
        let base = project
            .read_document(&screenplay_path)
            .map_err(|error| error.to_string())?;
        let region = scene_region(&base, scene_number)?;
        Ok(base
            .lines()
            .skip(region.start)
            .take(region.end - region.start)
            .collect::<Vec<_>>()
            .join("\n"))
    }
}

/// Git HEAD oid when the project is a repository — provenance only, same
/// rule as `SceneEditTask` (staleness is decided by content hash).
fn head_oid(root: &Path) -> Option<String> {
    let options = crate::git::OpOptions::default();
    let repository = crate::git::Repository::open(root, options.clone()).ok()?;
    repository
        .log(1, options)
        .ok()?
        .first()
        .map(|commit| commit.id.clone())
}

/// 0-based half-open line region of one scene, excluding the blank
/// separator that precedes the next heading (same rule as `SceneEditTask`).
struct SceneRegion {
    start: usize,
    end: usize,
}

fn scene_region(base: &str, scene_number: usize) -> Result<SceneRegion, String> {
    let scenes: Vec<usize> = fountain::parse(base)
        .iter()
        .filter(|element| element.kind == fountain::ElementKind::SceneHeading)
        .map(|element| element.line_number)
        .collect();
    if scenes.is_empty() {
        return Err("el guion no tiene escenas".to_string());
    }
    if scene_number > scenes.len() {
        return Err(format!(
            "la escena {scene_number} no existe: el guion tiene {} escenas",
            scenes.len()
        ));
    }
    let start = scenes[scene_number - 1] - 1;
    let end = scenes
        .get(scene_number)
        .map(|line| line - 1)
        .unwrap_or(base.lines().count());
    let mut region_end = end;
    let lines: Vec<&str> = base.lines().collect();
    while region_end > start && lines[region_end - 1].is_empty() {
        region_end -= 1;
    }
    Ok(SceneRegion {
        start,
        end: region_end,
    })
}

/// Replace a scene region with the rewritten text, keeping the blank
/// separator before the next scene untouched.
fn rewrite_region(base: &str, region: &SceneRegion, rewritten: &str) -> String {
    let lines: Vec<&str> = base.lines().collect();
    let mut out: Vec<&str> = lines[..region.start].to_vec();
    // Trim the model's answer exactly like `SceneEditTask` does.
    out.extend(rewritten.trim().lines());
    out.extend_from_slice(&lines[region.end..]);
    let mut text = out.join("\n");
    if base.ends_with('\n') {
        text.push('\n');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_project() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("narra-tools-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("screenplay")).expect("mkdir");
        std::fs::write(
            root.join("project.yaml"),
            "schema_version: 1\nproject_id: 00000000-0000-4000-8000-000000000001\n\
             title: Herramientas\nproject_type: feature\nprimary_language: es\n\
             additional_languages: []\ncreated_at: 2026-01-01T00:00:00Z\n\
             default_screenplay_path: screenplay/main.fountain\n",
        )
        .expect("manifest");
        std::fs::write(
            root.join("screenplay/main.fountain"),
            "INT. TALLER - NOCHE\n\nELENA\nEsto es una prueba.\n\nINT. CALLE - DÍA\n\nMARCOS\nHace sol.\n",
        )
        .expect("screenplay");
        root
    }

    fn origin() -> Origin {
        Origin {
            task: "tool_loop".to_string(),
            adapter: "mock".to_string(),
            model: "mock-1".to_string(),
            task_input: json!({"instruction": "prueba"}),
        }
    }

    fn call(id: &str, name: &str, arguments: Value) -> ToolCall {
        ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments,
        }
    }

    #[test]
    fn read_scene_is_allowed_by_default() {
        let root = sample_project();
        let mut executor = ToolExecutor::new(&root);
        let mut policy = GrantPolicy::new();
        let result = executor.execute(
            &call("c1", "read_scene", json!({"scene": 1})),
            &mut policy,
            &origin(),
        );
        assert_eq!(result.decision, PermissionDecision::Allow);
        assert!(result.succeeded(), "{result:?}");
        assert!(
            matches!(&result.outcome, ToolOutcome::Text(text) if text.contains("ELENA")),
            "{:?}",
            result.outcome
        );
    }

    #[test]
    fn read_only_tools_list_search_and_read_project_documents() {
        let root = sample_project();
        std::fs::create_dir_all(root.join("bible/characters")).expect("bible");
        std::fs::write(
            root.join("bible/characters/marta.md"),
            "# Marta\n\nMarta es la protagonista.\n",
        )
        .expect("character");
        let mut executor = ToolExecutor::new(&root);
        let mut policy = GrantPolicy::new();

        let listed = executor.execute(
            &call("c1", "list_documents", json!({})),
            &mut policy,
            &origin(),
        );
        assert!(
            matches!(&listed.outcome, ToolOutcome::Text(text) if text.contains("bible/characters/marta.md")),
            "{:?}",
            listed.outcome
        );

        let searched = executor.execute(
            &call("c2", "search_project", json!({"query": "protagonista"})),
            &mut policy,
            &origin(),
        );
        assert!(
            matches!(&searched.outcome, ToolOutcome::Text(text) if text.contains("bible/characters/marta.md:3")),
            "{:?}",
            searched.outcome
        );

        let read = executor.execute(
            &call(
                "c3",
                "read_document",
                json!({"path": "bible/characters/marta.md", "start_line": 3, "end_line": 3}),
            ),
            &mut policy,
            &origin(),
        );
        assert!(
            matches!(&read.outcome, ToolOutcome::Text(text) if text.contains("Marta es la protagonista.") && !text.contains("# Marta")),
            "{:?}",
            read.outcome
        );
    }

    #[test]
    fn read_document_rejects_config_hidden_and_non_text_paths() {
        let root = sample_project();
        std::fs::create_dir_all(root.join(".narra")).expect("narra");
        std::fs::write(root.join(".narra/instructions.md"), "secreto").expect("instructions");
        let mut executor = ToolExecutor::new(&root);
        let mut policy = GrantPolicy::new();
        for path in [
            "project.yaml",
            "AGENTS.md",
            ".narra/instructions.md",
            "../fuera.md",
        ] {
            let result = executor.execute(
                &call("c1", "read_document", json!({"path": path})),
                &mut policy,
                &origin(),
            );
            assert!(
                matches!(result.outcome, ToolOutcome::Failed { .. }),
                "{path} must be rejected: {:?}",
                result.outcome
            );
        }
    }

    #[test]
    fn load_skill_only_returns_trusted_catalogue_bodies() {
        let root = sample_project();
        let skill = Skill {
            id: "brief".to_string(),
            name: "Brief reviewer".to_string(),
            description: "Revisa briefs.".to_string(),
            body: "Revisa objetivo y audiencia.".to_string(),
            origin: crate::agent::SkillOrigin::User,
            hash: "hash".to_string(),
        };
        let mut executor = ToolExecutor::new(&root).with_skills(vec![skill]);
        let mut policy = GrantPolicy::new();
        let loaded = executor.execute(
            &call("c1", "load_skill", json!({"id": "brief"})),
            &mut policy,
            &origin(),
        );
        assert!(
            matches!(&loaded.outcome, ToolOutcome::Text(text) if text.contains("Revisa objetivo y audiencia.")),
            "{:?}",
            loaded.outcome
        );
        let missing = executor.execute(
            &call("c2", "load_skill", json!({"id": "other"})),
            &mut policy,
            &origin(),
        );
        assert!(
            matches!(missing.outcome, ToolOutcome::Failed { .. }),
            "{:?}",
            missing.outcome
        );
    }

    #[test]
    fn propose_write_is_ask_and_denied_without_grant() {
        let root = sample_project();
        let mut executor = ToolExecutor::new(&root);
        let mut policy = GrantPolicy::new();
        let result = executor.execute(
            &call(
                "c1",
                "propose_scene_edit",
                json!({
                    "scene": 1,
                    "instruction": "más tensión",
                    "rewritten_scene": "INT. TALLER - NOCHE\n\nELENA\nCerrando cuentas."
                }),
            ),
            &mut policy,
            &origin(),
        );
        assert!(matches!(result.decision, PermissionDecision::Ask(_)));
        assert!(
            matches!(result.outcome, ToolOutcome::Denied { .. }),
            "call must not execute: {:?}",
            result.outcome
        );
        // Nothing was written anywhere: no proposals, no canon changes.
        assert!(proposal::list(&root).expect("list").is_empty());
    }

    #[test]
    fn granted_propose_stages_a_proposal_without_touching_canon() {
        let root = sample_project();
        let canon_before =
            std::fs::read_to_string(root.join("screenplay/main.fountain")).expect("read");
        let mut executor = ToolExecutor::new(&root);
        let mut policy = GrantPolicy::new().allow("propose_scene_edit");
        let result = executor.execute(
            &call(
                "c1",
                "propose_scene_edit",
                json!({
                    "scene": 1,
                    "instruction": "más tensión",
                    "rewritten_scene": "INT. TALLER - NOCHE\n\nELENA\nCerrando cuentas."
                }),
            ),
            &mut policy,
            &origin(),
        );
        assert_eq!(result.decision, PermissionDecision::Allow);
        let ToolOutcome::ProposalCreated { id, target, hunks } = &result.outcome else {
            panic!("expected proposal, got {:?}", result.outcome);
        };
        assert_eq!(target, "screenplay/main.fountain");
        assert!(*hunks >= 1);
        assert!(
            !proposal::load(&root, id)
                .expect("persisted")
                .hunks
                .is_empty()
        );

        let canon_after =
            std::fs::read_to_string(root.join("screenplay/main.fountain")).expect("read");
        assert_eq!(canon_before, canon_after, "canon must be untouched");
    }

    #[test]
    fn write_grant_never_implies_destructive_apply() {
        let root = sample_project();
        // Even with the write tool granted, apply must be denied unless the
        // *destructive* grant names it.
        let mut policy = GrantPolicy::new()
            .allow("propose_scene_edit")
            .allow("apply_proposal"); // wrong bucket on purpose
        let mut executor = ToolExecutor::new(&root);
        let result = executor.execute(
            &call("c1", "apply_proposal", json!({"id": "whatever"})),
            &mut policy,
            &origin(),
        );
        assert!(
            matches!(result.decision, PermissionDecision::Deny(_)),
            "destructive needs allow_destructive: {:?}",
            result.decision
        );
        assert!(matches!(result.outcome, ToolOutcome::Denied { .. }));
    }

    #[test]
    fn destructive_grant_applies_only_reviewed_proposals() {
        let root = sample_project();
        let mut executor = ToolExecutor::new(&root);
        let mut policy = GrantPolicy::new()
            .allow("propose_scene_edit")
            .allow_destructive("apply_proposal");

        // Stage a proposal, then apply it with the explicit grant.
        let created = executor.execute(
            &call(
                "c1",
                "propose_scene_edit",
                json!({
                    "scene": 1,
                    "instruction": "más tensión",
                    "rewritten_scene": "INT. TALLER - NOCHE\n\nELENA\nCerrando cuentas."
                }),
            ),
            &mut policy,
            &origin(),
        );
        let ToolOutcome::ProposalCreated { id, .. } = created.outcome else {
            panic!("expected proposal: {:?}", created.outcome);
        };

        let applied = executor.execute(
            &call("c2", "apply_proposal", json!({"id": id})),
            &mut policy,
            &origin(),
        );
        assert_eq!(applied.decision, PermissionDecision::Allow);
        assert!(
            matches!(applied.outcome, ToolOutcome::ProposalApplied { .. }),
            "{:?}",
            applied.outcome
        );
        let canon = std::fs::read_to_string(root.join("screenplay/main.fountain")).expect("read");
        assert!(canon.contains("Cerrando cuentas."), "applied to canon");

        // Applying twice fails cleanly (state is now `applied`).
        let again = executor.execute(
            &call("c3", "apply_proposal", json!({"id": id})),
            &mut policy,
            &origin(),
        );
        assert!(
            matches!(again.outcome, ToolOutcome::Failed { .. }),
            "{:?}",
            again.outcome
        );
    }

    #[test]
    fn unknown_tool_is_refused_with_the_available_names() {
        let root = sample_project();
        let mut executor = ToolExecutor::new(&root);
        let mut policy = GrantPolicy::new().allow("rm_rf");
        let result = executor.execute(
            &call("c1", "rm_rf", json!({"path": "../../"})),
            &mut policy,
            &origin(),
        );
        assert!(matches!(result.outcome, ToolOutcome::Denied { .. }));
        let ToolOutcome::Denied { reason } = &result.outcome else {
            unreachable!()
        };
        assert!(reason.contains("desconocida"), "{reason}");
        assert!(reason.contains("read_scene"), "lists available: {reason}");
    }

    #[test]
    fn invalid_arguments_fail_without_side_effects() {
        let root = sample_project();
        let mut executor = ToolExecutor::new(&root);
        let mut policy = GrantPolicy::new().allow("propose_scene_edit");

        // Missing rewritten_scene.
        let missing = executor.execute(
            &call(
                "c1",
                "propose_scene_edit",
                json!({"scene": 1, "instruction": "x"}),
            ),
            &mut policy,
            &origin(),
        );
        assert!(matches!(missing.outcome, ToolOutcome::Failed { .. }));

        // Scene out of range.
        let out_of_range = executor.execute(
            &call(
                "c2",
                "propose_scene_edit",
                json!({"scene": 99, "instruction": "x", "rewritten_scene": "INT. A"}),
            ),
            &mut policy,
            &origin(),
        );
        assert!(matches!(out_of_range.outcome, ToolOutcome::Failed { .. }));

        assert!(proposal::list(&root).expect("list").is_empty());
    }

    #[test]
    fn every_call_is_auditable_even_when_denied() {
        let root = sample_project();
        let mut executor = ToolExecutor::new(&root);
        let mut policy = GrantPolicy::new();
        let denied = executor.execute(
            &call("c1", "apply_proposal", json!({"id": "x"})),
            &mut policy,
            &origin(),
        );
        assert_eq!(denied.call_id, "c1");
        assert_eq!(denied.tool, "apply_proposal");
        assert_eq!(denied.permission, ToolPermission::Destructive);
        assert!(matches!(denied.decision, PermissionDecision::Deny(_)));
        assert!(!denied.succeeded());
        // Serializable for the task output.
        let rendered = serde_json::to_value(&denied).expect("serde");
        assert_eq!(rendered["call_id"], "c1");
    }

    #[test]
    fn scene_region_excludes_the_separator_before_the_next_heading() {
        let base = "INT. A\n\nTEXTO\n\nINT. B\n\nOTRO\n";
        let region = scene_region(base, 1).expect("region");
        assert_eq!(region.start, 0);
        // Region ends at the last non-blank line of scene 1 (line 3), so the
        // blank separator before INT. B stays outside the replaced range.
        assert_eq!(region.end, 3, "{}", base.lines().count());
    }

    #[test]
    fn rewrite_keeps_the_separator_and_trailing_newline() {
        let base = "INT. A\n\nTEXTO\n\nINT. B\n\nOTRO\n";
        let region = scene_region(base, 1).expect("region");
        let rewritten = rewrite_region(base, &region, "  INT. A\n\nNUEVO  ");
        assert_eq!(rewritten, "INT. A\n\nNUEVO\n\nINT. B\n\nOTRO\n");
    }
}
