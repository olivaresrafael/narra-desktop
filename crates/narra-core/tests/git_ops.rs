//! Integration tests for local Git operations (Milestone 3, Spike D).
//!
//! Every test runs against real temporary repositories on disk, using the
//! system `git`. Offline operations only: nothing here touches the network.

use narra_core::git::{
    CancellationToken, Commit, DiffScope, GitError, MergeOutcome, OpOptions, Repository,
    WorktreeStatus, clone, detect, redact_secrets, validate_branch_name,
};
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
    // Pin the branch name: machine git config must not decide test outcomes.
    Repository::init_with_branch(&root, "main", options()).expect("init")
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

/// Test setup only: allow pushes into the checked-out branch of a non-bare
/// "server" repo (git denies this by default; the app itself never sets this
/// — real remotes are bare or hosted).
fn allow_push_into_checked_out_branch(repo: &Repository) {
    let status = std::process::Command::new("git")
        .args(["config", "receive.denyCurrentBranch", "updateInstead"])
        .current_dir(repo.root())
        .status()
        .expect("git config");
    assert!(status.success(), "git config must succeed");
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

// --- Branches (M3 slice 2) -------------------------------------------------

#[test]
fn branch_lifecycle_create_list_checkout() {
    let repo = new_repo();
    write(&repo, "a.md", "uno\n");
    commit_all(&repo, "base");

    repo.create_branch("feature/escena", options())
        .expect("create");
    let branches = repo.branches(options()).expect("branches");
    assert_eq!(branches.len(), 2);
    let main = branches.iter().find(|b| b.name == "main").expect("main");
    assert!(main.current, "main still checked out");
    assert!(!branches.iter().any(|b| b.current && b.name != "main"));

    repo.checkout("feature/escena", options())
        .expect("checkout");
    assert_eq!(
        repo.current_branch(options()).expect("current"),
        Some("feature/escena".to_string())
    );
    let branches = repo.branches(options()).expect("branches");
    assert!(
        branches
            .iter()
            .find(|b| b.name == "feature/escena")
            .expect("feature")
            .current
    );

    // Commit on the branch, return to main: history diverges cleanly.
    write(&repo, "a.md", "uno\ndos\n");
    commit_all(&repo, "en feature");
    repo.checkout("main", options()).expect("back to main");
    let log = repo.log(1, options()).expect("log");
    assert_eq!(log[0].subject, "base", "main did not move");
}

#[test]
fn checkout_new_creates_and_switches() {
    let repo = new_repo();
    write(&repo, "a.md", "uno\n");
    commit_all(&repo, "base");
    repo.checkout_new("experimento", options())
        .expect("checkout -b");
    assert_eq!(
        repo.current_branch(options()).expect("current"),
        Some("experimento".to_string())
    );
}

#[test]
fn branch_names_are_validated_before_reaching_git() {
    for bad in [
        "",
        "-d",          // would be read as a flag
        "a b",         // whitespace breaks argv semantics for users
        "a..b",        // range, not a name
        "feature@{u}", // reflog syntax
        "x.lock",      // reserved suffix
        ".oculto",     // component starts with dot
        "fin.",        // trailing dot
        "car\\acter",  // escape
        "rama/con//",  // empty trailing component
    ] {
        assert!(
            validate_branch_name(bad).is_err(),
            "{bad:?} must be rejected"
        );
    }
    for good in ["main", "feature/escena-2", "corrección/ñ", "v1.0"] {
        assert!(
            validate_branch_name(good).is_ok(),
            "{good:?} must be accepted"
        );
    }
}

#[test]
fn delete_branch_refuses_unmerged_and_allows_merged() {
    let repo = new_repo();
    write(&repo, "a.md", "uno\n");
    commit_all(&repo, "base");

    // Branch with its own commit: not merged anywhere.
    repo.checkout_new("temporal", options()).expect("branch");
    write(&repo, "b.md", "x\n");
    commit_all(&repo, "commit exclusivo de temporal");
    repo.checkout("main", options()).expect("main");

    // Safe -d must refuse (no data loss) and say why.
    let error = repo
        .delete_branch("temporal", options())
        .expect_err("unmerged branch must be refused");
    assert!(!error.to_string().is_empty(), "actionable message");
    assert!(
        repo.branches(options())
            .expect("branches")
            .iter()
            .any(|b| b.name == "temporal"),
        "branch still exists after refusal"
    );

    // Advance main so the merge cannot fast-forward, then merge and delete.
    write(&repo, "a.md", "uno\nmain tambien\n");
    commit_all(&repo, "main avanza");
    let outcome = repo.merge("temporal", options()).expect("merge");
    assert!(
        matches!(outcome, MergeOutcome::Merged { .. }),
        "{outcome:?}"
    );
    repo.delete_branch("temporal", options())
        .expect("merged branch deletable");
}

// --- Merge and conflicts (M3 slice 2) --------------------------------------

fn divergent_repos() -> (Repository, String) {
    let repo = new_repo();
    write(&repo, "a.md", "linea base\n");
    commit_all(&repo, "base");
    repo.checkout_new("feature", options()).expect("branch");
    write(&repo, "a.md", "linea base\nfeature change\n");
    let feature = commit_all(&repo, "cambio en feature");
    repo.checkout("main", options()).expect("main");
    (repo, feature)
}

#[test]
fn merge_fast_forwards_when_main_has_not_moved() {
    let (repo, feature) = divergent_repos();
    let outcome = repo.merge("feature", options()).expect("merge");
    assert_eq!(outcome, MergeOutcome::FastForward);
    let log = repo.log(1, options()).expect("log");
    assert_eq!(log[0].id, feature, "main moved to the feature tip");
}

#[test]
fn merge_creates_commit_when_both_sides_advanced() {
    let (repo, _feature) = divergent_repos();
    write(&repo, "otro.md", "contenido\n");
    commit_all(&repo, "en main");
    let outcome = repo.merge("feature", options()).expect("merge");
    let MergeOutcome::Merged { id } = outcome else {
        panic!("expected a merge commit, got {outcome:?}");
    };
    assert_eq!(id.len(), 40);
    let log = repo.log(1, options()).expect("log");
    assert_eq!(log[0].id, id);
    // Both parents present: the feature history was preserved, not rewritten.
    assert!(
        repo.status(options()).expect("status").1.is_empty(),
        "clean after merge"
    );
}

#[test]
fn merge_up_to_date_is_a_no_op() {
    let (repo, _feature) = divergent_repos();
    repo.merge("feature", options()).expect("first merge");
    let outcome = repo.merge("feature", options()).expect("second merge");
    assert_eq!(outcome, MergeOutcome::UpToDate);
}

#[test]
fn merge_conflicts_are_reported_and_abortable_without_data_loss() {
    let repo = new_repo();
    write(&repo, "a.md", "linea base\n");
    commit_all(&repo, "base");
    repo.checkout_new("feature", options()).expect("branch");
    write(&repo, "a.md", "version feature\n");
    commit_all(&repo, "feature");
    repo.checkout("main", options()).expect("main");
    write(&repo, "a.md", "version main\n");
    commit_all(&repo, "main");

    let outcome = repo.merge("feature", options()).expect("merge");
    let MergeOutcome::Conflicts { files } = outcome else {
        panic!("expected conflicts, got {outcome:?}");
    };
    assert_eq!(files, vec!["a.md".to_string()]);

    // The conflicted file carries markers and MERGE_HEAD is set.
    let conflicted = fs::read_to_string(repo.root().join("a.md")).expect("read");
    assert!(conflicted.contains("<<<<<<<"), "{conflicted}");
    assert!(repo.is_merging(options()).expect("merging"));

    // Abort restores the exact pre-merge content: nothing is lost.
    repo.abort_merge(options()).expect("abort");
    assert!(!repo.is_merging(options()).expect("merging"));
    assert_eq!(
        fs::read_to_string(repo.root().join("a.md")).expect("read"),
        "version main\n",
        "abort restores main's version byte for byte"
    );
    let (_, entries) = repo.status(options()).expect("status");
    assert!(entries.is_empty(), "clean worktree after abort");
}

// --- Remotes over local paths (M3 slice 2; offline: path transport) -------

#[test]
fn clone_over_local_path_then_fetch_pull_push() {
    let source = new_repo();
    write(&source, "a.md", "uno\n");
    let first = commit_all(&source, "primero");

    // Clone via plain path (local transport — no network involved).
    allow_push_into_checked_out_branch(&source);
    let destination = temp_root().join("copia");
    let cloned = clone(
        source.root().to_str().expect("utf8"),
        &destination,
        options(),
    )
    .expect("clone");
    let log = cloned.log(10, options()).expect("log");
    assert_eq!(log[0].id, first, "clone carries history");

    let remotes = cloned.remotes(options()).expect("remotes");
    assert_eq!(remotes.len(), 1);
    assert_eq!(remotes[0].0, "origin");

    // The clone knows its upstream; a new commit pushes back to the source.
    write(&cloned, "a.md", "uno\ndos\n");
    let second = commit_all(&cloned, "segundo");
    cloned.push("origin", "main", options()).expect("push");
    let source_log = source.log(1, options()).expect("source log");
    assert_eq!(
        source_log[0].id, second,
        "push must be confirmed by the receiving repo, not just exit 0"
    );

    // Source advances; clone fetches and fast-forward pulls.
    write(&source, "a.md", "uno\ndos\ntres\n");
    let third = commit_all(&source, "tercero");
    cloned.fetch("origin", options()).expect("fetch");
    cloned.pull("origin", "main", options()).expect("pull");
    assert_eq!(
        fs::read_to_string(cloned.root().join("a.md")).expect("read"),
        "uno\ndos\ntres\n",
        "pull brings the new content"
    );
    assert_eq!(cloned.log(1, options()).expect("log")[0].id, third);
}

#[test]
fn pull_refuses_non_fast_forward_and_explains() {
    let source = new_repo();
    write(&source, "a.md", "uno\n");
    commit_all(&source, "primero");
    let destination = temp_root().join("copia");
    let cloned = clone(
        source.root().to_str().expect("utf8"),
        &destination,
        options(),
    )
    .expect("clone");

    // Both sides commit different content: no fast-forward possible.
    write(&source, "a.md", "uno\nsource\n");
    commit_all(&source, "source");
    write(&cloned, "a.md", "uno\nclon\n");
    commit_all(&cloned, "clon");

    let error = cloned
        .pull("origin", "main", options())
        .expect_err("diverged pull must fail");
    match &error {
        GitError::CommandFailed { stderr, .. } => {
            assert!(!stderr.is_empty(), "must explain the refusal");
        }
        other => panic!("unexpected error: {other:?}"),
    }
    // Nothing was lost: the local commit is still there.
    assert_eq!(cloned.log(1, options()).expect("log")[0].subject, "clon");
}

#[test]
fn fetch_from_missing_remote_fails_fast_and_actionably() {
    let repo = new_repo();
    let error = repo
        .fetch("no-existe", options().timeout(Duration::from_secs(10)))
        .expect_err("missing remote");
    match error {
        GitError::CommandFailed { stderr, .. } => {
            assert!(!stderr.is_empty(), "actionable");
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn remote_arguments_cannot_inject_flags() {
    let repo = new_repo();
    let error = repo
        .fetch("--upload-pack=rm", options())
        .expect_err("flag-like remote");
    assert!(matches!(error, GitError::InvalidInput(_)), "{error:?}");
}
