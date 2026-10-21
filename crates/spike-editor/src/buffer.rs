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
}
