//! Headless agent harness (Milestone 4, Spike F).
//!
//! Runs an agent task with **no desktop window**:
//!
//! - task input is explicit and serializable (CLI arguments; a JSON input
//!   variant lands with the first real provider adapter);
//! - progress events are UI-neutral, emitted as JSONL on stdout;
//! - Ctrl-C cancels cooperatively;
//! - output is both human-readable (stderr summary) and machine-readable
//!   (final `task_output` JSONL record on stdout);
//! - exit status is defined: 0 success, 1 failure, 2 cancelled.
//!
//! Usage:
//!   narra-harness brief <project-root> [options]
//!
//! Options:
//!   --timeout <secs>   task timeout (default 120)
//!   --latency <ms>     mock adapter simulated latency (cancellation demos)
//!   --adapter <name>   mock (default) | openai-compat
//!   --base-url <url>   openai-compat endpoint (default http://localhost:11434/v1)
//!   --model <name>     model name for openai-compat (required with it)
//!
//! The provider API key is read **only** from the `OPENAI_API_KEY`
//! environment variable (empty/unset = no key, right for local servers).
//! It is never accepted as a command-line argument — argv is world-readable
//! via `ps`.

use std::io::Write;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use narra_core::agent::{
    AgentError, MockAdapter, ModelAdapter, OpenAiCompatAdapter, OpenAiCompatConfig,
    ProjectBriefTask, TaskEvent, TaskOptions,
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

    let result = match parsed.task {
        CliTask::Brief(root) => {
            ProjectBriefTask::new(root).run(adapter.as_ref(), &options, &mut emit)
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
       [--adapter mock|openai-compat] [--base-url <url>] [--model <nombre>]";

#[derive(Debug)]
struct Parsed {
    task: CliTask,
    timeout: Duration,
    latency: Duration,
    adapter: AdapterChoice,
}

#[derive(Debug)]
enum CliTask {
    Brief(String),
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
            }
            "--adapter" => {
                let value = iter.next().ok_or("--adapter requiere un valor")?;
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
            }
            "--model" => {
                let value = iter.next().ok_or("--model requiere un valor")?;
                model = Some((*value).clone());
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

    match positional.as_slice() {
        ["brief", root] if !root.is_empty() => Ok(Parsed {
            task: CliTask::Brief((*root).to_string()),
            timeout,
            latency,
            adapter,
        }),
        _ => Err(USAGE.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_brief_task_with_options() {
        let args: Vec<String> = ["brief", "/tmp/p", "--timeout", "5", "--latency", "10"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let parsed = parse_args(&args).expect("parse");
        assert!(matches!(parsed.task, CliTask::Brief(ref root) if root == "/tmp/p"));
        assert_eq!(parsed.timeout, Duration::from_secs(5));
        assert_eq!(parsed.latency, Duration::from_millis(10));
    }

    #[test]
    fn rejects_unknown_and_incomplete_input() {
        assert!(parse_args(&[]).is_err());
        assert!(parse_args(&["brief".to_string()]).is_err());
        assert!(parse_args(&["brief".to_string(), "/p".into(), "--nope".into()]).is_err());
        assert!(parse_args(&["brief".into(), "/p".into(), "--timeout".into()]).is_err());
    }

    #[test]
    fn defaults_to_the_mock_adapter() {
        let args: Vec<String> = ["brief", "/tmp/p"].iter().map(|s| s.to_string()).collect();
        let parsed = parse_args(&args).expect("parse");
        assert!(matches!(parsed.adapter, AdapterChoice::Mock));
    }

    #[test]
    fn openai_compat_requires_model() {
        let args: Vec<String> = ["brief", "/tmp/p", "--adapter", "openai-compat"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let error = parse_args(&args).expect_err("model required");
        assert!(error.contains("--model"), "{error}");
    }

    #[test]
    fn base_url_and_model_need_their_adapter() {
        let args: Vec<String> = ["brief", "/tmp/p", "--model", "qwen"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let error = parse_args(&args).expect_err("adapter required");
        assert!(error.contains("openai-compat"), "{error}");
    }

    #[test]
    fn unknown_adapter_is_rejected() {
        let args: Vec<String> = ["brief", "/tmp/p", "--adapter", "gemini"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(parse_args(&args).is_err());
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
        let args: Vec<String> = [
            "brief",
            "/tmp/p",
            "--adapter",
            "openai-compat",
            "--model",
            "qwen2.5vl:7b",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let parsed = parse_args(&args).expect("parse");
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
}
