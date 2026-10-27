//! Screenplay export (Milestone 7, Spike G-2).
//!
//! UI-neutral output built from the canonical project state: the core
//! produces bytes, callers (harness, desktop app) only choose where to
//! write them. Nothing here touches the network or a model provider —
//! export works with zero credentials (M7 acceptance).
//!
//! Rules that keep exports honest:
//! - the screenplay body is never re-serialized: Fountain source files
//!   stay authoritative (`DATA_MODEL.md` §1), so the export appends or
//!   prepends around the *verbatim* bytes;
//! - exported files are artifacts, never canonical project content, so
//!   they are written outside the document tree the editor lists.

use crate::fountain::{self, ElementKind};
use crate::manifest::Manifest;

/// Export a screenplay as a shareable Fountain document.
///
/// If the source already carries a title page, it is returned byte-for-byte
/// unchanged (the document's own title page wins). Otherwise a title page
/// derived from the manifest is prepended: `Title` from the project title
/// and `Draft date` from the project creation date (the manifest has no
/// author/source fields and this function invents none).
pub fn fountain(manifest: &Manifest, screenplay: &str) -> String {
    let elements = fountain::parse(screenplay);
    let has_title_page = elements
        .first()
        .is_some_and(|element| element.kind == ElementKind::TitlePage);
    if has_title_page {
        return screenplay.to_string();
    }
    let draft_date = manifest
        .created_at
        .get(..10)
        .unwrap_or(manifest.created_at.as_str());
    format!(
        "Title: {}\nDraft date: {}\n\n{}",
        manifest.title, draft_date, screenplay
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::parse_manifest;

    fn manifest(title: &str) -> Manifest {
        let yaml = format!(
            "schema_version: 1\nproject_id: 00000000-0000-0000-0000-000000000001\ntitle: '{title}'\nproject_type: feature\nprimary_language: es\ncreated_at: 2026-10-20T12:00:00Z\ndefault_screenplay_path: screenplay/main.fountain\n"
        );
        parse_manifest(&yaml).expect("manifest")
    }

    const BODY: &str = "INT. OFICINA - DÍA\n\nMarta teclea frente a la ventana.\n\nFIN.\n";

    #[test]
    fn prepends_a_manifest_title_page_and_keeps_the_body_verbatim() {
        let out = fountain(&manifest("Demo M6"), BODY);
        assert!(out.starts_with("Title: Demo M6\nDraft date: 2026-10-20\n\n"));
        assert!(out.ends_with(BODY));
        // The body bytes are untouched, accents included.
        assert!(out.contains("Marta teclea frente a la ventana."));
    }

    #[test]
    fn an_existing_title_page_wins_and_the_output_is_byte_identical() {
        let with_title = "Title: Guion propio\nAuthor: Alguien\n\nINT. CALLE - NOCHE\n";
        let out = fountain(&manifest("Demo"), with_title);
        assert_eq!(out, with_title);
    }

    #[test]
    fn export_is_deterministic() {
        let a = fountain(&manifest("Demo"), BODY);
        let b = fountain(&manifest("Demo"), BODY);
        assert_eq!(a, b);
    }

    #[test]
    fn title_page_keys_follow_the_fountain_spec() {
        let out = fountain(&manifest("Un título: con dos puntos"), BODY);
        // The title line parses back as a Fountain title-page key.
        let elements = fountain::parse(&out);
        assert_eq!(elements[0].kind, ElementKind::TitlePage);
        assert!(
            elements[0]
                .text
                .starts_with("Title: Un título: con dos puntos")
        );
    }
}
