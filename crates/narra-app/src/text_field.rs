//! Reusable text-field state for modal inputs (M17 slice 1).
//!
//! The editor owns one text buffer; small dialogs used to keep raw `String`s
//! and append/truncate characters. That made text selection, caret placement
//! and copy/paste impossible. This model keeps a grapheme-aware caret and
//! selection for single-line or multiline fields; the view in `main.rs`
//! renders it with the same line height and wrapping rules as the editor.

use std::ops::Range;

use gpui::{Bounds, Pixels, Point, Window};
use unicode_segmentation::UnicodeSegmentation;

use crate::{shape_line, theme, wrap_row_starts};

/// One visual row of a text field, as a byte range in the field text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextRow {
    pub start: usize,
    pub end: usize,
}

/// Caret and selection state for one input.
#[derive(Debug, Clone)]
pub struct TextField {
    text: String,
    /// Active end of the selection.
    caret: usize,
    /// Fixed end while extending; equal to `caret` when there is no selection.
    anchor: usize,
    multiline: bool,
    dragging: bool,
}

impl TextField {
    pub fn single_line() -> Self {
        Self {
            text: String::new(),
            caret: 0,
            anchor: 0,
            multiline: false,
            dragging: false,
        }
    }

    pub fn multiline() -> Self {
        Self {
            multiline: true,
            ..Self::single_line()
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn caret(&self) -> usize {
        self.caret
    }

    pub fn selection(&self) -> Range<usize> {
        self.anchor.min(self.caret)..self.anchor.max(self.caret)
    }

    pub fn has_selection(&self) -> bool {
        !self.selection().is_empty()
    }

    pub fn selected_text(&self) -> &str {
        let range = self.selection();
        &self.text[range]
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    /// Replace the selection with `text`. Newlines are accepted only in a
    /// multiline field; control characters never enter the model.
    pub fn insert(&mut self, text: &str) {
        let text = self.sanitize(text);
        let range = self.selection();
        self.text.replace_range(range.clone(), &text);
        self.caret = range.start + text.len();
        self.anchor = self.caret;
        self.dragging = false;
    }

    pub fn backspace(&mut self) {
        let range = self.selection();
        if range.is_empty() {
            let previous = self.prev_boundary(self.caret);
            self.replace(previous..self.caret);
        } else {
            self.replace(range);
        }
    }

    pub fn delete(&mut self) {
        let range = self.selection();
        if range.is_empty() {
            let next = self.next_boundary(self.caret);
            self.replace(self.caret..next);
        } else {
            self.replace(range);
        }
    }

    pub fn move_left(&mut self, extend: bool) {
        if !extend && self.has_selection() {
            self.set_caret(self.selection().start, false);
            return;
        }
        self.set_caret(self.prev_boundary(self.caret), extend);
    }

    pub fn move_right(&mut self, extend: bool) {
        if !extend && self.has_selection() {
            self.set_caret(self.selection().end, false);
            return;
        }
        self.set_caret(self.next_boundary(self.caret), extend);
    }

    pub fn move_home(&mut self, extend: bool) {
        let target = if self.multiline {
            self.line_start(self.caret)
        } else {
            0
        };
        self.set_caret(target, extend);
    }

    pub fn move_end(&mut self, extend: bool) {
        let target = if self.multiline {
            self.line_end(self.caret)
        } else {
            self.text.len()
        };
        self.set_caret(target, extend);
    }

    pub fn move_up(&mut self, extend: bool) {
        if !self.multiline {
            return;
        }
        let start = self.line_start(self.caret);
        if start == 0 {
            self.set_caret(0, extend);
            return;
        }
        let column = self.caret - start;
        let previous_end = start.saturating_sub(1);
        let previous_start = self.line_start(previous_end);
        let target = (previous_start + column).min(previous_end);
        self.set_caret(self.clamp_boundary(target), extend);
    }

    pub fn move_down(&mut self, extend: bool) {
        if !self.multiline {
            return;
        }
        let start = self.line_start(self.caret);
        let end = self.line_end(self.caret);
        if end >= self.text.len() {
            self.set_caret(self.text.len(), extend);
            return;
        }
        let column = self.caret - start;
        let next_start = end + 1;
        let next_end = self.line_end(next_start);
        let target = (next_start + column).min(next_end);
        self.set_caret(self.clamp_boundary(target), extend);
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.caret = self.text.len();
        self.dragging = false;
    }

    pub fn set_caret(&mut self, offset: usize, extend: bool) {
        let offset = self.clamp_boundary(offset);
        self.caret = offset;
        if !extend {
            self.anchor = offset;
        }
        self.dragging = false;
    }

    pub fn begin_drag(&mut self, offset: usize, extend: bool) {
        let offset = self.clamp_boundary(offset);
        if !extend {
            self.anchor = offset;
        }
        self.caret = offset;
        self.dragging = true;
    }

    pub fn drag_to(&mut self, offset: usize) {
        if self.dragging {
            self.caret = self.clamp_boundary(offset);
        }
    }

    pub fn end_drag(&mut self) {
        self.dragging = false;
    }

    /// Visual rows for rendering. Each row excludes the newline; empty
    /// logical lines still produce one zero-length row so the caret and
    /// placeholder have a line box.
    pub fn rows(&self, window: &Window, wrap_width: f32) -> Vec<TextRow> {
        if !self.multiline {
            return vec![TextRow {
                start: 0,
                end: self.text.len(),
            }];
        }
        let mut rows = Vec::new();
        let mut line_start = 0;
        for line in self.text.split('\n') {
            let line_end = line_start + line.len();
            let glyphs = glyph_positions(window, line);
            let starts = wrap_row_starts(&glyphs, wrap_width);
            for (index, start) in starts.iter().enumerate() {
                let end = starts.get(index + 1).copied().unwrap_or(line.len());
                rows.push(TextRow {
                    start: line_start + start,
                    end: line_start + end,
                });
            }
            line_start = line_end + 1;
        }
        if rows.is_empty() {
            rows.push(TextRow { start: 0, end: 0 });
        }
        rows
    }

    /// Byte offset under a window point. `bounds` is the field's content box
    /// (inside padding), so the mapping matches what the rows render.
    pub fn index_for_point(
        &self,
        window: &Window,
        point: Point<Pixels>,
        bounds: Bounds<Pixels>,
        wrap_width: f32,
    ) -> usize {
        let rows = self.rows(window, wrap_width);
        let line_height = theme::line_height();
        if point.y < bounds.top() {
            return 0;
        }
        if point.y >= bounds.bottom() {
            return self.text.len();
        }
        let row_index = (((f32::from(point.y - bounds.top())) / line_height).floor() as usize)
            .min(rows.len().saturating_sub(1));
        let row = rows[row_index];
        if row.start == row.end {
            return row.start;
        }
        let line_start = self.text[..row.start].rfind('\n').map_or(0, |ix| ix + 1);
        let row_text = &self.text[row.start..row.end];
        let shaped = shape_line(window, row_text);
        let glyphs = glyph_positions(window, row_text);
        let row_start_x = glyphs.first().map_or(0.0, |(_, x)| *x);
        let x = f32::from(point.x - bounds.left()) + row_start_x;
        let col = shaped.closest_index_for_x(x.into()).min(row_text.len());
        let offset = row.start + col;
        let snapped = self.prev_boundary(offset);
        if self.next_boundary(snapped) == offset {
            offset
        } else {
            snapped.max(line_start)
        }
    }

    fn replace(&mut self, range: Range<usize>) {
        let range = self.clamp_range(range);
        self.text.replace_range(range.clone(), "");
        self.caret = range.start;
        self.anchor = range.start;
        self.dragging = false;
    }

    fn sanitize(&self, text: &str) -> String {
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        let mut out = String::with_capacity(normalized.len());
        for ch in normalized.chars() {
            if ch == '\n' && self.multiline {
                out.push(ch);
            } else if ch == '\n' || ch == '\t' {
                out.push(' ');
            } else if !ch.is_control() {
                out.push(ch);
            }
        }
        out
    }

    fn clamp_boundary(&self, offset: usize) -> usize {
        let mut offset = offset.min(self.text.len());
        while offset > 0 && !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }

    fn clamp_range(&self, range: Range<usize>) -> Range<usize> {
        let start = self.clamp_boundary(range.start);
        let end = self.clamp_boundary(range.end.max(start));
        start..end
    }

    fn line_start(&self, offset: usize) -> usize {
        let offset = self.clamp_boundary(offset);
        self.text[..offset].rfind('\n').map_or(0, |ix| ix + 1)
    }

    fn line_end(&self, offset: usize) -> usize {
        let offset = self.clamp_boundary(offset);
        self.text[offset..]
            .find('\n')
            .map_or(self.text.len(), |ix| offset + ix)
    }

    fn prev_boundary(&self, offset: usize) -> usize {
        let offset = self.clamp_boundary(offset);
        self.text
            .grapheme_indices(true)
            .rev()
            .find_map(|(index, _)| (index < offset).then_some(index))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        let offset = self.clamp_boundary(offset);
        self.text
            .grapheme_indices(true)
            .find_map(|(index, _)| (index > offset).then_some(index))
            .unwrap_or(self.text.len())
    }
}

fn glyph_positions(window: &Window, text: &str) -> Vec<(usize, f32)> {
    shape_line(window, text)
        .runs
        .iter()
        .flat_map(|run| {
            run.glyphs
                .iter()
                .map(|glyph| (glyph.index, f32::from(glyph.position.x)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserting_and_selecting_uses_byte_offsets() {
        let mut field = TextField::single_line();
        field.insert("Hola");
        assert_eq!(field.text(), "Hola");
        assert_eq!(field.caret(), 4);
        field.set_caret(1, true);
        assert_eq!(field.selection(), 1..4);
        assert_eq!(field.selected_text(), "ola");
        field.insert("i");
        assert_eq!(field.text(), "Hi");
        assert_eq!(field.caret(), 2);
        assert_eq!(field.selection(), 2..2);
    }

    #[test]
    fn backspace_and_delete_are_grapheme_aware() {
        let mut field = TextField::single_line();
        field.insert("e\u{0301}x");
        field.backspace();
        assert_eq!(field.text(), "e\u{0301}");
        field.backspace();
        assert_eq!(field.text(), "");
        field.insert("FABIÁN");
        field.set_caret(0, false);
        field.delete();
        assert_eq!(field.text(), "ABIÁN");
    }

    #[test]
    fn single_line_replaces_newlines_and_control_characters() {
        let mut field = TextField::single_line();
        field.insert("uno\ndos\t\u{0007}");
        assert_eq!(field.text(), "uno dos ");
    }

    #[test]
    fn multiline_navigation_crosses_logical_lines() {
        let mut field = TextField::multiline();
        field.insert("uno\ndos\n");
        field.set_caret(6, false); // "dos", column 2
        field.move_up(false);
        assert_eq!(&field.text()[field.selection()], "");
        assert_eq!(field.caret(), 2);
        field.move_down(false);
        assert_eq!(field.caret(), 6);
        field.move_end(false);
        assert_eq!(field.caret(), 7);
    }
}
