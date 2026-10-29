//! Project-type packages (M17 slice 1, D-045).
//!
//! A project type is a directory with one `type.yaml`, a set of directories
//! and text templates. The six types shipped by Narra are built in; users can
//! add a package under `<config>/project-types/<id>/` and projects can carry
//! one under `<root>/.narra/types/<id>/`. Packages are configuration and
//! instructions only: they never execute code.
//!
//! The package id is the directory name and matches `project_type` in
//! `project.yaml`. Built-in ids are reserved and cannot be replaced.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use saphyr::{LoadableYamlNode, Yaml};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// File inside each package directory.
pub const TYPE_FILE: &str = "type.yaml";
/// Project-local type packages, relative to the project root.
pub const PROJECT_TYPES_DIR: &str = ".narra/types";
/// User-scope type packages, relative to the user config directory.
pub const USER_TYPES_DIR: &str = "project-types";
/// Trust record for project-level type packages.
pub const TYPE_TRUST_FILE: &str = ".narra/trusted-types.json";
/// Package format this build reads.
pub const SCHEMA_VERSION: u32 = 1;
/// Capability API this build implements.
pub const API_VERSION: u32 = 1;

const MAX_PACKAGE_BYTES: usize = 256 * 1024;
const MAX_DIRECTORIES: usize = 64;
const MAX_FILES: usize = 64;
const MAX_FILE_BYTES: usize = 64 * 1024;
const MAX_ID_CHARS: usize = 64;
const MAX_NAME_CHARS: usize = 80;
const MAX_DESCRIPTION_CHARS: usize = 500;
const MAX_PATH_CHARS: usize = 160;
const MAX_SKILLS: usize = 32;

const PACKAGE_KEYS: [&str; 9] = [
    "schema_version",
    "api_version",
    "id",
    "name",
    "description",
    "directories",
    "files",
    "skills",
    "capabilities",
];

/// Where a package was loaded from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeOrigin {
    Builtin,
    User,
    Project,
}

impl TypeOrigin {
    pub fn key(self) -> &'static str {
        match self {
            TypeOrigin::Builtin => "builtin",
            TypeOrigin::User => "user",
            TypeOrigin::Project => "project",
        }
    }
}

/// A template file with English and Spanish content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalizedText {
    pub en: String,
    pub es: String,
}

impl LocalizedText {
    pub fn for_language(&self, language: &str) -> &str {
        if language == "es" { &self.es } else { &self.en }
    }
}

/// One file written by a project-type template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateFile {
    pub path: String,
    pub content: LocalizedText,
}

/// What the editor exposes for a type. The UI consumes this profile instead
/// of matching on a type id. Unknown entity ids are ignored by this build.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TypeCapabilities {
    /// Fountain document support: element cycling, scene review and PDF.
    pub screenplay: bool,
    /// Entity kinds that can be created from a selection.
    pub entity_kinds: Vec<String>,
    /// PDF export is available for this type.
    pub export_pdf: bool,
}

impl TypeCapabilities {
    pub fn screenplay() -> Self {
        Self {
            screenplay: true,
            entity_kinds: vec![
                "character".to_string(),
                "location".to_string(),
                "object".to_string(),
            ],
            export_pdf: true,
        }
    }
}

/// UI-neutral view of one type's capabilities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectProfile {
    pub type_id: String,
    pub screenplay: bool,
    pub entity_kinds: Vec<String>,
    pub export_pdf: bool,
}

impl ProjectProfile {
    pub fn screenplay(type_id: impl Into<String>) -> Self {
        let capabilities = TypeCapabilities::screenplay();
        Self {
            type_id: type_id.into(),
            screenplay: capabilities.screenplay,
            entity_kinds: capabilities.entity_kinds,
            export_pdf: capabilities.export_pdf,
        }
    }

    pub fn allows_entity(&self, kind: &str) -> bool {
        self.entity_kinds.iter().any(|candidate| candidate == kind)
    }
}

impl Default for ProjectProfile {
    fn default() -> Self {
        Self {
            type_id: "other".to_string(),
            screenplay: false,
            entity_kinds: Vec::new(),
            export_pdf: false,
        }
    }
}

/// A validated project-type package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTypePackage {
    pub schema_version: u32,
    pub api_version: u32,
    pub id: String,
    pub name: String,
    pub description: String,
    pub directories: Vec<String>,
    pub files: Vec<TemplateFile>,
    pub skills: Vec<String>,
    pub capabilities: TypeCapabilities,
    pub origin: TypeOrigin,
    /// Source directory for user/project packages, used when copying the
    /// package into a created project. Built-ins have no source directory.
    pub source_dir: Option<PathBuf>,
    /// SHA-256 of the package files; project packages are trusted by hash.
    pub content_hash: String,
}

