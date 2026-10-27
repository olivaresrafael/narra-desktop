//! Project-boundary-safe filesystem operations.
//!
//! Every read or write of narrative content goes through these helpers:
//! - paths are validated against the project root (no `..`, no absolute
//!   paths, no symlink escape outside the root);
//! - writes are crash-safe: content lands in a temp file in the target
//!   directory and is atomically renamed over the destination.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use thiserror::Error;

/// Failures from boundary-checked document IO.
#[derive(Debug, Error)]
pub enum DocumentError {
    #[error("path `{0}` is outside the project: absolute paths are not allowed")]
    AbsolutePath(String),

    #[error("path `{0}` is outside the project: `..` traversal is not allowed")]
    Traversal(String),

    #[error("path `{0}` resolves outside the project root (symlink escape)")]
    SymlinkEscape(String),

    #[error("path `{0}` is not valid UTF-8")]
    NonUtf8Path(String),

    #[error("document `{0}` already exists (new documents never overwrite)")]
    AlreadyExists(String),

    #[error("failed to access `{path}`: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Resolve a caller-supplied relative path against `root`, refusing anything
/// that could leave the project boundary.
///
/// Rejections are explicit errors; nothing is normalized silently.
pub fn resolve_within(root: &Path, relative: &str) -> Result<PathBuf, DocumentError> {
    let raw = Path::new(relative);
    if raw.is_absolute() {
        return Err(DocumentError::AbsolutePath(relative.to_string()));
    }
    if raw.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(DocumentError::Traversal(relative.to_string()));
    }

    let joined = root.join(raw);

    // Canonicalize the deepest existing ancestor so a symlink inside the
    // project that points outside it is rejected before any IO happens.
    let mut existing = joined.as_path();
    loop {
        match fs::canonicalize(existing) {
            Ok(canonical) => {
                let canonical_root =
                    fs::canonicalize(root).map_err(|source| DocumentError::Io {
                        path: root.to_path_buf(),
                        source,
                    })?;
                if !canonical.starts_with(&canonical_root) {
                    return Err(DocumentError::SymlinkEscape(relative.to_string()));
                }
                break;
            }
            Err(_) => match existing.parent() {
                Some(parent) => existing = parent,
                None => break,
            },
        }
    }

    Ok(joined)
}

/// Read a UTF-8 document that lives inside the project.
pub fn read_document(root: &Path, relative: &str) -> Result<String, DocumentError> {
    let path = resolve_within(root, relative)?;
    fs::read_to_string(&path).map_err(|source| DocumentError::Io {
        path: relative.to_string().into(),
        source,
    })
}

/// Create a **new** document, refusing to overwrite an existing path.
///
/// Unlike [`write_document`] this is not a temp-and-rename: the file is
/// opened with `create_new`, which the filesystem makes atomic with respect
/// to existence (two creators cannot both succeed). A crash mid-write can
/// leave a partially written *new* file; templates are tiny and the user
/// can delete it, whereas silently replacing an existing document would be
/// data loss.
pub fn create_document(root: &Path, relative: &str, contents: &str) -> Result<(), DocumentError> {
    use std::io::Write;

    let path = resolve_within(root, relative)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| DocumentError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            return Err(DocumentError::AlreadyExists(relative.to_string()));
        }
        Err(source) => return Err(DocumentError::Io { path, source }),
    };
    file.write_all(contents.as_bytes())
        .map_err(|source| DocumentError::Io {
            path: path.clone(),
            source,
        })?;
    file.sync_all()
        .map_err(|source| DocumentError::Io { path, source })?;
    Ok(())
}

