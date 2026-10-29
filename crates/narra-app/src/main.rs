//! narra-app — the Narra desktop application (writing and agent
//! workspaces, M6). Formerly the `spike-editor` Spike A/B driver: the
//! verified editing core it grew (buffer, IME, wrapping, click-to-caret)
//! and its measurement instrumentation ship unchanged; the spike
//! provenance and verdicts live in PERFORMANCE_BASELINES.md and
//! TECHNICAL_SPIKES.md. Spike framing below is kept for history.
//!
//! Spike A — slice 2: is GPUI a viable *editing* surface?
//!
//! Slice 1 answered rendering/navigation; this slice adds a real editing core:
//! a multi-line buffer (`buffer.rs`), keyboard editing (typing, movement,
//! selection, undo/redo, clipboard), IME plumbing through GPUI's input handler,
//! and the same instrumentation as slice 1 so typing latency can be measured
//! against the approved thresholds (PERFORMANCE_BASELINES.md §6).
//!
//! Still NOT answered (slice 3 / Spike B): IME/selection on wrapped lines,
//! full visual-row navigation chain (limited by test infrastructure focus loss),
//! accessibility, large-file strategy. IME composition and click-to-caret ARE
//! now verified end-to-end against the live macOS input context and window
//! server, including one-undo-per-session grouping and wrapped-line caret
//! placement with visual-row navigation.
//!
//! Modes:
//!   narra-app --fixture <path>                 interactive
//!       cmd-g / cmd-shift-g  next/previous scene
//!       arrows, shift-arrows, home/end, cmd-a     movement and selection
//!       cmd-z / cmd-shift-z                       undo / redo
//!       cmd-c / cmd-x / cmd-v                     clipboard
//!       cmd-f                                     find bar (type to search)
//!       enter / cmd-g / cmd-shift-g / cmd-alt-f / cmd-enter / cmd-alt-enter
//!           next match / next+prev match / focus replacement /
//!           replace match / replace all
//!       escape                                     close find bar
//!       cmd-d                                     diff view (toggle)
//!       cmd-g / cmd-shift-g (diff open)           next/previous hunk
//!       cmd-q                                     quit (prints the JSON report)
//!   narra-app --fixture <path> --steps 60      automated scroll probe (slice 1)
//!   narra-app --fixture <path> --find-steps 60 automated find probe (slice 3)
//!   narra-app --fixture <path> --diff-steps 30 automated diff probe (M1-3b)
//!   narra-app --report-after-ms <ms>           print the report and exit
//!   narra-app --dump-after-ms <ms>             print a buffer dump and exit
//!       (used by the scripted typing verification: text, enter, backspace,
//!        selection and undo are observable without trusting a screenshot)
//!   narra-app --project <root>                 M6 slice 1: Narra project
//!       (mutually exclusive with --fixture) opens a real project through
//!       narra-core; the left panel lists its documents (hidden state like
//!       .git/.narra excluded), clicking one loads it into the buffer, ⌘S
//!       saves through the boundary-checked document IO, and switching
//!       documents with unsaved changes is refused explicitly — the status
//!       bar shows the document, a dirty marker and the last outcome.
//!   narra-app --project <root>                 M6 slice 3: agent review
//!       ⌘R reviews the scene under the caret grounded in project material
//!       (scene_review task on a worker thread); the right panel shows the
//!       run's status, the context sources read and a scrolled window of
//!       the review; ⌘. cancels; ⌥↑/⌥↓ scroll. Adapter flags (key never
//!       from argv, only from OPENAI_API_KEY):
//!         --agent-adapter mock|openai-compat|opencode|opencode-go
//!                                             (default: settings, mock offline)
//!         --agent-model <nombre>              (required for openai-compat)
//!         --agent-base-url <url>              (default: local Ollama)
//!         --agent-latency <ms>                (simulated mock latency)
//!       M6 slice 4 (proposals): ⌘K opens the instruction bar, enter runs
//!       scene_edit for the caret's scene and the panel shows the stored
//!       proposal's diff; ⌘⇧Enter applies (two-step: arm, then apply),
//!       ⌘⌫ rejects. M6 slice 5 (review focus): ⌘⇧R opens the focus bar,
//!       enter commits the focus and reviews, ⌘R reuses the last focus.
//!       M6 slice 6 (hunk selection): ⌥←/⌥→ walk the proposal's hunks,
//!       ⌥Enter toggles the one under the cursor and ⌘⇧Enter applies
//!       exactly the selected subset (empty selection is refused).
//!       M6 slice 8 (retry): ⌘⌥R re-runs the visible flow's failed or
//!       cancelled run with its own stored inputs (same scene, focus or
//!       instruction — never retyped).
//!       M6 slice 9 (i18n): `--lang en|es` (default from the environment)
//!       and ⌘⌥L localize every workspace string (i18n table with
//!       completeness/parity tests); the dump reports `lang`.
//!       M7 slice 4 (export): ⌘E writes the current buffer as a PDF to
//!       `<root>/exports/<screenplay>.pdf` through the core layout engine
//!       (worker thread; the dump reports `export`).
//!       M8 slice 2 (project search): ⌘⇧F opens the project-search bar;
//!       enter scans every project document on a worker (narra-core
//!       `search`), results replace the explorer, ⌘↑/⌘↓ move and enter (or
//!       a click) opens the match; the dump reports `search`.
//!       M8 slice 3 (outline): the explorer lists the open document's
//!       scene headings below the file list; clicking one jumps to the
//!       scene and the caret's scene is highlighted. Dump: `outline`.
//!       M9 slice 1 (mouse selection): dragging selects from the anchor,
//!       double click selects the word, triple click the line; the list
//!       auto-scrolls at the edges. Dump: `selected_text`.
//!       M9 slice 2 (word/line editing): ⌥←/→ words (⌥⇧ selects), ⌥⌫/⌥⌦
//!       delete words, ⌥↑/↓ move the line, ⇧⌥↑/↓ duplicate it, ⌘⇧K deletes
//!       it, ⌘↑/↓ document start/end, RePág/AvPág; ⌥ arrows stay with the
//!       proposal hunks while one is visible and ctrl-up/down scroll the
//!       agent panel when no search results are shown.
//!       M9 slice 3 (gutter): line numbers in a fixed gutter, the caret's
//!       line highlighted; click/caret/wrap math accounts for the gutter.
//!       M9 slice 4 (explorer tree): folders first with indentation and
//!       collapse toggles, file-type colors (fountain/markdown/yaml),
//!       dirty dot on the open document, wheel scrolling; dump: `tree`.
//!       M9 slice 5 (screenplay): element-kind highlighting (debounced full
//!       parse), Tab/⇧Tab element cycling with Fountain forced markers, and
//!       a smart Enter that inserts the blank separator after headings.
//!       M9 slice 6 (data protection): autosave to `.narra/backups/` every
//!       2 s while dirty (never canonical), ⌘⇧B restores into the buffer,
//!       ⌘⇧D discards, ⌘S removes it, and a save/discard/cancel prompt
//!       guards ⌘Q with unsaved changes; dump: `backup`, `quit_prompt`.
//!
//! Bar note (spike scope): one focus handle serves the document and the
//! single-line bars (find, propose instruction, review focus) — while a
//! bar is open, text input, backspace and enter are routed to it instead
//! of the buffer (the documented single-focus compromise; per-field
//! focus/IME is production work, not a spike question). Only one bar can
//! be open at a time. Arrow keys still move the document caret, which
//! doubles as a scroll preview while the bar is open.
//!
//! Output: JSON lines on stdout starting with `{"kind":"spike-a`.

mod agent_view;
mod assets;
mod buffer;
mod chat_view;
mod context_menu;
mod explorer;
mod git_status;
mod i18n;
mod markdown_view;
mod new_file;
mod new_project;
mod panel_resize;
mod project_settings;
mod recent;
mod screenplay;
mod scrollbar;
mod settings;
mod theme;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::OnceLock;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use agent_view::{
    AgentMessage, AgentState, AgentStatus, ProposeMessage, ProposeResult, ProposeStatus,
    ProposeView, RetryTarget, retry_target,
};
use buffer::{Buffer, Position};
use chat_view::{ChatProposalView, ChatStatus, ChatUiState, ProposalCardStatus};
use context_menu::{ContextAction, ContextMenuItem, ContextMenuState};
use explorer::{FileKind, Row};
use git_status::GitStatus;
use gpui::{
    App, Application, Bounds, ClickEvent, ClipboardItem, Context, ElementInputHandler,
    EntityInputHandler, FocusHandle, Focusable, KeyBinding, ListAlignment, ListState, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Rgba, ScrollHandle, ShapedLine,
    StyledText, TextRun, Timer, UTF16Selection, Window, WindowAppearance, WindowBounds,
    WindowOptions, actions, canvas, div, list, point, prelude::*, px, rgb, rgba, size, svg,
};
use i18n::Lang;
use narra_core::agent::{
    AgentError, ChatAttachment, ChatMode, ChatTask, ChatThread, LOCAL_DEFAULT_BASE_URL,
    MockAdapter, ModelAdapter, OpenAiCompatAdapter, OpenAiCompatConfig, ProjectScaffoldTask,
    ScaffoldPlan, SceneEditTask, SceneReviewTask, TaskOptions, fetch_model_ids, proposal, roles,
    write_plan,
};
use narra_core::cancel::CancellationToken;
use narra_core::entities::{EntityCategory, EntityIndex, Mention, split_by_mentions};
use narra_core::export;
use narra_core::fountain::ElementKind as FountainKind;
use narra_core::git::{Branch, Commit, OpOptions, Repository, WorktreeStatus};
use narra_core::language::{
    self, Diagnostic, LanguageId, LanguageReport, Severity as DiagnosticSeverity,
};
use narra_core::markdown::MarkdownDocument;
use narra_core::provider::{
    CachedSecretStore, GITHUB_TOKEN_ACCOUNT, GitHubClient, GitHubConfig, NewPullRequest,
    NewRepository, OsKeychain, SecretError, SecretStore,
};
use narra_core::{DocumentError, Project, ProjectError, ProjectType, create_project, open_project};
use new_file::Kind as NewFileKind;
use project_settings::{ProjectField, ProjectSettingsState};
use recent::RecentProjects;
use settings::{AiProvider, AppSettings, LanguageSetting};
use theme::ThemeMode;
use unicode_segmentation::UnicodeSegmentation;

actions!(
    narra_app,
    [
        // windows and navigation
        Quit,
        OpenSettings,
        OpenProjectSettings,
        ToggleConfigFiles,
        ShowContextMenu,
        FocusChat,
        ChatNewline,
        QuickOpen,
        CommandPalette,
        NextScene,
        PrevScene,
        // caret movement
        MoveLeft,
        MoveRight,
        MoveUp,
        MoveDown,
        MoveHome,
        MoveEnd,
        // selection
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectHome,
        SelectEnd,
        SelectAll,
        // editing
        Backspace,
        DeleteText,
        InsertNewline,
        Save,
        Undo,
        Redo,
        CopyText,
        CutText,
        PasteText,
        // find / replace (slice 3)
        ToggleFind,
        FocusReplace,
        CloseFind,
        ReplaceCurrent,
        ReplaceAll,
        // diff view (slice 3, M1-3b)
        ToggleDiff,
        // Markdown preview (M14a, D-033)
        TogglePreview,
        // agent review (M6 slice 3)
        ReviewScene,
        CancelReview,
        ReviewScrollUp,
        ReviewScrollDown,
        // proposals (M6 slice 4)
        OpenPropose,
        ApplyProposal,
        RejectProposal,
        // review focus (M6 slice 5)
        OpenFocusBar,
        // hunk selection (M6 slice 6)
        HunkPrev,
        HunkNext,
        ToggleHunk,
        // retry (M6 slice 8)
        RetryAgent,
        // language (M6 slice 9)
        ToggleLang,
        // export (M7 slice 4)
        ExportPdf,
        // project search (M8 slice 2)
        ProjectSearch,
        SearchPrev,
        SearchNext,
        // word and line editing (M9 slice 2)
        WordLeft,
        WordRight,
        SelectWordLeft,
        SelectWordRight,
        DeleteWordLeft,
        DeleteWordRight,
        MoveLineUp,
        MoveLineDown,
        DuplicateLineUp,
        DuplicateLineDown,
        DeleteLine,
        DocStart,
        DocEnd,
        SelectDocStart,
        SelectDocEnd,
        PageUp,
        PageDown,
        SelectPageUp,
        SelectPageDown,
        // screenplay elements (M9 slice 5)
        CycleElement,
        CycleElementBack,
        // crash-safety backups (M9 slice 6)
        RestoreBackup,
        DiscardBackup,
        // new documents (M10 slice 2)
        NewFile,
        // projects (M10 slice 2b)
        OpenProject,
        NewProject,
        ClearRecentProjects,
        // tabs (M10 slice 3b)
        CloseTab,
        NextTab,
        PrevTab,
        // panels and bottom bar (M10 slice 3c)
        ToggleExplorer,
        ToggleAgentPanel,
        ToggleGitPanel,
    ]
);

/// A recent-project menu item carrying its root. Menus construct it directly
/// (`no_json`), so it never comes from a keymap and needs no serde surface.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = narra_app, no_json, no_register)]
struct OpenRecentProject {
    path: PathBuf,
}

/// Baseline line height. Wrapping (slice 3) makes non-hot rows variable-height
/// (`list` measures each); this constant still sizes the caret, the hot-row
/// single-line fallback, and the list overdraw.
/// Lines moved by PageUp/PageDown (M9 slice 2): roughly one screenful at
/// the default editor size.
const PAGE_LINES: i64 = 30;

/// Explicit so that cursor x-positions (shaped manually) match rendered rows.
/// 14 px == `text_sm` (0.875 rem), which slice 1 used.
const FONT_FAMILY: &str = "Menlo";
/// `px_2` on every row.
const ROW_PADDING_X: f32 = 8.0;

/// Height of the status strip above the buffer list; the only chrome between
/// the window's content top and the list (the find bar sits below it when
/// open). Mouse hit-testing subtracts nothing — item bounds are already in
/// window coordinates — but clicks on this strip must be ignored.
const STATUS_BAR_H: f32 = 26.0;

/// Gaps longer than this are treated as idle time (user reading) and excluded
/// from frame statistics, so interactive sessions do not pollute the samples.
const IDLE_GAP_MS: f64 = 500.0;

/// Find-match highlight (all matches) and the current match — slice 3.
/// Find bar background (distinct from the status bar) and text colors.
/// Explorer state colors (M10 S3c): type is an icon, color is state.
/// Line-number gutter (M9 slice 3): fixed column before the text.
/// Outline rows rendered in the explorer (M8 slice 3). The panel has no
/// scrollbar yet; the remainder is reported as "… N more".
const OUTLINE_VISIBLE_ROWS: usize = 120;
/// Review lines visible in the panel at once; `⌥↑/⌥↓` scrolls by 3 lines.
/// Approximate chrome-free budget (the panel also shows header, sources and
/// hints); the dump always carries the full text, so this is display-only.
const AGENT_VISIBLE_LINES: usize = 20;
/// Hard cap on one review task (`scene_review`): model latency included.
const AGENT_TIMEOUT: Duration = Duration::from_secs(120);
/// Error/cancelled text in the agent panel.
/// Diff view colors (slice 3, M1-3b): added/removed row backgrounds and the
/// line-label gutter text.
/// Which find field owns keystrokes while the bar is open (single-focus
/// spike compromise — see the module header).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum FindField {
    #[default]
    Query,
    Replacement,
}

/// Find/replace state (slice 3). `matches` are document byte ranges from
/// `Buffer::find_all`, recomputed on every query change (the measured
/// operation) and after edits that touch the document.
#[derive(Debug, Default)]
struct FindState {
    open: bool,
    field: FindField,
    query: String,
    replacement: String,
    matches: Vec<Range<usize>>,
    /// Index of the highlighted current match in `matches` (0 when empty).
    current: usize,
    /// Replace-all counter for the dump/report (verified evidence).
    replaced_total: usize,
}

/// One row of the unified diff view: the label and the line text.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DiffRow {
    /// Unified-diff label: " ", "-", "+".
    label: char,
    text: String,
}

/// Instruction bar for proposal requests (M6 slice 4). Same single-focus
/// spike compromise as the find bar: while open, keystrokes route here —
/// enter submits the instruction, escape closes the bar.
#[derive(Debug, Default)]
struct ProposeBar {
    open: bool,
    instruction: String,
}

/// Reviewer-focus bar (M6 slice 5). Same single-focus compromise: while
/// open, keystrokes route here — enter commits the focus and runs the
/// review, escape closes the bar without running anything. The committed
/// focus lives in `AgentState::focus` (input for the run), so ⌘R with the
/// bar closed reuses the last focus.
#[derive(Debug, Default)]
struct FocusBar {
    open: bool,
    focus: String,
}

/// Outcome of the last PDF export (M7 slice 4), shown in the dump so
/// verification never depends on pixels.
#[derive(Debug, Default)]
struct ExportState {
    /// Written path, empty until a successful run.
    out: String,
    bytes: usize,
    pages: usize,
    /// Explicit failure text (empty on success).
    error: String,
}

/// Message from the export worker thread into the view. The generation
/// discards an export that finished after the user switched projects.
enum ExportMessage {
    Done {
        generation: u64,
        result: Result<ExportOutcome, String>,
    },
}

struct ExportOutcome {
    out: String,
    bytes: usize,
    pages: usize,
}

/// Project-wide search state (M8 slice 2): the bar, the last report and the
/// selected result. The report comes from `narra_core::search` on a worker.
#[derive(Debug, Default)]
struct SearchState {
    open: bool,
    query: String,
    /// The query the current hits belong to (Enter opens instead of
    /// re-searching while the query is unchanged).
    searched: String,
    hits: Vec<narra_core::search::SearchHit>,
    selected: usize,
    truncated: bool,
    scanned: usize,
    skipped: usize,
}

/// The New File modal (M10 slice 2): kind, name and the last error.
#[derive(Debug)]
struct NewFileState {
    kind: NewFileKind,
    name: String,
    error: String,
}

/// How a new project starts (M16 slices 8–9, D-044).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NewProjectMode {
    Template,
    Ai,
}

/// Which dialog field owns typing. The title is the default; the AI prompt
/// takes over when the AI mode is selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NewProjectField {
    Title,
    Prompt,
}

/// The New Project modal (M10 slice 2b): title, kind, primary language, the
/// destination folder and the native save-panel phase. Choosing a folder and
/// accepting are separate actions: the folder is only stored here, and
/// `accept_new_project` creates the template or starts the AI plan. `picking`
/// is true while the OS dialog owns the app, so duplicate panels cannot open.
/// The AI mode adds a prompt, a busy phase and a reviewable file plan.
#[derive(Debug)]
struct NewProjectState {
    title: String,
    kind: ProjectType,
    lang: Lang,
    mode: NewProjectMode,
    prompt: String,
    field: NewProjectField,
    error: String,
    picking: bool,
    /// AI generation in progress (the save panel has already returned).
    busy: bool,
    /// Destination folder chosen with the folder button. Nothing exists
    /// there until the user accepts.
    path: Option<PathBuf>,
    /// Reviewable AI plan; nothing is written until the user confirms.
    plan: Option<ScaffoldPlan>,
}

/// Snapshot of the dialog inputs handed to the AI scaffold worker: what the
/// user saw when pressing Accept decides the plan, even if the modal is still
/// on screen while the model runs.
#[derive(Debug, Clone)]
struct NewProjectDraft {
    title: String,
    kind: ProjectType,
    lang: Lang,
    prompt: String,
}

/// Field the settings modal is editing. Tab/click moves it; typing and paste
/// land in the active field (the API key is masked and never dumped).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsField {
    BaseUrl,
    Model,
    ApiKey,
    GitName,
    GitEmail,
    GitHubToken,
}

/// Sidebar section of the Preferences dialog (M10 S4c).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsSection {
    General,
    Editor,
    Ai,
    Git,
    Agents,
}

impl SettingsSection {
    const ALL: [SettingsSection; 5] = [
        SettingsSection::General,
        SettingsSection::Editor,
        SettingsSection::Ai,
        SettingsSection::Git,
        SettingsSection::Agents,
    ];

    fn i18n_key(self) -> &'static str {
        match self {
            SettingsSection::General => "settings.section.general",
            SettingsSection::Editor => "settings.section.editor",
            SettingsSection::Ai => "settings.section.ai",
            SettingsSection::Git => "settings.section.git",
            SettingsSection::Agents => "settings.section.agents",
        }
    }
}

/// The settings modal (M10 S4). `key_input` is transient; only Save writes it
/// to the OS keychain. Everything else is persisted non-secret settings.
struct SettingsState {
    section: SettingsSection,
    provider: AiProvider,
    base_url: String,
    model: String,
    key_input: String,
    key_present: bool,
    key_source: String,
    /// Whether the keychain answer is known in this session. Settings opens
    /// without reading the keychain (D-040).
    key_checked: bool,
    field: SettingsField,
    error: String,
    status: String,
    models: Vec<String>,
    models_request: u64,
    fetching_models: bool,
    language: LanguageSetting,
    theme: ThemeMode,
    font_size: u16,
    word_wrap: bool,
    line_numbers: bool,
    show_config_files: bool,
    git_name: String,
    git_email: String,
    github_token_input: String,
    github_token_present: bool,
    github_token_source: String,
    github_token_checked: bool,
    /// Request ids of the explicit "check" buttons; a stale worker answer
    /// must not overwrite a newer check.
    key_request: u64,
    github_request: u64,
    /// Effective roles loaded for the Agentes tab (built-ins + packages +
    /// project config). Read-only here; installation is file-based.
    agents: Vec<roles::Role>,
    agents_error: String,
}

impl std::fmt::Debug for SettingsState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SettingsState")
            .field("section", &self.section)
            .field("provider", &self.provider)
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field(
                "key_input",
                &if self.key_input.is_empty() {
                    "<empty>"
                } else {
                    "<redacted>"
                },
            )
            .field("key_present", &self.key_present)
            .field("key_source", &self.key_source)
            .field("key_checked", &self.key_checked)
            .field("field", &self.field)
            .field("error", &self.error)
            .field("status", &self.status)
            .field("models", &self.models.len())
            .field("fetching_models", &self.fetching_models)
            .field("language", &self.language)
            .field("theme", &self.theme)
            .field("font_size", &self.font_size)
            .field("word_wrap", &self.word_wrap)
            .field("line_numbers", &self.line_numbers)
            .field("show_config_files", &self.show_config_files)
            .field("git_name", &self.git_name)
            .field("git_email", &self.git_email)
            .field(
                "github_token_input",
                &if self.github_token_input.is_empty() {
                    "<empty>"
                } else {
                    "<redacted>"
                },
            )
            .field("github_token_present", &self.github_token_present)
            .field("github_token_source", &self.github_token_source)
            .field("github_token_checked", &self.github_token_checked)
            .finish()
    }
}

/// Result of one AI project-scaffold run (M16 slice 9). The generation token
/// prevents a stale run from landing in a reopened dialog.
enum NewProjectMessage {
    Plan {
        generation: u64,
        result: Result<ScaffoldPlan, String>,
    },
}

enum SettingsMessage {
    Models {
        request: u64,
        result: Result<Vec<String>, String>,
    },
    /// The worker resolved no key for a provider that requires one.
    MissingKey { request: u64 },
    /// Answer to the explicit "check" action for the provider key.
    KeyStatus {
        request: u64,
        provider: AiProvider,
        present: bool,
        source: String,
    },
    /// Answer to the explicit "check" action for the GitHub token.
    GitHubStatus {
        request: u64,
        present: bool,
        source: String,
    },
}

/// Quick-open / command-palette mode (M10 S5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaletteMode {
    Files,
    Commands,
}

impl PaletteMode {
    fn key(self) -> &'static str {
        match self {
            PaletteMode::Files => "files",
            PaletteMode::Commands => "commands",
        }
    }
}

/// The single overlay backing ⌘P (files) and ⌘⇧P (commands).
#[derive(Debug)]
struct PaletteState {
    mode: PaletteMode,
    query: String,
    selected: usize,
}

/// Commands exposed by the palette. Menus and keybindings remain the primary
/// surfaces; the palette is a discoverable index over the same handlers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaletteCommand {
    OpenSettings,
    OpenProjectSettings,
    NewFile,
    NewProject,
    OpenProject,
    Save,
    ExportPdf,
    ToggleExplorer,
    ToggleAgentPanel,
    ToggleGitPanel,
    ToggleConfigFiles,
    Find,
    ProjectSearch,
    ToggleDiff,
    TogglePreview,
    ReviewScene,
    Propose,
    CloseTab,
    NextTab,
    PrevTab,
}

impl PaletteCommand {
    const ALL: [PaletteCommand; 20] = [
        PaletteCommand::OpenSettings,
        PaletteCommand::OpenProjectSettings,
        PaletteCommand::NewFile,
        PaletteCommand::NewProject,
        PaletteCommand::OpenProject,
        PaletteCommand::Save,
        PaletteCommand::ExportPdf,
        PaletteCommand::ToggleExplorer,
        PaletteCommand::ToggleAgentPanel,
        PaletteCommand::ToggleGitPanel,
        PaletteCommand::ToggleConfigFiles,
        PaletteCommand::Find,
        PaletteCommand::ProjectSearch,
        PaletteCommand::ToggleDiff,
        PaletteCommand::TogglePreview,
        PaletteCommand::ReviewScene,
        PaletteCommand::Propose,
        PaletteCommand::CloseTab,
        PaletteCommand::NextTab,
        PaletteCommand::PrevTab,
    ];

    fn label_key(self) -> &'static str {
        match self {
            PaletteCommand::OpenSettings => "menu.settings",
            PaletteCommand::OpenProjectSettings => "menu.project_settings",
            PaletteCommand::NewFile => "menu.new_file",
            PaletteCommand::NewProject => "menu.new_project",
            PaletteCommand::OpenProject => "menu.open_project",
            PaletteCommand::Save => "menu.save",
            PaletteCommand::ExportPdf => "menu.export_pdf",
            PaletteCommand::ToggleExplorer => "menu.toggle_explorer",
            PaletteCommand::ToggleAgentPanel => "menu.toggle_agent",
            PaletteCommand::ToggleGitPanel => "menu.toggle_git",
            PaletteCommand::ToggleConfigFiles => "menu.show_config_files",
            PaletteCommand::Find => "menu.find",
            PaletteCommand::ProjectSearch => "menu.project_search",
            PaletteCommand::ToggleDiff => "menu.toggle_diff",
            PaletteCommand::TogglePreview => "menu.toggle_preview",
            PaletteCommand::ReviewScene => "menu.review",
            PaletteCommand::Propose => "menu.propose",
            PaletteCommand::CloseTab => "menu.close_tab",
            PaletteCommand::NextTab => "menu.next_tab",
            PaletteCommand::PrevTab => "menu.prev_tab",
        }
    }

    fn shortcut(self) -> &'static str {
        match self {
            PaletteCommand::OpenSettings => "⌘,",
            PaletteCommand::OpenProjectSettings => "⇧⌘,",
            PaletteCommand::NewFile => "⌘N",
            PaletteCommand::NewProject => "⇧⌘N",
            PaletteCommand::OpenProject => "⌘O",
            PaletteCommand::Save => "⌘S",
            PaletteCommand::ExportPdf => "⌘E",
            PaletteCommand::ToggleExplorer => "⌘B",
            PaletteCommand::ToggleAgentPanel => "⇧⌘A",
            PaletteCommand::ToggleGitPanel => "⌥⌘G",
            PaletteCommand::ToggleConfigFiles => "",
            PaletteCommand::Find => "⌘F",
            PaletteCommand::ProjectSearch => "⇧⌘F",
            PaletteCommand::ToggleDiff => "⌘D",
            PaletteCommand::TogglePreview => "⌥⌘P",
            PaletteCommand::ReviewScene => "⌘R",
            PaletteCommand::Propose => "⌘K",
            PaletteCommand::CloseTab => "⌘W",
            PaletteCommand::NextTab => "⇧⌘]",
            PaletteCommand::PrevTab => "⇧⌘[",
        }
    }
}

/// What the open menu acts on. Editor menus do not need a path; explorer and
/// tab menus resolve their target when an action is chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ContextTarget {
    Editor,
    File(String),
    Dir(String),
    Tab(usize),
}

/// Rename dialog state (M11 slice 3). The extension is preserved by the
/// caller; the user edits only the stem.
#[derive(Debug, Clone)]
struct FilePromptState {
    path: String,
    name: String,
    error: String,
}

/// Destructive confirmation for deleting a document.
#[derive(Debug, Clone)]
struct DeletePromptState {
    path: String,
    name: String,
    error: String,
}

/// Which surface the right-hand panel shows (M12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RightTab {
    Chat,
    Activity,
}

/// Messages from the chat worker.
enum ChatRunMessage {
    Event(narra_core::agent::TaskEvent),
    Done(Result<narra_core::agent::TaskOutput, AgentError>),
}

/// What to do once the unsaved-changes prompt is resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingUnsaved {
    Quit,
    CloseTab(usize),
}

/// The generalized save/discard/cancel prompt (M9 slice 6, generalized for
/// tabs in M10 S3b). `dirty` is how many documents the operation would lose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct UnsavedPrompt {
    action: PendingUnsaved,
    dirty: usize,
}

/// Local Git refresh result (M10 S3c), generation-checked like search/export.
enum GitMessage {
    Done {
        request: u64,
        result: Result<GitStatus, String>,
    },
    History {
        result: Result<Vec<Commit>, String>,
    },
    Branches {
        result: Result<Vec<Branch>, String>,
    },
    Diff {
        path: String,
        result: Result<String, String>,
    },
    Restore {
        path: String,
        result: Result<(), String>,
    },
    BranchSwitch {
        result: Result<(), String>,
    },
    BranchCreate {
        result: Result<(), String>,
    },
    Remotes {
        result: Result<Vec<(String, String)>, String>,
    },
    Publish {
        result: Result<String, String>,
    },
    Push {
        result: Result<(), String>,
    },
    Pull {
        result: Result<(), String>,
    },
    PullRequest {
        result: Result<String, String>,
    },
    Commit {
        result: Result<(), String>,
    },
    Init {
        result: Result<(), String>,
    },
}

/// “Guardar versión” dialog state (M13).
#[derive(Debug, Clone)]
struct GitCommitState {
    message: String,
    error: String,
    busy: bool,
    selected: std::collections::HashSet<String>,
}

#[derive(Debug, Clone)]
struct GitDiffState {
    path: String,
    diff: String,
    loading: bool,
    error: String,
}

#[derive(Debug, Clone)]
struct GitRestoreState {
    path: String,
    error: String,
}

#[derive(Debug, Clone)]
struct GitBranchPromptState {
    name: String,
    error: String,
    busy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GitSyncKind {
    Push,
    Pull,
}

struct GitSyncState {
    kind: GitSyncKind,
    error: String,
    busy: bool,
}

struct GitHubPublishState {
    name: String,
    private: bool,
    error: String,
    busy: bool,
}

struct GitHubPullRequestState {
    title: String,
    body: String,
    base: String,
    field: u8,
    error: String,
    busy: bool,
}

/// Small tooltip view used across the human Git surface.
struct GitTooltip {
    command: String,
    description: String,
}

impl gpui::Render for GitTooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .max_w(px(280.0))
            .p_2()
            .bg(rgb(theme::explorer_bg()))
            .border_1()
            .border_color(rgb(theme::explorer_border()))
            .flex()
            .flex_col()
            .gap_1()
            .text_sm()
            .child(
                div()
                    .text_color(rgb(theme::cue_fg()))
                    .child(format!("↳ {}", self.command)),
            )
            .child(
                div()
                    .text_color(rgb(theme::status_fg()))
                    .child(self.description.clone()),
            )
    }
}

/// Message from the project-search worker thread. The generation identifies
/// the workspace the query started in: a result that lands after a project
/// switch is discarded instead of populating the new workspace.
enum SearchMessage {
    Done {
        generation: u64,
        query: String,
        result: Result<narra_core::search::SearchReport, String>,
    },
}

/// Diff view state (slice 3, M1-3b). The base is a snapshot of the file as
/// loaded, so the diff stays meaningful while the buffer keeps changing —
/// the same model the M4 proposal surface needs (canonical file vs. edited
/// buffer). Rows are recomputed on open and on the explicit refresh.
#[derive(Debug, Default)]
struct DiffState {
    open: bool,
    /// Snapshot of the loaded file, one entry per line (no newline).
    base_lines: Vec<String>,
    rows: Vec<DiffRow>,
    /// Highlighted cursor row in `rows` (0 when empty).
    cursor: usize,
    /// Cumulative counters for the dump/report.
    added_total: usize,
    removed_total: usize,
}

fn is_scene_heading(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with("INT. ")
        || line.starts_with("EXT. ")
        || line.starts_with("I/E. ")
        || line.starts_with("EXT./INT. ")
}

/// 1-based scene under a caret line (M6 slice 3): the scene whose heading is
/// the last one at or above the caret. `None` when the caret sits before the
/// first heading — outside every scene, which the UI reports explicitly
/// instead of guessing.
fn scene_number_at(scene_lines: &[usize], caret_line: usize) -> Option<usize> {
    scene_lines
        .iter()
        .rposition(|line| *line <= caret_line)
        .map(|ix| ix + 1)
}

/// First 8 characters of a proposal id, for display (character-safe; never
/// panics on short ids).
/// Synthetic unified diff for a file that has no version in `HEAD` yet.
/// Additions only, matching the visual language of `git diff`.
fn added_file_diff(path: &str, text: &str) -> String {
    let mut diff = format!("--- /dev/null\n+++ b/{path}\n");
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        diff.push_str("@@ -0,0 +0,0 @@\n");
        return diff;
    }
    diff.push_str(&format!("@@ -0,0 +1,{} @@\n", lines.len()));
    for line in lines {
        diff.push('+');
        diff.push_str(line);
        diff.push('\n');
    }
    diff
}

fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

/// Localizable label for a role's origin; the dump keeps the raw key
/// (`"builtin"`/`"project"`) so machine checks never depend on wording.
/// Content hash for the chat Markdown cache (M16 slice 5). Stored assistant
/// turns are immutable, so the content is a stable key.
fn markdown_key(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Native window title for a project (M16 slice 7). The project name is the
/// only stable, user-facing label; it follows the manifest when it changes.
fn project_window_title(title: Option<&str>) -> String {
    match title.map(str::trim).filter(|title| !title.is_empty()) {
        Some(title) => format!("{title} — Narra"),
        None => "Narra".to_string(),
    }
}

fn role_source_label(lang: Lang, source: &str) -> &'static str {
    match source {
        "project" => i18n::tr(lang, "role.project"),
        "user_package" => i18n::tr(lang, "role.user_package"),
        "project_package" => i18n::tr(lang, "role.project_package"),
        _ => i18n::tr(lang, "role.builtin"),
    }
}

/// Diff rows for the proposal panel: one header per hunk, then removed and
/// added lines (labels '-', '+', '#'). Pre-rendered once from the persisted
/// record — the panel shows exactly what apply would write.
fn diff_rows_from_hunks(hunks: &[proposal::Hunk]) -> Vec<(char, String)> {
    let mut rows = Vec::new();
    for (ix, hunk) in hunks.iter().enumerate() {
        rows.push((
            '#',
            format!(
                "@@ hunk {} · L{} · -{} +{} @@",
                ix + 1,
                hunk.old_start + 1,
                hunk.removed.len(),
                hunk.added.len()
            ),
        ));
        rows.extend(hunk.removed.iter().map(|line| ('-', line.clone())));
        rows.extend(hunk.added.iter().map(|line| ('+', line.clone())));
    }
    rows
}

fn is_character_cue(line: &str) -> bool {
    let line = line.trim_end();
    !line.is_empty()
        && line.len() <= 40
        && line.chars().any(|c| c.is_alphabetic())
        && line.chars().all(|c| !c.is_alphabetic() || c.is_uppercase())
        && !line.ends_with(':')
}

fn line_color(line: &str) -> Rgba {
    if is_scene_heading(line) {
        rgb(theme::scene_fg())
    } else if line.starts_with('#') {
        rgb(theme::section_fg())
    } else if line.starts_with("[[") {
        rgb(theme::note_fg())
    } else if line.ends_with("TO:") || line.starts_with("FUNDIDO") {
        rgb(theme::transition_fg())
    } else if is_character_cue(line) {
        rgb(theme::cue_fg())
    } else {
        rgb(theme::default_fg())
    }
}

/// The macOS input context can deliver `\r` (or `\r\n`) for a return key.
/// The buffer stores `\n` exclusively.
fn normalize_newlines(text: &str) -> String {
    if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text.to_string()
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// Shape one line with the row's font so caret/selection positions can be
/// computed in pixel space. Runs only for rows that carry caret/selection/IME
/// state, plus hit-testing (mouse/IME), not for every visible row.
fn shape_line(window: &Window, text: &str) -> ShapedLine {
    let run = TextRun {
        len: text.len(),
        font: gpui::font(FONT_FAMILY),
        color: rgb(theme::default_fg()).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window.text_system().shape_line(
        text.to_string().into(),
        px(theme::text_size()),
        &[run],
        None,
    )
}

/// Background tint for an entity category (D-029). Foreground syntax colors
/// stay untouched, so a character cue still reads as a cue while “Marta” gets
/// a subtle mention tint.
fn mention_bg(category: EntityCategory) -> u32 {
    match category {
        EntityCategory::Character => theme::mention_character_bg(),
        EntityCategory::Location => theme::mention_location_bg(),
        EntityCategory::Object => theme::mention_object_bg(),
    }
}

/// Build styled runs for a cold (non-caret) line with entity mentions.
/// Wrapping stays with `StyledText`, so mention backgrounds do not change the
/// line's geometry.
fn mention_runs(
    text: &str,
    mentions: &[Mention],
    entities: &EntityIndex,
    color: Rgba,
) -> Vec<TextRun> {
    split_by_mentions(text, mentions)
        .into_iter()
        .map(|(range, entity)| TextRun {
            len: range.len(),
            font: gpui::font(FONT_FAMILY),
            color: color.into(),
            background_color: entity
                .map(|index| rgba(mention_bg(entities.entities()[index].category)).into()),
            underline: None,
            strikethrough: None,
        })
        .collect()
}

fn x_for_index(window: &Window, text: &str, col: usize) -> Pixels {
    shape_line(window, text).x_for_index(col)
}

struct Metrics {
    started: Instant,
    fixture: String,
    fixture_read_ms: f64,
    lines: usize,
    scenes: usize,
    first_render: Option<Instant>,
    last_render: Option<Instant>,
    pending_keystroke: Option<Instant>,
    /// A find query was just submitted; the next render closes the
    /// "query submitted → results rendered" sample (slice 3 baseline).
    pending_search: Option<Instant>,
    frame_gaps_ms: Vec<f64>,
    keystroke_to_frame_ms: Vec<f64>,
    /// Time spent inside `Buffer::find_all` per submission — the cost of
    /// the search itself, separate from the frame that renders it.
    search_compute_ms: Vec<f64>,
    /// Query submitted → next render pass (same method as
    /// keystroke_to_frame: input → start of render, no vsync term).
    search_to_frame_ms: Vec<f64>,
    /// Wall time of each replace-all over the whole document.
    replace_all_ms: Vec<f64>,
    /// A diff open/refresh was just requested; the next render closes the
    /// "diff requested → diff rendered" sample (slice 3, M1-3b baseline).
    pending_diff: Option<Instant>,
    /// Time spent computing the unified diff (base → current buffer).
    diff_compute_ms: Vec<f64>,
    /// Diff open/refresh requested → next render pass (same method as
    /// keystroke_to_frame: input → start of render, no vsync term).
    diff_render_ms: Vec<f64>,
    renders: usize,
    edits: usize,
    searches: usize,
    diff_opens: usize,
    probe_steps: usize,
    find_probe_steps: usize,
    diff_probe_steps: usize,
}

/// The three probe step counts. Grouped so `Metrics::new` stays under the
/// clippy argument limit instead of carrying an `#[allow]`.
struct ProbeSteps {
    scroll: usize,
    find: usize,
    diff: usize,
}

impl Metrics {
    fn new(
        started: Instant,
        fixture: &str,
        fixture_read_ms: f64,
        lines: usize,
        scenes: usize,
        probe: ProbeSteps,
    ) -> Self {
        Metrics {
            started,
            fixture: fixture.to_string(),
            fixture_read_ms,
            lines,
            scenes,
            first_render: None,
            last_render: None,
            pending_keystroke: None,
            pending_search: None,
            pending_diff: None,
            frame_gaps_ms: Vec::new(),
            keystroke_to_frame_ms: Vec::new(),
            search_compute_ms: Vec::new(),
            search_to_frame_ms: Vec::new(),
            replace_all_ms: Vec::new(),
            diff_compute_ms: Vec::new(),
            diff_render_ms: Vec::new(),
            renders: 0,
            edits: 0,
            searches: 0,
            diff_opens: 0,
            probe_steps: probe.scroll,
            find_probe_steps: probe.find,
            diff_probe_steps: probe.diff,
        }
    }

    fn on_render(&mut self, now: Instant) {
        self.renders += 1;
        if self.first_render.is_none() {
            self.first_render = Some(now);
        }
        if let Some(pending) = self.pending_keystroke.take() {
            self.keystroke_to_frame_ms.push(duration_ms(now - pending));
        }
        if let Some(pending) = self.pending_search.take() {
            self.search_to_frame_ms.push(duration_ms(now - pending));
        }
        if let Some(pending) = self.pending_diff.take() {
            self.diff_render_ms.push(duration_ms(now - pending));
        }
        if let Some(last) = self.last_render {
            let gap = duration_ms(now - last);
            if gap < IDLE_GAP_MS {
                self.frame_gaps_ms.push(gap);
            }
        }
        self.last_render = Some(now);
    }

    fn mark_keystroke(&mut self, now: Instant) {
        if self.pending_keystroke.is_none() {
            self.pending_keystroke = Some(now);
        }
    }

    /// A query was submitted; the next render closes the sample. The latest
    /// submission wins if several arrive before a frame (interactive typing
    /// bursts); the automated probe spaces steps by frames, so every step
    /// yields exactly one sample.
    fn mark_search(&mut self, now: Instant) {
        self.pending_search = Some(now);
        self.searches += 1;
    }

    /// A diff open/refresh was just requested; the next render closes the
    /// "requested → rendered" sample (same contract as `mark_search`).
    fn mark_diff(&mut self, now: Instant) {
        self.pending_diff = Some(now);
        self.diff_opens += 1;
    }

    fn cold_start_ms(&self) -> f64 {
        self.first_render
            .map(|first| duration_ms(first - self.started))
            .unwrap_or(0.0)
    }

    fn stats(samples: &[f64]) -> (usize, f64, f64, f64) {
        if samples.is_empty() {
            return (0, 0.0, 0.0, 0.0);
        }
        let mut sorted = samples.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        (
            sorted.len(),
            percentile(&sorted, 0.5),
            percentile(&sorted, 0.95),
            sorted[sorted.len() - 1],
        )
    }

    /// Steady state: the first frames pay one-time costs (font loading, first
    /// layout, GPU warm-up). Spike reports quote both views.
    fn steady(&self) -> &[f64] {
        const WARMUP_SAMPLES: usize = 10;
        if self.frame_gaps_ms.len() > WARMUP_SAMPLES {
            &self.frame_gaps_ms[WARMUP_SAMPLES..]
        } else {
            &[]
        }
    }

    /// Resident memory in MB, sampled through `ps` (spike-only convenience,
    /// no dependency added for a metric we could add properly later).
    fn rss_mb() -> f64 {
        let pid = std::process::id().to_string();
        std::process::Command::new("ps")
            .args(["-p", &pid, "-o", "rss="])
            .output()
            .ok()
            .and_then(|out| {
                String::from_utf8_lossy(&out.stdout)
                    .trim()
                    .parse::<f64>()
                    .ok()
            })
            .map(|kb| kb / 1024.0)
            .unwrap_or(0.0)
    }

    fn report(&self) -> String {
        let (frame_count, frame_p50, frame_p95, frame_max) = Self::stats(&self.frame_gaps_ms);
        let (steady_count, steady_p50, steady_p95, steady_max) = Self::stats(self.steady());
        let (key_count, key_p50, key_p95, key_max) = Self::stats(&self.keystroke_to_frame_ms);
        let (search_count, search_p50, search_p95, search_max) =
            Self::stats(&self.search_to_frame_ms);
        let (compute_count, compute_p50, compute_p95, compute_max) =
            Self::stats(&self.search_compute_ms);
        let (repl_count, repl_p50, repl_p95, repl_max) = Self::stats(&self.replace_all_ms);
        let (dcompute_count, dcompute_p50, dcompute_p95, dcompute_max) =
            Self::stats(&self.diff_compute_ms);
        let (drender_count, drender_p50, drender_p95, drender_max) =
            Self::stats(&self.diff_render_ms);
        format!(
            concat!(
                "{{\"kind\":\"spike-a-slice2\",",
                "\"fixture\":\"{}\",\"lines\":{},\"scenes\":{},\"probe_steps\":{},",
                "\"find_probe_steps\":{},\"searches\":{},",
                "\"diff_probe_steps\":{},\"diff_opens\":{},",
                "\"fixture_read_ms\":{:.3},\"cold_start_ms\":{:.3},\"rss_mb\":{:.1},",
                "\"renders\":{},\"edits\":{},",
                "\"frame_ms\":{{\"count\":{},\"p50\":{:.3},\"p95\":{:.3},\"max\":{:.3}}},",
                "\"steady_frame_ms\":{{\"count\":{},\"p50\":{:.3},\"p95\":{:.3},\"max\":{:.3}}},",
                "\"keystroke_to_frame_ms\":{{\"count\":{},\"p50\":{:.3},\"p95\":{:.3},\"max\":{:.3}}},",
                "\"search_to_frame_ms\":{{\"count\":{},\"p50\":{:.3},\"p95\":{:.3},\"max\":{:.3}}},",
                "\"search_compute_ms\":{{\"count\":{},\"p50\":{:.3},\"p95\":{:.3},\"max\":{:.3}}},",
                "\"replace_all_ms\":{{\"count\":{},\"p50\":{:.3},\"p95\":{:.3},\"max\":{:.3}}},",
                "\"diff_compute_ms\":{{\"count\":{},\"p50\":{:.3},\"p95\":{:.3},\"max\":{:.3}}},",
                "\"diff_render_ms\":{{\"count\":{},\"p50\":{:.3},\"p95\":{:.3},\"max\":{:.3}}}",
                "}}"
            ),
            self.fixture,
            self.lines,
            self.scenes,
            self.probe_steps,
            self.find_probe_steps,
            self.searches,
            self.diff_probe_steps,
            self.diff_opens,
            self.fixture_read_ms,
            self.cold_start_ms(),
            Self::rss_mb(),
            self.renders,
            self.edits,
            frame_count,
            frame_p50,
            frame_p95,
            frame_max,
            steady_count,
            steady_p50,
            steady_p95,
            steady_max,
            key_count,
            key_p50,
            key_p95,
            key_max,
            search_count,
            search_p50,
            search_p95,
            search_max,
            compute_count,
            compute_p50,
            compute_p95,
            compute_max,
            repl_count,
            repl_p50,
            repl_p95,
            repl_max,
            dcompute_count,
            dcompute_p50,
            dcompute_p95,
            dcompute_max,
            drender_count,
            drender_p50,
            drender_p95,
            drender_max,
        )
    }
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// One reversible edit. `range` is where `new_text` currently sits in the
/// document; undo replaces it back with `old_text`. `composition` marks the
/// entry that was opened by an IME composition session, so later marked
/// updates merge into it instead of stacking one undo step per keystroke.
#[derive(Clone)]
struct Edit {
    range: Range<usize>,
    new_text: String,
    old_text: String,
    composition: bool,
}

/// State owned by one open document (M10 S3a).
///
/// Everything here travels with a document when tabs arrive: content,
/// selection, undo history, per-document scroll and derived caches, its
/// recovery backup and its dirty revision. Project-level state (search,
/// agent, explorer, notices, language) stays in [`Editor`].
struct DocumentState {
    buffer: Buffer,
    /// Selection as a sorted doc-offset range; empty == caret only.
    selected: Range<usize>,
    /// When true the caret sits at `selected.start` instead of `selected.end`.
    reversed: bool,
    /// IME composition range (doc offsets).
    marked: Option<Range<usize>>,
    /// True from the first `setMarkedText` of a composition session until
    /// its commit (`insertText`) or cancellation (`unmarkText`): edits made
    /// inside this window merge into one undo entry (Spike B undo grouping).
    composing: bool,
    /// Drag-selection anchor (M9 slice 1): the offset where the current
    /// mouse drag started, kept fixed while the pointer moves.
    dragging: Option<usize>,
    /// Debug for mouse verification (Spike B click-to-caret): last left-click
    /// position in window coordinates, the resolved line index and its bounds,
    /// and the doc offset (or None if the click was ignored / above the list).
    /// (click_x, click_y, line, item_top, item_h, offset)
    #[allow(clippy::type_complexity)]
    last_click: Option<(f32, f32, Option<usize>, Option<f32>, f32, Option<usize>)>,
    undo_stack: Vec<Edit>,
    redo_stack: Vec<Edit>,

    scene_lines: Vec<usize>,
    /// Document outline (M8 slice 3; M14b for Markdown/YAML): (line index,
    /// heading text) for the open document.
    outline: Vec<(usize, String)>,
    /// Outline lines aligned with `outline`, used for click/highlight.
    outline_lines: Vec<usize>,
    /// Outline nesting levels aligned with `outline`.
    outline_levels: Vec<u8>,
    /// Derived language report (outline, symbols, diagnostics) for the open
    /// document. Rebuilt when the document becomes dirty.
    language: LanguageReport,
    /// Element kind per line for the open document (M9 slice 5), from the
    /// derived Fountain parser; drives the full screenplay highlighting.
    element_kinds: Vec<FountainKind>,
    /// The classification is stale (an edit happened). Recomputing costs a
    /// full parse (~2.2 ms on the 19k-line fixture), so it is debounced to
    /// keep the keystroke→frame path inside the D-019 thresholds.
    elements_dirty: bool,
    /// When the last full classification finished (debounce clock).
    elements_parsed_at: Option<Instant>,
    /// A delayed re-render is already scheduled after a debounced edit.
    elements_timer: bool,
    /// Last backup written for the open document, and its clock/error.
    backup_path: Option<String>,
    backup_saved_at: Option<Instant>,
    backup_error: Option<String>,

    /// Live page count (M9 slice 7): physical pages (body + title page),
    /// computed from the export layout in a worker thread.
    page_count: Option<usize>,
    pages_dirty: bool,
    pages_pending: bool,
    pages_requested_at: Option<Instant>,
    pages_generation: u64,

    scenes_dirty: bool,
    current_scene: usize,

    list_state: ListState,
    /// Scroll state for the diff view (its own list, sized to the rows).
    diff_list_state: ListState,

    /// Read-only Markdown preview (M14a, D-033). Per document/tab: the
    /// source stays untouched and toggling back restores editing in place.
    preview: bool,
    /// Derived blocks for the preview. Rebuilt when the toggle opens; while
    /// the preview is active edits are rejected, so it cannot go stale.
    preview_document: MarkdownDocument,

    /// Find/replace bar (slice 3). While `open`, keystrokes route here
    /// instead of the buffer (single-focus spike compromise).
    find: FindState,

    /// Unified diff view (slice 3, M1-3b). While `open`, it renders instead
    /// of the buffer rows and the probe/diff navigation takes over.
    diff: DiffState,

    /// Document this buffer represents: project-relative path in project
    /// mode, fixture path in spike mode. The status bar shows it.
    path: String,
    /// File text as last loaded or saved. Dirty is a per-document revision
    /// comparison (O(1)) rather than a scan of the document; the status bar
    /// reads it every frame.
    base_text: String,
    /// Edit generation for this document and the generation at its last
    /// load/save. `revision != saved_revision` means dirty. Each document
    /// owns its pair, so saving one tab never cleans another (S3).
    revision: u64,
    saved_revision: u64,
}

impl DocumentState {
    /// Fresh document state: loaded text is the canonical base and there are
    /// no unsaved edits. Everything else starts empty; derived caches are
    /// marked dirty so the first render builds them for this document.
    fn new(path: String, text: &str) -> Self {
        let buffer = Buffer::new(text);
        let line_count = buffer.line_count();
        Self {
            buffer,
            selected: 0..0,
            reversed: false,
            marked: None,
            composing: false,
            dragging: None,
            last_click: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            scene_lines: Vec::new(),
            outline: Vec::new(),
            outline_lines: Vec::new(),
            outline_levels: Vec::new(),
            language: LanguageReport::empty(LanguageId::PlainText),
            element_kinds: Vec::new(),
            elements_dirty: true,
            elements_parsed_at: None,
            elements_timer: false,
            backup_path: None,
            backup_saved_at: None,
            backup_error: None,
            page_count: None,
            pages_dirty: true,
            pages_pending: false,
            pages_requested_at: None,
            pages_generation: 0,
            scenes_dirty: true,
            current_scene: 0,
            list_state: ListState::new(
                line_count,
                ListAlignment::Top,
                px(theme::line_height() * 2.0),
            ),
            diff_list_state: ListState::new(0, ListAlignment::Top, px(theme::line_height() * 2.0)),
            find: FindState::default(),
            preview: false,
            preview_document: MarkdownDocument::default(),
            diff: DiffState {
                base_lines: text.split('\n').map(str::to_string).collect(),
                ..DiffState::default()
            },
            path,
            base_text: text.to_string(),
            revision: 0,
            saved_revision: 0,
        }
    }

    /// Dirty is per document: saving or loading one document snapshots only
    /// its own revision pair.
    fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
    }
}

/// One editor tab (M10 S3b).
///
/// The active tab's document lives in `Editor::doc` so the editor code keeps
/// working on one active document. Every inactive tab owns its document here;
/// switching tabs swaps the two. Exactly one tab (the active one) has `None`.
struct Tab {
    document: Option<DocumentState>,
    /// Preview tab (D-041): a single click replaces it; a double click or the
    /// first edit pins it. A preview tab is never dirty.
    preview: bool,
}

/// One active side-panel drag (M16 slice 3). Kept separate from
/// `doc.dragging`, which belongs to text selection.
#[derive(Debug, Clone, Copy)]
struct PanelDrag {
    target: panel_resize::Target,
    start_x: f32,
    start_w: f32,
}

/// One active chat-scrollbar drag (M16 slice 4).
#[derive(Debug, Clone, Copy)]
struct ChatScrollDrag {
    start_y: f32,
    start_offset: f32,
    viewport_h: f32,
    max_offset: f32,
}

/// Next/previous tab with wrap-around, or `None` when there is nowhere to go.
fn next_tab_index(active: usize, len: usize, forward: bool) -> Option<usize> {
    if len < 2 || active >= len {
        return None;
    }
    Some(if forward {
        (active + 1) % len
    } else {
        (active + len - 1) % len
    })
}

/// Tab that becomes active when `active` is closed: the right neighbour when
/// there is one, otherwise the left neighbour. `None` when it was the last tab.
fn close_target_index(active: usize, len: usize) -> Option<usize> {
    if len < 2 || active >= len {
        return None;
    }
    Some(if active + 1 < len {
        active + 1
    } else {
        active - 1
    })
}

/// The workspace: project-level state plus the active document (S3a/S3b).
struct Editor {
    doc: DocumentState,
    /// Tab strip order; the active tab's slot is `None` (its state is `doc`).
    tabs: Vec<Tab>,
    active_tab: usize,

    /// Autosave loop started (M9 slice 6).
    autosave_started: bool,
    /// The unsaved-changes prompt, when open (quit or close a dirty tab).
    unsaved_prompt: Option<UnsavedPrompt>,
    /// Append the performance metrics to the status bar (--debug-metrics).
    debug_metrics: bool,
    /// The New File modal, when open (M10 slice 2).
    new_file: Option<NewFileState>,
    /// The New Project modal, when open (M10 slice 2b).
    new_project: Option<NewProjectState>,
    /// Monotonic token for AI scaffold runs (M16 slice 9).
    new_project_generation: u64,
    /// Recent project roots, most recent first (M10 slice 2b).
    recents: RecentProjects,
    /// Bumped on every project switch; async search/export results carry the
    /// generation they started with so stale work never touches the new
    /// workspace.
    workspace_generation: u64,
    /// Last title written to the native window bar (M16 slice 7).
    window_title: String,

    /// Local Git status against HEAD (M10 S3c): files, folder aggregates and
    /// branch/ahead/behind. Refreshed on a worker, never on the UI thread.
    git: GitStatus,
    git_pending: bool,
    git_request: u64,
    git_refreshed_at: Option<Instant>,
    /// Panel visibility (session state; persistent settings arrive in S4).
    explorer_open: bool,
    agent_panel_open: bool,
    git_panel_open: bool,
    /// Side-panel widths in logical pixels (M16 slice 3). Session-only until
    /// the width-persistence choice in D-042 is decided.
    explorer_w: f32,
    right_panel_w: f32,
    /// Active splitter drag, when the user is dragging one.
    panel_drag: Option<PanelDrag>,

    focus_handle: FocusHandle,
    /// Wrap width for non-hot rows, refreshed from the viewport each frame and
    /// read by the `list` render callback (which reaches the editor via the
    /// entity, not through the render-time `&mut self`).
    wrap_width: Pixels,

    metrics: Rc<RefCell<Metrics>>,
    probe_left: usize,
    probe_cursor: usize,
    probe_scheduled: bool,

    /// Automated find probe (`--find-steps`): same functions as the
    /// interactive find bar, driven by the probe timer.
    find_probe_left: usize,
    find_probe_cursor: usize,

    /// Automated diff probe (`--diff-steps`): toggles the diff view through
    /// the same functions as cmd-d, so the measured path is the shipped one.
    diff_probe_left: usize,
    /// Total diff steps requested (detects the seeding first step).
    diff_probe_total: usize,

    /// Open Narra project (M6 slice 1). `None` in `--fixture` spike mode,
    /// which keeps the verified spike surface unchanged.
    project: Option<Project>,
    /// Documents offered by the explorer panel (project-relative paths).
    explorer: Vec<String>,
    /// Derived bible entities and mention matcher (M11).
    entities: EntityIndex,
    /// Folders the user collapsed in the explorer tree (M9 slice 4);
    /// persists across document switches for the session.
    collapsed: HashSet<String>,
    /// Last user-visible outcome (save/open/blocked switch). Display only.
    notice: Option<String>,
    /// UI language (M6 slice 9): every string the workspace displays is
    /// looked up through `i18n` with this; toggled live with ⌘⌥L.
    lang: Lang,
    /// Last PDF export outcome (M7 slice 4).
    export: ExportState,
    /// Project-wide search (M8 slice 2).
    search: SearchState,
    /// Agent review panel state (M6 slice 3). Pure reducer — see agent_view.
    agent: AgentState,
    /// Cancellation token for the in-flight agent task (review or propose;
    /// one at a time, so cancellation stays unambiguous).
    agent_cancel: Option<CancellationToken>,
    /// Bumped whenever a review/proposal run starts or the project changes;
    /// a pump from an older run returns immediately, so a cancelled task
    /// that finishes late cannot write into the new project.
    agent_generation: u64,
    /// Adapter settings for agent tasks (persisted settings, or explicit
    /// `--agent-*` flags for this run).
    agent_settings: AgentSettings,
    /// Persisted non-secret preferences (S4); keys live only in the keychain.
    app_settings: AppSettings,
    /// The Settings modal, when open (S4).
    settings: Option<SettingsState>,
    /// The Project Settings form, when open (S4c).
    project_settings: Option<ProjectSettingsState>,
    /// Quick open (⌘P) / command palette (⌘⇧P), when open (S5).
    palette: Option<PaletteState>,
    /// Editor right-click menu (M11 slice 1), when open.
    context_menu: Option<ContextMenuState>,
    /// What the open context menu acts on (editor, explorer file/folder, tab).
    context_target: Option<ContextTarget>,
    /// Human Git surface (M13): recent commits, commit prompt and busy flag.
    git_history: Vec<Commit>,
    git_history_loading: bool,
    git_history_error: String,
    git_history_stale: bool,
    git_commit: Option<GitCommitState>,
    git_diff: Option<GitDiffState>,
    git_restore: Option<GitRestoreState>,
    git_branches: Vec<Branch>,
    git_branches_loading: bool,
    git_branches_error: String,
    git_branches_stale: bool,
    git_branch_prompt: Option<GitBranchPromptState>,
    git_remotes: Vec<(String, String)>,
    git_remotes_loading: bool,
    git_remotes_error: String,
    git_remotes_stale: bool,
    github_publish: Option<GitHubPublishState>,
    git_sync: Option<GitSyncState>,
    github_pr: Option<GitHubPullRequestState>,
    git_busy: bool,
    /// Right-hand surface: Chat (M12) or the older review/proposal activity.
    right_tab: RightTab,
    /// Conversational workspace state (M12).
    chat: ChatUiState,
    /// Bumped for each chat run/project switch so stale events never land.
    chat_generation: u64,
    /// Cancellation token for the in-flight chat run.
    chat_cancel: Option<CancellationToken>,
    /// Saved chat threads and whether the history list is visible.
    chat_history: Vec<ChatThread>,
    chat_history_open: bool,
    /// Scroll state of the chat transcript (M16 slice 4); the app draws the
    /// scrollbar because GPUI 0.2.2 paints none.
    chat_scroll: ScrollHandle,
    /// Active chat-scrollbar drag.
    chat_scroll_drag: Option<ChatScrollDrag>,
    /// Parsed Markdown for assistant turns, keyed by a content hash (M16
    /// slice 5); cleared when the thread or project changes.
    chat_markdown: RefCell<HashMap<u64, MarkdownDocument>>,
    /// Rename dialog, when open.
    rename: Option<FilePromptState>,
    /// Delete confirmation, when open.
    delete_prompt: Option<DeletePromptState>,
    /// Show `project.yaml`/`AGENTS.md`/`agents.yaml` in the explorer (S4c).
    show_config_files: bool,
    /// Soft-wrap long lines (S4c); the geometry still uses `wrap_width`.
    word_wrap: bool,
    /// Proposal surface state (M6 slice 4). Pure reducer — see agent_view.
    propose: ProposeView,
    /// Instruction bar for ⌘K (M6 slice 4; moved from ⌘P in M10 S5).
    propose_bar: ProposeBar,
    /// Reviewer-focus bar for ⌘⇧R (M6 slice 5).
    focus_bar: FocusBar,
    /// Last committed reviewer focus: what ⌘R uses when the bar is closed.
    /// Cleared only by committing an empty focus, never by closing the bar.
    review_focus: String,
}

impl Focusable for Editor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Editor {
    // ---- tabs (M10 S3b) --------------------------------------------------

    fn tab_document(&self, index: usize) -> Option<&DocumentState> {
        if index == self.active_tab {
            Some(&self.doc)
        } else {
            self.tabs.get(index).and_then(|tab| tab.document.as_ref())
        }
    }

    fn tab_path(&self, index: usize) -> &str {
        self.tab_document(index)
            .map(|doc| doc.path.as_str())
            .unwrap_or("")
    }

    fn tab_is_dirty(&self, index: usize) -> bool {
        self.tab_document(index)
            .is_some_and(DocumentState::is_dirty)
    }

    fn dirty_tab_count(&self) -> usize {
        (0..self.tabs.len())
            .filter(|&index| self.tab_is_dirty(index))
            .count()
    }

    fn find_tab(&self, path: &str) -> Option<usize> {
        (0..self.tabs.len()).find(|&index| self.tab_path(index) == path)
    }

    fn tab_title(&self, index: usize) -> String {
        let path = self.tab_path(index);
        if path.is_empty() {
            self.t("tab.untitled").to_string()
        } else {
            path.rsplit('/').next().unwrap_or(path).to_string()
        }
    }

    /// Swap the active document with an inactive tab, preserving selection,
    /// undo, scroll and find/diff state for both. Returns false for the
    /// already-active or an invalid index.
    fn activate_tab(&mut self, index: usize) -> bool {
        if self.unsaved_prompt.is_some()
            || self.settings.is_some()
            || self.project_settings.is_some()
            || self.palette.is_some()
            || self.rename.is_some()
            || self.delete_prompt.is_some()
            || index == self.active_tab
            || index >= self.tabs.len()
        {
            return false;
        }
        let Some(next) = self.tabs[index].document.take() else {
            return false;
        };
        let current = std::mem::replace(&mut self.doc, next);
        self.tabs[self.active_tab].document = Some(current);
        self.active_tab = index;
        // A debounce timer from the previous document must not block this
        // document's first highlight refresh.
        self.doc.elements_timer = false;
        // A diff kept open per tab must describe this document, not the one
        // we just left.
        if self.doc.diff.open {
            self.recompute_diff();
        }
        self.check_backup_notice();
        self.chat.focused = false;
        true
    }

    fn open_new_tab(&mut self, doc: DocumentState, preview: bool) {
        let current = std::mem::replace(&mut self.doc, doc);
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            debug_assert!(tab.document.is_none(), "active tab owns no document");
            tab.document = Some(current);
        }
        self.tabs.push(Tab {
            document: None,
            preview,
        });
        self.active_tab = self.tabs.len() - 1;
    }

    /// Open a loaded document as a regular tab. The single clean blank tab
    /// left after closing the last document is reused instead of
    /// accumulating empty tabs.
    fn open_document_tab(&mut self, doc: DocumentState) {
        if self.tabs.len() == 1 && self.doc.path.is_empty() && !self.doc.is_dirty() {
            self.doc = doc;
            self.tabs[0].preview = false;
        } else {
            self.open_new_tab(doc, false);
        }
    }

    /// Open a document in the single preview slot (D-041). The next single
    /// click replaces it; a double click or the first edit pins it.
    fn open_preview_document(&mut self, doc: DocumentState) {
        if let Some(index) = self.tabs.iter().position(|tab| tab.preview) {
            if index == self.active_tab {
                self.doc = doc;
            } else if self.activate_tab(index) {
                // `activate_tab` swapped the old preview into `doc`; discard
                // it and show the new document in the same slot.
                self.doc = doc;
                self.tabs[self.active_tab].preview = true;
            }
            return;
        }
        if self.tabs.len() == 1 && self.doc.path.is_empty() && !self.doc.is_dirty() {
            self.doc = doc;
            self.tabs[0].preview = true;
        } else {
            self.open_new_tab(doc, true);
        }
    }

    /// Remove a tab and return its document (the active one when closing the
    /// active tab). The active slot always keeps a document: closing the last
    /// tab leaves a clean blank one.
    fn close_tab_at(&mut self, index: usize) -> Option<DocumentState> {
        if index >= self.tabs.len() {
            return None;
        }
        if self.tabs.len() == 1 {
            let old = std::mem::replace(&mut self.doc, DocumentState::new(String::new(), ""));
            self.active_tab = 0;
            return Some(old);
        }
        if index == self.active_tab {
            let target = close_target_index(index, self.tabs.len())?;
            let target_doc = self.tabs[target].document.take()?;
            let old = std::mem::replace(&mut self.doc, target_doc);
            self.tabs.remove(index);
            self.active_tab = if target > index { target - 1 } else { target };
            if self.doc.diff.open {
                self.recompute_diff();
            }
            Some(old)
        } else {
            let removed = self.tabs.remove(index);
            if index < self.active_tab {
                self.active_tab -= 1;
            }
            removed.document
        }
    }

    fn reset_tabs(&mut self) {
        self.tabs = vec![Tab {
            document: None,
            preview: false,
        }];
        self.active_tab = 0;
    }

    // ---- panels (M10 S3c) -------------------------------------------------

    fn on_toggle_explorer(
        &mut self,
        _: &ToggleExplorer,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.explorer_open = !self.explorer_open;
        // Re-opening the sidebar shows the file explorer, not a stale Git
        // panel from the previous session of the toggle.
        if self.explorer_open {
            self.git_panel_open = false;
        }
        cx.notify();
    }

    fn on_toggle_agent_panel(
        &mut self,
        _: &ToggleAgentPanel,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.agent_panel_open = !self.agent_panel_open;
        cx.notify();
    }

    fn on_toggle_git_panel(
        &mut self,
        _: &ToggleGitPanel,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.git_panel_open = !self.git_panel_open;
        if self.git_panel_open {
            self.explorer_open = true;
            self.refresh_git_now(cx);
            self.request_git_history(cx);
            self.request_git_branches(cx);
            self.git_remotes_stale = true;
        }
        cx.notify();
    }

    /// Start a splitter drag (M16 slice 3). The width is applied on
    /// `on_panel_resize`, so a plain click does not move the panel.
    fn begin_panel_resize(
        &mut self,
        target: panel_resize::Target,
        x: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let start_w = match target {
            panel_resize::Target::Explorer => self.explorer_w,
            panel_resize::Target::RightPanel => self.right_panel_w,
        };
        self.panel_drag = Some(PanelDrag {
            target,
            start_x: x,
            start_w,
        });
        self.chat_scroll_drag = None;
        // A splitter drag is not a text click; keep the editor focused so the
        // caret does not move underneath and the chat does not keep the
        // keyboard.
        self.chat.focused = false;
        window.focus(&self.focus_handle(cx));
        cx.notify();
    }

    /// Apply the active drag. The pointer can leave the 6 px handle while the
    /// button is held; the document column keeps its minimum (D-042).
    fn on_panel_resize(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(drag) = self.chat_scroll_drag {
            if event.pressed_button != Some(MouseButton::Left) {
                self.chat_scroll_drag = None;
                cx.notify();
                return;
            }
            let delta = f32::from(event.position.y) - drag.start_y;
            let offset =
                scrollbar::drag_offset(drag.start_offset, delta, drag.viewport_h, drag.max_offset);
            self.chat_scroll.set_offset(point(px(0.0), px(offset)));
            cx.notify();
            return;
        }
        let Some(drag) = self.panel_drag else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.panel_drag = None;
            cx.notify();
            return;
        }
        let delta = f32::from(event.position.x) - drag.start_x;
        let explorer_open = self.project.is_some() && self.explorer_open;
        let right_open = self.agent_panel_open;
        let width = panel_resize::drag_width(
            drag.target,
            drag.start_w,
            delta,
            f32::from(window.viewport_size().width),
            panel_resize::Panels {
                explorer_open,
                right_open,
                explorer_w: self.explorer_w,
                right_w: self.right_panel_w,
            },
        );
        match drag.target {
            panel_resize::Target::Explorer => self.explorer_w = width,
            panel_resize::Target::RightPanel => self.right_panel_w = width,
        }
        cx.notify();
    }

    fn end_panel_resize(&mut self, cx: &mut Context<Self>) {
        let panel = self.panel_drag.take().is_some();
        let chat_scroll = self.chat_scroll_drag.take().is_some();
        if panel || chat_scroll {
            cx.notify();
        }
    }

    /// Rebuild the derived bible entity index from the current project.
    /// Failures are non-fatal: the editor keeps working without mentions,
    /// and the next explicit refresh can recover.
    fn refresh_entities(&mut self) {
        self.entities = self
            .project
            .as_ref()
            .and_then(|project| EntityIndex::build(project).ok())
            .unwrap_or_default();
    }

    /// Open the Git panel and force a refresh (explicit user action).
    fn refresh_git_now(&mut self, cx: &mut Context<Self>) {
        self.git_refreshed_at = None;
        if !self.git_pending {
            self.request_git_refresh(cx);
        }
    }

    // ---- settings (M10 S4) ----------------------------------------------

    /// ⌘, / app menu: toggle the settings modal. It never stacks over another
    /// modal or the unsaved prompt.
    fn on_open_settings(&mut self, _: &OpenSettings, _window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.is_some() {
            self.settings = None;
            cx.notify();
            return;
        }
        if self.unsaved_prompt.is_some()
            || self.new_file.is_some()
            || self.new_project.is_some()
            || self.project_settings.is_some()
            || self.palette.is_some()
            || self.context_menu.is_some()
            || self.rename.is_some()
            || self.delete_prompt.is_some()
            || self.git_commit.is_some()
        {
            return;
        }
        let provider = self.app_settings.provider;
        // Opening Settings must not touch the keychain (D-040): show the
        // session cache when it is known, otherwise "not checked" until the
        // user presses Check or the key is used for the first time.
        let (key_present, key_source, key_checked) = match cached_provider_key_status(provider) {
            Some((present, source)) => (present, source, true),
            None => (false, String::new(), false),
        };
        let (github_token_present, github_token_source, github_token_checked) =
            match cached_github_token_status() {
                Some((present, source)) => (present, source, true),
                None => (false, String::new(), false),
            };
        let base_url = provider
            .fixed_base_url()
            .map(str::to_string)
            .unwrap_or_else(|| self.app_settings.base_url.clone());
        let model = if self.app_settings.model.trim().is_empty() {
            provider.default_model().to_string()
        } else {
            self.app_settings.model.clone()
        };
        let (agents, agents_error) = self.load_effective_agents();
        self.settings = Some(SettingsState {
            section: SettingsSection::General,
            provider,
            base_url,
            model,
            key_input: String::new(),
            key_present,
            key_source,
            key_checked,
            field: SettingsField::Model,
            error: String::new(),
            status: String::new(),
            models: provider
                .known_models()
                .iter()
                .map(|model| model.to_string())
                .collect(),
            models_request: 0,
            fetching_models: false,
            language: self.app_settings.language,
            theme: self.app_settings.theme,
            font_size: self.app_settings.font_size,
            word_wrap: self.app_settings.word_wrap,
            line_numbers: self.app_settings.line_numbers,
            show_config_files: self.app_settings.show_config_files,
            git_name: self.app_settings.git_name.clone(),
            git_email: self.app_settings.git_email.clone(),
            github_token_input: String::new(),
            github_token_present,
            github_token_source,
            github_token_checked,
            key_request: 0,
            github_request: 0,
            agents,
            agents_error,
        });
        cx.notify();
    }

    /// Effective roles for the Agentes tab: built-ins, user packages,
    /// project packages and `agents.yaml`. File-based installation only;
    /// the tab is a read-only visibility surface.
    fn load_effective_agents(&self) -> (Vec<roles::Role>, String) {
        let Some(project) = self.project.as_ref() else {
            return (Vec::new(), String::new());
        };
        match roles::load_roles_with_packages(project.root(), Some(&recent::config_dir())) {
            Ok(agents) => (agents, String::new()),
            Err(error) => (Vec::new(), error.to_string()),
        }
    }

    fn reload_settings_agents(&mut self, cx: &mut Context<Self>) {
        let (agents, agents_error) = self.load_effective_agents();
        if let Some(state) = self.settings.as_mut() {
            state.agents = agents;
            state.agents_error = agents_error;
            state.status.clear();
        }
        cx.notify();
    }

    fn settings_set_provider(&mut self, provider: AiProvider, cx: &mut Context<Self>) {
        // Switching providers never reads the keychain: use the session cache.
        let (key_present, key_source, key_checked) = match cached_provider_key_status(provider) {
            Some((present, source)) => (present, source, true),
            None => (false, String::new(), false),
        };
        if let Some(state) = self.settings.as_mut() {
            state.provider = provider;
            state.base_url = provider
                .fixed_base_url()
                .map(str::to_string)
                .unwrap_or_else(|| {
                    if state.base_url.trim().is_empty() {
                        LOCAL_DEFAULT_BASE_URL.to_string()
                    } else {
                        state.base_url.clone()
                    }
                });
            if state.model.trim().is_empty() {
                state.model = provider.default_model().to_string();
            }
            state.key_input.clear();
            state.key_present = key_present;
            state.key_source = key_source;
            state.key_checked = key_checked;
            state.models = provider
                .known_models()
                .iter()
                .map(|model| model.to_string())
                .collect();
            state.error.clear();
            state.status.clear();
        }
        cx.notify();
    }

    fn settings_input(&mut self, text: &str, cx: &mut Context<Self>) {
        let cleaned: String = text
            .chars()
            .filter(|c| !c.is_control() || *c == ' ')
            .collect();
        if let Some(state) = self.settings.as_mut() {
            match state.field {
                SettingsField::BaseUrl => state.base_url.push_str(&cleaned),
                SettingsField::Model => state.model.push_str(&cleaned),
                SettingsField::ApiKey => state.key_input.push_str(&cleaned),
                SettingsField::GitName => state.git_name.push_str(&cleaned),
                SettingsField::GitEmail => state.git_email.push_str(&cleaned),
                SettingsField::GitHubToken => state.github_token_input.push_str(&cleaned),
            }
            state.error.clear();
            state.status.clear();
        }
        cx.notify();
    }

    fn settings_backspace(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.settings.as_mut() {
            let target = match state.field {
                SettingsField::BaseUrl => &mut state.base_url,
                SettingsField::Model => &mut state.model,
                SettingsField::ApiKey => &mut state.key_input,
                SettingsField::GitName => &mut state.git_name,
                SettingsField::GitEmail => &mut state.git_email,
                SettingsField::GitHubToken => &mut state.github_token_input,
            };
            if let Some(last) = target.chars().last() {
                let len = target.len();
                target.truncate(len - last.len_utf8());
            }
            state.error.clear();
            state.status.clear();
        }
        cx.notify();
    }

    fn settings_cycle_field(&mut self, forward: bool, cx: &mut Context<Self>) {
        if let Some(state) = self.settings.as_mut() {
            let fields: &[SettingsField] = match state.section {
                SettingsSection::Ai => {
                    if state.provider.fixed_base_url().is_some() {
                        &[SettingsField::Model, SettingsField::ApiKey]
                    } else {
                        &[
                            SettingsField::BaseUrl,
                            SettingsField::Model,
                            SettingsField::ApiKey,
                        ]
                    }
                }
                SettingsSection::Git => &[
                    SettingsField::GitName,
                    SettingsField::GitEmail,
                    SettingsField::GitHubToken,
                ],
                SettingsSection::General | SettingsSection::Editor | SettingsSection::Agents => &[],
            };
            if fields.is_empty() {
                cx.notify();
                return;
            }
            let current = fields
                .iter()
                .position(|field| *field == state.field)
                .unwrap_or(0);
            let next = if forward {
                (current + 1) % fields.len()
            } else {
                (current + fields.len() - 1) % fields.len()
            };
            state.field = fields[next];
        }
        cx.notify();
    }

    fn settings_save(&mut self, cx: &mut Context<Self>) {
        let lang = self.lang;
        let Some(state) = self.settings.as_ref() else {
            return;
        };
        let provider = state.provider;
        let model = if provider == AiProvider::Mock {
            String::new()
        } else {
            state.model.trim().to_string()
        };
        let base_url = state
            .provider
            .fixed_base_url()
            .map(str::to_string)
            .unwrap_or_else(|| state.base_url.trim().to_string());
        let key_input = state.key_input.trim().to_string();
        if provider != AiProvider::Mock && model.is_empty() {
            if let Some(state) = self.settings.as_mut() {
                state.error = i18n::tr(lang, "settings.model_required").to_string();
            }
            cx.notify();
            return;
        }
        if provider == AiProvider::OpenAiCompat && base_url.is_empty() {
            if let Some(state) = self.settings.as_mut() {
                state.error = i18n::tr(lang, "settings.base_url_required").to_string();
            }
            cx.notify();
            return;
        }
        // A known-absent key is refused. An unknown answer is allowed through:
        // Settings never reads the keychain here (D-040), and the first use
        // reports a missing key explicitly.
        if provider.requires_key()
            && key_input.is_empty()
            && state.key_checked
            && !state.key_present
        {
            if let Some(state) = self.settings.as_mut() {
                state.error = i18n::tr(lang, "settings.key_required").to_string();
            }
            cx.notify();
            return;
        }
        if let Some(account) = provider.secret_account()
            && !key_input.is_empty()
        {
            let store = secret_cache();
            if let Err(error) = store.set(account, &key_input) {
                if let Some(state) = self.settings.as_mut() {
                    state.error = i18n::tf(
                        lang,
                        "settings.key_save_failed",
                        &[("error", &error.to_string())],
                    );
                }
                cx.notify();
                return;
            }
        }
        if let Some(state) = self.settings.as_ref()
            && !state.github_token_input.trim().is_empty()
        {
            let store = secret_cache();
            if let Err(error) = store.set(GITHUB_TOKEN_ACCOUNT, state.github_token_input.trim()) {
                let message = i18n::tf(
                    lang,
                    "settings.github_key_save_failed",
                    &[("error", &error.to_string())],
                );
                if let Some(state) = self.settings.as_mut() {
                    state.error = message;
                }
                cx.notify();
                return;
            }
        }
        let app_settings = AppSettings {
            provider,
            base_url: base_url.clone(),
            model: model.clone(),
            language: state.language,
            theme: state.theme,
            font_size: state.font_size,
            word_wrap: state.word_wrap,
            line_numbers: state.line_numbers,
            show_config_files: state.show_config_files,
            git_name: state.git_name.trim().to_string(),
            git_email: state.git_email.trim().to_string(),
            error: None,
        };
        if let Err(error) = app_settings.save() {
            if let Some(state) = self.settings.as_mut() {
                state.error = i18n::tf(lang, "settings.save_failed", &[("error", &error)]);
            }
            cx.notify();
            return;
        }
        let language_changed = self.lang != app_settings.language.resolved();
        let old_font_size = self.app_settings.font_size;
        let old_line_numbers = self.app_settings.line_numbers;
        self.app_settings = app_settings;
        self.agent_settings = AgentSettings {
            provider,
            base_url: self.app_settings.resolved_base_url(),
            model,
            latency_ms: self.agent_settings.latency_ms,
        };
        theme::set_mode(self.app_settings.theme);
        theme::set_font_size(self.app_settings.font_size);
        theme::set_line_numbers(self.app_settings.line_numbers);
        self.show_config_files = self.app_settings.show_config_files;
        self.word_wrap = self.app_settings.word_wrap;
        if old_font_size != self.app_settings.font_size
            || old_line_numbers != self.app_settings.line_numbers
        {
            self.doc.list_state = ListState::new(
                self.doc.buffer.lines().len(),
                ListAlignment::Top,
                px(theme::line_height() * 2.0),
            );
            for tab in &mut self.tabs {
                if let Some(doc) = tab.document.as_mut() {
                    doc.list_state = ListState::new(
                        doc.buffer.lines().len(),
                        ListAlignment::Top,
                        px(theme::line_height() * 2.0),
                    );
                }
            }
        }
        if language_changed {
            self.lang = self.app_settings.language.resolved();
            self.agent.lang = self.lang;
            self.propose.lang = self.lang;
            self.refresh_menus(cx);
        }
        self.settings = None;
        self.notice = Some(self.tf(
            "notice.settings_saved",
            &[("provider", self.t(provider.i18n_key()))],
        ));
        cx.notify();
    }

    /// Fetch the live catalog with the key currently in the modal (or the
    /// stored/env key). This is also the connection check for OpenCode.
    fn settings_fetch_models(&mut self, cx: &mut Context<Self>) {
        let lang = self.lang;
        let Some(state) = self.settings.as_ref() else {
            return;
        };
        if state.fetching_models {
            return;
        }
        let provider = state.provider;
        if provider == AiProvider::Mock {
            if let Some(state) = self.settings.as_mut() {
                state.status = i18n::tr(lang, "settings.mock_local").to_string();
            }
            cx.notify();
            return;
        }
        let base_url = if let Some(fixed) = provider.fixed_base_url() {
            fixed.to_string()
        } else {
            state.base_url.trim().to_string()
        };
        let Some(models_url) = provider.models_url(&base_url) else {
            return;
        };
        let key_input = state.key_input.trim().to_string();
        let needs_key = provider.requires_key();
        let request = state.models_request.saturating_add(1);
        if let Some(state) = self.settings.as_mut() {
            state.models_request = request;
            state.fetching_models = true;
            state.error.clear();
            state.status = i18n::tr(lang, "settings.fetching").to_string();
        }
        cx.notify();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            // The first keychain read happens here, never on the UI thread
            // (D-040).
            let key = if key_input.is_empty() {
                match resolve_provider_key(provider) {
                    Ok(key) => key,
                    Err(error) => {
                        let _ = tx.send(SettingsMessage::Models {
                            request,
                            result: Err(error),
                        });
                        return;
                    }
                }
            } else {
                Some(key_input)
            };
            if needs_key && key.is_none() {
                let _ = tx.send(SettingsMessage::MissingKey { request });
                return;
            }
            let result = fetch_model_ids(&models_url, key.as_deref(), Duration::from_secs(20))
                .map_err(|error| error.to_string());
            let _ = tx.send(SettingsMessage::Models { request, result });
        });
        Self::spawn_message_pump(cx, rx, |view, message| {
            let lang = view.lang;
            match message {
                SettingsMessage::MissingKey { request } => {
                    if let Some(state) = view.settings.as_mut()
                        && state.models_request == request
                    {
                        state.fetching_models = false;
                        state.error = i18n::tr(lang, "settings.key_required").to_string();
                    }
                }
                SettingsMessage::Models { request, result } => {
                    if let Some(state) = view.settings.as_mut()
                        && state.models_request == request
                    {
                        state.fetching_models = false;
                        match result {
                            Ok(mut models) => {
                                models.sort();
                                models.dedup();
                                let count = models.len();
                                state.models = models;
                                state.status = i18n::tf(
                                    lang,
                                    "settings.models_fetched",
                                    &[("n", &count.to_string())],
                                );
                            }
                            Err(error) => {
                                state.error = i18n::tf(
                                    lang,
                                    "settings.models_failed",
                                    &[("error", &error.to_string())],
                                );
                            }
                        }
                    }
                }
                // One-shot check answers have their own pumps.
                SettingsMessage::KeyStatus { .. } | SettingsMessage::GitHubStatus { .. } => {}
            }
            true
        });
    }

    fn settings_clear_key(&mut self, cx: &mut Context<Self>) {
        let lang = self.lang;
        let Some(state) = self.settings.as_ref() else {
            return;
        };
        let Some(account) = state.provider.secret_account() else {
            return;
        };
        let store = secret_cache();
        if let Err(error) = store.delete(account) {
            if let Some(state) = self.settings.as_mut() {
                state.error = i18n::tf(
                    lang,
                    "settings.key_save_failed",
                    &[("error", &error.to_string())],
                );
            }
            cx.notify();
            return;
        }
        let (key_present, key_source) = provider_key_status(state.provider);
        if let Some(state) = self.settings.as_mut() {
            state.key_input.clear();
            state.key_present = key_present;
            state.key_source = key_source;
            state.key_checked = true;
            state.status = i18n::tr(lang, "settings.key_cleared").to_string();
            state.error.clear();
        }
        cx.notify();
    }

    fn settings_clear_github_token(&mut self, cx: &mut Context<Self>) {
        let store = secret_cache();
        if let Err(error) = store.delete(GITHUB_TOKEN_ACCOUNT) {
            let message = self.tf(
                "settings.github_key_save_failed",
                &[("error", &error.to_string())],
            );
            if let Some(state) = self.settings.as_mut() {
                state.error = message;
            }
            cx.notify();
            return;
        }
        let (present, source) = github_token_status();
        let status = self.t("settings.github_key_cleared").to_string();
        if let Some(state) = self.settings.as_mut() {
            state.github_token_input.clear();
            state.github_token_present = present;
            state.github_token_source = source;
            state.github_token_checked = true;
            state.status = status;
            state.error.clear();
        }
        cx.notify();
    }

    /// Explicit "check" action for the provider key (D-040). This is the only
    /// Settings path that reads the keychain, and it runs on a worker so a
    /// macOS prompt cannot freeze the window.
    fn settings_check_key(&mut self, cx: &mut Context<Self>) {
        let Some(provider) = self.settings.as_ref().map(|state| state.provider) else {
            return;
        };
        let Some(request) = self.settings.as_mut().map(|state| {
            state.key_request = state.key_request.saturating_add(1);
            state.key_request
        }) else {
            return;
        };
        // An explicit Check retries a failed read (for example a denied
        // prompt); a known present/absent answer stays cached.
        if let Some(account) = provider.secret_account()
            && matches!(secret_cache().peek(account), Some(Err(_)))
        {
            secret_cache().invalidate(account);
        }
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let (present, source) = provider_key_status(provider);
            let _ = tx.send(SettingsMessage::KeyStatus {
                request,
                provider,
                present,
                source,
            });
        });
        Self::spawn_message_pump(cx, rx, |view, message| {
            let SettingsMessage::KeyStatus {
                request,
                provider,
                present,
                source,
            } = message
            else {
                return true;
            };
            if let Some(state) = view.settings.as_mut()
                && state.key_request == request
                && state.provider == provider
            {
                state.key_present = present;
                state.key_source = source;
                state.key_checked = true;
                state.status.clear();
                state.error.clear();
            }
            true
        });
        cx.notify();
    }

    fn settings_check_github_token(&mut self, cx: &mut Context<Self>) {
        let Some(request) = self.settings.as_mut().map(|state| {
            state.github_request = state.github_request.saturating_add(1);
            state.github_request
        }) else {
            return;
        };
        if matches!(secret_cache().peek(GITHUB_TOKEN_ACCOUNT), Some(Err(_))) {
            secret_cache().invalidate(GITHUB_TOKEN_ACCOUNT);
        }
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let (present, source) = github_token_status();
            let _ = tx.send(SettingsMessage::GitHubStatus {
                request,
                present,
                source,
            });
        });
        Self::spawn_message_pump(cx, rx, |view, message| {
            let SettingsMessage::GitHubStatus {
                request,
                present,
                source,
            } = message
            else {
                return true;
            };
            if let Some(state) = view.settings.as_mut()
                && state.github_request == request
            {
                state.github_token_present = present;
                state.github_token_source = source;
                state.github_token_checked = true;
                state.status.clear();
                state.error.clear();
            }
            true
        });
        cx.notify();
    }

    /// The Settings card. The API key is never echoed: the field shows a
    /// fixed mask and the dump only reports presence/source.
    fn settings_overlay(&self, cx: &mut Context<Self>) -> gpui::Div {
        let state = self.settings.as_ref().expect("overlay only when open");
        let field = |id: &'static str, text: String, active: bool| {
            div()
                .id(id)
                .flex_1()
                .px_2()
                .py_1()
                .bg(rgb(theme::explorer_bg()))
                .border_1()
                .border_color(rgb(if active {
                    theme::default_fg()
                } else {
                    theme::explorer_border()
                }))
                .text_color(rgb(theme::default_fg()))
                .child(text)
        };
        let chip = |id: (&'static str, usize), label: String, selected: bool| {
            div()
                .id(id)
                .px_3()
                .py_1()
                .bg(rgb(if selected {
                    theme::doc_selected_bg()
                } else {
                    theme::explorer_bg()
                }))
                .border_1()
                .border_color(rgb(if selected {
                    theme::default_fg()
                } else {
                    theme::explorer_border()
                }))
                .text_color(rgb(if selected {
                    theme::default_fg()
                } else {
                    theme::status_fg()
                }))
                .cursor_pointer()
                .child(label)
        };
        let toggle = |id: &'static str, label: String, on: bool| {
            div()
                .id(id)
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .bg(rgb(theme::explorer_bg()))
                .border_1()
                .border_color(rgb(theme::explorer_border()))
                .cursor_pointer()
                .child(
                    div()
                        .w(px(36.0))
                        .text_color(rgb(if on {
                            theme::git_added_fg()
                        } else {
                            theme::status_fg()
                        }))
                        .child(if on {
                            self.t("settings.on")
                        } else {
                            self.t("settings.off")
                        }),
                )
                .child(div().text_color(rgb(theme::default_fg())).child(label))
        };
        let mut tabs = div().flex().gap_1();
        for (index, section) in SettingsSection::ALL.into_iter().enumerate() {
            let selected = section == state.section;
            tabs = tabs.child(
                div()
                    .id(("settings-section", index))
                    .px_3()
                    .py_1()
                    .bg(rgb(if selected {
                        theme::doc_selected_bg()
                    } else {
                        theme::explorer_bg()
                    }))
                    .text_color(rgb(if selected {
                        theme::default_fg()
                    } else {
                        theme::status_fg()
                    }))
                    .cursor_pointer()
                    .child(self.t(section.i18n_key()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(state) = this.settings.as_mut() {
                            state.section = section;
                            state.error.clear();
                            state.status.clear();
                        }
                        cx.notify();
                    })),
            );
        }

        let content = match state.section {
            SettingsSection::General => {
                let mut languages = div().flex().flex_wrap().gap_1();
                for (index, language) in LanguageSetting::ALL.into_iter().enumerate() {
                    languages = languages.child(
                        chip(
                            ("settings-language", index),
                            self.t(language.i18n_key()).to_string(),
                            state.language == language,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(state) = this.settings.as_mut() {
                                state.language = language;
                                state.status.clear();
                            }
                            cx.notify();
                        })),
                    );
                }
                let mut themes = div().flex().flex_wrap().gap_1();
                for (index, mode) in ThemeMode::ALL.into_iter().enumerate() {
                    themes = themes.child(
                        chip(
                            ("settings-theme", index),
                            self.t(mode.i18n_key()).to_string(),
                            state.theme == mode,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(state) = this.settings.as_mut() {
                                state.theme = mode;
                                state.status.clear();
                            }
                            cx.notify();
                        })),
                    );
                }
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_color(rgb(theme::status_fg()))
                            .child(self.t("settings.language")),
                    )
                    .child(languages)
                    .child(
                        div()
                            .text_color(rgb(theme::status_fg()))
                            .child(self.t("settings.theme")),
                    )
                    .child(themes)
                    .child(
                        div()
                            .text_color(rgb(theme::gutter_fg()))
                            .child(self.t("settings.general_hint")),
                    )
            }
            SettingsSection::Editor => div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .w(px(110.0))
                                .text_color(rgb(theme::status_fg()))
                                .child(self.t("settings.font_size")),
                        )
                        .child(
                            div()
                                .id("settings-font-minus")
                                .px_2()
                                .py_1()
                                .bg(rgb(theme::explorer_bg()))
                                .border_1()
                                .border_color(rgb(theme::explorer_border()))
                                .cursor_pointer()
                                .child("−")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(state) = this.settings.as_mut() {
                                        state.font_size = state.font_size.saturating_sub(1).max(12);
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .w(px(42.0))
                                .text_color(rgb(theme::default_fg()))
                                .child(format!("{} pt", state.font_size)),
                        )
                        .child(
                            div()
                                .id("settings-font-plus")
                                .px_2()
                                .py_1()
                                .bg(rgb(theme::explorer_bg()))
                                .border_1()
                                .border_color(rgb(theme::explorer_border()))
                                .cursor_pointer()
                                .child("+")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(state) = this.settings.as_mut() {
                                        state.font_size = (state.font_size + 1).min(24);
                                    }
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    toggle(
                        "settings-word-wrap",
                        self.t("settings.word_wrap").to_string(),
                        state.word_wrap,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(state) = this.settings.as_mut() {
                            state.word_wrap = !state.word_wrap;
                        }
                        cx.notify();
                    })),
                )
                .child(
                    toggle(
                        "settings-line-numbers",
                        self.t("settings.line_numbers").to_string(),
                        state.line_numbers,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(state) = this.settings.as_mut() {
                            state.line_numbers = !state.line_numbers;
                        }
                        cx.notify();
                    })),
                )
                .child(
                    toggle(
                        "settings-config-files",
                        self.t("settings.show_config_files").to_string(),
                        state.show_config_files,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(state) = this.settings.as_mut() {
                            state.show_config_files = !state.show_config_files;
                        }
                        cx.notify();
                    })),
                )
                .child(
                    div()
                        .text_color(rgb(theme::gutter_fg()))
                        .child(self.t("settings.editor_hint")),
                ),
            SettingsSection::Ai => {
                let mut providers = div().flex().flex_wrap().gap_2();
                for (index, provider) in AiProvider::ALL.into_iter().enumerate() {
                    let selected = provider == state.provider;
                    providers = providers.child(
                        chip(
                            ("settings-provider", index),
                            self.t(provider.i18n_key()).to_string(),
                            selected,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.settings_set_provider(provider, cx)
                        })),
                    );
                }
                let fixed_base = state.provider.fixed_base_url();
                let base_text = if fixed_base.is_some() {
                    state.base_url.clone()
                } else {
                    format!("{}▌", state.base_url)
                };
                let key_text = if !state.key_input.is_empty() {
                    format!("{}▌", "•".repeat(state.key_input.chars().count().min(32)))
                } else if state.key_checked && state.key_present {
                    format!(
                        "•••••••• ({})",
                        self.t(if state.key_source == "env" {
                            "settings.key_source_env"
                        } else {
                            "settings.key_source_keychain"
                        })
                    )
                } else if !state.key_checked {
                    self.t("settings.key_unchecked").to_string()
                } else {
                    self.t("settings.key_empty").to_string()
                };
                let key_active = state.field == SettingsField::ApiKey;
                let model_active = state.field == SettingsField::Model;
                let base_active = state.field == SettingsField::BaseUrl;
                let mut model_list = div()
                    .id("settings-models")
                    .h(px(112.0))
                    .w_full()
                    .overflow_y_scroll()
                    .border_1()
                    .border_color(rgb(theme::explorer_border()));
                for (index, model) in state.models.iter().enumerate() {
                    let selected = model == &state.model;
                    let choice = model.clone();
                    model_list = model_list.child(
                        div()
                            .id(("settings-model", index))
                            .px_2()
                            .py_1()
                            .text_sm()
                            .cursor_pointer()
                            .bg(rgb(if selected {
                                theme::doc_selected_bg()
                            } else {
                                theme::explorer_bg()
                            }))
                            .text_color(rgb(if selected {
                                theme::default_fg()
                            } else {
                                theme::status_fg()
                            }))
                            .child(choice.clone())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(state) = this.settings.as_mut() {
                                    state.model = choice.clone();
                                    state.field = SettingsField::Model;
                                    state.error.clear();
                                }
                                cx.notify();
                            })),
                    );
                }
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(providers)
                    .when(state.provider == AiProvider::Mock, |card| {
                        card.child(
                            div()
                                .text_color(rgb(theme::status_fg()))
                                .child(self.t("settings.mock_hint")),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(110.0))
                                    .text_color(rgb(theme::status_fg()))
                                    .child(self.t("settings.base_url")),
                            )
                            .child(if fixed_base.is_some() {
                                div()
                                    .flex_1()
                                    .px_2()
                                    .py_1()
                                    .text_color(rgb(theme::status_fg()))
                                    .child(base_text)
                                    .into_any_element()
                            } else {
                                field("settings-base-url", base_text, base_active)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(state) = this.settings.as_mut() {
                                            state.field = SettingsField::BaseUrl;
                                        }
                                        cx.notify();
                                    }))
                                    .into_any_element()
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(110.0))
                                    .text_color(rgb(theme::status_fg()))
                                    .child(self.t("settings.model")),
                            )
                            .child(
                                field(
                                    "settings-model-field",
                                    format!("{}▌", state.model),
                                    model_active,
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        if let Some(state) = this.settings.as_mut() {
                                            state.field = SettingsField::Model;
                                        }
                                        cx.notify();
                                    },
                                )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(110.0))
                                    .text_color(rgb(theme::status_fg()))
                                    .child(self.t("settings.api_key")),
                            )
                            .child(
                                field("settings-api-key-field", key_text, key_active).on_click(
                                    cx.listener(|this, _, _, cx| {
                                        if let Some(state) = this.settings.as_mut() {
                                            state.field = SettingsField::ApiKey;
                                        }
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(
                                div()
                                    .id("settings-check-key")
                                    .px_2()
                                    .py_1()
                                    .cursor_pointer()
                                    .text_color(rgb(theme::status_fg()))
                                    .child(self.t("settings.check_key"))
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.settings_check_key(cx)),
                                    ),
                            )
                            .child(
                                div()
                                    .id("settings-clear-key")
                                    .px_2()
                                    .py_1()
                                    .cursor_pointer()
                                    .text_color(rgb(theme::status_fg()))
                                    .child(self.t("settings.clear_key"))
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.settings_clear_key(cx)),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .text_color(rgb(theme::gutter_fg()))
                            .child(self.t("settings.key_hint")),
                    )
                    .child(model_list)
                    .child(
                        div()
                            .text_color(rgb(theme::gutter_fg()))
                            .child(self.t("settings.model_route_hint")),
                    )
            }
            SettingsSection::Git => div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .w(px(110.0))
                                .text_color(rgb(theme::status_fg()))
                                .child(self.t("settings.git_name")),
                        )
                        .child(
                            field(
                                "settings-git-name",
                                format!("{}▌", state.git_name),
                                state.field == SettingsField::GitName,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(state) = this.settings.as_mut() {
                                    state.field = SettingsField::GitName;
                                }
                                cx.notify();
                            })),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .w(px(110.0))
                                .text_color(rgb(theme::status_fg()))
                                .child(self.t("settings.git_email")),
                        )
                        .child(
                            field(
                                "settings-git-email",
                                format!("{}▌", state.git_email),
                                state.field == SettingsField::GitEmail,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(state) = this.settings.as_mut() {
                                    state.field = SettingsField::GitEmail;
                                }
                                cx.notify();
                            })),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .w(px(110.0))
                                .text_color(rgb(theme::status_fg()))
                                .child(self.t("settings.github_token")),
                        )
                        .child(
                            field(
                                "settings-github-token",
                                if !state.github_token_input.is_empty() {
                                    format!(
                                        "{}▌",
                                        "•".repeat(
                                            state.github_token_input.chars().count().min(32)
                                        )
                                    )
                                } else if state.github_token_checked && state.github_token_present {
                                    format!(
                                        "•••••••• ({})",
                                        self.t(if state.github_token_source == "env" {
                                            "settings.key_source_env"
                                        } else {
                                            "settings.key_source_keychain"
                                        })
                                    )
                                } else if !state.github_token_checked {
                                    self.t("settings.key_unchecked").to_string()
                                } else {
                                    self.t("settings.github_token_empty").to_string()
                                },
                                state.field == SettingsField::GitHubToken,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(state) = this.settings.as_mut() {
                                    state.field = SettingsField::GitHubToken;
                                }
                                cx.notify();
                            })),
                        )
                        .child(
                            div()
                                .id("settings-check-github-token")
                                .px_2()
                                .py_1()
                                .cursor_pointer()
                                .text_color(rgb(theme::status_fg()))
                                .child(self.t("settings.check_key"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.settings_check_github_token(cx)
                                })),
                        )
                        .child(
                            div()
                                .id("settings-clear-github-token")
                                .px_2()
                                .py_1()
                                .cursor_pointer()
                                .text_color(rgb(theme::status_fg()))
                                .child(self.t("settings.clear_key"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.settings_clear_github_token(cx)
                                })),
                        ),
                )
                .child(
                    div()
                        .text_color(rgb(theme::gutter_fg()))
                        .child(self.t("settings.github_hint")),
                )
                .child(
                    div()
                        .text_color(rgb(theme::gutter_fg()))
                        .child(self.t("settings.git_hint")),
                ),
            SettingsSection::Agents => {
                let mut body = div().flex().flex_col().gap_2().child(
                    div()
                        .text_color(rgb(theme::gutter_fg()))
                        .child(self.t("settings.agents_hint")),
                );
                if !state.agents_error.is_empty() {
                    body = body.child(
                        div()
                            .text_color(rgb(theme::agent_err_fg()))
                            .child(state.agents_error.clone()),
                    );
                } else if state.agents.is_empty() {
                    body = body.child(
                        div()
                            .text_color(rgb(theme::status_fg()))
                            .child(self.t("settings.agents_empty")),
                    );
                } else {
                    let mut list = div()
                        .id("settings-agents-list")
                        .max_h(px(360.0))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap_1();
                    for (index, role) in state.agents.iter().enumerate() {
                        let version = role
                            .version
                            .as_deref()
                            .map(|version| format!("v{version}"))
                            .unwrap_or_else(|| "—".to_string());
                        list = list.child(
                            div()
                                .id(("settings-agent", index))
                                .p_2()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .border_1()
                                .border_color(rgb(theme::explorer_border()))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            div()
                                                .flex_1()
                                                .text_color(rgb(theme::default_fg()))
                                                .child(format!("{} · {}", role.name, role.id)),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(rgb(theme::gutter_fg()))
                                                .child(version),
                                        ),
                                )
                                .child(div().text_xs().text_color(rgb(theme::status_fg())).child(
                                    format!(
                                        "{} · {} · {}",
                                        role_source_label(self.lang, role.origin_key()),
                                        self.t("role.trust.instructions"),
                                        role.tasks.join(", ")
                                    ),
                                ))
                                .when(role.description.is_some(), |card| {
                                    card.child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(theme::gutter_fg()))
                                            .child(role.description.clone().unwrap_or_default()),
                                    )
                                }),
                        );
                    }
                    body = body.child(list);
                }
                body.child(
                    div()
                        .id("settings-agents-reload")
                        .px_3()
                        .py_1()
                        .bg(rgb(theme::status_bg()))
                        .text_color(rgb(theme::status_fg()))
                        .cursor_pointer()
                        .child(self.t("settings.agents_reload"))
                        .on_click(cx.listener(|this, _, _, cx| this.reload_settings_agents(cx))),
                )
            }
        };

        div()
            .absolute()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000099))
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .on_mouse_up(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .child(
                div()
                    .w(px(760.0))
                    .id("settings-card")
                    .max_h(px(640.0))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_4()
                    .bg(rgb(theme::explorer_bg()))
                    .border_1()
                    .border_color(rgb(theme::explorer_border()))
                    .child(
                        div()
                            .text_color(rgb(theme::default_fg()))
                            .child(self.t("settings.title")),
                    )
                    .child(tabs)
                    .child(content)
                    .when(!state.error.is_empty(), |card| {
                        card.child(
                            div()
                                .text_color(rgb(theme::agent_err_fg()))
                                .child(state.error.clone()),
                        )
                    })
                    .when(!state.status.is_empty(), |card| {
                        card.child(
                            div()
                                .text_color(rgb(theme::status_fg()))
                                .child(state.status.clone()),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .when(state.section == SettingsSection::Ai, |row| {
                                row.child(
                                    div()
                                        .id("settings-fetch")
                                        .px_3()
                                        .py_1()
                                        .bg(rgb(theme::status_bg()))
                                        .text_color(rgb(theme::default_fg()))
                                        .cursor_pointer()
                                        .child(if state.fetching_models {
                                            self.t("settings.fetching")
                                        } else {
                                            self.t("settings.fetch_models")
                                        })
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.settings_fetch_models(cx)
                                        })),
                                )
                            })
                            .child(
                                div()
                                    .id("settings-save")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(theme::status_bg()))
                                    .text_color(rgb(theme::default_fg()))
                                    .cursor_pointer()
                                    .child(self.t("settings.save"))
                                    .on_click(cx.listener(|this, _, _, cx| this.settings_save(cx))),
                            )
                            .child(
                                div()
                                    .id("settings-cancel")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(theme::status_bg()))
                                    .text_color(rgb(theme::status_fg()))
                                    .cursor_pointer()
                                    .child(self.t("settings.cancel"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.settings = None;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
    }

    // ---- project settings and file visibility (M10 S4c) -----------------

    /// Project menu / explorer gear: open the validated `project.yaml` form.
    fn on_open_project_settings(
        &mut self,
        _: &OpenProjectSettings,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.project_settings.is_some() {
            self.project_settings = None;
            cx.notify();
            return;
        }
        if self.unsaved_prompt.is_some()
            || self.settings.is_some()
            || self.new_file.is_some()
            || self.new_project.is_some()
            || self.palette.is_some()
            || self.context_menu.is_some()
            || self.rename.is_some()
            || self.delete_prompt.is_some()
            || self.git_commit.is_some()
        {
            return;
        }
        let Some(project) = self.project.as_ref() else {
            self.notice = Some(self.t("notice.fixture_no_project_short").to_string());
            cx.notify();
            return;
        };
        self.project_settings = Some(ProjectSettingsState::new(project));
        cx.notify();
    }

    fn on_toggle_config_files(
        &mut self,
        _: &ToggleConfigFiles,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_config_files = !self.show_config_files;
        self.app_settings.show_config_files = self.show_config_files;
        if let Some(state) = self.settings.as_mut() {
            state.show_config_files = self.show_config_files;
        }
        if let Err(error) = self.app_settings.save() {
            self.notice = Some(self.tf("notice.settings_save_failed", &[("error", &error)]));
        } else {
            self.notice = Some(
                self.t(if self.show_config_files {
                    "notice.config_files_shown"
                } else {
                    "notice.config_files_hidden"
                })
                .to_string(),
            );
        }
        cx.notify();
    }

    fn project_settings_input(&mut self, text: &str, cx: &mut Context<Self>) {
        if let Some(state) = self.project_settings.as_mut() {
            state.input(text);
        }
        cx.notify();
    }

    fn project_settings_backspace(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.project_settings.as_mut() {
            state.backspace();
        }
        cx.notify();
    }

    fn project_settings_cycle_field(&mut self, forward: bool, cx: &mut Context<Self>) {
        if let Some(state) = self.project_settings.as_mut() {
            state.cycle_field(forward);
        }
        cx.notify();
    }

    fn project_settings_save(&mut self, cx: &mut Context<Self>) {
        let lang = self.lang;
        let Some(state) = self.project_settings.clone() else {
            return;
        };
        let Some(project) = self.project.as_mut() else {
            self.notice = Some(i18n::tr(lang, "notice.fixture_no_project_short").to_string());
            cx.notify();
            return;
        };
        match state.save(project) {
            Ok(()) => {
                self.project_settings = None;
                self.notice = Some(i18n::tr(lang, "notice.project_settings_saved").to_string());
            }
            Err(error) => {
                if let Some(state) = self.project_settings.as_mut() {
                    state.error =
                        i18n::tf(lang, "project_settings.save_failed", &[("error", &error)]);
                }
            }
        }
        cx.notify();
    }

    fn project_settings_open_yaml(&mut self, cx: &mut Context<Self>) {
        self.project_settings = None;
        if let Err(error) = self.open_document_path("project.yaml", true) {
            self.notice = Some(error);
        }
        cx.notify();
    }

    fn project_settings_overlay(&self, cx: &mut Context<Self>) -> gpui::Div {
        let state = self
            .project_settings
            .as_ref()
            .expect("overlay only when open");
        let field = |id: &'static str, text: String, active: bool| {
            div()
                .id(id)
                .flex_1()
                .px_2()
                .py_1()
                .bg(rgb(theme::explorer_bg()))
                .border_1()
                .border_color(rgb(if active {
                    theme::default_fg()
                } else {
                    theme::explorer_border()
                }))
                .text_color(rgb(theme::default_fg()))
                .child(text)
        };
        let mut kinds = div().flex().flex_wrap().gap_1();
        for (index, kind) in [
            ProjectType::Feature,
            ProjectType::Series,
            ProjectType::WebSeries,
            ProjectType::Short,
            ProjectType::Microdrama,
            ProjectType::Other,
        ]
        .into_iter()
        .enumerate()
        {
            let selected = kind == state.project_type;
            kinds = kinds.child(
                div()
                    .id(("project-kind", index))
                    .px_3()
                    .py_1()
                    .bg(rgb(if selected {
                        theme::doc_selected_bg()
                    } else {
                        theme::explorer_bg()
                    }))
                    .border_1()
                    .border_color(rgb(if selected {
                        theme::default_fg()
                    } else {
                        theme::explorer_border()
                    }))
                    .cursor_pointer()
                    .text_color(rgb(if selected {
                        theme::default_fg()
                    } else {
                        theme::status_fg()
                    }))
                    .child(self.t(project_type_i18n(kind)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(state) = this.project_settings.as_mut() {
                            state.project_type = kind;
                            state.field = ProjectField::Type;
                            state.error.clear();
                        }
                        cx.notify();
                    })),
            );
        }
        let mut screenplay_choices = div().flex().flex_col().gap_1();
        let mut screenplays: Vec<&String> = self
            .explorer
            .iter()
            .filter(|path| FileKind::from_path(path) == FileKind::Fountain)
            .collect();
        screenplays.sort();
        if screenplays.is_empty() {
            screenplay_choices = screenplay_choices.child(
                div()
                    .text_color(rgb(theme::status_fg()))
                    .child(self.t("project_settings.no_screenplays")),
            );
        }
        for (index, path) in screenplays.into_iter().enumerate() {
            let selected = path == &state.default_screenplay;
            let choice = path.clone();
            screenplay_choices = screenplay_choices.child(
                div()
                    .id(("project-screenplay", index))
                    .px_2()
                    .py_1()
                    .bg(rgb(if selected {
                        theme::doc_selected_bg()
                    } else {
                        theme::explorer_bg()
                    }))
                    .text_color(rgb(if selected {
                        theme::default_fg()
                    } else {
                        theme::status_fg()
                    }))
                    .cursor_pointer()
                    .child(choice.clone())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(state) = this.project_settings.as_mut() {
                            state.default_screenplay = choice.clone();
                            state.field = ProjectField::DefaultScreenplay;
                            state.error.clear();
                        }
                        cx.notify();
                    })),
            );
        }
        div()
            .absolute()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000099))
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .on_mouse_up(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .child(
                div()
                    .id("project-settings-card")
                    .w(px(680.0))
                    .max_h(px(640.0))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_4()
                    .bg(rgb(theme::explorer_bg()))
                    .border_1()
                    .border_color(rgb(theme::explorer_border()))
                    .child(
                        div()
                            .text_color(rgb(theme::default_fg()))
                            .child(self.t("project_settings.title")),
                    )
                    .child(
                        div()
                            .text_color(rgb(theme::gutter_fg()))
                            .child(self.t("project_settings.hint")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(120.0))
                                    .text_color(rgb(theme::status_fg()))
                                    .child(self.t("project_settings.name")),
                            )
                            .child(
                                field(
                                    "project-title-field",
                                    format!("{}▌", state.title),
                                    state.field == ProjectField::Title,
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        if let Some(state) = this.project_settings.as_mut() {
                                            state.field = ProjectField::Title;
                                        }
                                        cx.notify();
                                    },
                                )),
                            ),
                    )
                    .child(
                        div()
                            .text_color(rgb(theme::status_fg()))
                            .child(self.t("project_settings.kind")),
                    )
                    .child(kinds)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(120.0))
                                    .text_color(rgb(theme::status_fg()))
                                    .child(self.t("project_settings.primary_language")),
                            )
                            .child(
                                field(
                                    "project-language-field",
                                    format!("{}▌", state.primary_language),
                                    state.field == ProjectField::PrimaryLanguage,
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        if let Some(state) = this.project_settings.as_mut() {
                                            state.field = ProjectField::PrimaryLanguage;
                                        }
                                        cx.notify();
                                    },
                                )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(120.0))
                                    .text_color(rgb(theme::status_fg()))
                                    .child(self.t("project_settings.additional_languages")),
                            )
                            .child(
                                field(
                                    "project-additional-languages",
                                    format!("{}▌", state.additional_languages),
                                    state.field == ProjectField::AdditionalLanguages,
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        if let Some(state) = this.project_settings.as_mut() {
                                            state.field = ProjectField::AdditionalLanguages;
                                        }
                                        cx.notify();
                                    },
                                )),
                            ),
                    )
                    .child(
                        div()
                            .text_color(rgb(theme::gutter_fg()))
                            .child(self.t("project_settings.additional_hint")),
                    )
                    .child(
                        div()
                            .text_color(rgb(theme::status_fg()))
                            .child(self.t("project_settings.default_screenplay")),
                    )
                    .child(screenplay_choices)
                    .when(!state.error.is_empty(), |card| {
                        card.child(
                            div()
                                .text_color(rgb(theme::agent_err_fg()))
                                .child(state.error.clone()),
                        )
                    })
                    .when(!state.status.is_empty(), |card| {
                        card.child(
                            div()
                                .text_color(rgb(theme::status_fg()))
                                .child(state.status.clone()),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                div()
                                    .id("project-settings-save")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(theme::status_bg()))
                                    .text_color(rgb(theme::default_fg()))
                                    .cursor_pointer()
                                    .child(self.t("project_settings.save"))
                                    .on_click(
                                        cx.listener(|this, _, _, cx| {
                                            this.project_settings_save(cx)
                                        }),
                                    ),
                            )
                            .child(
                                div()
                                    .id("project-settings-cancel")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(theme::status_bg()))
                                    .text_color(rgb(theme::status_fg()))
                                    .cursor_pointer()
                                    .child(self.t("project_settings.cancel"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.project_settings = None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id("project-settings-yaml")
                                    .px_3()
                                    .py_1()
                                    .cursor_pointer()
                                    .text_color(rgb(theme::status_fg()))
                                    .child(self.t("project_settings.open_yaml"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.project_settings_open_yaml(cx)
                                    })),
                            ),
                    ),
            )
    }

    // ---- quick open and command palette (M10 S5) ------------------------

    fn on_quick_open(&mut self, _: &QuickOpen, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_palette(PaletteMode::Files, cx);
    }

    fn on_command_palette(
        &mut self,
        _: &CommandPalette,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_palette(PaletteMode::Commands, cx);
    }

    fn open_palette(&mut self, mode: PaletteMode, cx: &mut Context<Self>) {
        if self.unsaved_prompt.is_some()
            || self.settings.is_some()
            || self.project_settings.is_some()
            || self.new_file.is_some()
            || self.new_project.is_some()
            || self.context_menu.is_some()
            || self.rename.is_some()
            || self.delete_prompt.is_some()
            || self.git_commit.is_some()
        {
            return;
        }
        if let Some(state) = self.palette.as_mut()
            && state.mode == mode
        {
            self.palette = None;
            cx.notify();
            return;
        }
        self.palette = Some(PaletteState {
            mode,
            query: String::new(),
            selected: 0,
        });
        cx.notify();
    }

    fn palette_input(&mut self, text: &str, cx: &mut Context<Self>) {
        let cleaned: String = text
            .chars()
            .filter(|c| !c.is_control() || *c == ' ')
            .collect();
        if let Some(state) = self.palette.as_mut() {
            state.query.push_str(&cleaned);
            state.selected = 0;
        }
        cx.notify();
    }

    fn palette_backspace(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.palette.as_mut() {
            if let Some(last) = state.query.chars().last() {
                let len = state.query.len();
                state.query.truncate(len - last.len_utf8());
            }
            state.selected = 0;
        }
        cx.notify();
    }

    fn palette_move(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = match self.palette.as_ref().map(|state| state.mode) {
            Some(PaletteMode::Files) => self.palette_files().len(),
            Some(PaletteMode::Commands) => self.palette_commands().len(),
            None => 0,
        };
        if let Some(state) = self.palette.as_mut()
            && count > 0
        {
            let current = state.selected.min(count - 1) as isize;
            state.selected = (current + delta).clamp(0, count as isize - 1) as usize;
        }
        cx.notify();
    }

    fn palette_files(&self) -> Vec<String> {
        let Some(state) = self.palette.as_ref() else {
            return Vec::new();
        };
        let query = state.query.trim().to_lowercase();
        filter_palette_paths(
            explorer::visible_paths(&self.explorer, self.show_config_files),
            &query,
        )
    }

    fn palette_commands(&self) -> Vec<PaletteCommand> {
        let Some(state) = self.palette.as_ref() else {
            return Vec::new();
        };
        let query = state.query.trim().to_lowercase();
        PaletteCommand::ALL
            .into_iter()
            .filter(|command| {
                query.is_empty() || self.t(command.label_key()).to_lowercase().contains(&query)
            })
            .collect()
    }

    fn palette_activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.palette.as_ref() else {
            return;
        };
        let selected = state.selected;
        let mode = state.mode;
        match mode {
            PaletteMode::Files => {
                let Some(path) = self.palette_files().get(selected).cloned() else {
                    return;
                };
                self.palette = None;
                self.select_path(&path, true, window, cx);
            }
            PaletteMode::Commands => {
                let Some(command) = self.palette_commands().get(selected).copied() else {
                    return;
                };
                self.palette = None;
                self.dispatch_palette_command(command, window, cx);
            }
        }
        cx.notify();
    }

    fn dispatch_palette_command(
        &mut self,
        command: PaletteCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match command {
            PaletteCommand::OpenSettings => {
                self.on_open_settings(&OpenSettings, window, cx);
            }
            PaletteCommand::OpenProjectSettings => {
                self.on_open_project_settings(&OpenProjectSettings, window, cx);
            }
            PaletteCommand::NewFile => self.on_new_file(&NewFile, window, cx),
            PaletteCommand::NewProject => self.on_new_project(&NewProject, window, cx),
            PaletteCommand::OpenProject => self.on_open_project(&OpenProject, window, cx),
            PaletteCommand::Save => self.on_save(&Save, window, cx),
            PaletteCommand::ExportPdf => self.on_export_pdf(&ExportPdf, window, cx),
            PaletteCommand::ToggleExplorer => {
                self.on_toggle_explorer(&ToggleExplorer, window, cx);
            }
            PaletteCommand::ToggleAgentPanel => {
                self.on_toggle_agent_panel(&ToggleAgentPanel, window, cx);
            }
            PaletteCommand::ToggleGitPanel => {
                self.on_toggle_git_panel(&ToggleGitPanel, window, cx);
            }
            PaletteCommand::ToggleConfigFiles => {
                self.on_toggle_config_files(&ToggleConfigFiles, window, cx);
            }
            PaletteCommand::Find => self.on_toggle_find(&ToggleFind, window, cx),
            PaletteCommand::ProjectSearch => {
                self.on_project_search(&ProjectSearch, window, cx);
            }
            PaletteCommand::ToggleDiff => self.on_toggle_diff(&ToggleDiff, window, cx),
            PaletteCommand::TogglePreview => {
                self.on_toggle_preview(&TogglePreview, window, cx);
            }
            PaletteCommand::ReviewScene => self.on_review_scene(&ReviewScene, window, cx),
            PaletteCommand::Propose => self.on_open_propose(&OpenPropose, window, cx),
            PaletteCommand::CloseTab => self.on_close_tab(&CloseTab, window, cx),
            PaletteCommand::NextTab => self.on_next_tab(&NextTab, window, cx),
            PaletteCommand::PrevTab => self.on_prev_tab(&PrevTab, window, cx),
        }
    }

    fn palette_overlay(&self, cx: &mut Context<Self>) -> gpui::Div {
        let state = self.palette.as_ref().expect("overlay only when open");
        let title = match state.mode {
            PaletteMode::Files => self.t("palette.files_title"),
            PaletteMode::Commands => self.t("palette.commands_title"),
        };
        let placeholder = match state.mode {
            PaletteMode::Files => self.t("palette.placeholder_files"),
            PaletteMode::Commands => self.t("palette.placeholder_commands"),
        };
        let mut list = div()
            .id("palette-list")
            .max_h(px(320.0))
            .overflow_y_scroll()
            .flex()
            .flex_col();
        match state.mode {
            PaletteMode::Files => {
                for (index, path) in self.palette_files().iter().enumerate() {
                    let selected = index == state.selected;
                    let kind = FileKind::from_path(path);
                    let path_owned = path.clone();
                    list = list.child(
                        div()
                            .id(("palette-file", index))
                            .px_3()
                            .py_1()
                            .flex()
                            .items_center()
                            .gap_2()
                            .cursor_pointer()
                            .when(selected, |row| row.bg(rgb(theme::doc_selected_bg())))
                            .child(icon(
                                kind.icon_name(),
                                if selected {
                                    theme::default_fg()
                                } else {
                                    theme::status_fg()
                                },
                                14.0,
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_color(rgb(if selected {
                                        theme::default_fg()
                                    } else {
                                        theme::status_fg()
                                    }))
                                    .child(path_owned.clone()),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.palette = None;
                                this.select_path(&path_owned, true, window, cx);
                                cx.notify();
                            })),
                    );
                }
            }
            PaletteMode::Commands => {
                for (index, command) in self.palette_commands().iter().enumerate() {
                    let selected = index == state.selected;
                    let command = *command;
                    list = list.child(
                        div()
                            .id(("palette-command", index))
                            .px_3()
                            .py_1()
                            .flex()
                            .items_center()
                            .gap_2()
                            .cursor_pointer()
                            .when(selected, |row| row.bg(rgb(theme::doc_selected_bg())))
                            .child(
                                div()
                                    .flex_1()
                                    .text_color(rgb(if selected {
                                        theme::default_fg()
                                    } else {
                                        theme::status_fg()
                                    }))
                                    .child(self.t(command.label_key())),
                            )
                            .child(
                                div()
                                    .text_color(rgb(theme::gutter_fg()))
                                    .child(command.shortcut()),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.palette = None;
                                this.dispatch_palette_command(command, window, cx);
                                cx.notify();
                            })),
                    );
                }
            }
        }
        let empty = match state.mode {
            PaletteMode::Files => self.palette_files().is_empty(),
            PaletteMode::Commands => self.palette_commands().is_empty(),
        };
        if empty {
            list = list.child(
                div()
                    .px_3()
                    .py_2()
                    .text_color(rgb(theme::status_fg()))
                    .child(self.t("palette.no_results")),
            );
        }
        div()
            .absolute()
            .size_full()
            .pt(px(72.0))
            .flex()
            .justify_center()
            .items_start()
            .bg(rgba(0x00000080))
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .on_mouse_up(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .child(
                div()
                    .w(px(620.0))
                    .flex()
                    .flex_col()
                    .bg(rgb(theme::explorer_bg()))
                    .border_1()
                    .border_color(rgb(theme::explorer_border()))
                    .child(
                        div()
                            .px_3()
                            .pt_2()
                            .text_sm()
                            .text_color(rgb(theme::status_fg()))
                            .child(title),
                    )
                    .child(
                        div()
                            .px_3()
                            .py_2()
                            .text_color(rgb(theme::default_fg()))
                            .child(format!(
                                "{}▌",
                                if state.query.is_empty() {
                                    placeholder
                                } else {
                                    state.query.as_str()
                                },
                            )),
                    )
                    .child(list)
                    .child(
                        div()
                            .px_3()
                            .py_1()
                            .text_sm()
                            .text_color(rgb(theme::gutter_fg()))
                            .child(self.t("palette.hint")),
                    ),
            )
    }

    // ---- editor context menu (M11 slice 1) ------------------------------

    /// Right-click in the buffer: select the word under the pointer when the
    /// click is outside the current selection, then open the menu. The
    /// selection is preserved when the click lands inside it, so Cut/Copy and
    /// Convert act on exactly what the user highlighted.
    fn open_editor_context_menu(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.doc.diff.open
            || self.unsaved_prompt.is_some()
            || self.settings.is_some()
            || self.project_settings.is_some()
            || self.palette.is_some()
            || self.new_file.is_some()
            || self.new_project.is_some()
        {
            return;
        }
        let Some(offset) = self.offset_for_point(position, window) else {
            return;
        };
        self.open_editor_context_menu_at(offset, position, window, cx);
    }

    /// Build and open the menu for a known document offset. Right-click uses
    /// the hit-tested offset; the keyboard entry point (Shift+F10) uses the
    /// caret and anchors the menu at its line.
    fn open_editor_context_menu_at(
        &mut self,
        offset: usize,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let inside_selection = !self.doc.selected.is_empty()
            && self.doc.selected.start <= offset
            && offset < self.doc.selected.end;
        if !inside_selection {
            let word = self.doc.buffer.word_range_at(offset);
            if word.is_empty() {
                self.set_caret(offset, false);
            } else {
                self.doc.selected = word;
                self.doc.reversed = false;
            }
        }
        let selected = self.doc.buffer.slice(self.doc.selected.clone());
        let has_selection = !selected.is_empty();
        let entity_name = selected.trim();
        let entity_ok = !entity_name.is_empty()
            && !entity_name.contains('\n')
            && new_file::file_stem(entity_name).is_some();
        let t = |key: &str| self.t(key).to_string();
        let mut items = vec![
            ContextMenuItem::action(ContextAction::Cut, t("context.cut"), "⌘X", has_selection),
            ContextMenuItem::action(ContextAction::Copy, t("context.copy"), "⌘C", has_selection),
            ContextMenuItem::action(ContextAction::Paste, t("context.paste"), "⌘V", true),
            ContextMenuItem::action(
                ContextAction::SelectAll,
                t("context.select_all"),
                "⌘A",
                true,
            ),
            ContextMenuItem::separator(),
            ContextMenuItem::action(ContextAction::Find, t("context.find"), "⌘F", true),
            ContextMenuItem::action(
                ContextAction::ProjectSearch,
                t("context.project_search"),
                "⇧⌘F",
                true,
            ),
            ContextMenuItem::separator(),
            ContextMenuItem::action(ContextAction::Propose, t("context.propose"), "⌘K", true),
            ContextMenuItem::separator(),
            ContextMenuItem::action(
                ContextAction::Convert(NewFileKind::Character),
                t("context.convert.character"),
                "",
                entity_ok,
            ),
            ContextMenuItem::action(
                ContextAction::Convert(NewFileKind::Location),
                t("context.convert.location"),
                "",
                entity_ok,
            ),
            ContextMenuItem::action(
                ContextAction::Convert(NewFileKind::Object),
                t("context.convert.object"),
                "",
                entity_ok,
            ),
            ContextMenuItem::separator(),
        ];
        for kind in screenplay::ElementType::ALL {
            items.push(ContextMenuItem::action(
                ContextAction::SetElement(kind),
                t(kind.i18n_key()),
                "",
                true,
            ));
        }
        self.show_context_menu(position, items, ContextTarget::Editor, window, cx);
    }

    /// Clamp a menu to the viewport and make it visible for a target.
    fn show_context_menu(
        &mut self,
        position: Point<Pixels>,
        items: Vec<ContextMenuItem>,
        target: ContextTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let viewport = window.viewport_size();
        let x = f32::from(position.x).min((f32::from(viewport.width) - 290.0).max(8.0));
        let y = f32::from(position.y).min((f32::from(viewport.height) - 560.0).max(8.0));
        self.context_target = Some(target);
        self.context_menu = Some(ContextMenuState::new(x, y, items));
        window.focus(&self.focus_handle(cx));
        cx.notify();
    }

    /// Shift+F10: open the same menu at the caret, selecting the word when
    /// there is no selection. Keyboard parity for the right-click surface.
    fn on_show_context_menu(
        &mut self,
        _: &ShowContextMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.doc.preview {
            return;
        }
        if self.context_menu.is_some() {
            self.context_menu = None;
            cx.notify();
            return;
        }
        let caret = self.caret();
        let pos = self.doc.buffer.position_at(caret);
        let Some(bounds) = self.doc.list_state.bounds_for_item(pos.line) else {
            return;
        };
        let x = f32::from(bounds.left()) + 80.0;
        let y = f32::from(bounds.top()) + 6.0;
        self.open_editor_context_menu_at(caret, point(px(x), px(y)), window, cx);
    }

    /// Right-click on an explorer row: create, rename, duplicate, delete,
    /// reveal or copy the path. Folders only expose safe actions.
    fn open_explorer_context_menu(
        &mut self,
        path: String,
        is_dir: bool,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.unsaved_prompt.is_some()
            || self.settings.is_some()
            || self.project_settings.is_some()
            || self.palette.is_some()
            || self.new_file.is_some()
            || self.new_project.is_some()
            || self.rename.is_some()
            || self.delete_prompt.is_some()
        {
            return;
        }
        let t = |key: &str| self.t(key).to_string();
        let mut items = vec![ContextMenuItem::action(
            ContextAction::FileNew,
            t("context.file.new"),
            "⌘N",
            true,
        )];
        if is_dir {
            items.push(ContextMenuItem::separator());
            items.push(ContextMenuItem::action(
                ContextAction::FileCopyPath,
                t("context.file.copy_path"),
                "",
                true,
            ));
            items.push(ContextMenuItem::action(
                ContextAction::FileReveal,
                t("context.file.reveal"),
                "",
                true,
            ));
        } else {
            items.push(ContextMenuItem::separator());
            items.push(ContextMenuItem::action(
                ContextAction::FileRename,
                t("context.file.rename"),
                "",
                true,
            ));
            items.push(ContextMenuItem::action(
                ContextAction::FileDuplicate,
                t("context.file.duplicate"),
                "",
                true,
            ));
            items.push(ContextMenuItem::action(
                ContextAction::FileCopyPath,
                t("context.file.copy_path"),
                "",
                true,
            ));
            items.push(ContextMenuItem::action(
                ContextAction::FileReveal,
                t("context.file.reveal"),
                "",
                true,
            ));
            items.push(ContextMenuItem::separator());
            items.push(ContextMenuItem::action(
                ContextAction::FileDelete,
                t("context.file.delete"),
                "",
                true,
            ));
        }
        let target = if is_dir {
            ContextTarget::Dir(path)
        } else {
            ContextTarget::File(path)
        };
        self.show_context_menu(position, items, target, window, cx);
    }

    /// Right-click on a tab: close, close others, copy its path or reveal it.
    fn open_tab_context_menu(
        &mut self,
        index: usize,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if index >= self.tabs.len()
            || self.unsaved_prompt.is_some()
            || self.settings.is_some()
            || self.project_settings.is_some()
            || self.palette.is_some()
            || self.rename.is_some()
            || self.delete_prompt.is_some()
        {
            return;
        }
        let t = |key: &str| self.t(key).to_string();
        let items = vec![
            ContextMenuItem::action(ContextAction::TabClose, t("context.tab.close"), "⌘W", true),
            ContextMenuItem::action(
                ContextAction::TabCloseOthers,
                t("context.tab.close_others"),
                "",
                self.tabs.len() > 1,
            ),
            ContextMenuItem::separator(),
            ContextMenuItem::action(
                ContextAction::TabCopyPath,
                t("context.file.copy_path"),
                "",
                true,
            ),
            ContextMenuItem::action(ContextAction::TabReveal, t("context.file.reveal"), "", true),
        ];
        self.show_context_menu(position, items, ContextTarget::Tab(index), window, cx);
    }

    fn target_path(&self) -> Option<String> {
        match self.context_target.as_ref()? {
            ContextTarget::File(path) | ContextTarget::Dir(path) => Some(path.clone()),
            ContextTarget::Tab(index) => Some(self.tab_path(*index).to_string()),
            ContextTarget::Editor => None,
        }
    }

    fn begin_rename(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.target_path() else {
            return;
        };
        if matches!(self.context_target, Some(ContextTarget::Dir(_))) {
            return;
        }
        let (_, stem, _) = split_path_name(&path);
        let name = stem.to_string();
        self.rename = Some(FilePromptState {
            path,
            name,
            error: String::new(),
        });
        cx.notify();
    }

    fn begin_delete(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.target_path() else {
            return;
        };
        let name = path.rsplit('/').next().unwrap_or(&path).to_string();
        self.delete_prompt = Some(DeletePromptState {
            path,
            name,
            error: String::new(),
        });
        cx.notify();
    }

    fn rename_input(&mut self, text: &str, cx: &mut Context<Self>) {
        if let Some(state) = self.rename.as_mut() {
            let cleaned: String = text
                .chars()
                .filter(|c| !c.is_control() || *c == ' ')
                .collect();
            state.name.push_str(&cleaned);
            state.error.clear();
        }
        cx.notify();
    }

    fn rename_backspace(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.rename.as_mut() {
            if let Some(last) = state.name.chars().last() {
                let len = state.name.len();
                state.name.truncate(len - last.len_utf8());
            }
            state.error.clear();
        }
        cx.notify();
    }

    fn rename_save(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.rename.clone() else {
            return;
        };
        let Some(project) = self.project.clone() else {
            self.notice = Some(self.t("notice.fixture_no_project").to_string());
            cx.notify();
            return;
        };
        let (parent, stem, ext) = split_path_name(&state.path);
        let Some(new_stem) = new_file::file_stem(&state.name) else {
            let message = self.t("rename.invalid").to_string();
            if let Some(state) = self.rename.as_mut() {
                state.error = message;
            }
            cx.notify();
            return;
        };
        if new_stem == stem {
            self.rename = None;
            cx.notify();
            return;
        }
        let target = if parent.is_empty() {
            format!("{new_stem}{ext}")
        } else {
            format!("{parent}/{new_stem}{ext}")
        };
        let mut project = project;
        match project.rename_document(&state.path, &target) {
            Ok(()) => {
                self.project = Some(project);
                self.rewrite_open_paths(&state.path, &target);
                self.rename = None;
                if let Some(project) = self.project.as_ref()
                    && let Ok(paths) = project.list_documents()
                {
                    self.explorer = paths;
                }
                self.refresh_entities();
                self.git_refreshed_at = None;
                self.notice =
                    Some(self.tf("notice.renamed", &[("from", &state.path), ("to", &target)]));
            }
            Err(error) => {
                let message = self.tf("notice.file_op_failed", &[("error", &error.to_string())]);
                if let Some(state) = self.rename.as_mut() {
                    state.error = message;
                }
            }
        }
        cx.notify();
    }

    fn duplicate_target_file(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.target_path() else {
            return;
        };
        let Some(target) = self.duplicate_path(&path) else {
            self.notice = Some(self.t("notice.file_op_failed_no_name").to_string());
            cx.notify();
            return;
        };
        let Some(project) = self.project.clone() else {
            self.notice = Some(self.t("notice.fixture_no_project").to_string());
            cx.notify();
            return;
        };
        match project.duplicate_document(&path, &target) {
            Ok(()) => {
                if let Some(project) = self.project.as_ref()
                    && let Ok(paths) = project.list_documents()
                {
                    self.explorer = paths;
                }
                self.refresh_entities();
                self.git_refreshed_at = None;
                self.notice =
                    Some(self.tf("notice.duplicated", &[("from", &path), ("to", &target)]));
            }
            Err(error) => {
                self.notice =
                    Some(self.tf("notice.file_op_failed", &[("error", &error.to_string())]));
            }
        }
        cx.notify();
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.delete_prompt.clone() else {
            return;
        };
        if self
            .find_tab(&prompt.path)
            .is_some_and(|index| self.tab_is_dirty(index))
        {
            let message = self.t("delete.dirty").to_string();
            if let Some(prompt) = self.delete_prompt.as_mut() {
                prompt.error = message;
            }
            cx.notify();
            return;
        }
        let Some(project) = self.project.as_ref() else {
            self.notice = Some(self.t("notice.fixture_no_project").to_string());
            cx.notify();
            return;
        };
        match project.delete_document(&prompt.path) {
            Ok(()) => {
                // Close every clean tab that referenced the deleted file,
                // without selecting a different document for the user.
                while let Some(index) = self.find_tab(&prompt.path) {
                    self.close_tab_at(index);
                }
                self.delete_prompt = None;
                if let Some(project) = self.project.as_ref()
                    && let Ok(paths) = project.list_documents()
                {
                    self.explorer = paths;
                }
                self.refresh_entities();
                self.git_refreshed_at = None;
                self.notice = Some(self.tf("notice.deleted", &[("path", &prompt.path)]));
            }
            Err(error) => {
                let message = self.tf("notice.file_op_failed", &[("error", &error.to_string())]);
                if let Some(prompt) = self.delete_prompt.as_mut() {
                    prompt.error = message;
                }
            }
        }
        cx.notify();
    }

    fn close_other_tabs(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.tabs.len() {
            return;
        }
        let dirty_other = (0..self.tabs.len())
            .filter(|candidate| *candidate != index)
            .any(|candidate| self.tab_is_dirty(candidate));
        if dirty_other {
            self.notice = Some(self.t("notice.close_others_dirty").to_string());
            cx.notify();
            return;
        }
        if index != self.active_tab && !self.activate_tab(index) {
            return;
        }
        while self.tabs.len() > 1 {
            let victim = if self.active_tab == 0 { 1 } else { 0 };
            self.close_tab_at(victim);
        }
        window.focus(&self.focus_handle(cx));
        cx.notify();
    }

    fn reveal_target(&mut self) {
        let Some(path) = self.target_path() else {
            return;
        };
        let Some(project) = self.project.as_ref() else {
            self.notice = Some(self.t("notice.fixture_no_project").to_string());
            return;
        };
        let absolute = project.root().join(&path);
        match std::process::Command::new("open")
            .arg("-R")
            .arg(&absolute)
            .spawn()
        {
            Ok(_) => {
                self.notice = Some(self.tf("notice.revealed", &[("path", &path)]));
            }
            Err(error) => {
                self.notice =
                    Some(self.tf("notice.reveal_failed", &[("error", &error.to_string())]));
            }
        }
    }

    fn copy_target_path(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = self.target_path() {
            cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
            self.notice = Some(self.tf("notice.path_copied", &[("path", &path)]));
            cx.notify();
        }
    }

    /// Rewrite the active buffer and every inactive tab when a rename changes
    /// their path. Content and dirty state are unchanged.
    fn rewrite_open_paths(&mut self, from: &str, to: &str) {
        if self.doc.path == from {
            self.doc.path = to.to_string();
        }
        for tab in &mut self.tabs {
            if let Some(doc) = tab.document.as_mut()
                && doc.path == from
            {
                doc.path = to.to_string();
            }
        }
    }

    fn duplicate_path(&self, path: &str) -> Option<String> {
        let (parent, stem, ext) = split_path_name(path);
        for copy in 0..100 {
            let suffix = if copy == 0 {
                " copia".to_string()
            } else {
                format!(" copia {}", copy + 1)
            };
            let candidate = if parent.is_empty() {
                format!("{stem}{suffix}{ext}")
            } else {
                format!("{parent}/{stem}{suffix}{ext}")
            };
            if !self.explorer.iter().any(|existing| existing == &candidate) {
                return Some(candidate);
            }
        }
        None
    }

    fn context_menu_move(&mut self, delta: isize, cx: &mut Context<Self>) {
        if let Some(menu) = self.context_menu.as_mut() {
            menu.move_selection(delta);
        }
        cx.notify();
    }

    fn context_menu_activate_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(action) = self
            .context_menu
            .as_ref()
            .and_then(ContextMenuState::activate_selected)
        else {
            return;
        };
        self.context_activate(action, window, cx);
    }

    fn context_activate(
        &mut self,
        action: ContextAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.context_menu = None;
        match action {
            ContextAction::Cut => self.on_cut(&CutText, window, cx),
            ContextAction::Copy => self.on_copy(&CopyText, window, cx),
            ContextAction::Paste => self.on_paste(&PasteText, window, cx),
            ContextAction::SelectAll => self.on_select_all(&SelectAll, window, cx),
            ContextAction::Find => self.on_toggle_find(&ToggleFind, window, cx),
            ContextAction::ProjectSearch => self.on_project_search(&ProjectSearch, window, cx),
            ContextAction::Propose => self.on_open_propose(&OpenPropose, window, cx),
            ContextAction::Convert(kind) => self.context_convert_entity(kind, cx),
            ContextAction::SetElement(kind) => self.context_set_element(kind, cx),
            ContextAction::FileNew => self.open_new_file(cx),
            ContextAction::FileRename => self.begin_rename(cx),
            ContextAction::FileDuplicate => self.duplicate_target_file(cx),
            ContextAction::FileDelete => self.begin_delete(cx),
            ContextAction::FileReveal => self.reveal_target(),
            ContextAction::FileCopyPath => self.copy_target_path(cx),
            ContextAction::TabClose => {
                if let Some(ContextTarget::Tab(index)) = self.context_target.clone() {
                    self.request_close_tab(index, window, cx);
                }
            }
            ContextAction::TabCloseOthers => {
                if let Some(ContextTarget::Tab(index)) = self.context_target.clone() {
                    self.close_other_tabs(index, window, cx);
                }
            }
            ContextAction::TabCopyPath => self.copy_target_path(cx),
            ContextAction::TabReveal => self.reveal_target(),
        }
        self.context_target = None;
    }

    /// Convert the selected word/selection into a bible dossier. Creating a
    /// dossier is an explicit canonical write; if the file already exists the
    /// menu opens it instead of overwriting anything.
    fn context_convert_entity(&mut self, kind: NewFileKind, cx: &mut Context<Self>) {
        if !kind.is_entity() {
            return;
        }
        let name = self
            .doc
            .buffer
            .slice(self.doc.selected.clone())
            .trim()
            .to_string();
        if name.is_empty() || name.contains('\n') {
            self.notice = Some(self.t("notice.entity_invalid").to_string());
            cx.notify();
            return;
        }
        let Some(path) = new_file::target_path(kind, &name) else {
            self.notice = Some(self.t("notice.entity_invalid").to_string());
            cx.notify();
            return;
        };
        let Some(project) = self.project.as_ref() else {
            self.notice = Some(self.t("notice.fixture_no_project_short").to_string());
            cx.notify();
            return;
        };
        if !self.explorer.iter().any(|existing| existing == &path) {
            let template = new_file::template(kind, &name);
            if let Err(error) = project.create_document(&path, &template) {
                self.notice = Some(self.tf(
                    "notice.entity_create_failed",
                    &[("error", &error.to_string())],
                ));
                cx.notify();
                return;
            }
        }
        if let Some(project) = self.project.as_ref()
            && let Ok(paths) = project.list_documents()
        {
            self.explorer = paths;
        }
        self.refresh_entities();
        if let Err(error) = self.open_document_path(&path, true) {
            self.notice = Some(error);
            cx.notify();
            return;
        }
        self.notice = Some(self.tf(
            "notice.entity_opened",
            &[("kind", self.t(kind.i18n_key())), ("path", &path)],
        ));
        cx.notify();
    }

    /// Apply a specific Fountain element type to every line touched by the
    /// selection (or to the caret line). Uses the same forced markers as the
    /// Tab cycle, so the text is preserved.
    fn context_set_element(&mut self, kind: screenplay::ElementType, cx: &mut Context<Self>) {
        let caret = self.caret();
        let start_line = self.doc.buffer.position_at(self.doc.selected.start).line;
        let end_line = if self.doc.selected.is_empty() {
            self.doc.buffer.position_at(caret).line
        } else {
            self.doc
                .buffer
                .position_at(self.doc.selected.end.saturating_sub(1))
                .line
        };
        let start = self.doc.buffer.line_content_range(start_line).start;
        let end = self.doc.buffer.line_content_range(end_line).end;
        let range = start..end;
        let old = self.doc.buffer.slice(range.clone());
        let new_text = old
            .split('\n')
            .map(|line| screenplay::apply(line, kind))
            .collect::<Vec<_>>()
            .join("\n");
        if new_text == old {
            return;
        }
        let col = caret.saturating_sub(start).min(new_text.len());
        self.apply_edit(range.clone(), &new_text);
        self.set_caret((range.start + col).min(self.doc.buffer.doc_len()), false);
        self.notice = Some(self.t("notice.element_applied").to_string());
        cx.notify();
        self.ensure_caret_visible();
    }

    fn context_menu_overlay(&self, cx: &mut Context<Self>) -> gpui::Div {
        let menu = self.context_menu.as_ref().expect("overlay only when open");
        let menu_height = menu
            .items
            .iter()
            .map(|item| if item.action.is_none() { 9.0 } else { 30.0 })
            .sum::<f32>()
            + 8.0;
        let (menu_x, menu_y) = (menu.x, menu.y);
        let mut rows = div()
            .id("context-menu-items")
            .max_h(px(560.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .py_1();
        for (index, item) in menu.items.iter().enumerate() {
            if item.action.is_none() {
                rows = rows.child(
                    div()
                        .mx_2()
                        .my_1()
                        .h(px(1.0))
                        .bg(rgb(theme::explorer_border())),
                );
                continue;
            }
            let selected = index == menu.selected;
            let enabled = item.enabled;
            let text_color = if !enabled {
                theme::gutter_fg()
            } else if selected {
                theme::default_fg()
            } else {
                theme::status_fg()
            };
            let action = item.action.expect("separator handled above");
            rows = rows.child(
                div()
                    .id(("context-item", index))
                    .mx_1()
                    .px_2()
                    .py_1()
                    .flex()
                    .items_center()
                    .gap_2()
                    .cursor_pointer()
                    .when(selected && enabled, |row| {
                        row.bg(rgb(theme::doc_selected_bg()))
                    })
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .text_color(rgb(text_color))
                            .child(item.label.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(theme::gutter_fg()))
                            .child(item.shortcut),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            if enabled {
                                this.context_activate(action, window, cx);
                            }
                        }),
                    ),
            );
        }
        div()
            .absolute()
            .size_full()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    let x = f32::from(event.position.x);
                    let y = f32::from(event.position.y);
                    let inside = x >= menu_x
                        && x <= menu_x + 280.0
                        && y >= menu_y
                        && y <= menu_y + menu_height;
                    if !inside {
                        this.context_menu = None;
                        cx.notify();
                    }
                }),
            )
            .child(
                div()
                    .id("context-menu")
                    .absolute()
                    .left(px(menu.x))
                    .top(px(menu.y))
                    .w(px(280.0))
                    .flex()
                    .flex_col()
                    .bg(rgb(theme::explorer_bg()))
                    .border_1()
                    .border_color(rgb(theme::explorer_border()))
                    .child(rows),
            )
    }

    fn rename_overlay(&self, cx: &mut Context<Self>) -> gpui::Div {
        let state = self.rename.as_ref().expect("overlay only when open");
        let (_, _, ext) = split_path_name(&state.path);
        div()
            .absolute()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000099))
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .on_mouse_up(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .child(
                div()
                    .w(px(620.0))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_4()
                    .bg(rgb(theme::explorer_bg()))
                    .border_1()
                    .border_color(rgb(theme::explorer_border()))
                    .child(
                        div()
                            .text_color(rgb(theme::default_fg()))
                            .child(self.t("rename.title")),
                    )
                    .child(
                        div()
                            .text_color(rgb(theme::status_fg()))
                            .child(self.tf("rename.current", &[("path", &state.path)])),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(100.0))
                                    .text_color(rgb(theme::status_fg()))
                                    .child(self.t("rename.name")),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .px_2()
                                    .py_1()
                                    .bg(rgb(theme::bg()))
                                    .border_1()
                                    .border_color(rgb(theme::default_fg()))
                                    .text_color(rgb(theme::default_fg()))
                                    .child(format!("{}▌", state.name)),
                            )
                            .child(
                                div()
                                    .text_color(rgb(theme::gutter_fg()))
                                    .child(ext.to_string()),
                            ),
                    )
                    .child(
                        div()
                            .text_color(rgb(theme::gutter_fg()))
                            .child(self.t("rename.hint")),
                    )
                    .when(!state.error.is_empty(), |card| {
                        card.child(
                            div()
                                .text_color(rgb(theme::agent_err_fg()))
                                .child(state.error.clone()),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                div()
                                    .id("rename-save")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(theme::status_bg()))
                                    .text_color(rgb(theme::default_fg()))
                                    .cursor_pointer()
                                    .child(self.t("rename.save"))
                                    .on_click(cx.listener(|this, _, _, cx| this.rename_save(cx))),
                            )
                            .child(
                                div()
                                    .id("rename-cancel")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(theme::status_bg()))
                                    .text_color(rgb(theme::status_fg()))
                                    .cursor_pointer()
                                    .child(self.t("rename.cancel"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.rename = None;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
    }

    fn delete_overlay(&self, cx: &mut Context<Self>) -> gpui::Div {
        let prompt = self.delete_prompt.as_ref().expect("overlay only when open");
        div()
            .absolute()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000099))
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .on_mouse_up(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .child(
                div()
                    .w(px(560.0))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_4()
                    .bg(rgb(theme::explorer_bg()))
                    .border_1()
                    .border_color(rgb(theme::agent_err_fg()))
                    .child(
                        div()
                            .text_color(rgb(theme::default_fg()))
                            .child(self.t("delete.title")),
                    )
                    .child(
                        div()
                            .text_color(rgb(theme::status_fg()))
                            .child(self.tf("delete.body", &[("name", &prompt.name)])),
                    )
                    .child(
                        div()
                            .text_color(rgb(theme::gutter_fg()))
                            .child(self.t("delete.hint")),
                    )
                    .when(!prompt.error.is_empty(), |card| {
                        card.child(
                            div()
                                .text_color(rgb(theme::agent_err_fg()))
                                .child(prompt.error.clone()),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                div()
                                    .id("delete-confirm")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(theme::agent_err_fg()))
                                    .text_color(rgb(theme::default_fg()))
                                    .cursor_pointer()
                                    .child(self.t("delete.confirm"))
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.confirm_delete(cx)),
                                    ),
                            )
                            .child(
                                div()
                                    .id("delete-cancel")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(theme::status_bg()))
                                    .text_color(rgb(theme::status_fg()))
                                    .cursor_pointer()
                                    .child(self.t("delete.cancel"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.delete_prompt = None;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
    }

    fn caret(&self) -> usize {
        if self.doc.reversed {
            self.doc.selected.start
        } else {
            self.doc.selected.end
        }
    }

    /// True when a modal dialog owns the keyboard. Caret movement and edits
    /// must not run behind it (M17 slice 0); text fields route their own
    /// input before reaching the buffer.
    fn modal_owns_keyboard(&self) -> bool {
        self.settings.is_some()
            || self.project_settings.is_some()
            || self.unsaved_prompt.is_some()
            || self.new_file.is_some()
            || self.new_project.is_some()
            || self.rename.is_some()
            || self.delete_prompt.is_some()
            || self.palette.is_some()
            || self.git_branch_prompt.is_some()
            || self.github_publish.is_some()
            || self.github_pr.is_some()
            || self.git_commit.is_some()
            || self.git_diff.is_some()
            || self.git_restore.is_some()
            || self.git_sync.is_some()
    }

    fn set_caret(&mut self, target: usize, extend: bool) {
        if self.modal_owns_keyboard() {
            return;
        }
        if extend {
            if self.doc.reversed {
                self.doc.selected.start = target;
                if self.doc.selected.start > self.doc.selected.end {
                    let (start, end) = (self.doc.selected.end, self.doc.selected.start);
                    self.doc.selected = start..end;
                    self.doc.reversed = false;
                }
            } else {
                self.doc.selected.end = target;
                if self.doc.selected.end < self.doc.selected.start {
                    let (start, end) = (self.doc.selected.end, self.doc.selected.start);
                    self.doc.selected = start..end;
                    self.doc.reversed = true;
                }
            }
        } else {
            self.doc.selected = target..target;
            self.doc.reversed = false;
        }
    }

    /// Doc offset under a window-coordinate point (mouse hit-test), or `None`
    /// when the point is above the list (status/find bar chrome). Item bounds
    /// come from `ListState::bounds_for_item` — window coordinates with scroll
    /// already applied, `Some` only for laid-out items — so the scan walks from
    /// the top and keeps the nearest measured row at or above the point.
    /// Measured rows are contiguous around the viewport, so the walk is bounded
    /// by the scroll position; it runs once per click, never per frame.
    fn offset_for_point(&self, position: Point<Pixels>, window: &Window) -> Option<usize> {
        let mut best: Option<(usize, Bounds<Pixels>)> = None;
        for ix in 0..self.doc.buffer.line_count() {
            let Some(bounds) = self.doc.list_state.bounds_for_item(ix) else {
                continue;
            };
            if bounds.top() > position.y {
                break;
            }
            best = Some((ix, bounds));
            if position.y < bounds.bottom() {
                break;
            }
        }
        let (line, bounds) = best?;
        // Below the last measured row: clamp to that row's content end (the
        // last line's end is the document end).
        if position.y >= bounds.bottom() {
            return Some(self.doc.buffer.line_content_range(line).end);
        }
        let text = &self.doc.buffer.lines()[line];
        let start = self.doc.buffer.line_content_range(line).start;
        if text.is_empty() {
            return Some(start);
        }
        let shaped = shape_line(window, text);
        let glyphs: Vec<(usize, f32)> = shaped
            .runs
            .iter()
            .flat_map(|run| {
                run.glyphs
                    .iter()
                    .map(|g| (g.index, f32::from(g.position.x)))
            })
            .collect();
        let wrap_width = f32::from(
            (bounds.size.width - px(ROW_PADDING_X * 2.0 + theme::gutter_width())).max(px(0.0)),
        );
        let rows = wrap_row_starts(&glyphs, wrap_width);
        let visual_row = (((position.y - bounds.top()) / px(theme::line_height())).floor()
            as usize)
            .min(rows.len() - 1);
        // Glyph x positions are absolute on the un-wrapped line, but wrapping
        // restarts each visual row at x = 0: shift the probe by the x of the
        // row's first glyph.
        let row_start_x = glyphs
            .iter()
            .find(|(index, _)| *index >= rows[visual_row])
            .map(|(_, x)| *x)
            .unwrap_or(0.0);
        let x = f32::from(position.x - bounds.left() - px(ROW_PADDING_X + theme::gutter_width()))
            + row_start_x;
        let col = shaped.closest_index_for_x(px(x)).min(text.len());
        // Snap down to a grapheme boundary: a combining mark's glyph index can
        // land inside one.
        let offset = start + col;
        let snapped = self.doc.buffer.prev_boundary(offset);
        let offset = if self.doc.buffer.next_boundary(snapped) == offset {
            offset
        } else {
            snapped
        };
        Some(offset.min(self.doc.buffer.doc_len()))
    }

    /// Click-to-caret (Spike B): a plain click moves the caret, shift-click
    /// extends the selection from the active end (`set_caret` handles the
    /// crossing flip). The diff view is read-only, so clicks there do nothing.
    fn on_buffer_click(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Record debug for every left click (even on diff), so the dump shows
        // what coordinates the handler received and what it resolved.
        if event.button == MouseButton::Left {
            self.doc.last_click = Some((
                f32::from(event.position.x),
                f32::from(event.position.y),
                None,
                None,
                0.0,
                None,
            ));
        }
        if event.button == MouseButton::Right {
            self.open_editor_context_menu(event.position, window, cx);
            return;
        }
        if self.doc.diff.open || event.button != MouseButton::Left {
            return;
        }
        self.chat.focused = false;
        let Some(offset) = self.offset_for_point(event.position, window) else {
            return;
        };
        // ⌘-click on a bible mention opens its dossier (M11). A click that
        // misses an entity keeps the normal caret behavior below.
        if event.modifiers.platform {
            let pos = self.doc.buffer.position_at(offset);
            let line = self.doc.buffer.lines()[pos.line].clone();
            if let Some(mention) = self
                .entities
                .mentions(&line)
                .into_iter()
                .find(|mention| pos.col >= mention.start && pos.col <= mention.end)
            {
                let document = mention.document(&self.entities).to_string();
                let name = mention.name(&self.entities).to_string();
                if let Err(error) = self.open_document_path(&document, true) {
                    self.notice = Some(error);
                } else {
                    self.notice = Some(self.tf(
                        "notice.entity_clicked",
                        &[("name", &name), ("path", &document)],
                    ));
                }
                window.focus(&self.focus_handle(cx));
                cx.notify();
                return;
            }
        }
        // Update debug with resolved line and bounds.
        let pos = self.doc.buffer.position_at(offset);
        let line = pos.line;
        let bounds = self.doc.list_state.bounds_for_item(line);
        let (item_top, item_h) = match bounds {
            Some(b) => (Some(f32::from(b.top())), f32::from(b.size.height)),
            None => (None, 0.0),
        };
        self.doc.last_click = Some((
            f32::from(event.position.x),
            f32::from(event.position.y),
            Some(line),
            item_top,
            item_h,
            Some(offset),
        ));
        // Double click selects the word, triple click the line (M9 slice 1);
        // a plain click moves the caret and arms the drag anchor.
        match event.click_count {
            2 => {
                self.doc.selected = self.doc.buffer.word_range_at(offset);
                self.doc.reversed = false;
                self.doc.dragging = Some(offset);
            }
            count if count >= 3 => {
                self.doc.selected = self.doc.buffer.line_range_at(offset);
                self.doc.reversed = false;
                self.doc.dragging = Some(offset);
            }
            _ => {
                self.set_caret(offset, event.modifiers.shift);
                self.doc.dragging = if event.modifiers.shift {
                    None
                } else {
                    Some(offset)
                };
            }
        }
        // A click re-targets typing (and IME composition) at the buffer.
        window.focus(&self.focus_handle(cx));
        cx.notify();
    }

    /// Drag selection (M9 slice 1): while the left button is held, the
    /// selection runs from the mouse-down anchor to the current point (the
    /// caret stays at the moving end). The list auto-scrolls one line when
    /// the pointer reaches the top/bottom edge so a drag can cross pages.
    fn on_mouse_drag(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.doc.diff.open || event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        let Some(anchor) = self.doc.dragging else {
            return;
        };
        let viewport = self.doc.list_state.viewport_bounds();
        let edge = px(theme::line_height());
        if event.position.y < viewport.top() + edge {
            self.doc.list_state.scroll_by(px(-theme::line_height()));
        } else if event.position.y > viewport.bottom() - edge {
            self.doc.list_state.scroll_by(px(theme::line_height()));
        }
        let Some(offset) = self.offset_for_point(event.position, window) else {
            return;
        };
        if offset >= anchor {
            self.doc.selected = anchor..offset;
            self.doc.reversed = false;
        } else {
            self.doc.selected = offset..anchor;
            self.doc.reversed = true;
        }
        cx.notify();
    }

    /// End a drag selection: the anchor is cleared, the selection stays.
    fn on_mouse_release(
        &mut self,
        _: &MouseUpEvent,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        self.doc.dragging = None;
    }

    /// Keep the `list` item count in sync with the buffer after an edit that
    /// replaced `deleted` (the removed text) with `inserted` starting at
    /// `start_offset`. Uses `splice`, not `reset`, so the scroll position is
    /// preserved across keystrokes (reset would jump to the top).
    fn splice_for_edit(&self, start_offset: usize, deleted: &str, inserted: &str) {
        let start_line = self.doc.buffer.position_at(start_offset).line;
        let old_span = deleted.matches('\n').count() + 1;
        let new_span = inserted.matches('\n').count() + 1;
        self.doc
            .list_state
            .splice(start_line..start_line + old_span, new_span);
    }

    fn clamp(&self, range: Range<usize>) -> Range<usize> {
        let len = self.doc.buffer.doc_len();
        let start = range.start.min(len);
        let end = range.end.min(len).max(start);
        start..end
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.doc.buffer.utf16_to_offset(range.start)..self.doc.buffer.utf16_to_offset(range.end)
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.doc.buffer.offset_to_utf16(range.start)..self.doc.buffer.offset_to_utf16(range.end)
    }

    /// Bump the document revision and pin the active preview tab: a preview
    /// tab is never dirty, so the first edit promotes it (D-041).
    fn bump_revision(&mut self) {
        self.doc.revision = self.doc.revision.saturating_add(1);
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.preview = false;
        }
    }

    /// Apply an edit, record undo state, move the caret after the inserted
    /// text and invalidate the derived scene index.
    fn apply_edit(&mut self, range: Range<usize>, text: &str) -> Range<usize> {
        if self.modal_owns_keyboard() {
            return range;
        }
        let start = range.start;
        let (inserted, deleted) = self.doc.buffer.replace(range.clone(), text);
        self.splice_for_edit(start, &deleted, text);
        self.metrics.borrow_mut().edits += 1;
        self.bump_revision();
        // Undo bookkeeping goes through `record_edit` so IME composition
        // updates collapse into the single open entry (one undo per session).
        record_edit(
            &mut self.doc.undo_stack,
            self.doc.composing,
            range,
            inserted.clone(),
            text,
            deleted,
        );
        const MAX_UNDO: usize = 5000;
        if self.doc.undo_stack.len() > MAX_UNDO {
            self.doc.undo_stack.remove(0);
        }
        self.doc.redo_stack.clear();
        self.doc.selected = inserted.end..inserted.end;
        self.doc.reversed = false;
        self.doc.marked = None;
        self.doc.scenes_dirty = true;
        self.doc.elements_dirty = true;
        self.doc.pages_dirty = true;
        self.doc.pages_generation += 1;
        // Offsets shifted: keep find matches valid (silent — edits are not
        // query submissions and must not pollute the search baseline).
        if self.doc.find.open {
            self.refresh_matches(false);
        }
        inserted
    }

    fn after_history_edit(&mut self, caret: usize) {
        self.metrics.borrow_mut().edits += 1;
        self.bump_revision();
        self.doc.selected = caret..caret;
        self.doc.reversed = false;
        self.doc.marked = None;
        self.doc.composing = false;
        self.doc.scenes_dirty = true;
        self.doc.elements_dirty = true;
        self.doc.pages_dirty = true;
        self.doc.pages_generation += 1;
        if self.doc.find.open {
            self.refresh_matches(false);
        }
    }

    fn undo(&mut self) {
        if self.modal_owns_keyboard() {
            return;
        }
        let Some(edit) = self.doc.undo_stack.pop() else {
            return;
        };
        let inserted = edit.range.start..edit.range.start + edit.new_text.len();
        let start = inserted.start;
        let (restored, deleted) = self.doc.buffer.replace(inserted, &edit.old_text);
        self.splice_for_edit(start, &deleted, &edit.old_text);
        self.doc.redo_stack.push(Edit {
            range: restored.clone(),
            new_text: edit.old_text.clone(),
            old_text: deleted,
            composition: false,
        });
        self.after_history_edit(restored.end);
    }

    fn redo(&mut self) {
        if self.modal_owns_keyboard() {
            return;
        }
        let Some(edit) = self.doc.redo_stack.pop() else {
            return;
        };
        let current = edit.range.start..edit.range.start + edit.new_text.len();
        let start = current.start;
        let (restored, deleted) = self.doc.buffer.replace(current, &edit.old_text);
        self.splice_for_edit(start, &deleted, &edit.old_text);
        self.doc.undo_stack.push(Edit {
            range: restored.clone(),
            new_text: edit.old_text.clone(),
            old_text: deleted,
            composition: false,
        });
        self.after_history_edit(restored.end);
    }

    fn refresh_scenes(&mut self) {
        if !self.doc.scenes_dirty {
            return;
        }
        // Fountain keeps the cheap M8 scene scan: a full fountain::parse on
        // the 19k-line fixture costs ~2.2 ms, which would put a full parse on
        // every keystroke. The language contract still exposes Fountain
        // outline/symbols/diagnostics for a future debounced pass.
        if FileKind::from_path(&self.doc.path) == FileKind::Fountain {
            self.doc.outline = self
                .doc
                .buffer
                .lines()
                .iter()
                .enumerate()
                .filter(|(_, line)| is_scene_heading(line))
                .map(|(ix, line)| (ix, line.trim().to_string()))
                .collect();
            self.doc.outline_lines = self.doc.outline.iter().map(|(line, _)| *line).collect();
            self.doc.outline_levels = vec![1; self.doc.outline.len()];
            self.doc.scene_lines = self.doc.outline_lines.clone();
            self.doc.language = LanguageReport::empty(LanguageId::Fountain);
            self.doc.scenes_dirty = false;
            return;
        }
        let text = self.doc.buffer.slice(0..self.doc.buffer.doc_len());
        let report = language::analyze(&self.doc.path, &text);
        let mut outline = Vec::new();
        let mut outline_lines = Vec::new();
        let mut outline_levels = Vec::new();
        for item in &report.outline {
            outline.push((item.line, item.title.clone()));
            outline_lines.push(item.line);
            outline_levels.push(item.level);
        }
        self.doc.scene_lines = Vec::new();
        self.doc.outline = outline;
        self.doc.outline_lines = outline_lines;
        self.doc.outline_levels = outline_levels;
        self.doc.language = report;
        self.doc.scenes_dirty = false;
    }

    /// UI copy for known diagnostic codes; unknown diagnostics keep the
    /// parser/provider detail as a technical fallback.
    fn diagnostic_text(&self, diagnostic: &Diagnostic) -> String {
        match diagnostic.code.as_str() {
            "fountain.empty_document" => self.t("diagnostic.fountain.empty_document").to_string(),
            "fountain.unclosed_boneyard" => {
                self.t("diagnostic.fountain.unclosed_boneyard").to_string()
            }
            "fountain.unclosed_note" => self.t("diagnostic.fountain.unclosed_note").to_string(),
            "markdown.unclosed_code_fence" => self
                .t("diagnostic.markdown.unclosed_code_fence")
                .to_string(),
            "markdown.empty_heading" => self.t("diagnostic.markdown.empty_heading").to_string(),
            _ => diagnostic.message.clone(),
        }
    }

    /// Refresh the line classification for the screenplay highlighting (M9
    /// slice 5), debounced: a full parse of the 19k-line fixture costs ~2.2
    /// ms, which would blow the keystroke→frame budget if paid per key, so
    /// the previous classification stays visible for 150 ms and a delayed
    /// render recomputes once typing pauses.
    fn refresh_element_kinds(&mut self, cx: &mut Context<Self>) {
        if !self.doc.elements_dirty {
            return;
        }
        let now = Instant::now();
        let wait = Duration::from_millis(150);
        if let Some(last) = self.doc.elements_parsed_at {
            let elapsed = now.duration_since(last);
            if elapsed < wait {
                if !self.doc.elements_timer {
                    self.doc.elements_timer = true;
                    let entity = cx.entity();
                    let remaining = wait - elapsed;
                    cx.spawn(async move |_, cx| {
                        Timer::after(remaining).await;
                        entity
                            .update(cx, |view, cx| {
                                view.doc.elements_timer = false;
                                view.refresh_element_kinds(cx);
                                cx.notify();
                            })
                            .ok();
                    })
                    .detach();
                }
                return;
            }
        }
        let text = self.doc.buffer.slice(0..self.doc.buffer.doc_len());
        let mut kinds = vec![FountainKind::Action; self.doc.buffer.line_count()];
        for element in narra_core::fountain::parse(&text) {
            if element.line_number >= 1 && element.line_number <= kinds.len() {
                kinds[element.line_number - 1] = element.kind;
            }
        }
        self.doc.element_kinds = kinds;
        self.doc.elements_dirty = false;
        self.doc.elements_parsed_at = Some(now);
    }

    fn jump(&mut self, direction: i64) {
        self.refresh_scenes();
        if self.doc.scene_lines.is_empty() {
            return;
        }
        let len = self.doc.scene_lines.len() as i64;
        let next = (self.doc.current_scene as i64 + direction).rem_euclid(len);
        self.doc.current_scene = next as usize;
        let line = self.doc.scene_lines[self.doc.current_scene];
        self.doc.list_state.scroll_to_reveal_item(line);
        let offset = self.doc.buffer.offset_at(Position { line, col: 0 });
        self.doc.selected = offset..offset;
        self.doc.reversed = false;
    }

    /// One probe step: jump to the next scene and request a frame.
    ///
    /// This runs from a timer callback, NOT from inside `render()`: calling
    /// `cx.notify()` during render only produced 2-3 frames and then stalled
    /// (observed in both debug and release, 2026-10-21).
    fn advance_probe(&mut self, cx: &mut Context<Self>) {
        self.refresh_scenes();
        if self.doc.scene_lines.is_empty() {
            return;
        }
        let target_line = self.doc.scene_lines[self.probe_cursor % self.doc.scene_lines.len()];
        self.doc.list_state.scroll_to_reveal_item(target_line);
        self.probe_cursor += 1;
        self.probe_left = self.probe_left.saturating_sub(1);

        // Quit only when no probe work remains (scroll, find and diff probes
        // may be combined in one run).
        let finished =
            self.probe_left == 0 && self.find_probe_left == 0 && self.diff_probe_left == 0;
        cx.notify();
        if finished {
            let report = self.metrics.borrow().report();
            println!("{report}");
            cx.defer(|cx| cx.quit());
        }
    }

    /// The diff probe exercises the shipped cmd-d path. The first step makes
    /// one large real edit (replace-all over the whole document) so there is
    /// a substantial change to diff; every step after that opens the diff
    /// (recompute + render) and then closes it, yielding one
    /// "requested → rendered" sample per open.
    fn advance_diff_probe(&mut self, cx: &mut Context<Self>) {
        if self.diff_probe_left == self.diff_probe_total {
            // Seed a large, real change once — the acceptance metric is
            // "open a diff of a large change", so the diff under measurement
            // must actually be big. Reuse the shipped find + replace-all
            // machinery (query a common word, swap every match) rather than a
            // bespoke edit, so the seeded change is produced by the same code
            // a user drives. `INT.` matches ~778 scene headings.
            self.doc.find.query = "INT.".to_string();
            self.doc.find.replacement = "CENA.".to_string();
            self.refresh_matches(false);
            self.replace_all(cx);
        }

        if self.doc.diff.open {
            self.close_diff(cx);
        } else {
            self.open_diff(cx);
        }

        self.diff_probe_left = self.diff_probe_left.saturating_sub(1);
        let finished =
            self.probe_left == 0 && self.find_probe_left == 0 && self.diff_probe_left == 0;
        cx.notify();
        if finished {
            let report = self.metrics.borrow().report();
            println!("{report}");
            cx.defer(|cx| cx.quit());
        }
    }

    fn schedule_probe_step(&mut self, window: &Window, cx: &Context<Self>) {
        let has_probe = self.probe_left > 0 || self.find_probe_left > 0 || self.diff_probe_left > 0;
        if !has_probe || self.probe_scheduled {
            return;
        }
        self.probe_scheduled = true;
        let entity = cx.entity();
        // 1 ms is the measurement floor: observed frame gap = 1 ms + real
        // render/present time. Documented alongside the baseline.
        window
            .spawn(cx, async move |cx| {
                Timer::after(Duration::from_millis(1)).await;
                cx.update(|_, cx| {
                    entity.update(cx, |view, cx| {
                        view.probe_scheduled = false;
                        if view.diff_probe_left > 0 {
                            view.advance_diff_probe(cx);
                        } else if view.find_probe_left > 0 {
                            view.advance_find_probe(cx);
                        } else {
                            view.advance_probe(cx);
                        }
                    });
                })
                .ok();
            })
            .detach();
    }

    fn ensure_caret_visible(&mut self) {
        let line = self.doc.buffer.position_at(self.caret()).line;
        self.doc.list_state.scroll_to_reveal_item(line);
    }

    // ---- caret movement -------------------------------------------------

    fn move_horiz(&mut self, direction: i64, extend: bool) {
        if !extend && !self.doc.selected.is_empty() {
            let target = if direction < 0 {
                self.doc.selected.start
            } else {
                self.doc.selected.end
            };
            self.set_caret(target, false);
            return;
        }
        let caret = self.caret();
        let target = if direction < 0 {
            self.doc.buffer.prev_boundary(caret)
        } else {
            self.doc.buffer.next_boundary(caret)
        };
        self.set_caret(target, extend);
    }

    /// Returns (visual_row, total_visual_rows, caret_x) for the caret position.
    /// visual_row is 0-based index of the visual row containing the caret.
    /// total_visual_rows is the number of visual rows the line wraps to.
    /// caret_x is the x-offset within the visual row where the caret sits.
    fn caret_visual_info(&self, window: &Window, caret: usize) -> (usize, usize, f32) {
        let pos = self.doc.buffer.position_at(caret);
        let line = pos.line;
        let text = &self.doc.buffer.lines()[line];
        let caret_col = pos.col;
        let wrap_w = f32::from(self.wrap_width);
        let (visual_row, caret_x) = caret_visual_row_and_x(window, text, caret_col, wrap_w);
        let starts = visual_row_starts(window, text, wrap_w);
        let total_visual_rows = starts.len();
        (visual_row, total_visual_rows, caret_x)
    }

    /// Returns the document offset for a given visual row and x-offset within a line.
    fn offset_in_visual_row(
        &self,
        caret: usize,
        target_visual_row: usize,
        caret_x: f32,
        window: &Window,
    ) -> usize {
        let pos = self.doc.buffer.position_at(caret);
        let line = pos.line;
        let text = &self.doc.buffer.lines()[line];
        let wrap_w = f32::from(self.wrap_width);
        let starts = visual_row_starts(window, text, wrap_w);
        if target_visual_row >= starts.len() {
            return self.doc.buffer.line_content_range(line).end;
        }
        let row_start = starts[target_visual_row];
        let row_end = starts
            .get(target_visual_row + 1)
            .copied()
            .unwrap_or(text.len());
        let row_text = &text[row_start..row_end];
        // Find the byte offset within the visual row that corresponds to caret_x
        let shaped = shape_line(window, row_text);
        let glyphs: Vec<(usize, f32)> = shaped
            .runs
            .iter()
            .flat_map(|run| {
                run.glyphs
                    .iter()
                    .map(|g| (g.index, f32::from(g.position.x)))
            })
            .collect();
        // caret_x is relative to the visual row start; we need absolute x
        let row_start_x = glyphs.first().map(|(_, x)| *x).unwrap_or(0.0);
        let absolute_x = caret_x + row_start_x;
        let col = glyphs
            .iter()
            .rev()
            .find(|(_, gx)| *gx <= absolute_x)
            .map(|(ix, _)| *ix)
            .unwrap_or(row_text.len())
            .min(row_text.len());
        (self.doc.buffer.line_content_range(line).start + starts[target_visual_row] + col)
            .min(self.doc.buffer.doc_len())
    }

    fn move_vert(&mut self, delta: i64, extend: bool, window: &Window) {
        let caret = self.caret();
        let (visual_row, total_visual_rows, caret_x) = self.caret_visual_info(window, caret);

        // If on a wrapped line (multiple visual rows), navigate visual rows first
        if total_visual_rows > 1 {
            if delta < 0 {
                // Up
                if visual_row > 0 {
                    // Move to previous visual row on same line
                    let target = self.offset_in_visual_row(caret, visual_row - 1, caret_x, window);
                    self.set_caret(target, extend);
                    return;
                }
                // At visual row 0: fall through to previous logical line
            } else if delta > 0 {
                // Down
                if visual_row < total_visual_rows - 1 {
                    // Move to next visual row on same line
                    let target = self.offset_in_visual_row(caret, visual_row + 1, caret_x, window);
                    self.set_caret(target, extend);
                    return;
                }
                // At last visual row: fall through to next logical line
            }
        }
        // Not on a wrapped line, or at boundary of wrapped line: use logical line navigation
        let target = self.doc.buffer.vertical(caret, delta);
        self.set_caret(target, extend);
    }

    fn move_line_edge(&mut self, to_start: bool, extend: bool) {
        let caret = self.caret();
        let target = if to_start {
            self.doc.buffer.line_start(caret)
        } else {
            self.doc.buffer.line_end(caret)
        };
        self.set_caret(target, extend);
    }

    fn select_all(&mut self) {
        if self.modal_owns_keyboard() {
            return;
        }
        self.doc.selected = 0..self.doc.buffer.doc_len();
        self.doc.reversed = false;
    }

    // ---- action handlers -------------------------------------------------

    fn next_scene(&mut self, _: &NextScene, _window: &mut Window, cx: &mut Context<Self>) {
        // With the find bar open, cmd-g/shift-g navigate matches (same
        // shortcuts the scene jump uses when it is closed); with the diff
        // view open they walk its hunks.
        if self.doc.diff.open {
            self.diff_move_cursor(1, cx);
            return;
        }
        if self.doc.find.open {
            self.next_match(cx);
            return;
        }
        self.jump(1);
        cx.notify();
    }

    fn prev_scene(&mut self, _: &PrevScene, _window: &mut Window, cx: &mut Context<Self>) {
        if self.doc.diff.open {
            self.diff_move_cursor(-1, cx);
            return;
        }
        if self.doc.find.open {
            self.prev_match(cx);
            return;
        }
        self.jump(-1);
        cx.notify();
    }

    fn on_left(&mut self, _: &MoveLeft, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_horiz(-1, false);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_right(&mut self, _: &MoveRight, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_horiz(1, false);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_up(&mut self, _: &MoveUp, window: &mut Window, cx: &mut Context<Self>) {
        if self.context_menu.is_some() {
            self.context_menu_move(-1, cx);
            return;
        }
        if self.palette.is_some() {
            self.palette_move(-1, cx);
            return;
        }
        self.move_vert(-1, false, window);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_down(&mut self, _: &MoveDown, window: &mut Window, cx: &mut Context<Self>) {
        if self.context_menu.is_some() {
            self.context_menu_move(1, cx);
            return;
        }
        if self.palette.is_some() {
            self.palette_move(1, cx);
            return;
        }
        self.move_vert(1, false, window);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_home(&mut self, _: &MoveHome, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_line_edge(true, false);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_end(&mut self, _: &MoveEnd, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_line_edge(false, false);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_select_left(&mut self, _: &SelectLeft, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_horiz(-1, true);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_select_right(&mut self, _: &SelectRight, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_horiz(1, true);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_select_up(&mut self, _: &SelectUp, window: &mut Window, cx: &mut Context<Self>) {
        self.move_vert(-1, true, window);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_select_down(&mut self, _: &SelectDown, window: &mut Window, cx: &mut Context<Self>) {
        self.move_vert(1, true, window);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_select_home(&mut self, _: &SelectHome, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_line_edge(true, true);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_select_end(&mut self, _: &SelectEnd, _window: &mut Window, cx: &mut Context<Self>) {
        self.move_line_edge(false, true);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_select_all(&mut self, _: &SelectAll, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_all();
        cx.notify();
    }

    // ---- word, line and document editing (M9 slice 2) --------------------

    /// Word-wise movement. While a proposal with hunks is on screen the
    /// option arrows belong to it (the visible panel wins, same rule as the
    /// other panel actions); otherwise they move by words in the buffer.
    fn move_word(&mut self, direction: i64, extend: bool) {
        if !extend && !self.doc.selected.is_empty() {
            let target = if direction < 0 {
                self.doc.selected.start
            } else {
                self.doc.selected.end
            };
            self.set_caret(target, false);
            return;
        }
        let caret = self.caret();
        let target = if direction < 0 {
            self.doc.buffer.word_left(caret)
        } else {
            self.doc.buffer.word_right(caret)
        };
        self.set_caret(target, extend);
    }

    fn on_word_left(&mut self, _: &WordLeft, _window: &mut Window, cx: &mut Context<Self>) {
        if self.proposal_hunks_active() {
            self.propose.move_hunk_cursor(-1);
            cx.notify();
            return;
        }
        self.move_word(-1, false);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_word_right(&mut self, _: &WordRight, _window: &mut Window, cx: &mut Context<Self>) {
        if self.proposal_hunks_active() {
            self.propose.move_hunk_cursor(1);
            cx.notify();
            return;
        }
        self.move_word(1, false);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_select_word_left(
        &mut self,
        _: &SelectWordLeft,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.proposal_hunks_active() {
            return;
        }
        self.move_word(-1, true);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_select_word_right(
        &mut self,
        _: &SelectWordRight,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.proposal_hunks_active() {
            return;
        }
        self.move_word(1, true);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn delete_word(&mut self, backward: bool) {
        if !self.doc.selected.is_empty() {
            let range = self.doc.selected.clone();
            self.apply_edit(range, "");
            return;
        }
        let caret = self.caret();
        let range = if backward {
            self.doc.buffer.word_left(caret)..caret
        } else {
            caret..self.doc.buffer.word_right(caret)
        };
        if range.start == range.end {
            return;
        }
        self.apply_edit(range, "");
    }

    fn on_delete_word_left(
        &mut self,
        _: &DeleteWordLeft,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.doc.find.open
            || self.search.open
            || self.propose_bar.open
            || self.focus_bar.open
            || self.doc.preview
        {
            return;
        }
        self.delete_word(true);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_delete_word_right(
        &mut self,
        _: &DeleteWordRight,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.doc.find.open
            || self.search.open
            || self.propose_bar.open
            || self.focus_bar.open
            || self.doc.preview
        {
            return;
        }
        self.delete_word(false);
        cx.notify();
        self.ensure_caret_visible();
    }

    /// ⌥↑/⌥↓: move the caret's line one position up/down (the caret keeps
    /// its column). A single edit keeps undo/redo and the scene scan honest.
    fn move_line(&mut self, direction: i64) {
        let position = self.doc.buffer.position_at(self.caret());
        let line = position.line;
        let target = line as i64 + direction;
        if target < 0 || target as usize >= self.doc.buffer.line_count() {
            return;
        }
        let other = target as usize;
        let other_range = self.doc.buffer.line_content_range(other);
        let line_range = self.doc.buffer.line_content_range(line);
        let other_text = self.doc.buffer.lines()[other].clone();
        let line_text = self.doc.buffer.lines()[line].clone();
        let (span, replacement, caret) = if direction < 0 {
            (
                other_range.start..line_range.end,
                format!("{line_text}\n{other_text}"),
                other_range.start + position.col.min(line_text.len()),
            )
        } else {
            (
                line_range.start..other_range.end,
                format!("{other_text}\n{line_text}"),
                line_range.start + other_text.len() + 1 + position.col.min(line_text.len()),
            )
        };
        self.apply_edit(span, &replacement);
        self.set_caret(caret, false);
    }

    fn on_move_line_up(&mut self, _: &MoveLineUp, _window: &mut Window, cx: &mut Context<Self>) {
        if self.doc.find.open
            || self.search.open
            || self.propose_bar.open
            || self.focus_bar.open
            || self.doc.preview
        {
            return;
        }
        self.move_line(-1);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_move_line_down(
        &mut self,
        _: &MoveLineDown,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.doc.find.open
            || self.search.open
            || self.propose_bar.open
            || self.focus_bar.open
            || self.doc.preview
        {
            return;
        }
        self.move_line(1);
        cx.notify();
        self.ensure_caret_visible();
    }

    /// ⇧⌥↑/⇧⌥↓: duplicate the caret's line below/above it.
    fn duplicate_line(&mut self, above: bool) {
        let position = self.doc.buffer.position_at(self.caret());
        let line = position.line;
        let range = self.doc.buffer.line_content_range(line);
        let text = self.doc.buffer.lines()[line].clone();
        if above {
            self.apply_edit(range.start..range.start, &format!("{text}\n"));
            self.set_caret(range.start + text.len() + 1 + position.col, false);
        } else {
            self.apply_edit(range.end..range.end, &format!("\n{text}"));
            self.set_caret(range.start + position.col.min(text.len()), false);
        }
    }

    fn on_duplicate_line_up(
        &mut self,
        _: &DuplicateLineUp,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.doc.find.open
            || self.search.open
            || self.propose_bar.open
            || self.focus_bar.open
            || self.doc.preview
        {
            return;
        }
        self.duplicate_line(true);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_duplicate_line_down(
        &mut self,
        _: &DuplicateLineDown,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.doc.find.open
            || self.search.open
            || self.propose_bar.open
            || self.focus_bar.open
            || self.doc.preview
        {
            return;
        }
        self.duplicate_line(false);
        cx.notify();
        self.ensure_caret_visible();
    }

    /// ⌘⇧K: delete the caret's line (VSCode parity).
    fn on_delete_line(&mut self, _: &DeleteLine, _window: &mut Window, cx: &mut Context<Self>) {
        if self.doc.find.open
            || self.search.open
            || self.propose_bar.open
            || self.focus_bar.open
            || self.doc.preview
        {
            return;
        }
        let position = self.doc.buffer.position_at(self.caret());
        let line = position.line;
        let count = self.doc.buffer.line_count();
        let range = self.doc.buffer.line_content_range(line);
        let span = if line + 1 < count {
            range.start..self.doc.buffer.line_content_range(line + 1).start
        } else if line > 0 {
            self.doc.buffer.line_content_range(line - 1).end..range.end
        } else {
            range
        };
        self.apply_edit(span.clone(), "");
        let target = span.start.min(self.doc.buffer.doc_len());
        self.set_caret(target, false);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_doc_start(&mut self, _: &DocStart, _window: &mut Window, cx: &mut Context<Self>) {
        self.set_caret(0, false);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_doc_end(&mut self, _: &DocEnd, _window: &mut Window, cx: &mut Context<Self>) {
        let end = self.doc.buffer.doc_len();
        self.set_caret(end, false);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_select_doc_start(
        &mut self,
        _: &SelectDocStart,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_caret(0, true);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_select_doc_end(
        &mut self,
        _: &SelectDocEnd,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let end = self.doc.buffer.doc_len();
        self.set_caret(end, true);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_page_up(&mut self, _: &PageUp, window: &mut Window, cx: &mut Context<Self>) {
        self.move_vert(-PAGE_LINES, false, window);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_page_down(&mut self, _: &PageDown, window: &mut Window, cx: &mut Context<Self>) {
        self.move_vert(PAGE_LINES, false, window);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_select_page_up(&mut self, _: &SelectPageUp, window: &mut Window, cx: &mut Context<Self>) {
        self.move_vert(-PAGE_LINES, true, window);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_select_page_down(
        &mut self,
        _: &SelectPageDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_vert(PAGE_LINES, true, window);
        cx.notify();
        self.ensure_caret_visible();
    }

    // ---- screenplay element cycling (M9 slice 5) -------------------------

    /// Tab/⇧Tab: cycle the caret line between the forceable element kinds
    /// (scene heading, action, character, transition). The line is rewritten
    /// only with Fountain forced markers; its text is preserved and the
    /// caret keeps its position over the same characters.
    fn cycle_element(&mut self, forward: bool) {
        if self.doc.find.open
            || self.search.open
            || self.propose_bar.open
            || self.focus_bar.open
            || self.new_file.is_some()
            || self.new_project.is_some()
            || self.unsaved_prompt.is_some()
        {
            return;
        }
        let position = self.doc.buffer.position_at(self.caret());
        let line = position.line;
        let range = self.doc.buffer.line_content_range(line);
        let text = self.doc.buffer.lines()[line].clone();
        let new_text = screenplay::cycle(&text, forward);
        if new_text == text {
            return;
        }
        let col = position.col.min(text.len());
        let new_col = (col as isize + (new_text.len() as isize - text.len() as isize))
            .clamp(0, new_text.len() as isize) as usize;
        self.apply_edit(range.clone(), &new_text);
        self.set_caret(range.start + new_col, false);
    }

    fn on_cycle_element(&mut self, _: &CycleElement, _window: &mut Window, cx: &mut Context<Self>) {
        if self.context_menu.is_some() || self.palette.is_some() {
            return;
        }
        if self.settings.is_some() {
            self.settings_cycle_field(true, cx);
            return;
        }
        if self.project_settings.is_some() {
            self.project_settings_cycle_field(true, cx);
            return;
        }
        if self.doc.preview {
            return;
        }
        self.cycle_element(true);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_cycle_element_back(
        &mut self,
        _: &CycleElementBack,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.palette.is_some() {
            return;
        }
        if self.settings.is_some() {
            self.settings_cycle_field(false, cx);
            return;
        }
        if self.project_settings.is_some() {
            self.project_settings_cycle_field(false, cx);
            return;
        }
        if self.doc.preview {
            return;
        }
        self.cycle_element(false);
        cx.notify();
        self.ensure_caret_visible();
    }

    // ---- new documents (M10 slice 2) ------------------------------------

    /// ⌘N / File → New File…: open the kind/name/format dialog; pressing it
    /// again closes it.
    fn on_new_file(&mut self, _: &NewFile, _window: &mut Window, cx: &mut Context<Self>) {
        // One modal/prompt owns the keyboard at a time.
        if self.new_project.is_some()
            || self.unsaved_prompt.is_some()
            || self.settings.is_some()
            || self.rename.is_some()
            || self.delete_prompt.is_some()
        {
            return;
        }
        if self.new_file.is_some() {
            self.new_file = None;
            cx.notify();
        } else {
            self.open_new_file(cx);
        }
    }

    fn open_new_file(&mut self, cx: &mut Context<Self>) {
        // The New Project dialog or the unsaved prompt owns the workspace.
        if self.new_project.is_some()
            || self.unsaved_prompt.is_some()
            || self.rename.is_some()
            || self.delete_prompt.is_some()
        {
            return;
        }
        if self.project.is_none() {
            self.notice = Some(self.t("notice.fixture_no_project").to_string());
            cx.notify();
            return;
        }
        self.new_file = Some(NewFileState {
            kind: NewFileKind::Screenplay,
            name: String::new(),
            error: String::new(),
        });
        cx.notify();
    }

    /// Create the document described by the dialog: an explicit canonical
    /// write (refusing existing names), then open it.
    fn create_new_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.new_file.as_ref() else {
            return;
        };
        let kind = state.kind;
        let name = state.name.clone();
        let Some(path) = new_file::target_path(kind, &name) else {
            let message = self.t("newfile.invalid_name").to_string();
            if let Some(state) = self.new_file.as_mut() {
                state.error = message;
            }
            cx.notify();
            return;
        };
        let Some(project) = self.project.clone() else {
            self.new_file = None;
            self.notice = Some(self.t("notice.fixture_no_project").to_string());
            cx.notify();
            return;
        };
        if project.read_document(&path).is_ok() {
            let message = self.t("newfile.exists").to_string();
            if let Some(state) = self.new_file.as_mut() {
                state.error = message;
            }
            cx.notify();
            return;
        }
        let body = new_file::template(kind, &name);
        match project.create_document(&path, &body) {
            Ok(()) => {
                self.new_file = None;
                self.explorer = project.list_documents().unwrap_or_default();
                self.refresh_entities();
                let created = self.tf("notice.created", &[("path", &path)]);
                match project.read_document(&path) {
                    Ok(text) => {
                        self.open_document_tab(DocumentState::new(path.clone(), &text));
                        window.focus(&self.focus_handle(cx));
                        self.notice = Some(created);
                        self.git_refreshed_at = None;
                    }
                    Err(error) => {
                        self.notice = Some(self.tf(
                            "notice.open_failed",
                            &[("path", &path), ("error", &error.to_string())],
                        ));
                    }
                }
            }
            Err(error) => {
                let message = self.tf("newfile.failed", &[("error", &error.to_string())]);
                if let Some(state) = self.new_file.as_mut() {
                    state.error = message;
                }
            }
        }
        cx.notify();
    }

    /// The New File dialog: kind chooser, name field and the format row
    /// (Fountain for screenplays, Markdown otherwise — the format is picked
    /// here until more screenplay formats and Save As exist).
    fn new_file_overlay(&self, cx: &mut Context<Self>) -> gpui::Div {
        let state = self.new_file.as_ref().expect("overlay only when open");
        let mut kinds = div().flex().gap_2();
        for kind in NewFileKind::ALL {
            let selected = kind == state.kind;
            kinds = kinds.child(
                div()
                    .id(kind.key())
                    .px_3()
                    .py_1()
                    .bg(rgb(if selected {
                        theme::doc_selected_bg()
                    } else {
                        theme::explorer_bg()
                    }))
                    .border_1()
                    .border_color(rgb(if selected {
                        theme::default_fg()
                    } else {
                        theme::explorer_border()
                    }))
                    .text_color(rgb(if selected {
                        theme::default_fg()
                    } else {
                        theme::status_fg()
                    }))
                    .cursor_pointer()
                    .child(self.t(kind.i18n_key()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(state) = this.new_file.as_mut() {
                            state.kind = kind;
                            state.error.clear();
                        }
                        cx.notify();
                    })),
            );
        }
        let format = match state.kind {
            NewFileKind::Screenplay => self.t("newfile.format_fountain"),
            _ => self.t("newfile.format_markdown"),
        };
        div()
            .absolute()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000099))
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .on_mouse_up(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .child(
                div()
                    .w(px(560.0))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_4()
                    .bg(rgb(theme::explorer_bg()))
                    .border_1()
                    .border_color(rgb(theme::explorer_border()))
                    .child(
                        div()
                            .text_color(rgb(theme::default_fg()))
                            .child(self.t("newfile.title")),
                    )
                    .child(kinds)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_color(rgb(theme::status_fg()))
                                    .child(self.t("newfile.name")),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .bg(rgb(theme::bg()))
                                    .px_2()
                                    .text_color(rgb(theme::default_fg()))
                                    .child(format!("{}▌", state.name)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_color(rgb(theme::status_fg()))
                                    .child(self.t("newfile.format")),
                            )
                            .child(div().text_color(rgb(theme::default_fg())).child(format)),
                    )
                    .child(
                        div()
                            .text_color(rgb(theme::status_fg()))
                            .child(self.t("newfile.formats_hint")),
                    )
                    .when(!state.error.is_empty(), |card| {
                        card.child(
                            div()
                                .text_color(rgb(theme::agent_err_fg()))
                                .child(state.error.clone()),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                div()
                                    .id("new-file-create")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(theme::status_bg()))
                                    .text_color(rgb(theme::default_fg()))
                                    .cursor_pointer()
                                    .child(self.t("newfile.create"))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.create_new_file(window, cx)
                                    })),
                            )
                            .child(
                                div()
                                    .id("new-file-cancel")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(theme::status_bg()))
                                    .text_color(rgb(theme::status_fg()))
                                    .cursor_pointer()
                                    .child(self.t("newfile.cancel"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.new_file = None;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
    }

    // ---- projects (M10 slice 2b) -----------------------------------------

    /// A project switch must never drop unsaved work or let an agent result
    /// from the previous project land in the new one. Open dialogs and the
    /// quit prompt also own the keyboard, so the menu cannot start a second
    /// flow behind them.
    fn can_switch_project(&mut self) -> bool {
        if self.unsaved_prompt.is_some()
            || self.settings.is_some()
            || self.project_settings.is_some()
            || self.palette.is_some()
            || self.rename.is_some()
            || self.delete_prompt.is_some()
            || self.git_commit.is_some()
        {
            return false;
        }
        if self.new_file.is_some() || self.new_project.is_some() {
            self.notice = Some(self.t("notice.finish_dialog").to_string());
            return false;
        }
        if self.agent_cancel.is_some() {
            self.notice = Some(self.t("notice.project_agent_running").to_string());
            return false;
        }
        if self.project.is_some() && self.dirty_tab_count() > 0 {
            self.notice = Some(self.t("notice.dirty_project_switch").to_string());
            return false;
        }
        true
    }

    /// ⌘O / File → Open Project…: native directory picker, then the core's
    /// validated `open_project`. The receiver is awaited off the UI path; a
    /// cancelled dialog changes nothing.
    fn on_open_project(&mut self, _: &OpenProject, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_switch_project() {
            cx.notify();
            return;
        }
        let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(self.t("dialog.open_project").into()),
        });
        cx.spawn(async move |view, cx| {
            let picked = match rx.await {
                Ok(Ok(Some(paths))) => Ok(paths.into_iter().next()),
                Ok(Ok(None)) => Ok(None),
                Ok(Err(error)) => Err(error.to_string()),
                Err(error) => Err(error.to_string()),
            };
            let _ = view.update(cx, |view, cx| view.finish_open_project(picked, cx));
        })
        .detach();
    }

    fn finish_open_project(
        &mut self,
        picked: Result<Option<PathBuf>, String>,
        cx: &mut Context<Self>,
    ) {
        match picked {
            Ok(Some(path)) => self.open_project_path(&path, cx),
            Ok(None) => {}
            Err(error) => {
                self.notice = Some(
                    self.tf("notice.project_dialog_failed", &[("error", &error)])
                        .to_string(),
                );
            }
        }
        cx.notify();
    }

    /// Open a project by path (dialog or recent menu). Missing folders are
    /// dropped from the recents list so a moved project does not linger.
    fn open_project_path(&mut self, path: &Path, cx: &mut Context<Self>) {
        let root = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if let Some(current) = self.project.as_ref() {
            let current =
                fs::canonicalize(current.root()).unwrap_or_else(|_| current.root().to_path_buf());
            if current == root {
                self.notice = Some(self.t("notice.project_already_open").to_string());
                return;
            }
        }
        match open_project(&root) {
            Ok(project) => {
                if let Err(error) = self.install_project(project, cx) {
                    self.notice = Some(self.tf(
                        "notice.project_open_failed",
                        &[("path", &root.display().to_string()), ("error", &error)],
                    ));
                }
            }
            Err(error) => {
                let stale = matches!(
                    error,
                    ProjectError::NotFound(_) | ProjectError::NotADirectory(_)
                );
                if stale && self.recents.remove(&root) {
                    self.recents.save();
                    self.refresh_menus(cx);
                }
                self.notice = Some(self.tf(
                    "notice.project_open_failed",
                    &[
                        ("path", &root.display().to_string()),
                        ("error", &error.to_string()),
                    ],
                ));
            }
        }
    }

    /// Install a freshly opened project: read its default screenplay and
    /// document list, then reset every surface that belonged to the previous
    /// project. Reads happen before any state is touched, so a permission or
    /// manifest problem leaves the current workspace intact.
    /// Keep the native macOS title bar in sync with the open project (M16
    /// slice 7). Called from `render`, so create/open/rename all land here
    /// without extra hooks.
    fn sync_window_title(&mut self, window: &mut Window) {
        let title = project_window_title(
            self.project
                .as_ref()
                .map(|project| project.manifest().title.as_str()),
        );
        if self.window_title != title {
            self.window_title = title.clone();
            window.set_window_title(&title);
        }
    }

    fn install_project(
        &mut self,
        mut project: Project,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        // macOS may hand back `/var/...` while recents canonicalize to
        // `/private/var/...`; keep the open project on the same canonical
        // root so comparisons, recents and diagnostics agree.
        let root =
            fs::canonicalize(project.root()).unwrap_or_else(|_| project.root().to_path_buf());
        if root != project.root() {
            project = open_project(&root).map_err(|error| error.to_string())?;
        }
        let doc_path = project.manifest().default_screenplay_path.clone();
        let title = project.manifest().title.clone();
        let content = match project.read_document(&doc_path) {
            Ok(text) => text,
            // A project may legitimately have no screenplay file yet; the
            // blank buffer saves through the same safe IO on ⌘S.
            Err(ProjectError::Document(DocumentError::Io { source, .. }))
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                String::new()
            }
            Err(error) => return Err(format!("{doc_path}: {error}")),
        };
        let explorer = project
            .list_documents()
            .map_err(|error| error.to_string())?;

        self.workspace_generation += 1;
        self.agent_generation += 1;
        self.git = GitStatus::default();
        self.git_pending = false;
        self.git_request = self.git_request.saturating_add(1);
        self.git_refreshed_at = None;
        self.project = Some(project);
        self.explorer = explorer;
        self.refresh_entities();
        self.collapsed.clear();
        self.chat_generation = self.chat_generation.saturating_add(1);
        if let Some(cancel) = self.chat_cancel.take() {
            cancel.cancel();
        }
        self.chat = ChatUiState::new(ChatMode::Plan);
        self.chat_markdown.borrow_mut().clear();
        self.chat_scroll = ScrollHandle::new();
        self.chat_scroll_drag = None;
        self.chat_history.clear();
        self.chat_history_open = false;
        self.right_tab = RightTab::Chat;
        // Project-level surfaces never carry over: a review, proposal,
        // search result or export belongs to the project that produced it.
        self.export = ExportState::default();
        self.search = SearchState::default();
        self.agent = AgentState::new();
        self.agent.lang = self.lang;
        self.agent_cancel = None;
        self.propose = ProposeView::new();
        self.propose.lang = self.lang;
        self.propose_bar = ProposeBar::default();
        self.focus_bar = FocusBar::default();
        self.review_focus.clear();
        self.new_file = None;
        self.unsaved_prompt = None;
        // A project switch starts from one tab: the new project's default
        // screenplay. The previous project's tabs are not carried over.
        self.reset_tabs();
        self.doc.backup_path = None;
        self.doc.backup_saved_at = None;
        self.doc.backup_error = None;
        self.doc.page_count = None;
        self.doc.pages_pending = false;
        self.doc.pages_requested_at = None;
        self.load_document(doc_path, content);
        // `load_document` clears the notice, so surface a newer recovery
        // backup once more before the project-opened confirmation.
        self.check_backup_notice();

        self.recents.record(&root);
        self.recents.save();
        self.refresh_menus(cx);
        if self.notice.is_none() {
            self.notice = Some(self.tf(
                "notice.project_opened",
                &[("title", &title), ("path", &root.display().to_string())],
            ));
        }
        Ok(())
    }

    /// File → Open Recent → <project>.
    fn on_open_recent(
        &mut self,
        action: &OpenRecentProject,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_switch_project() {
            cx.notify();
            return;
        }
        self.open_project_path(&action.path, cx);
        cx.notify();
    }

    fn on_clear_recents(
        &mut self,
        _: &ClearRecentProjects,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.recents.clear();
        self.recents.save();
        self.refresh_menus(cx);
        self.notice = Some(self.t("notice.recents_cleared").to_string());
        cx.notify();
    }

    /// ⇧⌘N / File → New Project…: toggle the title/kind/language dialog.
    fn on_new_project(&mut self, _: &NewProject, _window: &mut Window, cx: &mut Context<Self>) {
        if self.new_project.is_some() {
            if !self.new_project.as_ref().is_some_and(|state| state.picking) {
                self.new_project = None;
                self.new_project_generation = self.new_project_generation.saturating_add(1);
                cx.notify();
            }
            return;
        }
        if !self.can_switch_project() {
            cx.notify();
            return;
        }
        self.new_project_generation = self.new_project_generation.saturating_add(1);
        self.new_project = Some(NewProjectState {
            title: String::new(),
            kind: ProjectType::Feature,
            lang: self.lang,
            mode: NewProjectMode::Template,
            prompt: String::new(),
            field: NewProjectField::Title,
            error: String::new(),
            picking: false,
            busy: false,
            path: None,
            plan: None,
        });
        cx.notify();
    }

    fn set_new_project_error(&mut self, message: String) {
        if let Some(state) = self.new_project.as_mut() {
            state.error = message;
        }
    }

    /// Starting directory for the save panel: next to the current project
    /// when there is one, home otherwise.
    fn default_project_dir(&self) -> PathBuf {
        if let Some(project) = self.project.as_ref()
            && let Some(parent) = project.root().parent()
            && parent.is_dir()
        {
            return parent.to_path_buf();
        }
        recent::home_dir()
            .or_else(|| env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// Ask the OS for the destination folder without creating anything.
    /// Choosing a folder and accepting the dialog are separate actions.
    fn choose_new_project_folder(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.new_project.as_ref() else {
            return;
        };
        if state.picking || state.busy {
            return;
        }
        let folder = new_project::clean_title(&state.title)
            .and_then(|title| new_project::suggested_folder(&title))
            .unwrap_or_else(|| "Narra Project".to_string());
        let start = self.default_project_dir();
        let rx = cx.prompt_for_new_path(&start, Some(&folder));
        if let Some(state) = self.new_project.as_mut() {
            state.picking = true;
            state.error.clear();
        }
        cx.notify();
        cx.spawn(async move |view, cx| {
            let picked = match rx.await {
                Ok(Ok(path)) => Ok(path),
                Ok(Err(error)) => Err(error.to_string()),
                Err(error) => Err(error.to_string()),
            };
            let _ = view.update(cx, |view, cx| view.finish_new_project_path(picked, cx));
        })
        .detach();
    }

    fn finish_new_project_path(
        &mut self,
        picked: Result<Option<PathBuf>, String>,
        cx: &mut Context<Self>,
    ) {
        if self.new_project.is_none() {
            return;
        }
        if let Some(state) = self.new_project.as_mut() {
            state.picking = false;
        }
        match picked {
            Ok(None) => {}
            Err(error) => {
                let message = self.tf("newproject.dialog_failed", &[("error", &error)]);
                self.set_new_project_error(message);
            }
            Ok(Some(path)) => {
                if let Some(state) = self.new_project.as_mut() {
                    state.path = Some(path.clone());
                    state.error.clear();
                }
                if let Err(error) = new_project::check_destination(&path) {
                    let message = match error {
                        new_project::DestinationError::NotADirectory => {
                            self.t("newproject.not_directory").to_string()
                        }
                        new_project::DestinationError::NotEmpty => {
                            self.t("newproject.not_empty").to_string()
                        }
                    };
                    self.set_new_project_error(message);
                }
            }
        }
        cx.notify();
    }

    /// The explicit accept action. Template projects are created here; AI
    /// projects generate the reviewable plan first, then create on the next
    /// accept. It never opens the folder panel.
    fn accept_new_project(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.new_project.as_ref() else {
            return;
        };
        if state.picking || state.busy {
            return;
        }
        let Some(title) = new_project::clean_title(&state.title) else {
            self.set_new_project_error(self.t("newproject.invalid_title").to_string());
            cx.notify();
            return;
        };
        let Some(path) = state.path.clone() else {
            self.set_new_project_error(self.t("newproject.folder_required").to_string());
            cx.notify();
            return;
        };
        if let Err(error) = new_project::check_destination(&path) {
            let message = match error {
                new_project::DestinationError::NotADirectory => {
                    self.t("newproject.not_directory").to_string()
                }
                new_project::DestinationError::NotEmpty => {
                    self.t("newproject.not_empty").to_string()
                }
            };
            self.set_new_project_error(message);
            cx.notify();
            return;
        }
        let kind = state.kind;
        let lang = state.lang;
        let mode = state.mode;
        let prompt = state.prompt.trim().to_string();
        let plan_ready = state.plan.is_some();
        if mode == NewProjectMode::Ai {
            if !self.ai_scaffold_available() {
                self.set_new_project_error(self.t("newproject.ai_unavailable").to_string());
                cx.notify();
                return;
            }
            if prompt.is_empty() {
                self.set_new_project_error(self.t("newproject.prompt_required").to_string());
                cx.notify();
                return;
            }
            if plan_ready {
                self.create_new_project_from_plan(cx);
                return;
            }
            let draft = NewProjectDraft {
                title,
                kind,
                lang,
                prompt,
            };
            self.start_new_project_scaffold(path, draft, cx);
            cx.notify();
            return;
        }
        self.create_new_project_from_template(path, title, kind, lang, cx);
    }

    fn create_new_project_from_template(
        &mut self,
        path: PathBuf,
        title: String,
        kind: ProjectType,
        lang: Lang,
        cx: &mut Context<Self>,
    ) {
        match create_project(&path, &title, kind, lang.key()) {
            Ok(project) => {
                if let Err(error) = narra_core::project::seed_basic_template(&path, lang.key()) {
                    let message = self.tf("newproject.failed", &[("error", &error.to_string())]);
                    self.set_new_project_error(message);
                } else {
                    match self.install_project(project, cx) {
                        Ok(()) => {
                            self.new_project = None;
                        }
                        Err(error) => {
                            let message = self.tf("newproject.open_failed", &[("error", &error)]);
                            self.set_new_project_error(message);
                        }
                    }
                }
            }
            Err(error) => {
                let message = self.tf("newproject.failed", &[("error", &error.to_string())]);
                self.set_new_project_error(message);
            }
        }
        cx.notify();
    }

    /// The AI option is available only with a real provider (D-044 option A);
    /// Mock is the offline default and cannot plan a project.
    fn ai_scaffold_available(&self) -> bool {
        self.agent_settings.provider != AiProvider::Mock
    }

    /// Generate a reviewed plan without creating anything (M16 slice 9).
    fn start_new_project_scaffold(
        &mut self,
        path: PathBuf,
        draft: NewProjectDraft,
        cx: &mut Context<Self>,
    ) {
        if !self.ai_scaffold_available() {
            self.set_new_project_error(self.t("newproject.ai_unavailable").to_string());
            return;
        }
        self.new_project_generation = self.new_project_generation.saturating_add(1);
        let generation = self.new_project_generation;
        if let Some(state) = self.new_project.as_mut() {
            state.busy = true;
            state.error.clear();
            state.plan = None;
            state.path = Some(path.clone());
        }
        cx.notify();
        let root = path.to_string_lossy().into_owned();
        let settings = self.agent_settings.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = (|| -> Result<ScaffoldPlan, String> {
                let role = roles::resolve_default_with_packages(
                    Path::new(&root),
                    Some(&recent::config_dir()),
                    ProjectScaffoldTask::KIND,
                )
                .map_err(|error| error.to_string())?;
                let adapter = settings.build_adapter()?;
                let task = ProjectScaffoldTask::new(
                    root.clone(),
                    draft.title.clone(),
                    draft.kind,
                    draft.lang.key(),
                    draft.prompt.clone(),
                )
                .with_role(role);
                let options = TaskOptions {
                    timeout: AGENT_TIMEOUT,
                    cancel: CancellationToken::new(),
                    stream: false,
                };
                let output = task
                    .run(adapter.as_ref(), &options, |_| {})
                    .map_err(|error| error.to_string())?;
                serde_json::from_value::<ScaffoldPlan>(output.data["plan"].clone())
                    .map_err(|error| format!("invalid scaffold payload: {error}"))
            })();
            let _ = tx.send(NewProjectMessage::Plan { generation, result });
        });
        Self::spawn_message_pump(cx, rx, |view, message| {
            let NewProjectMessage::Plan { generation, result } = message;
            if generation != view.new_project_generation {
                return true;
            }
            if let Some(state) = view.new_project.as_mut() {
                state.busy = false;
                match result {
                    Ok(plan) => {
                        state.plan = Some(plan);
                        state.error.clear();
                    }
                    Err(error) => {
                        state.error = i18n::tf(
                            view.lang,
                            "newproject.scaffold_failed",
                            &[("error", &error)],
                        );
                    }
                }
            }
            true
        });
    }

    /// Create the already-chosen folder, write the reviewed plan create-only
    /// and open the project. A write failure rolls the generated files back;
    /// the base project remains usable and is still opened.
    fn create_new_project_from_plan(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.new_project.as_ref() else {
            return;
        };
        if state.busy || state.picking {
            return;
        }
        let Some(path) = state.path.clone() else {
            self.set_new_project_error(self.t("newproject.invalid_title").to_string());
            return;
        };
        let Some(plan) = state.plan.clone() else {
            return;
        };
        let title = new_project::clean_title(&state.title).unwrap_or_else(|| state.title.clone());
        let kind = state.kind;
        let lang = state.lang;
        match create_project(&path, &title, kind, lang.key()) {
            Ok(project) => match write_plan(&path, &plan) {
                Ok(()) => match self.install_project(project, cx) {
                    Ok(()) => {
                        self.new_project = None;
                    }
                    Err(error) => {
                        let message = self.tf("newproject.open_failed", &[("error", &error)]);
                        self.set_new_project_error(message);
                    }
                },
                Err(error) => {
                    let message =
                        self.tf("newproject.write_failed", &[("error", &error.to_string())]);
                    match self.install_project(project, cx) {
                        Ok(()) => {
                            self.new_project = None;
                            self.notice = Some(message);
                        }
                        Err(open_error) => {
                            self.set_new_project_error(
                                self.tf("newproject.open_failed", &[("error", &open_error)]),
                            );
                        }
                    }
                }
            },
            Err(error) => {
                let message = self.tf("newproject.failed", &[("error", &error.to_string())]);
                self.set_new_project_error(message);
            }
        }
        cx.notify();
    }

    /// New Project dialog: title, kind, primary language, destination folder,
    /// project base and review. The folder button opens the native panel;
    /// Enter accepts (creating the template or starting the AI plan). Esc
    /// closes the dialog.
    fn new_project_overlay(&self, cx: &mut Context<Self>) -> gpui::Div {
        let state = self.new_project.as_ref().expect("overlay only when open");
        let editable = !state.picking && !state.busy && state.plan.is_none();
        let mut modes = div().flex().flex_wrap().gap_2();
        for (mode, key) in [
            (NewProjectMode::Template, "newproject.mode.template"),
            (NewProjectMode::Ai, "newproject.mode.ai"),
        ] {
            let selected = state.mode == mode;
            let enabled = editable && (mode != NewProjectMode::Ai || self.ai_scaffold_available());
            let mut chip = div()
                .id(key)
                .px_3()
                .py_1()
                .bg(rgb(if selected {
                    theme::doc_selected_bg()
                } else {
                    theme::explorer_bg()
                }))
                .border_1()
                .border_color(rgb(if selected {
                    theme::default_fg()
                } else {
                    theme::explorer_border()
                }))
                .text_color(rgb(if enabled || selected {
                    theme::default_fg()
                } else {
                    theme::gutter_fg()
                }))
                .when(enabled, |chip| chip.cursor_pointer())
                .child(self.t(key));
            if enabled {
                chip = chip.on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(state) = this.new_project.as_mut()
                        && !state.busy
                        && state.plan.is_none()
                    {
                        state.mode = mode;
                        state.field = if mode == NewProjectMode::Ai {
                            NewProjectField::Prompt
                        } else {
                            NewProjectField::Title
                        };
                        state.error.clear();
                    }
                    cx.notify();
                }));
            }
            modes = modes.child(chip);
        }
        let mut kinds = div().flex().flex_wrap().gap_2();
        for kind in new_project::TYPES {
            let selected = kind == state.kind;
            kinds = kinds.child(
                div()
                    .id(kind.as_str())
                    .px_3()
                    .py_1()
                    .bg(rgb(if selected {
                        theme::doc_selected_bg()
                    } else {
                        theme::explorer_bg()
                    }))
                    .border_1()
                    .border_color(rgb(if selected {
                        theme::default_fg()
                    } else {
                        theme::explorer_border()
                    }))
                    .text_color(rgb(if selected {
                        theme::default_fg()
                    } else {
                        theme::status_fg()
                    }))
                    .cursor_pointer()
                    .child(self.t(new_project::type_i18n_key(kind)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(state) = this.new_project.as_mut()
                            && !state.picking
                            && !state.busy
                            && state.plan.is_none()
                        {
                            state.kind = kind;
                            state.error.clear();
                        }
                        cx.notify();
                    })),
            );
        }
        let mut languages = div().flex().gap_2();
        for lang in [Lang::En, Lang::Es] {
            let selected = lang == state.lang;
            languages = languages.child(
                div()
                    .id(lang.key())
                    .px_3()
                    .py_1()
                    .bg(rgb(if selected {
                        theme::doc_selected_bg()
                    } else {
                        theme::explorer_bg()
                    }))
                    .border_1()
                    .border_color(rgb(if selected {
                        theme::default_fg()
                    } else {
                        theme::explorer_border()
                    }))
                    .text_color(rgb(if selected {
                        theme::default_fg()
                    } else {
                        theme::status_fg()
                    }))
                    .cursor_pointer()
                    .child(lang.name())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(state) = this.new_project.as_mut()
                            && !state.picking
                            && !state.busy
                            && state.plan.is_none()
                        {
                            state.lang = lang;
                            state.error.clear();
                        }
                        cx.notify();
                    })),
            );
        }
        div()
            .absolute()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000099))
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .on_mouse_up(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .child(
                div()
                    .w(px(600.0))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_4()
                    .bg(rgb(theme::explorer_bg()))
                    .border_1()
                    .border_color(rgb(theme::explorer_border()))
                    .child(
                        div()
                            .text_color(rgb(theme::default_fg()))
                            .child(self.t("newproject.title")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_color(rgb(theme::status_fg()))
                                    .child(self.t("newproject.name")),
                            )
                            .child(
                                div()
                                    .id("new-project-title")
                                    .flex_1()
                                    .bg(rgb(theme::bg()))
                                    .px_2()
                                    .py_1()
                                    .border_1()
                                    .border_color(rgb(if state.field == NewProjectField::Title {
                                        theme::default_fg()
                                    } else {
                                        theme::explorer_border()
                                    }))
                                    .text_color(rgb(theme::default_fg()))
                                    .child(
                                        if state.field == NewProjectField::Title
                                            && !state.busy
                                            && state.plan.is_none()
                                        {
                                            format!("{}▌", state.title)
                                        } else {
                                            state.title.clone()
                                        },
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(state) = this.new_project.as_mut()
                                            && !state.busy
                                            && state.plan.is_none()
                                        {
                                            state.field = NewProjectField::Title;
                                        }
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .gap_2()
                            .child(
                                div()
                                    .pt_1()
                                    .text_color(rgb(theme::status_fg()))
                                    .child(self.t("newproject.type")),
                            )
                            .child(kinds),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_color(rgb(theme::status_fg()))
                                    .child(self.t("newproject.language")),
                            )
                            .child(languages),
                    )
                    .child({
                        let folder = state
                            .path
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| self.t("newproject.no_folder").to_string());
                        let folder_enabled = !state.picking && !state.busy && state.plan.is_none();
                        let mut button = div()
                            .id("new-project-folder")
                            .flex_none()
                            .px_3()
                            .py_1()
                            .bg(rgb(if folder_enabled {
                                theme::status_bg()
                            } else {
                                theme::explorer_bg()
                            }))
                            .text_color(rgb(if folder_enabled {
                                theme::default_fg()
                            } else {
                                theme::gutter_fg()
                            }))
                            .child(self.t("newproject.choose_folder"));
                        if folder_enabled {
                            button = button.cursor_pointer().on_click(cx.listener(
                                |this, _, window, cx| this.choose_new_project_folder(window, cx),
                            ));
                        }
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_color(rgb(theme::status_fg()))
                                    .child(self.t("newproject.folder")),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_xs()
                                    .text_color(rgb(if state.path.is_some() {
                                        theme::default_fg()
                                    } else {
                                        theme::gutter_fg()
                                    }))
                                    .child(folder),
                            )
                            .child(button)
                    })
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .gap_2()
                            .child(
                                div()
                                    .pt_1()
                                    .text_color(rgb(theme::status_fg()))
                                    .child(self.t("newproject.mode")),
                            )
                            .child(modes),
                    )
                    .when(!self.ai_scaffold_available(), |card| {
                        card.child(
                            div()
                                .text_xs()
                                .text_color(rgb(theme::gutter_fg()))
                                .child(self.t("newproject.ai_unavailable")),
                        )
                    })
                    .when(
                        state.mode == NewProjectMode::Ai && self.ai_scaffold_available(),
                        |card| {
                            card.child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_color(rgb(theme::status_fg()))
                                            .child(self.t("newproject.prompt")),
                                    )
                                    .child(
                                        div()
                                            .id("new-project-prompt")
                                            .min_h(px(60.0))
                                            .bg(rgb(theme::bg()))
                                            .p_2()
                                            .border_1()
                                            .border_color(rgb(
                                                if state.field == NewProjectField::Prompt {
                                                    theme::default_fg()
                                                } else {
                                                    theme::explorer_border()
                                                },
                                            ))
                                            .text_color(rgb(if state.prompt.is_empty() {
                                                theme::gutter_fg()
                                            } else {
                                                theme::default_fg()
                                            }))
                                            .whitespace_normal()
                                            .cursor_text()
                                            .child(if state.prompt.is_empty() {
                                                format!("{}▌", self.t("newproject.prompt_hint"))
                                            } else if state.field == NewProjectField::Prompt
                                                && !state.busy
                                                && state.plan.is_none()
                                            {
                                                format!("{}▌", state.prompt)
                                            } else {
                                                state.prompt.clone()
                                            })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                if let Some(state) = this.new_project.as_mut()
                                                    && !state.busy
                                                    && state.plan.is_none()
                                                {
                                                    state.field = NewProjectField::Prompt;
                                                }
                                                cx.notify();
                                            })),
                                    ),
                            )
                        },
                    )
                    .when(state.plan.is_some(), |card| {
                        let plan = state.plan.as_ref().expect("checked");
                        let mut list = div()
                            .id("new-project-plan")
                            .max_h(px(180.0))
                            .overflow_y_scroll()
                            .flex()
                            .flex_col()
                            .border_1()
                            .border_color(rgb(theme::explorer_border()));
                        for file in &plan.files {
                            let preview: String =
                                file.content.lines().take(2).collect::<Vec<_>>().join(" ");
                            let preview: String = preview.chars().take(160).collect();
                            list = list.child(
                                div()
                                    .p_2()
                                    .border_b_1()
                                    .border_color(rgb(theme::explorer_border()))
                                    .child(
                                        div()
                                            .text_color(rgb(theme::default_fg()))
                                            .child(file.path.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(theme::gutter_fg()))
                                            .child(preview),
                                    ),
                            );
                        }
                        card.child(list)
                    })
                    .child(
                        div()
                            .text_color(rgb(theme::status_fg()))
                            .child(if state.picking {
                                self.t("newproject.waiting")
                            } else if state.busy {
                                self.t("newproject.generating")
                            } else if state.plan.is_some() {
                                self.t("newproject.plan")
                            } else if state.path.is_none() {
                                self.t("newproject.folder_hint")
                            } else {
                                self.t("newproject.hint")
                            }),
                    )
                    .when(!state.error.is_empty(), |card| {
                        card.child(
                            div()
                                .text_color(rgb(theme::agent_err_fg()))
                                .child(state.error.clone()),
                        )
                    })
                    .child({
                        let plan_ready = state.plan.is_some();
                        let can_click = !state.picking && !state.busy && state.path.is_some();
                        let label = if plan_ready || state.mode == NewProjectMode::Template {
                            self.t("newproject.create")
                        } else {
                            self.t("newproject.generate")
                        };
                        let mut primary = div()
                            .id("new-project-create")
                            .px_3()
                            .py_1()
                            .bg(rgb(if can_click {
                                theme::status_bg()
                            } else {
                                theme::explorer_bg()
                            }))
                            .text_color(rgb(if can_click {
                                theme::default_fg()
                            } else {
                                theme::gutter_fg()
                            }))
                            .child(label);
                        if can_click {
                            primary =
                                primary
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.accept_new_project(cx);
                                    }));
                        }
                        div().flex().gap_2().child(primary).child(
                            div()
                                .id("new-project-cancel")
                                .px_3()
                                .py_1()
                                .bg(rgb(theme::status_bg()))
                                .text_color(rgb(theme::status_fg()))
                                .cursor_pointer()
                                .child(self.t("newproject.cancel"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if !this.new_project.as_ref().is_some_and(|state| state.picking)
                                    {
                                        this.new_project = None;
                                        this.new_project_generation =
                                            this.new_project_generation.saturating_add(1);
                                    }
                                    cx.notify();
                                })),
                        )
                    }),
            )
    }

    /// Rebuild the native menus with the current language and recents.
    fn refresh_menus(&self, cx: &gpui::App) {
        install_menus(self.lang, self.recents.paths(), cx);
    }

    // ---- live page count (M9 slice 7) ------------------------------------

    /// Keep the status bar's page count fresh: after a 250 ms throttle, a
    /// worker lays the buffer out with the export engine (11 ms on the 19k
    /// fixture) and the result is applied when the generation still matches
    /// — a newer edit leaves `pages_dirty` set and the next render retries.
    fn maybe_request_pages(&mut self, cx: &mut Context<Self>) {
        if !self.doc.pages_dirty || self.doc.pages_pending {
            return;
        }
        if let Some(last) = self.doc.pages_requested_at
            && last.elapsed() < Duration::from_millis(250)
        {
            return;
        }
        self.doc.pages_pending = true;
        self.doc.pages_requested_at = Some(Instant::now());
        let generation = self.doc.pages_generation;
        let document = self.doc.path.clone();
        let text = self.doc.buffer.slice(0..self.doc.buffer.doc_len());
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            // Physical pages: body pages plus the title page (the export
            // always carries one: the source's own or the manifest's).
            let pages = narra_core::export::pdf::layout(&text).pages.len() + 1;
            let _ = tx.send(pages);
        });
        Self::spawn_message_pump(cx, rx, move |view, pages| {
            view.apply_page_count(&document, generation, pages);
            true
        });
    }

    /// Apply a page-count result to the document that requested it, even if
    /// the user switched tabs while the worker was running. A closed tab is
    /// simply dropped.
    fn apply_page_count(&mut self, path: &str, generation: u64, pages: usize) {
        let doc = if self.doc.path == path {
            Some(&mut self.doc)
        } else {
            self.tabs
                .iter_mut()
                .find_map(|tab| tab.document.as_mut().filter(|doc| doc.path == path))
        };
        if let Some(doc) = doc {
            doc.pages_pending = false;
            if doc.pages_generation == generation {
                doc.page_count = Some(pages);
                doc.pages_dirty = false;
            }
        }
    }

    // ---- local Git status (M10 S3c) --------------------------------------

    /// Refresh Git at most every five seconds while a project is open, and
    /// immediately after explicit events (project open, save, new file) by
    /// clearing the clock. The command runs on a worker.
    fn maybe_refresh_git(&mut self, cx: &mut Context<Self>) {
        if self.project.is_none() {
            if self.git.available || !self.git.error.is_empty() {
                self.git = GitStatus::default();
            }
            return;
        }
        if self.git_pending {
            return;
        }
        if let Some(last) = self.git_refreshed_at
            && last.elapsed() < Duration::from_secs(5)
        {
            return;
        }
        self.request_git_refresh(cx);
    }

    fn request_git_refresh(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let root = project.root().to_path_buf();
        self.git_pending = true;
        self.git_request = self.git_request.saturating_add(1);
        let request = self.git_request;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = git_status::refresh(&root);
            let _ = tx.send(GitMessage::Done { request, result });
        });
        Self::spawn_message_pump(cx, rx, move |view, message| {
            let GitMessage::Done { request, result } = message else {
                return false;
            };
            if request != view.git_request {
                // A newer refresh (or a project switch) superseded this one.
                return true;
            }
            view.git_pending = false;
            view.git_refreshed_at = Some(Instant::now());
            match result {
                Ok(git) => {
                    if git.available && view.git_history.is_empty() {
                        view.git_history_stale = true;
                    }
                    if git.available && view.git_branches.is_empty() {
                        view.git_branches_stale = true;
                    }
                    view.git = git;
                }
                Err(error) => {
                    view.git = GitStatus {
                        error,
                        ..GitStatus::default()
                    };
                }
            }
            true
        });
    }

    fn request_git_branches(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let root = project.root().to_path_buf();
        self.git_branches_loading = true;
        self.git_branches_error.clear();
        self.git_branches_stale = false;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let options = OpOptions::default().timeout(Duration::from_secs(10));
            let result = Repository::open(&root, options.clone())
                .and_then(|repository| repository.branches(options))
                .map_err(|error| error.to_string());
            let _ = tx.send(GitMessage::Branches { result });
        });
        Self::spawn_message_pump(cx, rx, move |view, message| {
            let GitMessage::Branches { result } = message else {
                return false;
            };
            view.git_branches_loading = false;
            match result {
                Ok(branches) => view.git_branches = branches,
                Err(error) => view.git_branches_error = error,
            }
            true
        });
    }

    fn open_git_diff(&mut self, path: String, cx: &mut Context<Self>) {
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let root = project.root().to_path_buf();
        let is_new = matches!(
            self.git.file_status(&path),
            Some(WorktreeStatus::Added | WorktreeStatus::Untracked)
        );
        self.git_diff = Some(GitDiffState {
            path: path.clone(),
            diff: String::new(),
            loading: true,
            error: String::new(),
        });
        let project = project.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = if is_new {
                // `git diff HEAD` cannot show an untracked file, but the
                // writer still expects to see what they are about to save.
                project
                    .read_document(&path)
                    .map(|text| added_file_diff(&path, &text))
                    .map_err(|error| error.to_string())
            } else {
                let options = OpOptions::default().timeout(Duration::from_secs(10));
                Repository::open(&root, options.clone())
                    .and_then(|repository| {
                        repository.diff_path(
                            narra_core::git::DiffScope::WorktreeVsHead,
                            &path,
                            options,
                        )
                    })
                    .map_err(|error| error.to_string())
            };
            let _ = tx.send(GitMessage::Diff { path, result });
        });
        Self::spawn_message_pump(cx, rx, move |view, message| {
            let GitMessage::Diff { path, result } = message else {
                return false;
            };
            if let Some(diff) = view.git_diff.as_mut()
                && diff.path == path
            {
                diff.loading = false;
                match result {
                    Ok(text) => diff.diff = text,
                    Err(error) => diff.error = error,
                }
            }
            true
        });
    }

    fn open_git_restore(&mut self, path: String, cx: &mut Context<Self>) {
        self.git_restore = Some(GitRestoreState {
            path,
            error: String::new(),
        });
        cx.notify();
    }

    fn confirm_git_restore(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.git_restore.as_ref() else {
            return;
        };
        if self.doc.path == prompt.path && self.dirty() {
            let message = self.t("git.restore_dirty").to_string();
            if let Some(prompt) = self.git_restore.as_mut() {
                prompt.error = message;
            }
            cx.notify();
            return;
        }
        let Some(project) = self.project.clone() else {
            return;
        };
        let path = prompt.path.clone();
        self.git_busy = true;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let options = OpOptions::default().timeout(Duration::from_secs(10));
            let result = (|| {
                let repository =
                    Repository::open(project.root(), options.clone()).map_err(|e| e.to_string())?;
                let text = repository
                    .show_file("HEAD", &path, options)
                    .map_err(|e| e.to_string())?;
                project
                    .write_document(&path, &text)
                    .map_err(|e| e.to_string())
            })();
            let _ = tx.send(GitMessage::Restore { path, result });
        });
        Self::spawn_message_pump(cx, rx, move |view, message| {
            let GitMessage::Restore { path, result } = message else {
                return false;
            };
            view.git_busy = false;
            match result {
                Ok(()) => {
                    if view.doc.path == path
                        && let Some(project) = view.project.as_ref()
                        && let Ok(text) = project.read_document(&path)
                    {
                        view.load_document(path.clone(), text);
                    }
                    view.git_restore = None;
                    view.git_diff = None;
                    view.git_refreshed_at = None;
                    view.git_history_stale = true;
                    view.notice = Some(view.t("git.restored").to_string());
                }
                Err(error) => {
                    let message = view.tf("git.restore_failed", &[("error", &error)]);
                    if let Some(prompt) = view.git_restore.as_mut() {
                        prompt.error = message;
                    }
                }
            }
            true
        });
    }

    fn reload_open_documents(&mut self) {
        let Some(project) = self.project.clone() else {
            return;
        };
        let active = if self.doc.path.is_empty() {
            None
        } else {
            project
                .read_document(&self.doc.path)
                .ok()
                .map(|text| (self.doc.path.clone(), text))
        };
        let inactive: Vec<(usize, String, String)> = self
            .tabs
            .iter()
            .enumerate()
            .filter_map(|(index, tab)| {
                let doc = tab.document.as_ref()?;
                let text = project.read_document(&doc.path).ok()?;
                Some((index, doc.path.clone(), text))
            })
            .collect();
        if let Some((path, text)) = active {
            self.load_document(path, text);
        }
        for (index, path, text) in inactive {
            self.tabs[index].document = Some(DocumentState::new(path, &text));
        }
    }

    fn switch_git_branch(&mut self, name: String, cx: &mut Context<Self>) {
        if self.dirty_tab_count() > 0 {
            self.notice = Some(self.t("git.save_files_first").to_string());
            cx.notify();
            return;
        }
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let root = project.root().to_path_buf();
        self.git_busy = true;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let options = OpOptions::default().timeout(Duration::from_secs(20));
            let result = Repository::open(&root, options.clone())
                .and_then(|repository| repository.checkout(&name, options))
                .map_err(|error| error.to_string());
            let _ = tx.send(GitMessage::BranchSwitch { result });
        });
        Self::spawn_message_pump(cx, rx, move |view, message| {
            let GitMessage::BranchSwitch { result } = message else {
                return false;
            };
            view.git_busy = false;
            match result {
                Ok(()) => {
                    view.reload_open_documents();
                    view.git_refreshed_at = None;
                    view.git_history_stale = true;
                    view.notice = Some(view.t("git.branch_switched").to_string());
                    view.git_branches_stale = true;
                }
                Err(error) => {
                    view.notice = Some(view.tf("git.branch_failed", &[("error", &error)]));
                }
            }
            true
        });
    }

    fn open_branch_prompt(&mut self, cx: &mut Context<Self>) {
        self.git_branch_prompt = Some(GitBranchPromptState {
            name: String::new(),
            error: String::new(),
            busy: false,
        });
        cx.notify();
    }

    fn branch_prompt_input(&mut self, text: &str, cx: &mut Context<Self>) {
        let cleaned: String = text
            .chars()
            .filter(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | ' ')
            })
            .collect();
        if let Some(state) = self.git_branch_prompt.as_mut() {
            state.name.push_str(&cleaned);
            state.error.clear();
        }
        cx.notify();
    }

    fn branch_prompt_backspace(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.git_branch_prompt.as_mut() {
            if let Some(last) = state.name.chars().last() {
                let len = state.name.len();
                state.name.truncate(len - last.len_utf8());
            }
            state.error.clear();
        }
        cx.notify();
    }

    fn submit_branch_prompt(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.git_branch_prompt.as_ref() else {
            return;
        };
        if state.busy {
            return;
        }
        let name = state.name.trim().replace(' ', "-");
        if name.is_empty() {
            let message = self.t("git.branch_name_required").to_string();
            if let Some(state) = self.git_branch_prompt.as_mut() {
                state.error = message;
            }
            cx.notify();
            return;
        }
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let root = project.root().to_path_buf();
        if let Some(state) = self.git_branch_prompt.as_mut() {
            state.busy = true;
        }
        self.git_busy = true;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let options = OpOptions::default().timeout(Duration::from_secs(20));
            let result = Repository::open(&root, options.clone())
                .and_then(|repository| repository.checkout_new(&name, options))
                .map_err(|error| error.to_string());
            let _ = tx.send(GitMessage::BranchCreate { result });
        });
        Self::spawn_message_pump(cx, rx, move |view, message| {
            let GitMessage::BranchCreate { result } = message else {
                return false;
            };
            view.git_busy = false;
            match result {
                Ok(()) => {
                    view.git_branch_prompt = None;
                    view.reload_open_documents();
                    view.git_refreshed_at = None;
                    view.git_history_stale = true;
                    view.notice = Some(view.t("git.branch_created").to_string());
                    view.git_branches_stale = true;
                }
                Err(error) => {
                    let message = view.tf("git.branch_failed", &[("error", &error)]);
                    if let Some(state) = view.git_branch_prompt.as_mut() {
                        state.busy = false;
                        state.error = message;
                    }
                }
            }
            true
        });
    }

    fn request_git_remotes(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.project.as_ref() else {
            return;
        };
        if !self.git.available {
            self.git_remotes.clear();
            return;
        }
        let root = project.root().to_path_buf();
        self.git_remotes_loading = true;
        self.git_remotes_error.clear();
        self.git_remotes_stale = false;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let options = OpOptions::default().timeout(Duration::from_secs(10));
            let result = Repository::open(&root, options.clone())
                .and_then(|repository| repository.remotes(options))
                .map_err(|error| error.to_string());
            let _ = tx.send(GitMessage::Remotes { result });
        });
        Self::spawn_message_pump(cx, rx, move |view, message| {
            let GitMessage::Remotes { result } = message else {
                return false;
            };
            view.git_remotes_loading = false;
            match result {
                Ok(remotes) => view.git_remotes = remotes,
                Err(error) => view.git_remotes_error = error,
            }
            true
        });
    }

    fn github_remote_info(&self) -> Option<(String, String, String)> {
        let remote = self
            .git_remotes
            .iter()
            .find(|(name, _)| name == "origin")
            .or_else(|| self.git_remotes.first())?;
        let (owner, repo) = parse_github_remote(&remote.1)?;
        Some((remote.0.clone(), owner, repo))
    }

    fn open_github_publish(&mut self, cx: &mut Context<Self>) {
        if resolve_github_token().ok().flatten().is_none() {
            self.notice = Some(self.t("github.token_required").to_string());
            self.open_git_settings(cx);
            return;
        }
        let name = slugify(
            &self
                .project
                .as_ref()
                .map(|project| project.manifest().title.clone())
                .unwrap_or_else(|| "narra-project".to_string()),
        );
        self.github_publish = Some(GitHubPublishState {
            name,
            private: true,
            error: String::new(),
            busy: false,
        });
        cx.notify();
    }

    fn github_publish_input(&mut self, text: &str, cx: &mut Context<Self>) {
        let cleaned: String = text
            .chars()
            .filter(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
            })
            .collect();
        if let Some(state) = self.github_publish.as_mut() {
            state.name.push_str(&cleaned);
            state.error.clear();
        }
        cx.notify();
    }

    fn github_publish_backspace(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.github_publish.as_mut() {
            if let Some(last) = state.name.chars().last() {
                let len = state.name.len();
                state.name.truncate(len - last.len_utf8());
            }
            state.error.clear();
        }
        cx.notify();
    }

    fn submit_github_publish(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.github_publish.as_ref() else {
            return;
        };
        if state.busy {
            return;
        }
        let name = state.name.trim().to_string();
        let private = state.private;
        if name.is_empty() {
            let message = self.t("github.repo_required").to_string();
            if let Some(state) = self.github_publish.as_mut() {
                state.error = message;
            }
            cx.notify();
            return;
        }
        let token = match resolve_github_token() {
            Ok(Some(token)) => token,
            Ok(None) => {
                let message = self.t("github.token_required").to_string();
                if let Some(state) = self.github_publish.as_mut() {
                    state.error = message;
                }
                cx.notify();
                return;
            }
            Err(error) => {
                if let Some(state) = self.github_publish.as_mut() {
                    state.error = error;
                }
                cx.notify();
                return;
            }
        };
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let root = project.root().to_path_buf();
        let branch = if self.git.branch.is_empty() {
            "main".to_string()
        } else {
            self.git.branch.clone()
        };
        if let Some(state) = self.github_publish.as_mut() {
            state.busy = true;
            state.error.clear();
        }
        self.git_busy = true;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let options = OpOptions::default().timeout(Duration::from_secs(120));
            let result = (|| {
                let config =
                    GitHubConfig::github_com(token.clone()).with_timeout(Duration::from_secs(30));
                let client = GitHubClient::new(config);
                let _user = client
                    .authenticated_user()
                    .map_err(|error| error.to_string())?;
                let repository = client
                    .create_repository(&NewRepository {
                        name,
                        private,
                        description: Some("Narra project".to_string()),
                        auto_init: false,
                    })
                    .map_err(|error| error.to_string())?;
                let git_repository =
                    Repository::open(&root, options.clone()).map_err(|error| error.to_string())?;
                git_repository
                    .add_remote("origin", &repository.clone_url, options.clone())
                    .map_err(|error| error.to_string())?;
                git_repository
                    .push_with_token("origin", &branch, &token, options)
                    .map_err(|error| error.to_string())?;
                Ok(repository.html_url)
            })();
            let _ = tx.send(GitMessage::Publish { result });
        });
        Self::spawn_message_pump(cx, rx, move |view, message| {
            let GitMessage::Publish { result } = message else {
                return false;
            };
            view.git_busy = false;
            match result {
                Ok(url) => {
                    view.github_publish = None;
                    view.notice = Some(view.tf("github.published", &[("url", &url)]));
                    view.git_refreshed_at = None;
                    view.git_history_stale = true;
                    view.git_branches_stale = true;
                    view.git_remotes_stale = true;
                }
                Err(error) => {
                    let message = view.tf("github.publish_failed", &[("error", &error)]);
                    if let Some(state) = view.github_publish.as_mut() {
                        state.busy = false;
                        state.error = message;
                    }
                }
            }
            true
        });
    }

    fn open_git_sync(&mut self, kind: GitSyncKind, cx: &mut Context<Self>) {
        if self.dirty_tab_count() > 0 {
            self.notice = Some(self.t("git.save_files_first").to_string());
            cx.notify();
            return;
        }
        if self.github_remote_info().is_none() {
            self.notice = Some(self.t("github.no_remote").to_string());
            cx.notify();
            return;
        }
        self.git_sync = Some(GitSyncState {
            kind,
            error: String::new(),
            busy: false,
        });
        cx.notify();
    }

    fn confirm_git_sync(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.git_sync.as_ref() else {
            return;
        };
        if state.busy {
            return;
        }
        let kind = state.kind;
        let token = match resolve_github_token() {
            Ok(Some(token)) => token,
            Ok(None) => {
                let message = self.t("github.token_required").to_string();
                if let Some(state) = self.git_sync.as_mut() {
                    state.error = message;
                }
                cx.notify();
                return;
            }
            Err(error) => {
                if let Some(state) = self.git_sync.as_mut() {
                    state.error = error;
                }
                cx.notify();
                return;
            }
        };
        let Some((remote, _, _)) = self.github_remote_info() else {
            return;
        };
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let root = project.root().to_path_buf();
        let branch = if self.git.branch.is_empty() {
            "main".to_string()
        } else {
            self.git.branch.clone()
        };
        if let Some(state) = self.git_sync.as_mut() {
            state.busy = true;
            state.error.clear();
        }
        self.git_busy = true;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let options = OpOptions::default().timeout(Duration::from_secs(120));
            let result = (|| {
                let repository =
                    Repository::open(&root, options.clone()).map_err(|error| error.to_string())?;
                match kind {
                    GitSyncKind::Push => repository
                        .push_with_token(&remote, &branch, &token, options)
                        .map_err(|error| error.to_string()),
                    GitSyncKind::Pull => repository
                        .pull_with_token(&remote, &branch, &token, options)
                        .map_err(|error| error.to_string()),
                }
            })();
            let _ = tx.send(match kind {
                GitSyncKind::Push => GitMessage::Push { result },
                GitSyncKind::Pull => GitMessage::Pull { result },
            });
        });
        Self::spawn_message_pump(cx, rx, move |view, message| {
            let (kind, result) = match message {
                GitMessage::Push { result } => (GitSyncKind::Push, result),
                GitMessage::Pull { result } => (GitSyncKind::Pull, result),
                _ => return false,
            };
            view.git_busy = false;
            match result {
                Ok(()) => {
                    view.git_sync = None;
                    view.notice = Some(
                        view.t(if kind == GitSyncKind::Push {
                            "github.pushed"
                        } else {
                            "github.pulled"
                        })
                        .to_string(),
                    );
                    view.git_refreshed_at = None;
                    view.git_history_stale = true;
                    view.git_branches_stale = true;
                    if kind == GitSyncKind::Pull {
                        view.reload_open_documents();
                    }
                }
                Err(error) => {
                    let message = view.tf(
                        if kind == GitSyncKind::Push {
                            "github.push_failed"
                        } else {
                            "github.pull_failed"
                        },
                        &[("error", &error)],
                    );
                    if let Some(state) = view.git_sync.as_mut() {
                        state.busy = false;
                        state.error = message;
                    }
                }
            }
            true
        });
    }

    fn open_github_pr(&mut self, cx: &mut Context<Self>) {
        if resolve_github_token().ok().flatten().is_none() {
            self.notice = Some(self.t("github.token_required").to_string());
            self.open_git_settings(cx);
            return;
        }
        if self.github_remote_info().is_none() {
            self.notice = Some(self.t("github.no_remote").to_string());
            cx.notify();
            return;
        }
        let branch = if self.git.branch.is_empty() {
            "main".to_string()
        } else {
            self.git.branch.clone()
        };
        if branch == "main" {
            self.notice = Some(self.t("github.create_branch_first").to_string());
            cx.notify();
            return;
        }
        self.github_pr = Some(GitHubPullRequestState {
            title: self.tf("github.pr_default_title", &[("branch", &branch)]),
            body: String::new(),
            base: "main".to_string(),
            field: 0,
            error: String::new(),
            busy: false,
        });
        cx.notify();
    }

    fn github_pr_input(&mut self, text: &str, cx: &mut Context<Self>) {
        let cleaned: String = text
            .chars()
            .filter(|character| !character.is_control() || *character == ' ')
            .collect();
        if let Some(state) = self.github_pr.as_mut() {
            match state.field {
                0 => state.title.push_str(&cleaned),
                1 => state.body.push_str(&cleaned),
                _ => state.base.push_str(&cleaned),
            }
            state.error.clear();
        }
        cx.notify();
    }

    fn github_pr_backspace(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.github_pr.as_mut() {
            let target = match state.field {
                0 => &mut state.title,
                1 => &mut state.body,
                _ => &mut state.base,
            };
            if let Some(last) = target.chars().last() {
                let len = target.len();
                target.truncate(len - last.len_utf8());
            }
            state.error.clear();
        }
        cx.notify();
    }

    fn submit_github_pr(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.github_pr.as_ref() else {
            return;
        };
        if state.busy {
            return;
        }
        let title = state.title.trim().to_string();
        let body = state.body.trim().to_string();
        let base = state.base.trim().to_string();
        if title.is_empty() || base.is_empty() {
            let message = self.t("github.pr_required").to_string();
            if let Some(state) = self.github_pr.as_mut() {
                state.error = message;
            }
            cx.notify();
            return;
        }
        let branch = if self.git.branch.is_empty() {
            "main".to_string()
        } else {
            self.git.branch.clone()
        };
        if branch == base {
            let message = self.t("github.pr_same_branch").to_string();
            if let Some(state) = self.github_pr.as_mut() {
                state.error = message;
            }
            cx.notify();
            return;
        }
        let token = match resolve_github_token() {
            Ok(Some(token)) => token,
            _ => {
                let message = self.t("github.token_required").to_string();
                if let Some(state) = self.github_pr.as_mut() {
                    state.error = message;
                }
                cx.notify();
                return;
            }
        };
        let Some((_, owner, repo)) = self.github_remote_info() else {
            return;
        };
        if let Some(state) = self.github_pr.as_mut() {
            state.busy = true;
            state.error.clear();
        }
        self.git_busy = true;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let config = GitHubConfig::github_com(token).with_timeout(Duration::from_secs(30));
            let client = GitHubClient::new(config);
            let result = client
                .create_pull_request(
                    &owner,
                    &repo,
                    &NewPullRequest {
                        title,
                        head: branch,
                        base,
                        body: (!body.is_empty()).then_some(body),
                    },
                )
                .map(|pr| pr.html_url)
                .map_err(|error| error.to_string());
            let _ = tx.send(GitMessage::PullRequest { result });
        });
        Self::spawn_message_pump(cx, rx, move |view, message| {
            let GitMessage::PullRequest { result } = message else {
                return false;
            };
            view.git_busy = false;
            match result {
                Ok(url) => {
                    view.github_pr = None;
                    let _ = std::process::Command::new("open").arg(&url).spawn();
                    view.notice = Some(view.tf("github.pr_created", &[("url", &url)]));
                }
                Err(error) => {
                    let message = view.tf("github.pr_failed", &[("error", &error)]);
                    if let Some(state) = view.github_pr.as_mut() {
                        state.busy = false;
                        state.error = message;
                    }
                }
            }
            true
        });
    }

    fn request_git_history(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let root = project.root().to_path_buf();
        if !self.git.available {
            self.git_history.clear();
            self.git_history_error.clear();
            return;
        }
        self.git_history_loading = true;
        self.git_history_error.clear();
        self.git_history_stale = false;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let options = OpOptions::default().timeout(Duration::from_secs(10));
            let result = Repository::open(&root, options.clone())
                .and_then(|repository| repository.log(8, options))
                .map_err(|error| error.to_string());
            let _ = tx.send(GitMessage::History { result });
        });
        Self::spawn_message_pump(cx, rx, move |view, message| {
            let GitMessage::History { result } = message else {
                return false;
            };
            view.git_history_loading = false;
            match result {
                Ok(commits) => view.git_history = commits,
                Err(error) => view.git_history_error = error,
            }
            true
        });
    }

    fn open_git_commit(&mut self, cx: &mut Context<Self>) {
        if !self.git.available {
            self.notice = Some(self.t("git.no_repo").to_string());
            cx.notify();
            return;
        }
        if self.dirty_tab_count() > 0 {
            self.notice = Some(self.t("git.save_files_first").to_string());
            cx.notify();
            return;
        }
        if self.git.files.is_empty() {
            self.notice = Some(self.t("git.nothing_to_save").to_string());
            cx.notify();
            return;
        }
        if self.app_settings.git_name.trim().is_empty()
            || self.app_settings.git_email.trim().is_empty()
        {
            self.notice = Some(self.t("git.identity_missing").to_string());
            self.open_git_settings(cx);
            return;
        }
        self.git_commit = Some(GitCommitState {
            message: String::new(),
            error: String::new(),
            busy: false,
            selected: self.git.files.keys().cloned().collect(),
        });
        cx.notify();
    }

    fn open_git_settings(&mut self, cx: &mut Context<Self>) {
        if self.settings.is_none() {
            // The settings action needs a window; this helper is also called
            // from non-key paths, so it opens the modal state directly.
            let provider = self.app_settings.provider;
            let (key_present, key_source, key_checked) = match cached_provider_key_status(provider)
            {
                Some((present, source)) => (present, source, true),
                None => (false, String::new(), false),
            };
            let (github_token_present, github_token_source, github_token_checked) =
                match cached_github_token_status() {
                    Some((present, source)) => (present, source, true),
                    None => (false, String::new(), false),
                };
            let (agents, agents_error) = self.load_effective_agents();
            self.settings = Some(SettingsState {
                section: SettingsSection::Git,
                provider,
                base_url: provider
                    .fixed_base_url()
                    .map(str::to_string)
                    .unwrap_or_else(|| self.app_settings.base_url.clone()),
                model: self.app_settings.model.clone(),
                key_input: String::new(),
                key_present,
                key_source,
                key_checked,
                field: SettingsField::GitName,
                error: String::new(),
                status: String::new(),
                models: provider
                    .known_models()
                    .iter()
                    .map(|model| model.to_string())
                    .collect(),
                models_request: 0,
                fetching_models: false,
                language: self.app_settings.language,
                theme: self.app_settings.theme,
                font_size: self.app_settings.font_size,
                word_wrap: self.app_settings.word_wrap,
                line_numbers: self.app_settings.line_numbers,
                show_config_files: self.app_settings.show_config_files,
                git_name: self.app_settings.git_name.clone(),
                git_email: self.app_settings.git_email.clone(),
                github_token_input: String::new(),
                github_token_present,
                github_token_source,
                github_token_checked,
                key_request: 0,
                github_request: 0,
                agents,
                agents_error,
            });
        } else if let Some(settings) = self.settings.as_mut() {
            settings.section = SettingsSection::Git;
        }
        cx.notify();
    }

    fn git_commit_input(&mut self, text: &str, cx: &mut Context<Self>) {
        let cleaned: String = text
            .chars()
            .filter(|character| !character.is_control() || *character == ' ')
            .collect();
        if let Some(state) = self.git_commit.as_mut() {
            state.message.push_str(&cleaned);
            state.error.clear();
        }
        cx.notify();
    }

    fn git_commit_backspace(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.git_commit.as_mut() {
            if let Some(last) = state.message.chars().last() {
                let len = state.message.len();
                state.message.truncate(len - last.len_utf8());
            }
            state.error.clear();
        }
        cx.notify();
    }

    fn toggle_git_commit_path(&mut self, path: &str, cx: &mut Context<Self>) {
        if let Some(state) = self.git_commit.as_mut() {
            if state.selected.contains(path) {
                state.selected.remove(path);
            } else {
                state.selected.insert(path.to_string());
            }
            state.error.clear();
        }
        cx.notify();
    }

    fn submit_git_commit(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.git_commit.as_ref() else {
            return;
        };
        if state.busy {
            return;
        }
        let message = state.message.trim().to_string();
        if message.is_empty() {
            let error = self.t("git.commit_message_required").to_string();
            if let Some(state) = self.git_commit.as_mut() {
                state.error = error;
            }
            cx.notify();
            return;
        }
        let name = self.app_settings.git_name.trim().to_string();
        let email = self.app_settings.git_email.trim().to_string();
        if name.is_empty() || email.is_empty() {
            let error = self.t("git.identity_missing").to_string();
            if let Some(state) = self.git_commit.as_mut() {
                state.error = error;
            }
            cx.notify();
            return;
        }
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let root = project.root().to_path_buf();
        let paths: Vec<String> = self
            .git_commit
            .as_ref()
            .map(|state| state.selected.iter().cloned().collect())
            .unwrap_or_default();
        if paths.is_empty() {
            let message = self.t("git.nothing_to_save").to_string();
            if let Some(state) = self.git_commit.as_mut() {
                state.error = message;
            }
            cx.notify();
            return;
        }
        if let Some(state) = self.git_commit.as_mut() {
            state.busy = true;
            state.error.clear();
        }
        self.git_busy = true;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let options = OpOptions::default().timeout(Duration::from_secs(15));
            let result = (|| {
                let repository =
                    Repository::open(&root, options.clone()).map_err(|error| error.to_string())?;
                repository
                    .unstage_all(options.clone())
                    .map_err(|error| error.to_string())?;
                let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
                repository
                    .stage(&refs, options.clone())
                    .map_err(|error| error.to_string())?;
                repository
                    .commit(&message, &name, &email, options)
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })();
            let _ = tx.send(GitMessage::Commit { result });
        });
        Self::spawn_message_pump(cx, rx, move |view, message| {
            let GitMessage::Commit { result } = message else {
                return false;
            };
            view.git_busy = false;
            match result {
                Ok(()) => {
                    view.git_commit = None;
                    view.notice = Some(view.t("git.version_saved").to_string());
                    view.git_refreshed_at = None;
                    view.git_history_stale = true;
                }
                Err(error) => {
                    let message = view.tf("git.commit_failed", &[("error", &error)]);
                    if let Some(state) = view.git_commit.as_mut() {
                        state.busy = false;
                        state.error = message;
                    }
                }
            }
            true
        });
    }

    fn init_git_repo(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.project.as_ref() else {
            return;
        };
        if self.git_busy {
            return;
        }
        let root = project.root().to_path_buf();
        self.git_busy = true;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let options = OpOptions::default().timeout(Duration::from_secs(15));
            let result = Repository::init(&root, options)
                .map(|_| ())
                .map_err(|error| error.to_string());
            let _ = tx.send(GitMessage::Init { result });
        });
        Self::spawn_message_pump(cx, rx, move |view, message| {
            let GitMessage::Init { result } = message else {
                return false;
            };
            view.git_busy = false;
            match result {
                Ok(()) => {
                    view.notice = Some(view.t("git.init_done").to_string());
                    view.git_refreshed_at = None;
                    view.git_history_stale = true;
                }
                Err(error) => {
                    view.notice = Some(view.tf("git.init_failed", &[("error", &error)]));
                }
            }
            true
        });
    }

    fn git_tooltip(
        &self,
        cx: &mut Context<Self>,
        command: &str,
        description: impl Into<String>,
    ) -> gpui::AnyView {
        cx.new(|_| GitTooltip {
            command: command.to_string(),
            description: description.into(),
        })
        .into()
    }

    fn git_status_key(status: WorktreeStatus) -> &'static str {
        match status {
            WorktreeStatus::Modified => "git.status.modified",
            WorktreeStatus::Added => "git.status.added",
            WorktreeStatus::Untracked => "git.status.new",
            WorktreeStatus::Deleted => "git.status.deleted",
            WorktreeStatus::Renamed => "git.status.renamed",
            WorktreeStatus::Copied => "git.status.copied",
            WorktreeStatus::Conflicted => "git.status.conflict",
            WorktreeStatus::Unknown | WorktreeStatus::Unmodified => "git.status.unknown",
        }
    }

    fn git_diff_overlay(&self, cx: &mut Context<Self>) -> gpui::Div {
        let state = self.git_diff.as_ref().expect("overlay only when open");
        let mut lines = div()
            .id("git-diff-lines")
            .max_h(px(380.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .p_2()
            .bg(rgb(theme::bg()));
        for line in state.diff.lines() {
            let color = if line.starts_with('+') {
                theme::git_added_fg()
            } else if line.starts_with('-') {
                theme::agent_err_fg()
            } else if line.starts_with("@@") {
                theme::cue_fg()
            } else {
                theme::status_fg()
            };
            lines = lines.child(
                div()
                    .whitespace_nowrap()
                    .text_xs()
                    .text_color(rgb(color))
                    .child(line.to_string()),
            );
        }
        div()
            .absolute()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000099))
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .on_mouse_up(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .child(
                div()
                    .w(px(820.0))
                    .max_h(px(560.0))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_4()
                    .bg(rgb(theme::explorer_bg()))
                    .border_1()
                    .border_color(rgb(theme::explorer_border()))
                    .child(
                        div()
                            .text_color(rgb(theme::default_fg()))
                            .child(self.t("git.diff_title")),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(theme::status_fg()))
                            .child(state.path.clone()),
                    )
                    .when(state.loading, |card| {
                        card.child(
                            div()
                                .text_color(rgb(theme::status_fg()))
                                .child(self.t("git.diff_loading")),
                        )
                    })
                    .when(!state.error.is_empty(), |card| {
                        card.child(
                            div()
                                .text_color(rgb(theme::agent_err_fg()))
                                .child(state.error.clone()),
                        )
                    })
                    .when(!state.loading && state.error.is_empty(), |card| {
                        card.child(if state.diff.is_empty() {
                            div()
                                .text_color(rgb(theme::status_fg()))
                                .child(self.t("git.diff_empty"))
                                .into_any_element()
                        } else {
                            lines.into_any_element()
                        })
                    })
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                div()
                                    .id("git-diff-close")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(theme::status_bg()))
                                    .text_color(rgb(theme::status_fg()))
                                    .cursor_pointer()
                                    .child(self.t("git.close"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.git_diff = None;
                                        cx.notify();
                                    })),
                            )
                            .when(
                                matches!(
                                    self.git.file_status(&state.path),
                                    Some(WorktreeStatus::Modified | WorktreeStatus::Deleted)
                                ),
                                |row| {
                                    row.child(
                                        div()
                                            .id("git-diff-restore")
                                            .px_3()
                                            .py_1()
                                            .bg(rgb(theme::status_bg()))
                                            .text_color(rgb(theme::agent_err_fg()))
                                            .cursor_pointer()
                                            .child(self.t("git.restore"))
                                            .on_click(cx.listener({
                                                let path = state.path.clone();
                                                move |this, _, _, cx| {
                                                    this.open_git_restore(path.clone(), cx)
                                                }
                                            })),
                                    )
                                },
                            )
                            .when(
                                !matches!(
                                    self.git.file_status(&state.path),
                                    Some(WorktreeStatus::Modified | WorktreeStatus::Deleted)
                                ),
                                |row| {
                                    row.child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(theme::gutter_fg()))
                                            .child(self.t("git.no_previous_version")),
                                    )
                                },
                            ),
                    ),
            )
    }

    fn git_restore_overlay(&self, cx: &mut Context<Self>) -> gpui::Div {
        let prompt = self.git_restore.as_ref().expect("overlay only when open");
        div()
            .absolute()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000099))
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .on_mouse_up(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .child(
                div()
                    .w(px(600.0))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_4()
                    .bg(rgb(theme::explorer_bg()))
                    .border_1()
                    .border_color(rgb(theme::agent_err_fg()))
                    .child(
                        div()
                            .text_color(rgb(theme::default_fg()))
                            .child(self.t("git.restore_title")),
                    )
                    .child(
                        div()
                            .text_color(rgb(theme::status_fg()))
                            .child(self.tf("git.restore_body", &[("path", &prompt.path)])),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(theme::gutter_fg()))
                            .child(self.t("git.restore_hint")),
                    )
                    .when(!prompt.error.is_empty(), |card| {
                        card.child(
                            div()
                                .text_color(rgb(theme::agent_err_fg()))
                                .child(prompt.error.clone()),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                div()
                                    .id("git-restore-confirm")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(theme::agent_err_fg()))
                                    .text_color(rgb(theme::default_fg()))
                                    .cursor_pointer()
                                    .child(self.t("git.restore_confirm"))
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.confirm_git_restore(cx)),
                                    ),
                            )
                            .child(
                                div()
                                    .id("git-restore-cancel")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(theme::status_bg()))
                                    .text_color(rgb(theme::status_fg()))
                                    .cursor_pointer()
                                    .child(self.t("git.close"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.git_restore = None;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
    }

    fn git_branch_prompt_overlay(&self, cx: &mut Context<Self>) -> gpui::Div {
        let state = self
            .git_branch_prompt
            .as_ref()
            .expect("overlay only when open");
        div()
            .absolute()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000099))
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .on_mouse_up(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .child(
                div()
                    .w(px(560.0))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_4()
                    .bg(rgb(theme::explorer_bg()))
                    .border_1()
                    .border_color(rgb(theme::explorer_border()))
                    .child(
                        div()
                            .text_color(rgb(theme::default_fg()))
                            .child(self.t("git.branch_title")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(theme::gutter_fg()))
                            .child(self.t("git.branch_hint")),
                    )
                    .child(
                        div()
                            .id("git-branch-name")
                            .px_2()
                            .py_1()
                            .bg(rgb(theme::bg()))
                            .border_1()
                            .border_color(rgb(theme::default_fg()))
                            .text_color(rgb(if state.name.is_empty() {
                                theme::gutter_fg()
                            } else {
                                theme::default_fg()
                            }))
                            .child(if state.name.is_empty() {
                                self.t("git.branch_placeholder").to_string()
                            } else {
                                format!("{}▌", state.name)
                            }),
                    )
                    .when(!state.error.is_empty(), |card| {
                        card.child(
                            div()
                                .text_color(rgb(theme::agent_err_fg()))
                                .child(state.error.clone()),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                div()
                                    .id("git-branch-save")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(theme::status_bg()))
                                    .text_color(rgb(theme::default_fg()))
                                    .cursor_pointer()
                                    .child(self.t("git.branch_save"))
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.submit_branch_prompt(cx)),
                                    ),
                            )
                            .child(
                                div()
                                    .id("git-branch-cancel")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(theme::status_bg()))
                                    .text_color(rgb(theme::status_fg()))
                                    .cursor_pointer()
                                    .child(self.t("git.close"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.git_branch_prompt = None;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
    }

    fn github_publish_overlay(&self, cx: &mut Context<Self>) -> gpui::Div {
        let state = self
            .github_publish
            .as_ref()
            .expect("overlay only when open");
        let name_display = if state.name.is_empty() {
            self.t("github.repo_placeholder").to_string()
        } else {
            format!("{}▌", state.name)
        };
        div()
            .absolute()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000099))
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .on_mouse_up(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .child(
                div()
                    .w(px(620.0))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_4()
                    .bg(rgb(theme::explorer_bg()))
                    .border_1()
                    .border_color(rgb(theme::explorer_border()))
                    .child(
                        div()
                            .text_color(rgb(theme::default_fg()))
                            .child(self.t("github.publish_title")),
                    )
                    .child(
                        div()
                            .text_color(rgb(theme::status_fg()))
                            .child(self.t("github.publish_hint")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(theme::gutter_fg()))
                            .child(self.t("github.repo_name")),
                    )
                    .child(
                        div()
                            .id("github-publish-name")
                            .px_2()
                            .py_1()
                            .bg(rgb(theme::bg()))
                            .border_1()
                            .border_color(rgb(theme::default_fg()))
                            .text_color(rgb(if state.name.is_empty() {
                                theme::gutter_fg()
                            } else {
                                theme::default_fg()
                            }))
                            .child(name_display),
                    )
                    .child(
                        div()
                            .id("github-publish-private")
                            .px_2()
                            .py_1()
                            .flex()
                            .items_center()
                            .gap_1()
                            .cursor_pointer()
                            .text_sm()
                            .text_color(rgb(theme::status_fg()))
                            .child(format!("[{}]", if state.private { "x" } else { " " }))
                            .child(self.t("github.private"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(state) = this.github_publish.as_mut() {
                                    state.private = !state.private;
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(theme::gutter_fg()))
                            .child(self.t("github.publish_confirm_hint")),
                    )
                    .when(!state.error.is_empty(), |card| {
                        card.child(
                            div()
                                .text_color(rgb(theme::agent_err_fg()))
                                .child(state.error.clone()),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                div()
                                    .id("github-publish-confirm")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(theme::status_bg()))
                                    .text_color(rgb(if state.busy {
                                        theme::gutter_fg()
                                    } else {
                                        theme::default_fg()
                                    }))
                                    .cursor_pointer()
                                    .child(if state.busy {
                                        self.t("github.publishing").to_string()
                                    } else {
                                        self.t("github.publish_confirm").to_string()
                                    })
                                    .on_click(
                                        cx.listener(|this, _, _, cx| {
                                            this.submit_github_publish(cx)
                                        }),
                                    ),
                            )
                            .child(
                                div()
                                    .id("github-publish-cancel")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(theme::status_bg()))
                                    .text_color(rgb(theme::status_fg()))
                                    .cursor_pointer()
                                    .child(self.t("git.close"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if !this.git_busy {
                                            this.github_publish = None;
                                        }
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
    }

    fn git_sync_overlay(&self, cx: &mut Context<Self>) -> gpui::Div {
        let state = self.git_sync.as_ref().expect("overlay only when open");
        let remote = self
            .github_remote_info()
            .map(|(remote, owner, repo)| format!("{remote} · {owner}/{repo}"))
            .unwrap_or_default();
        let branch = if self.git.branch.is_empty() {
            "main".to_string()
        } else {
            self.git.branch.clone()
        };
        let (title, body, hint, confirm_label) = match state.kind {
            GitSyncKind::Push => (
                self.t("github.push_title"),
                self.tf(
                    "github.push_body",
                    &[("remote", &remote), ("branch", &branch)],
                ),
                self.t("github.push_hint"),
                self.t("github.push_confirm"),
            ),
            GitSyncKind::Pull => (
                self.t("github.pull_title"),
                self.tf(
                    "github.pull_body",
                    &[("remote", &remote), ("branch", &branch)],
                ),
                self.t("github.pull_hint"),
                self.t("github.pull_confirm"),
            ),
        };
        div()
            .absolute()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000099))
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .on_mouse_up(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .child(
                div()
                    .w(px(620.0))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_4()
                    .bg(rgb(theme::explorer_bg()))
                    .border_1()
                    .border_color(rgb(theme::explorer_border()))
                    .child(div().text_color(rgb(theme::default_fg())).child(title))
                    .child(div().text_color(rgb(theme::status_fg())).child(body))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(theme::gutter_fg()))
                            .child(hint),
                    )
                    .when(!state.error.is_empty(), |card| {
                        card.child(
                            div()
                                .text_color(rgb(theme::agent_err_fg()))
                                .child(state.error.clone()),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                div()
                                    .id("git-sync-confirm")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(theme::status_bg()))
                                    .text_color(rgb(if state.busy {
                                        theme::gutter_fg()
                                    } else {
                                        theme::default_fg()
                                    }))
                                    .cursor_pointer()
                                    .child(if state.busy {
                                        self.t("github.working").to_string()
                                    } else {
                                        confirm_label.to_string()
                                    })
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.confirm_git_sync(cx)),
                                    ),
                            )
                            .child(
                                div()
                                    .id("git-sync-cancel")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(theme::status_bg()))
                                    .text_color(rgb(theme::status_fg()))
                                    .cursor_pointer()
                                    .child(self.t("git.close"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if !this.git_busy {
                                            this.git_sync = None;
                                        }
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
    }

    fn github_pr_overlay(&self, cx: &mut Context<Self>) -> gpui::Div {
        let state = self.github_pr.as_ref().expect("overlay only when open");
        let field = |index: u8, active: bool, text: String| {
            let color = if active {
                theme::default_fg()
            } else {
                theme::status_fg()
            };
            div()
                .id(("github-pr-field", index as usize))
                .px_2()
                .py_1()
                .flex_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .bg(rgb(theme::bg()))
                .border_1()
                .border_color(rgb(if active {
                    theme::default_fg()
                } else {
                    theme::explorer_border()
                }))
                .text_color(rgb(color))
                .child(if active { format!("{text}▌") } else { text })
        };
        div()
            .absolute()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000099))
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .on_mouse_up(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .child(
                div()
                    .w(px(620.0))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_4()
                    .bg(rgb(theme::explorer_bg()))
                    .border_1()
                    .border_color(rgb(theme::explorer_border()))
                    .child(
                        div()
                            .text_color(rgb(theme::default_fg()))
                            .child(self.t("github.pr_title")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(theme::gutter_fg()))
                            .child(self.t("github.pr_hint")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(90.0))
                                    .text_xs()
                                    .text_color(rgb(theme::gutter_fg()))
                                    .child(self.t("github.pr_title_label")),
                            )
                            .child(field(0, state.field == 0, state.title.clone()).on_click(
                                cx.listener(|this, _, _, cx| {
                                    if let Some(state) = this.github_pr.as_mut() {
                                        state.field = 0;
                                    }
                                    cx.notify();
                                }),
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(90.0))
                                    .text_xs()
                                    .text_color(rgb(theme::gutter_fg()))
                                    .child(self.t("github.pr_body_label")),
                            )
                            .child(field(1, state.field == 1, state.body.clone()).on_click(
                                cx.listener(|this, _, _, cx| {
                                    if let Some(state) = this.github_pr.as_mut() {
                                        state.field = 1;
                                    }
                                    cx.notify();
                                }),
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(90.0))
                                    .text_xs()
                                    .text_color(rgb(theme::gutter_fg()))
                                    .child(self.t("github.pr_base_label")),
                            )
                            .child(field(2, state.field == 2, state.base.clone()).on_click(
                                cx.listener(|this, _, _, cx| {
                                    if let Some(state) = this.github_pr.as_mut() {
                                        state.field = 2;
                                    }
                                    cx.notify();
                                }),
                            )),
                    )
                    .when(!state.error.is_empty(), |card| {
                        card.child(
                            div()
                                .text_color(rgb(theme::agent_err_fg()))
                                .child(state.error.clone()),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                div()
                                    .id("github-pr-create")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(theme::status_bg()))
                                    .text_color(rgb(if state.busy {
                                        theme::gutter_fg()
                                    } else {
                                        theme::default_fg()
                                    }))
                                    .cursor_pointer()
                                    .child(if state.busy {
                                        self.t("github.working").to_string()
                                    } else {
                                        self.t("github.pr_create").to_string()
                                    })
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.submit_github_pr(cx)),
                                    ),
                            )
                            .child(
                                div()
                                    .id("github-pr-cancel")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(theme::status_bg()))
                                    .text_color(rgb(theme::status_fg()))
                                    .cursor_pointer()
                                    .child(self.t("git.close"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if !this.git_busy {
                                            this.github_pr = None;
                                        }
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
    }

    fn git_commit_overlay(&self, cx: &mut Context<Self>) -> gpui::Div {
        let state = self.git_commit.as_ref().expect("overlay only when open");
        let changed = state.selected.len();
        div()
            .absolute()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000099))
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .on_mouse_up(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .child(
                div()
                    .w(px(620.0))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_4()
                    .bg(rgb(theme::explorer_bg()))
                    .border_1()
                    .border_color(rgb(theme::explorer_border()))
                    .child(
                        div()
                            .text_color(rgb(theme::default_fg()))
                            .child(self.t("git.commit_title")),
                    )
                    .child(
                        div()
                            .text_color(rgb(theme::status_fg()))
                            .child(self.tf("git.commit_files", &[("n", &changed.to_string())])),
                    )
                    .child({
                        let mut files = div()
                            .id("git-commit-files")
                            .max_h(px(160.0))
                            .overflow_y_scroll()
                            .flex()
                            .flex_col()
                            .border_1()
                            .border_color(rgb(theme::explorer_border()));
                        let mut paths: Vec<&String> = self.git.files.keys().collect();
                        paths.sort();
                        for (index, path) in paths.into_iter().enumerate() {
                            let checked = state.selected.contains(path);
                            let status = self.git.file_status(path);
                            let label = status
                                .map(|status| self.t(Self::git_status_key(status)).to_string())
                                .unwrap_or_default();
                            let path_owned = path.clone();
                            files = files.child(
                                div()
                                    .id(("git-commit-file", index))
                                    .px_2()
                                    .py_1()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .cursor_pointer()
                                    .text_xs()
                                    .text_color(rgb(if checked {
                                        theme::default_fg()
                                    } else {
                                        theme::gutter_fg()
                                    }))
                                    .child(format!("[{}]", if checked { "x" } else { " " }))
                                    .child(
                                        div()
                                            .flex_1()
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .child(path_owned.clone()),
                                    )
                                    .child(label)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.toggle_git_commit_path(&path_owned, cx)
                                    })),
                            );
                        }
                        files
                    })
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(theme::gutter_fg()))
                            .child(self.tf(
                                "git.commit_author",
                                &[
                                    ("name", &self.app_settings.git_name),
                                    ("email", &self.app_settings.git_email),
                                ],
                            )),
                    )
                    .child(
                        div()
                            .id("git-commit-message")
                            .min_h(px(42.0))
                            .px_2()
                            .py_1()
                            .bg(rgb(theme::bg()))
                            .border_1()
                            .border_color(rgb(theme::default_fg()))
                            .text_color(rgb(if state.message.is_empty() {
                                theme::gutter_fg()
                            } else {
                                theme::default_fg()
                            }))
                            .child(format!(
                                "{}▌",
                                if state.message.is_empty() {
                                    self.t("git.commit_placeholder").to_string()
                                } else {
                                    state.message.clone()
                                }
                            )),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(theme::gutter_fg()))
                            .child(self.t("git.commit_help")),
                    )
                    .when(!state.error.is_empty(), |card| {
                        card.child(
                            div()
                                .text_color(rgb(theme::agent_err_fg()))
                                .child(state.error.clone()),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                div()
                                    .id("git-save-version")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(if state.busy {
                                        theme::gutter_fg()
                                    } else {
                                        theme::status_bg()
                                    }))
                                    .text_color(rgb(theme::default_fg()))
                                    .cursor_pointer()
                                    .child(if state.busy {
                                        self.t("git.saving")
                                    } else {
                                        self.t("git.commit_save")
                                    })
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.submit_git_commit(cx)),
                                    ),
                            )
                            .child(
                                div()
                                    .id("git-commit-cancel")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(theme::status_bg()))
                                    .text_color(rgb(theme::status_fg()))
                                    .cursor_pointer()
                                    .child(self.t("git.commit_cancel"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if !this.git_busy {
                                            this.git_commit = None;
                                        }
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
    }

    // ---- crash-safety backups and the quit guard (M9 slice 6) -----------

    /// Start the periodic autosave loop (M9 slice 6): every two seconds a
    /// dirty buffer is written to `.narra/backups/` — never the canonical
    /// file. The loop lives with the window; the dump reports the backup.
    fn start_autosave(&mut self, window: &Window, cx: &Context<Self>) {
        if self.autosave_started {
            return;
        }
        self.autosave_started = true;
        let entity = cx.entity();
        window
            .spawn(cx, async move |cx| {
                loop {
                    Timer::after(Duration::from_millis(2000)).await;
                    let alive =
                        cx.update(|_window, cx| entity.update(cx, |view, cx| view.autosave(cx)));
                    if alive.is_err() {
                        break;
                    }
                }
            })
            .detach();
    }

    fn autosave(&mut self, _cx: &mut Context<Self>) {
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let root = project.root().to_path_buf();
        // Every dirty tab gets a recovery copy, not only the visible one.
        let mut work: Vec<(String, String)> = Vec::new();
        if self.doc.is_dirty() && !self.doc.path.is_empty() {
            work.push((self.doc.path.clone(), self.full_text()));
        }
        for tab in &self.tabs {
            if let Some(doc) = tab.document.as_ref()
                && doc.is_dirty()
                && !doc.path.is_empty()
            {
                work.push((doc.path.clone(), doc.buffer.slice(0..doc.buffer.doc_len())));
            }
        }
        for (document, text) in work {
            match narra_core::backup::write_backup(&root, &document, &text) {
                Ok(path) => {
                    if document == self.doc.path {
                        self.doc.backup_path = Some(path.display().to_string());
                        self.doc.backup_saved_at = Some(Instant::now());
                        self.doc.backup_error = None;
                    } else if let Some(doc) = self
                        .tabs
                        .iter_mut()
                        .find_map(|tab| tab.document.as_mut().filter(|doc| doc.path == document))
                    {
                        doc.backup_path = Some(path.display().to_string());
                        doc.backup_saved_at = Some(Instant::now());
                        doc.backup_error = None;
                    }
                }
                Err(error) => {
                    if document == self.doc.path {
                        self.doc.backup_error = Some(error.to_string());
                    } else if let Some(doc) = self
                        .tabs
                        .iter_mut()
                        .find_map(|tab| tab.document.as_mut().filter(|doc| doc.path == document))
                    {
                        doc.backup_error = Some(error.to_string());
                    }
                }
            }
        }
    }

    /// Notice when a backup exists and differs from the loaded document.
    fn check_backup_notice(&mut self) {
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let root = project.root().to_path_buf();
        let document = self.doc.path.clone();
        if let Ok(Some(text)) = narra_core::backup::read_backup(&root, &document)
            && text != self.full_text()
        {
            self.notice = Some(self.t("notice.backup_found").to_string());
        }
    }

    /// ⌘⇧B: restore the backup into the buffer (unsaved changes; ⌘S decides).
    fn on_restore_backup(
        &mut self,
        _: &RestoreBackup,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.project.as_ref() else {
            self.notice = Some(self.t("notice.fixture_no_project").to_string());
            cx.notify();
            return;
        };
        let root = project.root().to_path_buf();
        let document = self.doc.path.clone();
        match narra_core::backup::read_backup(&root, &document) {
            Ok(Some(text)) => {
                self.replace_buffer_text(&text);
                self.notice = Some(self.t("notice.backup_restored").to_string());
            }
            Ok(None) => self.notice = Some(self.t("notice.backup_missing").to_string()),
            Err(error) => {
                self.notice =
                    Some(self.tf("notice.backup_failed", &[("error", &error.to_string())]))
            }
        }
        cx.notify();
    }

    /// ⌘⇧D: discard the backup.
    fn on_discard_backup(
        &mut self,
        _: &DiscardBackup,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.project.as_ref() else {
            self.notice = Some(self.t("notice.fixture_no_project").to_string());
            cx.notify();
            return;
        };
        let root = project.root().to_path_buf();
        let document = self.doc.path.clone();
        match narra_core::backup::remove_backup(&root, &document) {
            Ok(()) => {
                self.doc.backup_path = None;
                self.doc.backup_saved_at = None;
                self.notice = Some(self.t("notice.backup_discarded").to_string());
            }
            Err(error) => {
                self.notice =
                    Some(self.tf("notice.backup_failed", &[("error", &error.to_string())]))
            }
        }
        cx.notify();
    }

    /// Replace the buffer contents in place, keeping the canonical base: the
    /// restored text is unsaved (dirty) until the user saves.
    fn replace_buffer_text(&mut self, text: &str) {
        self.doc.buffer = Buffer::new(text);
        self.doc.selected = 0..0;
        self.doc.reversed = false;
        self.doc.marked = None;
        self.doc.composing = false;
        self.doc.dragging = None;
        self.doc.undo_stack.clear();
        self.doc.redo_stack.clear();
        self.doc.scene_lines.clear();
        self.doc.outline.clear();
        self.doc.elements_dirty = true;
        self.doc.elements_parsed_at = None;
        self.doc.scenes_dirty = true;
        self.doc.find.matches.clear();
        self.doc.find.current = 0;
        self.doc.diff.open = false;
        self.doc.list_state = ListState::new(
            self.doc.buffer.line_count(),
            ListAlignment::Top,
            px(theme::line_height() * 2.0),
        );
        // The restored text diverges from the canonical base: mark it dirty
        // without touching the canonical file.
        self.metrics.borrow_mut().edits += 1;
        self.bump_revision();
    }

    /// ⌘Q (M9 slice 6, tabs in S3b): never lose unsaved work silently. Any
    /// dirty tab opens the save/discard/cancel prompt; a clean workspace
    /// quits. Pressing the action again while the prompt is open dismisses it.
    fn on_quit(&mut self, _: &Quit, _window: &mut Window, cx: &mut Context<Self>) {
        if self.context_menu.is_some() {
            self.context_menu = None;
            cx.notify();
            return;
        }
        if self.rename.is_some() {
            self.rename = None;
            cx.notify();
            return;
        }
        if self.delete_prompt.is_some() {
            self.delete_prompt = None;
            cx.notify();
            return;
        }
        if self.git_commit.is_some() {
            if !self.git_busy {
                self.git_commit = None;
            }
            cx.notify();
            return;
        }
        if self.git_branch_prompt.is_some() {
            self.git_branch_prompt = None;
            cx.notify();
            return;
        }
        if self.github_publish.is_some() {
            if !self.git_busy {
                self.github_publish = None;
            }
            cx.notify();
            return;
        }
        if self.git_sync.is_some() {
            if !self.git_busy {
                self.git_sync = None;
            }
            cx.notify();
            return;
        }
        if self.github_pr.is_some() {
            if !self.git_busy {
                self.github_pr = None;
            }
            cx.notify();
            return;
        }
        if self.git_restore.is_some() {
            self.git_restore = None;
            cx.notify();
            return;
        }
        if self.git_diff.is_some() {
            self.git_diff = None;
            cx.notify();
            return;
        }
        if self.palette.is_some() {
            self.palette = None;
            cx.notify();
            return;
        }
        if self.settings.is_some() {
            self.settings = None;
            cx.notify();
            return;
        }
        if self.project_settings.is_some() {
            self.project_settings = None;
            cx.notify();
            return;
        }
        if self.unsaved_prompt.is_some() {
            self.unsaved_prompt = None;
            cx.notify();
            return;
        }
        let dirty = self.dirty_tab_count();
        if dirty > 0 {
            self.unsaved_prompt = Some(UnsavedPrompt {
                action: PendingUnsaved::Quit,
                dirty,
            });
            cx.notify();
            return;
        }
        self.finish_quit(cx);
    }

    /// Close the active tab (⌘W) or a clicked tab. A dirty tab goes through
    /// the prompt; a clean tab closes immediately, keeping the editor on a
    /// neighbour (or a blank tab when it was the last one).
    fn request_close_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.tabs.len()
            || self.unsaved_prompt.is_some()
            || self.settings.is_some()
            || self.project_settings.is_some()
            || self.palette.is_some()
            || self.rename.is_some()
            || self.delete_prompt.is_some()
        {
            return;
        }
        if self.tab_is_dirty(index) {
            self.unsaved_prompt = Some(UnsavedPrompt {
                action: PendingUnsaved::CloseTab(index),
                dirty: 1,
            });
            cx.notify();
            return;
        }
        self.close_tab_at(index);
        window.focus(&self.focus_handle(cx));
        cx.notify();
    }

    fn on_close_tab(&mut self, _: &CloseTab, window: &mut Window, cx: &mut Context<Self>) {
        self.request_close_tab(self.active_tab, window, cx);
    }

    fn switch_tab(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.unsaved_prompt.is_some() {
            return;
        }
        let Some(index) = next_tab_index(self.active_tab, self.tabs.len(), forward) else {
            return;
        };
        if self.activate_tab(index) {
            window.focus(&self.focus_handle(cx));
            cx.notify();
        }
    }

    fn on_next_tab(&mut self, _: &NextTab, window: &mut Window, cx: &mut Context<Self>) {
        self.switch_tab(true, window, cx);
    }

    fn on_prev_tab(&mut self, _: &PrevTab, window: &mut Window, cx: &mut Context<Self>) {
        self.switch_tab(false, window, cx);
    }

    /// Save branch of the prompt: save every affected document, then finish
    /// the pending action. A failed save keeps the prompt/operation blocked.
    fn prompt_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(prompt) = self.unsaved_prompt else {
            return;
        };
        match prompt.action {
            PendingUnsaved::Quit => {
                if self.save_all_tabs(window, cx) {
                    self.unsaved_prompt = None;
                    self.finish_quit(cx);
                } else {
                    cx.notify();
                }
            }
            PendingUnsaved::CloseTab(index) => {
                if index >= self.tabs.len() {
                    // Defensive: a stale index must cancel, never delete a
                    // different tab that happened to take the slot.
                    self.unsaved_prompt = None;
                    cx.notify();
                    return;
                }
                if self.save_tab(index, window, cx) {
                    self.unsaved_prompt = None;
                    self.close_tab_at(index);
                    window.focus(&self.focus_handle(cx));
                    cx.notify();
                } else {
                    cx.notify();
                }
            }
        }
    }

    /// Discard branch: drop the affected buffer(s) without writing.
    fn prompt_discard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(prompt) = self.unsaved_prompt.take() else {
            return;
        };
        match prompt.action {
            PendingUnsaved::Quit => self.finish_quit(cx),
            PendingUnsaved::CloseTab(index) => {
                self.close_tab_at(index);
                window.focus(&self.focus_handle(cx));
                cx.notify();
            }
        }
    }

    fn prompt_cancel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.unsaved_prompt = None;
        cx.notify();
    }

    fn finish_quit(&mut self, cx: &mut Context<Self>) {
        println!("{}", self.metrics.borrow().report());
        cx.quit();
    }

    /// The unsaved-changes prompt: a centered card over the window with save
    /// / discard / cancel. It owns the keyboard while open (typing cannot
    /// reach the buffer) and spells out how many documents are at stake.
    fn quit_overlay(&self, cx: &mut Context<Self>) -> gpui::Div {
        let dirty = self.unsaved_prompt.map(|prompt| prompt.dirty).unwrap_or(0);
        let body = if dirty > 1 {
            self.tf("quit.body_many", &[("n", &dirty.to_string())])
        } else {
            self.t("quit.body").to_string()
        };
        let save_label = if dirty > 1 {
            self.t("quit.save_all")
        } else {
            self.t("quit.save")
        };
        let discard_label = if dirty > 1 {
            self.t("quit.discard_all")
        } else {
            self.t("quit.discard")
        };
        div()
            .absolute()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000099))
            // A full-screen interactive backdrop swallows clicks so the
            // editor/explorer behind a prompt cannot switch tabs or close
            // documents while the prompt is open.
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .on_mouse_up(MouseButton::Left, cx.listener(|_, _, _, _| {}))
            .child(
                div()
                    .w(px(520.0))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_4()
                    .bg(rgb(theme::explorer_bg()))
                    .border_1()
                    .border_color(rgb(theme::explorer_border()))
                    .child(
                        div()
                            .text_color(rgb(theme::default_fg()))
                            .child(self.t("quit.title")),
                    )
                    .child(div().text_color(rgb(theme::status_fg())).child(body))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                div()
                                    .id("quit-save")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(theme::status_bg()))
                                    .text_color(rgb(theme::default_fg()))
                                    .cursor_pointer()
                                    .child(save_label)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.prompt_save(window, cx)
                                    })),
                            )
                            .child(
                                div()
                                    .id("quit-discard")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(theme::status_bg()))
                                    .text_color(rgb(theme::agent_err_fg()))
                                    .cursor_pointer()
                                    .child(discard_label)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.prompt_discard(window, cx)
                                    })),
                            )
                            .child(
                                div()
                                    .id("quit-cancel")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(theme::status_bg()))
                                    .text_color(rgb(theme::status_fg()))
                                    .cursor_pointer()
                                    .child(self.t("quit.cancel"))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.prompt_cancel(window, cx)
                                    })),
                            ),
                    ),
            )
    }

    fn on_backspace(&mut self, _: &Backspace, _window: &mut Window, cx: &mut Context<Self>) {
        if self.unsaved_prompt.is_some() {
            return;
        }
        if self.context_menu.is_some() {
            self.context_menu = None;
            cx.notify();
            return;
        }
        if self.rename.is_some() {
            self.rename_backspace(cx);
            return;
        }
        if self.delete_prompt.is_some() {
            return;
        }
        if self.git_branch_prompt.is_some() {
            self.branch_prompt_backspace(cx);
            return;
        }
        if self.github_publish.is_some() {
            self.github_publish_backspace(cx);
            return;
        }
        if self.github_pr.is_some() {
            self.github_pr_backspace(cx);
            return;
        }
        if self.git_commit.is_some() {
            self.git_commit_backspace(cx);
            return;
        }
        if self.git_diff.is_some() || self.git_restore.is_some() {
            return;
        }
        if self.chat.focused {
            self.chat.backspace();
            cx.notify();
            return;
        }
        if self.palette.is_some() {
            self.palette_backspace(cx);
            return;
        }
        if self.settings.is_some() {
            self.settings_backspace(cx);
            return;
        }
        if self.project_settings.is_some() {
            self.project_settings_backspace(cx);
            return;
        }
        if self.new_file.is_some() {
            if let Some(state) = self.new_file.as_mut() {
                if let Some(last) = state.name.chars().last() {
                    let len = state.name.len();
                    state.name.truncate(len - last.len_utf8());
                }
                state.error.clear();
            }
            cx.notify();
            return;
        }
        // The New Project dialog owns its title/prompt field (M10 slice 2b,
        // M16 slice 9).
        if self.new_project.is_some() {
            if let Some(state) = self.new_project.as_mut()
                && !state.picking
                && !state.busy
                && state.plan.is_none()
            {
                let target =
                    if state.mode == NewProjectMode::Ai && state.field == NewProjectField::Prompt {
                        &mut state.prompt
                    } else {
                        &mut state.title
                    };
                if let Some(last) = target.chars().last() {
                    let len = target.len();
                    target.truncate(len - last.len_utf8());
                }
                state.error.clear();
            }
            cx.notify();
            return;
        }
        if self.doc.find.open {
            self.find_backspace();
            cx.notify();
            return;
        }
        if self.search.open {
            if let Some(last) = self.search.query.chars().last() {
                let len = self.search.query.len();
                self.search.query.truncate(len - last.len_utf8());
            }
            cx.notify();
            return;
        }
        if self.propose_bar.open {
            // Grapheme-aware erase, same standard as the find field.
            if let Some(last) = self.propose_bar.instruction.chars().last() {
                let len = self.propose_bar.instruction.len();
                self.propose_bar.instruction.truncate(len - last.len_utf8());
            }
            cx.notify();
            return;
        }
        if self.focus_bar.open {
            if let Some(last) = self.focus_bar.focus.chars().last() {
                let len = self.focus_bar.focus.len();
                self.focus_bar.focus.truncate(len - last.len_utf8());
            }
            cx.notify();
            return;
        }
        if self.doc.preview {
            return;
        }
        if self.doc.selected.is_empty() {
            let caret = self.caret();
            let previous = self.doc.buffer.prev_boundary(caret);
            if previous < caret {
                self.apply_edit(previous..caret, "");
            }
        } else {
            let range = self.doc.selected.clone();
            self.apply_edit(range, "");
        }
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_delete(&mut self, _: &DeleteText, _window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.is_some()
            || self.project_settings.is_some()
            || self.palette.is_some()
            || self.unsaved_prompt.is_some()
            || self.doc.preview
        {
            return;
        }
        if self.doc.selected.is_empty() {
            let caret = self.caret();
            let next = self.doc.buffer.next_boundary(caret);
            if next > caret {
                self.apply_edit(caret..next, "");
            }
        } else {
            let range = self.doc.selected.clone();
            self.apply_edit(range, "");
        }
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_insert_newline(
        &mut self,
        _: &InsertNewline,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The unsaved-changes prompt owns enter (M9 slice 6): the default
        // button is "save and close" (macOS convention), Esc cancels.
        if self.unsaved_prompt.is_some() {
            self.prompt_save(window, cx);
            return;
        }
        if self.context_menu.is_some() {
            self.context_menu_activate_selected(window, cx);
            return;
        }
        if self.rename.is_some() {
            self.rename_save(cx);
            return;
        }
        if self.delete_prompt.is_some() {
            self.delete_prompt = None;
            cx.notify();
            return;
        }
        if self.git_branch_prompt.is_some() {
            self.submit_branch_prompt(cx);
            return;
        }
        if self.github_publish.is_some() {
            self.submit_github_publish(cx);
            return;
        }
        if self.git_sync.is_some() {
            self.confirm_git_sync(cx);
            return;
        }
        if self.github_pr.is_some() {
            self.submit_github_pr(cx);
            return;
        }
        if self.git_diff.is_some() || self.git_restore.is_some() {
            return;
        }
        if self.git_commit.is_some() {
            self.submit_git_commit(cx);
            return;
        }
        if self.chat.focused {
            self.chat_send(window, cx);
            return;
        }
        if self.palette.is_some() {
            self.palette_activate(window, cx);
            return;
        }
        if self.settings.is_some() {
            self.settings_save(cx);
            return;
        }
        if self.project_settings.is_some() {
            self.project_settings_save(cx);
            return;
        }
        // The New File modal creates on enter (M10 slice 2).
        if self.new_file.is_some() {
            self.create_new_file(window, cx);
            return;
        }
        // The New Project dialog accepts on enter. Choosing the folder is a
        // separate button; enter never opens the OS panel.
        if self.new_project.is_some() {
            self.accept_new_project(cx);
            return;
        }
        // While the find bar owns keystrokes, enter means "next match" in the
        // query field and "replace current + advance" in the replacement
        // field (the routing note in the module header).
        if self.doc.find.open {
            match self.doc.find.field {
                FindField::Query => self.next_match(cx),
                FindField::Replacement => {
                    self.replace_current(cx);
                    self.next_match(cx);
                }
            }
            return;
        }
        // While the instruction bar owns keystrokes, enter submits the
        // proposal (M6 slice 4).
        if self.propose_bar.open {
            self.submit_proposal(cx);
            return;
        }
        // While the search bar owns keystrokes, enter searches (first press)
        // and opens the selected result (second press, query unchanged).
        if self.search.open {
            self.submit_search(cx);
            return;
        }
        // While the focus bar owns keystrokes, enter commits the focus and
        // runs the review (M6 slice 5).
        if self.focus_bar.open {
            self.submit_focus(cx);
            return;
        }
        if self.doc.preview {
            return;
        }
        // Smart Enter (M9 slice 5): a scene heading, transition or section
        // needs the blank separator line Fountain's block structure uses,
        // so Enter inserts it and lands on the line below; everything else
        // keeps the plain newline (dialogue continues line by line).
        let caret = self.caret();
        let line = self.doc.buffer.position_at(caret).line;
        let inserted = if matches!(
            self.element_kind(line),
            Some(FountainKind::SceneHeading)
                | Some(FountainKind::Transition)
                | Some(FountainKind::Section)
        ) {
            "\n\n"
        } else {
            "\n"
        };
        self.apply_edit(caret..caret, inserted);
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_undo(&mut self, _: &Undo, _window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.is_some()
            || self.project_settings.is_some()
            || self.palette.is_some()
            || self.unsaved_prompt.is_some()
            || self.doc.preview
        {
            return;
        }
        self.undo();
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_redo(&mut self, _: &Redo, _window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.is_some()
            || self.project_settings.is_some()
            || self.palette.is_some()
            || self.unsaved_prompt.is_some()
            || self.doc.preview
        {
            return;
        }
        self.redo();
        cx.notify();
        self.ensure_caret_visible();
    }

    fn on_copy(&mut self, _: &CopyText, _window: &mut Window, cx: &mut Context<Self>) {
        if self.modal_owns_keyboard() {
            return;
        }
        if !self.doc.selected.is_empty() {
            let text = self.doc.buffer.slice(self.doc.selected.clone());
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn on_cut(&mut self, _: &CutText, _window: &mut Window, cx: &mut Context<Self>) {
        if self.modal_owns_keyboard() || self.doc.preview {
            return;
        }
        if !self.doc.selected.is_empty() {
            let text = self.doc.buffer.slice(self.doc.selected.clone());
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            let range = self.doc.selected.clone();
            self.apply_edit(range, "");
            cx.notify();
        }
    }

    fn on_paste(&mut self, _: &PasteText, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    // ---- find / replace (Spike A slice 3) -------------------------------

    /// Open the find bar (cmd-f) or focus its query field if it is already
    /// open; cmd-alt-f opens it with the replacement field focused. A
    /// non-single-line selection prefills the query on first open.
    fn open_find(&mut self, with_replacement: bool, cx: &mut Context<Self>) {
        if self.doc.preview {
            self.doc.preview = false;
        }
        let selection = if self.doc.selected.is_empty() {
            String::new()
        } else {
            self.doc.buffer.slice(self.doc.selected.clone())
        };
        let first_open = !self.doc.find.open;
        self.doc.find.open = true;
        self.doc.find.field = if with_replacement {
            FindField::Replacement
        } else {
            FindField::Query
        };
        if first_open && !selection.is_empty() && !selection.contains('\n') {
            self.doc.find.query = selection;
        }
        self.refresh_matches(true);
        cx.notify();
    }

    fn close_find(&mut self, cx: &mut Context<Self>) {
        self.doc.find.open = false;
        self.doc.find.matches.clear();
        self.doc.find.current = 0;
        cx.notify();
    }

    /// Recompute `find.matches` from `find.query`. `measured` records the
    /// baseline sample (compute time + query→next frame); silent refreshes
    /// after buffer edits keep the baseline free of maintenance noise.
    /// An empty query clears matches without recording anything.
    fn refresh_matches(&mut self, measured: bool) {
        if self.doc.find.query.is_empty() {
            self.doc.find.matches.clear();
            self.doc.find.current = 0;
            return;
        }
        let start = Instant::now();
        self.doc.find.matches = self.doc.buffer.find_all(&self.doc.find.query);
        if measured {
            let mut metrics = self.metrics.borrow_mut();
            metrics.search_compute_ms.push(duration_ms(start.elapsed()));
            metrics.mark_search(Instant::now());
        }
        if self.doc.find.current >= self.doc.find.matches.len() {
            self.doc.find.current = 0;
        }
    }

    /// Reveal and select the match at `index` (wraps).
    fn goto_match(&mut self, index: usize) {
        if self.doc.find.matches.is_empty() {
            return;
        }
        self.doc.find.current = index % self.doc.find.matches.len();
        let range = self.doc.find.matches[self.doc.find.current].clone();
        let line = self.doc.buffer.position_at(range.start).line;
        self.doc.selected = range;
        self.doc.reversed = false;
        self.doc.marked = None;
        self.doc.list_state.scroll_to_reveal_item(line);
    }

    fn next_match(&mut self, cx: &mut Context<Self>) {
        if self.doc.find.matches.is_empty() {
            return;
        }
        let next = (self.doc.find.current + 1) % self.doc.find.matches.len();
        self.goto_match(next);
        cx.notify();
    }

    fn prev_match(&mut self, cx: &mut Context<Self>) {
        if self.doc.find.matches.is_empty() {
            return;
        }
        let len = self.doc.find.matches.len();
        let prev = (self.doc.find.current + len - 1) % len;
        self.goto_match(prev);
        cx.notify();
    }

    /// Keystrokes typed while the bar is open go to the active field
    /// (single-focus spike compromise). Newlines are dropped: fields are
    /// single-line and `Buffer::find_all` matches nothing for `\n` queries.
    fn find_input(&mut self, text: &str) {
        let cleaned: String = text
            .chars()
            .filter(|c| !c.is_control() || *c == ' ')
            .collect();
        match self.doc.find.field {
            FindField::Query => {
                self.doc.find.query.push_str(&cleaned);
                self.refresh_matches(true);
            }
            FindField::Replacement => self.doc.find.replacement.push_str(&cleaned),
        }
    }

    /// Grapheme-aware backspace for the active find field (same standard as
    /// the buffer).
    fn find_backspace(&mut self) {
        let field = match self.doc.find.field {
            FindField::Query => &mut self.doc.find.query,
            FindField::Replacement => &mut self.doc.find.replacement,
        };
        let new_len = field
            .graphemes(true)
            .next_back()
            .map(|g| field.len() - g.len());
        if let Some(len) = new_len {
            field.truncate(len);
        }
        if self.doc.find.field == FindField::Query {
            self.refresh_matches(true);
        }
    }

    /// Replace the current match (cmd-enter, or enter in the replacement
    /// field) and keep the visual position near where the edit happened.
    fn replace_current(&mut self, cx: &mut Context<Self>) {
        if self.doc.find.matches.is_empty() {
            return;
        }
        let range = self.doc.find.matches[self.doc.find.current].clone();
        let anchor = range.start;
        let replacement = self.doc.find.replacement.clone();
        // apply_edit pushes one undo entry and silently refreshes matches.
        self.apply_edit(range, &replacement);
        self.doc.find.replaced_total += 1;
        if !self.doc.find.matches.is_empty() {
            let index = self
                .doc
                .find
                .matches
                .iter()
                .position(|m| m.start >= anchor)
                .unwrap_or(0);
            self.goto_match(index);
        }
        cx.notify();
    }

    /// Replace every match in a single document-wide edit (one undo entry),
    /// measured as one operation into `replace_all_ms`.
    fn replace_all(&mut self, cx: &mut Context<Self>) {
        if self.doc.find.matches.is_empty() {
            return;
        }
        let start = Instant::now();
        let count = self.doc.find.matches.len();
        let mut new_text = String::with_capacity(self.doc.buffer.doc_len());
        let mut last = 0usize;
        for range in &self.doc.find.matches {
            new_text.push_str(&self.doc.buffer.slice(last..range.start));
            new_text.push_str(&self.doc.find.replacement);
            last = range.end;
        }
        new_text.push_str(&self.doc.buffer.slice(last..self.doc.buffer.doc_len()));
        let doc_len = self.doc.buffer.doc_len();
        self.apply_edit(0..doc_len, &new_text);
        self.doc.find.replaced_total += count;
        self.metrics
            .borrow_mut()
            .replace_all_ms
            .push(duration_ms(start.elapsed()));
        cx.notify();
    }

    // ---- diff view (slice 3, M1-3b) --------------------------------------

    /// Recompute the unified diff rows: `diff.base_lines` (the file as
    /// loaded) vs. the current buffer. The compute time is recorded into
    /// `diff_compute_ms`; the caller marks the render sample with
    /// `mark_diff` so "requested → rendered" is captured separately. The
    /// correctness contract this surface owes the M4 proposal view is not
    /// "the diff is empty after an edit" (an edit must show), but that the
    /// diff **reconstructs the edited document from the base** — pinned by
    /// `diff_reconstructs_edited_document`.
    fn recompute_diff(&mut self) {
        let start = Instant::now();
        let base = self.doc.diff.base_lines.join("\n");
        let current = self.doc.buffer.lines().join("\n");
        let d = similar::TextDiff::from_lines(&base, &current);
        let mut rows = Vec::new();
        let mut added = 0usize;
        let mut removed = 0usize;
        for change in d.iter_all_changes() {
            let label = match change.tag() {
                similar::ChangeTag::Delete => {
                    removed += 1;
                    '-'
                }
                similar::ChangeTag::Insert => {
                    added += 1;
                    '+'
                }
                similar::ChangeTag::Equal => ' ',
            };
            rows.push(DiffRow {
                label,
                text: change.value().trim_end_matches('\n').to_string(),
            });
        }
        self.doc.diff.rows = rows;
        self.doc.diff.added_total += added;
        self.doc.diff.removed_total += removed;
        // Park the cursor on the first hunk so opening the diff shows the
        // change, not the unchanged head of the file.
        self.doc.diff.cursor = self
            .doc
            .diff
            .rows
            .iter()
            .position(|row| row.label != ' ')
            .unwrap_or(0);
        self.metrics
            .borrow_mut()
            .diff_compute_ms
            .push(duration_ms(start.elapsed()));
    }

    /// Open the diff view (cmd-d). The base is seeded from the loaded file
    /// the first time; the cursor and scroll reset to the first hunk.
    fn open_diff(&mut self, cx: &mut Context<Self>) {
        if self.doc.preview {
            self.doc.preview = false;
        }
        if !self.doc.diff.open {
            self.doc.diff.open = true;
        }
        self.recompute_diff();
        self.doc.diff_list_state = ListState::new(
            self.doc.diff.rows.len(),
            ListAlignment::Top,
            px(theme::line_height() * 2.0),
        );
        let cursor = self.doc.diff.cursor;
        self.doc.diff_list_state.scroll_to_reveal_item(cursor);
        self.metrics.borrow_mut().mark_diff(Instant::now());
        cx.notify();
    }

    fn close_diff(&mut self, cx: &mut Context<Self>) {
        self.doc.diff.open = false;
        cx.notify();
    }

    /// Move the diff cursor to the next/previous **hunk** (cmd-g/shift-g).
    /// A modified line renders as an adjacent delete+insert pair, so a plain
    /// row step would park between them; instead, leave the current block of
    /// consecutive change rows and land on the next one, wrapping.
    fn diff_move_cursor(&mut self, delta: isize, cx: &mut Context<Self>) {
        let len = self.doc.diff.rows.len();
        if len == 0 {
            return;
        }
        let step = if delta >= 0 { 1isize } else { -1 };
        let is_change = |ix: usize| self.doc.diff.rows[ix].label != ' ';
        let mut ix = self.doc.diff.cursor;
        // Leave the current change block (the +/− pair of one hunk)…
        let mut guard = 0usize;
        while is_change(ix) {
            ix = (ix as isize + step).rem_euclid(len as isize) as usize;
            guard += 1;
            if guard >= len {
                return; // the whole view is one change block: nowhere to go
            }
        }
        // …then walk through context rows to the next change block.
        guard = 0;
        while !is_change(ix) {
            ix = (ix as isize + step).rem_euclid(len as isize) as usize;
            guard += 1;
            if guard >= len {
                return; // no changes at all (empty diff): stay put
            }
        }
        self.doc.diff.cursor = ix;
        self.doc
            .diff_list_state
            .scroll_to_reveal_item(self.doc.diff.cursor);
        cx.notify();
    }

    /// The diff view row: label gutter + line text, colored by tag.
    fn diff_row(&self, ix: usize) -> gpui::Div {
        let Some(row) = self.doc.diff.rows.get(ix) else {
            return div();
        };
        let (label_color, bg) = match row.label {
            '+' => (rgb(theme::diff_add_label()), Some(theme::diff_add_bg())),
            '-' => (
                rgb(theme::diff_remove_label()),
                Some(theme::diff_remove_bg()),
            ),
            _ => (rgb(theme::diff_context_label()), None),
        };
        let mut element = div()
            .px_2()
            .h(px(theme::line_height()))
            .flex()
            .items_center()
            .whitespace_nowrap()
            .font_family(FONT_FAMILY)
            .text_size(px(theme::text_size()))
            // Match the editor rows: the default GPUI line box is taller than
            // the 18 px row and `items_center` would push the diff text up.
            .line_height(px(theme::line_height()));
        if ix == self.doc.diff.cursor {
            element = element.bg(rgba(theme::selection_bg()));
        } else if let Some(color) = bg {
            element = element.bg(rgb(color));
        }
        element
            .child(
                div()
                    .w(px(16.0))
                    .text_color(label_color)
                    .child(row.label.to_string()),
            )
            .child(
                div()
                    .text_color(rgb(theme::status_fg()))
                    .child(row.text.clone()),
            )
    }

    // ---- find bar actions ------------------------------------------------

    fn on_toggle_find(&mut self, _: &ToggleFind, _window: &mut Window, cx: &mut Context<Self>) {
        if self.doc.find.open && self.doc.find.field == FindField::Query {
            self.close_find(cx);
        } else {
            self.open_find(false, cx);
        }
    }

    fn on_focus_replace(&mut self, _: &FocusReplace, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_find(true, cx);
    }

    fn on_close_find(&mut self, _: &CloseFind, _window: &mut Window, cx: &mut Context<Self>) {
        // Escape closes whichever bar owns the keystrokes (single bar model:
        // find, the instruction bar and the focus bar are never open at the
        // same time). The unsaved-changes prompt wins over all of them.
        if self.unsaved_prompt.is_some() {
            self.unsaved_prompt = None;
            cx.notify();
            return;
        }
        if self.context_menu.is_some() {
            self.context_menu = None;
            cx.notify();
            return;
        }
        if self.rename.is_some() {
            self.rename = None;
            cx.notify();
            return;
        }
        if self.delete_prompt.is_some() {
            self.delete_prompt = None;
            cx.notify();
            return;
        }
        if self.git_commit.is_some() {
            if !self.git_busy {
                self.git_commit = None;
            }
            cx.notify();
            return;
        }
        if self.git_branch_prompt.is_some() {
            self.git_branch_prompt = None;
            cx.notify();
            return;
        }
        if self.github_publish.is_some() {
            if !self.git_busy {
                self.github_publish = None;
            }
            cx.notify();
            return;
        }
        if self.git_sync.is_some() {
            if !self.git_busy {
                self.git_sync = None;
            }
            cx.notify();
            return;
        }
        if self.github_pr.is_some() {
            if !self.git_busy {
                self.github_pr = None;
            }
            cx.notify();
            return;
        }
        if self.git_restore.is_some() {
            self.git_restore = None;
            cx.notify();
            return;
        }
        if self.git_diff.is_some() {
            self.git_diff = None;
            cx.notify();
            return;
        }
        if self.chat.focused {
            self.chat.focused = false;
            cx.notify();
            return;
        }
        if self.palette.is_some() {
            self.palette = None;
            cx.notify();
            return;
        }
        if self.settings.is_some() {
            self.settings = None;
            cx.notify();
            return;
        }
        if self.project_settings.is_some() {
            self.project_settings = None;
            cx.notify();
            return;
        }
        if self.new_file.is_some() {
            self.new_file = None;
            cx.notify();
            return;
        }
        if self.new_project.is_some() {
            if !self.new_project.as_ref().is_some_and(|state| state.picking) {
                self.new_project = None;
                self.new_project_generation = self.new_project_generation.saturating_add(1);
            }
            cx.notify();
            return;
        }
        if self.doc.find.open {
            self.close_find(cx);
        } else if self.search.open {
            // Escape clears the project-search results and closes the bar.
            self.search.open = false;
            self.search.query.clear();
            self.search.hits.clear();
            self.search.searched.clear();
            cx.notify();
        } else if self.propose_bar.open {
            self.propose_bar.open = false;
            self.propose_bar.instruction.clear();
            cx.notify();
        } else if self.focus_bar.open {
            // Closing the focus bar discards the typed (uncommitted) focus;
            // the last committed one in `agent.focus` stays in effect.
            self.focus_bar.open = false;
            self.focus_bar.focus.clear();
            cx.notify();
        }
    }

    fn on_replace_current(
        &mut self,
        _: &ReplaceCurrent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.doc.find.open {
            self.replace_current(cx);
        }
    }

    fn on_replace_all(&mut self, _: &ReplaceAll, _window: &mut Window, cx: &mut Context<Self>) {
        if self.doc.find.open {
            self.replace_all(cx);
        }
    }

    // ---- diff view actions (slice 3, M1-3b) ------------------------------

    fn on_toggle_diff(&mut self, _: &ToggleDiff, _window: &mut Window, cx: &mut Context<Self>) {
        if self.doc.diff.open {
            self.close_diff(cx);
        } else {
            self.open_diff(cx);
        }
    }

    // ---- Markdown preview (M14a, D-033) ----------------------------------

    fn active_is_markdown(&self) -> bool {
        FileKind::from_path(&self.doc.path) == FileKind::Markdown
    }

    /// Toggle the read-only preview for the active tab. The source buffer is
    /// never touched; the derived blocks are rebuilt from the current text
    /// each time the preview opens.
    fn set_preview(&mut self, on: bool, cx: &mut Context<Self>) {
        if on && !self.active_is_markdown() {
            return;
        }
        if on {
            let text = self.doc.buffer.slice(0..self.doc.buffer.doc_len());
            self.doc.preview_document = MarkdownDocument::parse(&text);
            self.doc.diff.open = false;
            self.doc.find.open = false;
            self.doc.find.matches.clear();
            self.doc.find.current = 0;
        }
        self.doc.preview = on;
        cx.notify();
    }

    fn on_toggle_preview(
        &mut self,
        _: &TogglePreview,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_preview(!self.doc.preview, cx);
    }

    /// Slim per-document toolbar for Markdown. The toggle is per tab because
    /// `preview` lives in `DocumentState`; switching tabs keeps each document
    /// in the mode the writer left it in.
    fn document_toolbar(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let preview = self.doc.preview;
        let tip = self.git_tooltip(cx, "⌘⌥P", self.t("preview.tip").to_string());
        let code_tip = tip.clone();
        let code_active = !preview;
        let preview_active = preview;
        div()
            .id("document-toolbar")
            .h(px(28.0))
            .w_full()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .bg(rgb(theme::explorer_bg()))
            .border_b_1()
            .border_color(rgb(theme::explorer_border()))
            .text_xs()
            .child(
                div()
                    .text_color(rgb(theme::gutter_fg()))
                    .child(language_name(&self.doc.path)),
            )
            .when(preview, |row| {
                row.child(
                    div()
                        .text_color(rgb(theme::gutter_fg()))
                        .child(self.t("preview.read_only")),
                )
            })
            .child(div().flex_1())
            .child(
                div()
                    .id("preview-code")
                    .px_2()
                    .py_1()
                    .cursor_pointer()
                    .when(code_active, |button| {
                        button
                            .bg(rgb(theme::status_bg()))
                            .text_color(rgb(theme::default_fg()))
                    })
                    .when(!code_active, |button| {
                        button.text_color(rgb(theme::status_fg()))
                    })
                    .tooltip(move |_, _| code_tip.clone())
                    .child(self.t("preview.code"))
                    .on_click(cx.listener(|this, _, _, cx| this.set_preview(false, cx))),
            )
            .child(
                div()
                    .id("preview-mode")
                    .px_2()
                    .py_1()
                    .cursor_pointer()
                    .when(preview_active, |button| {
                        button
                            .bg(rgb(theme::status_bg()))
                            .text_color(rgb(theme::default_fg()))
                    })
                    .when(!preview_active, |button| {
                        button.text_color(rgb(theme::status_fg()))
                    })
                    .tooltip(move |_, _| tip.clone())
                    .child(self.t("preview.preview"))
                    .on_click(cx.listener(|this, _, _, cx| this.set_preview(true, cx))),
            )
            .into_any_element()
    }

    // ---- find probe (slice 3 measurement) ---------------------------------

    /// Each probe step submits a real query from a fixed list against the
    /// whole document and reveals the next match — the exact functions the
    /// interactive bar calls, so the measured path is the shipped path.
    fn advance_find_probe(&mut self, cx: &mut Context<Self>) {
        const QUERIES: [&str; 5] = ["noche", "INT.", "toda la noche", "habitación", "escena"];
        self.doc.find.open = true;
        self.doc.find.field = FindField::Query;
        let query = QUERIES[self.find_probe_cursor % QUERIES.len()];
        self.doc.find.query = query.to_string();
        self.refresh_matches(true);
        if !self.doc.find.matches.is_empty() {
            self.doc.find.current = self.find_probe_cursor % self.doc.find.matches.len();
            self.goto_match(self.doc.find.current);
        }
        self.find_probe_cursor += 1;
        self.find_probe_left = self.find_probe_left.saturating_sub(1);

        let finished = self.find_probe_left == 0 && self.probe_left == 0;
        cx.notify();
        if finished {
            let report = self.metrics.borrow().report();
            println!("{report}");
            cx.defer(|cx| cx.quit());
        }
    }

    // ---- project workspace (M6 slice 1) ----------------------------------

    /// Full buffer text. Round-trips what was loaded: `Buffer::new` splits
    /// on `\n` and `slice(0..doc_len)` joins the same lines back.
    fn full_text(&self) -> String {
        self.doc.buffer.slice(0..self.doc.buffer.doc_len())
    }

    /// True when this document diverged from its last load/save.
    /// Generation-based (O(1)): every mutation path bumps the document's
    /// revision; saving snapshots it. Per-document, so tabs stay independent.
    fn dirty(&self) -> bool {
        self.doc.is_dirty()
    }

    /// Replace the buffer with a freshly loaded document, resetting all
    /// per-document state (selection, undo, find, diff, scene cache). Scroll
    /// starts at the top: this is an explicit document switch, not a jump the
    /// user must recover from. `base_text` becomes the new diff/save base.
    fn load_document(&mut self, path: String, text: String) {
        self.doc.buffer = Buffer::new(&text);
        self.doc.base_text = text.clone();
        self.doc.path = path;
        self.doc.selected = 0..0;
        self.doc.reversed = false;
        self.doc.marked = None;
        self.doc.composing = false;
        self.doc.dragging = None;
        self.doc.last_click = None;
        self.doc.undo_stack.clear();
        self.doc.redo_stack.clear();
        self.doc.scene_lines.clear();
        self.doc.scenes_dirty = true;
        self.doc.elements_dirty = true;
        self.doc.elements_parsed_at = None;
        self.doc.pages_dirty = true;
        self.doc.pages_generation += 1;
        self.doc.current_scene = 0;
        // M9 slice 6: surface a newer recovery backup for this document.
        self.check_backup_notice();
        self.doc.list_state = ListState::new(
            self.doc.buffer.line_count(),
            ListAlignment::Top,
            px(theme::line_height() * 2.0),
        );
        self.doc.find = FindState::default();
        self.doc.diff = DiffState {
            base_lines: text.split('\n').map(str::to_string).collect(),
            ..DiffState::default()
        };
        self.doc.diff_list_state =
            ListState::new(0, ListAlignment::Top, px(theme::line_height() * 2.0));
        // A freshly loaded document has no unsaved edits of its own.
        self.doc.revision = 0;
        self.doc.saved_revision = 0;
        self.notice = None;
    }

    /// Open a document as a tab, or activate its existing tab. A single
    /// explorer click opens a preview tab (`pin = false`); Quick Open, search
    /// hits and tooling pass `pin = true`. Unsaved state is preserved in the
    /// tab being left; tabs make switching safe, so the old document-switch
    /// refusal no longer applies here.
    fn open_document_path(&mut self, path: &str, pin: bool) -> Result<(), String> {
        if self.unsaved_prompt.is_some()
            || self.settings.is_some()
            || self.project_settings.is_some()
            || self.palette.is_some()
            || self.rename.is_some()
            || self.delete_prompt.is_some()
        {
            return Err(self.t("notice.finish_dialog").to_string());
        }
        if path == self.doc.path {
            if pin {
                self.tabs[self.active_tab].preview = false;
            }
            return Ok(());
        }
        if let Some(index) = self.find_tab(path) {
            self.activate_tab(index);
            if pin {
                self.tabs[index].preview = false;
            }
            return Ok(());
        }
        let Some(project) = self.project.clone() else {
            return Err(self.t("notice.fixture_no_project").to_string());
        };
        let text = project.read_document(path).map_err(|error| {
            self.tf(
                "notice.open_failed",
                &[("path", path), ("error", &error.to_string())],
            )
        })?;
        let doc = DocumentState::new(path.to_string(), &text);
        if pin {
            self.open_document_tab(doc);
        } else {
            self.open_preview_document(doc);
        }
        self.check_backup_notice();
        Ok(())
    }

    /// Explorer click: open the selected document in a tab (or activate its
    /// tab). A single click previews; a double click pins (D-041). A document
    /// with unsaved changes is kept in its tab, not dropped.
    fn select_path(&mut self, path: &str, pin: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.unsaved_prompt.is_some() {
            return;
        }
        self.chat.focused = false;
        match self.open_document_path(path, pin) {
            Ok(()) => {
                // A click re-targets typing (and IME composition) at the
                // buffer.
                window.focus(&self.focus_handle(cx));
            }
            Err(message) => self.notice = Some(message),
        }
        cx.notify();
    }

    /// Collapse/expand a folder in the explorer tree (M9 slice 4).
    fn toggle_folder(&mut self, path: &str, cx: &mut Context<Self>) {
        if self.unsaved_prompt.is_some() {
            return;
        }
        if !self.collapsed.remove(path) {
            self.collapsed.insert(path.to_string());
        }
        cx.notify();
    }

    /// Save one tab: the active tab goes through the normal save path (diff
    /// refresh and notice included); an inactive tab is written from its own
    /// buffer without activating it. A failed save leaves the tab dirty.
    fn save_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if index == self.active_tab {
            self.on_save(&Save, window, cx);
            return !self.doc.is_dirty();
        }
        let Some(doc) = self.tabs.get(index).and_then(|tab| tab.document.as_ref()) else {
            return false;
        };
        if !doc.is_dirty() {
            return true;
        }
        if doc.path.is_empty() {
            self.notice = Some(self.t("notice.no_document").to_string());
            return false;
        }
        let path = doc.path.clone();
        let text = doc.buffer.slice(0..doc.buffer.doc_len());
        let base = doc.base_text.clone();
        let revision = doc.revision;
        let Some(project) = self.project.clone() else {
            self.notice = Some(self.t("notice.fixture_no_project").to_string());
            return false;
        };
        if text == base {
            if let Some(doc) = self
                .tabs
                .get_mut(index)
                .and_then(|tab| tab.document.as_mut())
            {
                doc.saved_revision = revision;
            }
            return true;
        }
        match project.write_document(&path, &text) {
            Ok(()) => {
                if let Some(doc) = self
                    .tabs
                    .get_mut(index)
                    .and_then(|tab| tab.document.as_mut())
                {
                    doc.base_text = text.clone();
                    doc.saved_revision = revision;
                    doc.diff.base_lines = text.split('\n').map(str::to_string).collect();
                    doc.backup_path = None;
                    doc.backup_saved_at = None;
                    doc.backup_error = None;
                }
                let _ = narra_core::backup::remove_backup(project.root(), &path);
                self.notice = Some(self.tf("notice.saved", &[("path", &path)]));
                self.git_refreshed_at = None;
                true
            }
            Err(error) => {
                self.notice = Some(self.tf(
                    "notice.save_failed",
                    &[("path", &path), ("error", &error.to_string())],
                ));
                false
            }
        }
    }

    /// Save every dirty tab in order; stop at the first failure.
    fn save_all_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        for index in 0..self.tabs.len() {
            if self.tab_is_dirty(index) && !self.save_tab(index, window, cx) {
                return false;
            }
        }
        self.dirty_tab_count() == 0
    }

    /// ⌘S: write the buffer through the boundary-checked project IO. The
    /// canonical file changes only here, on an explicit save — never by an
    /// agent action and never as a side effect of switching documents.
    fn on_save(&mut self, _: &Save, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.project.clone() else {
            self.notice = Some(self.t("notice.fixture_no_project").to_string());
            cx.notify();
            return;
        };
        if self.doc.path.is_empty() {
            self.notice = Some(self.t("notice.no_document").to_string());
            cx.notify();
            return;
        }
        let text = self.full_text();
        if text == self.doc.base_text {
            // Converged back (e.g. undo to the original): clear the dirty
            // flag without touching the file.
            self.doc.saved_revision = self.doc.revision;
            self.notice = Some(self.t("notice.no_changes").to_string());
            cx.notify();
            return;
        }
        match project.write_document(&self.doc.path, &text) {
            Ok(()) => {
                self.doc.base_text = text.clone();
                self.doc.saved_revision = self.doc.revision;
                // The saved file becomes the diff base (same model as the M4
                // proposal surface: canonical file vs. edited buffer).
                self.doc.diff.base_lines = text.split('\n').map(str::to_string).collect();
                if self.doc.diff.open {
                    // Recompute and resize the rows list in one step.
                    self.open_diff(cx);
                } else {
                    // Keep the dump honest; opening the diff rebuilds rows.
                    self.recompute_diff();
                }
                // The canonical file now holds the buffer: the recovery
                // backup is no longer needed (best effort).
                let _ = narra_core::backup::remove_backup(project.root(), &self.doc.path);
                self.doc.backup_path = None;
                self.doc.backup_saved_at = None;
                self.doc.backup_error = None;
                self.notice = Some(self.tf("notice.saved", &[("path", &self.doc.path)]));
                self.git_refreshed_at = None;
                if self.doc.path.starts_with("bible/") {
                    self.refresh_entities();
                }
            }
            Err(error) => {
                self.notice = Some(self.tf(
                    "notice.save_failed",
                    &[("path", &self.doc.path), ("error", &error.to_string())],
                ));
            }
        }
        cx.notify();
    }

    /// Localized string (M6 slice 9): every workspace-visible label and
    /// notice goes through this (agent output and core errors excepted —
    /// they are producer text, not UI chrome).
    fn t(&self, key: &str) -> &'static str {
        i18n::tr(self.lang, key)
    }

    /// Localized template with `{name}` substitutions.
    fn tf(&self, key: &str, params: &[(&str, &str)]) -> String {
        i18n::tf(self.lang, key, params)
    }

    /// Localized word for a terminal proposal state in notices (the dump
    /// keeps machine-oriented English `status_key` values).
    fn propose_status_word(&self, status: ProposeStatus) -> &'static str {
        match status {
            ProposeStatus::Applied => self.t("status.applied"),
            _ => self.t("status.rejected"),
        }
    }

    /// Whether the option arrows and alt-enter belong to the proposal panel
    /// (M9 slice 2): only while the proposal is ready to review. A rejected,
    /// applied or failed proposal leaves the keys to the buffer.
    fn proposal_hunks_active(&self) -> bool {
        self.propose.is_open()
            && self.propose.status == ProposeStatus::Ready
            && self.propose.hunks > 0
    }

    /// Localized proposal status label for the panel title.
    fn propose_status_label(&self) -> &'static str {
        match self.propose.status {
            ProposeStatus::Idle => self.t("status.idle"),
            ProposeStatus::Running => self.t("status.running"),
            ProposeStatus::Ready => self.t("status.ready"),
            ProposeStatus::Applied => self.t("status.applied"),
            ProposeStatus::Rejected => self.t("status.rejected"),
            ProposeStatus::Failed => self.t("status.failed"),
            ProposeStatus::Cancelled => self.t("status.cancelled"),
        }
    }

    /// Common gates for agent tasks (M6 slice 3/4/8): refuse while another
    /// task runs, and resolve the project/screenplay/clean-buffer checks.
    /// The caller adds its own last check and arms the shared cancellation
    /// token, so a refused run never leaves the token registered.
    fn agent_task_base_gates(&mut self) -> Option<(String, AgentSettings, CancellationToken)> {
        if self.agent_cancel.is_some() {
            self.notice = Some(self.t("notice.agent_running").to_string());
            return None;
        }
        // Own the two strings we need so the borrow of `self.project` ends
        // before any mutation below (borrow-checker friendly shape).
        let project_info = self.project.as_ref().map(|project| {
            (
                project.root().to_string_lossy().into_owned(),
                project.manifest().default_screenplay_path.clone(),
            )
        });
        let Some((root, screenplay)) = project_info else {
            self.notice = Some(self.t("notice.fixture_no_project_agent").to_string());
            return None;
        };
        if self.doc.path != screenplay {
            self.notice = Some(self.tf("notice.open_screenplay", &[("path", &screenplay)]));
            return None;
        }
        if self.dirty() {
            self.notice = Some(self.t("notice.dirty_agent").to_string());
            return None;
        }
        let settings = self.agent_settings.clone();
        let cancel = CancellationToken::new();
        Some((root, settings, cancel))
    }

    /// Fresh-run preamble (M6 slice 3/4): the shared gates plus the
    /// caret-in-scene requirement; arms the cancellation token last.
    fn agent_task_gates(&mut self) -> Option<(String, usize, AgentSettings, CancellationToken)> {
        let (root, settings, cancel) = self.agent_task_base_gates()?;
        self.refresh_scenes();
        let caret_line = self.doc.buffer.position_at(self.caret()).line;
        let Some(scene_number) = scene_number_at(&self.doc.scene_lines, caret_line) else {
            self.notice = Some(self.t("notice.caret_outside_scene").to_string());
            return None;
        };
        self.agent_cancel = Some(cancel.clone());
        Some((root, scene_number, settings, cancel))
    }

    /// Pump task messages into the entity on a short timer: progress stays
    /// observable, the UI thread never blocks, and `apply` reports whether
    /// the final message arrived (or the entity/window went away).
    fn spawn_message_pump<A: 'static>(
        cx: &mut Context<Self>,
        rx: mpsc::Receiver<A>,
        mut apply: impl FnMut(&mut Self, A) -> bool + 'static,
    ) {
        cx.spawn(async move |view, cx| {
            loop {
                loop {
                    match rx.try_recv() {
                        Ok(message) => {
                            // Entity dropped (window closed) → stop pumping.
                            let done = view
                                .update(cx, |view, cx| {
                                    let done = apply(view, message);
                                    cx.notify();
                                    done
                                })
                                .unwrap_or(true);
                            if done {
                                return;
                            }
                        }
                        Err(mpsc::TryRecvError::Empty) => break,
                        // The worker died without a final message: stop the
                        // pump instead of spinning forever.
                        Err(mpsc::TryRecvError::Disconnected) => return,
                    }
                }
                Timer::after(Duration::from_millis(33)).await;
            }
        })
        .detach();
    }

    /// ⌘R and the focus bar's enter both land here: run the review for the
    /// caret's scene with the committed focus (M6 slice 3/5). Read-only: the
    /// task never writes, and it reviews the *saved* file — an unsaved
    /// buffer is refused with an explicit notice instead of being reviewed
    /// at the wrong version.
    fn on_review_scene(&mut self, _: &ReviewScene, _window: &mut Window, cx: &mut Context<Self>) {
        self.run_review(cx);
    }

    /// Run the review task for the caret's scene on a worker thread (M6
    /// slice 3); the committed focus (slice 5) rides along as input.
    fn run_review(&mut self, cx: &mut Context<Self>) {
        let Some((root, scene_number, settings, cancel)) = self.agent_task_gates() else {
            return;
        };
        // The committed focus (M6 slice 5): ⌘R reuses the last one; the
        // focus bar (⌘⇧R) commits a new one before running.
        let focus = self.review_focus.clone();
        self.launch_review(root, scene_number, focus, settings, cancel, cx);
    }

    /// ⌘⌥R (M6 slice 8): re-run the failed/cancelled review with its own
    /// stored inputs — same scene, same focus, never retyped. The shared
    /// gates still apply (dirty buffer or a running task refuses first).
    fn retry_review(&mut self, cx: &mut Context<Self>) {
        let scene_number = self.agent.scene_number;
        let focus = self.agent.focus.clone();
        let Some((root, settings, cancel)) = self.agent_task_base_gates() else {
            return;
        };
        self.agent_cancel = Some(cancel.clone());
        self.launch_review(root, scene_number, focus, settings, cancel, cx);
    }

    /// Start a review run with explicit inputs: the caret scene + committed
    /// focus for a fresh ⌘R, the stored ones for a ⌘⌥R retry. Resolves the
    /// configured role, begins the panel state and spawns the worker.
    fn launch_review(
        &mut self,
        root: String,
        scene_number: usize,
        focus: String,
        settings: AgentSettings,
        cancel: CancellationToken,
        cx: &mut Context<Self>,
    ) {
        let focus_note = if focus.is_empty() {
            String::new()
        } else {
            self.tf("notice.focus_suffix", &[("focus", &focus)])
        };
        // The configured role (M6 slice 7): reviews run under the
        // editor-reviewer role as configured in `agents.yaml`. A broken
        // role config refuses the run explicitly — never a silent
        // fallback to the built-in persona.
        let role = match roles::resolve_default_with_packages(
            Path::new(&root),
            Some(&recent::config_dir()),
            SceneReviewTask::KIND,
        ) {
            Ok(role) => role,
            Err(error) => {
                self.notice =
                    Some(self.tf("notice.roles_invalid", &[("error", &error.to_string())]));
                cx.notify();
                return;
            }
        };
        self.agent_panel_open = true;
        self.right_tab = RightTab::Activity;
        self.agent_generation += 1;
        let generation = self.agent_generation;
        self.agent.begin(scene_number, focus.clone());
        self.agent.role = role.id.clone();
        self.agent.role_source = role.origin_key().to_string();
        let mut notice = self.tf(
            "notice.review_running",
            &[
                ("scene", &scene_number.to_string()),
                ("role", &self.agent.role),
            ],
        );
        notice.push_str(&focus_note);
        self.notice = Some(notice);
        cx.notify();

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let options = TaskOptions {
                timeout: AGENT_TIMEOUT,
                cancel,
                stream: false,
            };
            let result = match settings.build_adapter() {
                Ok(adapter) => SceneReviewTask::new(root, scene_number, focus)
                    .with_role(role)
                    .run(&*adapter, &options, |event| {
                        let _ = tx.send(AgentMessage::Event(event));
                    }),
                Err(error) => Err(AgentError::Adapter(error)),
            };
            let _ = tx.send(AgentMessage::Done(result));
        });
        Self::spawn_message_pump(cx, rx, move |view, message| {
            if generation != view.agent_generation {
                return true;
            }
            let done = matches!(message, AgentMessage::Done(_));
            view.apply_agent_message(message, done);
            done
        });
    }

    /// Fold one agent message into the panel state; when the task ends, keep
    /// the notice bar in sync with the outcome.
    fn apply_agent_message(&mut self, message: AgentMessage, done: bool) {
        self.agent.apply(message);
        if done {
            self.agent_cancel = None;
            self.notice = Some(match self.agent.status {
                AgentStatus::Completed => self.tf(
                    "notice.review_done",
                    &[("scene", &self.agent.scene_number.to_string())],
                ),
                AgentStatus::Cancelled => self.t("notice.review_cancelled").to_string(),
                AgentStatus::Failed => {
                    self.tf("notice.review_failed", &[("error", &self.agent.error)])
                }
                AgentStatus::Running | AgentStatus::Idle => String::new(),
            });
        }
    }

    /// ⌘K submit: run `scene_edit` for the caret's scene with the typed
    /// instruction on a worker thread (same pump as the review). On success
    /// the worker also loads the persisted record and pre-renders its hunks,
    /// so the panel shows exactly what apply would write.
    fn run_propose_task(&mut self, instruction: String, cx: &mut Context<Self>) {
        let Some((root, scene_number, settings, cancel)) = self.agent_task_gates() else {
            return;
        };
        self.launch_propose(root, scene_number, instruction, settings, cancel, cx);
    }

    /// ⌘⌥R (M6 slice 8): re-run the failed/cancelled proposal with its own
    /// stored inputs — same scene, same instruction, never retyped. The
    /// shared gates still apply (dirty buffer or running task refuses).
    fn retry_propose(&mut self, cx: &mut Context<Self>) {
        let scene_number = self.propose.scene_number;
        let instruction = self.propose.instruction.clone();
        let Some((root, settings, cancel)) = self.agent_task_base_gates() else {
            return;
        };
        self.agent_cancel = Some(cancel.clone());
        self.launch_propose(root, scene_number, instruction, settings, cancel, cx);
    }

    /// Start a `scene_edit` run with explicit inputs (fresh ⌘K or ⌘⌥R
    /// retry): resolve the configured role, begin the proposal state and
    /// spawn the worker; on success the worker loads the persisted record.
    fn launch_propose(
        &mut self,
        root: String,
        scene_number: usize,
        instruction: String,
        settings: AgentSettings,
        cancel: CancellationToken,
        cx: &mut Context<Self>,
    ) {
        // The configured role (M6 slice 7): edits run under the writer
        // role as configured in `agents.yaml`; a broken config refuses the
        // run explicitly instead of silently changing the persona.
        let role = match roles::resolve_default_with_packages(
            Path::new(&root),
            Some(&recent::config_dir()),
            SceneEditTask::KIND,
        ) {
            Ok(role) => role,
            Err(error) => {
                self.notice =
                    Some(self.tf("notice.roles_invalid", &[("error", &error.to_string())]));
                cx.notify();
                return;
            }
        };
        self.agent_panel_open = true;
        self.right_tab = RightTab::Activity;
        self.agent_generation += 1;
        let generation = self.agent_generation;
        self.propose.begin(scene_number, instruction.clone());
        self.propose.role = role.id.clone();
        self.propose.role_source = role.origin_key().to_string();
        self.notice = Some(self.tf(
            "notice.propose_running",
            &[
                ("scene", &scene_number.to_string()),
                ("role", &self.propose.role),
            ],
        ));
        cx.notify();

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let options = TaskOptions {
                timeout: AGENT_TIMEOUT,
                cancel,
                stream: false,
            };
            let result = match settings.build_adapter() {
                Ok(adapter) => {
                    let run = SceneEditTask::new(root.clone(), scene_number, instruction)
                        .with_role(role)
                        .run(&*adapter, &options, |event| {
                            let _ = tx.send(ProposeMessage::Event(event));
                        });
                    match run {
                        Ok(output) => {
                            // Read back the persisted record and pre-render
                            // its hunks: the panel shows the stored proposal,
                            // not a recomputation.
                            let diff_rows = output.data["proposal_id"]
                                .as_str()
                                .and_then(|id| proposal::load(Path::new(&root), id).ok())
                                .map(|record| diff_rows_from_hunks(&record.hunks))
                                .unwrap_or_default();
                            Ok(ProposeResult { output, diff_rows })
                        }
                        Err(error) => Err(error),
                    }
                }
                Err(error) => Err(AgentError::Adapter(error)),
            };
            let _ = tx.send(ProposeMessage::Done(result));
        });
        Self::spawn_message_pump(cx, rx, move |view, message| {
            if generation != view.agent_generation {
                return true;
            }
            let done = matches!(message, ProposeMessage::Done(_));
            view.apply_propose_message(message, done);
            done
        });
    }

    /// ⌘⌥R (M6 slice 8): retry the visible flow's last run with its stored
    /// inputs. Only a failed or cancelled run is retryable, and only the
    /// flow whose panel is visible (the proposal panel wins when open —
    /// the same rule as the render); nothing to retry says so explicitly.
    fn on_retry_agent(&mut self, _: &RetryAgent, _window: &mut Window, cx: &mut Context<Self>) {
        match retry_target(
            self.propose.is_open(),
            self.propose.status,
            self.agent.status,
        ) {
            Some(RetryTarget::Propose) => self.retry_propose(cx),
            Some(RetryTarget::Review) => self.retry_review(cx),
            None => {
                self.notice = Some(self.t("notice.no_retry").to_string());
                cx.notify();
            }
        }
    }

    /// ⌘⌥L (M6 slice 9): toggle EN/ES live. New events and rendered labels
    /// follow immediately; stage/error text already on screen keeps the
    /// language it was generated in until the next event.
    fn on_toggle_lang(&mut self, _: &ToggleLang, _window: &mut Window, cx: &mut Context<Self>) {
        self.lang = match self.lang {
            Lang::En => Lang::Es,
            Lang::Es => Lang::En,
        };
        // The reducers speak the new language for everything they generate.
        self.agent.lang = self.lang;
        self.propose.lang = self.lang;
        self.notice = Some(self.tf("notice.lang", &[("lang", self.lang.name())]));
        // The native menus are localized too: rebuild them in the new
        // language (M10 slice 1), keeping the current recents (M10 slice 2b).
        self.refresh_menus(cx);
        cx.notify();
    }

    /// ⌘E (M7 slice 4): export the current buffer as PDF into
    /// `<root>/exports/<screenplay-stem>.pdf` on a worker thread — the UI
    /// thread never blocks on layout, serialization or IO. The buffer (what
    /// the user sees) is exported, never a silent disk re-read; the PDF is
    /// an artifact and no canonical file changes.
    fn on_export_pdf(&mut self, _: &ExportPdf, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.project.as_ref() else {
            self.notice = Some(self.t("notice.fixture_no_project").to_string());
            cx.notify();
            return;
        };
        // Tabs make it possible to have notes active while the screenplay is
        // open elsewhere; never export the wrong buffer under the guion name.
        if self.doc.path != project.manifest().default_screenplay_path {
            self.notice = Some(self.t("notice.export_screenplay").to_string());
            cx.notify();
            return;
        }
        let root = project.root().to_string_lossy().into_owned();
        let manifest = project.manifest().clone();
        let stem = Path::new(&manifest.default_screenplay_path)
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "screenplay".to_string());
        let text = self.full_text();
        let generation = self.workspace_generation;

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let body = export::pdf::layout(&text).pages.len();
            let bytes = export::pdf::export(&manifest, &text);
            let dir = Path::new(&root).join(narra_core::project::EXPORTS_DIR);
            let path = dir.join(format!("{stem}.pdf"));
            let result = (|| {
                std::fs::create_dir_all(&dir)
                    .map_err(|error| format!("{}: {error}", dir.display()))?;
                std::fs::write(&path, &bytes)
                    .map_err(|error| format!("{}: {error}", path.display()))?;
                Ok(ExportOutcome {
                    out: path.display().to_string(),
                    bytes: bytes.len(),
                    pages: body + 1,
                })
            })();
            let _ = tx.send(ExportMessage::Done { generation, result });
        });
        Self::spawn_message_pump(cx, rx, |view, message| {
            let ExportMessage::Done { generation, result } = message;
            if generation != view.workspace_generation {
                return true;
            }
            match result {
                Ok(outcome) => {
                    view.export = ExportState {
                        out: outcome.out.clone(),
                        bytes: outcome.bytes,
                        pages: outcome.pages,
                        error: String::new(),
                    };
                    view.notice = Some(view.tf(
                        "notice.exported",
                        &[
                            ("path", &outcome.out),
                            ("pages", &outcome.pages.to_string()),
                        ],
                    ));
                }
                Err(error) => {
                    view.export = ExportState {
                        error: error.clone(),
                        ..ExportState::default()
                    };
                    view.notice = Some(view.tf("notice.export_failed", &[("error", &error)]));
                }
            }
            true
        });
    }

    // ---- project search (M8 slice 2) -------------------------------------

    /// ⌘⇧F: toggle the project-search bar. Like the other bars it owns the
    /// single focus; opening it closes the find/instruction/focus bars and
    /// pre-fills the last query. Closing clears the results.
    fn on_project_search(
        &mut self,
        _: &ProjectSearch,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.doc.find.open {
            self.close_find(cx);
        }
        if self.propose_bar.open {
            self.propose_bar.open = false;
            self.propose_bar.instruction.clear();
        }
        if self.focus_bar.open {
            self.focus_bar.open = false;
            self.focus_bar.focus.clear();
        }
        self.search.open = !self.search.open;
        if self.search.open {
            self.search.query = self.search.searched.clone();
        } else {
            self.search.query.clear();
            self.search.hits.clear();
            self.search.searched.clear();
        }
        cx.notify();
    }

    fn on_search_prev(&mut self, _: &SearchPrev, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.search.hits.is_empty() {
            self.search_move(-1);
        } else {
            // No results on screen: ctrl-up/down scroll the agent panel
            // (M9 slice 2 moved these keys off the option arrows).
            self.scroll_agent_panel(-3);
        }
        cx.notify();
    }

    fn on_search_next(&mut self, _: &SearchNext, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.search.hits.is_empty() {
            self.search_move(1);
        } else {
            self.scroll_agent_panel(3);
        }
        cx.notify();
    }

    fn search_move(&mut self, delta: isize) {
        let len = self.search.hits.len();
        if len == 0 {
            return;
        }
        let current = self.search.selected.min(len - 1) as isize;
        self.search.selected = (current + delta).rem_euclid(len as isize) as usize;
    }

    /// Enter in the search bar: the first Enter runs the query on a worker
    /// thread; pressing it again with the query unchanged opens the selected
    /// result (the same two-step shape as the proposal surface).
    fn submit_search(&mut self, cx: &mut Context<Self>) {
        let query = self.search.query.trim().to_string();
        if query.is_empty() {
            self.search.hits.clear();
            self.search.searched.clear();
            self.search.truncated = false;
            self.search.selected = 0;
            cx.notify();
            return;
        }
        if query == self.search.searched && !self.search.hits.is_empty() {
            self.open_search_hit(cx);
            return;
        }
        let Some(project) = self.project.clone() else {
            self.notice = Some(self.t("notice.fixture_no_project").to_string());
            cx.notify();
            return;
        };
        let generation = self.workspace_generation;
        let (tx, rx) = mpsc::channel();
        let thread_query = query.clone();
        std::thread::spawn(move || {
            let result = narra_core::search::search(&project, &thread_query)
                .map_err(|error| error.to_string());
            let _ = tx.send(SearchMessage::Done {
                generation,
                query: thread_query,
                result,
            });
        });
        Self::spawn_message_pump(cx, rx, |view, message| {
            let SearchMessage::Done {
                generation,
                query,
                result,
            } = message;
            if generation != view.workspace_generation {
                return true;
            }
            match result {
                Ok(report) => {
                    view.search.selected = 0;
                    view.search.truncated = report.truncated;
                    view.search.scanned = report.documents_scanned;
                    view.search.skipped = report.documents_skipped;
                    view.search.searched = query.clone();
                    view.search.hits = report.hits;
                    let empty = view.search.hits.is_empty();
                    view.notice = Some(if empty {
                        view.tf("notice.search_empty", &[("query", &query)])
                    } else {
                        view.tf(
                            "notice.search_done",
                            &[
                                ("n", &view.search.hits.len().to_string()),
                                ("docs", &view.search.scanned.to_string()),
                            ],
                        )
                    });
                }
                Err(error) => {
                    view.search.hits.clear();
                    view.search.searched.clear();
                    view.notice = Some(view.tf("notice.search_failed", &[("error", &error)]));
                }
            }
            true
        });
    }

    /// Open the selected result: move the caret to its line/column. A
    /// different document is loaded from disk — and switching is refused
    /// while the open document has unsaved changes (the established rule),
    /// never silently discarding the buffer.
    fn open_search_hit(&mut self, cx: &mut Context<Self>) {
        let Some(hit) = self.search.hits.get(self.search.selected).cloned() else {
            return;
        };
        let notice = self.tf(
            "notice.search_open",
            &[("document", &hit.document), ("line", &hit.line.to_string())],
        );
        if hit.document == self.doc.path {
            let offset = self.offset_for_line_column(hit.line, hit.column);
            self.set_caret(offset, false);
            self.search.open = false;
            self.search.query.clear();
            self.search.hits.clear();
            self.search.searched.clear();
            self.notice = Some(notice);
            cx.notify();
            self.ensure_caret_visible();
            return;
        }
        match self.open_document_path(&hit.document, true) {
            Ok(()) => {
                let offset = self.offset_for_line_column(hit.line, hit.column);
                self.set_caret(offset, false);
                self.search.open = false;
                self.search.query.clear();
                self.search.hits.clear();
                self.search.searched.clear();
                self.notice = Some(notice);
                cx.notify();
                self.ensure_caret_visible();
            }
            Err(message) => {
                self.notice = Some(message);
                cx.notify();
            }
        }
    }

    /// Byte offset for a 1-based line and character column, clamped to the
    /// line (the buffer can be shorter than the hit, e.g. after edits).
    fn offset_for_line_column(&self, line: usize, column: usize) -> usize {
        let index = line
            .saturating_sub(1)
            .min(self.doc.buffer.line_count().saturating_sub(1));
        let range = self.doc.buffer.line_content_range(index);
        let text = &self.doc.buffer.lines()[index];
        let byte = text
            .char_indices()
            .nth(column.saturating_sub(1))
            .map(|(offset, _)| offset)
            .unwrap_or(text.len());
        (range.start + byte).min(range.end)
    }

    /// Click on an outline row (M8 slice 3): jump to that scene's heading.
    fn select_outline(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.unsaved_prompt.is_some() {
            return;
        }
        self.refresh_scenes();
        let Some(&line) = self.doc.outline_lines.get(index) else {
            return;
        };
        let offset = self.doc.buffer.line_content_range(line).start;
        self.set_caret(offset, false);
        window.focus(&self.focus_handle(cx));
        cx.notify();
        self.ensure_caret_visible();
    }

    /// Click on a result row: select it and open it.
    fn select_search_hit(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.unsaved_prompt.is_some() || index >= self.search.hits.len() {
            return;
        }
        self.search.selected = index;
        self.open_search_hit(cx);
        window.focus(&self.focus_handle(cx));
    }

    /// Fold one proposal message into the view state; when the task ends,
    /// keep the notice bar in sync with the outcome.
    fn apply_propose_message(&mut self, message: ProposeMessage, done: bool) {
        self.propose.apply(message);
        if done {
            self.agent_cancel = None;
            self.notice = Some(match self.propose.status {
                ProposeStatus::Ready => self.tf(
                    "notice.propose_ready",
                    &[("id", &short_id(&self.propose.id))],
                ),
                ProposeStatus::Cancelled => self.t("notice.propose_cancelled").to_string(),
                ProposeStatus::Failed => {
                    self.tf("notice.propose_failed", &[("error", &self.propose.error)])
                }
                ProposeStatus::Running | ProposeStatus::Idle => String::new(),
                ProposeStatus::Applied | ProposeStatus::Rejected => self.tf(
                    "notice.propose_status",
                    &[("status", self.propose_status_word(self.propose.status))],
                ),
            });
        }
    }

    /// ⌘K: toggle the instruction bar (proposal for the caret's scene). The
    /// bar borrows the single focus (spike compromise): the find bar closes
    /// when it opens and vice versa.
    fn on_open_propose(&mut self, _: &OpenPropose, _window: &mut Window, cx: &mut Context<Self>) {
        if self.doc.find.open {
            self.close_find(cx);
        }
        if self.focus_bar.open {
            self.focus_bar.open = false;
            self.focus_bar.focus.clear();
        }
        self.propose_bar.open = !self.propose_bar.open;
        if !self.propose_bar.open {
            self.propose_bar.instruction.clear();
        }
        cx.notify();
    }

    /// ⌘⇧R: toggle the reviewer-focus bar. It opens pre-filled with the
    /// last committed focus (so tweaking it is one edit, not a rewrite) and
    /// owns keystrokes until enter (commit + review) or escape (discard).
    fn on_open_focus_bar(
        &mut self,
        _: &OpenFocusBar,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.doc.find.open {
            self.close_find(cx);
        }
        if self.propose_bar.open {
            self.propose_bar.open = false;
            self.propose_bar.instruction.clear();
        }
        self.focus_bar.open = !self.focus_bar.open;
        if self.focus_bar.open {
            self.focus_bar.focus = self.review_focus.clone();
        } else {
            self.focus_bar.focus.clear();
        }
        cx.notify();
    }

    /// Enter with the focus bar open: commit the focus (empty = general
    /// review) and run the review for the caret's scene.
    fn submit_focus(&mut self, cx: &mut Context<Self>) {
        self.review_focus = self.focus_bar.focus.trim().to_string();
        self.focus_bar.open = false;
        self.focus_bar.focus.clear();
        self.run_review(cx);
    }

    /// Enter with the instruction bar open: submit (empty = explicit no-op).
    fn submit_proposal(&mut self, cx: &mut Context<Self>) {
        let instruction = self.propose_bar.instruction.trim().to_string();
        self.propose_bar.open = false;
        self.propose_bar.instruction.clear();
        if instruction.is_empty() {
            self.notice = Some(self.t("notice.empty_instruction").to_string());
            cx.notify();
            return;
        }
        self.run_propose_task(instruction, cx);
    }

    /// ⌘⌥← / ⌘⌥→: walk the record's hunks (M6 slice 6). Only meaningful
    /// once the record's hunk count is known; otherwise a no-op (the panel
    /// isn't showing a proposal).
    fn on_hunk_prev(&mut self, _: &HunkPrev, _window: &mut Window, cx: &mut Context<Self>) {
        if self.proposal_hunks_active() {
            self.propose.move_hunk_cursor(-1);
            cx.notify();
        }
    }

    fn on_hunk_next(&mut self, _: &HunkNext, _window: &mut Window, cx: &mut Context<Self>) {
        if self.proposal_hunks_active() {
            self.propose.move_hunk_cursor(1);
            cx.notify();
        }
    }

    /// ⌥Enter: toggle the hunk under the cursor (M6 slice 6). Arming is
    /// cleared: changing the selection invalidates a pending confirmation.
    fn on_toggle_hunk(&mut self, _: &ToggleHunk, _window: &mut Window, cx: &mut Context<Self>) {
        if self.proposal_hunks_active() {
            self.propose.toggle_hunk();
            // A pending "apply" confirmation referenced the previous
            // selection; drop it rather than apply a different set.
            self.propose.armed_apply = false;
            cx.notify();
        }
    }

    /// ⌘⇧Enter: apply the ready proposal. Two-step by design: the first
    /// press arms (writing the canonical file is never one keystroke), the
    /// second one applies through the core (content-hash checked, atomic).
    fn on_apply_proposal(
        &mut self,
        _: &ApplyProposal,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.doc.preview {
            return;
        }
        if !self.propose.can_apply() {
            self.notice = Some(self.t("notice.nothing_to_apply").to_string());
            cx.notify();
            return;
        }
        if !self.propose.armed_apply {
            self.propose.armed_apply = true;
            let selection = if self.propose.all_selected() {
                self.tf("notice.hunks_n", &[("n", &self.propose.hunks.to_string())])
            } else {
                let indices = self
                    .propose
                    .selected_hunks()
                    .iter()
                    .map(|ix| (ix + 1).to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                format!(
                    "{} [{indices}]",
                    self.tf(
                        "notice.hunks_n",
                        &[(
                            "n",
                            &format!(
                                "{}/{}",
                                self.propose.selected_hunks().len(),
                                self.propose.hunks
                            )
                        )],
                    )
                )
            };
            self.notice = Some(self.tf(
                "notice.apply_arm",
                &[
                    ("selection", &selection),
                    ("id", &short_id(&self.propose.id)),
                    ("target", &self.propose.target),
                ],
            ));
            cx.notify();
            return;
        }
        let Some(root) = self
            .project
            .as_ref()
            .map(|project| project.root().to_string_lossy().into_owned())
        else {
            self.notice = Some(self.t("notice.fixture_no_project_short").to_string());
            cx.notify();
            return;
        };
        // The proposal names its target document. With tabs the writer may
        // have switched elsewhere; activate the target and refuse a dirty
        // target buffer, because applying writes the canonical file and a
        // stale buffer must never be left looking current.
        let target = self.propose.target.clone();
        if target.is_empty() {
            self.notice = Some(self.t("notice.nothing_to_apply").to_string());
            cx.notify();
            return;
        }
        if let Err(message) = self.open_document_path(&target, true) {
            self.notice = Some(message);
            cx.notify();
            return;
        }
        if self.dirty() {
            self.notice = Some(self.t("notice.apply_target_dirty").to_string());
            cx.notify();
            return;
        }
        // The selected subset (M6 slice 6): all selected → None (apply-all,
        // the core's canonical path); a subset → Some(indices); nothing
        // selected → explicit refusal (applying nothing would rewrite the
        // file for no change and mark the record applied).
        let accepted: Option<Vec<usize>> = if self.propose.all_selected() {
            None
        } else {
            let selected = self.propose.selected_hunks();
            if selected.is_empty() {
                self.notice =
                    Some("sin hunks seleccionados: ⌥Enter selecciona al menos uno".to_string());
                cx.notify();
                return;
            }
            Some(selected)
        };
        match proposal::apply(Path::new(&root), &self.propose.id, accepted.as_deref()) {
            Ok(outcome) => {
                self.propose.status = ProposeStatus::Applied;
                self.propose.armed_apply = false;
                let applied = outcome.applied_hunks.len();
                let skipped = outcome.skipped_hunks.len();
                let target = outcome.proposal.target.clone();
                // Reload the buffer when the applied target is the open
                // document, so the user sees the applied result immediately.
                let reloaded = self
                    .project
                    .as_ref()
                    .filter(|_| self.doc.path == target)
                    .map(|project| project.read_document(&self.doc.path));
                let mut final_notice = if skipped > 0 {
                    self.tf(
                        "notice.applied_skipped",
                        &[
                            ("applied", &applied.to_string()),
                            ("target", &target),
                            ("skipped", &skipped.to_string()),
                        ],
                    )
                } else {
                    self.tf(
                        "notice.applied",
                        &[("applied", &applied.to_string()), ("target", &target)],
                    )
                };
                match reloaded {
                    Some(Ok(text)) => {
                        let path = self.doc.path.clone();
                        self.load_document(path, text);
                    }
                    Some(Err(error)) => {
                        final_notice = self.tf(
                            "notice.apply_reload_failed",
                            &[("error", &error.to_string())],
                        );
                    }
                    None => {}
                }
                self.notice = Some(final_notice);
            }
            Err(error) => {
                self.propose.armed_apply = false;
                self.propose.status = ProposeStatus::Failed;
                self.propose.error = error.to_string();
                self.notice =
                    Some(self.tf("notice.apply_failed", &[("error", &error.to_string())]));
            }
        }
        cx.notify();
    }

    /// ⌘⌫: reject the proposal. The record says so; no canonical file is
    /// touched (the core's reject contract).
    fn on_reject_proposal(
        &mut self,
        _: &RejectProposal,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.propose.is_open() || self.propose.id.is_empty() {
            self.notice = Some(self.t("notice.nothing_to_reject").to_string());
            cx.notify();
            return;
        }
        if matches!(
            self.propose.status,
            ProposeStatus::Applied | ProposeStatus::Rejected
        ) {
            self.notice = Some(self.tf(
                "notice.already_status",
                &[("status", self.propose_status_word(self.propose.status))],
            ));
            cx.notify();
            return;
        }
        let Some(root) = self
            .project
            .as_ref()
            .map(|project| project.root().to_string_lossy().into_owned())
        else {
            self.notice = Some(self.t("notice.fixture_no_project_short").to_string());
            cx.notify();
            return;
        };
        match proposal::reject(Path::new(&root), &self.propose.id) {
            Ok(_) => {
                self.propose.status = ProposeStatus::Rejected;
                self.propose.armed_apply = false;
                self.notice =
                    Some(self.tf("notice.rejected", &[("id", &short_id(&self.propose.id))]));
            }
            Err(error) => {
                self.notice =
                    Some(self.tf("notice.reject_failed", &[("error", &error.to_string())]));
            }
        }
        cx.notify();
    }

    /// ⌘.: cancel the in-flight review. Cooperative: the task checks the
    /// token between steps, so a model call already in flight finishes but
    /// no further work happens.
    fn on_cancel_review(&mut self, _: &CancelReview, _window: &mut Window, cx: &mut Context<Self>) {
        if self.chat.status == ChatStatus::Running {
            self.chat_stop(cx);
            return;
        }
        if let Some(token) = self.agent_cancel.take() {
            token.cancel();
            self.notice = Some(self.t("notice.cancelling_review").to_string());
        } else {
            self.notice = Some(self.t("notice.no_review_running").to_string());
        }
    }

    /// ⌥↑/⌥↓: scroll the review text inside the agent panel.
    fn on_review_scroll_up(
        &mut self,
        _: &ReviewScrollUp,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        self.scroll_agent_panel(-3);
    }

    fn on_review_scroll_down(
        &mut self,
        _: &ReviewScrollDown,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        self.scroll_agent_panel(3);
    }

    /// Scroll whichever agent panel section is visible (proposal diff while
    /// a proposal is open, review text otherwise) by `delta` lines.
    fn scroll_agent_panel(&mut self, delta: i64) {
        if self.propose.is_open() && !self.agent.is_open() {
            let total = self.propose.diff_rows.len();
            self.propose.scroll_lines(delta, total, AGENT_VISIBLE_LINES);
        } else {
            let total = self.agent.output.lines().count();
            self.agent.scroll_lines(delta, total, AGENT_VISIBLE_LINES);
        }
    }

    // ---- chat workspace (M12) -------------------------------------------

    fn on_focus_chat(&mut self, _: &FocusChat, window: &mut Window, cx: &mut Context<Self>) {
        self.agent_panel_open = true;
        self.right_tab = RightTab::Chat;
        self.chat.focused = true;
        self.chat_history_open = false;
        window.focus(&self.focus_handle(cx));
        cx.notify();
    }

    fn on_chat_newline(&mut self, _: &ChatNewline, _window: &mut Window, cx: &mut Context<Self>) {
        if self.chat.focused {
            self.chat.newline();
            cx.notify();
        }
    }

    fn chat_set_mode(&mut self, mode: ChatMode, cx: &mut Context<Self>) {
        self.chat.set_mode(mode);
        self.chat.clear_error();
        self.chat.focused = true;
        cx.notify();
    }

    fn open_ai_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.is_none() {
            self.on_open_settings(&OpenSettings, window, cx);
        }
        if let Some(settings) = self.settings.as_mut() {
            settings.section = SettingsSection::Ai;
        }
        cx.notify();
    }

    fn chat_attach_scene(&mut self, cx: &mut Context<Self>) {
        let caret_line = self.doc.buffer.position_at(self.caret()).line;
        if self.doc.scene_lines.is_empty() {
            self.notice = Some(self.t("chat.no_scene").to_string());
            cx.notify();
            return;
        }
        let index = self
            .doc
            .scene_lines
            .partition_point(|line| *line <= caret_line)
            .saturating_sub(1);
        let start = self.doc.scene_lines[index];
        let end = self
            .doc
            .scene_lines
            .get(index + 1)
            .copied()
            .unwrap_or_else(|| self.doc.buffer.line_count());
        let text: Vec<String> = self.doc.buffer.lines()[start..end].to_vec();
        self.chat.add_attachment(ChatAttachment::new(
            "scene",
            format!("Escena {}", index + 1),
            format!("{}:L{}-L{}", self.doc.path, start + 1, end),
            text.join("\n"),
        ));
        self.chat.focused = true;
        cx.notify();
    }

    fn chat_attach_selection(&mut self, cx: &mut Context<Self>) {
        if self.doc.selected.is_empty() {
            self.notice = Some(self.t("chat.no_selection").to_string());
            cx.notify();
            return;
        }
        let start = self.doc.buffer.position_at(self.doc.selected.start);
        let end = self.doc.buffer.position_at(self.doc.selected.end);
        self.chat.add_attachment(ChatAttachment::new(
            "selection",
            self.t("chat.selection_chip").to_string(),
            format!("{}:L{}-L{}", self.doc.path, start.line + 1, end.line + 1),
            self.doc.buffer.slice(self.doc.selected.clone()),
        ));
        self.chat.focused = true;
        cx.notify();
    }

    fn chat_attach_document(&mut self, cx: &mut Context<Self>) {
        let text = self.doc.buffer.slice(0..self.doc.buffer.doc_len());
        self.chat.add_attachment(ChatAttachment::new(
            "document",
            self.doc
                .path
                .rsplit('/')
                .next()
                .unwrap_or("documento")
                .to_string(),
            self.doc.path.clone(),
            text,
        ));
        self.chat.focused = true;
        cx.notify();
    }

    fn chat_send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.chat.status == ChatStatus::Running || !self.chat.can_send() {
            return;
        }
        let raw = self.chat.input.trim().to_string();
        if raw.is_empty() {
            return;
        }
        let (mode, instruction) = match split_chat_command(&raw) {
            Some(("/plan", rest)) => {
                self.chat.set_mode(ChatMode::Plan);
                if rest.is_empty() {
                    self.chat.set_input(String::new());
                    cx.notify();
                    return;
                }
                (ChatMode::Plan, rest.to_string())
            }
            Some(("/build", rest)) => {
                self.chat.set_mode(ChatMode::Build);
                if rest.is_empty() {
                    self.chat.set_input(String::new());
                    cx.notify();
                    return;
                }
                (ChatMode::Build, rest.to_string())
            }
            Some(("/review", rest)) => {
                self.chat.set_mode(ChatMode::Review);
                if rest.is_empty() {
                    self.chat.set_input(String::new());
                    cx.notify();
                    return;
                }
                (ChatMode::Review, rest.to_string())
            }
            Some(("/rewrite", rest)) => {
                self.chat.set_mode(ChatMode::Build);
                if rest.is_empty() {
                    self.chat
                        .set_input(self.t("chat.rewrite_prompt").to_string());
                    cx.notify();
                    return;
                }
                (ChatMode::Build, rest.to_string())
            }
            Some(("/continuity", rest)) => {
                self.chat.set_mode(ChatMode::Review);
                if rest.is_empty() {
                    self.chat
                        .set_input(self.t("chat.continuity_prompt").to_string());
                    cx.notify();
                    return;
                }
                (ChatMode::Review, rest.to_string())
            }
            _ => (self.chat.mode(), raw),
        };
        let Some(project) = self.project.as_ref() else {
            self.notice = Some(self.t("notice.fixture_no_project").to_string());
            cx.notify();
            return;
        };
        let root = project.root().to_string_lossy().into_owned();
        let history = self.chat.thread.messages.clone();
        let attachments = self.chat.attachments.clone();
        self.chat.set_input(String::new());
        self.chat
            .begin_run(instruction.clone(), attachments.clone());
        // Sending is an explicit action: show the new turn even if the reader
        // had scrolled up. Streaming afterwards follows only while at bottom.
        self.chat_scroll.scroll_to_bottom();
        self.chat.focused = true;
        self.agent_panel_open = true;
        self.right_tab = RightTab::Chat;
        self.chat_generation = self.chat_generation.saturating_add(1);
        let generation = self.chat_generation;
        let cancel = CancellationToken::new();
        self.chat_cancel = Some(cancel.clone());
        let task = ChatTask::new(root, mode, history, instruction).with_attachments(attachments);
        let settings = self.agent_settings.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let options = TaskOptions {
                timeout: AGENT_TIMEOUT,
                cancel,
                stream: mode != ChatMode::Build,
            };
            let result = match settings.build_adapter() {
                Ok(adapter) => task.run(&*adapter, &options, |event| {
                    let _ = tx.send(ChatRunMessage::Event(event));
                }),
                Err(error) => Err(AgentError::Adapter(error)),
            };
            let _ = tx.send(ChatRunMessage::Done(result));
        });
        self.save_chat_thread();
        Self::spawn_message_pump(cx, rx, move |view, message| {
            if generation != view.chat_generation {
                return true;
            }
            let done = matches!(message, ChatRunMessage::Done(_));
            match message {
                ChatRunMessage::Event(event) => {
                    view.chat.apply_event(&event);
                    view.chat_follow_stream();
                }
                ChatRunMessage::Done(result) => {
                    view.chat_cancel = None;
                    view.chat.finish(result);
                    view.chat_follow_stream();
                    view.load_chat_proposal_diffs();
                    view.save_chat_thread();
                    view.notice = Some(match view.chat.status {
                        ChatStatus::Completed => view.t("chat.completed").to_string(),
                        ChatStatus::Failed => {
                            view.tf("chat.failed", &[("error", &view.chat.error.clone())])
                        }
                        ChatStatus::Cancelled => view.t("chat.cancelled").to_string(),
                        _ => view.t("chat.completed").to_string(),
                    });
                }
            }
            done
        });
        window.focus(&self.focus_handle(cx));
        self.refresh_chat_scrollbar(cx);
    }

    fn chat_stop(&mut self, cx: &mut Context<Self>) {
        if let Some(cancel) = self.chat_cancel.take() {
            cancel.cancel();
            self.notice = Some(self.t("chat.stopping").to_string());
        } else {
            self.notice = Some(self.t("chat.not_running").to_string());
        }
        cx.notify();
    }

    /// Whether the transcript is at (or within two pixels of) the bottom.
    fn chat_at_bottom(&self) -> bool {
        scrollbar::at_bottom(
            f32::from(self.chat_scroll.offset().y),
            f32::from(self.chat_scroll.max_offset().height),
        )
    }

    /// Follow new assistant text only while the reader is at the bottom;
    /// scrolling up pauses the follow until they return (D-042).
    fn chat_follow_stream(&mut self) {
        if self.chat_at_bottom() {
            self.chat_scroll.scroll_to_bottom();
        }
    }

    /// Start a scrollbar drag from a click on the track or thumb: the click
    /// centers the thumb under the pointer and the root move handler
    /// translates the pointer into an offset.
    fn begin_chat_scroll_drag(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let bounds = self.chat_scroll.bounds();
        let viewport_h = f32::from(bounds.size.height);
        let max_offset = f32::from(self.chat_scroll.max_offset().height);
        if viewport_h <= 0.0 || max_offset <= 0.0 {
            return;
        }
        let pointer_y = f32::from(event.position.y) - f32::from(bounds.top());
        let offset = scrollbar::offset_for_pointer(pointer_y, viewport_h, max_offset);
        self.chat_scroll.set_offset(point(px(0.0), px(offset)));
        self.chat_scroll_drag = Some(ChatScrollDrag {
            start_y: f32::from(event.position.y),
            start_offset: offset,
            viewport_h,
            max_offset,
        });
        self.panel_drag = None;
        self.chat.focused = false;
        window.focus(&self.focus_handle(cx));
        cx.notify();
    }

    fn chat_new_thread(&mut self, cx: &mut Context<Self>) {
        if self.chat.status == ChatStatus::Running {
            return;
        }
        let mode = self.chat.mode();
        self.chat = ChatUiState::new(mode);
        self.chat_markdown.borrow_mut().clear();
        self.chat_scroll = ScrollHandle::new();
        self.chat_scroll_drag = None;
        self.chat.focused = true;
        self.chat_history_open = false;
        self.right_tab = RightTab::Chat;
        self.refresh_chat_scrollbar(cx);
    }

    fn chat_toggle_history(&mut self, cx: &mut Context<Self>) {
        if self.chat_history_open {
            self.chat_history_open = false;
        } else {
            self.chat_history = self
                .project
                .as_ref()
                .map(|project| ChatThread::list(project.root()))
                .unwrap_or_default();
            self.chat_history_open = true;
        }
        cx.notify();
    }

    fn chat_load_thread(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(thread) = self.chat_history.get(index).cloned() else {
            return;
        };
        self.chat = ChatUiState::from_thread(thread);
        self.chat_markdown.borrow_mut().clear();
        self.chat_scroll = ScrollHandle::new();
        self.chat_scroll_drag = None;
        self.chat_scroll.scroll_to_bottom();
        self.chat_history_open = false;
        self.right_tab = RightTab::Chat;
        self.load_chat_proposal_diffs();
        self.refresh_chat_scrollbar(cx);
    }

    fn save_chat_thread(&mut self) {
        let Some(project) = self.project.as_ref() else {
            return;
        };
        if self.chat.thread.messages.is_empty() {
            return;
        }
        if let Err(error) = self.chat.thread.save(project.root()) {
            self.notice = Some(self.tf("chat.save_failed", &[("error", &error)]));
        }
    }

    fn load_chat_proposal_diffs(&mut self) {
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let root = project.root().to_path_buf();
        for view in &mut self.chat.proposals {
            if !view.diff_rows.is_empty() {
                continue;
            }
            let Ok(record) = proposal::load(&root, &view.id) else {
                continue;
            };
            view.hunks = record.hunks.len();
            view.selected = vec![true; view.hunks];
            view.diff_rows = diff_rows_from_hunks(&record.hunks);
            view.status = match record.state {
                proposal::ProposalState::Applied => ProposalCardStatus::Applied,
                proposal::ProposalState::Rejected => ProposalCardStatus::Rejected,
                proposal::ProposalState::Stale => ProposalCardStatus::Failed,
                _ => ProposalCardStatus::Ready,
            };
        }
    }

    fn chat_apply_proposal(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.doc.preview {
            return;
        }
        let Some(project) = self.project.as_ref() else {
            self.notice = Some(self.t("notice.fixture_no_project_short").to_string());
            cx.notify();
            return;
        };
        let root = project.root().to_path_buf();
        let Some(view) = self.chat.proposal(&id).cloned() else {
            return;
        };
        if view.status != ProposalCardStatus::Ready {
            return;
        }
        if !view.armed {
            if let Some(view) = self.chat.proposal_mut(&id) {
                view.armed = true;
            }
            self.notice = Some(self.t("chat.apply_arm").to_string());
            cx.notify();
            return;
        }
        let accepted = if view.all_selected() {
            None
        } else {
            let selected = view.selected_hunks();
            if selected.is_empty() {
                self.notice = Some(self.t("chat.select_hunk").to_string());
                cx.notify();
                return;
            }
            Some(selected)
        };
        match proposal::apply(&root, &id, accepted.as_deref()) {
            Ok(outcome) => {
                if let Some(view) = self.chat.proposal_mut(&id) {
                    view.status = ProposalCardStatus::Applied;
                    view.armed = false;
                }
                let target = outcome.proposal.target.clone();
                if self.doc.path == target
                    && let Some(project) = self.project.as_ref()
                    && let Ok(text) = project.read_document(&target)
                {
                    self.load_document(target.clone(), text);
                }
                self.notice = Some(self.tf("chat.applied", &[("target", &target)]));
                self.chat
                    .thread
                    .push_assistant(self.t("chat.applied_note").to_string());
                self.save_chat_thread();
                window.focus(&self.focus_handle(cx));
            }
            Err(error) => {
                if let Some(view) = self.chat.proposal_mut(&id) {
                    view.status = ProposalCardStatus::Failed;
                    view.error = error.to_string();
                    view.armed = false;
                }
                self.notice = Some(self.tf("chat.apply_failed", &[("error", &error.to_string())]));
            }
        }
        cx.notify();
    }

    fn chat_proposal_move(&mut self, id: &str, delta: isize, cx: &mut Context<Self>) {
        if let Some(view) = self.chat.proposal_mut(id) {
            view.move_cursor(delta);
        }
        cx.notify();
    }

    fn chat_proposal_toggle(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(view) = self.chat.proposal_mut(id) {
            view.toggle();
        }
        cx.notify();
    }

    fn chat_reject_proposal(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(project) = self.project.as_ref() else {
            return;
        };
        match proposal::reject(project.root(), &id) {
            Ok(_) => {
                if let Some(view) = self.chat.proposal_mut(&id) {
                    view.status = ProposalCardStatus::Rejected;
                    view.armed = false;
                }
                self.chat
                    .thread
                    .push_assistant(self.t("chat.rejected_note").to_string());
                self.save_chat_thread();
                self.notice = Some(self.t("chat.rejected").to_string());
            }
            Err(error) => {
                self.notice = Some(self.tf("chat.reject_failed", &[("error", &error.to_string())]));
            }
        }
        cx.notify();
    }

    fn right_tab_bar(&self, cx: &mut Context<Self>) -> gpui::Div {
        div()
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .py_1()
            .border_b_1()
            .border_color(rgb(theme::explorer_border()))
            .child(self.right_tab_button("chat.tab.chat", RightTab::Chat, cx))
            .child(self.right_tab_button("chat.tab.activity", RightTab::Activity, cx))
    }

    fn right_tab_button(
        &self,
        label: &'static str,
        tab: RightTab,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let selected = self.right_tab == tab;
        div()
            .id(label)
            .px_2()
            .py_1()
            .text_sm()
            .cursor_pointer()
            .text_color(rgb(if selected {
                theme::default_fg()
            } else {
                theme::status_fg()
            }))
            .when(selected, |button| button.bg(rgb(theme::doc_selected_bg())))
            .child(self.t(label))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.right_tab = tab;
                if tab == RightTab::Chat {
                    this.chat_history_open = false;
                }
                cx.notify();
            }))
            .into_any_element()
    }

    /// A 6 px handle between a side panel and the document column. The width
    /// lives in `explorer_w`/`right_panel_w`; the drag itself is handled by the
    /// root `on_mouse_move` (M16 slice 3).
    fn panel_splitter(
        &self,
        target: panel_resize::Target,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let id = match target {
            panel_resize::Target::Explorer => "explorer-splitter",
            panel_resize::Target::RightPanel => "right-splitter",
        };
        div()
            .id(id)
            .w(px(panel_resize::SPLITTER_W))
            .h_full()
            .flex_none()
            .flex()
            .justify_center()
            .cursor_col_resize()
            .bg(rgb(theme::explorer_bg()))
            .child(div().w(px(1.0)).h_full().bg(rgb(theme::explorer_border())))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.begin_panel_resize(target, f32::from(event.position.x), window, cx)
                }),
            )
            .into_any_element()
    }

    /// The app-drawn chat scrollbar (M16 slice 4): a 10 px gutter with a
    /// thumb sized and positioned from the scroll handle. `None` while the
    /// transcript fits, so no empty track is shown.
    fn chat_scrollbar(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let viewport_h = f32::from(self.chat_scroll.bounds().size.height);
        let max_offset = f32::from(self.chat_scroll.max_offset().height);
        let offset_y = f32::from(self.chat_scroll.offset().y);
        let thumb = scrollbar::thumb(viewport_h, max_offset, offset_y);
        let has_thumb = thumb.is_some();
        let mut track = div()
            .id("chat-scrollbar")
            .w(px(scrollbar::WIDTH))
            .h_full()
            .flex_none()
            .relative()
            .bg(rgb(if has_thumb {
                theme::status_bg()
            } else {
                theme::explorer_bg()
            }))
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, cx.listener(Self::begin_chat_scroll_drag));
        if let Some((thumb_top, thumb_h)) = thumb {
            track = track.child(
                div()
                    .id("chat-scrollbar-thumb")
                    .absolute()
                    .top(px(thumb_top))
                    .right(px((scrollbar::WIDTH - scrollbar::THUMB_W) / 2.0))
                    .w(px(scrollbar::THUMB_W))
                    .h(px(thumb_h))
                    .bg(rgb(theme::gutter_fg()))
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::begin_chat_scroll_drag)),
            );
        }
        track.into_any_element()
    }

    /// The thumb is read from the handle's last layout, so a newly loaded
    /// thread needs one extra frame before the scrollbar appears. The same
    /// frame is the first with the final content height, so scroll to the real
    /// bottom then (the first `scroll_to_bottom` can run against a shorter
    /// content).
    fn refresh_chat_scrollbar(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        cx.spawn(async move |view, cx| {
            Timer::after(Duration::from_millis(16)).await;
            view.update(cx, |view, cx| {
                view.chat_scroll.scroll_to_bottom();
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn chat_panel(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = |key: &str| self.t(key).to_string();
        let mut panel = div()
            .id("chat-panel")
            .w(px(self.right_panel_w))
            .h_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(rgb(theme::explorer_bg()))
            .border_l_1()
            .border_color(rgb(theme::explorer_border()))
            .text_sm()
            .font_family(FONT_FAMILY)
            .child(self.right_tab_bar(cx));
        let header = div()
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .py_1()
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_color(rgb(theme::default_fg()))
                    .child(if self.chat.thread.title.is_empty() {
                        t("chat.new")
                    } else {
                        self.chat.thread.title.clone()
                    }),
            )
            .child(
                div()
                    .id("chat-new")
                    .px_2()
                    .py_1()
                    .cursor_pointer()
                    .text_color(rgb(theme::status_fg()))
                    .child("+")
                    .on_click(cx.listener(|this, _, _, cx| this.chat_new_thread(cx))),
            )
            .child(
                div()
                    .id("chat-history")
                    .px_2()
                    .py_1()
                    .cursor_pointer()
                    .text_color(rgb(theme::status_fg()))
                    .child(icon("list-tree", theme::status_fg(), 14.0))
                    .on_click(cx.listener(|this, _, _, cx| this.chat_toggle_history(cx))),
            );
        if self.chat_history_open {
            let mut history = div()
                .id("chat-history-list")
                .max_h(px(180.0))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .border_1()
                .border_color(rgb(theme::explorer_border()));
            if self.chat_history.is_empty() {
                history = history.child(
                    div()
                        .px_2()
                        .py_1()
                        .text_color(rgb(theme::status_fg()))
                        .child(t("chat.no_history")),
                );
            }
            for (index, thread) in self.chat_history.iter().enumerate() {
                let label = if thread.title.is_empty() {
                    thread.id.clone()
                } else {
                    thread.title.clone()
                };
                history = history.child(
                    div()
                        .id(("chat-history-row", index))
                        .px_2()
                        .py_1()
                        .cursor_pointer()
                        .text_color(rgb(theme::status_fg()))
                        .child(format!("{} · {}", label, thread.mode.key()))
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.chat_load_thread(index, cx)),
                        ),
                );
            }
            panel = panel.child(history);
        }
        panel = panel.child(header);
        let mut messages = div()
            .id("chat-messages")
            .flex_1()
            // A flex item defaults to its content's minimum width; without
            // this the transcript pushes the scrollbar column out of the panel.
            .min_w(px(0.0))
            .track_scroll(&self.chat_scroll)
            .overflow_y_scroll()
            // Wheel scrolling moves the handle; re-render so the app-drawn
            // thumb follows (M16 slice 4).
            .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()))
            .flex()
            .flex_col()
            .gap_2()
            .p_2();
        if self.chat.thread.messages.is_empty() && self.chat.streaming.is_empty() {
            messages = messages
                .child(
                    div()
                        .text_color(rgb(theme::status_fg()))
                        .child(t("chat.empty_title")),
                )
                .child(
                    div()
                        .text_color(rgb(theme::gutter_fg()))
                        .child(t("chat.empty_hint")),
                );
        }
        for (index, turn) in self.chat.thread.messages.iter().enumerate() {
            let user = turn.role == narra_core::agent::ChatTurnRole::User;
            // Assistant answers are Markdown; the cache keys on content so a
            // re-render (streaming, scroll) does not re-parse every turn
            // (M16 slice 5). Streaming text below stays plain until complete,
            // which avoids flicker and half-written code fences.
            let body = if user {
                div()
                    .whitespace_normal()
                    .child(turn.content.clone())
                    .into_any_element()
            } else {
                let key = markdown_key(&turn.content);
                let mut cache = self.chat_markdown.borrow_mut();
                let document = cache
                    .entry(key)
                    .or_insert_with(|| MarkdownDocument::parse(&turn.content));
                markdown_view::blocks(document, self.lang).into_any_element()
            };
            let mut card = div()
                .id(("chat-turn", index))
                .flex()
                .flex_col()
                .gap_1()
                .px_2()
                .py_1()
                .border_1()
                .border_color(rgb(theme::explorer_border()))
                .bg(rgb(if user {
                    theme::bg()
                } else {
                    theme::explorer_bg()
                }))
                .text_color(rgb(if user {
                    theme::cue_fg()
                } else {
                    theme::default_fg()
                }))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(theme::gutter_fg()))
                        .child(t(if user { "chat.you" } else { "chat.assistant" })),
                )
                .child(body);
            if !user {
                card = card.child(
                    div()
                        .text_xs()
                        .text_color(rgb(theme::gutter_fg()))
                        .child(t("chat.assistant_note")),
                );
            }
            messages = messages.child(card);
        }
        if self.chat.status == ChatStatus::Running {
            let streaming = if self.chat.streaming.is_empty() {
                t("chat.working")
            } else {
                self.chat.streaming.clone()
            };
            messages = messages.child(
                div()
                    .px_2()
                    .py_1()
                    .border_1()
                    .border_color(rgb(theme::explorer_border()))
                    .text_color(rgb(theme::status_fg()))
                    .child(streaming),
            );
        }
        if !self.chat.sources.is_empty() {
            let mut sources = div()
                .mx_2()
                .mb_2()
                .border_1()
                .border_color(rgb(theme::explorer_border()))
                .flex()
                .flex_col();
            sources = sources.child(
                div()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .text_color(rgb(theme::gutter_fg()))
                    .child(self.tf(
                        "chat.sources",
                        &[("n", &self.chat.sources.len().to_string())],
                    )),
            );
            for source in &self.chat.sources {
                sources = sources.child(
                    div()
                        .px_2()
                        .text_xs()
                        .text_color(rgb(theme::gutter_fg()))
                        .child(format!("{} · {}", source.kind, source.location)),
                );
            }
            messages = messages.child(sources);
        }
        for (index, view) in self.chat.proposals.iter().enumerate() {
            messages = messages.child(self.chat_proposal_card(view, index, cx));
        }
        if !self.chat.error.is_empty() {
            messages = messages.child(
                div()
                    .px_2()
                    .py_1()
                    .text_color(rgb(theme::agent_err_fg()))
                    .child(self.chat.error.clone()),
            );
        }
        // The scrollbar is a reserved column next to the transcript, not an
        // overlay: the scroll container cannot swallow its clicks (M16 slice 4).
        let transcript = div()
            .id("chat-transcript")
            .flex_1()
            .flex()
            .overflow_hidden()
            .child(messages)
            .child(self.chat_scrollbar(cx));
        panel = panel.child(transcript);
        panel = panel.child(self.chat_composer(cx));
        panel.into_any_element()
    }

    fn chat_proposal_card(
        &self,
        view: &ChatProposalView,
        index: usize,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let status = view.status_key();
        let mut card = div()
            .mx_2()
            .mb_2()
            .border_1()
            .border_color(rgb(if view.status == ProposalCardStatus::Ready {
                theme::git_modified_fg()
            } else {
                theme::explorer_border()
            }))
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .child(
                div()
                    .text_color(rgb(theme::default_fg()))
                    .child(format!("{} · {}", view.target, status)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(theme::gutter_fg()))
                    .child(format!(
                        "{} hunks · {} selected",
                        view.hunks,
                        view.selected_hunks().len()
                    )),
            );
        if view.hunks > 0 {
            let checked = view.selected.get(view.cursor).copied().unwrap_or(false);
            card = card.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_xs()
                    .child(
                        div()
                            .id(("chat-hunk-prev", index))
                            .px_1()
                            .cursor_pointer()
                            .text_color(rgb(theme::status_fg()))
                            .child("◀")
                            .on_click(cx.listener({
                                let id = view.id.clone();
                                move |this, _, _, cx| this.chat_proposal_move(&id, -1, cx)
                            })),
                    )
                    .child(
                        div()
                            .id(("chat-hunk-toggle", index))
                            .px_1()
                            .cursor_pointer()
                            .text_color(rgb(if checked {
                                theme::default_fg()
                            } else {
                                theme::gutter_fg()
                            }))
                            .child(format!(
                                "hunk {}/{} [{}]",
                                view.cursor + 1,
                                view.hunks,
                                if checked { "x" } else { " " }
                            ))
                            .on_click(cx.listener({
                                let id = view.id.clone();
                                move |this, _, _, cx| this.chat_proposal_toggle(&id, cx)
                            })),
                    )
                    .child(
                        div()
                            .id(("chat-hunk-next", index))
                            .px_1()
                            .cursor_pointer()
                            .text_color(rgb(theme::status_fg()))
                            .child("▶")
                            .on_click(cx.listener({
                                let id = view.id.clone();
                                move |this, _, _, cx| this.chat_proposal_move(&id, 1, cx)
                            })),
                    ),
            );
        }
        if !view.error.is_empty() {
            card = card.child(
                div()
                    .text_color(rgb(theme::agent_err_fg()))
                    .child(view.error.clone()),
            );
        }
        for row in view.diff_rows.iter().take(20) {
            card = card.child(
                div()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_xs()
                    .text_color(rgb(match row.0 {
                        '+' => theme::git_added_fg(),
                        '-' => theme::agent_err_fg(),
                        _ => theme::gutter_fg(),
                    }))
                    .child(format!("{}{}", row.0, row.1)),
            );
        }
        if view.status == ProposalCardStatus::Ready {
            card = card.child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        div()
                            .id(("chat-reject", index))
                            .px_2()
                            .py_1()
                            .cursor_pointer()
                            .bg(rgb(theme::status_bg()))
                            .text_color(rgb(theme::status_fg()))
                            .child(self.t("chat.reject"))
                            .on_click(cx.listener({
                                let id = view.id.clone();
                                move |this, _, _, cx| this.chat_reject_proposal(id.clone(), cx)
                            })),
                    )
                    .child(
                        div()
                            .id(("chat-apply", index))
                            .px_2()
                            .py_1()
                            .cursor_pointer()
                            .bg(rgb(theme::status_bg()))
                            .text_color(rgb(theme::default_fg()))
                            .child(if view.armed {
                                self.t("chat.apply_confirm")
                            } else {
                                self.t("chat.apply")
                            })
                            .on_click(cx.listener({
                                let id = view.id.clone();
                                move |this, _, window, cx| {
                                    this.chat_apply_proposal(id.clone(), window, cx)
                                }
                            })),
                    ),
            );
        }
        card
    }

    fn chat_composer(&self, cx: &mut Context<Self>) -> gpui::Div {
        let t = |key: &str| self.t(key).to_string();
        let mut attachments = div().flex().flex_wrap().gap_1();
        for (index, attachment) in self.chat.attachments.iter().enumerate() {
            attachments = attachments.child(
                div()
                    .id(("chat-attachment", index))
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .bg(rgb(theme::bg()))
                    .border_1()
                    .border_color(rgb(theme::explorer_border()))
                    .text_color(rgb(theme::cue_fg()))
                    .child(attachment.label.clone())
                    .child(
                        div()
                            .id(("chat-attachment-remove", index))
                            .cursor_pointer()
                            .child("×")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.chat.remove_attachment(index);
                                cx.notify();
                            })),
                    ),
            );
        }
        let mut modes = div().flex().items_center().gap_1();
        for mode in ChatMode::ALL {
            let selected = self.chat.mode() == mode;
            modes = modes.child(
                div()
                    .id(mode.key())
                    .px_2()
                    .py_1()
                    .text_xs()
                    .cursor_pointer()
                    .bg(rgb(if selected {
                        theme::doc_selected_bg()
                    } else {
                        theme::bg()
                    }))
                    .text_color(rgb(if selected {
                        theme::default_fg()
                    } else {
                        theme::status_fg()
                    }))
                    .child(t(chat_mode_key(mode)))
                    .on_click(cx.listener(move |this, _, _, cx| this.chat_set_mode(mode, cx))),
            );
        }
        let commands = [
            "/plan",
            "/build",
            "/review",
            "/rewrite",
            "/continuity",
            "/character",
        ];
        let mut command_row = div().flex().flex_wrap().gap_1();
        for command in commands {
            command_row = command_row.child(
                div()
                    .id(command)
                    .px_2()
                    .py_1()
                    .text_xs()
                    .cursor_pointer()
                    .bg(rgb(theme::bg()))
                    .border_1()
                    .border_color(rgb(theme::explorer_border()))
                    .text_color(rgb(theme::gutter_fg()))
                    .child(command)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.chat.focused = true;
                        this.chat.set_input(format!("{command} "));
                        window.focus(&this.focus_handle(cx));
                        cx.notify();
                    })),
            );
        }
        let input = if self.chat.input.is_empty() {
            format!("{}▌", t("chat.placeholder"))
        } else {
            format!("{}▌", self.chat.input)
        };
        let send_label = if self.chat.status == ChatStatus::Running {
            t("chat.stop")
        } else {
            t("chat.send")
        };
        div()
            .border_t_1()
            .border_color(rgb(theme::explorer_border()))
            .p_2()
            .flex()
            .flex_col()
            // The composer keeps its height while the transcript scrolls.
            .flex_shrink_0()
            .gap_1()
            .child(attachments)
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        div()
                            .id("chat-attach-scene")
                            .px_1()
                            .py_1()
                            .text_xs()
                            .cursor_pointer()
                            .text_color(rgb(theme::status_fg()))
                            .child("@ Escena")
                            .on_click(cx.listener(|this, _, _, cx| this.chat_attach_scene(cx))),
                    )
                    .child(
                        div()
                            .id("chat-attach-selection")
                            .px_1()
                            .py_1()
                            .text_xs()
                            .cursor_pointer()
                            .text_color(rgb(theme::status_fg()))
                            .child("@ Selección")
                            .on_click(cx.listener(|this, _, _, cx| this.chat_attach_selection(cx))),
                    )
                    .child(
                        div()
                            .id("chat-attach-document")
                            .px_1()
                            .py_1()
                            .text_xs()
                            .cursor_pointer()
                            .text_color(rgb(theme::status_fg()))
                            .child("@ Documento")
                            .on_click(cx.listener(|this, _, _, cx| this.chat_attach_document(cx))),
                    ),
            )
            .child(
                div()
                    .id("chat-input")
                    .min_h(px(54.0))
                    .max_h(px(120.0))
                    .overflow_y_scroll()
                    .p_2()
                    .bg(rgb(theme::bg()))
                    .border_1()
                    .border_color(rgb(if self.chat.focused {
                        theme::default_fg()
                    } else {
                        theme::explorer_border()
                    }))
                    .text_color(rgb(if self.chat.input.is_empty() {
                        theme::gutter_fg()
                    } else {
                        theme::default_fg()
                    }))
                    .whitespace_normal()
                    .cursor_text()
                    .child(input)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.chat.focused = true;
                        window.focus(&this.focus_handle(cx));
                        cx.notify();
                    })),
            )
            .child(command_row)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(modes)
                    .child(
                        div()
                            .id("chat-model")
                            .flex_1()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_xs()
                            .cursor_pointer()
                            .text_color(rgb(theme::gutter_fg()))
                            .child(format!(
                                "{} · {}",
                                self.agent_settings.provider.key(),
                                self.agent_settings.model
                            ))
                            .on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.open_ai_settings(window, cx)
                                }),
                            ),
                    )
                    .child(
                        div()
                            .id("chat-ai-settings")
                            .px_1()
                            .cursor_pointer()
                            .text_color(rgb(theme::status_fg()))
                            .child(icon("sliders-horizontal", theme::status_fg(), 14.0))
                            .on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.open_ai_settings(window, cx)
                                }),
                            ),
                    )
                    .child(
                        div()
                            .id("chat-send")
                            .px_2()
                            .py_1()
                            .bg(rgb(if self.chat.status == ChatStatus::Running {
                                theme::agent_err_fg()
                            } else {
                                theme::status_bg()
                            }))
                            .text_color(rgb(theme::default_fg()))
                            .cursor_pointer()
                            .child(send_label)
                            .on_click(cx.listener(|this, _, window, cx| {
                                if this.chat.status == ChatStatus::Running {
                                    this.chat_stop(cx);
                                } else {
                                    this.chat_send(window, cx);
                                }
                            })),
                    ),
            )
    }

    /// Agent review panel (M6 slice 3): right-hand column with the run's
    /// status, the context sources the task actually read, and a scrolled
    /// window of the review text. The dump prints the full text, so headless
    /// verification never depends on what fits on screen. M6 slice 4 adds a
    /// proposal section (diff + apply/reject hints) rendered when a proposal
    /// is active; the panel shows the proposal instead of an idle review.
    /// M12 adds the Chat tab as the default surface.
    fn agent_panel(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        if self.right_tab == RightTab::Chat {
            return self.chat_panel(cx);
        }
        // Proposal section (M6 slice 4) takes the panel whenever a proposal
        // flow is active; the review content shows only when a review has
        // been run and no proposal is open. An explicitly opened empty panel
        // explains how to start a review instead of pretending to be a chat.
        let content = if self.propose.is_open() {
            self.proposal_panel().into_any_element()
        } else if self.agent.is_open() {
            self.review_panel().into_any_element()
        } else {
            div()
                .id("agent-panel-empty")
                .p_3()
                .flex()
                .flex_col()
                .gap_2()
                .text_sm()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_color(rgb(theme::default_fg()))
                        .child(icon("message-square", theme::cue_fg(), 14.0))
                        .child(self.t("panel.agent_title")),
                )
                .child(
                    div()
                        .text_color(rgb(theme::status_fg()))
                        .child(self.t("panel.agent_empty")),
                )
                .into_any_element()
        };
        div()
            .w(px(self.right_panel_w))
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(theme::explorer_bg()))
            .border_l_1()
            .border_color(rgb(theme::explorer_border()))
            .child(self.right_tab_bar(cx))
            .child(div().flex_1().overflow_hidden().child(content))
            .into_any_element()
    }

    /// The review column (M6 slice 3), unchanged semantics.
    fn review_panel(&self) -> impl IntoElement {
        let status_label = match self.agent.status {
            AgentStatus::Idle => self.t("status.idle"),
            AgentStatus::Running => self.t("status.running"),
            AgentStatus::Completed => self.t("status.completed"),
            AgentStatus::Failed => self.t("status.failed"),
            AgentStatus::Cancelled => self.t("status.cancelled"),
        };
        let mut panel = div()
            .id("agent-panel")
            .w(px(self.right_panel_w))
            .h_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(rgb(theme::explorer_bg()))
            .border_l_1()
            .border_color(rgb(theme::explorer_border()))
            .text_sm()
            .font_family(FONT_FAMILY)
            .child(
                div()
                    .px_2()
                    .py_1()
                    .text_color(rgb(theme::default_fg()))
                    .child(self.tf(
                        "panel.review_title",
                        &[
                            ("scene", &self.agent.scene_number.to_string()),
                            ("status", status_label),
                        ],
                    )),
            );
        // The reviewer focus (M6 slice 5): what was asked, always visible.
        if !self.agent.focus.is_empty() {
            panel = panel.child(
                div()
                    .px_2()
                    .text_color(rgb(theme::find_fg()))
                    .child(self.tf("panel.focus", &[("focus", &self.agent.focus)])),
            );
        }
        // The configured role (M6 slice 7): which persona reviewed and
        // where that configuration came from.
        if !self.agent.role.is_empty() {
            panel = panel.child(
                div()
                    .px_2()
                    .text_color(rgb(theme::find_fg()))
                    .child(self.tf(
                        "panel.role",
                        &[
                            ("role", &self.agent.role),
                            (
                                "source",
                                role_source_label(self.lang, &self.agent.role_source),
                            ),
                        ],
                    )),
            );
        }
        if !self.agent.stage.is_empty() {
            panel = panel.child(
                div()
                    .px_2()
                    .text_color(rgb(theme::status_fg()))
                    .child(self.agent.stage.clone()),
            );
        }
        panel = panel.child(
            div()
                .px_2()
                .text_color(rgb(theme::status_fg()))
                .child(self.tf(
                    "panel.sources",
                    &[("n", &self.agent.sources.len().to_string())],
                )),
        );
        for (ix, source) in self.agent.sources.iter().enumerate() {
            panel = panel.child(
                div()
                    .id(("agent-source", ix))
                    .px_2()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_color(rgb(theme::status_fg()))
                    .child(format!("{} · {}", source.kind, source.location)),
            );
        }
        for line in self
            .agent
            .output
            .lines()
            .skip(self.agent.scroll)
            .take(AGENT_VISIBLE_LINES)
        {
            // Empty lines keep their height so paragraph breaks survive.
            let text = if line.is_empty() { " " } else { line };
            panel = panel.child(
                div()
                    .px_2()
                    .text_color(rgb(theme::default_fg()))
                    .child(text.to_string()),
            );
        }
        if matches!(
            self.agent.status,
            AgentStatus::Failed | AgentStatus::Cancelled
        ) {
            panel = panel.child(
                div()
                    .px_2()
                    .text_color(rgb(theme::agent_err_fg()))
                    .child(self.agent.error.clone()),
            );
        }
        panel.child(div().px_2().text_color(rgb(theme::status_fg())).child(
            if matches!(
                self.agent.status,
                AgentStatus::Failed | AgentStatus::Cancelled
            ) {
                self.t("panel.review_hint_retry")
            } else {
                self.t("panel.review_hint")
            },
        ))
    }

    /// Proposal column (M6 slice 4): status, the persisted record (id,
    /// target, hunks), a scrolled window of the pre-rendered diff and the
    /// apply/reject hints. The diff shows the stored proposal — exactly
    /// what ⌘⇧Enter would write — never a recomputation.
    fn proposal_panel(&self) -> impl IntoElement {
        let status_label = self.propose_status_label();
        let mut panel = div()
            .id("proposal-panel")
            .w(px(self.right_panel_w))
            .h_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(rgb(theme::explorer_bg()))
            .border_l_1()
            .border_color(rgb(theme::explorer_border()))
            .text_sm()
            .font_family(FONT_FAMILY)
            .child(
                div()
                    .px_2()
                    .py_1()
                    .text_color(rgb(theme::default_fg()))
                    .child(self.tf(
                        "panel.proposal_title",
                        &[
                            ("scene", &self.propose.scene_number.to_string()),
                            ("status", status_label),
                        ],
                    )),
            );
        if !self.propose.id.is_empty() {
            panel = panel.child(
                div()
                    .px_2()
                    .text_color(rgb(theme::status_fg()))
                    .child(self.tf(
                        "panel.proposal_meta",
                        &[
                            ("id", &short_id(&self.propose.id)),
                            ("target", &self.propose.target),
                            ("n", &self.propose.hunks.to_string()),
                        ],
                    )),
            );
        }
        // The configured role (M6 slice 7): which persona proposed.
        if !self.propose.role.is_empty() {
            panel = panel.child(
                div()
                    .px_2()
                    .text_color(rgb(theme::find_fg()))
                    .child(self.tf(
                        "panel.role",
                        &[
                            ("role", &self.propose.role),
                            (
                                "source",
                                role_source_label(self.lang, &self.propose.role_source),
                            ),
                        ],
                    )),
            );
        }
        if !self.propose.stage.is_empty() {
            panel = panel.child(
                div()
                    .px_2()
                    .text_color(rgb(theme::status_fg()))
                    .child(self.propose.stage.clone()),
            );
        }
        // Diff rows with the hunk selection overlay (M6 slice 6): each '#'
        // header row carries the checkbox ([x]/[ ]) and the cursor (▸);
        // rows of a deselected hunk are dimmed — they won't be applied.
        let mut hunk_ix: Option<usize> = None;
        for line in self
            .propose
            .diff_rows
            .iter()
            .skip(self.propose.scroll)
            .take(AGENT_VISIBLE_LINES)
        {
            let (label, text) = line;
            let mut color = match label {
                '+' => theme::diff_add_label(),
                '-' => theme::agent_err_fg(),
                _ => theme::status_fg(),
            };
            let mut prefix = format!("{label}");
            if *label == '#' {
                let ix = hunk_ix.map_or(0, |prev| prev + 1);
                hunk_ix = Some(ix);
                let checked = self.propose.selected.get(ix).copied().unwrap_or(true);
                let cursor = if ix == self.propose.hunk_cursor {
                    "▸"
                } else {
                    " "
                };
                prefix = format!("{cursor}[{}] ", if checked { "x" } else { " " });
                // The cursor row is highlighted so ⌥←/⌥→/⌥Enter have a
                // visible target.
                color = if ix == self.propose.hunk_cursor {
                    theme::default_fg()
                } else {
                    theme::status_fg()
                };
            } else if !hunk_ix
                .and_then(|ix| self.propose.selected.get(ix).copied())
                .unwrap_or(true)
            {
                // Deselected hunk body: dimmed, it will not be applied.
                color = theme::gutter_fg();
            }
            panel = panel.child(
                div()
                    .px_2()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_color(rgb(color))
                    .child(format!("{prefix}{text}")),
            );
        }
        if matches!(
            self.propose.status,
            ProposeStatus::Failed | ProposeStatus::Cancelled
        ) {
            panel = panel.child(
                div()
                    .px_2()
                    .text_color(rgb(theme::agent_err_fg()))
                    .child(self.propose.error.clone()),
            );
        }
        let hint = if self.propose.armed_apply {
            self.t("panel.proposal_hint_armed")
        } else if matches!(
            self.propose.status,
            ProposeStatus::Failed | ProposeStatus::Cancelled
        ) {
            self.t("panel.proposal_hint_retry")
        } else {
            self.t("panel.proposal_hint")
        };
        panel.child(div().px_2().text_color(rgb(theme::status_fg())).child(hint))
    }

    /// Search results panel (M8 slice 2): replaces the explorer while a
    /// search report is on screen. Clicking a row opens the document at the
    /// match; ⌘↑/⌘↓ move the selection and Enter opens it.
    fn search_results_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let title = self.tf(
            "panel.search_title",
            &[
                ("n", &self.search.hits.len().to_string()),
                ("query", &self.search.searched),
            ],
        );
        let title = if self.search.truncated {
            format!("{title}{}", self.t("panel.search_truncated"))
        } else {
            title
        };
        let mut panel = div()
            .id("search-results")
            .w(px(self.explorer_w))
            .h_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .overflow_y_scroll()
            .bg(rgb(theme::explorer_bg()))
            .border_r_1()
            .border_color(rgb(theme::explorer_border()))
            .child(
                div()
                    .px_2()
                    .py_1()
                    .text_sm()
                    .text_color(rgb(theme::status_fg()))
                    .child(title),
            );
        for (index, hit) in self.search.hits.iter().enumerate() {
            let selected = index == self.search.selected;
            panel = panel.child(
                div()
                    .id(("hit", index))
                    .px_2()
                    .py_1()
                    .text_sm()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .when(selected, |row| row.bg(rgb(theme::doc_selected_bg())))
                    .text_color(rgb(if selected {
                        theme::default_fg()
                    } else {
                        theme::status_fg()
                    }))
                    .cursor_pointer()
                    .child(format!("{}:{} · {}", hit.document, hit.line, hit.preview))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select_search_hit(index, window, cx)
                    })),
            );
        }
        panel
    }

    /// Element kind of a line (M9 slice 5); `None` before the first scan.
    fn element_kind(&self, ix: usize) -> Option<FountainKind> {
        self.doc.element_kinds.get(ix).copied()
    }

    /// Screenplay element color (M9 slice 5), falling back to the line
    /// heuristics before the first classification pass.
    fn element_color(&self, ix: usize, text: &str) -> Rgba {
        match self.element_kind(ix) {
            Some(FountainKind::SceneHeading) => rgb(theme::scene_fg()),
            Some(FountainKind::Character) => rgb(theme::cue_fg()),
            Some(FountainKind::Parenthetical) => rgb(theme::status_fg()),
            Some(FountainKind::Dialogue) => rgb(theme::default_fg()),
            Some(FountainKind::Transition) => rgb(theme::transition_fg()),
            Some(FountainKind::Section) => rgb(theme::section_fg()),
            Some(FountainKind::Synopsis) => rgb(theme::gutter_fg()),
            Some(FountainKind::Note) => rgb(theme::note_fg()),
            Some(FountainKind::Boneyard) => rgb(theme::gutter_fg()),
            Some(FountainKind::Lyric)
            | Some(FountainKind::Action)
            | Some(FountainKind::Centered)
            | Some(FountainKind::TitlePage)
            | Some(FountainKind::PageBreak) => rgb(theme::default_fg()),
            None => line_color(text),
        }
    }

    /// Bottom status bar (M10 S3c): panel toggles on the left, the workspace
    /// status/notice in the middle, language and Git state on the right.
    fn bottom_bar(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let title = self
            .project
            .as_ref()
            .map(|project| project.manifest().title.clone())
            .unwrap_or_default();
        let status = self.status_text();
        let lang = language_name(&self.doc.path);
        let git = self.git.branch_label();
        let changes = self.git.changed_count();
        let git_tint = if self.git.available {
            if changes > 0 {
                theme::git_modified_fg()
            } else {
                theme::git_added_fg()
            }
        } else {
            theme::gutter_fg()
        };
        let git_tip = self.git_tooltip(
            cx,
            "git status",
            self.tf("git.tip.status_bar", &[("n", &changes.to_string())]),
        );
        div()
            .id("bottom-bar")
            .h(px(26.0))
            .w_full()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .bg(rgb(theme::status_bg()))
            .text_sm()
            .whitespace_nowrap()
            .text_color(rgb(theme::status_fg()))
            .child(
                div()
                    .id("bar-explorer")
                    .p_1()
                    .cursor_pointer()
                    .text_color(rgb(if self.explorer_open {
                        theme::default_fg()
                    } else {
                        theme::status_fg()
                    }))
                    .child(icon(
                        "panel-left",
                        if self.explorer_open {
                            theme::default_fg()
                        } else {
                            theme::status_fg()
                        },
                        15.0,
                    ))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.on_toggle_explorer(&ToggleExplorer, window, cx)
                    })),
            )
            .child(
                div()
                    .max_w(px(180.0))
                    .overflow_hidden()
                    .text_color(rgb(theme::default_fg()))
                    .child(title),
            )
            .child(div().flex_1().overflow_hidden().child(status))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_color(rgb(theme::cue_fg()))
                    .child(div().child(format!("{lang} · {}", self.t("bar.language_native")))),
            )
            .child(
                div()
                    .id("bar-git")
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_1()
                    .cursor_pointer()
                    .text_color(rgb(git_tint))
                    .child(icon("git-branch", git_tint, 14.0))
                    .when(!git.is_empty(), |segment| segment.child(div().child(git)))
                    .when(changes > 0, |segment| {
                        segment.child(div().child(format!("· {changes}")))
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.on_toggle_git_panel(&ToggleGitPanel, window, cx)
                    }))
                    .tooltip(move |_, _| git_tip.clone()),
            )
            .child(
                div()
                    .id("bar-agent")
                    .p_1()
                    .cursor_pointer()
                    .child(icon(
                        "message-square",
                        if self.agent_panel_open {
                            theme::cue_fg()
                        } else {
                            theme::status_fg()
                        },
                        15.0,
                    ))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.on_toggle_agent_panel(&ToggleAgentPanel, window, cx)
                    })),
            )
            .into_any_element()
    }

    /// Tab strip (M10 S3b): one tab per open document, active tab highlighted,
    /// dirty tabs dotted. Clicking a title activates it; the close button
    /// asks for confirmation when that tab (or the active buffer) is dirty.
    fn tab_bar(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let mut bar = div()
            .id("tab-bar")
            .h(px(30.0))
            .w_full()
            .flex()
            .overflow_x_scroll()
            .bg(rgb(theme::explorer_bg()))
            .border_b_1()
            .border_color(rgb(theme::explorer_border()));
        for index in 0..self.tabs.len() {
            let active = index == self.active_tab;
            let preview = self.tabs.get(index).is_some_and(|tab| tab.preview);
            let path = self.tab_path(index).to_string();
            let title = self.tab_title(index);
            let dirty = self.tab_is_dirty(index);
            let label = if dirty { format!("{title} •") } else { title };
            let kind = FileKind::from_path(&path);
            let git = self.git.file_status(&path);
            let icon_color = git.map(git_color).unwrap_or(if active {
                theme::default_fg()
            } else {
                theme::status_fg()
            });
            let mut title_row = div()
                .id(("tab-title", index))
                .flex_1()
                .flex()
                .items_center()
                .gap_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .cursor_pointer()
                .text_color(rgb(if active {
                    theme::default_fg()
                } else {
                    theme::status_fg()
                }))
                .child(icon(kind.icon_name(), icon_color, 14.0))
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .when(preview, |label| label.italic())
                        .child(label),
                );
            if let Some(status) = git {
                title_row = title_row.child(
                    div()
                        .text_color(rgb(git_color(status)))
                        .child(git_letter(status)),
                );
            }
            let tab = div()
                .id(("tab", index))
                .w(px(190.0))
                .h_full()
                .flex_none()
                .flex()
                .items_center()
                .gap_1()
                .px_2()
                .border_r_1()
                .border_color(rgb(theme::explorer_border()))
                .bg(rgb(if active {
                    theme::bg()
                } else {
                    theme::explorer_bg()
                }))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        this.open_tab_context_menu(index, event.position, window, cx);
                    }),
                )
                .child(title_row.on_click(cx.listener(move |this, _, window, cx| {
                    if this.activate_tab(index) {
                        window.focus(&this.focus_handle(cx));
                        cx.notify();
                    }
                })))
                .child(
                    div()
                        .id(("tab-close", index))
                        .px_1()
                        .cursor_pointer()
                        .text_color(rgb(if active {
                            theme::status_fg()
                        } else {
                            theme::gutter_fg()
                        }))
                        .child(icon(
                            "x",
                            if active {
                                theme::status_fg()
                            } else {
                                theme::gutter_fg()
                            },
                            12.0,
                        ))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.request_close_tab(index, window, cx)
                        })),
                );
            bar = bar.child(tab);
        }
        bar.into_any_element()
    }

    /// Human Git panel (M13): plain-language changes, one-step version save,
    /// recent history and command tooltips. No Git jargon is required to use
    /// the primary actions, but every tooltip names the underlying command.
    /// GitHub connection block inside the “Cambios” panel (M13): publish,
    /// upload, bring and pull-request actions in plain language. Every
    /// remote action opens a confirmation overlay first.
    fn github_panel_section(&self, cx: &mut Context<Self>) -> gpui::Div {
        let mut section = div()
            .flex()
            .flex_col()
            .border_t_1()
            .border_color(rgb(theme::explorer_border()));
        section = section.child(
            div()
                .px_2()
                .py_1()
                .flex()
                .items_center()
                .gap_1()
                .text_xs()
                .text_color(rgb(theme::gutter_fg()))
                .child(icon("github", theme::gutter_fg(), 13.0))
                .child(self.t("github.section")),
        );
        if self.git_remotes_loading {
            return section.child(
                div()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .text_color(rgb(theme::status_fg()))
                    .child(self.t("github.loading")),
            );
        }
        if !self.git_remotes_error.is_empty() {
            return section.child(
                div()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .text_color(rgb(theme::agent_err_fg()))
                    .child(self.git_remotes_error.clone()),
            );
        }
        if let Some((remote, owner, repo)) = self.github_remote_info() {
            let label = format!("{owner}/{repo}");
            let remote_tip = self.git_tooltip(
                cx,
                "git remote -v",
                self.tf(
                    "github.tip.remote",
                    &[("remote", &remote), ("repo", &label)],
                ),
            );
            section = section.child(
                div()
                    .id("github-remote")
                    .px_2()
                    .py_1()
                    .text_xs()
                    .text_color(rgb(theme::git_added_fg()))
                    .tooltip(move |_, _| remote_tip.clone())
                    .child(label),
            );
            let branch = if self.git.branch.is_empty() {
                "main".to_string()
            } else {
                self.git.branch.clone()
            };
            let push_tip = self.git_tooltip(
                cx,
                "git push",
                self.tf(
                    "github.tip.push",
                    &[("remote", &remote), ("branch", &branch)],
                ),
            );
            let pull_tip = self.git_tooltip(
                cx,
                "git pull --ff-only",
                self.tf(
                    "github.tip.pull",
                    &[("remote", &remote), ("branch", &branch)],
                ),
            );
            let pr_tip = self.git_tooltip(cx, "gh pr create", self.t("github.tip.pr").to_string());
            section = section.child(
                div()
                    .px_2()
                    .py_1()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .child(
                        div()
                            .id("github-push")
                            .px_2()
                            .py_1()
                            .text_xs()
                            .bg(rgb(theme::status_bg()))
                            .text_color(rgb(theme::default_fg()))
                            .cursor_pointer()
                            .tooltip(move |_, _| push_tip.clone())
                            .child(self.t("github.push"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.open_git_sync(GitSyncKind::Push, cx)
                            })),
                    )
                    .child(
                        div()
                            .id("github-pull")
                            .px_2()
                            .py_1()
                            .text_xs()
                            .bg(rgb(theme::status_bg()))
                            .text_color(rgb(theme::status_fg()))
                            .cursor_pointer()
                            .tooltip(move |_, _| pull_tip.clone())
                            .child(self.t("github.pull"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.open_git_sync(GitSyncKind::Pull, cx)
                            })),
                    )
                    .when(branch != "main", |row| {
                        row.child(
                            div()
                                .id("github-pr")
                                .px_2()
                                .py_1()
                                .text_xs()
                                .bg(rgb(theme::status_bg()))
                                .text_color(rgb(theme::status_fg()))
                                .cursor_pointer()
                                .tooltip(move |_, _| pr_tip.clone())
                                .child(self.t("github.pr"))
                                .on_click(cx.listener(|this, _, _, cx| this.open_github_pr(cx))),
                        )
                    }),
            );
            if self.git.ahead > 0 || self.git.behind > 0 {
                section = section.child(
                    div()
                        .px_2()
                        .py_1()
                        .text_xs()
                        .text_color(rgb(theme::git_modified_fg()))
                        .child(self.tf(
                            "github.pending",
                            &[
                                ("ahead", &self.git.ahead.to_string()),
                                ("behind", &self.git.behind.to_string()),
                            ],
                        )),
                );
            }
            return section;
        }
        if !self.git_remotes.is_empty() {
            return section.child(
                div()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .text_color(rgb(theme::status_fg()))
                    .child(self.t("github.remote_not_github")),
            );
        }
        let publish_tip = self.git_tooltip(
            cx,
            "POST /user/repos",
            self.t("github.tip.publish").to_string(),
        );
        section.child(
            div()
                .px_2()
                .py_1()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(theme::status_fg()))
                        .child(self.t("github.not_connected")),
                )
                .child(
                    div()
                        .id("github-publish")
                        .px_2()
                        .py_1()
                        .text_xs()
                        .bg(rgb(theme::status_bg()))
                        .text_color(rgb(theme::default_fg()))
                        .cursor_pointer()
                        .tooltip(move |_, _| publish_tip.clone())
                        .child(self.t("github.publish"))
                        .on_click(cx.listener(|this, _, _, cx| this.open_github_publish(cx))),
                ),
        )
    }

    fn git_panel(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let mut panel = div()
            .id("git-panel")
            .w(px(self.explorer_w))
            .h_full()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .bg(rgb(theme::explorer_bg()))
            .border_r_1()
            .border_color(rgb(theme::explorer_border()))
            .text_sm();
        let changes = self.git.changed_count();
        panel = panel.child(
            div()
                .px_2()
                .py_1()
                .flex()
                .items_center()
                .gap_1()
                .text_color(rgb(theme::default_fg()))
                .child(icon("git-branch", theme::default_fg(), 14.0))
                .child(div().flex_1().child(self.t("git.changes")))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(if changes > 0 {
                            theme::git_modified_fg()
                        } else {
                            theme::git_added_fg()
                        }))
                        .child(changes.to_string()),
                ),
        );
        if !self.git.available {
            let tip = self.git_tooltip(cx, "git init", self.t("git.tip.init").to_string());
            panel = panel
                .child(
                    div()
                        .px_2()
                        .py_1()
                        .text_color(rgb(theme::status_fg()))
                        .child(self.t("git.no_repo")),
                )
                .child(
                    div()
                        .px_2()
                        .py_1()
                        .text_xs()
                        .text_color(rgb(theme::gutter_fg()))
                        .child(self.t("git.no_repo_hint")),
                )
                .child(
                    div()
                        .id("git-init")
                        .mx_2()
                        .my_1()
                        .px_3()
                        .py_1()
                        .bg(rgb(theme::status_bg()))
                        .text_color(rgb(theme::default_fg()))
                        .cursor_pointer()
                        .tooltip(move |_, _| tip.clone())
                        .child(self.t("git.init"))
                        .on_click(cx.listener(|this, _, _, cx| this.init_git_repo(cx))),
                );
            if !self.git.error.is_empty() {
                panel = panel.child(
                    div()
                        .px_2()
                        .py_1()
                        .text_color(rgb(theme::agent_err_fg()))
                        .child(self.git.error.clone()),
                );
            }
            return panel.into_any_element();
        }
        let branch = if self.git.branch.is_empty() {
            self.t("git.detached").to_string()
        } else {
            self.git.branch.clone()
        };
        let branch_tip = self.git_tooltip(cx, "git branch", self.t("git.tip.branch").to_string());
        panel = panel.child(
            div()
                .id("git-branch-row")
                .px_2()
                .py_1()
                .flex()
                .items_center()
                .gap_1()
                .text_xs()
                .text_color(rgb(theme::status_fg()))
                .tooltip(move |_, _| branch_tip.clone())
                .child(format!("{}: {branch}", self.t("git.draft")))
                .when(self.git.ahead > 0, |row| {
                    row.child(format!(" · {} ↑", self.git.ahead))
                })
                .when(self.git.behind > 0, |row| {
                    row.child(format!(" · {} ↓", self.git.behind))
                }),
        );
        let summary = if changes == 0 {
            self.t("git.clean").to_string()
        } else {
            self.tf("git.summary", &[("n", &changes.to_string())])
        };
        panel = panel.child(
            div()
                .px_2()
                .py_1()
                .text_xs()
                .text_color(rgb(if changes > 0 {
                    theme::git_modified_fg()
                } else {
                    theme::git_added_fg()
                }))
                .child(summary),
        );
        let commit_tip = self.git_tooltip(cx, "git commit", self.t("git.tip.commit").to_string());
        let log_tip = self.git_tooltip(cx, "git log", self.t("git.tip.log").to_string());
        panel = panel.child(
            div()
                .px_2()
                .py_1()
                .flex()
                .items_center()
                .gap_1()
                .when(changes > 0, |row| {
                    row.child(
                        div()
                            .id("git-save-version-action")
                            .px_2()
                            .py_1()
                            .bg(rgb(theme::status_bg()))
                            .text_color(rgb(theme::default_fg()))
                            .cursor_pointer()
                            .tooltip(move |_, _| commit_tip.clone())
                            .child(self.t("git.save_version"))
                            .on_click(cx.listener(|this, _, _, cx| this.open_git_commit(cx))),
                    )
                })
                .child(
                    div()
                        .id("git-history-action")
                        .px_2()
                        .py_1()
                        .cursor_pointer()
                        .text_color(rgb(theme::status_fg()))
                        .tooltip(move |_, _| log_tip.clone())
                        .child(self.t("git.history"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.request_git_history(cx);
                        })),
                ),
        );
        if changes == 0 {
            panel = panel.child(
                div()
                    .px_2()
                    .py_2()
                    .text_color(rgb(theme::git_added_fg()))
                    .child(self.t("git.clean")),
            );
        } else {
            let mut paths: Vec<&String> = self.git.files.keys().collect();
            paths.sort();
            for (index, path) in paths.into_iter().enumerate() {
                let Some(status) = self.git.file_status(path) else {
                    continue;
                };
                let label = self.t(Self::git_status_key(status));
                let path_owned = path.clone();
                let tip = self.git_tooltip(
                    cx,
                    "git diff",
                    format!("{label}: {path} — {}", self.t("git.tip.diff")),
                );
                panel = panel.child(
                    div()
                        .id(("git-file", index))
                        .px_2()
                        .py_1()
                        .flex()
                        .items_center()
                        .gap_1()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .cursor_pointer()
                        .text_color(rgb(theme::status_fg()))
                        .tooltip(move |_, _| tip.clone())
                        .child(icon(
                            FileKind::from_path(path).icon_name(),
                            git_color(status),
                            14.0,
                        ))
                        .child(div().flex_1().overflow_hidden().child(path_owned.clone()))
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(git_color(status)))
                                .child(label),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_git_diff(path_owned.clone(), cx)
                        })),
                );
            }
        }
        panel = panel.child(
            div()
                .px_2()
                .py_1()
                .text_xs()
                .text_color(rgb(theme::gutter_fg()))
                .child(self.t("git.drafts")),
        );
        if self.git_branches_loading {
            panel = panel.child(
                div()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .text_color(rgb(theme::status_fg()))
                    .child(self.t("git.loading_drafts")),
            );
        } else if !self.git_branches_error.is_empty() {
            panel = panel.child(
                div()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .text_color(rgb(theme::agent_err_fg()))
                    .child(self.git_branches_error.clone()),
            );
        } else {
            for (index, branch) in self.git_branches.iter().take(8).enumerate() {
                let is_current = branch.current;
                let name = branch.name.clone();
                let tip = self.git_tooltip(
                    cx,
                    "git branch",
                    if is_current {
                        self.t("git.tip.branch_current").to_string()
                    } else {
                        format!("{}: {name}", self.t("git.tip.branch_switch"))
                    },
                );
                panel = panel.child(
                    div()
                        .id(("git-branch", index))
                        .px_2()
                        .py_1()
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_xs()
                        .cursor_pointer()
                        .text_color(rgb(if is_current {
                            theme::default_fg()
                        } else {
                            theme::status_fg()
                        }))
                        .tooltip(move |_, _| tip.clone())
                        .child(if is_current { "•" } else { "○" })
                        .child(name.clone())
                        .when(is_current, |row| {
                            row.child(
                                div()
                                    .text_color(rgb(theme::git_added_fg()))
                                    .child(self.t("git.current")),
                            )
                        })
                        .when(!is_current, |row| {
                            row.on_click(cx.listener(move |this, _, _, cx| {
                                this.switch_git_branch(name.clone(), cx)
                            }))
                        }),
                );
            }
            panel = panel.child(
                div()
                    .id("git-new-branch")
                    .px_2()
                    .py_1()
                    .text_xs()
                    .cursor_pointer()
                    .text_color(rgb(theme::status_fg()))
                    .child(self.t("git.new_draft"))
                    .on_click(cx.listener(|this, _, _, cx| this.open_branch_prompt(cx))),
            );
        }
        panel = panel.child(self.github_panel_section(cx));
        panel = panel.child(
            div()
                .px_2()
                .py_1()
                .text_xs()
                .text_color(rgb(theme::gutter_fg()))
                .child(self.t("git.recent")),
        );
        if self.git_history_loading {
            panel = panel.child(
                div()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .text_color(rgb(theme::status_fg()))
                    .child(self.t("git.loading_history")),
            );
        } else if !self.git_history_error.is_empty() {
            panel = panel.child(
                div()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .text_color(rgb(theme::agent_err_fg()))
                    .child(self.git_history_error.clone()),
            );
        } else if self.git_history.is_empty() {
            panel = panel.child(
                div()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .text_color(rgb(theme::status_fg()))
                    .child(self.t("git.no_history")),
            );
        } else {
            for (index, commit) in self.git_history.iter().take(8).enumerate() {
                let tip = self.git_tooltip(
                    cx,
                    "git log",
                    format!(
                        "{} · {}",
                        short_id(&commit.id),
                        self.t("git.tip.log_detail")
                    ),
                );
                panel = panel.child(
                    div()
                        .id(("git-commit", index))
                        .px_2()
                        .py_1()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .text_xs()
                        .tooltip(move |_, _| tip.clone())
                        .child(
                            div()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_color(rgb(theme::default_fg()))
                                .child(commit.subject.clone()),
                        )
                        .child(div().text_color(rgb(theme::gutter_fg())).child(format!(
                            "{} · {}",
                            short_id(&commit.id),
                            commit.date.get(..10).unwrap_or(&commit.date)
                        ))),
                );
            }
        }
        panel.into_any_element()
    }

    /// Explorer panel (M6 slice 1): one row per project document. Clicking
    /// loads it (`select_path`); the active document is highlighted to mirror
    /// `doc_path`. Long paths clip at the panel edge (status bar shows the
    /// full path). While a search report is on screen the results replace
    /// it (M8 slice 2).
    fn explorer_panel(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        if !self.search.hits.is_empty() {
            return self.search_results_panel(cx).into_any_element();
        }
        if self.git_panel_open {
            return self.git_panel(cx);
        }
        let title = self
            .project
            .as_ref()
            .map(|project| project.manifest().title.clone())
            .unwrap_or_default();
        let mut panel = div()
            .id("explorer")
            .w(px(self.explorer_w))
            .h_full()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .bg(rgb(theme::explorer_bg()))
            .border_r_1()
            .border_color(rgb(theme::explorer_border()))
            .child(
                div()
                    .px_2()
                    .py_1()
                    .text_sm()
                    .text_color(rgb(theme::status_fg()))
                    .flex()
                    .items_center()
                    .child(div().flex_1().child(title))
                    .child(
                        div()
                            .id("project-settings")
                            .px_1()
                            .text_color(rgb(theme::status_fg()))
                            .cursor_pointer()
                            .child(icon("sliders-horizontal", theme::status_fg(), 14.0))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.on_open_project_settings(&OpenProjectSettings, window, cx)
                            })),
                    )
                    .child(
                        div()
                            .id("new-file")
                            .px_1()
                            .text_color(rgb(theme::default_fg()))
                            .cursor_pointer()
                            .child("+")
                            .on_click(cx.listener(|this, _, _, cx| this.open_new_file(cx))),
                    ),
            );
        // Tree rows (M9 slice 4 / S3c): type is an icon, color is state.
        // Dirty is an accent dot; Git status tints the icon and adds a letter
        // so the information does not depend on color alone.
        let rows: Vec<Row> = explorer::build_rows(
            &explorer::visible_paths(&self.explorer, self.show_config_files),
            &self.collapsed,
        );
        for (index, row) in rows.iter().enumerate() {
            let selected = !row.is_dir && row.path == self.doc.path;
            let dirty = !row.is_dir
                && self
                    .find_tab(&row.path)
                    .is_some_and(|tab| self.tab_is_dirty(tab));
            let git = if row.is_dir {
                self.git.dir_status(&row.path)
            } else {
                self.git.file_status(&row.path)
            };
            let base_color = if selected {
                theme::default_fg()
            } else {
                theme::status_fg()
            };
            let icon_color = git.map(git_color).unwrap_or(base_color);
            let icon_name = if row.is_dir {
                row.folder_kind.icon_name()
            } else {
                row.kind.icon_name()
            };
            let label = if row.is_dir {
                format!("{} {}", if row.collapsed { "▸" } else { "▾" }, row.name)
            } else {
                row.name.clone()
            };
            let mut content = div()
                .flex()
                .items_center()
                .gap_1()
                .child(icon(icon_name, icon_color, 14.0))
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_color(rgb(base_color))
                        .child(label),
                );
            if dirty {
                content = content.child(div().text_color(rgb(theme::dirty_fg())).child("•"));
            }
            if let Some(status) = git {
                content = content.child(
                    div()
                        .text_color(rgb(git_color(status)))
                        .child(git_letter(status)),
                );
            }
            let path = row.path.clone();
            let is_dir = row.is_dir;
            let right_path = path.clone();
            panel = panel.child(
                div()
                    .id(("tree", index))
                    .pl(px(8.0 + row.depth as f32 * 12.0))
                    .pr_2()
                    .py_1()
                    .text_sm()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .when(selected, |row| row.bg(rgb(theme::doc_selected_bg())))
                    .cursor_pointer()
                    .child(content)
                    .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                        if is_dir {
                            this.toggle_folder(&path, cx);
                        } else {
                            // Single click previews; double click pins (D-041).
                            this.select_path(&path, event.click_count() >= 2, window, cx);
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            this.open_explorer_context_menu(
                                right_path.clone(),
                                is_dir,
                                event.position,
                                window,
                                cx,
                            );
                        }),
                    ),
            );
        }
        // Document outline (M8 slice 3; M14b for Markdown/YAML). Fountain
        // keeps its scene list, Markdown shows headings and YAML top-level
        // keys. Clicking a row jumps to that source line; the caret's
        // section is highlighted.
        if !self.doc.outline.is_empty() {
            let caret_line = self.doc.buffer.position_at(self.caret()).line;
            let current_outline = scene_number_at(&self.doc.outline_lines, caret_line);
            let title_key = match self.doc.language.language {
                LanguageId::Markdown => "panel.outline_title_sections",
                LanguageId::Yaml => "panel.outline_title_keys",
                _ => "panel.outline_title",
            };
            panel = panel.child(
                div()
                    .px_2()
                    .py_1()
                    .text_sm()
                    .text_color(rgb(theme::status_fg()))
                    .child(self.tf(title_key, &[("n", &self.doc.outline.len().to_string())])),
            );
            let visible = OUTLINE_VISIBLE_ROWS.min(self.doc.outline.len());
            for (index, (_, heading)) in self.doc.outline.iter().take(visible).enumerate() {
                let selected = current_outline == Some(index + 1);
                let level = self.doc.outline_levels.get(index).copied().unwrap_or(1);
                let indent = (level.saturating_sub(1) as f32) * 10.0;
                panel = panel.child(
                    div()
                        .id(("outline", index))
                        .ml(px(indent))
                        .px_2()
                        .py_1()
                        .text_sm()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .when(selected, |row| row.bg(rgb(theme::doc_selected_bg())))
                        .text_color(rgb(if selected {
                            theme::default_fg()
                        } else {
                            theme::status_fg()
                        }))
                        .cursor_pointer()
                        .child(format!("{} · {}", index + 1, heading))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.select_outline(index, window, cx)
                        })),
                );
            }
            if self.doc.outline.len() > visible {
                panel = panel.child(
                    div()
                        .px_2()
                        .py_1()
                        .text_sm()
                        .text_color(rgb(theme::status_fg()))
                        .child(self.tf(
                            "panel.outline_more",
                            &[("n", &(self.doc.outline.len() - visible).to_string())],
                        )),
                );
            }
        }
        if !self.doc.language.diagnostics.is_empty() {
            let total = self.doc.language.diagnostics.len();
            panel = panel.child(
                div()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .text_color(rgb(theme::git_modified_fg()))
                    .child(self.tf("panel.diagnostics_title", &[("n", &total.to_string())])),
            );
            for (index, diagnostic) in self.doc.language.diagnostics.iter().take(4).enumerate() {
                let color = match diagnostic.severity {
                    DiagnosticSeverity::Error => theme::agent_err_fg(),
                    DiagnosticSeverity::Warning => theme::git_modified_fg(),
                    DiagnosticSeverity::Info => theme::status_fg(),
                };
                panel = panel.child(
                    div()
                        .id(("diagnostic", index))
                        .px_2()
                        .py_1()
                        .text_xs()
                        .text_color(rgb(color))
                        .child(format!(
                            "⚠ L{} · {}",
                            diagnostic.line + 1,
                            self.diagnostic_text(diagnostic)
                        )),
                );
            }
            if total > 4 {
                panel = panel.child(
                    div()
                        .px_2()
                        .py_1()
                        .text_xs()
                        .text_color(rgb(theme::status_fg()))
                        .child(
                            self.tf("panel.diagnostics_more", &[("n", &(total - 4).to_string())]),
                        ),
                );
            }
        }
        panel.into_any_element()
    }

    fn status_text(&self) -> String {
        let caret = self.doc.buffer.position_at(self.caret());
        // M6 slice 1: show the document (not just the run's fixture path) with
        // a dirty marker, plus the last outcome (save/open/blocked switch).
        let document = format!("{}{}", self.doc.path, if self.dirty() { " •" } else { "" });
        let notice = self
            .notice
            .as_ref()
            .map(|text| format!(" · {text}"))
            .unwrap_or_default();
        // M6 slice 3: the review's lifecycle at a glance next to the rest of
        // the workspace state.
        let agent = match self.agent.status {
            AgentStatus::Idle => String::new(),
            AgentStatus::Running => self.tf(
                "bar.review_running",
                &[("scene", &self.agent.scene_number.to_string())],
            ),
            AgentStatus::Completed => self.tf(
                "bar.review_done",
                &[("scene", &self.agent.scene_number.to_string())],
            ),
            AgentStatus::Failed => self.t("bar.review_failed").to_string(),
            AgentStatus::Cancelled => self.t("bar.review_cancelled").to_string(),
        };
        // M6 slice 4: the proposal's lifecycle at a glance too.
        let propose = match self.propose.status {
            ProposeStatus::Idle => String::new(),
            ProposeStatus::Running => self.tf(
                "bar.propose_running",
                &[("scene", &self.propose.scene_number.to_string())],
            ),
            ProposeStatus::Ready => format!(
                "{}{}",
                self.tf("bar.propose_ready", &[("id", &short_id(&self.propose.id))]),
                if self.propose.armed_apply {
                    self.t("bar.armed")
                } else {
                    ""
                }
            ),
            ProposeStatus::Applied => self.t("bar.propose_applied").to_string(),
            ProposeStatus::Rejected => self.t("bar.propose_rejected").to_string(),
            ProposeStatus::Failed => self.t("bar.propose_failed").to_string(),
            ProposeStatus::Cancelled => self.t("bar.propose_cancelled").to_string(),
        };
        // M9 slice 7: document · line:col · scenes · ~pages, then the
        // agent/proposal lifecycle and the last notice. The development
        // metrics moved behind --debug-metrics (the report/dump keep them).
        let pages = self
            .doc
            .page_count
            .map(|pages| pages.to_string())
            .unwrap_or_else(|| "…".to_string());
        let mut status = self.tf(
            "bar.status",
            &[
                ("doc", &document),
                ("line", &(caret.line + 1).to_string()),
                ("col", &caret.col.to_string()),
                ("scenes", &self.doc.scene_lines.len().to_string()),
                ("pages", &pages),
            ],
        );
        status.push_str(&agent);
        status.push_str(&propose);
        status.push_str(&notice);
        if self.debug_metrics {
            let metrics = self.metrics.borrow();
            let (_, p50, p95, max) = Metrics::stats(&metrics.frame_gaps_ms);
            let debug = self.tf(
                "bar.metrics",
                &[
                    ("edits", &metrics.edits.to_string()),
                    ("keys", &metrics.keystroke_to_frame_ms.len().to_string()),
                    ("lines", &metrics.lines.to_string()),
                    ("frames", &metrics.renders.to_string()),
                    ("p50", &format!("{p50:.2}")),
                    ("p95", &format!("{p95:.2}")),
                    ("max", &format!("{max:.2}")),
                    ("probe", &self.probe_left.to_string()),
                ],
            );
            status = format!("{status} · {debug}");
        }
        status
    }

    /// The find/replace bar (slice 3). Rendered above the document when
    /// open; it shows the two fields, which one owns keystrokes (a trailing
    /// caret marker), the match counter and the key hints. The fields are
    /// display-only: keystrokes are routed here by the input handler
    /// (single-focus spike compromise, see the module header).
    fn find_bar(&self) -> gpui::Div {
        let total = self.doc.find.matches.len();
        let counter = if self.doc.find.query.is_empty() {
            String::new()
        } else if total == 0 {
            "0/0".to_string()
        } else {
            format!("{}/{}", self.doc.find.current + 1, total)
        };
        let marker = |active: bool| if active { "▌" } else { "" };
        let field = |label: &str, value: String, active: bool| {
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .text_color(rgb(theme::find_dim_fg()))
                        .child(label.to_string()),
                )
                .child(
                    div()
                        .text_color(rgb(if active {
                            theme::find_fg()
                        } else {
                            theme::find_dim_fg()
                        }))
                        .child(format!("{value}{}", marker(active))),
                )
        };
        div()
            .w_full()
            .flex()
            .items_center()
            .gap_3()
            .px_2()
            .h(px(STATUS_BAR_H))
            .bg(rgb(theme::find_bg()))
            .text_sm()
            .font_family(FONT_FAMILY)
            .whitespace_nowrap()
            .child(field(
                self.t("find.query_label"),
                self.doc.find.query.clone(),
                self.doc.find.field == FindField::Query,
            ))
            .child(field(
                self.t("find.replace_label"),
                self.doc.find.replacement.clone(),
                self.doc.find.field == FindField::Replacement,
            ))
            .child(div().text_color(rgb(theme::find_fg())).child(counter))
            .child(
                div()
                    .text_color(rgb(theme::find_dim_fg()))
                    .child(self.t("find.hint")),
            )
    }

    /// Instruction bar (M6 slice 4): one field, the same single-focus
    /// compromise as the find bar. Rendered above the document while open;
    /// keystrokes are routed here by the input handler, enter submits and
    /// escape closes the bar (never the task — cancel is ⌘.).
    fn propose_bar(&self) -> gpui::Div {
        div()
            .w_full()
            .flex()
            .items_center()
            .gap_3()
            .px_2()
            .h(px(STATUS_BAR_H))
            .bg(rgb(theme::find_bg()))
            .text_sm()
            .font_family(FONT_FAMILY)
            .whitespace_nowrap()
            .child(
                div()
                    .text_color(rgb(theme::find_dim_fg()))
                    .child(self.t("proposebar.label")),
            )
            .child(
                div()
                    .text_color(rgb(theme::find_fg()))
                    .child(format!("{}▌", self.propose_bar.instruction)),
            )
            .child(
                div()
                    .text_color(rgb(theme::find_dim_fg()))
                    .child(self.t("proposebar.hint")),
            )
    }

    /// Project-search bar (M8 slice 2): one field, same single-focus
    /// compromise. Enter searches (again to open the selected result);
    /// results render in the left panel.
    fn search_bar(&self) -> gpui::Div {
        div()
            .w_full()
            .flex()
            .items_center()
            .gap_3()
            .px_2()
            .h(px(STATUS_BAR_H))
            .bg(rgb(theme::find_bg()))
            .text_sm()
            .font_family(FONT_FAMILY)
            .whitespace_nowrap()
            .child(
                div()
                    .text_color(rgb(theme::find_dim_fg()))
                    .child(self.t("searchbar.label")),
            )
            .child(
                div()
                    .text_color(rgb(theme::find_fg()))
                    .child(format!("{}▌", self.search.query)),
            )
            .child(
                div()
                    .text_color(rgb(theme::find_dim_fg()))
                    .child(self.t("searchbar.hint")),
            )
    }

    /// Reviewer-focus bar (M6 slice 5): one field, same single-focus
    /// compromise. Enter commits the focus and runs the review; escape
    /// discards the typed text (the last committed focus stays in effect).
    fn focus_bar(&self) -> gpui::Div {
        let hint = if self.focus_bar.focus.is_empty() {
            self.t("focusbar.hint_general")
        } else {
            self.t("focusbar.hint_focus")
        };
        div()
            .w_full()
            .flex()
            .items_center()
            .gap_3()
            .px_2()
            .h(px(STATUS_BAR_H))
            .bg(rgb(theme::find_bg()))
            .text_sm()
            .font_family(FONT_FAMILY)
            .whitespace_nowrap()
            .child(
                div()
                    .text_color(rgb(theme::find_dim_fg()))
                    .child(self.t("focusbar.label")),
            )
            .child(
                div()
                    .text_color(rgb(theme::find_fg()))
                    .child(format!("{}▌", self.focus_bar.focus)),
            )
            .child(div().text_color(rgb(theme::find_dim_fg())).child(hint))
    }

    /// Render a line that has only a caret (no selection, no IME, no matches)
    /// with soft-wrapping, placing the caret on the correct visual row.
    /// Returns a flex column where each visual row is a flex child; the row
    /// containing the caret gets a cursor element at the correct x offset.
    fn render_caret_only_wrapped_row(
        &self,
        window: &Window,
        base: gpui::Div,
        ix: usize,
        text: &str,
        doc: Range<usize>,
        caret: usize,
    ) -> gpui::Div {
        let active = self.doc.buffer.position_at(caret).line == ix;
        let caret_col = (caret - doc.start).min(text.len());
        let wrap_w = f32::from(self.wrap_width);
        // The visual row comes from the pure geometry; the caret's x within
        // the row is implied by the flow width of the text before it
        // (Menlo is fixed-pitch), so the cursor must NOT also get a
        // margin-left: that double-counted the offset and opened a gap
        // between the caret and the rest of the line.
        let (visual_row, _caret_x) = caret_visual_row_and_x(window, text, caret_col, wrap_w);
        let starts = visual_row_starts(window, text, wrap_w);
        let mentions = self.entities.mentions(text);

        // Build visual rows as a column inside the gutter row.
        let mut col = div().flex().flex_col().w(self.wrap_width);
        let color = self.element_color(ix, text);
        let italic = self.element_kind(ix) == Some(FountainKind::Lyric);
        for (row_idx, &start) in starts.iter().enumerate() {
            let end = starts.get(row_idx + 1).copied().unwrap_or(text.len());
            let segment_text = &text[start..end];
            let mut row_div = div()
                .font_family(FONT_FAMILY)
                .text_size(px(theme::text_size()))
                .text_color(color)
                .when(italic, |row| row.italic())
                .h(px(theme::line_height()))
                .flex()
                .items_center();

            if row_idx == visual_row {
                let caret_local = caret_col.clamp(start, end);
                row_div = row_div.child(self.mention_segments(
                    text,
                    start,
                    end,
                    Some(caret_local),
                    &mentions,
                ));
            } else if mentions.is_empty() {
                row_div = row_div.child(segment_text.to_string());
            } else {
                row_div = row_div.child(self.mention_segments(text, start, end, None, &mentions));
            }
            col = col.child(row_div);
        }
        base.flex().child(gutter_element(ix, active)).child(col)
    }

    /// Render one visual row as text segments, optionally with the caret and
    /// entity mention backgrounds. Used by the wrapped caret row so mentions
    /// stay visible on the line that owns the caret.
    fn mention_segments(
        &self,
        text: &str,
        start: usize,
        end: usize,
        caret: Option<usize>,
        mentions: &[Mention],
    ) -> gpui::Div {
        let mut cuts = vec![start, end];
        if let Some(caret) = caret {
            cuts.push(caret.clamp(start, end));
        }
        for mention in mentions {
            if mention.end > start && mention.start < end {
                cuts.push(mention.start.clamp(start, end));
                cuts.push(mention.end.clamp(start, end));
            }
        }
        cuts.sort_unstable();
        cuts.dedup();
        let mut row = div().flex().items_center();
        for pair in cuts.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if caret == Some(a) {
                row = row.child(cursor_element());
            }
            if a == b {
                continue;
            }
            let mut segment = div().whitespace_nowrap();
            if let Some(mention) = mentions
                .iter()
                .find(|mention| a >= mention.start && b <= mention.end)
            {
                segment = segment.bg(rgba(mention_bg(
                    self.entities.entities()[mention.entity].category,
                )));
            }
            row = row.child(segment.child(text[a..b].to_string()));
        }
        if caret == Some(end) {
            row = row.child(cursor_element());
        }
        row
    }

    /// One row of the virtualized list.
    ///
    /// Non-hot rows (no caret/selection/IME) wrap at `wrap_width` and take
    /// whatever height the wrap needs; `list` measures each, so virtualization
    /// is preserved with variable heights. Rows carrying caret/selection/IME
    /// state keep the slice-2 single-line representation (fixed height, no
    /// wrap) so the caret stays exact — the documented spike compromise for
    /// wrapped editing.
    fn row(&self, ix: usize, window: &Window) -> gpui::Div {
        let text = &self.doc.buffer.lines()[ix];

        let doc = self.doc.buffer.line_content_range(ix);
        let caret = self.caret();
        let caret_line = self.doc.buffer.position_at(caret).line;
        let selection = if self.doc.selected.is_empty() {
            None
        } else {
            Some(self.doc.selected.clone())
        };
        let marked = self.doc.marked.clone().filter(|range| !range.is_empty());

        let intersects = |range: &Range<usize>| range.end > doc.start && range.start < doc.end;
        let selection_here = selection.as_ref().filter(|range| intersects(range));
        let marked_here = marked.as_ref().filter(|range| intersects(range));
        let caret_here = caret >= doc.start && caret <= doc.end;

        // Find matches intersecting this line. `matches` is sorted and
        // non-overlapping, so a binary search finds the first candidate and
        // the scan stops at the first match past the line (bounded work per
        // visible row even with hundreds of matches).
        let matches_here: Vec<(Range<usize>, bool)> =
            if self.doc.find.open && !self.doc.find.matches.is_empty() {
                let first = self
                    .doc
                    .find
                    .matches
                    .partition_point(|m| m.end <= doc.start);
                self.doc.find.matches[first..]
                    .iter()
                    .enumerate()
                    .take_while(|(_, m)| m.start < doc.end)
                    .map(|(offset, m)| (m.clone(), first + offset == self.doc.find.current))
                    .collect()
            } else {
                Vec::new()
            };

        let mentions_here = self.entities.mentions(text);

        let base = div()
            .px_2()
            .font_family(FONT_FAMILY)
            .text_size(px(theme::text_size()))
            // Keep the text line box equal to `theme::line_height()`. GPUI's
            // default is 1.618× the font size (22.5 px at 14 pt), so cold
            // rows measured taller than the 18 px hot/caret rows and selecting
            // a line shifted its glyphs up when the row became fixed-height.
            .line_height(px(theme::line_height()))
            .text_color(self.element_color(ix, text))
            .when(self.element_kind(ix) == Some(FountainKind::Lyric), |row| {
                row.italic()
            })
            .when(ix == caret_line, |row| row.bg(rgb(theme::active_line_bg())));

        // Cold row: no caret/selection/IME/matches -> gutter + wrapped text.
        // With entity mentions the text is rendered as styled runs so each
        // category gets its background tint without losing soft wrapping.
        if selection_here.is_none()
            && marked_here.is_none()
            && !caret_here
            && matches_here.is_empty()
        {
            let body = if !mentions_here.is_empty() {
                let color = self.element_color(ix, text);
                div().w(self.wrap_width).child(
                    StyledText::new(text.clone()).with_runs(mention_runs(
                        text,
                        &mentions_here,
                        &self.entities,
                        color,
                    )),
                )
            } else {
                div().w(self.wrap_width).child(text.clone())
            };
            return base
                .flex()
                .child(gutter_element(ix, ix == caret_line))
                .child(body);
        }

        // Caret-only (no selection, no IME, no matches): wrap the line and
        // place the caret on the correct visual row using a flex column of
        // visual rows. This is the B-3 wrapped-caret placement.
        if selection_here.is_none()
            && marked_here.is_none()
            && matches_here.is_empty()
            && caret_here
        {
            return self.render_caret_only_wrapped_row(window, base, ix, text, doc, caret);
        }

        let mut row = base
            .h(px(theme::line_height()))
            .flex()
            .items_center()
            .whitespace_nowrap()
            .child(gutter_element(ix, ix == caret_line));

        // Clamp every document range to this line before turning it into
        // line-local columns. A selection that starts on an earlier line (or
        // a match that spans lines) must never underflow the unsigned
        // subtraction while the row renders.
        let selection_local = selection_here.and_then(|range| line_local_range(range, &doc));
        let marked_local = marked_here.and_then(|range| line_local_range(range, &doc));
        let matches_local: Vec<(Range<usize>, bool)> = matches_here
            .iter()
            .filter_map(|(range, current)| {
                line_local_range(range, &doc).map(|local| (local, *current))
            })
            .collect();

        let len = text.len();
        let mut cuts: Vec<usize> = vec![0, len];
        if let Some(range) = &selection_local {
            cuts.push(range.start);
            cuts.push(range.end);
        }
        if let Some(range) = &marked_local {
            cuts.push(range.start);
            cuts.push(range.end);
        }
        for (range, _) in &matches_local {
            cuts.push(range.start);
            cuts.push(range.end);
        }
        for mention in &mentions_here {
            cuts.push(mention.start.min(len));
            cuts.push(mention.end.min(len));
        }
        // Only read the caret when it is on this line; a selection that
        // extends upward puts the caret before `doc.start`.
        let caret_col = if caret_here {
            (caret - doc.start).min(len)
        } else {
            0
        };
        if caret_here {
            cuts.push(caret_col);
        }
        cuts.sort_unstable();
        cuts.dedup();

        for pair in cuts.windows(2) {
            let (start, end) = (pair[0], pair[1]);
            if caret_here && start == caret_col {
                row = row.child(cursor_element());
            }
            if start == end {
                continue;
            }
            let selected = selection_local
                .as_ref()
                .is_some_and(|range| start >= range.start && end <= range.end);
            let composing = marked_local
                .as_ref()
                .is_some_and(|range| start >= range.start && end <= range.end);
            let match_state = matches_local
                .iter()
                .find(|(range, _)| start >= range.start && end <= range.end);
            let mention_state = mentions_here
                .iter()
                .find(|mention| start >= mention.start && end <= mention.end);
            let mut segment = div().whitespace_nowrap();
            if selected {
                segment = segment.bg(rgba(theme::selection_bg()));
            } else if let Some((_, is_current)) = match_state {
                // Selection wins over highlight; the current match is louder
                // than the rest.
                segment = segment.bg(rgba(if *is_current {
                    theme::match_current_bg()
                } else {
                    theme::match_bg()
                }));
            } else if let Some(mention) = mention_state {
                segment = segment.bg(rgba(mention_bg(
                    self.entities.entities()[mention.entity].category,
                )));
            }
            if composing {
                segment = segment.underline();
            }
            row = row.child(segment.child(text[start..end].to_string()));
        }
        if caret_here && caret_col == len {
            row = row.child(cursor_element());
        }
        row
    }

    /// JSON snapshot used by the scripted typing verification
    /// (`--dump-after-ms`): objective evidence of what the buffer contains
    /// after a driven sequence of keystrokes.
    fn dump_json(&self) -> String {
        let caret = self.caret();
        let position = self.doc.buffer.position_at(caret);
        let len = self.doc.buffer.doc_len();

        let mut start = caret.saturating_sub(64);
        while start < caret && !self.is_boundary(start) {
            start += 1;
        }
        let mut end = (caret + 64).min(len);
        while end > caret && !self.is_boundary(end) {
            end -= 1;
        }
        let around = escape_json(&self.doc.buffer.slice(start..end));
        // The selected text travels in the dump so drag/double/triple-click
        // verification compares the actual selection (capped to stay small).
        let selected_text = {
            let text = self.doc.buffer.slice(self.doc.selected.clone());
            let mut capped: String = text.chars().take(200).collect();
            if text.chars().count() > 200 {
                capped.push('…');
            }
            escape_json(&capped)
        };

        // The diff counts are the ground truth the M4 proposal surface
        // leans on: every row must be a real change (no phantom EOF line)
        // and `buffer_hash` lets an external check compare the rendered
        // buffer against an independently computed expectation.
        let diff_lines = self.doc.diff.rows.len();
        let diff_added = self
            .doc
            .diff
            .rows
            .iter()
            .filter(|row| row.label == '+')
            .count();
        let diff_removed = self
            .doc
            .diff
            .rows
            .iter()
            .filter(|row| row.label == '-')
            .count();
        let buffer_hash = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            for line in self.doc.buffer.lines() {
                line.hash(&mut hasher);
            }
            format!("{:016x}", hasher.finish())
        };

        // Composition state (Spike B): `marked` is the live setMarkedText
        // range in doc byte offsets (null when no composition is open) and
        // `composing` is the undo-group session flag. Together they let an
        // external check observe a composition mid-flight and confirm the
        // session closed after commit.
        let marked_dump = match &self.doc.marked {
            Some(range) => format!("[{},{}]", range.start, range.end),
            None => "null".to_string(),
        };

        // Click debug (Spike B): the last left-click's window coordinates,
        // resolved line, item bounds, and doc offset. `null` when no click
        // has been recorded or the click was ignored.
        let (click_x, click_y, click_line, click_top, click_h, click_offset) = self
            .doc
            .last_click
            .unwrap_or((0.0, 0.0, None, None, 0.0, None));
        let click_line_dump = click_line
            .map(|l| l.to_string())
            .unwrap_or("null".to_string());
        let click_top_dump = click_top
            .map(|t| format!("{:.1}", t))
            .unwrap_or("null".to_string());
        let click_h_dump = format!("{:.1}", click_h);
        let click_offset_dump = click_offset
            .map(|o| o.to_string())
            .unwrap_or("null".to_string());

        // M6 slice 1: which document the buffer represents, whether it
        // diverged from the last load/save, the last user-visible outcome
        // (save/open/blocked switch) and the explorer's document list — all
        // observable in the dump without trusting a screenshot.
        let notice_dump = match &self.notice {
            Some(text) => format!("\"{}\"", escape_json(text)),
            None => "null".to_string(),
        };
        let explorer_dump = self
            .explorer
            .iter()
            .map(|path| format!("\"{}\"", escape_json(path)))
            .collect::<Vec<_>>()
            .join(",");

        // M6 slice 3: the review panel's full state — status, scene, sources
        // audited, the *entire* review text, summary, error and timing — so
        // headless verification never depends on what a screenshot shows.
        let agent_scene_dump = if self.agent.scene_number > 0 {
            self.agent.scene_number.to_string()
        } else {
            "null".to_string()
        };
        let agent_elapsed_dump = self
            .agent
            .elapsed_ms()
            .map(|ms| ms.to_string())
            .unwrap_or_else(|| "null".to_string());
        let agent_sources_dump = self
            .agent
            .sources
            .iter()
            .map(|source| {
                format!(
                    "{{\"kind\":\"{}\",\"location\":\"{}\",\"detail\":\"{}\"}}",
                    escape_json(&source.kind),
                    escape_json(&source.location),
                    escape_json(&source.detail)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let outline_dump: Vec<String> = self
            .doc
            .outline
            .iter()
            .map(|(line, heading)| {
                format!(
                    "{{\"line\":{},\"heading\":\"{}\"}}",
                    line + 1,
                    escape_json(heading),
                )
            })
            .collect();
        let language_dump = {
            let report = &self.doc.language;
            let outline = report
                .outline
                .iter()
                .map(|item| {
                    format!(
                        "{{\"kind\":\"{}\",\"level\":{},\"line\":{},\"title\":\"{}\"}}",
                        item.kind.key(),
                        item.level,
                        item.line + 1,
                        escape_json(&item.title)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            let symbols = report
                .symbols
                .iter()
                .take(200)
                .map(|symbol| {
                    let detail = symbol
                        .detail
                        .as_deref()
                        .map(|detail| format!("\"{}\"", escape_json(detail)))
                        .unwrap_or_else(|| "null".to_string());
                    format!(
                        "{{\"kind\":\"{}\",\"name\":\"{}\",\"line\":{},\"column\":{},\"detail\":{}}}",
                        symbol.kind.key(),
                        escape_json(&symbol.name),
                        symbol.line + 1,
                        symbol.column + 1,
                        detail
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            let diagnostics = report
                .diagnostics
                .iter()
                .map(|diagnostic| {
                    format!(
                        "{{\"severity\":\"{}\",\"code\":\"{}\",\"message\":\"{}\",\"line\":{},\"column\":{},\"end_line\":{},\"end_column\":{}}}",
                        diagnostic.severity.key(),
                        escape_json(&diagnostic.code),
                        escape_json(&diagnostic.message),
                        diagnostic.line + 1,
                        diagnostic.column + 1,
                        diagnostic.end_line + 1,
                        diagnostic.end_column + 1
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{{\"id\":\"{}\",\"outline\":[{}],\"symbols\":[{}],\"diagnostics\":[{}]}}",
                report.language.key(),
                outline,
                symbols,
                diagnostics
            )
        };
        let search_results: Vec<String> = self
            .search
            .hits
            .iter()
            .map(|hit| {
                format!(
                    "{{\"document\":\"{}\",\"line\":{},\"column\":{},\"preview\":\"{}\"}}",
                    escape_json(&hit.document),
                    hit.line,
                    hit.column,
                    escape_json(&hit.preview),
                )
            })
            .collect();
        let new_file_dump = match &self.new_file {
            Some(state) => format!(
                "{{\"open\":true,\"kind\":\"{}\",\"name\":\"{}\",\"error\":\"{}\"}}",
                state.kind.key(),
                escape_json(&state.name),
                escape_json(&state.error),
            ),
            None => "{\"open\":false,\"kind\":\"\",\"name\":\"\",\"error\":\"\"}".to_string(),
        };
        let new_project_dump = match &self.new_project {
            Some(state) => format!(
                "{{\"open\":true,\"title\":\"{}\",\"type\":\"{}\",\"lang\":\"{}\",\
                 \"mode\":\"{}\",\"error\":\"{}\",\"picking\":{},\"busy\":{},\
                 \"path\":\"{}\",\"plan_files\":{}}}",
                escape_json(&state.title),
                state.kind.as_str(),
                state.lang.key(),
                match state.mode {
                    NewProjectMode::Template => "template",
                    NewProjectMode::Ai => "ai",
                },
                escape_json(&state.error),
                state.picking,
                state.busy,
                escape_json(
                    &state
                        .path
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_default()
                ),
                state
                    .plan
                    .as_ref()
                    .map(|plan| plan.files.len())
                    .unwrap_or(0),
            ),
            None => "{\"open\":false,\"title\":\"\",\"type\":\"\",\"lang\":\"\",\
                     \"mode\":\"\",\"error\":\"\",\"picking\":false,\"busy\":false,\
                     \"path\":\"\",\"plan_files\":0}"
                .to_string(),
        };
        let project_dump = self
            .project
            .as_ref()
            .map(|project| project.root().display().to_string())
            .unwrap_or_default();
        let recents_dump = self
            .recents
            .paths()
            .iter()
            .map(|path| format!("\"{}\"", escape_json(&path.display().to_string())))
            .collect::<Vec<_>>()
            .join(",");
        let recents_error = self.recents.error().unwrap_or("").to_string();
        let git_dump = {
            let mut files: Vec<(&String, &WorktreeStatus)> = self.git.files.iter().collect();
            files.sort_by(|a, b| a.0.cmp(b.0));
            let files = files
                .iter()
                .map(|(path, status)| {
                    format!(
                        "{{\"path\":\"{}\",\"status\":\"{}\"}}",
                        escape_json(path),
                        worktree_status_key(**status)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            let history = self
                .git_history
                .iter()
                .take(8)
                .map(|commit| {
                    format!(
                        "{{\"id\":\"{}\",\"subject\":\"{}\"}}",
                        escape_json(&short_id(&commit.id)),
                        escape_json(&commit.subject)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{{\"available\":{},\"branch\":\"{}\",\"ahead\":{},\"behind\":{},\
                 \"changed\":{},\"files\":[{}],\"history\":[{}],\"error\":\"{}\"}}",
                self.git.available,
                escape_json(&self.git.branch),
                self.git.ahead,
                self.git.behind,
                self.git.changed_count(),
                files,
                history,
                escape_json(&self.git.error),
            )
        };
        let panels_dump = format!(
            "{{\"explorer\":{},\"agent\":{},\"git\":{},\"explorer_w\":{},\"right_w\":{},\"wrap_w\":{},\
             \"chat_scroll_offset\":{},\"chat_scroll_max\":{},\"chat_at_bottom\":{},\
             \"window_title\":\"{}\"}}",
            self.explorer_open,
            self.agent_panel_open,
            self.git_panel_open,
            self.explorer_w,
            self.right_panel_w,
            f32::from(self.wrap_width),
            f32::from(self.chat_scroll.offset().y),
            f32::from(self.chat_scroll.max_offset().height),
            self.chat_at_bottom(),
            escape_json(&self.window_title)
        );
        let settings_dump = if let Some(state) = self.settings.as_ref() {
            format!(
                "{{\"open\":true,\"provider\":\"{}\",\"model\":\"{}\",\
                 \"base_url\":\"{}\",\"key_present\":{},\"key_source\":\"{}\",\"key_checked\":{},\
                 \"models\":{},\"fetching\":{},\"error\":\"{}\",\
                 \"language\":\"{}\",\"theme\":\"{}\",\"resolved_theme\":\"{}\",\
                 \"font_size\":{},\"word_wrap\":{},\"line_numbers\":{},\
                 \"show_config_files\":{},\"project_open\":{},\"project_title\":\"{}\",\
                 \"github_key_present\":{},\"github_key_source\":\"{}\",\"github_key_checked\":{},\
                 \"agents\":{},\"agents_error\":\"{}\",\"secret_reads\":{}}}",
                state.provider.key(),
                escape_json(&state.model),
                escape_json(&state.base_url),
                state.key_present || !state.key_input.is_empty(),
                escape_json(&state.key_source),
                state.key_checked,
                state.models.len(),
                state.fetching_models,
                escape_json(&state.error),
                state.language.key(),
                state.theme.key(),
                theme::resolved_key(),
                state.font_size,
                state.word_wrap,
                state.line_numbers,
                state.show_config_files,
                self.project_settings.is_some(),
                escape_json(
                    &self
                        .project
                        .as_ref()
                        .map(|project| project.manifest().title.clone())
                        .unwrap_or_default()
                ),
                state.github_token_present,
                escape_json(&state.github_token_source),
                state.github_token_checked,
                state.agents.len(),
                escape_json(&state.agents_error),
                secret_cache().read_count(),
            )
        } else {
            format!(
                "{{\"open\":false,\"provider\":\"{}\",\"model\":\"{}\",\
                 \"base_url\":\"{}\",\"key_present\":false,\"key_source\":\"\",\"key_checked\":false,\
                 \"models\":0,\"fetching\":false,\"error\":\"\",\
                 \"language\":\"{}\",\"theme\":\"{}\",\"resolved_theme\":\"{}\",\
                 \"font_size\":{},\"word_wrap\":{},\"line_numbers\":{},\
                 \"show_config_files\":{},\"project_open\":{},\"project_title\":\"{}\",\
                 \"github_key_present\":{},\"github_key_source\":\"{}\",\"github_key_checked\":{},\
                 \"agents\":0,\"agents_error\":\"\",\"secret_reads\":{}}}",
                self.agent_settings.provider.key(),
                escape_json(&self.agent_settings.model),
                escape_json(&self.agent_settings.base_url),
                self.app_settings.language.key(),
                self.app_settings.theme.key(),
                theme::resolved_key(),
                self.app_settings.font_size,
                self.app_settings.word_wrap,
                self.app_settings.line_numbers,
                self.app_settings.show_config_files,
                self.project_settings.is_some(),
                escape_json(
                    &self
                        .project
                        .as_ref()
                        .map(|project| project.manifest().title.clone())
                        .unwrap_or_default()
                ),
                cached_github_token_status().is_some_and(|(present, _)| present),
                escape_json(
                    &cached_github_token_status()
                        .map(|(_, source)| source)
                        .unwrap_or_default(),
                ),
                cached_github_token_status().is_some(),
                secret_cache().read_count(),
            )
        };
        let palette_dump = if let Some(state) = self.palette.as_ref() {
            let count = match state.mode {
                PaletteMode::Files => self.palette_files().len(),
                PaletteMode::Commands => self.palette_commands().len(),
            };
            format!(
                "{{\"open\":true,\"mode\":\"{}\",\"query\":\"{}\",\"selected\":{},\"results\":{}}}",
                state.mode.key(),
                escape_json(&state.query),
                state.selected,
                count,
            )
        } else {
            "{\"open\":false,\"mode\":\"\",\"query\":\"\",\"selected\":0,\"results\":0}".to_string()
        };
        let context_dump = if let Some(menu) = self.context_menu.as_ref() {
            let items = menu
                .items
                .iter()
                .map(|item| {
                    format!(
                        "{{\"label\":\"{}\",\"shortcut\":\"{}\",\"enabled\":{}}}",
                        escape_json(&item.label),
                        escape_json(item.shortcut),
                        item.enabled
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            let target = match self.context_target.as_ref() {
                Some(ContextTarget::Editor) => "editor",
                Some(ContextTarget::File(_)) => "file",
                Some(ContextTarget::Dir(_)) => "dir",
                Some(ContextTarget::Tab(_)) => "tab",
                None => "",
            };
            format!(
                "{{\"open\":true,\"target\":\"{}\",\"x\":{:.1},\"y\":{:.1},\"selected\":{},\"items\":[{}]}}",
                target, menu.x, menu.y, menu.selected, items
            )
        } else {
            "{\"open\":false,\"target\":\"\",\"x\":0,\"y\":0,\"selected\":0,\"items\":[]}"
                .to_string()
        };
        let chat_dump = self.chat.dump_json();
        let rename_dump = if let Some(state) = self.rename.as_ref() {
            format!(
                "{{\"open\":true,\"path\":\"{}\",\"name\":\"{}\",\"error\":\"{}\"}}",
                escape_json(&state.path),
                escape_json(&state.name),
                escape_json(&state.error)
            )
        } else {
            "{\"open\":false,\"path\":\"\",\"name\":\"\",\"error\":\"\"}".to_string()
        };
        let delete_dump = if let Some(prompt) = self.delete_prompt.as_ref() {
            format!(
                "{{\"open\":true,\"path\":\"{}\",\"error\":\"{}\"}}",
                escape_json(&prompt.path),
                escape_json(&prompt.error)
            )
        } else {
            "{\"open\":false,\"path\":\"\",\"error\":\"\"}".to_string()
        };
        let unsaved_dump = match self.unsaved_prompt {
            Some(prompt) => {
                let (action, tab) = match prompt.action {
                    PendingUnsaved::Quit => ("quit", "null".to_string()),
                    PendingUnsaved::CloseTab(index) => ("close_tab", index.to_string()),
                };
                format!(
                    "{{\"open\":true,\"action\":\"{action}\",\"tab\":{tab},\"dirty\":{}}}",
                    prompt.dirty
                )
            }
            None => "{\"open\":false,\"action\":\"\",\"tab\":null,\"dirty\":0}".to_string(),
        };
        let tabs_dump = (0..self.tabs.len())
            .filter_map(|index| {
                let doc = self.tab_document(index)?;
                let caret = if doc.reversed {
                    doc.selected.start
                } else {
                    doc.selected.end
                };
                let position = doc.buffer.position_at(caret);
                let kind = match FileKind::from_path(&doc.path) {
                    FileKind::Fountain => "fountain",
                    FileKind::Markdown => "markdown",
                    FileKind::Yaml => "yaml",
                    FileKind::Other => "other",
                };
                Some(format!(
                    "{{\"path\":\"{}\",\"title\":\"{}\",\"kind\":\"{}\",\
                     \"dirty\":{},\"active\":{},\"tab_preview\":{},\"preview\":{},\"revision\":{},\"saved_revision\":{},\
                     \"caret_line\":{},\"caret_col\":{},\"selection\":[{},{}],\
                     \"scroll_item\":{},\"undo\":{},\"redo\":{}}}",
                    escape_json(&doc.path),
                    escape_json(&self.tab_title(index)),
                    kind,
                    doc.is_dirty(),
                    index == self.active_tab,
                    self.tabs[index].preview,
                    doc.preview,
                    doc.revision,
                    doc.saved_revision,
                    position.line + 1,
                    position.col,
                    doc.selected.start,
                    doc.selected.end,
                    doc.list_state.logical_scroll_top().item_ix,
                    doc.undo_stack.len(),
                    doc.redo_stack.len(),
                ))
            })
            .collect::<Vec<_>>()
            .join(",");
        let entities_dump = self.entities.dump_json(200);
        let tree_rows: Vec<String> = explorer::build_rows(
            &explorer::visible_paths(&self.explorer, self.show_config_files),
            &self.collapsed,
        )
        .iter()
        .map(|row| {
            let kind = match row.kind {
                FileKind::Fountain => "fountain",
                FileKind::Markdown => "markdown",
                FileKind::Yaml => "yaml",
                FileKind::Other => "other",
            };
            format!(
                "{{\"path\":\"{}\",\"name\":\"{}\",\"depth\":{},\"dir\":{},\
                     \"collapsed\":{},\"kind\":\"{}\"}}",
                escape_json(&row.path),
                escape_json(&row.name),
                row.depth,
                row.is_dir,
                row.collapsed,
                kind,
            )
        })
        .collect();
        let search_dump = format!(
            "{{\"open\":{},\"query\":\"{}\",\"searched\":\"{}\",\"hits\":{},\
             \"selected\":{},\"truncated\":{},\"scanned\":{},\"skipped\":{},\
             \"results\":[{}]}}",
            self.search.open,
            escape_json(&self.search.query),
            escape_json(&self.search.searched),
            self.search.hits.len(),
            self.search.selected,
            self.search.truncated,
            self.search.scanned,
            self.search.skipped,
            search_results.join(","),
        );
        let export_dump = format!(
            "{{\"out\":\"{}\",\"bytes\":{},\"pages\":{},\"error\":\"{}\"}}",
            escape_json(&self.export.out),
            self.export.bytes,
            self.export.pages,
            escape_json(&self.export.error),
        );
        let agent_dump = format!(
            "{{\"status\":\"{}\",\"scene\":{},\"focus\":\"{}\",\"role\":\"{}\",\
             \"role_source\":\"{}\",\"stage\":\"{}\",\"sources\":[{}],\
             \"output\":\"{}\",\"summary\":\"{}\",\"error\":\"{}\",\"scroll\":{},\
             \"elapsed_ms\":{}}}",
            self.agent.status_key(),
            agent_scene_dump,
            escape_json(&self.agent.focus),
            escape_json(&self.agent.role),
            escape_json(&self.agent.role_source),
            escape_json(&self.agent.stage),
            agent_sources_dump,
            escape_json(&self.agent.output),
            escape_json(&self.agent.summary),
            escape_json(&self.agent.error),
            self.agent.scroll,
            agent_elapsed_dump,
        );

        // M6 slice 5: the focus bar's own state — open/typed/committed — so
        // the bar routing and the focus persistence are verifiable headless.
        let focus_bar_dump = format!(
            "{{\"open\":{},\"typed\":\"{}\",\"committed\":\"{}\"}}",
            self.focus_bar.open,
            escape_json(&self.focus_bar.focus),
            escape_json(&self.review_focus),
        );

        // M6 slice 4: the proposal surface's full state — status, scene,
        // instruction, record (id/target/hunks), the *entire* pre-rendered
        // diff, arming and timing — so apply/reject flows are verified from
        // the dump, never from a screenshot.
        let proposal_scene_dump = if self.propose.scene_number > 0 {
            self.propose.scene_number.to_string()
        } else {
            "null".to_string()
        };
        let proposal_elapsed_dump = self
            .propose
            .started
            .map(|at| at.elapsed().as_millis() as u64)
            .map(|ms| ms.to_string())
            .unwrap_or_else(|| "null".to_string());
        let proposal_diff_dump = self
            .propose
            .diff_rows
            .iter()
            .map(|(label, text)| format!("[\"{}\",\"{}\"]", label, escape_json(text)))
            .collect::<Vec<_>>()
            .join(",");
        // M6 slice 6: the hunk selection — which hunks ⌘⇧Enter would apply.
        let proposal_selected_dump = self
            .propose
            .selected
            .iter()
            .map(|on| on.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let proposal_dump = format!(
            "{{\"status\":\"{}\",\"scene\":{},\"instruction\":\"{}\",\"role\":\"{}\",\
             \"role_source\":\"{}\",\"id\":\"{}\",\
             \"target\":\"{}\",\"hunks\":{},\"armed\":{},\"scroll\":{},\
             \"hunk_cursor\":{},\"selected\":[{}],\
             \"stage\":\"{}\",\"summary\":\"{}\",\"error\":\"{}\",\"diff_rows\":[{}],\
             \"elapsed_ms\":{}}}",
            self.propose.status_key(),
            proposal_scene_dump,
            escape_json(&self.propose.instruction),
            escape_json(&self.propose.role),
            escape_json(&self.propose.role_source),
            escape_json(&self.propose.id),
            escape_json(&self.propose.target),
            self.propose.hunks,
            self.propose.armed_apply,
            self.propose.scroll,
            self.propose.hunk_cursor,
            proposal_selected_dump,
            escape_json(&self.propose.stage),
            escape_json(&self.propose.summary),
            escape_json(&self.propose.error),
            proposal_diff_dump,
            proposal_elapsed_dump,
        );

        format!(
            concat!(
                "{{\"kind\":\"spike-a-buffer-dump\",",
                "\"lines\":{},\"doc_len\":{},\"edits\":{},",
                "\"revision\":{},\"saved_revision\":{},",
                "\"caret_line\":{},\"caret_col\":{},\"selection\":[{},{}],\"selected_text\":\"{}\",",
                "\"scenes\":{},\"undo\":{},\"redo\":{},\"around\":\"{}\",",
                "\"find\":{{\"open\":{},\"query\":\"{}\",\"matches\":{},",
                "\"current\":{},\"replaced\":{}}},",
                "\"diff\":{{\"open\":{},\"lines\":{},\"added\":{},",
                "\"removed\":{},\"cursor\":{},\"buffer_hash\":\"{}\"}},",
                "\"ime\":{{\"marked\":{},\"composing\":{}}},",
                "\"click\":{{\"x\":{},\"y\":{},\"line\":{},\"top\":{},\"item_h\":{},\"offset\":{}}},",
                "\"document\":\"{}\",\"language\":\"{}\",\"preview\":{{\"active\":{},\"blocks\":{}}},\"dirty\":{},\"notice\":{},\"explorer\":[{}],\
                 \"agent\":{},\"focus_bar\":{},\"proposal\":{},\"export\":{},\"search\":{},\
                 \"outline\":[{}],\"language\":{},\"tree\":[{}],\"entities\":[{}],\"pages\":{},\"quit_prompt\":{},\
                 \"unsaved\":{},\"git\":{},\"panels\":{},\"settings\":{},\"palette\":{},\"context_menu\":{},\
                 \"rename\":{},\"delete_prompt\":{},\"chat\":{},\
                 \"new_file\":{},\"new_project\":{},\"tabs\":[{}],\"active_tab\":{},\
                 \"project\":\"{}\",\"recents\":[{}],\"recents_error\":\"{}\",\
                 \"workspace\":{},\
                 \"backup\":{{\"path\":\"{}\",\"saved\":{},\"error\":\"{}\"}},\
                 \"lang\":\"{}\"}}"
            ),
            self.doc.buffer.line_count(),
            len,
            self.metrics.borrow().edits,
            self.doc.revision,
            self.doc.saved_revision,
            position.line + 1,
            position.col,
            self.doc.selected.start,
            self.doc.selected.end,
            selected_text,
            self.doc.scene_lines.len(),
            self.doc.undo_stack.len(),
            self.doc.redo_stack.len(),
            around,
            self.doc.find.open,
            escape_json(&self.doc.find.query),
            self.doc.find.matches.len(),
            self.doc.find.current,
            self.doc.find.replaced_total,
            self.doc.diff.open,
            diff_lines,
            diff_added,
            diff_removed,
            self.doc.diff.cursor,
            buffer_hash,
            marked_dump,
            self.doc.composing,
            click_x,
            click_y,
            click_line_dump,
            click_top_dump,
            click_h_dump,
            click_offset_dump,
            escape_json(&self.doc.path),
            language_name(&self.doc.path),
            self.doc.preview,
            self.doc.preview_document.blocks.len(),
            self.dirty(),
            notice_dump,
            explorer_dump,
            agent_dump,
            focus_bar_dump,
            proposal_dump,
            export_dump,
            search_dump,
            outline_dump.join(","),
            language_dump,
            tree_rows.join(","),
            entities_dump,
            self.doc
                .page_count
                .map(|pages| pages.to_string())
                .unwrap_or_else(|| "null".to_string()),
            self.unsaved_prompt.is_some(),
            unsaved_dump,
            git_dump,
            panels_dump,
            settings_dump,
            palette_dump,
            context_dump,
            rename_dump,
            delete_dump,
            chat_dump,
            new_file_dump,
            new_project_dump,
            tabs_dump,
            self.active_tab,
            escape_json(&project_dump),
            recents_dump,
            escape_json(&recents_error),
            self.workspace_generation,
            escape_json(self.doc.backup_path.as_deref().unwrap_or("")),
            self.doc.backup_saved_at.is_some(),
            escape_json(self.doc.backup_error.as_deref().unwrap_or("")),
            self.lang.key(),
        )
    }

    fn is_boundary(&self, offset: usize) -> bool {
        let position = self.doc.buffer.position_at(offset);
        self.doc.buffer.lines()[position.line].is_char_boundary(position.col)
    }
}

fn cursor_element() -> gpui::Div {
    // The negative right margin keeps the cursor's flow width at zero so the
    // text after the caret is not pushed sideways.
    div()
        .w(px(2.0))
        .mr(px(-2.0))
        .h_full()
        .bg(rgb(theme::cursor_fg()))
}

/// Install the native macOS menus (M10 slice 1). Labels come from the i18n
/// table and are rebuilt when the language toggles; items dispatch the same
/// actions as the keybindings, and the shortcut column is resolved by the
/// platform from the keymap.
fn install_menus(lang: Lang, recents: &[PathBuf], cx: &gpui::App) {
    let label = |key: &str| i18n::tr(lang, key);
    let menu = |key: &str, items: Vec<gpui::MenuItem>| gpui::Menu {
        name: label(key).into(),
        items,
    };
    let mut file_items = vec![
        gpui::MenuItem::action(label("menu.new_file"), NewFile),
        gpui::MenuItem::action(label("menu.new_project"), NewProject),
        gpui::MenuItem::action(label("menu.open_project"), OpenProject),
    ];
    if !recents.is_empty() {
        let mut recent_items: Vec<gpui::MenuItem> = recents
            .iter()
            .map(|path| {
                gpui::MenuItem::action(
                    recent::display_path(path),
                    OpenRecentProject { path: path.clone() },
                )
            })
            .collect();
        recent_items.push(gpui::MenuItem::separator());
        recent_items.push(gpui::MenuItem::action(
            label("menu.clear_recents"),
            ClearRecentProjects,
        ));
        file_items.push(gpui::MenuItem::submenu(gpui::Menu {
            name: label("menu.open_recent").into(),
            items: recent_items,
        }));
    }
    file_items.push(gpui::MenuItem::separator());
    file_items.push(gpui::MenuItem::action(label("menu.close_tab"), CloseTab));
    file_items.push(gpui::MenuItem::action(label("menu.save"), Save));
    file_items.push(gpui::MenuItem::action(label("menu.export_pdf"), ExportPdf));

    cx.set_menus(vec![
        menu(
            "menu.app",
            vec![
                gpui::MenuItem::action(label("menu.settings"), OpenSettings),
                gpui::MenuItem::separator(),
                gpui::MenuItem::action(label("menu.quit"), Quit),
            ],
        ),
        menu("menu.file", file_items),
        menu(
            "menu.project",
            vec![
                gpui::MenuItem::action(label("menu.project_settings"), OpenProjectSettings),
                gpui::MenuItem::action(label("menu.show_config_files"), ToggleConfigFiles),
            ],
        ),
        menu(
            "menu.edit",
            vec![
                gpui::MenuItem::action(label("menu.undo"), Undo),
                gpui::MenuItem::action(label("menu.redo"), Redo),
                gpui::MenuItem::separator(),
                gpui::MenuItem::action(label("menu.cut"), CutText),
                gpui::MenuItem::action(label("menu.copy"), CopyText),
                gpui::MenuItem::action(label("menu.paste"), PasteText),
                gpui::MenuItem::action(label("menu.select_all"), SelectAll),
                gpui::MenuItem::separator(),
                gpui::MenuItem::action(label("menu.find"), ToggleFind),
                gpui::MenuItem::action(label("menu.replace"), FocusReplace),
                gpui::MenuItem::action(label("menu.project_search"), ProjectSearch),
                gpui::MenuItem::separator(),
                gpui::MenuItem::action(label("menu.context_menu"), ShowContextMenu),
            ],
        ),
        menu(
            "menu.view",
            vec![
                gpui::MenuItem::action(label("menu.prev_tab"), PrevTab),
                gpui::MenuItem::action(label("menu.next_tab"), NextTab),
                gpui::MenuItem::separator(),
                gpui::MenuItem::action(label("menu.toggle_explorer"), ToggleExplorer),
                gpui::MenuItem::action(label("menu.toggle_agent"), ToggleAgentPanel),
                gpui::MenuItem::action(label("menu.toggle_git"), ToggleGitPanel),
                gpui::MenuItem::separator(),
                gpui::MenuItem::action(label("menu.toggle_diff"), ToggleDiff),
                gpui::MenuItem::action(label("menu.toggle_preview"), TogglePreview),
                gpui::MenuItem::separator(),
                gpui::MenuItem::action(label("menu.quick_open"), QuickOpen),
                gpui::MenuItem::action(label("menu.command_palette"), CommandPalette),
                gpui::MenuItem::separator(),
                gpui::MenuItem::action(label("menu.language"), ToggleLang),
            ],
        ),
        menu(
            "menu.script",
            vec![
                gpui::MenuItem::action(label("menu.review"), ReviewScene),
                gpui::MenuItem::action(label("menu.focus"), OpenFocusBar),
                gpui::MenuItem::action(label("menu.propose"), OpenPropose),
                gpui::MenuItem::action(label("menu.retry"), RetryAgent),
                gpui::MenuItem::separator(),
                gpui::MenuItem::action(label("menu.cycle_element"), CycleElement),
                gpui::MenuItem::action(label("menu.cycle_element_back"), CycleElementBack),
                gpui::MenuItem::separator(),
                gpui::MenuItem::action(label("menu.restore_backup"), RestoreBackup),
                gpui::MenuItem::action(label("menu.discard_backup"), DiscardBackup),
            ],
        ),
    ]);
}

/// Line-number gutter cell (M9 slice 3): fixed width, right-aligned digits.
fn gutter_element(ix: usize, active: bool) -> gpui::Div {
    if !theme::line_numbers() {
        return div().w(px(0.0)).flex_none();
    }
    div()
        .w(px(theme::gutter_width()))
        .flex_none()
        .pr_2()
        .text_align(gpui::TextAlign::Right)
        .text_color(rgb(if active {
            theme::status_fg()
        } else {
            theme::gutter_fg()
        }))
        .child(format!("{}", ix + 1))
}

/// Explorer file-type colors (M9 slice 4).
/// Rank quick-open paths: filename prefix matches first, then path
/// substring matches; empty query lists the first 80 sorted paths.
fn filter_palette_paths(paths: Vec<String>, query: &str) -> Vec<String> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        let mut paths = paths;
        paths.sort();
        paths.truncate(80);
        return paths;
    }
    let mut starts = Vec::new();
    let mut contains = Vec::new();
    for path in paths {
        let name = path.rsplit('/').next().unwrap_or(&path).to_lowercase();
        if name.starts_with(&query) {
            starts.push(path);
        } else if path.to_lowercase().contains(&query) {
            contains.push(path);
        }
    }
    starts.sort();
    contains.sort();
    starts.extend(contains);
    starts.truncate(80);
    starts
}

/// Parse `/command rest` at the start of a chat message. Returns the command
/// (including slash) and the remaining instruction; unknown text returns
/// `None` and is sent untouched.
fn split_chat_command(input: &str) -> Option<(&str, &str)> {
    let trimmed = input.trim_start();
    if !trimmed.starts_with('/') {
        return None;
    }
    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let command = parts.next()?;
    let rest = parts.next().unwrap_or("").trim_start();
    match command {
        "/plan" | "/build" | "/review" | "/rewrite" | "/continuity" | "/character" => {
            Some((command, rest))
        }
        _ => None,
    }
}

/// i18n key for a chat mode button.
fn chat_mode_key(mode: ChatMode) -> &'static str {
    match mode {
        ChatMode::Plan => "chat.mode.plan",
        ChatMode::Build => "chat.mode.build",
        ChatMode::Review => "chat.mode.review",
    }
}

/// Split a project-relative path into `(parent, stem, extension)`; the
/// extension includes its leading dot and is empty when the name has none.
fn split_path_name(path: &str) -> (&str, &str, &str) {
    let (parent, filename) = path.rsplit_once('/').unwrap_or(("", path));
    match filename.rfind('.') {
        Some(dot) if dot > 0 && dot + 1 < filename.len() => {
            (parent, &filename[..dot], &filename[dot..])
        }
        _ => (parent, filename, ""),
    }
}

/// Localized project kind label for the Project Settings form.
fn project_type_i18n(kind: ProjectType) -> &'static str {
    match kind {
        ProjectType::Feature => "project.type.feature",
        ProjectType::Series => "project.type.series",
        ProjectType::WebSeries => "project.type.web_series",
        ProjectType::Short => "project.type.short",
        ProjectType::Microdrama => "project.type.microdrama",
        ProjectType::Other => "project.type.other",
    }
}

fn language_name(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("fountain") => "Fountain",
        Some("md") => "Markdown",
        Some("yaml") | Some("yml") => "YAML",
        _ => "Text",
    }
}

fn git_color(status: WorktreeStatus) -> u32 {
    match status {
        WorktreeStatus::Modified => theme::git_modified_fg(),
        WorktreeStatus::Added | WorktreeStatus::Untracked => theme::git_added_fg(),
        WorktreeStatus::Deleted => theme::git_deleted_fg(),
        WorktreeStatus::Renamed | WorktreeStatus::Copied => theme::git_renamed_fg(),
        WorktreeStatus::Conflicted => theme::git_conflict_fg(),
        WorktreeStatus::Unmodified | WorktreeStatus::Unknown => theme::status_fg(),
    }
}

/// Letter shown next to a changed file/folder so the state is not conveyed by
/// color alone.
fn git_letter(status: WorktreeStatus) -> &'static str {
    match status {
        WorktreeStatus::Modified => "M",
        WorktreeStatus::Added => "A",
        WorktreeStatus::Untracked => "U",
        WorktreeStatus::Deleted => "D",
        WorktreeStatus::Renamed => "R",
        WorktreeStatus::Copied => "C",
        WorktreeStatus::Conflicted => "!",
        WorktreeStatus::Unmodified | WorktreeStatus::Unknown => "",
    }
}

fn worktree_status_key(status: WorktreeStatus) -> &'static str {
    match status {
        WorktreeStatus::Unmodified => "unmodified",
        WorktreeStatus::Modified => "modified",
        WorktreeStatus::Added => "added",
        WorktreeStatus::Deleted => "deleted",
        WorktreeStatus::Renamed => "renamed",
        WorktreeStatus::Copied => "copied",
        WorktreeStatus::Untracked => "untracked",
        WorktreeStatus::Conflicted => "conflicted",
        WorktreeStatus::Unknown => "unknown",
    }
}

/// Monochrome Lucide icon from the embedded asset set (S3c). The SVG is
/// painted as an alpha mask and tinted with the current text color.
fn icon(name: &'static str, color: u32, size: f32) -> gpui::Svg {
    svg()
        .path(format!("icons/{name}.svg"))
        .flex_none()
        .w(px(size))
        .h(px(size))
        .text_color(rgb(color))
}

fn escape_json(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

/// Undo bookkeeping for one applied edit (Spike B undo grouping). While an
/// IME composition session is open (`composing`), consecutive edits that
/// replace the composition's own range merge into the single open entry, so
/// one composition = one undo step no matter how many marked updates the IME
/// sends. A range that does not line up opens a fresh entry instead of
/// corrupting the merge.
fn record_edit(
    undo_stack: &mut Vec<Edit>,
    composing: bool,
    replaced: Range<usize>,
    inserted: Range<usize>,
    new_text: &str,
    old_text: String,
) {
    if composing
        && let Some(prev) = undo_stack.last_mut()
        && prev.composition
        && prev.range == replaced
    {
        prev.new_text = new_text.to_string();
        prev.range = inserted;
        return;
    }
    undo_stack.push(Edit {
        range: inserted,
        new_text: new_text.to_string(),
        old_text,
        composition: composing,
    });
}

/// Byte offsets inside `text` for a UTF-16 range expressed **relative to
/// `text`** — what `setMarkedText:selectedRange:` delivers (the selection
/// lives inside the marked string, not in document coordinates; treating it
/// as document UTF-16 drifts as soon as the document head is non-ASCII).
/// Offsets past the end clamp to `text.len()`, and offsets inside a
/// surrogate pair clamp to the enclosing character boundary.
fn utf16_to_byte_in(text: &str, utf16: Range<usize>) -> Range<usize> {
    let mut units = 0usize;
    let mut start = None;
    let mut end = None;
    for (byte, ch) in text.char_indices() {
        if start.is_none() && units >= utf16.start {
            start = Some(byte);
        }
        if end.is_none() && units >= utf16.end {
            end = Some(byte);
        }
        units += ch.len_utf16();
    }
    start.unwrap_or(text.len())..end.unwrap_or(text.len())
}

/// Intersect a document range with one line and return it in line-local byte
/// columns. Clamping both ends makes selections that start or end on another
/// line safe: no unsigned subtraction can underflow while a row renders.
fn line_local_range(range: &Range<usize>, line: &Range<usize>) -> Option<Range<usize>> {
    let start = range.start.max(line.start);
    let end = range.end.min(line.end);
    (start < end).then(|| (start - line.start)..(end - line.start))
}

/// Byte columns where each visual (soft-wrapped) row of a line starts,
/// derived from shaped glyph positions crossing `wrap_width`. Glyph x
/// positions are absolute on the un-wrapped line, so a glyph past the wrap
/// width marks the start of the next visual row. Word-wrap edges can differ
/// by a word between GPUI's div layout and this approximation; the spike
/// accepts and documents that (mouse caret lands within the same row).
fn wrap_row_starts(glyphs: &[(usize, f32)], wrap_width: f32) -> Vec<usize> {
    let mut starts = vec![0usize];
    let mut current_row_start_x = 0.0;
    for &(index, x) in glyphs {
        // When the glyph's absolute x minus the current row's start x
        // exceeds the wrap width, we've wrapped to a new visual row.
        if x - current_row_start_x >= wrap_width {
            starts.push(index);
            current_row_start_x = x;
        }
    }
    starts
}

/// Compute the visual (soft-wrapped) row boundaries for a line of text,
/// given the available content width. Returns a vec of byte offsets within
/// `line` where each visual row starts (first is always 0). Uses shaped
/// glyph x-positions to find wrap points at `wrap_width`.
fn visual_row_starts(window: &Window, line: &str, wrap_width: f32) -> Vec<usize> {
    let shaped = shape_line(window, line);
    let glyphs: Vec<(usize, f32)> = shaped
        .runs
        .iter()
        .flat_map(|run| {
            run.glyphs
                .iter()
                .map(|g| (g.index, f32::from(g.position.x)))
        })
        .collect();
    wrap_row_starts(&glyphs, wrap_width)
}

/// Given a line, caret column (byte offset within the line), and the
/// visual row starts, returns the visual row index (0-based) containing
/// the caret, and the x-offset within that visual row for cursor rendering.
fn caret_visual_row_and_x(
    window: &Window,
    line: &str,
    caret_col: usize,
    wrap_width: f32,
) -> (usize, f32) {
    let starts = visual_row_starts(window, line, wrap_width);
    let row = starts
        .iter()
        .rposition(|&start| start <= caret_col)
        .unwrap_or(0);
    // x within the visual row: compute via shaped glyphs
    let shaped = shape_line(window, line);
    let glyphs: Vec<(usize, f32)> = shaped
        .runs
        .iter()
        .flat_map(|run| {
            run.glyphs
                .iter()
                .map(|g| (g.index, f32::from(g.position.x)))
        })
        .collect();
    let row_start_x = glyphs
        .iter()
        .find(|(ix, _)| *ix >= starts[row])
        .map(|(_, x)| *x)
        .unwrap_or(0.0);
    let caret_x = glyphs
        .iter()
        .find(|(ix, _)| *ix >= caret_col)
        .map(|(_, x)| *x)
        .unwrap_or_else(|| {
            // Past end: use line width
            shaped
                .runs
                .last()
                .map(|r| {
                    r.glyphs
                        .last()
                        .map(|g| f32::from(g.position.x))
                        .unwrap_or(0.0)
                })
                .unwrap_or(0.0)
        });
    (row, caret_x - row_start_x)
}

impl EntityInputHandler for Editor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.clamp(self.range_from_utf16(&range_utf16));
        adjusted_range.replace(self.range_to_utf16(&range));
        Some(self.doc.buffer.slice(range))
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.doc.selected),
            reversed: self.doc.reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.doc
            .marked
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // The system calls this after commit (insertText) or when it drops
        // the composition; the buffer text was already settled by the
        // preceding call, so this only ends the session.
        self.doc.marked = None;
        self.doc.composing = false;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The unsaved-changes prompt owns the keyboard (M9 slice 6).
        if self.unsaved_prompt.is_some() {
            return;
        }
        if self.context_menu.is_some() {
            self.context_menu = None;
            cx.notify();
            return;
        }
        if self.rename.is_some() {
            self.rename_input(new_text, cx);
            return;
        }
        if self.delete_prompt.is_some() {
            return;
        }
        if self.git_branch_prompt.is_some() {
            self.branch_prompt_input(new_text, cx);
            return;
        }
        if self.github_publish.is_some() {
            self.github_publish_input(new_text, cx);
            return;
        }
        if self.github_pr.is_some() {
            self.github_pr_input(new_text, cx);
            return;
        }
        if self.git_commit.is_some() {
            self.git_commit_input(new_text, cx);
            return;
        }
        // Diff and restore are read-only modals: typing must never reach the
        // buffer behind them.
        if self.git_diff.is_some() || self.git_restore.is_some() {
            return;
        }
        if self.chat.focused {
            self.chat.push_input(new_text);
            cx.notify();
            return;
        }
        if self.palette.is_some() {
            self.palette_input(new_text, cx);
            return;
        }
        if self.settings.is_some() {
            self.settings_input(new_text, cx);
            return;
        }
        if self.project_settings.is_some() {
            self.project_settings_input(new_text, cx);
            return;
        }
        // The New File modal owns typing while open (M10 slice 2).
        if self.new_file.is_some() {
            let cleaned: String = new_text
                .chars()
                .filter(|c| !c.is_control() || *c == ' ')
                .collect();
            if let Some(state) = self.new_file.as_mut() {
                state.name.push_str(&cleaned);
                state.error.clear();
            }
            cx.notify();
            return;
        }
        // The New Project dialog owns its title/prompt field (M10 slice 2b,
        // M16 slice 9).
        if self.new_project.is_some() {
            let cleaned: String = new_text
                .chars()
                .filter(|c| !c.is_control() || *c == ' ')
                .collect();
            if let Some(state) = self.new_project.as_mut()
                && !state.picking
                && !state.busy
                && state.plan.is_none()
            {
                let target =
                    if state.mode == NewProjectMode::Ai && state.field == NewProjectField::Prompt {
                        &mut state.prompt
                    } else {
                        &mut state.title
                    };
                target.push_str(&cleaned);
                state.error.clear();
            }
            cx.notify();
            return;
        }
        // Find bar owns keystrokes while open: typing, IME commits and
        // pastes land in the active field, never in the buffer.
        if self.doc.find.open {
            self.find_input(new_text);
            cx.notify();
            return;
        }
        // Search bar (M8 slice 2) owns keystrokes while open: single line,
        // control chars out, same rules as the other bars.
        if self.search.open {
            let cleaned: String = new_text
                .chars()
                .filter(|c| !c.is_control() || *c == ' ')
                .collect();
            self.search.query.push_str(&cleaned);
            cx.notify();
            return;
        }
        // Instruction bar owns keystrokes while open (M6 slice 4): the text
        // becomes the proposal instruction, single line, control chars out.
        if self.propose_bar.open {
            let cleaned: String = new_text
                .chars()
                .filter(|c| !c.is_control() || *c == ' ')
                .collect();
            self.propose_bar.instruction.push_str(&cleaned);
            cx.notify();
            return;
        }
        // Focus bar owns keystrokes while open (M6 slice 5): same rules.
        if self.focus_bar.open {
            let cleaned: String = new_text
                .chars()
                .filter(|c| !c.is_control() || *c == ' ')
                .collect();
            self.focus_bar.focus.push_str(&cleaned);
            cx.notify();
            return;
        }
        if self.doc.preview {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or(self.doc.marked.clone())
            .unwrap_or_else(|| self.doc.selected.clone());
        let text = normalize_newlines(new_text);
        let range = {
            let mut range = self.clamp(range);
            if range.end < range.start {
                range = range.start..range.start;
            }
            range
        };
        self.apply_edit(range, &text);
        // `insertText` is both plain typing and composition commit; either
        // way no marked session survives it (the system's later unmarkText
        // becomes a no-op). This also guarantees a stale `composing` flag
        // can never swallow the next keystroke into an old entry.
        self.doc.composing = false;
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.unsaved_prompt.is_some() {
            return;
        }
        if self.context_menu.is_some() {
            self.context_menu = None;
            cx.notify();
            return;
        }
        if self.rename.is_some() {
            self.rename_input(new_text, cx);
            return;
        }
        if self.delete_prompt.is_some() {
            return;
        }
        if self.git_branch_prompt.is_some() {
            self.branch_prompt_input(new_text, cx);
            return;
        }
        if self.github_publish.is_some() {
            self.github_publish_input(new_text, cx);
            return;
        }
        if self.github_pr.is_some() {
            self.github_pr_input(new_text, cx);
            return;
        }
        if self.git_commit.is_some() {
            self.git_commit_input(new_text, cx);
            return;
        }
        // Diff and restore are read-only modals: typing must never reach the
        // buffer behind them.
        if self.git_diff.is_some() || self.git_restore.is_some() {
            return;
        }
        if self.chat.focused {
            self.chat.push_input(new_text);
            cx.notify();
            return;
        }
        if self.palette.is_some() {
            self.palette_input(new_text, cx);
            return;
        }
        if self.settings.is_some() {
            self.settings_input(new_text, cx);
            return;
        }
        if self.project_settings.is_some() {
            self.project_settings_input(new_text, cx);
            return;
        }
        // IME while the find bar is open: commit into the active field with
        // no marked state (the documented single-focus compromise — live
        // composition inside the bar is production work).
        if self.doc.find.open {
            self.find_input(new_text);
            cx.notify();
            return;
        }
        if self.doc.preview {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or(self.doc.marked.clone())
            .unwrap_or_else(|| self.doc.selected.clone());
        let text = normalize_newlines(new_text);
        let range = self.clamp(range);
        // First update of a session opens the undo group; subsequent marked
        // updates merge into that entry inside `record_edit`.
        self.doc.composing = true;
        let inserted = self.apply_edit(range, &text);
        self.doc.marked = (!text.is_empty()).then_some(inserted.clone());
        if text.is_empty() {
            // An empty setMarkedText is the IME removing the composition
            // (cancellation): the session ends here even if the system's
            // unmarkText never follows, so the stale flag can never swallow
            // the next keystroke into an old entry.
            self.doc.composing = false;
        }
        // `selectedRange` from setMarkedText is relative to the *marked
        // string*, not to the document: convert it inside `text` (treating it
        // as document UTF-16 drifts as soon as the document head is
        // non-ASCII) and anchor the result at the inserted span.
        let selection = new_selected_range_utf16
            .as_ref()
            .map(|utf16| {
                let local = utf16_to_byte_in(&text, utf16.clone());
                (inserted.start + local.start)..(inserted.start + local.end)
            })
            .unwrap_or_else(|| inserted.end..inserted.end);
        self.doc.selected = self.clamp(selection);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _bounds: Bounds<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.clamp(self.range_from_utf16(&range_utf16));
        let start = self.doc.buffer.position_at(range.start);
        let end = self.doc.buffer.position_at(range.end);
        // `bounds_for_item` already accounts for scroll. Only rendered (visible)
        // items report bounds, which is fine: IME positions over visible text.
        let start_bounds = self.doc.list_state.bounds_for_item(start.line)?;
        let end_bounds = self.doc.list_state.bounds_for_item(end.line);
        let x_start = x_for_index(window, &self.doc.buffer.lines()[start.line], start.col);
        let x_end = x_for_index(window, &self.doc.buffer.lines()[end.line], end.col);
        let top_left = point(start_bounds.left() + x_start, start_bounds.top());
        let bottom_right = match end_bounds {
            Some(bounds) => point(bounds.left() + x_end, bounds.bottom()),
            None => point(start_bounds.left() + x_end, start_bounds.bottom()),
        };
        Some(Bounds::from_corners(top_left, bottom_right))
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        // NSTextInputClient's `characterIndexForPoint:` uses the same mapping
        // as a mouse click; the index crosses the Cocoa boundary as UTF-16,
        // like every other range this handler reports.
        let offset = self.offset_for_point(position, window)?;
        Some(self.doc.buffer.offset_to_utf16(offset))
    }
}

impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        self.metrics.borrow_mut().on_render(now);
        // Keep the scene index/outline fresh for the explorer (cheap: the
        // scan only runs after an edit or a document load); the element
        // classification is debounced.
        self.refresh_scenes();
        self.refresh_element_kinds(cx);
        self.maybe_request_pages(cx);
        self.maybe_refresh_git(cx);
        if self.git_history_stale && !self.git_history_loading && self.git.available {
            self.request_git_history(cx);
        }
        if self.git_branches_stale && !self.git_branches_loading && self.git.available {
            self.request_git_branches(cx);
        }
        if self.git_remotes_stale && !self.git_remotes_loading && self.git.available {
            self.request_git_remotes(cx);
        }
        theme::set_system_dark(matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        ));

        // The probe is driven by a timer scheduled from here, never by
        // notifying during render (see `advance_probe`).
        self.schedule_probe_step(window, cx);
        self.sync_window_title(window);

        // Wrap width for non-hot rows, refreshed from the viewport each frame.
        // The `list` render callback reads it back through the entity. With a
        // side panel open the document area is narrower by the panel width and
        // its 6 px splitter; the fitted widths keep the editor at its minimum
        // when the window shrinks (M16 slice 3).
        let viewport_w = f32::from(window.viewport_size().width);
        let explorer_open = self.project.is_some() && self.explorer_open;
        let right_open = self.agent_panel_open;
        let (explorer_fit, right_fit) = panel_resize::fit(
            viewport_w,
            panel_resize::Panels {
                explorer_open,
                right_open,
                explorer_w: self.explorer_w,
                right_w: self.right_panel_w,
            },
        );
        if explorer_open {
            self.explorer_w = explorer_fit;
        }
        if right_open {
            self.right_panel_w = right_fit;
        }
        let explorer_chrome = if explorer_open {
            self.explorer_w + panel_resize::SPLITTER_W
        } else {
            0.0
        };
        let right_chrome = if right_open {
            self.right_panel_w + panel_resize::SPLITTER_W
        } else {
            0.0
        };
        self.wrap_width = if !self.word_wrap {
            px(100_000.0)
        } else {
            (window.viewport_size().width
                - px(explorer_chrome + right_chrome + ROW_PADDING_X * 2.0 + theme::gutter_width()))
            .max(px(200.0))
        };

        let focus = self.focus_handle.clone();
        let entity = cx.entity();

        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(theme::bg()))
            .track_focus(&self.focus_handle(cx))
            // Side-panel splitter drag (M16 slice 3). The root receives the
            // moves while the pointer is over any child, so the 6 px handle
            // never needs to keep the pointer.
            .on_mouse_move(cx.listener(Self::on_panel_resize))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.end_panel_resize(cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.end_panel_resize(cx)),
            )
            .on_action(cx.listener(Self::next_scene))
            .on_action(cx.listener(Self::prev_scene))
            .on_action(cx.listener(Self::on_left))
            .on_action(cx.listener(Self::on_right))
            .on_action(cx.listener(Self::on_up))
            .on_action(cx.listener(Self::on_down))
            .on_action(cx.listener(Self::on_home))
            .on_action(cx.listener(Self::on_end))
            .on_action(cx.listener(Self::on_select_left))
            .on_action(cx.listener(Self::on_select_right))
            .on_action(cx.listener(Self::on_select_up))
            .on_action(cx.listener(Self::on_select_down))
            .on_action(cx.listener(Self::on_select_home))
            .on_action(cx.listener(Self::on_select_end))
            .on_action(cx.listener(Self::on_select_all))
            // word/line editing and document navigation (M9 slice 2)
            .on_action(cx.listener(Self::on_word_left))
            .on_action(cx.listener(Self::on_word_right))
            .on_action(cx.listener(Self::on_select_word_left))
            .on_action(cx.listener(Self::on_select_word_right))
            .on_action(cx.listener(Self::on_delete_word_left))
            .on_action(cx.listener(Self::on_delete_word_right))
            .on_action(cx.listener(Self::on_move_line_up))
            .on_action(cx.listener(Self::on_move_line_down))
            .on_action(cx.listener(Self::on_duplicate_line_up))
            .on_action(cx.listener(Self::on_duplicate_line_down))
            .on_action(cx.listener(Self::on_delete_line))
            .on_action(cx.listener(Self::on_doc_start))
            .on_action(cx.listener(Self::on_doc_end))
            .on_action(cx.listener(Self::on_select_doc_start))
            .on_action(cx.listener(Self::on_select_doc_end))
            .on_action(cx.listener(Self::on_page_up))
            .on_action(cx.listener(Self::on_page_down))
            .on_action(cx.listener(Self::on_select_page_up))
            .on_action(cx.listener(Self::on_select_page_down))
            .on_action(cx.listener(Self::on_cycle_element))
            .on_action(cx.listener(Self::on_cycle_element_back))
            // crash-safety backups (M9 slice 6)
            .on_action(cx.listener(Self::on_restore_backup))
            .on_action(cx.listener(Self::on_discard_backup))
            // new documents (M10 slice 2)
            .on_action(cx.listener(Self::on_new_file))
            // projects (M10 slice 2b)
            .on_action(cx.listener(Self::on_open_project))
            .on_action(cx.listener(Self::on_new_project))
            .on_action(cx.listener(Self::on_open_recent))
            .on_action(cx.listener(Self::on_clear_recents))
            // tabs (M10 slice 3b)
            .on_action(cx.listener(Self::on_close_tab))
            .on_action(cx.listener(Self::on_next_tab))
            .on_action(cx.listener(Self::on_prev_tab))
            // panels (M10 slice 3c)
            .on_action(cx.listener(Self::on_toggle_explorer))
            .on_action(cx.listener(Self::on_toggle_agent_panel))
            .on_action(cx.listener(Self::on_toggle_git_panel))
            .on_action(cx.listener(Self::on_open_settings))
            .on_action(cx.listener(Self::on_open_project_settings))
            .on_action(cx.listener(Self::on_toggle_config_files))
            .on_action(cx.listener(Self::on_show_context_menu))
            .on_action(cx.listener(Self::on_focus_chat))
            .on_action(cx.listener(Self::on_chat_newline))
            .on_action(cx.listener(Self::on_quick_open))
            .on_action(cx.listener(Self::on_command_palette))
            .on_action(cx.listener(Self::on_quit))
            .on_action(cx.listener(Self::on_backspace))
            .on_action(cx.listener(Self::on_delete))
            .on_action(cx.listener(Self::on_insert_newline))
            .on_action(cx.listener(Self::on_undo))
            .on_action(cx.listener(Self::on_redo))
            .on_action(cx.listener(Self::on_copy))
            .on_action(cx.listener(Self::on_cut))
            .on_action(cx.listener(Self::on_paste))
            .on_action(cx.listener(Self::on_save))
            .on_action(cx.listener(Self::on_toggle_find))
            .on_action(cx.listener(Self::on_focus_replace))
            .on_action(cx.listener(Self::on_close_find))
            .on_action(cx.listener(Self::on_replace_current))
            .on_action(cx.listener(Self::on_replace_all))
            .on_action(cx.listener(Self::on_toggle_diff))
            .on_action(cx.listener(Self::on_toggle_preview))
            // agent review (M6 slice 3)
            .on_action(cx.listener(Self::on_review_scene))
            .on_action(cx.listener(Self::on_cancel_review))
            .on_action(cx.listener(Self::on_review_scroll_up))
            .on_action(cx.listener(Self::on_review_scroll_down))
            // proposals (M6 slice 4)
            .on_action(cx.listener(Self::on_open_propose))
            .on_action(cx.listener(Self::on_apply_proposal))
            .on_action(cx.listener(Self::on_reject_proposal))
            // review focus (M6 slice 5)
            .on_action(cx.listener(Self::on_open_focus_bar))
            // hunk selection (M6 slice 6)
            .on_action(cx.listener(Self::on_hunk_prev))
            .on_action(cx.listener(Self::on_hunk_next))
            .on_action(cx.listener(Self::on_toggle_hunk))
            // retry (M6 slice 8)
            .on_action(cx.listener(Self::on_retry_agent))
            // language (M6 slice 9)
            .on_action(cx.listener(Self::on_toggle_lang))
            // export (M7 slice 4)
            .on_action(cx.listener(Self::on_export_pdf))
            // project search (M8 slice 2)
            .on_action(cx.listener(Self::on_project_search))
            .on_action(cx.listener(Self::on_search_prev))
            .on_action(cx.listener(Self::on_search_next))
            .when(self.doc.find.open, |bar| bar.child(self.find_bar()))
            .when(self.search.open, |bar| bar.child(self.search_bar()))
            .when(self.propose_bar.open, |bar| bar.child(self.propose_bar()))
            .when(self.focus_bar.open, |bar| bar.child(self.focus_bar()))
            .child(
                div()
                    .flex_1()
                    .flex()
                    // The body row clips its children: a long transcript must
                    // scroll inside the chat panel, never grow the window
                    // (M16 slice 2).
                    .overflow_hidden()
                    // Body row (M6 slice 1): explorer panel (project mode
                    // only) + document area. Fixture mode has no panel, so
                    // the verified full-width spike geometry is unchanged.
                    .when(self.project.is_some() && self.explorer_open, |body| {
                        body.child(self.explorer_panel(cx))
                            .child(self.panel_splitter(panel_resize::Target::Explorer, cx))
                    })
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            // Keep the document column inside the body row;
                            // its editor list already scrolls on its own.
                            .overflow_hidden()
                            .when(self.project.is_some(), |column| {
                                column.child(self.tab_bar(cx))
                            })
                            .when(
                                self.project.is_some() && self.active_is_markdown(),
                                |column| column.child(self.document_toolbar(cx)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .relative()
                                    // Click-to-caret for the buffer list (the diff list is
                            // read-only and ignores clicks in the handler).
                            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_buffer_click))
                            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_buffer_click))
                            .on_mouse_move(cx.listener(Self::on_mouse_drag))
                            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_release))
                            // The diff view replaces the buffer rows while open; the
                            // buffer list keeps its own (idle) scroll state so
                            // closing the diff restores the caret position.
                            .when(self.doc.diff.open, |host| {
                                host.child(
                                    list(self.doc.diff_list_state.clone(), {
                                        let entity = entity.clone();
                                        move |ix: usize, _window: &mut Window, cx: &mut App| {
                                            entity.read(cx).diff_row(ix).into_any_element()
                                        }
                                    })
                                    .size_full(),
                                )
                            })
                            .when(!self.doc.diff.open && self.doc.preview, |host| {
                                host.child(markdown_view::render(
                                    &self.doc.preview_document,
                                    self.lang,
                                ))
                            })
                            .when(!self.doc.diff.open && !self.doc.preview, |host| {
                                host.child(
                                    list(self.doc.list_state.clone(), {
                                        let entity = entity.clone();
                                        move |ix: usize, window: &mut Window, cx: &mut App| {
                                            entity.read(cx).row(ix, window).into_any_element()
                                        }
                                    })
                                    .size_full(),
                                )
                            })
                            // Invisible overlay that registers the platform input
                            // handler for this frame: typing and IME flow into
                            // `EntityInputHandler` while the root has focus.
                            .child(
                                canvas(
                                    |_bounds: Bounds<Pixels>,
                                     _window: &mut Window,
                                     _cx: &mut App| (),
                                    {
                                        let entity = entity.clone();
                                        let focus = focus.clone();
                                        move |bounds: Bounds<Pixels>,
                                              _: (),
                                              window: &mut Window,
                                              cx: &mut App| {
                                            window.handle_input(
                                                &focus,
                                                ElementInputHandler::new(bounds, entity.clone()),
                                                cx,
                                            );
                                        }
                                    },
                                )
                                        .absolute()
                                        .size_full(),
                                    ),
                            ),
                    )
                    // Agent panel (review/proposal or the empty state): right
                    // column of the body row, toggled from the bottom bar.
                    .when(self.agent_panel_open, |body| {
                        body.child(self.panel_splitter(panel_resize::Target::RightPanel, cx))
                            .child(self.agent_panel(cx))
                    }),
            )
            .child(self.bottom_bar(cx))
            .when(self.unsaved_prompt.is_some(), |root| {
                root.child(self.quit_overlay(cx))
            })
            .when(self.settings.is_some(), |root| {
                root.child(self.settings_overlay(cx))
            })
            .when(self.project_settings.is_some(), |root| {
                root.child(self.project_settings_overlay(cx))
            })
            .when(self.palette.is_some(), |root| {
                root.child(self.palette_overlay(cx))
            })
            .when(self.context_menu.is_some(), |root| {
                root.child(self.context_menu_overlay(cx))
            })
            .when(self.rename.is_some(), |root| root.child(self.rename_overlay(cx)))
            .when(self.delete_prompt.is_some(), |root| {
                root.child(self.delete_overlay(cx))
            })
            .when(self.git_commit.is_some(), |root| {
                root.child(self.git_commit_overlay(cx))
            })
            .when(self.git_diff.is_some(), |root| {
                root.child(self.git_diff_overlay(cx))
            })
            .when(self.git_restore.is_some(), |root| {
                root.child(self.git_restore_overlay(cx))
            })
            .when(self.git_branch_prompt.is_some(), |root| {
                root.child(self.git_branch_prompt_overlay(cx))
            })
            .when(self.github_publish.is_some(), |root| {
                root.child(self.github_publish_overlay(cx))
            })
            .when(self.git_sync.is_some(), |root| {
                root.child(self.git_sync_overlay(cx))
            })
            .when(self.github_pr.is_some(), |root| {
                root.child(self.github_pr_overlay(cx))
            })
            .when(self.new_file.is_some(), |root| {
                root.child(self.new_file_overlay(cx))
            })
            .when(self.new_project.is_some(), |root| {
                root.child(self.new_project_overlay(cx))
            })
    }
}

/// Adapter settings for agent reviews, derived from persisted settings or
/// explicit `--agent-*` flags. API keys are never stored here: they are
/// resolved from the OS keychain (or the provider's env fallback) at run time.
#[derive(Clone, Debug)]
struct AgentSettings {
    provider: AiProvider,
    base_url: String,
    model: String,
    /// Simulated mock latency in ms (lets a driver observe the running
    /// state; real providers ignore it).
    latency_ms: u64,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            provider: AiProvider::Mock,
            base_url: LOCAL_DEFAULT_BASE_URL.to_string(),
            model: String::new(),
            latency_ms: 0,
        }
    }
}

/// Resolve a provider key without ever exposing it to settings/dump/logs.
/// One process-wide secret cache (D-040). The OS keychain can ask for access
/// on every read while the binary's signature is not stable, so a session
/// reads each account at most once. The harness and tests keep their own
/// stores; this one is only for the desktop session.
fn secret_cache() -> &'static CachedSecretStore {
    static CACHE: OnceLock<CachedSecretStore> = OnceLock::new();
    CACHE.get_or_init(|| CachedSecretStore::new(Box::new(OsKeychain::new())))
}

/// True when an environment variable is set to a non-empty value. Reading the
/// environment never touches the keychain.
fn secret_env_present(name: &str) -> bool {
    env::var(name)
        .ok()
        .is_some_and(|value| !value.trim().is_empty())
}

fn provider_env_present(provider: AiProvider) -> bool {
    provider.env_var().is_some_and(secret_env_present)
}

/// Status derived from a store answer, with the environment fallback that
/// `resolve_secret` applies when the store has no value or fails.
fn secret_status(result: Result<Option<String>, SecretError>, env_present: bool) -> (bool, String) {
    match result {
        Ok(Some(secret)) if !secret.trim().is_empty() => (true, "keychain".to_string()),
        Ok(_) | Err(_) if env_present => (true, "env".to_string()),
        Ok(_) => (false, String::new()),
        Err(error) => (false, error.to_string()),
    }
}

/// Status for displays that must not read the keychain (D-040). `None` means
/// the session has not resolved this account yet, so the UI shows "not
/// checked" and offers an explicit Check action.
fn cached_secret_status(
    cached: Option<Result<Option<String>, SecretError>>,
    env_present: bool,
) -> Option<(bool, String)> {
    let env_source = || (true, "env".to_string());
    match cached {
        Some(Ok(Some(secret))) if !secret.trim().is_empty() => Some((true, "keychain".to_string())),
        Some(Ok(_)) => Some(if env_present {
            env_source()
        } else {
            (false, String::new())
        }),
        Some(Err(error)) => Some(if env_present {
            env_source()
        } else {
            (false, error.to_string())
        }),
        None => env_present.then(env_source),
    }
}

fn cached_provider_key_status(provider: AiProvider) -> Option<(bool, String)> {
    let Some(account) = provider.secret_account() else {
        return Some((false, String::new()));
    };
    cached_secret_status(secret_cache().peek(account), provider_env_present(provider))
}

fn cached_github_token_status() -> Option<(bool, String)> {
    cached_secret_status(
        secret_cache().peek(GITHUB_TOKEN_ACCOUNT),
        secret_env_present("GITHUB_TOKEN"),
    )
}

/// Keychain first, then the provider's environment fallback. Mock has none.
fn resolve_provider_key(provider: AiProvider) -> Result<Option<String>, String> {
    let Some(account) = provider.secret_account() else {
        return Ok(None);
    };
    let store = secret_cache();
    let env_value = provider
        .env_var()
        .and_then(|name| env::var(name).ok())
        .filter(|value| !value.trim().is_empty());
    let key = narra_core::provider::resolve_secret(store, account, env_value.as_deref())
        .map_err(|error| error.to_string())?;
    if provider.requires_key() && key.is_none() {
        return Err(format!(
            "missing API key for {}: set it in Settings (⌘,) or export {}",
            provider.key(),
            provider.env_var().unwrap_or("the provider key")
        ));
    }
    Ok(key)
}

/// Parse `https://github.com/owner/repo(.git)` or `git@github.com:owner/repo(.git)`
/// into `(owner, repo)`. Returns `None` for anything that is not GitHub.
fn parse_github_remote(url: &str) -> Option<(String, String)> {
    let rest = if let Some(rest) = url.strip_prefix("https://github.com/") {
        rest
    } else if let Some(rest) = url.strip_prefix("http://github.com/") {
        rest
    } else if let Some(rest) = url.strip_prefix("git@github.com:") {
        rest
    } else {
        url.strip_prefix("ssh://git@github.com/")?
    };
    let rest = rest.trim_end_matches('/').trim_end_matches(".git");
    let mut parts = rest.splitn(2, '/');
    let owner = parts.next()?.trim();
    let repo = parts.next()?.trim();
    if owner.is_empty() || repo.is_empty() || repo.contains('/') {
        return None;
    }
    Some((owner.to_string(), repo.to_string()))
}

/// Turn a project title into a conservative GitHub repository name.
fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for character in value.chars() {
        let ascii = match character {
            'á' | 'à' | 'ä' | 'â' | 'ã' | 'Á' | 'À' | 'Ä' | 'Â' | 'Ã' => 'a',
            'é' | 'è' | 'ë' | 'ê' | 'É' | 'È' | 'Ë' | 'Ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' | 'Í' | 'Ì' | 'Ï' | 'Î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' | 'õ' | 'Ó' | 'Ò' | 'Ö' | 'Ô' | 'Õ' => 'o',
            'ú' | 'ù' | 'ü' | 'û' | 'Ú' | 'Ù' | 'Ü' | 'Û' => 'u',
            'ñ' | 'Ñ' => 'n',
            'ç' | 'Ç' => 'c',
            other => other,
        };
        if ascii.is_ascii_alphanumeric() {
            slug.push(ascii.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !slug.is_empty() {
            slug.push('-');
            last_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "narra-project".to_string()
    } else {
        slug
    }
}

fn github_token_status() -> (bool, String) {
    secret_status(
        secret_cache().get(GITHUB_TOKEN_ACCOUNT),
        secret_env_present("GITHUB_TOKEN"),
    )
}

fn resolve_github_token() -> Result<Option<String>, String> {
    let env_value = env::var("GITHUB_TOKEN")
        .ok()
        .filter(|value| !value.trim().is_empty());
    narra_core::provider::resolve_secret(secret_cache(), GITHUB_TOKEN_ACCOUNT, env_value.as_deref())
        .map_err(|error| error.to_string())
}

fn provider_key_status(provider: AiProvider) -> (bool, String) {
    let Some(account) = provider.secret_account() else {
        return (false, String::new());
    };
    secret_status(secret_cache().get(account), provider_env_present(provider))
}

impl AgentSettings {
    fn from_settings(settings: &AppSettings) -> Self {
        let model = if settings.model.trim().is_empty() {
            settings.provider.default_model().to_string()
        } else {
            settings.model.trim().to_string()
        };
        Self {
            provider: settings.provider,
            base_url: settings.resolved_base_url(),
            model,
            latency_ms: 0,
        }
    }

    fn provider_key(&self) -> Result<Option<String>, String> {
        resolve_provider_key(self.provider)
    }

    /// Build the adapter for one run. Mock stays offline; OpenCode Zen/Go
    /// use their OpenAI chat-completions endpoints and their own keys.
    fn build_adapter(&self) -> Result<Box<dyn ModelAdapter>, String> {
        if self.provider == AiProvider::Mock {
            return Ok(Box::new(MockAdapter::with_latency(Duration::from_millis(
                self.latency_ms,
            ))));
        }
        let model = self.model.trim();
        if model.is_empty() {
            return Err(format!(
                "the {} provider needs a model id",
                self.provider.key()
            ));
        }
        let api_key = self.provider_key()?;
        let mut extra_headers = Vec::new();
        if matches!(self.provider, AiProvider::OpenCode | AiProvider::OpenCodeGo) {
            extra_headers.push((
                "User-Agent".to_string(),
                "narra-desktop/0.0 (Narra)".to_string(),
            ));
            if self.provider == AiProvider::OpenCodeGo {
                // OpenCode Go asks clients for a stable session id for
                // routing/caching; this run is one conversation.
                let session = format!(
                    "narra-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|duration| duration.as_nanos())
                        .unwrap_or(0)
                );
                extra_headers.push(("x-opencode-session".to_string(), session));
            }
        }
        let base_url = self
            .provider
            .fixed_base_url()
            .map(str::to_string)
            .unwrap_or_else(|| self.base_url.clone());
        Ok(Box::new(OpenAiCompatAdapter::new(OpenAiCompatConfig {
            base_url,
            api_key,
            model: model.to_string(),
            timeout: AGENT_TIMEOUT,
            extra_headers,
        })))
    }
}

struct Args {
    fixture: String,
    /// Narra project root (`--project`); mutually exclusive with an explicit
    /// `--fixture`. The default fixture stays valid when neither is given.
    project: Option<String>,
    steps: usize,
    find_steps: usize,
    diff_steps: usize,
    report_after_ms: Option<u64>,
    dump_after_ms: Option<u64>,
    agent: AgentSettings,
    /// An explicit `--agent-*` flag wins over persisted settings for this
    /// run (headless verification and one-off real-provider tests).
    agent_explicit: bool,
    /// UI language (M6 slice 9): `--lang en|es`; default from persisted
    /// settings, then the environment (`LC_ALL`/`LANG`), English otherwise.
    lang: Lang,
    /// True when `--lang` was passed, so persisted settings do not override it.
    lang_explicit: bool,
    /// Append the performance metrics to the status bar (on-screen debugging;
    /// the report/dump always carry them).
    debug_metrics: bool,
}

fn parse_args() -> Result<Args, String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut fixture = "fixtures/screenplay-1000.fountain".to_string();
    let mut fixture_given = false;
    let mut project: Option<String> = None;
    let mut steps = 0usize;
    let mut find_steps = 0usize;
    let mut diff_steps = 0usize;
    let mut report_after_ms = None;
    let mut dump_after_ms = None;
    let mut agent = AgentSettings::default();
    let mut agent_explicit = false;
    let mut base_url_given = false;
    let mut lang = Lang::detect();
    let mut lang_explicit = false;
    let mut debug_metrics = false;

    let mut i = 0;
    while i < args.len() {
        let key = args[i].as_str();
        // Boolean flags first: they take no value.
        if key == "--debug-metrics" {
            debug_metrics = true;
            i += 1;
            continue;
        }
        let value = args.get(i + 1).ok_or(format!("{key} requiere un valor"))?;
        match key {
            "--fixture" => {
                fixture = value.clone();
                fixture_given = true;
            }
            // M6 slice 1: open a real Narra project through narra-core
            // (explorer panel, ⌘S save). Mutually exclusive with --fixture so
            // the verified spike driver can never run against a project by
            // accident (and vice versa).
            "--project" => project = Some(value.clone()),
            "--steps" => {
                steps = value
                    .parse()
                    .map_err(|_| format!("--steps inválido: {value}"))?
            }
            "--find-steps" => {
                find_steps = value
                    .parse()
                    .map_err(|_| format!("--find-steps inválido: {value}"))?
            }
            "--diff-steps" => {
                diff_steps = value
                    .parse()
                    .map_err(|_| format!("--diff-steps inválido: {value}"))?
            }
            "--report-after-ms" => {
                report_after_ms = Some(
                    value
                        .parse()
                        .map_err(|_| format!("--report-after-ms inválido: {value}"))?,
                )
            }
            "--dump-after-ms" => {
                dump_after_ms = Some(
                    value
                        .parse()
                        .map_err(|_| format!("--dump-after-ms inválido: {value}"))?,
                )
            }
            // Agent review adapter (M6 slice 3). Key never from argv.
            "--agent-adapter" => {
                agent.provider = AiProvider::parse(value).ok_or_else(|| {
                    format!(
                        "--agent-adapter inválido: {value} (mock|openai-compat|opencode|opencode-go)"
                    )
                })?;
                agent_explicit = true;
            }
            "--agent-base-url" => {
                agent.base_url = value.clone();
                base_url_given = true;
                agent_explicit = true;
            }
            "--agent-model" => {
                agent.model = value.clone();
                agent_explicit = true;
            }
            "--agent-latency" => {
                agent.latency_ms = value
                    .parse()
                    .map_err(|_| format!("--agent-latency inválido: {value}"))?;
                agent_explicit = true;
            }
            "--lang" => {
                lang = Lang::from_key(value)
                    .ok_or_else(|| format!("--lang inválido: {value} (en|es)"))?;
                lang_explicit = true;
            }
            other => return Err(format!("argumento desconocido: {other}")),
        }
        i += 2;
    }

    if project.is_some() && fixture_given {
        return Err("usa --fixture o --project, no ambos".to_string());
    }
    if !base_url_given {
        agent.base_url = agent
            .provider
            .fixed_base_url()
            .unwrap_or(LOCAL_DEFAULT_BASE_URL)
            .to_string();
    }

    Ok(Args {
        fixture,
        project,
        steps,
        find_steps,
        diff_steps,
        report_after_ms,
        dump_after_ms,
        agent,
        agent_explicit,
        lang,
        lang_explicit,
        debug_metrics,
    })
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let started = Instant::now();

    // M10 slice 2b: the per-user recent list feeds File → Open Recent. A
    // project opened through `--project` counts as opened and moves to the
    // front before the first menu is installed.
    let mut recents = RecentProjects::load();

    // M6 slice 1: `--project` goes through narra-core (validated manifest,
    // boundary-checked IO); `--fixture` is the untouched spike path.
    let project = match &args.project {
        Some(root) => Some(
            open_project(Path::new(root))
                .map_err(|error| format!("no se pudo abrir el proyecto {root}: {error}"))?,
        ),
        None => None,
    };
    if let Some(project) = project.as_ref() {
        recents.record(project.root());
        recents.save();
    }

    let (content, doc_path) = match &project {
        Some(project) => {
            let path = project.manifest().default_screenplay_path.clone();
            let text = match project.read_document(&path) {
                Ok(text) => text,
                // A project created without a screenplay opens as a blank
                // document; ⌘S creates it through the same safe IO.
                Err(ProjectError::Document(DocumentError::Io { source, .. }))
                    if source.kind() == std::io::ErrorKind::NotFound =>
                {
                    String::new()
                }
                Err(error) => return Err(format!("no se pudo leer {path}: {error}")),
            };
            (text, path)
        }
        None => (
            fs::read_to_string(&args.fixture)
                .map_err(|e| format!("no se pudo leer {}: {e}", args.fixture))?,
            args.fixture.clone(),
        ),
    };
    let fixture_read_ms = duration_ms(started.elapsed());

    // The explorer's document list comes from the core: sorted, hidden state
    // (.git/.narra/dot-files) excluded, symlinks not followed.
    let explorer = match &project {
        Some(project) => project
            .list_documents()
            .map_err(|error| format!("no se pudo listar el proyecto: {error}"))?,
        None => Vec::new(),
    };
    let entities = project
        .as_ref()
        .and_then(|project| EntityIndex::build(project).ok())
        .unwrap_or_default();

    let scene_count = content
        .lines()
        .filter(|line| is_scene_heading(line))
        .count();
    // The scene gate is fixture-specific (the spike measures scene
    // navigation); a real project may legitimately start with a blank
    // screenplay, so it must not block opening the workspace.
    if scene_count == 0 && project.is_none() {
        return Err(format!(
            "no se detectaron escenas en {} (¿es un Fountain?)",
            args.fixture
        ));
    }

    // Fixture mode must keep reporting exactly `args.fixture` (existing
    // scripts parse it); project mode reports the project root.
    let metrics_label = args.project.clone().unwrap_or_else(|| args.fixture.clone());

    let metrics = Rc::new(RefCell::new(Metrics::new(
        started,
        &metrics_label,
        fixture_read_ms,
        content.lines().count(),
        scene_count,
        ProbeSteps {
            scroll: args.steps,
            find: args.find_steps,
            diff: args.diff_steps,
        },
    )));

    let steps = args.steps;
    let find_steps = args.find_steps;
    let diff_steps = args.diff_steps;
    let report_after_ms = args.report_after_ms;
    let dump_after_ms = args.dump_after_ms;
    // Persisted provider settings are the default; explicit CLI flags win
    // for a single run (headless tests and one-off real-provider checks).
    let app_settings = AppSettings::load();
    let settings_error = app_settings.error.clone();
    let agent_settings = if args.agent_explicit {
        args.agent
    } else {
        AgentSettings::from_settings(&app_settings)
    };
    let lang = if args.lang_explicit {
        args.lang
    } else {
        app_settings.language.resolved()
    };
    let initial_show_config_files = app_settings.show_config_files;
    let initial_word_wrap = app_settings.word_wrap;
    theme::set_mode(app_settings.theme);
    theme::set_font_size(app_settings.font_size);
    theme::set_line_numbers(app_settings.line_numbers);
    let debug_metrics_flag = args.debug_metrics;
    Application::new()
        .with_assets(assets::Assets)
        .run(move |cx: &mut App| {
            cx.activate(true);
            cx.bind_keys([
                // navigation and quit
                KeyBinding::new("cmd-g", NextScene, None),
                KeyBinding::new("cmd-shift-g", PrevScene, None),
                KeyBinding::new("cmd-q", Quit, None),
                KeyBinding::new("cmd-,", OpenSettings, None),
                KeyBinding::new("cmd-shift-,", OpenProjectSettings, None),
                KeyBinding::new("shift-f10", ShowContextMenu, None),
                KeyBinding::new("cmd-p", QuickOpen, None),
                KeyBinding::new("cmd-shift-p", CommandPalette, None),
                // caret movement
                KeyBinding::new("left", MoveLeft, None),
                KeyBinding::new("right", MoveRight, None),
                KeyBinding::new("up", MoveUp, None),
                KeyBinding::new("down", MoveDown, None),
                KeyBinding::new("home", MoveHome, None),
                KeyBinding::new("end", MoveEnd, None),
                // selection
                KeyBinding::new("shift-left", SelectLeft, None),
                KeyBinding::new("shift-right", SelectRight, None),
                KeyBinding::new("shift-up", SelectUp, None),
                KeyBinding::new("shift-down", SelectDown, None),
                KeyBinding::new("shift-home", SelectHome, None),
                KeyBinding::new("shift-end", SelectEnd, None),
                KeyBinding::new("cmd-a", SelectAll, None),
                // word/line editing and document navigation (M9 slice 2, VSCode
                // parity: option=word, cmd=line/document, pageup/pagedown).
                KeyBinding::new("alt-left", WordLeft, None),
                KeyBinding::new("alt-right", WordRight, None),
                KeyBinding::new("alt-shift-left", SelectWordLeft, None),
                KeyBinding::new("alt-shift-right", SelectWordRight, None),
                KeyBinding::new("alt-backspace", DeleteWordLeft, None),
                KeyBinding::new("alt-delete", DeleteWordRight, None),
                KeyBinding::new("alt-up", MoveLineUp, None),
                KeyBinding::new("alt-down", MoveLineDown, None),
                KeyBinding::new("alt-shift-up", DuplicateLineUp, None),
                KeyBinding::new("alt-shift-down", DuplicateLineDown, None),
                KeyBinding::new("cmd-shift-k", DeleteLine, None),
                KeyBinding::new("cmd-up", DocStart, None),
                KeyBinding::new("cmd-down", DocEnd, None),
                KeyBinding::new("cmd-shift-up", SelectDocStart, None),
                KeyBinding::new("cmd-shift-down", SelectDocEnd, None),
                KeyBinding::new("cmd-left", MoveHome, None),
                KeyBinding::new("cmd-right", MoveEnd, None),
                KeyBinding::new("cmd-shift-left", SelectHome, None),
                KeyBinding::new("cmd-shift-right", SelectEnd, None),
                KeyBinding::new("pageup", PageUp, None),
                KeyBinding::new("pagedown", PageDown, None),
                KeyBinding::new("shift-pageup", SelectPageUp, None),
                KeyBinding::new("shift-pagedown", SelectPageDown, None),
                // screenplay elements (M9 slice 5): Tab/⇧Tab cycle the caret
                // line's forceable element type (scene heading, action,
                // character, transition) via Fountain forced markers.
                KeyBinding::new("tab", CycleElement, None),
                KeyBinding::new("shift-tab", CycleElementBack, None),
                // crash-safety backups (M9 slice 6): restore the newest backup
                // into the buffer (⌘S decides whether it becomes canonical) or
                // discard it.
                KeyBinding::new("cmd-shift-b", RestoreBackup, None),
                KeyBinding::new("cmd-shift-d", DiscardBackup, None),
                // new documents (M10 slice 2): ⌘N opens the kind/name/format
                // dialog; the file is created in the canonical folder.
                KeyBinding::new("cmd-n", NewFile, None),
                // projects (M10 slice 2b): ⌘O picks a project folder; ⇧⌘N opens
                // the new-project dialog (native save panel for the location).
                KeyBinding::new("cmd-o", OpenProject, None),
                KeyBinding::new("cmd-shift-n", NewProject, None),
                // tabs (M10 slice 3b): ⌘W closes the active tab, ⌘⇧[/] walk the
                // strip; unsaved tabs get the save/discard/cancel prompt.
                KeyBinding::new("cmd-w", CloseTab, None),
                KeyBinding::new("cmd-shift-[", PrevTab, None),
                KeyBinding::new("cmd-shift-]", NextTab, None),
                // panels (M10 slice 3c), approved D-026 additions.
                KeyBinding::new("cmd-b", ToggleExplorer, None),
                KeyBinding::new("cmd-shift-a", ToggleAgentPanel, None),
                KeyBinding::new("cmd-alt-g", ToggleGitPanel, None),
                // editing
                KeyBinding::new("backspace", Backspace, None),
                KeyBinding::new("delete", DeleteText, None),
                KeyBinding::new("enter", InsertNewline, None),
                KeyBinding::new("cmd-z", Undo, None),
                KeyBinding::new("cmd-shift-z", Redo, None),
                KeyBinding::new("cmd-c", CopyText, None),
                KeyBinding::new("cmd-x", CutText, None),
                KeyBinding::new("cmd-v", PasteText, None),
                // save through the project IO (M6 slice 1; no-op notice in
                // fixture mode, where there is no project to write to)
                KeyBinding::new("cmd-s", Save, None),
                // find / replace (slice 3)
                KeyBinding::new("cmd-f", ToggleFind, None),
                KeyBinding::new("cmd-alt-f", FocusReplace, None),
                KeyBinding::new("escape", CloseFind, None),
                KeyBinding::new("cmd-enter", ReplaceCurrent, None),
                KeyBinding::new("cmd-alt-enter", ReplaceAll, None),
                // diff view (slice 3, M1-3b): cmd-d toggles, cmd-g/shift-g walk
                // hunks while it is open.
                KeyBinding::new("cmd-d", ToggleDiff, None),
                KeyBinding::new("cmd-alt-p", TogglePreview, None),
                // agent review (M6 slice 3): cmd-r reviews the caret's scene,
                // cmd-. cancels the run. The review panel scrolls with
                // ctrl-up/down when no search results are on screen (M9 slice 2
                // freed option arrows for word/line editing).
                KeyBinding::new("cmd-r", ReviewScene, None),
                KeyBinding::new("cmd-.", CancelReview, None),
                // proposals (M6 slice 4): cmd-p opens the instruction bar,
                // cmd-shift-enter applies the ready proposal (two-step: arm,
                // then apply), cmd-backspace rejects it.
                KeyBinding::new("cmd-k", OpenPropose, None),
                // chat (M12): cmd-l focuses the composer; shift-enter inserts
                // a newline while enter sends.
                KeyBinding::new("cmd-l", FocusChat, None),
                KeyBinding::new("shift-enter", ChatNewline, None),
                KeyBinding::new("cmd-shift-enter", ApplyProposal, None),
                KeyBinding::new("cmd-backspace", RejectProposal, None),
                // review focus (M6 slice 5): cmd-shift-r opens the focus bar;
                // enter commits the focus and runs the review.
                KeyBinding::new("cmd-shift-r", OpenFocusBar, None),
                // hunk selection (M6 slice 6): option-arrows walk the record's
                // hunks while a proposal is visible (M9 slice 2 scopes this in
                // the WordLeft/WordRight handlers; the buffer gets word
                // movement otherwise); alt-enter toggles the hunk under the
                // cursor and apply gets exactly the selected subset.
                KeyBinding::new("alt-enter", ToggleHunk, None),
                // retry (M6 slice 8): cmd-alt-r re-runs the visible flow's last
                // task with the same stored inputs (only if it failed or was
                // cancelled).
                KeyBinding::new("cmd-alt-r", RetryAgent, None),
                // language (M6 slice 9): cmd-alt-l toggles EN/ES live.
                KeyBinding::new("cmd-alt-l", ToggleLang, None),
                // export (M7 slice 4): cmd-e writes exports/<screenplay>.pdf.
                KeyBinding::new("cmd-e", ExportPdf, None),
                // project search (M8 slice 2): cmd-shift-f searches every
                // project document; ctrl-up/down move through the results (and
                // scroll the agent panel when there are none, M9 slice 2).
                KeyBinding::new("cmd-shift-f", ProjectSearch, None),
                KeyBinding::new("ctrl-up", SearchPrev, None),
                KeyBinding::new("ctrl-down", SearchNext, None),
            ]);

            cx.observe_keystrokes({
                let metrics = metrics.clone();
                move |_, _, _| metrics.borrow_mut().mark_keystroke(Instant::now())
            })
            .detach();

            // Native macOS menus (M10 slice 1), localized with the startup
            // language and rebuilt by the ⌘⌥L toggle. The recents submenu is
            // part of the same rebuild (M10 slice 2b).
            install_menus(lang, recents.paths(), cx);

            let entity = cx.new(|cx| Editor {
                doc: DocumentState::new(doc_path, &content),
                tabs: vec![Tab {
                    document: None,
                    preview: false,
                }],
                active_tab: 0,
                autosave_started: false,
                unsaved_prompt: None,
                debug_metrics: debug_metrics_flag,
                new_file: None,
                new_project: None,
                new_project_generation: 0,
                recents,
                workspace_generation: 0,
                window_title: "Narra".to_string(),
                git: GitStatus::default(),
                git_pending: false,
                git_request: 0,
                git_refreshed_at: None,
                explorer_open: true,
                agent_panel_open: false,
                git_panel_open: false,
                explorer_w: panel_resize::EXPLORER_DEFAULT_W,
                right_panel_w: panel_resize::RIGHT_PANEL_DEFAULT_W,
                panel_drag: None,
                focus_handle: cx.focus_handle(),
                wrap_width: px(800.0),
                metrics: metrics.clone(),
                probe_left: steps,
                probe_cursor: 1,
                probe_scheduled: false,
                find_probe_left: find_steps,
                find_probe_cursor: 0,
                diff_probe_left: diff_steps,
                diff_probe_total: diff_steps,
                project,
                explorer,
                entities,
                collapsed: HashSet::new(),
                notice: None,
                lang,
                export: ExportState::default(),
                search: SearchState::default(),
                agent: AgentState::new(),
                agent_cancel: None,
                agent_generation: 0,
                agent_settings: agent_settings.clone(),
                app_settings,
                settings: None,
                project_settings: None,
                palette: None,
                context_menu: None,
                context_target: None,
                git_history: Vec::new(),
                git_history_loading: false,
                git_history_error: String::new(),
                git_history_stale: false,
                git_commit: None,
                git_diff: None,
                git_restore: None,
                git_branches: Vec::new(),
                git_branches_loading: false,
                git_branches_error: String::new(),
                git_branches_stale: false,
                git_branch_prompt: None,
                git_remotes: Vec::new(),
                git_remotes_loading: false,
                git_remotes_error: String::new(),
                git_remotes_stale: false,
                github_publish: None,
                git_sync: None,
                github_pr: None,
                git_busy: false,
                right_tab: RightTab::Chat,
                chat: ChatUiState::new(ChatMode::Plan),
                chat_generation: 0,
                chat_cancel: None,
                chat_history: Vec::new(),
                chat_history_open: false,
                chat_scroll: ScrollHandle::new(),
                chat_scroll_drag: None,
                chat_markdown: RefCell::new(HashMap::new()),
                rename: None,
                delete_prompt: None,
                show_config_files: initial_show_config_files,
                word_wrap: initial_word_wrap,
                propose: ProposeView::new(),
                propose_bar: ProposeBar::default(),
                focus_bar: FocusBar::default(),
                review_focus: String::new(),
            });
            let bounds = Bounds::centered(None, size(px(1100.0), px(780.0)), cx);
            let window = match cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |_, _| entity.clone(),
            ) {
                Ok(window) => window,
                Err(err) => {
                    eprintln!("no se pudo abrir la ventana: {err}");
                    std::process::exit(1);
                }
            };

            window
                .update(cx, |view, window, cx| {
                    // M6 slice 9: the reducers follow the startup language.
                    view.agent.lang = lang;
                    view.propose.lang = lang;
                    if let Some(error) = settings_error {
                        view.notice =
                            Some(view.tf("notice.settings_invalid", &[("error", &error)]));
                    }
                    // M9 slice 6: periodic crash-safety backups.
                    view.start_autosave(window, cx);
                    window.focus(&view.focus_handle(cx));
                })
                .ok();

            // Timed self-report: lets an automated driver capture keystroke
            // metrics without relying on the window keeping focus.
            if let Some(ms) = report_after_ms {
                let metrics = metrics.clone();
                cx.spawn(async move |cx| {
                    Timer::after(Duration::from_millis(ms)).await;
                    println!("{}", metrics.borrow().report());
                    cx.update(|cx| cx.quit()).ok();
                })
                .detach();
            }

            // Timed buffer dump: scripted keystroke verification.
            if let Some(ms) = dump_after_ms {
                cx.spawn(async move |cx| {
                    Timer::after(Duration::from_millis(ms)).await;
                    cx.update(|cx| {
                        entity.update(cx, |view, _| println!("{}", view.dump_json()));
                        cx.quit();
                    })
                    .ok();
                })
                .detach();
            }
        });

    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            eprintln!(
                "uso: narra-app [--fixture <ruta> | --project <ruta>] [--steps <n>] [--report-after-ms <ms>] [--dump-after-ms <ms>]\n      [--agent-adapter mock|openai-compat|opencode|opencode-go] [--agent-model <nombre>] [--agent-base-url <url>] [--agent-latency <ms>]"
            );
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_cache_keys_are_content_stable() {
        assert_eq!(markdown_key("hola"), markdown_key("hola"));
        assert_ne!(markdown_key("hola"), markdown_key("hola "));
    }

    #[test]
    fn window_title_uses_the_project_name() {
        assert_eq!(
            project_window_title(Some("Luces de Invierno")),
            "Luces de Invierno — Narra"
        );
        assert_eq!(project_window_title(Some("  ")), "Narra");
        assert_eq!(project_window_title(None), "Narra");
    }

    #[test]
    fn secret_status_prefers_a_stored_key_over_the_environment() {
        assert_eq!(
            secret_status(Ok(Some("sk-stored".to_string())), true),
            (true, "keychain".to_string())
        );
        assert_eq!(
            secret_status(Ok(Some("sk-stored".to_string())), false),
            (true, "keychain".to_string())
        );
    }

    #[test]
    fn secret_status_falls_back_to_the_environment_and_reports_absence_clearly() {
        assert_eq!(secret_status(Ok(None), true), (true, "env".to_string()));
        assert_eq!(secret_status(Ok(None), false), (false, String::new()));
        assert_eq!(
            secret_status(Ok(Some("   ".to_string())), false),
            (false, String::new())
        );
        let denied = SecretError::AccessDenied {
            detail: "denied".to_string(),
        };
        assert_eq!(
            secret_status(Err(denied.clone()), true),
            (true, "env".to_string())
        );
        assert!(
            !secret_status(Err(denied), false).0,
            "a failed read is not treated as a present key"
        );
    }

    #[test]
    fn cached_status_distinguishes_unknown_from_absent() {
        assert_eq!(cached_secret_status(None, false), None, "not checked yet");
        assert_eq!(
            cached_secret_status(None, true),
            Some((true, "env".to_string()))
        );
        assert_eq!(
            cached_secret_status(Some(Ok(None)), false),
            Some((false, String::new()))
        );
        assert_eq!(
            cached_secret_status(Some(Ok(Some("sk-stored".to_string()))), true),
            Some((true, "keychain".to_string())),
            "the keychain answer wins over the environment for display"
        );
    }

    #[test]
    fn scene_number_at_finds_the_scene_under_the_caret() {
        let scenes = vec![2, 7, 11];
        // On or after a heading, up to the line before the next heading.
        assert_eq!(scene_number_at(&scenes, 0), None, "before every scene");
        assert_eq!(
            scene_number_at(&scenes, 1),
            None,
            "before the first heading"
        );
        assert_eq!(scene_number_at(&scenes, 2), Some(1));
        assert_eq!(scene_number_at(&scenes, 6), Some(1));
        assert_eq!(scene_number_at(&scenes, 7), Some(2));
        assert_eq!(scene_number_at(&scenes, 100), Some(3), "past the last");
        assert_eq!(scene_number_at(&[], 4), None, "no scenes at all");
    }

    #[test]
    fn record_edit_pushes_plain_edits_separately() {
        let mut stack = Vec::new();
        record_edit(&mut stack, false, 0..0, 0..1, "a", String::new());
        record_edit(&mut stack, false, 1..1, 1..2, "b", String::new());
        assert_eq!(stack.len(), 2);
        assert!(!stack[0].composition && !stack[1].composition);
        assert_eq!(stack[1].new_text, "b");
    }

    #[test]
    fn record_edit_merges_composition_updates_into_one_entry() {
        let mut stack = Vec::new();
        // setMarkedText inserts the dead-key base at the caret (5..5).
        record_edit(&mut stack, true, 5..5, 5..6, "e", String::new());
        // The IME updates the marked span (5..6) with the composed char (two
        // UTF-8 bytes), then commits it over the same span.
        record_edit(&mut stack, true, 5..6, 5..7, "\u{e9}", String::new());
        record_edit(&mut stack, true, 5..7, 5..7, "\u{e9}", String::new());
        assert_eq!(stack.len(), 1);
        let merged = &stack[0];
        assert!(merged.composition);
        assert_eq!(merged.range, 5..7);
        assert_eq!(merged.new_text, "\u{e9}");
        // One undo restores the full pre-composition state.
        assert_eq!(merged.old_text, "");
    }

    #[test]
    fn record_edit_keeps_pre_composition_text_for_undo() {
        let mut stack = Vec::new();
        // Composition over a selection: the marked span started as "old".
        record_edit(&mut stack, true, 5..8, 5..6, "e", "old".into());
        record_edit(&mut stack, true, 5..6, 5..7, "\u{e9}", String::new());
        assert_eq!(stack.len(), 1);
        assert_eq!(stack[0].old_text, "old");
        assert_eq!(stack[0].new_text, "\u{e9}");
        assert_eq!(stack[0].range, 5..7);
    }

    #[test]
    fn record_edit_does_not_merge_across_ranges() {
        let mut stack = Vec::new();
        record_edit(&mut stack, true, 0..0, 0..1, "x", String::new());
        // A composition edit at a different span opens a fresh entry rather
        // than corrupting the first one.
        record_edit(&mut stack, true, 9..9, 9..10, "y", String::new());
        assert_eq!(stack.len(), 2);
        assert_eq!(stack[0].new_text, "x");
        assert_eq!(stack[1].new_text, "y");
    }

    #[test]
    fn record_edit_stops_merging_after_session_ends() {
        let mut stack = Vec::new();
        record_edit(&mut stack, true, 0..0, 0..1, "x", String::new());
        // After unmark/commit cleared `composing`, the next edit must not
        // merge even though the last entry is still flagged as composition.
        record_edit(&mut stack, false, 1..1, 1..2, "y", String::new());
        assert_eq!(stack.len(), 2);
        assert_eq!(stack[1].new_text, "y");
    }

    #[test]
    fn line_local_range_clamps_ranges_from_other_lines() {
        let line = 10..20;
        assert_eq!(line_local_range(&(10..20), &line), Some(0..10));
        // A selection that starts on an earlier line clamps to column 0.
        assert_eq!(line_local_range(&(0..15), &line), Some(0..5));
        // A selection that continues past the line clamps to its end.
        assert_eq!(line_local_range(&(15..30), &line), Some(5..10));
        // A selection that begins after the line (or ends before it) does
        // not touch this row at all.
        assert_eq!(line_local_range(&(20..30), &line), None);
        assert_eq!(line_local_range(&(0..10), &line), None);
        assert_eq!(line_local_range(&(10..10), &line), None);
        // The old `range.start - line.start` underflowed here.
        assert_eq!(line_local_range(&(0..30), &line), Some(0..10));
    }

    #[test]
    fn wrap_row_starts_splits_at_the_wrap_width() {
        assert_eq!(wrap_row_starts(&[], 100.0), vec![0]);
        // Everything fits on one row.
        assert_eq!(wrap_row_starts(&[(0, 0.0), (3, 10.0)], 100.0), vec![0]);
        // Crossing glyphs open second visual row at wrap boundary.
        // Glyphs at x=0, 10, 20, 30 with wrap_width=15:
        // - x=0: row 0 starts at 0
        // - x=10: still row 0 (10-0 < 15)
        // - x=20: wraps (20-0 >= 15), new row at index 6
        // - x=30: same row (30-20 < 15)
        let glyphs = [(0, 0.0), (3, 10.0), (6, 20.0), (9, 30.0)];
        assert_eq!(wrap_row_starts(&glyphs, 15.0), vec![0, 6]);
        // A repeated glyph index (combining mark) does not open a new row.
        let glyphs = [(0, 0.0), (0, 8.0), (2, 20.0)];
        assert_eq!(wrap_row_starts(&glyphs, 10.0), vec![0, 2]);
    }

    // visual_row_starts tests require a Window, so they're integration-tested
    // via the live click verification (B-2) and the wrapped-caret live run below.

    #[test]
    fn git_state_has_a_letter_and_a_distinct_color() {
        assert_eq!(git_letter(WorktreeStatus::Modified), "M");
        assert_eq!(git_letter(WorktreeStatus::Untracked), "U");
        assert_eq!(git_letter(WorktreeStatus::Conflicted), "!");
        assert_eq!(git_letter(WorktreeStatus::Unmodified), "");
        assert_ne!(
            git_color(WorktreeStatus::Modified),
            git_color(WorktreeStatus::Deleted)
        );
        assert_eq!(
            worktree_status_key(WorktreeStatus::Conflicted),
            "conflicted"
        );
    }

    #[test]
    fn language_names_follow_the_document_extension() {
        assert_eq!(language_name("screenplay/main.fountain"), "Fountain");
        assert_eq!(language_name("notes/a.md"), "Markdown");
        assert_eq!(language_name("agents.yml"), "YAML");
        assert_eq!(language_name("README"), "Text");
    }

    #[test]
    fn tab_navigation_wraps_and_handles_one_tab() {
        assert_eq!(next_tab_index(0, 3, true), Some(1));
        assert_eq!(next_tab_index(2, 3, true), Some(0));
        assert_eq!(next_tab_index(0, 3, false), Some(2));
        assert_eq!(next_tab_index(1, 3, false), Some(0));
        assert_eq!(next_tab_index(0, 1, true), None);
        assert_eq!(next_tab_index(3, 3, true), None);
    }

    #[test]
    fn closing_a_tab_picks_the_right_neighbour_or_left_at_the_end() {
        assert_eq!(close_target_index(0, 3), Some(1));
        assert_eq!(close_target_index(1, 3), Some(2));
        assert_eq!(close_target_index(2, 3), Some(1));
        assert_eq!(close_target_index(0, 1), None);
        assert_eq!(close_target_index(1, 1), None);
    }

    #[test]
    fn document_state_seeds_the_save_and_diff_base() {
        let doc = DocumentState::new("screenplay/main.fountain".to_string(), "uno\ndos\n");
        assert_eq!(doc.path, "screenplay/main.fountain");
        assert_eq!(doc.base_text, "uno\ndos\n");
        assert_eq!(doc.buffer.line_count(), 3);
        assert_eq!(doc.diff.base_lines, vec!["uno", "dos", ""]);
        assert!(!doc.is_dirty());
    }

    #[test]
    fn scroll_state_is_owned_by_each_document() {
        let first = DocumentState::new("a.fountain".to_string(), "1\n2\n3\n4\n5");
        let second = DocumentState::new("b.fountain".to_string(), "1\n2\n3\n4\n5");
        first.list_state.scroll_to(gpui::ListOffset {
            item_ix: 3,
            offset_in_item: px(0.0),
        });
        assert_eq!(first.list_state.logical_scroll_top().item_ix, 3);
        assert_eq!(
            second.list_state.logical_scroll_top().item_ix,
            0,
            "one document's scroll never leaks into another"
        );
    }

    #[test]
    fn dirty_is_per_document_and_saving_only_snapshots_that_document() {
        let mut first = DocumentState::new("a.md".to_string(), "a");
        let mut second = DocumentState::new("b.md".to_string(), "b");

        first.revision += 1;
        assert!(first.is_dirty());
        assert!(!second.is_dirty(), "a separate document stays clean");

        first.saved_revision = first.revision;
        assert!(!first.is_dirty());

        second.revision += 1;
        assert!(second.is_dirty());
        assert!(!first.is_dirty(), "saving one never cleans another");

        // Loading replaces both sides of the pair for that document only.
        first.revision = 0;
        first.saved_revision = 0;
        assert!(!first.is_dirty());
        assert!(second.is_dirty());
    }

    #[test]
    fn quick_open_ranks_filename_prefix_matches_first() {
        let paths = vec![
            "notes/decisions.md".to_string(),
            "bible/desire.md".to_string(),
            "screenplay/main.fountain".to_string(),
            "notes/old-decisions.md".to_string(),
        ];
        assert_eq!(
            filter_palette_paths(paths.clone(), "dec"),
            vec![
                "notes/decisions.md".to_string(),
                "notes/old-decisions.md".to_string()
            ]
        );
        assert_eq!(
            filter_palette_paths(paths.clone(), "des"),
            vec!["bible/desire.md".to_string()]
        );
        assert_eq!(
            filter_palette_paths(paths, "MAIN"),
            vec!["screenplay/main.fountain".to_string()]
        );
        assert!(filter_palette_paths(Vec::new(), "x").is_empty());
    }

    #[test]
    fn path_names_split_into_parent_stem_and_extension() {
        assert_eq!(
            split_path_name("notes/decisions.md"),
            ("notes", "decisions", ".md")
        );
        assert_eq!(
            split_path_name("screenplay/S01E01.fountain"),
            ("screenplay", "S01E01", ".fountain")
        );
        assert_eq!(split_path_name("README"), ("", "README", ""));
        assert_eq!(split_path_name("notes/.hidden"), ("notes", ".hidden", ""));
    }

    #[test]
    fn added_file_diff_marks_every_line_as_new() {
        let diff = added_file_diff("notes/new.md", "uno\ndos\n");
        assert!(diff.starts_with("--- /dev/null\n+++ b/notes/new.md\n"));
        assert!(diff.contains("@@ -0,0 +1,2 @@"));
        assert!(diff.contains("+uno\n+dos\n"));
        assert!(added_file_diff("empty.md", "").contains("@@ -0,0 +0,0 @@"));
    }

    #[test]
    fn github_remote_parsing_and_slugify_are_conservative() {
        assert_eq!(
            parse_github_remote("https://github.com/example/narra-fixture.git"),
            Some(("example".to_string(), "narra-fixture".to_string()))
        );
        assert_eq!(
            parse_github_remote("git@github.com:example/narra-fixture.git"),
            Some(("example".to_string(), "narra-fixture".to_string()))
        );
        assert_eq!(
            parse_github_remote("ssh://git@github.com/example/narra-fixture"),
            Some(("example".to_string(), "narra-fixture".to_string()))
        );
        assert_eq!(parse_github_remote("https://gitlab.com/a/b.git"), None);
        assert_eq!(parse_github_remote("https://github.com/only-owner"), None);
        assert_eq!(parse_github_remote("https://github.com/a/b/c"), None);
        assert_eq!(slugify("Serie Nueva"), "serie-nueva");
        assert_eq!(slugify("  ¿Quién?  "), "quien");
        assert_eq!(slugify("Guión de prueba"), "guion-de-prueba");
        assert_eq!(slugify(""), "narra-project");
        assert_eq!(slugify("---"), "narra-project");
    }

    #[test]
    fn utf16_to_byte_in_maps_relative_offsets() {
        assert_eq!(utf16_to_byte_in("abc", 0..0), 0..0);
        assert_eq!(utf16_to_byte_in("abc", 1..3), 1..3);
        // Decomposed e-acute = two chars (two UTF-16 units) in one
        // grapheme: offset 2 lands past both, at byte 3.
        assert_eq!(utf16_to_byte_in("e\u{301}", 2..2), 3..3);
        // Four units: a(1) + emoji(2) + b(1). Offsets count units, not bytes.
        assert_eq!(utf16_to_byte_in("a\u{1f600}b", 3..4), 5..6);
        // An offset inside a surrogate pair clamps to the char boundary.
        assert_eq!(utf16_to_byte_in("a\u{1f600}b", 2..4), 5..6);
        // Offsets past the end clamp to the text length.
        assert_eq!(utf16_to_byte_in("ab", 5..9), 2..2);
    }
}
