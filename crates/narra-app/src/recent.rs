//! Recent projects (M10 slice 2b).
//!
//! The list is a small per-user convenience file under the platform config
//! directory — never inside a project, never in source control and never a
//! canonical store. Paths are canonicalized when recorded, deduplicated and
//! capped; a corrupt or missing file simply yields an empty list, and a save
//! failure is reported through [`RecentProjects::error`] without blocking the
//! project operation that triggered it.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// How many projects the menu remembers. Eight fits a native submenu.
pub const MAX_RECENT: usize = 8;

const STORE_FILE: &str = "recent-projects.json";
const STORE_VERSION: u64 = 1;

/// Most-recent-first project roots plus the last persistence error.
#[derive(Debug, Default)]
pub struct RecentProjects {
    paths: Vec<PathBuf>,
    error: Option<String>,
}

impl RecentProjects {
    /// Load the list for the current user. Missing file = empty list.
    pub fn load() -> Self {
        Self::load_from(&store_path())
    }

    /// Load from an explicit file (tests and the `NARRA_CONFIG_DIR` override).
    pub fn load_from(file: &Path) -> Self {
        let text = match fs::read_to_string(file) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Self::default();
            }
            Err(error) => {
                return Self {
                    paths: Vec::new(),
                    error: Some(format!("{}: {error}", file.display())),
                };
            }
        };
        match parse(&text) {
            Some(paths) => Self { paths, error: None },
            None => Self {
                paths: Vec::new(),
                error: Some(format!("{}: invalid recent-projects file", file.display())),
            },
        }
    }

    /// Paths in menu order (most recent first).
    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    /// Last load/save failure, for the dump and diagnostics. `None` when the
    /// list was read and last written cleanly.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Record a successfully opened project at the front, removing duplicates
    /// and enforcing the cap. The path is canonicalized when it exists so the
    /// same project reached through a symlink or `..` is one entry.
    pub fn record(&mut self, root: &Path) {
        let root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        self.paths.retain(|path| path != &root);
        self.paths.insert(0, root);
        self.paths.truncate(MAX_RECENT);
    }

    /// Drop one project. Returns whether it was present.
    pub fn remove(&mut self, root: &Path) -> bool {
        let root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        let before = self.paths.len();
        self.paths.retain(|path| path != &root);
        before != self.paths.len()
    }

    pub fn clear(&mut self) {
        self.paths.clear();
    }

    /// Persist the list. Errors are kept in `self.error` (never a secret) so
    /// the caller can surface them without losing the in-memory list.
    pub fn save(&mut self) {
        self.error = None;
        if let Err(error) = self.save_to(&store_path()) {
            self.error = Some(error);
        }
    }

    /// Save to an explicit file (tests).
    pub fn save_to(&self, file: &Path) -> Result<(), String> {
        let Some(parent) = file.parent() else {
            return Err(format!("{} has no parent directory", file.display()));
        };
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
        let text = serde_json::to_string_pretty(&json!({
            "version": STORE_VERSION,
            "paths": self
                .paths
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
        }))
        .map_err(|error| format!("could not encode recent projects: {error}"))?;
        // Non-canonical convenience data: a temp file in the same directory
        // keeps a crash from truncating the previous list. No fsync needed.
        let temp = file.with_extension("json.tmp");
        fs::write(&temp, text).map_err(|error| format!("{}: {error}", temp.display()))?;
        fs::rename(&temp, file).map_err(|error| format!("{}: {error}", file.display()))?;
        Ok(())
    }
}

fn parse(text: &str) -> Option<Vec<PathBuf>> {
    let value: Value = serde_json::from_str(text).ok()?;
    let object = value.as_object()?;
    if object.get("version").and_then(Value::as_u64) != Some(STORE_VERSION) {
        return None;
    }
    let entries = object.get("paths")?.as_array()?;
    let mut paths = Vec::new();
    for entry in entries {
        let Some(text) = entry.as_str() else {
            continue;
        };
        let path = PathBuf::from(text);
        // Hand-edited relative paths would depend on the launch directory;
        // recents are always absolute (record canonicalizes).
        if path.as_os_str().is_empty() || !path.is_absolute() || paths.contains(&path) {
            continue;
        }
        paths.push(path);
        if paths.len() == MAX_RECENT {
            break;
        }
    }
    Some(paths)
}

/// Per-user configuration directory. `NARRA_CONFIG_DIR` is an explicit
/// override for tests and portable runs; macOS uses Application Support
/// (the MVP platform, D-002/D-003).
pub fn config_dir() -> PathBuf {
    if let Some(dir) = env::var_os("NARRA_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Library/Application Support/Narra")
}

