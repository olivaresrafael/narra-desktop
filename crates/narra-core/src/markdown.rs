//! Read-only Markdown preview model (M14a, D-033).
//!
//! The canonical Markdown files stay plain text. This module only derives a
//! flat, render-ready model from a borrowed string: it never writes, never
//! reformats and never resolves links. A UI layer decides how to draw each
//! block; the CLI can reuse the same model later.
//!
//! Supported today: ATX/setext headings, paragraphs, emphasis, strong
//! emphasis, strikethrough, inline code, links, ordered/unordered lists,
//! block quotes, fenced and indented code, tables, rules and task markers.
//! Unsupported inline/block HTML is kept as raw text so nothing disappears
//! silently. Nested list indentation is flattened in source order.

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

/// Inline styling flags for one span. A span can combine several flags (for
/// example bold text inside a link); the UI maps each flag to a text run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InlineStyle {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strikethrough: bool,
    pub link: bool,
}

/// One run of inline text with a single style and optional link target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineSpan {
    pub text: String,
    pub style: InlineStyle,
    /// Raw destination URL for links (never fetched by this module).
    pub link: Option<String>,
}

/// One render-ready Markdown block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Heading {
        level: u8,
        /// 0-based source line where the heading starts.
        line: usize,
        spans: Vec<InlineSpan>,
    },
    Paragraph {
        spans: Vec<InlineSpan>,
    },
    ListItem {
        ordered: bool,
        marker: String,
        depth: usize,
        spans: Vec<InlineSpan>,
    },
    Quote {
        depth: usize,
        spans: Vec<InlineSpan>,
    },
    Code {
        language: Option<String>,
        text: String,
    },
    Table {
        headers: Vec<Vec<InlineSpan>>,
        rows: Vec<Vec<Vec<InlineSpan>>>,
    },
    Rule,
    /// Raw HTML or another construct the preview shows verbatim.
    Raw(String),
}

/// Derived Markdown document. Empty input yields zero blocks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarkdownDocument {
    pub blocks: Vec<Block>,
}

impl MarkdownDocument {
    /// Parse `source` into a read-only preview model. Never panics on
    /// malformed input: the parser closes unbalanced constructs itself.
    pub fn parse(source: &str) -> Self {
        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TASKLISTS);
        let parser = Parser::new_ext(source, options);
        let mut line_starts = vec![0usize];
        line_starts.extend(
            source
                .char_indices()
                .filter(|(_, character)| *character == '\n')
                .map(|(index, _)| index + 1),
        );
        let mut builder = Builder {
            line_starts,
            ..Builder::default()
        };
        for (event, range) in parser.into_offset_iter() {
            builder.event(event, range.start);
        }
        builder.finish()
    }
}

#[derive(Debug, Clone, Copy)]
struct ListState {
    ordered: bool,
    next: u64,
}

#[derive(Debug, Default)]
struct CodeState {
    language: Option<String>,
    text: String,
}

#[derive(Debug, Default)]
struct TableState {
    headers: Vec<Vec<InlineSpan>>,
    rows: Vec<Vec<Vec<InlineSpan>>>,
    row: Vec<Vec<InlineSpan>>,
}

#[derive(Debug)]
enum Container {
    Paragraph(Vec<InlineSpan>),
    Heading {
        level: u8,
        line: usize,
        spans: Vec<InlineSpan>,
    },
    Item {
        ordered: bool,
        marker: String,
        depth: usize,
        spans: Vec<InlineSpan>,
    },
    Quote {
        depth: usize,
        spans: Vec<InlineSpan>,
    },
    TableCell(Vec<InlineSpan>),
}

impl Container {
    fn spans_mut(&mut self) -> &mut Vec<InlineSpan> {
        match self {
            Container::Paragraph(spans)
            | Container::Item { spans, .. }
            | Container::Quote { spans, .. }
            | Container::TableCell(spans) => spans,
            Container::Heading { spans, .. } => spans,
        }
    }
}

#[derive(Debug, Default)]
struct Builder {
    blocks: Vec<Block>,
    stack: Vec<Container>,
    lists: Vec<ListState>,
    code: Option<CodeState>,
    table: Option<TableState>,
    bold: usize,
    italic: usize,
    strike: usize,
    links: Vec<String>,
    /// Byte offsets of line starts, for headings/symbols.
    line_starts: Vec<usize>,
    /// Text the parser emitted outside a block (malformed input only).
    loose: Vec<InlineSpan>,
}

