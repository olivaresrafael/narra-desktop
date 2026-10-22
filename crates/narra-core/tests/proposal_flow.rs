//! Integration tests for write proposals (Milestone 4, slice 2b): a task
//! produces a reviewable proposal without touching canonical files, the
//! reviewer applies all or part of it, rejects it, or is stopped by a stale
//! base — with provenance recorded end to end (deterministic mock adapter).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use narra_core::ProjectType;
use narra_core::agent::{
    AgentError, MockAdapter, ModelAdapter, ModelRequest, ProposalState, SceneEditTask, TaskEvent,
    TaskOptions, proposal,
};
use narra_core::cancel::CancellationToken;
use narra_core::project::create_project;

const TWO_SCENES: &str =
    "INT. TALLER - NOCHE\n\nELENA\nEsto es una prueba.\n\nINT. CALLE - DÍA\n\nMARCOS\nHace sol.\n";

fn temp_root() -> PathBuf {
    tempfile::tempdir().expect("tempdir").keep()
}

fn sample_project() -> PathBuf {
    let root = temp_root();
    let project =
        create_project(&root, "Luces de Invierno", ProjectType::Feature, "es").expect("create");
    project
        .write_document(narra_core::DEFAULT_SCREENPLAY, TWO_SCENES)
        .expect("screenplay");
    root
}

fn task_options() -> TaskOptions {
    TaskOptions {
        timeout: Duration::from_secs(30),
        cancel: CancellationToken::new(),
    }
}

fn propose(root: &Path, scene: usize, instruction: &str) -> narra_core::agent::TaskOutput {
    SceneEditTask::new(root.to_str().expect("utf8"), scene, instruction)
        .run(&MockAdapter::new(), &task_options(), |_| {})
        .expect("task succeeds")
}

fn screenplay(root: &Path) -> String {
    fs::read_to_string(root.join(narra_core::DEFAULT_SCREENPLAY)).expect("read screenplay")
}

// --- Creating a proposal ----------------------------------------------------

#[test]
fn propose_persists_a_reviewable_record_and_leaves_the_screenplay_untouched() {
    let root = sample_project();
    let before = screenplay(&root);
    let mut events: Vec<TaskEvent> = Vec::new();

    let output = SceneEditTask::new(root.to_str().expect("utf8"), 1, "más tensión en el diálogo")
        .run(&MockAdapter::new(), &task_options(), |event| {
            events.push(event)
        })
        .expect("task succeeds");

    // Canonical narrative content is byte-identical after proposing.
    assert_eq!(screenplay(&root), before, "propose must not write canon");

    // The record exists, is reviewable, and carries full provenance.
    let id = output.data["proposal_id"].as_str().expect("proposal id");
    let record = proposal::load(&root, id).expect("load proposal");
    assert_eq!(record.state, ProposalState::ReadyForReview);
    assert_eq!(record.target, narra_core::DEFAULT_SCREENPLAY);
    assert_eq!(record.origin.task, "scene_edit");
    assert_eq!(record.origin.adapter, "mock");
    assert_eq!(record.origin.model, "mock-1");
    assert_eq!(record.rationale, "más tensión en el diálogo");
    assert!(!record.hunks.is_empty(), "diff must be reviewable");
    assert_eq!(record.base_sha256, proposal::sha256_hex(&before));
    assert!(!record.created_at.is_empty(), "timestamp recorded");

    // Events name the proposal for any UI-neutral client.
    assert!(matches!(
        events.first(),
        Some(TaskEvent::Started { task, .. }) if task == "scene_edit"
    ));
    assert!(
        events.iter().any(|event| matches!(
            event,
            TaskEvent::ProposalCreated { id: event_id, target, hunks }
            if event_id == id && target == narra_core::DEFAULT_SCREENPLAY && *hunks > 0
        )),
        "proposal_created event must identify the record: {events:?}"
    );
    assert!(matches!(events.last(), Some(TaskEvent::Completed { .. })));
}

#[test]
fn proposal_list_shows_records_newest_first() {
    let root = sample_project();
    propose(&root, 1, "instrucción uno");
    propose(&root, 2, "instrucción dos");

    let records = proposal::list(&root).expect("list");
    assert_eq!(records.len(), 2, "{records:?}");
    assert_eq!(records[0].origin.task, "scene_edit");
}

#[test]
fn invalid_scene_numbers_fail_with_an_explicit_error() {
    let root = sample_project();
    let error = SceneEditTask::new(root.to_str().expect("utf8"), 0, "x")
        .run(&MockAdapter::new(), &task_options(), |_| {})
        .expect_err("scene numbers start at 1");
    assert!(matches!(error, AgentError::InvalidInput(_)), "{error:?}");

    let error = SceneEditTask::new(root.to_str().expect("utf8"), 9, "x")
        .run(&MockAdapter::new(), &task_options(), |_| {})
        .expect_err("out of range");
    assert!(
        matches!(
            error,
            AgentError::SceneNumber {
                number: 9,
                total: 2
            }
        ),
        "{error:?}"
    );
}

