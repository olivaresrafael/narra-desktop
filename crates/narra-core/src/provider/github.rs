//! GitHub REST client — the first concrete provider (D-018, Spike E).
//!
//! Wire behavior follows the Spike E capability matrix: one JSON error
//! envelope (`message`/`status`), `Link` pagination, `Retry-After` on rate
//! limits, and `404` used to mask some permission failures. Success means
//! "the provider said 2xx and the body parsed" — nothing else (M5 rule:
//! no false success states).

use std::fmt;
use std::time::Duration;

use serde_json::{Value, json};

use super::{Capability, ProviderError};
use crate::git::redact_secrets;

/// GitHub.com REST root.
pub const GITHUB_API_BASE_URL: &str = "https://api.github.com";

/// `X-GitHub-Api-Version` we pin. `2022-11-28` is the version verified
/// live during Spike E; newer versions are accepted by GitHub but not yet
/// evidence-checked here.
pub const GITHUB_API_VERSION: &str = "2022-11-28";

/// Capabilities the GitHub adapter actually exposes today. This list is the
/// UI contract: an operation missing here must be presented as unsupported
/// with guidance, never attempted. Review submission is documented in the
/// Spike E matrix but lands with a later M5 slice, so it is deliberately
/// absent below.
pub const GITHUB_CAPABILITIES: &[Capability] = &[
    Capability::ListRepositories,
    Capability::CreateRepository,
    Capability::ListBranches,
    Capability::ListPullRequests,
    Capability::CreatePullRequest,
    Capability::ViewPullRequest,
    Capability::UpdatePullRequest,
    Capability::CommentOnPullRequest,
    Capability::MergePullRequest,
];

/// Connection settings for one GitHub-compatible host. The token is private
/// and excluded from `Debug` output by construction (D-022: it comes from
/// OS secret storage or `GITHUB_TOKEN`, never from project files).
#[derive(Clone, PartialEq, Eq)]
pub struct GitHubConfig {
    /// API root, without a trailing slash. GitHub.com by default; a GHES
    /// host can be configured later without touching call sites.
    pub base_url: String,
    /// Bearer token (fine-grained PAT, D-022). Redacted everywhere.
    token: String,
    /// Per-request timeout covering the whole exchange.
    pub timeout: Duration,
}

impl GitHubConfig {
    /// GitHub.com with a token and a 30-second per-request timeout.
    pub fn github_com(token: impl Into<String>) -> Self {
        Self {
            base_url: GITHUB_API_BASE_URL.to_string(),
            token: token.into(),
            timeout: Duration::from_secs(30),
        }
    }

    /// Custom root (tests point this at a local mock server).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Override the per-request timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl fmt::Debug for GitHubConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GitHubConfig")
            .field("base_url", &self.base_url)
            .field("token", &"<redacted>")
            .field("timeout", &self.timeout)
            .finish()
    }
}

/// The authenticated user (`GET /user`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub login: String,
}

/// Effective permissions the token has on a repository, when the API
/// reports them (`GET /repos/{owner}/{repo}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Permissions {
    pub admin: bool,
    pub push: bool,
    pub pull: bool,
}

/// A repository as the product needs it: identity, clone target and the
/// default branch (Spike E: "connect" = this endpoint succeeds).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repository {
    pub full_name: String,
    pub name: String,
    pub default_branch: String,
    pub clone_url: String,
    pub html_url: String,
    pub private: bool,
    pub permissions: Option<Permissions>,
}

/// A branch with the commit it points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub name: String,
    pub sha: String,
    pub protected: bool,
}

/// Pull request state as GitHub reports it (`merged` PRs are `Closed`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullRequestState {
    Open,
    Closed,
}

impl PullRequestState {
    fn as_str(self) -> &'static str {
        match self {
            PullRequestState::Open => "open",
            PullRequestState::Closed => "closed",
        }
    }

    fn parse(raw: &str) -> Result<Self, ProviderError> {
        match raw {
            "open" => Ok(PullRequestState::Open),
            "closed" => Ok(PullRequestState::Closed),
            other => Err(decode(format!("estado de PR desconocido: {other}"))),
        }
    }
}

/// A pull request with the fields the review UI needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequest {
    pub number: u64,
    pub state: PullRequestState,
    pub merged: bool,
    pub title: String,
    pub html_url: String,
    pub head_ref: String,
    pub base_ref: String,
}

