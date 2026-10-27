//! Project-wide literal search (M8 slice 1).
//!
//! The editor's find bar searches the open document; the MVP also requires
//! project-wide search (PRODUCT_SPEC §5). This module scans the canonical
//! documents through the same boundary-checked project IO as everything
//! else — no new IO paths, no symlinks, nothing outside the project — and
//! returns a bounded, deterministically ordered report that both the app
//! and the harness can render.
//!
//! Scope decisions (documented, deliberate):
//! - literal, case-insensitive matching (no regex engine: a regex is a new
//!   dependency and an unbounded-cost surface for an MVP search);
//! - accent-sensitive but case-folded (searching `DÍA` finds `día`, not
//!   `dia`: folding accents silently would surprise the writer);
//! - every scan is bounded (documents, hits) and unreadable documents are
//!   skipped and counted instead of failing the whole search.

use crate::project::{Project, ProjectError};

/// Documents scanned at most per search, in `list_documents` order.
pub const MAX_SCANNED_DOCUMENTS: usize = 200;
/// Hits collected at most per search; the report says when it stopped.
pub const MAX_HITS: usize = 200;
/// Longest preview, in characters.
pub const MAX_PREVIEW_CHARS: usize = 120;

/// One match: where it is and a one-line preview for the result list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    /// Project-relative document path.
    pub document: String,
    /// 1-based line number.
    pub line: usize,
    /// 1-based character column of the match within the line.
    pub column: usize,
    /// Trimmed line text, capped at [`MAX_PREVIEW_CHARS`] with `…` markers
    /// when a window had to be taken.
    pub preview: String,
}

/// Search outcome. `truncated` means the hit cap was reached, not that the
/// scan is incomplete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchReport {
    pub hits: Vec<SearchHit>,
    pub documents_scanned: usize,
    /// Documents that could not be read as UTF-8 text (binary artifacts,
    /// for example); they never fail the search.
    pub documents_skipped: usize,
    pub truncated: bool,
}

/// Case-insensitive literal search across the project's documents.
///
/// Results are ordered by document path, then line, then column. An empty
/// or whitespace-only query returns an empty report without touching disk.
pub fn search(project: &Project, query: &str) -> Result<SearchReport, ProjectError> {
    let needle = query.trim().to_lowercase();
    let mut report = SearchReport {
        hits: Vec::new(),
        documents_scanned: 0,
        documents_skipped: 0,
        truncated: false,
    };
    if needle.is_empty() {
        return Ok(report);
    }

    let documents = project.list_documents()?;
    for document in documents.iter().take(MAX_SCANNED_DOCUMENTS) {
        let text = match project.read_document(document) {
            Ok(text) => text,
            Err(_) => {
                report.documents_skipped += 1;
                continue;
            }
        };
        report.documents_scanned += 1;
        for (index, line) in text.lines().enumerate() {
            let lower = line.to_lowercase();
            let mut from = 0;
            while let Some(found) = lower[from..].find(&needle) {
                let byte_at = from + found;
                // Characters folded 1:1 for Latin text; the column is exact
                // for every letter Spanish can produce.
                let column = lower[..byte_at].chars().count() + 1;
                report.hits.push(SearchHit {
                    document: document.clone(),
                    line: index + 1,
                    column,
                    preview: preview(line, column - 1),
                });
                if report.hits.len() >= MAX_HITS {
                    report.truncated = true;
                    return Ok(report);
                }
                // Next search starts after this match (never loops on an
                // empty needle; `needle` is non-empty here).
                from = byte_at + needle.len().max(1);
                if from >= lower.len() {
                    break;
                }
            }
        }
    }
    Ok(report)
}

