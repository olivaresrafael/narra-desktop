//! Reviewable change proposals (Milestone 4, slice 2b).
//!
//! Implements the `Change proposal` record from `DATA_MODEL.md` §98: a set of
//! proposed file changes with base revision, affected path, line diff,
//! rationale, originating task/agent and a state machine. Product rules
//! (`ARCHITECTURE.md` §Proposal/review service):
//!
//! - a proposal never writes canonical files by itself: only [`apply`] does,
//!   and only after an explicit selection of hunks (partial acceptance);
//! - a rejected proposal leaves canonical files untouched;
//! - staleness is decided by **content hash**: if the target file changed
//!   after the proposal was created, applying is refused and the proposal is
//!   marked [`ProposalState::Stale`] — renewed review (a fresh proposal run)
//!   is required. Git HEAD is recorded as provenance only: a file can change
//!   without a commit, and a commit can move without touching this file.
//!
//! Proposals are workflow state, not canon: they are persisted as JSON under
//! `<project>/.narra/proposals/<uuid>.json` through boundary-checked document
//! IO (atomic write, no path escape).

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use similar::{DiffOp, TextDiff};
use thiserror::Error;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::document::{self, DocumentError};

/// Project-relative directory that holds persisted proposals.
pub const PROPOSALS_DIR: &str = ".narra/proposals";

/// Failures from proposal storage and lifecycle operations.
#[derive(Debug, Error)]
pub enum ProposalError {
    #[error(transparent)]
    Document(#[from] DocumentError),

    #[error("invalid proposal id: {0}")]
    InvalidId(String),

    #[error("proposal not found: {0}")]
    NotFound(String),

    #[error("proposal {id} is in state `{state}` and cannot be {action}")]
    InvalidState {
        id: String,
        state: ProposalState,
        action: &'static str,
    },

    #[error(
        "proposal {id} is stale: `{target}` changed after it was created; \
         renewed review required (run the propose task again)"
    )]
    Stale { id: String, target: String },

    #[error("hunk index {index} is out of range (proposal has {len} hunks)")]
    InvalidHunk { index: usize, len: usize },

    #[error(
        "proposal anchors do not match the current content of `{path}`; \
         renewed review required"
    )]
    AnchorMismatch { path: String },

    #[error("proposal {id} has no hunks to apply")]
    Empty { id: String },

    #[error("the model produced no changes for `{target}`")]
    NoChanges { target: String },

    #[error("failed to persist proposal: {0}")]
    Io(#[source] std::io::Error),

    #[error("failed to encode proposal: {0}")]
    Serde(#[from] serde_json::Error),
}

/// Proposal lifecycle states (`DATA_MODEL.md` §98). Only the states with real
/// semantics in this milestone are defined; `draft`, `approved` and `failed`
/// stay reserved until a flow needs them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalState {
    Draft,
    ReadyForReview,
    Applied,
    Rejected,
    Stale,
}

impl std::fmt::Display for ProposalState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Draft => "draft",
            Self::ReadyForReview => "ready_for_review",
            Self::Applied => "applied",
            Self::Rejected => "rejected",
            Self::Stale => "stale",
        };
        f.write_str(label)
    }
}

/// One contiguous line change. `old_start` is a 0-based line index into the
/// base content; `removed` lines must still be present there when the
/// proposal is applied (that check is the defense behind the content hash).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hunk {
    pub old_start: usize,
    pub removed: Vec<String>,
    pub added: Vec<String>,
}

/// Where the proposal came from (`DATA_MODEL.md`: originating task/agent).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    /// Task kind, e.g. `"scene_edit"`.
    pub task: String,
    /// Adapter name, e.g. `"mock"` or `"openai-compat"`.
    pub adapter: String,
    /// Provider model identifier reported with the response.
    pub model: String,
    /// The originating task input, so a stale proposal can be renewed by
    /// re-running the same task against the current content.
    pub task_input: serde_json::Value,
}

/// A persisted, reviewable set of file changes (`DATA_MODEL.md` §98).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileProposal {
    pub id: String,
    /// RFC 3339 creation time.
    pub created_at: String,
    pub state: ProposalState,
    /// Project-relative path of the affected file.
    pub target: String,
    /// Git HEAD oid when the project was a repository; `null` otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_revision: Option<String>,
    /// SHA-256 (hex) of the target content the hunks were computed from.
    pub base_sha256: String,
    /// Why the change was requested (the user-facing instruction).
    pub rationale: String,
    pub origin: Origin,
    pub hunks: Vec<Hunk>,
    /// Indices into `hunks`, recorded when the proposal was applied.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applied_hunks: Vec<usize>,
}

