//! Versioned, instruction-only agent packages (M15, Phase 2 slice 1).
//!
//! A package is a directory with one `agent.yaml`:
//!
//! ```yaml
//! schema_version: 1
//! api_version: 1
//! id: "documentalista"
//! name: "Documentalista"
//! version: "1.0.0"
//! description: "Contrasta datos y fuentes del proyecto."
//! instructions: |
//!   ...
//! tasks:
//!   - scene_review
//! trust: instructions
//! ```
//!
//! Trust is explicit. Only `instructions` is executable today: a package that
//! asks for `tools` is rejected instead of silently running code. The
//! `api_version` is the compatibility contract; unknown versions fail.
//!
//! Project packages live in `<root>/.narra/agents/<id>/agent.yaml`; user
//! packages live in `<user-config>/agents/<id>/agent.yaml`. Project packages
//! override user packages with the same id; `agents.yaml` overrides both.
//! Discovery is read-only and never touches the narrative canon.

use std::fs;
use std::path::{Path, PathBuf};

use saphyr::{LoadableYamlNode, Yaml};
use serde::Serialize;
use thiserror::Error;

use crate::document::{self, DocumentError};

use super::roles::TASK_KINDS;

/// File inside each package directory.
pub const PACKAGE_FILE: &str = "agent.yaml";
/// Project-local package root, relative to the project.
pub const PROJECT_PACKAGES_DIR: &str = ".narra/agents";
/// The package-format version this build writes and accepts.
pub const PACKAGE_SCHEMA_VERSION: u32 = 1;
/// The extension API version this build implements.
pub const PACKAGE_API_VERSION: u32 = 1;

const MAX_PACKAGE_BYTES: usize = 64 * 1024;
const MAX_PACKAGES: usize = 32;
const MAX_ID_CHARS: usize = 32;
const MAX_NAME_CHARS: usize = 64;
const MAX_VERSION_CHARS: usize = 32;
const MAX_DESCRIPTION_CHARS: usize = 280;
const MAX_INSTRUCTION_CHARS: usize = 8_000;

const PACKAGE_KEYS: [&str; 10] = [
    "schema_version",
    "api_version",
    "id",
    "name",
    "version",
    "description",
    "instructions",
    "tasks",
    "model",
    "trust",
];

/// Explicit trust level requested by a package. Executable tools are not
/// supported yet, so `Tools` never becomes an effective role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageTrust {
    Instructions,
    Tools,
}

impl PackageTrust {
    pub fn key(self) -> &'static str {
        match self {
            PackageTrust::Instructions => "instructions",
            PackageTrust::Tools => "tools",
        }
    }
}

/// Scope a package was loaded from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageOrigin {
    User,
    Project,
}

impl PackageOrigin {
    pub fn key(self) -> &'static str {
        match self {
            PackageOrigin::User => "user",
            PackageOrigin::Project => "project",
        }
    }
}

/// One validated agent package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentPackage {
    pub schema_version: u32,
    pub api_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub instructions: String,
    pub tasks: Vec<String>,
    pub model: Option<String>,
    pub trust: PackageTrust,
    pub origin: PackageOrigin,
}

/// Package loading/validation failures. Messages name the file/field and
/// never echo whole file contents.
#[derive(Debug, Error)]
pub enum PackageError {
    #[error("agent package: {0}")]
    Parse(String),
    #[error(transparent)]
    Document(#[from] DocumentError),
    #[error("agent packages: failed to read `{path}`: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("agent package `{directory}` declares id `{id}` (the directory name must match)")]
    IdMismatch { directory: String, id: String },
    #[error(
        "agent package `{id}` requests executable tools, but only instruction-only packages are supported yet"
    )]
    UnsupportedTrust { id: String },
    #[error(
        "agent package `{id}` requires API version {found}, but this build supports {supported}"
    )]
    UnsupportedApiVersion {
        id: String,
        found: u32,
        supported: u32,
    },
    #[error("agent packages: more than {MAX_PACKAGES} packages")]
    TooMany,
}

