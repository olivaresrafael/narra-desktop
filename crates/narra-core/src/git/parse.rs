//! Parsers for Git's machine-readable outputs.
//!
//! Everything here consumes stable, machine-oriented formats (`--version`,
//! `--porcelain=v2`, custom `log` formats) so parsing never depends on
//! localized human-facing messages.

use super::{BranchInfo, Commit, FileChange, StatusEntry, WorktreeStatus};

/// `git version 2.50.1 (Apple Git-155)` → `(2, 50, 1)`.
pub(crate) fn parse_version(raw: &str) -> Option<(u32, u32, u32)> {
    let rest = raw.trim().strip_prefix("git version ")?;
    let numbers: Vec<u32> = rest
        .split(|c: char| c == '.' || c == '-' || c.is_whitespace())
        .filter_map(|part| part.parse::<u32>().ok())
        .take(3)
        .collect();
    match numbers.as_slice() {
        [major, minor, patch] => Some((*major, *minor, *patch)),
        [major, minor] => Some((*major, *minor, 0)),
        _ => None,
    }
}

/// Parse `git status --porcelain=v2 --branch` output.
///
/// Branch headers look like `# branch.oid <oid>`; entries are `1 <XY> …`,
/// `2 <XY> …` (rename), `u <XY> …` (unmerged), `? <path>` (untracked) and
/// `! <path>` (ignored).
pub(crate) fn parse_status(raw: &str) -> (Option<BranchInfo>, Vec<StatusEntry>) {
    let mut branch = BranchInfo::default();
    let mut entries = Vec::new();

    for line in raw.lines() {
        if let Some(header) = line.strip_prefix("# ") {
            parse_branch_header(header, &mut branch);
            continue;
        }
        let mut fields = line.splitn(2, ' ');
        let Some(kind) = fields.next() else { continue };
        let rest = fields.next().unwrap_or("");
        match kind {
            "?" => entries.push(StatusEntry {
                path: rest.to_string(),
                index: WorktreeStatus::Untracked,
                worktree: WorktreeStatus::Untracked,
            }),
            "!" => {}
            "1" | "2" => {
                // `1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>`
                // `2 … <X><score> <path>\t<origPath>` — the path is field 9.
                let parts: Vec<&str> = line.splitn(9, ' ').collect();
                if parts.len() < 9 {
                    continue;
                }
                let xy = parts[1].as_bytes();
                let (index, worktree) = split_xy(xy);
                let path = parts[8].to_string();
                entries.push(StatusEntry {
                    path,
                    index,
                    worktree,
                });
            }
            "u" => {
                // `u <XY> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>`
                let parts: Vec<&str> = line.splitn(11, ' ').collect();
                if parts.len() < 11 {
                    continue;
                }
                entries.push(StatusEntry {
                    path: parts[10].to_string(),
                    index: WorktreeStatus::Conflicted,
                    worktree: WorktreeStatus::Conflicted,
                });
            }
            _ => {}
        }
    }
    (
        if branch.oid.is_some() || branch.head.is_some() {
            Some(branch)
        } else {
            None
        },
        entries,
    )
}

fn parse_branch_header(header: &str, branch: &mut BranchInfo) {
    let mut parts = header.splitn(2, ' ');
    let key = parts.next().unwrap_or("");
    let value = parts.next().unwrap_or("");
    match key {
        "branch.oid" if value != "(initial)" => branch.oid = Some(value.to_string()),
        "branch.head" if value != "(detached)" => branch.head = Some(value.to_string()),
        "branch.upstream" => branch.upstream = Some(value.to_string()),
        "branch.ab" => {
            // `+<ahead> -<behind>`
            let mut segments = value.split_whitespace();
            if let Some(ahead) = segments.next().and_then(|s| s.strip_prefix('+')) {
                branch.ahead = ahead.parse().unwrap_or(0);
            }
            if let Some(behind) = segments.next().and_then(|s| s.strip_prefix('-')) {
                branch.behind = behind.parse().unwrap_or(0);
            }
        }
        _ => {}
    }
}

/// `XY` in porcelain v2: index status then worktree status, `.` = unmodified.
fn split_xy(xy: &[u8]) -> (WorktreeStatus, WorktreeStatus) {
    fn one(byte: u8) -> WorktreeStatus {
        WorktreeStatus::from_code(byte as char)
    }
    match xy {
        [index, worktree] => (one(*index), one(*worktree)),
        _ => (WorktreeStatus::Unknown, WorktreeStatus::Unknown),
    }
}

/// Parse `git for-each-ref refs/heads` output: `<short>%09<objectname>%09<HEAD>`
/// per line, where the third field is `*` for the current branch.
pub(crate) fn parse_branches(raw: &str) -> Vec<super::Branch> {
    raw.lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let name = fields.next()?.trim();
            let id = fields.next()?.trim();
            let current = fields.next().is_some_and(|flag| flag.trim() == "*");
            if name.is_empty() || id.is_empty() {
                return None;
            }
            Some(super::Branch {
                name: name.to_string(),
                id: id.to_string(),
                current,
            })
        })
        .collect()
}

