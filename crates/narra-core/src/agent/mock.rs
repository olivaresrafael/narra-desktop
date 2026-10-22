//! Deterministic mock provider for tests and the headless harness (Spike C:
//! "use a mock adapter for the free repeatable runs").
//!
//! Behavior is a pure function of the request: same input, same output, no
//! network, no credentials. Cancellation is honored between short sleeps so
//! tests can exercise the cancellation path without real latency.

use std::thread;
use std::time::Duration;

use super::AgentError;
use super::adapter::{ModelAdapter, ModelRequest, ModelResponse};
use crate::cancel::CancellationToken;

/// A provider that never talks to a network.
#[derive(Debug, Clone)]
pub struct MockAdapter {
    /// Simulated latency in 5 ms slices; 0 keeps tests instant.
    pub simulated_latency: Duration,
}

impl MockAdapter {
    pub fn new() -> Self {
        Self {
            simulated_latency: Duration::ZERO,
        }
    }

    pub fn with_latency(latency: Duration) -> Self {
        Self {
            simulated_latency: latency,
        }
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

        // Deterministic output derived only from the request.
        let text = format!(
            "MOCK BRIEF ({} entrada / {} salida tokens declarados)\n{}",
            request.max_output_tokens,
            request.max_output_tokens / 2,
            summarize(&request.prompt)
        );
        Ok(ModelResponse {
            text,
            model: "mock-1".to_string(),
            input_tokens: Some(request.prompt.len() as u32 / 4),
            output_tokens: Some(request.max_output_tokens / 2),
        })
    }
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
}
