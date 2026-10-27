//! Screenplay element helpers (M9 slice 5): forceable element cycling and
//! classification used by Tab/⇧Tab and the smart Enter.
//!
//! Fountain classifies lines by context (blank lines around headings, a cue
//! before dialogue). For *editing gestures* we need only the forceable
//! kinds — scene heading, action, character, transition — which can be
//! expressed with the spec's forced markers (`.`, `!`, `@`, `>`) without
//! touching the line's text. Dialogue and parentheticals depend on a cue
//! context and are deliberately not part of the cycle.

/// Forceable element kind, in cycle order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementType {
    SceneHeading,
    Action,
    Character,
    Transition,
}

impl ElementType {
    pub const ALL: [ElementType; 4] = [
        ElementType::SceneHeading,
        ElementType::Action,
        ElementType::Character,
        ElementType::Transition,
    ];

    pub fn i18n_key(self) -> &'static str {
        match self {
            ElementType::SceneHeading => "context.element.heading",
            ElementType::Action => "context.element.action",
            ElementType::Character => "context.element.character",
            ElementType::Transition => "context.element.transition",
        }
    }

    fn next(self) -> ElementType {
        match self {
            ElementType::SceneHeading => ElementType::Action,
            ElementType::Action => ElementType::Character,
            ElementType::Character => ElementType::Transition,
            ElementType::Transition => ElementType::SceneHeading,
        }
    }

    fn previous(self) -> ElementType {
        match self {
            ElementType::SceneHeading => ElementType::Transition,
            ElementType::Action => ElementType::SceneHeading,
            ElementType::Character => ElementType::Action,
            ElementType::Transition => ElementType::Character,
        }
    }
}

/// Cycle the caret line's element type (Tab/⇧Tab), returning the new line
/// text. The current kind is classified *before* stripping the markers (a
/// forced action line whose text looks like a heading must stay action),
/// then the target kind's marker is applied; the content is preserved (no
/// case changes, no rewrites).
pub fn cycle(line: &str, forward: bool) -> String {
    let current = classify(line);
    let stripped = strip_markers(line);
    let target = if forward {
        current.next()
    } else {
        current.previous()
    };
    apply(&stripped, target)
}

/// Classify a line by its forced marker first, then by shape.
pub fn classify(line: &str) -> ElementType {
    let trimmed = line.trim_start();
    if trimmed.starts_with('@') {
        return ElementType::Character;
    }
    if trimmed.starts_with('>') {
        return ElementType::Transition;
    }
    if is_forced_heading(trimmed) {
        return ElementType::SceneHeading;
    }
    if trimmed.starts_with('!') {
        return ElementType::Action;
    }
    if is_scene_prefix(trimmed) {
        return ElementType::SceneHeading;
    }
    if looks_like_transition(trimmed) {
        return ElementType::Transition;
    }
    if looks_like_character(trimmed) {
        return ElementType::Character;
    }
    ElementType::Action
}

/// Strip the forced markers (`!`, `@`, `>`, leading `.`) a cycle may have
/// added, so the text survives repeated cycles.
fn strip_markers(line: &str) -> String {
    let leading = line.len() - line.trim_start().len();
    let (indent, body) = line.split_at(leading);
    let mut body = body;
    for marker in ['!', '@', '>'] {
        if let Some(rest) = body.strip_prefix(marker) {
            body = rest;
            break;
        }
    }
    if is_forced_heading(body) {
        body = &body[1..];
    }
    format!("{indent}{body}")
}

pub fn apply(line: &str, target: ElementType) -> String {
    let leading = line.len() - line.trim_start().len();
    let (indent, body) = line.split_at(leading);
    match target {
        ElementType::SceneHeading => {
            if body.trim().is_empty() {
                format!("{indent}INT. ")
            } else if is_scene_prefix(body) {
                line.to_string()
            } else {
                format!("{indent}.{body}")
            }
        }
        ElementType::Action => {
            // Force action when the bare text would be read as another kind.
            if classify(body) != ElementType::Action {
                format!("{indent}!{body}")
            } else {
                line.to_string()
            }
        }
        ElementType::Character => format!("{indent}@{body}"),
        ElementType::Transition => format!("{indent}>{body}"),
    }
}

