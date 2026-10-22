//! Versioned `project.yaml` manifest: model, strict parser and serializer.
//!
//! Rules enforced here (Milestone 2, Spike G-1):
//! - `schema_version` is authoritative; unknown versions fail with an explicit
//!   error and the file is never silently rewritten;
//! - unknown fields, wrong types and invalid values fail loudly (no guessing);
//! - no credentials and no machine-specific absolute paths ever enter a
//!   manifest (D-004, `DATA_MODEL.md` §3).
//!
//! The manifest is intentionally parsed without a serde layer: it is a small,
//! controlled document and hand parsing gives exact control over rejection
//! semantics and error messages.

use saphyr::{LoadableYamlNode, Yaml};
use thiserror::Error;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

/// The only schema version this build writes or accepts.
pub const MANIFEST_SCHEMA_VERSION: u32 = 1;

/// Fields a v1 manifest may contain. Anything else is rejected.
const V1_FIELDS: &[&str] = &[
    "schema_version",
    "project_id",
    "title",
    "project_type",
    "primary_language",
    "additional_languages",
    "created_at",
    "default_screenplay_path",
];

/// Project kinds from `DATA_MODEL.md` §3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectType {
    Feature,
    Series,
    WebSeries,
    Short,
    Microdrama,
    Other,
}

impl ProjectType {
    pub fn as_str(self) -> &'static str {
        match self {
            ProjectType::Feature => "feature",
            ProjectType::Series => "series",
            ProjectType::WebSeries => "web_series",
            ProjectType::Short => "short",
            ProjectType::Microdrama => "microdrama",
            ProjectType::Other => "other",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "feature" => ProjectType::Feature,
            "series" => ProjectType::Series,
            "web_series" => ProjectType::WebSeries,
            "short" => ProjectType::Short,
            "microdrama" => ProjectType::Microdrama,
            "other" => ProjectType::Other,
            _ => return None,
        })
    }
}

/// An in-memory `project.yaml` (v1).
#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    pub schema_version: u32,
    pub project_id: Uuid,
    pub title: String,
    pub project_type: ProjectType,
    pub primary_language: String,
    pub additional_languages: Vec<String>,
    /// RFC 3339, UTC. Set once at creation and never rewritten afterwards.
    pub created_at: String,
    /// Project-relative path, e.g. `screenplay/main.fountain`.
    pub default_screenplay_path: String,
}

impl Manifest {
    /// Build a new v1 manifest with a fresh id and creation timestamp.
    pub fn new_v1(
        title: &str,
        project_type: ProjectType,
        primary_language: &str,
        default_screenplay_path: &str,
    ) -> Self {
        let created_at = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .expect("RFC 3339 formatting cannot fail for UTC timestamps");
        Self {
            schema_version: MANIFEST_SCHEMA_VERSION,
            project_id: Uuid::new_v4(),
            title: title.to_string(),
            project_type,
            primary_language: primary_language.to_string(),
            additional_languages: Vec::new(),
            created_at,
            default_screenplay_path: default_screenplay_path.to_string(),
        }
    }

    fn validate(&self) -> Result<(), ManifestError> {
        if self.schema_version != MANIFEST_SCHEMA_VERSION {
            return Err(ManifestError::UnsupportedSchemaVersion {
                found: i64::from(self.schema_version),
                supported: MANIFEST_SCHEMA_VERSION,
            });
        }
        require_non_empty("title", &self.title)?;
        require_language("primary_language", &self.primary_language)?;
        for language in &self.additional_languages {
            require_language("additional_languages", language)?;
        }
        require_relative_path("default_screenplay_path", &self.default_screenplay_path)?;
        parse_rfc3339("created_at", &self.created_at)?;
        Ok(())
    }
}

/// Errors surfaced when reading or validating a manifest.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ManifestError {
    /// The document is not valid YAML or not a single YAML mapping.
    #[error("project.yaml is not a valid YAML mapping: {0}")]
    Parse(String),

    /// A required field is missing or has the wrong type.
    #[error("project.yaml is missing required field `{field}` (expected {expected})")]
    MissingField {
        field: &'static str,
        expected: &'static str,
    },

    /// The file contains a field this schema version does not define.
    #[error("project.yaml contains unknown field `{field}` for schema v{version}")]
    UnknownField { field: String, version: u32 },

    /// `schema_version` is present but not supported by this build.
    /// The file is left untouched: migrations are explicit, never silent.
    #[error(
        "unsupported project.yaml schema_version {found}; this build supports {supported} \
         (the file was not modified)"
    )]
    UnsupportedSchemaVersion { found: i64, supported: u32 },

    /// The field is present but its value violates a schema constraint.
    #[error("project.yaml field `{field}` is invalid: {detail}")]
    InvalidValue { field: &'static str, detail: String },
}

