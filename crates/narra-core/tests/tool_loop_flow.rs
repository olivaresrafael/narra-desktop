//! Integration tests for the permission-gated tool loop (Milestone 4,
//! slice 3b). A scripted mock adapter plays the model, so every round is
//! deterministic and offline; the assertions are about *policy behavior*:
//! what executes, what is refused, what the audit shows and what the
//! canonical files look like afterwards.

use std::fs;
use std::path::{Path, PathBuf};

use narra_core::agent::{
    AgentError, GrantPolicy, MockAdapter, ModelResponse, TaskEvent, TaskExit, TaskOptions,
    ToolLoopTask, ToolOutcome, ToolSpec,
};
use narra_core::cancel::CancellationToken;

fn sample_project() -> PathBuf {
    let root = std::env::temp_dir().join(format!("narra-loop-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(root.join("screenplay")).expect("mkdir");
    fs::write(
        root.join("project.yaml"),
        "schema_version: 1\nproject_id: 00000000-0000-4000-8000-000000000002\n\
         title: Bucle de Prueba\nproject_type: feature\nprimary_language: es\n\
         additional_languages: []\ncreated_at: 2026-01-01T00:00:00Z\n\
         default_screenplay_path: screenplay/main.fountain\n",
    )
    .expect("manifest");
    fs::write(
        root.join("screenplay/main.fountain"),
        "INT. TALLER - NOCHE\n\nELENA\nEsto es una prueba.\n\nINT. CALLE - DÍA\n\nMARCOS\nHace sol.\n",
    )
    .expect("screenplay");
    root
}

fn options() -> TaskOptions {
    TaskOptions {
        timeout: std::time::Duration::from_secs(30),
        cancel: CancellationToken::new(),
        stream: false,
    }
}

/// A model response that requests one tool call.
fn tool_round(model: &str, id: &str, name: &str, arguments: serde_json::Value) -> ModelResponse {
    ModelResponse {
        text: String::new(),
        model: model.to_string(),
        tool_calls: vec![narra_core::agent::ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments,
        }],
        input_tokens: Some(1),
        output_tokens: Some(1),
    }
}

/// The final text-only round.
fn text_round(model: &str, text: &str) -> ModelResponse {
    ModelResponse {
        text: text.to_string(),
        model: model.to_string(),
        tool_calls: Vec::new(),
        input_tokens: Some(1),
        output_tokens: Some(1),
    }
}

fn rewrite_arguments() -> serde_json::Value {
    serde_json::json!({
        "scene": 1,
        "instruction": "haz el diálogo más tenso",
        "rewritten_scene": "INT. TALLER - NOCHE\n\nELENA\nCerrando cuentas."
    })
}

#[test]
fn granted_loop_reads_and_stages_a_proposal_without_touching_canon() {
    let root = sample_project();
    let canon_before =
        fs::read_to_string(root.join("screenplay/main.fountain")).expect("read canon");

    // Round 1: request read_scene. Round 2: propose. Round 3: final text.
    let adapter = MockAdapter::scripted(vec![
        tool_round(
            "mock-1",
            "call-1",
            "read_scene",
            serde_json::json!({"scene": 1}),
        ),
        tool_round(
            "mock-1",
            "call-2",
            "propose_scene_edit",
            rewrite_arguments(),
        ),
        text_round("mock-1", "Listo: leí la escena y dejé una propuesta."),
    ]);
    let mut policy = GrantPolicy::new().allow("propose_scene_edit");
    let mut events = Vec::new();
    let output = ToolLoopTask::new(root.to_str().expect("utf8"), "haz el diálogo más tenso")
        .run(&adapter, &mut policy, &options(), |e| events.push(e))
        .expect("loop completes");

    assert_eq!(output.exit, TaskExit::Success);
    assert_eq!(output.data["model"], "mock-1");

    // Audit: two calls, both allowed.
    let tools = output.data["tools"].as_array().expect("audit array");
    assert_eq!(tools.len(), 2, "audit: {tools:?}");
    assert_eq!(tools[0]["tool"], "read_scene");
    assert_eq!(tools[1]["tool"], "propose_scene_edit");
    assert!(tools.iter().all(|entry| entry["decision"] == "Allow"));
    let ToolOutcome::ProposalCreated { .. } = outcome_of(&tools[1]) else {
        panic!("proposal expected: {}", tools[1]);
    };

    // Events surface each call.
    let tool_events: Vec<_> = events
        .iter()
        .filter(|event| matches!(event, TaskEvent::ToolExecuted { .. }))
        .collect();
    assert_eq!(tool_events.len(), 2, "{tool_events:?}");

    // Canon untouched; exactly one reviewable proposal exists.
    let canon_after =
        fs::read_to_string(root.join("screenplay/main.fountain")).expect("read canon");
    assert_eq!(canon_before, canon_after, "canon must be untouched");
    let proposals = narra_core::agent::proposal::list(&root).expect("list");
    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0].target, "screenplay/main.fountain");
    assert_eq!(proposals[0].origin.task, "tool_loop");
}

