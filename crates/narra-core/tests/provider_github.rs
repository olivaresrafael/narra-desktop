//! Contract tests for the GitHub provider adapter (Milestone 5, Spike E
//! matrix). Every case runs against a local mock server — no network, no
//! credentials, no fixtures containing secrets (M5 acceptance). They pin
//! the behaviors the product depends on: provider-confirmed success only,
//! typed errors with the provider's own messages, bounded `Link`
//! pagination, rate-limit detection, and token hygiene.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use narra_core::provider::{
    Capability, GitHubClient, GitHubConfig, MergeMethod, MergeOptions, NewPullRequest,
    NewRepository, ProviderError, PullRequestState, UpdatePullRequest,
};
use serde_json::json;

/// One canned reply. `{BASE}` inside headers/bodies expands to the mock
/// server's own base URL, so `Link` pagination headers can point at it.
struct Reply {
    status: u16,
    headers: Vec<(&'static str, String)>,
    body: String,
}

impl Reply {
    fn json(status: u16, body: serde_json::Value) -> Self {
        Self {
            status,
            headers: vec![("content-type", "application/json".to_string())],
            body: body.to_string(),
        }
    }

    fn with_header(mut self, name: &'static str, value: String) -> Self {
        self.headers.push((name, value));
        self
    }
}

/// What the mock server observed: method, request target, headers, body.
#[derive(Debug, Clone)]
struct Recorded {
    method: String,
    target: String,
    headers: Vec<(String, String)>,
    body: String,
}

/// A single-connection-per-reply HTTP server on an ephemeral port. Replies
/// are consumed in order; every request is recorded for assertions.
fn spawn_server(replies: Vec<Reply>) -> (String, Arc<Mutex<Vec<Recorded>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
    let base_url = format!("http://{}", listener.local_addr().expect("addr"));
    let recorded: Arc<Mutex<Vec<Recorded>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&recorded);
    let server_base = base_url.clone();

    thread::spawn(move || {
        for reply in replies {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let Ok(recorded_request) = read_request(&mut stream) else {
                return;
            };
            sink.lock().expect("recorded lock").push(recorded_request);
            let status = reply.status;
            let reason = match status {
                200 => "OK",
                201 => "Created",
                401 => "Unauthorized",
                403 => "Forbidden",
                404 => "Not Found",
                409 => "Conflict",
                422 => "Unprocessable Entity",
                429 => "Too Many Requests",
                500 => "Internal Server Error",
                _ => "Status",
            };
            let body = reply.body.replace("{BASE}", &server_base);
            let mut head_out = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n",
                body.len()
            );
            for (name, value) in &reply.headers {
                let value = value.replace("{BASE}", &server_base);
                head_out.push_str(&format!("{name}: {value}\r\n"));
            }
            head_out.push_str("\r\n");
            let _ = stream.write_all(head_out.as_bytes());
            let _ = stream.write_all(body.as_bytes());
            let _ = stream.flush();
        }
    });

    (base_url, recorded)
}

/// Read one request (head plus `Content-Length` body) into a `Recorded`.
fn read_request(stream: &mut TcpStream) -> std::io::Result<Recorded> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break buffer.len();
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(position) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break position + 4;
        }
    };
    let head = String::from_utf8_lossy(&buffer[..head_end]).to_string();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or_default().to_string();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default().to_string();
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            Some((name.trim().to_ascii_lowercase(), value.trim().to_string()))
        })
        .collect();
    let content_length: usize = headers
        .iter()
        .find(|(name, _)| name == "content-length")
        .and_then(|(_, value)| value.parse().ok())
        .unwrap_or(0);
    let mut body = String::from_utf8_lossy(&buffer[head_end..]).to_string();
    let mut body_read = body.len();
    while body_read < content_length {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        body.push_str(&String::from_utf8_lossy(&chunk[..read]));
        body_read += read;
    }
    Ok(Recorded {
        method,
        target,
        headers,
        body,
    })
}

fn client_for(base_url: &str) -> GitHubClient {
    GitHubClient::new(GitHubConfig::github_com("ghp_testtokennotreal").with_base_url(base_url))
}

fn repo_json() -> serde_json::Value {
    json!({
        "full_name": "olivaresrafael/narra-desktop",
        "name": "narra-desktop",
        "default_branch": "main",
        "clone_url": "https://github.com/olivaresrafael/narra-desktop.git",
        "html_url": "https://github.com/olivaresrafael/narra-desktop",
        "private": false,
        "permissions": { "admin": true, "push": true, "pull": true }
    })
}

