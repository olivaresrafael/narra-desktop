//! Headless agent harness (Milestone 4, Spike F).
//!
//! Runs agent tasks and proposal review with **no desktop window**:
//!
//! - task input is explicit and serializable (CLI arguments; a JSON input
//!   variant lands with the first real provider adapter);
//! - progress events are UI-neutral, emitted as JSONL on stdout;
//! - Ctrl-C cancels cooperatively;
//! - output is both human-readable (stderr summary) and machine-readable
//!   (final record JSONL on stdout);
//! - exit status is defined: 0 success, 1 failure, 2 cancelled.
//!
//! Usage:
//!   narra-harness brief <project-root> [options]
//!   narra-harness propose <project-root> --scene <n> --instruction <texto> [options]
//!   narra-harness review <project-root> --scene <n> [--focus <texto>] [options]
//!   narra-harness proposals <project-root> [list]
//!   narra-harness proposals <project-root> show <proposal-id>
//!   narra-harness proposals <project-root> apply <proposal-id> [--hunks 0,2]
//!   narra-harness proposals <project-root> reject <proposal-id>
//!   narra-harness proposals <project-root> renew <proposal-id> [options]
//!   narra-harness provider caps
//!   narra-harness provider login [--from-env] [--api-url <url>]
//!   narra-harness provider logout
//!   narra-harness provider whoami [--api-url <url>]
//!   narra-harness provider connect <owner>/<repo> [--api-url <url>]
//!   narra-harness pr <owner>/<repo> list [--state open|closed|all]
//!   narra-harness pr <owner>/<repo> view <número>
//!   narra-harness pr <owner>/<repo> create --title <t> --head <rama> --base <rama>
//!       [--body <t>] --confirm
//!   narra-harness pr <owner>/<repo> update <número> [--title <t>] [--body <t>]
//!       [--state open|closed] --confirm
//!   narra-harness pr <owner>/<repo> merge <número> [--title <t>]
//!       [--method merge|squash|rebase] --confirm
//!   narra-harness pr <owner>/<repo> comment <número> --body <t> --confirm
//!
//! Options:
//!   --timeout <secs>   task timeout (default 120)
//!   --latency <ms>     mock adapter simulated latency (cancellation demos)
//!   --adapter <name>   mock (default) | openai-compat
//!   --base-url <url>   openai-compat endpoint (default http://localhost:11434/v1)
//!   --model <name>     model name for openai-compat (required with it)
//!   --stream           stream text deltas (JSONL `model_delta` events)
//!   --allow-tool <n>     (loop) grant a propose-write tool by name
//!   --allow-destructive <n> (loop) grant a destructive tool by name
//!   --scene <n>        1-based scene number (propose and review)
//!   --instruction <t>  writer's instruction (propose only)
//!   --focus <t>        reviewer focus/question (review only)
//!   --hunks <a,b>      0-based hunk indices to accept (apply only; default all)
//!
//! `apply` refuses a stale proposal (the file changed after it was created)
//! and exits 1; `renew` re-runs the originating task against the current
//! content to produce a fresh proposal for renewed review.
//!
//! The provider API key is read **only** from the `OPENAI_API_KEY`
//! environment variable (empty/unset = no key, right for local servers).
//! It is never accepted as a command-line argument — argv is world-readable
//! via `ps`. The same rule applies to GitHub tokens: `provider login` reads
//! the token from stdin (or `--from-env`), stores it in the OS keychain and
//! everything else resolves it from there. `NARRA_SECRET_STORE=memory`
//! switches the store to in-memory for tests and CI.
//!
//! Remote writes (`pr create/update/merge/comment`) additionally require
//! `--confirm`: without it the harness prints a dry-run record of exactly
//! what would be sent and exits 1 without touching the network — a remote
//! action is never implicit. `merged: false` from the provider is surfaced
//! as a non-zero exit, never reinterpreted as success.

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use narra_core::agent::proposal;
use narra_core::agent::{
    AgentError, GrantPolicy, MockAdapter, ModelAdapter, OpenAiCompatAdapter, OpenAiCompatConfig,
    ProjectBriefTask, SceneEditTask, SceneReviewTask, TaskEvent, TaskOptions, ToolLoopTask,
};
use narra_core::cancel::CancellationToken;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parsed = match parse_args(&args) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("{message}");
            eprintln!("{}", USAGE);
            return ExitCode::from(1);
        }
    };

    // Local review operations never touch the network or a model.
    if let Command::Proposals { root, action } = &parsed.command {
        return run_proposals(root, action);
    }

    // Provider commands use the token store and (some) the REST API, but
    // never a model adapter.
    if let Command::Provider { action } = &parsed.command {
        return run_provider(action, &parsed.provider, parsed.timeout);
    }

    // Pull-request commands: same token rules, same no-false-success rule;
    // writes additionally need `--confirm`.
    if let Command::Pr { repo, action } = &parsed.command {
        return run_pr(repo, action, &parsed.provider, parsed.timeout);
    }

    let token = CancellationToken::new();
    // Ctrl-C sets the token; the task observes it between stages and the
    // mock adapter aborts within ~5 ms. `ctrlc` is the minimal, maintained
    // (MIT/Apache-2.0) way to receive SIGINT without unsafe signal code.
    {
        let token = token.clone();
        let flag = Arc::new(AtomicBool::new(false));
        let handler_flag = flag.clone();
        if ctrlc::set_handler(move || {
            if handler_flag.swap(true, Ordering::SeqCst) {
                // Second Ctrl-C: hard exit for an unresponsive provider.
                std::process::exit(130);
            }
            token.cancel();
        })
        .is_err()
        {
            eprintln!("aviso: no se pudo instalar el handler de Ctrl-C");
        }
    }

    let options = TaskOptions {
        timeout: parsed.timeout,
        cancel: token,
        stream: parsed.stream,
    };
    let adapter: Box<dyn ModelAdapter> = match parsed.adapter {
        AdapterChoice::Mock => Box::new(MockAdapter::with_latency(parsed.latency)),
        AdapterChoice::OpenAiCompat(OpenAiCompatChoice {
            base_url,
            model,
            api_key,
        }) => Box::new(OpenAiCompatAdapter::new(OpenAiCompatConfig {
            base_url,
            api_key,
            model,
            timeout: parsed.timeout,
            extra_headers: Vec::new(),
        })),
    };

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut emit = |event: TaskEvent| {
        // One JSON object per line: machine-readable, streamable, greppable.
        if let Ok(line) = serde_json::to_string(&event) {
            let _ = writeln!(out, "{line}");
            let _ = out.flush();
        }
    };

    let result = match parsed.command {
        Command::Brief { root } => {
            ProjectBriefTask::new(root).run(adapter.as_ref(), &options, &mut emit)
        }
        Command::Propose {
            root,
            scene,
            instruction,
        } => {
            SceneEditTask::new(root, scene, instruction).run(adapter.as_ref(), &options, &mut emit)
        }
        Command::Review { root, scene, focus } => {
            SceneReviewTask::new(root, scene, focus).run(adapter.as_ref(), &options, &mut emit)
        }
        Command::Renew { root, id } => proposal::load(Path::new(&root), &id)
            .map_err(AgentError::Proposal)
            .and_then(|record| SceneEditTask::from_proposal(root, &record))
            .and_then(|task| task.run(adapter.as_ref(), &options, &mut emit)),
        Command::Loop {
            root,
            instruction,
            allowed,
            destructive,
        } => {
            // Deny-by-default: only the named tools are granted, and a
            // destructive grant must name the tool explicitly. In headless
            // mode `Ask` becomes an actionable denial in the audit trail.
            let mut policy = GrantPolicy::new();
            for tool in &allowed {
                policy = policy.allow(tool.clone());
            }
            for tool in &destructive {
                policy = policy.allow_destructive(tool.clone());
            }
            ToolLoopTask::new(root, instruction).run(
                adapter.as_ref(),
                &mut policy,
                &options,
                &mut emit,
            )
        }
        // Handled before the adapter is built.
        Command::Proposals { .. } | Command::Provider { .. } | Command::Pr { .. } => {
            unreachable!("proposals, provider y pr run locally")
        }
    };

    match result {
        Ok(output) => {
            let record = serde_json::json!({
                "record": "task_output",
                "exit": output.exit,
                "summary": output.summary,
                "data": output.data,
                "sources": output.sources,
            });
            let _ = writeln!(out, "{}", serde_json::to_string(&record).expect("json"));
            let _ = out.flush();
            // Human-readable summary also goes to stderr for interactive use.
            eprintln!("{}", output.summary);
            ExitCode::from(output.exit.exit_code() as u8)
        }
        Err(error) => {
            let message = error.to_string();
            let event = match error {
                AgentError::Cancelled => TaskEvent::Cancelled,
                _ => TaskEvent::Failed {
                    error: message.clone(),
                },
            };
            let code = match event {
                TaskEvent::Cancelled => 2,
                _ => 1,
            };
            if let Ok(line) = serde_json::to_string(&event) {
                let _ = writeln!(out, "{line}");
            }
            let _ = writeln!(
                out,
                "{}",
                serde_json::json!({ "record": "task_output", "exit": "failure", "error": message })
            );
            eprintln!("tarea fallida: {message}");
            ExitCode::from(code)
        }
    }
}

