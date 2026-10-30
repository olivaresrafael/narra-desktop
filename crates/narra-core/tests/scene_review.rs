//! Integration tests for `scene_review` (Milestone 6): a read-only scene
//! review grounded in project material, with every context source shown and
//! canon never touched — headless, with an adapter that records the exact
//! request it received (so "grounded in project material" is asserted on
//! the actual prompt, not inferred from events).

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use narra_core::ProjectType;
use narra_core::agent::{
    AgentError, MockAdapter, ModelAdapter, ModelRequest, ModelResponse, SceneReviewTask, TaskEvent,
    TaskOptions,
};
use narra_core::cancel::CancellationToken;
use narra_core::project::create_project;

fn temp_root() -> PathBuf {
    tempfile::tempdir().expect("tempdir").keep()
}

/// Two scenes plus material in every directory the review considers, and one
/// file in a directory it must ignore (`reviews/`).
fn sample_project() -> PathBuf {
    let root = temp_root();
    let project =
        create_project(&root, "Luces de Invierno", ProjectType::Feature, "es").expect("create");
    project
        .write_document(
            narra_core::DEFAULT_SCREENPLAY,
            "INT. TALLER - NOCHE\n\nELENA\nEsto es una prueba.\n\nINT. CALLE - DÍA\n\nMARCOS\nHace sol.\n",
        )
        .expect("screenplay");
    project
        .write_document(
            "bible/characters/elena.md",
            "# Elena\nArtesana del taller.\n",
        )
        .expect("bible");
    project
        .write_document(
            "structure/outline.md",
            "# Outline\nActo 1: el taller arde.\n",
        )
        .expect("outline");
    project
        .write_document("research/fuentes.md", "# Fuentes\nEntrevistas 2026.\n")
        .expect("research");
    project
        .write_document(
            "notes/decisions.md",
            "# Decisions\n- El taller cierra al final.\n",
        )
        .expect("decisions");
    project
        .write_document(
            "notes/open-questions.md",
            "# Open questions\n- ¿Quién prende fuego?\n",
        )
        .expect("questions");
    project
        .write_document("reviews/old.md", "# Old review\n")
        .expect("review");
    root
}

fn review_options() -> TaskOptions {
    TaskOptions {
        timeout: Duration::from_secs(30),
        cancel: CancellationToken::new(),
        stream: false,
    }
}

/// Adapter that records the exact request it received (grounding checks).
struct RecordingAdapter {
    request: RefCell<Option<ModelRequest>>,
}

impl RecordingAdapter {
    fn new() -> Self {
        Self {
            request: RefCell::new(None),
        }
    }
}

impl ModelAdapter for RecordingAdapter {
    fn name(&self) -> &str {
        "recording"
    }

    fn complete(
        &self,
        request: &ModelRequest,
        _cancel: &CancellationToken,
    ) -> Result<ModelResponse, AgentError> {
        *self.request.borrow_mut() = Some(request.clone());
        Ok(ModelResponse {
            text: "1) Lectura.\n2) Continuidad: sin contradicciones.\n3) Diálogo plano.\n\
                   4) Acortar la réplica 2."
                .to_string(),
            model: "recording-1".to_string(),
            tool_calls: Vec::new(),
            input_tokens: Some(11),
            output_tokens: Some(22),
        })
    }
}