/// Parse and validate one `agent.yaml` without touching the filesystem.
pub fn parse_package(text: &str, origin: PackageOrigin) -> Result<AgentPackage, PackageError> {
    if text.len() > MAX_PACKAGE_BYTES {
        return Err(PackageError::Parse(format!(
            "{PACKAGE_FILE} is larger than {MAX_PACKAGE_BYTES} bytes"
        )));
    }
    let documents =
        Yaml::load_from_str(text).map_err(|error| PackageError::Parse(error.to_string()))?;
    if documents.len() != 1 {
        return Err(PackageError::Parse(format!(
            "expected exactly one YAML document, found {}",
            documents.len()
        )));
    }
    let root = documents.into_iter().next().expect("length checked above");
    let mapping = root
        .as_mapping()
        .ok_or_else(|| PackageError::Parse("top level must be a mapping".to_string()))?;
    for key in mapping.keys() {
        let key = key
            .as_str()
            .ok_or_else(|| PackageError::Parse("mapping keys must be strings".to_string()))?;
        if !PACKAGE_KEYS.contains(&key) {
            return Err(PackageError::Parse(format!("unknown key `{key}`")));
        }
    }

    let schema_version = required_integer(&root, "schema_version")?;
    if schema_version != i64::from(PACKAGE_SCHEMA_VERSION) {
        return Err(PackageError::Parse(format!(
            "unsupported schema_version {schema_version}; this build supports {PACKAGE_SCHEMA_VERSION}"
        )));
    }
    let api_version = required_integer(&root, "api_version")?;
    let id = required_string(&root, "id", "package id")?;
    validate_id(id)?;
    if api_version != i64::from(PACKAGE_API_VERSION) {
        return Err(PackageError::UnsupportedApiVersion {
            id: id.to_string(),
            found: u32::try_from(api_version).unwrap_or(0),
            supported: PACKAGE_API_VERSION,
        });
    }

    let name = required_string(&root, "name", "display name")?;
    if name.trim().is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(PackageError::Parse(format!(
            "package `{id}`: `name` must be 1..={MAX_NAME_CHARS} characters"
        )));
    }
    let version = required_string(&root, "version", "package version")?;
    if version.is_empty()
        || version.chars().count() > MAX_VERSION_CHARS
        || !version.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '+')
        })
    {
        return Err(PackageError::Parse(format!(
            "package `{id}`: `version` must be 1..={MAX_VERSION_CHARS} chars (letters, digits, `.`, `-`, `+`)"
        )));
    }
    let instructions = required_string(&root, "instructions", "persona instructions")?;
    if instructions.trim().is_empty() {
        return Err(PackageError::Parse(format!(
            "package `{id}`: `instructions` must not be empty"
        )));
    }
    if instructions.chars().count() > MAX_INSTRUCTION_CHARS {
        return Err(PackageError::Parse(format!(
            "package `{id}`: `instructions` exceed {MAX_INSTRUCTION_CHARS} characters"
        )));
    }
    let description = match root.as_mapping_get("description") {
        None => None,
        Some(value) => {
            let description = value.as_str().ok_or_else(|| {
                PackageError::Parse(format!("package `{id}`: `description` must be a string"))
            })?;
            if description.trim().is_empty() {
                return Err(PackageError::Parse(format!(
                    "package `{id}`: `description` must not be empty"
                )));
            }
            if description.chars().count() > MAX_DESCRIPTION_CHARS {
                return Err(PackageError::Parse(format!(
                    "package `{id}`: `description` exceeds {MAX_DESCRIPTION_CHARS} characters"
                )));
            }
            Some(description.to_string())
        }
    };
    let model = match root.as_mapping_get("model") {
        None => None,
        Some(value) => {
            let model = value.as_str().ok_or_else(|| {
                PackageError::Parse(format!("package `{id}`: `model` must be a string"))
            })?;
            if model.trim().is_empty() {
                return Err(PackageError::Parse(format!(
                    "package `{id}`: `model` must not be empty"
                )));
            }
            Some(model.to_string())
        }
    };
    let tasks = parse_tasks(&root, id)?;
    let trust_raw = required_string(&root, "trust", "trust level")?;
    let trust = match trust_raw {
        "instructions" => PackageTrust::Instructions,
        "tools" => {
            return Err(PackageError::UnsupportedTrust { id: id.to_string() });
        }
        other => {
            return Err(PackageError::Parse(format!(
                "package `{id}`: unknown trust level `{other}` (expected `instructions`)"
            )));
        }
    };

    Ok(AgentPackage {
        schema_version: PACKAGE_SCHEMA_VERSION,
        api_version: PACKAGE_API_VERSION,
        id: id.to_string(),
        name: name.to_string(),
        version: version.to_string(),
        description,
        instructions: instructions.to_string(),
        tasks,
        model,
        trust,
        origin,
    })
}

