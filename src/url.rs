use chrono::NaiveDateTime;

/// Parsed URL query: date filter, tag filter, label lookup.
#[derive(Debug, Default, Clone)]
pub struct ContentQuery {
    /// Date prefix to filter by (variable precision)
    pub date_prefix: Option<String>,
    /// Tags combined with AND (from `+tag1+tag2`)
    pub and_tags: Vec<String>,
    /// Tags combined with OR (from `+tag1,tag2`)
    pub or_tags: Vec<String>,
    /// Label to match
    pub label: Option<String>,
    /// Whether this is a listing request (trailing slash)
    pub is_listing: bool,
    /// Raw file extension requested (e.g., "jpg", "md") — serve raw bytes
    pub raw_extension: Option<String>,
}

impl ContentQuery {
    /// Check if an entry matches this query.
    pub fn matches(&self, timestamp: &NaiveDateTime, label: &Option<String>, tags: &[String]) -> bool {
        // Date prefix filter
        if let Some(ref prefix) = self.date_prefix {
            let ts_str = format_timestamp_for_prefix(timestamp, prefix.len());
            if !ts_str.starts_with(prefix.as_str()) {
                return false;
            }
        }

        // AND tags: entry must have ALL of them
        if !self.and_tags.is_empty() {
            let lower_tags: Vec<String> = tags.iter().map(|t| t.to_lowercase()).collect();
            for tag in &self.and_tags {
                if !lower_tags.contains(&tag.to_lowercase()) {
                    return false;
                }
            }
        }

        // OR tags: entry must have AT LEAST ONE
        if !self.or_tags.is_empty() {
            let lower_tags: Vec<String> = tags.iter().map(|t| t.to_lowercase()).collect();
            let has_any = self.or_tags.iter().any(|tag| lower_tags.contains(&tag.to_lowercase()));
            if !has_any {
                return false;
            }
        }

        // Label filter
        if let Some(ref query_label) = self.label {
            match label {
                Some(entry_label) => {
                    if entry_label.to_lowercase() != query_label.to_lowercase() {
                        return false;
                    }
                }
                None => return false,
            }
        }

        true
    }
}

/// Format a timestamp to match against a prefix of a given length.
fn format_timestamp_for_prefix(ts: &NaiveDateTime, prefix_len: usize) -> String {
    // Full format: 2026-03-03T143052
    let full = ts.format("%Y-%m-%dT%H%M%S").to_string();
    full[..full.len().min(prefix_len + 5)].to_string()
}

/// Parse a URL path into a ContentQuery.
///
/// Examples:
///   `/` → timeline (all entries)
///   `/2026-03` → date filter
///   `/2026-03-03/sunset` → date + label
///   `/2026-03-03/sunset.jpg` → raw file request
///   `/+amusing` → tag filter
///   `/+amusing+personal` → AND tags
///   `/+amusing,personal` → OR tags
///   `/2026-03/+amusing` → date + tag
pub fn parse_url_path(path: &str) -> ContentQuery {
    let mut query = ContentQuery::default();

    let path = path.trim_start_matches('/');

    if path.is_empty() {
        query.is_listing = true;
        return query;
    }

    // Check for trailing slash → listing
    if path.ends_with('/') {
        query.is_listing = true;
    }

    let path = path.trim_end_matches('/');
    let segments: Vec<&str> = path.split('/').collect();

    for segment in &segments {
        if segment.is_empty() {
            continue;
        }

        if segment.starts_with('+') {
            // Tag segment
            parse_tag_segment(&segment[1..], &mut query);
        } else if is_date_segment(segment) {
            // Date segment — accumulate into date_prefix
            match &query.date_prefix {
                Some(existing) => {
                    query.date_prefix = Some(format!("{}/{}", existing, segment));
                }
                None => {
                    query.date_prefix = Some(segment.to_string());
                }
            }
        } else {
            // Label or raw file — check for extension
            if let Some(dot_pos) = segment.rfind('.') {
                let name = &segment[..dot_pos];
                let ext = &segment[dot_pos + 1..];
                if !ext.is_empty() && !name.is_empty() {
                    query.label = Some(name.to_string());
                    query.raw_extension = Some(ext.to_string());
                } else {
                    query.label = Some(segment.to_string());
                }
            } else {
                query.label = Some(segment.to_string());
            }
        }
    }

    // If we have a date_prefix, normalize it to the compact timestamp format
    if let Some(ref prefix) = query.date_prefix {
        query.date_prefix = Some(normalize_date_prefix(prefix));
    }

    query
}

/// Detect whether a URL segment looks like a date/time component.
fn is_date_segment(segment: &str) -> bool {
    let len = segment.len();
    // Year: 2026
    // Year-month: 2026-03
    // Full date: 2026-03-03
    // Date+hour: 2026-03-03T14
    // Date+minute: 2026-03-03T1430
    // Full timestamp: 2026-03-03T143052
    matches!(len, 4 | 7 | 10 | 13 | 15 | 17)
        && segment.as_bytes()[0].is_ascii_digit()
        && (len <= 4 || segment.as_bytes()[4] == b'-')
}

/// Normalize a date prefix to compact timestamp format for matching.
/// "2026-03" stays "2026-03", "2026-03-03" stays "2026-03-03", etc.
fn normalize_date_prefix(prefix: &str) -> String {
    // If there's a slash (from multi-segment dates like "2026-03-03/sunset"),
    // we don't have that case since labels are handled separately.
    prefix.to_string()
}

