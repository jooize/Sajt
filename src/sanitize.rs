//! HTML sanitization — the single home for the two allowlist policies that
//! stand between untrusted HTML and the trusted page shell.
//!
//! Both back onto `ammonia` (a real html5ever-backed sanitizer), so neither
//! depends on the fragile string-scanning the hand-rolled sanitizer used to do:
//! mixed-case `</ScRiPt>`, split/uppercase attributes, UTF-8 boundaries, and
//! unquoted `on*=` handlers are all handled by the parser, not by us.
//!
//! Trust model — two distinct contexts:
//!
//! * [`body`] — the rendered post body. pandoc turns the author's Markdown/RST/…
//!   into HTML, but pandoc passes *raw* embedded HTML straight through (its
//!   `-raw_html` reader extension is a no-op in pandoc 3.7), so a dropped-in
//!   `<script>` would otherwise reach the trusted origin. `body` strips every
//!   script vector while preserving the structural markup the page relies on
//!   (syntax-highlight `class`es, footnote/​heading `id`s, table alignment).
//!
//! * [`oembed`] — third-party embed HTML fetched from Twitter/Bluesky/Mastodon.
//!   This is the least-trusted input we render, so it gets a *tight* allowlist:
//!   only the handful of inline/text tags a quoted post needs, no attributes but
//!   `href`, and `rel="noopener noreferrer nofollow"` forced onto every link.
//!
//! Neither policy permits inline `style`, `<style>`, `<iframe>`, event handlers,
//! or `javascript:`/`data:` URLs, so both are clean under a strict CSP
//! (`script-src 'self'; style-src 'self'`). Table alignment — the one piece of
//! pandoc output that arrives as an inline `style` — is rewritten to a
//! `data-align` attribute before sanitizing (see [`rewrite_table_align`]) so the
//! external stylesheet can honor it without any inline style surviving.

use ammonia::Builder;
use std::sync::LazyLock;

/// Allowlist for rendered post bodies: pandoc's structural output survives,
/// every script vector is removed. Built once and shared (`clean` takes `&self`).
static BODY: LazyLock<Builder<'static>> = LazyLock::new(|| {
    let mut b = Builder::default();
    // Pandoc leans on `class` (syntax highlighting, footnotes) and `id`
    // (heading anchors, footnote targets); keep them. They carry no script.
    b.add_generic_attributes(["class", "id"]);
    // Table cell alignment, rewritten out of inline `style` into `data-align`.
    b.add_tag_attributes("td", ["data-align"]);
    b.add_tag_attributes("th", ["data-align"]);
    // Pixel image sizing arrives as plain attributes (CSP-clean); keep them.
    // (Percentage widths arrive as inline `style` and are intentionally dropped
    // — images fall back to the responsive `max-width` in the stylesheet.)
    b.add_tag_attributes("img", ["width", "height", "loading", "alt"]);
    b
});

/// Allowlist for third-party oEmbed HTML: only what a quoted post needs.
static OEMBED: LazyLock<Builder<'static>> = LazyLock::new(|| {
    let mut b = Builder::default();
    b.tags(
        [
            "blockquote", "p", "a", "br", "b", "strong", "i", "em", "span", "ul", "ol", "li",
        ]
        .into_iter()
        .collect(),
    );
    // No `class`/`id`/`style` reach us from a third party; `href` on `<a>` is a
    // default tag attribute and is all we keep. Force safe link rels.
    b.link_rel(Some("noopener noreferrer nofollow"));
    b
});

/// Sanitize a rendered post body (pandoc output) for the trusted page shell.
pub fn body(html: &str) -> String {
    let prepared = rewrite_table_align(html);
    BODY.clean(&prepared).to_string()
}

/// Sanitize third-party oEmbed HTML down to a tight text/inline allowlist.
pub fn oembed(html: &str) -> String {
    OEMBED.clean(html).to_string()
}

