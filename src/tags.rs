use std::path::Path;

/// A macOS Finder tag: a name plus its color index (0-7).
///
/// Finder stores each tag as `"name\nN"` in the `_kMDItemUserTags` xattr, where
/// `N` is the color index the user picked in Finder. The index maps to the seven
/// Finder colors: 0 none, 1 gray, 2 green, 3 purple, 4 blue, 5 yellow, 6 red,
/// 7 orange. We keep the index so the site can render each tag in its Finder
/// color: it is emitted as a `data-tag-color` attribute the stylesheet paints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub color: u8,
}

impl Tag {
    /// The `public` visibility tag — the allow half of the fail-closed gate.
    /// `public-original` is a `public` that also publishes the exact bytes, so it
    /// counts as public for visibility (post-model.md §8).
    pub fn is_public(&self) -> bool {
        self.name.eq_ignore_ascii_case("public")
            || self.name.eq_ignore_ascii_case("public-original")
    }
    /// The `private` visibility tag — the deny half; it wins over `public`.
    pub fn is_private(&self) -> bool {
        self.name.eq_ignore_ascii_case("private")
    }
    /// The `public-original` tag: publish this image with its embedded metadata
    /// intact (EXIF/GPS and all) — the author's explicit opt-out of the strip.
    /// Compound-with-`public` on purpose: there is no dangling "original" whose
    /// visibility is ambiguous, and the name states the publish consequence.
    pub fn is_original(&self) -> bool {
        self.name.eq_ignore_ascii_case("public-original")
    }
}

/// Fail-closed visibility for a filesystem path: served **iff every path
/// component** from `root` (exclusive) down to `path` (inclusive) is tagged
/// `public` **and none** is tagged `private` (deny-wins AND up the whole chain).
///
/// This is the master gate (see `post-model.md` §6 / the [review] amendment):
/// untagged is not served, `private` hides a subtree, and a mistagged ancestor
/// hides everything beneath it. `path` must sit inside `root`; anything outside
/// is not visible (fail closed). Reads tags per component via the same read-only
/// accessor as everywhere else — the content tree is never written.
pub fn path_visible(root: &Path, path: &Path) -> bool {
    let rel = match path.strip_prefix(root) {
        Ok(r) => r,
        Err(_) => return false, // outside the content root — fail closed
    };
    let mut cur = root.to_path_buf();
    for comp in rel.components() {
        cur.push(comp);
        let tags = read_tags_colored(&cur);
        let public = tags.iter().any(Tag::is_public);
        let private = tags.iter().any(Tag::is_private);
        if private || !public {
            return false;
        }
    }
    true
}

/// Parse one raw `_kMDItemUserTags` entry (`"name\nN"`) into a `Tag`.
fn parse_tag_entry(raw: &str) -> Tag {
    match raw.split_once('\n') {
        Some((name, color)) => Tag {
            name: name.to_string(),
            color: color.trim().parse().unwrap_or(0),
        },
        None => Tag {
            name: raw.to_string(),
            color: 0,
        },
    }
}

/// Read the raw string array out of the `_kMDItemUserTags` xattr plist.
fn read_raw_tags(path: &Path) -> Vec<String> {
    let attr = match xattr::get(path, "com.apple.metadata:_kMDItemUserTags") {
        Ok(Some(data)) => data,
        _ => return Vec::new(),
    };

    let value: plist::Value = match plist::from_bytes(&attr) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    match value.as_array() {
        Some(array) => array
            .iter()
            .filter_map(|v| v.as_string().map(str::to_string))
            .collect(),
        None => Vec::new(),
    }
}

/// Read macOS xattr tags with their Finder colors.
///
/// This is the only tag accessor: the server is read-only on the content tree,
/// so there is deliberately no tag-writing counterpart.
pub fn read_tags_colored(path: &Path) -> Vec<Tag> {
    read_raw_tags(path).iter().map(|s| parse_tag_entry(s)).collect()
}

/// Read the macOS Finder comment (`kMDItemFinderComment`) as plain text.
///
/// This is the Spotlight comment a user types in Finder's Get Info panel. Finder
/// stores it as a binary-plist string in the
/// `com.apple.metadata:kMDItemFinderComment` extended attribute — the same class
/// of per-file metadata as the tag xattr above, and portable the same way
/// (`rsync -X`). We read only this per-file xattr, never the parent folder's
/// `.DS_Store` copy, so a comment on an unpublished neighbor can never surface.
/// Read-only, like every content accessor; fails closed to `None` on any error
/// so a malformed value yields no description rather than garbage.
///
/// Returns the trimmed comment, or `None` when absent, unreadable, or empty.
pub fn read_finder_comment(path: &Path) -> Option<String> {
    let attr = match xattr::get(path, "com.apple.metadata:kMDItemFinderComment") {
        Ok(Some(data)) => data,
        _ => return None,
    };
    let value: plist::Value = plist::from_bytes(&attr).ok()?;
    let comment = value.as_string()?.trim();
    if comment.is_empty() {
        None
    } else {
        Some(comment.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn read_tags_nonexistent_file() {
        let tags = read_tags_colored(&PathBuf::from("/nonexistent/file.txt"));
        assert!(tags.is_empty());
    }

    #[test]
    fn parse_tag_with_color() {
        assert_eq!(parse_tag_entry("design\n3"), Tag { name: "design".into(), color: 3 });
    }

    #[test]
    fn parse_tag_without_color() {
        assert_eq!(parse_tag_entry("design"), Tag { name: "design".into(), color: 0 });
    }

    #[test]
    fn parse_tag_bad_color_defaults_to_zero() {
        assert_eq!(parse_tag_entry("design\nx"), Tag { name: "design".into(), color: 0 });
    }
}