const USAGE: &str = "uso: narra-harness brief <project-root> [--timeout <seg>] [--latency <ms>]
       [--adapter mock|openai-compat] [--base-url <url>] [--model <nombre>] [--stream]
     narra-harness propose <project-root> --scene <n> --instruction <texto> [mismas opciones]
     narra-harness review <project-root> --scene <n> [--focus <texto>] [mismas opciones]
     narra-harness proposals <project-root> [list]
     narra-harness proposals <project-root> show <id>
     narra-harness proposals <project-root> apply <id> [--hunks 0,2]
     narra-harness proposals <project-root> reject <id>
     narra-harness proposals <project-root> renew <id> [--adapter ...] [--model ...]
     narra-harness loop <project-root> --instruction <texto> [--allow-tool <nombre>]...
         [--allow-destructive <nombre>]... [mismas opciones de modelo]
     narra-harness provider caps
     narra-harness provider login [--from-env] [--api-url <url>]
     narra-harness provider logout
     narra-harness provider whoami [--api-url <url>]
     narra-harness provider connect <owner>/<repo> [--api-url <url>]
     narra-harness pr <owner>/<repo> list [--state open|closed|all] [--api-url <url>]
     narra-harness pr <owner>/<repo> view <número> [--api-url <url>]
     narra-harness pr <owner>/<repo> create --title <t> --head <rama> --base <rama>
         [--body <t>] --confirm [--api-url <url>]
     narra-harness pr <owner>/<repo> update <número> [--title <t>] [--body <t>]
         [--state open|closed] --confirm [--api-url <url>]
     narra-harness pr <owner>/<repo> merge <número> [--title <t>]
         [--method merge|squash|rebase] --confirm [--api-url <url>]
     narra-harness pr <owner>/<repo> comment <número> --body <t> --confirm [--api-url <url>]";

#[derive(Debug)]
struct Parsed {
    command: Command,
    timeout: Duration,
    latency: Duration,
    adapter: AdapterChoice,
    stream: bool,
    provider: ProviderOptions,
}

/// Options for the `provider` subcommands.
#[derive(Debug, Default)]
struct ProviderOptions {
    /// API root override (tests/mocks; GHES support later). Default:
    /// GitHub.com.
    api_url: Option<String>,
}

#[derive(Debug)]
enum Command {
    Brief {
        root: String,
    },
    Propose {
        root: String,
        scene: usize,
        instruction: String,
    },
    Review {
        root: String,
        scene: usize,
        focus: String,
    },
    Renew {
        root: String,
        id: String,
    },
    Proposals {
        root: String,
        action: ProposalsAction,
    },
    /// Tool loop: the model requests declared tools under an explicit
    /// permission policy (deny-by-default; see --allow-tool).
    Loop {
        root: String,
        instruction: String,
        allowed: Vec<String>,
        destructive: Vec<String>,
    },
    /// Remote provider operations (M5): capability matrix, token storage
    /// in OS secret storage, auth check and repository connect.
    Provider {
        action: ProviderAction,
    },
    /// Pull-request operations over the provider adapter (M5 slice 3).
    /// Reads are plain; writes require `--confirm` (explicit confirmation
    /// for remote actions, AGENTS.md).
    Pr {
        repo: String,
        action: PrAction,
    },
}

/// Provider subactions. The token itself is never an argument: `login`
/// reads it from stdin (or `--from-env`), everything else resolves it from
/// the keychain with `GITHUB_TOKEN` as the headless fallback (D-022).
#[derive(Debug)]
enum ProviderAction {
    /// Print the GitHub capability matrix (what the UI must show).
    Caps,
    /// Verify a token against the API and store it in the OS keychain.
    Login { from_env: bool },
    /// Remove the stored token (idempotent).
    Logout,
    /// Verify the resolved token (`GET /user`).
    Whoami,
    /// "Connect" a repository: `GET /repos/{owner}/{repo}`.
    Connect { repo: String },
}

/// Pull-request subactions. Reads (`List`, `View`) need no confirmation;
/// every write carries a `confirm` flag: without it the harness emits a
/// dry-run record and exits 1 without any network request (AGENTS.md:
/// remote actions are explicit, and a remote action is never implicit).
#[derive(Debug)]
enum PrAction {
    /// `GET /repos/{owner}/{repo}/pulls?state=…` (open|closed|all).
    List { state: String },
    /// `GET /repos/{owner}/{repo}/pulls/{number}`.
    View { number: u64 },
    /// `POST /repos/{owner}/{repo}/pulls` — provider-confirmed creation.
    Create {
        title: String,
        head: String,
        base: String,
        body: Option<String>,
        confirm: bool,
    },
    /// `PATCH /repos/{owner}/{repo}/pulls/{number}` — title/body/state.
    Update {
        number: u64,
        title: Option<String>,
        body: Option<String>,
        state: Option<String>,
        confirm: bool,
    },
    /// `PUT /repos/{owner}/{repo}/pulls/{number}/merge`. The provider's
    /// own verdict (`merged: false` on a 200) is surfaced as failure.
    Merge {
        number: u64,
        commit_title: Option<String>,
        method: Option<String>,
        confirm: bool,
    },
    /// `POST /repos/{owner}/{repo}/issues/{number}/comments`.
    Comment {
        number: u64,
        body: String,
        confirm: bool,
    },
}

impl PrAction {
    /// The wire action name (dry-run records and error records).
    fn name(&self) -> &'static str {
        match self {
            Self::List { .. } => "list",
            Self::View { .. } => "view",
            Self::Create { .. } => "create",
            Self::Update { .. } => "update",
            Self::Merge { .. } => "merge",
            Self::Comment { .. } => "comment",
        }
    }

    /// `true` for remote writes (they require `--confirm`).
    fn is_write(&self) -> bool {
        matches!(
            self,
            Self::Create { .. } | Self::Update { .. } | Self::Merge { .. } | Self::Comment { .. }
        )
    }

    /// `true` for a write that was parsed *without* `--confirm`.
    fn needs_confirmation(&self) -> bool {
        self.is_write()
            && !matches!(
                self,
                Self::Create { confirm: true, .. }
                    | Self::Update { confirm: true, .. }
                    | Self::Merge { confirm: true, .. }
                    | Self::Comment { confirm: true, .. }
            )
    }

    /// The exact payload a dry run would send (JSON), so the user confirms
    /// what will really be written, never a paraphrase.
    fn dry_run_payload(&self) -> serde_json::Value {
        match self {
            Self::Create {
                title,
                head,
                base,
                body,
                ..
            } => serde_json::json!({ "title": title, "head": head, "base": base, "body": body }),
            Self::Update {
                title, body, state, ..
            } => serde_json::json!({ "title": title, "body": body, "state": state }),
            Self::Merge {
                commit_title,
                method,
                ..
            } => serde_json::json!({ "commit_title": commit_title, "merge_method": method }),
            Self::Comment { body, .. } => serde_json::json!({ "body": body }),
            Self::List { .. } | Self::View { .. } => serde_json::json!({}),
        }
    }
}

