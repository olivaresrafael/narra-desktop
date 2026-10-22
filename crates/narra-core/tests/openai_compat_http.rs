//! HTTP-level tests for the OpenAI-compatible adapter against a local mock
//! server (Spike C: free, repeatable runs — no network, no credentials, no
//! real provider). Each case exercises one wire behavior we depend on:
//! success parsing, actionable errors, timeouts, probes and secret hygiene.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

use narra_core::agent::{
    AgentError, ModelAdapter, ModelRequest, OpenAiCompatAdapter, OpenAiCompatConfig,
};
use narra_core::cancel::CancellationToken;

/// One canned HTTP exchange: what the mock server returns, and (optionally)
/// how long it waits before answering, to exercise timeouts.
struct MockReply {
    status: u16,
    body: String,
    delay: Duration,
}

/// A single-request HTTP server: binds an ephemeral port, answers exactly
/// one request, then exits. Returns the base URL (`http://127.0.0.1:<port>/v1`).
fn spawn_mock_server(reply: MockReply) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
    let base_url = format!("http://{}/v1", listener.local_addr().expect("addr"));

    let handle = thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            if !reply.delay.is_zero() {
                thread::sleep(reply.delay);
            }
            let _ = serve_once(&mut stream, reply.status, &reply.body);
        }
    });
    // Detach: the test does not need to join, but keep the join handle
    // referenced so the thread is not leaked silently in panicking paths.
    drop(handle);
    base_url
}

fn serve_once(stream: &mut TcpStream, status: u16, body: &str) -> std::io::Result<()> {
    read_request(stream)?;

    let reason = match status {
        200 => "OK",
        401 => "Unauthorized",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Status",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()
}

/// Read one HTTP request (head plus `Content-Length` body) off the stream.
fn read_request(stream: &mut TcpStream) -> std::io::Result<()> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Ok(());
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(position) = find_subsequence(&buffer, b"\r\n\r\n") {
            break position + 4;
        }
    };
    let head = String::from_utf8_lossy(&buffer[..head_end]).to_string();
    let content_length: usize = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().ok())?
        })
        .unwrap_or(0);
    let mut body_read = buffer.len() - head_end;
    while body_read < content_length {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        body_read += read;
    }
    Ok(())
}

/// An SSE streaming server: accepts one request and writes `events` (each a
/// complete SSE `data:` line without terminator) one by one, flushing and
/// pausing between them so the client observes genuine streaming. When
/// `tail_stall` is set, the server keeps the connection open and silent for
/// that long after the last event — modeling a provider that stalls
/// mid-generation. The stream ends by closing the connection.
fn spawn_sse_server(
    events: Vec<String>,
    inter_event_delay: Duration,
    tail_stall: Option<Duration>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind sse server");
    let base_url = format!("http://{}/v1", listener.local_addr().expect("addr"));

    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let _ = read_request(&mut stream);
            let head =
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n";
            if stream.write_all(head.as_bytes()).is_err() {
                return;
            }
            let _ = stream.flush();
            for event in events {
                if stream.write_all(format!("{event}\n\n").as_bytes()).is_err() {
                    return;
                }
                let _ = stream.flush();
                if !inter_event_delay.is_zero() {
                    thread::sleep(inter_event_delay);
                }
            }
            if let Some(stall) = tail_stall {
                thread::sleep(stall);
            }
            // Connection close terminates the (close-delimited) stream.
        }
    });
    base_url
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn request(system: &str, prompt: &str) -> ModelRequest {
    ModelRequest {
        system: system.to_string(),
        prompt: prompt.to_string(),
        max_output_tokens: 64,
        tools: Vec::new(),
        history: Vec::new(),
    }
}