/// Parse and validate a `project.yaml` document.
pub fn parse_manifest(text: &str) -> Result<Manifest, ManifestError> {
    let documents =
        Yaml::load_from_str(text).map_err(|error| ManifestError::Parse(error.to_string()))?;
    if documents.len() != 1 {
        return Err(ManifestError::Parse(format!(
            "expected exactly one YAML document, found {}",
            documents.len()
        )));
    }
    let root = documents.into_iter().next().expect("length checked above");
    if root.as_mapping().is_none() {
        return Err(ManifestError::Parse(
            "top level must be a mapping".to_string(),
        ));
    }

    // schema_version first: an unknown version is rejected before the field
    // list of that version is applied.
    let schema_version = match root.as_mapping_get("schema_version") {
        Some(value) => value.as_integer().ok_or(ManifestError::MissingField {
            field: "schema_version",
            expected: "integer",
        })?,
        None => {
            return Err(ManifestError::MissingField {
                field: "schema_version",
                expected: "integer",
            });
        }
    };
    if schema_version != i64::from(MANIFEST_SCHEMA_VERSION) {
        return Err(ManifestError::UnsupportedSchemaVersion {
            found: schema_version,
            supported: MANIFEST_SCHEMA_VERSION,
        });
    }

    // Reject unknown fields so typo reports do not mask missing fields.
    let mapping = root.as_mapping().expect("checked above");
    for key in mapping.keys() {
        let key = key
            .as_str()
            .ok_or_else(|| ManifestError::Parse("mapping keys must be strings".to_string()))?;
        if !V1_FIELDS.contains(&key) {
            return Err(ManifestError::UnknownField {
                field: key.to_string(),
                version: MANIFEST_SCHEMA_VERSION,
            });
        }
    }

    let project_id = required_string(&root, "project_id", "UUID string")?
        .parse::<Uuid>()
        .map_err(|_| ManifestError::InvalidValue {
            field: "project_id",
            detail: "not a valid UUID".to_string(),
        })?;

    let title = required_string(&root, "title", "string")?.to_string();
    let project_type_raw = required_string(&root, "project_type", "string")?;
    let project_type =
        ProjectType::parse(project_type_raw).ok_or_else(|| ManifestError::InvalidValue {
            field: "project_type",
            detail: format!(
                "unknown project type `{project_type_raw}` (expected one of: feature, series, \
                 web_series, short, microdrama, other)"
            ),
        })?;

    let primary_language = required_string(&root, "primary_language", "string")?.to_string();
    let additional_languages = match root.as_mapping_get("additional_languages") {
        None => Vec::new(),
        Some(value) => {
            let sequence = value
                .as_sequence()
                .ok_or_else(|| ManifestError::InvalidValue {
                    field: "additional_languages",
                    detail: "must be a list of strings".to_string(),
                })?;
            let mut languages = Vec::with_capacity(sequence.len());
            for item in sequence {
                let language = item.as_str().ok_or_else(|| ManifestError::InvalidValue {
                    field: "additional_languages",
                    detail: "must be a list of strings".to_string(),
                })?;
                languages.push(language.to_string());
            }
            languages
        }
    };

    let created_at = required_string(&root, "created_at", "RFC 3339 timestamp string")?.to_string();
    let default_screenplay_path =
        required_string(&root, "default_screenplay_path", "relative path string")?.to_string();

    let manifest = Manifest {
        schema_version: u32::try_from(schema_version).map_err(|_| ManifestError::InvalidValue {
            field: "schema_version",
            detail: format!("must be a positive 32-bit integer, found {schema_version}"),
        })?,
        project_id,
        title,
        project_type,
        primary_language,
        additional_languages,
        created_at,
        default_screenplay_path,
    };
    manifest.validate()?;
    Ok(manifest)
}