impl ProjectTypePackage {
    pub fn file(&self, path: &str) -> Option<&TemplateFile> {
        self.files.iter().find(|file| file.path == path)
    }

    pub fn profile(&self) -> ProjectProfile {
        ProjectProfile {
            type_id: self.id.clone(),
            screenplay: self.capabilities.screenplay,
            entity_kinds: self.capabilities.entity_kinds.clone(),
            export_pdf: self.capabilities.export_pdf,
        }
    }

    /// Validate the package without touching the filesystem. The id must be
    /// safe and every path must stay inside the project.
    pub fn validate(&self) -> Result<(), TypePackageError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(TypePackageError::UnsupportedSchema {
                found: self.schema_version,
                supported: SCHEMA_VERSION,
            });
        }
        if self.api_version != API_VERSION {
            return Err(TypePackageError::UnsupportedApi {
                found: self.api_version,
                supported: API_VERSION,
            });
        }
        validate_id(&self.id)?;
        if self.name.trim().is_empty() {
            return Err(TypePackageError::Invalid {
                field: "name",
                detail: "must not be empty".to_string(),
            });
        }
        if self.name.chars().count() > MAX_NAME_CHARS {
            return Err(TypePackageError::Invalid {
                field: "name",
                detail: format!("must be at most {MAX_NAME_CHARS} characters"),
            });
        }
        if self.description.chars().count() > MAX_DESCRIPTION_CHARS {
            return Err(TypePackageError::Invalid {
                field: "description",
                detail: format!("must be at most {MAX_DESCRIPTION_CHARS} characters"),
            });
        }
        if self.directories.len() > MAX_DIRECTORIES {
            return Err(TypePackageError::Invalid {
                field: "directories",
                detail: format!("at most {MAX_DIRECTORIES} directories are allowed"),
            });
        }
        if self.files.len() > MAX_FILES {
            return Err(TypePackageError::Invalid {
                field: "files",
                detail: format!("at most {MAX_FILES} files are allowed"),
            });
        }
        if self.skills.len() > MAX_SKILLS {
            return Err(TypePackageError::Invalid {
                field: "skills",
                detail: format!("at most {MAX_SKILLS} skills are allowed"),
            });
        }

        let mut dirs = BTreeSet::new();
        for directory in &self.directories {
            validate_relative_path("directories", directory)?;
            if !dirs.insert(directory.as_str()) {
                return Err(TypePackageError::Invalid {
                    field: "directories",
                    detail: format!("duplicate directory `{directory}`"),
                });
            }
        }

        let mut paths = BTreeSet::new();
        for file in &self.files {
            validate_relative_path("files", &file.path)?;
            if !paths.insert(file.path.as_str()) {
                return Err(TypePackageError::Invalid {
                    field: "files",
                    detail: format!("duplicate file `{}`", file.path),
                });
            }
            for (language, content) in [("en", &file.content.en), ("es", &file.content.es)] {
                if content.len() > MAX_FILE_BYTES {
                    return Err(TypePackageError::Invalid {
                        field: "files",
                        detail: format!(
                            "`{}` ({language}) is larger than {MAX_FILE_BYTES} bytes",
                            file.path
                        ),
                    });
                }
            }
        }

        let mut skills = BTreeSet::new();
        for skill in &self.skills {
            validate_id(skill)?;
            if !skills.insert(skill.as_str()) {
                return Err(TypePackageError::Invalid {
                    field: "skills",
                    detail: format!("duplicate skill `{skill}`"),
                });
            }
        }
        if self.capabilities.entity_kinds.len() > 16 {
            return Err(TypePackageError::Invalid {
                field: "capabilities.entity_kinds",
                detail: "at most 16 entity kinds are allowed".to_string(),
            });
        }
        let mut entities = BTreeSet::new();
        for entity in &self.capabilities.entity_kinds {
            validate_id(entity)?;
            if !entities.insert(entity.as_str()) {
                return Err(TypePackageError::Invalid {
                    field: "capabilities.entity_kinds",
                    detail: format!("duplicate entity kind `{entity}`"),
                });
            }
        }
        Ok(())
    }
}

