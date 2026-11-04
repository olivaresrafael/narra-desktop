//! Portable `AGENT.md` agents and slash commands (D-062, Spike I).
//!
//! Creative capability ships as data, never as Rust:
//! - `.narra/agents/<id>/AGENT.md` describes an agent — YAML frontmatter
//!   (`name`, `description`, `mode`, `write_roots`, `skills`) plus a
//!   Markdown instruction body;
//! - `.narra/commands/<id>.md` describes a slash command — frontmatter
//!   (`name`, `description`, optional `agent`) plus the prompt body.
//!
//! Both formats are instruction-only (D-062): they describe behavior and
//! permissions but can never execute code or grant tools outside the core
//! catalog. `agent.yaml` packages keep loading from the same directory; when
//! a directory holds both files, `agent.yaml` wins (the structured, versioned
//! contract) and the portable loader skips it.
//!
//! Files are read through the boundary-checked [`document::read_document`],
//! so a package can never point a loader outside its project.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path};

use saphyr::{LoadableYamlNode, Yaml};
use serde::Serialize;

use crate::document;

use super::frontmatter;
use super::package::{self, PackageError, PackageOrigin};
use super::proposal::sha256_hex;

/// Agent file inside each `.narra/agents/<id>/` directory.
pub const AGENT_FILE: &str = "AGENT.md";
/// Project-local slash commands, relative to the project root.
pub const PROJECT_COMMANDS_DIR: &str = ".narra/commands";
/// User-scope slash commands, relative to the user config directory.
pub const USER_COMMANDS_DIR: &str = "commands";

const MAX_FILE_BYTES: usize = 64 * 1024;
const MAX_INSTRUCTION_CHARS: usize = 16_000;
const MAX_AGENTS: usize = 32;
const MAX_COMMANDS: usize = 64;
const MAX_NAME_CHARS: usize = 64;
const MAX_DESCRIPTION_CHARS: usize = 500;
const MAX_LIST: usize = 32;
const MAX_PATH_CHARS: usize = 160;

/// Frontmatter keys an `AGENT.md` may declare.
const AGENT_KEYS: [&str; 5] = ["name", "description", "mode", "write_roots", "skills"];
/// Frontmatter keys a command `.md` may declare.
const COMMAND_KEYS: [&str; 3] = ["name", "description", "agent"];

/// Interaction mode a portable agent declares. This is behavior policy, not
/// creative content: `plan` and `review` are read-only, while `build` may
/// propose edits inside the declared write roots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentMode {
    Plan,
    Build,
    Review,
}

impl AgentMode {
    /// Stable key for dumps and provenance records.
    pub fn key(self) -> &'static str {
        match self {
            AgentMode::Plan => "plan",
            AgentMode::Build => "build",
            AgentMode::Review => "review",
        }
    }
}

/// One portable agent loaded from `.narra/agents/<id>/AGENT.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortableAgent {
    /// Directory name; must match the agent id rules of `agent.yaml`.
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub mode: AgentMode,
    /// Folders this agent may propose writes into. The type package
    /// cross-checks them against its own `write_roots`; the kernel still
    /// sandboxes every path, so this list can only narrow.
    pub write_roots: Vec<String>,
    /// Skill ids the agent asks for (installed in the same scope).
    pub skills: Vec<String>,
    /// Markdown instruction body; instructions only, never executable.
    pub instructions: String,
    pub origin: PackageOrigin,
    /// SHA-256 of the file: the trust anchor for confirmation flows.
    pub hash: String,
}

/// One slash command loaded from `.narra/commands/<id>.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlashCommand {
    /// File stem; must follow the agent/command id rules.
    pub id: String,
    pub name: String,
    pub description: String,
    /// Agent id that runs the prompt, when the command names one.
    pub agent: Option<String>,
    /// Prompt body inserted when the user invokes `/id`.
    pub prompt: String,
    pub origin: PackageOrigin,
    /// SHA-256 of the file: the trust anchor for confirmation flows.
    pub hash: String,
}

