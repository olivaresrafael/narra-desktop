//! OpenAI-compatible chat-completions adapter (D-018 clarified 2026-10-22).
//!
//! One adapter speaks the *OpenAI chat format* — the industry's de-facto
//! standard — against any compatible endpoint. Providers are configuration,
//! not code:
//!
//! | provider | base URL | key |
//! |---|---|---|
//! | Ollama (local, no account) | `http://localhost:11434/v1` | none |
//! | OpenAI | `https://api.openai.com/v1` | required |
//! | LM Studio / llama.cpp / vLLM | local | none |
//! | OpenRouter / Groq / Together / DeepSeek | cloud | required |
//!
//! Security rules (`AGENTS.md`):
//! - the API key travels only in the `Authorization` header of outgoing
//!   requests; it never appears in error messages, logs or provenance;
//! - errors are actionable and bounded by explicit timeouts;
//! - no provider SDK is used — the wire format is small and stable, and a
//!   dependency-free client keeps provider code behind one trait.

use std::time::Duration;

use serde_json::{Value, json};

use super::AgentError;
use super::adapter::{ModelAdapter, ModelRequest, ModelResponse};
use crate::cancel::CancellationToken;

/// Default endpoints, used when the caller does not configure one.
pub const LOCAL_DEFAULT_BASE_URL: &str = "http://localhost:11434/v1";

/// Configuration for an OpenAI-compatible endpoint.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenAiCompatConfig {
    /// Endpoint root, without the trailing `/chat/completions`
    /// (e.g. `http://localhost:11434/v1`).
    pub base_url: String,
    /// Bearer token; `None` for local servers (Ollama, llama.cpp, …).
    pub api_key: Option<String>,
    /// Model name as the endpoint knows it (`qwen2.5vl:7b`, `gpt-4o-mini`, …).
    pub model: String,
    /// Per-request timeout.
    pub timeout: Duration,
    /// Optional extra headers (e.g. provider routing). Never put secrets
    /// here: only the `api_key` field is redacted in error paths.
    pub extra_headers: Vec<(String, String)>,
}

impl OpenAiCompatConfig {
    /// Local Ollama endpoint with a model name; no key required.
    pub fn ollama(model: impl Into<String>) -> Self {
        Self {
            base_url: LOCAL_DEFAULT_BASE_URL.to_string(),
            api_key: None,
            model: model.into(),
            timeout: Duration::from_secs(120),
            extra_headers: Vec::new(),
        }
    }

    /// OpenAI's public endpoint.
    pub fn openai(model: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: Some(api_key.into()),
            model: model.into(),
            timeout: Duration::from_secs(120),
            extra_headers: Vec::new(),
        }
    }

    fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }
}

/// The single provider adapter: OpenAI wire format, any compatible server.
#[derive(Debug, Clone)]
pub struct OpenAiCompatAdapter {
    config: OpenAiCompatConfig,
    agent: ureq::Agent,
}

impl OpenAiCompatAdapter {
    pub fn new(config: OpenAiCompatConfig) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(config.timeout))
            // Non-2xx responses are data, not opaque errors: we read the
            // server's JSON error body to build an actionable message.
            .http_status_as_error(false)
            .build()
            .into();
        Self { config, agent }
    }

    /// True when the endpoint answers at all (bounded, read-only probe).
    /// Used for capability detection before offering the provider in the UI.
    pub fn probe(&self) -> Result<bool, AgentError> {
        let url = format!("{}/models", self.config.base_url.trim_end_matches('/'));
        let mut request = self.agent.get(&url);
        if let Some(key) = &self.config.api_key {
            request = request.header("Authorization", bearer(key));
        }
        match request.call() {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    Ok(true)
                } else {
                    // The endpoint exists and speaks HTTP; 401/403 mean auth
                    // is the problem, not compatibility.
                    Ok(status == 401 || status == 403)
                }
            }
            // Transport-class failures (io, dns, connection): the endpoint is
            // unreachable. `Display` for these never includes request
            // headers, and the text passes through the redactor anyway.
            Err(error) => Err(AgentError::Adapter(format!(
                "no se pudo alcanzar el endpoint en `{}`: {}",
                self.config.base_url,
                crate::git::redact_secrets(&error.to_string())
            ))),
        }
    }
}

impl ModelAdapter for OpenAiCompatAdapter {
    fn name(&self) -> &str {
        "openai-compat"
    }

    fn complete(
        &self,
        request: &ModelRequest,
        cancel: &CancellationToken,
    ) -> Result<ModelResponse, AgentError> {
        if cancel.is_cancelled() {
            return Err(AgentError::Cancelled);
        }

        let body = json!({
            "model": self.config.model,
            "messages": [
                {"role": "system", "content": request.system},
                {"role": "user", "content": request.prompt},
            ],
            "max_tokens": request.max_output_tokens,
            "stream": false,
        });

        let mut call = self
            .agent
            .post(self.config.chat_url())
            .header("Content-Type", "application/json");
        // The key exists only here: header value, never in our error text.
        if let Some(key) = &self.config.api_key {
            call = call.header("Authorization", bearer(key));
        }
        for (name, value) in &self.config.extra_headers {
            call = call.header(name, value);
        }

        let mut response = call
            .send_json(&body)
            .map_err(|error| classify_error(error, &self.config))?;
        let status = response.status();

        // Body first, status second: the server's own error message is the
        // most actionable text we can show ("model not found", …).
        let payload: Value = response.body_mut().read_json().map_err(|error| {
            AgentError::Adapter(format!(
                "respuesta no interpretable del endpoint (HTTP {status}): {error}"
            ))
        })?;

        if !status.is_success() {
            let detail =
                server_error_detail(&payload).unwrap_or_else(|| self.config.base_url.clone());
            return Err(AgentError::Adapter(format!(
                "el endpoint devolvió HTTP {status}: {}",
                crate::git::redact_secrets(&detail)
            )));
        }

        if cancel.is_cancelled() {
            return Err(AgentError::Cancelled);
        }

        parse_response(payload)
    }
}