#[test]
fn success_parses_content_model_and_usage() {
    let body = serde_json::json!({
        "model": "qwen2.5vl:7b",
        "choices": [{"message": {"role": "assistant", "content": "Hola desde el mock"}}],
        "usage": {"prompt_tokens": 21, "completion_tokens": 7}
    })
    .to_string();
    let base_url = spawn_mock_server(MockReply {
        status: 200,
        body,
        delay: Duration::ZERO,
    });

    let adapter = OpenAiCompatAdapter::new(OpenAiCompatConfig {
        base_url,
        api_key: None,
        model: "qwen2.5vl:7b".to_string(),
        timeout: Duration::from_secs(5),
        extra_headers: Vec::new(),
    });
    let token = CancellationToken::new();
    let response = adapter
        .complete(&request("s", "p"), &token)
        .expect("mock success");

    assert_eq!(response.text, "Hola desde el mock");
    assert_eq!(response.model, "qwen2.5vl:7b");
    assert_eq!(response.input_tokens, Some(21));
    assert_eq!(response.output_tokens, Some(7));
}

#[test]
fn unauthorized_surfaces_server_message_without_the_key() {
    // The server echoes a message that *contains* a token-like string; the
    // adapter must redact it before it reaches the error value.
    let body = serde_json::json!({
        "error": {"message": "invalid authorization: sk-abcdefghijklmnop"}
    })
    .to_string();
    let base_url = spawn_mock_server(MockReply {
        status: 401,
        body,
        delay: Duration::ZERO,
    });

    let adapter = OpenAiCompatAdapter::new(OpenAiCompatConfig {
        base_url,
        api_key: Some("sk-should-never-appear-anywhere".to_string()),
        model: "gpt-4o-mini".to_string(),
        timeout: Duration::from_secs(5),
        extra_headers: Vec::new(),
    });
    let token = CancellationToken::new();
    let error = adapter
        .complete(&request("s", "p"), &token)
        .expect_err("401 must fail");
    let rendered = error.to_string();

    assert!(rendered.contains("401"), "{rendered}");
    assert!(rendered.contains("invalid authorization"), "{rendered}");
    assert!(
        !rendered.contains("sk-should-never-appear-anywhere"),
        "configured key leaked: {rendered}"
    );
    assert!(
        !rendered.contains("sk-abcdefghijk"),
        "server echo leaked: {rendered}"
    );
}

#[test]
fn unknown_model_is_actionable() {
    let body = serde_json::json!({
        "error": {"message": "model 'nope' not found, try pulling it first"}
    })
    .to_string();
    let base_url = spawn_mock_server(MockReply {
        status: 404,
        body,
        delay: Duration::ZERO,
    });

    let adapter = OpenAiCompatAdapter::new(OpenAiCompatConfig {
        base_url,
        api_key: None,
        model: "nope".to_string(),
        timeout: Duration::from_secs(5),
        extra_headers: Vec::new(),
    });
    let token = CancellationToken::new();
    let error = adapter
        .complete(&request("s", "p"), &token)
        .expect_err("404 must fail");
    let rendered = error.to_string();
    assert!(rendered.contains("404"), "{rendered}");
    assert!(rendered.contains("not found"), "{rendered}");
}

#[test]
fn native_ollama_error_string_is_surfaced() {
    // Ollama's native error wire style is a bare string, not an object.
    let body = serde_json::json!({"error": "model requires more system memory"}).to_string();
    let base_url = spawn_mock_server(MockReply {
        status: 500,
        body,
        delay: Duration::ZERO,
    });

    let adapter = OpenAiCompatAdapter::new(OpenAiCompatConfig {
        base_url,
        api_key: None,
        model: "big".to_string(),
        timeout: Duration::from_secs(5),
        extra_headers: Vec::new(),
    });
    let token = CancellationToken::new();
    let error = adapter
        .complete(&request("s", "p"), &token)
        .expect_err("500 must fail");
    let rendered = error.to_string();
    assert!(rendered.contains("500"), "{rendered}");
    assert!(rendered.contains("system memory"), "{rendered}");
}

