//! Local Git integration using the system `git` executable.
//!
//! This is the Milestone 3 / Spike D slice: local operations only (no
//! network), system Git as the engine (default hypothesis under test), every
//! call blocking-but-cancellable — invoke from a worker thread, never the UI
//! thread.
//!
//! Security and robustness rules:
//! - arguments are passed as an argv vector; no shell is involved;
//! - errors are actionable and pass through [`redact_secrets`];
//! - operations are bounded by timeouts and accept cancellation tokens;
//! - credentials are never read, written or stored by this module.

mod parse;
mod runner;

pub use runner::{CancellationToken, GitError, OpOptions, redact_secrets};

use std::path::{Path, PathBuf};

use runner::run_git;

/// Minimum Git version supported by the app; older versions are rejected
/// with an actionable message instead of failing later in surprising ways.
pub const MIN_GIT_VERSION: (u32, u32, u32) = (2, 25, 0);

/// Result of probing the system Git installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    /// Raw `git --version` output, already safe to display.
    pub raw: String,
}

impl GitVersion {
    pub fn is_supported(&self) -> bool {
        (self.major, self.minor, self.patch) >= MIN_GIT_VERSION
    }
}

/// Detect the system Git and parse its version.
///
/// Errors: [`GitError::Unavailable`] when Git is not installed/on PATH.
pub fn detect(options: OpOptions) -> Result<GitVersion, GitError> {
    let output = run_git(None, "--version", &["--version"], options)?;
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let raw = redact_secrets(&raw);
    let (major, minor, patch) = parse::parse_version(&raw).ok_or_else(|| {
        GitError::Unavailable(format!("could not parse `git --version` output: {raw}"))
    })?;
    Ok(GitVersion {
        major,
        minor,
        patch,
        raw,
    })
}

/// Branch/remote context from `git status`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BranchInfo {
    pub oid: Option<String>,
    pub head: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
}

/// Working-tree state of a single path (both sides of `XY`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreeStatus {
    Unmodified,
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    Untracked,
    Conflicted,
    Unknown,
}

impl WorktreeStatus {
    fn from_code(code: char) -> Self {
        match code {
            '.' => WorktreeStatus::Unmodified,
            'M' => WorktreeStatus::Modified,
            'A' => WorktreeStatus::Added,
            'D' => WorktreeStatus::Deleted,
            'R' => WorktreeStatus::Renamed,
            'C' => WorktreeStatus::Copied,
            '?' => WorktreeStatus::Untracked,
            'U' => WorktreeStatus::Conflicted,
            _ => WorktreeStatus::Unknown,
        }
    }

    pub fn is_clean(self) -> bool {
        matches!(self, WorktreeStatus::Unmodified)
    }
}

/// One path from `git status --porcelain=v2`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusEntry {
    pub path: String,
    pub index: WorktreeStatus,
    pub worktree: WorktreeStatus,
}

impl StatusEntry {
    pub fn is_conflicted(&self) -> bool {
        self.index == WorktreeStatus::Conflicted || self.worktree == WorktreeStatus::Conflicted
    }
}

/// A parsed commit from `git log`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub id: String,
    pub author: String,
    pub email: String,
    /// RFC 3339 timestamp as Git reports it.
    pub date: String,
    pub subject: String,
}

/// Per-file insertions/deletions from `git diff --numstat`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub added: u32,
    pub removed: u32,
}

/// Which comparison a diff should compute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffScope {
    /// Unstaged changes (working tree vs index).
    Worktree,
    /// Staged changes (index vs HEAD).
    Staged,
    /// Everything, working tree vs HEAD.
    WorktreeVsHead,
}

/// A handle to a Git working tree on disk.
///
/// Cheap to construct; it validates existence on [`Repository::open`] /
/// [`Repository::init`] and trusts the path afterwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repository {
    root: PathBuf,
}

