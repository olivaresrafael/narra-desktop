//! M17 load measurement: a type package with 30 localized templates and a
//! project with 20 trusted skills. The thresholds are deliberately generous
//! for CI; the printed timings are the useful output for the M17 record.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use narra_core::agent::skill::{active_skills, load_skills, trust_skill};
use narra_core::project::create_project_from_package;
use narra_core::project_type::{TypeOrigin, load_package};

fn temp_root(label: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("narra-m17-load-{label}-{}", uuid::Uuid::new_v4()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("temp dir");
    root
}

fn template_type_yaml() -> String {
    let mut yaml = String::from(
        "schema_version: 1\napi_version: 1\nid: measured\nname: Measured\n\
         description: Load measurement package\ndirectories: [docs]\nfiles:\n",
    );
    for index in 0..30 {
        yaml.push_str(&format!(
            "  - path: docs/file-{index:02}.md\n    content:\n      en: |\n        # File {index}\n        English body {index}.\n      es: |\n        # Archivo {index}\n        Cuerpo en español {index}.\n"
        ));
    }
    yaml.push_str("context:\n  pinned: [docs]\n");
    yaml
}

fn write_skills(root: &std::path::Path, count: usize) {
    for index in 0..count {
        let directory = root.join(format!(".narra/skills/skill-{index:02}"));
        fs::create_dir_all(&directory).expect("skill dir");
        fs::write(
            directory.join("SKILL.md"),
            format!(
                "---\nname: Skill {index:02}\ndescription: Measurement skill {index:02}.\n---\nReview item {index:02} without inventing facts.\n"
            ),
        )
        .expect("skill file");
    }
}

#[test]
fn twenty_skills_and_thirty_templates_load_within_budget() {
    let source = temp_root("package");
    let package_dir = source.join("measured");
    fs::create_dir_all(&package_dir).expect("package dir");
    fs::write(package_dir.join("type.yaml"), template_type_yaml()).expect("type.yaml");

    let started = Instant::now();
    let package = load_package(&package_dir, TypeOrigin::User).expect("load package");
    let package_elapsed = started.elapsed();
    assert_eq!(package.files.len(), 30);

    let project_root = temp_root("project");
    let started = Instant::now();
    let project = create_project_from_package(&project_root, "Medición", &package, "es")
        .expect("create project");
    let create_elapsed = started.elapsed();
    assert_eq!(
        project.manifest().project_type_id,
        "measured",
        "custom type id survives creation"
    );
    assert!(project_root.join("docs/file-29.md").is_file());

    write_skills(&project_root, 20);
    let started = Instant::now();
    let skills = load_skills(&project_root, None).expect("load skills");
    let load_elapsed = started.elapsed();
    assert_eq!(skills.len(), 20);

    let started = Instant::now();
    for skill in &skills {
        trust_skill(&project_root, &skill.id, &skill.hash).expect("trust skill");
    }
    let active = active_skills(&project_root, &skills).expect("active skills");
    let trust_elapsed = started.elapsed();
    assert_eq!(active.len(), 20);

    println!(
        "M17 load: package={package_elapsed:?} create={create_elapsed:?} \
         skills={load_elapsed:?} trust={trust_elapsed:?}"
    );
    let total = package_elapsed + create_elapsed + load_elapsed + trust_elapsed;
    assert!(
        total < Duration::from_secs(2),
        "20 skills + 30 templates took {total:?}"
    );
}