impl Builder {
    fn event(&mut self, event: Event<'_>, offset: usize) {
        match event {
            Event::Start(tag) => self.start(tag, offset),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.push_text(&text),
            Event::Code(text) => self.push_styled(
                &text,
                InlineStyle {
                    code: true,
                    ..InlineStyle::default()
                },
            ),
            Event::InlineMath(text) | Event::DisplayMath(text) => self.push_styled(
                &text,
                InlineStyle {
                    code: true,
                    ..InlineStyle::default()
                },
            ),
            Event::Html(text) | Event::InlineHtml(text) => {
                if self.stack.is_empty() {
                    self.blocks.push(Block::Raw(text.to_string()));
                } else {
                    self.push_styled(
                        &text,
                        InlineStyle {
                            code: true,
                            ..InlineStyle::default()
                        },
                    );
                }
            }
            Event::SoftBreak => self.push_text(" "),
            Event::HardBreak => self.push_text("\n"),
            Event::Rule => self.blocks.push(Block::Rule),
            Event::TaskListMarker(checked) => {
                self.push_text(if checked { "☑ " } else { "☐ " });
            }
            Event::FootnoteReference(label) => self.push_text(&format!("[^{label}]")),
        }
    }

    fn start(&mut self, tag: Tag<'_>, offset: usize) {
        match tag {
            Tag::Paragraph => {
                // Paragraphs inside an item or a quote share that container:
                // inserting another container would detach the text from the
                // list/quote block.
                let item_has_text = matches!(
                    self.stack.last(),
                    Some(Container::Item { spans, .. }) if !spans.is_empty()
                );
                let quote_has_text = matches!(
                    self.stack.last(),
                    Some(Container::Quote { spans, .. }) if !spans.is_empty()
                );
                let collects_inline = matches!(
                    self.stack.last(),
                    Some(Container::Item { .. })
                        | Some(Container::Quote { .. })
                        | Some(Container::TableCell(_))
                        | Some(Container::Heading { .. })
                );
                if item_has_text || quote_has_text {
                    self.push_text("\n\n");
                } else if !collects_inline {
                    self.stack.push(Container::Paragraph(Vec::new()));
                }
            }
            Tag::Heading { level, .. } => self.stack.push(Container::Heading {
                level: level as u8,
                line: self.line_for(offset),
                spans: Vec::new(),
            }),
            Tag::BlockQuote(_) => {
                let depth = self
                    .stack
                    .iter()
                    .filter(|container| matches!(container, Container::Quote { .. }))
                    .count();
                self.stack.push(Container::Quote {
                    depth,
                    spans: Vec::new(),
                });
            }
            Tag::CodeBlock(kind) => {
                let language = match kind {
                    CodeBlockKind::Fenced(language) if !language.trim().is_empty() => {
                        Some(language.trim().to_string())
                    }
                    _ => None,
                };
                self.code = Some(CodeState {
                    language,
                    text: String::new(),
                });
            }
            Tag::List(start) => {
                // A nested list starts while its parent item is still open.
                // Emit the parent's text first so the preview keeps source
                // order (parent, then children) instead of showing the
                // parent after its nested items.
                let flushed = if let Some(Container::Item {
                    ordered,
                    marker,
                    depth,
                    spans,
                }) = self.stack.last_mut()
                {
                    let spans = trim_spans(std::mem::take(spans));
                    (!spans.is_empty()).then_some((*ordered, marker.clone(), *depth, spans))
                } else {
                    None
                };
                if let Some((ordered, marker, depth, spans)) = flushed {
                    self.blocks.push(Block::ListItem {
                        ordered,
                        marker,
                        depth,
                        spans,
                    });
                }
                self.lists.push(ListState {
                    ordered: start.is_some(),
                    next: start.unwrap_or(1),
                });
            }
            Tag::Item => {
                let (ordered, marker, depth) = if let Some(list) = self.lists.last_mut() {
                    let ordered = list.ordered;
                    let marker = if ordered {
                        let number = list.next;
                        list.next = list.next.saturating_add(1);
                        format!("{number}.")
                    } else {
                        "•".to_string()
                    };
                    (ordered, marker, self.lists.len().saturating_sub(1))
                } else {
                    (false, "•".to_string(), 0)
                };
                self.stack.push(Container::Item {
                    ordered,
                    marker,
                    depth,
                    spans: Vec::new(),
                });
            }
            Tag::Table(_) => self.table = Some(TableState::default()),
            Tag::TableHead => {
                if let Some(table) = self.table.as_mut() {
                    table.row.clear();
                }
            }
            Tag::TableRow => {
                if let Some(table) = self.table.as_mut() {
                    table.row.clear();
                }
            }
            Tag::TableCell => self.stack.push(Container::TableCell(Vec::new())),
            Tag::Emphasis => self.italic = self.italic.saturating_add(1),
            Tag::Strong => self.bold = self.bold.saturating_add(1),
            Tag::Strikethrough => self.strike = self.strike.saturating_add(1),
            Tag::Link { dest_url, .. } => self.links.push(dest_url.to_string()),
            // Image alt text arrives as ordinary Text events; showing it is
            // more useful than dropping the whole image reference.
            Tag::Image { .. } => {}
            _ => {}
        }
    }

