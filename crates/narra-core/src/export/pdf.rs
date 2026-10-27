//! Screenplay PDF layout and rendering (Milestone 7, Spike G-2).
//!
//! Two separable layers, both UI-neutral:
//! - [`layout`] turns Fountain source into pages of positioned lines using
//!   deterministic fixed-pitch rules (Courier 12pt, 10 characters/inch,
//!   55 lines per page on US Letter). Pagination follows screenplay
//!   conventions and every rule is unit-tested against the M1 fixture;
//! - [`write_pdf`] serializes a layout into PDF bytes with `pdf-writer`,
//!   using the base-14 Courier/Courier-Oblique fonts (no font embedding,
//!   WinAnsi encoding covers Spanish) so the output opens anywhere and the
//!   bytes are deterministic run to run.
//!
//! The layout is the testable truth; the PDF is a thin serialization of it.

use crate::fountain::{self, Element, ElementKind};
use crate::manifest::Manifest;
use pdf_writer::{Content, Name, Pdf, Rect, Ref, Str};

// --- page geometry (points; US Letter) ---------------------------------

const PAGE_W: f32 = 612.0;
const PAGE_H: f32 = 792.0;
/// 1.5" left margin: action and scene headings start here.
const LEFT_MARGIN: f32 = 108.0;
/// 1" top margin.
const TOP_MARGIN: f32 = 72.0;
/// 12pt leading: the screenplay standard.
const LEADING: f32 = 12.0;
/// Courier 12pt advances exactly 7.2pt per character (10 characters/inch).
const ADVANCE: f32 = 7.2;
/// Conventional 55-line screenplay page (top 1", bottom ~0.83").
pub const LINES_PER_PAGE: usize = 55;

// --- element columns (characters from the left text margin) ------------

/// Action and headings: 6" wide (60 characters).
pub const ACTION_COLS: usize = 60;
/// Character cue: 3.7" from the page edge.
pub const CHARACTER_INDENT: usize = 22;
/// Parenthetical: 3.1" from the page edge, 2" wide.
pub const PARENTHETICAL_INDENT: usize = 16;
pub const PARENTHETICAL_COLS: usize = 20;
/// Dialogue: 2.5" from the page edge, 3.5" wide.
pub const DIALOGUE_INDENT: usize = 10;
pub const DIALOGUE_COLS: usize = 35;
/// (MORE) sits at the dialogue indent.
pub const MORE_INDENT: usize = DIALOGUE_INDENT;

/// Horizontal alignment of a laid-out line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
    Center,
}

/// Font style of a laid-out line (both are base-14 PDF fonts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Normal,
    Oblique,
}

/// One line ready to draw, already wrapped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    /// Left offset in character columns from the text margin (left align).
    pub indent: usize,
    /// Column width the text was wrapped to (used by right/center align).
    pub width: usize,
    pub align: Align,
    pub style: Style,
}

impl Line {
    fn left(text: impl Into<String>, indent: usize) -> Line {
        Line {
            text: text.into(),
            indent,
            width: 0,
            align: Align::Left,
            style: Style::Normal,
        }
    }

    fn right(text: impl Into<String>) -> Line {
        Line {
            text: text.into(),
            indent: 0,
            width: ACTION_COLS,
            align: Align::Right,
            style: Style::Normal,
        }
    }

    fn center(text: impl Into<String>) -> Line {
        Line {
            text: text.into(),
            indent: 0,
            width: ACTION_COLS,
            align: Align::Center,
            style: Style::Normal,
        }
    }

    fn blank() -> Line {
        Line::left("", 0)
    }

    fn is_blank(&self) -> bool {
        self.text.is_empty()
    }
}

/// One physical page of body text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub lines: Vec<Line>,
}

/// A complete laid-out document: the title page (empty when the source has
/// none and none was synthesized) plus body pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// Title-page source lines (`Title: …`, `Draft date: …` style).
    pub title: Vec<String>,
    pub pages: Vec<Page>,
}

impl Layout {
    /// Printed page numbers, one entry per physical page: the title page is
    /// unnumbered, and content pages keep counting from it (first content
    /// page = 2). Without a title page, content starts at 1.
    pub fn page_numbers(&self) -> Vec<Option<usize>> {
        let mut numbers = Vec::new();
        if !self.title.is_empty() {
            numbers.push(None);
        }
        let first = if self.title.is_empty() { 1 } else { 2 };
        numbers.extend((first..).take(self.pages.len()).map(Some));
        numbers
    }
}