/// Discover packages under `root/relative_dir` (or `root` when empty).
/// Missing directories are empty, never an error. Results are deterministic
/// (directory names sorted), and the package id must match its directory.
pub fn discover(
    root: &Path,
    relative_dir: &str,
    origin: PackageOrigin,
) -> Result<Vec<AgentPackage>, PackageError> {
    let dir = if relative_dir.is_empty() {
        root.to_path_buf()
    } else {
        root.join(relative_dir)
    };
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(PackageError::Io { path: dir, source });
        }
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| PackageError::Io {
            path: dir.clone(),
            source,
        })?;
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
    if names.len() > MAX_PACKAGES {
        return Err(PackageError::TooMany);
    }

    let mut packages = Vec::with_capacity(names.len());
    for name in names {
        validate_id(&name)?;
        let relative = if relative_dir.is_empty() {
            format!("{name}/{PACKAGE_FILE}")
        } else {
            format!("{relative_dir}/{name}/{PACKAGE_FILE}")
        };
        let text = document::read_document(root, &relative)?;
        let package = parse_package(&text, origin)?;
        if package.id != name {
            return Err(PackageError::IdMismatch {
                directory: name,
                id: package.id,
            });
        }
        packages.push(package);
    }
    Ok(packages)
}

/// The package directory for a project.
pub fn project_packages_dir(root: &Path) -> PathBuf {
    root.join(PROJECT_PACKAGES_DIR)
}

/// The package directory for a user config directory.
pub fn user_packages_dir(config_dir: &Path) -> PathBuf {
    config_dir.join("agents")
}

fn validate_id(id: &str) -> Result<(), PackageError> {
    let valid = !id.is_empty()
        && id.len() <= MAX_ID_CHARS
        && id.starts_with(|character: char| character.is_ascii_lowercase())
        && id.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        });
    if !valid {
        return Err(PackageError::Parse(format!(
            "invalid package id `{id}` (1..={MAX_ID_CHARS} chars: a-z, 0-9 and `-`, starting with a letter)"
        )));
    }
    Ok(())
}

fn required_string<'a>(yaml: &'a Yaml, key: &str, what: &str) -> Result<&'a str, PackageError> {
    yaml.as_mapping_get(key)
        .and_then(Yaml::as_str)
        .ok_or_else(|| PackageError::Parse(format!("missing or non-string `{key}` ({what})")))
}

fn required_integer(yaml: &Yaml, key: &str) -> Result<i64, PackageError> {
    yaml.as_mapping_get(key)
        .and_then(Yaml::as_integer)
        .ok_or_else(|| PackageError::Parse(format!("missing or non-integer `{key}`")))
}