fn pr_json(number: u64) -> serde_json::Value {
    json!({
        "number": number,
        "state": "open",
        "merged": false,
        "title": "M5 slice",
        "html_url": format!("https://github.com/o/r/pull/{number}"),
        "head": { "ref": "feature/provider" },
        "base": { "ref": "main" }
    })
}

// ---------------------------------------------------------------------------
// Success paths (provider-confirmed 2xx + parseable body)
// ---------------------------------------------------------------------------

#[test]
fn authenticated_user_sends_required_headers() {
    let (base, recorded) =
        spawn_server(vec![Reply::json(200, json!({ "login": "olivaresrafael" }))]);
    let user = client_for(&base)
        .authenticated_user()
        .expect("GET /user should succeed");
    assert_eq!(user.login, "olivaresrafael");

    let log = recorded.lock().expect("lock").clone();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].method, "GET");
    assert_eq!(log[0].target, "/user");
    let header = |name: &str| {
        log[0]
            .headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
            .unwrap_or("<missing>")
            .to_string()
    };
    assert_eq!(header("authorization"), "Bearer ghp_testtokennotreal");
    assert_eq!(header("accept"), "application/vnd.github+json");
    assert_eq!(header("x-github-api-version"), "2022-11-28");
}

#[test]
fn get_repository_parses_identity_and_permissions() {
    let (base, _recorded) = spawn_server(vec![Reply::json(200, repo_json())]);
    let repo = client_for(&base)
        .get_repository("olivaresrafael", "narra-desktop")
        .expect("connect should succeed");
    assert_eq!(repo.full_name, "olivaresrafael/narra-desktop");
    assert_eq!(repo.default_branch, "main");
    assert_eq!(
        repo.clone_url,
        "https://github.com/olivaresrafael/narra-desktop.git"
    );
    let permissions = repo.permissions.expect("permissions reported");
    assert!(permissions.admin && permissions.push && permissions.pull);
}

#[test]
fn create_repository_sends_body_and_needs_201() {
    let (base, recorded) = spawn_server(vec![Reply::json(201, repo_json())]);
    let input = NewRepository::new("narra-demo").private().auto_init();
    let repo = client_for(&base)
        .create_repository(&input)
        .expect("create should succeed on 201");
    assert_eq!(repo.name, "narra-desktop");

    let log = recorded.lock().expect("lock").clone();
    assert_eq!(log[0].method, "POST");
    assert_eq!(log[0].target, "/user/repos");
    let body: serde_json::Value = serde_json::from_str(&log[0].body).expect("json body");
    assert_eq!(body["name"], "narra-demo");
    assert_eq!(body["private"], true);
    assert_eq!(body["auto_init"], true);
}

#[test]
fn list_branches_follows_link_pagination() {
    let (base, recorded) = spawn_server(vec![
        Reply::json(
            200,
            json!([
                { "name": "main", "commit": { "sha": "aaaa" }, "protected": true },
                { "name": "develop", "commit": { "sha": "bbbb" }, "protected": false }
            ]),
        )
        .with_header(
            "link",
            "<{BASE}/repos/o/r/branches?per_page=100&page=2>; rel=\"next\", <{BASE}/repos/o/r/branches?per_page=100&page=2>; rel=\"last\""
                .to_string(),
        ),
        Reply::json(
            200,
            json!([
                { "name": "feature/x", "commit": { "sha": "cccc" }, "protected": false }
            ]),
        ),
    ]);
    let branches = client_for(&base)
        .list_branches("o", "r", 5)
        .expect("pagination should succeed");
    assert_eq!(branches.len(), 3);
    assert_eq!(branches[0].name, "main");
    assert!(branches[0].protected);
    assert_eq!(branches[2].sha, "cccc");

    let log = recorded.lock().expect("lock").clone();
    assert_eq!(log.len(), 2, "one request per page");
    assert_eq!(log[0].target, "/repos/o/r/branches?per_page=100");
    assert_eq!(log[1].target, "/repos/o/r/branches?per_page=100&page=2");
}

#[test]
fn list_branches_stops_at_max_pages_even_with_more_links() {
    let page = || {
        Reply::json(
            200,
            json!([{ "name": "x", "commit": { "sha": "dddd" }, "protected": false }]),
        )
        .with_header(
            "link",
            "<{BASE}/repos/o/r/branches?per_page=100&page=2>; rel=\"next\"".to_string(),
        )
    };
    let (base, recorded) = spawn_server(vec![page(), page()]);
    let branches = client_for(&base)
        .list_branches("o", "r", 2)
        .expect("bounded list should succeed");
    assert_eq!(branches.len(), 2);
    assert_eq!(
        recorded.lock().expect("lock").len(),
        2,
        "max_pages caps the requests"
    );
}

