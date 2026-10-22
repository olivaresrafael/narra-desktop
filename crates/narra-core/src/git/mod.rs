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
use std::time::Duration;

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

/// Default bound for network operations (clone/fetch/pull/push): much more
/// generous than local ops, still bounded so a stalled network fails instead
/// of hanging a worker forever.
pub const NETWORK_TIMEOUT: Duration = Duration::from_secs(300);

/// A local branch from `git for-each-ref refs/heads`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub name: String,
    /// Full object id of the branch tip.
    pub id: String,
    /// True for the branch currently checked out.
    pub current: bool,
}

/// Result of [`Repository::merge`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeOutcome {
    /// The branch was already an ancestor; nothing changed.
    UpToDate,
    /// The merge was a fast-forward; HEAD moved to the merged tip, no merge
    /// commit was created.
    FastForward,
    /// A merge commit was created; carries its id.
    Merged { id: String },
    /// The merge stopped on conflicts. Nothing was committed: conflicted
    /// files carry conflict markers and can be resolved and committed, or
    /// discarded with [`Repository::abort_merge`].
    Conflicts { files: Vec<String> },
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

    /// Current branch name, or `None` on a detached HEAD / unborn branch.
    pub fn current_branch(&self, options: OpOptions) -> Result<Option<String>, GitError> {
        let output = run_git(
            Some(&self.root),
            "branch",
            &["branch", "--show-current"],
            options,
        )?;
        if !output.status.success() {
            return Err(command_failed("branch", &output.stderr));
        }
        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(if name.is_empty() { None } else { Some(name) })
    }

    /// All local branches, current one flagged.
    pub fn branches(&self, options: OpOptions) -> Result<Vec<Branch>, GitError> {
        let output = run_git(
            Some(&self.root),
            "for-each-ref",
            &[
                "for-each-ref",
                "refs/heads",
                "--format=%(refname:short)%09%(objectname)%09%(HEAD)",
            ],
            options,
        )?;
        if !output.status.success() {
            return Err(command_failed("for-each-ref", &output.stderr));
        }
        Ok(parse::parse_branches(&String::from_utf8_lossy(
            &output.stdout,
        )))
    }

    /// Create `name` at the current HEAD without switching to it.
    ///
    /// Branch names are validated against a conservative subset of
    /// `git check-ref-format`; anything flag-like or ambiguous is rejected
    /// before it reaches Git.
    pub fn create_branch(&self, name: &str, options: OpOptions) -> Result<(), GitError> {
        validate_branch_name(name)?;
        let output = run_git(Some(&self.root), "branch", &["branch", "--", name], options)?;
        if !output.status.success() {
            return Err(command_failed("branch", &output.stderr));
        }
        Ok(())
    }

    /// Switch to an existing branch. Git itself refuses when uncommitted
    /// changes would be overwritten — no data is ever discarded here.
    pub fn checkout(&self, name: &str, options: OpOptions) -> Result<(), GitError> {
        validate_branch_name(name)?;
        let output = run_git(Some(&self.root), "checkout", &["checkout", name], options)?;
        if !output.status.success() {
            return Err(command_failed("checkout", &output.stderr));
        }
        Ok(())
    }

    /// Create `name` at HEAD and switch to it in one step.
    pub fn checkout_new(&self, name: &str, options: OpOptions) -> Result<(), GitError> {
        validate_branch_name(name)?;
        let output = run_git(
            Some(&self.root),
            "checkout",
            &["checkout", "-b", "--", name],
            options,
        )?;
        if !output.status.success() {
            return Err(command_failed("checkout", &output.stderr));
        }
        Ok(())
    }

    /// Delete `name` with Git's *safe* `-d`: a branch that is not fully
    /// merged is refused instead of force-removed.
    pub fn delete_branch(&self, name: &str, options: OpOptions) -> Result<(), GitError> {
        validate_branch_name(name)?;
        let output = run_git(
            Some(&self.root),
            "branch",
            &["branch", "-d", "--", name],
            options,
        )?;
        if !output.status.success() {
            return Err(command_failed("branch -d", &output.stderr));
        }
        Ok(())
    }

    /// Merge `name` into the current branch without opening an editor.
    ///
    /// Conflicts are reported as [`MergeOutcome::Conflicts`] — never resolved
    /// automatically — and the repository is left in a merge state the user
    /// can complete (commit) or undo ([`Repository::abort_merge`]).
    pub fn merge(&self, name: &str, options: OpOptions) -> Result<MergeOutcome, GitError> {
        validate_branch_name(name)?;
        let head_before = self.head_id(options.clone())?;
        let output = run_git(
            Some(&self.root),
            "merge",
            &["merge", "--no-edit", "--", name],
            options.clone(),
        )?;
        if output.status.success() {
            let head_after = self.head_id(options)?;
            return Ok(if head_after == head_before {
                MergeOutcome::UpToDate
            } else if self.is_merge_commit(&head_after)? {
                MergeOutcome::Merged { id: head_after }
            } else {
                MergeOutcome::FastForward
            });
        }
        // Conflict or refusal: distinguish by the merge state Git records.
        if self.is_merging(options)? {
            let (_, entries) = self.status(OpOptions::default())?;
            let files = entries
                .into_iter()
                .filter(|entry| entry.is_conflicted())
                .map(|entry| entry.path)
                .collect();
            return Ok(MergeOutcome::Conflicts { files });
        }
        Err(command_failed("merge", &output.stderr))
    }

    /// Abort an in-progress merge, restoring the pre-merge state exactly.
    /// Safe no-op when no merge is in progress.
    pub fn abort_merge(&self, options: OpOptions) -> Result<(), GitError> {
        let output = run_git(Some(&self.root), "merge", &["merge", "--abort"], options)?;
        if !output.status.success() {
            return Err(command_failed("merge --abort", &output.stderr));
        }
        Ok(())
    }

    /// True while a conflicted merge is unresolved (MERGE_HEAD exists).
    pub fn is_merging(&self, options: OpOptions) -> Result<bool, GitError> {
        let output = run_git(
            Some(&self.root),
            "rev-parse",
            &["rev-parse", "-q", "--verify", "MERGE_HEAD"],
            options,
        )?;
        Ok(output.status.success())
    }

    /// Configured remotes as `(name, fetch_url)` pairs. URLs are redacted:
    /// credential-bearing remotes are safe to display and log.
    pub fn remotes(&self, options: OpOptions) -> Result<Vec<(String, String)>, GitError> {
        let output = run_git(Some(&self.root), "remote", &["remote", "-v"], options)?;
        if !output.status.success() {
            return Err(command_failed("remote", &output.stderr));
        }
        // `origin  https://host/x.git (fetch)` — dedupe fetch/push lines.
        let mut remotes = Vec::new();
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let mut fields = line.split_whitespace();
            let (Some(name), Some(url)) = (fields.next(), fields.next()) else {
                continue;
            };
            let url = redact_secrets(url);
            if !remotes.iter().any(|(existing, _)| existing == name) {
                remotes.push((name.to_string(), url));
            }
        }
        Ok(remotes)
    }

    /// Fetch from `remote` (no merge, no working-tree changes).
    pub fn fetch(&self, remote: &str, options: OpOptions) -> Result<(), GitError> {
        validate_remote_name(remote)?;
        let output = run_git(Some(&self.root), "fetch", &["fetch", remote], options)?;
        if !output.status.success() {
            return Err(command_failed("fetch", &output.stderr));
        }
        Ok(())
    }

    /// Fast-forward pull of `branch` from `remote`.
    ///
    /// Deliberately `--ff-only`: a pull never creates surprise merge commits
    /// and never resolves conflicts implicitly — when it fails, the user
    /// brings the branches together with an explicit [`Repository::merge`].
    pub fn pull(&self, remote: &str, branch: &str, options: OpOptions) -> Result<(), GitError> {
        validate_remote_name(remote)?;
        validate_branch_name(branch)?;
        let output = run_git(
            Some(&self.root),
            "pull",
            &["pull", "--ff-only", remote, branch],
            options,
        )?;
        if !output.status.success() {
            return Err(command_failed("pull --ff-only", &output.stderr));
        }
        Ok(())
    }

    /// Push `branch` to `remote`. Callers must have shown explicit user
    /// confirmation first (`AGENTS.md`: publication requires confirmation).
    pub fn push(&self, remote: &str, branch: &str, options: OpOptions) -> Result<(), GitError> {
        validate_remote_name(remote)?;
        validate_branch_name(branch)?;
        let output = run_git(Some(&self.root), "push", &["push", remote, branch], options)?;
        if !output.status.success() {
            return Err(command_failed("push", &output.stderr));
        }
        Ok(())
    }

    /// HEAD commit id (empty repository → empty string).
    fn head_id(&self, options: OpOptions) -> Result<String, GitError> {
        let output = run_git(
            Some(&self.root),
            "rev-parse",
            &["rev-parse", "-q", "--verify", "HEAD"],
            options,
        )?;
        Ok(if output.status.success() {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        } else {
            String::new()
        })
    }

    /// True when `id` has more than one parent (a real merge commit).
    fn is_merge_commit(&self, id: &str) -> Result<bool, GitError> {
        let output = run_git(
            Some(&self.root),
            "rev-list",
            &["rev-list", "--parents", "-n", "1", id],
            OpOptions::default(),
        )?;
        if !output.status.success() {
            return Err(command_failed("rev-list", &output.stderr));
        }
        let parents = String::from_utf8_lossy(&output.stdout);
        Ok(parents.split_whitespace().count() > 2)
    }
}