/// Serialize a manifest to stable, deterministic `project.yaml` text.
///
/// Key order is fixed and all strings are emitted as escaped double-quoted
/// scalars, so round trips are byte-stable for the same manifest.
pub fn serialize_manifest(manifest: &Manifest) -> Result<String, ManifestError> {
    manifest.validate()?;
    let mut out = String::new();
    out.push_str(&format!("schema_version: {}\n", manifest.schema_version));
    out.push_str(&format!(
        "project_id: {}\n",
        yaml_string(&manifest.project_id.to_string())
    ));
    out.push_str(&format!("title: {}\n", yaml_string(&manifest.title)));
    out.push_str(&format!(
        "project_type: {}\n",
        yaml_string(manifest.project_type.as_str())
    ));
    out.push_str(&format!(
        "primary_language: {}\n",
        yaml_string(&manifest.primary_language)
    ));
    out.push_str("additional_languages: ");
    if manifest.additional_languages.is_empty() {
        out.push_str("[]");
    } else {
        out.push('[');
        for (index, language) in manifest.additional_languages.iter().enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            out.push_str(&yaml_string(language));
        }
        out.push(']');
    }
    out.push('\n');
    out.push_str(&format!(
        "created_at: {}\n",
        yaml_string(&manifest.created_at)
    ));
    out.push_str(&format!(
        "default_screenplay_path: {}\n",
        yaml_string(&manifest.default_screenplay_path)
    ));
    Ok(out)
}

fn required_string<'a>(
    root: &'a Yaml,
    field: &'static str,
    expected: &'static str,
) -> Result<&'a str, ManifestError> {
    match root.as_mapping_get(field) {
        Some(value) => value
            .as_str()
            .ok_or(ManifestError::MissingField { field, expected }),
        None => Err(ManifestError::MissingField { field, expected }),
    }
}

fn require_non_empty(field: &'static str, value: &str) -> Result<(), ManifestError> {
    if value.trim().is_empty() {
        return Err(ManifestError::InvalidValue {
            field,
            detail: "must not be empty".to_string(),
        });
    }
    Ok(())
}

/// Loose BCP 47-ish check: ASCII letters, digits and dashes, non-empty.
fn require_language(field: &'static str, value: &str) -> Result<(), ManifestError> {
    require_non_empty(field, value)?;
    if !value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(ManifestError::InvalidValue {
            field,
            detail: format!("`{value}` is not a language tag (letters, digits and '-' only)"),
        });
    }
    Ok(())
}

/// Manifest paths must be project-relative and must not traverse upward,
/// which also guarantees no machine-specific absolute paths (M2 acceptance).
pub fn require_relative_path(field: &'static str, value: &str) -> Result<(), ManifestError> {
    require_non_empty(field, value)?;
    let path = std::path::Path::new(value);
    if path.is_absolute() {
        return Err(ManifestError::InvalidValue {
            field,
            detail: "must be a project-relative path, not an absolute path".to_string(),
        });
    }
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(ManifestError::InvalidValue {
            field,
            detail: "must not contain '..' segments".to_string(),
        });
    }
    Ok(())
}

fn parse_rfc3339(field: &'static str, value: &str) -> Result<OffsetDateTime, ManifestError> {
    OffsetDateTime::parse(value, &Rfc3339).map_err(|_| ManifestError::InvalidValue {
        field,
        detail: "not an RFC 3339 timestamp".to_string(),
    })
}

