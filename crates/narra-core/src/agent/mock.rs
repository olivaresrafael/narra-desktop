//! Deterministic mock provider for tests and the headless harness (Spike C:
//! "use a mock adapter for the free repeatable runs").
//!
//! Behavior is a pure function of the request: same input, same output, no
//! network, no credentials. Cancellation is honored between short sleeps so
//! tests can exercise the cancellation path without real latency. For tool
//! loops, [`MockAdapter::scripted`] replays a fixed sequence of responses
//! (text and/or tool calls) so multi-round behavior stays deterministic.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use super::AgentError;
use super::adapter::{ModelAdapter, ModelRequest, ModelResponse, StreamPart};
use crate::cancel::CancellationToken;

/// A provider that never talks to a network.
#[derive(Debug)]
pub struct MockAdapter {
    /// Simulated latency in 5 ms slices; 0 keeps tests instant.
    pub simulated_latency: Duration,
    /// Optional scripted responses, replayed one per `complete` call (tool
    /// loop tests). Empty script = the default deterministic answer.
    script: Option<Mutex<VecDeque<ModelResponse>>>,
    /// Optional reasoning text emitted before the answer in streamed runs.
    reasoning: Option<String>,
}

impl MockAdapter {
    pub fn new() -> Self {
        Self {
            simulated_latency: Duration::ZERO,
            script: None,
            reasoning: None,
        }
    }

    pub fn with_latency(latency: Duration) -> Self {
        Self {
            simulated_latency: latency,
            script: None,
            reasoning: None,
        }
    }

    /// A mock that replays `responses` in order, one per `complete` call.
    /// Once the script is exhausted it falls back to the default answer.
    pub fn scripted(responses: Vec<ModelResponse>) -> Self {
        Self {
            simulated_latency: Duration::ZERO,
            script: Some(Mutex::new(VecDeque::from(responses))),
            reasoning: None,
        }
    }

    /// Emit a fixed reasoning trace before the streamed answer. Used by
    /// tests and the offline demo; real providers supply their own deltas.
    pub fn with_reasoning(mut self, reasoning: impl Into<String>) -> Self {
        self.reasoning = Some(reasoning.into());
        self
    }

    /// Next scripted response, or the deterministic default once the script
    /// is exhausted. Shared by `complete` and `complete_streaming` so a
    /// streamed tool round consumes exactly one script entry.
    fn next_response(&self, request: &ModelRequest) -> ModelResponse {
        if let Some(script) = &self.script
            && let Some(next) = script.lock().expect("script lock").pop_front()
        {
            return next;
        }
        response_for(request)
    }

    /// Cooperative latency: wake in 5 ms slices, observing cancellation.
    fn wait_latency(&self, cancel: &CancellationToken) -> Result<(), AgentError> {
        let mut waited = Duration::ZERO;
        while waited < self.simulated_latency {
            if cancel.is_cancelled() {
                return Err(AgentError::Cancelled);
            }
            let slice = Duration::from_millis(5);
            thread::sleep(slice);
            waited += slice;
        }
        if cancel.is_cancelled() {
            return Err(AgentError::Cancelled);
        }
        Ok(())
    }
}

impl Default for MockAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelAdapter for MockAdapter {
    fn name(&self) -> &str {
        "mock"
    }

    fn complete(
        &self,
        request: &ModelRequest,
        cancel: &CancellationToken,
    ) -> Result<ModelResponse, AgentError> {
        // Cooperative latency: wake in small slices and observe cancellation.
        self.wait_latency(cancel)?;

        // Scripted runs (tool loops) replay their fixed sequence first.
        Ok(self.next_response(request))
    }

    fn complete_streaming(
        &self,
        request: &ModelRequest,
        cancel: &CancellationToken,
        on_delta: &mut dyn FnMut(&str),
    ) -> Result<ModelResponse, AgentError> {
        // Same deterministic or scripted response, delivered in fixed-size
        // chunks with the simulated latency spread between them: cancellation
        // is observed *between* chunks, mirroring a real streaming provider.
        let response = self.next_response(request);
        let chunks: Vec<&str> = chunked(&response.text, 12);
        let slice = if chunks.is_empty() {
            Duration::ZERO
        } else {
            self.simulated_latency / chunks.len().max(1) as u32
        };
        for chunk in chunks {
            if cancel.is_cancelled() {
                return Err(AgentError::Cancelled);
            }
            if slice > Duration::ZERO {
                thread::sleep(slice);
            }
            on_delta(chunk);
        }
        if cancel.is_cancelled() {
            return Err(AgentError::Cancelled);
        }
        Ok(response)
    }