/// Local review operations: no model, no events, JSONL records on stdout.
#[derive(Debug)]
enum ProposalsAction {
    List,
    Show(String),
    Apply(String, Option<Vec<usize>>),
    Reject(String),
}

impl Command {
    /// Commands that call a model adapter (the others must not take
    /// provider/latency options).
    fn runs_model(&self) -> bool {
        matches!(
            self,
            Self::Brief { .. } | Self::Propose { .. } | Self::Renew { .. } | Self::Loop { .. }
        )
    }
}

/// Which provider to use. The API key itself is never an argument.
#[derive(Debug)]
enum AdapterChoice {
    Mock,
    OpenAiCompat(OpenAiCompatChoice),
}

/// The OpenAI-compatible provider settings. `Debug` is manual so the key
/// can never reach logs, panics or test output.
struct OpenAiCompatChoice {
    base_url: String,
    model: String,
    api_key: Option<String>,
}

impl std::fmt::Debug for OpenAiCompatChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiCompatChoice")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

/// The environment variable the harness reads the provider key from.
const API_KEY_ENV: &str = "OPENAI_API_KEY";

fn parse_args(args: &[String]) -> Result<Parsed, String> {
    let mut timeout = Duration::from_secs(120);
    let mut latency = Duration::ZERO;
    let mut adapter = AdapterChoice::Mock;
    let mut base_url: Option<String> = None;
    let mut model: Option<String> = None;
    let mut scene: Option<usize> = None;
    let mut instruction: Option<String> = None;
    let mut focus: Option<String> = None;
    let mut hunks: Option<Vec<usize>> = None;
    let mut model_options_seen = false;
    let mut stream = false;
    let mut allow_tool: Vec<String> = Vec::new();
    let mut allow_destructive: Vec<String> = Vec::new();
    let mut api_url: Option<String> = None;
    let mut from_env = false;
    // `pr` write/read options.
    let mut title: Option<String> = None;
    let mut head: Option<String> = None;
    let mut base: Option<String> = None;
    let mut body: Option<String> = None;
    let mut state: Option<String> = None;
    let mut method: Option<String> = None;
    let mut confirm = false;
    let mut positional: Vec<&str> = Vec::new();

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--timeout" => {
                let value = iter.next().ok_or("--timeout requiere un valor")?;
                let secs: u64 = value
                    .parse()
                    .map_err(|_| format!("--timeout: «{value}» no es un número"))?;
                timeout = Duration::from_secs(secs);
            }
            "--latency" => {
                let value = iter.next().ok_or("--latency requiere un valor")?;
                let ms: u64 = value
                    .parse()
                    .map_err(|_| format!("--latency: «{value}» no es un número"))?;
                latency = Duration::from_millis(ms);
                model_options_seen = true;
            }
            "--stream" => {
                stream = true;
                model_options_seen = true;
            }
            "--allow-tool" => {
                let value = iter.next().ok_or("--allow-tool requiere un nombre")?;
                allow_tool.push((*value).clone());
            }
            "--allow-destructive" => {
                let value = iter
                    .next()
                    .ok_or("--allow-destructive requiere un nombre")?;
                allow_destructive.push((*value).clone());
            }
            "--adapter" => {
                let value = iter.next().ok_or("--adapter requiere un valor")?;
                model_options_seen = true;
                adapter = match value.as_str() {
                    "mock" => AdapterChoice::Mock,
                    "openai-compat" => AdapterChoice::OpenAiCompat(OpenAiCompatChoice {
                        base_url: narra_core::agent::LOCAL_DEFAULT_BASE_URL.to_string(),
                        model: String::new(),
                        api_key: None,
                    }),
                    other => {
                        return Err(format!(
                            "adapter desconocido: «{other}» (esperado mock u openai-compat)"
                        ));
                    }
                };
            }
            "--base-url" => {
                let value = iter.next().ok_or("--base-url requiere un valor")?;
                base_url = Some((*value).clone());
                model_options_seen = true;
            }
            "--model" => {
                let value = iter.next().ok_or("--model requiere un valor")?;
                model = Some((*value).clone());
                model_options_seen = true;
            }
            "--scene" => {
                let value = iter.next().ok_or("--scene requiere un valor")?;
                scene = Some(value.parse().map_err(|_| {
                    format!("--scene: «{value}» no es un número (escenas desde la 1)")
                })?);
            }
            "--instruction" => {
                let value = iter.next().ok_or("--instruction requiere un valor")?;
                instruction = Some((*value).clone());
            }
            "--focus" => {
                let value = iter.next().ok_or("--focus requiere un valor")?;
                focus = Some((*value).clone());
            }
            "--hunks" => {
                let value = iter.next().ok_or("--hunks requiere un valor")?;
                hunks = Some(parse_hunks(value)?);
            }
            "--api-url" => {
                let value = iter.next().ok_or("--api-url requiere un valor")?;
                api_url = Some((*value).clone());
            }
            "--from-env" => {
                from_env = true;
            }
            "--title" => {
                let value = iter.next().ok_or("--title requiere un valor")?;
                title = Some((*value).clone());
            }
            "--head" => {
                let value = iter.next().ok_or("--head requiere una rama")?;
                head = Some((*value).clone());
            }
            "--base" => {
                let value = iter.next().ok_or("--base requiere una rama")?;
                base = Some((*value).clone());
            }
            "--body" => {
                let value = iter.next().ok_or("--body requiere un texto")?;
                body = Some((*value).clone());
            }
            "--state" => {
                let value = iter.next().ok_or("--state requiere un valor")?;
                state = Some((*value).clone());
            }
            "--method" => {
                let value = iter.next().ok_or("--method requiere un valor")?;
                method = Some((*value).clone());
            }
            "--confirm" => {
                confirm = true;
            }
            other if other.starts_with('-') => {
                return Err(format!("opción desconocida: {other}"));
            }
            other => positional.push(other),
        }
    }

    // Late options only make sense together with --adapter openai-compat.
    if (base_url.is_some() || model.is_some())
        && !matches!(adapter, AdapterChoice::OpenAiCompat { .. })
    {
        return Err(
            "--base-url/--model solo se pueden usar con --adapter openai-compat".to_string(),
        );
    }
    if let AdapterChoice::OpenAiCompat(OpenAiCompatChoice {
        base_url: url,
        model: name,
        api_key,
    }) = &mut adapter
    {
        if let Some(value) = base_url {
            *url = value;
        }
        match model {
            Some(value) => *name = value,
            None => return Err("--adapter openai-compat requiere --model <nombre>".to_string()),
        }
        // Key from the environment only: never from argv (visible in `ps`).
        *api_key = std::env::var(API_KEY_ENV)
            .ok()
            .filter(|key| !key.is_empty());
    }

    // Capture presence before the match consumes the options.
    let scene_seen = scene.is_some();
    let instruction_seen = instruction.is_some();
    let focus_seen = focus.is_some();
    let hunks_seen = hunks.is_some();
    let allow_seen = !allow_tool.is_empty() || !allow_destructive.is_empty();
    let api_url_seen = api_url.is_some();
    let from_env_seen = from_env;
    let title_seen = title.is_some();
    let head_seen = head.is_some();
    let base_seen = base.is_some();
    let body_seen = body.is_some();
    let state_seen = state.is_some();
    let method_seen = method.is_some();
    let confirm_seen = confirm;

    let command = match positional.as_slice() {
        ["brief", root] if !root.is_empty() => Command::Brief {
            root: (*root).to_string(),
        },
        ["propose", root] if !root.is_empty() => Command::Propose {
            root: (*root).to_string(),
            scene: scene.ok_or("--propose requiere --scene <número>")?,
            instruction: instruction.ok_or("--propose requiere --instruction <texto>")?,
        },
        ["review", root] if !root.is_empty() => Command::Review {
            root: (*root).to_string(),
            scene: scene.ok_or("--review requiere --scene <número>")?,
            focus: focus.unwrap_or_default(),
        },
        ["proposals", root] | ["proposals", root, "list"] if !root.is_empty() => {
            Command::Proposals {
                root: (*root).to_string(),
                action: ProposalsAction::List,
            }
        }
        ["proposals", root, "show", id] if !root.is_empty() && !id.is_empty() => {
            Command::Proposals {
                root: (*root).to_string(),
                action: ProposalsAction::Show((*id).to_string()),
            }
        }
        ["proposals", root, "apply", id] if !root.is_empty() && !id.is_empty() => {
            Command::Proposals {
                root: (*root).to_string(),
                action: ProposalsAction::Apply((*id).to_string(), hunks.take()),
            }
        }
        ["proposals", root, "reject", id] if !root.is_empty() && !id.is_empty() => {
            Command::Proposals {
                root: (*root).to_string(),
                action: ProposalsAction::Reject((*id).to_string()),
            }
        }
        ["proposals", root, "renew", id] if !root.is_empty() && !id.is_empty() => Command::Renew {
            root: (*root).to_string(),
            id: (*id).to_string(),
        },
        ["loop", root] if !root.is_empty() => Command::Loop {
            root: (*root).to_string(),
            instruction: instruction
                .take()
                .ok_or("--loop requiere --instruction <texto>")?,
            allowed: std::mem::take(&mut allow_tool),
            destructive: std::mem::take(&mut allow_destructive),
        },
        ["provider", "caps"] => Command::Provider {
            action: ProviderAction::Caps,
        },
        ["provider", "login"] => Command::Provider {
            action: ProviderAction::Login { from_env },
        },
        ["provider", "logout"] => Command::Provider {
            action: ProviderAction::Logout,
        },
        ["provider", "whoami"] => Command::Provider {
            action: ProviderAction::Whoami,
        },
        ["provider", "connect", repo] if !repo.is_empty() && repo.contains('/') => {
            Command::Provider {
                action: ProviderAction::Connect {
                    repo: (*repo).to_string(),
                },
            }
        }
        ["pr", repo, "list"] if !repo.is_empty() && repo.contains('/') => {
            let state = state.unwrap_or_else(|| "open".to_string());
            if !matches!(state.as_str(), "open" | "closed" | "all") {
                return Err(format!(
                    "--state: «{state}» no es open, closed ni all (pr list)"
                ));
            }
            Command::Pr {
                repo: (*repo).to_string(),
                action: PrAction::List { state },
            }
        }
        ["pr", repo, "view", number] if !repo.is_empty() && repo.contains('/') => Command::Pr {
            repo: (*repo).to_string(),
            action: PrAction::View {
                number: parse_pr_number(number)?,
            },
        },
        ["pr", repo, "create"] if !repo.is_empty() && repo.contains('/') => Command::Pr {
            repo: (*repo).to_string(),
            action: PrAction::Create {
                title: title.ok_or("pr create requiere --title <texto>")?,
                head: head.ok_or("pr create requiere --head <rama>")?,
                base: base.ok_or("pr create requiere --base <rama>")?,
                body: body.take(),
                confirm,
            },
        },
        ["pr", repo, "update", number] if !repo.is_empty() && repo.contains('/') => {
            let number = parse_pr_number(number)?;
            if title.is_none() && body.is_none() && state.is_none() {
                return Err(
                    "pr update requiere al menos un campo: --title, --body o --state".to_string(),
                );
            }
            if let Some(value) = &state
                && !matches!(value.as_str(), "open" | "closed")
            {
                return Err(format!(
                    "--state: «{value}» no es open ni closed (pr update)"
                ));
            }
            Command::Pr {
                repo: (*repo).to_string(),
                action: PrAction::Update {
                    number,
                    title: title.take(),
                    body: body.take(),
                    state: state.take(),
                    confirm,
                },
            }
        }
        ["pr", repo, "merge", number] if !repo.is_empty() && repo.contains('/') => {
            let number = parse_pr_number(number)?;
            if let Some(value) = &method
                && !matches!(value.as_str(), "merge" | "squash" | "rebase")
            {
                return Err(format!(
                    "--method: «{value}» no es merge, squash ni rebase (pr merge)"
                ));
            }
            Command::Pr {
                repo: (*repo).to_string(),
                action: PrAction::Merge {
                    number,
                    commit_title: title.take(),
                    method: method.take(),
                    confirm,
                },
            }
        }
        ["pr", repo, "comment", number] if !repo.is_empty() && repo.contains('/') => Command::Pr {
            repo: (*repo).to_string(),
            action: PrAction::Comment {
                number: parse_pr_number(number)?,
                body: body.ok_or("pr comment requiere --body <texto>")?,
                confirm,
            },
        },
        _ => return Err(USAGE.to_string()),
    };

    // Per-command flags: a stray flag is an explicit error, not a silence.
    if scene_seen && !matches!(command, Command::Propose { .. } | Command::Review { .. }) {
        return Err("--scene solo se puede usar con las órdenes propose y review".to_string());
    }
    if instruction_seen && !matches!(command, Command::Propose { .. } | Command::Loop { .. }) {
        return Err("--instruction solo se puede usar con las órdenes propose y loop".to_string());
    }
    if focus_seen && !matches!(command, Command::Review { .. }) {
        return Err("--focus solo se puede usar con la orden review".to_string());
    }
    if allow_seen && !matches!(command, Command::Loop { .. }) {
        return Err(
            "--allow-tool/--allow-destructive solo se pueden usar con la orden loop".to_string(),
        );
    }
    if hunks_seen
        && !matches!(
            command,
            Command::Proposals {
                action: ProposalsAction::Apply(..),
                ..
            }
        )
    {
        return Err("--hunks solo se puede usar con proposals <root> apply".to_string());
    }
    if api_url_seen
        && !matches!(
            command,
            Command::Provider {
                action: ProviderAction::Login { .. }
                    | ProviderAction::Whoami
                    | ProviderAction::Connect { .. }
            } | Command::Pr { .. }
        )
    {
        return Err(
            "--api-url solo se puede usar con provider login, whoami, connect o pr".to_string(),
        );
    }
    if from_env_seen
        && !matches!(
            command,
            Command::Provider {
                action: ProviderAction::Login { .. }
            }
        )
    {
        return Err("--from-env solo se puede usar con provider login".to_string());
    }
    let pr_flags_seen =
        title_seen || head_seen || base_seen || body_seen || state_seen || method_seen;
    if pr_flags_seen && !matches!(command, Command::Pr { .. }) {
        return Err(
            "--title/--head/--base/--body/--state/--method solo se pueden usar con la orden pr"
                .to_string(),
        );
    }
    if (head_seen || base_seen)
        && !matches!(
            command,
            Command::Pr {
                action: PrAction::Create { .. },
                ..
            }
        )
    {
        return Err("--head/--base solo se pueden usar con pr <repo> create".to_string());
    }
    if method_seen
        && !matches!(
            command,
            Command::Pr {
                action: PrAction::Merge { .. },
                ..
            }
        )
    {
        return Err("--method solo se puede usar con pr <repo> merge".to_string());
    }
    if confirm_seen
        && !matches!(
            &command,
            Command::Pr { action, .. } if action.is_write()
        )
    {
        return Err(
            "--confirm solo se puede usar con las escrituras pr (create, update, merge, comment)"
                .to_string(),
        );
    }
    if model_options_seen && !command.runs_model() {
        return Err(
            "esta orden no ejecuta el modelo; quita --adapter/--latency/--base-url/--model/--stream"
                .to_string(),
        );
    }

    Ok(Parsed {
        command,
        timeout,
        latency,
        adapter,
        stream,
        provider: ProviderOptions { api_url },
    })
}