/// Input for `POST /user/repos`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRepository {
    pub name: String,
    pub private: bool,
    pub description: Option<String>,
    /// Create an initial commit with an empty README.
    pub auto_init: bool,
}

impl NewRepository {
    /// A public, empty repository named `name` (GitHub defaults).
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            private: false,
            description: None,
            auto_init: false,
        }
    }

    /// Mark the repository private.
    pub fn private(mut self) -> Self {
        self.private = true;
        self
    }

    /// Initialize with a README commit.
    pub fn auto_init(mut self) -> Self {
        self.auto_init = true;
        self
    }
}

/// Input for `POST /repos/{owner}/{repo}/pulls`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPullRequest {
    pub title: String,
    pub head: String,
    pub base: String,
    pub body: Option<String>,
}

/// Input for `PATCH /repos/{owner}/{repo}/pulls/{number}`. At least one
/// field must be set; the client rejects an empty update locally.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdatePullRequest {
    pub title: Option<String>,
    pub body: Option<String>,
    pub state: Option<PullRequestState>,
}

/// Merge strategy per GitHub's `merge_method` parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeMethod {
    Merge,
    Squash,
    Rebase,
}

impl MergeMethod {
    fn as_str(self) -> &'static str {
        match self {
            MergeMethod::Merge => "merge",
            MergeMethod::Squash => "squash",
            MergeMethod::Rebase => "rebase",
        }
    }
}

/// Options for `PUT /repos/{owner}/{repo}/pulls/{number}/merge`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeOptions {
    pub commit_title: Option<String>,
    pub commit_message: Option<String>,
    pub merge_method: Option<MergeMethod>,
}

/// The provider's own merge verdict. `merged: false` with a 200 is a real
/// GitHub answer ("head branch was modified" races) — the product must
/// surface it, never reinterpret it as success.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeResult {
    pub merged: bool,
    pub sha: Option<String>,
    pub message: String,
}

/// A comment on an issue/pull-request conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub id: u64,
    pub html_url: String,
}

/// HTTP method used by this client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Method {
    Get,
    Post,
    Patch,
    Put,
}

/// One raw HTTP exchange, kept small: status, lowercased headers, body.
struct HttpReply {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl HttpReply {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

/// GitHub REST client. Synchronous by design (D-021-era core is sync;
/// async comes with the UI slice that needs it).
#[derive(Debug, Clone)]
pub struct GitHubClient {
    config: GitHubConfig,
    agent: ureq::Agent,
}

impl GitHubClient {
    pub fn new(config: GitHubConfig) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(config.timeout))
            // Non-2xx is data: we read the provider's JSON error envelope to
            // build an actionable, typed error (same stance as the model
            // adapter in `agent::openai_compat`).
            .http_status_as_error(false)
            .build()
            .into();
        Self { config, agent }
    }

