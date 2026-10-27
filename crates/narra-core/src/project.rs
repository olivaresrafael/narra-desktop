//! Project lifecycle: create, open, validate and rebuild derived indexes.
//!
//! Milestone 2 (Spike G-1). Canonical rules:
//! - on-disk directory names are English (D-017); localized labels belong to
//!   the UI only;
//! - the manifest is the authority for project identity, files are the
//!   authority for narrative content (parser stays derived, never canonical);
//! - indexes are disposable and rebuilt from source files;
//! - an unknown `schema_version` fails safely and is never rewritten.

use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::document::{self, DocumentError};
use crate::manifest::{self, Manifest, ManifestError, ProjectType};

/// Directory template required by D-017. Not every project must use every
/// directory later, but a created project ships the full layout.
pub const TEMPLATE_DIRS: &[&str] = &[
    "bible/characters",
    "structure",
    "screenplay",
    "research",
    "notes",
    "reviews",
];

/// Files created by `create_project`.
pub const MANIFEST_FILE: &str = "project.yaml";
pub const DEFAULT_SCREENPLAY: &str = "screenplay/main.fountain";
/// Export artifacts (M7): written by `narra-harness export` and the app's
/// ⌘E, and excluded from the document scan — they are outputs, never
/// canonical content.
pub const EXPORTS_DIR: &str = "exports";

/// Maximum directory depth scanned by [`Project::list_documents`].
const MAX_DOC_DEPTH: usize = 8;
/// Upper bound on listed documents (M6 slice 1): after sorting, the cut is
/// deterministic — the lexicographically first paths win — so a pathological
/// tree cannot stall the file explorer, and canonical files (which sort near
/// the top of their directories) are never the ones dropped.
const MAX_LISTED_DOCUMENTS: usize = 10_000;

