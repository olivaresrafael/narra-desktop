//! Local Git status for the explorer and the bottom bar (M10 S3c).
//!
//! Read-only and local: `git status` runs through `narra-core::git` with a
//! timeout, never network. The UI consumes a flat map of project-relative
//! paths plus aggregated folder statuses; no commit/push/merge lives here.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use narra_core::git::{BranchInfo, OpOptions, Repository, StatusEntry, WorktreeStatus};

const STATUS_TIMEOUT: Duration = Duration::from_secs(5);

/// Git state for one project. `available == false` means “not a repository”
/// (or Git is unavailable); `error` carries the actionable reason when the
/// refresh failed after detecting a repository.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitStatus {
    pub available: bool,
    pub branch: String,
    pub ahead: u32,
    pub behind: u32,
    /// Changed files, project-relative path → highest status.
    pub files: HashMap<String, WorktreeStatus>,
    /// Folders containing changed files, with an aggregated status.
    pub dirs: HashMap<String, WorktreeStatus>,
    pub error: String,
}

impl GitStatus {
    pub fn from_status(branch: Option<BranchInfo>, entries: Vec<StatusEntry>) -> Self {
        let (branch, ahead, behind) = branch
            .map(|branch| (branch.head.unwrap_or_default(), branch.ahead, branch.behind))
            .unwrap_or_default();
        let mut files = HashMap::new();
        for entry in entries {
            let status = display_status(&entry);
            if status != WorktreeStatus::Unmodified {
                files.insert(entry.path, status);
            }
        }
        let mut dirs: HashMap<String, WorktreeStatus> = HashMap::new();
        for (path, status) in &files {
            let mut parent = path.as_str();
            while let Some((dir, _)) = parent.rsplit_once('/') {
                dirs.entry(dir.to_string())
                    .and_modify(|current| {
                        if status_rank(*status) > status_rank(*current) {
                            *current = *status;
                        }
                    })
                    .or_insert(*status);
                parent = dir;
            }
        }
        Self {
            available: true,
            branch,
            ahead,
            behind,
            files,
            dirs,
            error: String::new(),
        }
    }

    pub fn file_status(&self, path: &str) -> Option<WorktreeStatus> {
        self.files
            .get(path)
            .copied()
            .filter(|status| *status != WorktreeStatus::Unmodified)
    }

    pub fn dir_status(&self, path: &str) -> Option<WorktreeStatus> {
        self.dirs
            .get(path)
            .copied()
            .filter(|status| *status != WorktreeStatus::Unmodified)
    }

    pub fn changed_count(&self) -> usize {
        self.files.len()
    }

    /// Compact status-bar label: branch plus ahead/behind when known.
    pub fn branch_label(&self) -> String {
        if !self.available {
            return String::new();
        }
        let branch = if self.branch.is_empty() {
            "detached"
        } else {
            &self.branch
        };
        let mut label = branch.to_string();
        if self.ahead > 0 {
            label.push_str(&format!(" ↑{}", self.ahead));
        }
        if self.behind > 0 {
            label.push_str(&format!(" ↓{}", self.behind));
        }
        label
    }
}

/// Highest-priority status for a file that changed in both index and worktree.
fn display_status(entry: &StatusEntry) -> WorktreeStatus {
    if entry.is_conflicted() {
        WorktreeStatus::Conflicted
    } else if !entry.worktree.is_clean() {
        entry.worktree
    } else {
        entry.index
    }
}

fn status_rank(status: WorktreeStatus) -> u8 {
    match status {
        WorktreeStatus::Conflicted => 6,
        WorktreeStatus::Deleted => 5,
        WorktreeStatus::Modified => 4,
        WorktreeStatus::Renamed => 3,
        WorktreeStatus::Copied => 3,
        WorktreeStatus::Added => 2,
        WorktreeStatus::Untracked => 1,
        WorktreeStatus::Unknown | WorktreeStatus::Unmodified => 0,
    }
}

/// Local Git refresh. Not a repository is reported as an unavailable but
/// successful state; a repository whose status command fails is an error.
pub fn refresh(root: &Path) -> Result<GitStatus, String> {
    if !root.join(".git").exists() {
        return Ok(GitStatus::default());
    }
    let options = OpOptions::default().timeout(STATUS_TIMEOUT);
    let repository = Repository::open(root, options.clone()).map_err(|error| error.to_string())?;
    let (branch, entries) = repository
        .status(options)
        .map_err(|error| error.to_string())?;
    Ok(GitStatus::from_status(branch, entries))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn entry(path: &str, index: WorktreeStatus, worktree: WorktreeStatus) -> StatusEntry {
        StatusEntry {
            path: path.to_string(),
            index,
            worktree,
        }
    }

    #[test]
    fn files_and_folders_get_the_highest_status() {
        let status = GitStatus::from_status(
            Some(BranchInfo {
                head: Some("main".to_string()),
                ahead: 2,
                behind: 1,
                ..BranchInfo::default()
            }),
            vec![
                entry(
                    "notes/decisions.md",
                    WorktreeStatus::Unmodified,
                    WorktreeStatus::Modified,
                ),
                entry(
                    "notes/open-questions.md",
                    WorktreeStatus::Unmodified,
                    WorktreeStatus::Untracked,
                ),
                entry(
                    "bible/characters/marta.md",
                    WorktreeStatus::Modified,
                    WorktreeStatus::Unmodified,
                ),
                entry(
                    "screenplay/main.fountain",
                    WorktreeStatus::Conflicted,
                    WorktreeStatus::Conflicted,
                ),
            ],
        );
        assert!(status.available);
        assert_eq!(status.branch_label(), "main ↑2 ↓1");
        assert_eq!(
            status.file_status("notes/decisions.md"),
            Some(WorktreeStatus::Modified)
        );
        // Modified beats untracked in the parent folder aggregate.
        assert_eq!(status.dir_status("notes"), Some(WorktreeStatus::Modified));
        // Index-only changes still count.
        assert_eq!(
            status.dir_status("bible/characters"),
            Some(WorktreeStatus::Modified)
        );
        assert_eq!(
            status.dir_status("screenplay"),
            Some(WorktreeStatus::Conflicted)
        );
        assert_eq!(status.changed_count(), 4);
        assert_eq!(status.dir_status("research"), None);
    }

    #[test]
    fn unavailable_status_is_quiet_and_has_no_branch_label() {
        let status = GitStatus::default();
        assert!(!status.available);
        assert_eq!(status.branch_label(), "");
        assert_eq!(status.changed_count(), 0);
    }

    #[test]
    fn refresh_outside_a_repository_is_not_an_error() {
        let dir: PathBuf = std::env::temp_dir().join(format!(
            "narra-git-status-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let status = refresh(&dir).expect("no repo is not an error");
        assert!(!status.available);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
