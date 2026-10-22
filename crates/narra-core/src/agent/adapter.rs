//! Provider boundary for model calls (D-018: first provider OpenAI).
//!
//! Tasks depend only on this trait; concrete providers (OpenAI, others)
//! implement it, and tests use [`crate::agent::MockAdapter`]. No provider
//! SDK is pulled into the core: the first real adapter speaks the OpenAI
//! HTTP API directly behind this interface.
//!
//! The trait is synchronous by design for now: tasks already run on worker
//! threads (never the UI thread), and cancellation is cooperative through
//! [`CancellationToken`]. If Spike C shows streaming is needed, the trait
//! gains an event-callback variant — a compatible, additive change.

use crate::agent::AgentError;
use crate::cancel::CancellationToken;

/// Explicit, serializable model input (Spike F).
#[derive(Debug, Clone, PartialEq)]
pub struct ModelRequest {
    /// System/instruction text.
    pub system: String,
    /// User content: the scoped context plus the task question.
    pub prompt: String,
    /// Hard cap on generated tokens; adapters must enforce it.
    pub max_output_tokens: u32,
}

/// Minimal model output contract. The raw text stays available for display;
/// structured payloads are parsed by the calling task, not the adapter.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelResponse {
    pub text: String,
    /// Provider-reported model identifier (for provenance records).
    pub model: String,
    /// Token usage if the provider reported it.
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
}

/// A model provider. Implementations must honor the cancellation token
/// promptly and must not embed credentials in errors.
pub trait ModelAdapter {
    /// Stable provider name for logs and provenance ("openai", "mock").
    fn name(&self) -> &str;

    /// Execute one completion. Must return [`AgentError::Cancelled`] quickly
    /// when `cancel` is triggered, and must never block the calling thread
    /// longer than the provider's own bounded request.
    fn complete(
        &self,
        request: &ModelRequest,
        cancel: &CancellationToken,
    ) -> Result<ModelResponse, AgentError>;

    /// Stream the completion, delivering incremental text through `on_delta`
    /// as the provider generates it (Spike C: real mid-generation
    /// cancellation — the token is observed *between* chunks, not only
    /// around the request).
    ///
    /// The returned [`ModelResponse`] carries the **full** accumulated text
    /// (plus model and usage), so callers can treat streaming as an
    /// enhancement of [`ModelAdapter::complete`], never as a different
    /// result. Adapters without native streaming keep the default: one
    /// final delta, then the response.
    fn complete_streaming(
        &self,
        request: &ModelRequest,
        cancel: &CancellationToken,
        on_delta: &mut dyn FnMut(&str),
    ) -> Result<ModelResponse, AgentError> {
        let response = self.complete(request, cancel)?;
        on_delta(&response.text);
        Ok(response)
    }
}
