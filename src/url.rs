use crate::postdate::PostDate;

/// Parsed URL query: date filter, tag filter, label lookup.
#[derive(Debug, Default, Clone)]
pub struct ContentQuery {
    /// Date filter in compact hierarchy form: "2026", "2026-03", or
    /// "2026-03-25" (year / year-month / year-month-day). Assembled from the
    /// slash-separated path segments; matched as a prefix of the timestamp.
    pub date_prefix: Option<String>,
    /// Time-of-day disambiguator, exactly `HHMMSS`. A URL **path segment**
    /// (`/2026/07/04/191430`), after a full day — only same-day slug collisions
    /// and unlabeled entries need it. Six digits and no other length: a shorter
    /// number there is a post's name, so the time is matched exactly, never as
    /// a prefix.
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
    /// View axis: notable-only (the reserved `/notable` path segment).
    pub notable: bool,
    /// View axis: favorites-only (the reserved `/favorites` path segment).
    pub favorites: bool,
    /// The clean-JPEG rendition of a raw file (`/photo.tif/jpeg`) — the
    /// pipeline's own output, addressed as a subresource of the file so the
    /// URL never lies about the bytes.
    pub rendition_jpeg: bool,
    /// The small gallery-tile rendition (`/photo.jpg/thumb`,
    /// `/photo.tif/jpeg/thumb`).
    pub rendition_thumb: bool,
    /// The path is not an address of this site at all: a segment that fits no
    /// part of the grammar. A post address is `/name`, or the date form
    /// `/[Y[/M[/D[/HHMMSS]]]]/name`, with tag segments and view words in any
    /// order — nothing else. A second name (`/x/brev`, `/2026/99/brev`: `99`
    /// is no month, so it is already the name), a BCE-shaped segment that is
    /// no year (`/-99`), or anything trailing a raw file but its rendition
    /// rungs (`/photo.tif/thumb/jpeg`) sets this, and the router answers a
    /// plain 404: the same reply as any missing path, so a malformed URL is
    /// no existence oracle either.
    ///
    /// [`ContentQuery::matches`] refuses everything while it is set, so a
    /// caller that forgets to check still cannot resolve a malformed path.
    pub malformed: bool,
}

impl ContentQuery {
    /// Check if an entry matches this query. `slug` is the entry's URL slug (the
    /// address projection of its name); the query label is slugified before
    /// comparison, so `/Fog%20Over%20The%20Bay` matches `fog-over-the-bay`.
    pub fn matches(&self, timestamp: &PostDate, slug: &Option<String>, tags: &[String]) -> bool {
        // A path that is not an address matches nothing, ever (fail closed).
        if self.malformed {
            return false;
        }

        // Date prefix filter — compare against the padded, precision-aware date.
        if let Some(ref prefix) = self.date_prefix {
            if !timestamp.match_string().starts_with(prefix.as_str()) {
                return false;
            }
        }

        // Time-of-day disambiguator: the whole `HHMMSS`, matched exactly.
        if let Some(ref time) = self.time {
            if timestamp.hms() != *time {
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

    /// The canonical path for a scope (a listing filtered by date, tags, and
    /// view — no label): date rungs, then one tag segment, then the view
    /// suffix. Tags are sorted case-insensitively, so every ordering of the
    /// same filter names one address ("sorting is complete at every depth" —
    /// sajt.md rung 3). `/` when nothing filters.
    pub fn canonical_scope_path(&self) -> String {
        let mut out = self.scope_base_path();
        if self.notable {
            push_segment(&mut out, "notable");
        }
        if self.favorites {
            push_segment(&mut out, "favorites");
        }
        out
    }

    /// The canonical scope path WITHOUT the view suffix — the base the header
    /// controls compose view links onto.
    pub fn scope_base_path(&self) -> String {
        let mut out = String::from("/");
        if let Some(ref prefix) = self.date_prefix {
            for rung in prefix.split('-') {
                // A BCE year is "-3000": splitting on '-' yields an empty first
                // chunk, so re-join it with its sign.
                if rung.is_empty() {
                    continue;
                }
                let seg = if prefix.starts_with('-') && out == "/" {
                    format!("-{}", rung)
                } else {
                    rung.to_string()
                };
                push_segment(&mut out, &seg);
            }
        }
        if let Some(ref time) = self.time {
            push_segment(&mut out, time);
        }
        if !self.and_tags.is_empty() {
            let mut tags = self.and_tags.clone();
            tags.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()).then_with(|| a.cmp(b)));
            push_segment(&mut out, &format!("+{}", tags.join("+")));
        }
        if !self.or_tags.is_empty() {
            let mut tags = self.or_tags.clone();
            tags.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()).then_with(|| a.cmp(b)));
            push_segment(&mut out, &format!("+{}", tags.join(",")));
        }
        out
    }
}