#[test]
fn non_json_body_is_reported_not_panicked() {
    let base_url = spawn_mock_server(MockReply {
        status: 200,
        body: "<html>oops</html>".to_string(),
        delay: Duration::ZERO,
    });

    let adapter = OpenAiCompatAdapter::new(OpenAiCompatConfig {
        base_url,
        api_key: None,
        model: "x".to_string(),
        timeout: Duration::from_secs(5),
        extra_headers: Vec::new(),
    });
    let token = CancellationToken::new();
    let error = adapter
        .complete(&request("s", "p"), &token)
        .expect_err("non-JSON must fail");
    assert!(error.to_string().contains("interpretable"), "{error}");
}

#[test]
fn timeout_bounds_a_stuck_endpoint() {
    // The server waits far longer than the adapter's timeout.
    let base_url = spawn_mock_server(MockReply {
        status: 200,
        body: "{}".to_string(),
        delay: Duration::from_secs(30),
    });

    let adapter = OpenAiCompatAdapter::new(OpenAiCompatConfig {
        base_url,
        api_key: None,
        model: "x".to_string(),
        timeout: Duration::from_millis(300),
        extra_headers: Vec::new(),
    });
    let token = CancellationToken::new();
    let started = std::time::Instant::now();
    let error = adapter
        .complete(&request("s", "p"), &token)
        .expect_err("timeout must fail");
    let elapsed = started.elapsed();

    assert!(error.to_string().contains("timeout"), "{error}");
    assert!(elapsed < Duration::from_secs(10), "bounded: {elapsed:?}");
}

#[test]
fn unreachable_endpoint_is_actionable() {
    // Port 1 on localhost: nothing listens there (reserved, unprivileged).
    let adapter = OpenAiCompatAdapter::new(OpenAiCompatConfig {
        base_url: "http://127.0.0.1:1/v1".to_string(),
        api_key: None,
        model: "x".to_string(),
        timeout: Duration::from_secs(5),
        extra_headers: Vec::new(),
    });
    let token = CancellationToken::new();
    let error = adapter
        .complete(&request("s", "p"), &token)
        .expect_err("must fail");
    let rendered = error.to_string();
    assert!(rendered.contains("127.0.0.1:1"), "{rendered}");
}

#[test]
fn probe_reports_compatible_auth_required_and_down() {
    // 200 on /models → fully compatible.
    let ok_url = spawn_mock_server(MockReply {
        status: 200,
        body: "{}".to_string(),
        delay: Duration::ZERO,
    });
    let probe = |base_url: &str| {
        OpenAiCompatAdapter::new(OpenAiCompatConfig {
            base_url: base_url.to_string(),
            api_key: None,
            model: "x".to_string(),
            timeout: Duration::from_secs(5),
            extra_headers: Vec::new(),
        })
        .probe()
    };
    assert!(probe(&ok_url).expect("200 probe"));

    // 401 on /models → endpoint speaks HTTP, only auth is missing.
    let auth_url = spawn_mock_server(MockReply {
        status: 401,
        body: "{}".to_string(),
        delay: Duration::ZERO,
    });
    assert!(probe(&auth_url).expect("401 probe"));

    // Connection refused → unreachable, reported as an error.
    let down = probe("http://127.0.0.1:1/v1").expect_err("down must error");
    assert!(down.to_string().contains("127.0.0.1:1"), "{down}");
}

#[test]
fn success_response_with_error_envelope_fails_cleanly() {
    // Some servers answer 200 but carry an error envelope; we must not
    // treat that as a model response.
    let body = serde_json::json!({
        "error": {"message": "quota exceeded for this org"}
    })
    .to_string();
    let base_url = spawn_mock_server(MockReply {
        status: 200,
        body,
        delay: Duration::ZERO,
    });
    let adapter = OpenAiCompatAdapter::new(OpenAiCompatConfig {
        base_url,
        api_key: None,
        model: "x".to_string(),
        timeout: Duration::from_secs(5),
        extra_headers: Vec::new(),
    });
    let token = CancellationToken::new();
    let error = adapter
        .complete(&request("s", "p"), &token)
        .expect_err("error envelope must fail");
    assert!(error.to_string().contains("quota exceeded"), "{error}");
}

