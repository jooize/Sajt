use crate::postdate::PostDate;

/// Parsed URL query: date filter, tag filter, label lookup.
#[derive(Debug, Default, Clone)]
pub struct ContentQuery {
    /// Date filter in compact hierarchy form: "2026", "2026-03", or
    /// "2026-03-25" (year / year-month / year-month-day). Assembled from the
    /// slash-separated path segments; matched as a prefix of the timestamp.
    pub date_prefix: Option<String>,
    /// Time-of-day disambiguator (`HHMMSS`, or any left-anchored prefix like
    /// `14` or `1430`). A URL **path segment** now (`/2026/07/04/191430`), after
    /// a full day — only same-day slug collisions and unlabeled entries need it.
    pub time: Option<String>,
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
    /// Check if an entry matches this query. `slug` is the entry's URL slug (the
    /// address projection of its name); the query label is slugified before
    /// comparison, so `/Fog%20Over%20The%20Bay` matches `fog-over-the-bay`.
    pub fn matches(&self, timestamp: &PostDate, slug: &Option<String>, tags: &[String]) -> bool {
        // Date prefix filter — compare against the padded, precision-aware date.
        if let Some(ref prefix) = self.date_prefix {
            if !timestamp.match_string().starts_with(prefix.as_str()) {
                return false;
            }
        }

        // Time-of-day disambiguator (left-anchored prefix of HHMMSS).
        if let Some(ref time) = self.time {
            if !timestamp.hms().starts_with(time.as_str()) {
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

        // Label filter — compared on the slug (the address projection), so a
        // mixed-case or spaced URL segment resolves to the same entry.
        if let Some(ref query_label) = self.label {
            let want = crate::slug::slug(query_label);
            match (slug, want) {
                (Some(entry_slug), Some(w)) if *entry_slug == w => {}
                _ => return false,
            }
        }

        true
    }
}

/// Parse a URL path into a ContentQuery.
///
/// Dates are a slash hierarchy consumed from the front of the path; tags and a
/// label follow. There is no hyphenated-date form and no `T`-timestamp segment —
/// sub-day disambiguation is carried by `?time=` (set separately by the caller).
///
/// Examples:
///   `/`                       → timeline (all entries)
///   `/2026`                   → year filter
///   `/2026/03`                → year + month filter
///   `/2026/03/25/`            → that day (listing)
///   `/2026/03/25/sunset`      → date + label
///   `/2026/03/25/sunset.jpg`  → date + raw file request
///   `/2026/03/04.txt`         → date + raw file for an unlabeled entry
///   `/sunset`                 → bare label
///   `/+amusing`               → tag filter
///   `/+amusing+personal`      → AND tags
///   `/+amusing,personal`      → OR tags
///   `/2026/03/+amusing`       → date + tag
pub fn parse_url_path(path: &str) -> ContentQuery {
    let mut query = ContentQuery::default();

    let path = path.trim_start_matches('/');

    if path.is_empty() {
        query.is_listing = true;
        return query;
    }

    // Trailing slash → listing.
    if path.ends_with('/') {
        query.is_listing = true;
    }

    let path = path.trim_end_matches('/');
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    // --- Consume the leading date hierarchy: year [/ month [/ day]] ---
    // Each rung may carry a file extension (`04.txt`), which ends the date and
    // records a raw-file request for an otherwise unlabeled entry.
    let mut idx = 0;
    let mut date = String::new();
    let mut has_day = false;

    if let Some(seg) = segments.get(idx) {
        let (name, ext) = split_ext(seg);
        if is_year(name) {
            date.push_str(name);
            idx += 1;
            if let Some(e) = ext {
                query.raw_extension = Some(e.to_string());
            } else if let Some(seg) = segments.get(idx) {
                let (name, ext) = split_ext(seg);
                if in_range(name, 1, 12) {
                    date.push('-');
                    date.push_str(name);
                    idx += 1;
                    if let Some(e) = ext {
                        query.raw_extension = Some(e.to_string());
                    } else if let Some(seg) = segments.get(idx) {
                        let (name, ext) = split_ext(seg);
                        if in_range(name, 1, 31) {
                            date.push('-');
                            date.push_str(name);
                            idx += 1;
                            has_day = true;
                            if let Some(e) = ext {
                                query.raw_extension = Some(e.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    if !date.is_empty() {
        query.date_prefix = Some(date);
    }

    // After a full Y/M/D, a purely-numeric segment is the time-of-day disambiguator
    // (`/2026/07/04/191430`, or a left-anchored prefix) — a path segment now, not
    // `?time=`. It may carry a raw-file extension for an unlabeled entry.
    if has_day && query.raw_extension.is_none() {
        if let Some(seg) = segments.get(idx) {
            let (name, ext) = split_ext(seg);
            if !name.is_empty() && name.len() <= 6 && name.bytes().all(|b| b.is_ascii_digit()) {
                query.time = Some(name.to_string());
                idx += 1;
                if let Some(e) = ext {
                    query.raw_extension = Some(e.to_string());
                }
            }
        }
    }

    // --- Remaining segments: tags (`+…`) and a single label ---
    for segment in &segments[idx..] {
        if segment.starts_with('+') {
            parse_tag_segment(&segment[1..], &mut query);
        } else if let Some(dot_pos) = segment.rfind('.') {
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

    query
}

/// Split a trailing `.ext` off a segment. Returns `(name, Some(ext))` only when
/// both sides are non-empty; otherwise `(segment, None)`.
fn split_ext(seg: &str) -> (&str, Option<&str>) {
    if let Some(pos) = seg.rfind('.') {
        let (name, dotext) = seg.split_at(pos);
        let ext = &dotext[1..];
        if !name.is_empty() && !ext.is_empty() {
            return (name, Some(ext));
        }
    }
    (seg, None)
}

/// A 4-digit year, optionally BCE with a leading `-` (`2026`, `-3000`).
fn is_year(s: &str) -> bool {
    let digits = s.strip_prefix('-').unwrap_or(s);
    digits.len() == 4 && digits.bytes().all(|b| b.is_ascii_digit())
}

/// A zero-padded 2-digit number within `[lo, hi]` (month 1..=12, day 1..=31).
fn in_range(s: &str, lo: u8, hi: u8) -> bool {
    s.len() == 2
        && s.bytes().all(|b| b.is_ascii_digit())
        && s.parse::<u8>().map_or(false, |v| (lo..=hi).contains(&v))
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

    fn ts(s: &str) -> PostDate {
        PostDate::from_mtime(NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H%M%S").unwrap())
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
        let q = parse_url_path("/2026/03");
        assert_eq!(q.date_prefix.as_deref(), Some("2026-03"));
        assert!(q.label.is_none());
    }

    #[test]
    fn day_filter() {
        let q = parse_url_path("/2026/03/25");
        assert_eq!(q.date_prefix.as_deref(), Some("2026-03-25"));
        assert!(q.label.is_none());
    }

    #[test]
    fn date_with_label() {
        let q = parse_url_path("/2026/03/25/sunset");
        assert_eq!(q.date_prefix.as_deref(), Some("2026-03-25"));
        assert_eq!(q.label.as_deref(), Some("sunset"));
        assert!(q.raw_extension.is_none());
    }

    #[test]
    fn month_then_label() {
        // A non-day segment after the month is the label, not a date rung.
        let q = parse_url_path("/2026/03/sunset");
        assert_eq!(q.date_prefix.as_deref(), Some("2026-03"));
        assert_eq!(q.label.as_deref(), Some("sunset"));
    }

    #[test]
    fn bare_label() {
        let q = parse_url_path("/hello-world");
        assert!(q.date_prefix.is_none());
        assert_eq!(q.label.as_deref(), Some("hello-world"));
    }

    #[test]
    fn raw_file_request() {
        let q = parse_url_path("/2026/03/25/sunset.jpg");
        assert_eq!(q.date_prefix.as_deref(), Some("2026-03-25"));
        assert_eq!(q.label.as_deref(), Some("sunset"));
        assert_eq!(q.raw_extension.as_deref(), Some("jpg"));
    }

    #[test]
    fn raw_file_for_unlabeled_entry() {
        // The day rung carries the extension; there is no label.
        let q = parse_url_path("/2026/03/04.txt");
        assert_eq!(q.date_prefix.as_deref(), Some("2026-03-04"));
        assert!(q.label.is_none());
        assert_eq!(q.raw_extension.as_deref(), Some("txt"));
    }

    #[test]
    fn time_is_a_path_segment() {
        // After a full day, a numeric segment is the time-of-day.
        let q = parse_url_path("/2026/07/04/191430");
        assert_eq!(q.date_prefix.as_deref(), Some("2026-07-04"));
        assert_eq!(q.time.as_deref(), Some("191430"));
        assert!(q.label.is_none());

        // Time plus a slug (a same-day collision's disambiguated address).
        let q = parse_url_path("/2026/07/04/191430/fog-over-the-bay");
        assert_eq!(q.time.as_deref(), Some("191430"));
        assert_eq!(q.label.as_deref(), Some("fog-over-the-bay"));

        // A left-anchored prefix works as a coarse disambiguator.
        let q = parse_url_path("/2026/07/04/1914");
        assert_eq!(q.time.as_deref(), Some("1914"));
    }

    #[test]
    fn bce_year_filter() {
        let q = parse_url_path("/-3000");
        assert_eq!(q.date_prefix.as_deref(), Some("-3000"));
        assert!(q.label.is_none());
    }

    #[test]
    fn invalid_month_becomes_label() {
        let q = parse_url_path("/2026/13");
        assert_eq!(q.date_prefix.as_deref(), Some("2026"));
        assert_eq!(q.label.as_deref(), Some("13"));
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
        let q = parse_url_path("/2026/03/+amusing");
        assert_eq!(q.date_prefix.as_deref(), Some("2026-03"));
        assert_eq!(q.and_tags, vec!["amusing"]);
    }

    #[test]
    fn trailing_slash_listing() {
        let q = parse_url_path("/2026/03/25/");
        assert!(q.is_listing);
        assert_eq!(q.date_prefix.as_deref(), Some("2026-03-25"));
    }

    #[test]
    fn matches_date_prefix_year() {
        let q = parse_url_path("/2026");
        assert!(q.matches(&ts("2026-03-03T143052"), &None, &[]));
        assert!(!q.matches(&ts("2025-12-31T235959"), &None, &[]));
    }

    #[test]
    fn matches_date_prefix_month() {
        let q = parse_url_path("/2026/03");
        assert!(q.matches(&ts("2026-03-03T143052"), &None, &[]));
        assert!(!q.matches(&ts("2026-04-01T000000"), &None, &[]));
    }

    #[test]
    fn matches_date_prefix_day() {
        let q = parse_url_path("/2026/03/03");
        assert!(q.matches(&ts("2026-03-03T143052"), &None, &[]));
        // A different day in the same month must not match (guards the T boundary).
        assert!(!q.matches(&ts("2026-03-30T143052"), &None, &[]));
    }

    #[test]
    fn matches_time_disambiguator() {
        let mut q = parse_url_path("/2026/03/12/cookie-consent-tests");
        q.time = Some("133513".to_string());
        assert!(q.matches(&ts("2026-03-12T133513"), &Some("cookie-consent-tests".into()), &[]));
        assert!(!q.matches(&ts("2026-03-12T170005"), &Some("cookie-consent-tests".into()), &[]));
    }

    #[test]
    fn matches_label() {
        let q = parse_url_path("/2026/03/03/sunset");
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
    fn raw_markdown_source() {
        let q = parse_url_path("/2026/03/03/hello-world.md");
        assert_eq!(q.label.as_deref(), Some("hello-world"));
        assert_eq!(q.raw_extension.as_deref(), Some("md"));
    }
}