/// Parse and validate one `AGENT.md` without touching the filesystem.
pub fn parse_agent(
    text: &str,
    id: &str,
    origin: PackageOrigin,
) -> Result<PortableAgent, PackageError> {
    package::validate_id(id)?;
    let file = format!("{id}/{AGENT_FILE}");
    check_size(&file, text)?;
    let (frontmatter, body) = split(text, &file)?;
    let documents = Yaml::load_from_str(&frontmatter)
        .map_err(|error| PackageError::Parse(format!("{file}: {error}")))?;
    if documents.len() != 1 {
        return Err(PackageError::Parse(format!(
            "{file}: frontmatter must be exactly one YAML document"
        )));
    }
    let root = documents.first().expect("length checked above");
    reject_unknown_keys(root, &AGENT_KEYS, &file)?;

    let name = required_string(root, "name", &file)?;
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(PackageError::Parse(format!(
            "{file}: `name` must be at most {MAX_NAME_CHARS} characters"
        )));
    }
    let description = optional_string(root, "description", &file)?;
    let mode_raw = required_string(root, "mode", &file)?;
    let mode = match mode_raw {
        "plan" => AgentMode::Plan,
        "build" => AgentMode::Build,
        "review" => AgentMode::Review,
        other => {
            return Err(PackageError::Parse(format!(
                "{file}: unknown mode `{other}` (expected plan, build or review)"
            )));
        }
    };
    let write_roots = parse_root_list(root, "write_roots", &file)?;
    let skills = parse_id_list(root, "skills", &file)?;

    let instructions = instruction_body(body, &file)?;
    Ok(PortableAgent {
        id: id.to_string(),
        name: name.to_string(),
        description,
        mode,
        write_roots,
        skills,
        instructions,
        origin,
        hash: sha256_hex(text),
    })
}

/// Parse and validate one command `.md` without touching the filesystem.
pub fn parse_command(
    text: &str,
    id: &str,
    origin: PackageOrigin,
) -> Result<SlashCommand, PackageError> {
    package::validate_id(id)?;
    let file = format!("{id}.md");
    check_size(&file, text)?;
    let (frontmatter, body) = split(text, &file)?;
    let documents = Yaml::load_from_str(&frontmatter)
        .map_err(|error| PackageError::Parse(format!("{file}: {error}")))?;
    if documents.len() != 1 {
        return Err(PackageError::Parse(format!(
            "{file}: frontmatter must be exactly one YAML document"
        )));
    }
    let root = documents.first().expect("length checked above");
    reject_unknown_keys(root, &COMMAND_KEYS, &file)?;

    let name = required_string(root, "name", &file)?;
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(PackageError::Parse(format!(
            "{file}: `name` must be at most {MAX_NAME_CHARS} characters"
        )));
    }
    let description = required_string(root, "description", &file)?;
    if description.chars().count() > MAX_DESCRIPTION_CHARS {
        return Err(PackageError::Parse(format!(
            "{file}: `description` must be at most {MAX_DESCRIPTION_CHARS} characters"
        )));
    }
    let agent = match root.as_mapping_get("agent") {
        None => None,
        Some(value) => {
            let agent = value
                .as_str()
                .ok_or_else(|| PackageError::Parse(format!("{file}: `agent` must be a string")))?;
            package::validate_id(agent)?;
            Some(agent.to_string())
        }
    };
    let prompt = instruction_body(body, &file)?;
    Ok(SlashCommand {
        id: id.to_string(),
        name: name.to_string(),
        description: description.to_string(),
        agent,
        prompt,
        origin,
        hash: sha256_hex(text),
    })
}

/// Portable agents under `root/relative_dir` (empty means `root` itself).
/// A missing directory is empty, never an error. Directories holding an
/// `agent.yaml` are skipped — the structured contract wins when both files
/// exist — and directories with neither file are the `agent.yaml` loader's
/// explicit error, not this one's.
pub fn discover_agents(
    root: &Path,
    relative_dir: &str,
    origin: PackageOrigin,
) -> Result<Vec<PortableAgent>, PackageError> {
    let dir = scoped_dir(root, relative_dir);
    let names = directory_names(&dir, MAX_AGENTS)?;
    let mut agents = Vec::new();
    for name in names {
        if dir.join(&name).join(package::PACKAGE_FILE).is_file() {
            continue;
        }
        let relative = scoped_relative(relative_dir, &format!("{name}/{AGENT_FILE}"));
        if !root.join(&relative).is_file() {
            continue;
        }
        let text = document::read_document(root, &relative)?;
        agents.push(parse_agent(&text, &name, origin)?);
    }
    Ok(agents)
}