#[test]
fn list_pull_requests_sends_state_filter() {
    let (base, recorded) = spawn_server(vec![Reply::json(200, json!([pr_json(7)]))]);
    let prs = client_for(&base)
        .list_pull_requests("o", "r", "open", 1)
        .expect("list PRs");
    assert_eq!(prs.len(), 1);
    assert_eq!(prs[0].number, 7);
    assert_eq!(prs[0].state, PullRequestState::Open);
    assert_eq!(prs[0].head_ref, "feature/provider");

    let log = recorded.lock().expect("lock").clone();
    assert_eq!(log[0].target, "/repos/o/r/pulls?per_page=100&state=open");
}

#[test]
fn create_pull_request_success() {
    let (base, recorded) = spawn_server(vec![Reply::json(201, pr_json(1))]);
    let input = NewPullRequest {
        title: "M5 slice".to_string(),
        head: "feature/provider".to_string(),
        base: "main".to_string(),
        body: Some("Contract-tested.".to_string()),
    };
    let pr = client_for(&base)
        .create_pull_request("o", "r", &input)
        .expect("create PR");
    assert_eq!(pr.number, 1);
    let log = recorded.lock().expect("lock").clone();
    let body: serde_json::Value = serde_json::from_str(&log[0].body).expect("json");
    assert_eq!(body["head"], "feature/provider");
    assert_eq!(body["base"], "main");
    assert_eq!(body["body"], "Contract-tested.");
}

#[test]
fn update_pull_request_patches_only_given_fields() {
    let (base, recorded) = spawn_server(vec![Reply::json(200, pr_json(3))]);
    let input = UpdatePullRequest {
        title: Some("Renamed".to_string()),
        ..Default::default()
    };
    client_for(&base)
        .update_pull_request("o", "r", 3, &input)
        .expect("update PR");
    let log = recorded.lock().expect("lock").clone();
    assert_eq!(log[0].method, "PATCH");
    assert_eq!(log[0].target, "/repos/o/r/pulls/3");
    let body: serde_json::Value = serde_json::from_str(&log[0].body).expect("json");
    assert_eq!(body["title"], "Renamed");
    assert!(body.get("state").is_none(), "untouched fields are not sent");
}

#[test]
fn update_pull_request_rejects_empty_input_locally() {
    let error = client_for("http://127.0.0.1:1")
        .update_pull_request("o", "r", 1, &UpdatePullRequest::default())
        .expect_err("empty update must fail before any request");
    assert!(
        matches!(error, ProviderError::Validation { .. }),
        "{error:?}"
    );
}

#[test]
fn merge_pull_request_success_is_provider_confirmed() {
    let (base, recorded) = spawn_server(vec![Reply::json(
        200,
        json!({ "merged": true, "sha": "ef01", "message": "Pull Request successfully merged" }),
    )]);
    let options = MergeOptions {
        commit_title: None,
        commit_message: None,
        merge_method: Some(MergeMethod::Squash),
    };
    let result = client_for(&base)
        .merge_pull_request("o", "r", 9, &options)
        .expect("merge");
    assert!(result.merged);
    assert_eq!(result.sha.as_deref(), Some("ef01"));
    let log = recorded.lock().expect("lock").clone();
    assert_eq!(log[0].method, "PUT");
    assert_eq!(log[0].target, "/repos/o/r/pulls/9/merge");
    let body: serde_json::Value = serde_json::from_str(&log[0].body).expect("json");
    assert_eq!(body["merge_method"], "squash");
}

#[test]
fn merge_200_with_merged_false_is_not_success() {
    // Real GitHub race: head branch modified after approval. The product
    // must surface "not merged", never claim success (M5 rule).
    let (base, _recorded) = spawn_server(vec![Reply::json(
        200,
        json!({ "merged": false, "message": "Head branch was modified after the pull request was approved." }),
    )]);
    let result = client_for(&base)
        .merge_pull_request("o", "r", 9, &MergeOptions::default())
        .expect("200 is a valid answer");
    assert!(!result.merged);
    assert!(result.message.contains("Head branch was modified"));
}