    /// The capability contract for this provider (for the UI matrix).
    pub fn capabilities() -> &'static [Capability] {
        GITHUB_CAPABILITIES
    }

    /// `GET /user` — verifies auth and identifies the account.
    pub fn authenticated_user(&self) -> Result<User, ProviderError> {
        let reply = self.send(Method::Get, "/user", &[], None)?;
        let payload = expect_success(reply)?;
        Ok(User {
            login: req_str(&payload, "/login")?,
        })
    }

    /// `GET /user/repos` — paginated; `max_pages` bounds the work (each
    /// page holds up to 100 repositories).
    pub fn list_repositories(&self, max_pages: usize) -> Result<Vec<Repository>, ProviderError> {
        self.list_paginated(
            "/user/repos",
            &[("per_page", "100")],
            max_pages,
            parse_repository,
        )
    }

    /// `GET /repos/{owner}/{repo}` — the "connect this repository" check:
    /// returns clone URL, default branch and effective permissions.
    pub fn get_repository(&self, owner: &str, repo: &str) -> Result<Repository, ProviderError> {
        let reply = self.send(Method::Get, &format!("/repos/{owner}/{repo}"), &[], None)?;
        let payload = expect_success(reply)?;
        parse_repository(&payload)
    }

    /// `POST /user/repos` — creates a repository owned by the
    /// authenticated user. Confirmed only by the provider's 201 + body.
    pub fn create_repository(&self, input: &NewRepository) -> Result<Repository, ProviderError> {
        let body = match &input.description {
            Some(description) => json!({
                "name": input.name,
                "private": input.private,
                "auto_init": input.auto_init,
                "description": description,
            }),
            None => json!({
                "name": input.name,
                "private": input.private,
                "auto_init": input.auto_init,
            }),
        };
        let reply = self.send(Method::Post, "/user/repos", &[], Some(&body))?;
        let payload = expect_success(reply)?;
        parse_repository(&payload)
    }

    /// `GET /repos/{owner}/{repo}/branches` — paginated.
    pub fn list_branches(
        &self,
        owner: &str,
        repo: &str,
        max_pages: usize,
    ) -> Result<Vec<Branch>, ProviderError> {
        self.list_paginated(
            &format!("/repos/{owner}/{repo}/branches"),
            &[("per_page", "100")],
            max_pages,
            |item| {
                Ok(Branch {
                    name: req_str(item, "/name")?,
                    sha: req_str(item, "/commit/sha")?,
                    protected: opt_bool(item, "/protected").unwrap_or(false),
                })
            },
        )
    }

    /// `GET /repos/{owner}/{repo}/pulls?state=…` — paginated. `state` is
    /// `open`, `closed` or `all`.
    pub fn list_pull_requests(
        &self,
        owner: &str,
        repo: &str,
        state: &str,
        max_pages: usize,
    ) -> Result<Vec<PullRequest>, ProviderError> {
        match state {
            "open" | "closed" | "all" => {}
            other => {
                return Err(super::ProviderError::Validation {
                    detail: format!("estado de PR inválido: {other} (use open, closed o all)"),
                });
            }
        }
        self.list_paginated(
            &format!("/repos/{owner}/{repo}/pulls"),
            &[("per_page", "100"), ("state", state)],
            max_pages,
            parse_pull_request,
        )
    }

    /// `GET /repos/{owner}/{repo}/pulls/{number}`.
    pub fn get_pull_request(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<PullRequest, ProviderError> {
        let reply = self.send(
            Method::Get,
            &format!("/repos/{owner}/{repo}/pulls/{number}"),
            &[],
            None,
        )?;
        let payload = expect_success(reply)?;
        parse_pull_request(&payload)
    }

    /// `POST /repos/{owner}/{repo}/pulls` — provider-confirmed creation.
    pub fn create_pull_request(
        &self,
        owner: &str,
        repo: &str,
        input: &NewPullRequest,
    ) -> Result<PullRequest, ProviderError> {
        let mut body = json!({
            "title": input.title,
            "head": input.head,
            "base": input.base,
        });
        if let Some(text) = &input.body {
            body["body"] = Value::String(text.clone());
        }
        let reply = self.send(
            Method::Post,
            &format!("/repos/{owner}/{repo}/pulls"),
            &[],
            Some(&body),
        )?;
        let payload = expect_success(reply)?;
        parse_pull_request(&payload)
    }

    /// `PATCH /repos/{owner}/{repo}/pulls/{number}` — title/body/state.
    pub fn update_pull_request(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        input: &UpdatePullRequest,
    ) -> Result<PullRequest, ProviderError> {
        if input.title.is_none() && input.body.is_none() && input.state.is_none() {
            return Err(super::ProviderError::Validation {
                detail: "la actualización requiere al menos un campo (title, body o state)"
                    .to_string(),
            });
        }
        let mut body = json!({});
        if let Some(title) = &input.title {
            body["title"] = Value::String(title.clone());
        }
        if let Some(text) = &input.body {
            body["body"] = Value::String(text.clone());
        }
        if let Some(state) = input.state {
            body["state"] = Value::String(state.as_str().to_string());
        }
        let reply = self.send(
            Method::Patch,
            &format!("/repos/{owner}/{repo}/pulls/{number}"),
            &[],
            Some(&body),
        )?;
        let payload = expect_success(reply)?;
        parse_pull_request(&payload)
    }

    /// `POST /repos/{owner}/{repo}/issues/{number}/comments` — PR
    /// conversation comments live in the issues API (Spike E note).
    pub fn create_issue_comment(
        &self,
        owner: &str,
        repo: &str,
        issue: u64,
        text: &str,
    ) -> Result<Comment, ProviderError> {
        if text.trim().is_empty() {
            return Err(super::ProviderError::Validation {
                detail: "el comentario no puede estar vacío".to_string(),
            });
        }
        let body = json!({ "body": text });
        let reply = self.send(
            Method::Post,
            &format!("/repos/{owner}/{repo}/issues/{issue}/comments"),
            &[],
            Some(&body),
        )?;
        let payload = expect_success(reply)?;
        Ok(Comment {
            id: req_u64(&payload, "/id")?,
            html_url: req_str(&payload, "/html_url")?,
        })
    }

    /// `PUT /repos/{owner}/{repo}/pulls/{number}/merge`. Returns the
    /// provider's own verdict: a 200 with `merged: false` is surfaced as
    /// such (never converted to success).
    pub fn merge_pull_request(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        options: &MergeOptions,
    ) -> Result<MergeResult, ProviderError> {
        let mut body = json!({});
        if let Some(title) = &options.commit_title {
            body["commit_title"] = Value::String(title.clone());
        }
        if let Some(message) = &options.commit_message {
            body["commit_message"] = Value::String(message.clone());
        }
        if let Some(method) = options.merge_method {
            body["merge_method"] = Value::String(method.as_str().to_string());
        }
        let reply = self.send(
            Method::Put,
            &format!("/repos/{owner}/{repo}/pulls/{number}/merge"),
            &[],
            Some(&body),
        )?;
        let payload = expect_success(reply)?;
        let merged = opt_bool(&payload, "/merged")
            .ok_or_else(|| decode("campo `merged` ausente en la respuesta de merge".to_string()))?;
        Ok(MergeResult {
            merged,
            sha: opt_str(&payload, "/sha"),
            message: opt_str(&payload, "/message").unwrap_or_default(),
        })
    }

    // ---------------------------------------------------------------------
    // Wire plumbing
    // ---------------------------------------------------------------------

    /// Headers every request carries. The token exists only here (header
    /// value), never in error text; the version pin is deliberate (see
    /// `GITHUB_API_VERSION`).
    fn with_common_headers<B>(&self, builder: ureq::RequestBuilder<B>) -> ureq::RequestBuilder<B> {
        builder
            .header("Authorization", &format!("Bearer {}", self.config.token))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", GITHUB_API_VERSION)
    }

    /// One request with query parameters, following the GitHub envelope
    /// rules. `path` starts with `/`; the base URL is prepended.
    fn send(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<&Value>,
    ) -> Result<HttpReply, ProviderError> {
        // Query values here are controlled literals (`per_page`, `state`);
        // user data travels in JSON bodies, never in URLs.
        let mut url = format!("{}{}", self.config.base_url.trim_end_matches('/'), path);
        if !query.is_empty() {
            url.push('?');
            url.push_str(
                &query
                    .iter()
                    .map(|(key, value)| format!("{key}={value}"))
                    .collect::<Vec<_>>()
                    .join("&"),
            );
        }
        self.send_url(method, &url, body)
    }

    fn send_url(
        &self,
        method: Method,
        url: &str,
        body: Option<&Value>,
    ) -> Result<HttpReply, ProviderError> {
        // Write methods without an explicit body send an empty JSON object
        // (no call site relies on this today; bodyless writes would be
        // rejected by GitHub anyway).
        let empty = json!({});
        let result = match (method, body) {
            (Method::Get, _) => self.with_common_headers(self.agent.get(url)).call(),
            (Method::Post, value) => self
                .with_common_headers(self.agent.post(url))
                .send_json(value.unwrap_or(&empty)),
            (Method::Patch, value) => self
                .with_common_headers(self.agent.patch(url))
                .send_json(value.unwrap_or(&empty)),
            (Method::Put, value) => self
                .with_common_headers(self.agent.put(url))
                .send_json(value.unwrap_or(&empty)),
        };
        match result {
            Ok(mut response) => {
                let status = response.status().as_u16();
                let headers = collect_headers(&response);
                let body = response
                    .body_mut()
                    .read_to_string()
                    // A unreadable body on a 2xx is not success: the caller
                    // must see the parse failure, not a silent empty result.
                    .map_err(|error| decode(format!("cuerpo ilegible: {error}")))?;
                Ok(HttpReply {
                    status,
                    headers,
                    body,
                })
            }
            Err(error) => Err(ProviderError::Transport {
                detail: redact_secrets(&error.to_string()),
            }),
        }
    }

    /// Shared pagination: GET `path` with `query`, then follow `Link`
    /// `rel="next"` up to `max_pages` total requests.
    fn list_paginated<T>(
        &self,
        path: &str,
        query: &[(&str, &str)],
        max_pages: usize,
        parse_item: impl Fn(&Value) -> Result<T, ProviderError>,
    ) -> Result<Vec<T>, ProviderError> {
        let pages = max_pages.max(1);
        let mut items = Vec::new();
        let mut next: Option<String> = None;
        for page in 0..pages {
            let reply = if page == 0 {
                self.send(Method::Get, path, query, None)?
            } else {
                match next.take() {
                    Some(url) => self.send_url(Method::Get, &url, None)?,
                    None => break,
                }
            };
            let link = reply.header("link").map(str::to_string);
            let payload = expect_success(reply)?;
            let array = payload
                .as_array()
                .ok_or_else(|| decode(format!("se esperaba una lista en `{path}`")))?;
            for item in array {
                items.push(parse_item(item)?);
            }
            next = link.and_then(|value| next_page_url(&value));
            if next.is_none() {
                break;
            }
        }
        Ok(items)
    }
}