// --- Streaming (Spike C) -------------------------------------------------

/// Build one SSE `data:` event carrying the given JSON chunk.
fn sse(chunk: serde_json::Value) -> String {
    format!("data: {chunk}")
}

fn stream_adapter(base_url: &str, timeout: Duration) -> OpenAiCompatAdapter {
    OpenAiCompatAdapter::new(OpenAiCompatConfig {
        base_url: base_url.to_string(),
        api_key: None,
        model: "x".to_string(),
        timeout,
        extra_headers: Vec::new(),
    })
}

#[test]
fn streaming_delivers_deltas_in_order_and_assembles_the_text() {
    let events = vec![
        sse(serde_json::json!({
            "model": "qwen", "choices": [{"index": 0, "delta": {"content": "Hola"}}]
        })),
        sse(serde_json::json!({
            "model": "qwen", "choices": [{"index": 0, "delta": {"content": " mundo"}}]
        })),
        sse(serde_json::json!({
            "model": "qwen",
            "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 9, "completion_tokens": 2}
        })),
        "data: [DONE]".to_string(),
    ];
    let base_url = spawn_sse_server(events, Duration::from_millis(5), None);

    let adapter = stream_adapter(&base_url, Duration::from_secs(5));
    let token = CancellationToken::new();
    let mut deltas: Vec<String> = Vec::new();
    let response = adapter
        .complete_streaming(&request("s", "p"), &token, &mut |chunk| {
            deltas.push(chunk.to_string())
        })
        .expect("stream completes");

    assert_eq!(deltas, vec!["Hola", " mundo"], "deltas in order");
    assert_eq!(response.text, "Hola mundo", "assembled text");
    assert_eq!(response.model, "qwen", "model from chunks");
    assert_eq!(response.input_tokens, Some(9), "usage from final chunk");
    assert_eq!(response.output_tokens, Some(2), "usage from final chunk");
}

#[test]
fn streaming_cancel_between_chunks_stops_before_the_stall() {
    // The server stalls 30 s after the first event; the consumer cancels as
    // soon as the first delta arrives. The adapter must return Cancelled
    // immediately — this is real mid-generation cancellation.
    let events = vec![
        sse(serde_json::json!({
            "model": "qwen", "choices": [{"index": 0, "delta": {"content": "primero"}}]
        })),
        sse(serde_json::json!({
            "model": "qwen", "choices": [{"index": 0, "delta": {"content": "segundo"}}]
        })),
    ];
    let base_url = spawn_sse_server(
        events,
        Duration::from_millis(5),
        Some(Duration::from_secs(30)),
    );

    let adapter = stream_adapter(&base_url, Duration::from_secs(60));
    let token = CancellationToken::new();
    let mut seen = 0;
    let started = std::time::Instant::now();
    let error = adapter
        .complete_streaming(&request("s", "p"), &token, &mut |_| {
            seen += 1;
            token.cancel();
        })
        .expect_err("cancel between chunks");
    let elapsed = started.elapsed();

    assert!(matches!(error, AgentError::Cancelled), "{error:?}");
    assert_eq!(seen, 1, "exactly one delta before cancellation");
    assert!(
        elapsed < Duration::from_secs(5),
        "must not wait for the stall: {elapsed:?}"
    );
}

#[test]
fn streaming_stall_is_bounded_by_the_timeout() {
    let events = vec![sse(serde_json::json!({
        "model": "qwen", "choices": [{"index": 0, "delta": {"content": "inicio"}}]
    }))];
    let base_url = spawn_sse_server(events, Duration::ZERO, Some(Duration::from_secs(30)));

    let adapter = stream_adapter(&base_url, Duration::from_millis(400));
    let token = CancellationToken::new();
    let started = std::time::Instant::now();
    let error = adapter
        .complete_streaming(&request("s", "p"), &token, &mut |_| {})
        .expect_err("stalled stream must error");
    let elapsed = started.elapsed();

    assert!(
        matches!(error, AgentError::Adapter(_)),
        "bounded stream error: {error:?}"
    );
    assert!(elapsed < Duration::from_secs(10), "bounded: {elapsed:?}");
}

