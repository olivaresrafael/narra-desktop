//! New-document flow (D-062): the chooser is built from the active type
//! package, never from a closed list in the app. Entities become labeled
//! options (their folder plus the covering write root's extension); every
//! other write root becomes a generic document option with the package's
//! label. Pure and unit-tested; the UI only collects a name and calls
//! `Project::create_document`.

use narra_core::project_type::{LocalizedText, ProjectProfile, WriteRoot};

/// One option in the New File chooser, derived from the package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kind {
    /// Stable key: entity id or write-root path (dumps and UI ids).
    pub key: String,
    pub label: LocalizedText,
    /// Project-relative folder the document lands in.
    pub folder: String,
    /// Extension without the dot (`md`, `fountain`).
    pub extension: String,
}

impl Kind {
    /// Options for a profile: entities first (in declaration order), then
    /// write roots that are not exactly an entity folder.
    pub fn from_profile(profile: &ProjectProfile) -> Vec<Kind> {
        let mut kinds = Vec::new();
        for entity in &profile.entities {
            kinds.push(Kind {
                key: entity.id.clone(),
                label: entity.label.clone(),
                folder: entity.folder.clone(),
                extension: extension_for(&profile.write_roots, &entity.folder),
            });
        }
        for root in &profile.write_roots {
            if profile
                .entities
                .iter()
                .any(|entity| entity.folder == root.path)
            {
                continue;
            }
            kinds.push(Kind {
                key: root.path.clone(),
                label: root
                    .label
                    .clone()
                    .unwrap_or_else(|| folder_label(&root.path)),
                folder: root.path.clone(),
                extension: preferred_extension(root),
            });
        }
        kinds
    }

    /// Project-relative path for a new document, or `None` for an invalid name.
    pub fn target_path(&self, name: &str) -> Option<String> {
        let stem = file_stem(name)?;
        Some(format!("{}/{}.{}", self.folder, stem, self.extension))
    }

    /// Initial content: a blank Fountain page for `.fountain`, otherwise a
    /// minimal Markdown title. Section scaffolds belong to package templates,
    /// never to this module.
    pub fn template(&self, name: &str) -> String {
        if self.extension.eq_ignore_ascii_case("fountain") {
            String::new()
        } else {
            format!("# {}\n\n", name.trim())
        }
    }
}

/// Extension a folder's documents use, from the most specific write root
/// that covers it. Folders outside every write root fall back to Markdown.
pub fn extension_for(roots: &[WriteRoot], folder: &str) -> String {
    let mut best: Option<&WriteRoot> = None;
    for root in roots {
        let covers = folder == root.path || folder.starts_with(&format!("{}/", root.path));
        if !covers {
            continue;
        }
        if best.is_none_or(|current| root.path.len() > current.path.len()) {
            best = Some(root);
        }
    }
    best.map(preferred_extension)
        .unwrap_or_else(|| "md".to_string())
}

/// The first declared extension is the root's primary format: the package
/// controls the order, so `[.fountain, .md]` means Fountain documents by
/// default.
fn preferred_extension(root: &WriteRoot) -> String {
    root.extensions
        .first()
        .map(|extension| extension.trim_start_matches('.').to_string())
        .unwrap_or_else(|| "md".to_string())
}

/// Label for a root without one: its last path segment, capitalized. This is
/// presentation, not a creative rule; packages can override it with `label`.
fn folder_label(path: &str) -> LocalizedText {
    let name = path.rsplit('/').next().unwrap_or(path);
    let mut chars = name.chars();
    let label = match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => name.to_string(),
    };
    LocalizedText {
        en: label.clone(),
        es: label,
    }
}