/// Write a document inside the project atomically (temp file + rename).
///
/// The temp file lives in the destination directory so the rename stays on the
/// same volume; a crash mid-write leaves the previous contents intact.
pub fn write_document(root: &Path, relative: &str, contents: &str) -> Result<(), DocumentError> {
    let path = resolve_within(root, relative)?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| DocumentError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| DocumentError::NonUtf8Path(relative.to_string()))?;
    let temp = path.with_file_name(format!(".{file_name}.narra-tmp"));

    fs::write(&temp, contents).map_err(|source| DocumentError::Io {
        path: temp.clone(),
        source,
    })?;
    if let Err(source) = fs::rename(&temp, &path) {
        let _ = fs::remove_file(&temp);
        return Err(DocumentError::Io {
            path: path.clone(),
            source,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        // Tests keep their directory alive for their own duration.
        tempfile::tempdir().expect("tempdir").keep()
    }

    #[test]
    fn writes_and_reads_round_trip() {
        let root = root();
        write_document(&root, "screenplay/main.fountain", "INT. ROOM - DAY\n").expect("write");
        let read = read_document(&root, "screenplay/main.fountain").expect("read");
        assert_eq!(read, "INT. ROOM - DAY\n");
    }

    #[test]
    fn creates_missing_parent_directories() {
        let root = root();
        write_document(&root, "notes/decisions.md", "# Decisions\n").expect("write");
        assert!(root.join("notes/decisions.md").is_file());
    }

    #[test]
    fn overwrites_atomically_without_leaving_temp_files() {
        let root = root();
        write_document(&root, "a.md", "one").expect("first write");
        write_document(&root, "a.md", "two").expect("second write");
        assert_eq!(read_document(&root, "a.md").expect("read"), "two");
        let leftovers: Vec<_> = fs::read_dir(&root)
            .expect("read_dir")
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("narra-tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp files left behind: {leftovers:?}"
        );
    }

    #[test]
    fn rejects_absolute_paths() {
        let root = root();
        let error = write_document(&root, "/etc/hosts", "x").expect_err("absolute");
        assert!(matches!(error, DocumentError::AbsolutePath(_)));
    }

    #[test]
    fn rejects_parent_traversal() {
        let root = root();
        let error = read_document(&root, "../../etc/passwd").expect_err("traversal");
        assert!(matches!(error, DocumentError::Traversal(_)));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape_on_write() {
        use std::os::unix::fs::symlink;
        let root = root();
        let outside = tempfile::tempdir().expect("outside");
        symlink(outside.path(), root.join("leak")).expect("symlink");
        let error = write_document(&root, "leak/escape.md", "x").expect_err("escape");
        assert!(matches!(error, DocumentError::SymlinkEscape(_)));
        assert!(!outside.path().join("escape.md").exists());
    }

    #[cfg(unix)]
    #[test]
    fn allows_symlinks_that_stay_inside_the_project() {
        use std::os::unix::fs::symlink;
        let root = root();
        fs::create_dir_all(root.join("notes")).expect("mkdir");
        symlink(root.join("notes"), root.join("alias")).expect("symlink");
        write_document(&root, "alias/inside.md", "ok").expect("write inside symlink");
        assert!(root.join("notes/inside.md").is_file());
    }

    #[test]
    fn reads_existing_files_written_by_hand() {
        let root = root();
        fs::write(root.join("project.yaml"), "schema_version: 1\n").expect("hand write");
        assert_eq!(
            read_document(&root, "project.yaml").expect("read"),
            "schema_version: 1\n"
        );
    }
}

#[cfg(test)]
mod create_tests {
    use super::*;

    fn temp_root() -> PathBuf {
        tempfile::tempdir().expect("tempdir").keep()
    }

    #[test]
    fn create_writes_and_never_overwrites() {
        let root = temp_root();
        create_document(&root, "bible/characters/marta.md", "# Marta\n").expect("create");
        assert_eq!(
            fs::read_to_string(root.join("bible/characters/marta.md")).expect("read"),
            "# Marta\n"
        );
        // A second create with different content is refused and leaves the
        // original bytes untouched.
        let error =
            create_document(&root, "bible/characters/marta.md", "# Otra\n").expect_err("exists");
        assert!(matches!(error, DocumentError::AlreadyExists(_)));
        assert_eq!(
            fs::read_to_string(root.join("bible/characters/marta.md")).expect("read"),
            "# Marta\n"
        );
    }

    #[test]
    fn create_keeps_the_boundary_rules() {
        let root = temp_root();
        assert!(create_document(&root, "/tmp/escape.md", "x").is_err());
        assert!(create_document(&root, "../escape.md", "x").is_err());
        assert!(!root.join("escape.md").exists());
    }
}
