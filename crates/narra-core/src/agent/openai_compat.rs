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

use std::collections::BTreeMap;
use std::io::BufRead;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::AgentError;
use super::adapter::{ModelAdapter, ModelRequest, ModelResponse};
use super::tools::{ChatRole, ToolCall, ToolSpec};
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

/// Catalog metadata for one model. Optional fields stay `None` when the
/// provider does not advertise them; the UI never invents a limit or price.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_limit: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_limit: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_price_per_million: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_price_per_million: Option<f64>,
    #[serde(default)]
    pub reasoning: bool,
    #[serde(default)]
    pub tools: bool,
    #[serde(default)]
    pub image_input: bool,
}

impl ModelInfo {
    /// Metadata-less fallback for curated model ids.
    pub fn from_id(id: impl Into<String>) -> Self {
        let id = id.into();
        Self {
            name: id.clone(),
            id,
            ..Self::default()
        }
    }
}

/// Fetch the full catalog with limits and prices. The OpenAI-compatible
/// `/models` route is the same one used by `fetch_model_ids`.
pub fn fetch_models(
    models_url: &str,
    api_key: Option<&str>,
    timeout: Duration,
) -> Result<Vec<ModelInfo>, AgentError> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .build()
        .into();
    let mut request = agent.get(models_url);
    if let Some(key) = api_key {
        request = request.header("Authorization", bearer(key));
    }
    let mut response = request.call().map_err(|error| {
        AgentError::Adapter(format!(
            "no se pudo alcanzar `{models_url}`: {}",
            crate::git::redact_secrets(&error.to_string())
        ))
    })?;
    let status = response.status();
    let payload: Value = response.body_mut().read_json().map_err(|error| {
        AgentError::Adapter(format!(
            "respuesta no interpretable de `{models_url}` (HTTP {status}): {error}"
        ))
    })?;
    if !status.is_success() {
        let detail =
            server_error_detail(&payload).unwrap_or_else(|| format!("HTTP {}", status.as_u16()));
        return Err(AgentError::Adapter(format!(
            "el catálogo de modelos devolvió HTTP {}: {}",
            status.as_u16(),
            crate::git::redact_secrets(&detail)
        )));
    }
    Ok(parse_model_infos(&payload))
}

/// Fetch model ids from an OpenAI-style `/models` endpoint. Kept as a thin
/// wrapper for callers that only need the picker labels.
pub fn fetch_model_ids(
    models_url: &str,
    api_key: Option<&str>,
    timeout: Duration,
) -> Result<Vec<String>, AgentError> {
    Ok(fetch_models(models_url, api_key, timeout)?
        .into_iter()
        .map(|model| model.id)
        .collect())
}

