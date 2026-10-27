# Installing Narra (macOS)

Narra is a local-first screenwriting app. Projects are plain files on your
disk; nothing here needs an account, a cloud connection or a model
provider. Phase 0 and the MVP target **macOS on Apple Silicon** (D-014);
other platforms are not tested and make no claims.

## From source (development and the reference machine)

Requirements: macOS with the Xcode command-line tools, Rust 1.95+ (the
workspace pins `rust-version`), and the **Metal toolchain** — GPUI compiles
Metal shaders at build time and the build fails with
`cannot execute tool 'metal'` when it is missing (install it from Xcode or
`xcodebuild -downloadComponent MetalToolchain`).

```sh
git clone https://github.com/olivaresrafael/narra-desktop.git
cd narra-desktop
cargo build --release -p narra-app
./target/release/narra-app --project /path/to/a/project
```

## Packaged app bundle

```sh
scripts/package_macos.sh
```

This builds the release binary, assembles `target/release/Narra.app`
(Info.plist, bundle identifier `io.github.olivaresrafael.narra`, version
from the workspace) and runs a launch smoke test with no credentials in
the environment. Pass `--no-smoke` on hosts without a window server.

The bundle is **unsigned and unnotarized** — it is meant for local builds
and the reference machine. If you copy it from another machine and macOS
refuses to open it, right-click → Open once, or remove the quarantine
attribute:

```sh
xattr -d com.apple.quarantine Narra.app
```

## Opening a project

`Narra.app` opens the project named by `--project` when launched from a
terminal. A project is any directory with a valid `project.yaml` (see
`DATA_MODEL.md` for the schema); create one with `narra-core`'s
`create_project` or by copying the template layout (`screenplay/`,
`bible/`, `structure/`, `research/`, `notes/`).

Editing, saving (⌘S) and PDF export (⌘E) never touch the network. Agent
tasks default to the offline mock adapter; a real model is opt-in through
the `--agent-*` flags and `OPENAI_API_KEY`.

## Backup

Everything that matters is plain text and portable:

- **Canonical content**: `screenplay/`, `bible/`, `structure/`,
  `research/`, `notes/` and `project.yaml`. Back these up with any file
  tool, Time Machine, or Git — a local Git repository is the recommended
  default (the app's Git/​GitHub integration builds on it).
- **`.narra/`**: local state (proposal records, settings). Worth backing
  up if you want pending proposals to survive a restore; never canonical
  story content.
- **`exports/`**: generated artifacts (PDF/Fountain copies). Safe to
  delete and regenerate with ⌘E or `narra-harness export`.

There is no database and no hidden account: restoring the project folder
restores the writing.

## Troubleshooting

| Symptom | Cause | Fix |
| --- | --- | --- |
| Build fails with `cannot execute tool 'metal'` | Metal toolchain missing | Install it via Xcode (`xcodebuild -downloadComponent MetalToolchain`) |
| macOS says the app is damaged/unsigned | Quarantine on a copied bundle | Right-click → Open once, or `xattr -d com.apple.quarantine Narra.app` |
| App cannot open files in Documents/Desktop | macOS privacy prompt (TCC) | Accept the prompt; the app only reads the project folder you open |
| Agent tasks fail with auth errors | No key/model configured | Use the default mock, or set `OPENAI_API_KEY` and `--agent-model` for a local OpenAI-compatible server |
| `provider whoami` fails | Token missing/expired in Keychain | `narra-harness provider login`, then `provider whoami` again |
| Export says "export failed: …" | Unwritable output path | Check permissions on `<project>/exports/` or pass `--out` |
| Proposals refuse to apply | Stale base (file changed after the proposal) | Review the fresh diff; the core refuses stale applies by design |

## Verifying an offline session

1. Quit any VPN/network helper you do not need (nothing here requires one).
2. Unset credentials: `env -u OPENAI_API_KEY -u GITHUB_TOKEN …`.
3. Open a project, edit a scene, save with ⌘S and export with ⌘E.
4. Open the PDF outside the app (Preview, any reader).

The editor, save and export paths perform no network calls; only agent
tasks and provider commands ever reach a remote endpoint, and they say so
explicitly in their notices and records.
