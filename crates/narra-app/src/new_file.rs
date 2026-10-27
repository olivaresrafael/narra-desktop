//! New-document flow (M10 slice 2): the kind chooser and the path/template
//! rules. Pure and unit-tested; the UI only collects a name and calls
//! `Project::create_document`.
//!
//! For now only Frame Fountain is a screenplay format; when the native format
//! and other screenplay formats arrive, the screenplay kind grows a format
//! selector (the path extension follows the chosen format).

/// What kind of document is being created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    Character,
    Notes,
    Research,
    #[default]
    Screenplay,
}

impl Kind {
    pub const ALL: [Kind; 4] = [
        Kind::Character,
        Kind::Notes,
        Kind::Research,
        Kind::Screenplay,
    ];

    /// Dump/CLI key.
    pub fn key(self) -> &'static str {
        match self {
            Kind::Character => "character",
            Kind::Notes => "notes",
            Kind::Research => "research",
            Kind::Screenplay => "screenplay",
        }
    }

    /// Project folder the document belongs to.
    pub fn folder(self) -> &'static str {
        match self {
            Kind::Character => "bible/characters",
            Kind::Notes => "notes",
            Kind::Research => "research",
            Kind::Screenplay => "screenplay",
        }
    }

    /// File extension: screenplays are Fountain for now, the rest Markdown.
    pub fn extension(self) -> &'static str {
        match self {
            Kind::Screenplay => "fountain",
            Kind::Character | Kind::Notes | Kind::Research => "md",
        }
    }

    /// i18n key for the chooser label.
    pub fn i18n_key(self) -> &'static str {
        match self {
            Kind::Character => "newfile.kind.character",
            Kind::Notes => "newfile.kind.notes",
            Kind::Research => "newfile.kind.research",
            Kind::Screenplay => "newfile.kind.screenplay",
        }
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

/// Project-relative path for a new document, or `None` for an invalid name.
pub fn target_path(kind: Kind, name: &str) -> Option<String> {
    let stem = file_stem(name)?;
    Some(format!("{}/{}.{}", kind.folder(), stem, kind.extension()))
}

/// Initial content of a new document.
pub fn template(kind: Kind, name: &str) -> String {
    let title = name.trim();
    match kind {
        Kind::Character => format!("# {title}\n\n## Rasgos\n\n## Objetivo\n\n## Notas\n"),
        Kind::Notes | Kind::Research => format!("# {title}\n\n"),
        // A screenplay starts from a blank Fountain page, like the project
        // template's default screenplay.
        Kind::Screenplay => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_follow_the_kind_and_its_extension() {
        assert_eq!(
            target_path(Kind::Character, "Marta Ruiz").as_deref(),
            Some("bible/characters/Marta Ruiz.md")
        );
        assert_eq!(
            target_path(Kind::Notes, "ideas").as_deref(),
            Some("notes/ideas.md")
        );
        assert_eq!(
            target_path(Kind::Research, "policía 1920").as_deref(),
            Some("research/policía 1920.md")
        );
        assert_eq!(
            target_path(Kind::Screenplay, "S01E01 Piloto").as_deref(),
            Some("screenplay/S01E01 Piloto.fountain")
        );
    }

    #[test]
    fn names_are_sanitized_without_rewriting_case_or_accents() {
        assert_eq!(file_stem("  Marta   Ruiz  ").as_deref(), Some("Marta Ruiz"));
        assert_eq!(file_stem("Ñandú").as_deref(), Some("Ñandú"));
        assert_eq!(file_stem("Episodio 1.").as_deref(), Some("Episodio 1"));
    }

    #[test]
    fn unsafe_names_are_refused() {
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
            assert!(
                target_path(Kind::Notes, name).is_none(),
                "{name:?} accepted"
            );
        }
    }

    #[test]
    fn templates_seed_the_documents() {
        let character = template(Kind::Character, "Marta");
        assert!(character.starts_with("# Marta\n"));
        assert!(character.contains("## Rasgos"));
        assert!(template(Kind::Notes, "Ideas").contains("# Ideas"));
        assert!(template(Kind::Research, "Fuentes").contains("# Fuentes"));
        assert_eq!(template(Kind::Screenplay, "Piloto"), "");
    }
}