fn parse_model_infos(payload: &Value) -> Vec<ModelInfo> {
    payload
        .get("data")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    let id = entry.get("id").and_then(Value::as_str)?;
                    let context = entry
                        .pointer("/limit/context")
                        .and_then(Value::as_u64)
                        .map(|value| value.min(u32::MAX as u64) as u32);
                    let output = entry
                        .pointer("/limit/output")
                        .and_then(Value::as_u64)
                        .map(|value| value.min(u32::MAX as u64) as u32);
                    let input_price = entry
                        .pointer("/cost/text/input/price")
                        .and_then(Value::as_f64);
                    let output_price = entry
                        .pointer("/cost/text/output/price")
                        .and_then(Value::as_f64);
                    let modalities = entry
                        .pointer("/modalities/input")
                        .and_then(Value::as_array)
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(Value::as_str)
                                .any(|mime| mime.starts_with("image"))
                        })
                        .unwrap_or(false);
                    Some(ModelInfo {
                        id: id.to_string(),
                        name: entry
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or(id)
                            .to_string(),
                        context_limit: context,
                        output_limit: output,
                        input_price_per_million: input_price,
                        output_price_per_million: output_price,
                        reasoning: entry
                            .pointer("/capabilities/reasoning/supported")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        tools: entry
                            .pointer("/capabilities/tools/supported")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        image_input: modalities,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
fn parse_model_ids(payload: &Value) -> Vec<String> {
    parse_model_infos(payload)
        .into_iter()
        .map(|model| model.id)
        .collect()
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

        let body = chat_body(&self.config.model, request, false);

        let mut response = self
            .send_chat(&body)
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
            return Err(http_error(status.as_u16(), &payload, &self.config));
        }

        if cancel.is_cancelled() {
            return Err(AgentError::Cancelled);
        }

        parse_response(payload)
    }

    fn complete_streaming(
        &self,
        request: &ModelRequest,
        cancel: &CancellationToken,
        on_delta: &mut dyn FnMut(&str),
    ) -> Result<ModelResponse, AgentError> {
        if cancel.is_cancelled() {
            return Err(AgentError::Cancelled);
        }

        let mut body = chat_body(&self.config.model, request, true);
        // Standard OpenAI knob: usage arrives in a final chunk instead of
        // being dropped. Servers that ignore it (older local servers)
        // simply report no usage — never an error.
        body["stream_options"] = json!({"include_usage": true});

        let response = self
            .send_chat(&body)
            .map_err(|error| classify_error(error, &self.config))?;
        let status = response.status();

        // Non-2xx: the server answered with a JSON error body, not a
        // stream — surface it exactly like the non-streaming path.
        if !status.is_success() {
            let payload: Value = response.into_body().read_json().unwrap_or(Value::Null);
            return Err(http_error(status.as_u16(), &payload, &self.config));
        }

        // The SSE stream. `timeout_global` (set in `new`) bounds the whole
        // exchange including this read, so a stalled provider errors instead
        // of hanging; `cancel` is observed *between* chunks, which is what
        // makes mid-generation cancellation real.
        let mut reader = std::io::BufReader::new(response.into_body().into_reader());
        let mut text = String::new();
        let mut model: Option<String> = None;
        let mut usage: Option<Value> = None;
        let mut tool_calls: BTreeMap<usize, PartialToolCall> = BTreeMap::new();
        let mut line = String::new();

        loop {
            if cancel.is_cancelled() {
                return Err(AgentError::Cancelled);
            }
            line.clear();
            let read = reader.read_line(&mut line).map_err(|error| {
                AgentError::Adapter(format!("el stream se interrumpió: {error}"))
            })?;
            if read == 0 {
                // Clean EOF: some servers close without `[DONE]`. Accept it
                // as end-of-stream; content validity is checked below.
                break;
            }
            let trimmed = line.trim_end();
            // SSE framing: blank separators, `event:` fields and `:`
            // comments are not data. Only `data:` lines carry payloads.
            let Some(data) = trimmed.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim_start();
            if data == "[DONE]" {
                break;
            }
            let Ok(chunk) = serde_json::from_str::<Value>(data) else {
                // A malformed chunk is a protocol violation, not something
                // to silently drop: the caller must know the text is partial.
                return Err(AgentError::Adapter(
                    "el stream contiene un chunk que no es JSON válido".to_string(),
                ));
            };
            if let Some(message) = server_error_detail(&chunk) {
                return Err(AgentError::Adapter(crate::git::redact_secrets(&message)));
            }
            if let Some(name) = chunk.get("model").and_then(Value::as_str) {
                model = Some(name.to_string());
            }
            if chunk.get("usage").is_some_and(|usage| !usage.is_null()) {
                usage = chunk.get("usage").cloned();
            }
            if let Some(delta) = chunk.pointer("/choices/0/delta/content")
                && let Some(piece) = delta.as_str()
                && !piece.is_empty()
            {
                text.push_str(piece);
                on_delta(piece);
            }
            // OpenAI streams function calls as deltas: `id`/`name` normally
            // arrive first, `arguments` in fragments, all keyed by `index`.
            if let Some(calls) = chunk
                .pointer("/choices/0/delta/tool_calls")
                .and_then(Value::as_array)
            {
                for (position, call) in calls.iter().enumerate() {
                    let index = call
                        .get("index")
                        .and_then(Value::as_u64)
                        .map(|index| index as usize)
                        .unwrap_or(position);
                    let partial = tool_calls.entry(index).or_default();
                    if let Some(id) = call.get("id").and_then(Value::as_str)
                        && !id.is_empty()
                    {
                        partial.id = id.to_string();
                    }
                    if let Some(name) = call.pointer("/function/name").and_then(Value::as_str)
                        && !name.is_empty()
                    {
                        partial.name.push_str(name);
                    }
                    if let Some(arguments) =
                        call.pointer("/function/arguments").and_then(Value::as_str)
                    {
                        partial.arguments.push_str(arguments);
                    }
                }
            }
        }

        if cancel.is_cancelled() {
            return Err(AgentError::Cancelled);
        }
        if text.is_empty() && tool_calls.is_empty() {
            return Err(AgentError::Adapter(
                "el stream terminó sin contenido de texto ni tool_calls".to_string(),
            ));
        }

        let mut parsed_calls = Vec::with_capacity(tool_calls.len());
        for partial in tool_calls.into_values() {
            if partial.name.trim().is_empty() {
                return Err(AgentError::Adapter(
                    "el stream contiene un tool_call sin function.name".to_string(),
                ));
            }
            let arguments = if partial.arguments.trim().is_empty() {
                json!({})
            } else {
                super::scaffold::parse_any_json_object(&partial.arguments).ok_or_else(|| {
                    AgentError::Adapter(format!(
                        "los argumentos de la herramienta «{}» no son JSON válido",
                        partial.name
                    ))
                })?
            };
            parsed_calls.push(ToolCall {
                id: partial.id,
                name: partial.name,
                arguments,
            });
        }

        let usage = usage.unwrap_or(Value::Null);
        Ok(ModelResponse {
            text,
            model: model.unwrap_or_else(|| "desconocido".to_string()),
            tool_calls: parsed_calls,
            input_tokens: usage
                .get("prompt_tokens")
                .and_then(Value::as_u64)
                .map(|tokens| tokens as u32),
            output_tokens: usage
                .get("completion_tokens")
                .and_then(Value::as_u64)
                .map(|tokens| tokens as u32),
        })
    }
}