/// Errors from project lifecycle operations.
#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("a project already exists at `{0}` (project.yaml found)")]
    AlreadyExists(PathBuf),

    #[error("`{0}` is not a directory")]
    NotADirectory(PathBuf),

    #[error("no project found at `{0}` (project.yaml missing)")]
    NotFound(PathBuf),

    #[error(transparent)]
    Manifest(#[from] ManifestError),

    #[error(transparent)]
    Document(#[from] DocumentError),

    #[error("failed to access `{path}`: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// An opened project: validated manifest plus its root on disk.
#[derive(Debug, Clone)]
pub struct Project {
    root: PathBuf,
    manifest: Manifest,
}

impl Project {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Read a UTF-8 document inside the project boundary.
    pub fn read_document(&self, relative: &str) -> Result<String, ProjectError> {
        Ok(document::read_document(&self.root, relative)?)
    }

    /// Atomically write a document inside the project boundary.
    pub fn write_document(&self, relative: &str, contents: &str) -> Result<(), ProjectError> {
        Ok(document::write_document(&self.root, relative, contents)?)
    }

    /// Scan the canonical narrative sources and rebuild the derived index.
    ///
    /// The index is a convenience only; source files remain authoritative
    /// (`DATA_MODEL.md` §1), so any read error here is reported, never papered
    /// over with stale data.
    pub fn rebuild_index(&self) -> Result<ProjectIndex, ProjectError> {
        let mut index = ProjectIndex::default();

        // Scene headings from the primary screenplay via the derived Fountain
        // parser (Spike G-1). The Fountain file, not this index, stays
        // authoritative (`DATA_MODEL.md` §1).
        match self.read_document(&self.manifest.default_screenplay_path) {
            Ok(text) => {
                for element in crate::fountain::parse(&text) {
                    if element.kind == crate::fountain::ElementKind::SceneHeading {
                        index.scenes.push(SceneRef {
                            heading: element.text,
                            line_number: element.line_number,
                        });
                    }
                }
            }
            // A project may be created before any screenplay exists; an empty
            // index is correct then. Any other failure is real and surfaced.
            Err(ProjectError::Document(DocumentError::Io { source, .. }))
                if source.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }

        for note in ["notes/decisions.md", "notes/open-questions.md"] {
            if self.read_document(note).is_ok() {
                index.tracked_notes.push(note.to_string());
            }
        }
        Ok(index)
    }

    /// List the project's documents for a file explorer (M6 slice 1).
    ///
    /// Project-relative paths (`/`-separated) in sorted order so the UI can
    /// render a stable tree. Hidden entries (`.git`, `.narra`, dot-files and
    /// editor temp files) are excluded: they are state, not narrative
    /// content, and the explorer must never offer them. Symlinks are neither
    /// followed nor listed (the boundary check in `document.rs` rejects
    /// escapes anyway), and a non-UTF-8 name is skipped because it can never
    /// pass the boundary-checked document IO.
    pub fn list_documents(&self) -> Result<Vec<String>, ProjectError> {
        let mut documents = Vec::new();
        walk_documents(&self.root, "", 0, &mut documents)?;
        documents.sort();
        documents.truncate(MAX_LISTED_DOCUMENTS);
        Ok(documents)
    }
}

/// Depth-first collection for [`Project::list_documents`]; `prefix` is the
/// project-relative path of `dir` (`""` at the root).
fn walk_documents(
    dir: &Path,
    prefix: &str,
    depth: usize,
    out: &mut Vec<String>,
) -> Result<(), ProjectError> {
    if depth > MAX_DOC_DEPTH {
        return Ok(());
    }
    let entries = fs::read_dir(dir).map_err(|source| ProjectError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| ProjectError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        // Non-UTF-8 names can never be addressed by document IO: skip them.
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        // Hidden entries are state, not content (`.git`, `.narra`, temp files).
        if name.starts_with('.') {
            continue;
        }
        // Exports are artifacts, not canonical documents (M7): the editor
        // never lists them, so a binary PDF can never be opened as text.
        if prefix.is_empty() && name == EXPORTS_DIR {
            continue;
        }
        let relative = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let file_type = entry.file_type().map_err(|source| ProjectError::Io {
            path: entry.path(),
            source,
        })?;
        if file_type.is_dir() {
            walk_documents(&entry.path(), &relative, depth + 1, out)?;
        } else if file_type.is_file() {
            out.push(relative);
        }
        // Symlinks report neither `is_dir` nor `is_file` on the DirEntry
        // type (which does not follow links), so they fall through here by
        // design: following one could escape the project or duplicate content.
    }
    Ok(())
}

/// Derived, disposable scan results (`DATA_MODEL.md` §1).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ProjectIndex {
    pub scenes: Vec<SceneRef>,
    pub tracked_notes: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct SceneRef {
    pub heading: String,
    /// 1-based line in the source file.
    pub line_number: usize,
}

/// Create a new project at `root` with the D-017 template.
///
/// Fails with [`ProjectError::AlreadyExists`] if a manifest is already there —
/// creation never overwrites an existing project.
pub fn create_project(
    root: &Path,
    title: &str,
    project_type: ProjectType,
    primary_language: &str,
) -> Result<Project, ProjectError> {
    if root.exists() {
        if !root.is_dir() {
            return Err(ProjectError::NotADirectory(root.to_path_buf()));
        }
        if root.join(MANIFEST_FILE).exists() {
            return Err(ProjectError::AlreadyExists(root.to_path_buf()));
        }
    }
    fs::create_dir_all(root).map_err(|source| ProjectError::Io {
        path: root.to_path_buf(),
        source,
    })?;

    for dir in TEMPLATE_DIRS {
        fs::create_dir_all(root.join(dir)).map_err(|source| ProjectError::Io {
            path: root.join(dir),
            source,
        })?;
    }

    let manifest = Manifest::new_v1(title, project_type, primary_language, DEFAULT_SCREENPLAY);
    let text = manifest::serialize_manifest(&manifest)?;
    document::write_document(root, MANIFEST_FILE, &text)?;

    // Seed content files. Empty screenplay: the writer starts from a blank page.
    document::write_document(root, DEFAULT_SCREENPLAY, "")?;
    document::write_document(
        root,
        "notes/decisions.md",
        "# Decisions\n\nRecord accepted decisions here. Each entry: date, decision, consequence.\n",
    )?;
    document::write_document(
        root,
        "notes/open-questions.md",
        "# Open questions\n\nRecord unresolved questions here.\n",
    )?;

    Ok(Project {
        root: root.to_path_buf(),
        manifest,
    })
}

/// Open an existing project: `project.yaml` must exist and validate.
///
/// Validation failures are surfaced as-is; the manifest is never rewritten.
pub fn open_project(root: &Path) -> Result<Project, ProjectError> {
    if !root.is_dir() {
        return Err(ProjectError::NotFound(root.to_path_buf()));
    }
    let text = match document::read_document(root, MANIFEST_FILE) {
        Ok(text) => text,
        Err(DocumentError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            return Err(ProjectError::NotFound(root.to_path_buf()));
        }
        Err(error) => return Err(error.into()),
    };
    let manifest = manifest::parse_manifest(&text)?;
    Ok(Project {
        root: root.to_path_buf(),
        manifest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        tempfile::tempdir().expect("tempdir").keep()
    }

    #[test]
    fn create_then_open_round_trips() {
        let root = temp_root();
        let created = create_project(&root, "Muestra", ProjectType::Feature, "es").expect("create");
        assert_eq!(created.manifest().schema_version, 1);
        let opened = open_project(&root).expect("open");
        assert_eq!(opened.manifest(), created.manifest());
        assert_eq!(opened.root(), root);
    }

    #[test]
    fn creates_english_template_layout() {
        let root = temp_root();
        create_project(&root, "Muestra", ProjectType::Feature, "es").expect("create");
        for dir in TEMPLATE_DIRS {
            assert!(root.join(dir).is_dir(), "missing dir {dir}");
        }
        assert!(root.join(MANIFEST_FILE).is_file());
        assert!(root.join(DEFAULT_SCREENPLAY).is_file());
        assert!(root.join("notes/decisions.md").is_file());
        assert!(root.join("notes/open-questions.md").is_file());
        // No Spanish directory names on disk (D-017).
        assert!(!root.join("guion").exists());
        assert!(!root.join("notas").exists());
    }

    #[test]
    fn create_refuses_to_overwrite_existing_project() {
        let root = temp_root();
        create_project(&root, "Uno", ProjectType::Feature, "es").expect("first");
        let error = create_project(&root, "Dos", ProjectType::Short, "en").expect_err("second");
        assert!(matches!(error, ProjectError::AlreadyExists(_)));
        let opened = open_project(&root).expect("reopen");
        assert_eq!(opened.manifest().title, "Uno");
    }

    #[test]
    fn open_missing_manifest_fails_explicitly() {
        let root = temp_root();
        let error = open_project(&root).expect_err("empty dir");
        assert!(matches!(error, ProjectError::NotFound(_)));
    }

    #[test]
    fn unsupported_schema_version_fails_and_never_rewrites() {
        let root = temp_root();
        create_project(&root, "Muestra", ProjectType::Feature, "es").expect("create");
        let manifest_path = root.join(MANIFEST_FILE);
        let original = fs::read_to_string(&manifest_path).expect("read");
        let bumped = original.replace("schema_version: 1", "schema_version: 99");
        fs::write(&manifest_path, &bumped).expect("tamper");

        let error = open_project(&root).expect_err("v99 must be rejected");
        assert!(matches!(
            error,
            ProjectError::Manifest(ManifestError::UnsupportedSchemaVersion { found: 99, .. })
        ));
        let after = fs::read_to_string(&manifest_path).expect("read back");
        assert_eq!(
            after, bumped,
            "manifest must be byte-identical after failure"
        );
    }

    #[test]
    fn manifest_has_no_absolute_or_machine_paths() {
        let root = temp_root();
        create_project(&root, "Muestra", ProjectType::Feature, "es").expect("create");
        let text = fs::read_to_string(root.join(MANIFEST_FILE)).expect("read");
        assert!(!text.contains("/Users/"), "no machine-specific paths");
        assert!(!text.contains("/home/"), "no machine-specific paths");
        assert!(!text.contains("HOME"), "no env leakage");
    }

    #[test]
    fn project_stays_readable_by_hand() {
        let root = temp_root();
        create_project(&root, "Muestra", ProjectType::Feature, "es").expect("create");
        // Plain files, no binary blobs: a normal editor sees YAML and text.
        let manifest_text = fs::read_to_string(root.join(MANIFEST_FILE)).expect("manifest");
        assert!(manifest_text.starts_with("schema_version: 1"));
        assert!(manifest_text.contains("project_type: \"feature\""));
    }

    #[test]
    fn rebuild_index_finds_scene_headings() {
        let root = temp_root();
        let project = create_project(&root, "Muestra", ProjectType::Feature, "es").expect("create");
        project
            .write_document(
                DEFAULT_SCREENPLAY,
                "INT. CASA - DÍA\n\nUn texto cualquiera.\n\nEXT. CALLE - NOCHE\n",
            )
            .expect("write screenplay");
        let index = project.rebuild_index().expect("index");
        assert_eq!(index.scenes.len(), 2);
        assert_eq!(index.scenes[0].heading, "INT. CASA - DÍA");
        assert_eq!(index.scenes[0].line_number, 1);
        assert_eq!(index.scenes[1].heading, "EXT. CALLE - NOCHE");
        assert_eq!(index.scenes[1].line_number, 5);
        assert!(
            index
                .tracked_notes
                .contains(&"notes/decisions.md".to_string())
        );
        assert!(
            index
                .tracked_notes
                .contains(&"notes/open-questions.md".to_string())
        );
    }

    #[test]
    fn rebuild_index_ignores_non_headings() {
        let root = temp_root();
        let project = create_project(&root, "Muestra", ProjectType::Feature, "es").expect("create");
        project
            .write_document(
                DEFAULT_SCREENPLAY,
                "MARÍA\n(interior, por decir algo)\n\nINTRO de canción\n",
            )
            .expect("write");
        let index = project.rebuild_index().expect("index");
        assert!(index.scenes.is_empty(), "got {:?}", index.scenes);
    }

    #[test]
    fn rebuild_index_on_empty_screenplay_is_empty() {
        let root = temp_root();
        let project = create_project(&root, "Muestra", ProjectType::Feature, "es").expect("create");
        let index = project.rebuild_index().expect("index");
        assert!(index.scenes.is_empty());
    }

    #[test]
    fn read_write_document_enforces_boundary() {
        let root = temp_root();
        let project = create_project(&root, "Muestra", ProjectType::Feature, "es").expect("create");
        let error = project
            .write_document("../escape.md", "x")
            .expect_err("escape");
        assert!(matches!(error, ProjectError::Document(_)));
    }

    #[test]
    fn list_documents_excludes_hidden_state_and_sorts() {
        let root = temp_root();
        let project = create_project(&root, "Muestra", ProjectType::Feature, "es").expect("create");

        // State that must never show up in the file explorer.
        fs::create_dir_all(root.join(".git")).expect("git dir");
        fs::write(root.join(".git/HEAD"), "ref: refs/heads/main\n").expect("git head");
        fs::create_dir_all(root.join(".narra/proposals")).expect("narra dir");
        fs::write(root.join(".narra/proposals/draft.diff"), "x\n").expect("proposal");
        fs::write(root.join(".env"), "NOPE=1\n").expect("dotfile");
        // Export artifacts are outputs, never documents (M7).
        fs::create_dir_all(root.join("exports")).expect("exports dir");
        fs::write(root.join("exports/main.pdf"), "%PDF-1.7\n").expect("pdf");

        // Narrative content, nested and unsorted on purpose.
        fs::create_dir_all(root.join("bible/characters")).expect("chars");
        fs::write(root.join("bible/characters/marta.md"), "# Marta\n").expect("marta");

        let listed = project.list_documents().expect("list");
        assert_eq!(
            listed,
            vec![
                "bible/characters/marta.md",
                "notes/decisions.md",
                "notes/open-questions.md",
                "project.yaml",
                "screenplay/main.fountain",
            ],
            "sorted, nested, no hidden entries"
        );
        assert!(listed.len() <= MAX_LISTED_DOCUMENTS);
    }

    #[test]
    fn list_documents_does_not_follow_symlinks() {
        let root = temp_root();
        let outside = temp_root();
        fs::write(outside.join("secret.md"), "fuera del proyecto\n").expect("outside file");
        let project = create_project(&root, "Muestra", ProjectType::Feature, "es").expect("create");
        std::os::unix::fs::symlink(outside.join("secret.md"), root.join("leak.md"))
            .expect("symlink");

        let listed = project.list_documents().expect("list");
        assert!(
            !listed.iter().any(|path| path == "leak.md"),
            "a symlink out of the project must not be listed: {listed:?}"
        );
    }
}
