use chrono::NaiveDateTime;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub timestamp: NaiveDateTime,
    pub label: Option<String>,
    /// Auto-generated display label (e.g. "@handle · date" for social embeds).
    /// Templates use display_label.as_ref().or(label.as_ref()) for display.
    pub display_label: Option<String>,
    pub extension: String,
    pub tags: Vec<String>,
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
