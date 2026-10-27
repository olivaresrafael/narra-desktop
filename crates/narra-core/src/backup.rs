//! Crash-safety backups (M9 slice 6).
//!
//! While the user types, the app periodically writes the buffer to
//! `<root>/.narra/backups/<document>.fountain~`. Backups are **never**
//! canonical content: the canonical file changes only through an explicit
//! save (⌘S) or a confirmed proposal apply (`PRODUCT_SPEC`). A backup is a
//! recovery aid — the user restores it into the buffer and decides whether
//! to save.
//!
//! Layout: one file per document, the project-relative path flattened with
//! `/` → `__` (so nested notes never collide), written atomically
//! (temp file + rename) so a crash mid-write cannot leave a truncated
//! backup.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Directory (project-relative) for backups.
pub const BACKUP_DIR: &str = ".narra/backups";

#[derive(Debug)]
pub enum BackupError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    /// The document path cannot be addressed safely.
    InvalidDocument(String),
}

impl std::fmt::Display for BackupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackupError::Io { path, source } => {
                write!(f, "{}: {source}", path.display())
            }
            BackupError::InvalidDocument(path) => {
                write!(f, "document path `{path}` cannot be used for a backup")
            }
        }
    }
}

impl std::error::Error for BackupError {}

/// Flatten a project-relative document path into a backup file name.
fn backup_name(document: &str) -> Option<String> {
    let document = document.trim_start_matches("./");
    if document.is_empty()
        || document.starts_with('/')
        || document.contains("..")
        || document.contains('\\')
        || document.chars().any(|c| c.is_control())
    {
        return None;
    }
    Some(format!("{}~", document.replace('/', "__")))
}

/// Path of a document's backup inside `root`.
pub fn backup_path(root: &Path, document: &str) -> Option<PathBuf> {
    backup_name(document).map(|name| root.join(BACKUP_DIR).join(name))
}

/// Write (or replace) a document's backup atomically.
pub fn write_backup(root: &Path, document: &str, text: &str) -> Result<PathBuf, BackupError> {
    let path = backup_path(root, document)
        .ok_or_else(|| BackupError::InvalidDocument(document.to_string()))?;
    let dir = root.join(BACKUP_DIR);
    fs::create_dir_all(&dir).map_err(|source| BackupError::Io {
        path: dir.clone(),
        source,
    })?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, text).map_err(|source| BackupError::Io {
        path: tmp.clone(),
        source,
    })?;
    fs::rename(&tmp, &path).map_err(|source| BackupError::Io {
        path: path.clone(),
        source,
    })?;
    Ok(path)
}

/// Read a document's backup, if one exists.
pub fn read_backup(root: &Path, document: &str) -> Result<Option<String>, BackupError> {
    let Some(path) = backup_path(root, document) else {
        return Err(BackupError::InvalidDocument(document.to_string()));
    };
    match fs::read_to_string(&path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(BackupError::Io { path, source }),
    }
}

/// Delete a document's backup (after a save or an explicit discard).
pub fn remove_backup(root: &Path, document: &str) -> Result<(), BackupError> {
    let Some(path) = backup_path(root, document) else {
        return Err(BackupError::InvalidDocument(document.to_string()));
    };
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(BackupError::Io { path, source }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_root() -> PathBuf {
        tempfile::tempdir().expect("tempdir").keep()
    }

    #[test]
    fn backups_round_trip_into_the_hidden_state_directory() {
        let root = temp_root();
        let path = write_backup(&root, "screenplay/main.fountain", "INT. CASA\n").expect("write");
        assert_eq!(path, root.join(".narra/backups/screenplay__main.fountain~"));
        assert_eq!(
            read_backup(&root, "screenplay/main.fountain").expect("read"),
            Some("INT. CASA\n".to_string())
        );
        // The canonical file is untouched: only the backup exists.
        assert!(!root.join("screenplay/main.fountain").exists());
        remove_backup(&root, "screenplay/main.fountain").expect("remove");
        assert_eq!(
            read_backup(&root, "screenplay/main.fountain").expect("read"),
            None
        );
    }

    #[test]
    fn nested_documents_do_not_collide() {
        let root = temp_root();
        write_backup(&root, "notes/a.md", "A").expect("a");
        write_backup(&root, "notes/b.md", "B").expect("b");
        assert_eq!(
            read_backup(&root, "notes/a.md").expect("read"),
            Some("A".into())
        );
        assert_eq!(
            read_backup(&root, "notes/b.md").expect("read"),
            Some("B".into())
        );
    }

    #[test]
    fn unsafe_document_paths_are_refused() {
        let root = temp_root();
        for document in ["/etc/passwd", "../escape.md", "a/../../b.md", "", "a\\b.md"] {
            assert!(
                write_backup(&root, document, "x").is_err(),
                "{document} should be refused"
            );
        }
    }

    #[test]
    fn writing_replaces_the_previous_backup_atomically() {
        let root = temp_root();
        write_backup(&root, "screenplay/main.fountain", "primera").expect("first");
        write_backup(&root, "screenplay/main.fountain", "segunda").expect("second");
        assert_eq!(
            read_backup(&root, "screenplay/main.fountain").expect("read"),
            Some("segunda".to_string())
        );
        // No temp file is left behind.
        let leftovers: Vec<_> = std::fs::read_dir(root.join(BACKUP_DIR))
            .expect("dir")
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }
}
