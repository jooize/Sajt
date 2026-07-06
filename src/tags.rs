use std::path::Path;

/// A macOS Finder tag: a name plus its color index (0-7).
///
/// Finder stores each tag as `"name\nN"` in the `_kMDItemUserTags` xattr, where
/// `N` is the color index the user picked in Finder. The index maps to the seven
/// Finder colors: 0 none, 1 gray, 2 green, 3 purple, 4 blue, 5 yellow, 6 red,
/// 7 orange. We keep the index so the site can render each tag in its Finder
/// color (see `stats::finder_color_var`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub color: u8,
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
