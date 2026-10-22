//! Rebuild the derived index against the real performance fixture
//! (`fixtures/screenplay-1000.fountain`): 19 347 lines, 1 000 scenes,
//! accented text, character cues, parentheticals, notes and sections. The
//! parser and index must stay derived conveniences: they never modify the
//! source file.

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

/// Element census of the real fixture, cross-checked against an independent
/// line-by-line analysis of `screenplay-1000.fountain` (Python, 2026-10-22).
/// Locks the parser's behavior on real content: 19 347 lines, accented text,
/// inline notes, sections and a title page.
#[test]
fn fixture_element_census_is_stable() {
    use narra_core::{ElementKind, parse_fountain};
    use std::collections::BTreeMap;

    let fixture = fs::read_to_string(fixture_path()).expect("fixture present");
    let elements = parse_fountain(&fixture);

    // One element per line, strictly sequential (never merges or skips lines).
    assert_eq!(elements.len(), fixture.lines().count());
    assert!(
        elements
            .windows(2)
            .all(|w| w[1].line_number == w[0].line_number + 1)
    );

    let mut counts: BTreeMap<ElementKind, usize> = BTreeMap::new();
    for element in &elements {
        *counts.entry(element.kind).or_default() += 1;
    }
    let get = |kind: ElementKind| counts.get(&kind).copied().unwrap_or(0);
    // Census frozen after fixing the fixture (2026-10-22): 798 character cues
    // had been generated glued to the previous dialogue block, which Fountain
    // treats as dialogue continuation — the spec requires a blank line before
    // a cue. The fixture was corrected; the parser was already spec-correct.
    assert_eq!(get(ElementKind::SceneHeading), 1000);
    assert_eq!(get(ElementKind::Character), 2834);
    assert_eq!(get(ElementKind::Dialogue), 2834);
    assert_eq!(get(ElementKind::Parenthetical), 731);
    assert_eq!(get(ElementKind::Note), 244);
    assert_eq!(get(ElementKind::Section), 4);
    assert_eq!(get(ElementKind::TitlePage), 4);
    assert_eq!(get(ElementKind::Action), 11696);
    // The fixture contains none of these elements today; if the fixture grows
    // them, the census must be updated deliberately.
    assert_eq!(get(ElementKind::Transition), 0);
    assert_eq!(get(ElementKind::Centered), 0);
    assert_eq!(get(ElementKind::Lyric), 0);
    assert_eq!(get(ElementKind::PageBreak), 0);
    assert_eq!(get(ElementKind::Boneyard), 0);
}
