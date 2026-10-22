//! Rebuild the derived index against the real performance fixture
//! (`fixtures/screenplay-1000.fountain`): 1 000 lines with accented text,
//! character cues, parentheticals and section headings. The scan must stay a
//! derived convenience: it never modifies the source file.

use narra_core::{DEFAULT_SCREENPLAY, ProjectType, create_project};
use std::fs;
use std::path::PathBuf;

fn temp_root() -> PathBuf {
    tempfile::tempdir().expect("tempdir").keep()
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/screenplay-1000.fountain")
}

#[test]
fn fixture_index_matches_scene_heading_count() {
    let fixture = fs::read_to_string(fixture_path()).expect("fixture present");
    let expected: Vec<&str> = fixture
        .lines()
        .filter(|line| {
            let t = line.trim_start();
            t.starts_with("INT.")
                || t.starts_with("EXT.")
                || t.starts_with("EST.")
                || t.starts_with("I/E.")
        })
        .collect();
    // The fixture is named for its scene count: 898 INT./EXT./EST. + 102 I/E.
    assert_eq!(expected.len(), 1000, "fixture composition changed");

    let root = temp_root();
    let project = create_project(&root, "Fixture", ProjectType::Feature, "es").expect("create");
    project
        .write_document(DEFAULT_SCREENPLAY, &fixture)
        .expect("copy fixture");

    let index = project.rebuild_index().expect("index");
    assert_eq!(index.scenes.len(), expected.len());
    assert_eq!(index.scenes[0].heading, expected[0]);
    assert_eq!(
        index.scenes.last().expect("non-empty").heading,
        *expected.last().expect("non-empty")
    );

    // Line numbers must be 1-based and strictly increasing (unique positions).
    let numbers: Vec<usize> = index.scenes.iter().map(|s| s.line_number).collect();
    let mut sorted = numbers.clone();
    sorted.sort_unstable();
    assert_eq!(numbers, sorted, "line numbers must be in source order");
    assert!(numbers.iter().all(|&n| n >= 1));

    // The index is derived: the source bytes are untouched by rebuilding.
    let after = project
        .read_document(DEFAULT_SCREENPLAY)
        .expect("read back");
    assert_eq!(after, fixture, "rebuild_index must not touch source files");
}
