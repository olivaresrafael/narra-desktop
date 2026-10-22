# narra-desktop (working label)

Native desktop environment for audiovisual writing with AI-assisted agents.
**The final product name is undecided (D-010); `narra-desktop` is a replaceable working identifier.**

- License: GPL-3.0-or-later (D-015)
- Target platform for Phase 0/MVP: macOS on Apple Silicon only (D-014)
- Implementation lives in this repository; the product/engineering context pack (`DECISIONS.md`, specs, spike reports) lives alongside it as `../narra-project` (D-016)

> Nota: este repositorio contiene el código de implementación. Las especificaciones, decisiones (`DECISIONS.md`) y los informes de spikes viven en el context pack. El nombre `narra-desktop` es provisional.

## Status

Milestones 0–1 foundation complete (editor spike + approved performance baselines). Milestone 2 complete: the project model — `project.yaml` schema v1 validation, create/open with the English template (D-017), boundary-safe document IO, the derived Fountain 1.1 parser and the disposable scene index — lives in `narra-core`. Milestone 3 in progress: local Git operations over the system `git` executable.

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