/// Parse a 0-based comma-separated hunk list, e.g. `0,2`.
fn parse_hunks(value: &str) -> Result<Vec<usize>, String> {
    if value.trim().is_empty() {
        return Err("--hunks requiere índices separados por comas, p. ej. 0,2".to_string());
    }
    value
        .split(',')
        .map(|part| {
            part.trim().parse::<usize>().map_err(|_| {
                format!("--hunks: «{part}» no es un índice válido (0-based, p. ej. 0,2)")
            })
        })
        .collect()
}

/// Parse a PR number positional (`view`/`update`/`merge`/`comment`).
fn parse_pr_number(value: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .ok()
        .filter(|number| *number > 0)
        .ok_or_else(|| format!("«{value}» no es un número de PR positivo"))
}

/// Emit one JSONL record on stdout (local review operations).
fn emit_line(value: &serde_json::Value) {
    let Ok(line) = serde_json::to_string(value) else {
        return;
    };
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

/// Run a local proposal review operation (list/show/apply/reject): JSONL on
/// stdout, human summary on stderr, exit 0/1.
fn run_proposals(root: &str, action: &ProposalsAction) -> ExitCode {
    let path = Path::new(root);
    let action_name = match action {
        ProposalsAction::List => "list",
        ProposalsAction::Show(_) => "show",
        ProposalsAction::Apply(..) => "apply",
        ProposalsAction::Reject(_) => "reject",
    };

    let outcome: Result<(serde_json::Value, String), proposal::ProposalError> = match action {
        ProposalsAction::List => proposal::list(path).map(|records| {
            for record in &records {
                emit_line(&serde_json::json!({
                    "record": "proposal",
                    "id": record.id,
                    "state": record.state,
                    "target": record.target,
                    "hunks": record.hunks.len(),
                    "task": record.origin.task,
                    "created_at": record.created_at,
                }));
            }
            (
                serde_json::json!({ "record": "proposals_list", "count": records.len() }),
                format!("{} propuestas en {root}.", records.len()),
            )
        }),
        ProposalsAction::Show(id) => proposal::load(path, id).map(|record| {
            let summary = format!(
                "Propuesta {} ({}) sobre {}: {} hunks.",
                &record.id[..8],
                record.state,
                record.target,
                record.hunks.len()
            );
            (
                serde_json::json!({ "record": "proposal_detail", "proposal": record }),
                summary,
            )
        }),
        ProposalsAction::Apply(id, hunks) => {
            proposal::apply(path, id, hunks.as_deref()).map(|outcome| {
                let total = outcome.applied_hunks.len() + outcome.skipped_hunks.len();
                let summary = if outcome.skipped_hunks.is_empty() {
                    format!(
                        "Propuesta {} aplicada: {} hunks en {}.",
                        &outcome.proposal.id[..8],
                        outcome.applied_hunks.len(),
                        outcome.proposal.target
                    )
                } else {
                    format!(
                        "Propuesta {} aplicada parcialmente: {} de {} hunks en {}.",
                        &outcome.proposal.id[..8],
                        outcome.applied_hunks.len(),
                        total,
                        outcome.proposal.target
                    )
                };
                let record = serde_json::json!({
                    "record": "proposal_apply",
                    "proposal_id": outcome.proposal.id,
                    "state": outcome.proposal.state,
                    "target": outcome.proposal.target,
                    "applied_hunks": outcome.applied_hunks,
                    "skipped_hunks": outcome.skipped_hunks,
                });
                (record, summary)
            })
        }
        ProposalsAction::Reject(id) => proposal::reject(path, id).map(|record| {
            (
                serde_json::json!({
                    "record": "proposal_reject",
                    "proposal_id": record.id,
                    "state": record.state,
                }),
                format!(
                    "Propuesta {} rechazada; {} queda intacto.",
                    &record.id[..8],
                    record.target
                ),
            )
        }),
    };

    match outcome {
        Ok((record, summary)) => {
            emit_line(&record);
            eprintln!("{summary}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            emit_line(&serde_json::json!({
                "record": "proposal_error",
                "action": action_name,
                "error": error.to_string(),
            }));
            eprintln!("orden fallida: {error}");
            ExitCode::from(1)
        }
    }
}

