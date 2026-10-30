//! Embedded UI assets (M10 S3c).
//!
//! Only the small Lucide subset the shell needs is bundled. The SVGs are
//! compiled into the binary so development and the packaged app resolve the
//! same paths without a working-directory or bundle-resource dependency.
//! Lucide is ISC-licensed (GPL-3.0 compatible); see `assets/icons/LICENSE`.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

/// Icon name → bytes. Paths are the strings passed to `svg().path(...)`.
const ICONS: &[(&str, &[u8])] = &[
    (
        "icons/arrow-up.svg",
        include_bytes!("../assets/icons/arrow-up.svg"),
    ),
    (
        "icons/clapperboard.svg",
        include_bytes!("../assets/icons/clapperboard.svg"),
    ),
    (
        "icons/chevron-down.svg",
        include_bytes!("../assets/icons/chevron-down.svg"),
    ),
    (
        "icons/chevron-right.svg",
        include_bytes!("../assets/icons/chevron-right.svg"),
    ),
    ("icons/copy.svg", include_bytes!("../assets/icons/copy.svg")),
    (
        "icons/crosshair.svg",
        include_bytes!("../assets/icons/crosshair.svg"),
    ),
    (
        "icons/ellipsis.svg",
        include_bytes!("../assets/icons/ellipsis.svg"),
    ),
    (
        "icons/expand.svg",
        include_bytes!("../assets/icons/expand.svg"),
    ),
    (
        "icons/image.svg",
        include_bytes!("../assets/icons/image.svg"),
    ),
    ("icons/plus.svg", include_bytes!("../assets/icons/plus.svg")),
    (
        "icons/square.svg",
        include_bytes!("../assets/icons/square.svg"),
    ),
    ("icons/user.svg", include_bytes!("../assets/icons/user.svg")),
    (
        "icons/pencil.svg",
        include_bytes!("../assets/icons/pencil.svg"),
    ),
    (
        "icons/book-open.svg",
        include_bytes!("../assets/icons/book-open.svg"),
    ),
    (
        "icons/brain.svg",
        include_bytes!("../assets/icons/brain.svg"),
    ),
    (
        "icons/file-text.svg",
        include_bytes!("../assets/icons/file-text.svg"),
    ),
    (
        "icons/gauge.svg",
        include_bytes!("../assets/icons/gauge.svg"),
    ),
    ("icons/file.svg", include_bytes!("../assets/icons/file.svg")),
    (
        "icons/folder.svg",
        include_bytes!("../assets/icons/folder.svg"),
    ),
    (
        "icons/git-branch.svg",
        include_bytes!("../assets/icons/git-branch.svg"),
    ),
    (
        "icons/list-tree.svg",
        include_bytes!("../assets/icons/list-tree.svg"),
    ),
    (
        "icons/map-pin.svg",
        include_bytes!("../assets/icons/map-pin.svg"),
    ),
    (
        "icons/message-square.svg",
        include_bytes!("../assets/icons/message-square.svg"),
    ),
    (
        "icons/notebook-pen.svg",
        include_bytes!("../assets/icons/notebook-pen.svg"),
    ),
    (
        "icons/package.svg",
        include_bytes!("../assets/icons/package.svg"),
    ),
    (
        "icons/panel-left.svg",
        include_bytes!("../assets/icons/panel-left.svg"),
    ),
    (
        "icons/search.svg",
        include_bytes!("../assets/icons/search.svg"),
    ),
    (
        "icons/sliders-horizontal.svg",
        include_bytes!("../assets/icons/sliders-horizontal.svg"),
    ),
    (
        "icons/square-check-big.svg",
        include_bytes!("../assets/icons/square-check-big.svg"),
    ),
    (
        "icons/users.svg",
        include_bytes!("../assets/icons/users.svg"),
    ),
    ("icons/x.svg", include_bytes!("../assets/icons/x.svg")),
];

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_icon_loads_and_unknown_paths_are_empty() {
        for (name, _) in ICONS {
            assert!(
                Assets.load(name).expect("asset load").is_some(),
                "missing embedded icon {name}"
            );
        }
        assert!(Assets.load("icons/nope.svg").expect("asset load").is_none());
    }
}
