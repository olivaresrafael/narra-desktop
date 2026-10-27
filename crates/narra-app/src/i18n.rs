//! EN/ES localization for the workspace UI (M6 slice 9). Every string the
//! app itself displays comes from this table (`AGENTS.md`: user-visible
//! strings localizable, English and Spanish first); agent/task output text
//! and core error `Display`s are model/diagnostic content and stay as the
//! producer wrote them. The JSON dump remains machine-oriented English
//! (`lang` is reported as a key) — verification never depends on the UI
//! language.

/// UI language. English is the neutral default for headless contexts; the
/// editor detects the environment at startup and offers a live toggle.
/// The derived default is Spanish — the spike's working language and the
/// reducer states' initial value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    En,
    #[default]
    Es,
}

impl Lang {
    /// BCP-47-ish key used in the dump and CLI (`en`, `es`).
    pub fn key(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Es => "es",
        }
    }

    /// Parse a `--lang` value or dump key.
    pub fn from_key(value: &str) -> Option<Lang> {
        match value {
            "en" => Some(Lang::En),
            "es" => Some(Lang::Es),
            _ => None,
        }
    }

    /// Detect from a POSIX `LANG`/`LC_ALL` tag (`es_ES.UTF-8`, `en_US`…);
    /// anything unknown falls back to English, the neutral default.
    pub fn from_tag(tag: &str) -> Lang {
        if tag.trim().to_ascii_lowercase().starts_with("es") {
            Lang::Es
        } else {
            Lang::En
        }
    }

    /// Detect from the process environment (macOS sets `LANG` through the
    /// user's region settings; no system configuration is read or changed).
    pub fn detect() -> Lang {
        std::env::var("LC_ALL")
            .or_else(|_| std::env::var("LANG"))
            .map(|tag| Lang::from_tag(&tag))
            .unwrap_or(Lang::En)
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Es => "español",
        }
    }
}