/// Package loading and validation failures.
#[derive(Debug, Error)]
pub enum TypePackageError {
    #[error("project type package is too large: {path}")]
    TooLarge { path: PathBuf },
    #[error("could not read project type package `{path}`: {error}")]
    Io { path: PathBuf, error: String },
    #[error("project type package `{path}` is invalid: {error}")]
    Parse { path: PathBuf, error: String },
    #[error("unsupported project type schema_version {found}; this build supports {supported}")]
    UnsupportedSchema { found: u32, supported: u32 },
    #[error("unsupported project type api_version {found}; this build supports {supported}")]
    UnsupportedApi { found: u32, supported: u32 },
    #[error("invalid project type package: `{field}` {detail}")]
    Invalid { field: &'static str, detail: String },
    #[error("project type id `{0}` is reserved by a built-in type")]
    ReservedId(String),
}

/// The six built-in packages, in menu order.
pub const BUILTIN_IDS: [&str; 6] = [
    "feature",
    "series",
    "web_series",
    "short",
    "microdrama",
    "other",
];

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn builtin_hash(id: &str) -> String {
    sha256_hex(format!("builtin-project-type:{id}:v{SCHEMA_VERSION}").as_bytes())
}

fn directory_hash(directory: &Path) -> Result<String, TypePackageError> {
    fn collect(
        base: &Path,
        current: &Path,
        files: &mut Vec<PathBuf>,
    ) -> Result<(), TypePackageError> {
        let entries = fs::read_dir(current).map_err(|error| TypePackageError::Io {
            path: current.to_path_buf(),
            error: error.to_string(),
        })?;
        for entry in entries {
            let entry = entry.map_err(|error| TypePackageError::Io {
                path: current.to_path_buf(),
                error: error.to_string(),
            })?;
            let path = entry.path();
            if entry
                .file_type()
                .map_err(|error| TypePackageError::Io {
                    path: path.clone(),
                    error: error.to_string(),
                })?
                .is_dir()
            {
                collect(base, &path, files)?;
            } else {
                files.push(path.strip_prefix(base).unwrap_or(&path).to_path_buf());
            }
        }
        Ok(())
    }
    let mut relative = Vec::new();
    collect(directory, directory, &mut relative)?;
    relative.sort();
    let mut hasher = Sha256::new();
    for path in relative {
        hasher.update(path.to_string_lossy().as_bytes());
        hasher.update([0]);
        let bytes = fs::read(directory.join(&path)).map_err(|error| TypePackageError::Io {
            path: directory.join(&path),
            error: error.to_string(),
        })?;
        hasher.update(bytes);
        hasher.update([0]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Hashes explicitly trusted for project-level type packages.
pub fn trusted_type_hashes(project_root: &Path) -> Result<BTreeMap<String, String>, String> {
    let path = project_root.join(TYPE_TRUST_FILE);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => return Err(format!("{}: {error}", path.display())),
    };
    let value: TypeTrustFile =
        serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    if value.schema_version != 1 {
        return Err(format!(
            "unsupported type trust schema_version {}",
            value.schema_version
        ));
    }
    Ok(value.types)
}

/// Trust one project-level type package at its current content hash.
pub fn trust_type(project_root: &Path, id: &str, hash: &str) -> Result<(), String> {
    let mut types = trusted_type_hashes(project_root)?;
    types.insert(id.to_string(), hash.to_string());
    let file = TypeTrustFile {
        schema_version: 1,
        types,
    };
    let text = serde_json::to_string_pretty(&file)
        .map_err(|error| format!("could not encode type trust: {error}"))?;
    let root = project_root.join(".narra");
    fs::create_dir_all(&root).map_err(|error| format!("{}: {error}", root.display()))?;
    crate::document::write_document(project_root, TYPE_TRUST_FILE, &text)
        .map_err(|error| error.to_string())
}

/// Whether a project-level package needs confirmation.
pub fn untrusted_project_type(
    project_root: &Path,
    package: &ProjectTypePackage,
) -> Result<bool, String> {
    if package.origin != TypeOrigin::Project {
        return Ok(false);
    }
    let trusted = trusted_type_hashes(project_root)?;
    Ok(trusted.get(&package.id) != Some(&package.content_hash))
}

#[derive(Debug, Serialize, Deserialize)]
struct TypeTrustFile {
    schema_version: u32,
    #[serde(default)]
    types: BTreeMap<String, String>,
}

/// Look up a built-in package by id.
pub fn builtin(id: &str) -> Option<ProjectTypePackage> {
    let (name, description) = match id {
        "feature" => ("Feature film", "A feature-length screenplay project."),
        "series" => (
            "Series",
            "A series project with one screenplay per episode.",
        ),
        "web_series" => ("Web series", "A short-form series project."),
        "short" => ("Short film", "A short-film screenplay project."),
        "microdrama" => (
            "Microdrama",
            "A microdrama project with very short episodes.",
        ),
        "other" => ("Other", "A writing project with a custom structure."),
        _ => return None,
    };
    let (directories, files) = builtin_template();
    Some(ProjectTypePackage {
        schema_version: SCHEMA_VERSION,
        api_version: API_VERSION,
        id: id.to_string(),
        name: name.to_string(),
        description: description.to_string(),
        directories,
        files,
        skills: Vec::new(),
        capabilities: TypeCapabilities::screenplay(),
        origin: TypeOrigin::Builtin,
        source_dir: None,
        content_hash: builtin_hash(id),
    })
}

/// All six built-in packages, in menu order.
pub fn builtins() -> Vec<ProjectTypePackage> {
    BUILTIN_IDS.iter().filter_map(|id| builtin(id)).collect()
}

/// Load one package directory. The directory name must match `id`.
pub fn load_package(
    directory: &Path,
    origin: TypeOrigin,
) -> Result<ProjectTypePackage, TypePackageError> {
    let path = directory.join(TYPE_FILE);
    let text = fs::read_to_string(&path).map_err(|error| TypePackageError::Io {
        path: path.clone(),
        error: error.to_string(),
    })?;
    if text.len() > MAX_PACKAGE_BYTES {
        return Err(TypePackageError::TooLarge { path });
    }
    let mut package = parse_package(&text, origin).map_err(|error| TypePackageError::Parse {
        path: path.clone(),
        error: error.to_string(),
    })?;
    package.source_dir = Some(directory.to_path_buf());
    package.content_hash = directory_hash(directory)?;
    let directory_name = directory
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if package.id != directory_name {
        return Err(TypePackageError::Invalid {
            field: "id",
            detail: format!(
                "`{}` must match its directory name `{directory_name}`",
                package.id
            ),
        });
    }
    Ok(package)
}

/// Load every package in a scope directory. A missing directory is empty.
pub fn load_scope(
    directory: &Path,
    origin: TypeOrigin,
) -> Result<Vec<ProjectTypePackage>, TypePackageError> {
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let entries = fs::read_dir(directory).map_err(|error| TypePackageError::Io {
        path: directory.to_path_buf(),
        error: error.to_string(),
    })?;
    let mut packages = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| TypePackageError::Io {
            path: directory.to_path_buf(),
            error: error.to_string(),
        })?;
        if entry
            .file_type()
            .map_err(|error| TypePackageError::Io {
                path: entry.path(),
                error: error.to_string(),
            })?
            .is_dir()
        {
            packages.push(load_package(&entry.path(), origin)?);
        }
    }
    packages.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(packages)
}

/// Resolve built-ins, user packages and project packages. Project packages
/// override user packages with the same id; built-in ids stay reserved.
pub fn registry(
    project_root: Option<&Path>,
    config_dir: Option<&Path>,
) -> Result<Vec<ProjectTypePackage>, TypePackageError> {
    let mut by_id: std::collections::BTreeMap<String, ProjectTypePackage> = builtins()
        .into_iter()
        .map(|package| (package.id.clone(), package))
        .collect();
    if let Some(config_dir) = config_dir {
        for package in load_scope(&config_dir.join(USER_TYPES_DIR), TypeOrigin::User)? {
            if builtin(&package.id).is_some() {
                return Err(TypePackageError::ReservedId(package.id));
            }
            by_id.insert(package.id.clone(), package);
        }
    }
    if let Some(project_root) = project_root {
        for package in load_scope(&project_root.join(PROJECT_TYPES_DIR), TypeOrigin::Project)? {
            if builtin(&package.id).is_some() {
                return Err(TypePackageError::ReservedId(package.id));
            }
            by_id.insert(package.id.clone(), package);
        }
    }
    Ok(by_id.into_values().collect())
}

/// Parse a `type.yaml` document without a directory (used by tests and the
/// loader). The id is validated but not compared with a directory here.
pub fn parse_package(
    text: &str,
    origin: TypeOrigin,
) -> Result<ProjectTypePackage, TypePackageError> {
    let documents = Yaml::load_from_str(text).map_err(|error| TypePackageError::Invalid {
        field: "type.yaml",
        detail: error.to_string(),
    })?;
    if documents.len() != 1 {
        return Err(TypePackageError::Invalid {
            field: "type.yaml",
            detail: format!("expected one YAML document, found {}", documents.len()),
        });
    }
    let root = documents.into_iter().next().expect("length checked");
    let mapping = root.as_mapping().ok_or_else(|| TypePackageError::Invalid {
        field: "type.yaml",
        detail: "top level must be a mapping".to_string(),
    })?;
    for key in mapping.keys() {
        let key = key.as_str().ok_or_else(|| TypePackageError::Invalid {
            field: "type.yaml",
            detail: "keys must be strings".to_string(),
        })?;
        if !PACKAGE_KEYS.contains(&key) {
            return Err(TypePackageError::Invalid {
                field: "type.yaml",
                detail: format!("unknown key `{key}`"),
            });
        }
    }
    let schema_version = required_integer(&root, "schema_version")?;
    let api_version = required_integer(&root, "api_version")?;
    if schema_version != i64::from(SCHEMA_VERSION) {
        return Err(TypePackageError::UnsupportedSchema {
            found: u32::try_from(schema_version).unwrap_or(0),
            supported: SCHEMA_VERSION,
        });
    }
    if api_version != i64::from(API_VERSION) {
        return Err(TypePackageError::UnsupportedApi {
            found: u32::try_from(api_version).unwrap_or(0),
            supported: API_VERSION,
        });
    }
    let id = required_string(&root, "id")?.to_string();
    let name = required_string(&root, "name")?.to_string();
    let description = optional_string(&root, "description").unwrap_or_default();
    let capabilities = match root.as_mapping_get("capabilities") {
        Some(value) => parse_capabilities(value)?,
        None => TypeCapabilities::default(),
    };

    let directories = match root.as_mapping_get("directories") {
        Some(value) => string_sequence(value, "directories")?,
        None => Vec::new(),
    };
    let skills = match root.as_mapping_get("skills") {
        Some(value) => string_sequence(value, "skills")?,
        None => Vec::new(),
    };
    let files = match root.as_mapping_get("files") {
        Some(value) => {
            let files = value
                .as_sequence()
                .ok_or_else(|| TypePackageError::Invalid {
                    field: "files",
                    detail: "must be a list".to_string(),
                })?;
            let mut out = Vec::with_capacity(files.len());
            for file in files {
                let file = file.as_mapping().ok_or_else(|| TypePackageError::Invalid {
                    field: "files",
                    detail: "each entry must be a mapping".to_string(),
                })?;
                for key in file.keys() {
                    let key = key.as_str().ok_or_else(|| TypePackageError::Invalid {
                        field: "files",
                        detail: "file keys must be strings".to_string(),
                    })?;
                    if key != "path" && key != "content" {
                        return Err(TypePackageError::Invalid {
                            field: "files",
                            detail: format!("unknown file key `{key}`"),
                        });
                    }
                }
                let path = mapping_get(file, "path")
                    .and_then(Yaml::as_str)
                    .ok_or_else(|| TypePackageError::Invalid {
                        field: "files",
                        detail: "each file needs a string `path`".to_string(),
                    })?
                    .to_string();
                let content =
                    mapping_get(file, "content").ok_or_else(|| TypePackageError::Invalid {
                        field: "files",
                        detail: format!("`{path}` needs `content`"),
                    })?;
                out.push(TemplateFile {
                    path,
                    content: parse_localized(content)?,
                });
            }
            out
        }
        None => Vec::new(),
    };

    let package = ProjectTypePackage {
        schema_version: u32::try_from(schema_version).map_err(|_| TypePackageError::Invalid {
            field: "schema_version",
            detail: "must be a positive 32-bit integer".to_string(),
        })?,
        api_version: u32::try_from(api_version).map_err(|_| TypePackageError::Invalid {
            field: "api_version",
            detail: "must be a positive 32-bit integer".to_string(),
        })?,
        id,
        name,
        description,
        directories,
        files,
        skills,
        capabilities,
        origin,
        source_dir: None,
        content_hash: String::new(),
    };
    package.validate()?;
    Ok(package)
}

fn parse_capabilities(value: &Yaml) -> Result<TypeCapabilities, TypePackageError> {
    let mapping = value
        .as_mapping()
        .ok_or_else(|| TypePackageError::Invalid {
            field: "capabilities",
            detail: "must be a mapping".to_string(),
        })?;
    for key in mapping.keys() {
        let key = key.as_str().ok_or_else(|| TypePackageError::Invalid {
            field: "capabilities",
            detail: "keys must be strings".to_string(),
        })?;
        if !matches!(key, "screenplay" | "entity_kinds" | "export_pdf") {
            return Err(TypePackageError::Invalid {
                field: "capabilities",
                detail: format!("unknown key `{key}`"),
            });
        }
    }
    let screenplay = match value.as_mapping_get("screenplay") {
        Some(node) => node.as_bool().ok_or_else(|| TypePackageError::Invalid {
            field: "capabilities.screenplay",
            detail: "must be a boolean".to_string(),
        })?,
        None => false,
    };
    let export_pdf = match value.as_mapping_get("export_pdf") {
        Some(node) => node.as_bool().ok_or_else(|| TypePackageError::Invalid {
            field: "capabilities.export_pdf",
            detail: "must be a boolean".to_string(),
        })?,
        None => false,
    };
    let entity_kinds = match value.as_mapping_get("entity_kinds") {
        Some(node) => string_sequence(node, "capabilities.entity_kinds")?,
        None => Vec::new(),
    };
    Ok(TypeCapabilities {
        screenplay,
        entity_kinds,
        export_pdf,
    })
}

fn mapping_get<'a>(mapping: &'a saphyr::Mapping<'a>, key: &str) -> Option<&'a Yaml<'a>> {
    mapping
        .iter()
        .find(|(candidate, _)| candidate.as_str() == Some(key))
        .map(|(_, value)| value)
}