    fn line_for(&self, offset: usize) -> usize {
        self.line_starts
            .partition_point(|start| *start <= offset)
            .saturating_sub(1)
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                if matches!(self.stack.last(), Some(Container::Paragraph(_))) {
                    if let Some(Container::Paragraph(spans)) = self.stack.pop() {
                        self.emit_paragraph(spans);
                    }
                } else if let Some(Container::Quote { depth, spans }) = self.stack.last_mut() {
                    // A quote can contain several paragraphs. Emit each one
                    // as its own block but keep the quote container open.
                    let spans = trim_spans(std::mem::take(spans));
                    if !spans.is_empty() {
                        self.blocks.push(Block::Quote {
                            depth: *depth,
                            spans,
                        });
                    }
                }
            }
            TagEnd::Heading(_) => {
                if let Some(Container::Heading { level, line, spans }) = self.stack.pop() {
                    let spans = trim_spans(spans);
                    if !spans.is_empty() {
                        self.blocks.push(Block::Heading { level, line, spans });
                    }
                }
            }
            TagEnd::BlockQuote(_) => {
                if let Some(Container::Quote { depth, spans }) = self.stack.pop() {
                    let spans = trim_spans(spans);
                    if !spans.is_empty() {
                        self.blocks.push(Block::Quote { depth, spans });
                    }
                }
            }
            TagEnd::CodeBlock => {
                if let Some(code) = self.code.take() {
                    self.blocks.push(Block::Code {
                        language: code.language,
                        text: code.text.trim_end_matches('\n').to_string(),
                    });
                }
            }
            TagEnd::List(_) => {
                self.lists.pop();
            }
            TagEnd::Item => {
                if let Some(Container::Item {
                    ordered,
                    marker,
                    depth,
                    spans,
                }) = self.stack.pop()
                {
                    let spans = trim_spans(spans);
                    if !spans.is_empty() {
                        self.blocks.push(Block::ListItem {
                            ordered,
                            marker,
                            depth,
                            spans,
                        });
                    }
                }
            }
            TagEnd::TableHead => {
                if let Some(table) = self.table.as_mut() {
                    table.headers = trim_cells(std::mem::take(&mut table.row));
                }
            }
            TagEnd::TableRow => {
                if let Some(table) = self.table.as_mut() {
                    let row = trim_cells(std::mem::take(&mut table.row));
                    if !row.is_empty() {
                        table.rows.push(row);
                    }
                }
            }
            TagEnd::TableCell => {
                if let Some(Container::TableCell(spans)) = self.stack.pop()
                    && let Some(table) = self.table.as_mut()
                {
                    table.row.push(trim_spans(spans));
                }
            }
            TagEnd::Table => {
                if let Some(table) = self.table.take()
                    && (!table.headers.is_empty() || !table.rows.is_empty())
                {
                    self.blocks.push(Block::Table {
                        headers: table.headers,
                        rows: table.rows,
                    });
                }
            }
            TagEnd::Emphasis => self.italic = self.italic.saturating_sub(1),
            TagEnd::Strong => self.bold = self.bold.saturating_sub(1),
            TagEnd::Strikethrough => self.strike = self.strike.saturating_sub(1),
            TagEnd::Link => {
                self.links.pop();
            }
            _ => {}
        }
    }

    fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if let Some(code) = self.code.as_mut() {
            code.text.push_str(text);
            return;
        }
        let style = InlineStyle {
            bold: self.bold > 0,
            italic: self.italic > 0,
            code: false,
            strikethrough: self.strike > 0,
            link: !self.links.is_empty(),
        };
        self.push_with_style(text, style);
    }

    fn push_styled(&mut self, text: &str, style: InlineStyle) {
        if text.is_empty() {
            return;
        }
        if let Some(code) = self.code.as_mut() {
            code.text.push_str(text);
            return;
        }
        self.push_with_style(text, style);
    }

    fn push_with_style(&mut self, text: &str, style: InlineStyle) {
        let link = self.links.last().cloned();
        if let Some(container) = self.stack.last_mut() {
            push_span(container.spans_mut(), text, style, link);
        } else {
            // Text outside any block (malformed input): keep it instead of
            // silently dropping content.
            push_span(&mut self.loose, text, style, link);
        }
    }

    fn emit_paragraph(&mut self, spans: Vec<InlineSpan>) {
        let spans = trim_spans(spans);
        if !spans.is_empty() {
            self.blocks.push(Block::Paragraph { spans });
        }
    }

    fn finish(mut self) -> MarkdownDocument {
        if let Some(code) = self.code.take() {
            self.blocks.push(Block::Code {
                language: code.language,
                text: code.text.trim_end_matches('\n').to_string(),
            });
        }
        while let Some(container) = self.stack.pop() {
            match container {
                Container::Paragraph(spans) => self.emit_paragraph(spans),
                Container::Heading { level, line, spans } => {
                    let spans = trim_spans(spans);
                    if !spans.is_empty() {
                        self.blocks.push(Block::Heading { level, line, spans });
                    }
                }
                Container::Item {
                    ordered,
                    marker,
                    depth,
                    spans,
                } => {
                    let spans = trim_spans(spans);
                    if !spans.is_empty() {
                        self.blocks.push(Block::ListItem {
                            ordered,
                            marker,
                            depth,
                            spans,
                        });
                    }
                }
                Container::Quote { depth, spans } => {
                    let spans = trim_spans(spans);
                    if !spans.is_empty() {
                        self.blocks.push(Block::Quote { depth, spans });
                    }
                }
                Container::TableCell(_) => {}
            }
        }
        let loose = trim_spans(self.loose);
        if !loose.is_empty() {
            self.blocks.push(Block::Paragraph { spans: loose });
        }
        MarkdownDocument {
            blocks: self.blocks,
        }
    }
}