#[test]
fn streaming_malformed_chunk_is_an_error_not_a_silent_drop() {
    let events = vec![
        sse(serde_json::json!({
            "model": "qwen", "choices": [{"index": 0, "delta": {"content": "ok"}}]
        })),
        "data: {esto no es json}".to_string(),
    ];
    let base_url = spawn_sse_server(events, Duration::from_millis(5), None);

    let adapter = stream_adapter(&base_url, Duration::from_secs(5));
    let token = CancellationToken::new();
    let error = adapter
        .complete_streaming(&request("s", "p"), &token, &mut |_| {})
        .expect_err("malformed chunk must fail");
    assert!(error.to_string().contains("JSON"), "{error}");
}

#[test]
fn streaming_eof_without_done_marker_is_accepted() {
    // Some servers close without `[DONE]`; content already delivered is kept.
    let events = vec![sse(serde_json::json!({
        "model": "qwen", "choices": [{"index": 0, "delta": {"content": "hola"}}]
    }))];
    let base_url = spawn_sse_server(events, Duration::from_millis(5), None);

    let adapter = stream_adapter(&base_url, Duration::from_secs(5));
    let token = CancellationToken::new();
    let response = adapter
        .complete_streaming(&request("s", "p"), &token, &mut |_| {})
        .expect("EOF ends the stream");
    assert_eq!(response.text, "hola");
}

#[test]
fn streaming_without_any_content_is_an_error() {
    let events = vec!["data: [DONE]".to_string()];
    let base_url = spawn_sse_server(events, Duration::ZERO, None);

    let adapter = stream_adapter(&base_url, Duration::from_secs(5));
    let token = CancellationToken::new();
    let error = adapter
        .complete_streaming(&request("s", "p"), &token, &mut |_| {})
        .expect_err("empty stream must fail");
    assert!(error.to_string().contains("sin contenido"), "{error}");
}

#[test]
fn streaming_error_status_is_actionable() {
    // Errors are JSON bodies, not streams — even with stream: true.
    let base_url = spawn_mock_server(MockReply {
        status: 404,
        body: serde_json::json!({"error": {"message": "model not found"}}).to_string(),
        delay: Duration::ZERO,
    });

    let adapter = stream_adapter(&base_url, Duration::from_secs(5));
    let token = CancellationToken::new();
    let error = adapter
        .complete_streaming(&request("s", "p"), &token, &mut |_| {})
        .expect_err("404 must fail");
    let rendered = error.to_string();
    assert!(rendered.contains("404"), "{rendered}");
    assert!(rendered.contains("not found"), "{rendered}");
}

/// End-to-end against a real Ollama server when present. Ignored by default;
/// run with `cargo test -p narra-core --test openai_compat_http -- --ignored`
/// on a machine with `ollama serve` running and a pulled model.
#[test]
#[ignore = "requires a running Ollama server (OLLAMA_URL, default localhost:11434)"]
fn real_ollama_completes_a_brief_exchange() {
    let base_url = std::env::var("OLLAMA_URL")
        .unwrap_or_else(|_| narra_core::agent::LOCAL_DEFAULT_BASE_URL.to_string());
    let model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "qwen2.5vl:7b".to_string());

    let adapter = OpenAiCompatAdapter::new(OpenAiCompatConfig {
        base_url,
        api_key: None,
        model,
        timeout: Duration::from_secs(120),
        extra_headers: Vec::new(),
    });
    let token = CancellationToken::new();
    let response = adapter
        .complete(
            &request(
                "Eres un asistente de guion.",
                "Di hola en una frase corta de menos de diez palabras.",
            ),
            &token,
        )
        .expect("real Ollama completion");

    assert!(!response.text.is_empty(), "model produced no text");
    eprintln!("ollama respondió ({}): {}", response.model, response.text);
}

