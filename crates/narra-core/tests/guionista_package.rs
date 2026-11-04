//! Spike I (D-062) fixture integrity: the portable `guionista` package.
//!
//! The package is *data*, not runtime: this test validates that the port of
//! `guionista-kit` is internally consistent — skills, portable agents
//! (`AGENT.md`) and slash commands load with the real loaders, and the
//! cross-references between `type.yaml` v2 and the shipped files resolve.

use std::fs;
use std::path::PathBuf;

use narra_core::agent::skill::{SkillOrigin, load_scope};
use narra_core::agent::{PackageOrigin, portable};
use narra_core::project_type::{TypeOrigin, load_package};
use saphyr::{LoadableYamlNode, Yaml};

fn package_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/packages/guionista")
}

/// The real v2 parser must accept and validate the ported package.
#[test]
fn the_real_v2_parser_loads_the_package() {
    let package = load_package(&package_root(), TypeOrigin::User).expect("load guionista");
    assert_eq!(package.id, "guionista");
    assert_eq!(package.format.key(), "fountain");
    let workspace = package.workspace.as_ref().expect("workspace");
    assert_eq!(workspace.mode.key(), "single");
    assert_eq!(workspace.folder, "guion");
    assert_eq!(
        workspace.default_document.as_deref(),
        Some("guion/main.fountain")
    );
    assert_eq!(package.pinned_context(), ["biblia", "estructura"]);
    assert_eq!(package.skills.len(), 9);
    assert_eq!(package.commands.len(), 8);
    assert_eq!(package.agents.len(), 4);

    let profile = package.profile();
    assert!(
        profile.screenplay,
        "fountain types keep screenplay mechanics"
    );
    assert!(profile.export_pdf);
    assert_eq!(profile.entities.len(), 2);
    assert_eq!(profile.entities[0].id, "personaje");
    assert_eq!(profile.entities[0].label.es, "Personaje");
    assert_eq!(profile.entities[1].id, "mundo");
    assert_eq!(profile.write_roots.len(), 5);
    assert_eq!(
        profile.write_roots[0].label.as_ref().map(|l| l.es.as_str()),
        Some("Biblia")
    );
    assert_eq!(profile.explorer_groups.len(), 4);

    let roots: Vec<&str> = package
        .write_roots
        .iter()
        .map(|root| root.path.as_str())
        .collect();
    assert_eq!(
        roots,
        ["biblia", "estructura", "investigacion", "notas", "guion"]
    );

    let kinds: Vec<&str> = package
        .context_actions
        .iter()
        .map(|action| match action.kind {
            narra_core::project_type::ContextActionKind::ConvertSelectionTo => "convert",
            narra_core::project_type::ContextActionKind::NewFileFromTemplate => "create",
            narra_core::project_type::ContextActionKind::RunCommand => "command",
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "convert", "convert", "create", "create", "command", "command"
        ]
    );
}

/// A package cannot widen the sandbox: a write root outside the declared
/// tree is rejected by the validator.
#[test]
fn write_roots_cannot_escape_the_declared_tree() {
    let text = "\
schema_version: 2
api_version: 1
id: escape
name: Escape
directories: [docs]
write_roots:
  - path: ../outside
    extensions: [.md]
";
    let error = narra_core::project_type::parse_package(text, TypeOrigin::User)
        .expect_err("escaping write root must be rejected");
    assert!(error.to_string().contains("write_roots"), "{error}");
}

fn type_yaml_text() -> String {
    fs::read_to_string(package_root().join("type.yaml")).expect("type.yaml")
}

fn load_type_yaml(text: &str) -> Yaml<'_> {
    Yaml::load_from_str(text)
        .expect("parse type.yaml")
        .remove(0)
}

fn string_list(value: Option<&Yaml>) -> Vec<String> {
    value.and_then(Yaml::as_vec).map_or_else(Vec::new, |items| {
        items
            .iter()
            .filter_map(Yaml::as_str)
            .map(str::to_string)
            .collect()
    })
}