/// Every file in the project — hidden included — with its content. Catches
/// any write, including proposals under `.narra/`.
fn snapshot(root: &Path) -> Vec<(String, String)> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<(String, String)>) {
        let mut entries: Vec<_> = fs::read_dir(dir)
            .expect("read_dir")
            .map(|entry| entry.expect("entry"))
            .collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .expect("inside root")
                .to_string_lossy()
                .replace('\\', "/");
            if path.is_dir() {
                walk(&path, root, out);
            } else {
                let content = fs::read_to_string(&path).unwrap_or_default();
                out.push((relative, content));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out
}

#[test]
fn review_is_read_only_and_shows_every_source() {
    let root = sample_project();
    let before = snapshot(&root);
    let mut events: Vec<TaskEvent> = Vec::new();

    let output = SceneReviewTask::new(root.to_str().expect("utf8"), 1, "")
        .run(&RecordingAdapter::new(), &review_options(), |event| {
            events.push(event)
        })
        .expect("task succeeds");

    assert_eq!(output.exit.exit_code(), 0);
    assert_eq!(
        output.data["scene_heading"].as_str(),
        Some("INT. TALLER - NOCHE")
    );
    assert_eq!(output.data["scene_range"].as_str(), Some("L1-L4"));
    assert_eq!(output.data["adapter"].as_str(), Some("recording"));
    assert_eq!(output.data["model"].as_str(), Some("recording-1"));
    assert!(!output.data["review"].as_str().expect("review").is_empty());

    // Every narrative-context document was sent, deterministically sorted;
    // `reviews/` (not a context dir), the manifest and the screenplay are
    // not double-counted as material.
    let docs: Vec<&str> = output.data["material_docs"]
        .as_array()
        .expect("material_docs array")
        .iter()
        .map(|value| value.as_str().expect("path"))
        .collect();
    assert_eq!(
        docs,
        vec![
            "bible/characters/elena.md",
            "notes/decisions.md",
            "notes/open-questions.md",
            "research/fuentes.md",
            "structure/outline.md",
        ]
    );

    // Sources are auditable: manifest, the scene region and one per doc.
    let scene_location = output
        .sources
        .iter()
        .find(|source| source.kind == "scene")
        .expect("scene source");
    assert_eq!(scene_location.location, "screenplay/main.fountain:L1-L4");
    let material_locations: Vec<&str> = output
        .sources
        .iter()
        .filter(|source| source.kind != "manifest" && source.kind != "scene")
        .map(|source| source.location.as_str())
        .collect();
    assert_eq!(material_locations, docs, "sources match material_docs");
    assert!(
        !material_locations
            .iter()
            .any(|location| location.starts_with("reviews/")),
        "{material_locations:?}"
    );

    // Event order: started → progress → context → model → completed.
    assert!(matches!(
        events.first(),
        Some(TaskEvent::Started { task, .. }) if task == "scene_review"
    ));
    assert!(matches!(events.last(), Some(TaskEvent::Completed { .. })));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, TaskEvent::ContextCollected { sources } if sources.len() == output.sources.len()))
    );
    assert!(events.iter().any(
        |event| matches!(event, TaskEvent::ModelResponded { model, .. } if model == "recording-1")
    ));

    // Canon untouched: byte-identical tree, and no proposal state created.
    let after = snapshot(&root);
    assert_eq!(before, after, "scene_review must not touch any file");
    assert!(
        !root.join(".narra/proposals").exists() && !root.join(".narra/chats").exists(),
        "a read-only task must not create proposal or chat state"
    );
}

#[test]
fn review_prompt_is_grounded_in_scene_and_material() {
    let root = sample_project();
    let adapter = RecordingAdapter::new();

    let output = SceneReviewTask::new(
        root.to_str().expect("utf8"),
        1,
        "¿respeta la línea de Elena?",
    )
    .run(&adapter, &review_options(), |_| {})
    .expect("task succeeds");

    let recorded = adapter.request.borrow();
    let request = recorded.as_ref().expect("request recorded");

    // The scene itself, quoted whole.
    assert!(request.prompt.contains("INT. TALLER - NOCHE"));
    assert!(request.prompt.contains("Esto es una prueba."));
    // Project material from each context directory, quoted with its path.
    assert!(request.prompt.contains("--- bible/characters/elena.md ---"));
    assert!(request.prompt.contains("Artesana del taller."));
    assert!(request.prompt.contains("El taller cierra al final."));
    assert!(request.prompt.contains("¿Quién prende fuego?"));
    assert!(request.prompt.contains("Entrevistas 2026."));
    assert!(request.prompt.contains("Acto 1: el taller arde."));
    // The focus travels with the context…
    assert!(request.prompt.contains("¿respeta la línea de Elena?"));
    // …and non-context material stays out.
    assert!(!request.prompt.contains("Old review"));
    // Read-only by construction: no tools offered to the model.
    assert!(request.tools.is_empty());
    assert!(request.max_output_tokens > 0);

    // The focus is also shown to the user as its own source.
    let focus_source = output
        .sources
        .iter()
        .find(|source| source.kind == "focus")
        .expect("focus source");
    assert_eq!(focus_source.detail, "¿respeta la línea de Elena?");
    assert_eq!(
        output.data["focus"].as_str(),
        Some("¿respeta la línea de Elena?")
    );
}