fn push_span(spans: &mut Vec<InlineSpan>, text: &str, style: InlineStyle, link: Option<String>) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = spans.last_mut()
        && last.style == style
        && last.link.as_deref() == link.as_deref()
    {
        last.text.push_str(text);
        return;
    }
    spans.push(InlineSpan {
        text: text.to_string(),
        style,
        link,
    });
}

/// Trim outer whitespace without losing interior formatting. The parser
/// often leaves a trailing newline before a block end; previews should not
/// show that as an empty line.
fn trim_spans(mut spans: Vec<InlineSpan>) -> Vec<InlineSpan> {
    while let Some(first) = spans.first_mut() {
        let trimmed = first.text.trim_start().to_string();
        if trimmed.len() == first.text.len() {
            break;
        }
        first.text = trimmed;
        if !first.text.is_empty() {
            break;
        }
        spans.remove(0);
    }
    while let Some(last) = spans.last_mut() {
        let trimmed = last.text.trim_end().to_string();
        if trimmed.len() == last.text.len() {
            break;
        }
        last.text = trimmed;
        if !last.text.is_empty() {
            break;
        }
        spans.pop();
    }
    spans.retain(|span| !span.text.is_empty());
    spans
}

fn trim_cells(cells: Vec<Vec<InlineSpan>>) -> Vec<Vec<InlineSpan>> {
    cells.into_iter().map(trim_spans).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of(spans: &[InlineSpan]) -> String {
        spans.iter().map(|span| span.text.as_str()).collect()
    }

    #[test]
    fn headings_paragraphs_and_emphasis_become_spans() {
        let document = MarkdownDocument::parse(
            "# Título\n\nUna **negrita** y *cursiva* con `code` y [enlace](https://example.com).\n",
        );
        assert_eq!(document.blocks.len(), 2);
        let Block::Heading { level, spans, line } = &document.blocks[0] else {
            panic!("expected heading");
        };
        assert_eq!(*level, 1);
        assert_eq!(*line, 0);
        assert_eq!(text_of(spans), "Título");
        let Block::Paragraph { spans } = &document.blocks[1] else {
            panic!("expected paragraph");
        };
        assert_eq!(text_of(spans), "Una negrita y cursiva con code y enlace.");
        assert!(spans.iter().any(|span| span.style.bold));
        assert!(spans.iter().any(|span| span.style.italic));
        assert!(spans.iter().any(|span| span.style.code));
        let link = spans
            .iter()
            .find(|span| span.style.link)
            .expect("link span");
        assert_eq!(link.link.as_deref(), Some("https://example.com"));
    }

    #[test]
    fn lists_quotes_and_code_blocks_are_structured() {
        let document = MarkdownDocument::parse(
            "1. Primero\n2. Segundo\n\n- Uno\n- Dos\n\n> Nota\n> citada\n\n```rust\nfn main() {}\n```\n",
        );
        let items: Vec<&Block> = document
            .blocks
            .iter()
            .filter(|block| matches!(block, Block::ListItem { .. }))
            .collect();
        assert_eq!(items.len(), 4, "{:#?}", document.blocks);
        let Block::ListItem {
            ordered,
            marker,
            depth,
            spans,
        } = items[0]
        else {
            unreachable!()
        };
        assert!(*ordered);
        assert_eq!(marker, "1.");
        assert_eq!(*depth, 0);
        assert_eq!(text_of(spans), "Primero");
        assert!(matches!(items[2], Block::ListItem { ordered: false, .. }));
        assert!(document.blocks.iter().any(|block| matches!(
            block,
            Block::Quote { spans, .. } if text_of(spans).contains("Nota")
        )));
        let Block::Code { language, text } = document
            .blocks
            .iter()
            .find(|block| matches!(block, Block::Code { .. }))
            .expect("code block")
        else {
            unreachable!()
        };
        assert_eq!(language.as_deref(), Some("rust"));
        assert_eq!(text, "fn main() {}");
    }

    #[test]
    fn tables_keep_headers_and_rows() {
        let document = MarkdownDocument::parse(
            "| Personaje | Rol |\n| --- | --- |\n| Marta | Protagonista |\n| Luis | Aliado |\n",
        );
        let Block::Table { headers, rows } = &document.blocks[0] else {
            panic!("expected table: {:#?}", document.blocks);
        };
        assert_eq!(headers.len(), 2);
        assert_eq!(text_of(&headers[0]), "Personaje");
        assert_eq!(rows.len(), 2);
        assert_eq!(text_of(&rows[1][1]), "Aliado");
    }

    #[test]
    fn task_markers_and_rules_are_visible() {
        let document = MarkdownDocument::parse("- [x] hecha\n- [ ] pendiente\n\n---\n");
        let text: String = document
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::ListItem { spans, .. } => Some(text_of(spans)),
                _ => None,
            })
            .collect();
        assert!(text.contains("☑"), "{text}");
        assert!(text.contains("☐"), "{text}");
        assert!(
            document
                .blocks
                .iter()
                .any(|block| matches!(block, Block::Rule))
        );
    }

    #[test]
    fn html_is_kept_raw_and_malformed_input_does_not_panic() {
        let html = MarkdownDocument::parse("<div>hola</div>\n");
        assert!(
            html.blocks
                .iter()
                .any(|block| matches!(block, Block::Raw(text) if text.contains("hola")))
        );

        let malformed =
            MarkdownDocument::parse("**sin cerrar\n\n| a | b\n| --- |\n| 1 |\n\n```\nsin cierre\n");
        assert!(!malformed.blocks.is_empty());
        let _ = MarkdownDocument::parse("");
    }

    #[test]
    fn nested_list_keeps_source_order() {
        let document = MarkdownDocument::parse("- Marta\n  - hija\n- Luis\n");
        let items: Vec<(usize, String)> = document
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::ListItem { depth, spans, .. } => Some((*depth, text_of(spans))),
                _ => None,
            })
            .collect();
        assert_eq!(
            items,
            vec![
                (0, "Marta".to_string()),
                (1, "hija".to_string()),
                (0, "Luis".to_string())
            ],
            "{:#?}",
            document.blocks
        );
    }

    #[test]
    fn empty_and_plain_text_round_trip_read_only() {
        let source = "Solo texto\nsin formato.\n";
        let document = MarkdownDocument::parse(source);
        assert_eq!(
            document.blocks,
            vec![Block::Paragraph {
                spans: vec![InlineSpan {
                    text: "Solo texto sin formato.".to_string(),
                    style: InlineStyle::default(),
                    link: None,
                }],
            }]
        );
        assert_eq!(source, "Solo texto\nsin formato.\n");
    }
}
