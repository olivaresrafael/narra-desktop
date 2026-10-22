//! Derived Fountain parser (Spike G-1, Milestone 2).
//!
//! Implements the [Fountain 1.1 syntax](https://fountain.io/syntax) as a
//! line-classification pass: one [`Element`] per source line, with the
//! 1-based line number. The parser is **derived, never authoritative**
//! (`DATA_MODEL.md` §1): source files remain canonical, hand-edited files keep
//! working, and nothing here rewrites or re-serializes the input.
//!
//! Known, documented limitations (accepted for M2; the surface is exercised by
//! tests, not hidden):
//! - inline `[[notes]]` inside action/dialogue lines stay in the line text and
//!   are not extracted as separate elements;
//! - emphasis (`*italics*`, `_underline_`) is presentation-only and ignored;
//! - `FADE IN:` at the very start of a document can match the title-page key
//!   pattern — an inherent Fountain ambiguity resolved in favor of the title
//!   page, matching the spec ("the Title Page is always the first thing").

/// Classification of a single source line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ElementKind {
    TitlePage,
    SceneHeading,
    Action,
    Character,
    Dialogue,
    Parenthetical,
    Transition,
    Centered,
    Section,
    Synopsis,
    Lyric,
    PageBreak,
    Note,
    Boneyard,
}

/// One classified line of a Fountain document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    pub kind: ElementKind,
    /// 1-based line number in the source file.
    pub line_number: usize,
    /// Line text. Kept verbatim except: forced markers (`!`, `@`, `~`, `=`,
    /// `#`, `>`, and the forced-heading period) are removed, and leading
    /// whitespace is trimmed for every element except [`ElementKind::Action`]
    /// and [`ElementKind::Dialogue`], whose indentation is meaningful.
    pub text: String,
}

/// Longest-prefix-first list from the spec, followed by `.` or ` `.
const SCENE_PREFIXES: &[&str] = &["INT./EXT", "INT/EXT", "I/E", "INT", "EXT", "EST"];

/// Classify every line of a Fountain document.
pub fn parse(text: &str) -> Vec<Element> {
    let lines: Vec<&str> = text.lines().collect();
    let mut elements = Vec::with_capacity(lines.len());
    let mut in_boneyard = false;
    let mut in_title_page = false;

    for (index, line) in lines.iter().enumerate() {
        let number = index + 1;
        let prev_blank = index == 0 || lines[index - 1].is_empty();
        let next_blank = index + 1 == lines.len() || lines[index + 1].is_empty();

        // Boneyard spans lines and wins over everything (spec: the only
        // element allowed to carry syntax across line breaks).
        if in_boneyard {
            if line.contains("*/") {
                in_boneyard = false;
            }
            elements.push(element(ElementKind::Boneyard, number, line));
            continue;
        }
        if line.contains("/*") && !line.contains("*/") {
            in_boneyard = true;
            elements.push(element(ElementKind::Boneyard, number, line));
            continue;
        }

        // Title page: only at the very start, keys end with a colon.
        if index == 0 || (in_title_page && prev_line_was_title(&elements)) {
            if let Some((key, value)) = split_title_key(line) {
                in_title_page = true;
                let text = format!("{key}: {value}");
                let text = text.trim_end();
                elements.push(element(ElementKind::TitlePage, number, text));
                continue;
            }
            if in_title_page && is_indented(line) {
                elements.push(element(ElementKind::TitlePage, number, line));
                continue;
            }
            in_title_page = false;
        }

        let trimmed = line.trim_start();

        // Standalone notes (inline notes remain in the line's text).
        if trimmed.starts_with("[[") {
            elements.push(element(ElementKind::Note, number, line));
            continue;
        }

        // Page break: three or more equals signs and nothing else.
        if is_page_break(trimmed) {
            elements.push(element(ElementKind::PageBreak, number, line));
            continue;
        }

        // Sections and synopses are writer tools; stripped from output.
        if let Some(rest) = trimmed.strip_prefix('#') {
            let text = rest.trim_start().trim_start_matches('#').trim_start();
            elements.push(element(ElementKind::Section, number, text));
            continue;
        }
        if trimmed.starts_with('=') {
            let text = trimmed.trim_start_matches('=').trim();
            elements.push(element(ElementKind::Synopsis, number, text));
            continue;
        }

        // Lyrics are always forced.
        if let Some(rest) = trimmed.strip_prefix('~') {
            elements.push(element(ElementKind::Lyric, number, rest.trim_start()));
            continue;
        }

        // Centered text and forced transitions both start with '>'.
        if trimmed.starts_with('>') {
            if trimmed.ends_with('<') && trimmed.len() >= 2 {
                let inner = trimmed[1..trimmed.len() - 1].trim();
                elements.push(element(ElementKind::Centered, number, inner));
            } else {
                let inner = trimmed.trim_start_matches('>').trim();
                elements.push(element(ElementKind::Transition, number, inner));
            }
            continue;
        }

        // Forced action.
        if trimmed.starts_with('!') {
            let inner = trimmed.trim_start_matches('!').trim_start();
            elements.push(element(ElementKind::Action, number, inner));
            continue;
        }

        // Forced scene heading: a single leading period + alphanumeric.
        if is_forced_heading(trimmed) {
            elements.push(element(ElementKind::SceneHeading, number, &trimmed[1..]));
            continue;
        }

        // Forced character: '@name' at element position (blank line before).
        if prev_blank && trimmed.starts_with('@') {
            let inner = trimmed.trim_start_matches('@').trim_start();
            elements.push(element(ElementKind::Character, number, inner));
            continue;
        }

        // Blank lines end any open block; classify them as empty Action so the
        // line-per-element invariant holds and dialogue never swallows them.
        if line.is_empty() {
            elements.push(element(ElementKind::Action, number, line));
            continue;
        }

        // Dialogue continuation: everything until the next blank line after a
        // Character or Parenthetical is dialogue; wrapped lines are parentheticals.
        if prev_was_dialogue_host(&elements) {
            if trimmed.starts_with('(') && trimmed.ends_with(')') {
                elements.push(element(ElementKind::Parenthetical, number, trimmed));
            } else {
                elements.push(element(ElementKind::Dialogue, number, line));
            }
            continue;
        }

        // Automatic scene heading: spec prefix + '.' or ' ' after it,
        // isolated by blank lines.
        if prev_blank && next_blank && is_scene_heading(trimmed) {
            elements.push(element(ElementKind::SceneHeading, number, line));
            continue;
        }

        // Automatic transition: uppercase, ends in TO:, isolated.
        if prev_blank && next_blank && is_transition(trimmed) {
            elements.push(element(ElementKind::Transition, number, line));
            continue;
        }

        // Automatic character: uppercase name (extensions may be lowercase),
        // blank line before, none after, at least one alphabetic character.
        if prev_blank && !next_blank && is_character(trimmed) {
            elements.push(element(ElementKind::Character, number, trimmed));
            continue;
        }

        elements.push(element(ElementKind::Action, number, line));
    }
    elements
}

