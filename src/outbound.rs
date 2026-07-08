//! The single outbound-destination guard (`post-model.md` §7).
//!
//! Every external destination — a link post's `link_url`, a `<cite>` line, and
//! every `<a href>` in rendered body HTML — funnels through [`classify_scheme`].
//! It is an **allowlist** (fail closed): only `https`, `http`, `mailto`, and
//! `tel` reach the reader as clickable links. Anything else (`javascript:`,
//! `data:`, `vbscript:`, `file:`, and any custom URL handler) is refused at
//! render time and rendered as flagged plain text, never an anchor — because
//! CSS cannot stop navigation and those schemes execute in our origin, read the
//! reader's disk, or invoke arbitrary local handlers. Widening the allowlist is
//! a one-line, deliberate edit here.

/// How a destination's scheme is classified against the allowlist.
///
/// Normalization matches what a browser does before it parses a scheme: tab,
/// newline and carriage-return are stripped from anywhere in the string, and
/// leading/trailing C0 controls and spaces are trimmed. Only then is the scheme
/// read (the run of `[a-zA-Z0-9+.-]` before the first `:`, with no `/ \ ? #`
/// intervening — otherwise there is no scheme and the reference is relative).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scheme {
    /// No scheme: a fragment (`#x`), query, or relative/absolute path. A
    /// same-origin reference that never leaves the site — kept verbatim, no
    /// `rel`/flagging. (Protocol-relative `//host` is *not* this — see `Refused`.)
    Relative,
    /// `https://` — allowed, secure, web (external; gets `rel="noreferrer"`).
    HttpsWeb,
    /// `http://` — allowed but plaintext; additionally flagged in the UI via the
    /// pure-CSS `a[href^="http://"]` rule (no server-set class). External web.
    HttpWeb,
    /// `mailto:` / `tel:` — allowed; a non-web handler with no referrer to leak,
    /// so no `rel`/`target` rewriting is applied.
    Handler,
    /// Anything else, including protocol-relative `//host`. Refused: never
    /// rendered as a clickable anchor. Carries the offending scheme (lowercased,
    /// best-effort; `"//"` for protocol-relative) for the notice.
    Refused(String),
}

impl Scheme {
    /// Whether this destination may be emitted as a clickable anchor at all.
    pub fn is_allowed(&self) -> bool {
        !matches!(self, Scheme::Refused(_))
    }

    /// Whether this is an external web link (`http`/`https`) — the case that
    /// leaves the site and so earns `rel="noreferrer"` for reader privacy.
    pub fn is_web_external(&self) -> bool {
        matches!(self, Scheme::HttpsWeb | Scheme::HttpWeb)
    }
}

/// A valid URL scheme token: an ASCII letter, then letters/digits/`+`/`-`/`.`.
fn is_scheme_token(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
}

