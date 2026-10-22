//! Integration tests for the first agent task (`project_brief`, Milestone 4):
//! UI-neutral events, shown context sources, cancellation and defined exit
//! status — all headless, with the deterministic mock adapter (Spike C/F).

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use narra_core::ProjectType;
use narra_core::agent::{
    AgentError, MockAdapter, ModelAdapter, ModelRequest, ProjectBriefTask, TaskEvent, TaskOptions,
};
use narra_core::cancel::CancellationToken;
use narra_core::git::{OpOptions, Repository};
use narra_core::project::create_project;

fn temp_root() -> PathBuf {
    tempfile::tempdir().expect("tempdir").keep()
}

fn sample_project() -> PathBuf {
    let root = temp_root();
    let project =
        create_project(&root, "Luces de Invierno", ProjectType::Feature, "es").expect("create");
    project
        .write_document(
            narra_core::DEFAULT_SCREENPLAY,
            "INT. TALLER - NOCHE\n\nELENA\nEsto es una prueba.\n",
        )
        .expect("screenplay");
    root
}

fn brief_options() -> TaskOptions {
    TaskOptions {
        timeout: Duration::from_secs(30),
        cancel: CancellationToken::new(),
    }
}

#[test]
fn brief_runs_headless_and_emits_ordered_events() {
    let root = sample_project();
    let mut events: Vec<TaskEvent> = Vec::new();

    let output = ProjectBriefTask::new(root.to_str().expect("utf8"))
        .run(&MockAdapter::new(), &brief_options(), |event| {
            events.push(event)
        })
        .expect("task succeeds");

    // Defined exit status and machine-readable payload.
    assert_eq!(output.exit.exit_code(), 0);
    assert_eq!(
        output.data["project_title"].as_str(),
        Some("Luces de Invierno")
    );
    assert_eq!(output.data["total_scenes"].as_u64(), Some(1));
    assert_eq!(output.data["adapter"].as_str(), Some("mock"));

    // Event order: started → progress → context → model → completed.
    assert!(
        matches!(events.first(), Some(TaskEvent::Started { task, .. }) if task == "project_brief")
    );
    assert!(matches!(events.last(), Some(TaskEvent::Completed { .. })));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, TaskEvent::ContextCollected { sources } if !sources.is_empty())),
        "context used must be shown as events"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, TaskEvent::ModelResponded { model, .. } if model == "mock-1")),
        "model provenance must be reported"
    );

    // Sources are auditable: manifest, scene index and git status are listed.
    let kinds: Vec<&str> = output.sources.iter().map(|s| s.kind.as_str()).collect();
    assert!(kinds.contains(&"manifest"), "{kinds:?}");
    assert!(kinds.contains(&"scene_index"), "{kinds:?}");
    assert!(kinds.contains(&"git_status"), "{kinds:?}");
    // The sample project is a git-less directory: reported, not fatal.
    assert!(
        output.data["git"]
            .as_str()
            .expect("git summary")
            .contains("sin repositorio"),
        "{:?}",
        output.data["git"]
    );
}

#[test]
fn brief_reports_git_status_when_repository_exists() {
    let root = sample_project();
    Repository::init_with_branch(&root, "main", OpOptions::default()).expect("git init");

    let output = ProjectBriefTask::new(root.to_str().expect("utf8"))
        .run(&MockAdapter::new(), &brief_options(), |_| {})
        .expect("task succeeds");
    let git = output.data["git"].as_str().expect("git summary");
    assert!(git.contains("rama main"), "{git}");
    // The fresh project's template files are all untracked: status sees them.
    assert!(
        git.contains("4 archivos"),
        "template files untracked: {git}"
    );
}

#[test]
fn brief_is_deterministic_with_the_mock_adapter() {
    let root = sample_project();
    let run = || {
        ProjectBriefTask::new(root.to_str().expect("utf8"))
            .run(&MockAdapter::new(), &brief_options(), |_| {})
            .expect("task succeeds")
    };
    assert_eq!(
        run().data["brief"],
        run().data["brief"],
        "same input, same output"
    );
}

#[test]
fn pre_cancelled_task_stops_before_any_work() {
    let root = sample_project();
    let token = CancellationToken::new();
    token.cancel();
    let options = TaskOptions {
        timeout: Duration::from_secs(30),
        cancel: token,
    };
    let mut events = Vec::new();
    let error = ProjectBriefTask::new(root.to_str().expect("utf8"))
        .run(&MockAdapter::new(), &options, |event| events.push(event))
        .expect_err("cancelled before start");
    assert!(matches!(error, AgentError::Cancelled), "{error:?}");
    assert!(matches!(events.first(), Some(TaskEvent::Started { .. })));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, TaskEvent::ModelResponded { .. })),
        "no model call after cancellation"
    );
}

#[test]
fn cancellation_during_model_call_stops_the_task() {
    let root = sample_project();
    let token = CancellationToken::new();
    let killer = token.clone();
    let options = TaskOptions {
        timeout: Duration::from_secs(30),
        cancel: token,
    };
    // Cancel while the mock adapter is "thinking" (10 s simulated latency).
    let handle = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        killer.cancel();
    });
    let started = std::time::Instant::now();
    let error = ProjectBriefTask::new(root.to_str().expect("utf8"))
        .run(
            &MockAdapter::with_latency(Duration::from_secs(10)),
            &options,
            |_| {},
        )
        .expect_err("must cancel mid-flight");
    handle.join().expect("killer thread");
    assert!(matches!(error, AgentError::Cancelled), "{error:?}");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "cancellation must be prompt, took {:?}",
        started.elapsed()
    );
}

#[test]
fn empty_project_root_is_rejected_before_opening() {
    let error = ProjectBriefTask::new("   ")
        .run(&MockAdapter::new(), &brief_options(), |_| {})
        .expect_err("blank root");
    assert!(matches!(error, AgentError::InvalidInput(_)), "{error:?}");
}

#[test]
fn brief_leaves_canonical_files_unchanged() {
    let root = sample_project();
    let screenplay = root.join(narra_core::DEFAULT_SCREENPLAY);
    let before = fs::read(&screenplay).expect("read");

    ProjectBriefTask::new(root.to_str().expect("utf8"))
        .run(&MockAdapter::new(), &brief_options(), |_| {})
        .expect("task succeeds");

    assert_eq!(
        fs::read(&screenplay).expect("read"),
        before,
        "read-only task"
    );
}

// --- Provider boundary (Spike C: mock first, HTTP adapter later) -----------

/// A hostile adapter used only in tests: proves tasks do not trust providers.
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

#[test]
fn adapter_failure_becomes_a_failed_task_not_a_panic() {
    let root = sample_project();
    let mut events = Vec::new();
    let error = ProjectBriefTask::new(root.to_str().expect("utf8"))
        .run(&FailingAdapter, &brief_options(), |e| events.push(e))
        .expect_err("adapter failure propagates");
    assert!(matches!(error, AgentError::Adapter(_)), "{error:?}");
    // Context was already shown before the failure — auditable either way.
    assert!(
        events
            .iter()
            .any(|e| matches!(e, TaskEvent::ContextCollected { .. }))
    );
}