/// Environment variable selecting the secret store: `memory` for tests and
/// CI (nothing persisted); unset/anything else = OS keychain (D-022). Same
/// process privilege either way — it is not a privilege boundary.
const SECRET_STORE_ENV: &str = "NARRA_SECRET_STORE";

/// Remote provider commands (M5): capability matrix, token lifecycle and
/// repository connect. Tokens are resolved from the OS keychain with
/// `GITHUB_TOKEN` as the headless fallback (D-022) and are never printed.
fn run_provider(action: &ProviderAction, options: &ProviderOptions, timeout: Duration) -> ExitCode {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    // `memory` keeps automated runs deterministic (no keychain access in
    // CI or test suites); the default is the real OS keychain.
    let store: Box<dyn narra_core::provider::SecretStore> =
        match std::env::var(SECRET_STORE_ENV).as_deref() {
            Ok("memory") => Box::new(narra_core::provider::MemorySecretStore::default()),
            _ => Box::new(narra_core::provider::OsKeychain::new()),
        };

    match provider_record(action, options, store.as_ref(), timeout) {
        Ok(record) => {
            let _ = writeln!(out, "{}", serde_json::to_string(&record).expect("json"));
            ExitCode::SUCCESS
        }
        Err(message) => {
            let _ = writeln!(
                out,
                "{}",
                serde_json::json!({ "record": "provider_error", "error": message })
            );
            eprintln!("orden fallida: {message}");
            ExitCode::from(1)
        }
    }
}

/// The provider-command logic, separated from I/O so errors stay typed
/// strings and the exit-code policy lives in one place.
fn provider_record(
    action: &ProviderAction,
    options: &ProviderOptions,
    store: &dyn narra_core::provider::SecretStore,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    use narra_core::provider::{
        GITHUB_API_BASE_URL, GITHUB_CAPABILITIES, GitHubClient, GitHubConfig,
    };

    let base_url = options
        .api_url
        .clone()
        .unwrap_or_else(|| GITHUB_API_BASE_URL.to_string());
    let client_for = |token: String| {
        GitHubClient::new(
            GitHubConfig::github_com(token)
                .with_base_url(base_url.clone())
                .with_timeout(timeout),
        )
    };

    match action {
        ProviderAction::Caps => Ok(serde_json::json!({
            "record": "provider_capabilities",
            "provider": "github",
            "capabilities": GITHUB_CAPABILITIES
                .iter()
                .map(|capability| capability.label())
                .collect::<Vec<_>>(),
        })),
        ProviderAction::Login { from_env } => {
            // Token never via argv (world-readable in `ps`): stdin by
            // default, `--from-env` for automation. Verify against the API
            // *before* storing — a rejected token is never persisted.
            let token = if *from_env {
                std::env::var("GITHUB_TOKEN")
                    .ok()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| "GITHUB_TOKEN no está definida o está vacía".to_string())?
            } else {
                let mut line = String::new();
                std::io::stdin()
                    .read_line(&mut line)
                    .map_err(|error| format!("no se pudo leer el token de stdin: {error}"))?;
                let token = line.trim().to_string();
                if token.is_empty() {
                    return Err("login requiere el token por stdin, p. ej.:
  pbpaste | narra-harness provider login
o bien --from-env con la variable GITHUB_TOKEN"
                        .to_string());
                }
                token
            };
            let user = client_for(token.clone())
                .authenticated_user()
                .map_err(|error| format!("el token fue rechazado por el proveedor: {error}"))?;
            store
                .set(narra_core::provider::GITHUB_TOKEN_ACCOUNT, &token)
                .map_err(|error| format!("no se pudo guardar el token: {error}"))?;
            Ok(serde_json::json!({
                "record": "provider_login",
                "login": user.login,
                "stored": true,
            }))
        }
        ProviderAction::Logout => {
            store
                .delete(narra_core::provider::GITHUB_TOKEN_ACCOUNT)
                .map_err(|error| format!("no se pudo eliminar el token: {error}"))?;
            Ok(serde_json::json!({ "record": "provider_logout", "removed": true }))
        }
        ProviderAction::Whoami => {
            let token = resolve_token(store)?;
            let user = client_for(token)
                .authenticated_user()
                .map_err(|error| format!("GET /user falló: {error}"))?;
            Ok(serde_json::json!({ "record": "provider_whoami", "login": user.login }))
        }
        ProviderAction::Connect { repo } => {
            let (owner, name) = repo
                .split_once('/')
                .filter(|(owner, name)| !owner.is_empty() && !name.is_empty())
                .ok_or_else(|| format!("«{repo}» no es un repositorio <owner>/<repo>"))?;
            let token = resolve_token(store)?;
            let repository = client_for(token)
                .get_repository(owner, name)
                .map_err(|error| format!("GET /repos/{owner}/{name} falló: {error}"))?;
            let permissions = repository
                .permissions
                .as_ref()
                .map(|p| serde_json::json!({ "admin": p.admin, "push": p.push, "pull": p.pull }));
            Ok(serde_json::json!({
                "record": "provider_connect",
                "full_name": repository.full_name,
                "default_branch": repository.default_branch,
                "clone_url": repository.clone_url,
                "html_url": repository.html_url,
                "private": repository.private,
                "permissions": permissions,
            }))
        }
    }
}