#[test]
fn missing_scene_is_an_explicit_error() {
    let root = sample_project();
    let root = root.to_str().expect("utf8");

    let error = SceneReviewTask::new(root, 9, "")
        .run(&MockAdapter::new(), &review_options(), |_| {})
        .expect_err("scene 9 does not exist");
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

    let error = SceneReviewTask::new(root, 0, "")
        .run(&MockAdapter::new(), &review_options(), |_| {})
        .expect_err("scene numbering starts at 1");
    assert!(
        matches!(error, AgentError::InvalidInput("scene numbers start at 1")),
        "{error:?}"
    );
}

#[test]
fn oversized_material_is_truncated_and_the_source_says_so() {
    let root = sample_project();
    let project = narra_core::project::open_project(&root).expect("open");
    // ~4,600 chars: over the 2,000-char per-document context cap, with a
    // unique marker at the very end (repeated text would appear anywhere).
    let mut long = "El mundo entero. ".repeat(270);
    long.push_str("MARCADOR-FINAL-UNICO");
    project
        .write_document("bible/world.md", &long)
        .expect("world");

    let adapter = RecordingAdapter::new();
    let output = SceneReviewTask::new(root.to_str().expect("utf8"), 1, "")
        .run(&adapter, &review_options(), |_| {})
        .expect("task succeeds");

    let source = output
        .sources
        .iter()
        .find(|source| source.location == "bible/world.md")
        .expect("world.md source");
    assert!(source.detail.contains("truncado"), "{}", source.detail);

    let recorded = adapter.request.borrow();
    let request = recorded.as_ref().expect("request recorded");
    assert!(request.prompt.contains("El mundo entero."), "head is sent");
    assert!(
        !request.prompt.contains("MARCADOR-FINAL-UNICO"),
        "the tail beyond the cap must not be sent"
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
        stream: false,
    };
    let mut events: Vec<TaskEvent> = Vec::new();

    let error = SceneReviewTask::new(root.to_str().expect("utf8"), 1, "")
        .run(&MockAdapter::new(), &options, |event| events.push(event))
        .expect_err("cancelled before any work");

    assert!(matches!(error, AgentError::Cancelled), "{error:?}");
    // Only the Started event: no project read, no context, no model call.
    assert_eq!(events.len(), 1, "{events:?}");
    assert!(matches!(events[0], TaskEvent::Started { .. }));
}

#[test]
fn review_runs_under_the_configured_role_and_refuses_foreign_profiles() {
    use narra_core::agent::roles;

    let root = sample_project();
    let builtins = roles::builtin_roles();
    let role = roles::role_by_id(&builtins, "editor-reviewer")
        .expect("editor-reviewer")
        .clone();
    let adapter = RecordingAdapter::new();
    let output = SceneReviewTask::new(root.to_str().expect("utf8"), 1, "")
        .with_role(role)
        .run(&adapter, &review_options(), |_| {})
        .expect("review");
    let request = adapter.request.borrow().clone().expect("recorded request");
    assert!(
        request
            .system
            .contains("PERFIL (rol `editor-reviewer`, builtin)"),
        "the role persona rides in the system prompt"
    );
    assert_eq!(output.data["role"], "editor-reviewer");
    assert_eq!(output.data["role_source"], "builtin");

    // A role whose profile does not grant scene_review is refused before
    // any model call (M6 slice 7 permission profile).
    let writer = roles::role_by_id(&builtins, "writer")
        .expect("writer")
        .clone();
    let denied = RecordingAdapter::new();
    let error = SceneReviewTask::new(root.to_str().expect("utf8"), 1, "")
        .with_role(writer)
        .run(&denied, &review_options(), |_| {})
        .expect_err("denied");
    assert!(
        matches!(
            &error,
            AgentError::RoleDenied { role, task } if role == "writer" && task == "scene_review"
        ),
        "{error:?}"
    );
    assert!(denied.request.borrow().is_none(), "no model call happened");
}
