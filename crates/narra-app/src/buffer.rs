//! Spike A slice 2: multi-line text buffer.
//!
//! Design notes:
//! - Lines are `Vec<String>`; document offsets are **byte offsets** into the
//!   text joined with `\n`.
//! - Document-wide conversions are O(number of lines). That is a deliberate
//!   spike shortcut: the production path needs a rope/sum-tree (Spike B,
//!   large-file strategy), but a linear buffer keeps the correctness logic
//!   small and fully testable today.
//! - Cursor movement is grapheme-aware (`unicode-segmentation`, already in the
//!   dependency tree via GPUI): `FABIÁN`, `e` + combining acute, and emoji must
//!   move as single units (Spike B requires this).

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

/// A caret location: line index and byte column within that line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: usize,
    pub col: usize,
}

pub struct Buffer {
    lines: Vec<String>,
}

impl Buffer {
    pub fn new(text: &str) -> Self {
        Buffer {
            // `split('\n')` keeps a trailing empty line after a final newline,
            // which is what an editor buffer must show.
            lines: text.split('\n').map(str::to_string).collect(),
        }
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Length of the document in bytes (the `\n` separators included).
    pub fn doc_len(&self) -> usize {
        self.lines
            .iter()
            .map(|line| line.len() + 1)
            .sum::<usize>()
            .saturating_sub(1)
    }

    /// Clamped conversion: offsets outside the document snap to its start/end.
    pub fn position_at(&self, offset: usize) -> Position {
        let mut remaining = offset;
        for (ix, line) in self.lines.iter().enumerate() {
            if remaining <= line.len() {
                return Position {
                    line: ix,
                    col: remaining,
                };
            }
            remaining -= line.len() + 1; // skip the newline
        }
        let last = self.lines.len() - 1;
        Position {
            line: last,
            col: self.lines[last].len(),
        }
    }

    pub fn offset_at(&self, position: Position) -> usize {
        let line = position.line.min(self.lines.len() - 1);
        let mut offset = 0;
        for text in &self.lines[..line] {
            offset += text.len() + 1;
        }
        offset + position.col.min(self.lines[line].len())
    }

    /// Byte range of a line's content (the trailing `\n` is excluded).
    pub fn line_content_range(&self, line: usize) -> Range<usize> {
        let line = line.min(self.lines.len() - 1);
        let start = self.offset_at(Position { line, col: 0 });
        start..start + self.lines[line].len()
    }

    /// Text covered by a document range (newlines included).
    pub fn slice(&self, range: Range<usize>) -> String {
        if range.start >= range.end {
            return String::new();
        }
        let start = self.position_at(range.start);
        let end = self.position_at(range.end);
        if start.line == end.line {
            return self.lines[start.line][start.col..end.col].to_string();
        }
        let mut out = String::with_capacity(range.end - range.start);
        out.push_str(&self.lines[start.line][start.col..]);
        for ix in start.line + 1..end.line {
            out.push('\n');
            out.push_str(&self.lines[ix]);
        }
        out.push('\n');
        out.push_str(&self.lines[end.line][..end.col]);
        out
    }

    /// Replace a document range with `text` (which may itself contain
    /// newlines). Returns the range the new text occupies and the text that
    /// was removed (enough to implement undo).
    pub fn replace(&mut self, range: Range<usize>, text: &str) -> (Range<usize>, String) {
        let deleted = self.slice(range.clone());
        let start = self.position_at(range.start);
        let end = self.position_at(range.end);

        let head = self.lines[start.line][..start.col].to_string();
        let tail = self.lines[end.line][end.col..].to_string();
        let replacement = format!("{head}{text}{tail}");
        let new_lines: Vec<String> = replacement.split('\n').map(str::to_string).collect();

        self.lines.splice(start.line..=end.line, new_lines);

        let inserted = range.start..range.start + text.len();
        (inserted, deleted)
    }

    /// Every non-overlapping occurrence of `query` in the document, as byte
    /// ranges, in document order (Spike A slice 3 / find bar).
    ///
    /// - **Case:** Unicode-simple-lowercase folded per character, so `nOs`
    ///   matches `NOS` and `ó` matches `Ó` without breaking byte offsets
    ///   (the fold is applied char-by-char while walking, never on a copied
    ///   string whose lengths could shift).
    /// - **Scope:** one line at a time; a query containing `\n` matches
    ///   nothing. Cross-line queries belong to the production search (Spike
    ///   B), not to this spike's find bar.
    /// - **Empty query:** no matches (no highlight-everything surprise).
    /// - **Complexity:** O(document bytes × query chars) worst case — the
    ///   documented linear-buffer shortcut; the measurement below is exactly
    ///   what tells us whether a real index is needed.
    pub fn find_all(&self, query: &str) -> Vec<Range<usize>> {
        if query.is_empty() || query.contains('\n') {
            return Vec::new();
        }
        let mut matches = Vec::new();
        let mut line_start = 0usize;
        for line in &self.lines {
            find_in_line(line, query, line_start, &mut matches);
            line_start += line.len() + 1; // the newline
        }
        matches
    }

    /// Previous grapheme boundary before `offset`, stopping at the document
    /// start (crossing line boundaries backwards).
    pub fn prev_boundary(&self, offset: usize) -> usize {
        let position = self.position_at(offset);
        if position.col > 0 {
            let line = &self.lines[position.line];
            let mut previous = None;
            for (ix, _) in line.grapheme_indices(true) {
                if ix >= position.col {
                    break;
                }
                previous = Some(ix);
            }
            return previous.map_or(offset, |ix| {
                self.offset_at(Position {
                    line: position.line,
                    col: ix,
                })
            });
        }
        if position.line > 0 {
            return self.offset_at(Position {
                line: position.line - 1,
                col: self.lines[position.line - 1].len(),
            });
        }
        offset
    }

    /// Next grapheme boundary after `offset`, stopping at the document end.
    pub fn next_boundary(&self, offset: usize) -> usize {
        let position = self.position_at(offset);
        let line = &self.lines[position.line];
        if position.col < line.len() {
            let next = line
                .grapheme_indices(true)
                .map(|(ix, _)| ix)
                .find(|ix| *ix > position.col);
            return next.map_or(offset, |ix| {
                self.offset_at(Position {
                    line: position.line,
                    col: ix,
                })
            });
        }
        if position.line + 1 < self.lines.len() {
            return self.offset_at(Position {
                line: position.line + 1,
                col: 0,
            });
        }
        offset
    }

    pub fn line_start(&self, offset: usize) -> usize {
        let position = self.position_at(offset);
        self.offset_at(Position {
            line: position.line,
            col: 0,
        })
    }

    pub fn line_end(&self, offset: usize) -> usize {
        let position = self.position_at(offset);
        self.offset_at(Position {
            line: position.line,
            col: self.lines[position.line].len(),
        })
    }

    /// Character range of the word under `offset` (M9 slice 1): letters,
    /// digits, `_`, and apostrophes *between* word characters (`don't`).
    /// A caret on a non-word character returns an empty range at `offset`
    /// (double-click there selects nothing).
    pub fn word_range_at(&self, offset: usize) -> Range<usize> {
        let position = self.position_at(offset);
        let line = &self.lines[position.line];
        let chars: Vec<char> = line.chars().collect();
        let caret_chars = line[..position.col.min(line.len())].chars().count();
        let is_word = |index: usize| {
            let c = chars[index];
            c.is_alphanumeric()
                || c == '_'
                || (matches!(c, '\'' | '\u{2019}')
                    && index > 0
                    && index + 1 < chars.len()
                    && chars[index - 1].is_alphanumeric()
                    && chars[index + 1].is_alphanumeric())
        };
        // Double-click resolves to the boundary after the clicked glyph, so
        // prefer the character before the caret when it is a word char.
        let anchor = if caret_chars < chars.len() && is_word(caret_chars) {
            caret_chars
        } else if caret_chars > 0 && is_word(caret_chars - 1) {
            caret_chars - 1
        } else {
            return offset..offset;
        };
        let mut start = anchor;
        while start > 0 && is_word(start - 1) {
            start -= 1;
        }
        let mut end = anchor + 1;
        while end < chars.len() && is_word(end) {
            end += 1;
        }
        let byte_of = |index: usize| -> usize { chars[..index].iter().map(|c| c.len_utf8()).sum() };
        let line_start = self.line_content_range(position.line).start;
        (line_start + byte_of(start))..(line_start + byte_of(end))
    }

    /// Content range of the line under `offset` (triple-click selection):
    /// the line's text without its trailing newline.
    pub fn line_range_at(&self, offset: usize) -> Range<usize> {
        let position = self.position_at(offset);
        self.line_content_range(position.line)
    }

    /// Offset of the start of the word to the left of `offset` (⌥←):
    /// skip non-word characters, then walk to the start of the word.
    pub fn word_left(&self, offset: usize) -> usize {
        let text = self.full_text();
        let mut index = offset.min(text.len());
        while index > 0 {
            let previous = self.prev_boundary(index);
            if is_word_text_at(&text, previous) {
                break;
            }
            index = previous;
        }
        while index > 0 {
            let previous = self.prev_boundary(index);
            if !is_word_text_at(&text, previous) {
                break;
            }
            index = previous;
        }
        index
    }

    /// Offset just past the word to the right of `offset` (⌥→): in the
    /// middle of a word it stops at that word's end; in whitespace it skips
    /// to the end of the next word (the macOS/VSCode behaviour).
    pub fn word_right(&self, offset: usize) -> usize {
        let text = self.full_text();
        let mut index = offset.min(text.len());
        while index < text.len() && !is_word_text_at(&text, index) {
            index = self.next_boundary(index);
        }
        while index < text.len() && is_word_text_at(&text, index) {
            index = self.next_boundary(index);
        }
        index
    }

    /// Joined document text (allocating; used by word navigation and
    /// project-independent helpers).
    fn full_text(&self) -> String {
        self.lines.join("\n")
    }
    /// Move `offset` up/down by `delta` lines, clamping the column to the
    /// target line. Columns are byte-based: exact visual-column tracking
    /// across lines with different encodings is out of scope for the spike
    /// (monospace fixtures make it rare) and is recorded as a limitation.
    pub fn vertical(&self, offset: usize, delta: i64) -> usize {
        let position = self.position_at(offset);
        let target = (position.line as i64 + delta).clamp(0, self.lines.len() as i64 - 1) as usize;
        self.offset_at(Position {
            line: target,
            col: position.col.min(self.lines[target].len()),
        })
    }

    pub fn offset_to_utf16(&self, offset: usize) -> usize {
        let position = self.position_at(offset);
        let mut utf16 = 0;
        for text in &self.lines[..position.line] {
            utf16 += text.chars().map(char::len_utf16).sum::<usize>() + 1;
        }
        utf16
            + self.lines[position.line][..position.col]
                .chars()
                .map(char::len_utf16)
                .sum::<usize>()
    }

    pub fn utf16_to_offset(&self, utf16: usize) -> usize {
        let mut seen = 0usize;
        let mut byte = 0usize;
        for text in &self.lines {
            for ch in text.chars() {
                if seen >= utf16 {
                    return byte;
                }
                seen += ch.len_utf16();
                byte += ch.len_utf8();
            }
            if seen >= utf16 {
                return byte;
            }
            seen += 1; // the newline
            byte += 1;
        }
        self.doc_len()
    }
}

/// Word character at `index` in `text` (mirrors `word_range_at`): letters,
/// digits, `_`, and apostrophes between word characters.
fn is_word_text_at(text: &str, index: usize) -> bool {
    let Some(c) = text[index..].chars().next() else {
        return false;
    };
    if c.is_alphanumeric() || c == '_' {
        return true;
    }
    if matches!(c, '\'' | '\u{2019}') {
        let before = text[..index].chars().next_back();
        let after = text[index + c.len_utf8()..].chars().next();
        return before.is_some_and(|b| b.is_alphanumeric())
            && after.is_some_and(|a| a.is_alphanumeric());
    }
    false
}

/// Push every non-overlapping occurrence of `query` inside `line` as
/// document byte ranges into `out`. Matching is case-insensitive via
/// *simple* per-character lowercase folding (`c.to_lowercase()` compared
/// char-by-char), so byte offsets are never recomputed from a folded copy —
/// Turkish dotted-I style length changes can therefore desync the fold from
/// the source, which degrades to a missed/odd match, never a panic: the
/// comparison only ever advances by the *source* character's length.
fn find_in_line(line: &str, query: &str, line_start: usize, out: &mut Vec<Range<usize>>) {
    let query_chars: Vec<char> = query.chars().collect();
    if query_chars.is_empty() {
        return;
    }
    let source: Vec<(usize, char)> = line.char_indices().collect();
    let mut i = 0;
    while i < source.len() {
        let mut j = 0;
        let mut k = i;
        while j < query_chars.len() && k < source.len() {
            let mut source_fold = source[k].1.to_lowercase();
            let mut query_fold = query_chars[j].to_lowercase();
            let source_cp = source_fold.next();
            let query_cp = query_fold.next();
            if source_cp != query_cp {
                break;
            }
            // Multi-char folds must agree on the remainder too (rare; e.g.
            // a folded source char producing an extra combining mark while
            // the query char does not). Mismatch = no match here.
            if source_fold.eq(query_fold) {
                j += 1;
                k += 1;
            } else {
                break;
            }
        }
        if j == query_chars.len() {
            let start = source[i].0;
            // End = start byte of the first source char AFTER the match, or
            // the line length.
            let end = source
                .get(k)
                .map(|(offset, _)| *offset)
                .unwrap_or_else(|| line.len());
            out.push(line_start + start..line_start + end);
            i = k; // non-overlapping: continue after this match
        } else {
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_keeps_trailing_empty_line() {
        let buffer = Buffer::new("alpha\n");
        assert_eq!(buffer.lines(), ["alpha", ""]);
        assert_eq!(Buffer::new("").lines(), [""]);
    }

    #[test]
    fn position_and_offset_roundtrip() {
        let buffer = Buffer::new("hola\nmundo\n");
        for offset in 0..=buffer.doc_len() {
            assert_eq!(buffer.offset_at(buffer.position_at(offset)), offset);
        }
        // Offset pointing at a newline belongs to the end of its line.
        assert_eq!(buffer.position_at(4), Position { line: 0, col: 4 });
        // Offset after the newline belongs to the next line.
        assert_eq!(buffer.position_at(5), Position { line: 1, col: 0 });
    }

    #[test]
    fn insert_splits_and_joins_lines() {
        let mut buffer = Buffer::new("hola\nmundo");

        let (range, deleted) = buffer.replace(4..4, "\n");
        assert_eq!(range, 4..5);
        assert_eq!(deleted, "");
        assert_eq!(buffer.lines(), ["hola", "", "mundo"]);

        let (range, deleted) = buffer.replace(4..6, "x");
        assert_eq!(range, 4..5);
        // 4..6 covers the two newlines of ["hola", "", "mundo"].
        assert_eq!(deleted, "\n\n");
        assert_eq!(buffer.lines(), ["holaxmundo"]);
    }

    #[test]
    fn replace_across_lines_keeps_text_intact() {
        let mut buffer = Buffer::new("uno\ndos\ntres");
        let (range, deleted) = buffer.replace(3..8, "-");
        assert_eq!(deleted, "\ndos\n");
        assert_eq!(range, 3..4);
        assert_eq!(buffer.slice(0..buffer.doc_len()), "uno-tres");
    }

    #[test]
    fn delete_backward_is_grapheme_aware() {
        let buffer = Buffer::new("FABIÁN");
        // "Á" here is a single code point; also cover a decomposed "é".
        let decomposed = "e\u{0301}x".to_string();
        let buffer2 = Buffer::new(&decomposed);

        let end = buffer.doc_len();
        assert_eq!(buffer.prev_boundary(end), end - 'N'.len_utf8());

        let end2 = buffer2.doc_len();
        let step1 = buffer2.prev_boundary(end2);
        // After deleting "x", the caret sits after the combining pair...
        assert_eq!(step1, 3);
        assert_eq!(&decomposed[..step1], "e\u{0301}");
        // ...and one more step skips the whole grapheme, not just the mark.
        assert_eq!(buffer2.prev_boundary(step1), 0);
    }

    #[test]
    fn vertical_move_clamps_column() {
        let buffer = Buffer::new("largo texto\nab\nmucho texto");
        let offset = buffer.offset_at(Position { line: 0, col: 9 });
        // Column 9 does not exist on line 1: clamp to its end.
        let target = buffer.vertical(offset, 1);
        assert_eq!(buffer.position_at(target), Position { line: 1, col: 2 });
        // Both ends of the document clamp instead of panicking: the column is
        // preserved when the target line is long enough, the line clamps first.
        assert_eq!(buffer.vertical(offset, -5), offset);
        assert_eq!(
            buffer.position_at(buffer.vertical(offset, 5)).line,
            buffer.line_count() - 1
        );
    }

    #[test]
    fn find_all_finds_case_insensitive_matches_with_doc_offsets() {
        let buffer = Buffer::new("Noche en la ciudad\nla NOCHE vuelve\nno hay noche");
        let matches = buffer.find_all("noche");
        assert_eq!(matches.len(), 3);
        // Offsets are document byte offsets (newlines included).
        assert_eq!(&buffer.slice(matches[0].clone()), "Noche");
        assert_eq!(&buffer.slice(matches[1].clone()), "NOCHE");
        assert_eq!(&buffer.slice(matches[2].clone()), "noche");
        // Line 2 starts after "Noche en la ciudad" (18) + newline (1) = 19,
        // plus "la " (3) → "NOCHE" begins at document byte 22.
        assert_eq!(matches[1].start, 22);
    }

    #[test]
    fn find_all_is_non_overlapping_and_skips_partial_matches() {
        let buffer = Buffer::new("banana anana");
        let matches = buffer.find_all("ana");
        assert_eq!(matches.len(), 2, "non-overlapping: {matches:?}");
        assert_eq!(matches[0], 1..4);
        assert_eq!(matches[1], 7..10);
        // Query longer than the line matches nothing.
        assert!(buffer.find_all("banana split").is_empty());
    }

    #[test]
    fn find_all_handles_accents_and_empty_or_multiline_queries() {
        let buffer = Buffer::new("Habitación dos\nhabitación tres");
        let matches = buffer.find_all("HABITACIÓN");
        assert_eq!(matches.len(), 2);
        assert_eq!(&buffer.slice(matches[0].clone()), "Habitación");
        // The fold is per character: both accented case pairs match.
        let buffer2 = Buffer::new("ÁÉÍÓÚ áéíóú");
        let m = buffer2.find_all("áÉíÓú");
        assert_eq!(m.len(), 2, "both case variants match: {m:?}");
        assert_eq!(&buffer2.slice(m[0].clone()), "ÁÉÍÓÚ");
        assert_eq!(&buffer2.slice(m[1].clone()), "áéíóú");
        // Empty query and queries with a newline match nothing by contract.
        assert!(buffer.find_all("").is_empty());
        assert!(buffer.find_all("dos\nhabitación").is_empty());
    }

    #[test]
    fn utf16_mapping_handles_astral_chars() {
        let buffer = Buffer::new("ÁÉ\n🎨 emoji");
        let offset = buffer.doc_len();
        // Roundtrip at the end of the document: bytes → UTF-16 → bytes.
        assert_eq!(
            buffer.utf16_to_offset(buffer.offset_to_utf16(offset)),
            offset
        );
        assert_eq!(buffer.offset_to_utf16(0), 0);
        let emoji_line = buffer.position_at(offset);
        assert_eq!(emoji_line.line, 1);
        // Round trip for every char boundary (offsets inside a multi-byte
        // character are not valid positions, so they are excluded).
        let mut probes = Vec::new();
        let mut base = 0;
        for line in buffer.lines() {
            for col in 0..=line.len() {
                if line.is_char_boundary(col) {
                    probes.push(base + col);
                }
            }
            base += line.len() + 1;
        }
        assert!(!probes.is_empty());
        for probe in probes {
            let utf16 = buffer.offset_to_utf16(probe);
            let back = buffer.utf16_to_offset(utf16);
            assert_eq!(back, probe, "roundtrip failed at byte {probe}");
        }
    }

    /// M1-3b integrity: the diff must reconstruct the edited document from
    /// the base. This is exactly what an M4 proposal does when it is applied
    /// (base + diff = proposal result), so a wrong diff is a safety bug, not
    /// a cosmetic one. The invariant is the round trip: replaying the
    /// delete/insert hunks over the base must reproduce the edited text byte
    /// for byte, and must be a no-op when the two sides are identical.
    #[test]
    fn diff_reconstructs_edited_document() {
        let base = "INT. SALA - DIA\n\nNoche cerrada.\nMARIANA entra.\n";
        let buffer = Buffer::new(base);
        let matches = buffer.find_all("Noche");
        assert_eq!(matches.len(), 1, "expected one case-folded match");

        // Apply the replace the editor's replace-all performs, in reverse
        // offset order so earlier offsets stay valid.
        let mut edited = Buffer::new(base);
        for range in matches.iter().rev() {
            edited.replace(range.clone(), "Luz");
        }

        // Build both diff sides from `Buffer::lines()` — the exact
        // representation `recompute_diff` uses — so this test pins the
        // shipped invariant rather than a slightly different one.
        let base_text = buffer.lines().join("\n");
        let edited_text = edited.lines().join("\n");
        let d = similar::TextDiff::from_lines(&base_text, &edited_text);

        // Replay the hunks over the base: every Equal line is kept, every
        // Delete is dropped, every Insert is taken from the new side. That
        // reconstruction must equal the edited document.
        let mut replayed = String::new();
        for change in d.iter_all_changes() {
            if change.tag() != similar::ChangeTag::Delete {
                replayed.push_str(change.value());
            }
        }
        assert_eq!(
            replayed, edited_text,
            "replaying the diff over the base must reproduce the edited document"
        );

        // And a diff of a document against itself must be pure Equal.
        let identity = similar::TextDiff::from_lines(&base_text, &base_text);
        let changed = identity
            .iter_all_changes()
            .filter(|c| c.tag() != similar::ChangeTag::Equal)
            .count();
        assert_eq!(changed, 0, "self-diff must contain no changes");
    }

    /// A genuine single-line edit must produce exactly one remove + one add,
    /// so the diff is not trivially always-empty.
    #[test]
    fn single_edit_produces_one_add_one_remove() {
        let base = "uno\ndos\ntres\n";
        let buffer = Buffer::new(base);
        let range = buffer.find_all("dos")[0].clone();
        let mut edited = Buffer::new(base);
        edited.replace(range, "DOS");

        let edited_text = edited.lines().join("\n");
        let d = similar::TextDiff::from_lines(base, &edited_text);
        let mut added = 0;
        let mut removed = 0;
        for change in d.iter_all_changes() {
            match change.tag() {
                similar::ChangeTag::Insert => added += 1,
                similar::ChangeTag::Delete => removed += 1,
                similar::ChangeTag::Equal => {}
            }
        }
        assert_eq!((added, removed), (1, 1));
    }

    /// The diff base is seeded with `text.split('\n')` (see `run()`), the
    /// same representation `Buffer::new` uses. If the two ever diverge, every
    /// diff would show a phantom one-line change at EOF, so pin the match.
    #[test]
    fn diff_base_seeding_matches_buffer_lines() {
        for base in ["a\nb\n", "a\nb", "a\n\nb\n", "", "\n"] {
            let buffer = Buffer::new(base);
            let seeded: Vec<String> = base.split('\n').map(str::to_string).collect();
            assert_eq!(
                buffer.lines(),
                seeded.as_slice(),
                "base seeding diverged for {base:?}"
            );
        }
    }

    #[test]
    fn word_range_covers_letters_digits_underscores_and_inner_apostrophes() {
        let buffer = Buffer::new("Marta dijo: don't stop, 3 veces.\n");
        // Inside `Marta` (offset over `r`): the whole word.
        assert_eq!(&buffer.slice(buffer.word_range_at(2)), "Marta");
        // `don't` is one word (inner apostrophe).
        let at = "Marta dijo: ".len() + 2;
        assert_eq!(&buffer.slice(buffer.word_range_at(at)), "don't");
        // Digits are word characters.
        let at = "Marta dijo: don't stop, ".len();
        assert_eq!(&buffer.slice(buffer.word_range_at(at)), "3");
        // The caret just after `Marta` (on the space) selects the word the
        // click resolved to, matching the boundary the platform reports.
        let at = "Marta".len();
        assert_eq!(&buffer.slice(buffer.word_range_at(at)), "Marta");
        // Inside a multi-space gap (no adjacent word char before) selects
        // nothing.
        let gap = Buffer::new("a   b\n");
        assert_eq!(gap.word_range_at(2), 2..2);
    }

    #[test]
    fn word_range_keeps_accented_words_whole() {
        let buffer = Buffer::new("INT. OFICINA - DÍA\n");
        let at = "INT. OFICINA - ".len() + 1;
        assert_eq!(&buffer.slice(buffer.word_range_at(at)), "DÍA");
    }

    #[test]
    fn line_range_is_the_line_content() {
        let buffer = Buffer::new("primera línea\nsegunda\n");
        assert_eq!(&buffer.slice(buffer.line_range_at(3)), "primera línea");
        let second = "primera línea\n".len() + 2;
        assert_eq!(&buffer.slice(buffer.line_range_at(second)), "segunda");
        // Empty lines select an empty range at the line start.
        let empty = Buffer::new("a\n\nb\n");
        let at = 2;
        assert_eq!(empty.line_range_at(at), 2..2);
    }

    #[test]
    fn word_navigation_matches_macos_behaviour() {
        //           0123456789012345678901234567
        let buffer = Buffer::new("Marta dijo: don't stop.\n");
        // ⌥← from the middle of a word: its start.
        assert_eq!(buffer.word_left(3), 0);
        // ⌥← from a word start crosses the space to the previous word.
        assert_eq!(buffer.word_left(6), 0);
        // ⌥→ inside a word: that word's end.
        assert_eq!(buffer.word_right(2), 5);
        // ⌥→ from the space after a word: the end of the next word.
        assert_eq!(buffer.word_right(5), 10);
        // Inner apostrophes keep `don't` whole.
        let dont_start = "Marta dijo: ".len();
        assert_eq!(buffer.word_right(dont_start), dont_start + 5);
        assert_eq!(buffer.word_left(dont_start + 4), dont_start);
        // Crossing a newline walks to the previous line's last word.
        let two_lines = Buffer::new("primera\nsegunda\n");
        assert_eq!(
            two_lines.word_left("primera\nseg".len()),
            "primera".len() + 1
        );
        // Ends clamp.
        let end = buffer.doc_len();
        assert_eq!(buffer.word_right(end), end);
        assert_eq!(buffer.word_left(0), 0);
    }
}
