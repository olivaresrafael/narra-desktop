# narra-desktop (working label)

Native desktop environment for audiovisual writing with AI-assisted agents.
**The final product name is undecided (D-010); `narra-desktop` is a replaceable working identifier.**

- License: GPL-3.0-or-later (D-015)
- Target platform for Phase 0/MVP: macOS on Apple Silicon only (D-014)
- Implementation lives in this repository; the product/engineering context pack (`DECISIONS.md`, specs, spike reports) lives alongside it as `../narra-project` (D-016)

> Nota: este repositorio contiene el código de implementación. Las especificaciones, decisiones (`DECISIONS.md`) y los informes de spikes viven en el context pack. El nombre `narra-desktop` es provisional.

## Status

Milestones 0–1 foundation complete (editor spike + approved performance baselines). Milestone 2 complete: the project model — `project.yaml` schema v1 validation, create/open with the English template (D-017), boundary-safe document IO, the derived Fountain 1.1 parser and the disposable scene index — lives in `narra-core`. Milestone 3 (Spike D) complete in `narra-core::git`: local and remote Git operations over the system `git` executable — init/open, status, log, diff, commit, branches, merge with conflict reporting and abort, clone/fetch/pull/push — all cancellable, timeout-bounded and with secret-redacted errors (D-021). Milestone 4 in progress: UI-neutral agent task contracts and the `project_brief` task in `narra-core::agent`, run headless by `narra-harness` (Spike F complete). Slice 2a complete: one OpenAI-compatible model adapter (`OpenAiCompatAdapter`) verified against a real local Ollama and against a deterministic mock server, with the API key accepted only from the environment (D-018 clarified). Slice 2b complete: the `scene_edit` task turns a model rewrite into a reviewable `FileProposal` (hunks, provenance, `.narra/proposals/` storage) with hunk-level partial acceptance, rejection and content-hash stale detection; `narra-harness` drives the full propose/review/apply/reject/renew flow headless. Milestone 5 in progress (Spike E complete): slice 1 delivers `narra-core::provider` — a GitHub REST adapter behind a capability matrix with provider-confirmed results only and typed rate-limit/auth/permission errors; slice 2 adds token storage in the macOS Keychain (D-022, `keyring`) and the headless `provider` commands (`caps`, `login`, `logout`, `whoami`, `connect`).

## Layout

```text
narra-desktop/
├── LICENSE                     # GPL-3.0-or-later
├── .github/workflows/ci.yml    # fmt + clippy + test on macOS
└── crates/
    ├── narra-core/             # UI-neutral application/agent core
    │   └── src/manifest.rs     # project.yaml v1: parse/serialize/validate
    │   └── src/project.rs      # create/open project, template, index rebuild
    │   └── src/document.rs     # boundary-checked, atomic document IO
    │   └── src/fountain.rs     # derived Fountain 1.1 parser (index only)
    │   └── src/git/            # local Git via system git (cancellable ops)
    │   └── src/agent/          # agent task contracts + first task (project_brief)
    │   │                        + OpenAI-compatible model adapter (D-018)
    │   └── src/cancel.rs       # shared cooperative cancellation
    ├── narra-harness/          # headless agent runner: JSONL events, exit codes (Spike F)
    ├── narra-app/              # desktop app: writing + agent workspaces (M6)
    └── fixture-gen/            # deterministic Fountain fixture generator
```

`narra-core` carries the project model (Milestone 2). Core services must not depend on desktop UI state (D-005) and must be invocable later by a CLI, so the crate boundary exists from day one.

## Development

Requires Rust stable (currently 1.95).

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same three commands on `macos-latest`.

### Running the agent harness

`narra-harness` runs an agent task with no desktop window. The default
provider is the deterministic mock (no network, no key):

```sh
cargo run -p narra-harness -- brief <project-root>
```

To use a real model, point it at any OpenAI-compatible endpoint. For a local
Ollama server (no account, no cost):

```sh
cargo run -p narra-harness -- brief <project-root> \
    --adapter openai-compat --model qwen2.5vl:7b
```