impl FileProposal {
    /// Build a proposal from the base content and the proposed replacement.
    /// Returns [`ProposalError::Empty`] when the model produced no change.
    pub fn new(
        target: impl Into<String>,
        base: &str,
        proposed: &str,
        rationale: impl Into<String>,
        origin: Origin,
        base_revision: Option<String>,
    ) -> Result<Self, ProposalError> {
        let hunks = compute_hunks(base, proposed);
        if hunks.is_empty() {
            return Err(ProposalError::NoChanges {
                target: target.into(),
            });
        }
        Ok(Self {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .expect("RFC 3339 formatting cannot fail for UTC timestamps"),
            state: ProposalState::ReadyForReview,
            target: target.into(),
            base_revision,
            base_sha256: sha256_hex(base),
            rationale: rationale.into(),
            origin,
            hunks,
            applied_hunks: Vec::new(),
        })
    }
}

/// Result of applying a proposal to its target file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApplyOutcome {
    pub proposal: FileProposal,
    pub applied_hunks: Vec<usize>,
    pub skipped_hunks: Vec<usize>,
}

/// SHA-256 of `content` as lowercase hex. Used for proposal base identity:
/// stable across Rust releases (unlike `std`'s default hasher), persisted.
pub fn sha256_hex(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Compute line hunks that turn `base` into `proposed`.
///
/// Consecutive non-equal diff operations are merged into a single hunk so a
/// mixed edit reads as one reviewable block instead of a delete plus an
/// insert. Hunks are ordered and non-overlapping by construction.
pub fn compute_hunks(base: &str, proposed: &str) -> Vec<Hunk> {
    let base_lines: Vec<&str> = base.lines().collect();
    let proposed_lines: Vec<&str> = proposed.lines().collect();
    let diff = TextDiff::from_lines(base, proposed);

    let mut hunks: Vec<Hunk> = Vec::new();
    let mut current: Option<Hunk> = None;
    // First base line not yet consumed by the pending hunk; used to place a
    // pure insertion that `similar` reports without an old line index.
    let mut old_cursor = 0usize;

    for op in diff.ops() {
        match *op {
            DiffOp::Equal { old_index, len, .. } => {
                if let Some(hunk) = current.take() {
                    hunks.push(hunk);
                }
                old_cursor = old_index + len;
            }
            DiffOp::Delete {
                old_index, old_len, ..
            }
            | DiffOp::Replace {
                old_index, old_len, ..
            } => {
                let removed = slice_lines(&base_lines, old_index, old_len);
                let added = match *op {
                    DiffOp::Replace {
                        new_index, new_len, ..
                    } => slice_lines(&proposed_lines, new_index, new_len),
                    _ => Vec::new(),
                };
                let contiguous = matches!(
                    &current,
                    Some(hunk) if hunk.old_start + hunk.removed.len() == old_index
                );
                match current {
                    Some(mut hunk) if contiguous => {
                        hunk.removed.extend(removed);
                        hunk.added.extend(added);
                        current = Some(hunk);
                    }
                    _ => {
                        if let Some(hunk) = current.take() {
                            hunks.push(hunk);
                        }
                        current = Some(Hunk {
                            old_start: old_index,
                            removed,
                            added,
                        });
                    }
                }
                old_cursor = old_index + old_len;
            }
            DiffOp::Insert {
                new_index, new_len, ..
            } => {
                let added = slice_lines(&proposed_lines, new_index, new_len);
                match current {
                    // A pending hunk that ends exactly here grows instead of
                    // splitting a single visual edit into two reviews.
                    Some(mut hunk) if hunk.old_start + hunk.removed.len() == old_cursor => {
                        hunk.added.extend(added);
                        current = Some(hunk);
                    }
                    _ => {
                        if let Some(hunk) = current.take() {
                            hunks.push(hunk);
                        }
                        current = Some(Hunk {
                            old_start: old_cursor,
                            removed: Vec::new(),
                            added,
                        });
                    }
                }
            }
        }
    }
    if let Some(hunk) = current.take() {
        hunks.push(hunk);
    }
    hunks
}

fn slice_lines(lines: &[&str], start: usize, len: usize) -> Vec<String> {
    lines
        .get(start..start + len)
        .map(|slice| slice.iter().map(|line| (*line).to_string()).collect())
        .unwrap_or_default()
}

/// Rebuild `base` applying only the accepted hunks (by index).
///
/// Rejected hunks keep their base lines. The recorded `removed` lines are
/// re-checked against `base` before writing: any mismatch means the diff and
/// the content disagree and application must stop instead of corrupting the
/// file.
pub fn apply_hunks(
    base: &str,
    hunks: &[Hunk],
    accepted: &[usize],
) -> Result<String, ProposalError> {
    let base_lines: Vec<&str> = base.lines().collect();
    let mut out: Vec<String> = Vec::new();
    let mut cursor = 0usize;

    for (index, hunk) in hunks.iter().enumerate() {
        let start = hunk.old_start;
        let end = start + hunk.removed.len();
        let anchors = start >= cursor
            && end <= base_lines.len()
            && base_lines[start..end]
                .iter()
                .zip(hunk.removed.iter())
                .all(|(current, recorded)| *current == recorded.as_str());
        if !anchors {
            return Err(ProposalError::AnchorMismatch {
                path: String::new(),
            });
        }
        out.extend(
            base_lines[cursor..start]
                .iter()
                .map(|line| (*line).to_string()),
        );
        if accepted.contains(&index) {
            out.extend(hunk.added.iter().cloned());
        } else {
            out.extend(hunk.removed.iter().cloned());
        }
        cursor = end;
    }
    out.extend(base_lines[cursor..].iter().map(|line| (*line).to_string()));

    let mut text = out.join("\n");
    if base.ends_with('\n') {
        text.push('\n');
    }
    Ok(text)
}

/// Validate a proposal id before it becomes part of a path (UUID shape only:
/// hex groups separated by dashes — no traversal is possible).
fn proposal_rel_path(id: &str) -> Result<String, ProposalError> {
    let valid = id.len() == 36
        && id.chars().enumerate().all(|(index, ch)| match index {
            8 | 13 | 18 | 23 => ch == '-',
            _ => ch.is_ascii_hexdigit(),
        });
    if !valid {
        return Err(ProposalError::InvalidId(id.to_string()));
    }
    Ok(format!("{PROPOSALS_DIR}/{id}.json"))
}

/// Persist a proposal inside the project (atomic, boundary-checked write).
pub fn save(root: &Path, proposal: &FileProposal) -> Result<(), ProposalError> {
    let relative = proposal_rel_path(&proposal.id)?;
    let json = serde_json::to_string_pretty(proposal)?;
    document::write_document(root, &relative, &json)?;
    Ok(())
}

/// Load one proposal by id.
pub fn load(root: &Path, id: &str) -> Result<FileProposal, ProposalError> {
    let relative = proposal_rel_path(id)?;
    let json = document::read_document(root, &relative).map_err(|error| match error {
        DocumentError::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
            ProposalError::NotFound(id.to_string())
        }
        other => ProposalError::Document(other),
    })?;
    Ok(serde_json::from_str(&json)?)
}