#[test]
fn bundled_skills_load_with_the_real_loader() {
    let skills = load_scope(&package_root().join(".narra/skills"), SkillOrigin::Project)
        .expect("portable skills must satisfy the SKILL.md contract");
    let ids: Vec<&str> = skills.iter().map(|skill| skill.id.as_str()).collect();
    for expected in [
        "desarrollo-idea",
        "personajes",
        "metodos-estructura",
        "escaleta",
        "escribir-escena",
        "formato-fountain",
        "editor-guion",
        "investigacion",
        "microdrama",
    ] {
        assert!(ids.contains(&expected), "missing skill {expected}");
    }
}

#[test]
fn type_yaml_declares_only_files_that_exist() {
    let root = package_root();
    let text = type_yaml_text();
    let doc = load_type_yaml(&text);

    for key in ["agents", "commands", "skills"] {
        for id in string_list(doc.as_mapping_get(key)) {
            let path = match key {
                "agents" => root.join(".narra/agents").join(id).join("AGENT.md"),
                "commands" => root.join(".narra/commands").join(format!("{id}.md")),
                _ => root.join(".narra/skills").join(id).join("SKILL.md"),
            };
            assert!(
                path.is_file(),
                "declared {key} entry missing: {}",
                path.display()
            );
        }
    }

    for template in doc
        .as_mapping_get("templates")
        .and_then(Yaml::as_vec)
        .map_or(&[][..], |items| items.as_slice())
    {
        let source = template
            .as_mapping_get("source")
            .and_then(Yaml::as_str)
            .expect("template needs a source");
        assert!(
            root.join("templates").join(source).is_file(),
            "template source missing: {source}"
        );
    }
}

#[test]
fn write_roots_and_pinned_context_stay_inside_the_seed_tree() {
    let text = type_yaml_text();
    let doc = load_type_yaml(&text);
    let directories = string_list(doc.as_mapping_get("directories"));
    assert!(!directories.is_empty(), "the package must seed a tree");

    for pinned in string_list(doc.as_mapping_get("pinned_context")) {
        assert!(
            directories.contains(&pinned),
            "`pinned_context` entry `{pinned}` is not a declared directory"
        );
    }

    for write_root in doc
        .as_mapping_get("write_roots")
        .and_then(Yaml::as_vec)
        .map_or(&[][..], |items| items.as_slice())
        .iter()
    {
        let path = write_root
            .as_mapping_get("path")
            .and_then(Yaml::as_str)
            .expect("write root needs a path");
        assert!(
            directories.iter().any(|dir| dir == path),
            "write root `{path}` is not a declared directory"
        );
    }
}

#[test]
fn context_actions_resolve_inside_the_package() {
    let root = package_root();
    let text = type_yaml_text();
    let doc = load_type_yaml(&text);
    let directories = string_list(doc.as_mapping_get("directories"));

    for action in doc
        .as_mapping_get("context_actions")
        .and_then(Yaml::as_vec)
        .map_or(&[][..], |items| items.as_slice())
        .iter()
    {
        let kind = action
            .as_mapping_get("kind")
            .and_then(Yaml::as_str)
            .expect("context action needs a kind");
        assert!(
            [
                "convert_selection_to",
                "new_file_from_template",
                "run_command"
            ]
            .contains(&kind),
            "unknown context action kind `{kind}`"
        );

        if let Some(target) = action.as_mapping_get("target").and_then(Yaml::as_str) {
            let top = target.split('/').next().unwrap_or(target);
            assert!(
                directories.iter().any(|dir| dir == top),
                "context action target `{target}` escapes the seed tree"
            );
        }
        if let Some(template) = action.as_mapping_get("template").and_then(Yaml::as_str) {
            assert!(
                root.join("templates").join(template).is_file(),
                "context action template missing: {template}"
            );
        }
        if kind == "run_command" {
            let command = action
                .as_mapping_get("command")
                .and_then(Yaml::as_str)
                .expect("run_command action needs a command");
            assert!(
                root.join(".narra/commands")
                    .join(format!("{command}.md"))
                    .is_file(),
                "run_command references missing command `{command}`"
            );
        }
        for key in ["id", "label"] {
            assert!(
                action.as_mapping_get(key).is_some(),
                "context action lacks `{key}`"
            );
        }
    }
}

