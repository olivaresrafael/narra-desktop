//! Remote Git provider integrations (Milestone 5, D-006/D-018/D-022).
//!
//! This module is the provider boundary: a small trait-free surface of typed
//! clients (GitHub first) behind a capability matrix, so the UI can show
//! exactly which operations the active provider supports and never promise
//! parity that the API does not offer (D-018).
//!
//! Rules that apply to everything here (M5 acceptance):
//! - a remote action succeeded only if the provider confirmed it with a 2xx
//!   response and a parseable body — no false success states, ever;
//! - tokens live only in `GitHubConfig` (never in errors, `Debug` output,
//!   logs or fixtures); D-022 stores them in OS secret storage upstream;
//! - provider error messages are surfaced verbatim (redacted) so the user
//!   sees the provider's own diagnosis, not ours;
//! - rate limits and auth expiry are first-class outcomes, not string matching.
//!
//! Verification provenance: the GitHub behavior encoded here comes from the
//! Spike E capability matrix in `TECHNICAL_SPIKES.md` (official docs plus
//! read-only live probes, 2026-10-22).

pub mod github;
pub mod secrets;

use std::fmt;
use std::time::Duration;

pub use github::{
    Comment, GITHUB_API_BASE_URL, GITHUB_CAPABILITIES, GitHubClient, GitHubConfig, MergeMethod,
    MergeOptions, MergeResult, NewPullRequest, NewRepository, Permissions, PullRequest,
    PullRequestState, Repository, UpdatePullRequest, User,
};
pub use secrets::{
    GITHUB_TOKEN_ACCOUNT, MemorySecretStore, NARRA_SERVICE, OsKeychain, SecretError, SecretStore,
};

/// One operation the product may want from a remote provider. The enum is
/// closed on purpose: unsupported operations cannot even be requested, and
/// `Capability::label` gives the UI a localizable key (English today).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    /// List the authenticated user's repositories.
    ListRepositories,
    /// Create a repository.
    CreateRepository,
    /// List branches.
    ListBranches,
    /// List pull/merge requests.
    ListPullRequests,
    /// Create a pull/merge request.
    CreatePullRequest,
    /// View one pull/merge request.
    ViewPullRequest,
    /// Update title/body/state of a pull/merge request.
    UpdatePullRequest,
    /// Comment on a pull/merge request.
    CommentOnPullRequest,
    /// Merge a pull/merge request.
    MergePullRequest,
    /// Submit a review (approve/request changes/comment).
    ReviewPullRequest,
}

impl Capability {
    /// Stable identifier for UI, settings and audits (English; on-disk
    /// names are English per D-017).
    pub fn label(self) -> &'static str {
        match self {
            Capability::ListRepositories => "list_repositories",
            Capability::CreateRepository => "create_repository",
            Capability::ListBranches => "list_branches",
            Capability::ListPullRequests => "list_pull_requests",
            Capability::CreatePullRequest => "create_pull_request",
            Capability::ViewPullRequest => "view_pull_request",
            Capability::UpdatePullRequest => "update_pull_request",
            Capability::CommentOnPullRequest => "comment_on_pull_request",
            Capability::MergePullRequest => "merge_pull_request",
            Capability::ReviewPullRequest => "review_pull_request",
        }
    }
}

/// Typed provider failures. Variants mirror the provider's own semantics
/// (Spike E): auth, permission, existence, validation, rate limiting and
/// server/transport trouble. Every detail string is already redacted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    /// 401: the token is missing, expired or invalid.
    Unauthorized { detail: String },
    /// 403: authenticated but not allowed (and not a rate limit).
    Forbidden { detail: String },
    /// 404: GitHub masks some insufficient-permission cases as 404, so
    /// "not found" is never proof of non-existence (Spike E evidence).
    NotFound { detail: String },
    /// 400/422: the provider rejected the request itself (validation).
    Validation { detail: String },
    /// 429, or the secondary-limit 403 with `Retry-After`.
    RateLimited {
        detail: String,
        retry_after: Option<Duration>,
    },
    /// 409: state conflict (e.g. merge conflict).
    Conflict { detail: String },
    /// 5xx: provider-side failure.
    Server { status: u16, detail: String },
    /// The provider answered, but not in a shape we can trust.
    Decode { detail: String },
    /// Network/DNS/connection failure before a response existed.
    Transport { detail: String },
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderError::Unauthorized { detail } => write!(f, "unauthorized: {detail}"),
            ProviderError::Forbidden { detail } => write!(f, "forbidden: {detail}"),
            ProviderError::NotFound { detail } => write!(f, "not found: {detail}"),
            ProviderError::Validation { detail } => write!(f, "validation failed: {detail}"),
            ProviderError::RateLimited {
                detail,
                retry_after,
            } => match retry_after {
                Some(after) => write!(f, "rate limited: {detail} (retry after {after:?})"),
                None => write!(f, "rate limited: {detail}"),
            },
            ProviderError::Conflict { detail } => write!(f, "conflict: {detail}"),
            ProviderError::Server { status, detail } => {
                write!(f, "server error {status}: {detail}")
            }
            ProviderError::Decode { detail } => write!(f, "unexpected response: {detail}"),
            ProviderError::Transport { detail } => write!(f, "network failure: {detail}"),
        }
    }
}

impl std::error::Error for ProviderError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_labels_are_stable_identifiers() {
        assert_eq!(Capability::ListRepositories.label(), "list_repositories");
        assert_eq!(Capability::MergePullRequest.label(), "merge_pull_request");
        assert_eq!(Capability::ReviewPullRequest.label(), "review_pull_request");
    }

    #[test]
    fn display_never_adds_secrets() {
        let error = ProviderError::Unauthorized {
            detail: "Bad credentials".to_string(),
        };
        assert_eq!(error.to_string(), "unauthorized: Bad credentials");
    }
}
