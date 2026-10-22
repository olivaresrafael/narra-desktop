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
//!   narra-harness proposals <project-root> [list]
//!   narra-harness proposals <project-root> show <proposal-id>
//!   narra-harness proposals <project-root> apply <proposal-id> [--hunks 0,2]
//!   narra-harness proposals <project-root> reject <proposal-id>
//!   narra-harness proposals <project-root> renew <proposal-id> [options]
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
//!   --scene <n>        1-based scene number (propose only)
//!   --instruction <t>  writer's instruction (propose only)
//!   --hunks <a,b>      0-based hunk indices to accept (apply only; default all)
//!
//! `apply` refuses a stale proposal (the file changed after it was created)
//! and exits 1; `renew` re-runs the originating task against the current
//! content to produce a fresh proposal for renewed review.
//!
//! The provider API key is read **only** from the `OPENAI_API_KEY`
//! environment variable (empty/unset = no key, right for local servers).
//! It is never accepted as a command-line argument — argv is world-readable
//! via `ps`.

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use narra_core::agent::proposal;
use narra_core::agent::{
    AgentError, GrantPolicy, MockAdapter, ModelAdapter, OpenAiCompatAdapter, OpenAiCompatConfig,
    ProjectBriefTask, SceneEditTask, TaskEvent, TaskOptions, ToolLoopTask,
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
        Command::Proposals { .. } => unreachable!("proposals run locally"),
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
     narra-harness proposals <project-root> [list]
     narra-harness proposals <project-root> show <id>
     narra-harness proposals <project-root> apply <id> [--hunks 0,2]
     narra-harness proposals <project-root> reject <id>
     narra-harness proposals <project-root> renew <id> [--adapter ...] [--model ...]
     narra-harness loop <project-root> --instruction <texto> [--allow-tool <nombre>]...
         [--allow-destructive <nombre>]... [mismas opciones de modelo]";

#[derive(Debug)]
struct Parsed {
    command: Command,
    timeout: Duration,
    latency: Duration,
    adapter: AdapterChoice,
    stream: bool,
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
    let mut hunks: Option<Vec<usize>> = None;
    let mut model_options_seen = false;
    let mut stream = false;
    let mut allow_tool: Vec<String> = Vec::new();
    let mut allow_destructive: Vec<String> = Vec::new();
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
            "--hunks" => {
                let value = iter.next().ok_or("--hunks requiere un valor")?;
                hunks = Some(parse_hunks(value)?);
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
    let hunks_seen = hunks.is_some();
    let allow_seen = !allow_tool.is_empty() || !allow_destructive.is_empty();

    let command = match positional.as_slice() {
        ["brief", root] if !root.is_empty() => Command::Brief {
            root: (*root).to_string(),
        },
        ["propose", root] if !root.is_empty() => Command::Propose {
            root: (*root).to_string(),
            scene: scene.ok_or("--propose requiere --scene <número>")?,
            instruction: instruction.ok_or("--propose requiere --instruction <texto>")?,
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
        _ => return Err(USAGE.to_string()),
    };

    // Per-command flags: a stray flag is an explicit error, not a silence.
    if scene_seen && !matches!(command, Command::Propose { .. }) {
        return Err("--scene solo se puede usar con la orden propose".to_string());
    }
    if instruction_seen && !matches!(command, Command::Propose { .. } | Command::Loop { .. }) {
        return Err("--instruction solo se puede usar con las órdenes propose y loop".to_string());
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
}