/// Slash commands under `root/relative_dir` (empty means `root` itself).
/// A missing directory is empty, never an error. Every `.md` file in the
/// directory is a command: a malformed one is an explicit error so typos
/// never silently drop a command.
pub fn discover_commands(
    root: &Path,
    relative_dir: &str,
    origin: PackageOrigin,
) -> Result<Vec<SlashCommand>, PackageError> {
    let dir = scoped_dir(root, relative_dir);
    let names = command_names(&dir)?;
    let mut commands = Vec::with_capacity(names.len());
    for name in names {
        let relative = scoped_relative(relative_dir, &format!("{name}.md"));
        let text = document::read_document(root, &relative)?;
        commands.push(parse_command(&text, &name, origin)?);
    }
    Ok(commands)
}

// --- parsing helpers -------------------------------------------------------

fn scoped_dir(root: &Path, relative_dir: &str) -> std::path::PathBuf {
    if relative_dir.is_empty() {
        root.to_path_buf()
    } else {
        root.join(relative_dir)
    }
}

fn scoped_relative(relative_dir: &str, file: &str) -> String {
    if relative_dir.is_empty() {
        file.to_string()
    } else {
        format!("{relative_dir}/{file}")
    }
}

fn check_size(file: &str, text: &str) -> Result<(), PackageError> {
    if text.len() > MAX_FILE_BYTES {
        return Err(PackageError::Parse(format!(
            "{file} is larger than {MAX_FILE_BYTES} bytes"
        )));
    }
    Ok(())
}

fn split(text: &str, file: &str) -> Result<(String, String), PackageError> {
    frontmatter::split_frontmatter(text)
        .map_err(|error| PackageError::Parse(format!("{file}: {error}")))
}

fn reject_unknown_keys(mapping: &Yaml, allowed: &[&str], file: &str) -> Result<(), PackageError> {
    let mapping = mapping
        .as_mapping()
        .ok_or_else(|| PackageError::Parse(format!("{file}: frontmatter must be a mapping")))?;
    for key in mapping.keys() {
        let key = key
            .as_str()
            .ok_or_else(|| PackageError::Parse(format!("{file}: keys must be strings")))?;
        if !allowed.contains(&key) {
            return Err(PackageError::Parse(format!(
                "{file}: unknown frontmatter key `{key}`"
            )));
        }
    }
    Ok(())
}

fn required_string<'a>(root: &'a Yaml, key: &str, file: &str) -> Result<&'a str, PackageError> {
    let value = root
        .as_mapping_get(key)
        .and_then(Yaml::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            PackageError::Parse(format!(
                "{file}: frontmatter needs a non-empty `{key}` string"
            ))
        })?;
    Ok(value)
}

fn optional_string(root: &Yaml, key: &str, file: &str) -> Result<Option<String>, PackageError> {
    match root.as_mapping_get(key) {
        None => Ok(None),
        Some(value) => {
            let text = value
                .as_str()
                .ok_or_else(|| PackageError::Parse(format!("{file}: `{key}` must be a string")))?;
            if text.trim().is_empty() {
                return Err(PackageError::Parse(format!(
                    "{file}: `{key}` must not be empty"
                )));
            }
            if text.chars().count() > MAX_DESCRIPTION_CHARS {
                return Err(PackageError::Parse(format!(
                    "{file}: `{key}` must be at most {MAX_DESCRIPTION_CHARS} characters"
                )));
            }
            Ok(Some(text.to_string()))
        }
    }
}