/// Clone `url` into `destination` (which must not exist yet) and return the
/// new repository. Credentials in `url` never appear in errors or messages.
pub fn clone(url: &str, destination: &Path, options: OpOptions) -> Result<Repository, GitError> {
    if url.trim().is_empty() {
        return Err(GitError::InvalidInput("clone URL must not be empty"));
    }
    let destination_str = destination.to_str().ok_or(GitError::InvalidInput(
        "clone destination is not valid UTF-8",
    ))?;
    let output = run_git(None, "clone", &["clone", url, destination_str], options)?;
    if !output.status.success() {
        return Err(command_failed("clone", &output.stderr));
    }
    Repository::open(destination, OpOptions::default())
}

/// Validate a branch name against a conservative subset of
/// `git check-ref-format`: enough to guarantee the name is safe as an argv
/// element and unambiguous as a ref, without a full refspec implementation.
pub fn validate_branch_name(name: &str) -> Result<(), GitError> {
    let invalid = |reason: &'static str| Err(GitError::InvalidInput(reason));
    if name.is_empty() {
        return invalid("branch name must not be empty");
    }
    if name.starts_with('-') || name.starts_with('/') || name.ends_with('/') {
        return invalid("branch name must not start with '-' or '/' or end with '/'");
    }
    if name.ends_with(".lock") || name.ends_with('.') {
        return invalid("branch name must not end with '.lock' or '.'");
    }
    if name.contains("..") || name.contains("@{") {
        return invalid("branch name must not contain '..' or '@{'");
    }
    if name.split('/').any(|component| {
        component.is_empty() || component.starts_with('.') || component.ends_with('.')
    }) {
        return invalid("branch name components must not be empty, start or end with '.'");
    }
    if name.chars().any(|c| {
        c.is_ascii_control() || matches!(c, ' ' | '~' | '^' | ':' | '?' | '*' | '[' | '\\' | '\x7f')
    }) {
        return invalid("branch name contains forbidden characters");
    }
    Ok(())
}

/// Remote names get the same treatment as branch names (they are argv
/// elements in fetch/pull/push).
fn validate_remote_name(name: &str) -> Result<(), GitError> {
    validate_branch_name(name)
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