// --- internal block model ----------------------------------------------

/// One dialogue/parenthetical line before wrapping: (indent, width, text,
/// style).
type DialogueLine = (usize, usize, String, Style);

/// A character cue plus its pending lines.
type PendingDialogue = (String, Vec<DialogueLine>);

enum Block {
    Heading(String),
    Action(String),
    Dialogue {
        cue: String,
        /// Lines in source order.
        lines: Vec<DialogueLine>,
    },
    Transition(String),
    Centered(String),
    Lyric(String),
    PageBreak,
}

impl Block {
    /// Whether placing this block puts at least one body line on the page.
    fn emits_lines(&self) -> bool {
        !matches!(self, Block::PageBreak)
    }
}

// --- layout ------------------------------------------------------------

/// Lay out a Fountain screenplay into pages. Pure and deterministic.
pub fn layout(source: &str) -> Layout {
    let elements = fountain::parse(source);
    let mut title = Vec::new();
    let blocks = blocks(&elements, &mut title);
    Layout {
        title,
        pages: place(&blocks),
    }
}

/// Group classified lines into layout blocks: paragraphs, dialogue blocks
/// with their cue, transitions. Writer tools (sections, synopses, notes,
/// boneyards) never reach the page.
fn blocks(elements: &[Element], title: &mut Vec<String>) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut action = String::new();
    let mut dialogue: Option<PendingDialogue> = None;

    fn flush_action(action: &mut String, blocks: &mut Vec<Block>) {
        if !action.is_empty() {
            blocks.push(Block::Action(std::mem::take(action)));
        }
    }
    fn flush_dialogue(dialogue: &mut Option<PendingDialogue>, blocks: &mut Vec<Block>) {
        if let Some((cue, lines)) = dialogue.take()
            && !lines.is_empty()
        {
            blocks.push(Block::Dialogue { cue, lines });
        }
    }

    for element in elements {
        match element.kind {
            ElementKind::TitlePage => title.push(element.text.clone()),
            ElementKind::Section
            | ElementKind::Synopsis
            | ElementKind::Note
            | ElementKind::Boneyard => {}
            ElementKind::PageBreak => {
                flush_action(&mut action, &mut blocks);
                flush_dialogue(&mut dialogue, &mut blocks);
                blocks.push(Block::PageBreak);
            }
            ElementKind::SceneHeading => {
                flush_action(&mut action, &mut blocks);
                flush_dialogue(&mut dialogue, &mut blocks);
                blocks.push(Block::Heading(element.text.clone()));
            }
            ElementKind::Action => {
                if element.text.trim().is_empty() {
                    flush_action(&mut action, &mut blocks);
                    flush_dialogue(&mut dialogue, &mut blocks);
                } else {
                    flush_dialogue(&mut dialogue, &mut blocks);
                    if !action.is_empty() {
                        action.push(' ');
                    }
                    action.push_str(element.text.trim());
                }
            }
            ElementKind::Character => {
                flush_action(&mut action, &mut blocks);
                flush_dialogue(&mut dialogue, &mut blocks);
                dialogue = Some((element.text.trim().to_string(), Vec::new()));
            }
            ElementKind::Parenthetical => match &mut dialogue {
                Some((_, lines)) => lines.push((
                    PARENTHETICAL_INDENT,
                    PARENTHETICAL_COLS,
                    element.text.trim().to_string(),
                    Style::Normal,
                )),
                None => {
                    if !action.is_empty() {
                        action.push(' ');
                    }
                    action.push_str(element.text.trim());
                }
            },
            ElementKind::Dialogue => match &mut dialogue {
                Some((_, lines)) => lines.push((
                    DIALOGUE_INDENT,
                    DIALOGUE_COLS,
                    element.text.trim().to_string(),
                    Style::Normal,
                )),
                None => {
                    if !action.is_empty() {
                        action.push(' ');
                    }
                    action.push_str(element.text.trim());
                }
            },
            ElementKind::Transition => {
                flush_action(&mut action, &mut blocks);
                flush_dialogue(&mut dialogue, &mut blocks);
                blocks.push(Block::Transition(element.text.trim().to_string()));
            }
            ElementKind::Centered => {
                flush_action(&mut action, &mut blocks);
                flush_dialogue(&mut dialogue, &mut blocks);
                blocks.push(Block::Centered(element.text.trim().to_string()));
            }
            ElementKind::Lyric => {
                flush_action(&mut action, &mut blocks);
                flush_dialogue(&mut dialogue, &mut blocks);
                blocks.push(Block::Lyric(element.text.trim().to_string()));
            }
        }
    }
    flush_action(&mut action, &mut blocks);
    flush_dialogue(&mut dialogue, &mut blocks);
    blocks
}