fn parse_localized(value: &Yaml) -> Result<LocalizedText, TypePackageError> {
    if let Some(text) = value.as_str() {
        return Ok(LocalizedText {
            en: text.to_string(),
            es: text.to_string(),
        });
    }
    let mapping = value
        .as_mapping()
        .ok_or_else(|| TypePackageError::Invalid {
            field: "files",
            detail: "content must be a string or an {en, es} mapping".to_string(),
        })?;
    for key in mapping.keys() {
        let key = key.as_str().ok_or_else(|| TypePackageError::Invalid {
            field: "files",
            detail: "content keys must be strings".to_string(),
        })?;
        if key != "en" && key != "es" {
            return Err(TypePackageError::Invalid {
                field: "files",
                detail: format!("unknown content language `{key}`"),
            });
        }
    }
    let en = value
        .as_mapping_get("en")
        .and_then(Yaml::as_str)
        .ok_or_else(|| TypePackageError::Invalid {
            field: "files",
            detail: "content needs an `en` string".to_string(),
        })?
        .to_string();
    let es = value
        .as_mapping_get("es")
        .and_then(Yaml::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| en.clone());
    Ok(LocalizedText { en, es })
}

fn validate_id(id: &str) -> Result<(), TypePackageError> {
    if id.is_empty() || id.chars().count() > MAX_ID_CHARS {
        return Err(TypePackageError::Invalid {
            field: "id",
            detail: format!("must be 1–{MAX_ID_CHARS} characters"),
        });
    }
    let mut last_separator = true;
    for ch in id.chars() {
        let separator = matches!(ch, '-' | '_');
        if !(ch.is_ascii_lowercase() || ch.is_ascii_digit() || separator) {
            return Err(TypePackageError::Invalid {
                field: "id",
                detail: "use lowercase letters, digits, '-' and '_' only".to_string(),
            });
        }
        if separator && last_separator {
            return Err(TypePackageError::Invalid {
                field: "id",
                detail: "separators cannot repeat or lead".to_string(),
            });
        }
        last_separator = separator;
    }
    if last_separator {
        return Err(TypePackageError::Invalid {
            field: "id",
            detail: "cannot end with a separator".to_string(),
        });
    }
    Ok(())
}