/// Pull-request commands (M5 slice 3): JSONL record on stdout, human
/// summary on stderr, exit 0/1. Writes without `--confirm` print a
/// dry-run record of the exact payload and exit 1 — no network, no token
/// lookup, nothing persisted.
fn run_pr(repo: &str, action: &PrAction, options: &ProviderOptions, timeout: Duration) -> ExitCode {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    // Dry-run before token resolution: confirming what would be sent must
    // work even without credentials, and must never touch the network.
    if action.needs_confirmation() {
        let record = serde_json::json!({
            "record": "pr_dry_run",
            "repository": repo,
            "action": action.name(),
            "payload": action.dry_run_payload(),
            "confirm_required": true,
        });
        let _ = writeln!(out, "{}", serde_json::to_string(&record).expect("json"));
        eprintln!(
            "orden «{}» NO ejecutada: una acción remota requiere confirmación explícita. \
             Repite la orden añadiendo --confirm para enviar exactamente este payload.",
            action.name()
        );
        return ExitCode::from(1);
    }

    let store: Box<dyn narra_core::provider::SecretStore> =
        match std::env::var(SECRET_STORE_ENV).as_deref() {
            Ok("memory") => Box::new(narra_core::provider::MemorySecretStore::default()),
            _ => Box::new(narra_core::provider::OsKeychain::new()),
        };

    match pr_record(repo, action, options, store.as_ref(), timeout) {
        Ok(record) => {
            let _ = writeln!(out, "{}", serde_json::to_string(&record).expect("json"));
            eprintln!("{}", pr_summary(&record));
            ExitCode::SUCCESS
        }
        Err(message) => {
            let _ = writeln!(
                out,
                "{}",
                serde_json::json!({
                    "record": "pr_error",
                    "repository": repo,
                    "action": action.name(),
                    "error": message,
                })
            );
            eprintln!("orden fallida: {message}");
            ExitCode::from(1)
        }
    }
}

/// One-line human summary for a successful PR record (stderr).
fn pr_summary(record: &serde_json::Value) -> String {
    // `as_str` keeps quotes out of the human summary (stderr shows values,
    // not their JSON encoding).
    let text = |key: &str| record[key].as_str().unwrap_or_default().to_string();
    match record["record"].as_str().unwrap_or_default() {
        "pr_list" => format!(
            "{} pull requests ({}).",
            record["count"],
            text("repository")
        ),
        "pr_view" | "pr_updated" => format!(
            "PR #{} «{}» ({}) en {}: {}.",
            record["number"],
            text("title"),
            text("state"),
            text("repository"),
            text("html_url")
        ),
        "pr_created" => format!(
            "PR #{} creado en {}: {}.",
            record["number"],
            text("repository"),
            text("html_url")
        ),
        "pr_merge" => format!(
            "Merge de PR #{} en {}: {}.",
            record["number"],
            text("repository"),
            text("message")
        ),
        "pr_comment" => format!(
            "Comentario {} publicado en PR #{} ({}).",
            record["id"],
            record["number"],
            text("repository")
        ),
        other => format!("Operación «{other}» completada."),
    }
}

/// The provider-command logic for PRs, separated from I/O. Every result
/// reflects the provider's own answer: a PR record only exists if the API
/// returned 2xx with a parseable body, and a `merged: false` merge verdict
/// is an error, not a success (M5 acceptance: no false successes).
fn pr_record(
    repo: &str,
    action: &PrAction,
    options: &ProviderOptions,
    store: &dyn narra_core::provider::SecretStore,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    use narra_core::provider::{
        GITHUB_API_BASE_URL, GitHubClient, GitHubConfig, MergeMethod, MergeOptions, NewPullRequest,
        PullRequestState, UpdatePullRequest,
    };

    let (owner, name) = repo
        .split_once('/')
        .filter(|(owner, name)| !owner.is_empty() && !name.is_empty())
        .ok_or_else(|| format!("«{repo}» no es un repositorio <owner>/<repo>"))?;

    let base_url = options
        .api_url
        .clone()
        .unwrap_or_else(|| GITHUB_API_BASE_URL.to_string());
    let client_for = |token: String| {
        GitHubClient::new(
            GitHubConfig::github_com(token)
                .with_base_url(base_url.clone())
                .with_timeout(timeout),
        )
    };

    let pr_json = |pr: &narra_core::provider::PullRequest| {
        serde_json::json!({
            "record": match action {
                PrAction::Create { .. } => "pr_created",
                PrAction::Update { .. } => "pr_updated",
                _ => "pr_view",
            },
            "repository": repo,
            "number": pr.number,
            "state": pr.state.as_str(),
            "merged": pr.merged,
            "title": pr.title,
            "html_url": pr.html_url,
            "head": pr.head_ref,
            "base": pr.base_ref,
        })
    };

    // Reads first: no token dance needed to reach here without --confirm.
    match action {
        PrAction::List { state } => {
            let token = resolve_token(store)?;
            let pull_requests = client_for(token)
                .list_pull_requests(owner, name, state, 10)
                .map_err(|error| format!("GET /repos/{owner}/{name}/pulls falló: {error}"))?;
            Ok(serde_json::json!({
                "record": "pr_list",
                "repository": repo,
                "count": pull_requests.len(),
                "pull_requests": pull_requests
                    .iter()
                    .map(|pr| serde_json::json!({
                        "number": pr.number,
                        "state": pr.state.as_str(),
                        "merged": pr.merged,
                        "title": pr.title,
                        "html_url": pr.html_url,
                        "head": pr.head_ref,
                        "base": pr.base_ref,
                    }))
                    .collect::<Vec<_>>(),
            }))
        }
        PrAction::View { number } => {
            let token = resolve_token(store)?;
            let pr = client_for(token)
                .get_pull_request(owner, name, *number)
                .map_err(|error| {
                    format!("GET /repos/{owner}/{name}/pulls/{number} falló: {error}")
                })?;
            Ok(pr_json(&pr))
        }
        PrAction::Create {
            title,
            head,
            base,
            body,
            ..
        } => {
            let token = resolve_token(store)?;
            let pr = client_for(token)
                .create_pull_request(
                    owner,
                    name,
                    &NewPullRequest {
                        title: title.clone(),
                        head: head.clone(),
                        base: base.clone(),
                        body: body.clone(),
                    },
                )
                .map_err(|error| format!("POST /repos/{owner}/{name}/pulls falló: {error}"))?;
            Ok(pr_json(&pr))
        }
        PrAction::Update {
            number,
            title,
            body,
            state,
            ..
        } => {
            let state = state
                .as_deref()
                .map(PullRequestState::parse)
                .transpose()
                .map_err(|error| format!("--state inválido: {error}"))?;
            let token = resolve_token(store)?;
            let pr = client_for(token)
                .update_pull_request(
                    owner,
                    name,
                    *number,
                    &UpdatePullRequest {
                        title: title.clone(),
                        body: body.clone(),
                        state,
                    },
                )
                .map_err(|error| {
                    format!("PATCH /repos/{owner}/{name}/pulls/{number} falló: {error}")
                })?;
            Ok(pr_json(&pr))
        }
        PrAction::Merge {
            number,
            commit_title,
            method,
            ..
        } => {
            // `--method` was validated at parse time (merge|squash|rebase).
            let method = match method.as_deref() {
                Some("squash") => Some(MergeMethod::Squash),
                Some("rebase") => Some(MergeMethod::Rebase),
                Some(_) => Some(MergeMethod::Merge),
                None => None,
            };
            let token = resolve_token(store)?;
            let result = client_for(token)
                .merge_pull_request(
                    owner,
                    name,
                    *number,
                    &MergeOptions {
                        commit_title: commit_title.clone(),
                        commit_message: None,
                        merge_method: method,
                    },
                )
                .map_err(|error| {
                    format!("PUT /repos/{owner}/{name}/pulls/{number}/merge falló: {error}")
                })?;
            // The provider's own verdict: `merged: false` on a 200 means
            // the merge did NOT happen (e.g. head moved). Surface it as a
            // failure — never reinterpret it as success.
            if !result.merged {
                return Err(format!(
                    "el proveedor respondió merged=false para PR #{number}: {}",
                    result.message
                ));
            }
            Ok(serde_json::json!({
                "record": "pr_merge",
                "repository": repo,
                "number": number,
                "merged": result.merged,
                "sha": result.sha,
                "message": result.message,
            }))
        }
        PrAction::Comment { number, body, .. } => {
            let token = resolve_token(store)?;
            let comment = client_for(token)
                .create_issue_comment(owner, name, *number, body)
                .map_err(|error| {
                    format!("POST /repos/{owner}/{name}/issues/{number}/comments falló: {error}")
                })?;
            Ok(serde_json::json!({
                "record": "pr_comment",
                "repository": repo,
                "number": number,
                "id": comment.id,
                "html_url": comment.html_url,
            }))
        }
    }
}