fn parse_root_list(root: &Yaml, key: &str, file: &str) -> Result<Vec<String>, PackageError> {
    let values = string_list(root, key, file)?;
    if values.len() > MAX_LIST {
        return Err(PackageError::Parse(format!(
            "{file}: at most {MAX_LIST} `{key}` entries are allowed"
        )));
    }
    let mut seen = BTreeSet::new();
    for value in &values {
        if let Some(detail) = validate_write_root(value) {
            return Err(PackageError::Parse(format!(
                "{file}: `{key}` entry `{value}`: {detail}"
            )));
        }
        if !seen.insert(value.as_str()) {
            return Err(PackageError::Parse(format!(
                "{file}: duplicate `{key}` entry `{value}`"
            )));
        }
    }
    Ok(values)
}

fn parse_id_list(root: &Yaml, key: &str, file: &str) -> Result<Vec<String>, PackageError> {
    let values = string_list(root, key, file)?;
    if values.len() > MAX_LIST {
        return Err(PackageError::Parse(format!(
            "{file}: at most {MAX_LIST} `{key}` entries are allowed"
        )));
    }
    let mut seen = BTreeSet::new();
    for value in &values {
        if let Some(detail) = validate_skill_ref(value) {
            return Err(PackageError::Parse(format!(
                "{file}: `{key}` entry `{value}`: {detail}"
            )));
        }
        if !seen.insert(value.as_str()) {
            return Err(PackageError::Parse(format!(
                "{file}: duplicate `{key}` entry `{value}`"
            )));
        }
    }
    Ok(values)
}

fn string_list(root: &Yaml, key: &str, file: &str) -> Result<Vec<String>, PackageError> {
    let Some(value) = root.as_mapping_get(key) else {
        return Ok(Vec::new());
    };
    let sequence = value
        .as_sequence()
        .ok_or_else(|| PackageError::Parse(format!("{file}: `{key}` must be a list of strings")))?;
    let mut values = Vec::with_capacity(sequence.len());
    for item in sequence {
        let text = item.as_str().ok_or_else(|| {
            PackageError::Parse(format!("{file}: `{key}` must be a list of strings"))
        })?;
        values.push(text.to_string());
    }
    Ok(values)
}

fn instruction_body(body: String, file: &str) -> Result<String, PackageError> {
    if body.trim().is_empty() {
        return Err(PackageError::Parse(format!(
            "{file}: the instruction body must not be empty"
        )));
    }
    if body.chars().count() > MAX_INSTRUCTION_CHARS {
        return Err(PackageError::Parse(format!(
            "{file}: the instruction body exceeds {MAX_INSTRUCTION_CHARS} characters"
        )));
    }
    Ok(body.trim().to_string())
}

/// `None` when the root is a safe relative path, otherwise the reason.
fn validate_write_root(path: &str) -> Option<&'static str> {
    if path.is_empty() || path.chars().count() > MAX_PATH_CHARS {
        return Some("must be 1..=160 characters");
    }
    if path.starts_with('/') || path.ends_with('/') || path.contains('\\') {
        return Some("must be a relative forward-slash path");
    }
    for component in Path::new(path).components() {
        let Component::Normal(name) = component else {
            return Some("cannot contain `.` or `..`");
        };
        let Some(name) = name.to_str() else {
            return Some("must be UTF-8");
        };
        if name.is_empty() || name.starts_with('.') {
            return Some("cannot contain hidden names");
        }
    }
    None
}

/// `None` when `id` is a valid skill reference, otherwise the reason.
fn validate_skill_ref(id: &str) -> Option<&'static str> {
    let valid = !id.is_empty()
        && id.chars().count() <= 64
        && id.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
        && !id.starts_with('-')
        && !id.ends_with('-')
        && !id.contains("--");
    if valid {
        None
    } else {
        Some("invalid skill id")
    }
}

// --- filesystem discovery --------------------------------------------------

fn directory_names(dir: &Path, max: usize) -> Result<Vec<String>, PackageError> {
    let entries = read_entries(dir)?;
    let mut names = Vec::new();
    for entry in entries {
        let file_type = entry.file_type().map_err(|source| PackageError::Io {
            path: entry.path(),
            source,
        })?;
        if !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        names.push(name);
    }
    names.sort();
    if names.len() > max {
        return Err(PackageError::TooMany);
    }
    Ok(names)
}

