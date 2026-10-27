//! Explorer tree (M9 slice 4).
//!
//! `Project::list_documents()` returns sorted, project-relative paths. The
//! explorer renders them as folders and files: folders first, then names,
//! each level alphabetical (case-insensitive), descendants of collapsed
//! folders hidden. The tree is rebuilt from the flat list on demand — the
//! list is bounded (200 documents), so no persistent index is needed.

use std::collections::{BTreeSet, HashSet};

/// File kind, used for the explorer's icon (M10 S3c).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Fountain,
    Markdown,
    Yaml,
    Other,
}

impl FileKind {
    pub fn from_path(path: &str) -> FileKind {
        match path.rsplit('.').next() {
            Some("fountain") => FileKind::Fountain,
            Some("md") => FileKind::Markdown,
            Some("yaml") | Some("yml") => FileKind::Yaml,
            _ => FileKind::Other,
        }
    }

    /// Lucide asset name (without the `icons/` prefix or `.svg` suffix).
    pub fn icon_name(self) -> &'static str {
        match self {
            FileKind::Fountain => "clapperboard",
            FileKind::Markdown => "file-text",
            FileKind::Yaml => "sliders-horizontal",
            FileKind::Other => "file",
        }
    }
}

/// Folder category, used for the explorer's icon (M10 S3c). The canonical
/// project layout is fixed (D-017); unknown folders stay generic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderKind {
    Bible,
    Characters,
    Locations,
    Objects,
    Screenplay,
    Structure,
    Research,
    Notes,
    Reviews,
    Generic,
}

impl FolderKind {
    pub fn from_path(path: &str) -> FolderKind {
        match path {
            "bible" => FolderKind::Bible,
            "bible/characters" => FolderKind::Characters,
            "bible/locations" => FolderKind::Locations,
            "bible/objects" => FolderKind::Objects,
            "screenplay" => FolderKind::Screenplay,
            "structure" => FolderKind::Structure,
            "research" => FolderKind::Research,
            "notes" => FolderKind::Notes,
            "reviews" => FolderKind::Reviews,
            _ => FolderKind::Generic,
        }
    }

    /// Lucide asset name (without the `icons/` prefix or `.svg` suffix).
    pub fn icon_name(self) -> &'static str {
        match self {
            FolderKind::Bible => "book-open",
            FolderKind::Characters => "users",
            FolderKind::Locations => "map-pin",
            FolderKind::Objects => "package",
            FolderKind::Screenplay => "clapperboard",
            FolderKind::Structure => "list-tree",
            FolderKind::Research => "search",
            FolderKind::Notes => "notebook-pen",
            FolderKind::Reviews => "square-check-big",
            FolderKind::Generic => "folder",
        }
    }
}

/// One rendered row: a folder (with its collapsed state) or a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Project-relative path (for folders, their directory path).
    pub path: String,
    /// Display name (last path component).
    pub name: String,
    /// Nesting depth (0 = project root).
    pub depth: usize,
    pub is_dir: bool,
    /// Folders only.
    pub collapsed: bool,
    /// Files only (`Other` for folders).
    pub kind: FileKind,
    /// Folders only (`Generic` for files).
    pub folder_kind: FolderKind,
}

/// Build the visible rows for `paths`, honoring `collapsed` folders.
pub fn build_rows(paths: &[String], collapsed: &HashSet<String>) -> Vec<Row> {
    let mut dirs: BTreeSet<String> = BTreeSet::new();
    let mut files: Vec<&str> = Vec::new();
    for path in paths {
        files.push(path.as_str());
        // Every ancestor directory of every document.
        let mut prefix = path.as_str();
        while let Some((parent, _)) = prefix.rsplit_once('/') {
            dirs.insert(parent.to_string());
            prefix = parent;
        }
    }

    let mut rows = Vec::new();
    emit("", 0, &dirs, &files, collapsed, &mut rows);
    rows
}

/// Project-internal files hidden from the explorer by default (M10 S4c).
/// `.narra/` is never listed by `Project::list_documents`; the explicit
/// prefix check keeps this rule honest if that changes.
pub fn is_config_path(path: &str) -> bool {
    matches!(
        path,
        "project.yaml" | "agents.yaml" | "AGENTS.md" | ".gitignore"
    ) || path.starts_with(".narra/")
}

/// Documents the writer should see: narrative content only, unless the user
/// explicitly opted into seeing configuration files.
pub fn visible_paths(paths: &[String], show_config_files: bool) -> Vec<String> {
    paths
        .iter()
        .filter(|path| show_config_files || !is_config_path(path))
        .cloned()
        .collect()
}

/// Parent directory of a project-relative path (`""` at the root).
fn parent_of(path: &str) -> &str {
    path.rsplit_once('/')
        .map(|(parent, _)| parent)
        .unwrap_or("")
}

