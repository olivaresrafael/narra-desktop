# narra-desktop (working label)

Native desktop environment for audiovisual writing with AI-assisted agents.
**The final product name is undecided (D-010); `narra-desktop` is a replaceable working identifier.**

- License: GPL-3.0-or-later (D-015)
- Target platform for Phase 0/MVP: macOS on Apple Silicon only (D-014)
- Implementation lives in this repository; the product/engineering context pack (`DECISIONS.md`, specs, spike reports) lives alongside it as `../narra-project` (D-016)

> Nota: este repositorio contiene el código de implementación. Las especificaciones, decisiones (`DECISIONS.md`) y los informes de spikes viven en el context pack. El nombre `narra-desktop` es provisional.

## Status

Milestones 0–1 foundation complete (editor spike + approved performance baselines). Milestone 2 complete: the project model — `project.yaml` schema v1 validation, create/open with the English template (D-017), boundary-safe document IO, the derived Fountain 1.1 parser and the disposable scene index — lives in `narra-core`. Milestone 3 (Spike D) complete in `narra-core::git`: local and remote Git operations over the system `git` executable — init/open, status, log, diff, commit, branches, merge with conflict reporting and abort, clone/fetch/pull/push — all cancellable, timeout-bounded and with secret-redacted errors (D-021). Milestone 4 in progress: UI-neutral agent task contracts and the `project_brief` task in `narra-core::agent`, run headless by `narra-harness` (Spike F complete). Slice 2a complete: one OpenAI-compatible model adapter (`OpenAiCompatAdapter`) verified against a real local Ollama and against a deterministic mock server, with the API key accepted only from the environment (D-018 clarified). Slice 2b complete: the `scene_edit` task turns a model rewrite into a reviewable `FileProposal` (hunks, provenance, `.narra/proposals/` storage) with hunk-level partial acceptance, rejection and content-hash stale detection; `narra-harness` drives the full propose/review/apply/reject/renew flow headless.

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
    ├── spike-editor/           # GPUI editor spike (Milestone 1 evidence)
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

### macOS build prerequisite (affects `spike-editor`)

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