#[test]
fn bundled_portable_agents_load_with_the_real_loader() {
    let root = package_root();
    let agents = portable::discover_agents(&root, ".narra/agents", PackageOrigin::Project)
        .expect("portable AGENT.md files must satisfy the agent contract");
    let ids: Vec<&str> = agents.iter().map(|agent| agent.id.as_str()).collect();
    assert_eq!(ids, ["editor", "guionista", "investigador", "personaje"]);

    let mode_of = |id: &str| {
        agents
            .iter()
            .find(|agent| agent.id == id)
            .unwrap_or_else(|| panic!("missing agent {id}"))
            .mode
    };
    assert_eq!(mode_of("guionista"), portable::AgentMode::Build);
    assert_eq!(mode_of("investigador"), portable::AgentMode::Build);
    assert_eq!(mode_of("editor"), portable::AgentMode::Review);
    assert_eq!(mode_of("personaje"), portable::AgentMode::Plan);

    // `type.yaml` declares which agents ship, and every declared agent loads.
    let text = type_yaml_text();
    let doc = load_type_yaml(&text);
    let mut declared = string_list(doc.as_mapping_get("agents"));
    declared.sort();
    assert_eq!(declared, ids, "type.yaml and .narra/agents must agree");

    // Agents may only narrow the package contract: their write roots and
    // skills must be subsets of what `type.yaml` declares.
    let package = load_package(&root, TypeOrigin::User).expect("package");
    let package_roots: Vec<&str> = package
        .write_roots
        .iter()
        .map(|root| root.path.as_str())
        .collect();
    for agent in &agents {
        for write_root in &agent.write_roots {
            assert!(
                package_roots.contains(&write_root.as_str()),
                "agent `{}` declares write root `{write_root}` outside the package's write roots",
                agent.id
            );
        }
        for skill in &agent.skills {
            assert!(
                package.skills.contains(skill),
                "agent `{}` asks for skill `{skill}` the package does not ship",
                agent.id
            );
        }
    }
    let guionista = agents
        .iter()
        .find(|agent| agent.id == "guionista")
        .expect("guionista");
    assert_eq!(
        guionista.write_roots,
        ["biblia", "estructura", "guion", "investigacion"]
    );
    assert!(guionista.skills.contains(&"escaleta".to_string()));
    assert!(guionista.instructions.contains("AGENTS.md"));
    assert_eq!(
        guionista.hash.len(),
        64,
        "the file hash is the trust anchor"
    );
}

#[test]
fn bundled_commands_load_with_the_real_loader_and_resolve_their_agents() {
    let root = package_root();
    let commands = portable::discover_commands(&root, ".narra/commands", PackageOrigin::Project)
        .expect("command .md files must satisfy the slash-command contract");
    assert_eq!(commands.len(), 8, "the package ships eight commands");
    for id in [
        "idea",
        "investigar",
        "personaje",
        "estructura",
        "escaleta",
        "escena",
        "revisar",
        "voz",
    ] {
        assert!(
            commands.iter().any(|command| command.id == id),
            "missing command {id}"
        );
    }

    // `type.yaml`'s `commands` list (and every `run_command` context action)
    // must resolve to a command that actually loads.
    let text = type_yaml_text();
    let doc = load_type_yaml(&text);
    let mut declared = string_list(doc.as_mapping_get("commands"));
    declared.sort();
    let mut loaded: Vec<String> = commands.iter().map(|command| command.id.clone()).collect();
    loaded.sort();
    assert_eq!(declared, loaded, "type.yaml and .narra/commands must agree");

    // Every command resolves its `agent:` to a bundled portable agent.
    let agents =
        portable::discover_agents(&root, ".narra/agents", PackageOrigin::Project).expect("agents");
    for command in &commands {
        let agent = command
            .agent
            .as_deref()
            .unwrap_or_else(|| panic!("command `{}` must name an agent", command.id));
        assert!(
            agents.iter().any(|candidate| candidate.id == agent),
            "command `{}` references unknown agent `{agent}`",
            command.id
        );
        assert!(
            !command.prompt.trim().is_empty(),
            "command `{}` has no prompt",
            command.id
        );
        assert!(!command.description.trim().is_empty());
    }
}
