//! Configured agent roles (M6 slice 7): the MVP's initial coordinator,
//! writer, researcher and editor/reviewer configurations.
//!
//! `PRODUCT_SPEC.md` (MVP) requires an "agent workspace with an initial
//! coordinator, writer, researcher and editor/reviewer roles" plus "basic
//! skill/agent configuration from project or user-local files", and
//! `DATA_MODEL.md` defines an agent as "a configured role with
//! instructions, available tools, model/provider preferences and
//! permission profile".
//!
//! Design rules (`AGENTS.md`):
//! - built-in defaults ship with the core, so a project works with zero
//!   configuration;
//! - `<root>/agents.yaml` may override any built-in (or add a new role) and
//!   is read through the boundary-checked [`document::read_document`];
//! - a role's `tasks` list is its permission profile: a task refuses to run
//!   under a role that does not grant its kind ([`AgentError::RoleDenied`]);
//! - the hard guarantees live in the tasks, not in configuration: reviews
//!   stay read-only and edits stay proposals whatever a role file says;
//! - roles never carry credentials: `model` is a provider hint only.

use std::path::Path;

use saphyr::{LoadableYamlNode, Yaml};
use serde::Serialize;
use thiserror::Error;

use crate::document::{self, DocumentError};

use super::package::{self, AgentPackage, PackageError, PackageOrigin, PackageTrust};

/// The project-level role configuration file (relative to the root).
pub const CONFIG_PATH: &str = "agents.yaml";

/// Task kinds a permission profile may grant (`tasks` in the file).
pub const TASK_KINDS: [&str; 3] = ["project_brief", "scene_review", "scene_edit"];

/// Bounds: a role file is small, human-edited configuration.
const MAX_CONFIG_BYTES: usize = 64 * 1024;
const MAX_ROLES: usize = 16;
const MAX_ID_CHARS: usize = 32;
const MAX_NAME_CHARS: usize = 64;
const MAX_INSTRUCTION_CHARS: usize = 8_000;

/// Top-level and per-role keys accepted in `agents.yaml` (unknown keys are
/// explicit errors so typos do not silently change behavior).
const CONFIG_KEYS: [&str; 1] = ["roles"];
const ROLE_KEYS: [&str; 5] = ["id", "name", "instructions", "tasks", "model"];

/// Failures while resolving the effective role set. Messages name the file
/// and the field; they never echo whole file contents.
#[derive(Debug, Error)]
pub enum RolesError {
    #[error("agents.yaml: {0}")]
    Parse(String),
    #[error(transparent)]
    Document(#[from] DocumentError),
    #[error(transparent)]
    Package(#[from] PackageError),
}

/// Where the effective role came from (auditable in dumps and `roles`
/// output).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleOrigin {
    Builtin,
    /// Existing `agents.yaml` at the project root.
    Project,
    /// Instruction-only package under the user config directory.
    UserPackage,
    /// Instruction-only package under `<project>/.narra/agents`.
    ProjectPackage,
}

/// A configured role: persona + permission profile (+ optional model hint).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Role {
    /// Stable identifier, e.g. `"editor-reviewer"`.
    pub id: String,
    /// Display name (localizable by the UI; not used in prompts).
    pub name: String,
    /// Persona instructions appended to the task's fixed system prompt.
    pub instructions: String,
    /// Task kinds this role may run (the permission profile).
    pub tasks: Vec<String>,
    /// Optional provider/model hint — never a credential.
    pub model: Option<String>,
    /// Optional package description (project roles keep it absent).
    pub description: Option<String>,
    /// Package version, when the role came from a package.
    pub version: Option<String>,
    /// Trust level. Only `instructions` is executable today; packages that
    /// request tools are rejected during discovery.
    pub trust: PackageTrust,
    /// Whether this effective role is built in, comes from `agents.yaml` or
    /// from a project/user package.
    pub origin: RoleOrigin,
}

impl Role {
    /// Whether the role's profile grants `task_kind`.
    pub fn allows(&self, task_kind: &str) -> bool {
        self.tasks.iter().any(|kind| kind == task_kind)
    }