/// Real streaming against Ollama, same rules as the completion test above.
#[test]
#[ignore = "requires a running Ollama server (OLLAMA_URL, default localhost:11434)"]
fn real_ollama_streams_a_brief_exchange() {
    let base_url = std::env::var("OLLAMA_URL")
        .unwrap_or_else(|_| narra_core::agent::LOCAL_DEFAULT_BASE_URL.to_string());
    let model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "qwen2.5vl:7b".to_string());

    let adapter = OpenAiCompatAdapter::new(OpenAiCompatConfig {
        base_url,
        api_key: None,
        model,
        timeout: Duration::from_secs(120),
        extra_headers: Vec::new(),
    });
    let token = CancellationToken::new();
    let mut deltas = 0usize;
    let response = adapter
        .complete_streaming(
            &request(
                "Eres un asistente de guion.",
                "Cuenta hasta cinco, un número por línea.",
            ),
            &token,
            &mut |_| deltas += 1,
        )
        .expect("real Ollama stream");

    assert!(!response.text.is_empty(), "model produced no text");
    assert!(deltas > 1, "expected multiple deltas, got {deltas}");
    eprintln!(
        "ollama streamió {} deltas ({}): {}",
        deltas, response.model, response.text
    );
}

// --- Tool-calling wire format (slice 3b) ---------------------------------

/// A single-request server that echoes the received request body back as a
/// `tool_calls` response, so we can assert what goes *out* on the wire and
/// what comes back, deterministically.
fn spawn_tool_server(received: std::sync::Arc<std::sync::Mutex<String>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind tool server");
    let base_url = format!("http://{}/v1", listener.local_addr().expect("addr"));
    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            // Read the request head and body, capturing the body.
            let mut buffer = Vec::new();
            let mut chunk = [0u8; 4096];
            let head_end = loop {
                let Ok(read) = stream.read(&mut chunk) else {
                    return;
                };
                if read == 0 {
                    return;
                }
                buffer.extend_from_slice(&chunk[..read]);
                if let Some(position) = find_subsequence(&buffer, b"\r\n\r\n") {
                    break position + 4;
                }
            };
            let head = String::from_utf8_lossy(&buffer[..head_end]).to_string();
            let content_length: usize = head
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse().ok())?
                })
                .unwrap_or(0);
            let mut body = buffer[head_end..].to_vec();
            while body.len() < content_length {
                let Ok(read) = stream.read(&mut chunk) else {
                    return;
                };
                if read == 0 {
                    break;
                }
                body.extend_from_slice(&chunk[..read]);
            }
            *received.lock().expect("lock") = String::from_utf8_lossy(&body).to_string();

            // Respond with a tool call whose `arguments` is a JSON *string*
            // (the wire format) — the adapter must parse it into a value.
            let response_body = serde_json::json!({
                "model": "tool-model",
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [{
                            "id": "call_abc",
                            "type": "function",
                            "function": {
                                "name": "read_scene",
                                "arguments": "{\"scene\": 1}"
                            }
                        }]
                    },
                    "finish_reason": "tool_calls"
                }],
                "usage": {"prompt_tokens": 12, "completion_tokens": 4}
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    base_url
}

