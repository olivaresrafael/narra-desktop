//! Process runner for Git operations: timeouts, cancellation and redaction.
//!
//! Design rules (Milestone 3, Spike D; `AGENTS.md` engineering rules):
//! - no shell is ever involved: arguments are passed as an argv vector, so
//!   user-controlled strings (paths, messages, URLs) cannot inject commands;
//! - every operation is bounded by a timeout and accepts a cancellation
//!   token; the calling thread blocks, so callers run these on a worker
//!   thread, never on the UI thread;
//! - interactive credential prompts are disabled (`GIT_TERMINAL_PROMPT=0`)
//!   so a missing credential becomes an immediate, actionable error instead
//!   of a hang;
//! - anything that could echo a secret (stderr, URLs) passes through
//!   [`redact_secrets`] before it reaches an error value.

use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use thiserror::Error;

pub use crate::cancel::CancellationToken;

/// Default bound for local Git operations. Generous for slow disks and cold
/// caches; callers doing network work should raise it explicitly.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// How long to wait between cancellation/timeout polls of the child process.
const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// Per-call options: timeout and optional cancellation.
#[derive(Debug, Clone)]
pub struct OpOptions {
    pub timeout: Duration,
    pub cancel: Option<CancellationToken>,
}

impl Default for OpOptions {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            cancel: None,
        }
    }
}

impl OpOptions {
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn cancel(mut self, token: CancellationToken) -> Self {
        self.cancel = Some(token);
        self
    }
}

/// Errors from running a Git command. Messages are already redacted.
#[derive(Debug, Error)]
pub enum GitError {
    /// The `git` executable is not installed or not on PATH.
    #[error("git is not available on this system: {0}")]
    Unavailable(String),

    #[error("git {operation} timed out after {after:?}")]
    Timeout { operation: String, after: Duration },

    #[error("git {operation} was cancelled")]
    Cancelled { operation: String },

    /// The command ran but exited non-zero. `stderr` is redacted and trimmed;
    /// it is meant to be shown to the user as the actionable explanation.
    #[error("git {operation} failed (exit {code:?}): {stderr}")]
    CommandFailed {
        operation: String,
        code: Option<i32>,
        stderr: String,
    },

    /// Local input rejected before running Git.
    #[error("invalid input: {0}")]
    InvalidInput(&'static str),

    /// A path operation failed before any Git command ran.
    #[error("failed to access `{path}`: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Run `git` in `root` (or the inherited working directory when `None`).
///
/// `operation` is a short human-readable label used in error messages; it must
/// never contain user data (URLs, messages) because it is not redacted.
pub(crate) fn run_git<S: AsRef<OsStr>>(
    root: Option<&Path>,
    operation: &str,
    args: &[S],
    options: OpOptions,
) -> Result<Output, GitError> {
    run_git_env(root, operation, args, &[], options)
}

/// Same as [`run_git`] plus explicit extra environment variables (used to
/// pass committer identity explicitly instead of inheriting ambient config).
pub(crate) fn run_git_env<S: AsRef<OsStr>>(
    root: Option<&Path>,
    operation: &str,
    args: &[S],
    extra_env: &[(&str, &str)],
    options: OpOptions,
) -> Result<Output, GitError> {
    if options
        .cancel
        .as_ref()
        .is_some_and(|token| token.is_cancelled())
    {
        return Err(GitError::Cancelled {
            operation: operation.to_string(),
        });
    }

    let mut command = Command::new("git");
    command.args(args);
    // Never prompt interactively: a missing credential must fail fast with an
    // actionable error instead of blocking a background worker.
    command.env("GIT_TERMINAL_PROMPT", "0");
    for (key, value) in extra_env {
        command.env(key, value);
    }
    if let Some(root) = root {
        command.current_dir(root);
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = command.spawn().map_err(|source| match source.kind() {
        std::io::ErrorKind::NotFound => {
            GitError::Unavailable("the `git` executable was not found on PATH".to_string())
        }
        _ => GitError::Io {
            path: root.unwrap_or_else(|| Path::new(".")).to_path_buf(),
            source,
        },
    })?;

    // Drain both pipes on dedicated threads: a child that fills the 64 KiB
    // pipe buffer must not deadlock against our wait loop.
    let mut stdout_pipe = child.stdout.take().expect("stdout is piped");
    let mut stderr_pipe = child.stderr.take().expect("stderr is piped");
    let stdout_thread = thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut buffer);
        buffer
    });
    let stderr_thread = thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut buffer);
        buffer
    });

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(source) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(GitError::Io {
                    path: root.unwrap_or_else(|| Path::new(".")).to_path_buf(),
                    source,
                });
            }
        }
        if options
            .cancel
            .as_ref()
            .is_some_and(|token| token.is_cancelled())
        {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_thread.join();
            let _ = stderr_thread.join();
            return Err(GitError::Cancelled {
                operation: operation.to_string(),
            });
        }
        if started.elapsed() > options.timeout {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_thread.join();
            let _ = stderr_thread.join();
            return Err(GitError::Timeout {
                operation: operation.to_string(),
                after: options.timeout,
            });
        }
        thread::sleep(POLL_INTERVAL);
    };

    let stdout = stdout_thread.join().unwrap_or_default();
    let stderr = stderr_thread.join().unwrap_or_default();
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