/// `(key, english, spanish)` — one entry per user-visible string. Keys are
/// stable identifiers (`area.message`); placeholders are `{name}` tokens
/// substituted by [`tf`], and both languages must declare the same ones
/// (asserted by tests). A `static` (not `const`) so `tr` can return
/// `&'static str` borrowed from the table itself.
static STRINGS: &[(&str, &str, &str)] = &[
    // --- notices (notice bar) -------------------------------------------
    (
        "notice.dirty_switch",
        "unsaved changes: press ⌘S before switching documents",
        "cambios sin guardar: guarda con ⌘S antes de cambiar de documento",
    ),
    (
        "notice.open_failed",
        "could not open {path}: {error}",
        "no se pudo abrir {path}: {error}",
    ),
    (
        "notice.fixture_no_project",
        "fixture mode: no project open (use --project)",
        "modo fixture: no hay proyecto abierto (usa --project)",
    ),
    (
        "notice.no_changes",
        "no changes to save",
        "sin cambios que guardar",
    ),
    ("notice.saved", "saved: {path}", "guardado: {path}"),
    (
        "notice.save_failed",
        "error saving {path}: {error}",
        "error al guardar {path}: {error}",
    ),
    (
        "notice.agent_running",
        "an agent task is already running (⌘. cancels it)",
        "ya hay una tarea de agente en curso (⌘. la cancela)",
    ),
    (
        "notice.fixture_no_project_agent",
        "fixture mode: no project open (use --project)",
        "modo fixture: sin proyecto abierto (usa --project)",
    ),
    (
        "notice.open_screenplay",
        "open {path} to work on a scene",
        "abre {path} para trabajar sobre una escena",
    ),
    (
        "notice.dirty_agent",
        "unsaved changes: agent tasks read the file from disk (⌘S)",
        "cambios sin guardar: las tareas de agente leen el archivo en disco (⌘S)",
    ),
    (
        "notice.caret_outside_scene",
        "the caret is not inside a scene",
        "el caret no está dentro de una escena",
    ),
    (
        "notice.roles_invalid",
        "invalid project roles: {error}",
        "roles del proyecto inválidos: {error}",
    ),
    (
        "notice.review_running",
        "review of scene {scene} running ({role})",
        "revisión de la escena {scene} en curso ({role})",
    ),
    (
        "notice.focus_suffix",
        " · focus: {focus}",
        " · foco: {focus}",
    ),
    (
        "notice.review_done",
        "review of scene {scene} is ready",
        "revisión de la escena {scene} lista",
    ),
    (
        "notice.review_cancelled",
        "review cancelled",
        "revisión cancelada",
    ),
    (
        "notice.review_failed",
        "review failed: {error}",
        "revisión fallida: {error}",
    ),
    (
        "notice.no_retry",
        "nothing to retry (⌘R reviews · ⌘P proposes)",
        "nada que reintentar (⌘R revisa · ⌘P propone)",
    ),
    (
        "notice.propose_running",
        "proposal for scene {scene} running ({role})",
        "propuesta para la escena {scene} en curso ({role})",
    ),
    (
        "notice.propose_ready",
        "proposal {id} ready (⌘⇧Enter applies · ⌘⌫ rejects)",
        "propuesta {id} lista (⌘⇧Enter aplica · ⌘⌫ descarta)",
    ),
    (
        "notice.propose_cancelled",
        "proposal cancelled",
        "propuesta cancelada",
    ),
    (
        "notice.propose_failed",
        "proposal failed: {error}",
        "propuesta fallida: {error}",
    ),
    (
        "notice.propose_status",
        "proposal {status}",
        "propuesta {status}",
    ),
    (
        "notice.already_status",
        "the proposal is already {status}",
        "la propuesta ya está {status}",
    ),
    (
        "notice.empty_instruction",
        "empty instruction: proposal not started",
        "instrucción vacía: propuesta no iniciada",
    ),
    (
        "notice.nothing_to_apply",
        "no proposal is ready to apply",
        "no hay ninguna propuesta lista para aplicar",
    ),
    ("notice.hunks_n", "{n} hunk(s)", "{n} hunk(s)"),
    (
        "notice.apply_arm",
        "press ⌘⇧Enter again to apply {selection} of {id} to {target} (writes to disk) · ⌘⌫ rejects",
        "⌘⇧Enter de nuevo para aplicar {selection} de {id} a {target} (escribe en disco) · ⌘⌫ descarta",
    ),
    (
        "notice.fixture_no_project_short",
        "fixture mode: no project open",
        "modo fixture: sin proyecto abierto",
    ),
    (
        "notice.applied",
        "proposal applied: {applied} hunk(s) in {target}",
        "propuesta aplicada: {applied} hunk(s) en {target}",
    ),
    (
        "notice.applied_skipped",
        "proposal applied: {applied} hunk(s) in {target} · {skipped} skipped by selection",
        "propuesta aplicada: {applied} hunk(s) en {target} · {skipped} omitido(s) por selección",
    ),
    (
        "notice.apply_failed",
        "could not apply: {error}",
        "no se pudo aplicar: {error}",
    ),
    (
        "notice.apply_reload_failed",
        "applied, but could not reload: {error}",
        "aplicado, pero no se pudo recargar: {error}",
    ),
    (
        "notice.nothing_to_reject",
        "no proposal to reject",
        "no hay ninguna propuesta que descartar",
    ),
    (
        "notice.rejected",
        "proposal {id} rejected (file untouched)",
        "propuesta {id} descartada (archivo intacto)",
    ),
    (
        "notice.reject_failed",
        "could not reject: {error}",
        "no se pudo descartar: {error}",
    ),
    (
        "notice.cancelling_review",
        "cancelling the review…",
        "cancelando la revisión…",
    ),
    (
        "notice.no_review_running",
        "no review is running",
        "no hay ninguna revisión en curso",
    ),
    (
        "notice.exported",
        "PDF exported: {path} ({pages} pages)",
        "PDF exportado: {path} ({pages} páginas)",
    ),
    (
        "notice.export_failed",
        "export failed: {error}",
        "export fallido: {error}",
    ),
    ("notice.lang", "language: {lang}", "idioma: {lang}"),
    (
        "notice.backup_found",
        "a newer backup is available (⌘⇧B restores · ⌘⇧D discards)",
        "hay un backup más reciente (⌘⇧B restaura · ⌘⇧D descarta)",
    ),
    (
        "notice.backup_restored",
        "backup restored into the buffer (⌘S saves)",
        "backup restaurado en el buffer (⌘S guarda)",
    ),
    (
        "notice.backup_discarded",
        "backup discarded",
        "backup descartado",
    ),
    (
        "notice.backup_missing",
        "no backup to restore",
        "no hay backup que restaurar",
    ),
    (
        "notice.backup_failed",
        "backup error: {error}",
        "error de backup: {error}",
    ),
    // --- unsaved-changes prompt ------------------------------------------
    ("quit.title", "Unsaved changes", "Cambios sin guardar"),
    (
        "quit.body",
        "Save before closing, or discard the changes.",
        "Guarda antes de cerrar o descarta los cambios.",
    ),
    (
        "quit.body_many",
        "{n} documents have unsaved changes.",
        "{n} documentos tienen cambios sin guardar.",
    ),
    ("quit.save", "Save and close", "Guardar y cerrar"),
    (
        "quit.save_all",
        "Save all and close",
        "Guardar todo y cerrar",
    ),
    ("quit.discard", "Discard and close", "Descartar y cerrar"),
    (
        "quit.discard_all",
        "Discard all and close",
        "Descartar todo y cerrar",
    ),
    ("quit.cancel", "Cancel", "Cancelar"),
    ("tab.untitled", "Untitled", "Sin título"),
    ("menu.close_tab", "Close Tab", "Cerrar pestaña"),
    ("menu.next_tab", "Next Tab", "Pestaña siguiente"),
    ("menu.prev_tab", "Previous Tab", "Pestaña anterior"),
    (
        "notice.no_document",
        "no document is open to save",
        "no hay un documento abierto para guardar",
    ),
    (
        "notice.export_screenplay",
        "open the screenplay tab to export the PDF",
        "abre la pestaña del guion para exportar el PDF",
    ),
    (
        "notice.apply_target_dirty",
        "the proposal target has unsaved changes: save it before applying",
        "el destino de la propuesta tiene cambios sin guardar: guárdalo antes de aplicar",
    ),
    // --- new file (M10 slice 2) ------------------------------------------
    ("newfile.title", "New file", "Nuevo archivo"),
    ("newfile.name", "Name:", "Nombre:"),
    ("newfile.format", "Format:", "Formato:"),
    (
        "newfile.format_fountain",
        "Fountain (.fountain)",
        "Fountain (.fountain)",
    ),
    (
        "newfile.format_markdown",
        "Markdown (.md)",
        "Markdown (.md)",
    ),
    (
        "newfile.formats_hint",
        "More formats later; the native format is in design.",
        "Más formatos después; el formato nativo está en diseño.",
    ),
    ("newfile.create", "Create", "Crear"),
    ("newfile.cancel", "Cancel", "Cancelar"),
    (
        "newfile.invalid_name",
        "Invalid name: letters, digits, spaces and accents (no /, \\, :, *, ?, \" or |).",
        "Nombre inválido: letras, números, espacios y acentos (sin /, \\, :, *, ?, \" ni |).",
    ),
    (
        "newfile.exists",
        "A document with that name already exists.",
        "Ya existe un documento con ese nombre.",
    ),
    (
        "newfile.failed",
        "Could not create the document: {error}",
        "No se pudo crear el documento: {error}",
    ),
    ("newfile.kind.character", "Character", "Personaje"),
    ("newfile.kind.notes", "Notes", "Notas"),
    ("newfile.kind.research", "Research", "Investigación"),
    ("newfile.kind.screenplay", "Screenplay", "Guion"),
    ("notice.created", "created {path}", "creado {path}"),
    ("menu.new_file", "New File…", "Nuevo archivo…"),
    // --- projects (M10 slice 2b) -----------------------------------------
    ("menu.new_project", "New Project…", "Nuevo proyecto…"),
    ("menu.open_project", "Open Project…", "Abrir proyecto…"),
    ("menu.open_recent", "Open Recent", "Abrir reciente"),
    ("menu.clear_recents", "Clear Menu", "Borrar menú"),
    ("dialog.open_project", "Open Project", "Abrir proyecto"),
    ("newproject.title", "New project", "Nuevo proyecto"),
    ("newproject.name", "Name:", "Nombre:"),
    ("newproject.type", "Type:", "Tipo:"),
    ("newproject.language", "Language:", "Idioma:"),
    (
        "newproject.hint",
        "The system panel will ask for the project folder.",
        "El panel del sistema pedirá la carpeta del proyecto.",
    ),
    (
        "newproject.waiting",
        "Waiting for the folder in the system panel…",
        "Esperando la carpeta en el panel del sistema…",
    ),
    ("newproject.create", "Create", "Crear"),
    ("newproject.cancel", "Cancel", "Cancelar"),
    (
        "newproject.invalid_title",
        "Give the project a name.",
        "Ponle un nombre al proyecto.",
    ),
    (
        "newproject.dialog_failed",
        "Could not open the folder panel: {error}",
        "No se pudo abrir el panel de carpetas: {error}",
    ),
    (
        "newproject.not_directory",
        "The selected path is not a folder.",
        "La ruta elegida no es una carpeta.",
    ),
    (
        "newproject.not_empty",
        "The selected folder is not empty: choose a new or empty folder.",
        "La carpeta elegida no está vacía: elige una carpeta nueva o vacía.",
    ),
    (
        "newproject.failed",
        "Could not create the project: {error}",
        "No se pudo crear el proyecto: {error}",
    ),
    (
        "newproject.open_failed",
        "The project was created but could not be opened: {error}",
        "El proyecto se creó, pero no se pudo abrir: {error}",
    ),
    ("newproject.type.feature", "Feature", "Largometraje"),
    ("newproject.type.series", "Series", "Serie"),
    ("newproject.type.web_series", "Web series", "Serie web"),
    ("newproject.type.short", "Short", "Cortometraje"),
    ("newproject.type.microdrama", "Microdrama", "Microdrama"),
    ("newproject.type.other", "Other", "Otro"),
    (
        "notice.finish_dialog",
        "finish or close the open dialog first",
        "termina o cierra el diálogo abierto primero",
    ),
    (
        "notice.project_agent_running",
        "an agent task is running: cancel it (⌘.) before switching projects",
        "hay una tarea de agente en curso: cancélala (⌘.) antes de cambiar de proyecto",
    ),
    (
        "notice.dirty_project_switch",
        "unsaved changes: press ⌘S before opening another project",
        "cambios sin guardar: guarda con ⌘S antes de abrir otro proyecto",
    ),
    (
        "notice.project_dialog_failed",
        "could not open the folder panel: {error}",
        "no se pudo abrir el panel de carpetas: {error}",
    ),
    (
        "notice.project_already_open",
        "that project is already open",
        "ese proyecto ya está abierto",
    ),
    (
        "notice.project_open_failed",
        "could not open the project {path}: {error}",
        "no se pudo abrir el proyecto {path}: {error}",
    ),
    (
        "notice.project_opened",
        "opened {title} · {path}",
        "abierto {title} · {path}",
    ),
    (
        "notice.recents_cleared",
        "recent-projects menu cleared",
        "menú de proyectos recientes borrado",
    ),
    // --- native menus (M10 slice 1) --------------------------------------
    ("menu.app", "Narra", "Narra"),
    ("menu.file", "File", "Archivo"),
    ("menu.edit", "Edit", "Edición"),
    ("menu.view", "View", "Ver"),
    ("menu.script", "Script", "Guion"),
    ("menu.save", "Save", "Guardar"),
    ("menu.export_pdf", "Export PDF", "Exportar PDF"),
    ("menu.quit", "Quit", "Salir"),
    ("menu.undo", "Undo", "Deshacer"),
    ("menu.redo", "Redo", "Rehacer"),
    ("menu.cut", "Cut", "Cortar"),
    ("menu.copy", "Copy", "Copiar"),
    ("menu.paste", "Paste", "Pegar"),
    ("menu.select_all", "Select All", "Seleccionar todo"),
    ("menu.find", "Find…", "Buscar…"),
    ("menu.replace", "Replace…", "Reemplazar…"),
    (
        "menu.project_search",
        "Project Search…",
        "Buscar en el proyecto…",
    ),
    ("menu.toggle_diff", "Toggle Diff", "Ver diferencias"),
    ("menu.language", "Toggle Language", "Cambiar idioma"),
    ("menu.review", "Review Scene", "Revisar escena"),
    ("menu.focus", "Reviewer Focus…", "Foco del revisor…"),
    ("menu.propose", "Propose Scene Edit…", "Proponer edición…"),
    ("menu.retry", "Retry Agent Task", "Reintentar tarea"),
    ("menu.cycle_element", "Cycle Element", "Ciclar elemento"),
    (
        "menu.cycle_element_back",
        "Cycle Element Back",
        "Ciclar elemento atrás",
    ),
    ("menu.restore_backup", "Restore Backup", "Restaurar backup"),
    ("menu.discard_backup", "Discard Backup", "Descartar backup"),
    // --- shared status words --------------------------------------------
    ("status.idle", "idle", "inactivo"),
    ("status.running", "running", "en curso"),
    ("status.ready", "ready", "lista"),
    ("status.completed", "completed", "completada"),
    ("status.failed", "failed", "fallida"),
    ("status.cancelled", "cancelled", "cancelada"),
    ("status.applied", "applied", "aplicada"),
    ("status.rejected", "rejected", "descartada"),
    // --- agent/proposal panels ------------------------------------------
    (
        "panel.review_title",
        "Review · scene {scene} · {status}",
        "Revisión · escena {scene} · {status}",
    ),
    ("panel.focus", "Focus: {focus}", "Foco: {focus}"),
    (
        "panel.role",
        "Role: {role} · {source}",
        "Rol: {role} · {source}",
    ),
    ("role.project", "project", "proyecto"),
    ("role.builtin", "builtin", "integrado"),
    ("panel.sources", "Sources ({n})", "Fuentes ({n})"),
    (
        "panel.review_hint",
        "⌘R reviews · ⌘. cancels · ⌥↑/⌥↓ scrolls",
        "⌘R revisa · ⌘. cancela · ⌥↑/⌥↓ desplaza",
    ),
    (
        "panel.review_hint_retry",
        "⌘⌥R retries · ⌘R reviews again · ⌥↑/⌥↓ scrolls",
        "⌘⌥R reintenta · ⌘R revisa de nuevo · ⌥↑/⌥↓ desplaza",
    ),
    (
        "panel.proposal_title",
        "Proposal · scene {scene} · {status}",
        "Propuesta · escena {scene} · {status}",
    ),
    (
        "panel.proposal_meta",
        "{id} · {target} · {n} hunk(s)",
        "{id} · {target} · {n} hunk(s)",
    ),
    (
        "panel.proposal_hint_armed",
        "⚠︎ cmd-shift-enter APPLIES (writes to disk) · cmd-backspace rejects",
        "⚠︎ cmd-shift-enter APLICA (escribe en disco) · cmd-backspace descarta",
    ),
    (
        "panel.proposal_hint_retry",
        "⌘⌥R retries · ⌘P proposes again · ⌘⌫ rejects",
        "⌘⌥R reintenta · ⌘P propone otra vez · ⌘⌫ descarta",
    ),
    (
        "panel.proposal_hint",
        "⌘⇧Enter applies · ⌥←/⌥→ hunks · ⌥Enter toggles · ⌘⌫ rejects · ⌥↑/⌥↓ scrolls",
        "⌘⇧Enter aplica · ⌥←/⌥→ hunks · ⌥Enter alterna · ⌘⌫ descarta · ⌥↑/⌥↓ desplaza",
    ),
    // --- status bar ------------------------------------------------------
    (
        "bar.review_running",
        " · review running (scene {scene})",
        " · revisión en curso (escena {scene})",
    ),
    (
        "bar.review_done",
        " · review ready (scene {scene})",
        " · revisión lista (escena {scene})",
    ),
    (
        "bar.review_failed",
        " · review failed",
        " · revisión fallida",
    ),
    (
        "bar.review_cancelled",
        " · review cancelled",
        " · revisión cancelada",
    ),
    (
        "bar.propose_running",
        " · proposal running (scene {scene})",
        " · propuesta en curso (escena {scene})",
    ),
    (
        "bar.propose_ready",
        " · proposal ready ({id})",
        " · propuesta lista ({id})",
    ),
    ("bar.armed", " · armed", " · armada"),
    (
        "bar.propose_applied",
        " · proposal applied",
        " · propuesta aplicada",
    ),
    (
        "bar.propose_rejected",
        " · proposal rejected",
        " · propuesta descartada",
    ),
    (
        "bar.propose_failed",
        " · proposal failed",
        " · propuesta fallida",
    ),
    (
        "bar.propose_cancelled",
        " · proposal cancelled",
        " · propuesta cancelada",
    ),
    (
        "bar.status",
        "{doc} · L{line}:C{col} · {scenes} scenes · ~{pages} pages",
        "{doc} · L{line}:C{col} · {scenes} escenas · ~{pages} páginas",
    ),
    (
        "bar.metrics",
        "edits {edits} · key→frame {keys} · {lines} lines · frames {frames} · gap ms p50 {p50} / p95 {p95} / max {max} · probe{probe}",
        "ediciones {edits} · tecla→frame {keys} · {lines} líneas · frames {frames} · gap ms p50 {p50} / p95 {p95} / max {max} · probe{probe}",
    ),
    // --- bars ------------------------------------------------------------
    ("find.query_label", "Find:", "Buscar:"),
    ("find.replace_label", "Replace:", "Reemplazar:"),
    (
        "find.hint",
        "enter next · cmd-enter replaces · cmd-alt-enter all · esc closes",
        "enter siguiente · cmd-enter reemplaza · cmd-alt-enter todo · esc cierra",
    ),
    ("proposebar.label", "Instruction:", "Instrucción:"),
    (
        "proposebar.hint",
        "enter proposes · esc closes · ⌘⇧Enter applies · ⌥←/⌥→ hunks · ⌥Enter toggles",
        "enter propone · esc cierra · ⌘⇧Enter aplica · ⌥←/⌥→ hunks · ⌥Enter alterna",
    ),
    ("searchbar.label", "Project:", "Proyecto:"),
    (
        "searchbar.hint",
        "enter searches · enter opens · ⌘↑/⌘↓ moves · esc clears",
        "enter busca · enter abre · ⌘↑/⌘↓ mueve · esc limpia",
    ),
    (
        "panel.search_title",
        "Results ({n}) · “{query}”",
        "Resultados ({n}) · «{query}»",
    ),
    ("panel.outline_title", "Scenes ({n})", "Escenas ({n})"),
    ("panel.outline_more", "… {n} more", "… {n} más"),
    ("panel.search_truncated", " (truncated)", " (truncado)"),
    (
        "notice.search_done",
        "{n} results in {docs} documents",
        "{n} resultados en {docs} documentos",
    ),
    (
        "notice.search_empty",
        "no results for “{query}”",
        "sin resultados para «{query}»",
    ),
    (
        "notice.search_failed",
        "search failed: {error}",
        "búsqueda fallida: {error}",
    ),
    (
        "notice.search_open",
        "opened {document}:{line}",
        "abierto {document}:{line}",
    ),
    ("focusbar.label", "Focus:", "Foco:"),
    (
        "focusbar.hint_general",
        "enter reviews (general) · esc closes",
        "enter revisa (general) · esc cierra",
    ),
    (
        "focusbar.hint_focus",
        "enter reviews with focus · esc closes (the previous focus stays in effect)",
        "enter revisa con foco · esc cierra (el foco anterior sigue activo)",
    ),
    // --- reducer stage/error strings (agent_view) -------------------------
    ("stage.started", "task started", "tarea iniciada"),
    (
        "stage.context_collected",
        "context collected",
        "contexto recogido",
    ),
    (
        "stage.model_responded",
        "responded {model} ({chars} characters)",
        "respondió {model} ({chars} caracteres)",
    ),
    (
        "stage.proposal_persisted",
        "proposal created (stored in .narra/proposals)",
        "propuesta creada (grabada en .narra/proposals)",
    ),
    (
        "error.review_cancelled",
        "review cancelled",
        "revisión cancelada",
    ),
    (
        "error.propose_cancelled",
        "proposal cancelled",
        "propuesta cancelada",
    ),
];

