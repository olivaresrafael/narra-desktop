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
//!   narra-harness brief <project-root> [--timeout <secs>] [--latency <ms>]
//!
//! `--latency` sets the mock adapter's simulated latency, so cancellation
//! can be exercised deterministically without a network.

use std::io::Write;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use narra_core::agent::{MockAdapter, ProjectBriefTask, TaskEvent, TaskOptions};
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
    let adapter = MockAdapter::with_latency(parsed.latency);

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
        CliTask::Brief(root) => ProjectBriefTask::new(root).run(&adapter, &options, &mut emit),
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
                narra_core::agent::AgentError::Cancelled => TaskEvent::Cancelled,
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

const USAGE: &str = "uso: narra-harness brief <project-root> [--timeout <seg>] [--latency <ms>]";

struct Parsed {
    task: CliTask,
    timeout: Duration,
    latency: Duration,
}

enum CliTask {
    Brief(String),
}

fn parse_args(args: &[String]) -> Result<Parsed, String> {
    let mut timeout = Duration::from_secs(120);
    let mut latency = Duration::ZERO;
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
            other if other.starts_with('-') => {
                return Err(format!("opción desconocida: {other}"));
            }
            other => positional.push(other),
        }
    }

    match positional.as_slice() {
        ["brief", root] if !root.is_empty() => Ok(Parsed {
            task: CliTask::Brief((*root).to_string()),
            timeout,
            latency,
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
}
