# narra-desktop (working label)

Native desktop environment for audiovisual writing with AI-assisted agents.
**The final product name is undecided (D-010); `narra-desktop` is a replaceable working identifier.**

- License: GPL-3.0-or-later (D-015)
- Target platform for Phase 0/MVP: macOS on Apple Silicon only (D-014)
- Implementation lives in this repository; the product/engineering context pack (`DECISIONS.md`, specs, spike reports) lives alongside it as `../narra-project` (D-016)

> Nota: este repositorio contiene el código de implementación. Las especificaciones, decisiones (`DECISIONS.md`) y los informes de spikes viven en el context pack. El nombre `narra-desktop` es provisional.

## Status

Milestone 0 — repository and engineering foundation. No feature implementation yet: the editor spike (Milestone 1) gates everything after it.

## Layout

```text
narra-desktop/
├── LICENSE                     # GPL-3.0-or-later
├── .github/workflows/ci.yml    # fmt + clippy + test on macOS
└── crates/
    └── narra-core/             # UI-neutral application/agent core (placeholder)
```

`narra-core` is intentionally empty of features. Core services must not depend on desktop UI state (D-005) and must be invocable later by a CLI, so the crate boundary exists from day one.

## Development

Requires Rust stable (currently 1.95).

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same three commands on `macos-latest`.

## Conventions

- Decisions are recorded with IDs in the context pack `DECISIONS.md`; code and PRs reference those IDs rather than restating decisions.
- On-disk project directory names are English; visible UI labels are localized (D-017).
- Every dependency needs a stated justification and a license compatible with GPL-3.0 (no GPL-2.0-only).
- No secrets in the repository, in project files, in prompts or in logs; credentials go to the OS secret store.