fn outcome_of(entry: &serde_json::Value) -> ToolOutcome {
    serde_json::from_value(entry["outcome"].clone()).expect("outcome")
}

#[test]
fn denied_write_is_visible_in_audit_and_canon_survives() {
    let root = sample_project();
    let canon_before =
        fs::read_to_string(root.join("screenplay/main.fountain")).expect("read canon");

    // The model asks to write with *no* grants at all.
    let adapter = MockAdapter::scripted(vec![
        tool_round(
            "mock-1",
            "call-1",
            "propose_scene_edit",
            rewrite_arguments(),
        ),
        text_round("mock-1", "No me permitieron editar; lo dejo indicado."),
    ]);
    let mut policy = GrantPolicy::new(); // deny-by-default
    let mut events = Vec::new();
    let output = ToolLoopTask::new(root.to_str().expect("utf8"), "haz el diálogo más tenso")
        .run(&adapter, &mut policy, &options(), |e| events.push(e))
        .expect("loop still completes: denials are outcomes, not crashes");

    let tools = output.data["tools"].as_array().expect("audit array");
    assert_eq!(tools.len(), 1);
    assert!(
        tools[0]["decision"]["Ask"].is_string(),
        "{}",
        tools[0]["decision"]
    );
    let ToolOutcome::Denied { reason } = outcome_of(&tools[0]) else {
        panic!("denied expected: {}", tools[0]);
    };
    assert!(
        reason.contains("permisos") || reason.contains("confirma"),
        "{reason}"
    );

    // The model saw the denial as a tool result.
    assert!(
        output.data["final_text"]
            .as_str()
            .expect("text")
            .contains("permitieron"),
        "model adapted to the denial: {}",
        output.data["final_text"]
    );

    // The denied event is recorded with allowed=false.
    assert!(
        events
            .iter()
            .any(|event| matches!(event, TaskEvent::ToolExecuted { allowed: false, .. }))
    );

    // No proposal, no canon change.
    assert!(
        narra_core::agent::proposal::list(&root)
            .expect("list")
            .is_empty()
    );
    let canon_after =
        fs::read_to_string(root.join("screenplay/main.fountain")).expect("read canon");
    assert_eq!(canon_before, canon_after);
}

#[test]
fn destructive_apply_is_denied_even_when_writes_are_granted() {
    let root = sample_project();
    // Stage a proposal first (as a prior granted run would have).
    let mut policy = GrantPolicy::new().allow("propose_scene_edit");
    let mut executor = narra_core::agent::tools::ToolExecutor::new(Path::new(&root));
    let origin = narra_core::agent::proposal::Origin {
        task: "tool_loop".to_string(),
        adapter: "mock".to_string(),
        model: "mock-1".to_string(),
        task_input: serde_json::json!({"instruction": "x"}),
    };
    let created = executor.execute(
        &narra_core::agent::ToolCall {
            id: "c0".to_string(),
            name: "propose_scene_edit".to_string(),
            arguments: rewrite_arguments(),
        },
        &mut policy,
        &origin,
    );
    let narra_core::agent::ToolOutcome::ProposalCreated { id, .. } = created.outcome else {
        panic!("proposal expected: {:?}", created.outcome);
    };
    let canon_before =
        fs::read_to_string(root.join("screenplay/main.fountain")).expect("read canon");

    // The model asks to apply it — the write grant must NOT cover this.
    let adapter = MockAdapter::scripted(vec![
        tool_round(
            "mock-1",
            "call-1",
            "apply_proposal",
            serde_json::json!({"id": id}),
        ),
        text_round("mock-1", "La aplicación fue denegada."),
    ]);
    let mut events = Vec::new();
    let output = ToolLoopTask::new(root.to_str().expect("utf8"), "aplica la propuesta")
        .run(&adapter, &mut policy, &options(), |e| events.push(e))
        .expect("loop completes");

    let tools = output.data["tools"].as_array().expect("audit array");
    assert_eq!(tools.len(), 1);
    assert!(
        tools[0]["decision"]["Deny"].is_string(),
        "{}",
        tools[0]["decision"]
    );
    let ToolOutcome::Denied { reason } = outcome_of(&tools[0]) else {
        panic!("denied expected: {}", tools[0]);
    };
    assert!(reason.contains("destructiva"), "{reason}");

    let canon_after =
        fs::read_to_string(root.join("screenplay/main.fountain")).expect("read canon");
    assert_eq!(canon_before, canon_after, "canon must be untouched");
    // The proposal is still ready for review.
    let record = narra_core::agent::proposal::load(Path::new(&root), &id).expect("load");
    assert_eq!(
        format!("{:?}", record.state),
        "ReadyForReview",
        "proposal must stay reviewable"
    );
}