/// Escape a string as a YAML double-quoted scalar.
fn yaml_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Manifest {
        Manifest::new_v1(
            "Mi serie",
            ProjectType::Series,
            "es",
            "screenplay/main.fountain",
        )
    }

    #[test]
    fn round_trip_is_stable() {
        let manifest = sample();
        let text = serialize_manifest(&manifest).expect("serialize");
        let parsed = parse_manifest(&text).expect("parse");
        assert_eq!(parsed, manifest);
        let again = serialize_manifest(&parsed).expect("serialize");
        assert_eq!(again, text);
    }

    #[test]
    fn rejects_unknown_schema_version() {
        let text = "schema_version: 2\nproject_id: \"00000000-0000-0000-0000-000000000000\"\n";
        let error = parse_manifest(text).expect_err("v2 must be rejected");
        assert_eq!(
            error,
            ManifestError::UnsupportedSchemaVersion {
                found: 2,
                supported: MANIFEST_SCHEMA_VERSION
            }
        );
        assert!(error.to_string().contains("was not modified"));
    }

    #[test]
    fn rejects_unknown_field() {
        let text = serialize_manifest(&sample()).expect("serialize");
        let tampered = format!("{text}secret_token: abc123\n");
        let error = parse_manifest(&tampered).expect_err("unknown field must be rejected");
        assert!(matches!(
            error,
            ManifestError::UnknownField { ref field, version: 1 } if field == "secret_token"
        ));
    }

    #[test]
    fn rejects_missing_required_field() {
        let text = "schema_version: 1\n";
        let error = parse_manifest(text).expect_err("missing fields");
        assert!(matches!(error, ManifestError::MissingField { .. }));
    }

    #[test]
    fn rejects_bad_uuid() {
        let manifest = sample();
        let text = serialize_manifest(&manifest).expect("serialize");
        let tampered = text.replace(&manifest.project_id.to_string(), "not-a-uuid");
        let error = parse_manifest(&tampered).expect_err("bad uuid");
        assert!(matches!(
            error,
            ManifestError::InvalidValue {
                field: "project_id",
                ..
            }
        ));
    }

    #[test]
    fn rejects_unknown_project_type() {
        let text = serialize_manifest(&sample())
            .expect("serialize")
            .replace("\"series\"", "\"telenovela\"");
        let error = parse_manifest(&text).expect_err("bad project type");
        assert!(matches!(
            error,
            ManifestError::InvalidValue {
                field: "project_type",
                ..
            }
        ));
    }

    #[test]
    fn rejects_absolute_screenplay_path() {
        let mut manifest = sample();
        manifest.default_screenplay_path = "/Users/someone/screen.fountain".to_string();
        let error = serialize_manifest(&manifest).expect_err("absolute path");
        assert!(matches!(
            error,
            ManifestError::InvalidValue {
                field: "default_screenplay_path",
                ..
            }
        ));
    }

    #[test]
    fn rejects_parent_traversal_in_screenplay_path() {
        let text = serialize_manifest(&sample()).expect("serialize").replace(
            "default_screenplay_path: \"screenplay/main.fountain\"",
            "default_screenplay_path: \"../outside.fountain\"",
        );
        let error = parse_manifest(&text).expect_err("traversal must be rejected");
        assert!(matches!(
            error,
            ManifestError::InvalidValue {
                field: "default_screenplay_path",
                ..
            }
        ));
    }

    #[test]
    fn rejects_invalid_language_tag() {
        let mut manifest = sample();
        manifest.primary_language = "español!".to_string();
        let error = serialize_manifest(&manifest).expect_err("bad language");
        assert!(matches!(
            error,
            ManifestError::InvalidValue {
                field: "primary_language",
                ..
            }
        ));
    }

    #[test]
    fn rejects_invalid_created_at() {
        let mut manifest = sample();
        manifest.created_at = "ayer".to_string();
        let error = serialize_manifest(&manifest).expect_err("bad timestamp");
        assert!(matches!(
            error,
            ManifestError::InvalidValue {
                field: "created_at",
                ..
            }
        ));
    }

    #[test]
    fn rejects_non_mapping_document() {
        let error = parse_manifest("- just\n- a\n- list\n").expect_err("list is not a mapping");
        assert!(matches!(error, ManifestError::Parse(_)));
    }

    #[test]
    fn escapes_control_characters_in_title() {
        let mut manifest = sample();
        manifest.title = "Comilla \" y \n salto".to_string();
        let text = serialize_manifest(&manifest).expect("serialize");
        let parsed = parse_manifest(&text).expect("parse");
        assert_eq!(parsed.title, manifest.title);
    }

    #[test]
    fn carries_additional_languages() {
        let mut manifest = sample();
        manifest.additional_languages = vec!["en".to_string(), "pt-BR".to_string()];
        let text = serialize_manifest(&manifest).expect("serialize");
        let parsed = parse_manifest(&text).expect("parse");
        assert_eq!(parsed.additional_languages, manifest.additional_languages);
    }

    #[test]
    fn parses_hand_written_yaml_from_a_normal_editor() {
        let text = concat!(
            "schema_version: 1\n",
            "project_id: \"7f9f4a2e-9a58-4a2e-8d5b-1a2b3c4d5e6f\"\n",
            "title: Proyecto a mano\n",
            "project_type: feature\n",
            "primary_language: es\n",
            "created_at: 2026-10-22T09:00:00Z\n",
            "default_screenplay_path: screenplay/main.fountain\n",
        );
        let manifest = parse_manifest(text).expect("hand-written manifest");
        assert_eq!(manifest.title, "Proyecto a mano");
        assert!(manifest.additional_languages.is_empty());
    }
}