/// Page builder: accumulates lines and inserts the screenplay pagination
/// rules — no orphaned scene headings, no orphaned character cues, and
/// (MORE)/(CONT'D) around dialogue split across a page break.
struct Builder {
    pages: Vec<Page>,
    current: Page,
}

impl Builder {
    fn new() -> Builder {
        Builder {
            pages: Vec::new(),
            current: Page { lines: Vec::new() },
        }
    }

    fn is_empty(&self) -> bool {
        self.current.lines.is_empty()
    }

    fn remaining(&self) -> usize {
        LINES_PER_PAGE - self.current.lines.len()
    }

    fn full(&self) -> bool {
        self.remaining() == 0
    }

    fn push(&mut self, line: Line) {
        debug_assert!(!self.full(), "push into a full page");
        self.current.lines.push(line);
    }

    /// Close the current page (a page break never leaves a blank page).
    fn new_page(&mut self) {
        if !self.current.lines.is_empty() {
            self.pages.push(std::mem::replace(
                &mut self.current,
                Page { lines: Vec::new() },
            ));
        }
    }

    fn finish(mut self) -> Vec<Page> {
        self.new_page();
        self.pages
    }
}

fn place(blocks: &[Block]) -> Vec<Page> {
    let mut builder = Builder::new();
    for (index, block) in blocks.iter().enumerate() {
        let next_emits = blocks
            .get(index + 1)
            .map(Block::emits_lines)
            .unwrap_or(false);
        match block {
            Block::PageBreak => builder.new_page(),
            Block::Heading(text) => {
                let wrapped = wrap(&text.to_uppercase(), ACTION_COLS);
                let blank = usize::from(!builder.is_empty());
                // Orphan rule: a scene heading must be followed by at least
                // one line of its scene on the same page.
                let needed = blank + wrapped.len() + usize::from(next_emits);
                if builder.remaining() < needed {
                    builder.new_page();
                }
                if !builder.is_empty() {
                    builder.push(Line::blank());
                }
                for line in wrapped {
                    builder.push(Line::left(line, 0));
                }
            }
            Block::Action(text) => {
                let wrapped = wrap(text, ACTION_COLS);
                if !builder.is_empty() {
                    // A separating blank line must never be the page's last line.
                    if builder.remaining() < 2 {
                        builder.new_page();
                    } else {
                        builder.push(Line::blank());
                    }
                }
                for line in wrapped {
                    if builder.full() {
                        builder.new_page();
                    }
                    builder.push(Line::left(line, 0));
                }
            }
            Block::Dialogue { cue, lines } => {
                let mut content = vec![Line::left(cue.to_uppercase(), CHARACTER_INDENT)];
                for (indent, width, text, style) in lines {
                    for part in wrap(text, *width) {
                        content.push(Line {
                            text: part,
                            indent: *indent,
                            width: *width,
                            align: Align::Left,
                            style: *style,
                        });
                    }
                }
                let blank = usize::from(!builder.is_empty());
                // Cue + first content line always share a page.
                let needed = blank + 2.min(content.len());
                if builder.remaining() < needed {
                    builder.new_page();
                }
                if !builder.is_empty() {
                    builder.push(Line::blank());
                }
                let contd_cue =
                    Line::left(format!("{} (CONT'D)", cue.to_uppercase()), CHARACTER_INDENT);
                let more = Line::left("(MORE)", MORE_INDENT);
                let mut next = 0;
                while next < content.len() {
                    if builder.full() {
                        builder.new_page();
                        builder.push(contd_cue.clone());
                    }
                    let pending = content.len() - next;
                    if builder.remaining() == 1 && pending > 1 {
                        // Use the last slot for the (MORE) marker.
                        builder.push(more.clone());
                        builder.new_page();
                        builder.push(contd_cue.clone());
                    }
                    builder.push(content[next].clone());
                    next += 1;
                }
            }
            Block::Transition(text) => {
                let wrapped = wrap(text, ACTION_COLS);
                if !builder.is_empty() {
                    if builder.remaining() < 2 {
                        builder.new_page();
                    } else {
                        builder.push(Line::blank());
                    }
                }
                for line in wrapped {
                    if builder.full() {
                        builder.new_page();
                    }
                    builder.push(Line::right(line));
                }
            }
            Block::Centered(text) => {
                let wrapped = wrap(text, ACTION_COLS);
                if !builder.is_empty() {
                    if builder.remaining() < 2 {
                        builder.new_page();
                    } else {
                        builder.push(Line::blank());
                    }
                }
                for line in wrapped {
                    if builder.full() {
                        builder.new_page();
                    }
                    builder.push(Line::center(line));
                }
            }
            Block::Lyric(text) => {
                let wrapped = wrap(text, DIALOGUE_COLS);
                if !builder.is_empty() {
                    if builder.remaining() < 2 {
                        builder.new_page();
                    } else {
                        builder.push(Line::blank());
                    }
                }
                for line in wrapped {
                    if builder.full() {
                        builder.new_page();
                    }
                    builder.push(Line {
                        text: line,
                        indent: DIALOGUE_INDENT,
                        width: DIALOGUE_COLS,
                        align: Align::Left,
                        style: Style::Oblique,
                    });
                }
            }
        }
    }
    builder.finish()
}