/// List persisted proposals, newest first. An absent directory is an empty
/// list, not an error.
pub fn list(root: &Path) -> Result<Vec<FileProposal>, ProposalError> {
    let dir = root.join(PROPOSALS_DIR);
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(ProposalError::Document(DocumentError::Io {
                path: dir,
                source,
            }));
        }
    };

    let mut proposals = Vec::new();
    for entry in entries {
        let entry = entry.map_err(ProposalError::Io)?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(id) = name.strip_suffix(".json") else {
            continue;
        };
        proposals.push(load(root, id)?);
    }
    proposals.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(proposals)
}

/// Apply accepted hunks to the target file.
///
/// Refuses when the target content no longer matches the proposal's base
/// (the proposal becomes [`ProposalState::Stale`] and requires a renewed
/// review) and when the proposal is not in a reviewable state.
pub fn apply(
    root: &Path,
    id: &str,
    accepted: Option<&[usize]>,
) -> Result<ApplyOutcome, ProposalError> {
    let mut proposal = load(root, id)?;
    match proposal.state {
        ProposalState::ReadyForReview | ProposalState::Draft => {}
        state => {
            return Err(ProposalError::InvalidState {
                id: id.to_string(),
                state,
                action: "applied",
            });
        }
    }

    let base = document::read_document(root, &proposal.target)?;
    if sha256_hex(&base) != proposal.base_sha256 {
        proposal.state = ProposalState::Stale;
        save(root, &proposal)?;
        return Err(ProposalError::Stale {
            id: id.to_string(),
            target: proposal.target.clone(),
        });
    }
    if proposal.hunks.is_empty() {
        return Err(ProposalError::Empty { id: id.to_string() });
    }

    let applied: Vec<usize> = match accepted {
        Some(indices) => {
            for &index in indices {
                if index >= proposal.hunks.len() {
                    return Err(ProposalError::InvalidHunk {
                        index,
                        len: proposal.hunks.len(),
                    });
                }
            }
            let mut sorted = indices.to_vec();
            sorted.sort_unstable();
            sorted.dedup();
            sorted
        }
        None => (0..proposal.hunks.len()).collect(),
    };

    let content = apply_hunks(&base, &proposal.hunks, &applied).map_err(|error| match error {
        ProposalError::AnchorMismatch { .. } => ProposalError::AnchorMismatch {
            path: proposal.target.clone(),
        },
        other => other,
    })?;
    document::write_document(root, &proposal.target, &content)?;

    // Bookkeeping runs after the canonical write; a failure here is reported
    // honestly (the file changed, the record did not).
    proposal.applied_hunks = applied.clone();
    proposal.state = ProposalState::Applied;
    save(root, &proposal)?;

    let skipped = (0..proposal.hunks.len())
        .filter(|index| !applied.contains(index))
        .collect();
    Ok(ApplyOutcome {
        proposal,
        applied_hunks: applied,
        skipped_hunks: skipped,
    })
}