/// Decode the HTML entities a scheme could be obfuscated with, matching what a
/// browser's tokenizer does inside an attribute value — so `&#106avascript:`
/// (a **numeric** reference with the semicolon *omitted*, which browsers still
/// decode to `javascript:`) classifies as `javascript:`, not as a relative URL.
/// Only enough to make scheme detection honest; not a general HTML unescaper.
///
/// Numeric references (`&#106`, `&#x6a`) decode with the semicolon optional, as
/// browsers do; the digit/hex-digit run is consumed greedily. Named references
/// (`&amp;`, `&lt;`, …) require the semicolon (browsers leave a bare named
/// reference literal in attributes, and named references can't spell a scheme in
/// any case).
fn decode_entities(s: &str) -> String {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'&' {
            out.push(b[i]);
            i += 1;
            continue;
        }
        match parse_entity(&b[i..]) {
            Some((c, consumed)) => {
                let mut buf = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                i += consumed;
            }
            None => {
                out.push(b'&');
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Parse one HTML entity beginning at `rest[0] == b'&'`. Returns the decoded
/// character and the number of bytes consumed, or `None` if `rest` does not
/// begin a recognized entity.
fn parse_entity(rest: &[u8]) -> Option<(char, usize)> {
    debug_assert_eq!(rest.first(), Some(&b'&'));
    if rest.get(1) == Some(&b'#') {
        // Numeric reference: `&#<dec>` or `&#x<hex>`, trailing ';' optional.
        let hex = matches!(rest.get(2), Some(b'x') | Some(b'X'));
        let digits_start = if hex { 3 } else { 2 };
        let mut j = digits_start;
        if hex {
            while j < rest.len() && rest[j].is_ascii_hexdigit() {
                j += 1;
            }
        } else {
            while j < rest.len() && rest[j].is_ascii_digit() {
                j += 1;
            }
        }
        if j == digits_start {
            return None; // `&#` with no digits.
        }
        // Safe: the run is ASCII digits/hex-digits only.
        let digits = std::str::from_utf8(&rest[digits_start..j]).ok()?;
        let code = if hex {
            u32::from_str_radix(digits, 16).ok()?
        } else {
            digits.parse::<u32>().ok()?
        };
        let mut consumed = j;
        if rest.get(j) == Some(&b';') {
            consumed += 1;
        }
        let c = char::from_u32(code)?;
        Some((c, consumed))
    } else {
        // Named reference: require the terminating ';'.
        let semi = rest.iter().position(|&c| c == b';')?;
        let name = std::str::from_utf8(&rest[1..semi]).ok()?;
        let c = match name {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ => return None,
        };
        Some((c, semi + 1))
    }
}

/// Classify a destination against the scheme allowlist (fail closed).
///
/// The input is the raw href/URL as authored; entities are decoded and browser
/// URL normalization (strip tab/newline/CR, trim leading/trailing controls and
/// spaces) is applied before the scheme is read, so obfuscations classify the
/// same way the browser would resolve them.
pub fn classify_scheme(raw: &str) -> Scheme {
    // Decode entities first (defense in depth against `&#106;avascript:`), then
    // apply the browser's URL normalization.
    let decoded = decode_entities(raw);
    let stripped: String = decoded
        .chars()
        .filter(|&c| c != '\t' && c != '\n' && c != '\r')
        .collect();
    let probe = stripped.trim_matches(|c: char| c.is_ascii_control() || c == ' ');

    if probe.is_empty() {
        return Scheme::Relative;
    }
    if probe.starts_with("//") {
        // Protocol-relative: no explicit scheme, but it *does* leave same-origin.
        // The §4 predicate never produces one; refuse it anyway (fail closed).
        return Scheme::Refused("//".to_string());
    }

    // A scheme exists only if a run of valid scheme chars precedes the first
    // ':' with no path/query/fragment separator intervening.
    match probe.find(|c: char| c == ':' || c == '/' || c == '?' || c == '#' || c == '\\') {
        Some(idx) if probe.as_bytes()[idx] == b':' && is_scheme_token(&probe[..idx]) => {
            let scheme = probe[..idx].to_ascii_lowercase();
            match scheme.as_str() {
                "https" => Scheme::HttpsWeb,
                "http" => Scheme::HttpWeb,
                "mailto" | "tel" => Scheme::Handler,
                other => Scheme::Refused(other.to_string()),
            }
        }
        // No scheme separator before a path/query/fragment char (or no ':' at
        // all): a relative reference.
        _ => Scheme::Relative,
    }
}

/// Rewrite every `<a href>` in a rendered HTML fragment so no unsafe scheme can
/// survive as a clickable link, and every external web link carries
/// `rel="noreferrer"` for reader privacy (`post-model.md` §7).
///
/// - `https`/`http` external links keep their href (the browser normalizes tab/
///   newline exactly as [`classify_scheme`] did) and gain `rel="noreferrer"`
///   unless they already declare a `rel`. `http` links are additionally flagged
///   by the `a[href^="http://"]` CSS rule.
/// - `mailto:`/`tel:` and relative/same-origin links are left untouched.
/// - Refused schemes are neutralized: the `<a>…</a>` becomes
///   `<span data-unsafe-link title="…">…</span>`, keeping the (already-escaped)
///   link text but removing all clickability.
///
/// The scan is a single pass that respects attribute quoting and never nests
/// (HTML anchors cannot nest), so inner markup is preserved verbatim. It is
/// deliberately conservative: anything it cannot positively classify as safe
/// keeps flowing through as text, and only positively-recognized anchors are
/// touched.
pub fn sanitize_body_links(html: &str) -> String {
    let bytes = html.as_bytes();
    let mut out = String::with_capacity(html.len() + 64);
    let mut i = 0;
    // Stack of "was the currently-open anchor rewritten into a span?"; depth is
    // ≤1 for valid HTML but a stack keeps stray/nested tags from desyncing.
    let mut anchor_stack: Vec<bool> = Vec::new();

    while i < bytes.len() {
        if bytes[i] == b'<' {
            // Closing anchor?
            if let Some(end) = match_close_anchor(bytes, i) {
                match anchor_stack.pop() {
                    Some(true) => out.push_str("</span>"),
                    Some(false) => out.push_str("</a>"),
                    None => out.push_str(&html[i..end]), // stray </a>, pass through
                }
                i = end;
                continue;
            }
            // Opening anchor?
            if let Some(tag_end) = match_open_anchor(bytes, i) {
                let tag = &html[i..tag_end]; // "<a ...>"
                let inner = &html[i + 2..tag_end - 1]; // attributes only
                match anchor_href(inner) {
                    Some(href) => {
                        let scheme = classify_scheme(&href);
                        if scheme.is_allowed() {
                            if scheme.is_web_external() && !has_rel_attr(inner) {
                                out.push_str(&tag[..tag.len() - 1]);
                                out.push_str(" rel=\"noreferrer\">");
                            } else {
                                out.push_str(tag);
                            }
                            anchor_stack.push(false);
                        } else {
                            let sch = match &scheme {
                                Scheme::Refused(s) => s.as_str(),
                                _ => unreachable!(),
                            };
                            out.push_str("<span data-unsafe-link title=\"Unsafe link removed (");
                            out.push_str(&attr_escape(sch));
                            out.push_str(":)\">");
                            anchor_stack.push(true);
                        }
                    }
                    None => {
                        // Anchor with no href (a named anchor) — leave it.
                        out.push_str(tag);
                        anchor_stack.push(false);
                    }
                }
                i = tag_end;
                continue;
            }
        }
        // Copy this byte through (UTF-8 continuation bytes flow through untouched
        // since we only branch on ASCII '<').
        let ch_len = utf8_len(bytes[i]);
        out.push_str(&html[i..i + ch_len]);
        i += ch_len;
    }

    out
}

/// Length in bytes of the UTF-8 sequence beginning with `b`.
fn utf8_len(b: u8) -> usize {
    match b {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

/// If `bytes[i..]` begins an `<a>` open tag, return the index just past its `>`.
/// Requires `<a` followed by whitespace, `>`, or `/` (so `<abbr>` never matches),
/// and respects quoted attribute values when finding the `>`.
fn match_open_anchor(bytes: &[u8], i: usize) -> Option<usize> {
    if i + 2 > bytes.len() || bytes[i] != b'<' || !eq_ascii_lower(bytes[i + 1], b'a') {
        return None;
    }
    let after = *bytes.get(i + 2)?;
    if !(after.is_ascii_whitespace() || after == b'>' || after == b'/') {
        return None;
    }
    let mut j = i + 2;
    let mut quote: Option<u8> = None;
    while j < bytes.len() {
        let c = bytes[j];
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                b'"' | b'\'' => quote = Some(c),
                b'>' => return Some(j + 1),
                _ => {}
            },
        }
        j += 1;
    }
    None
}

/// If `bytes[i..]` begins a `</a>` close tag, return the index just past its `>`.
fn match_close_anchor(bytes: &[u8], i: usize) -> Option<usize> {
    if i + 3 > bytes.len()
        || bytes[i] != b'<'
        || bytes[i + 1] != b'/'
        || !eq_ascii_lower(bytes[i + 2], b'a')
    {
        return None;
    }
    let mut j = i + 3;
    while j < bytes.len() && bytes[j].is_ascii_whitespace() {
        j += 1;
    }
    if j < bytes.len() && bytes[j] == b'>' {
        Some(j + 1)
    } else {
        None
    }
}

fn eq_ascii_lower(a: u8, lower: u8) -> bool {
    a.to_ascii_lowercase() == lower
}

/// Extract the raw (still HTML-escaped) value of the `href` attribute from an
/// open tag's attribute string, if present. Handles double/single quoted and
/// unquoted values; the attribute name match is case-insensitive and anchored to
/// a word boundary so `data-href` never matches.
fn anchor_href(attrs: &str) -> Option<String> {
    let bytes = attrs.as_bytes();
    let lower = attrs.to_ascii_lowercase();
    let mut search_from = 0;
    while let Some(rel) = lower[search_from..].find("href") {
        let pos = search_from + rel;
        search_from = pos + 4;
        // Word boundary before "href".
        if pos > 0 {
            let prev = bytes[pos - 1];
            if !(prev.is_ascii_whitespace() || prev == b'/') {
                continue;
            }
        }
        // After the name: optional whitespace then '='.
        let mut k = pos + 4;
        while k < bytes.len() && bytes[k].is_ascii_whitespace() {
            k += 1;
        }
        if k >= bytes.len() || bytes[k] != b'=' {
            continue;
        }
        k += 1;
        while k < bytes.len() && bytes[k].is_ascii_whitespace() {
            k += 1;
        }
        if k >= bytes.len() {
            return Some(String::new());
        }
        let value = match bytes[k] {
            q @ (b'"' | b'\'') => {
                let start = k + 1;
                let end = attrs[start..].find(q as char).map(|o| start + o)?;
                &attrs[start..end]
            }
            _ => {
                let start = k;
                let end = attrs[start..]
                    .find(|c: char| c.is_ascii_whitespace())
                    .map(|o| start + o)
                    .unwrap_or(attrs.len());
                &attrs[start..end]
            }
        };
        return Some(value.to_string());
    }
    None
}

/// Whether the open tag already declares a `rel` attribute (so we don't add a
/// duplicate). Word-boundary anchored like [`anchor_href`].
fn has_rel_attr(attrs: &str) -> bool {
    let bytes = attrs.as_bytes();
    let lower = attrs.to_ascii_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find("rel") {
        let pos = from + rel;
        from = pos + 3;
        if pos > 0 {
            let prev = bytes[pos - 1];
            if !(prev.is_ascii_whitespace() || prev == b'/') {
                continue;
            }
        }
        let mut k = pos + 3;
        while k < bytes.len() && bytes[k].is_ascii_whitespace() {
            k += 1;
        }
        if k < bytes.len() && bytes[k] == b'=' {
            return true;
        }
    }
    false
}

/// Minimal attribute-context escape for the scheme name shown in the title text.
fn attr_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_https_mailto_tel_and_relative() {
        assert_eq!(classify_scheme("https://example.com/x"), Scheme::HttpsWeb);
        assert_eq!(classify_scheme("HTTPS://Example.com"), Scheme::HttpsWeb);
        assert_eq!(classify_scheme("http://example.com"), Scheme::HttpWeb);
        assert_eq!(classify_scheme("mailto:me@example.com"), Scheme::Handler);
        assert_eq!(classify_scheme("tel:+15551234567"), Scheme::Handler);
        assert_eq!(classify_scheme("/local/path"), Scheme::Relative);
        assert_eq!(classify_scheme("#footnote-1"), Scheme::Relative);
        assert_eq!(classify_scheme("../sibling"), Scheme::Relative);
        assert_eq!(classify_scheme("page.html?a=b:c"), Scheme::Relative);
        assert_eq!(classify_scheme(""), Scheme::Relative);
    }

    #[test]
    fn refuses_dangerous_and_unknown_schemes() {
        assert!(matches!(classify_scheme("javascript:alert(1)"), Scheme::Refused(s) if s == "javascript"));
        assert!(matches!(classify_scheme("JavaScript:alert(1)"), Scheme::Refused(_)));
        assert!(matches!(classify_scheme("data:text/html,<b>x"), Scheme::Refused(s) if s == "data"));
        assert!(matches!(classify_scheme("vbscript:msgbox"), Scheme::Refused(_)));
        assert!(matches!(classify_scheme("file:///etc/passwd"), Scheme::Refused(s) if s == "file"));
        assert!(matches!(classify_scheme("myapp://open"), Scheme::Refused(_)));
        assert!(matches!(classify_scheme("//evil.example/x"), Scheme::Refused(s) if s == "//"));
    }

    #[test]
    fn defeats_whitespace_and_entity_obfuscation() {
        // Browsers strip tab/newline/CR anywhere; so must we.
        assert!(matches!(classify_scheme("java\tscript:alert(1)"), Scheme::Refused(_)));
        assert!(matches!(classify_scheme("java\nscript:alert(1)"), Scheme::Refused(_)));
        assert!(matches!(classify_scheme("  \n javascript:alert(1)"), Scheme::Refused(_)));
        // Entity-encoded scheme, semicolon present.
        assert!(matches!(classify_scheme("&#106;avascript:alert(1)"), Scheme::Refused(_)));
        assert!(matches!(classify_scheme("&#x6a;avascript:alert(1)"), Scheme::Refused(_)));
        // Numeric reference with the semicolon OMITTED — browsers still decode
        // the digit run, so we must too (S1 regression).
        assert!(matches!(classify_scheme("&#106avascript:alert(1)"), Scheme::Refused(_)));
        assert!(matches!(classify_scheme("&#0000106avascript:alert(1)"), Scheme::Refused(_)));
        // And the neutralizer must strip such an anchor, not pass it through.
        let out = sanitize_body_links(r#"<a href="&#106avascript:alert(1)">x</a>"#);
        assert!(!out.contains("<a "), "no anchor may survive: {out}");
        assert!(out.contains("data-unsafe-link"), "{out}");
    }

    #[test]
    fn sanitize_neutralizes_javascript_anchor() {
        let html = r#"<p><a href="javascript:alert(1)">click me</a></p>"#;
        let out = sanitize_body_links(html);
        assert!(!out.contains("<a "), "no anchor should survive: {out}");
        assert!(!out.contains("javascript:alert"));
        assert!(out.contains("data-unsafe-link"));
        assert!(out.contains("click me"));
        assert!(out.contains("</span>"));
        assert!(!out.contains("</a>"));
    }

    #[test]
    fn sanitize_adds_noreferrer_to_external_and_leaves_internal() {
        let html = r##"<a href="https://example.com">x</a> and <a href="#sec">y</a>"##;
        let out = sanitize_body_links(html);
        assert!(out.contains(r#"<a href="https://example.com" rel="noreferrer">x</a>"#), "{out}");
        // Internal fragment link untouched (no rel injected).
        assert!(out.contains(r##"<a href="#sec">y</a>"##), "{out}");
    }

    #[test]
    fn sanitize_leaves_mailto_and_existing_rel() {
        let html = r#"<a href="mailto:a@b.com">mail</a><a href="https://x.com" rel="nofollow">z</a>"#;
        let out = sanitize_body_links(html);
        assert!(out.contains(r#"<a href="mailto:a@b.com">mail</a>"#), "{out}");
        // Existing rel is preserved, not duplicated.
        assert_eq!(out.matches("rel=").count(), 1, "{out}");
    }

    #[test]
    fn sanitize_preserves_inner_markup_and_abbr_tags() {
        let html = r#"<p>See <abbr title="x">A</abbr> and <a href="https://e.com/x"><code>foo</code></a>.</p>"#;
        let out = sanitize_body_links(html);
        assert!(out.contains("<abbr title=\"x\">A</abbr>"), "abbr untouched: {out}");
        assert!(out.contains("<code>foo</code>"), "inner markup kept: {out}");
        assert!(out.contains("rel=\"noreferrer\""), "{out}");
    }

    #[test]
    fn sanitize_handles_uppercase_and_gt_inside_quoted_href() {
        // Uppercase tag/attr, and a '>' inside the quoted href value must not
        // fool the tag-end scan.
        let html = r#"<A HREF="https://e.com/a?x=1>2">t</A>"#;
        let out = sanitize_body_links(html);
        assert!(out.contains(r#"HREF="https://e.com/a?x=1>2""#), "href preserved: {out}");
        assert!(out.contains("rel=\"noreferrer\""), "rel added: {out}");
        assert!(out.contains("</a>") || out.contains("</A>"), "close kept: {out}");
    }

    #[test]
    fn sanitize_ignores_data_href_and_named_anchor() {
        // `data-href` is not `href`; a named anchor (no href) is left alone.
        let html = r#"<a data-href="javascript:x">n</a><a name="top">m</a>"#;
        let out = sanitize_body_links(html);
        assert!(out.contains(r#"<a data-href="javascript:x">n</a>"#), "{out}");
        assert!(out.contains(r#"<a name="top">m</a>"#), "{out}");
    }

    #[test]
    fn sanitize_neutralizes_unquoted_and_protocol_relative() {
        let html = "<a href=javascript:alert(1)>x</a><a href=//evil.example/y>z</a>";
        let out = sanitize_body_links(html);
        assert!(!out.contains("<a "), "no anchor survives: {out}");
        assert_eq!(out.matches("data-unsafe-link").count(), 2, "{out}");
    }

    #[test]
    fn sanitize_handles_http_flagged_and_singlequotes() {
        let html = r#"<a href='http://old.example/g'>old</a>"#;
        let out = sanitize_body_links(html);
        // http kept (CSS flags it), rel added.
        assert!(out.contains("href='http://old.example/g'"), "{out}");
        assert!(out.contains("rel=\"noreferrer\""), "{out}");
    }
}
