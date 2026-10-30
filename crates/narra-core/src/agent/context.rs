//! Project context assembly for agent tasks (D-047).
//!
//! The brief is deliberately layered: a bounded inventory tells the model
//! which files exist, and the active type's pinned paths provide the material
//! a writer expects the assistant to know without attaching it. Content is
//! capped per file and per turn; anything that does not fit remains visible in
//! the inventory and can be read on demand once the read-only tools land.
//!
//! The module is UI-neutral and uses the same boundary-checked project IO as
//! the rest of the core. Hidden state, exports and configuration are excluded
//! by `Project::list_documents` and the path validation in the type package.

use std::collections::BTreeSet;

use super::ContextSource;
use crate::project::{INSTRUCTIONS_FILE, Project};
use crate::project_type;

/// Documents listed in the inventory at most.
pub const MAX_INVENTORY_FILES: usize = 200;
/// Characters included per pinned document.
pub const MAX_DOC_CHARS: usize = 8_000;
/// Characters included by the pinned layer in one turn.
pub const MAX_PINNED_CHARS: usize = 24_000;
/// Characters included from `.narra/instructions.md`.
pub const MAX_INSTRUCTIONS_CHARS: usize = 8_000;

/// Build the always-on project brief and its visible sources.
pub fn project_context(project: &Project) -> (String, Vec<ContextSource>) {
    let mut context = String::new();
    let mut sources = Vec::new();
    let documents = project.list_documents().unwrap_or_default();
    push_inventory(project, &documents, &mut context, &mut sources);
    push_instructions(project, &mut context, &mut sources);
    push_pinned(project, &documents, &mut context, &mut sources);
    (context, sources)
}

fn push_instructions(project: &Project, context: &mut String, sources: &mut Vec<ContextSource>) {
    let Ok(content) = project.read_document(INSTRUCTIONS_FILE) else {
        return;
    };
    if content.trim().is_empty() {
        return;
    }
    let chars = content.chars().count();
    let text = truncate(&content, MAX_INSTRUCTIONS_CHARS);
    context.push_str(&format!("\n[instrucciones del proyecto]\n{text}\n"));
    sources.push(ContextSource {
        kind: "instructions".to_string(),
        location: INSTRUCTIONS_FILE.to_string(),
        detail: if chars > MAX_INSTRUCTIONS_CHARS {
            format!("{chars} caracteres (truncados a {MAX_INSTRUCTIONS_CHARS})")
        } else {
            format!("{chars} caracteres")
        },
    });
}

fn push_inventory(
    project: &Project,
    documents: &[String],
    context: &mut String,
    sources: &mut Vec<ContextSource>,
) {
    if documents.is_empty() {
        return;
    }
    let shown = documents.len().min(MAX_INVENTORY_FILES);
    context.push_str("\n[inventario del proyecto]\n");
    for path in documents.iter().take(shown) {
        let size = std::fs::metadata(project.root().join(path))
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        context.push_str(&format!("- {path} ({size} B)\n"));
    }
    if documents.len() > shown {
        context.push_str(&format!(
            "- … {} archivos más no listados\n",
            documents.len() - shown
        ));
    }
    sources.push(ContextSource {
        kind: "inventory".to_string(),
        location: "project".to_string(),
        detail: format!("{shown} archivos del proyecto"),
    });
}

fn push_pinned(
    project: &Project,
    documents: &[String],
    context: &mut String,
    sources: &mut Vec<ContextSource>,
) {
    let type_id = project.manifest().project_type_id.as_str();
    let package = project_type::resolve(project.root(), type_id);
    let pinned = package
        .as_ref()
        .map(project_type::ProjectTypePackage::pinned_context)
        .unwrap_or_else(|| vec!["bible".to_string()]);
    let mut paths = BTreeSet::new();
    for entry in &pinned {
        let prefix = format!("{entry}/");
        for path in documents {
            if (path == entry || path.starts_with(&prefix)) && is_text_path(path) {
                paths.insert(path.clone());
            }
        }
    }

    let mut used = 0usize;
    for path in paths {
        if used >= MAX_PINNED_CHARS {
            break;
        }
        let Ok(content) = project.read_document(&path) else {
            continue;
        };
        let chars = content.chars().count();
        let cap = MAX_DOC_CHARS.min(MAX_PINNED_CHARS - used);
        let text = truncate(&content, cap);
        used += text.chars().count();
        context.push_str(&format!("\n[documento: {path}]\n{text}\n"));
        sources.push(ContextSource {
            kind: "document".to_string(),
            location: path,
            detail: if chars > cap {
                format!("{chars} caracteres (truncados a {cap})")
            } else {
                format!("{chars} caracteres")
            },
        });
    }
}

