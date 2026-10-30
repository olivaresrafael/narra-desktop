//! D-047 evaluation fixture: eight representative chat prompts against a
//! deterministic fake model. The assertions are about context behavior, not
//! model quality: every successful lookup must be a visible source, forbidden
//! state must never appear, Plan/Review must not write, and an absent fact
//! must produce an explicit "not found" answer.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use narra_core::agent::{
    ChatAttachment, ChatMode, ChatTask, MockAdapter, ModelResponse, Skill, SkillOrigin, TaskOptions,
};
use narra_core::project::{create_project, create_project_from_package, open_project};
use narra_core::project_type::{TypeOrigin, load_package, trust_type};
use narra_core::{ProjectType, ProjectTypePackage};

fn temp_root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "narra-chat-fixture-{label}-{}",
        uuid::Uuid::new_v4()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("temp dir");
    root
}

fn feature_project() -> PathBuf {
    let root = temp_root("feature");
    let project =
        create_project(&root, "Fixture", ProjectType::Feature, "es").expect("create project");
    project
        .write_document(
            "bible/characters/marta.md",
            "# Marta\n\nMarta es la protagonista y quiere recuperar el faro.\n",
        )
        .expect("character");
    project
        .write_document(
            "bible/locations/casa.md",
            "# Casa\n\nLa casa de Marta está frente al mar.\n",
        )
        .expect("location");
    project
        .write_document(
            "structure/outline.md",
            "# Escaleta\n\n- Acto I: Marta vuelve.\n- Acto II: descubre la verdad.\n- Acto III: recupera el faro.\n",
        )
        .expect("outline");
    project
        .write_document(
            "notes/decisions.md",
            "# Decisiones\n\n- El faro es de la familia.\n",
        )
        .expect("decisions");
    project
        .write_document(
            ".narra/instructions.md",
            "# Instrucciones\n\nNo inventes canon. MARCA_FIXTURE.\n",
        )
        .expect("instructions");
    root
}

fn campaign_project() -> PathBuf {
    let source = temp_root("campaign-package");
    let package_dir = source.join("campaign");
    fs::create_dir_all(&package_dir).expect("package dir");
    fs::write(
        package_dir.join("type.yaml"),
        "schema_version: 1\napi_version: 1\nid: campaign\nname: Campaign\ndirectories: [brief]\nfiles:\n  - path: brief/brief.md\n    content:\n      en: |\n        # Brief\n        Launch the new product.\n      es: |\n        # Brief\n        Lanzar el producto nuevo.\ncontext:\n  pinned: [brief]\n",
    )
    .expect("type.yaml");
    let package: ProjectTypePackage =
        load_package(&package_dir, TypeOrigin::User).expect("package");
    let root = temp_root("campaign");
    create_project_from_package(&root, "Campaña", &package, "es").expect("create campaign");
    trust_type(&root, &package.id, &package.content_hash).expect("trust type");
    root
}

fn text_round(text: &str) -> ModelResponse {
    ModelResponse {
        text: text.to_string(),
        model: "fixture".to_string(),
        tool_calls: Vec::new(),
        input_tokens: Some(1),
        output_tokens: Some(1),
    }
}

fn tool_round(id: &str, name: &str, arguments: serde_json::Value) -> ModelResponse {
    ModelResponse {
        text: String::new(),
        model: "fixture".to_string(),
        tool_calls: vec![narra_core::agent::ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments,
        }],
        input_tokens: Some(1),
        output_tokens: Some(1),
    }
}

fn run(
    root: &Path,
    mode: ChatMode,
    message: &str,
    responses: Vec<ModelResponse>,
    skills: Vec<Skill>,
    attachments: Vec<ChatAttachment>,
) -> narra_core::agent::TaskOutput {
    let task = ChatTask::new(root.to_string_lossy(), mode, Vec::new(), message)
        .with_project_context(true)
        .with_skills(skills)
        .with_attachments(attachments);
    let output = task
        .run(
            &MockAdapter::scripted(responses),
            &TaskOptions::default(),
            |_| {},
        )
        .expect("chat fixture");
    assert_eq!(
        forbidden_source(&output),
        None,
        "forbidden source in prompt: {message}"
    );
    output
}

fn has_source(output: &narra_core::agent::TaskOutput, kind: &str, location: &str) -> bool {
    output
        .sources
        .iter()
        .any(|source| source.kind == kind && source.location == location)
}

fn forbidden_source(output: &narra_core::agent::TaskOutput) -> Option<&str> {
    output.sources.iter().find_map(|source| {
        let location = source.location.as_str();
        let hidden_narra = location.starts_with(".narra/") && source.kind != "instructions";
        let hidden_file = location.starts_with('.') && source.kind != "instructions";
        let forbidden = hidden_narra
            || hidden_file
            || location.starts_with(".git/")
            || location == ".git"
            || location.starts_with("exports/")
            || location == "agents.yaml"
            || location == "AGENTS.md";
        forbidden.then_some(location)
    })
}

fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(base: &Path, current: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        let Ok(entries) = fs::read_dir(current) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                walk(base, &path, out);
            } else if file_type.is_file() {
                let relative = path
                    .strip_prefix(base)
                    .expect("inside")
                    .to_string_lossy()
                    .into_owned();
                out.insert(relative, fs::read(&path).expect("read"));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

#[test]
fn eight_prompts_use_visible_context_without_writes_or_forbidden_sources() {
    let feature = feature_project();
    let campaign = campaign_project();
    let before = snapshot(&feature);
    let mut passed = 0usize;

    // 1. Character lookup outside the original four base documents.
    let output = run(
        &feature,
        ChatMode::Plan,
        "¿Quién es Marta?",
        vec![
            tool_round(
                "c1",
                "read_document",
                serde_json::json!({"path": "bible/characters/marta.md"}),
            ),
            text_round("Marta es la protagonista (bible/characters/marta.md)."),
        ],
        Vec::new(),
        Vec::new(),
    );
    if has_source(&output, "tool", "bible/characters/marta.md")
        && output.summary.contains("bible/characters/marta.md")
    {
        passed += 1;
    }

    // 2. Location lookup.
    let output = run(
        &feature,
        ChatMode::Plan,
        "¿Dónde vive Marta?",
        vec![
            tool_round(
                "c2",
                "read_document",
                serde_json::json!({"path": "bible/locations/casa.md"}),
            ),
            text_round("Vive en la casa frente al mar (bible/locations/casa.md)."),
        ],
        Vec::new(),
        Vec::new(),
    );
    if has_source(&output, "tool", "bible/locations/casa.md") {
        passed += 1;
    }

    // 3. Continuity answer grounded in the pinned decisions document.
    let output = run(
        &feature,
        ChatMode::Review,
        "¿Hay una contradicción sobre el faro?",
        vec![text_round(
            "El faro es de la familia según notes/decisions.md; no hay contradicción.",
        )],
        Vec::new(),
        Vec::new(),
    );
    if has_source(&output, "document", "notes/decisions.md") {
        passed += 1;
    }

    // 4. An absent fact must be answered explicitly, not invented.
    let output = run(
        &feature,
        ChatMode::Plan,
        "¿Quién es el dragón?",
        vec![
            tool_round(
                "c4",
                "search_project",
                serde_json::json!({"query": "dragón"}),
            ),
            text_round("No encuentro dragón en los documentos del proyecto."),
        ],
        Vec::new(),
        Vec::new(),
    );
    if output.summary.starts_with("No encuentro") && has_source(&output, "tool", "búsqueda: dragón")
    {
        passed += 1;
    }

    // 5. Cross-document outline read.
    let output = run(
        &feature,
        ChatMode::Plan,
        "¿Qué pasa en el Acto II?",
        vec![
            tool_round(
                "c5",
                "read_document",
                serde_json::json!({"path": "structure/outline.md"}),
            ),
            text_round("En el Acto II, Marta descubre la verdad (structure/outline.md)."),
        ],
        Vec::new(),
        Vec::new(),
    );
    if has_source(&output, "tool", "structure/outline.md") {
        passed += 1;
    }

    // 6. Type-specific pinned context: campaign brief, not the screenplay bible.
    let output = run(
        &campaign,
        ChatMode::Plan,
        "Resume el brief",
        vec![text_round(
            "La campaña busca lanzar el producto nuevo (brief/brief.md).",
        )],
        Vec::new(),
        Vec::new(),
    );
    if has_source(&output, "document", "brief/brief.md") {
        passed += 1;
    }

    // 7. Trusted skill metadata is visible; the body loads on demand.
    let skill = Skill {
        id: "brief".to_string(),
        name: "Brief reviewer".to_string(),
        description: "Revisa briefs.".to_string(),
        body: "Revisa objetivo y audiencia.".to_string(),
        origin: SkillOrigin::User,
        hash: "fixture".to_string(),
    };
    let output = run(
        &feature,
        ChatMode::Plan,
        "Revisa el brief con la skill",
        vec![
            tool_round("c7", "load_skill", serde_json::json!({"id": "brief"})),
            text_round("Revisé objetivo y audiencia (skill: brief)."),
        ],
        vec![skill],
        Vec::new(),
    );
    if has_source(&output, "tool", "skill: brief") {
        passed += 1;
    }

    // 8. Explicit attachment remains additive and wins over retrieved context.
    let output = run(
        &feature,
        ChatMode::Plan,
        "¿Cuál es la palabra clave?",
        vec![text_round("La palabra clave es FARO.")],
        Vec::new(),
        vec![ChatAttachment::new(
            "selection",
            "Selección",
            "screenplay/main.fountain:L1",
            "La palabra clave es FARO.",
        )],
    );
    if output.summary.contains("FARO")
        && has_source(&output, "selection", "screenplay/main.fountain:L1")
    {
        passed += 1;
    }

    let after = snapshot(&feature);
    assert_eq!(before, after, "Plan/Review must not write canonical files");
    assert_eq!(passed, 8, "expected all eight fixture prompts to pass");
}

#[test]
fn forbidden_project_state_never_becomes_a_source() {
    let root = feature_project();
    fs::create_dir_all(root.join(".git")).expect("git dir");
    fs::write(root.join(".git/HEAD"), "ref: refs/heads/main\n").expect("git head");
    fs::write(root.join("agents.yaml"), "secret: true\n").expect("agents");
    fs::write(root.join("AGENTS.md"), "coding-agent instructions\n").expect("agents md");
    let _ = open_project(&root).expect("project opens");
    let output = run(
        &root,
        ChatMode::Plan,
        "Resume el proyecto",
        vec![text_round("Resumen.")],
        Vec::new(),
        Vec::new(),
    );
    assert_eq!(forbidden_source(&output), None);
    assert!(has_source(
        &output,
        "instructions",
        ".narra/instructions.md"
    ));
}
