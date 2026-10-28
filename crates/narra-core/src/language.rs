//! Native, UI-neutral language service contract (M14b, D-033).
//!
//! The canonical files stay plain text. This module derives a structured
//! report (outline, symbols and diagnostics) for Fountain, Markdown and YAML.
//! Line and column positions are **0-based** (the LSP convention) so an
//! optional external LSP adapter can map its data into the same model later.
//!
//! The service never writes, reformats or resolves links. Diagnostics carry a
//! stable `code`; the UI may localize known codes and fall back to `message`
//! for provider/parser detail.

use crate::fountain::{self, ElementKind};
use crate::manifest::{ManifestError, parse_manifest};
use crate::markdown::{Block, InlineSpan, MarkdownDocument};
use saphyr::{LoadableYamlNode, Yaml};

/// Languages with a native analyzer today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageId {
    Fountain,
    Markdown,
    Yaml,
    PlainText,
}

impl LanguageId {
    /// Detect from the file extension. Unknown extensions are plain text.
    pub fn from_path(path: &str) -> Self {
        match path.rsplit('.').next() {
            Some("fountain") => LanguageId::Fountain,
            Some("md") => LanguageId::Markdown,
            Some("yaml") | Some("yml") => LanguageId::Yaml,
            _ => LanguageId::PlainText,
        }
    }

    /// Stable key for dumps and adapters.
    pub fn key(self) -> &'static str {
        match self {
            LanguageId::Fountain => "fountain",
            LanguageId::Markdown => "markdown",
            LanguageId::Yaml => "yaml",
            LanguageId::PlainText => "plain_text",
        }
    }
}

/// What an outline row represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutlineKind {
    Scene,
    Section,
    Heading,
    Key,
}

impl OutlineKind {
    pub fn key(self) -> &'static str {
        match self {
            OutlineKind::Scene => "scene",
            OutlineKind::Section => "section",
            OutlineKind::Heading => "heading",
            OutlineKind::Key => "key",
        }
    }
}

/// One outline entry. `level` is 1-based for documents (scene = 1, heading
/// h1 = 1) and indentation-based for YAML keys (top level = 0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineItem {
    pub kind: OutlineKind,
    pub level: u8,
    pub title: String,
    /// 0-based source line.
    pub line: usize,
}

/// Symbol categories exposed to future completion/navigation adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Character,
    Location,
    Heading,
    Key,
}

impl SymbolKind {
    pub fn key(self) -> &'static str {
        match self {
            SymbolKind::Character => "character",
            SymbolKind::Location => "location",
            SymbolKind::Heading => "heading",
            SymbolKind::Key => "key",
        }
    }
}

/// One named symbol at a 0-based line/column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub kind: SymbolKind,
    pub name: String,
    pub line: usize,
    pub column: usize,
    pub detail: Option<String>,
}

/// Diagnostic severity, ordered from least to most urgent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl Severity {
    pub fn key(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

/// One derived problem. `code` is stable; `message` is parser/provider detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

impl Diagnostic {
    fn at(severity: Severity, code: &str, message: impl Into<String>, line: usize) -> Diagnostic {
        Diagnostic::at_position(severity, code, message, line, 0)
    }

    fn at_position(
        severity: Severity,
        code: &str,
        message: impl Into<String>,
        line: usize,
        column: usize,
    ) -> Diagnostic {
        Diagnostic {
            severity,
            code: code.to_string(),
            message: message.into(),
            line,
            column,
            end_line: line,
            end_column: column,
        }
    }
}

/// Complete language report for one document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageReport {
    pub language: LanguageId,
    pub outline: Vec<OutlineItem>,
    pub symbols: Vec<Symbol>,
    pub diagnostics: Vec<Diagnostic>,
}

impl LanguageReport {
    /// Empty report for unknown/plain documents.
    pub fn empty(language: LanguageId) -> Self {
        Self {
            language,
            outline: Vec::new(),
            symbols: Vec::new(),
            diagnostics: Vec::new(),
        }
    }
}

/// Analyze a document by path and text. `project.yaml` gets the manifest
/// validator; other YAML files get a generic YAML parse.
pub fn analyze(path: &str, text: &str) -> LanguageReport {
    match LanguageId::from_path(path) {
        LanguageId::Fountain => analyze_fountain(text),
        LanguageId::Markdown => analyze_markdown(text),
        LanguageId::Yaml if is_project_manifest(path) => analyze_project_manifest(text),
        LanguageId::Yaml => analyze_yaml(text),
        LanguageId::PlainText => LanguageReport::empty(LanguageId::PlainText),
    }
}

