//! Versioned `SKILL.md` skills (M17 slice 3, D-045).
//!
//! Skills are directories with one `SKILL.md`: YAML frontmatter (`name`,
//! `description`, optional metadata) plus a Markdown instruction body. The
//! directory name is the skill id. User skills live under
//! `<config>/skills/<id>/`; project skills under `<root>/.narra/skills/<id>/`.
//! Project skills are untrusted content and need an explicit confirmation
//! bound to the file hash before they are activated.
//!
//! Skills are instructions only. They cannot execute code or grant tools; the
//! chat surface may attach a skill body as context after it is trusted.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use saphyr::{LoadableYamlNode, Yaml};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::proposal::sha256_hex;

/// File inside each skill directory.
pub const SKILL_FILE: &str = "SKILL.md";
/// Project-local skills, relative to the project root.
pub const PROJECT_SKILLS_DIR: &str = ".narra/skills";
/// User-scope skills, relative to the user config directory.
pub const USER_SKILLS_DIR: &str = "skills";
/// Trust record for project skills.
pub const TRUST_FILE: &str = ".narra/trusted-skills.json";
const TRUST_SCHEMA_VERSION: u32 = 1;
const MAX_SKILL_BYTES: usize = 128 * 1024;
const MAX_BODY_BYTES: usize = 64 * 1024;
const MAX_DESCRIPTION_CHARS: usize = 1024;
const MAX_NAME_CHARS: usize = 64;

/// Where a skill was loaded from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillOrigin {
    User,
    Project,
}

impl SkillOrigin {
    pub fn key(self) -> &'static str {
        match self {
            SkillOrigin::User => "user",
            SkillOrigin::Project => "project",
        }
    }
}

/// One validated skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub body: String,
    pub origin: SkillOrigin,
    pub hash: String,
}

#[derive(Debug, Error)]
pub enum SkillError {
    #[error("could not read skill `{path}`: {error}")]
    Io { path: PathBuf, error: String },
    #[error("invalid skill `{path}`: {error}")]
    Invalid { path: PathBuf, error: String },
    #[error("skill is too large: {path}")]
    TooLarge { path: PathBuf },
}

/// Load user and project skills. Project skills override user skills with the
/// same id. Missing scope directories are empty.
pub fn load_skills(
    project_root: &Path,
    config_dir: Option<&Path>,
) -> Result<Vec<Skill>, SkillError> {
    let mut by_id = BTreeMap::new();
    if let Some(config_dir) = config_dir {
        for skill in load_scope(&config_dir.join(USER_SKILLS_DIR), SkillOrigin::User)? {
            by_id.insert(skill.id.clone(), skill);
        }
    }
    for skill in load_scope(&project_root.join(PROJECT_SKILLS_DIR), SkillOrigin::Project)? {
        by_id.insert(skill.id.clone(), skill);
    }
    Ok(by_id.into_values().collect())
}

/// Load one scope directory. A missing directory is empty.
pub fn load_scope(directory: &Path, origin: SkillOrigin) -> Result<Vec<Skill>, SkillError> {
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let entries = fs::read_dir(directory).map_err(|error| SkillError::Io {
        path: directory.to_path_buf(),
        error: error.to_string(),
    })?;
    let mut skills = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| SkillError::Io {
            path: directory.to_path_buf(),
            error: error.to_string(),
        })?;
        if entry
            .file_type()
            .map_err(|error| SkillError::Io {
                path: entry.path(),
                error: error.to_string(),
            })?
            .is_dir()
        {
            skills.push(load_skill(&entry.path(), origin)?);
        }
    }
    skills.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(skills)
}

/// Load and validate one skill directory. The directory name is the id.
pub fn load_skill(directory: &Path, origin: SkillOrigin) -> Result<Skill, SkillError> {
    let path = directory.join(SKILL_FILE);
    let text = fs::read_to_string(&path).map_err(|error| SkillError::Io {
        path: path.clone(),
        error: error.to_string(),
    })?;
    if text.len() > MAX_SKILL_BYTES {
        return Err(SkillError::TooLarge { path });
    }
    let id = directory
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| SkillError::Invalid {
            path: path.clone(),
            error: "directory name must be UTF-8".to_string(),
        })?
        .to_string();
    validate_id(&id, &path)?;
    let (frontmatter, body) = split_frontmatter(&text, &path)?;
    let (name, description) = parse_frontmatter(&frontmatter, &path)?;
    if body.trim().is_empty() {
        return Err(SkillError::Invalid {
            path: path.clone(),
            error: "the instruction body must not be empty".to_string(),
        });
    }
    if body.len() > MAX_BODY_BYTES {
        return Err(SkillError::TooLarge { path });
    }
    Ok(Skill {
        id,
        name,
        description,
        body: body.trim().to_string(),
        origin,
        hash: sha256_hex(&text),
    })
}