fn validate_relative_path(field: &'static str, path: &str) -> Result<(), TypePackageError> {
    if path.is_empty() || path.chars().count() > MAX_PATH_CHARS {
        return Err(TypePackageError::Invalid {
            field,
            detail: format!("`{path}` must be 1–{MAX_PATH_CHARS} characters"),
        });
    }
    if path.starts_with('/') || path.contains('\\') || path.ends_with('/') {
        return Err(TypePackageError::Invalid {
            field,
            detail: format!("`{path}` must be a relative forward-slash path"),
        });
    }
    let mut names = Vec::new();
    for component in Path::new(path).components() {
        let Component::Normal(name) = component else {
            return Err(TypePackageError::Invalid {
                field,
                detail: format!("`{path}` cannot contain `.` or `..`"),
            });
        };
        let name = name.to_str().ok_or_else(|| TypePackageError::Invalid {
            field,
            detail: format!("`{path}` must be UTF-8"),
        })?;
        if name.is_empty() || name.starts_with('.') {
            return Err(TypePackageError::Invalid {
                field,
                detail: format!("`{path}` cannot contain hidden names"),
            });
        }
        if matches!(
            name,
            "project.yaml" | "AGENTS.md" | "agents.yaml" | ".gitignore"
        ) {
            return Err(TypePackageError::Invalid {
                field,
                detail: format!("`{path}` is reserved"),
            });
        }
        names.push(name);
    }
    if names.is_empty() {
        return Err(TypePackageError::Invalid {
            field,
            detail: format!("`{path}` must name a file or directory"),
        });
    }
    Ok(())
}