fn element(kind: ElementKind, line_number: usize, text: &str) -> Element {
    let text = if kind == ElementKind::Action || kind == ElementKind::Dialogue {
        text.to_string()
    } else {
        text.trim().to_string()
    };
    Element {
        kind,
        line_number,
        text,
    }
}

/// `key: value` at the start of a document; keys are letters/digits/spaces.
fn split_title_key(line: &str) -> Option<(String, String)> {
    if line.is_empty() || line.starts_with(char::is_whitespace) {
        return None;
    }
    let colon = line.find(':')?;
    let key = &line[..colon];
    if key.is_empty() || !key.chars().all(|c| c.is_alphanumeric() || c == ' ') {
        return None;
    }
    // A key must be followed by a space or end of line (not `INT. X:` style).
    let rest = &line[colon + 1..];
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }
    Some((key.to_string(), rest.trim_start().trim_end().to_string()))
}

fn is_indented(line: &str) -> bool {
    let mut chars = line.chars();
    match chars.next() {
        Some('\t') => true,
        Some(' ') => {
            let spaces = line.chars().take_while(|c| *c == ' ').count();
            spaces >= 3
        }
        _ => false,
    }
}

fn prev_line_was_title(elements: &[Element]) -> bool {
    matches!(elements.last(), Some(e) if e.kind == ElementKind::TitlePage)
}

fn is_page_break(trimmed: &str) -> bool {
    trimmed.len() >= 3 && trimmed.chars().all(|c| c == '=')
}

fn is_forced_heading(trimmed: &str) -> bool {
    let mut chars = trimmed.chars();
    chars.next() == Some('.') && chars.next().is_some_and(|c| c.is_alphanumeric())
}

fn is_scene_heading(trimmed: &str) -> bool {
    SCENE_PREFIXES.iter().any(|prefix| {
        trimmed
            .as_bytes()
            .get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix.as_bytes()))
            && trimmed
                .as_bytes()
                .get(prefix.len())
                .is_some_and(|&b| b == b'.' || b == b' ')
    })
}

fn is_transition(trimmed: &str) -> bool {
    trimmed.ends_with("TO:")
        && trimmed.len() > 3
        && trimmed == trimmed.to_uppercase()
        && trimmed.chars().any(|c| c.is_alphabetic())
}