/// Collect headers into a lowercase-keyed vector.
fn collect_headers(response: &ureq::http::Response<ureq::Body>) -> Vec<(String, String)> {
    response
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_ascii_lowercase(),
                value.to_str().unwrap_or_default().to_string(),
            )
        })
        .collect()
}

/// Extract the next page URL from a `Link` header value
/// (`<url>; rel="next", <url>; rel="last"`).
fn next_page_url(link: &str) -> Option<String> {
    for part in link.split(',') {
        let mut segments = part.split(';');
        let target = segments
            .next()?
            .trim()
            .trim_start_matches('<')
            .trim_end_matches('>');
        let rel_is_next = segments.any(|segment| {
            let segment = segment.trim();
            segment == "rel=\"next\"" || segment == "rel=next"
        });
        if rel_is_next && target.starts_with("http") {
            return Some(target.to_string());
        }
    }
    None
}

/// Turn a 2xx reply into its JSON payload (empty body → `Null`), or a typed
/// error for every other status — the single choke point where "success"
/// is decided (M5 rule: only the provider confirms success).
fn expect_success(reply: HttpReply) -> Result<Value, ProviderError> {
    if (200..300).contains(&reply.status) {
        if reply.body.trim().is_empty() {
            return Ok(Value::Null);
        }
        return serde_json::from_str(&reply.body)
            .map_err(|error| decode(format!("JSON inválido: {error}")));
    }

    // Error envelope: `message` (Spike E, live-verified for 401/404/422).
    let server_message = serde_json::from_str::<Value>(&reply.body)
        .ok()
        .and_then(|payload| {
            payload
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "el servidor no devolvió mensaje".to_string());
    let detail = redact_secrets(&server_message);

    match reply.status {
        400 | 422 => Err(ProviderError::Validation { detail }),
        401 => Err(ProviderError::Unauthorized { detail }),
        403 => {
            // Secondary rate limits answer 403 with `Retry-After`; exhausted
            // primary budgets answer 403 with `X-Ratelimit-Remaining: 0`.
            let retry_after = retry_after(&reply);
            let exhausted = reply.header("x-ratelimit-remaining") == Some("0");
            if retry_after.is_some() || exhausted {
                Err(ProviderError::RateLimited {
                    detail,
                    retry_after,
                })
            } else {
                Err(ProviderError::Forbidden { detail })
            }
        }
        // 404 masks some permission failures (Spike E live evidence); the
        // variant carries that caveat so the UI cannot claim non-existence.
        404 => Err(ProviderError::NotFound { detail }),
        409 => Err(ProviderError::Conflict { detail }),
        429 => Err(ProviderError::RateLimited {
            detail,
            retry_after: retry_after(&reply),
        }),
        // 5xx and anything else the API may grow: provider-side failure.
        status => Err(ProviderError::Server { status, detail }),
    }
}

fn retry_after(reply: &HttpReply) -> Option<Duration> {
    reply
        .header("retry-after")
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
}

fn decode(detail: String) -> ProviderError {
    ProviderError::Decode { detail }
}

// ---------------------------------------------------------------------------
// Payload parsing: required fields fail loudly (Decode), optional ones stay
// optional — the client never invents data it did not receive.
// ---------------------------------------------------------------------------

fn req_str(payload: &Value, pointer: &str) -> Result<String, ProviderError> {
    payload
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| decode(format!("campo requerido ausente: {pointer}")))
}

