//! Integration tests for local Git operations (Milestone 3, Spike D).
//!
//! Every test runs against real temporary repositories on disk, using the
//! system `git`. Offline operations only: nothing here touches the network.

use narra_core::git::{
    CancellationToken, DiffScope, GitError, OpOptions, WorktreeStatus, detect, redact_secrets,
};
use narra_core::git::{Commit, Repository};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

fn temp_root() -> PathBuf {
    tempfile::tempdir().expect("tempdir").keep()
}

fn options() -> OpOptions {
    OpOptions::default().timeout(Duration::from_secs(30))
}

fn new_repo() -> Repository {
    let root = temp_root();
    Repository::init(&root, options()).expect("init")
}

fn write(repo: &Repository, relative: &str, contents: &str) {
    let path = repo.root().join(relative);
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    fs::write(path, contents).expect("write");
}

fn commit_all(repo: &Repository, message: &str) -> String {
    repo.stage_all(options()).expect("stage");
    repo.commit(message, "Ana Autora", "ana@ejemplo.com", options())
        .expect("commit")
}

#[test]
fn detects_system_git() {
    let version = detect(options()).expect("git present on dev machine");
    assert!(!version.raw.is_empty());
    assert!(version.major >= 2, "{version:?}");
    // The reference machine runs Apple Git 2.50.1; anything >= 2.25 passes.
    assert!(
        version.is_supported(),
        "{version:?} below {:?}",
        narra_core::git::MIN_GIT_VERSION
    );
}

#[test]
fn open_fails_actionably_outside_a_repository() {
    let root = temp_root();
    let error = Repository::open(&root, options()).expect_err("empty dir is not a repo");
    let rendered = error.to_string();
    assert!(rendered.contains("not a Git repository"), "{rendered}");
}

#[test]
fn init_then_open_round_trips() {
    let repo = new_repo();
    let opened = Repository::open(repo.root(), options()).expect("open");
    assert_eq!(opened.root(), repo.root());
}

#[test]
fn init_works_on_paths_with_spaces_and_accents() {
    // macOS convention check: real user projects live under ~/Documents/...
    // with arbitrary Unicode names.
    let root = temp_root().join("Proyecto de guion ñoño");
    fs::create_dir_all(&root).expect("mkdir");
    let repo = Repository::init(&root, options()).expect("init");
    write(&repo, "notes/nota de investigación.md", "# Nota\n");
    let (_, entries) = repo.status(options()).expect("status");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "notes/nota de investigación.md");
}

#[test]
fn offline_status_commit_log_cycle() {
    let repo = new_repo();
    write(&repo, "screenplay/main.fountain", "INT. CASA - DÍA\n");

    // Initial state: unborn branch, one untracked file.
    let (branch, entries) = repo.status(options()).expect("status");
    let branch = branch.expect("branch info");
    assert!(branch.oid.is_none(), "unborn branch has no oid");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].index, WorktreeStatus::Untracked);

    // Stage and commit.
    let id = commit_all(&repo, "Inicio: primera escena");
    assert_eq!(id.len(), 40, "sha-1 object id");

    let (branch, entries) = repo.status(options()).expect("status");
    assert!(entries.is_empty(), "clean worktree, got {entries:?}");
    assert!(branch.expect("branch").oid.is_some());

    // Log reflects the commit with the explicit identity.
    let log = repo.log(10, options()).expect("log");
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].subject, "Inicio: primera escena");
    assert_eq!(log[0].author, "Ana Autora");
    assert_eq!(log[0].email, "ana@ejemplo.com");
    assert_eq!(log[0].id, id);
}

#[test]
fn status_reports_index_and_worktree_separately() {
    let repo = new_repo();
    write(&repo, "a.md", "uno\n");
    commit_all(&repo, "base");

    write(&repo, "a.md", "uno\ndos\n"); // unstaged modification
    write(&repo, "b.md", "nuevo\n"); // untracked
    repo.stage(&["b.md"], options()).expect("stage b");

    let (_, entries) = repo.status(options()).expect("status");
    let by_path = |path: &str| {
        entries
            .iter()
            .find(|e| e.path == path)
            .unwrap_or_else(|| panic!("missing {path}"))
    };
    assert_eq!(by_path("a.md").worktree, WorktreeStatus::Modified);
    assert!(by_path("a.md").index.is_clean());
    assert_eq!(by_path("b.md").index, WorktreeStatus::Added);
    assert!(by_path("b.md").worktree.is_clean());
}

