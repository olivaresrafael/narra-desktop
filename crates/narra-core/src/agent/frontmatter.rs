//! Shared YAML frontmatter splitting for portable Markdown files
//! (`SKILL.md`, `AGENT.md` and `.narra/commands/*.md`).
//!
//! The split rule is identical everywhere: a file starts with `---`, the
//! frontmatter follows until the next `---` line, and the rest is the
//! Markdown body. Each file type validates its own key set; only the
//! splitting lives here so the three loaders cannot drift apart.

/// Split `text` into `(frontmatter, body)`. CRLF line endings are tolerated.
/// Error messages are plain strings; the caller attaches the path.
pub(crate) fn split_frontmatter(text: &str) -> Result<(String, String), String> {
    let normalized = text.replace("\r\n", "\n");
    let rest = normalized
        .strip_prefix("---\n")
        .ok_or_else(|| "must start with YAML frontmatter (`---`)".to_string())?;
    let end = rest
        .find("\n---\n")
        .or_else(|| rest.strip_suffix("\n---").map(|body| body.len()))
        .ok_or_else(|| "frontmatter must end with a `---` line".to_string())?;
    let frontmatter = rest[..end].to_string();
    let body_start = if rest[end..].starts_with("\n---\n") {
        end + 5
    } else {
        rest.len()
    };
    Ok((frontmatter, rest[body_start..].to_string()))
}