    /// `"builtin"` / `"project"` for dumps and provenance records.
    pub fn origin_key(&self) -> &'static str {
        match self.origin {
            RoleOrigin::Builtin => "builtin",
            RoleOrigin::Project => "project",
            RoleOrigin::UserPackage => "user_package",
            RoleOrigin::ProjectPackage => "project_package",
        }
    }

    /// The system prompt a task sends under this role: the task's fixed
    /// base text (the hard constraints) plus the role's persona block.
    pub fn system_with(&self, base: &str) -> String {
        format!(
            "{base}\n\nPERFIL (rol `{}`, {}):\n{}",
            self.id,
            self.origin_key(),
            self.instructions
        )
    }
}

/// The four initial roles (`PRODUCT_SPEC.md` MVP), in spec order.
pub fn builtin_roles() -> Vec<Role> {
    [
        (
            "coordinator",
            "Coordinador",
            "Coordinas el trabajo del proyecto: ordenas las prioridades, pides solo el \
             contexto necesario y devuelves orientación breve, concreta y accionable. No \
             reescribes archivos sin una instrucción explícita del usuario.",
            &["project_brief"][..],
        ),
        (
            "writer",
            "Guionista",
            "Escribes y reescribes escenas con oficio: respetas el tono del proyecto, el \
             formato Fountain y las decisiones aprobadas de la biblia. Entregas solo el \
             texto solicitado.",
            &["scene_edit"][..],
        ),
        (
            "researcher",
            "Investigador",
            "Investigas y contrastas el material del proyecto (biblia, estructura, \
             investigación y notas) en modo lectura: citas la fuente concreta que sostiene \
             cada afirmación y no propones reescrituras.",
            &["scene_review"][..],
        ),
        (
            "editor-reviewer",
            "Editor/revisor",
            "Revisas escenas con criterio de editor profesional — continuidad, estructura, \
             diálogo y formato — y fundamentas cada observación en el material entregado.",
            &["scene_review"][..],
        ),
    ]
    .into_iter()
    .map(|(id, name, instructions, tasks)| Role {
        id: id.to_string(),
        name: name.to_string(),
        instructions: instructions.to_string(),
        tasks: tasks.iter().map(|task| task.to_string()).collect(),
        model: None,
        description: None,
        version: None,
        trust: PackageTrust::Instructions,
        origin: RoleOrigin::Builtin,
    })
    .collect()
}

/// The built-in role that runs `task_kind` when the caller does not pick
/// one: coordinator → brief, writer → edit, editor-reviewer → review.
pub fn default_role_for(task_kind: &str) -> Option<&'static str> {
    match task_kind {
        "project_brief" => Some("coordinator"),
        "scene_edit" => Some("writer"),
        "scene_review" => Some("editor-reviewer"),
        _ => None,
    }
}

/// The effective role with `id`, if present.
pub fn role_by_id<'a>(roles: &'a [Role], id: &str) -> Option<&'a Role> {
    roles.iter().find(|role| role.id == id)
}

/// Effective roles for a project: the built-ins, overridden and extended
/// by `<root>/agents.yaml` when it exists. A missing file is not an error.
/// Package discovery is opt-in through [`load_roles_with_packages`].
pub fn load_roles(root: &Path) -> Result<Vec<Role>, RolesError> {
    load_roles_with_packages(root, None)
}

/// Effective roles including instruction-only packages.
///
/// Precedence (later wins): built-in < user package < project package <
/// `<root>/agents.yaml`. Package problems are explicit errors; a missing
/// package directory is simply empty.
pub fn load_roles_with_packages(
    root: &Path,
    user_config_dir: Option<&Path>,
) -> Result<Vec<Role>, RolesError> {
    let mut roles = builtin_roles();
    if let Some(config_dir) = user_config_dir {
        let user_dir = package::user_packages_dir(config_dir);
        let packages = package::discover(&user_dir, "", PackageOrigin::User)?;
        merge_packages(&mut roles, packages);
    }
    let packages = package::discover(root, package::PROJECT_PACKAGES_DIR, PackageOrigin::Project)?;
    merge_packages(&mut roles, packages);

    let text = match document::read_document(root, CONFIG_PATH) {
        Ok(text) => text,
        Err(DocumentError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(roles);
        }
        Err(error) => return Err(RolesError::Document(error)),
    };
    if text.len() > MAX_CONFIG_BYTES {
        return Err(RolesError::Parse(format!(
            "{CONFIG_PATH} is larger than {MAX_CONFIG_BYTES} bytes"
        )));
    }
    merge_project_roles(&mut roles, &text)?;
    Ok(roles)
}