/// Minimal percent-decoding for callers that receive raw URLs (the `get`
/// subcommand, the closure builder; the server gets this from axum). Invalid
/// escapes pass through literally; invalid UTF-8 is replaced, never trusted.
pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(b) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Append one path segment, collapsing the root's slash ("/" + "x" → "/x").
fn push_segment(path: &mut String, segment: &str) {
    if !path.ends_with('/') {
        path.push('/');
    }
    path.push_str(segment);
}

/// Parse a URL path into a ContentQuery.
///
/// Segments are classified independently of position, so **every ordering of
/// the same filters parses to the same query** (sajt.md: "all orderings
/// are accepted; non-canonical orderings 301" — the caller compares against
/// `canonical_scope_path` and redirects). Classification per segment:
///
/// - `notable` / `favorites` (whole segment) → view flags. Reserved words: a
///   post with that name stays reachable at its date address.
/// - `+…` → tag segment (`+a+b` AND, `+a,b` OR).
/// - Date rungs by a state machine (year, then month, then day, then the
///   six-digit time-of-day disambiguator), wherever they appear.
/// - Anything else → the label, numbers included: a digit segment that fits
///   no rung is an ordinary name (`/42`, `/2026/13` is the post `13` dated
///   2026), because only a four-digit year, a valid month/day, and a
///   six-digit time belong to the hierarchy. A second name is no address.
/// - A trailing `.ext` on a date rung or label records a raw-file request.
///
/// Examples:
///   `/`                       → timeline (all entries)
///   `/2026/03/25/`            → that day (listing)
///   `/2026/03/25/sunset.jpg`  → date + raw file request
///   `/2026/03/04.txt`         → date + raw file for an unlabeled entry
///   `/2026/+design/notable`   → date + tag + view (canonical order)
///   `/+amusing,personal`      → OR tags
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

    // Date state machine: rungs unlock in order, but other segment kinds may
    // interleave. An extension on a rung ends the hierarchy (`/2026/03/04.txt`).
    let mut year: Option<String> = None;
    let mut month: Option<String> = None;
    let mut day: Option<String> = None;
    let mut dates_done = false;

    for segment in path.split('/').filter(|s| !s.is_empty()) {
        // Reserved view words (whole segment only — `notable.txt` is a file).
        if segment == "notable" {
            query.notable = true;
            continue;
        }
        if segment == "favorites" {
            query.favorites = true;
            continue;
        }

        if let Some(rest) = segment.strip_prefix('+') {
            parse_tag_segment(rest, &mut query);
            continue;
        }

        let (name, ext) = split_ext(segment);
        if !dates_done {
            if year.is_none() && is_year(name) {
                year = Some(name.to_string());
                if let Some(e) = ext {
                    query.raw_extension = Some(e.to_string());
                    dates_done = true;
                }
                continue;
            }
            if year.is_some() && month.is_none() && in_range(name, 1, 12) {
                month = Some(name.to_string());
                if let Some(e) = ext {
                    query.raw_extension = Some(e.to_string());
                    dates_done = true;
                }
                continue;
            }
            if month.is_some() && day.is_none() && in_range(name, 1, 31) {
                day = Some(name.to_string());
                if let Some(e) = ext {
                    query.raw_extension = Some(e.to_string());
                    dates_done = true;
                }
                continue;
            }
            // After a full Y/M/D, a six-digit segment is the time-of-day
            // disambiguator (`/2026/07/04/191430`). Exactly six: a shorter
            // number there is a post's name, and a coarser time would name a
            // set of posts rather than one address.
            if day.is_some()
                && query.time.is_none()
                && name.len() == 6
                && name.bytes().all(|b| b.is_ascii_digit())
            {
                query.time = Some(name.to_string());
                if let Some(e) = ext {
                    query.raw_extension = Some(e.to_string());
                    dates_done = true;
                }
                continue;
            }
            // A BCE-shaped segment (`-99`) is a year or it is nothing: the
            // sign belongs to the hierarchy, never to a name. Every other
            // number that fits no rung falls through to the label below, so a
            // post may simply be named `42`.
            if is_numeric(name) && name.starts_with('-') {
                query.malformed = true;
                continue;
            }
        }

        // Rendition subresources of a raw file, in canonical order only:
        // `jpeg` directly after the extension-carrying segment, `thumb` after
        // the extension or the `jpeg` rung. Any other spelling is a miss.
        if query.raw_extension.is_some() {
            if segment == "jpeg" && !query.rendition_jpeg && !query.rendition_thumb {
                query.rendition_jpeg = true;
                continue;
            }
            if segment == "thumb" && !query.rendition_thumb {
                query.rendition_thumb = true;
                continue;
            }
            // A raw file's address is complete: only its rendition rungs, in
            // their canonical order, may follow it.
            query.malformed = true;
            continue;
        }

        // The label; a trailing extension makes it a raw-file request. A post
        // address carries exactly one name, so a second one is no address.
        if query.label.is_some() {
            query.malformed = true;
            continue;
        }
        match ext {
            Some(e) if !name.is_empty() => {
                query.label = Some(name.to_string());
                query.raw_extension = Some(e.to_string());
            }
            _ => {
                query.label = Some(segment.to_string());
            }
        }
    }

    let mut date = String::new();
    if let Some(y) = year {
        date.push_str(&y);
        if let Some(m) = month {
            date.push('-');
            date.push_str(&m);
            if let Some(d) = day {
                date.push('-');
                date.push_str(&d);
            }
        }
    }
    if !date.is_empty() {
        query.date_prefix = Some(date);
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

/// Whether a segment is a bare number (a BCE year's leading `-` allowed) —
/// the shape the date hierarchy owns. Such a segment is a date rung or it is
/// nothing; it is never a post's name.
fn is_numeric(s: &str) -> bool {
    let digits = s.strip_prefix('-').unwrap_or(s);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
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

        // Exactly six digits: a shorter number after the day is a name.
        let q = parse_url_path("/2026/07/04/1914");
        assert!(q.time.is_none());
        assert_eq!(q.label.as_deref(), Some("1914"));
    }

    #[test]
    fn a_number_that_fits_no_rung_is_a_name() {
        // Numbers are ordinary names; only the rungs of the hierarchy (a
        // four-digit year, a valid month/day, a six-digit time) are the
        // router's. `/42` is the post named 42.
        let q = parse_url_path("/42");
        assert!(!q.malformed);
        assert!(q.date_prefix.is_none());
        assert_eq!(q.label.as_deref(), Some("42"));
        assert!(q.matches(&ts("2026-03-12T120000"), &Some("42".to_string()), &[]));

        // `13` is no month, so it is the name of a post dated 2026.
        let q = parse_url_path("/2026/13");
        assert!(!q.malformed);
        assert_eq!(q.date_prefix.as_deref(), Some("2026"));
        assert_eq!(q.label.as_deref(), Some("13"));

        // The same at every depth, including after a full day.
        let q = parse_url_path("/2026/03/25/42");
        assert_eq!(q.date_prefix.as_deref(), Some("2026-03-25"));
        assert_eq!(q.label.as_deref(), Some("42"));
        assert!(q.time.is_none());

        // A name is still exactly one: a second one is no address, and a
        // BCE-shaped segment that is no year belongs to the hierarchy.
        assert!(parse_url_path("/2026/99/brev").malformed, "99 is the name, brev a second one");
        assert!(parse_url_path("/-99").malformed);
        assert!(parse_url_path("/2026/-99").malformed);
        // The valid rungs are untouched, at every depth.
        assert!(!parse_url_path("/2026/brev").malformed);
        assert!(!parse_url_path("/2026/03/12/191430/brev").malformed);
        assert!(!parse_url_path("/-3000/brev").malformed);
    }

    #[test]
    fn bce_year_filter() {
        let q = parse_url_path("/-3000");
        assert_eq!(q.date_prefix.as_deref(), Some("-3000"));
        assert!(q.label.is_none());
    }

    #[test]
    fn a_post_address_carries_exactly_one_name() {
        // The strict rule (2026-09-10): `/x/brev` used to resolve to `brev`
        // and 301 there. A path with a stray segment names nothing now.
        for path in ["/x/brev", "/foo/bar/brev", "/brev/brev", "/notable/x/brev"] {
            assert!(parse_url_path(path).malformed, "{path} is not an address");
        }
        for path in ["/brev", "/brev/", "/notable/brev", "/+design/brev", "/2026/03/brev"] {
            assert!(!parse_url_path(path).malformed, "{path} is an address");
        }
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

    #[test]
    fn view_words_are_reserved_segments() {
        let q = parse_url_path("/notable");
        assert!(q.notable && !q.favorites);
        assert!(q.label.is_none());

        let q = parse_url_path("/favorites");
        assert!(q.favorites && !q.notable);

        let q = parse_url_path("/2026/+design/notable/favorites");
        assert!(q.notable && q.favorites);
        assert_eq!(q.date_prefix.as_deref(), Some("2026"));
        assert_eq!(q.and_tags, vec!["design"]);
        assert!(q.label.is_none());

        // Whole-segment only: an extension makes it a file of that name.
        let q = parse_url_path("/notable.txt");
        assert!(!q.notable);
        assert_eq!(q.label.as_deref(), Some("notable"));
        assert_eq!(q.raw_extension.as_deref(), Some("txt"));
    }

    #[test]
    fn all_orderings_parse_alike() {
        // Every permutation of the same filters yields the same query…
        let canon = parse_url_path("/2026/+design/notable");
        for path in [
            "/notable/2026/+design",
            "/+design/notable/2026",
            "/+design/2026/notable",
            "/2026/notable/+design",
        ] {
            let q = parse_url_path(path);
            assert_eq!(q.date_prefix, canon.date_prefix, "{path}");
            assert_eq!(q.and_tags, canon.and_tags, "{path}");
            assert_eq!(q.notable, canon.notable, "{path}");
            // …and every one serializes to the one canonical address.
            assert_eq!(q.canonical_scope_path(), "/2026/+design/notable", "{path}");
        }
    }

    #[test]
    fn split_date_rungs_reassemble() {
        // Date rungs unlock in order even with segments interleaved.
        let q = parse_url_path("/2026/+design/03");
        assert_eq!(q.date_prefix.as_deref(), Some("2026-03"));
        assert_eq!(q.canonical_scope_path(), "/2026/03/+design");
    }

    #[test]
    fn canonical_scope_paths() {
        assert_eq!(parse_url_path("/").canonical_scope_path(), "/");
        assert_eq!(parse_url_path("/2026/03/").canonical_scope_path(), "/2026/03");
        assert_eq!(
            parse_url_path("/favorites/notable").canonical_scope_path(),
            "/notable/favorites"
        );
        // AND tags sort case-insensitively inside one segment.
        assert_eq!(
            parse_url_path("/+zeta+Alpha").canonical_scope_path(),
            "/+Alpha+zeta"
        );
        assert_eq!(
            parse_url_path("/+b,a").canonical_scope_path(),
            "/+a,b"
        );
        assert_eq!(parse_url_path("/-3000").canonical_scope_path(), "/-3000");
    }

    #[test]
    fn rendition_path_segments() {
        let q = parse_url_path("/photo.tif/jpeg");
        assert_eq!(q.label.as_deref(), Some("photo"));
        assert_eq!(q.raw_extension.as_deref(), Some("tif"));
        assert!(q.rendition_jpeg && !q.rendition_thumb);

        let q = parse_url_path("/photo.tif/jpeg/thumb");
        assert!(q.rendition_jpeg && q.rendition_thumb);

        // A JPEG source's tile needs no /jpeg rung.
        let q = parse_url_path("/photo.jpg/thumb");
        assert!(!q.rendition_jpeg && q.rendition_thumb);

        // Renditions compose with the date address of an unlabeled file.
        let q = parse_url_path("/2026/03/04.tif/jpeg");
        assert_eq!(q.date_prefix.as_deref(), Some("2026-03-04"));
        assert!(q.rendition_jpeg);

        // Only the canonical order exists: /thumb/jpeg is not a rendition, and
        // nothing else may trail a raw file's address either.
        let q = parse_url_path("/photo.tif/thumb/jpeg");
        assert!(!q.rendition_jpeg);
        assert!(q.malformed);
        assert!(parse_url_path("/photo.tif/other").malformed);

        // Without a raw file there is no rendition — "jpeg" is a plain label.
        let q = parse_url_path("/jpeg");
        assert!(!q.rendition_jpeg);
        assert_eq!(q.label.as_deref(), Some("jpeg"));
    }

    #[test]
    fn scope_base_path_excludes_view() {
        let q = parse_url_path("/2026/+design/notable/favorites");
        assert_eq!(q.scope_base_path(), "/2026/+design");
        assert_eq!(q.canonical_scope_path(), "/2026/+design/notable/favorites");
    }
}
