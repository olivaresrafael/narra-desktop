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

Estado a 2026-11-03 (commits `dfb9e57`, `32b9c40`, `fb24441`):

- ✅ **`type.yaml` v2**: el parser es solo schema 2 (sin legacy). Lee
  `format`, `content_layout`, `write_roots`, `pinned_context`, `entities`
  como objetos con etiqueta localizada, `export`, `context_actions` y las
  secciones `agents`/`commands`/`skills`/`templates`/`instructions`, con
  validación estricta de referencias cruzadas.
- ✅ **Agentes portables**: `narra-core` carga `.narra/agents/<id>/AGENT.md`
  (frontmatter `name`/`description`/`mode`/`write_roots`/`skills`) junto a
  `agent.yaml`; `agent.yaml` gana si conviven. Los agentes se convierten en
  roles con trust de solo instrucciones y `mode` como perfil de permisos.
- ⚠️ **Comandos**: existe el loader `.narra/commands/*.md` (frontmatter
  `name`/`description`/`agent` + prompt) y la validación contra
  `type.yaml`, pero falta el **despacho** desde el menú `/` del chat y la
  UI. Ese es el resto de Fase 1/Fase 3.
- ✅ **Tools genéricas**: `read_scene`/`propose_scene_edit` retiradas;
  `propose_document_edit`/`propose_document_create` y el scaffold resuelven
  `write_roots` (ruta + extensiones) del paquete en cada llamada. Si el
  paquete no resuelve, no se puede escribir nada. El kernel sigue
  sandboxeando por encima.
- ✅ **Contexto pineado**: `context.rs` lee `pinned_context` del paquete;
  sin paquete no se pinea nada (no hay fallback `bible/`). El índice de
  archivos es genérico (texto), sin lógica de escenas.
- ⏳ **`context_actions`**: el contrato y la validación existen; falta el
  despacho en `narra-app` (convertir selección, crear archivo desde
  template, lanzar comando) y comprobar que el `target` cae en
  `write_roots`.
- ⏳ **Entidades**: el catálogo `character`/`location`/`object` sigue
  cerrado en `narra-app` (`NewFileKind`); el paquete usa ids propios
  (`personaje`, `mundo`) con etiquetas localizadas, lo que exige hacerlo
  data-driven.
- ⏳ **Plantillas y siembra**: `templates`/`instructions` se validan, pero
  el runtime de creación de proyecto aún no copia el paquete completo
  (Fase 2).

Este fixture sigue siendo **datos de prueba**: la suite Rust verifica la
integridad del paquete y que los loaders reales (skills, agentes, comandos,
parser v2) lo aceptan; el despacho de comandos y `context_actions` en la UI
es trabajo pendiente.