fn required_integer(root: &Yaml, field: &'static str) -> Result<i64, TypePackageError> {
    root.as_mapping_get(field)
        .and_then(Yaml::as_integer)
        .ok_or_else(|| TypePackageError::Invalid {
            field,
            detail: "required integer".to_string(),
        })
}

fn required_string<'a>(root: &'a Yaml, field: &'static str) -> Result<&'a str, TypePackageError> {
    root.as_mapping_get(field)
        .and_then(Yaml::as_str)
        .ok_or_else(|| TypePackageError::Invalid {
            field,
            detail: "required string".to_string(),
        })
}

fn optional_string(root: &Yaml, field: &'static str) -> Option<String> {
    root.as_mapping_get(field)
        .and_then(Yaml::as_str)
        .map(str::to_string)
}

fn string_sequence(value: &Yaml, field: &'static str) -> Result<Vec<String>, TypePackageError> {
    let sequence = value
        .as_sequence()
        .ok_or_else(|| TypePackageError::Invalid {
            field,
            detail: "must be a list of strings".to_string(),
        })?;
    let mut out = Vec::with_capacity(sequence.len());
    for item in sequence {
        out.push(
            item.as_str()
                .ok_or_else(|| TypePackageError::Invalid {
                    field,
                    detail: "must be a list of strings".to_string(),
                })?
                .to_string(),
        );
    }
    Ok(out)
}

