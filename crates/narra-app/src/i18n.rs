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
    ("newfile.kind.location", "Location", "Localización"),
    ("newfile.kind.object", "Object", "Objeto"),
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
    ("menu.project", "Project", "Proyecto"),
    (
        "menu.project_settings",
        "Project Settings…",
        "Ajustes del proyecto…",
    ),
    (
        "menu.show_config_files",
        "Show Configuration Files",
        "Mostrar archivos de configuración",
    ),
    ("menu.edit", "Edit", "Edición"),
    ("menu.view", "View", "Ver"),
    ("menu.script", "Script", "Guion"),
    ("menu.save", "Save", "Guardar"),
    ("menu.export_pdf", "Export PDF", "Exportar PDF"),
    ("menu.quit", "Quit", "Salir"),
    ("menu.settings", "Settings…", "Ajustes…"),
    (
        "notice.settings_invalid",
        "settings could not be loaded: {error}",
        "no se pudieron cargar los ajustes: {error}",
    ),
    (
        "notice.settings_saved",
        "settings saved · AI provider: {provider}",
        "ajustes guardados · proveedor de IA: {provider}",
    ),
    (
        "notice.settings_save_failed",
        "could not save settings: {error}",
        "no se pudieron guardar los ajustes: {error}",
    ),
    (
        "notice.config_files_shown",
        "configuration files are now visible",
        "los archivos de configuración ya son visibles",
    ),
    (
        "notice.config_files_hidden",
        "configuration files are hidden",
        "los archivos de configuración están ocultos",
    ),
    (
        "notice.project_settings_saved",
        "project settings saved",
        "ajustes del proyecto guardados",
    ),
    (
        "notice.entity_invalid",
        "select a single word or name to create a bible dossier",
        "selecciona una palabra o nombre para crear una ficha de la biblia",
    ),
    (
        "notice.entity_create_failed",
        "could not create the dossier: {error}",
        "no se pudo crear la ficha: {error}",
    ),
    (
        "notice.entity_opened",
        "{kind} dossier ready: {path}",
        "ficha de {kind} lista: {path}",
    ),
    (
        "notice.entity_clicked",
        "opened dossier for {name}: {path}",
        "ficha de {name} abierta: {path}",
    ),
    (
        "notice.element_applied",
        "screenplay element updated",
        "elemento de guion actualizado",
    ),
    // --- editor context menu (M11 slice 1) --------------------------------
    ("context.cut", "Cut", "Cortar"),
    ("context.copy", "Copy", "Copiar"),
    ("context.paste", "Paste", "Pegar"),
    ("context.select_all", "Select All", "Seleccionar todo"),
    ("context.find", "Find in file", "Buscar en el archivo"),
    (
        "context.project_search",
        "Search in project",
        "Buscar en el proyecto",
    ),
    (
        "context.propose",
        "Propose rewrite with AI…",
        "Proponer reescritura con IA…",
    ),
    (
        "context.convert.character",
        "Convert to character",
        "Convertir en personaje",
    ),
    (
        "context.convert.location",
        "Convert to location",
        "Convertir en localización",
    ),
    (
        "context.convert.object",
        "Convert to object",
        "Convertir en objeto",
    ),
    (
        "context.element.heading",
        "Make scene heading",
        "Convertir en encabezado de escena",
    ),
    (
        "context.element.action",
        "Make action",
        "Convertir en acción",
    ),
    (
        "context.element.character",
        "Make character cue",
        "Convertir en personaje (diálogo)",
    ),
    (
        "context.element.transition",
        "Make transition",
        "Convertir en transición",
    ),
    // --- explorer/tab menus (M11 slice 3) --------------------------------
    ("context.file.new", "New file…", "Nuevo archivo…"),
    ("context.file.rename", "Rename…", "Renombrar…"),
    ("context.file.duplicate", "Duplicate", "Duplicar"),
    ("context.file.delete", "Delete…", "Eliminar…"),
    ("context.file.reveal", "Show in Finder", "Mostrar en Finder"),
    ("context.file.copy_path", "Copy path", "Copiar ruta"),
    ("context.tab.close", "Close tab", "Cerrar pestaña"),
    (
        "context.tab.close_others",
        "Close other tabs",
        "Cerrar otras pestañas",
    ),
    (
        "notice.renamed",
        "renamed {from} → {to}",
        "renombrado {from} → {to}",
    ),
    (
        "notice.duplicated",
        "duplicated {from} → {to}",
        "duplicado {from} → {to}",
    ),
    ("notice.deleted", "deleted {path}", "eliminado {path}"),
    (
        "notice.file_op_failed",
        "the file operation failed: {error}",
        "la operación de archivo falló: {error}",
    ),
    (
        "notice.file_op_failed_no_name",
        "could not find an available duplicate name",
        "no se encontró un nombre disponible para la copia",
    ),
    (
        "notice.close_others_dirty",
        "save the other tabs before closing them",
        "guarda las otras pestañas antes de cerrarlas",
    ),
    (
        "notice.revealed",
        "revealed {path} in Finder",
        "se mostró {path} en Finder",
    ),
    (
        "notice.reveal_failed",
        "could not reveal the file: {error}",
        "no se pudo mostrar el archivo: {error}",
    ),
    (
        "notice.path_copied",
        "path copied: {path}",
        "ruta copiada: {path}",
    ),
    // --- rename / delete dialogs -----------------------------------------
    ("rename.title", "Rename file", "Renombrar archivo"),
    ("rename.current", "Current: {path}", "Actual: {path}"),
    ("rename.name", "New name:", "Nuevo nombre:"),
    (
        "rename.hint",
        "The extension is preserved. The file stays in its current folder.",
        "La extensión se conserva. El archivo permanece en su carpeta actual.",
    ),
    (
        "rename.invalid",
        "enter a valid file name without slashes",
        "introduce un nombre de archivo válido sin barras",
    ),
    ("rename.save", "Rename", "Renombrar"),
    ("rename.cancel", "Cancel", "Cancelar"),
    ("delete.title", "Delete file?", "¿Eliminar archivo?"),
    (
        "delete.body",
        "“{name}” will be removed from the project. This cannot be undone outside Git.",
        "“{name}” se eliminará del proyecto. Esto no se puede deshacer fuera de Git.",
    ),
    (
        "delete.hint",
        "Cancel is the safe default. Delete is only performed from the red button.",
        "Cancelar es la opción segura. Eliminar solo se ejecuta desde el botón rojo.",
    ),
    (
        "delete.dirty",
        "this file has unsaved changes; save it before deleting",
        "este archivo tiene cambios sin guardar; guárdalo antes de eliminarlo",
    ),
    ("delete.confirm", "Delete", "Eliminar"),
    ("delete.cancel", "Cancel", "Cancelar"),
    // --- chat workspace (M12) ---------------------------------------------
    ("chat.tab.chat", "Chat", "Chat"),
    ("chat.tab.activity", "Activity", "Actividad"),
    ("chat.new", "New chat", "Nuevo chat"),
    (
        "chat.no_history",
        "No saved conversations yet.",
        "Todavía no hay conversaciones guardadas.",
    ),
    (
        "chat.empty_title",
        "Ask, plan or propose a change",
        "Pregunta, planifica o propone un cambio",
    ),
    (
        "chat.empty_hint",
        "⌘L focuses this box. Enter sends, Shift+Enter adds a line. Use / for commands and @ to attach context.",
        "⌘L enfoca esta caja. Enter envía, Mayús+Enter añade una línea. Usa / para comandos y @ para adjuntar contexto.",
    ),
    ("chat.you", "You", "Tú"),
    ("chat.assistant", "Assistant", "Asistente"),
    (
        "chat.assistant_note",
        "AI suggestion — review before applying",
        "Sugerencia de IA — revisa antes de aplicar",
    ),
    ("chat.working", "Working…", "Trabajando…"),
    (
        "chat.sources",
        "Context: {n} source(s)",
        "Contexto: {n} fuente(s)",
    ),
    ("chat.reject", "Reject", "Rechazar"),
    ("chat.apply", "Apply…", "Aplicar…"),
    (
        "chat.apply_confirm",
        "Confirm apply",
        "Confirmar aplicación",
    ),
    (
        "chat.placeholder",
        "Write a message… (/ commands, @ context)",
        "Escribe un mensaje… (/ comandos, @ contexto)",
    ),
    ("chat.send", "Send", "Enviar"),
    ("chat.stop", "Stop", "Detener"),
    ("chat.mode.plan", "Plan", "Plan"),
    ("chat.mode.build", "Build", "Construir"),
    ("chat.mode.review", "Review", "Revisar"),
    ("chat.completed", "chat ready", "chat listo"),
    (
        "chat.failed",
        "chat failed: {error}",
        "el chat falló: {error}",
    ),
    ("chat.cancelled", "chat cancelled", "chat cancelado"),
    ("chat.stopping", "stopping chat…", "deteniendo el chat…"),
    (
        "chat.not_running",
        "no chat is running",
        "no hay ningún chat en marcha",
    ),
    (
        "chat.no_scene",
        "no scene under the caret",
        "no hay escena bajo el cursor",
    ),
    (
        "chat.no_selection",
        "select some text first",
        "selecciona texto primero",
    ),
    ("chat.selection_chip", "Selection", "Selección"),
    (
        "chat.rewrite_prompt",
        "Rewrite the attached scene to…",
        "Reescribe la escena adjunta para…",
    ),
    (
        "chat.continuity_prompt",
        "Check the continuity of…",
        "Revisa la continuidad de…",
    ),
    (
        "chat.character_prompt",
        "Help me develop this character:",
        "Ayúdame a desarrollar este personaje:",
    ),
    (
        "chat.save_failed",
        "could not save the conversation: {error}",
        "no se pudo guardar la conversación: {error}",
    ),
    (
        "chat.apply_arm",
        "click Apply again to confirm the selected hunks",
        "pulsa Aplicar otra vez para confirmar los bloques seleccionados",
    ),
    (
        "chat.select_hunk",
        "select at least one hunk",
        "selecciona al menos un bloque",
    ),
    (
        "chat.applied",
        "proposal applied to {target}",
        "propuesta aplicada a {target}",
    ),
    (
        "chat.applied_note",
        "Proposal applied to the canonical file.",
        "Propuesta aplicada al archivo canónico.",
    ),
    (
        "chat.apply_failed",
        "could not apply the proposal: {error}",
        "no se pudo aplicar la propuesta: {error}",
    ),
    ("chat.rejected", "proposal rejected", "propuesta rechazada"),
    (
        "chat.rejected_note",
        "Proposal rejected; the canonical file was not modified.",
        "Propuesta rechazada; el archivo canónico no se modificó.",
    ),
    (
        "chat.reject_failed",
        "could not reject the proposal: {error}",
        "no se pudo rechazar la propuesta: {error}",
    ),
    // --- settings (M10 slice 4) -------------------------------------------
    ("settings.title", "Settings", "Ajustes"),
    ("settings.section.general", "General", "General"),
    ("settings.section.editor", "Editor", "Editor"),
    ("settings.section.ai", "AI", "IA"),
    ("settings.section.git", "Versions", "Versiones"),
    ("settings.language", "Language", "Idioma"),
    ("settings.language.system", "System", "Sistema"),
    ("settings.language.english", "English", "English"),
    ("settings.language.spanish", "Español", "Español"),
    ("settings.theme", "Appearance", "Apariencia"),
    ("settings.theme.system", "System", "Sistema"),
    ("settings.theme.dark", "Dark", "Oscuro"),
    ("settings.theme.light", "Light", "Claro"),
    (
        "settings.general_hint",
        "Language and appearance are saved per user, not in the project.",
        "El idioma y la apariencia se guardan por usuario, no en el proyecto.",
    ),
    ("settings.font_size", "Font size:", "Tamaño de letra:"),
    (
        "settings.word_wrap",
        "Wrap long lines",
        "Ajustar líneas largas",
    ),
    ("settings.line_numbers", "Line numbers", "Números de línea"),
    (
        "settings.show_config_files",
        "Show project.yaml, agents.yaml and AGENTS.md in the explorer",
        "Mostrar project.yaml, agents.yaml y AGENTS.md en el explorador",
    ),
    (
        "settings.editor_hint",
        "Font size, wrapping and line numbers apply immediately after Save.",
        "El tamaño, el ajuste de líneas y los números se aplican al guardar.",
    ),
    ("settings.on", "On", "Sí"),
    ("settings.off", "Off", "No"),
    ("settings.github_token", "GitHub token:", "Token de GitHub:"),
    (
        "settings.github_token_empty",
        "not connected",
        "sin conectar",
    ),
    (
        "settings.github_hint",
        "Stored in Keychain. Used for GitHub repository creation, uploads and pull requests.",
        "Se guarda en Keychain. Se usa para crear repositorios, subir versiones y pull requests en GitHub.",
    ),
    (
        "settings.github_key_save_failed",
        "Could not store the GitHub token: {error}",
        "No se pudo guardar el token de GitHub: {error}",
    ),
    (
        "settings.github_key_cleared",
        "GitHub token removed.",
        "Token de GitHub eliminado.",
    ),
    ("settings.git_name", "Author name:", "Nombre del autor:"),
    ("settings.git_email", "Author email:", "Correo del autor:"),
    (
        "settings.git_hint",
        "Used when you save a version. Leave empty to use your Mac Git configuration.",
        "Se usa al guardar una versión. Déjalo vacío para usar la configuración Git de tu Mac.",
    ),
    (
        "settings.provider.mock",
        "Mock (offline)",
        "Mock (sin conexión)",
    ),
    (
        "settings.provider.openai_compat",
        "OpenAI-compatible",
        "Compatible con OpenAI",
    ),
    ("settings.provider.opencode", "OpenCode", "OpenCode"),
    (
        "settings.provider.opencode_go",
        "OpenCode Go",
        "OpenCode Go",
    ),
    ("settings.base_url", "Endpoint:", "Endpoint:"),
    ("settings.model", "Model:", "Modelo:"),
    ("settings.api_key", "API key:", "Clave API:"),
    ("settings.key_empty", "not stored", "sin guardar"),
    (
        "settings.key_source_keychain",
        "stored in Keychain",
        "guardada en Keychain",
    ),
    (
        "settings.key_source_env",
        "from environment",
        "desde el entorno",
    ),
    (
        "settings.key_hint",
        "The key is written to macOS Keychain on Save; it never enters settings.json, logs or the dump.",
        "La clave se guarda en Keychain al guardar; nunca entra en settings.json, logs ni el dump.",
    ),
    ("settings.clear_key", "Clear key", "Borrar clave"),
    (
        "settings.fetch_models",
        "Check / fetch models",
        "Comprobar / listar modelos",
    ),
    ("settings.fetching", "Fetching…", "Consultando…"),
    (
        "settings.models_fetched",
        "{n} models available",
        "{n} modelos disponibles",
    ),
    (
        "settings.models_failed",
        "could not fetch models: {error}",
        "no se pudieron listar los modelos: {error}",
    ),
    (
        "settings.mock_local",
        "Mock is local; there is no remote model catalog.",
        "Mock es local; no hay catálogo remoto de modelos.",
    ),
    (
        "settings.mock_hint",
        "Mock needs no endpoint, model or key. Choose another provider for real calls.",
        "Mock no necesita endpoint, modelo ni clave. Elige otro proveedor para llamadas reales.",
    ),
    (
        "settings.model_required",
        "Choose a model id.",
        "Elige un identificador de modelo.",
    ),
    (
        "settings.base_url_required",
        "Enter an endpoint URL.",
        "Introduce la URL del endpoint.",
    ),
    (
        "settings.key_required",
        "This provider needs an API key (Keychain or environment).",
        "Este proveedor necesita una API key (Keychain o entorno).",
    ),
    (
        "settings.key_save_failed",
        "Could not store the key: {error}",
        "No se pudo guardar la clave: {error}",
    ),
    (
        "settings.save_failed",
        "Could not save settings: {error}",
        "No se pudieron guardar los ajustes: {error}",
    ),
    (
        "settings.key_cleared",
        "Stored key removed.",
        "Clave guardada eliminada.",
    ),
    (
        "settings.model_route_hint",
        "Narra uses the OpenAI chat-completions route; pick a model that supports it.",
        "Narra usa la ruta OpenAI chat-completions; elige un modelo que la soporte.",
    ),
    ("settings.save", "Save", "Guardar"),
    ("settings.cancel", "Cancel", "Cancelar"),
    // --- project settings (M10 slice 4c) ---------------------------------
    (
        "project_settings.title",
        "Project Settings",
        "Ajustes del proyecto",
    ),
    (
        "project_settings.hint",
        "These fields are saved to project.yaml with validation. The file stays plain YAML and can still be edited manually.",
        "Estos campos se guardan en project.yaml con validación. El archivo sigue siendo YAML simple y puede editarse a mano.",
    ),
    ("project_settings.name", "Title:", "Título:"),
    ("project_settings.kind", "Project type", "Tipo de proyecto"),
    (
        "project_settings.primary_language",
        "Primary language:",
        "Idioma principal:",
    ),
    (
        "project_settings.additional_languages",
        "Additional languages:",
        "Idiomas adicionales:",
    ),
    (
        "project_settings.additional_hint",
        "Comma-separated language tags, for example: es, en.",
        "Etiquetas separadas por comas, por ejemplo: es, en.",
    ),
    (
        "project_settings.default_screenplay",
        "Default screenplay",
        "Guion principal",
    ),
    (
        "project_settings.no_screenplays",
        "No .fountain documents found yet.",
        "Todavía no hay documentos .fountain.",
    ),
    ("project_settings.save", "Save", "Guardar"),
    ("project_settings.cancel", "Cancel", "Cancelar"),
    (
        "project_settings.open_yaml",
        "Open project.yaml as text",
        "Abrir project.yaml como texto",
    ),
    (
        "project_settings.save_failed",
        "could not save project settings: {error}",
        "no se pudieron guardar los ajustes del proyecto: {error}",
    ),
    ("project.type.feature", "Feature film", "Largometraje"),
    ("project.type.series", "Series", "Serie"),
    ("project.type.web_series", "Web series", "Serie web"),
    ("project.type.short", "Short film", "Cortometraje"),
    ("project.type.microdrama", "Microdrama", "Microdrama"),
    ("project.type.other", "Other", "Otro"),
    ("menu.undo", "Undo", "Deshacer"),
    ("menu.redo", "Redo", "Rehacer"),
    ("menu.cut", "Cut", "Cortar"),
    ("menu.copy", "Copy", "Copiar"),
    ("menu.paste", "Paste", "Pegar"),
    ("menu.select_all", "Select All", "Seleccionar todo"),
    ("menu.find", "Find…", "Buscar…"),
    ("menu.replace", "Replace…", "Reemplazar…"),
    ("menu.context_menu", "Context Menu", "Menú contextual"),
    (
        "menu.project_search",
        "Project Search…",
        "Buscar en el proyecto…",
    ),
    ("menu.toggle_diff", "Toggle Diff", "Ver diferencias"),
    (
        "menu.toggle_explorer",
        "Toggle Explorer",
        "Mostrar/ocultar explorador",
    ),
    (
        "menu.toggle_agent",
        "Toggle Agent Panel",
        "Mostrar/ocultar agente",
    ),
    (
        "menu.toggle_git",
        "Toggle Git Panel",
        "Mostrar/ocultar panel Git",
    ),
    ("menu.language", "Toggle Language", "Cambiar idioma"),
    ("menu.quick_open", "Quick Open…", "Abrir rápido…"),
    (
        "menu.command_palette",
        "Command Palette…",
        "Paleta de comandos…",
    ),
    // --- quick open / command palette (M10 slice 5) -----------------------
    ("palette.files_title", "Go to file", "Ir a archivo"),
    ("palette.commands_title", "Commands", "Comandos"),
    (
        "palette.placeholder_files",
        "Type a file name…",
        "Escribe un nombre de archivo…",
    ),
    (
        "palette.placeholder_commands",
        "Type a command…",
        "Escribe un comando…",
    ),
    ("palette.no_results", "No matches", "Sin resultados"),
    (
        "palette.hint",
        "↑↓ move · Enter open · Esc close",
        "↑↓ mover · Enter abrir · Esc cerrar",
    ),
    ("bar.language_native", "native", "nativo"),
    ("panel.git_none", "no repository", "sin repositorio"),
    // --- human Git surface (M13) -----------------------------------------
    ("git.changes", "Changes", "Cambios"),
    (
        "git.no_repo",
        "This project is not under version control yet.",
        "Este proyecto todavía no tiene control de versiones.",
    ),
    (
        "git.no_repo_hint",
        "Start saving versions so you can return to any point later.",
        "Empieza a guardar versiones para poder volver a cualquier punto más tarde.",
    ),
    (
        "git.init",
        "Start version control",
        "Iniciar control de versiones",
    ),
    ("git.detached", "detached", "sin borrador"),
    ("git.draft", "Draft", "Borrador"),
    (
        "git.clean",
        "Everything is saved in the latest version.",
        "Todo está guardado en la última versión.",
    ),
    (
        "git.summary",
        "{n} file(s) changed since the last saved version",
        "{n} archivo(s) con cambios desde la última versión guardada",
    ),
    ("git.save_version", "Save version…", "Guardar versión…"),
    ("git.history", "History", "Historial"),
    (
        "git.loading_history",
        "Loading versions…",
        "Cargando versiones…",
    ),
    (
        "git.no_history",
        "No saved versions yet.",
        "Todavía no hay versiones guardadas.",
    ),
    ("git.recent", "Recent versions", "Versiones recientes"),
    ("git.status.modified", "Edited", "Editado"),
    ("git.status.added", "Added", "Añadido"),
    ("git.status.new", "New", "Nuevo"),
    ("git.status.deleted", "Deleted", "Eliminado"),
    ("git.status.renamed", "Renamed", "Renombrado"),
    ("git.status.copied", "Copied", "Copiado"),
    ("git.status.conflict", "Conflict", "Conflicto"),
    ("git.status.unknown", "Changed", "Cambiado"),
    (
        "git.tip.status",
        "Shows what changed in your working files since the last saved version.",
        "Muestra qué cambió en tus archivos desde la última versión guardada.",
    ),
    (
        "git.tip.status_bar",
        "{n} file(s) changed. Click to open the Changes panel.",
        "{n} archivo(s) con cambios. Haz clic para abrir el panel de Cambios.",
    ),
    (
        "git.tip.commit",
        "Saves a snapshot of the project that you can return to later. It does not upload anything.",
        "Guarda una foto del proyecto a la que podrás volver después. No sube nada a internet.",
    ),
    (
        "git.tip.log",
        "Shows the versions already saved in this project.",
        "Muestra las versiones ya guardadas en este proyecto.",
    ),
    (
        "git.tip.log_detail",
        "View this version in the project history.",
        "Ver esta versión en el historial del proyecto.",
    ),
    (
        "git.tip.init",
        "Starts a local version history for this project.",
        "Inicia un historial de versiones local para este proyecto.",
    ),
    (
        "git.tip.branch",
        "The current line of work. Parallel drafts are separate lines you can switch between.",
        "La línea de trabajo actual. Los borradores paralelos son líneas separadas entre las que puedes cambiar.",
    ),
    (
        "git.save_files_first",
        "save your open files before creating a version",
        "guarda los archivos abiertos antes de crear una versión",
    ),
    (
        "git.nothing_to_save",
        "there are no changes to save",
        "no hay cambios que guardar",
    ),
    (
        "git.identity_missing",
        "set your author name and email in Settings → Versions",
        "configura tu nombre y correo en Ajustes → Versiones",
    ),
    (
        "git.diff_title",
        "What changed in this file",
        "Qué cambió en este archivo",
    ),
    ("git.diff_loading", "Loading changes…", "Cargando cambios…"),
    (
        "git.diff_empty",
        "No changes to show.",
        "No hay cambios que mostrar.",
    ),
    ("git.close", "Close", "Cerrar"),
    (
        "git.restore",
        "Restore from latest version…",
        "Restaurar desde la última versión…",
    ),
    (
        "git.restore_title",
        "Restore this file?",
        "¿Restaurar este archivo?",
    ),
    (
        "git.restore_body",
        "“{path}” will be replaced by its content from the latest saved version.",
        "“{path}” se reemplazará por su contenido de la última versión guardada.",
    ),
    (
        "git.restore_hint",
        "This changes the working file only. Your version history is not rewritten.",
        "Esto cambia solo el archivo de trabajo. El historial de versiones no se reescribe.",
    ),
    ("git.restore_confirm", "Restore file", "Restaurar archivo"),
    (
        "git.no_previous_version",
        "This file has no previous saved version to restore.",
        "Este archivo no tiene una versión anterior guardada que restaurar.",
    ),
    (
        "git.restore_dirty",
        "save this file before restoring it",
        "guarda este archivo antes de restaurarlo",
    ),
    ("git.restored", "file restored", "archivo restaurado"),
    (
        "git.restore_failed",
        "could not restore the file: {error}",
        "no se pudo restaurar el archivo: {error}",
    ),
    ("git.drafts", "Parallel drafts", "Borradores paralelos"),
    (
        "git.loading_drafts",
        "Loading drafts…",
        "Cargando borradores…",
    ),
    ("git.current", "current", "actual"),
    (
        "git.new_draft",
        "New parallel draft…",
        "Nuevo borrador paralelo…",
    ),
    (
        "git.branch_title",
        "New parallel draft",
        "Nuevo borrador paralelo",
    ),
    (
        "git.branch_hint",
        "Creates a separate line of work without losing the current one.",
        "Crea una línea de trabajo separada sin perder la actual.",
    ),
    (
        "git.branch_placeholder",
        "e.g. final-alternativo",
        "p. ej. final-alternativo",
    ),
    ("git.branch_save", "Create draft", "Crear borrador"),
    (
        "git.branch_name_required",
        "enter a name for the draft",
        "introduce un nombre para el borrador",
    ),
    ("git.branch_switched", "draft switched", "borrador cambiado"),
    (
        "git.branch_created",
        "parallel draft created",
        "borrador paralelo creado",
    ),
    (
        "git.branch_failed",
        "could not change drafts: {error}",
        "no se pudo cambiar de borrador: {error}",
    ),
    (
        "git.tip.diff",
        "Shows the exact lines that changed since the last saved version.",
        "Muestra las líneas exactas que cambiaron desde la última versión guardada.",
    ),
    (
        "git.tip.branch_current",
        "This is the draft you are working in now.",
        "Este es el borrador en el que trabajas ahora.",
    ),
    (
        "git.tip.branch_switch",
        "Switch to this parallel draft.",
        "Cambiar a este borrador paralelo.",
    ),
    ("preview.code", "Code", "Código"),
    ("preview.preview", "Preview", "Vista previa"),
    (
        "preview.read_only",
        "Read-only preview · edit in Code",
        "Vista previa de solo lectura · edita en Código",
    ),
    (
        "preview.empty",
        "Nothing to preview yet.",
        "Nada que previsualizar todavía.",
    ),
    (
        "preview.tip",
        "⌘⌥P — Show Markdown with formatting. The file is never modified.",
        "⌘⌥P — Muestra el Markdown con formato. El archivo nunca se modifica.",
    ),
    (
        "menu.toggle_preview",
        "Toggle Markdown Preview",
        "Alternar vista previa de Markdown",
    ),
    ("github.section", "GitHub", "GitHub"),
    (
        "github.loading",
        "Checking GitHub connection…",
        "Comprobando la conexión con GitHub…",
    ),
    (
        "github.not_connected",
        "This project is not on GitHub yet.",
        "Este proyecto aún no está en GitHub.",
    ),
    (
        "github.publish",
        "Publish on GitHub…",
        "Publicar en GitHub…",
    ),
    (
        "github.publish_title",
        "Publish project on GitHub",
        "Publicar el proyecto en GitHub",
    ),
    (
        "github.publish_hint",
        "Creates a private repository in your GitHub account and uploads this draft. Nothing is made public without your confirmation.",
        "Crea un repositorio privado en tu cuenta de GitHub y sube este borrador. Nada se hace público sin tu confirmación.",
    ),
    (
        "github.repo_name",
        "Repository name",
        "Nombre del repositorio",
    ),
    ("github.repo_placeholder", "my-screenplay", "mi-guion"),
    (
        "github.private",
        "Private repository",
        "Repositorio privado",
    ),
    (
        "github.publish_confirm_hint",
        "Uploading creates the repository and pushes the current draft. You can change visibility later on GitHub.",
        "Al subir se crea el repositorio y se envía el borrador actual. Puedes cambiar la visibilidad después en GitHub.",
    ),
    (
        "github.publish_confirm",
        "Create and upload",
        "Crear y subir",
    ),
    ("github.publishing", "Publishing…", "Publicando…"),
    (
        "github.published",
        "Project published: {url}",
        "Proyecto publicado: {url}",
    ),
    (
        "github.publish_failed",
        "could not publish the project: {error}",
        "no se pudo publicar el proyecto: {error}",
    ),
    (
        "github.repo_required",
        "enter a repository name",
        "introduce un nombre de repositorio",
    ),
    (
        "github.remote_not_github",
        "This project already has a remote that is not GitHub. M13 only supports GitHub.",
        "Este proyecto ya tiene un remoto que no es de GitHub. M13 solo admite GitHub.",
    ),
    ("github.push", "Upload changes", "Subir cambios"),
    ("github.pull", "Bring changes", "Traer cambios"),
    (
        "github.pr",
        "Propose changes (PR)…",
        "Proponer cambios (PR)…",
    ),
    (
        "github.pending",
        "{ahead} to upload · {behind} to bring",
        "{ahead} por subir · {behind} por traer",
    ),
    (
        "github.tip.remote",
        "git remote -v — This project is linked to {repo} through “{remote}”.",
        "git remote -v — Este proyecto está vinculado con {repo} mediante «{remote}».",
    ),
    (
        "github.tip.push",
        "git push {remote} {branch} — Uploads your saved versions to GitHub.",
        "git push {remote} {branch} — Sube tus versiones guardadas a GitHub.",
    ),
    (
        "github.tip.pull",
        "git pull --ff-only {remote} {branch} — Brings the versions published on GitHub.",
        "git pull --ff-only {remote} {branch} — Trae las versiones publicadas en GitHub.",
    ),
    (
        "github.tip.pr",
        "gh pr create — Proposes integrating this draft into the main version.",
        "gh pr create — Propone integrar este borrador en la versión principal.",
    ),
    (
        "github.tip.publish",
        "POST /user/repos — Creates a repository in your GitHub account and uploads the project.",
        "POST /user/repos — Crea un repositorio en tu cuenta de GitHub y sube el proyecto.",
    ),
    (
        "github.no_remote",
        "Connect the project to GitHub first.",
        "Primero conecta el proyecto con GitHub.",
    ),
    (
        "github.token_required",
        "Add a GitHub token in Settings → Versions first.",
        "Añade un token de GitHub en Ajustes → Versiones.",
    ),
    (
        "github.push_title",
        "Upload changes to GitHub?",
        "¿Subir los cambios a GitHub?",
    ),
    (
        "github.push_body",
        "Draft {branch} will be uploaded to {remote}.",
        "El borrador {branch} se subirá a {remote}.",
    ),
    (
        "github.push_hint",
        "Only saved versions are uploaded. Nothing is published until you confirm.",
        "Solo se suben las versiones guardadas. No se publica nada hasta que confirmes.",
    ),
    ("github.push_confirm", "Upload", "Subir"),
    (
        "github.push_failed",
        "could not upload changes: {error}",
        "no se pudieron subir los cambios: {error}",
    ),
    (
        "github.pushed",
        "changes uploaded to GitHub",
        "cambios subidos a GitHub",
    ),
    (
        "github.pull_title",
        "Bring changes from GitHub?",
        "¿Traer cambios desde GitHub?",
    ),
    (
        "github.pull_body",
        "Draft {branch} will bring the latest versions from {remote}.",
        "El borrador {branch} traerá las últimas versiones de {remote}.",
    ),
    (
        "github.pull_hint",
        "Only fast-forward updates are applied; nothing is merged or overwritten automatically.",
        "Solo se aplican actualizaciones directas; no se fusiona ni sobrescribe nada automáticamente.",
    ),
    ("github.pull_confirm", "Bring", "Traer"),
    (
        "github.pull_failed",
        "could not bring changes: {error}",
        "no se pudieron traer los cambios: {error}",
    ),
    (
        "github.pulled",
        "latest versions brought from GitHub",
        "últimas versiones traídas de GitHub",
    ),
    (
        "github.create_branch_first",
        "Create a parallel draft before proposing changes.",
        "Crea un borrador paralelo antes de proponer cambios.",
    ),
    (
        "github.pr_title",
        "Propose changes on GitHub",
        "Proponer cambios en GitHub",
    ),
    (
        "github.pr_hint",
        "Opens a pull request so the team can review this draft. It does not merge anything.",
        "Abre una pull request para que el equipo revise este borrador. No fusiona nada.",
    ),
    ("github.pr_title_label", "Title", "Título"),
    ("github.pr_body_label", "Note", "Nota"),
    ("github.pr_base_label", "Into", "Hacia"),
    ("github.pr_create", "Create proposal", "Crear propuesta"),
    (
        "github.pr_required",
        "enter a title and a destination",
        "introduce un título y un destino",
    ),
    (
        "github.pr_same_branch",
        "the proposal needs a different destination than the current draft",
        "la propuesta necesita un destino distinto del borrador actual",
    ),
    (
        "github.pr_default_title",
        "Review {branch}",
        "Revisar {branch}",
    ),
    (
        "github.pr_created",
        "proposal created: {url}",
        "propuesta creada: {url}",
    ),
    (
        "github.pr_failed",
        "could not create the proposal: {error}",
        "no se pudo crear la propuesta: {error}",
    ),
    ("github.working", "Working…", "Trabajando…"),
    ("git.commit_title", "Save version", "Guardar versión"),
    (
        "git.commit_files",
        "{n} changed file(s) will be included.",
        "Se incluirán {n} archivo(s) con cambios.",
    ),
    (
        "git.commit_author",
        "Author: {name} <{email}>",
        "Autor: {name} <{email}>",
    ),
    (
        "git.commit_placeholder",
        "What changed in this version?",
        "¿Qué cambió en esta versión?",
    ),
    (
        "git.commit_help",
        "This saves a local snapshot. Uploading to GitHub is a separate action.",
        "Esto guarda una foto local. Subir a GitHub es una acción separada.",
    ),
    (
        "git.commit_message_required",
        "describe what changed",
        "describe qué cambió",
    ),
    ("git.saving", "Saving…", "Guardando…"),
    ("git.commit_save", "Save version", "Guardar versión"),
    ("git.commit_cancel", "Cancel", "Cancelar"),
    (
        "git.commit_failed",
        "could not save the version: {error}",
        "no se pudo guardar la versión: {error}",
    ),
    ("git.version_saved", "version saved", "versión guardada"),
    (
        "git.init_done",
        "version control started",
        "control de versiones iniciado",
    ),
    (
        "git.init_failed",
        "could not start version control: {error}",
        "no se pudo iniciar el control de versiones: {error}",
    ),
    (
        "panel.git_clean",
        "working tree clean",
        "árbol de trabajo limpio",
    ),
    ("panel.agent_title", "Agent", "Agente"),
    (
        "panel.agent_empty",
        "No active review. Select a scene and press ⌘R.",
        "No hay una revisión activa. Selecciona una escena y pulsa ⌘R.",
    ),
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