fn command_names(dir: &Path) -> Result<Vec<String>, PackageError> {
    let entries = read_entries(dir)?;
    let mut names = Vec::new();
    for entry in entries {
        let file_type = entry.file_type().map_err(|source| PackageError::Io {
            path: entry.path(),
            source,
        })?;
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || !name.ends_with(".md") {
            continue;
        }
        let stem = name[..name.len() - 3].to_string();
        package::validate_id(&stem).map_err(|_| {
            PackageError::Parse(format!(
                "invalid command file `{name}` (command ids: 1..=32 chars, a-z, 0-9 and `-`, starting with a letter)"
            ))
        })?;
        names.push(stem);
    }
    names.sort();
    if names.len() > MAX_COMMANDS {
        return Err(PackageError::Parse(format!(
            "at most {MAX_COMMANDS} commands are allowed per scope"
        )));
    }
    Ok(names)
}

fn read_entries(dir: &Path) -> Result<Vec<fs::DirEntry>, PackageError> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(PackageError::Io {
                path: dir.to_path_buf(),
                source,
            });
        }
    };
    entries
        .map(|entry| {
            entry.map_err(|source| PackageError::Io {
                path: dir.to_path_buf(),
                source,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_agent() -> String {
        "\
---
name: Escritor
description: Escribe y propone cambios.
mode: build
write_roots: [guion, notas]
skills: [escaleta]
---

Escribe siguiendo la biblia del proyecto.
"
        .to_string()
    }

    fn valid_command() -> String {
        "\
---
name: Escaleta
description: Desglosa la estructura en escenas.
agent: escritor
---
Desglosa la estructura.
"
        .to_string()
    }

    #[test]
    fn parses_a_valid_portable_agent() {
        let agent = parse_agent(&valid_agent(), "escritor", PackageOrigin::Project)
            .expect("valid AGENT.md");
        assert_eq!(agent.id, "escritor");
        assert_eq!(agent.name, "Escritor");
        assert_eq!(agent.mode, AgentMode::Build);
        assert_eq!(agent.mode.key(), "build");
        assert_eq!(agent.write_roots, ["guion", "notas"]);
        assert_eq!(agent.skills, ["escaleta"]);
        assert!(agent.instructions.starts_with("Escribe siguiendo"));
        assert_eq!(agent.origin, PackageOrigin::Project);
        assert_eq!(agent.hash.len(), 64, "the hash is the trust anchor");
    }

    #[test]
    fn rejects_unknown_keys_unknown_modes_and_unsafe_roots() {
        let error = parse_agent(
            &valid_agent().replace("mode: build", "mode: build\ntrust: tools"),
            "escritor",
            PackageOrigin::Project,
        )
        .expect_err("unknown key");
        assert!(
            error.to_string().contains("unknown frontmatter key"),
            "{error}"
        );

        let error = parse_agent(
            &valid_agent().replace("mode: build", "mode: autonomous"),
            "escritor",
            PackageOrigin::Project,
        )
        .expect_err("unknown mode");
        assert!(error.to_string().contains("unknown mode"), "{error}");

        for unsafe_root in ["../outside", "/abs", ".oculto", "guion/../../salida"] {
            let text = valid_agent().replace(
                "write_roots: [guion, notas]",
                &format!("write_roots: [{unsafe_root}]"),
            );
            let error = parse_agent(&text, "escritor", PackageOrigin::Project)
                .expect_err("unsafe write root");
            assert!(
                error.to_string().contains("write_roots"),
                "{error}: {unsafe_root}"
            );
        }

        let error =
            parse_agent(&valid_agent(), "Escritor", PackageOrigin::Project).expect_err("bad id");
        assert!(error.to_string().contains("invalid package id"), "{error}");

        let error = parse_agent(
            &valid_agent().replace("Escribe siguiendo la biblia del proyecto.\n", "\n"),
            "escritor",
            PackageOrigin::Project,
        )
        .expect_err("empty body");
        assert!(error.to_string().contains("instruction body"), "{error}");
    }

    #[test]
    fn parses_a_command_and_validates_its_agent_reference() {
        let command = parse_command(&valid_command(), "escaleta", PackageOrigin::Project)
            .expect("valid command");
        assert_eq!(command.id, "escaleta");
        assert_eq!(command.name, "Escaleta");
        assert_eq!(command.description, "Desglosa la estructura en escenas.");
        assert_eq!(command.agent.as_deref(), Some("escritor"));
        assert!(command.prompt.starts_with("Desglosa la estructura."));

        let error = parse_command(
            &valid_command().replace("agent: escritor", "agent: ../otro"),
            "escaleta",
            PackageOrigin::Project,
        )
        .expect_err("unsafe agent id");
        assert!(error.to_string().contains("invalid package id"), "{error}");

        let missing =
            valid_command().replace("description: Desglosa la estructura en escenas.\n", "");
        let error =
            parse_command(&missing, "escaleta", PackageOrigin::Project).expect_err("description");
        assert!(error.to_string().contains("`description`"), "{error}");
    }

    #[test]
    fn discovery_skips_agent_yaml_directories_and_loads_portable_ones() {
        let root = tempfile::tempdir().expect("tempdir");
        let agents = root.path().join(package::PROJECT_PACKAGES_DIR);

        // Directory with both files: `agent.yaml` wins, portable skips it.
        fs::create_dir_all(agents.join("ambos")).expect("dir");
        fs::write(agents.join("ambos/AGENT.md"), valid_agent()).expect("agent md");
        fs::write(
            agents.join("ambos/agent.yaml"),
            "schema_version: 1\napi_version: 1\nid: ambos\nname: Ambos\nversion: 1.0.0\ninstructions: |\n  Hola.\ntasks:\n  - scene_review\ntrust: instructions\n",
        )
        .expect("agent yaml");

        // Portable-only directory: loaded from AGENT.md.
        fs::create_dir_all(agents.join("portatil")).expect("dir");
        fs::write(
            agents.join("portatil/AGENT.md"),
            valid_agent().replace("name: Escritor", "name: Portátil"),
        )
        .expect("agent md");

        let loaded = discover_agents(
            root.path(),
            package::PROJECT_PACKAGES_DIR,
            PackageOrigin::Project,
        )
        .expect("discover");
        let ids: Vec<&str> = loaded.iter().map(|agent| agent.id.as_str()).collect();
        assert_eq!(ids, ["portatil"], "agent.yaml owns its own directory");

        // Commands: every `.md` is a command; other files are ignored.
        let commands = root.path().join(PROJECT_COMMANDS_DIR);
        fs::create_dir_all(&commands).expect("commands dir");
        fs::write(
            commands.join("idea.md"),
            valid_command().replace("agent: escritor", "agent: portatil"),
        )
        .expect("command");
        fs::write(
            commands.join("voz.md"),
            valid_command().replace("agent: escritor", "agent: portatil"),
        )
        .expect("command");
        fs::write(commands.join("NOTES.txt"), "not a command").expect("non md");

        let loaded = discover_commands(root.path(), PROJECT_COMMANDS_DIR, PackageOrigin::Project)
            .expect("discover commands");
        let ids: Vec<&str> = loaded.iter().map(|command| command.id.as_str()).collect();
        assert_eq!(ids, ["idea", "voz"], "sorted by id");
        assert_eq!(
            loaded[0].agent.as_deref(),
            Some("portatil"),
            "commands resolve their agent by id"
        );

        // A malformed command file is an explicit error, never a silent drop.
        fs::write(commands.join("rota.md"), "sin frontmatter").expect("broken");
        let error = discover_commands(root.path(), PROJECT_COMMANDS_DIR, PackageOrigin::Project)
            .expect_err("broken command");
        assert!(error.to_string().contains("rota.md"), "{error}");
    }

    #[test]
    fn missing_directories_are_empty_not_an_error() {
        let root = tempfile::tempdir().expect("tempdir");
        assert!(
            discover_agents(
                root.path(),
                package::PROJECT_PACKAGES_DIR,
                PackageOrigin::Project
            )
            .expect("empty agents")
            .is_empty()
        );
        assert!(
            discover_commands(root.path(), PROJECT_COMMANDS_DIR, PackageOrigin::Project)
                .expect("empty commands")
                .is_empty()
        );
    }
}