fn builtin_template() -> (Vec<String>, Vec<TemplateFile>) {
    let directories = [
        "bible/characters",
        "bible/locations",
        "bible/objects",
        "structure",
        "screenplay",
        "research",
        "notes",
        "reviews",
    ]
    .iter()
    .map(|directory| (*directory).to_string())
    .collect();
    let (premise_en, outline_en) = basic_template_documents("en");
    let (premise_es, outline_es) = basic_template_documents("es");
    let decisions =
        "# Decisions\n\nRecord accepted decisions here. Each entry: date, decision, consequence.\n";
    let questions = "# Open questions\n\nRecord unresolved questions here.\n";
    let files = vec![
        TemplateFile {
            path: "screenplay/main.fountain".to_string(),
            content: LocalizedText {
                en: String::new(),
                es: String::new(),
            },
        },
        TemplateFile {
            path: "notes/decisions.md".to_string(),
            content: LocalizedText {
                en: decisions.to_string(),
                es: decisions.to_string(),
            },
        },
        TemplateFile {
            path: "notes/open-questions.md".to_string(),
            content: LocalizedText {
                en: questions.to_string(),
                es: questions.to_string(),
            },
        },
        TemplateFile {
            path: "bible/premise.md".to_string(),
            content: LocalizedText {
                en: premise_en.to_string(),
                es: premise_es.to_string(),
            },
        },
        TemplateFile {
            path: "structure/outline.md".to_string(),
            content: LocalizedText {
                en: outline_en.to_string(),
                es: outline_es.to_string(),
            },
        },
    ];
    (directories, files)
}

/// Localized starter documents. Kept here so project creation and package
/// templates share the exact same text.
pub fn basic_template_documents(primary_language: &str) -> (&'static str, &'static str) {
    if primary_language == "es" {
        (
            "# Premisa\n\nEscribe aquí la premisa de la historia: quién, qué quiere y qué se lo impide.\n",
            "# Escaleta\n\n## Acto I\n\n- Presentación del mundo y del protagonista.\n\n## Acto II\n\n- Conflicto y punto medio.\n\n## Acto III\n\n- Resolución.\n",
        )
    } else {
        (
            "# Premise\n\nWrite the story premise here: who wants what, and what stands in the way.\n",
            "# Outline\n\n## Act I\n\n- Introduce the world and the protagonist.\n\n## Act II\n\n- Conflict and midpoint.\n\n## Act III\n\n- Resolution.\n",
        )
    }
}