// --- Applying (full and partial) --------------------------------------------

#[test]
fn applying_every_hunk_writes_the_proposed_content_and_records_it() {
    let root = sample_project();
    let output = propose(&root, 1, "reescribe la escena");
    let id = output.data["proposal_id"].as_str().expect("proposal id");

    let outcome = proposal::apply(&root, id, None).expect("apply");

    let record = proposal::load(&root, id).expect("reload");
    assert_eq!(record.state, ProposalState::Applied);
    assert_eq!(record.applied_hunks.len(), record.hunks.len());
    assert_eq!(outcome.skipped_hunks, Vec::<usize>::new());

    // The screenplay now contains the mock rewrite of scene 1 and the rest
    // of the file is intact.
    let after = screenplay(&root);
    assert_ne!(after, TWO_SCENES, "accepted hunks must land");
    assert!(
        after.contains("MOCK BRIEF"),
        "proposed text applied: {after}"
    );
    assert!(after.contains("INT. CALLE - DÍA"), "untouched scene intact");
}

#[test]
fn applying_only_selected_hunks_is_partial_acceptance() {
    let root = sample_project();
    // Two distant changes → two hunks (unit-tested mapping, here end to end).
    let base = "uno\ndos\ntres\ncuatro\ncinco\n";
    let proposed = "uno\nDOS\ntres\ncuatro\nCINCO\n";
    let record = proposal::FileProposal::new(
        "notes/partial.md",
        base,
        proposed,
        "prueba parcial",
        proposal::Origin {
            task: "test".into(),
            adapter: "test".into(),
            model: "test-1".into(),
            task_input: serde_json::json!({}),
        },
        None,
    )
    .expect("two hunks");
    assert_eq!(record.hunks.len(), 2, "{:?}", record.hunks);
    proposal::save(&root, &record).expect("save");

    fs::create_dir_all(root.join("notes")).expect("notes dir");
    fs::write(root.join("notes/partial.md"), base).expect("write base");

    let outcome = proposal::apply(&root, &record.id, Some(&[1])).expect("apply subset");
    assert_eq!(outcome.applied_hunks, vec![1]);
    assert_eq!(outcome.skipped_hunks, vec![0]);

    let after = fs::read_to_string(root.join("notes/partial.md")).expect("read");
    assert_eq!(after, "uno\ndos\ntres\ncuatro\nCINCO\n");

    let record = proposal::load(&root, &record.id).expect("reload");
    assert_eq!(record.state, ProposalState::Applied);
    assert_eq!(record.applied_hunks, vec![1]);
}

#[test]
fn out_of_range_hunk_selection_is_rejected() {
    let root = sample_project();
    let output = propose(&root, 1, "reescribe");
    let id = output.data["proposal_id"].as_str().expect("proposal id");

    let error = proposal::apply(&root, id, Some(&[99])).expect_err("bad index");
    assert!(
        matches!(
            error,
            proposal::ProposalError::InvalidHunk { index: 99, .. }
        ),
        "{error:?}"
    );
    // The failed attempt changed nothing.
    let record = proposal::load(&root, id).expect("reload");
    assert_eq!(record.state, ProposalState::ReadyForReview);
}

// --- Rejecting --------------------------------------------------------------

#[test]
fn rejecting_leaves_canonical_files_unchanged() {
    let root = sample_project();
    let before = screenplay(&root);
    let output = propose(&root, 1, "reescribe");
    let id = output.data["proposal_id"].as_str().expect("proposal id");

    let record = proposal::reject(&root, id).expect("reject");
    assert_eq!(record.state, ProposalState::Rejected);
    assert_eq!(screenplay(&root), before, "rejection touches no canon");

    // A rejected proposal cannot be applied afterwards.
    let error = proposal::apply(&root, id, None).expect_err("rejected");
    assert!(
        matches!(
            error,
            proposal::ProposalError::InvalidState {
                state: ProposalState::Rejected,
                ..
            }
        ),
        "{error:?}"
    );
}

// --- Stale bases and renewed review -----------------------------------------

#[test]
fn applying_over_a_changed_file_is_refused_as_stale() {
    let root = sample_project();
    let output = propose(&root, 1, "reescribe");
    let id = output.data["proposal_id"].as_str().expect("proposal id");

    // A later user edit to the same file invalidates the base.
    let user_edit = "INT. TALLER - NOCHE\n\nELENA\nEditado por el usuario.\n";
    fs::write(root.join(narra_core::DEFAULT_SCREENPLAY), user_edit).expect("user edit");

    let error = proposal::apply(&root, id, None).expect_err("stale base");
    assert!(
        matches!(
            error,
            proposal::ProposalError::Stale { id: ref stale_id, .. } if stale_id == id
        ),
        "{error:?}"
    );

    // The user's edit survived: nothing was silently overwritten.
    assert_eq!(screenplay(&root), user_edit);
    // And the refusal is recorded on the proposal itself.
    let record = proposal::load(&root, id).expect("reload");
    assert_eq!(record.state, ProposalState::Stale);
}