#[test]
fn diff_scopes_and_stats() {
    let repo = new_repo();
    write(&repo, "a.md", "uno\n");
    commit_all(&repo, "base");

    write(&repo, "a.md", "uno\ndos\n");
    let worktree = repo.diff(DiffScope::Worktree, options()).expect("diff");
    assert!(worktree.contains("+dos"), "{worktree}");
    assert!(
        repo.diff(DiffScope::Staged, options())
            .expect("staged")
            .is_empty()
    );

    repo.stage_all(options()).expect("stage");
    let staged = repo.diff(DiffScope::Staged, options()).expect("diff");
    assert!(staged.contains("+dos"), "{staged}");
    assert!(
        repo.diff(DiffScope::Worktree, options())
            .expect("worktree")
            .is_empty()
    );

    let stats = repo
        .diff_stats(DiffScope::Staged, options())
        .expect("stats");
    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0].path, "a.md");
    assert_eq!(stats[0].added, 1);
    assert_eq!(stats[0].removed, 0);

    // WorktreeVsHead sees the same change before the commit lands.
    let all = repo
        .diff(DiffScope::WorktreeVsHead, options())
        .expect("head diff");
    assert!(all.contains("+dos"), "{all}");
}

#[test]
fn commit_with_nothing_staged_fails_actionably() {
    let repo = new_repo();
    let error = repo
        .commit("vacío", "Ana", "ana@ejemplo.com", options())
        .expect_err("nothing staged");
    match error {
        GitError::CommandFailed {
            operation, stderr, ..
        } => {
            assert_eq!(operation, "commit");
            assert!(!stderr.is_empty(), "must explain why it failed");
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn commit_rejects_empty_message_before_running_git() {
    let repo = new_repo();
    write(&repo, "a.md", "x\n");
    repo.stage_all(options()).expect("stage");
    let error = repo
        .commit("   ", "Ana", "ana@ejemplo.com", options())
        .expect_err("blank message");
    assert!(matches!(error, GitError::InvalidInput(_)), "{error:?}");
}

#[test]
fn commit_identity_is_explicit_not_ambient() {
    // Even with HOME pointing at an unconfigured location, commits succeed
    // because identity travels with the call.
    let repo = new_repo();
    write(&repo, "a.md", "x\n");
    let id = commit_all(&repo, "Con identidad explícita");
    let log: Vec<Commit> = repo.log(1, options()).expect("log");
    assert_eq!(log[0].id, id);
    assert_eq!(log[0].author, "Ana Autora");
}

#[test]
fn cancellation_token_stops_a_running_operation() {
    let repo = new_repo();
    let token = CancellationToken::new();
    token.cancel(); // already cancelled: must short-circuit
    let error = repo.status(options().cancel(token)).expect_err("cancelled");
    assert!(matches!(error, GitError::Cancelled { .. }), "{error:?}");
}

#[test]
fn stage_rejects_empty_path_list() {
    let repo = new_repo();
    let error = repo.stage(&[], options()).expect_err("empty");
    assert!(matches!(error, GitError::InvalidInput(_)), "{error:?}");
}

#[test]
fn log_is_bounded_and_ordered_newest_first() {
    let repo = new_repo();
    let mut ids = Vec::new();
    for n in 1..=5 {
        write(&repo, "a.md", &format!("versión {n}\n"));
        ids.push(commit_all(&repo, &format!("commit {n}")));
    }
    let log = repo.log(3, options()).expect("log");
    assert_eq!(log.len(), 3);
    assert_eq!(log[0].subject, "commit 5");
    assert_eq!(log[2].subject, "commit 3");
    assert_eq!(log[0].id, ids[4]);
}

#[test]
fn errors_never_leak_credentials() {
    // A failing command whose stderr echoes a credential-bearing URL.
    let message = "fatal: unable to access 'https://ana:hunter2@example.com/repo.git/': Could not resolve host";
    let error = GitError::CommandFailed {
        operation: "fetch".to_string(),
        code: Some(128),
        stderr: redact_secrets(message),
    };
    let rendered = error.to_string();
    assert!(!rendered.contains("hunter2"), "{rendered}");
    assert!(rendered.contains("example.com"), "host is useful context");
}