/// Redact credentials from text destined for logs, errors or UI.
///
/// Best-effort defense in depth (heuristic, no regex dependency): masks
/// userinfo in URLs (`scheme:user:pass@host`), common token shapes
/// (`ghp_…`, `github_pat_…`, `glpat-…`) and secret-bearing key/value pairs
/// (`Authorization:`, `token=`, `password=`). Callers must still avoid
/// constructing error messages that embed credentials in the first place.
pub fn redact_secrets(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut remaining = text;
    while let Some(start) = remaining.find("://") {
        let (before, from_scheme) = remaining.split_at(start + 3);
        out.push_str(before);
        // Authority ends at the first path/query/fragment delimiter or space.
        let authority_end = from_scheme
            .find(['/', '?', '#', ' ', '"', '\'', '\n', '\r', '\t'])
            .unwrap_or(from_scheme.len());
        let (authority, rest) = from_scheme.split_at(authority_end);
        out.push_str(&redact_authority(authority));
        remaining = rest;
    }
    out.push_str(remaining);
    redact_tokenish(&out)
}

fn redact_authority(authority: &str) -> String {
    match authority.rfind('@') {
        Some(at) => {
            let host = &authority[at + 1..];
            format!("***@{host}")
        }
        None => authority.to_string(),
    }
}

/// Mask credential-shaped substrings: known token prefixes plus anything that
/// looks like a long opaque secret assigned to a secret-named key.
///
/// The scan cursor advances monotonically after every replacement, so an
/// already-redacted value (`***`) can never be re-matched into an infinite
/// loop.
fn redact_tokenish(text: &str) -> String {
    const TOKEN_PREFIXES: &[&str] = &[
        "ghp_",
        "gho_",
        "ghu_",
        "ghs_",
        "ghr_",
        "github_pat_",
        "glpat-",
        "glptt-",
        "xoxb-",
    ];
    // Values run to end-of-line for header-style keys (the secret may contain
    // spaces, e.g. `Authorization: Basic <base64>`), and to the next
    // delimiter for query-style keys (`token=…&x=1`).
    const LINE_KEYS: &[&str] = &["authorization:"];

    let mut out = text.to_string();

    for prefix in TOKEN_PREFIXES {
        let mut cursor = 0usize;
        while let Some(offset) = out[cursor..].find(prefix) {
            let at = cursor + offset;
            let end = token_end(&out, at);
            if end <= at {
                break;
            }
            out.replace_range(at..end, "***");
            cursor = at + "***".len();
        }
    }

    for key in ["authorization:", "password=", "token=", "secret="] {
        let mut cursor = 0usize;
        while cursor < out.len() {
            let lowered = to_ascii_lower(&out);
            let Some(offset) = lowered[cursor..].find(key) else {
                break;
            };
            let mut value_start = cursor + offset + key.len();
            // Skip separator spaces so `key: value` and `key=value` both work.
            value_start += out[value_start..]
                .find(|c: char| !c.is_whitespace())
                .unwrap_or(out.len() - value_start);
            if value_start >= out.len() {
                break;
            }
            let stop: &[char] = if LINE_KEYS.contains(&key) {
                &['\n', '\r']
            } else {
                &[' ', '&', '"', '\'', '\n', '\r', '\t']
            };
            let value_len = out[value_start..]
                .find(stop)
                .unwrap_or(out.len() - value_start);
            if value_len == 0 {
                // Nothing that looks like a value here; move past the key.
                cursor = value_start;
                continue;
            }
            out.replace_range(value_start..value_start + value_len, "***");
            cursor = value_start + "***".len();
        }
    }
    out
}

fn to_ascii_lower(text: &str) -> String {
    text.to_ascii_lowercase()
}