/// Last path component (the display name).
fn name_of(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

fn emit(
    prefix: &str,
    depth: usize,
    dirs: &BTreeSet<String>,
    files: &[&str],
    collapsed: &HashSet<String>,
    rows: &mut Vec<Row>,
) {
    let mut child_dirs: Vec<&String> = dirs.iter().filter(|dir| parent_of(dir) == prefix).collect();
    let mut child_files: Vec<&&str> = files
        .iter()
        .filter(|file| parent_of(file) == prefix)
        .collect();
    child_dirs.sort_by_key(|dir| name_of(dir).to_lowercase());
    child_files.sort_by_key(|file| name_of(file).to_lowercase());

    for dir in child_dirs {
        let is_collapsed = collapsed.contains(dir);
        rows.push(Row {
            path: dir.clone(),
            name: name_of(dir),
            depth,
            is_dir: true,
            collapsed: is_collapsed,
            kind: FileKind::Other,
            folder_kind: FolderKind::from_path(dir),
        });
        if !is_collapsed {
            emit(dir, depth + 1, dirs, files, collapsed, rows);
        }
    }
    for file in child_files {
        rows.push(Row {
            path: file.to_string(),
            name: name_of(file),
            depth,
            is_dir: false,
            collapsed: false,
            kind: FileKind::from_path(file),
            folder_kind: FolderKind::Generic,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths() -> Vec<String> {
        vec![
            "agents.yaml".to_string(),
            "bible/characters/marta.md".to_string(),
            "bible/world.md".to_string(),
            "project.yaml".to_string(),
            "screenplay/main.fountain".to_string(),
        ]
    }

    #[test]
    fn folders_first_then_files_at_every_level() {
        let rows = build_rows(&paths(), &HashSet::new());
        let rendered: Vec<(usize, &str)> =
            rows.iter().map(|r| (r.depth, r.name.as_str())).collect();
        assert_eq!(
            rendered,
            vec![
                (0, "bible"),
                (1, "characters"),
                (2, "marta.md"),
                (1, "world.md"),
                (0, "screenplay"),
                (1, "main.fountain"),
                (0, "agents.yaml"),
                (0, "project.yaml"),
            ]
        );
    }

    #[test]
    fn collapsed_folders_hide_their_descendants() {
        let mut collapsed = HashSet::new();
        collapsed.insert("bible".to_string());
        let rows = build_rows(&paths(), &collapsed);
        assert!(rows.iter().any(|row| row.path == "bible" && row.collapsed));
        assert!(!rows.iter().any(|row| row.path.starts_with("bible/")));
        // Root files still follow.
        assert_eq!(
            rows.last().map(|row| row.name.as_str()),
            Some("project.yaml")
        );

        // Collapsing a nested folder hides only its own children.
        collapsed.insert("screenplay".to_string());
        let rows = build_rows(&paths(), &collapsed);
        assert!(
            !rows
                .iter()
                .any(|row| row.path == "screenplay/main.fountain")
        );
    }

    #[test]
    fn folder_kinds_pick_the_category_icons() {
        assert_eq!(FolderKind::from_path("bible"), FolderKind::Bible);
        assert_eq!(
            FolderKind::from_path("bible/characters"),
            FolderKind::Characters
        );
        assert_eq!(
            FolderKind::from_path("bible/locations"),
            FolderKind::Locations
        );
        assert_eq!(FolderKind::from_path("bible/objects"), FolderKind::Objects);
        assert_eq!(FolderKind::from_path("screenplay"), FolderKind::Screenplay);
        assert_eq!(FolderKind::from_path("notes"), FolderKind::Notes);
        assert_eq!(FolderKind::from_path("notes/nested"), FolderKind::Generic);
        assert_eq!(FolderKind::Characters.icon_name(), "users");
        assert_eq!(FolderKind::Generic.icon_name(), "folder");
    }

    #[test]
    fn file_kinds_drive_the_explorer_icons() {
        assert_eq!(
            FileKind::from_path("screenplay/main.fountain"),
            FileKind::Fountain
        );
        assert_eq!(
            FileKind::from_path("notes/decisions.md"),
            FileKind::Markdown
        );
        assert_eq!(FileKind::from_path("agents.yaml"), FileKind::Yaml);
        assert_eq!(FileKind::from_path("project.yaml"), FileKind::Yaml);
        assert_eq!(FileKind::from_path("exports/main.pdf"), FileKind::Other);
        assert_eq!(FileKind::from_path("README"), FileKind::Other);
        assert_eq!(FileKind::Markdown.icon_name(), "file-text");
        assert_eq!(FileKind::Fountain.icon_name(), "clapperboard");
        assert_eq!(FileKind::Yaml.icon_name(), "sliders-horizontal");
        assert_eq!(FileKind::Other.icon_name(), "file");
    }

    #[test]
    fn config_paths_are_hidden_unless_requested() {
        let paths = vec![
            "AGENTS.md".to_string(),
            "agents.yaml".to_string(),
            "bible/world.md".to_string(),
            "project.yaml".to_string(),
            ".narra/backups/x~".to_string(),
            "screenplay/main.fountain".to_string(),
        ];
        let visible = visible_paths(&paths, false);
        assert_eq!(
            visible,
            vec![
                "bible/world.md".to_string(),
                "screenplay/main.fountain".to_string()
            ]
        );
        assert_eq!(visible_paths(&paths, true), paths);
        assert!(is_config_path("project.yaml"));
        assert!(!is_config_path("notes/decisions.md"));
    }

    #[test]
    fn shared_prefixes_produce_one_folder_row() {
        let rows = build_rows(
            &[
                "notes/a.md".to_string(),
                "notes/b.md".to_string(),
                "notes/deep/c.md".to_string(),
            ],
            &HashSet::new(),
        );
        assert_eq!(
            rows.iter().filter(|row| row.path == "notes").count(),
            1,
            "one row per folder"
        );
    }
}