/// Translate `key`. A missing key is a programming error: it leaks the key
/// text as the visible string (bounded by distinct typos, and the
/// completeness/parity tests prevent regressions) — never a panic and
/// never a silent blank.
pub fn tr(lang: Lang, key: &str) -> &'static str {
    match STRINGS.iter().find(|(k, ..)| *k == key) {
        Some((_, en, es)) => match lang {
            Lang::En => en,
            Lang::Es => es,
        },
        None => Box::leak(key.to_string().into_boxed_str()),
    }
}

/// Translate and substitute `{name}` placeholders (params in any order).
pub fn tf(lang: Lang, key: &str, params: &[(&str, &str)]) -> String {
    let mut out = tr(lang, key).to_string();
    for (name, value) in params {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extract the `{name}` placeholder set of a template.
    fn placeholders(text: &str) -> Vec<String> {
        let mut found = Vec::new();
        let mut rest = text;
        while let Some(start) = rest.find('{') {
            let after = &rest[start + 1..];
            let Some(end) = after.find('}') else {
                break;
            };
            found.push(after[..end].to_string());
            rest = &after[end + 1..];
        }
        found
    }

    #[test]
    fn every_key_has_non_empty_translations_and_no_duplicates() {
        let mut keys: Vec<&str> = Vec::new();
        for (key, en, es) in STRINGS {
            assert!(!key.is_empty(), "empty key");
            assert!(!en.trim().is_empty(), "empty english for {key}");
            assert!(!es.trim().is_empty(), "empty spanish for {key}");
            assert!(!keys.contains(key), "duplicate key {key}");
            keys.push(key);
        }
        assert!(keys.len() > 60, "the table looks truncated: {}", keys.len());
    }

    #[test]
    fn both_languages_declare_the_same_placeholders() {
        for (key, en, es) in STRINGS {
            let mut en_ph = placeholders(en);
            let mut es_ph = placeholders(es);
            en_ph.sort();
            es_ph.sort();
            assert_eq!(en_ph, es_ph, "placeholder mismatch for {key}");
        }
    }

    #[test]
    fn unknown_keys_fall_back_to_the_key_itself() {
        assert_eq!(tr(Lang::En, "no.such.key"), "no.such.key");
        assert_eq!(tr(Lang::Es, "no.such.key"), "no.such.key");
    }

    #[test]
    fn translate_and_format_substitutes_placeholders_in_any_order() {
        assert_eq!(
            tf(
                Lang::Es,
                "notice.saved",
                &[("path", "screenplay/main.fountain")]
            ),
            "guardado: screenplay/main.fountain"
        );
        assert_eq!(
            tf(Lang::En, "notice.saved", &[("path", "a.fountain")]),
            "saved: a.fountain"
        );
        // Params not present in the template are ignored; missing params
        // leave the placeholder visible (never a panic).
        assert_eq!(
            tf(Lang::En, "notice.saved", &[("nope", "x")]),
            "saved: {path}"
        );
    }

    #[test]
    fn language_detection_from_posix_tags() {
        assert_eq!(Lang::from_tag("es_ES.UTF-8"), Lang::Es);
        assert_eq!(Lang::from_tag("ES"), Lang::Es);
        assert_eq!(Lang::from_tag("en_US.UTF-8"), Lang::En);
        assert_eq!(Lang::from_tag("C"), Lang::En);
        assert_eq!(Lang::from_tag(""), Lang::En);
    }

    #[test]
    fn language_keys_round_trip() {
        for lang in [Lang::En, Lang::Es] {
            assert_eq!(Lang::from_key(lang.key()), Some(lang));
        }
        assert_eq!(Lang::from_key("fr"), None);
    }
}