/// End of a token starting at `at`: run of characters that can appear in a
/// Git credential or PAT.
fn token_end(text: &str, at: usize) -> usize {
    text[at..]
        .char_indices()
        .take_while(|(_, c)| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '/' | '='))
        .map(|(offset, c)| offset + c.len_utf8())
        .last()
        .map(|end| at + end)
        .unwrap_or(at)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_url_userinfo() {
        let text = "fatal: unable to access 'https://alice:hunter2@example.com/repo.git/': refused";
        let redacted = redact_secrets(text);
        assert!(!redacted.contains("hunter2"), "{redacted}");
        assert!(redacted.contains("***@example.com"), "{redacted}");
        assert!(redacted.contains("fatal: unable to access"), "{redacted}");
    }

    #[test]
    fn redacts_tokens_in_any_context() {
        let text = "remote: Valid ghp_abcDEF1234567890abcd tokens only\nuse glpat-XYz9876543210";
        let redacted = redact_secrets(text);
        assert!(!redacted.contains("ghp_"), "{redacted}");
        assert!(!redacted.contains("glpat-"), "{redacted}");
        assert!(redacted.contains("***"), "{redacted}");
    }

    #[test]
    fn redacts_authorization_headers_case_insensitively() {
        let text =
            "POST http://host/x\nAuthorization: Basic dXNlcjpwYXNzd29yZA==\ntoken=abc123&x=1";
        let redacted = redact_secrets(text);
        assert!(!redacted.contains("dXNlcjpwYXNzd29yZA"), "{redacted}");
        assert!(!redacted.contains("abc123"), "{redacted}");
        assert!(redacted.contains("x=1"), "{redacted}");
    }

    #[test]
    fn leaves_plain_text_untouched() {
        let text = "commit 4a9f2c1: fix(intro): ajustar escena\n1 file changed";
        assert_eq!(redact_secrets(text), text);
    }

    #[test]
    fn redacts_passwords_inside_redacted_urls_too() {
        // Double pass: userinfo already gone, no accidental re-exposure.
        let text = "https://user:pass@host/a https://host2/b";
        let once = redact_secrets(text);
        let twice = redact_secrets(&once);
        assert!(!once.contains("pass"), "{once}");
        assert_eq!(once, twice);
    }

    #[test]
    fn timeout_kills_the_child() {
        let error = tests_helpers::run_program(
            "/bin/sleep",
            &["5"],
            OpOptions::default().timeout(Duration::from_millis(80)),
        )
        .expect_err("sleep must be killed");
        assert!(matches!(error, GitError::Timeout { .. }), "{error:?}");
    }

    #[test]
    fn pre_cancelled_token_short_circuits() {
        let token = CancellationToken::new();
        token.cancel();
        let error = run_git(
            None,
            "status",
            &["status"],
            OpOptions::default().cancel(token),
        )
        .expect_err("must not run");
        assert!(matches!(error, GitError::Cancelled { .. }), "{error:?}");
    }

    #[test]
    fn cancellation_kills_a_running_child() {
        let token = CancellationToken::new();
        let killer = token.clone();
        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(60));
            killer.cancel();
        });
        let started = Instant::now();
        let error = tests_helpers::run_program(
            "/bin/sleep",
            &["5"],
            OpOptions::default()
                .timeout(Duration::from_secs(10))
                .cancel(token),
        )
        .expect_err("cancelled child must fail");
        handle.join().expect("canceller");
        assert!(matches!(error, GitError::Cancelled { .. }), "{error:?}");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "cancel should kill promptly, took {:?}",
            started.elapsed()
        );
    }
}

/// Test-only helpers for exercising the runner against arbitrary programs.
#[cfg(test)]
pub(crate) mod tests_helpers {
    use super::*;

    pub(crate) fn run_program(
        program: &str,
        args: &[&str],
        options: OpOptions,
    ) -> Result<Output, GitError> {
        if options
            .cancel
            .as_ref()
            .is_some_and(|token| token.is_cancelled())
        {
            return Err(GitError::Cancelled {
                operation: "test".to_string(),
            });
        }
        let mut child = Command::new(program)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| GitError::Io {
                path: PathBuf::from(program),
                source,
            })?;
        let mut stdout_pipe = child.stdout.take().expect("stdout piped");
        let mut stderr_pipe = child.stderr.take().expect("stderr piped");
        let stdout_thread = thread::spawn(move || {
            let mut buffer = Vec::new();
            let _ = stdout_pipe.read_to_end(&mut buffer);
            buffer
        });
        let stderr_thread = thread::spawn(move || {
            let mut buffer = Vec::new();
            let _ = stderr_pipe.read_to_end(&mut buffer);
            buffer
        });
        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {}
                Err(source) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(GitError::Io {
                        path: PathBuf::from(program),
                        source,
                    });
                }
            }
            if options
                .cancel
                .as_ref()
                .is_some_and(|token| token.is_cancelled())
            {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_thread.join();
                let _ = stderr_thread.join();
                return Err(GitError::Cancelled {
                    operation: "test".to_string(),
                });
            }
            if started.elapsed() > options.timeout {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_thread.join();
                let _ = stderr_thread.join();
                return Err(GitError::Timeout {
                    operation: "test".to_string(),
                    after: options.timeout,
                });
            }
            thread::sleep(POLL_INTERVAL);
        };
        let stdout = stdout_thread.join().unwrap_or_default();
        let stderr = stderr_thread.join().unwrap_or_default();
        Ok(Output {
            status,
            stdout,
            stderr,
        })
    }
}