/// The role with `id` as configured in `root` (built-in, project config or
/// project package).
pub fn resolve_role(root: &Path, id: &str) -> Result<Role, RolesError> {
    resolve_role_with_packages(root, None, id)
}

/// The role with `id`, including user packages when `user_agents_dir` is set.
pub fn resolve_role_with_packages(
    root: &Path,
    user_config_dir: Option<&Path>,
    id: &str,
) -> Result<Role, RolesError> {
    let roles = load_roles_with_packages(root, user_config_dir)?;
    role_by_id(&roles, id).cloned().ok_or_else(|| {
        RolesError::Parse(format!(
            "unknown role `{id}` (known: {})",
            roles
                .iter()
                .map(|role| role.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    })
}

/// The default role for `task_kind` as configured in `root`. Explicit
/// errors for a broken config or a missing role — never a silent fallback.
pub fn resolve_default(root: &Path, task_kind: &str) -> Result<Role, RolesError> {
    resolve_default_with_packages(root, None, task_kind)
}

/// The default role for `task_kind`, including user packages when set.
pub fn resolve_default_with_packages(
    root: &Path,
    user_config_dir: Option<&Path>,
    task_kind: &str,
) -> Result<Role, RolesError> {
    let id = default_role_for(task_kind)
        .ok_or_else(|| RolesError::Parse(format!("no default role for task kind `{task_kind}`")))?;
    resolve_role_with_packages(root, user_config_dir, id)
}

/// Convert discovered packages into roles and apply precedence.
fn merge_packages(roles: &mut Vec<Role>, packages: Vec<AgentPackage>) {
    for package in packages {
        let role = Role {
            id: package.id,
            name: package.name,
            instructions: package.instructions,
            tasks: package.tasks,
            model: package.model,
            description: package.description,
            version: Some(package.version),
            trust: package.trust,
            origin: match package.origin {
                PackageOrigin::User => RoleOrigin::UserPackage,
                PackageOrigin::Project => RoleOrigin::ProjectPackage,
            },
        };
        match roles.iter_mut().find(|existing| existing.id == role.id) {
            Some(existing) => *existing = role,
            None => roles.push(role),
        }
    }
}

/// Apply `<root>/agents.yaml` contents on top of `roles`: an entry whose
/// `id` matches replaces that role (explicit config wins); any other `id`
/// adds a new role.
fn merge_project_roles(roles: &mut Vec<Role>, text: &str) -> Result<(), RolesError> {
    let documents =
        Yaml::load_from_str(text).map_err(|error| RolesError::Parse(error.to_string()))?;
    if documents.len() != 1 {
        return Err(RolesError::Parse(format!(
            "expected exactly one YAML document, found {}",
            documents.len()
        )));
    }
    let root = documents.into_iter().next().expect("length checked above");
    let mapping = root
        .as_mapping()
        .ok_or_else(|| RolesError::Parse("top level must be a mapping".to_string()))?;
    for key in mapping.keys() {
        let key = key
            .as_str()
            .ok_or_else(|| RolesError::Parse("mapping keys must be strings".to_string()))?;
        if !CONFIG_KEYS.contains(&key) {
            return Err(RolesError::Parse(format!("unknown key `{key}`")));
        }
    }
    let Some(list) = root.as_mapping_get("roles").and_then(Yaml::as_sequence) else {
        return Err(RolesError::Parse("missing `roles` sequence".to_string()));
    };
    if list.len() > MAX_ROLES {
        return Err(RolesError::Parse(format!(
            "too many roles: {} (max {MAX_ROLES})",
            list.len()
        )));
    }

    let mut seen: Vec<String> = Vec::new();
    for entry in list {
        let role_map = entry
            .as_mapping()
            .ok_or_else(|| RolesError::Parse("each entry of `roles` must be a mapping".into()))?;
        for key in role_map.keys() {
            let key = key
                .as_str()
                .ok_or_else(|| RolesError::Parse("mapping keys must be strings".to_string()))?;
            if !ROLE_KEYS.contains(&key) {
                return Err(RolesError::Parse(format!("unknown key `{key}` in role")));
            }
        }

        let id = required_string(entry, "id", "role id")?;
        validate_id(id)?;
        if seen.iter().any(|seen| seen == id) {
            return Err(RolesError::Parse(format!("duplicate role `{id}`")));
        }
        seen.push(id.to_string());

        let builtin_ix = roles.iter().position(|role| role.id == id);

        // Persona: required for a new role; an override that omits it
        // keeps the built-in persona (the file changes what it names).
        let instructions = match entry.as_mapping_get("instructions") {
            None => match builtin_ix {
                Some(ix) => roles[ix].instructions.clone(),
                None => {
                    return Err(RolesError::Parse(format!(
                        "role `{id}`: `instructions` is required for a new role"
                    )));
                }
            },
            Some(value) => {
                let instructions = value.as_str().ok_or_else(|| {
                    RolesError::Parse(format!("role `{id}`: `instructions` must be a string"))
                })?;
                if instructions.trim().is_empty() {
                    return Err(RolesError::Parse(format!(
                        "role `{id}`: instructions must not be empty"
                    )));
                }
                if instructions.chars().count() > MAX_INSTRUCTION_CHARS {
                    return Err(RolesError::Parse(format!(
                        "role `{id}`: instructions exceed {MAX_INSTRUCTION_CHARS} characters"
                    )));
                }
                instructions.to_string()
            }
        };

        // Display name: falls back to the built-in name, or to the id for
        // a new role.
        let name = match entry.as_mapping_get("name") {
            None => match builtin_ix {
                Some(ix) => roles[ix].name.clone(),
                None => id.to_string(),
            },
            Some(value) => {
                let name = value.as_str().ok_or_else(|| {
                    RolesError::Parse(format!("role `{id}`: `name` must be a string"))
                })?;
                if name.trim().is_empty() || name.chars().count() > MAX_NAME_CHARS {
                    return Err(RolesError::Parse(format!(
                        "role `{id}`: `name` must be 1..={MAX_NAME_CHARS} characters"
                    )));
                }
                name.to_string()
            }
        };

        let model = match entry.as_mapping_get("model") {
            None => None,
            Some(value) => {
                let model = value.as_str().ok_or_else(|| {
                    RolesError::Parse(format!("role `{id}`: `model` must be a string"))
                })?;
                if model.trim().is_empty() {
                    return Err(RolesError::Parse(format!(
                        "role `{id}`: `model` must not be empty"
                    )));
                }
                Some(model.to_string())
            }
        };

        // `tasks` is the permission profile: required for a new role; an
        // override that omits it keeps the built-in profile (the file only
        // changes the persona).
        let tasks = match entry.as_mapping_get("tasks") {
            None => match builtin_ix {
                Some(ix) => roles[ix].tasks.clone(),
                None => {
                    return Err(RolesError::Parse(format!(
                        "role `{id}`: `tasks` is required for a new role"
                    )));
                }
            },
            Some(value) => parse_tasks(value, id)?,
        };

        let resolved = Role {
            id: id.to_string(),
            name,
            instructions: instructions.to_string(),
            tasks,
            model,
            description: None,
            version: None,
            trust: PackageTrust::Instructions,
            origin: RoleOrigin::Project,
        };
        match builtin_ix {
            Some(ix) => roles[ix] = resolved,
            None => roles.push(resolved),
        }
    }
    Ok(())
}

/// A required string field on a role entry.
fn required_string<'a>(yaml: &'a Yaml, key: &str, what: &str) -> Result<&'a str, RolesError> {
    yaml.as_mapping_get(key)
        .and_then(Yaml::as_str)
        .ok_or_else(|| RolesError::Parse(format!("missing or non-string `{key}` ({what})")))
}

/// Parse and validate a role's `tasks` list against [`TASK_KINDS`].
fn parse_tasks(value: &Yaml, id: &str) -> Result<Vec<String>, RolesError> {
    let sequence = value
        .as_sequence()
        .ok_or_else(|| RolesError::Parse(format!("role `{id}`: `tasks` must be a list")))?;
    if sequence.is_empty() {
        return Err(RolesError::Parse(format!(
            "role `{id}`: `tasks` must not be empty"
        )));
    }
    let mut tasks: Vec<String> = Vec::new();
    for item in sequence {
        let task = item.as_str().ok_or_else(|| {
            RolesError::Parse(format!("role `{id}`: every task must be a string"))
        })?;
        if !TASK_KINDS.contains(&task) {
            return Err(RolesError::Parse(format!(
                "role `{id}`: unknown task `{task}` (expected one of: {})",
                TASK_KINDS.join(", ")
            )));
        }
        if tasks.iter().any(|seen| seen == task) {
            return Err(RolesError::Parse(format!(
                "role `{id}`: duplicate task `{task}`"
            )));
        }
        tasks.push(task.to_string());
    }
    Ok(tasks)
}

/// Role ids are lowercase slugs so they survive keys, paths and prompts.
fn validate_id(id: &str) -> Result<(), RolesError> {
    let valid = !id.is_empty()
        && id.len() <= MAX_ID_CHARS
        && id.starts_with(|c: char| c.is_ascii_lowercase())
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !valid {
        return Err(RolesError::Parse(format!(
            "invalid role id `{id}` (1..={MAX_ID_CHARS} chars: a-z, 0-9 and `-`, starting with a letter)"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// An empty project directory (no `agents.yaml`).
    fn project_empty() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    /// A project directory containing `agents.yaml` with `text`.
    fn project_with(text: &str) -> tempfile::TempDir {
        let dir = project_empty();
        let mut file = std::fs::File::create(dir.path().join(CONFIG_PATH)).expect("create");
        file.write_all(text.as_bytes()).expect("write");
        dir
    }

    #[test]
    fn builtins_are_the_four_initial_roles_in_spec_order() {
        let roles = builtin_roles();
        let ids: Vec<&str> = roles.iter().map(|role| role.id.as_str()).collect();
        assert_eq!(
            ids,
            ["coordinator", "writer", "researcher", "editor-reviewer"]
        );
        assert!(roles.iter().all(|role| role.origin == RoleOrigin::Builtin));
        assert!(roles.iter().all(|role| !role.instructions.is_empty()));

        // Permission profiles: one task kind each, matching the mapping.
        assert_eq!(
            role_by_id(&roles, "coordinator").unwrap().tasks,
            ["project_brief"]
        );
        assert_eq!(role_by_id(&roles, "writer").unwrap().tasks, ["scene_edit"]);
        assert_eq!(
            role_by_id(&roles, "researcher").unwrap().tasks,
            ["scene_review"]
        );
        assert_eq!(
            role_by_id(&roles, "editor-reviewer").unwrap().tasks,
            ["scene_review"]
        );

        // Default role per task kind (what the workspace resolves).
        assert_eq!(default_role_for("project_brief"), Some("coordinator"));
        assert_eq!(default_role_for("scene_edit"), Some("writer"));
        assert_eq!(default_role_for("scene_review"), Some("editor-reviewer"));
        assert_eq!(default_role_for("nope"), None);
    }

    #[test]
    fn allows_checks_the_permission_profile() {
        let roles = builtin_roles();
        let writer = role_by_id(&roles, "writer").expect("writer");
        assert!(writer.allows("scene_edit"));
        assert!(!writer.allows("scene_review"));
    }

    #[test]
    fn missing_config_keeps_the_builtins() {
        let dir = project_empty();
        let roles = load_roles(dir.path()).expect("builtins");
        assert_eq!(roles, builtin_roles());
    }

    #[test]
    fn override_replaces_the_persona_and_keeps_the_profile() {
        let dir = project_with(
            "roles:\n\
             \x20 - id: editor-reviewer\n\
             \x20   name: Revisor del proyecto\n\
             \x20   instructions: |\n\
             \x20     Revisa citando la biblia.\n",
        );
        let roles = load_roles(dir.path()).expect("roles");
        let role = role_by_id(&roles, "editor-reviewer").expect("overridden");
        assert_eq!(role.origin, RoleOrigin::Project);
        assert_eq!(role.name, "Revisor del proyecto");
        assert!(role.instructions.contains("Revisa citando la biblia"));
        // Omitted `tasks`: the built-in profile stands.
        assert_eq!(role.tasks, ["scene_review"]);
        // The other built-ins are untouched.
        assert_eq!(
            role_by_id(&roles, "writer").expect("writer").origin,
            RoleOrigin::Builtin
        );
    }

    #[test]
    fn override_can_narrow_or_widen_the_profile() {
        let dir = project_with(
            "roles:\n\
             \x20 - id: editor-reviewer\n\
             \x20   instructions: Ok.\n\
             \x20   tasks: [scene_review, project_brief]\n",
        );
        let roles = load_roles(dir.path()).expect("roles");
        let role = role_by_id(&roles, "editor-reviewer").expect("overridden");
        assert!(role.allows("scene_review"));
        assert!(role.allows("project_brief"));
        assert!(!role.allows("scene_edit"));
    }

    #[test]
    fn tasks_only_override_inherits_the_persona_and_name() {
        let dir = project_with("roles:\n  - id: writer\n    tasks: [scene_edit, scene_review]\n");
        let roles = load_roles(dir.path()).expect("roles");
        let builtins = builtin_roles();
        let builtin = role_by_id(&builtins, "writer").expect("builtin writer");
        let role = role_by_id(&roles, "writer").expect("writer");
        assert_eq!(role.origin, RoleOrigin::Project);
        assert_eq!(role.instructions, builtin.instructions);
        assert_eq!(role.name, builtin.name);
        assert!(role.allows("scene_edit"));
        assert!(role.allows("scene_review"));
    }

    #[test]
    fn new_role_is_added_and_requires_tasks() {
        let added = project_with(
            "roles:\n\
             \x20 - id: peer\n\
             \x20   name: Par\n\
             \x20   instructions: Ayudas a un par.\n\
             \x20   tasks: [scene_review]\n",
        );
        let roles = load_roles(added.path()).expect("roles");
        let peer = role_by_id(&roles, "peer").expect("peer");
        assert_eq!(peer.origin, RoleOrigin::Project);
        assert!(peer.allows("scene_review"));

        let missing = project_with(
            "roles:\n\
             \x20 - id: peer\n\
             \x20   instructions: Ayudas a un par.\n",
        );
        let error = load_roles(missing.path()).expect_err("tasks required");
        assert!(error.to_string().contains("`tasks` is required"), "{error}");
    }

    #[test]
    fn unknown_keys_and_duplicates_are_explicit_errors() {
        let unknown_top = project_with("roles: []\nextra: 1\n");
        let error = load_roles(unknown_top.path()).expect_err("unknown key");
        assert!(error.to_string().contains("unknown key `extra`"), "{error}");

        let unknown_field = project_with("roles:\n  - id: writer\n    persona: x\n");
        let error = load_roles(unknown_field.path()).expect_err("unknown field");
        assert!(
            error.to_string().contains("unknown key `persona`"),
            "{error}"
        );

        let duplicate = project_with(
            "roles:\n  - id: writer\n    instructions: a\n  - id: writer\n    instructions: b\n",
        );
        let error = load_roles(duplicate.path()).expect_err("duplicate");
        assert!(
            error.to_string().contains("duplicate role `writer`"),
            "{error}"
        );
    }

    #[test]
    fn invalid_tasks_ids_and_instructions_are_rejected() {
        // An override may widen a profile as long as the kinds are known.
        let widened = project_with(
            "roles:\n  - id: writer\n    instructions: Escribe con tono.\n    tasks: [scene_review]\n",
        );
        let roles = load_roles(widened.path()).expect("known task");
        assert!(role_by_id(&roles, "writer").unwrap().allows("scene_review"));

        let unknown_task = project_with("roles:\n  - id: writer\n    tasks: [launch_missiles]\n");
        let error = load_roles(unknown_task.path()).expect_err("unknown task");
        assert!(
            error.to_string().contains("unknown task `launch_missiles`"),
            "{error}"
        );

        let empty = project_with("roles:\n  - id: writer\n    instructions: \"  \"\n");
        let error = load_roles(empty.path()).expect_err("empty instructions");
        assert!(error.to_string().contains("must not be empty"), "{error}");

        let bad_id = project_with("roles:\n  - id: Writer\n    tasks: [scene_edit]\n");
        let error = load_roles(bad_id.path()).expect_err("bad id");
        assert!(
            error.to_string().contains("invalid role id `Writer`"),
            "{error}"
        );
    }

    #[test]
    fn oversized_config_is_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let text = format!("roles: []\n# {}\n", "x".repeat(MAX_CONFIG_BYTES));
        std::fs::write(dir.path().join(CONFIG_PATH), text).expect("write");
        let error = load_roles(dir.path()).expect_err("oversized");
        assert!(error.to_string().contains("larger than"), "{error}");
    }

    #[test]
    fn resolve_helpers_name_the_broken_config_or_unknown_id() {
        let dir = project_with("roles: {}\n");
        let error = resolve_default(dir.path(), "scene_review").expect_err("broken config");
        assert!(error.to_string().contains("agents.yaml"), "{error}");

        let empty = project_empty();
        let role = resolve_default(empty.path(), "scene_review").expect("default");
        assert_eq!(role.id, "editor-reviewer");

        let error = resolve_role(empty.path(), "nobody").expect_err("unknown");
        let message = error.to_string();
        assert!(message.contains("unknown role `nobody`"), "{message}");
        assert!(
            message.contains("editor-reviewer"),
            "lists known roles: {message}"
        );

        let error = resolve_default(empty.path(), "not-a-task").expect_err("no default");
        assert!(error.to_string().contains("no default role"), "{error}");
    }

    fn package_text(id: &str, name: &str) -> String {
        format!(
            "schema_version: 1\napi_version: 1\nid: {id}\nname: {name}\nversion: 2.0.0\ndescription: Paquete de prueba.\ninstructions: Revisa con criterio.\ntasks:\n  - scene_review\ntrust: instructions\n"
        )
    }

    #[test]
    fn project_and_user_packages_merge_with_explicit_precedence() {
        let project = project_empty();
        let user = project_empty();
        let user_dir = user.path().join("agents/documentalista");
        std::fs::create_dir_all(&user_dir).expect("user dir");
        std::fs::write(
            user_dir.join("agent.yaml"),
            package_text("documentalista", "Usuario"),
        )
        .expect("user package");
        let project_dir = project.path().join(".narra/agents/documentalista");
        std::fs::create_dir_all(&project_dir).expect("project dir");
        std::fs::write(
            project_dir.join("agent.yaml"),
            package_text("documentalista", "Proyecto"),
        )
        .expect("project package");

        let roles = load_roles_with_packages(project.path(), Some(user.path())).expect("packages");
        let role = role_by_id(&roles, "documentalista").expect("package role");
        assert_eq!(role.name, "Proyecto");
        assert_eq!(role.origin, RoleOrigin::ProjectPackage);
        assert_eq!(role.version.as_deref(), Some("2.0.0"));
        assert_eq!(role.trust, PackageTrust::Instructions);
        assert_eq!(role.description.as_deref(), Some("Paquete de prueba."));

        std::fs::write(
            project.path().join(CONFIG_PATH),
            "roles:\n  - id: documentalista\n    name: Config\n    instructions: Manda el archivo.\n    tasks: [scene_review]\n",
        )
        .expect("agents.yaml");
        let roles = load_roles_with_packages(project.path(), Some(user.path())).expect("config");
        let role = role_by_id(&roles, "documentalista").expect("config role");
        assert_eq!(role.name, "Config");
        assert_eq!(role.origin, RoleOrigin::Project);
        assert_eq!(role.version, None);
    }

    #[test]
    fn a_user_package_can_add_a_role_without_project_files() {
        let project = project_empty();
        let user = project_empty();
        let dir = user.path().join("agents/pep");
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join("agent.yaml"), package_text("pep", "Pep")).expect("package");
        let roles = load_roles_with_packages(project.path(), Some(user.path())).expect("user pkg");
        let role = role_by_id(&roles, "pep").expect("pep");
        assert_eq!(role.origin, RoleOrigin::UserPackage);
        assert!(role.allows("scene_review"));
    }

    #[test]
    fn packages_requesting_tools_are_rejected() {
        let project = project_empty();
        let user = project_empty();
        let dir = user.path().join("agents/runner");
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(
            dir.join("agent.yaml"),
            package_text("runner", "Runner").replace("trust: instructions", "trust: tools"),
        )
        .expect("package");
        let error = load_roles_with_packages(project.path(), Some(user.path()))
            .expect_err("tools rejected");
        assert!(error.to_string().contains("executable tools"), "{error}");
    }
    #[test]
    fn system_with_appends_the_persona_to_the_fixed_base() {
        let roles = builtin_roles();
        let role = role_by_id(&roles, "editor-reviewer").expect("role");
        let system = role.system_with("BASE FIJO");
        assert!(system.starts_with("BASE FIJO"));
        assert!(system.contains("PERFIL (rol `editor-reviewer`, builtin)"));
        assert!(system.contains(&role.instructions));
    }
}