/// Parse the NUL-safe record format produced by
/// `git log --format=%H%x1f%an%x1f%ae%x1f%aI%x1f%s%x1e`.
pub(crate) fn parse_log(raw: &str) -> Vec<Commit> {
    raw.split('\x1e')
        .filter(|record| !record.trim().is_empty())
        .filter_map(|record| {
            let fields: Vec<&str> = record.trim_start_matches('\n').split('\x1f').collect();
            let [id, author, email, date, subject] = fields.as_slice() else {
                return None;
            };
            Some(Commit {
                id: (*id).to_string(),
                author: (*author).to_string(),
                email: (*email).to_string(),
                date: (*date).to_string(),
                subject: (*subject).to_string(),
            })
        })
        .collect()
}

/// Parse `git diff --numstat` output into per-file change counts.
pub(crate) fn parse_numstat(raw: &str) -> Vec<FileChange> {
    raw.lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, '\t');
            let added = fields.next()?.parse::<u32>().ok()?;
            let removed = fields.next()?.parse::<u32>().ok()?;
            let path = fields.next()?.to_string();
            Some(FileChange {
                path,
                added,
                removed,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_apple_git_version() {
        assert_eq!(
            parse_version("git version 2.50.1 (Apple Git-155)"),
            Some((2, 50, 1))
        );
    }

    #[test]
    fn parses_plain_and_short_versions() {
        assert_eq!(parse_version("git version 2.39.3"), Some((2, 39, 3)));
        assert_eq!(parse_version("git version 2.40"), Some((2, 40, 0)));
        assert_eq!(parse_version("not git at all"), None);
    }

    #[test]
    fn parses_status_with_branch_headers() {
        let raw = concat!(
            "# branch.oid 4a9f2c1d\n",
            "# branch.head main\n",
            "# branch.upstream origin/main\n",
            "# branch.ab +1 -2\n",
            "1 M. N... 100644 100644 100644 abc def screenplay/main.fountain\n",
            "1 .M N... 100644 100644 100644 abc def notes/decisions.md\n",
            "? notes/nueva.md\n",
        );
        let (branch, entries) = parse_status(raw);
        let branch = branch.expect("branch info");
        assert_eq!(branch.oid.as_deref(), Some("4a9f2c1d"));
        assert_eq!(branch.head.as_deref(), Some("main"));
        assert_eq!(branch.ahead, 1);
        assert_eq!(branch.behind, 2);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].index, WorktreeStatus::Modified);
        assert_eq!(entries[0].worktree, WorktreeStatus::Unmodified);
        assert_eq!(entries[1].worktree, WorktreeStatus::Modified);
        assert_eq!(entries[2].index, WorktreeStatus::Untracked);
    }

    #[test]
    fn parses_status_paths_with_spaces_and_accents() {
        let raw = "1 .M N... 100644 100644 100644 abc def notes/nota de investigación.md\n";
        let (_, entries) = parse_status(raw);
        assert_eq!(entries[0].path, "notes/nota de investigación.md");
    }

    #[test]
    fn initial_repo_has_no_oid() {
        let raw = "# branch.oid (initial)\n# branch.head main\n? archivo.md\n";
        let (branch, entries) = parse_status(raw);
        let branch = branch.expect("head still known");
        assert!(branch.oid.is_none());
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn parses_branch_list() {
        let raw = "main\tabc123\t*\nfeature/escena\tdef456\t\n";
        let branches = parse_branches(raw);
        assert_eq!(branches.len(), 2);
        assert!(branches[0].current);
        assert_eq!(branches[0].name, "main");
        assert!(!branches[1].current);
        assert_eq!(branches[1].name, "feature/escena");
    }

    #[test]
    fn parses_log_records() {
        let raw = "abc123\x1fAna Autora\x1fana@ejemplo.com\x1f2026-10-22T10:00:00+02:00\x1fEmpieza la serie\x1e\ndef456\x1fLópez\x1flo@ejemplo.com\x1f2026-10-22T11:00:00+02:00\x1fAjusta escena 2\x1e";
        let commits = parse_log(raw);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].subject, "Empieza la serie");
        assert_eq!(commits[1].author, "López");
    }

    #[test]
    fn parses_numstat() {
        let raw = "12\t3\tscr/main.fountain\n-\t-\tbinary.png\n1\t0\tnotes/a.md\n";
        let changes = parse_numstat(raw);
        assert_eq!(changes.len(), 2, "binary rows are skipped");
        assert_eq!(changes[0].added, 12);
        assert_eq!(changes[0].removed, 3);
        assert_eq!(changes[1].path, "notes/a.md");
    }
}