    fn complete_streaming_with_reasoning(
        &self,
        request: &ModelRequest,
        cancel: &CancellationToken,
        on_delta: &mut dyn FnMut(StreamPart<'_>),
    ) -> Result<ModelResponse, AgentError> {
        if let Some(reasoning) = &self.reasoning {
            for chunk in chunked(reasoning, 12) {
                if cancel.is_cancelled() {
                    return Err(AgentError::Cancelled);
                }
                on_delta(StreamPart::Reasoning(chunk));
            }
        }
        let response = self.next_response(request);
        let chunks: Vec<&str> = chunked(&response.text, 12);
        let slice = if chunks.is_empty() {
            Duration::ZERO
        } else {
            self.simulated_latency / chunks.len().max(1) as u32
        };
        for chunk in chunks {
            if cancel.is_cancelled() {
                return Err(AgentError::Cancelled);
            }
            if slice > Duration::ZERO {
                thread::sleep(slice);
            }
            on_delta(StreamPart::Content(chunk));
        }
        if cancel.is_cancelled() {
            return Err(AgentError::Cancelled);
        }
        Ok(response)
    }
}

/// The mock's deterministic answer for a request.
fn response_for(request: &ModelRequest) -> ModelResponse {
    let text = format!(
        "MOCK BRIEF ({} entrada / {} salida tokens declarados)\n{}",
        request.max_output_tokens,
        request.max_output_tokens / 2,
        summarize(&request.prompt)
    );
    ModelResponse {
        text,
        model: "mock-1".to_string(),
        tool_calls: Vec::new(),
        input_tokens: Some(request.prompt.len() as u32 / 4),
        output_tokens: Some(request.max_output_tokens / 2),
    }
}

/// Split `text` into fixed-size chunks on char boundaries. Deterministic:
/// same text, same chunks. Empty text yields no chunks.
fn chunked(text: &str, size: usize) -> Vec<&str> {
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let mut end = (start + size).min(text.len());
        while !text.is_char_boundary(end) {
            end += 1;
        }
        chunks.push(&text[start..end]);
        start = end;
    }
    chunks
}

/// Deterministic, allocation-light digest of the prompt for the mock output.
fn summarize(prompt: &str) -> String {
    let lines = prompt.lines().count();
    let chars = prompt.chars().count();
    let head: String = prompt.chars().take(80).collect();
    format!("prompt: {lines} líneas, {chars} caracteres; empieza: {head:?}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_request_same_response() {
        let adapter = MockAdapter::new();
        let request = ModelRequest {
            system: "s".to_string(),
            prompt: "hola mundo".to_string(),
            max_output_tokens: 100,
            tools: Vec::new(),
            history: Vec::new(),
        };
        let token = CancellationToken::new();
        let a = adapter.complete(&request, &token).expect("ok");
        let b = adapter.complete(&request, &token).expect("ok");
        assert_eq!(a, b, "mock must be deterministic");
        assert_eq!(a.model, "mock-1");
    }

    #[test]
    fn honors_pre_cancelled_token() {
        let adapter = MockAdapter::new();
        let request = ModelRequest {
            system: "s".to_string(),
            prompt: "p".to_string(),
            max_output_tokens: 10,
            tools: Vec::new(),
            history: Vec::new(),
        };
        let token = CancellationToken::new();
        token.cancel();
        let error = adapter.complete(&request, &token).expect_err("cancelled");
        assert!(matches!(error, AgentError::Cancelled), "{error:?}");
    }

    #[test]
    fn honors_latency_cancellation_quickly() {
        let adapter = MockAdapter::with_latency(Duration::from_secs(10));
        let request = ModelRequest {
            system: "s".to_string(),
            prompt: "p".to_string(),
            max_output_tokens: 10,
            tools: Vec::new(),
            history: Vec::new(),
        };
        let token = CancellationToken::new();
        let killer = token.clone();
        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(30));
            killer.cancel();
        });
        let error = adapter.complete(&request, &token).expect_err("must cancel");
        handle.join().expect("killer thread");
        assert!(matches!(error, AgentError::Cancelled), "{error:?}");
    }

    #[test]
    fn streaming_deltas_reassemble_the_full_text() {
        let adapter = MockAdapter::new();
        let request = ModelRequest {
            system: "s".to_string(),
            prompt: "hola mundo".to_string(),
            max_output_tokens: 100,
            tools: Vec::new(),
            history: Vec::new(),
        };
        let token = CancellationToken::new();
        let mut deltas = String::new();
        let streamed = adapter
            .complete_streaming(&request, &token, &mut |chunk| deltas.push_str(chunk))
            .expect("stream ok");
        let direct = adapter.complete(&request, &token).expect("complete ok");
        assert_eq!(streamed, direct, "streaming must not change the result");
        assert_eq!(deltas, streamed.text, "deltas reassemble the full text");
        assert!(
            deltas.len() > 12,
            "mock streams multiple chunks: {deltas:?}"
        );
    }

    #[test]
    fn streaming_cancellation_between_chunks_is_immediate() {
        // Latency spread across chunks; the consumer cancels after the
        // first delta — exactly the mid-generation cancellation Spike C
        // requires.
        let adapter = MockAdapter::with_latency(Duration::from_millis(200));
        let request = ModelRequest {
            system: "s".to_string(),
            prompt: "p".to_string(),
            max_output_tokens: 10,
            tools: Vec::new(),
            history: Vec::new(),
        };
        let token = CancellationToken::new();
        let mut seen = 0;
        let error = adapter
            .complete_streaming(&request, &token, &mut |_| {
                seen += 1;
                token.cancel();
            })
            .expect_err("cancel between chunks");
        assert!(matches!(error, AgentError::Cancelled), "{error:?}");
        assert_eq!(seen, 1, "exactly one delta before cancellation");
    }
}