/// Analyze a document when the caller already knows its language. YAML here
/// is generic; use [`analyze`] for the `project.yaml` manifest validator.
pub fn analyze_language(language: LanguageId, text: &str) -> LanguageReport {
    match language {
        LanguageId::Fountain => analyze_fountain(text),
        LanguageId::Markdown => analyze_markdown(text),
        LanguageId::Yaml => analyze_yaml(text),
        LanguageId::PlainText => LanguageReport::empty(LanguageId::PlainText),
    }
}

fn is_project_manifest(path: &str) -> bool {
    path.rsplit('/').next() == Some("project.yaml")
}

fn analyze_fountain(text: &str) -> LanguageReport {
    let elements = fountain::parse(text);
    let mut outline = Vec::new();
    let mut symbols = Vec::new();
    let mut diagnostics = Vec::new();

    for element in &elements {
        let line = element.line_number.saturating_sub(1);
        match element.kind {
            ElementKind::SceneHeading => {
                outline.push(OutlineItem {
                    kind: OutlineKind::Scene,
                    level: 1,
                    title: element.text.trim().to_string(),
                    line,
                });
                if let Some(location) = scene_location(&element.text)
                    && !symbols.iter().any(|symbol: &Symbol| {
                        symbol.kind == SymbolKind::Location && symbol.name == location
                    })
                {
                    symbols.push(Symbol {
                        kind: SymbolKind::Location,
                        name: location,
                        line,
                        column: 0,
                        detail: None,
                    });
                }
            }
            ElementKind::Section => {
                let level = text
                    .lines()
                    .nth(line)
                    .map(|source| {
                        source
                            .trim_start()
                            .chars()
                            .take_while(|character| *character == '#')
                            .count()
                    })
                    .unwrap_or(1)
                    .clamp(1, 6) as u8;
                outline.push(OutlineItem {
                    kind: OutlineKind::Section,
                    level,
                    title: element.text.trim().to_string(),
                    line,
                });
            }
            ElementKind::Character => {
                let name = character_name(&element.text);
                if !name.is_empty()
                    && !symbols
                        .iter()
                        .any(|symbol| symbol.kind == SymbolKind::Character && symbol.name == name)
                {
                    symbols.push(Symbol {
                        kind: SymbolKind::Character,
                        name,
                        line,
                        column: 0,
                        detail: None,
                    });
                }
            }
            _ => {}
        }
    }

    if text.trim().is_empty() {
        diagnostics.push(Diagnostic::at(
            Severity::Info,
            "fountain.empty_document",
            "the screenplay is empty",
            0,
        ));
    }
    if let Some(line) = unmatched_marker_line(text, "/*", "*/") {
        diagnostics.push(Diagnostic::at(
            Severity::Warning,
            "fountain.unclosed_boneyard",
            "boneyard comment is not closed",
            line,
        ));
    }
    if let Some(line) = unmatched_marker_line(text, "[[", "]]") {
        diagnostics.push(Diagnostic::at(
            Severity::Warning,
            "fountain.unclosed_note",
            "note is not closed",
            line,
        ));
    }

    LanguageReport {
        language: LanguageId::Fountain,
        outline,
        symbols,
        diagnostics,
    }
}

fn analyze_markdown(text: &str) -> LanguageReport {
    let document = MarkdownDocument::parse(text);
    let mut outline = Vec::new();
    let mut symbols = Vec::new();
    let mut diagnostics = Vec::new();

    for block in &document.blocks {
        if let Block::Heading { level, line, spans } = block {
            let title = inline_text(spans);
            if title.is_empty() {
                continue;
            }
            outline.push(OutlineItem {
                kind: OutlineKind::Heading,
                level: *level,
                title: title.clone(),
                line: *line,
            });
            symbols.push(Symbol {
                kind: SymbolKind::Heading,
                name: title,
                line: *line,
                column: 0,
                detail: Some(format!("h{level}")),
            });
        }
    }

    if let Some(line) = unclosed_fence_line(text) {
        diagnostics.push(Diagnostic::at(
            Severity::Warning,
            "markdown.unclosed_code_fence",
            "fenced code block is not closed",
            line,
        ));
    }
    if let Some(line) = empty_heading_line(text) {
        diagnostics.push(Diagnostic::at(
            Severity::Warning,
            "markdown.empty_heading",
            "heading has no text",
            line,
        ));
    }

    LanguageReport {
        language: LanguageId::Markdown,
        outline,
        symbols,
        diagnostics,
    }
}

fn analyze_project_manifest(text: &str) -> LanguageReport {
    let mut report = yaml_keys_report(text, LanguageId::Yaml);
    if let Err(error) = parse_manifest(text) {
        let field: Option<&str> = match &error {
            ManifestError::MissingField { field, .. } => Some(*field),
            ManifestError::UnknownField { field, .. } => Some(field.as_str()),
            ManifestError::InvalidValue { field, .. } => Some(*field),
            ManifestError::UnsupportedSchemaVersion { .. } => Some("schema_version"),
            ManifestError::Parse(_) => None,
        };
        let line = field.map(|field| field_line(text, field)).unwrap_or(0);
        report.diagnostics.push(Diagnostic::at(
            Severity::Error,
            "yaml.manifest",
            error.to_string(),
            line,
        ));
    }
    report
}