/// A trimmed, bounded one-line preview; when the match sits outside the
/// first [`MAX_PREVIEW_CHARS`] characters the window moves to include it,
/// marked with `…` on either side.
fn preview(line: &str, match_col: usize) -> String {
    let chars: Vec<char> = line.trim().chars().collect();
    if chars.len() <= MAX_PREVIEW_CHARS {
        return chars.into_iter().collect();
    }
    let lead = MAX_PREVIEW_CHARS / 3;
    let start = match_col
        .saturating_sub(lead)
        .min(chars.len() - MAX_PREVIEW_CHARS);
    let end = start + MAX_PREVIEW_CHARS;
    let mut text = String::new();
    if start > 0 {
        text.push('…');
    }
    text.extend(&chars[start..end]);
    if end < chars.len() {
        text.push('…');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProjectType;
    use crate::project::{create_project, open_project};
    use std::fs;
    use std::path::PathBuf;

    fn temp_root() -> PathBuf {
        tempfile::tempdir().expect("tempdir").keep()
    }

    fn project_with(files: &[(&str, &str)]) -> Project {
        let root = temp_root();
        let project =
            create_project(&root, "Búsqueda", ProjectType::Feature, "es").expect("create");
        for (path, text) in files {
            project.write_document(path, text).expect("write");
        }
        open_project(&root).expect("open")
    }

    #[test]
    fn finds_literal_matches_across_documents_in_deterministic_order() {
        let project = project_with(&[
            ("screenplay/main.fountain", "INT. CASA\n\nMarta entra.\n"),
            (
                "bible/characters/marta.md",
                "# Marta\n\nMarta es la protagonista.\n",
            ),
        ]);
        let report = search(&project, "marta").expect("search");
        assert!(
            report.documents_scanned >= 2,
            "scanned the project documents"
        );
        assert_eq!(report.documents_skipped, 0);
        assert!(!report.truncated);
        let located: Vec<(&str, usize)> = report
            .hits
            .iter()
            .map(|hit| (hit.document.as_str(), hit.line))
            .collect();
        assert_eq!(
            located,
            vec![
                ("bible/characters/marta.md", 1),
                ("bible/characters/marta.md", 3),
                ("screenplay/main.fountain", 3),
            ],
            "ordered by document path, then line"
        );
        assert_eq!(report.hits[0].column, 3, "1-based character column");
        assert_eq!(report.hits[0].preview, "# Marta");
    }

    #[test]
    fn matching_is_case_insensitive_but_keeps_accents() {
        let project = project_with(&[(
            "screenplay/main.fountain",
            "INT. OFICINA - DÍA\n\nELENA\nBuen día.\n",
        )]);
        let report = search(&project, "DÍA").expect("search");
        // Case-folded: the heading and "Buen día." both match.
        assert_eq!(report.hits.len(), 2);
        assert_eq!((report.hits[0].line, report.hits[0].column), (1, 16));
        assert_eq!((report.hits[1].line, report.hits[1].column), (4, 6));
        // Accents are not folded: `dia` does not match `día`.
        assert!(search(&project, "dia").expect("search").hits.is_empty());
    }

    #[test]
    fn empty_or_whitespace_queries_return_an_empty_report() {
        let project = project_with(&[("notes/decisions.md", "nada\n")]);
        for query in ["", "   "] {
            let report = search(&project, query).expect("search");
            assert!(report.hits.is_empty());
            assert_eq!(report.documents_scanned, 0, "no scan for an empty query");
        }
    }

    #[test]
    fn hits_are_capped_and_truncation_is_reported() {
        let body: String = (0..300).map(|n| format!("línea marta {n}\n")).collect();
        let project = project_with(&[("notes/largo.md", &body)]);
        let report = search(&project, "marta").expect("search");
        assert_eq!(report.hits.len(), MAX_HITS);
        assert!(report.truncated);
    }

    #[test]
    fn unreadable_documents_are_skipped_without_failing_the_search() {
        let root = temp_root();
        let project = create_project(&root, "Mixto", ProjectType::Short, "es").expect("create");
        fs::write(root.join("research/ruido.bin"), [0xff, 0xfe, 0x00, 0x01]).expect("binary");
        project
            .write_document("screenplay/main.fountain", "INT. CASA\n\nMarta entra.\n")
            .expect("screenplay");
        let report = search(&project, "marta").expect("search");
        assert_eq!(report.hits.len(), 1);
        assert_eq!(report.documents_skipped, 1);
    }

    #[test]
    fn long_lines_get_a_capped_window_around_the_match() {
        let padding = "a ".repeat(200);
        let line = format!("{padding}MARTA{padding}");
        let project = project_with(&[("notes/largo.md", &format!("{line}\n"))]);
        let report = search(&project, "marta").expect("search");
        assert_eq!(report.hits.len(), 1);
        let hit = &report.hits[0];
        assert!(hit.preview.chars().count() <= MAX_PREVIEW_CHARS + 2);
        assert!(hit.preview.contains("MARTA"));
        assert!(hit.preview.starts_with('…') && hit.preview.ends_with('…'));
        // Column points at the match in the original line (1-based).
        assert_eq!(hit.column, 401);
    }
}