/// Write a package's directories and files into a new project root. Files are
/// create-only; an existing file is an error and is never overwritten.
pub fn write_template(
    root: &Path,
    package: &ProjectTypePackage,
    language: &str,
) -> Result<(), TypePackageError> {
    package.validate()?;
    for directory in &package.directories {
        let path = root.join(directory);
        fs::create_dir_all(&path).map_err(|error| TypePackageError::Io {
            path,
            error: error.to_string(),
        })?;
    }
    for file in &package.files {
        crate::document::create_document(root, &file.path, file.content.for_language(language))
            .map_err(|error| TypePackageError::Invalid {
                field: "files",
                detail: format!("could not write `{}`: {error}", file.path),
            })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        tempfile::tempdir().expect("tempdir").keep()
    }

    #[test]
    fn builtin_ids_are_stable_and_complete() {
        assert_eq!(builtins().len(), 6);
        for id in BUILTIN_IDS {
            let package = builtin(id).expect("builtin");
            assert_eq!(package.id, id);
            package.validate().expect("valid");
            let profile = package.profile();
            assert!(profile.screenplay);
            assert!(profile.allows_entity("character"));
            assert!(profile.export_pdf);
        }
        assert!(builtin("unknown").is_none());
    }

    #[test]
    fn capabilities_parse_and_default_to_generic() {
        let generic = parse_package(
            "schema_version: 1\napi_version: 1\nid: documentary\nname: Documentary\n",
            TypeOrigin::User,
        )
        .expect("generic package");
        assert!(!generic.profile().screenplay);
        assert!(generic.profile().entity_kinds.is_empty());
        assert!(!generic.profile().export_pdf);

        let typed = parse_package(
            "schema_version: 1\napi_version: 1\nid: campaign\nname: Campaign\ncapabilities:\n  screenplay: true\n  entity_kinds: [audience, channel]\n  export_pdf: true\n",
            TypeOrigin::User,
        )
        .expect("typed package");
        assert!(typed.profile().screenplay);
        assert_eq!(typed.profile().entity_kinds, ["audience", "channel"]);
        assert!(typed.profile().export_pdf);
    }

    #[test]
    fn builtin_templates_write_the_expected_files() {
        let root = temp_root();
        let package = builtin("feature").expect("feature");
        write_template(&root, &package, "es").expect("write");
        let premise = fs::read_to_string(root.join("bible/premise.md")).expect("premise");
        assert!(premise.starts_with("# Premisa"));
        assert!(root.join("structure/outline.md").is_file());
        assert!(root.join("bible/characters").is_dir());
        assert_eq!(
            fs::read_to_string(root.join("screenplay/main.fountain")).expect("screenplay"),
            ""
        );
    }

    #[test]
    fn rejects_invalid_paths_ids_and_duplicates() {
        let mut package = builtin("feature").expect("feature");
        package.id = "Feature".to_string();
        assert!(package.validate().is_err());

        let mut package = builtin("feature").expect("feature");
        package.directories.push("../outside".to_string());
        assert!(package.validate().is_err());

        let mut package = builtin("feature").expect("feature");
        package.files.push(package.files[0].clone());
        assert!(package.validate().is_err());

        let mut package = builtin("feature").expect("feature");
        package.files[0].path = ".narra/agents/x/agent.yaml".to_string();
        assert!(package.validate().is_err());
    }

    #[test]
    fn project_type_trust_is_hash_bound() {
        let root = temp_root();
        let dir = root.join(PROJECT_TYPES_DIR).join("campaign");
        fs::create_dir_all(&dir).expect("dir");
        fs::write(
            dir.join(TYPE_FILE),
            "schema_version: 1\napi_version: 1\nid: campaign\nname: Campaign\n",
        )
        .expect("type");
        let mut package = load_package(&dir, TypeOrigin::Project).expect("package");
        assert!(untrusted_project_type(&root, &package).expect("untrusted"));
        trust_type(&root, &package.id, &package.content_hash).expect("trust");
        assert!(!untrusted_project_type(&root, &package).expect("trusted"));
        fs::write(
            dir.join(TYPE_FILE),
            "schema_version: 1\napi_version: 1\nid: campaign\nname: Campaign v2\n",
        )
        .expect("change");
        package = load_package(&dir, TypeOrigin::Project).expect("reload");
        assert!(untrusted_project_type(&root, &package).expect("changed"));
    }

    #[test]
    fn loads_user_and_project_scopes_and_reserves_builtin_ids() {
        let config = temp_root();
        let project = temp_root();
        let user = config.join(USER_TYPES_DIR).join("documentary");
        fs::create_dir_all(&user).expect("user dir");
        fs::write(
            user.join(TYPE_FILE),
            "schema_version: 1\napi_version: 1\nid: documentary\nname: Documentary\ndescription: Doc\ndirectories: [docs]\nfiles:\n  - path: docs/brief.md\n    content:\n      en: Brief\n      es: Guion\nskills: []\n",
        )
        .expect("user package");
        let loaded = registry(Some(&project), Some(&config)).expect("registry");
        assert!(loaded.iter().any(|package| package.id == "documentary"));
        assert!(loaded.iter().any(|package| package.id == "feature"));

        let project_package = project.join(PROJECT_TYPES_DIR).join("documentary");
        fs::create_dir_all(&project_package).expect("project dir");
        fs::write(
            project_package.join(TYPE_FILE),
            "schema_version: 1\napi_version: 1\nid: documentary\nname: Project Documentary\ndirectories: []\nfiles: []\n",
        )
        .expect("project package");
        let loaded = registry(Some(&project), Some(&config)).expect("registry");
        let documentary = loaded
            .iter()
            .find(|package| package.id == "documentary")
            .expect("documentary");
        assert_eq!(documentary.origin, TypeOrigin::Project);
        assert_eq!(documentary.name, "Project Documentary");

        let reserved = config.join(USER_TYPES_DIR).join("feature");
        fs::create_dir_all(&reserved).expect("reserved dir");
        fs::write(
            reserved.join(TYPE_FILE),
            "schema_version: 1\napi_version: 1\nid: feature\nname: Fake\n",
        )
        .expect("reserved package");
        assert!(matches!(
            registry(Some(&project), Some(&config)),
            Err(TypePackageError::ReservedId(id)) if id == "feature"
        ));
    }
}