fn analyze_yaml(text: &str) -> LanguageReport {
    let mut report = yaml_keys_report(text, LanguageId::Yaml);
    if let Err(error) = Yaml::load_from_str(text) {
        let marker = error.marker();
        let line = marker.line().saturating_sub(1);
        let column = marker.col().saturating_sub(1);
        report.diagnostics.push(Diagnostic::at_position(
            Severity::Error,
            "yaml.parse",
            error.to_string(),
            line,
            column,
        ));
    }
    report
}

fn yaml_keys_report(text: &str, language: LanguageId) -> LanguageReport {
    let mut outline = Vec::new();
    let mut symbols = Vec::new();
    for (line, level, key) in yaml_keys(text) {
        if level == 0 {
            outline.push(OutlineItem {
                kind: OutlineKind::Key,
                level: 0,
                title: key.clone(),
                line,
            });
        }
        symbols.push(Symbol {
            kind: SymbolKind::Key,
            name: key,
            line,
            column: 0,
            detail: Some(format!("level {level}")),
        });
    }
    LanguageReport {
        language,
        outline,
        symbols,
        diagnostics: Vec::new(),
    }
}

/// Minimal indentation-aware YAML key scan. Values are intentionally ignored;
/// saphyr remains the source of truth for validity, while this scan preserves
/// source lines for the outline.
fn yaml_keys(text: &str) -> Vec<(usize, usize, String)> {
    let mut keys = Vec::new();
    for (line, raw) in text.lines().enumerate() {
        let trimmed = raw.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('-') {
            continue;
        }
        let Some((key, rest)) = trimmed.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty()
            || key.contains(' ')
            || key.starts_with('"')
            || key.starts_with('\'')
            || key.starts_with('{')
            || key.starts_with('[')
        {
            continue;
        }
        let indent = raw.len() - trimmed.len();
        let level = indent / 2;
        if rest.trim().is_empty() || rest.starts_with(' ') || rest.is_empty() {
            keys.push((line, level, key.to_string()));
        }
    }
    keys
}

fn scene_location(text: &str) -> Option<String> {
    let upper = text.to_ascii_uppercase();
    let prefixes = ["INT./EXT", "INT/EXT", "I/E", "INT", "EXT", "EST"];
    let mut rest = text;
    for prefix in prefixes {
        if upper.starts_with(prefix) {
            rest = &text[prefix.len()..];
            break;
        }
    }
    let rest = rest.trim_start_matches(['.', ':', ' ']);
    let location = rest
        .split(" - ")
        .next()
        .unwrap_or(rest)
        .split(" — ")
        .next()
        .unwrap_or(rest)
        .trim();
    (!location.is_empty()).then(|| location.to_string())
}

fn character_name(text: &str) -> String {
    text.trim()
        .split('(')
        .next()
        .unwrap_or(text)
        .trim()
        .to_string()
}

fn inline_text(spans: &[InlineSpan]) -> String {
    let mut text = String::new();
    for span in spans {
        text.push_str(&span.text);
    }
    text.trim().to_string()
}

fn unmatched_marker_line(text: &str, open: &str, close: &str) -> Option<usize> {
    let opens = text.match_indices(open).count();
    let closes = text.match_indices(close).count();
    if opens <= closes {
        return None;
    }
    text.lines()
        .position(|line| line.contains(open))
        .or(Some(0))
}

fn unclosed_fence_line(text: &str) -> Option<usize> {
    let mut open: Option<(usize, char)> = None;
    for (line, raw) in text.lines().enumerate() {
        let trimmed = raw.trim_start();
        let Some(marker) = trimmed.chars().next() else {
            continue;
        };
        if marker != '`' && marker != '~' {
            continue;
        }
        let run = trimmed
            .chars()
            .take_while(|character| *character == marker)
            .count();
        if run < 3 {
            continue;
        }
        match open {
            Some((_, open_marker)) if open_marker == marker => open = None,
            Some(_) => {}
            None => open = Some((line, marker)),
        }
    }
    open.map(|(line, _)| line)
}

fn empty_heading_line(text: &str) -> Option<usize> {
    text.lines().enumerate().find_map(|(line, raw)| {
        let trimmed = raw.trim();
        let hashes = trimmed
            .chars()
            .take_while(|character| *character == '#')
            .count();
        if hashes == 0 || hashes > 6 {
            return None;
        }
        let rest = trimmed[hashes..].trim();
        (rest.is_empty()).then_some(line)
    })
}