#[derive(Debug, Default)]
struct PartialToolCall {
    id: String,
    name: String,
    arguments: String,
}

impl OpenAiCompatAdapter {
    /// POST one chat-completions body with the configured auth and headers.
    fn send_chat(&self, body: &Value) -> Result<ureq::http::Response<ureq::Body>, ureq::Error> {
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
        call.send_json(body)
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

/// Build the actionable message for a non-2xx reply, quoting the server's
/// own error detail when present and never leaking credentials.
fn http_error(status: u16, payload: &Value, config: &OpenAiCompatConfig) -> AgentError {
    let detail = server_error_detail(payload).unwrap_or_else(|| config.base_url.clone());
    AgentError::Adapter(format!(
        "el endpoint devolvió HTTP {status}: {}",
        crate::git::redact_secrets(&detail)
    ))
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

    // Tool calls (OpenAI function-calling wire): `arguments` is a JSON
    // *string* on the wire; a malformed one is an actionable error, never
    // silently dropped.
    let mut tool_calls = Vec::new();
    if let Some(calls) = payload.pointer("/choices/0/message/tool_calls") {
        let Some(calls) = calls.as_array() else {
            return Err(AgentError::Adapter(
                "tool_calls del proveedor no es una lista".to_string(),
            ));
        };
        for call in calls {
            let id = call
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_default();
            let name = call
                .pointer("/function/name")
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| {
                    AgentError::Adapter("tool_call sin function.name en la respuesta".to_string())
                })?;
            let arguments = match call.pointer("/function/arguments") {
                // Wire format: JSON-encoded string. Models often emit raw
                // newlines or trailing commas inside long content; use the
                // same conservative repair as the scaffold plan parser.
                Some(Value::String(raw)) => super::scaffold::parse_any_json_object(raw)
                    .ok_or_else(|| {
                        AgentError::Adapter(format!(
                            "los argumentos de la herramienta «{name}» no son JSON válido"
                        ))
                    })?,
                // Some local servers already send an object.
                Some(value @ Value::Object(_)) => value.clone(),
                _ => json!({}),
            };
            tool_calls.push(ToolCall {
                id,
                name,
                arguments,
            });
        }
    }

    let text = payload
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_default();

    // A response with neither text nor tool calls carries nothing usable.
    // The finish reason tells the user *why* (e.g. a reasoning model spent
    // its whole token budget on reasoning and stopped at the length limit).
    if text.trim().is_empty() && tool_calls.is_empty() {
        let finish = payload
            .pointer("/choices/0/finish_reason")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| "desconocida".to_string());
        return Err(AgentError::Adapter(format!(
            "la respuesta no contiene choices[0].message.content ni tool_calls \
             (finish_reason: {finish}); posible presupuesto de tokens agotado \
             en el razonamiento del modelo"
        )));
    }

