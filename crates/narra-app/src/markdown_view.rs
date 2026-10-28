//! Read-only Markdown preview rendering (M14a, D-033).
//!
//! `narra-core::markdown` derives the block model; this module only paints
//! it with GPUI. Nothing here writes to the document or reformats the source.

use gpui::prelude::*;
use gpui::{
    AnyElement, Div, IntoElement, StrikethroughStyle, StyledText, TextRun, UnderlineStyle, div, px,
    rgb, rgba,
};
use narra_core::markdown::{Block, InlineSpan, MarkdownDocument};

use crate::i18n::{self, Lang};
use crate::{FONT_FAMILY, theme};

/// Paint a derived Markdown document inside an independently scrollable
/// read-only surface.
pub fn render(document: &MarkdownDocument, lang: Lang) -> AnyElement {
    let mut content = div()
        .w_full()
        .max_w(px(820.0))
        .flex()
        .flex_col()
        .px_6()
        .py_6();
    if document.blocks.is_empty() {
        content = content.child(
            div()
                .text_color(rgb(theme::status_fg()))
                .child(i18n::tr(lang, "preview.empty")),
        );
    } else {
        for (index, block) in document.blocks.iter().enumerate() {
            content = content.child(render_block(block, index));
        }
    }
    div()
        .id("markdown-preview")
        .size_full()
        .overflow_y_scroll()
        .bg(rgb(theme::bg()))
        .child(div().w_full().flex().justify_center().child(content))
        .into_any_element()
}

fn render_block(block: &Block, index: usize) -> Div {
    match block {
        Block::Heading { level, spans, .. } => {
            let factor = match level {
                1 => 1.75,
                2 => 1.45,
                3 => 1.25,
                _ => 1.1,
            };
            let mut heading = spans.clone();
            for span in &mut heading {
                span.style.bold = true;
            }
            div()
                .mt_4()
                .mb_2()
                .text_size(px(theme::text_size() * factor))
                .text_color(rgb(theme::default_fg()))
                .child(inline_column(&heading))
        }
        Block::Paragraph { spans } => div()
            .mb_3()
            .text_size(px(theme::text_size()))
            .text_color(rgb(theme::default_fg()))
            .child(inline_column(spans)),
        Block::ListItem {
            marker,
            depth,
            spans,
            ..
        } => div()
            .mb_1()
            .ml(px(*depth as f32 * 18.0))
            .flex()
            .flex_row()
            .gap_2()
            .text_size(px(theme::text_size()))
            .text_color(rgb(theme::default_fg()))
            .child(
                div()
                    .w(px(26.0))
                    .flex_none()
                    .text_color(rgb(theme::status_fg()))
                    .child(marker.clone()),
            )
            .child(div().flex_1().child(inline_column(spans))),
        Block::Quote { depth, spans } => div()
            .mb_3()
            .ml(px(*depth as f32 * 12.0))
            .pl_3()
            .border_l_1()
            .border_color(rgb(theme::status_fg()))
            .text_size(px(theme::text_size()))
            .text_color(rgb(theme::status_fg()))
            .child(inline_column(spans)),
        Block::Code { language, text } => {
            let mut code = div()
                .id(("markdown-code", index))
                .mb_3()
                .p_3()
                .flex()
                .flex_col()
                .gap_1()
                .bg(rgb(theme::status_bg()))
                .border_1()
                .border_color(rgb(theme::explorer_border()))
                .overflow_x_scroll()
                .text_size(px(theme::text_size() * 0.95));
            if let Some(language) = language {
                code = code.child(
                    div()
                        .text_xs()
                        .text_color(rgb(theme::gutter_fg()))
                        .child(language.clone()),
                );
            }
            if text.is_empty() {
                code = code.child(div().text_color(rgb(theme::gutter_fg())).child(" "));
            } else {
                for line in text.lines() {
                    code = code.child(
                        div()
                            .whitespace_nowrap()
                            .text_color(rgb(theme::default_fg()))
                            .child(line.to_string()),
                    );
                }
            }
            div().child(code)
        }
        Block::Table { headers, rows } => {
            let mut table = div()
                .id(("markdown-table", index))
                .mb_3()
                .w_full()
                .flex()
                .flex_col()
                .border_1()
                .border_color(rgb(theme::explorer_border()))
                .overflow_x_scroll();
            if !headers.is_empty() {
                table = table.child(table_row(headers, true));
            }
            for row in rows {
                table = table.child(table_row(row, false));
            }
            div().child(table)
        }
        Block::Rule => div()
            .my_3()
            .h(px(1.0))
            .w_full()
            .bg(rgb(theme::explorer_border())),
        Block::Raw(text) => div()
            .mb_3()
            .p_3()
            .bg(rgb(theme::status_bg()))
            .text_xs()
            .text_color(rgb(theme::gutter_fg()))
            .child(text.clone()),
    }
}