fn field_line(text: &str, field: &str) -> usize {
    text.lines()
        .position(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with(&format!("{field}:")) || trimmed.starts_with(&format!("{field} :"))
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FOUNTAIN: &str = "\
Title: Prueba

# Acto uno

INT. CASA - DÍA

Marta entra.

MARTA (V.O.)
Hola.

EXT. CALLE - NOCHE

Luis espera.
";

    #[test]
    fn detects_languages_by_extension() {
        assert_eq!(
            LanguageId::from_path("screenplay/main.fountain"),
            LanguageId::Fountain
        );
        assert_eq!(LanguageId::from_path("notes/a.md"), LanguageId::Markdown);
        assert_eq!(LanguageId::from_path("project.yaml"), LanguageId::Yaml);
        assert_eq!(LanguageId::from_path("README"), LanguageId::PlainText);
    }

    #[test]
    fn fountain_report_has_scenes_sections_symbols_and_no_errors() {
        let report = analyze("screenplay/main.fountain", FOUNTAIN);
        assert_eq!(report.language, LanguageId::Fountain);
        let scenes: Vec<&OutlineItem> = report
            .outline
            .iter()
            .filter(|item| item.kind == OutlineKind::Scene)
            .collect();
        assert_eq!(scenes.len(), 2);
        assert_eq!(scenes[0].title, "INT. CASA - DÍA");
        assert_eq!(scenes[0].line, 4);
        assert!(
            report
                .outline
                .iter()
                .any(|item| item.kind == OutlineKind::Section)
        );
        assert!(
            report
                .symbols
                .iter()
                .any(|symbol| { symbol.kind == SymbolKind::Character && symbol.name == "MARTA" })
        );
        assert!(
            report
                .symbols
                .iter()
                .any(|symbol| { symbol.kind == SymbolKind::Location && symbol.name == "CASA" })
        );
        assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    }

    #[test]
    fn fountain_reports_unclosed_boneyard_and_notes() {
        let report = analyze(
            "screenplay/main.fountain",
            "INT. CASA - DÍA\n\n/* no cierra\n\n[[ nota\n",
        );
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "fountain.unclosed_boneyard" && diagnostic.line == 2
        }));
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "fountain.unclosed_note" && diagnostic.line == 4
        }));
    }

    #[test]
    fn markdown_outline_and_unclosed_fence() {
        let report = analyze(
            "notes/a.md",
            "# Uno\n\ntexto\n\n## Dos\n\n```rust\nfn main() {}\n",
        );
        assert_eq!(report.outline.len(), 2);
        assert_eq!(report.outline[0].title, "Uno");
        assert_eq!(report.outline[0].line, 0);
        assert_eq!(report.outline[1].level, 2);
        assert_eq!(report.outline[1].line, 4);
        assert!(report.symbols.iter().any(|symbol| symbol.name == "Dos"));
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "markdown.unclosed_code_fence" && diagnostic.line == 6
        }));
    }

    #[test]
    fn project_manifest_reports_invalid_field_at_its_line() {
        let report = analyze(
            "project.yaml",
            "schema_version: 1\nproject_id: 00000000-0000-4000-8000-000000000001\ntitle: \"\"\nproject_type: feature\nprimary_language: es\ncreated_at: 2026-10-28T00:00:00Z\ndefault_screenplay_path: screenplay/main.fountain\n",
        );
        let diagnostic = report
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "yaml.manifest")
            .expect("manifest diagnostic");
        assert_eq!(diagnostic.severity, Severity::Error);
        assert_eq!(diagnostic.line, 2);
        assert!(report.outline.iter().any(|item| item.title == "title"));
    }

    #[test]
    fn valid_project_manifest_has_no_diagnostics() {
        let text = "\
schema_version: 1
project_id: 00000000-0000-4000-8000-000000000001
title: Serie Nueva
project_type: series
primary_language: es
additional_languages: []
created_at: 2026-10-28T00:00:00Z
default_screenplay_path: screenplay/main.fountain
";
        let report = analyze("project.yaml", text);
        assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
        assert!(report.outline.iter().any(|item| item.title == "title"));
        assert!(
            report
                .symbols
                .iter()
                .any(|symbol| symbol.name == "project_type")
        );
    }

    #[test]
    fn generic_yaml_parse_error_is_reported() {
        let report = analyze("structure/beats.yml", "a: [unclosed\n");
        assert_eq!(report.language, LanguageId::Yaml);
        let diagnostic = report
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "yaml.parse")
            .expect("yaml parse diagnostic");
        assert_eq!(diagnostic.severity, Severity::Error);
        assert_eq!(diagnostic.line, 1, "{diagnostic:?}");
        assert_eq!(diagnostic.column, 0, "{diagnostic:?}");
    }
}