#[test]
fn tool_declarations_and_history_go_out_in_openai_wire_format() {
    use narra_core::agent::{ChatMessage, ChatRole, ToolCall, ToolSpec};

    let received = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let base_url = spawn_tool_server(received.clone());
    let adapter = stream_adapter(&base_url, Duration::from_secs(5));

    let request = ModelRequest {
        system: "sistema".to_string(),
        prompt: "prompt".to_string(),
        max_output_tokens: 100,
        tools: vec![ToolSpec {
            name: "read_scene".to_string(),
            description: "lee una escena".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {"scene": {"type": "integer"}}
            }),
            permission: narra_core::agent::ToolPermission::ReadOnly,
        }],
        history: vec![
            ChatMessage {
                role: ChatRole::Assistant,
                content: String::new(),
                tool_calls: vec![ToolCall {
                    id: "call_prev".to_string(),
                    name: "read_scene".to_string(),
                    arguments: serde_json::json!({"scene": 2}),
                }],
                tool_call_id: None,
                tool_name: None,
            },
            ChatMessage {
                role: ChatRole::Tool,
                content: "INT. CALLE - DÍA".to_string(),
                tool_calls: Vec::new(),
                tool_call_id: Some("call_prev".to_string()),
                tool_name: Some("read_scene".to_string()),
            },
        ],
    };
    let token = CancellationToken::new();
    let response = adapter.complete(&request, &token).expect("tool round");

    // Outgoing wire: tools in function shape, arguments re-encoded as a
    // JSON string, tool results with role/tool_call_id.
    let sent: serde_json::Value =
        serde_json::from_str(&received.lock().expect("lock")).expect("valid JSON body");
    assert_eq!(sent["tools"][0]["type"], "function", "{}", sent["tools"]);
    assert_eq!(sent["tools"][0]["function"]["name"], "read_scene");
    assert_eq!(sent["tools"][0]["function"]["parameters"]["type"], "object");
    assert_eq!(
        sent["messages"][2]["tool_calls"][0]["function"]["arguments"],
        "{\"scene\":2}"
    );
    assert_eq!(sent["messages"][3]["role"], "tool");
    assert_eq!(sent["messages"][3]["tool_call_id"], "call_prev");
    assert_eq!(sent["stream"], false);

    // Incoming: arguments string parsed into a value.
    assert_eq!(response.tool_calls.len(), 1);
    assert_eq!(response.tool_calls[0].id, "call_abc");
    assert_eq!(response.tool_calls[0].name, "read_scene");
    assert_eq!(
        response.tool_calls[0].arguments,
        serde_json::json!({"scene": 1})
    );
    assert!(response.text.is_empty(), "tool round has no final text");
}

#[test]
fn empty_tools_are_omitted_from_the_wire() {
    let received = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let base_url = spawn_tool_server(received.clone());
    let adapter = stream_adapter(&base_url, Duration::from_secs(5));
    let token = CancellationToken::new();
    let _ = adapter.complete(&request("s", "p"), &token).expect("round");

    let sent: serde_json::Value =
        serde_json::from_str(&received.lock().expect("lock")).expect("valid JSON body");
    assert!(
        sent.get("tools").is_none(),
        "no tools key without declarations: {sent}"
    );
    assert!(
        sent["messages"].as_array().expect("messages").len() == 2,
        "single-shot messages unchanged: {}",
        sent["messages"]
    );
}

#[test]
fn malformed_tool_arguments_are_an_error_not_a_guess() {
    use narra_core::agent::{AgentError, OpenAiCompatAdapter, OpenAiCompatConfig};

    // A server that returns tool_calls with non-JSON arguments.
    let body = serde_json::json!({
        "model": "x",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_bad",
                    "type": "function",
                    "function": {"name": "read_scene", "arguments": "esto no es json"}
                }]
            }
        }]
    })
    .to_string();
    let base_url = spawn_mock_server(MockReply {
        status: 200,
        body,
        delay: Duration::ZERO,
    });
    let adapter = OpenAiCompatAdapter::new(OpenAiCompatConfig {
        base_url,
        api_key: None,
        model: "x".to_string(),
        timeout: Duration::from_secs(5),
        extra_headers: Vec::new(),
    });
    let token = CancellationToken::new();
    let error = adapter
        .complete(&request("s", "p"), &token)
        .expect_err("bad arguments must fail");
    assert!(matches!(error, AgentError::Adapter(_)), "{error:?}");
    assert!(error.to_string().contains("JSON"), "{error}");
}