fn table_row(cells: &[Vec<InlineSpan>], header: bool) -> Div {
    let mut row = div()
        .flex()
        .w_full()
        .when(header, |row| row.bg(rgb(theme::status_bg())));
    for cell in cells {
        let mut text = cell.clone();
        if header {
            for span in &mut text {
                span.style.bold = true;
            }
        }
        row = row.child(
            div()
                .flex_1()
                .min_w(px(80.0))
                .px_2()
                .py_1()
                .border_r_1()
                .border_b_1()
                .border_color(rgb(theme::explorer_border()))
                .text_size(px(theme::text_size()))
                .text_color(rgb(theme::default_fg()))
                .child(inline_column(&text)),
        );
    }
    row
}

/// Render inline spans as a column of lines so hard breaks and any future
/// multiline spans keep their line structure. Soft breaks arrive as spaces.
fn inline_column(spans: &[InlineSpan]) -> Div {
    let mut column = div().flex().flex_col();
    for line in inline_lines(spans) {
        column = column.child(styled_text(&line));
    }
    column
}

fn inline_lines(spans: &[InlineSpan]) -> Vec<Vec<InlineSpan>> {
    let mut lines = vec![Vec::new()];
    for span in spans {
        let mut start = 0;
        for (index, character) in span.text.char_indices() {
            if character == '\n' {
                if index > start {
                    lines
                        .last_mut()
                        .expect("at least one line")
                        .push(span_fragment(span, &span.text[start..index]));
                }
                lines.push(Vec::new());
                start = index + character.len_utf8();
            }
        }
        if start < span.text.len() {
            lines
                .last_mut()
                .expect("at least one line")
                .push(span_fragment(span, &span.text[start..]));
        }
    }
    lines
}

fn span_fragment(span: &InlineSpan, text: &str) -> InlineSpan {
    InlineSpan {
        text: text.to_string(),
        style: span.style,
        link: span.link.clone(),
    }
}

fn styled_text(spans: &[InlineSpan]) -> StyledText {
    let mut text = String::new();
    let mut runs = Vec::new();
    for span in spans {
        if span.text.is_empty() {
            continue;
        }
        let mut font = gpui::font(FONT_FAMILY);
        if span.style.bold {
            font = font.bold();
        }
        if span.style.italic {
            font = font.italic();
        }
        let color = if span.style.link {
            theme::cue_fg()
        } else if span.style.code {
            theme::note_fg()
        } else {
            theme::default_fg()
        };
        text.push_str(&span.text);
        runs.push(TextRun {
            len: span.text.len(),
            font,
            color: rgb(color).into(),
            background_color: span.style.code.then(|| rgba(theme::status_bg()).into()),
            underline: span.style.link.then(|| UnderlineStyle {
                thickness: px(1.0),
                color: Some(rgb(theme::cue_fg()).into()),
                wavy: false,
            }),
            strikethrough: span.style.strikethrough.then(|| StrikethroughStyle {
                thickness: px(1.0),
                color: Some(rgb(theme::gutter_fg()).into()),
            }),
        });
    }
    StyledText::new(text).with_runs(runs)
}