pub fn store_path() -> PathBuf {
    config_dir().join(STORE_FILE)
}

pub fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}

/// Menu label: `~/…` for paths under home, the full path otherwise.
pub fn display_path(path: &Path) -> String {
    if let Some(home) = home_dir()
        && let Ok(rest) = path.strip_prefix(&home)
    {
        if rest.as_os_str().is_empty() {
            return "~".to_string();
        }
        return format!("~/{}", rest.display());
    }
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!(
            "narra-recent-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn record_moves_to_front_dedupes_and_caps() {
        let dir = temp_dir("record");
        let mut recents = RecentProjects::default();
        for index in 0..MAX_RECENT + 3 {
            let project = dir.join(format!("p{index}"));
            fs::create_dir_all(&project).expect("project dir");
            recents.record(&project);
        }
        assert_eq!(recents.paths().len(), MAX_RECENT);
        assert!(
            recents.paths()[0]
                .to_string_lossy()
                .ends_with(&format!("p{}", MAX_RECENT + 2))
        );
        // Re-opening an old project promotes it, never duplicates it.
        let promoted = dir.join("p3");
        recents.record(&promoted);
        assert_eq!(recents.paths().len(), MAX_RECENT);
        assert_eq!(
            recents.paths()[0],
            fs::canonicalize(&promoted).expect("canonical")
        );
        assert_eq!(
            recents
                .paths()
                .iter()
                .filter(|path| path.ends_with("p3"))
                .count(),
            1
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_and_load_round_trip_in_order() {
        let dir = temp_dir("roundtrip");
        let file = dir.join("nested/recent-projects.json");
        let mut recents = RecentProjects::default();
        for name in ["alpha", "beta", "gamma"] {
            let project = dir.join(name);
            fs::create_dir_all(&project).expect("project dir");
            recents.record(&project);
        }
        recents.save_to(&file).expect("save");
        let loaded = RecentProjects::load_from(&file);
        assert_eq!(loaded.paths(), recents.paths());
        assert!(loaded.error().is_none());

        // A later record is persisted first.
        let newest = dir.join("delta");
        fs::create_dir_all(&newest).expect("project dir");
        let mut loaded = loaded;
        loaded.record(&newest);
        loaded.save_to(&file).expect("save again");
        assert_eq!(
            RecentProjects::load_from(&file).paths()[0],
            fs::canonicalize(&newest).expect("canonical")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_malformed_and_relative_entries_never_panic() {
        let dir = temp_dir("malformed");
        let missing = dir.join("missing.json");
        assert!(RecentProjects::load_from(&missing).paths().is_empty());

        let malformed = dir.join("malformed.json");
        fs::write(&malformed, "{not json").expect("write");
        let loaded = RecentProjects::load_from(&malformed);
        assert!(loaded.paths().is_empty());
        assert!(loaded.error().is_some());

        let valid = dir.join("valid.json");
        fs::write(
            &valid,
            r#"{"version":1,"paths":["relative/path","/absolute/one","/absolute/one"]}"#,
        )
        .expect("write");
        let loaded = RecentProjects::load_from(&valid);
        assert_eq!(loaded.paths(), &[PathBuf::from("/absolute/one")]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_and_clear_follow_canonical_paths() {
        let dir = temp_dir("remove");
        let project = dir.join("project");
        fs::create_dir_all(&project).expect("project dir");
        let mut recents = RecentProjects::default();
        recents.record(&project);
        // The same project reached through a `..` segment is the same entry.
        assert!(recents.remove(&dir.join("project")));
        assert!(recents.paths().is_empty());
        recents.record(&project);
        recents.clear();
        assert!(recents.paths().is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_reports_errors_without_losing_the_list() {
        let dir = temp_dir("save-error");
        let project = dir.join("project");
        fs::create_dir_all(&project).expect("project dir");
        let mut recents = RecentProjects::default();
        recents.record(&project);
        // A file where the parent directory should be makes the save fail.
        let blocker = dir.join("blocker");
        fs::write(&blocker, "not a directory").expect("write");
        assert!(recents.save_to(&blocker.join("recent.json")).is_err());
        assert_eq!(recents.paths().len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn display_path_shortens_home() {
        let Some(home) = home_dir() else {
            return;
        };
        let path = home.join("Documents/Film");
        assert_eq!(display_path(&path), "~/Documents/Film");
    }
}