/// Reject a proposal: record the decision, touch no canonical file.
pub fn reject(root: &Path, id: &str) -> Result<FileProposal, ProposalError> {
    let mut proposal = load(root, id)?;
    if proposal.state == ProposalState::Applied {
        return Err(ProposalError::InvalidState {
            id: id.to_string(),
            state: proposal.state,
            action: "rejected",
        });
    }
    proposal.state = ProposalState::Rejected;
    save(root, &proposal)?;
    Ok(proposal)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hunk(start: usize, removed: &[&str], added: &[&str]) -> Hunk {
        Hunk {
            old_start: start,
            removed: removed.iter().map(|l| (*l).to_string()).collect(),
            added: added.iter().map(|l| (*l).to_string()).collect(),
        }
    }

    #[test]
    fn single_replacement_is_one_hunk() {
        let base = "INT. TALLER - NOCHE\n\nELENA\nHola.\n\nFIN\n";
        let proposed = "INT. TALLER - NOCHE\n\nELENA\n¿Dónde estabas?\n\nFIN\n";
        let hunks = compute_hunks(base, proposed);
        assert_eq!(hunks.len(), 1, "{hunks:?}");
        assert_eq!(hunks[0].old_start, 3);
        assert_eq!(hunks[0].removed, vec!["Hola.".to_string()]);
        assert_eq!(hunks[0].added, vec!["¿Dónde estabas?".to_string()]);
    }

    #[test]
    fn distant_changes_become_separate_hunks() {
        let base = "uno\ndos\ntres\ncuatro\ncinco\n";
        let proposed = "uno\nDOS\ntres\ncuatro\nCINCO\n";
        let hunks = compute_hunks(base, proposed);
        assert_eq!(hunks.len(), 2, "{hunks:?}");
        assert_eq!(hunks[0].old_start, 1);
        assert_eq!(hunks[1].old_start, 4);
    }

    #[test]
    fn adjacent_delete_and_insert_merge_into_one_hunk() {
        let base = "a\nb\nc\n";
        let proposed = "a\nB2\nB3\nc\n";
        let hunks = compute_hunks(base, proposed);
        assert_eq!(hunks.len(), 1, "{hunks:?}");
        assert_eq!(hunks[0].removed, vec!["b".to_string()]);
        assert_eq!(hunks[0].added, vec!["B2".to_string(), "B3".to_string()]);
    }

    #[test]
    fn applying_every_hunk_reproduces_the_proposal() {
        let base = "uno\ndos\ntres\ncuatro\ncinco\nseis\n";
        let proposed = "uno\nDOS\ntres\ncuatro\nCINCO\nseis\n";
        let hunks = compute_hunks(base, proposed);
        let accepted: Vec<usize> = (0..hunks.len()).collect();
        let rebuilt = apply_hunks(base, &hunks, &accepted).expect("apply");
        assert_eq!(rebuilt, proposed);
    }

    #[test]
    fn applying_nothing_reproduces_the_base() {
        let base = "uno\ndos\ntres\n";
        let proposed = "uno\nDOS\ntres\n";
        let hunks = compute_hunks(base, proposed);
        let rebuilt = apply_hunks(base, &hunks, &[]).expect("apply");
        assert_eq!(rebuilt, base);
    }

    #[test]
    fn partial_acceptance_applies_only_selected_hunks() {
        let base = "uno\ndos\ntres\ncuatro\ncinco\n";
        let proposed = "uno\nDOS\ntres\ncuatro\nCINCO\n";
        let hunks = compute_hunks(base, proposed);
        assert_eq!(hunks.len(), 2);
        let rebuilt = apply_hunks(base, &hunks, &[1]).expect("apply");
        assert_eq!(rebuilt, "uno\ndos\ntres\ncuatro\nCINCO\n");
    }

    #[test]
    fn anchor_mismatch_is_refused_instead_of_corrupting() {
        let hunks = vec![hunk(2, &["gone"], &["new"])];
        let error = apply_hunks("one\ntwo\nthree\n", &hunks, &[0]).expect_err("anchors");
        assert!(
            matches!(error, ProposalError::AnchorMismatch { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn insertion_hunk_at_file_start() {
        let base = "b\n";
        let proposed = "a\nb\n";
        let hunks = compute_hunks(base, proposed);
        assert_eq!(hunks.len(), 1, "{hunks:?}");
        assert!(hunks[0].removed.is_empty());
        let rebuilt = apply_hunks(base, &hunks, &[0]).expect("apply");
        assert_eq!(rebuilt, proposed);
    }

    #[test]
    fn trailing_newline_is_preserved() {
        let base = "a\nb\n";
        let proposed = "a\nb\nc\n";
        let hunks = compute_hunks(base, proposed);
        let rebuilt = apply_hunks(base, &hunks, &[0]).expect("apply");
        assert_eq!(rebuilt, proposed);
        let no_newline = "a\nb";
        let proposed_no_newline = "a\nb\nc";
        let hunks = compute_hunks(no_newline, proposed_no_newline);
        let rebuilt = apply_hunks(no_newline, &hunks, &[0]).expect("apply");
        assert_eq!(rebuilt, proposed_no_newline);
    }

    #[test]
    fn proposal_ids_are_validated_before_becoming_paths() {
        assert!(proposal_rel_path("../../etc/passwd").is_err());
        assert!(proposal_rel_path("short").is_err());
        let id = "123e4567-e89b-12d3-a456-426614174000";
        let path = proposal_rel_path(id).expect("valid uuid");
        assert_eq!(path, format!("{PROPOSALS_DIR}/{id}.json"));
    }

    #[test]
    fn sha256_is_stable_and_content_sensitive() {
        assert_eq!(sha256_hex(""), sha256_hex(""));
        assert_ne!(sha256_hex("a"), sha256_hex("b"));
        assert_eq!(sha256_hex("abc").len(), 64);
    }

    #[test]
    fn state_and_hunks_serialize_with_data_model_names() {
        let state = serde_json::to_string(&ProposalState::ReadyForReview).expect("json");
        assert_eq!(state, "\"ready_for_review\"");
        let stale = serde_json::to_string(&ProposalState::Stale).expect("json");
        assert_eq!(stale, "\"stale\"");
    }

    #[test]
    fn creating_a_proposal_without_changes_is_an_explicit_error() {
        let origin = Origin {
            task: "scene_edit".into(),
            adapter: "mock".into(),
            model: "mock-1".into(),
            task_input: serde_json::json!({"scene": 1}),
        };
        let error = FileProposal::new(
            "screenplay/main.fountain",
            "same\n",
            "same\n",
            "r",
            origin,
            None,
        )
        .expect_err("no changes");
        assert!(
            matches!(error, ProposalError::NoChanges { .. }),
            "{error:?}"
        );
    }
}