fn bearer(key: &str) -> String {
    format!("Bearer {key}")
}

/// Best-effort extraction of the server's own error message from either
/// wire style: OpenAI's `{"error": {"message": …}}` or Ollama's native
/// `{"error": "…"}`.
fn server_error_detail(payload: &Value) -> Option<String> {
    match payload.get("error") {
        Some(Value::String(message)) if !message.is_empty() => Some(message.clone()),
        Some(Value::Object(_)) => payload
            .pointer("/error/message")
            .and_then(Value::as_str)
            .filter(|message| !message.is_empty())
            .map(str::to_string),
        _ => None,
    }
}

/// Turn transport failures into actionable messages with **no secret
/// content**: we only ever quote the endpoint URL (never credentials) and
/// pass the text through the shared redactor. HTTP status failures never
/// reach here — with `http_status_as_error(false)` they arrive as responses
/// and are handled by the caller with the server's own error body.
fn classify_error(error: ureq::Error, config: &OpenAiCompatConfig) -> AgentError {
    AgentError::Adapter(format!(
        "fallo de red contra `{}`: {}",
        config.base_url,
        crate::git::redact_secrets(&error.to_string())
    ))
}

fn parse_response(payload: Value) -> Result<ModelResponse, AgentError> {
    // Error envelope some servers use with 200 (never log raw: may echo
    // request data).
    if let Some(message) = server_error_detail(&payload) {
        return Err(AgentError::Adapter(crate::git::redact_secrets(&message)));
    }

    let text = payload
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| {
            AgentError::Adapter("la respuesta no contiene choices[0].message.content".to_string())
        })?;

    let usage = payload.get("usage");
    Ok(ModelResponse {
        text,
        model: payload
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| "desconocido".to_string()),
        input_tokens: usage
            .and_then(|usage| usage.get("prompt_tokens"))
            .and_then(Value::as_u64)
            .map(|tokens| tokens as u32),
        output_tokens: usage
            .and_then(|usage| usage.get("completion_tokens"))
            .and_then(Value::as_u64)
            .map(|tokens| tokens as u32),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_url_is_built_without_double_slash() {
        let mut config = OpenAiCompatConfig::ollama("qwen");
        assert_eq!(
            config.chat_url(),
            "http://localhost:11434/v1/chat/completions"
        );
        config.base_url = "http://host/v1/".to_string();
        assert_eq!(config.chat_url(), "http://host/v1/chat/completions");
    }

    #[test]
    fn parses_standard_chat_completion_payload() {
        let payload = json!({
            "model": "qwen2.5vl:7b",
            "choices": [{"message": {"role": "assistant", "content": "Hola"}}],
            "usage": {"prompt_tokens": 12, "completion_tokens": 5}
        });
        let response = parse_response(payload).expect("parse");
        assert_eq!(response.text, "Hola");
        assert_eq!(response.model, "qwen2.5vl:7b");
        assert_eq!(response.input_tokens, Some(12));
        assert_eq!(response.output_tokens, Some(5));
    }

    #[test]
    fn error_envelope_is_surfaced_and_redacted() {
        let payload = json!({
            "error": {"message": "invalid key ghp_abcdef1234567890abcd"}
        });
        let error = parse_response(payload).expect_err("must fail");
        let rendered = error.to_string();
        assert!(!rendered.contains("ghp_"), "no token in errors: {rendered}");
        assert!(rendered.contains("invalid key"), "{rendered}");
    }

    #[test]
    fn missing_choices_is_an_actionable_error() {
        let error = parse_response(json!({"model": "x"})).expect_err("fail");
        assert!(error.to_string().contains("choices[0]"), "{error}");
    }

    #[test]
    fn bearer_token_never_leaks_into_classify_error_messages() {
        let config = OpenAiCompatConfig::openai("gpt-4o-mini", "sk-super-secret-value");
        let error = classify_error(ureq::Error::StatusCode(401), &config);
        let rendered = error.to_string();
        assert!(!rendered.contains("sk-super-secret-value"), "{rendered}");
        assert!(rendered.contains("401"), "{rendered}");
    }

    #[test]
    fn pre_cancelled_token_short_circuits_before_any_request() {
        let adapter = OpenAiCompatAdapter::new(OpenAiCompatConfig::ollama("qwen"));
        let token = CancellationToken::new();
        token.cancel();
        let request = ModelRequest {
            system: "s".into(),
            prompt: "p".into(),
            max_output_tokens: 10,
        };
        let error = adapter.complete(&request, &token).expect_err("cancelled");
        assert!(matches!(error, AgentError::Cancelled), "{error:?}");
    }
}