/// Hashes explicitly trusted by this project, keyed by skill id.
pub fn trusted_hashes(project_root: &Path) -> Result<BTreeMap<String, String>, String> {
    let path = project_root.join(TRUST_FILE);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => return Err(format!("{}: {error}", path.display())),
    };
    let value: TrustFile =
        serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    if value.schema_version != TRUST_SCHEMA_VERSION {
        return Err(format!(
            "unsupported trust schema_version {}",
            value.schema_version
        ));
    }
    Ok(value.skills)
}

/// Trust one project skill at its current content hash.
pub fn trust_skill(project_root: &Path, id: &str, hash: &str) -> Result<(), String> {
    let mut skills = trusted_hashes(project_root)?;
    skills.insert(id.to_string(), hash.to_string());
    let file = TrustFile {
        schema_version: TRUST_SCHEMA_VERSION,
        skills,
    };
    let text = serde_json::to_string_pretty(&file)
        .map_err(|error| format!("could not encode skill trust: {error}"))?;
    let root = project_root.join(".narra");
    fs::create_dir_all(&root).map_err(|error| format!("{}: {error}", root.display()))?;
    crate::document::write_document(project_root, TRUST_FILE, &text)
        .map_err(|error| error.to_string())
}

/// Project skills whose current hash is not trusted.
pub fn untrusted_project_skills(
    project_root: &Path,
    skills: &[Skill],
) -> Result<Vec<Skill>, String> {
    let trusted = trusted_hashes(project_root)?;
    Ok(skills
        .iter()
        .filter(|skill| {
            skill.origin == SkillOrigin::Project && trusted.get(&skill.id) != Some(&skill.hash)
        })
        .cloned()
        .collect())
}

/// Trusted skills for activation: user skills plus project skills whose hash
/// matches the trust record.
pub fn active_skills(project_root: &Path, skills: &[Skill]) -> Result<Vec<Skill>, String> {
    let trusted = trusted_hashes(project_root)?;
    Ok(skills
        .iter()
        .filter(|skill| {
            skill.origin == SkillOrigin::User || trusted.get(&skill.id) == Some(&skill.hash)
        })
        .cloned()
        .collect())
}

#[derive(Debug, Serialize, Deserialize)]
struct TrustFile {
    schema_version: u32,
    #[serde(default)]
    skills: BTreeMap<String, String>,
}

fn split_frontmatter(text: &str, path: &Path) -> Result<(String, String), SkillError> {
    let normalized = text.replace("\r\n", "\n");
    let rest = normalized
        .strip_prefix("---\n")
        .ok_or_else(|| SkillError::Invalid {
            path: path.to_path_buf(),
            error: "must start with YAML frontmatter (`---`)".to_string(),
        })?;
    let end = rest
        .find("\n---\n")
        .or_else(|| rest.strip_suffix("\n---").map(|body| body.len()))
        .ok_or_else(|| SkillError::Invalid {
            path: path.to_path_buf(),
            error: "frontmatter must end with a `---` line".to_string(),
        })?;
    let frontmatter = rest[..end].to_string();
    let body_start = if rest[end..].starts_with("\n---\n") {
        end + 5
    } else {
        rest.len()
    };
    Ok((frontmatter, rest[body_start..].to_string()))
}