/// Sanitize a user-supplied name into a file stem.
///
/// The name is preserved (case and accents included: the file stem is the
/// entity's display name for bible documents), with inner whitespace
/// collapsed to single spaces. Path separators, control characters, Windows
/// reserved punctuation, leading dots and empty results are refused — the
/// caller shows an explicit error, nothing is silently rewritten.
pub fn file_stem(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut out = String::new();
    let mut last_space = false;
    for c in trimmed.chars() {
        if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
            return None;
        }
        if c.is_whitespace() {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(c);
            last_space = false;
        }
    }
    let out = out.trim().trim_end_matches('.').to_string();
    if out.is_empty() || out.starts_with('.') {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use narra_core::project_type::{TypeOrigin, builtin, parse_package};

    fn feature() -> ProjectProfile {
        builtin("feature").expect("feature package").profile()
    }

    #[test]
    fn chooser_comes_from_the_package() {
        let kinds = Kind::from_profile(&feature());
        let keys: Vec<&str> = kinds.iter().map(|kind| kind.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "character",
                "location",
                "object",
                "bible",
                "structure",
                "research",
                "notes",
                "screenplay"
            ]
        );
        let character = kinds
            .iter()
            .find(|kind| kind.key == "character")
            .expect("character");
        assert_eq!(character.folder, "bible/characters");
        assert_eq!(character.extension, "md");
        assert_eq!(character.label.es, "Personaje");
        let screenplay = kinds
            .iter()
            .find(|kind| kind.key == "screenplay")
            .expect("screenplay");
        assert_eq!(screenplay.extension, "fountain");
        assert_eq!(screenplay.label.es, "Guion");
    }

    #[test]
    fn custom_package_roots_and_entities_drive_the_chooser() {
        let package = parse_package(
            "schema_version: 2\napi_version: 1\nid: campaign\nname: Campaign\ndirectories: [brief, audiences]\nwrite_roots:\n  - path: brief\n    extensions: [.md]\n    label: { en: Brief, es: Briefing }\nentities:\n  - id: audience\n    label: { en: Audience, es: Audiencia }\n    folder: audiences\n",
            TypeOrigin::User,
        )
        .expect("campaign package");
        let kinds = Kind::from_profile(&package.profile());
        let keys: Vec<&str> = kinds.iter().map(|kind| kind.key.as_str()).collect();
        assert_eq!(keys, ["audience", "brief"]);
        assert_eq!(kinds[0].extension, "md", "outside a root falls back to md");
        assert_eq!(kinds[1].label.es, "Briefing", "package label wins");
    }

    #[test]
    fn paths_and_templates_follow_the_package() {
        let kinds = Kind::from_profile(&feature());
        let character = kinds
            .iter()
            .find(|kind| kind.key == "character")
            .expect("character");
        assert_eq!(
            character.target_path("Marta Ruiz").as_deref(),
            Some("bible/characters/Marta Ruiz.md")
        );
        assert_eq!(character.template("Marta"), "# Marta\n\n");

        let screenplay = kinds
            .iter()
            .find(|kind| kind.key == "screenplay")
            .expect("screenplay");
        assert_eq!(
            screenplay.target_path("S01E01 Piloto").as_deref(),
            Some("screenplay/S01E01 Piloto.fountain")
        );
        assert_eq!(screenplay.template("Piloto"), "");
    }

    #[test]
    fn names_are_sanitized_without_rewriting_case_or_accents() {
        assert_eq!(file_stem("  Marta   Ruiz  ").as_deref(), Some("Marta Ruiz"));
        assert_eq!(file_stem("Ñandú").as_deref(), Some("Ñandú"));
        assert_eq!(file_stem("Episodio 1.").as_deref(), Some("Episodio 1"));
    }

    #[test]
    fn unsafe_names_are_refused() {
        let kind = Kind::from_profile(&feature())
            .into_iter()
            .find(|kind| kind.key == "notes")
            .expect("notes");
        for name in [
            "",
            "   ",
            "../escape",
            "a/b",
            "a\\b",
            ".hidden",
            "a:b",
            "a*b",
            "a?b",
            "a|b",
            "a\u{0}b",
        ] {
            assert!(kind.target_path(name).is_none(), "{name:?} accepted");
        }
    }
}