impl Repository {
    /// Open an existing repository rooted at `root`.
    ///
    /// Fails with an actionable error when `root` is not inside a work tree.
    pub fn open(root: &Path, options: OpOptions) -> Result<Self, GitError> {
        let output = run_git(
            Some(root),
            "rev-parse",
            &["rev-parse", "--is-inside-work-tree"],
            options,
        )?;
        if !output.status.success() {
            return Err(not_a_repository(root, &output.stderr));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim() != "true" {
            return Err(not_a_repository(root, &output.stderr));
        }
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    /// Initialize a new repository at `root` (the directory must exist).
    pub fn init(root: &Path, options: OpOptions) -> Result<Self, GitError> {
        let output = run_git(Some(root), "init", &["init"], options)?;
        if !output.status.success() {
            return Err(command_failed("init", &output.stderr));
        }
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `git status --porcelain=v2 --branch` as structured data.
    ///
    /// `core.quotepath=false` keeps non-ASCII paths (accents, ñ, CJK)
    /// verbatim instead of Git's quoted octal escapes.
    pub fn status(
        &self,
        options: OpOptions,
    ) -> Result<(Option<BranchInfo>, Vec<StatusEntry>), GitError> {
        let output = run_git(
            Some(&self.root),
            "status",
            &[
                "-c",
                "core.quotepath=false",
                "status",
                "--porcelain=v2",
                "--branch",
                "--untracked-files=all",
            ],
            options,
        )?;
        if !output.status.success() {
            return Err(command_failed("status", &output.stderr));
        }
        Ok(parse::parse_status(&String::from_utf8_lossy(
            &output.stdout,
        )))
    }

    /// Last `limit` commits (newest first), at most 1000.
    pub fn log(&self, limit: usize, options: OpOptions) -> Result<Vec<Commit>, GitError> {
        let limit = limit.clamp(1, 1000);
        let format = "--format=%H%x1f%an%x1f%ae%x1f%aI%x1f%s%x1e";
        let output = run_git(
            Some(&self.root),
            "log",
            &["log", &format!("--max-count={limit}"), format],
            options,
        )?;
        if !output.status.success() {
            return Err(command_failed("log", &output.stderr));
        }
        Ok(parse::parse_log(&String::from_utf8_lossy(&output.stdout)))
    }

    /// Unified diff for the given scope. Empty string means "no changes".
    ///
    /// Binary files are summarized by Git's own "Binary files differ" line;
    /// raw binary content is never returned.
    pub fn diff(&self, scope: DiffScope, options: OpOptions) -> Result<String, GitError> {
        let scope_arg = match scope {
            DiffScope::Worktree => &[],
            DiffScope::Staged => &["--cached"][..],
            DiffScope::WorktreeVsHead => &["HEAD"][..],
        };
        let mut args = vec!["diff", "--no-color", "--find-renames"];
        args.extend_from_slice(scope_arg);
        let output = run_git(Some(&self.root), "diff", &args, options)?;
        if !output.status.success() {
            return Err(command_failed("diff", &output.stderr));
        }
        Ok(redact_secrets(&String::from_utf8_lossy(&output.stdout)))
    }

    /// Per-file insertion/deletion counts for the given scope.
    pub fn diff_stats(
        &self,
        scope: DiffScope,
        options: OpOptions,
    ) -> Result<Vec<FileChange>, GitError> {
        let scope_arg = match scope {
            DiffScope::Worktree => &[],
            DiffScope::Staged => &["--cached"][..],
            DiffScope::WorktreeVsHead => &["HEAD"][..],
        };
        let mut args = vec!["diff", "--numstat"];
        args.extend_from_slice(scope_arg);
        let output = run_git(Some(&self.root), "diff-stats", &args, options)?;
        if !output.status.success() {
            return Err(command_failed("diff-stats", &output.stderr));
        }
        Ok(parse::parse_numstat(&String::from_utf8_lossy(
            &output.stdout,
        )))
    }

    /// Stage paths (or everything with [`Repository::stage_all`]).
    pub fn stage(&self, paths: &[&str], options: OpOptions) -> Result<(), GitError> {
        if paths.is_empty() {
            return Err(GitError::InvalidInput("stage requires at least one path"));
        }
        let mut args = vec!["add", "--"];
        args.extend_from_slice(paths);
        let output = run_git(Some(&self.root), "add", &args, options)?;
        if !output.status.success() {
            return Err(command_failed("add", &output.stderr));
        }
        Ok(())
    }

    /// Stage every tracked and untracked path.
    pub fn stage_all(&self, options: OpOptions) -> Result<(), GitError> {
        let output = run_git(Some(&self.root), "add", &["add", "--all"], options)?;
        if !output.status.success() {
            return Err(command_failed("add", &output.stderr));
        }
        Ok(())
    }

    /// Commit the staged index with an explicit message and identity.
    ///
    /// Identity is passed explicitly (never silently read from ambient
    /// configuration) so agent-attributed commits stay distinguishable from
    /// user commits later. Fails with the Git error when nothing is staged.
    pub fn commit(
        &self,
        message: &str,
        author_name: &str,
        author_email: &str,
        options: OpOptions,
    ) -> Result<String, GitError> {
        if message.trim().is_empty() {
            return Err(GitError::InvalidInput("commit message must not be empty"));
        }
        let author = format!("{author_name} <{author_email}>");
        // Committer identity is set explicitly too: never inherit whatever
        // ambient machine configuration happens to exist.
        let env = [
            ("GIT_COMMITTER_NAME", author_name),
            ("GIT_COMMITTER_EMAIL", author_email),
        ];
        let output = runner::run_git_env(
            Some(&self.root),
            "commit",
            &["commit", "--author", &author, "-m", message],
            &env,
            options.clone(),
        )?;
        if !output.status.success() {
            return Err(command_failed("commit", &output.stderr));
        }
        let head = run_git(
            Some(&self.root),
            "rev-parse",
            &["rev-parse", "HEAD"],
            options,
        )?;
        if !head.status.success() {
            return Err(command_failed("rev-parse", &head.stderr));
        }
        Ok(String::from_utf8_lossy(&head.stdout).trim().to_string())
    }
}

fn not_a_repository(root: &Path, stderr: &[u8]) -> GitError {
    GitError::CommandFailed {
        operation: format!("`{}` is not a Git repository", root.display()),
        code: Some(128),
        stderr: redact_secrets(String::from_utf8_lossy(stderr).trim()),
    }
}

fn command_failed(operation: &str, stderr: &[u8]) -> GitError {
    let message = String::from_utf8_lossy(stderr).trim().to_string();
    GitError::CommandFailed {
        operation: operation.to_string(),
        code: None,
        stderr: if message.is_empty() {
            "no details reported by git".to_string()
        } else {
            redact_secrets(&message)
        },
    }
}