    let usage = payload.get("usage");
    Ok(ModelResponse {
        text,
        model: payload
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| "desconocido".to_string()),
        tool_calls,
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

/// Full chat body for the wire: system + user + tool-loop history, plus the
/// tool declarations when the request offers any (omitted otherwise so
/// single-shot traffic stays byte-identical to pre-tool-call requests).
fn chat_body(model: &str, request: &ModelRequest, stream: bool) -> Value {
    let mut body = json!({
        "model": model,
        "messages": chat_messages(request),
        "max_tokens": request.max_output_tokens,
        "stream": stream,
    });
    if !request.tools.is_empty() {
        body["tools"] = tool_declarations(&request.tools);
    }
    body
}

/// System, user, then the recorded tool-loop turns (assistant tool requests
/// and tool results), oldest first.
fn chat_messages(request: &ModelRequest) -> Value {
    let mut messages = vec![
        json!({"role": "system", "content": request.system}),
        json!({"role": "user", "content": request.prompt}),
    ];
    for turn in &request.history {
        match turn.role {
            ChatRole::Assistant => {
                let mut message = json!({
                    "role": "assistant",
                    "content": turn.content,
                });
                if !turn.tool_calls.is_empty() {
                    let calls: Vec<Value> = turn
                        .tool_calls
                        .iter()
                        .map(|call| {
                            json!({
                                "id": call.id,
                                "type": "function",
                                "function": {
                                    "name": call.name,
                                    // Wire format: arguments as a JSON string.
                                    "arguments": serde_json::to_string(&call.arguments)
                                        .unwrap_or_else(|_| "{}".to_string()),
                                },
                            })
                        })
                        .collect();
                    message["tool_calls"] = Value::Array(calls);
                }
                messages.push(message);
            }
            ChatRole::Tool => {
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": turn.tool_call_id,
                    "content": turn.content,
                }));
            }
        }
    }
    Value::Array(messages)
}

/// Tool declarations in the OpenAI function-calling shape.
fn tool_declarations(tools: &[ToolSpec]) -> Value {
    Value::Array(
        tools
            .iter()
            .map(|spec| {
                json!({
                    "type": "function",
                    "function": {
                        "name": spec.name,
                        "description": spec.description,
                        "parameters": spec.parameters,
                    },
                })
            })
            .collect(),
    )
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
    fn tool_arguments_tolerate_raw_newlines_and_trailing_commas() {
        let payload = json!({
            "model": "x",
            "choices": [{"message": {"role": "assistant", "content": "", "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "propose_document_create",
                    "arguments": "{\"path\":\"bible/characters/elena.md\",\"instruction\":\"crea\",\"content\":\"# Elena\n\nHermana de Marta.\",}"
                }
            }]}}]
        });
        let response = parse_response(payload).expect("tolerant tool args");
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(
            response.tool_calls[0].arguments["content"],
            "# Elena\n\nHermana de Marta."
        );
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
    fn model_catalog_metadata_is_parsed() {
        let payload: Value = serde_json::from_str(
            r#"{"data":[{
                "id":"deepseek-v4.1-flash",
                "name":"DeepSeek V4.1 Flash",
                "limit":{"context":1000000,"output":384000},
                "cost":{"text":{"input":{"price":0.15},"output":{"price":0.6}}},
                "capabilities":{"reasoning":{"supported":true},"tools":{"supported":true}},
                "modalities":{"input":["text","image/png"]}
            },{"id":"plain-model"}]}"#,
        )
        .expect("json");
        let models = parse_model_infos(&payload);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "deepseek-v4.1-flash");
        assert_eq!(models[0].name, "DeepSeek V4.1 Flash");
        assert_eq!(models[0].context_limit, Some(1_000_000));
        assert_eq!(models[0].output_limit, Some(384_000));
        assert_eq!(models[0].input_price_per_million, Some(0.15));
        assert_eq!(models[0].output_price_per_million, Some(0.6));
        assert!(models[0].reasoning);
        assert!(models[0].tools);
        assert!(models[0].image_input);
        assert_eq!(models[1].name, "plain-model");
        assert_eq!(models[1].context_limit, None);
    }

    #[test]
    fn model_catalog_ids_are_parsed_and_missing_entries_are_skipped() {
        let payload: Value = serde_json::from_str(
            r#"{"object":"list","data":[{"id":"deepseek-v4.1-flash"},{"id":"glm-5.3-flash"},{"name":"no id"}]}"#,
        )
        .expect("json");
        assert_eq!(
            parse_model_ids(&payload),
            vec!["deepseek-v4.1-flash", "glm-5.3-flash"]
        );
        assert!(parse_model_ids(&json!({})).is_empty());
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
            tools: Vec::new(),
            history: Vec::new(),
        };
        let error = adapter.complete(&request, &token).expect_err("cancelled");
        assert!(matches!(error, AgentError::Cancelled), "{error:?}");
    }
}