/// The read tools will use the same allowlist. Binary/unknown extensions are
/// never included in the brief; YAML is content the writer may pin.
pub fn is_text_path(path: &str) -> bool {
    matches!(
        path.rsplit('.').next(),
        Some("md" | "markdown" | "fountain" | "txt" | "yaml" | "yml")
    )
}

pub(crate) fn truncate(text: &str, max_chars: usize) -> String {
    let mut out: String = text.chars().take(max_chars).collect();
    if text.chars().count() > max_chars {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProjectType;
    use crate::project::create_project;
    use std::fs;
    use std::path::PathBuf;

    fn temp_root() -> PathBuf {
        tempfile::tempdir().expect("tempdir").keep()
    }

    #[test]
    fn builtin_feature_pins_the_bible_and_structure() {
        let root = temp_root();
        let project =
            create_project(&root, "Contexto", ProjectType::Feature, "es").expect("create project");
        project
            .write_document("bible/characters/marta.md", "# Marta\n\nProtagonista.")
            .expect("character");
        project
            .write_document("structure/beats.md", "# Beats\n\nActo I.")
            .expect("beats");

        let (context, sources) = project_context(&project);
        assert!(context.contains("[inventario del proyecto]"));
        assert!(context.contains("[documento: bible/characters/marta.md]"));
        assert!(context.contains("[documento: structure/beats.md]"));
        assert!(
            sources
                .iter()
                .any(|source| source.location == "bible/characters/marta.md")
        );
        assert!(
            sources
                .iter()
                .any(|source| source.location == "structure/beats.md")
        );
    }

    #[test]
    fn project_instructions_are_always_included() {
        let root = temp_root();
        let project =
            create_project(&root, "Contexto", ProjectType::Feature, "es").expect("create project");
        project
            .write_document(INSTRUCTIONS_FILE, "# Instrucciones\n\nNo inventes canon.")
            .expect("instructions");
        let (context, sources) = project_context(&project);
        assert!(context.contains("[instrucciones del proyecto]"));
        assert!(context.contains("No inventes canon."));
        assert!(sources.iter().any(|source| {
            source.kind == "instructions" && source.location == INSTRUCTIONS_FILE
        }));
    }

    #[test]
    fn custom_package_can_pin_its_own_folders() {
        let package = project_type::parse_package(
            "schema_version: 1\napi_version: 1\nid: campaign\nname: Campaign\ndirectories: [brief]\nfiles: []\ncontext:\n  pinned: [brief]\n",
            project_type::TypeOrigin::User,
        )
        .expect("package");
        assert_eq!(package.pinned_context(), vec!["brief".to_string()]);
    }

    #[test]
    fn omitted_context_inherits_bible_and_empty_context_pins_nothing() {
        let omitted = project_type::parse_package(
            "schema_version: 1\napi_version: 1\nid: documentary\nname: Documentary\n",
            project_type::TypeOrigin::User,
        )
        .expect("package");
        assert_eq!(omitted.pinned_context(), vec!["bible".to_string()]);

        let empty = project_type::parse_package(
            "schema_version: 1\napi_version: 1\nid: documentary\nname: Documentary\ncontext:\n  pinned: []\n",
            project_type::TypeOrigin::User,
        )
        .expect("package");
        assert!(empty.pinned_context().is_empty());
    }

    #[test]
    fn pinned_content_is_capped_per_turn() {
        let root = temp_root();
        let project =
            create_project(&root, "Contexto", ProjectType::Feature, "es").expect("create project");
        for index in 0..4 {
            let body = format!("# Doc {index}\n\n{}", "x".repeat(8_000));
            project
                .write_document(&format!("bible/doc-{index}.md"), &body)
                .expect("doc");
        }
        let (context, sources) = project_context(&project);
        let document_chars: usize = sources
            .iter()
            .filter(|source| source.kind == "document")
            .map(|source| {
                let path = &source.location;
                context
                    .split(&format!("[documento: {path}]"))
                    .nth(1)
                    .and_then(|rest| rest.split("\n[documento:").next())
                    .map(|body| body.chars().count())
                    .unwrap_or(0)
            })
            .sum();
        assert!(
            document_chars <= MAX_PINNED_CHARS + 8 * 4,
            "pinned content exceeded the turn budget: {document_chars}"
        );
        assert!(
            sources
                .iter()
                .any(|source| source.detail.contains("truncados")),
            "expected at least one capped document"
        );
    }

    #[test]
    fn binary_files_are_not_included_as_content() {
        let root = temp_root();
        let project =
            create_project(&root, "Contexto", ProjectType::Feature, "es").expect("create project");
        fs::create_dir_all(root.join("bible")).expect("bible");
        fs::write(root.join("bible/ruido.bin"), [0xff, 0xfe, 0x00]).expect("binary");
        let (context, sources) = project_context(&project);
        assert!(!context.contains("[documento: bible/ruido.bin]"));
        assert!(
            !sources
                .iter()
                .any(|source| source.location == "bible/ruido.bin")
        );
    }
}