/// Token resolution shared by whoami/connect: keychain first, then
/// `GITHUB_TOKEN` (D-022). The error tells the user exactly what to do.
fn resolve_token(store: &dyn narra_core::provider::SecretStore) -> Result<String, String> {
    narra_core::provider::secrets::github_token(
        store,
        std::env::var("GITHUB_TOKEN").ok().as_deref(),
    )
    .map_err(|error| format!("no se pudo leer el token guardado: {error}"))?
    .ok_or_else(|| {
        "no hay token de GitHub: ejecuta `narra-harness provider login` o define GITHUB_TOKEN"
            .to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Parsed, String> {
        let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        parse_args(&owned)
    }

    #[test]
    fn parses_brief_task_with_options() {
        let parsed =
            parse(&["brief", "/tmp/p", "--timeout", "5", "--latency", "10"]).expect("parse");
        assert!(matches!(parsed.command, Command::Brief { ref root } if root == "/tmp/p"));
        assert_eq!(parsed.timeout, Duration::from_secs(5));
        assert_eq!(parsed.latency, Duration::from_millis(10));
    }

    #[test]
    fn rejects_unknown_and_incomplete_input() {
        assert!(parse(&[]).is_err());
        assert!(parse(&["brief"]).is_err());
        assert!(parse(&["brief", "/p", "--nope"]).is_err());
        assert!(parse(&["brief", "/p", "--timeout"]).is_err());
    }

    #[test]
    fn defaults_to_the_mock_adapter() {
        let parsed = parse(&["brief", "/tmp/p"]).expect("parse");
        assert!(matches!(parsed.adapter, AdapterChoice::Mock));
    }

    #[test]
    fn openai_compat_requires_model() {
        let error =
            parse(&["brief", "/tmp/p", "--adapter", "openai-compat"]).expect_err("model required");
        assert!(error.contains("--model"), "{error}");
    }

    #[test]
    fn base_url_and_model_need_their_adapter() {
        let error = parse(&["brief", "/tmp/p", "--model", "qwen"]).expect_err("adapter required");
        assert!(error.contains("openai-compat"), "{error}");
    }

    #[test]
    fn unknown_adapter_is_rejected() {
        assert!(parse(&["brief", "/tmp/p", "--adapter", "gemini"]).is_err());
    }

    #[test]
    fn api_key_never_appears_in_debug_output() {
        let choice = OpenAiCompatChoice {
            base_url: "http://localhost:11434/v1".to_string(),
            model: "qwen".to_string(),
            api_key: Some("sk-super-secret-value".to_string()),
        };
        let rendered = format!("{choice:?}");
        assert!(
            !rendered.contains("sk-super-secret-value"),
            "key leaked in Debug: {rendered}"
        );
        assert!(rendered.contains("<redacted>"), "{rendered}");
    }

    #[test]
    fn openai_compat_with_model_parses_with_local_default_url() {
        let parsed = parse(&[
            "brief",
            "/tmp/p",
            "--adapter",
            "openai-compat",
            "--model",
            "qwen2.5vl:7b",
        ])
        .expect("parse");
        match parsed.adapter {
            AdapterChoice::OpenAiCompat(OpenAiCompatChoice {
                ref base_url,
                ref model,
                ..
            }) => {
                assert_eq!(base_url, narra_core::agent::LOCAL_DEFAULT_BASE_URL);
                assert_eq!(model, "qwen2.5vl:7b");
            }
            AdapterChoice::Mock => panic!("expected openai-compat"),
        }
    }

    #[test]
    fn parses_propose_with_scene_and_instruction() {
        let parsed = parse(&[
            "propose",
            "/tmp/p",
            "--scene",
            "3",
            "--instruction",
            "haz el diálogo más tenso",
        ])
        .expect("parse");
        match parsed.command {
            Command::Propose {
                ref root,
                scene,
                ref instruction,
            } => {
                assert_eq!(root, "/tmp/p");
                assert_eq!(scene, 3);
                assert_eq!(instruction, "haz el diálogo más tenso");
            }
            other => panic!("expected propose, got {other:?}"),
        }
    }

    #[test]
    fn propose_requires_scene_and_instruction() {
        let error = parse(&["propose", "/tmp/p"]).expect_err("both required");
        assert!(error.contains("--scene"), "{error}");
        let error = parse(&["propose", "/tmp/p", "--scene", "1"]).expect_err("instruction");
        assert!(error.contains("--instruction"), "{error}");
        // Scene 0 parses (a number) and is rejected by the task itself with
        // a clear message; a non-number is a parse error up front.
        let error = parse(&["propose", "/tmp/p", "--scene", "una", "--instruction", "x"])
            .expect_err("not a number");
        assert!(error.contains("--scene"), "{error}");
    }

    #[test]
    fn parses_review_with_scene_and_optional_focus() {
        let parsed = parse(&[
            "review",
            "/tmp/p",
            "--scene",
            "2",
            "--focus",
            "¿respeta la continuidad?",
        ])
        .expect("parse");
        match parsed.command {
            Command::Review {
                ref root,
                scene,
                ref focus,
            } => {
                assert_eq!(root, "/tmp/p");
                assert_eq!(scene, 2);
                assert_eq!(focus, "¿respeta la continuidad?");
            }
            other => panic!("expected review, got {other:?}"),
        }
        // Focus is optional (general review); scene is required like propose.
        let parsed = parse(&["review", "/tmp/p", "--scene", "1"]).expect("parse without focus");
        assert!(
            matches!(parsed.command, Command::Review { ref focus, .. } if focus.is_empty()),
            "empty focus by default"
        );
        let error = parse(&["review", "/tmp/p"]).expect_err("scene required");
        assert!(error.contains("--scene"), "{error}");
    }

    #[test]
    fn focus_flag_is_review_only() {
        let error = parse(&["brief", "/tmp/p", "--focus", "x"]).expect_err("review only");
        assert!(error.contains("review"), "{error}");
        let error = parse(&["loop", "/tmp/p", "--instruction", "x", "--focus", "y"])
            .expect_err("review only");
        assert!(error.contains("review"), "{error}");
    }

    #[test]
    fn parses_proposal_review_actions() {
        let parsed = parse(&["proposals", "/tmp/p"]).expect("list");
        assert!(matches!(
            parsed.command,
            Command::Proposals {
                action: ProposalsAction::List,
                ..
            }
        ));
        let parsed = parse(&["proposals", "/tmp/p", "show", "abc"]).expect("show");
        assert!(matches!(
            parsed.command,
            Command::Proposals {
                action: ProposalsAction::Show(ref id),
                ..
            } if id == "abc"
        ));
        let parsed =
            parse(&["proposals", "/tmp/p", "apply", "abc", "--hunks", "0,2"]).expect("apply");
        assert!(matches!(
            parsed.command,
            Command::Proposals {
                action: ProposalsAction::Apply(ref id, Some(ref hunks)),
                ..
            } if id == "abc" && hunks == &vec![0, 2]
        ));
        let parsed = parse(&["proposals", "/tmp/p", "reject", "abc"]).expect("reject");
        assert!(matches!(
            parsed.command,
            Command::Proposals {
                action: ProposalsAction::Reject(_),
                ..
            }
        ));
        let parsed = parse(&["proposals", "/tmp/p", "renew", "abc"]).expect("renew");
        assert!(matches!(parsed.command, Command::Renew { .. }));
    }

    #[test]
    fn review_flags_reject_wrong_commands() {
        let error =
            parse(&["brief", "/tmp/p", "--scene", "1"]).expect_err("--scene only with propose");
        assert!(error.contains("propose"), "{error}");
        let error =
            parse(&["brief", "/tmp/p", "--hunks", "0"]).expect_err("--hunks only with apply");
        assert!(error.contains("apply"), "{error}");
        let error = parse(&["proposals", "/tmp/p", "list", "--adapter", "mock"])
            .expect_err("local op without a model");
        assert!(error.contains("modelo"), "{error}");
        assert!(parse(&["proposals", "/tmp/p", "renew", "abc", "--latency", "5"]).is_ok());
    }

    #[test]
    fn hunks_list_is_strictly_parsed() {
        assert_eq!(parse_hunks("0,2").expect("ok"), vec![0, 2]);
        assert!(parse_hunks("").is_err());
        assert!(parse_hunks("a,b").is_err());
        assert!(parse_hunks("0,,1").is_err());
    }

    #[test]
    fn provider_subcommands_parse() {
        let parsed = parse(&["provider", "caps"]).expect("caps");
        assert!(matches!(
            parsed.command,
            Command::Provider {
                action: ProviderAction::Caps
            }
        ));
        let parsed = parse(&["provider", "login", "--from-env"]).expect("login");
        assert!(matches!(
            parsed.command,
            Command::Provider {
                action: ProviderAction::Login { from_env: true }
            }
        ));
        let parsed = parse(&["provider", "logout"]).expect("logout");
        assert!(matches!(
            parsed.command,
            Command::Provider {
                action: ProviderAction::Logout
            }
        ));
        let parsed =
            parse(&["provider", "whoami", "--api-url", "http://127.0.0.1:9"]).expect("whoami");
        assert!(matches!(
            parsed.command,
            Command::Provider {
                action: ProviderAction::Whoami
            }
        ));
        assert_eq!(
            parsed.provider.api_url.as_deref(),
            Some("http://127.0.0.1:9")
        );
        let parsed =
            parse(&["provider", "connect", "olivaresrafael/narra-desktop"]).expect("connect");
        assert!(matches!(
            parsed.command,
            Command::Provider {
                action: ProviderAction::Connect { .. }
            }
        ));
    }

    #[test]
    fn provider_flags_reject_wrong_commands() {
        let error = parse(&["provider", "connect", "owner"]).expect_err("owner/repo required");
        assert!(error.contains("provider"), "{error}");
        let error = match parse(&["brief", "/tmp/p", "--api-url", "http://x"]) {
            Err(error) => error,
            Ok(_) => panic!("--api-url solo con provider"),
        };
        assert!(error.contains("provider"), "{error}");
        let error =
            parse(&["provider", "whoami", "--from-env"]).expect_err("--from-env only with login");
        assert!(error.contains("login"), "{error}");
        // The token is never an argument.
        assert!(parse(&["provider", "login", "ghp_token"]).is_err());
    }

    #[test]
    fn pr_reads_parse_without_confirmation() {
        let parsed = parse(&["pr", "owner/repo", "list"]).expect("list");
        assert!(matches!(
            parsed.command,
            Command::Pr {
                ref action,
                ..
            } if matches!(action, PrAction::List { state } if state == "open")
        ));
        let parsed = parse(&["pr", "owner/repo", "list", "--state", "all"]).expect("list all");
        assert!(matches!(
            parsed.command,
            Command::Pr {
                action: PrAction::List { ref state },
                ..
            } if state == "all"
        ));
        let parsed = parse(&["pr", "owner/repo", "view", "42"]).expect("view");
        assert!(matches!(
            parsed.command,
            Command::Pr {
                action: PrAction::View { number: 42 },
                ..
            }
        ));
    }

    #[test]
    fn pr_writes_parse_with_and_without_confirm() {
        // With --confirm: a real write.
        let parsed = parse(&[
            "pr",
            "owner/repo",
            "create",
            "--title",
            "Revisión M5",
            "--head",
            "feature/x",
            "--base",
            "main",
            "--body",
            "hola",
            "--confirm",
        ])
        .expect("create");
        assert!(matches!(
            parsed.command,
            Command::Pr {
                action: PrAction::Create { confirm: true, .. },
                ..
            }
        ));
        // Without --confirm it still parses, but needs_confirmation() drives
        // the dry-run path (no network, exit 1).
        let parsed = parse(&[
            "pr",
            "owner/repo",
            "merge",
            "7",
            "--method",
            "squash",
            "--title",
            "Merge M5",
        ])
        .expect("merge dry");
        assert!(matches!(
            parsed.command,
            Command::Pr {
                action: PrAction::Merge {
                    number: 7,
                    confirm: false,
                    ..
                },
                ..
            }
        ));
        let parsed = parse(&[
            "pr",
            "owner/repo",
            "update",
            "3",
            "--state",
            "closed",
            "--confirm",
        ])
        .expect("update");
        assert!(matches!(
            parsed.command,
            Command::Pr {
                action: PrAction::Update {
                    number: 3,
                    confirm: true,
                    ..
                },
                ..
            }
        ));
        let parsed = parse(&[
            "pr",
            "owner/repo",
            "comment",
            "9",
            "--body",
            "listo",
            "--confirm",
        ])
        .expect("comment");
        assert!(matches!(
            parsed.command,
            Command::Pr {
                action: PrAction::Comment {
                    number: 9,
                    confirm: true,
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn pr_write_required_fields_are_enforced() {
        let error = parse(&["pr", "owner/repo", "create", "--confirm"]).expect_err("title");
        assert!(error.contains("--title"), "{error}");
        let error =
            parse(&["pr", "owner/repo", "create", "--title", "t", "--confirm"]).expect_err("head");
        assert!(error.contains("--head"), "{error}");
        let error = parse(&["pr", "owner/repo", "update", "3", "--confirm"]).expect_err("a field");
        assert!(error.contains("--title"), "{error}");
        let error = parse(&["pr", "owner/repo", "comment", "3", "--confirm"]).expect_err("body");
        assert!(error.contains("--body"), "{error}");
    }

    #[test]
    fn pr_rejects_bad_values_and_stray_flags() {
        let error = parse(&["pr", "owner/repo", "view", "abc"]).expect_err("number");
        assert!(error.contains("número"), "{error}");
        let error = parse(&["pr", "owner/repo", "view", "0"]).expect_err("positive");
        assert!(error.contains("número"), "{error}");
        let error = parse(&["pr", "owner/repo", "list", "--state", "merged"]).expect_err("state");
        assert!(error.contains("--state"), "{error}");
        let error = parse(&["pr", "owner/repo", "merge", "1", "--method", "fast-forward"])
            .expect_err("method");
        assert!(error.contains("--method"), "{error}");
        // Malformed repository.
        let error = parse(&["pr", "sin-owner", "list"]).expect_err("owner/repo");
        assert!(error.contains("uso:"), "{error}");
        // Stray flags on the wrong command.
        let error =
            parse(&["pr", "owner/repo", "list", "--head", "rama"]).expect_err("head on list");
        assert!(error.contains("--head"), "{error}");
        let error =
            parse(&["pr", "owner/repo", "view", "1", "--method", "merge"]).expect_err("method");
        assert!(error.contains("--method"), "{error}");
        // --confirm is for writes only.
        let error =
            parse(&["pr", "owner/repo", "view", "1", "--confirm"]).expect_err("confirm on read");
        assert!(error.contains("--confirm"), "{error}");
        let error = parse(&["brief", "/tmp/p", "--confirm"]).expect_err("confirm on brief");
        assert!(error.contains("--confirm"), "{error}");
        let error = parse(&["brief", "/tmp/p", "--title", "t"]).expect_err("title on brief");
        assert!(error.contains("pr"), "{error}");
    }
}