#[test]
fn create_issue_comment_uses_issues_api() {
    let (base, recorded) = spawn_server(vec![Reply::json(
        201,
        json!({ "id": 42, "html_url": "https://github.com/o/r/pull/1#issuecomment-42" }),
    )]);
    let comment = client_for(&base)
        .create_issue_comment("o", "r", 1, "LGTM after review.")
        .expect("comment");
    assert_eq!(comment.id, 42);
    let log = recorded.lock().expect("lock").clone();
    assert_eq!(log[0].target, "/repos/o/r/issues/1/comments");
}

#[test]
fn create_issue_comment_rejects_empty_text_locally() {
    let error = client_for("http://127.0.0.1:1")
        .create_issue_comment("o", "r", 1, "   ")
        .expect_err("empty comment must fail before any request");
    assert!(
        matches!(error, ProviderError::Validation { .. }),
        "{error:?}"
    );
}

#[test]
fn list_repositories_follows_pages_and_stops() {
    let (base, _recorded) = spawn_server(vec![Reply::json(200, json!([repo_json()]))]);
    let repos = client_for(&base).list_repositories(3).expect("list repos");
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].full_name, "olivaresrafael/narra-desktop");
}

// ---------------------------------------------------------------------------
// Failure paths: typed errors, provider's own messages, no false success
// ---------------------------------------------------------------------------

#[test]
fn unauthorized_401_keeps_provider_message() {
    let (base, _r) = spawn_server(vec![Reply::json(
        401,
        json!({ "message": "Bad credentials", "documentation_url": "https://docs.github.com/rest", "status": "401" }),
    )]);
    let error = client_for(&base)
        .authenticated_user()
        .expect_err("401 must fail");
    match error {
        ProviderError::Unauthorized { detail } => assert_eq!(detail, "Bad credentials"),
        other => panic!("expected Unauthorized, got {other:?}"),
    }
}

#[test]
fn forbidden_403_is_permission_not_rate_limit() {
    let (base, _r) = spawn_server(vec![Reply::json(
        403,
        json!({ "message": "Resource not accessible by personal access token", "status": "403" }),
    )]);
    let error = client_for(&base)
        .get_repository("o", "r")
        .expect_err("403 must fail");
    assert!(
        matches!(error, ProviderError::Forbidden { .. }),
        "{error:?}"
    );
    assert!(error.to_string().contains("personal access token"));
}

#[test]
fn not_found_404_is_typed_with_caveat() {
    // Spike E evidence: some admin-only endpoints answer 404 for
    // insufficient permission. The variant exists so callers cannot treat
    // 404 as proof of non-existence.
    let (base, _r) = spawn_server(vec![Reply::json(
        404,
        json!({ "message": "Not Found", "status": "404" }),
    )]);
    let error = client_for(&base)
        .get_repository("o", "missing")
        .expect_err("404 must fail");
    assert!(matches!(error, ProviderError::NotFound { .. }), "{error:?}");
}

#[test]
fn validation_422_surfaces_server_detail() {
    let (base, _r) = spawn_server(vec![Reply::json(
        422,
        json!({
            "message": "Validation Failed",
            "errors": [{ "resource": "PullRequest", "field": "title", "code": "missing_field" }],
            "status": "422"
        }),
    )]);
    let input = NewPullRequest {
        title: "x".to_string(),
        head: "a".to_string(),
        base: "b".to_string(),
        body: None,
    };
    let error = client_for(&base)
        .create_pull_request("o", "r", &input)
        .expect_err("422 must fail");
    match error {
        ProviderError::Validation { detail } => assert_eq!(detail, "Validation Failed"),
        other => panic!("expected Validation, got {other:?}"),
    }
}

#[test]
fn rate_limited_429_reports_retry_after() {
    let (base, _r) = spawn_server(vec![
        Reply::json(
            429,
            json!({ "message": "API rate limit exceeded for user", "status": "429" }),
        )
        .with_header("retry-after", "30".to_string()),
    ]);
    let error = client_for(&base)
        .list_repositories(1)
        .expect_err("429 must fail");
    match error {
        ProviderError::RateLimited {
            retry_after,
            detail,
        } => {
            assert_eq!(retry_after, Some(Duration::from_secs(30)));
            assert!(detail.contains("rate limit"), "{detail}");
        }
        other => panic!("expected RateLimited, got {other:?}"),
    }
}