fn req_u64(payload: &Value, pointer: &str) -> Result<u64, ProviderError> {
    payload
        .pointer(pointer)
        .and_then(Value::as_u64)
        .ok_or_else(|| decode(format!("campo requerido ausente: {pointer}")))
}

fn opt_str(payload: &Value, pointer: &str) -> Option<String> {
    payload
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn opt_bool(payload: &Value, pointer: &str) -> Option<bool> {
    payload.pointer(pointer).and_then(Value::as_bool)
}

fn parse_repository(payload: &Value) -> Result<Repository, ProviderError> {
    let permissions = payload.get("permissions").map(|raw| Permissions {
        admin: opt_bool(raw, "/admin").unwrap_or(false),
        push: opt_bool(raw, "/push").unwrap_or(false),
        pull: opt_bool(raw, "/pull").unwrap_or(false),
    });
    Ok(Repository {
        full_name: req_str(payload, "/full_name")?,
        name: req_str(payload, "/name")?,
        default_branch: req_str(payload, "/default_branch")?,
        clone_url: req_str(payload, "/clone_url")?,
        html_url: req_str(payload, "/html_url")?,
        private: opt_bool(payload, "/private").unwrap_or(false),
        permissions,
    })
}

fn parse_pull_request(payload: &Value) -> Result<PullRequest, ProviderError> {
    let state_raw = req_str(payload, "/state")?;
    Ok(PullRequest {
        number: req_u64(payload, "/number")?,
        state: PullRequestState::parse(&state_raw)?,
        merged: opt_bool(payload, "/merged").unwrap_or(false),
        title: req_str(payload, "/title")?,
        html_url: req_str(payload, "/html_url")?,
        head_ref: req_str(payload, "/head/ref")?,
        base_ref: req_str(payload, "/base/ref")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_debug_never_shows_the_token() {
        let config = GitHubConfig::github_com("ghp_supersecret");
        let printed = format!("{config:?}");
        assert!(!printed.contains("ghp_supersecret"), "{printed}");
        assert!(printed.contains("<redacted>"), "{printed}");
    }

    #[test]
    fn client_debug_never_shows_the_token() {
        let client = GitHubClient::new(GitHubConfig::github_com("github_pat_masctoken"));
        let printed = format!("{client:?}");
        assert!(!printed.contains("masctoken"), "{printed}");
    }

    #[test]
    fn link_header_parses_next_page() {
        let link = "<https://api.github.com/user/repos?per_page=1&page=2>; rel=\"next\", <https://api.github.com/user/repos?per_page=1&page=23>; rel=\"last\"";
        assert_eq!(
            next_page_url(link),
            Some("https://api.github.com/user/repos?per_page=1&page=2".to_string())
        );
    }

    #[test]
    fn link_header_without_next_is_none() {
        let link = "<https://api.github.com/user/repos?per_page=1&page=1>; rel=\"prev\", <https://api.github.com/user/repos?per_page=1&page=3>; rel=\"last\"";
        assert_eq!(next_page_url(link), None);
    }

    #[test]
    fn capabilities_expose_exactly_the_adapter_surface() {
        assert!(GITHUB_CAPABILITIES.contains(&Capability::MergePullRequest));
        assert!(!GITHUB_CAPABILITIES.contains(&Capability::ReviewPullRequest));
    }

    #[test]
    fn secondary_rate_limit_403_needs_retry_after_header() {
        let reply = HttpReply {
            status: 403,
            headers: vec![("retry-after".to_string(), "60".to_string())],
            body: r#"{"message":"You have exceeded a secondary rate limit."}"#.to_string(),
        };
        let error = expect_success(reply).expect_err("403 must fail");
        assert!(
            matches!(
                error,
                ProviderError::RateLimited {
                    retry_after: Some(_),
                    ..
                }
            ),
            "{error:?}"
        );
    }

    #[test]
    fn plain_403_is_forbidden_not_rate_limited() {
        let reply = HttpReply {
            status: 403,
            headers: vec![("x-ratelimit-remaining".to_string(), "4999".to_string())],
            body: r#"{"message":"Resource not accessible by integration"}"#.to_string(),
        };
        let error = expect_success(reply).expect_err("403 must fail");
        assert!(
            matches!(error, ProviderError::Forbidden { .. }),
            "{error:?}"
        );
    }
}