#[test]
fn a_stale_proposal_is_renewed_by_rerunning_the_same_task() {
    let root = sample_project();
    let output = propose(&root, 1, "reescribe");
    let id = output.data["proposal_id"].as_str().expect("proposal id");

    let user_edit = "INT. TALLER - NOCHE\n\nELENA\nEditado por el usuario.\n\nINT. CALLE - DÍA\n\nMARCOS\nHace sol.\n";
    fs::write(root.join(narra_core::DEFAULT_SCREENPLAY), user_edit).expect("user edit");
    proposal::apply(&root, id, None).expect_err("stale base");

    // Renewed review: rebuild the task from provenance and run it again.
    let stale = proposal::load(&root, id).expect("load stale");
    let renewed = SceneEditTask::from_proposal(root.to_str().expect("utf8"), &stale)
        .expect("renewable task")
        .run(&MockAdapter::new(), &task_options(), |_| {})
        .expect("renew run");

    let new_id = renewed.data["proposal_id"].as_str().expect("new id");
    assert_ne!(new_id, id, "renewed review produces a fresh record");

    // The fresh record is computed against the *current* content: the user's
    // edit is part of its base, so it can never be silently overwritten.
    let renewed_record = proposal::load(&root, new_id).expect("load renewed");
    assert_eq!(
        renewed_record.base_sha256,
        proposal::sha256_hex(user_edit),
        "renewed base includes the user's later edit"
    );

    // It applies cleanly, and the rewrite covers scene 1 (the region that was
    // asked for), leaving scene 2 intact.
    proposal::apply(&root, new_id, None).expect("apply renewed");
    let after = screenplay(&root);
    assert!(
        after.contains("MOCK BRIEF"),
        "renewed rewrite applied: {after}"
    );
    assert!(
        after.contains("INT. CALLE - DÍA"),
        "scene 2 intact: {after}"
    );

    // The stale record stays stale — history is preserved, not rewritten.
    let stale = proposal::load(&root, id).expect("reload stale");
    assert_eq!(stale.state, ProposalState::Stale);
}

// --- Provider boundary --------------------------------------------------------

/// A test adapter that returns a *fixed* rewrite: makes applied content
/// assertable without depending on mock digest internals.
struct FixedRewriter {
    text: &'static str,
}

impl ModelAdapter for FixedRewriter {
    fn name(&self) -> &str {
        "fixed"
    }
    fn complete(
        &self,
        _request: &ModelRequest,
        _cancel: &CancellationToken,
    ) -> Result<narra_core::agent::ModelResponse, AgentError> {
        Ok(narra_core::agent::ModelResponse {
            text: self.text.to_string(),
            model: "fixed-1".to_string(),
            input_tokens: Some(1),
            output_tokens: Some(2),
        })
    }
}

#[test]
fn applied_content_is_exactly_what_the_model_returned() {
    let root = sample_project();
    let rewrite = "INT. TALLER - NOCHE\n\nELENA\nTodo está en silencio.\n";
    SceneEditTask::new(root.to_str().expect("utf8"), 1, "silencio")
        .run(&FixedRewriter { text: rewrite }, &task_options(), |_| {})
        .expect("task succeeds");

    let records = proposal::list(&root).expect("list");
    assert_eq!(records.len(), 1);
    proposal::apply(&root, &records[0].id, None).expect("apply");

    let after = screenplay(&root);
    assert_eq!(
        after,
        format!("{rewrite}\nINT. CALLE - DÍA\n\nMARCOS\nHace sol.\n"),
        "scene 1 replaced verbatim, scene 2 intact"
    );
}

#[test]
fn adapter_failure_produces_no_proposal_and_no_writes() {
    let root = sample_project();
    let before = screenplay(&root);

    struct FailingAdapter;
    impl ModelAdapter for FailingAdapter {
        fn name(&self) -> &str {
            "failing"
        }
        fn complete(
            &self,
            _request: &ModelRequest,
            _cancel: &CancellationToken,
        ) -> Result<narra_core::agent::ModelResponse, AgentError> {
            Err(AgentError::Adapter("provider rejected the request".into()))
        }
    }

    let error = SceneEditTask::new(root.to_str().expect("utf8"), 1, "reescribe")
        .run(&FailingAdapter, &task_options(), |_| {})
        .expect_err("adapter failure propagates");
    assert!(matches!(error, AgentError::Adapter(_)), "{error:?}");
    assert_eq!(screenplay(&root), before, "no canonical writes");
    assert!(
        proposal::list(&root).expect("list").is_empty(),
        "no proposal persisted on failure"
    );
}