The endpoint defaults to `http://localhost:11434/v1`; override with
`--base-url`. The API key is read **only** from the `OPENAI_API_KEY`
environment variable (unset = no key, which is correct for local servers) —
never pass it as an argument.

### Exporting a screenplay (M7)

Export is local-only: no credentials, no network, no model. The core owns
the layout, so the PDF pagination (Courier 12pt, 55-line pages, orphan
control, `(MORE)`/`(CONT'D)` dialogue splits) is deterministic and tested.

```sh
# PDF into <project>/exports/main.pdf (the app does the same with ⌘E)
cargo run -p narra-harness -- export <project-root>

# A shareable Fountain copy (title page from the manifest when absent)
cargo run -p narra-harness -- export <project-root> --format fountain --out copia.fountain
```

Installation, backup and troubleshooting live in
[`docs/INSTALL.md`](docs/INSTALL.md); `scripts/package_macos.sh` assembles
`Narra.app` for the reference machine.

Add `--stream` to any model task (`brief`, `propose`, `renew`) to receive the
text as it is generated: the JSONL stream then carries `model_delta` events
before the final `task_output`, and cancellation (Ctrl-C) takes effect
*between* chunks instead of waiting for the request to end. The final result
is identical with or without `--stream`.

### Reviewing edit proposals (M4 slice 2b)

The `scene_edit` task asks the model to rewrite one scene and persists the
answer as a **reviewable proposal** — canonical files are never written by
the task. Records live in `<project>/.narra/proposals/<uuid>.json` with the
line diff (hunks), provenance (task, adapter, model, timestamp, base
content hash) and lifecycle state.

```sh
# 1. Propose a rewrite of scene 2 (no canonical file is touched)
cargo run -p narra-harness -- propose <project-root> \
    --scene 2 --instruction "haz el diálogo más tenso"

# 2. Inspect what was proposed
cargo run -p narra-harness -- proposals <project-root> list
cargo run -p narra-harness -- proposals <project-root> show <proposal-id>

# 3. Accept everything, only some hunks (0-based), or reject
cargo run -p narra-harness -- proposals <project-root> apply <proposal-id>
cargo run -p narra-harness -- proposals <project-root> apply <proposal-id> --hunks 0,2
cargo run -p narra-harness -- proposals <project-root> reject <proposal-id>
```

Applying is refused (exit 1) when the target file changed after the
proposal was created: staleness is decided by content hash, so a later user
edit can never be silently overwritten. Renewed review re-runs the same
task against the current content and produces a fresh proposal:

```sh
cargo run -p narra-harness -- proposals <project-root> renew <proposal-id>
```

`renew` accepts the same adapter options as `propose` (mock by default).

### Permission-gated tool loop (M4 slice 3b)

The `loop` task lets the model request declared tools over the standard
OpenAI tool-calling wire. Every call is checked against an explicit
permission policy **before** execution — deny-by-default:

```sh
# Read-only tools work out of the box; writes are refused until granted.
cargo run -p narra-harness -- loop <project-root> \
    --instruction "rehaz el diálogo de la escena 1"

# Grant a tool by name (stages a reviewable proposal; canon untouched).
cargo run -p narra-harness -- loop <project-root> \
    --instruction "rehaz el diálogo de la escena 1" \
    --allow-tool propose_scene_edit

# Applying to the canonical file needs its own destructive grant.
cargo run -p narra-harness -- loop <project-root> \
    --instruction "aplica la propuesta <id>" \
    --allow-destructive apply_proposal
```

`--allow-tool` never implies `--allow-destructive` (a write grant cannot
silently unlock canonical writes). Each call appears as a `tool_executed`
event and in the final output's `data.tools` audit (decision + outcome),
including the denials: the model sees why it was refused, and the audit is
the record of what actually ran — not the model's claims.

### Agent packages (M15)

Agents can be extended without changing core code. A package is a directory
with one `agent.yaml`; only instruction-only packages are accepted in this
version (`trust: tools` is rejected):

