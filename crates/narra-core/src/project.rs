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
/// Versioned project-level instructions for the assistant (D-047).
pub const INSTRUCTIONS_FILE: &str = ".narra/instructions.md";
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

    #[error("project type package: {0}")]
    TypePackage(#[from] crate::project_type::TypePackageError),

    #[error("failed to access `{path}`: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("the default screenplay cannot be deleted; choose another default screenplay first")]
    CannotDeleteDefaultScreenplay,
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

    /// Create a new document, refusing to overwrite an existing one.
    pub fn create_document(&self, relative: &str, contents: &str) -> Result<(), ProjectError> {
        Ok(document::create_document(&self.root, relative, contents)?)
    }

    /// Replace `project.yaml` from a validated manifest (M10 S4c).
    ///
    /// The caller builds the manifest from the Project Settings form; the
    /// serializer validates it before anything is written, and the document
    /// write is atomic. Unknown/invalid data can never be introduced through
    /// this path, and a failed write leaves the previous manifest intact.
    pub fn update_manifest(&mut self, manifest: Manifest) -> Result<(), ProjectError> {
        let text = manifest::serialize_manifest(&manifest)?;
        document::write_document(&self.root, MANIFEST_FILE, &text)?;
        self.manifest = manifest;
        Ok(())
    }

    /// Rename a document and, when it is the default screenplay, update the
    /// manifest atomically after the file move. A manifest failure rolls the
    /// file rename back so the project never points at a missing screenplay.
    pub fn rename_document(&mut self, from: &str, to: &str) -> Result<(), ProjectError> {
        document::rename_document(&self.root, from, to)?;
        if self.manifest.default_screenplay_path == from {
            let mut manifest = self.manifest.clone();
            manifest.default_screenplay_path = to.to_string();
            if let Err(error) = self.update_manifest(manifest) {
                let _ = document::rename_document(&self.root, to, from);
                return Err(error);
            }
        }
        Ok(())
    }

    /// Duplicate a document to a new project-relative path. Never overwrites
    /// and never follows a path outside the project.
    pub fn duplicate_document(&self, from: &str, to: &str) -> Result<(), ProjectError> {
        let text = self.read_document(from)?;
        self.create_document(to, &text)
    }

    /// Delete a document. The default screenplay is protected because losing
    /// it would leave the project opening into a missing path.
    pub fn delete_document(&self, relative: &str) -> Result<(), ProjectError> {
        if relative == self.manifest.default_screenplay_path {
            return Err(ProjectError::CannotDeleteDefaultScreenplay);
        }
        Ok(document::delete_document(&self.root, relative)?)
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
    write_project_instructions(root, primary_language)?;

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

/// Create a project from a built-in type package (M17 slice 1, D-045). The
/// package owns the directories and starter files, so the resulting tree is
/// byte-identical to the previous `create_project` + `seed_basic_template`
/// path. Creation never overwrites an existing manifest.
pub fn create_project_from_type(
    root: &Path,
    title: &str,
    project_type: ProjectType,
    primary_language: &str,
) -> Result<Project, ProjectError> {
    let package =
        crate::project_type::builtin(project_type.as_str()).expect("built-in type package");
    create_project_from_package(root, title, &package, primary_language)
}

/// Create a project from any loaded package, including a user or project
/// package with a custom id. The package is validated before the manifest is
/// written; a failure after creation leaves no partial manifest.
pub fn create_project_from_package(
    root: &Path,
    title: &str,
    package: &crate::project_type::ProjectTypePackage,
    primary_language: &str,
) -> Result<Project, ProjectError> {
    let project = create_project_base_from_package(root, title, package, primary_language)?;
    crate::project_type::write_template(root, package, primary_language)?;
    Ok(project)
}

/// Create only the manifest and package directories, leaving the starter
/// files for a reviewed AI plan. Used by the AI project base so the plan can
/// create its own `bible/` or `structure/` files create-only.
pub fn create_project_base_from_package(
    root: &Path,
    title: &str,
    package: &crate::project_type::ProjectTypePackage,
    primary_language: &str,
) -> Result<Project, ProjectError> {
    package.validate()?;
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
    for directory in &package.directories {
        let path = root.join(directory);
        fs::create_dir_all(&path).map_err(|source| ProjectError::Io {
            path: path.clone(),
            source,
        })?;
    }
    // A user/project package travels with the project so it still opens on
    // another machine. Built-ins are embedded and referenced by id.
    if package.origin != crate::project_type::TypeOrigin::Builtin
        && let Some(source) = &package.source_dir
    {
        let destination = root
            .join(crate::project_type::PROJECT_TYPES_DIR)
            .join(&package.id);
        copy_directory(source, &destination)?;
    }
    let manifest =
        Manifest::new_v1_with_type_id(title, &package.id, primary_language, DEFAULT_SCREENPLAY)?;
    let text = manifest::serialize_manifest(&manifest)?;
    document::write_document(root, MANIFEST_FILE, &text)?;
    write_project_instructions(root, primary_language)?;
    Ok(Project {
        root: root.to_path_buf(),
        manifest,
    })
}

/// A new project starts with a small, editable instruction file. It is
/// create-only and lives in versioned `.narra/` config, not workflow state.
fn write_project_instructions(root: &Path, primary_language: &str) -> Result<(), ProjectError> {
    let text = if primary_language.starts_with("es") {
        "# Instrucciones del proyecto\n\nIndica al asistente cómo ayudar en este proyecto: tono, reglas de canon, qué evitar y qué documentos son prioritarios.\n"
    } else {
        "# Project instructions\n\nTell the assistant how to help with this project: tone, canon rules, what to avoid, and which documents matter most.\n"
    };
    document::create_document(root, INSTRUCTIONS_FILE, text)?;
    Ok(())
}

fn copy_directory(source: &Path, destination: &Path) -> Result<(), ProjectError> {
    fs::create_dir_all(destination).map_err(|error| ProjectError::Io {
        path: destination.to_path_buf(),
        source: error,
    })?;
    for entry in fs::read_dir(source).map_err(|error| ProjectError::Io {
        path: source.to_path_buf(),
        source: error,
    })? {
        let entry = entry.map_err(|error| ProjectError::Io {
            path: source.to_path_buf(),
            source: error,
        })?;
        let target = destination.join(entry.file_name());
        if entry
            .file_type()
            .map_err(|error| ProjectError::Io {
                path: entry.path(),
                source: error,
            })?
            .is_dir()
        {
            copy_directory(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target).map_err(|error| ProjectError::Io {
                path: target,
                source: error,
            })?;
        }
    }
    Ok(())
}

/// Starter documents for the New Project **basic template** (M16 slice 8,
/// D-044). Every file is create-only: an existing file is an explicit error
/// and is never overwritten. The entity folders match the bible layout.
pub fn seed_basic_template(root: &Path, primary_language: &str) -> Result<(), ProjectError> {
    let (premise, outline) = basic_template_documents(primary_language);
    document::create_document(root, "bible/premise.md", premise)?;
    document::create_document(root, "structure/outline.md", outline)?;
    for dir in ["bible/characters", "bible/locations", "bible/objects"] {
        fs::create_dir_all(root.join(dir)).map_err(|source| ProjectError::Io {
            path: root.join(dir),
            source,
        })?;
    }
    Ok(())
}

/// Localized starter files for [`seed_basic_template`]. Unknown languages
/// fall back to English, matching the manifest's English on-disk convention.
/// The text lives in the project-type package module so package templates and
/// the legacy seeder cannot drift.
pub fn basic_template_documents(primary_language: &str) -> (&'static str, &'static str) {
    crate::project_type::basic_template_documents(primary_language)
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
    fn basic_template_seeds_localized_docs_and_never_overwrites() {
        let root = temp_root();
        create_project(&root, "Muestra", ProjectType::Feature, "es").expect("create");
        seed_basic_template(&root, "es").expect("seed");
        let premise = std::fs::read_to_string(root.join("bible/premise.md")).expect("premise");
        assert!(premise.starts_with("# Premisa"), "{premise}");
        assert!(root.join("structure/outline.md").is_file());
        assert!(root.join("bible/locations").is_dir());

        let error = seed_basic_template(&root, "es").expect_err("no overwrite");
        assert!(error.to_string().contains("premise.md"), "{error}");
        assert_eq!(
            std::fs::read_to_string(root.join("bible/premise.md")).expect("premise"),
            premise
        );
    }

    #[test]
    fn basic_template_uses_english_for_en_or_unknown_languages() {
        let (premise, outline) = basic_template_documents("en");
        assert!(premise.contains("Premise"), "{premise}");
        assert!(outline.contains("Outline"), "{outline}");
        let (fallback, _) = basic_template_documents("fr");
        assert!(fallback.contains("Premise"), "{fallback}");
    }

    #[test]
    fn type_packages_match_the_legacy_starter_tree() {
        use std::collections::BTreeMap;
        fn snapshot(root: &Path) -> BTreeMap<String, String> {
            fn walk(base: &Path, current: &Path, out: &mut BTreeMap<String, String>) {
                for entry in fs::read_dir(current).expect("read dir") {
                    let entry = entry.expect("entry");
                    let path = entry.path();
                    let relative = path
                        .strip_prefix(base)
                        .expect("relative")
                        .to_string_lossy()
                        .replace('\\', "/");
                    if relative == MANIFEST_FILE {
                        continue;
                    }
                    if entry.file_type().expect("type").is_dir() {
                        out.insert(format!("{relative}/"), String::new());
                        walk(base, &path, out);
                    } else {
                        out.insert(
                            relative,
                            fs::read_to_string(&path).expect("read starter file"),
                        );
                    }
                }
            }
            let mut out = BTreeMap::new();
            walk(root, root, &mut out);
            out
        }

        let kinds = [
            ProjectType::Feature,
            ProjectType::Series,
            ProjectType::WebSeries,
            ProjectType::Short,
            ProjectType::Microdrama,
            ProjectType::Other,
        ];
        for kind in kinds {
            for language in ["en", "es"] {
                let legacy = temp_root();
                create_project(&legacy, "Muestra", kind, language).expect("legacy create");
                seed_basic_template(&legacy, language).expect("legacy seed");
                let packaged = temp_root();
                create_project_from_type(&packaged, "Muestra", kind, language)
                    .expect("package create");
                assert_eq!(
                    snapshot(&legacy),
                    snapshot(&packaged),
                    "starter tree mismatch for {} ({language})",
                    kind.as_str()
                );
            }
        }
    }

    #[test]
    fn new_projects_get_project_instructions() {
        let root = temp_root();
        create_project(&root, "Instrucciones", ProjectType::Feature, "es").expect("create");
        let text = fs::read_to_string(root.join(INSTRUCTIONS_FILE)).expect("instructions");
        assert!(text.starts_with("# Instrucciones del proyecto"));
    }

    #[test]
    fn custom_package_creates_and_reopens_a_custom_type_id() {
        let root = temp_root();
        let package = crate::project_type::parse_package(
            "schema_version: 1\napi_version: 1\nid: documentary\nname: Documentary\ndirectories: [docs]\nfiles:\n  - path: docs/brief.md\n    content:\n      en: Brief\n      es: Guion\ncapabilities:\n  screenplay: false\n  export_pdf: false\n",
            crate::project_type::TypeOrigin::User,
        )
        .expect("package");
        let project = create_project_from_package(&root, "Documental", &package, "es")
            .expect("create custom package");
        assert_eq!(project.manifest().project_type_id, "documentary");
        assert_eq!(project.manifest().project_type, ProjectType::Other);
        assert_eq!(
            fs::read_to_string(root.join("docs/brief.md")).expect("brief"),
            "Guion"
        );
        let reopened = open_project(&root).expect("reopen");
        assert_eq!(reopened.manifest().project_type_id, "documentary");
    }

    #[test]
    fn user_type_package_is_copied_into_the_project() {
        let source = temp_root();
        let package_dir = source.join("documentary");
        fs::create_dir_all(&package_dir).expect("package dir");
        fs::write(
            package_dir.join(crate::project_type::TYPE_FILE),
            "schema_version: 1\napi_version: 1\nid: documentary\nname: Documentary\ndirectories: [docs]\nfiles:\n  - path: docs/brief.md\n    content: Brief\n",
        )
        .expect("type.yaml");
        let package =
            crate::project_type::load_package(&package_dir, crate::project_type::TypeOrigin::User)
                .expect("package");
        let root = temp_root();
        create_project_from_package(&root, "Documental", &package, "es").expect("create");
        assert!(
            root.join(".narra/types/documentary/type.yaml").is_file(),
            "package should travel with the project"
        );
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
    fn update_manifest_validates_and_preserves_identity() {
        let root = temp_root();
        let mut project =
            create_project(&root, "Antes", ProjectType::Feature, "es").expect("create");
        let id = project.manifest().project_id;
        let created = project.manifest().created_at.clone();
        let mut manifest = project.manifest().clone();
        manifest.title = "Después".to_string();
        manifest.project_type = ProjectType::Series;
        manifest.project_type_id = "series".to_string();
        project.update_manifest(manifest).expect("update");
        let reopened = open_project(&root).expect("reopen");
        assert_eq!(reopened.manifest().title, "Después");
        assert_eq!(reopened.manifest().project_type, ProjectType::Series);
        assert_eq!(reopened.manifest().project_id, id);
        assert_eq!(reopened.manifest().created_at, created);

        // Invalid manifests are rejected before writing: the file keeps the
        // last valid content.
        let mut invalid = project.manifest().clone();
        invalid.primary_language = "not a tag!".to_string();
        assert!(project.update_manifest(invalid).is_err());
        let again = open_project(&root).expect("reopen after rejection");
        assert_eq!(again.manifest().title, "Después");
    }

    #[test]
    fn rename_updates_the_default_screenplay_and_never_overwrites() {
        let root = temp_root();
        let mut project =
            create_project(&root, "Serie", ProjectType::Series, "es").expect("create");
        project
            .write_document("screenplay/S01E01.fountain", "INT. CASA - DÍA\n")
            .expect("write");
        let mut manifest = project.manifest().clone();
        manifest.default_screenplay_path = "screenplay/S01E01.fountain".to_string();
        project.update_manifest(manifest).expect("manifest");
        project
            .rename_document(
                "screenplay/S01E01.fountain",
                "screenplay/S01E01 Piloto.fountain",
            )
            .expect("rename");
        assert_eq!(
            project.manifest().default_screenplay_path,
            "screenplay/S01E01 Piloto.fountain"
        );
        assert!(root.join("screenplay/S01E01 Piloto.fountain").is_file());
        assert!(!root.join("screenplay/S01E01.fountain").exists());
        let reopened = open_project(&root).expect("reopen");
        assert_eq!(
            reopened.manifest().default_screenplay_path,
            "screenplay/S01E01 Piloto.fountain"
        );

        project
            .write_document("screenplay/other.fountain", "")
            .expect("other");
        assert!(
            project
                .rename_document(
                    "screenplay/S01E01 Piloto.fountain",
                    "screenplay/other.fountain"
                )
                .is_err()
        );
        assert!(root.join("screenplay/S01E01 Piloto.fountain").is_file());
    }

    #[test]
    fn duplicate_and_delete_are_bounded_and_protect_the_default_screenplay() {
        let root = temp_root();
        let project = create_project(&root, "Serie", ProjectType::Series, "es").expect("create");
        project
            .write_document("notes/idea.md", "hola\n")
            .expect("write");
        project
            .duplicate_document("notes/idea.md", "notes/idea copia.md")
            .expect("duplicate");
        assert_eq!(
            project.read_document("notes/idea copia.md").expect("read"),
            "hola\n"
        );
        assert!(
            project
                .duplicate_document("notes/idea.md", "notes/idea copia.md")
                .is_err(),
            "duplicate never overwrites"
        );
        assert!(
            project
                .delete_document(project.manifest().default_screenplay_path.as_str())
                .is_err(),
            "the default screenplay is protected"
        );
        project
            .delete_document("notes/idea copia.md")
            .expect("delete");
        assert!(!root.join("notes/idea copia.md").exists());
        assert!(root.join("notes/idea.md").is_file());
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