/// Parse tag segment: `amusing+personal` → AND, `amusing,personal` → OR
fn parse_tag_segment(segment: &str, query: &mut ContentQuery) {
    if segment.contains(',') {
        // OR tags
        query.or_tags.extend(segment.split(',').filter(|s| !s.is_empty()).map(String::from));
    } else if segment.contains('+') {
        // AND tags (split on +)
        query.and_tags.extend(segment.split('+').filter(|s| !s.is_empty()).map(String::from));
    } else if !segment.is_empty() {
        // Single tag → AND
        query.and_tags.push(segment.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDateTime;

    fn ts(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H%M%S").unwrap()
    }

    #[test]
    fn root_is_listing() {
        let q = parse_url_path("/");
        assert!(q.is_listing);
        assert!(q.date_prefix.is_none());
        assert!(q.label.is_none());
    }

    #[test]
    fn year_filter() {
        let q = parse_url_path("/2026");
        assert_eq!(q.date_prefix.as_deref(), Some("2026"));
        assert!(q.label.is_none());
    }

    #[test]
    fn month_filter() {
        let q = parse_url_path("/2026-03");
        assert_eq!(q.date_prefix.as_deref(), Some("2026-03"));
    }

    #[test]
    fn date_with_label() {
        let q = parse_url_path("/2026-03-03/sunset");
        assert_eq!(q.date_prefix.as_deref(), Some("2026-03-03"));
        assert_eq!(q.label.as_deref(), Some("sunset"));
        assert!(q.raw_extension.is_none());
    }

    #[test]
    fn raw_file_request() {
        let q = parse_url_path("/2026-03-03/sunset.jpg");
        assert_eq!(q.label.as_deref(), Some("sunset"));
        assert_eq!(q.raw_extension.as_deref(), Some("jpg"));
    }

    #[test]
    fn single_tag() {
        let q = parse_url_path("/+amusing");
        assert_eq!(q.and_tags, vec!["amusing"]);
    }

    #[test]
    fn and_tags() {
        let q = parse_url_path("/+amusing+personal");
        assert_eq!(q.and_tags, vec!["amusing", "personal"]);
    }

    #[test]
    fn or_tags() {
        let q = parse_url_path("/+amusing,personal");
        assert_eq!(q.or_tags, vec!["amusing", "personal"]);
    }

    #[test]
    fn date_with_tag() {
        let q = parse_url_path("/2026-03/+amusing");
        assert_eq!(q.date_prefix.as_deref(), Some("2026-03"));
        assert_eq!(q.and_tags, vec!["amusing"]);
    }

    #[test]
    fn trailing_slash_listing() {
        let q = parse_url_path("/2026-03-03/");
        assert!(q.is_listing);
        assert_eq!(q.date_prefix.as_deref(), Some("2026-03-03"));
    }

    #[test]
    fn matches_date_prefix_year() {
        let q = parse_url_path("/2026");
        assert!(q.matches(&ts("2026-03-03T143052"), &None, &[]));
        assert!(!q.matches(&ts("2025-12-31T235959"), &None, &[]));
    }

    #[test]
    fn matches_date_prefix_month() {
        let q = parse_url_path("/2026-03");
        assert!(q.matches(&ts("2026-03-03T143052"), &None, &[]));
        assert!(!q.matches(&ts("2026-04-01T000000"), &None, &[]));
    }

    #[test]
    fn matches_label() {
        let q = parse_url_path("/2026-03-03/sunset");
        assert!(q.matches(&ts("2026-03-03T143052"), &Some("sunset".into()), &[]));
        assert!(!q.matches(&ts("2026-03-03T143052"), &Some("other".into()), &[]));
        assert!(!q.matches(&ts("2026-03-03T143052"), &None, &[]));
    }

    #[test]
    fn matches_and_tags() {
        let q = parse_url_path("/+amusing+personal");
        let tags = vec!["amusing".into(), "personal".into(), "other".into()];
        assert!(q.matches(&ts("2026-03-03T143052"), &None, &tags));

        let tags_missing = vec!["amusing".into()];
        assert!(!q.matches(&ts("2026-03-03T143052"), &None, &tags_missing));
    }

    #[test]
    fn matches_or_tags() {
        let q = parse_url_path("/+amusing,personal");
        let tags = vec!["amusing".into()];
        assert!(q.matches(&ts("2026-03-03T143052"), &None, &tags));

        let tags_none: Vec<String> = vec!["unrelated".into()];
        assert!(!q.matches(&ts("2026-03-03T143052"), &None, &tags_none));
    }

    #[test]
    fn full_timestamp_url() {
        let q = parse_url_path("/2026-03-03T143052");
        assert_eq!(q.date_prefix.as_deref(), Some("2026-03-03T143052"));
    }

    #[test]
    fn timestamp_with_label() {
        let q = parse_url_path("/2026-03-03T143052/sunset");
        assert_eq!(q.date_prefix.as_deref(), Some("2026-03-03T143052"));
        assert_eq!(q.label.as_deref(), Some("sunset"));
    }

    #[test]
    fn raw_markdown_source() {
        let q = parse_url_path("/2026-03-03/hello-world.md");
        assert_eq!(q.label.as_deref(), Some("hello-world"));
        assert_eq!(q.raw_extension.as_deref(), Some("md"));
    }
}
