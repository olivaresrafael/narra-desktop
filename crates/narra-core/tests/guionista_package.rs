//! Spike I (D-062) fixture integrity: the portable `guionista` package.
//!
//! The package is *data*, not runtime: this test validates that the port of
//! `guionista-kit` is internally consistent — skills load with the real
//! loader, cross-references between `type.yaml` v2 and the shipped files
//! resolve, and declared write roots stay inside the seeded tree. Runtime
//! support for `format`, `context_actions`, commands and portable agents
//! lands in Fase 1; until then this test only guards the contract draft.

use std::fs;
use std::path::PathBuf;

use narra_core::agent::skill::{SkillOrigin, load_scope};
use saphyr::{LoadableYamlNode, Yaml};

fn package_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/packages/guionista")
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
fn portable_agents_declare_mode_and_roots() {
    let agents = package_root().join(".narra/agents");
    let mut seen = 0;
    for entry in fs::read_dir(&agents).expect("agents dir") {
        let dir = entry.expect("entry").path();
        if !dir.is_dir() {
            continue;
        }
        seen += 1;
        let file = dir.join("AGENT.md");
        let text =
            fs::read_to_string(&file).unwrap_or_else(|_| panic!("missing {}", file.display()));
        assert!(
            text.starts_with("---"),
            "frontmatter required: {}",
            file.display()
        );
        let front = text
            .split("---")
            .nth(1)
            .expect("closing frontmatter")
            .trim();
        let parsed = Yaml::load_from_str(front).expect("frontmatter yaml");
        let agent = parsed.first().expect("frontmatter document");
        for key in ["name", "description", "mode", "write_roots"] {
            assert!(
                agent.as_mapping_get(key).is_some(),
                "{} lacks `{key}`",
                file.display()
            );
        }
        let mode = agent
            .as_mapping_get("mode")
            .and_then(Yaml::as_str)
            .expect("mode");
        assert!(
            ["plan", "build", "review"].contains(&mode),
            "unknown mode `{mode}` in {}",
            file.display()
        );
    }
    assert_eq!(seen, 4, "the package ships four portable agents");
}