#[test]
fn explicit_destructive_grant_applies_in_the_loop() {
    let root = sample_project();
    let adapter = MockAdapter::scripted(vec![
        tool_round(
            "mock-1",
            "call-1",
            "propose_scene_edit",
            rewrite_arguments(),
        ),
        // The model cannot know the id it just created... the loop feeds it
        // back as text, so the scripted second round is the final answer and
        // the apply happens in a separate direct-executor check below.
        text_round("mock-1", "Propuesta lista para revisión."),
    ]);
    let mut policy = GrantPolicy::new()
        .allow("propose_scene_edit")
        .allow_destructive("apply_proposal");
    let output = ToolLoopTask::new(root.to_str().expect("utf8"), "haz el diálogo más tenso")
        .run(&adapter, &mut policy, &options(), |_| {})
        .expect("loop completes");
    let tools = output.data["tools"].as_array().expect("audit");
    assert_eq!(tools[0]["decision"], "Allow");

    // With the explicit destructive grant, the same executor applies it.
    let narra_core::agent::ToolOutcome::ProposalCreated { id, .. } = outcome_of(&tools[0]) else {
        panic!("proposal expected: {}", tools[0]);
    };
    let mut executor = narra_core::agent::tools::ToolExecutor::new(Path::new(&root));
    let origin = narra_core::agent::proposal::Origin {
        task: "tool_loop".to_string(),
        adapter: "mock".to_string(),
        model: "mock-1".to_string(),
        task_input: serde_json::json!({"instruction": "x"}),
    };
    let applied = executor.execute(
        &narra_core::agent::ToolCall {
            id: "c9".to_string(),
            name: "apply_proposal".to_string(),
            arguments: serde_json::json!({"id": id}),
        },
        &mut policy,
        &origin,
    );
    assert!(
        matches!(applied.outcome, ToolOutcome::ProposalApplied { .. }),
        "{:?}",
        applied.outcome
    );
    let canon = fs::read_to_string(root.join("screenplay/main.fountain")).expect("read");
    assert!(canon.contains("Cerrando cuentas."), "applied to canon");
}

#[test]
fn loop_honors_cancellation_between_rounds() {
    let root = sample_project();
    let adapter = MockAdapter::scripted(vec![
        tool_round(
            "mock-1",
            "call-1",
            "read_scene",
            serde_json::json!({"scene": 1}),
        ),
        text_round("mock-1", "nunca llegaré"),
    ]);
    let mut policy = GrantPolicy::new();
    let token = CancellationToken::new();
    token.cancel(); // cancelled before the first round
    let options = TaskOptions {
        timeout: std::time::Duration::from_secs(30),
        cancel: token,
        stream: false,
    };
    let error = ToolLoopTask::new(root.to_str().expect("utf8"), "instrucción")
        .run(&adapter, &mut policy, &options, |_| {})
        .expect_err("cancelled before start");
    assert!(matches!(error, AgentError::Cancelled));
}

#[test]
fn tool_specs_declare_permissions_and_schemas() {
    let specs: Vec<ToolSpec> = narra_core::agent::tools::tool_specs();
    assert_eq!(specs.len(), 3);
    for spec in &specs {
        assert_eq!(spec.parameters["type"], "object", "{}", spec.name);
        assert!(!spec.description.is_empty());
    }
    let apply = specs
        .iter()
        .find(|spec| spec.name == "apply_proposal")
        .expect("apply declared");
    assert_eq!(
        format!("{:?}", apply.permission),
        "Destructive",
        "apply must be classified destructive"
    );
}