#[test]
fn server_500_is_not_success() {
    let (base, _r) = spawn_server(vec![Reply::json(
        500,
        json!({ "message": "Server Error", "status": "500" }),
    )]);
    let error = client_for(&base)
        .get_repository("o", "r")
        .expect_err("500 must fail");
    match error {
        ProviderError::Server { status, .. } => assert_eq!(status, 500),
        other => panic!("expected Server, got {other:?}"),
    }
}

#[test]
fn invalid_pr_state_filter_is_rejected_locally() {
    let error = client_for("http://127.0.0.1:1")
        .list_pull_requests("o", "r", "banana", 1)
        .expect_err("invalid state must fail before any request");
    assert!(
        matches!(error, ProviderError::Validation { .. }),
        "{error:?}"
    );
}

#[test]
fn missing_required_field_is_decode_error() {
    let (base, _r) = spawn_server(vec![Reply::json(
        200,
        json!({ "full_name": "o/r", "name": "r", "default_branch": "main" }),
    )]);
    let error = client_for(&base)
        .get_repository("o", "r")
        .expect_err("missing clone_url must fail");
    assert!(matches!(error, ProviderError::Decode { .. }), "{error:?}");
    assert!(error.to_string().contains("clone_url"), "{error}");
}

#[test]
fn transport_failure_is_actionable_and_bounded() {
    // Nothing listens on this port: the request fails at the transport
    // layer, with the configured timeout as the upper bound.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let dead_port = listener.local_addr().expect("addr");
    drop(listener);
    let client = GitHubClient::new(
        GitHubConfig::github_com("ghp_testtokennotreal")
            .with_base_url(format!("http://{dead_port}"))
            .with_timeout(Duration::from_secs(5)),
    );
    let error = client
        .authenticated_user()
        .expect_err("dead port must fail");
    assert!(
        matches!(error, ProviderError::Transport { .. }),
        "{error:?}"
    );
}

#[test]
fn token_never_appears_in_error_output() {
    // Defense in depth: even if a server echoed the token in its error
    // body, the redactor masks it before it reaches Display/logs.
    let token = "ghp_echoedsecrettoken";
    let (base, _r) = spawn_server(vec![Reply::json(
        401,
        json!({ "message": format!("Bad credentials for {token}"), "status": "401" }),
    )]);
    let client = GitHubClient::new(GitHubConfig::github_com(token).with_base_url(base));
    let error = client.authenticated_user().expect_err("401 must fail");
    let printed = format!("{error}");
    assert!(!printed.contains(token), "token leaked: {printed}");
    assert!(printed.contains("***"), "{printed}");
}

// ---------------------------------------------------------------------------
// Capability contract
// ---------------------------------------------------------------------------

#[test]
fn capability_matrix_excludes_unimplemented_operations() {
    // Honesty contract: the UI lists exactly `GITHUB_CAPABILITIES`; review
    // submission is in the Spike E matrix but not yet in the adapter.
    let caps = GitHubClient::capabilities();
    assert!(caps.contains(&Capability::CreatePullRequest));
    assert!(caps.contains(&Capability::MergePullRequest));
    assert!(!caps.contains(&Capability::ReviewPullRequest));
}

// ---------------------------------------------------------------------------
// Live verification (ignored by default): read-only calls against the real
// GitHub API with `GITHUB_TOKEN` set — no writes, ever, without explicit
// maintainer approval (AGENTS.md).
// ---------------------------------------------------------------------------

#[test]
#[ignore = "requires GITHUB_TOKEN (read-only live verification)"]
fn real_github_verifies_auth_and_reads_own_repository() {
    let Ok(token) = std::env::var("GITHUB_TOKEN") else {
        eprintln!("GITHUB_TOKEN not set; skipping live verification");
        return;
    };
    let client = GitHubClient::new(GitHubConfig::github_com(token));
    let user = client
        .authenticated_user()
        .expect("GET /user must authenticate");
    let repo = client
        .get_repository(&user.login, "narra-desktop")
        .expect("GET /repos/{login}/narra-desktop must succeed");
    assert_eq!(repo.full_name, format!("{}/narra-desktop", user.login));
    assert!(!repo.default_branch.is_empty(), "default branch discovered");
    assert!(!repo.clone_url.is_empty(), "clone URL discovered");
    let branches = client
        .list_branches(&user.login, "narra-desktop", 1)
        .expect("GET branches must succeed");
    assert!(!branches.is_empty(), "at least one branch");
    println!(
        "live OK: {} — {} ({}), {} branches, admin={:?}",
        user.login,
        repo.full_name,
        repo.default_branch,
        branches.len(),
        repo.permissions.as_ref().map(|p| p.admin)
    );
}