/// Name portion before a parenthesized extension must be uppercase.
fn is_character(trimmed: &str) -> bool {
    let name = match trimmed.find('(') {
        Some(at) => trimmed[..at].trim_end(),
        None => trimmed.trim_end(),
    };
    // Dual dialogue caret is ignored for the uppercase test.
    let name = name.strip_suffix('^').unwrap_or(name).trim_end();
    !name.is_empty()
        && name.chars().any(|c| c.is_alphabetic())
        && name == name.to_uppercase()
        && !is_scene_heading(trimmed)
        && !trimmed.ends_with("TO:")
}

fn prev_was_dialogue_host(elements: &[Element]) -> bool {
    matches!(
        elements.last(),
        Some(Element {
            kind: ElementKind::Character | ElementKind::Parenthetical | ElementKind::Dialogue,
            ..
        })
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<Element> {
        parse(text)
    }

    /// Test-only sugar: compare an element against `(kind, text)`.
    impl PartialEq<(ElementKind, &str)> for Element {
        fn eq(&self, other: &(ElementKind, &str)) -> bool {
            self.kind == other.0 && self.text == other.1
        }
    }

    #[test]
    fn one_element_per_line_with_line_numbers() {
        let text = "INT. CASA - DÍA\n\nAcción.\n\nFIN\n";
        let elements = parse(text);
        assert_eq!(elements.len(), 5);
        assert_eq!(elements[0].line_number, 1);
        assert_eq!(elements[4].line_number, 5);
    }

    #[test]
    fn scene_heading_automatic_and_case_insensitive() {
        let got = kinds(
            "INT. CASA - DÍA\n\next. calle - noche\n\nEST. PLAYA - DÍA\n\nI/E. CARRETERA - NOCHE\n\nINT./EXT. TÚNEL - NOCHE\n\nINT/EXT. COCHE - DÍA\n",
        );
        let headings: Vec<&str> = got
            .iter()
            .filter(|e| e.kind == ElementKind::SceneHeading)
            .map(|e| e.text.as_str())
            .collect();
        assert_eq!(
            headings,
            vec![
                "INT. CASA - DÍA",
                "ext. calle - noche",
                "EST. PLAYA - DÍA",
                "I/E. CARRETERA - NOCHE",
                "INT./EXT. TÚNEL - NOCHE",
                "INT/EXT. COCHE - DÍA",
            ]
        );
    }

    #[test]
    fn heading_needs_blank_line_on_both_sides() {
        let got = kinds("Texto.\nINT. PEGADO - DÍA\nMás texto.\n");
        assert!(got.iter().all(|e| e.kind == ElementKind::Action));
    }

    #[test]
    fn ellipsis_is_action_not_forced_heading() {
        // Spec example: a leading period forces a heading only when followed
        // by an alphanumeric character.
        let got = kinds("\n...donde el carnaval aparcado.\n");
        assert_eq!(got[1].kind, ElementKind::Action);
        let got = kinds("\n.SNIPER SCOPE POV\n\n");
        assert_eq!(got[1], (ElementKind::SceneHeading, "SNIPER SCOPE POV"));
    }

    #[test]
    fn character_dialogue_parenthetical_chain() {
        // Per the spec a Character needs one blank line before it; TOMÁS is
        // only a character because of that blank line.
        let got = kinds(
            "\nELENA\n(con voz plana)\nDime algo que no sea una excusa.\n\nPAULA\n(susurrando)\nSi llegamos tarde, es tu culpa.\n\nTOMÁS\nLo pensé toda la noche.\n",
        );
        assert_eq!(got[1], (ElementKind::Character, "ELENA"));
        assert_eq!(got[2], (ElementKind::Parenthetical, "(con voz plana)"));
        assert_eq!(
            got[3],
            (ElementKind::Dialogue, "Dime algo que no sea una excusa.")
        );
        assert_eq!(got[5], (ElementKind::Character, "PAULA"));
        assert_eq!(got[9], (ElementKind::Character, "TOMÁS"));
        assert_eq!(got[10], (ElementKind::Dialogue, "Lo pensé toda la noche."));

        // Without the blank line the cue is dialogue — documented behavior.
        let glued = kinds("\nPAULA\nSusurrando.\nTOMÁS\nSin blanco.\n");
        assert_eq!(glued[3], (ElementKind::Dialogue, "TOMÁS"));
    }

    #[test]
    fn character_extension_may_be_lowercase() {
        let got =
            kinds("\nMOM (O. S.)\n¡Luke! Baja a comer.\n\nHANS (on the radio)\n¿Qué dijiste?\n");
        assert_eq!(got[1], (ElementKind::Character, "MOM (O. S.)"));
        assert_eq!(got[4], (ElementKind::Character, "HANS (on the radio)"));
    }

    #[test]
    fn character_must_contain_a_letter() {
        // Spec: "R2D2" works, "23" does not.
        assert!(is_character("R2D2"));
        assert!(!is_character("23"));
    }

    #[test]
    fn forced_character_keeps_mixed_case() {
        let got = kinds("\n@McCLANE\nYippie ki-yay!\n");
        assert_eq!(got[1], (ElementKind::Character, "McCLANE"));
    }

    #[test]
    fn dual_dialogue_caret() {
        let got = kinds("\nBRICK\nSueña con jubilarse.\n\nSTEEL ^\nSueña con jubilarse.\n");
        // Index 4 = line 5: the caret stays in the text, ignored by detection.
        assert_eq!(got[4], (ElementKind::Character, "STEEL ^"));
    }

    #[test]
    fn uppercase_line_followed_by_text_is_character_like_spec() {
        // The documented Fountain quirk: SCANNING THE AISLES… becomes a
        // character; '!' forces action instead.
        let got = kinds("\nSCANNING THE AISLES…\n¿Dónde está el jefe?\n");
        assert_eq!(got[1].kind, ElementKind::Character);
        let got = kinds("\n!SCANNING THE AISLES…\n¿Dónde está el jefe?\n");
        assert_eq!(got[1].kind, ElementKind::Action);
    }

    #[test]
    fn transitions_automatic_and_forced() {
        // Automatic transitions follow the spec literally: uppercase, ends in
        // "TO:", isolated. Spanish-style transitions like "CORTE A:" are not
        // auto-detected (documented limitation); the forced form covers them.
        let got = kinds("\nSMASH CUT TO:\n\nEXT. PLAYA - NOCHE\n\n>CORTE A:\n");
        assert_eq!(got[1], (ElementKind::Transition, "SMASH CUT TO:"));
        assert_eq!(got[5], (ElementKind::Transition, "CORTE A:"));
        // A colon plus a space is action, per the spec's power-user note.
        let got = kinds("\nSMASH CUT TO: \n\nmás texto\n");
        assert_eq!(got[1].kind, ElementKind::Action);
    }

    #[test]
    fn centered_text() {
        let got = kinds("\n>FIN<\n");
        assert_eq!(got[1], (ElementKind::Centered, "FIN"));
    }

    #[test]
    fn sections_synopses_lyrics_page_breaks() {
        let got = kinds(
            "# Acto I\n\n## Secuencia A\n\n= Preparar la entrada.\n\n~Canta la sirena\n\n===\n",
        );
        assert_eq!(got[0], (ElementKind::Section, "Acto I"));
        assert_eq!(got[2], (ElementKind::Section, "Secuencia A"));
        assert_eq!(got[4], (ElementKind::Synopsis, "Preparar la entrada."));
        assert_eq!(got[6], (ElementKind::Lyric, "Canta la sirena"));
        assert_eq!(got[8], (ElementKind::PageBreak, "==="));
    }

    #[test]
    fn standalone_notes_and_boneyard() {
        let got = kinds(
            "\n[[revisar continuidad]]\n\nAcción.\n\n/*\nINT. OCULTA - DÍA\n*/\n\nOtra acción.\n",
        );
        assert_eq!(got[1], (ElementKind::Note, "[[revisar continuidad]]"));
        assert_eq!(got[6], (ElementKind::Boneyard, "INT. OCULTA - DÍA"));
        // Scene inside boneyard must not be indexed.
        assert!(got.iter().all(|e| e.kind != ElementKind::SceneHeading));
    }

    #[test]
    fn title_page_block() {
        let text = "Title: Mi obra\nCredit: Written by\nAuthor: Ana Autora\nDraft date: 1/20/2026\nContact:\n   Estudio Norte\n   Calle Mayor 1\n\nINT. OFICINA - DÍA\n";
        let got = kinds(text);
        assert_eq!(got[0], (ElementKind::TitlePage, "Title: Mi obra"));
        assert_eq!(got[4], (ElementKind::TitlePage, "Contact:"));
        assert_eq!(got[5], (ElementKind::TitlePage, "Estudio Norte"));
        assert_eq!(got[8], (ElementKind::SceneHeading, "INT. OFICINA - DÍA"));
    }

    #[test]
    fn action_preserves_indentation() {
        let got = kinds("\n    Scott --\n    Jacob Billups\n");
        assert_eq!(got[1], (ElementKind::Action, "    Scott --"));
    }

    #[test]
    fn parser_never_touches_source() {
        let text = "Title: Obra\n\nINT. CASA - DÍA\n\nELENA\nHola.\n\n[[nota]]\n";
        let before = text.to_string();
        let elements = parse(text);
        assert_eq!(text, before);
        // Full line coverage: every source line is classified exactly once.
        assert_eq!(elements.len(), text.lines().count());
    }
}