/// Rewrite pandoc's table-cell alignment from an inline `style` (which a strict
/// `style-src 'self'` would refuse to apply) into a `data-align` attribute the
/// external stylesheet targets. Pandoc's output for aligned cells is exactly
/// ` style="text-align: <dir>;"`, so a precise substring swap is deterministic;
/// the `table_align_rewrite` test pins the format against pandoc upgrades.
fn rewrite_table_align(html: &str) -> String {
    html.replace(" style=\"text-align: left;\"", " data-align=\"left\"")
        .replace(" style=\"text-align: center;\"", " data-align=\"center\"")
        .replace(" style=\"text-align: right;\"", " data-align=\"right\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_removes_scripts_and_handlers_keeps_structure() {
        let input = r#"<h1 id="t">T</h1><p>x<script>alert(1)</script><b onclick="e()">y</b></p>
<div class="sourceCode"><span class="kw">fn</span></div>
<a href="javascript:evil()">bad</a> <a href="https://ok.example">ok</a>
<iframe src="https://evil.example"></iframe><style>*{}</style>"#;
        let out = body(input);
        assert!(!out.contains("script"), "script tag/word must be gone: {out}");
        assert!(!out.contains("onclick"));
        assert!(!out.contains("<iframe"));
        assert!(!out.contains("<style"));
        assert!(!out.contains("javascript:"));
        // Structural markup pandoc depends on survives.
        assert!(out.contains(r#"id="t""#));
        assert!(out.contains(r#"class="sourceCode""#));
        assert!(out.contains(r#"class="kw""#));
        assert!(out.contains(r#"href="https://ok.example""#));
    }

    #[test]
    fn body_drops_inline_style_but_keeps_pixel_dims() {
        let input = r#"<img src="/a.png" style="width:50.0%" alt="a">
<img src="/b.png" width="200" height="100" alt="b">"#;
        let out = body(input);
        assert!(!out.contains("style="), "no inline style survives: {out}");
        assert!(out.contains(r#"width="200""#));
        assert!(out.contains(r#"height="100""#));
    }

    #[test]
    fn table_align_rewrite() {
        // The exact shape pandoc 3.7 emits for aligned cells (cells must sit in a
        // real table or the HTML parser foster-parents them away).
        let input = r#"<table><thead><tr><th style="text-align: left;">L</th></tr></thead><tbody><tr><td style="text-align: right;">R</td><td style="text-align: center;">C</td></tr></tbody></table>"#;
        let out = body(input);
        assert!(out.contains(r#"data-align="left""#), "{out}");
        assert!(out.contains(r#"data-align="right""#), "{out}");
        assert!(out.contains(r#"data-align="center""#), "{out}");
        assert!(!out.contains("text-align"), "inline align style gone: {out}");
    }

    #[test]
    fn oembed_tightens_to_text_and_links() {
        let input = r#"<blockquote class="twitter-tweet"><p>Hello <a href="https://t.co/x">l</a> <b onclick="s()">w</b></p></blockquote>
<script async src="https://platform.twitter.com/widgets.js"></script>
<img src="x" onerror="alert(1)"><style>*{}</style>"#;
        let out = oembed(input);
        assert!(!out.contains("script"));
        assert!(!out.contains("onclick"));
        assert!(!out.contains("onerror"));
        assert!(!out.contains("<img"));
        assert!(!out.contains("<style"));
        assert!(!out.contains("twitter-tweet"), "class stripped: {out}");
        assert!(out.contains("<blockquote>"));
        assert!(out.contains("Hello"));
        assert!(out.contains(r#"rel="noopener noreferrer nofollow""#));
    }

    #[test]
    fn mixed_case_script_terminator_handled() {
        // The hand-rolled sanitizer missed `</ScRiPt>`; the parser does not.
        assert_eq!(body(r#"a<ScRiPt>x</ScRiPt>b"#).replace('\n', ""), "ab");
    }
}
