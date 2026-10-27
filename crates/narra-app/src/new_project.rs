//! New-project rules (M10 slice 2b): title validation, the folder name
//! suggested to the native save panel, and destination safety.
//!
//! The native panel returns the exact folder path; this module only decides
//! what may be created there. A project is never written into a non-empty
//! existing directory: the template would mix with unrelated user files.

use std::fs;
use std::path::Path;

use narra_core::ProjectType;

/// Project kinds offered by the dialog, in menu order.
pub const TYPES: [ProjectType; 6] = [
    ProjectType::Feature,
    ProjectType::Series,
    ProjectType::WebSeries,
    ProjectType::Short,
    ProjectType::Microdrama,
    ProjectType::Other,
];

/// i18n key for a kind label.
pub fn type_i18n_key(kind: ProjectType) -> &'static str {
    match kind {
        ProjectType::Feature => "newproject.type.feature",
        ProjectType::Series => "newproject.type.series",
        ProjectType::WebSeries => "newproject.type.web_series",
        ProjectType::Short => "newproject.type.short",
        ProjectType::Microdrama => "newproject.type.microdrama",
        ProjectType::Other => "newproject.type.other",
    }
}

/// Trim a title, collapse inner whitespace and reject empty or control-only
/// input. Case and accents are preserved: the title is content, not a path.
pub fn clean_title(title: &str) -> Option<String> {
    let mut out = String::new();
    let mut last_space = false;
    for c in title.trim().chars() {
        if c.is_control() {
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
    if out.is_empty() { None } else { Some(out) }
}

/// Folder name suggested in the save panel. Path separators and characters
/// Windows/macOS refuse become `-`; inner whitespace is collapsed, leading
/// dots are dropped (no hidden folders) and empty results are refused. The
/// user can still rename the folder in the panel; the manifest keeps the
/// title exactly as typed.
pub fn suggested_folder(title: &str) -> Option<String> {
    let title = clean_title(title)?;
    let mut out = String::new();
    let mut last_space = false;
    for c in title.chars() {
        let c = if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
            '-'
        } else {
            c
        };
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
    let out = out.trim().trim_end_matches('.').trim_start_matches('.');
    if out.is_empty() {
        None
    } else {
        Some(out.to_string())
    }
}

/// Why a destination cannot receive a new project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestinationError {
    /// The path exists and is not a directory.
    NotADirectory,
    /// The path is an existing directory with contents; creating the template
    /// there would mix project files with unknown user files.
    NotEmpty,
}

/// Refuse unsafe destinations. A missing path is fine (the native panel
/// creates folders); an empty existing directory is fine; a non-empty one is
/// not. `create_project` separately refuses an existing manifest.
pub fn check_destination(root: &Path) -> Result<(), DestinationError> {
    if !root.exists() {
        return Ok(());
    }
    if !root.is_dir() {
        return Err(DestinationError::NotADirectory);
    }
    let empty = fs::read_dir(root)
        .map_err(|_| DestinationError::NotEmpty)?
        .next()
        .is_none();
    if empty {
        Ok(())
    } else {
        Err(DestinationError::NotEmpty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "narra-newproject-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn titles_keep_case_and_accents_and_collapse_spaces() {
        assert_eq!(
            clean_title("  La   Casa  Verde ").as_deref(),
            Some("La Casa Verde")
        );
        assert_eq!(clean_title("Ñandú").as_deref(), Some("Ñandú"));
        assert_eq!(clean_title("   "), None);
        assert_eq!(clean_title("a\u{0}b"), None);
    }

    #[test]
    fn suggested_folders_are_filesystem_safe_but_keep_the_meaning() {
        assert_eq!(
            suggested_folder("Mi: Proyecto / Norte").as_deref(),
            Some("Mi- Proyecto - Norte")
        );
        assert_eq!(suggested_folder("Ñandú").as_deref(), Some("Ñandú"));
        assert_eq!(suggested_folder(".oculto").as_deref(), Some("oculto"));
        assert_eq!(suggested_folder("..."), None);
        assert_eq!(suggested_folder("   "), None);
    }

    #[test]
    fn every_kind_has_a_label_key() {
        for kind in TYPES {
            assert!(type_i18n_key(kind).starts_with("newproject.type."));
        }
    }

    #[test]
    fn destinations_reject_files_and_non_empty_directories() {
        let dir = temp_dir("destination");
        let missing = dir.join("missing");
        assert_eq!(check_destination(&missing), Ok(()));

        let empty = dir.join("empty");
        fs::create_dir_all(&empty).expect("empty dir");
        assert_eq!(check_destination(&empty), Ok(()));

        let full = dir.join("full");
        fs::create_dir_all(&full).expect("full dir");
        fs::write(full.join("notes.txt"), "keep me").expect("write");
        assert_eq!(check_destination(&full), Err(DestinationError::NotEmpty));

        let file = dir.join("file");
        fs::write(&file, "not a directory").expect("write");
        assert_eq!(
            check_destination(&file),
            Err(DestinationError::NotADirectory)
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
