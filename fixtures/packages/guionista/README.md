# Paquete spike `guionista` (Spike I, D-062)

Puerto de [guionista-kit](https://github.com/) (plantilla OpenCode de
referencia) a un paquete de tipo de proyecto Narra, para validar que
toda la lógica creativa puede vivir en datos y no en Rust.

## Qué contiene

- `type.yaml` — borrador del contrato **v2**: `format`, `content_layout`,
  `write_roots`, `pinned_context`, `entities`, `export`, `context_actions`
  y secciones `agents`/`commands`/`skills`/`templates`/`instructions`.
- `AGENTS.md` — contrato creativo superior del proyecto (se siembra en la
  raíz del proyecto nuevo).
- `.narra/agents/<id>/AGENT.md` — cuatro agentes portables (guionista,
  editor, investigador, personaje) con frontmatter YAML (`name`,
  `description`, `mode`, `write_roots`, `skills`).
- `.narra/commands/*.md` — ocho comandos (`/idea`, `/personaje`,
  `/estructura`, `/escaleta`, `/escena`, `/revisar`, `/investigar`,
  `/voz`).
- `.narra/skills/*/SKILL.md` — nueve skills de metodología, incluidos los
  métodos de estructura como archivos de referencia.
- `templates/` — árbol semilla (`biblia/premisa.md`, cronología y
  fichas template).

## Qué valida este spike (criterios de D-062)

1. Las propuestas del modelo caen bajo las raíces que declara el paquete,
   no bajo constantes de Rust.
2. `/escaleta` es invocable como comando.
3. El contexto pineado trae la biblia sin que ningún código nombre `bible/`.
4. Todo cambio llega como diff revisable; los archivos canónicos no se
   tocan hasta aplicar.
5. Añadir una acción creativa nueva al menú contextual es editar solo este
   paquete.

## Gaps de contrato detectados (entradas para Fase 1)

- **`type.yaml` v2**: el parser actual (D-045) ignora `format`,
  `content_layout`, `write_roots`, `pinned_context`, `entities` como lista
  de objetos, `context_actions` y las secciones de agentes/comandos.
  Las capacidades v1 (`capabilities.screenplay`, `entity_kinds`,
  `explorer_groups`) son el equivalente plano que hay que migrar.
- **Agentes portables**: `narra-core` hoy solo carga `agent.yaml`
  (instruction-only). Falta el loader de `AGENT.md` con frontmatter y los
  campos `mode`/`write_roots`/`skills`; el trust de instrucciones ya existe
  y debe reaprovecharse.
- **Comandos**: no hay concepto de comando slashable en el runtime; el menú
  `/` del chat solo resuelve skills. Falta `.narra/commands/*.md`
  (frontmatter `name`/`description`/`agent`) y su despacho.
- **Tools genéricas**: `read_scene`/`propose_scene_edit` y
  `scaffold::ALLOWED_ROOTS` siguen hardcodeados; hay que sustituirlos por
  `read_document`/`propose_document_edit`/`propose_new_files` parametrizados
  por `write_roots`.
- **Contexto pineado**: `context.rs` cae a `bible` por defecto; debe leer
  `pinned_context` del paquete y solo pinear el índice de escenas cuando
  `format: fountain`.
- **`context_actions`**: no existe el despacho. El motor necesita mapear
  cada `kind` a una acción existente (convertir selección, crear archivo
  desde template, lanzar comando) y validar que el `target` cae dentro de
  `write_roots`.
- **Entidades**: el catálogo `character`/`location`/`object` está cerrado en
  `narra-app` (`NewFileKind`); el paquete usa ids propios (`personaje`,
  `mundo`) y etiquetas localizadas, lo que exige hacerlo data-driven.

Mientras esos gaps no se cierren, este fixture es **datos de prueba**: la
prueba Rust asociada solo verifica la integridad del paquete (frontmatter,
rutas y referencias cruzadas), no el runtime v2.
