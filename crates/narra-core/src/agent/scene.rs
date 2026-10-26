//! Shared scene-block extraction for agent tasks (read-only, M6).
//!
//! The screenplay file stays authoritative (`DATA_MODEL.md` §1): a scene
//! spans its heading line up to the line before the next heading, excluding
//! the blank separator lines that precede the next scene — those belong to
//! the separator, not to either scene. `scene_edit` (replacing the region)
//! and `scene_review` (reading it) both go through this, so the two can
//! never disagree about where a scene starts or ends.

use crate::fountain;

/// The lines of one scene, as the derived parser counts them.
pub(crate) struct SceneBlock {
    /// Scene heading text, e.g. "INT. TALLER - NOCHE".
    pub heading: String,
    /// 1-based first line of the scene (the heading itself).
    pub start_line: usize,
    /// 1-based last content line. Numerically equal to the 0-based
    /// *exclusive* end index (callers that splice use it as such).
    pub end_line: usize,
    /// The scene's lines joined with `\n` (no trailing separator blanks).
    pub text: String,
}

/// Outcome of looking up a scene: the block, or why it is unavailable.
pub(crate) enum SceneLookup {
    Found(SceneBlock),
    /// The screenplay has no scene headings at all.
    NoScenes,
    /// The requested number (or 0) exceeds the scene count.
    OutOfRange {
        total: usize,
    },
}

/// Extract scene `scene_number` (1-based) from `script`.
pub(crate) fn scene_block(script: &str, scene_number: usize) -> SceneLookup {
    let scenes: Vec<(usize, String)> = fountain::parse(script)
        .iter()
        .filter(|element| element.kind == fountain::ElementKind::SceneHeading)
        .map(|element| (element.line_number, element.text.clone()))
        .collect();
    if scenes.is_empty() {
        return SceneLookup::NoScenes;
    }
    if scene_number == 0 || scene_number > scenes.len() {
        return SceneLookup::OutOfRange {
            total: scenes.len(),
        };
    }

    let base_lines: Vec<&str> = script.lines().collect();
    let (heading_line, heading) = &scenes[scene_number - 1];
    let start = heading_line - 1;
    let end = scenes
        .get(scene_number)
        .map(|(line, _)| line - 1)
        .unwrap_or(base_lines.len());
    // The blank line(s) separating this scene from the next are part of
    // the *separator*, not of the scene: for an edit they must stay outside
    // the replaced region (so they can never be lost), and for a review
    // they are not scene content either.
    let mut end_line = end;
    while end_line > start && base_lines[end_line - 1].is_empty() {
        end_line -= 1;
    }

    SceneLookup::Found(SceneBlock {
        heading: heading.clone(),
        start_line: start + 1,
        end_line,
        text: base_lines[start..end_line].join("\n"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO_SCENES: &str = "INT. TALLER - NOCHE\n\nELENA\nEsto es una prueba.\n\nINT. CALLE - DÍA\n\nMARCOS\nHace sol.\n";

    #[test]
    fn extracts_first_scene_without_the_separator_blanks() {
        let SceneLookup::Found(block) = scene_block(TWO_SCENES, 1) else {
            panic!("scene 1 must be found");
        };
        assert_eq!(block.heading, "INT. TALLER - NOCHE");
        assert_eq!(block.start_line, 1);
        // Lines 1..4, excluding the two blank separator lines before scene 2.
        assert_eq!(block.end_line, 4);
        assert_eq!(
            block.text,
            "INT. TALLER - NOCHE\n\nELENA\nEsto es una prueba."
        );
    }

    #[test]
    fn last_scene_runs_to_end_of_file() {
        let SceneLookup::Found(block) = scene_block(TWO_SCENES, 2) else {
            panic!("scene 2 must be found");
        };
        assert_eq!(block.heading, "INT. CALLE - DÍA");
        assert_eq!(block.start_line, 6);
        assert_eq!(block.end_line, 9);
        assert_eq!(block.text, "INT. CALLE - DÍA\n\nMARCOS\nHace sol.");
    }

    #[test]
    fn trailing_blank_lines_at_eof_are_trimmed() {
        let with_trailing_blank = format!("{TWO_SCENES}\n");
        let SceneLookup::Found(block) = scene_block(&with_trailing_blank, 2) else {
            panic!("scene 2 must be found");
        };
        // The blank line(s) after the last scene are separator, not content.
        assert_eq!(block.end_line, 9);
        assert_eq!(block.text, "INT. CALLE - DÍA\n\nMARCOS\nHace sol.");
    }

    #[test]
    fn no_scenes_and_out_of_range_are_distinct() {
        assert!(matches!(
            scene_block("Sin encabezados.\n", 1),
            SceneLookup::NoScenes
        ));
        assert!(matches!(
            scene_block(TWO_SCENES, 3),
            SceneLookup::OutOfRange { total: 2 }
        ));
        assert!(matches!(
            scene_block(TWO_SCENES, 0),
            SceneLookup::OutOfRange { total: 2 }
        ));
    }
}