/// Greedy word wrap at `cols` characters; words longer than the column are
/// hard-broken so no line ever exceeds the width.
pub fn wrap(text: &str, cols: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_len = 0;
    for word in text.split_whitespace() {
        let word_len = word.chars().count();
        if word_len > cols {
            if current_len > 0 {
                lines.push(std::mem::take(&mut current));
                current_len = 0;
            }
            let chars: Vec<char> = word.chars().collect();
            for chunk in chars.chunks(cols) {
                lines.push(chunk.iter().collect());
            }
            continue;
        }
        if current_len == 0 {
            current.push_str(word);
            current_len = word_len;
        } else if current_len + 1 + word_len <= cols {
            current.push(' ');
            current.push_str(word);
            current_len += 1 + word_len;
        } else {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
            current_len = word_len;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

// --- PDF rendering -----------------------------------------------------

/// Title page as positioned lines: the title (first entry, `Title:` prefix
/// stripped and uppercased) sits a third of the way down, the remaining
/// entries follow centered below.
fn title_page_layout(title: &[String]) -> Page {
    let mut lines = Vec::new();
    let title_text = title
        .first()
        .map(|line| match line.split_once(':') {
            Some((key, rest)) if key.eq_ignore_ascii_case("title") => rest.trim().to_string(),
            _ => line.clone(),
        })
        .unwrap_or_default();
    // Position: title on body line 16 (a third of the page down).
    while lines.len() < 16 {
        lines.push(Line::blank());
    }
    lines.push(Line::center(title_text.to_uppercase()));
    for extra in title.iter().skip(1) {
        lines.push(Line::blank());
        lines.push(Line::blank());
        lines.push(Line::center(extra.clone()));
    }
    Page { lines }
}

/// Encode text for the WinAnsi (CP1252) encoding of the base-14 fonts:
/// Latin-1 covers Spanish; the common typographic marks map to their
/// WinAnsi positions; anything else becomes `?` rather than corrupting
/// the stream.
fn winansi(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| match c {
            '\u{2018}' => 0x91,
            '\u{2019}' => 0x92,
            '\u{201C}' => 0x93,
            '\u{201D}' => 0x94,
            '\u{2022}' => 0x95,
            '\u{2013}' => 0x96,
            '\u{2014}' => 0x97,
            '\u{2026}' => 0x85,
            c if (c as u32) <= 0xFF => c as u8,
            _ => b'?',
        })
        .collect()
}

/// Serialize a layout into deterministic PDF bytes.
pub fn write_pdf(layout: &Layout) -> Vec<u8> {
    let mut pdf = Pdf::new();
    let mut alloc = Ref::new(1);
    let catalog = alloc.bump();
    let page_tree = alloc.bump();
    let normal = alloc.bump();
    let oblique = alloc.bump();

    pdf.catalog(catalog).pages(page_tree);
    pdf.type1_font(normal)
        .base_font(Name(b"Courier"))
        .encoding_predefined(Name(b"WinAnsiEncoding"));
    pdf.type1_font(oblique)
        .base_font(Name(b"Courier-Oblique"))
        .encoding_predefined(Name(b"WinAnsiEncoding"));

    let title_page = (!layout.title.is_empty()).then(|| title_page_layout(&layout.title));
    let numbers = layout.page_numbers();
    let mut all: Vec<(&Page, Option<usize>)> = Vec::new();
    if let Some(title) = &title_page {
        all.push((title, None));
    }
    for (index, page) in layout.pages.iter().enumerate() {
        all.push((page, numbers[index + usize::from(title_page.is_some())]));
    }

    let mut page_ids = Vec::new();
    for (page, number) in all {
        let page_id = alloc.bump();
        let content_id = alloc.bump();
        page_ids.push(page_id);

        let mut content = Content::new();
        content.begin_text();
        for (index, line) in page.lines.iter().enumerate() {
            if line.is_blank() {
                continue;
            }
            let len = line.text.chars().count();
            let x = match line.align {
                Align::Left => LEFT_MARGIN + (line.indent as f32) * ADVANCE,
                Align::Right => LEFT_MARGIN + (line.width.saturating_sub(len) as f32) * ADVANCE,
                Align::Center => {
                    LEFT_MARGIN + (line.width.saturating_sub(len) as f32 / 2.0) * ADVANCE
                }
            };
            let y = PAGE_H - TOP_MARGIN - ((index + 1) as f32) * LEADING;
            let font = match line.style {
                Style::Normal => Name(b"F1"),
                Style::Oblique => Name(b"F2"),
            };
            content.set_font(font, 12.0);
            content.set_text_matrix([1.0, 0.0, 0.0, 1.0, x, y]);
            content.show(Str(&winansi(&line.text)));
        }
        if let Some(number) = number {
            let label = format!("{number}.");
            let x = PAGE_W - 72.0 - (label.chars().count() as f32) * ADVANCE;
            content.set_font(Name(b"F1"), 12.0);
            content.set_text_matrix([1.0, 0.0, 0.0, 1.0, x, PAGE_H - 36.0]);
            content.show(Str(&winansi(&label)));
        }
        content.end_text();
        let stream = content.finish();
        pdf.stream(content_id, &stream);

        let mut page_writer = pdf.page(page_id);
        page_writer.media_box(Rect::new(0.0, 0.0, PAGE_W, PAGE_H));
        page_writer.parent(page_tree);
        page_writer.contents(content_id);
        page_writer
            .resources()
            .fonts()
            .pair(Name(b"F1"), normal)
            .pair(Name(b"F2"), oblique);
    }

    let page_count = page_ids.len() as i32;
    pdf.pages(page_tree).kids(page_ids).count(page_count);
    pdf.finish()
}

/// Export a screenplay as PDF, using the manifest for the title page when
/// the source has none. No network, no credentials.
pub fn export(manifest: &Manifest, screenplay: &str) -> Vec<u8> {
    let mut layout = layout(screenplay);
    if layout.title.is_empty() {
        let draft_date = manifest
            .created_at
            .get(..10)
            .unwrap_or(manifest.created_at.as_str());
        layout.title = vec![
            format!("Title: {}", manifest.title),
            format!("Draft date: {draft_date}"),
        ];
    }
    write_pdf(&layout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::parse_manifest;

    /// The committed M1 fixture: the pagination reference for M7.
    const FIXTURE: &str = include_str!("../../../../fixtures/screenplay-1000.fountain");

    fn manifest(title: &str) -> Manifest {
        let yaml = format!(
            "schema_version: 1\nproject_id: 00000000-0000-0000-0000-000000000001\ntitle: '{title}'\nproject_type: feature\nprimary_language: es\ncreated_at: 2026-10-20T12:00:00Z\ndefault_screenplay_path: screenplay/main.fountain\n"
        );
        parse_manifest(&yaml).expect("manifest")
    }

    /// `count` one-line action paragraphs (blank line between each).
    fn paragraphs(count: usize) -> String {
        (1..=count)
            .map(|n| format!("Acción número {n}."))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    fn page_text(page: &Page) -> Vec<&str> {
        page.lines.iter().map(|line| line.text.as_str()).collect()
    }

    #[test]
    fn wrap_respects_the_column_width_and_hard_breaks_long_words() {
        let lines = wrap("una frase corta", 20);
        assert_eq!(lines, vec!["una frase corta"]);
        for line in wrap(&"palabra ".repeat(20), 20).iter() {
            assert!(line.chars().count() <= 20, "line too wide: {line:?}");
        }
        // An unbreakable word longer than the column is split to fit.
        let long = "A".repeat(45);
        let lines = wrap(&long, 20);
        assert_eq!(lines.len(), 3);
        assert!(lines.iter().all(|l| l.chars().count() <= 20));
    }

    #[test]
    fn scene_heading_never_ends_a_page() {
        let source = format!("{}\n\nINT. CASA - DÍA\n\nMarta entra.\n", paragraphs(27));
        let layout = layout(&source);
        // 27 paragraphs = 53 lines (one line each, 26 separators).
        assert_eq!(layout.pages[0].lines.len(), 53);
        assert!(layout.pages.len() >= 2);
        let second = page_text(&layout.pages[1]);
        assert_eq!(second[0], "INT. CASA - DÍA");
        // The heading is followed by its scene on the same page.
        assert!(second.len() >= 2, "heading alone on a page: {second:?}");
    }

    #[test]
    fn character_cue_stays_with_its_first_dialogue_line() {
        let source = format!("{}\n\nMARTA\nHola, ¿qué tal?\n", paragraphs(27));
        let layout = layout(&source);
        assert_eq!(layout.pages[0].lines.len(), 53);
        let second = page_text(&layout.pages[1]);
        assert_eq!(second[0], "MARTA");
        assert_eq!(second[1], "Hola, ¿qué tal?");
    }

    #[test]
    fn split_dialogue_adds_more_and_a_continued_cue() {
        let speech = "Esta es una línea de diálogo larga que se envuelve. ".repeat(12);
        let source = format!("{}\n\nMARTA\n{}\n", paragraphs(20), speech.trim_end());
        let layout = layout(&source);
        assert!(layout.pages.len() >= 2);
        let mut splits = 0;
        for page in &layout.pages {
            if let Some(last) = page.lines.last()
                && last.text == "(MORE)"
            {
                splits += 1;
                assert_eq!(last.indent, MORE_INDENT);
            }
        }
        assert!(splits >= 1, "expected at least one (MORE) split");
        let mut continuations = 0;
        for page in &layout.pages {
            if page
                .lines
                .first()
                .is_some_and(|l| l.text.ends_with("(CONT'D)"))
            {
                continuations += 1;
                assert_eq!(page.lines[0].text, "MARTA (CONT'D)");
                assert_eq!(page.lines[0].indent, CHARACTER_INDENT);
            }
        }
        assert_eq!(continuations, splits);
    }

    #[test]
    fn explicit_page_break_starts_a_new_page() {
        let layout =
            layout("INT. A - DÍA\n\nAcción uno.\n\n===\n\nINT. B - NOCHE\n\nAcción dos.\n");
        assert_eq!(layout.pages.len(), 2);
        assert_eq!(page_text(&layout.pages[1])[0], "INT. B - NOCHE");
    }

    #[test]
    fn writer_tools_never_reach_the_page() {
        let layout = layout(
            "# Sección uno\n= Sinopsis de la escena\n[[una nota privada]]\n/* boneyard */\n\nINT. A - DÍA\n\nAcción.\n",
        );
        let text = layout
            .pages
            .iter()
            .flat_map(page_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!text.contains("Sección"));
        assert!(!text.contains("Sinopsis"));
        assert!(!text.contains("nota privada"));
        assert!(!text.contains("boneyard"));
        assert!(text.contains("INT. A - DÍA"));
    }

    #[test]
    fn element_columns_follow_the_screenplay_conventions() {
        let layout =
            layout("INT. CASA - DÍA\n\nMarta entra.\n\nMARTA\n(seria)\nHola.\n\n>CORTE A:\n");
        let lines = &layout.pages[0].lines;
        let find = |text: &str| lines.iter().find(|l| l.text == text).expect(text);
        assert_eq!(find("INT. CASA - DÍA").indent, 0);
        assert_eq!(find("Marta entra.").indent, 0);
        assert_eq!(find("MARTA").indent, CHARACTER_INDENT);
        assert_eq!(find("(seria)").indent, PARENTHETICAL_INDENT);
        assert_eq!(find("Hola.").indent, DIALOGUE_INDENT);
        assert_eq!(find("CORTE A:").align, Align::Right);
    }

    #[test]
    fn the_m1_fixture_lays_out_within_capacity() {
        let layout = layout(FIXTURE);
        assert!(layout.pages.len() > 1, "fixture must span pages");
        for page in &layout.pages {
            assert!(page.lines.len() <= LINES_PER_PAGE, "overflow page");
        }
    }

    #[test]
    fn pdf_bytes_are_well_formed_and_deterministic() {
        let layout = layout(FIXTURE);
        let first = write_pdf(&layout);
        let second = write_pdf(&layout);
        assert_eq!(first, second, "PDF output must be deterministic");
        assert!(first.starts_with(b"%PDF-"));
        let tail = &first[first.len().saturating_sub(16)..];
        assert!(
            first.windows(5).any(|w| w == b"%%EOF") || tail.ends_with(b"%%EOF\n"),
            "missing %%EOF trailer"
        );
        for marker in [&b"/Courier"[..], b"/WinAnsiEncoding", b"/Type /Page"] {
            assert!(
                first.windows(marker.len()).any(|w| w == marker),
                "missing marker {marker:?}"
            );
        }
    }

    #[test]
    fn winansi_keeps_spanish_and_maps_typography() {
        assert_eq!(winansi("DÍA"), b"D\xCDA");
        assert_eq!(winansi("ñandú"), b"\xF1and\xFA");
        assert_eq!(winansi("\u{2019}"), vec![0x92]);
        assert_eq!(winansi("→"), b"?");
    }

    #[test]
    fn page_numbers_start_after_an_unnumbered_title_page() {
        let no_title = layout("INT. A - DÍA\n\nAcción.\n");
        assert_eq!(no_title.page_numbers(), vec![Some(1)]);
        let with_title = layout("Title: Demo\n\nINT. A - DÍA\n\nAcción.\n");
        assert_eq!(with_title.title.len(), 1);
        assert_eq!(with_title.page_numbers(), vec![None, Some(2)]);
    }

    #[test]
    fn export_uses_the_manifest_title_page_when_the_source_has_none() {
        let body = "INT. A - DÍA\n\nAcción.\n";
        let bytes = export(&manifest("Demo M6"), body);
        // The manifest title rides on the title page (uppercased).
        assert!(
            bytes.windows(7).any(|w| w.eq_ignore_ascii_case(b"DEMO M6")),
            "manifest title missing from the PDF"
        );
        // And the layout the export built carries the synthesized title.
        let bare = write_pdf(&layout(body));
        assert_ne!(bytes, bare);
    }

    #[test]
    fn export_keeps_a_title_page_that_the_source_already_has() {
        let source = "Title: Guion propio\nAuthor: Alguien\n\nINT. A - DÍA\n\nAcción.\n";
        let expected = write_pdf(&layout(source));
        assert_eq!(export(&manifest("Demo"), source), expected);
    }
}

#[cfg(test)]
mod perf {
    /// Perf probe (ignored by default): full layout of the M1 fixture, the
    /// cost the editor pays (debounced) to show the live page count.
    /// Run with `cargo test -p narra-core --release --lib perf_layout_19k
    /// -- --ignored --nocapture`.
    #[test]
    #[ignore = "perf probe"]
    fn perf_layout_19k_lines() {
        let text =
            std::fs::read_to_string("../../fixtures/screenplay-1000.fountain").expect("fixture");
        let start = std::time::Instant::now();
        let layout = super::layout(&text);
        println!(
            "laid out {} pages in {:?}",
            layout.pages.len(),
            start.elapsed()
        );
    }
}