fn parse_frontmatter(frontmatter: &str, path: &Path) -> Result<(String, String), SkillError> {
    let documents = Yaml::load_from_str(frontmatter).map_err(|error| SkillError::Invalid {
        path: path.to_path_buf(),
        error: error.to_string(),
    })?;
    if documents.len() != 1 {
        return Err(SkillError::Invalid {
            path: path.to_path_buf(),
            error: "frontmatter must be one mapping".to_string(),
        });
    }
    let root = &documents[0];
    let mapping = root.as_mapping().ok_or_else(|| SkillError::Invalid {
        path: path.to_path_buf(),
        error: "frontmatter must be a mapping".to_string(),
    })?;
    for key in mapping.keys() {
        let key = key.as_str().ok_or_else(|| SkillError::Invalid {
            path: path.to_path_buf(),
            error: "frontmatter keys must be strings".to_string(),
        })?;
        if !matches!(
            key,
            "name" | "description" | "license" | "compatibility" | "metadata" | "allowed-tools"
        ) {
            return Err(SkillError::Invalid {
                path: path.to_path_buf(),
                error: format!("unknown frontmatter key `{key}`"),
            });
        }
    }
    let name = root
        .as_mapping_get("name")
        .and_then(Yaml::as_str)
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| SkillError::Invalid {
            path: path.to_path_buf(),
            error: "frontmatter needs a non-empty `name`".to_string(),
        })?;
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(SkillError::Invalid {
            path: path.to_path_buf(),
            error: format!("`name` must be at most {MAX_NAME_CHARS} characters"),
        });
    }
    let description = root
        .as_mapping_get("description")
        .and_then(Yaml::as_str)
        .filter(|description| !description.trim().is_empty())
        .ok_or_else(|| SkillError::Invalid {
            path: path.to_path_buf(),
            error: "frontmatter needs a non-empty `description`".to_string(),
        })?;
    if description.chars().count() > MAX_DESCRIPTION_CHARS {
        return Err(SkillError::Invalid {
            path: path.to_path_buf(),
            error: format!("`description` must be at most {MAX_DESCRIPTION_CHARS} characters"),
        });
    }
    Ok((name.to_string(), description.to_string()))
}

fn validate_id(id: &str, path: &Path) -> Result<(), SkillError> {
    let invalid = |reason: &str| SkillError::Invalid {
        path: path.to_path_buf(),
        error: format!("invalid skill id `{id}`: {reason}"),
    };
    if id.is_empty() || id.chars().count() > 64 {
        return Err(invalid("must be 1–64 characters"));
    }
    let mut last_separator = true;
    for ch in id.chars() {
        let separator = ch == '-';
        if !(ch.is_ascii_lowercase() || ch.is_ascii_digit() || separator) {
            return Err(invalid("use lowercase letters, digits and '-' only"));
        }
        if separator && last_separator {
            return Err(invalid("'-' cannot lead or repeat"));
        }
        last_separator = separator;
    }
    if last_separator {
        return Err(invalid("cannot end with '-'"));
    }
    // Keep the path component rule explicit even though the directory name
    // already came from the filesystem.
    if Path::new(id)
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(invalid("must be one path component"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        tempfile::tempdir().expect("tempdir").keep()
    }

    fn write_skill(root: &Path, id: &str, name: &str, body: &str) -> PathBuf {
        let dir = root.join(id);
        fs::create_dir_all(&dir).expect("skill dir");
        fs::write(
            dir.join(SKILL_FILE),
            format!("---\nname: {name}\ndescription: Use when testing.\n---\n{body}"),
        )
        .expect("skill file");
        dir
    }

    #[test]
    fn loads_user_and_project_skills_with_project_precedence() {
        let config = temp_root();
        let project = temp_root();
        write_skill(&config.join(USER_SKILLS_DIR), "brief", "Brief", "User body");
        write_skill(
            &project.join(PROJECT_SKILLS_DIR),
            "brief",
            "Brief project",
            "Project body",
        );
        let skills = load_skills(&project, Some(&config)).expect("skills");
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].origin, SkillOrigin::Project);
        assert_eq!(skills[0].body, "Project body");
    }

    #[test]
    fn rejects_invalid_frontmatter_and_ids() {
        let root = temp_root();
        let dir = root.join("Bad ID");
        fs::create_dir_all(&dir).expect("dir");
        fs::write(dir.join(SKILL_FILE), "no frontmatter").expect("file");
        assert!(load_skill(&dir, SkillOrigin::Project).is_err());

        let dir = write_skill(&root, "valid", "Valid", "Body");
        let bad = dir.join(SKILL_FILE);
        fs::write(&bad, "---\nname: Valid\n---\nBody").expect("bad frontmatter");
        assert!(load_skill(&dir, SkillOrigin::Project).is_err());
    }

    #[test]
    fn project_skill_trust_is_hash_bound() {
        let project = temp_root();
        let dir = write_skill(&project.join(PROJECT_SKILLS_DIR), "brief", "Brief", "Body");
        let skill = load_skill(&dir, SkillOrigin::Project).expect("skill");
        let skills = vec![skill.clone()];
        assert_eq!(
            untrusted_project_skills(&project, &skills)
                .expect("untrusted")
                .len(),
            1
        );
        trust_skill(&project, &skill.id, &skill.hash).expect("trust");
        assert!(
            untrusted_project_skills(&project, &skills)
                .expect("trusted")
                .is_empty()
        );
        let mut changed = skill;
        changed.hash = "changed".to_string();
        assert_eq!(
            untrusted_project_skills(&project, &[changed])
                .expect("changed")
                .len(),
            1
        );
    }
}
