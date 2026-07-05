use crate::tags::Tag;
use chrono::NaiveDateTime;
use std::path::PathBuf;

/// Tags the site treats as machinery, not topics — never shown in the cloud or
/// the row rail. `favorite` drives the ★, `public`/`private` drive visibility,
/// and `Do…` are one-shot action tags the scanner consumes.
pub fn is_reserved_tag(name: &str) -> bool {
    matches!(name.to_ascii_lowercase().as_str(), "public" | "private" | "favorite")
        || name.starts_with("Do")
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub timestamp: NaiveDateTime,
    pub label: Option<String>,
    /// Auto-generated display label (e.g. "@handle · date" for social embeds).
    /// Templates use display_label.as_ref().or(label.as_ref()) for display.
    pub display_label: Option<String>,
    pub extension: String,
    pub tags: Vec<Tag>,
    /// Pairwise-grade percentile in `0.0..=1.0`, or `None` until the entry has
    /// been graded. The grading flow is not built yet, so this is always `None`
    /// today; the quality meter and level filter read it when it exists.
    pub grade: Option<f32>,
}

impl Entry {
    /// Whether the author marked this entry a favorite (the `favorite` tag → ★).
    pub fn is_favorite(&self) -> bool {
        self.tags.iter().any(|t| t.name.eq_ignore_ascii_case("favorite"))
    }

    /// Tags shown to readers: everything that isn't machinery, in file order.
    pub fn topical_tags(&self) -> impl Iterator<Item = &Tag> {
        self.tags.iter().filter(|t| !is_reserved_tag(&t.name))
    }

    /// Tag names (all of them) for URL/query matching, which is name-based.
    pub fn tag_names(&self) -> Vec<String> {
        self.tags.iter().map(|t| t.name.clone()).collect()
    }

    /// The entry's *type*, derived from the file itself — never a tag. Search
    /// matches it, so typing "photo" finds photos with zero extra UI.
    pub fn kind(&self) -> &'static str {
        match self.extension.to_ascii_lowercase().as_str() {
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "avif" | "heic" | "heif" | "tiff" | "bmp" => "photo",
            "html" | "htm" => "page",
            "link" => "link",
            "" | "/" => "folder",
            "md" | "markdown" | "txt" | "text" | "adoc" | "asciidoc" | "rst" | "org" | "tex" => "note",
            _ => "file",
        }
    }
}

/// Parse a filename like `2026-03-03T143052_sunset.md` into components.
///
/// Timestamp is always 17 chars: `YYYY-MM-DDTHHMMSS`
/// Label after first `_`, optional.
/// Extension from last `.`
/// Returns None if the filename doesn't match the convention.
pub fn parse_filename(filename: &str) -> Option<(NaiveDateTime, Option<String>, String)> {
    // Must be at least 17 chars for the timestamp
    if filename.len() < 17 {
        return None;
    }

    let ts_str = &filename[..17];
    let timestamp = NaiveDateTime::parse_from_str(ts_str, "%Y-%m-%dT%H%M%S").ok()?;

    let rest = &filename[17..];

    // Find the extension (last `.` in the rest portion)
    let ext_pos = rest.rfind('.')?;
    let extension = rest[ext_pos + 1..].to_string();
    if extension.is_empty() {
        return None;
    }

    // Everything between timestamp and extension is the label area
    let label_area = &rest[..ext_pos];

    let label = if label_area.starts_with('_') && label_area.len() > 1 {
        Some(label_area[1..].to_string())
    } else if label_area.is_empty() {
        None
    } else {
        // Unexpected characters between timestamp and extension
        return None;
    };

    Some((timestamp, label, extension))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_filename() {
        let (ts, label, ext) = parse_filename("2026-03-03T143052_sunset.md").unwrap();
        assert_eq!(ts, NaiveDateTime::parse_from_str("2026-03-03T143052", "%Y-%m-%dT%H%M%S").unwrap());
        assert_eq!(label.as_deref(), Some("sunset"));
        assert_eq!(ext, "md");
    }

    #[test]
    fn parse_no_label() {
        let (ts, label, ext) = parse_filename("2026-03-04T091500.txt").unwrap();
        assert_eq!(ts, NaiveDateTime::parse_from_str("2026-03-04T091500", "%Y-%m-%dT%H%M%S").unwrap());
        assert_eq!(label, None);
        assert_eq!(ext, "txt");
    }

    #[test]
    fn parse_image_with_original_name() {
        let (_, label, ext) = parse_filename("2026-03-03T150000_IMG_4392.jpg").unwrap();
        assert_eq!(label.as_deref(), Some("IMG_4392"));
        assert_eq!(ext, "jpg");
    }

    #[test]
    fn parse_compound_label() {
        let (_, label, ext) = parse_filename("2026-03-03T143052_hello-world.md").unwrap();
        assert_eq!(label.as_deref(), Some("hello-world"));
        assert_eq!(ext, "md");
    }

    #[test]
    fn reject_too_short() {
        assert!(parse_filename("short.md").is_none());
    }

    #[test]
    fn reject_bad_timestamp() {
        assert!(parse_filename("2026-13-03T143052_sunset.md").is_none());
    }

    #[test]
    fn reject_no_extension() {
        assert!(parse_filename("2026-03-03T143052_sunset").is_none());
    }

    #[test]
    fn reject_garbage_between_timestamp_and_ext() {
        assert!(parse_filename("2026-03-03T143052xyz.md").is_none());
    }
}