```yaml
schema_version: 1
api_version: 1
id: documentalista
name: Documentalista
version: 1.0.0
description: Contrasta datos y fuentes del proyecto.
instructions: |
  Contrastas datos y citas siempre la fuente concreta.
tasks:
  - scene_review
trust: instructions
```

- Project scope: `<project>/.narra/agents/<id>/agent.yaml`.
- User scope: `<user-config>/agents/<id>/agent.yaml` (default
  `~/Library/Application Support/Narra/agents`; override the base with
  `NARRA_CONFIG_DIR`).
- Precedence: built-in < user package < project package < `<project>/agents.yaml`.
- Inspect effective roles (including origin, version and trust):

```sh
cargo run -p narra-harness -- roles <project-root> --user-dir <config-dir>
```

The desktop surfaces the same list read-only in **Ajustes → Agentes**.

### GitHub provider (M5 slices 1–3)

Remote provider integration starts with GitHub (D-018). The token is a
fine-grained PAT that the app stores in the **macOS Keychain** (D-022);
`GITHUB_TOKEN` works as the headless/CI fallback. The token is never an
argument (argv is world-readable via `ps`), never printed, and only the
provider's own 2xx response counts as success:

```sh
# Show exactly which operations this provider supports (what the UI lists).
cargo run -p narra-harness -- provider caps

# Store a token in the Keychain (verified against the API *before* storing).
pbpaste | cargo run -p narra-harness -- provider login
# …or, for automation/CI:
GITHUB_TOKEN=… cargo run -p narra-harness -- provider login --from-env

# Verify the stored token and connect a repository (read-only).
cargo run -p narra-harness -- provider whoami
cargo run -p narra-harness -- provider connect <owner>/<repo>

# Remove the stored token (idempotent).
cargo run -p narra-harness -- provider logout

# Pull requests (reads need no confirmation; writes require --confirm).
cargo run -p narra-harness -- pr <owner>/<repo> list --state open|closed|all
cargo run -p narra-harness -- pr <owner>/<repo> view <número>
cargo run -p narra-harness -- pr <owner>/<repo> create \
    --title <texto> --head <rama> --base <rama> [--body <texto>] --confirm
cargo run -p narra-harness -- pr <owner>/<repo> update <número> \
    [--title <texto>] [--body <texto>] [--state open|closed] --confirm
cargo run -p narra-harness -- pr <owner>/<repo> merge <número> \
    [--title <texto>] [--method merge|squash|rebase] --confirm
cargo run -p narra-harness -- pr <owner>/<repo> comment <número> \
    --body <texto> --confirm
```

Every command emits one JSON record on stdout. `NARRA_SECRET_STORE=memory`
switches to an in-memory store for tests and CI. Unsupported operations
(merge reviews today) are listed as absent by `caps` — the product shows
capability gaps instead of failing at request time.

**Remote writes are explicit.** `pr create/update/merge/comment` without
`--confirm` print a `pr_dry_run` record with the exact payload they *would*
send and exit 1 without any network request — an action is never implicit.
A merge whose provider verdict is `merged: false` (e.g. the head branch
moved) exits non-zero and surfaces the provider's message; it is never
reinterpreted as success.

### macOS build prerequisite (affects `narra-app`)

GPUI compiles Metal shaders at build time. If you see
`cannot execute tool 'metal' due to missing Metal Toolchain`:

```sh
xcodebuild -checkFirstLaunchStatus
xcodebuild -runFirstLaunch
xcodebuild -downloadComponent MetalToolchain
```

Recorded in the context pack `TECHNICAL_SPIKES.md` (Spike A, environment findings).

## Conventions

- Decisions are recorded with IDs in the context pack `DECISIONS.md`; code and PRs reference those IDs rather than restating decisions.
- On-disk project directory names are English; visible UI labels are localized (D-017).
- Every dependency needs a stated justification and a license compatible with GPL-3.0 (no GPL-2.0-only).
- No secrets in the repository, in project files, in prompts or in logs; credentials go to the OS secret store.