fn is_scene_prefix(text: &str) -> bool {
    let text = text.trim_start();
    ["INT./EXT", "INT/EXT", "I/E", "INT", "EXT", "EST"]
        .iter()
        .any(|prefix| {
            text.strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('.') || rest.starts_with(' '))
        })
}

/// A leading `.` followed by alphanumeric text (the spec's forced heading).
fn is_forced_heading(text: &str) -> bool {
    let mut chars = text.chars();
    matches!(chars.next(), Some('.')) && chars.next().is_some_and(|c| c.is_alphanumeric())
}

fn looks_like_transition(text: &str) -> bool {
    let trimmed = text.trim_end();
    trimmed.ends_with("TO:") || trimmed.starts_with("FUNDIDO") || trimmed.starts_with("CORTE")
}

/// Uppercase-ish, short, no sentence punctuation: the shape of a character
/// cue. Extensions like `(V.O.)` are allowed.
fn looks_like_character(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > 40 {
        return false;
    }
    let has_letter = text.chars().any(|c| c.is_alphabetic());
    let lower_outside_parens = {
        let mut depth: i32 = 0;
        let mut found = false;
        for c in text.chars() {
            match c {
                '(' => depth += 1,
                ')' => depth = depth.saturating_sub(1),
                c if depth == 0 && c.is_lowercase() => found = true,
                _ => {}
            }
        }
        found
    };
    has_letter && !lower_outside_parens && !text.ends_with('.') && !text.contains(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_walks_scene_action_character_transition_and_back() {
        // An empty line starts as action; Tab cycles to character, then
        // transition, then a scene-heading stub.
        let character = cycle("", true);
        assert_eq!(character, "@");
        let transition = cycle(&character, true);
        assert_eq!(transition, ">");
        let heading = cycle(&transition, true);
        assert_eq!(heading, "INT. ");
        assert_eq!(classify("INT. "), ElementType::SceneHeading);
        // Scene heading -> action (forced with `!` because the text would
        // otherwise still read as a heading).
        let action = cycle("INT. CASA - DÍA", true);
        assert_eq!(action, "!INT. CASA - DÍA");
        assert_eq!(classify(&action), ElementType::Action);
        // Action -> character (cue marker).
        assert_eq!(cycle(&action, true), "@INT. CASA - DÍA");
    }

    #[test]
    fn cycling_preserves_the_text_and_strips_old_markers() {
        let line = "Marta entra en la sala.";
        let mut current = line.to_string();
        for _ in 0..8 {
            current = cycle(&current, true);
        }
        // Full cycle returns to the starting element type's text.
        let stripped = current
            .trim_start_matches(['!', '@', '>'])
            .trim_start_matches('.');
        assert_eq!(stripped, line);
    }

    #[test]
    fn shift_tab_cycles_backwards() {
        // Action -> scene heading (empty action line gets a heading stub).
        assert_eq!(cycle("", false), "INT. ");
        // A heading -> transition via backward cycling.
        assert_eq!(cycle("INT. CASA", false), ">INT. CASA");
        assert_eq!(classify(">INT. CASA"), ElementType::Transition);
    }

    #[test]
    fn classify_recognises_spec_shapes() {
        assert_eq!(classify("INT. OFICINA - DÍA"), ElementType::SceneHeading);
        assert_eq!(classify(".CASA"), ElementType::SceneHeading);
        assert_eq!(classify("CORTE A:"), ElementType::Transition);
        assert_eq!(classify("FUNDIDO A NEGRO"), ElementType::Transition);
        assert_eq!(classify("ELENA (V.O.)"), ElementType::Character);
        assert_eq!(classify("Marta teclea."), ElementType::Action);
        assert_eq!(classify("una frase en minúsculas"), ElementType::Action);
    }

    #[test]
    fn forced_markers_are_stripped_before_reapplying() {
        assert_eq!(cycle("@ELENA", true), ">ELENA");
        assert_eq!(cycle(">ELENA", true), ".ELENA");
        assert_eq!(cycle(".ELENA", true), "!ELENA");
        assert_eq!(cycle("!ELENA", true), "@ELENA");
    }
}