fn parse_tasks(yaml: &Yaml, id: &str) -> Result<Vec<String>, PackageError> {
    let sequence = yaml
        .as_mapping_get("tasks")
        .and_then(Yaml::as_sequence)
        .ok_or_else(|| PackageError::Parse(format!("package `{id}`: missing `tasks` sequence")))?;
    if sequence.is_empty() {
        return Err(PackageError::Parse(format!(
            "package `{id}`: `tasks` must not be empty"
        )));
    }
    let mut tasks = Vec::new();
    for value in sequence {
        let task = value.as_str().ok_or_else(|| {
            PackageError::Parse(format!("package `{id}`: every task must be a string"))
        })?;
        if !TASK_KINDS.contains(&task) {
            return Err(PackageError::Parse(format!(
                "package `{id}`: unknown task `{task}` (known: {})",
                TASK_KINDS.join(", ")
            )));
        }
        if tasks.iter().any(|existing| existing == task) {
            return Err(PackageError::Parse(format!(
                "package `{id}`: duplicate task `{task}`"
            )));
        }
        tasks.push(task.to_string());
    }
    Ok(tasks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_package() -> String {
        "\
schema_version: 1
api_version: 1
id: documentalista
name: Documentalista
version: 1.0.0
description: Contrasta datos y fuentes.
instructions: |
  Contrastas datos y citas la fuente.
tasks:
  - scene_review
trust: instructions
"
        .to_string()
    }

    #[test]
    fn parses_a_valid_instruction_package() {
        let package =
            parse_package(&valid_package(), PackageOrigin::Project).expect("valid package");
        assert_eq!(package.id, "documentalista");
        assert_eq!(package.version, "1.0.0");
        assert_eq!(package.tasks, vec!["scene_review"]);
        assert_eq!(package.trust, PackageTrust::Instructions);
        assert_eq!(package.origin, PackageOrigin::Project);
    }

    #[test]
    fn rejects_tools_trust_explicitly() {
        let text = valid_package().replace("trust: instructions", "trust: tools");
        let error = parse_package(&text, PackageOrigin::User).expect_err("tools rejected");
        assert!(
            matches!(error, PackageError::UnsupportedTrust { .. }),
            "{error}"
        );
    }

    #[test]
    fn rejects_unknown_api_version_and_unknown_keys() {
        let text = valid_package().replace("api_version: 1", "api_version: 2");
        let error = parse_package(&text, PackageOrigin::Project).expect_err("api version");
        assert!(
            matches!(error, PackageError::UnsupportedApiVersion { .. }),
            "{error}"
        );

        let text = valid_package() + "unknown: true\n";
        let error = parse_package(&text, PackageOrigin::Project).expect_err("unknown key");
        assert!(error.to_string().contains("unknown key"), "{error}");
    }

    #[test]
    fn rejects_unknown_tasks_and_bad_ids() {
        let text = valid_package().replace("  - scene_review", "  - scene_dance");
        let error = parse_package(&text, PackageOrigin::Project).expect_err("unknown task");
        assert!(error.to_string().contains("unknown task"), "{error}");

        let text = valid_package().replace("id: documentalista", "id: Documentalista!");
        let error = parse_package(&text, PackageOrigin::Project).expect_err("bad id");
        assert!(error.to_string().contains("invalid package id"), "{error}");
    }

    #[test]
    fn discovery_requires_directory_and_id_to_match() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join(".narra/agents/documentalista")).expect("dirs");
        fs::write(
            dir.path().join(".narra/agents/documentalista/agent.yaml"),
            valid_package(),
        )
        .expect("write");
        let packages =
            discover(dir.path(), PROJECT_PACKAGES_DIR, PackageOrigin::Project).expect("discover");
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].origin, PackageOrigin::Project);

        fs::write(
            dir.path().join(".narra/agents/documentalista/agent.yaml"),
            valid_package().replace("id: documentalista", "id: otro"),
        )
        .expect("write");
        let error = discover(dir.path(), PROJECT_PACKAGES_DIR, PackageOrigin::Project)
            .expect_err("id mismatch");
        assert!(matches!(error, PackageError::IdMismatch { .. }), "{error}");
    }
}
