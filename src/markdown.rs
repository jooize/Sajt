//! CommonMark rendering, in-process with comrak.
//!
//! The extension set mirrors what Pandoc's `commonmark_x` gave posts before
//! the port (tables, footnotes, strikeout, task lists, definition lists,
//! super/subscript, `$` math, `:emoji:` codes, GitHub alerts, autolinked bare
//! URLs, smart punctuation), so existing posts render as they did. Raw HTML
//! is passed through here on purpose and neutralised by `sanitize::body`
//! afterwards: the sanitizer is the one trust boundary for author markup,
//! whichever engine produced the body.
//!
//! Two shapes are adjusted after comrak so the page scripts and stylesheet
//! see one markup regardless of engine:
//!
//! * heading ids come from the site's own slug recipe (`slug.rs`), the same
//!   one the in-page anchor script mirrors, so `#section` links resolve
//!   without JavaScript and agree with what the script would have minted;
//! * an image alone in its paragraph becomes a `<figure>` with the alt text
//!   as its caption (Pandoc's implicit figures), and fenced code is
//!   highlighted by `highlight::code_blocks`.

use std::collections::HashSet;
use std::fmt;
use std::sync::Mutex;

use comrak::adapters::{HeadingAdapter, HeadingMeta};
use comrak::nodes::Sourcepos;
use comrak::options::Plugins;
use comrak::Options;

/// Render a CommonMark document to an HTML fragment (unsanitized: the
/// caller runs `sanitize::body`).
pub fn to_html(source: &str) -> String {
    let mut options = Options::default();
    let ext = &mut options.extension;
    ext.strikethrough = true;
    ext.table = true;
    ext.autolink = true;
    ext.tasklist = true;
    ext.superscript = true;
    ext.subscript = true;
    ext.footnotes = true;
    ext.description_lists = true;
    ext.math_dollars = true;
    ext.shortcodes = true;
    ext.alerts = true;
    options.parse.smart = true;
    // Raw HTML survives this stage; `sanitize::body` strips what must not
    // reach the trusted origin and keeps the harmless rest (a `<details>`,
    // a `<span lang>`), exactly as with Pandoc before.
    options.render.r#unsafe = true;

    let headings = HeadingIds::default();
    let mut plugins = Plugins::default();
    plugins.render.heading_adapter = Some(&headings);

    let html = comrak::markdown_to_html_with_plugins(source, &options, &plugins);
    let html = implicit_figures(&html);
    crate::highlight::code_blocks(&html)
}

/// Heading renderer: `<hN id="slug">`, ids unique within one document with
/// the same `-2`, `-3` suffixes the client script uses.
#[derive(Default)]
struct HeadingIds {
    seen: Mutex<HashSet<String>>,
}

impl HeadingAdapter for HeadingIds {
    fn enter(
        &self,
        output: &mut dyn fmt::Write,
        heading: &HeadingMeta,
        _sourcepos: Option<Sourcepos>,
    ) -> fmt::Result {
        let base = crate::slug::slug(&heading.content).unwrap_or_else(|| "section".to_string());
        let mut seen = self.seen.lock().unwrap_or_else(|e| e.into_inner());
        let mut id = base.clone();
        let mut n = 2;
        while seen.contains(&id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        seen.insert(id.clone());
        write!(output, "<h{} id=\"{}\">", heading.level, id)
    }

    fn exit(&self, output: &mut dyn fmt::Write, heading: &HeadingMeta) -> fmt::Result {
        writeln!(output, "</h{}>", heading.level)
    }
}

/// Pandoc's implicit figures: a paragraph holding nothing but one image with
/// alt text becomes `<figure><img ...><figcaption>alt</figcaption></figure>`.
/// comrak's own figure option wraps every image, inline ones included, which
/// html5ever would then split out of their paragraph; this is the narrower
/// rule. Only comrak's exact `<p><img ... /></p>` shape matches.
fn implicit_figures(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(i) = rest.find("<p><img ") {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(figure) = figure_at(rest) else {
            out.push_str("<p>");
            rest = &rest["<p>".len()..];
            continue;
        };
        out.push_str(&figure.html);
        rest = &rest[figure.len..];
    }
    out.push_str(rest);
    out
}

struct Figure {
    html: String,
    len: usize,
}

fn figure_at(s: &str) -> Option<Figure> {
    let img = s.strip_prefix("<p>")?;
    let tag_end = img.find("/>")?;
    let tag = &img[..tag_end + 2];
    // `s` starts with `<p><img `; a second `<` before the tag closes means
    // this is not a lone image tag.
    if tag[1..].contains('<') {
        return None;
    }
    let after = &img[tag_end + 2..];
    let after = after.strip_prefix("</p>")?;
    let alt_start = tag.find(" alt=\"")? + " alt=\"".len();
    let alt_end = alt_start + tag[alt_start..].find('"')?;
    let alt = &tag[alt_start..alt_end];
    if alt.is_empty() {
        return None;
    }
    Some(Figure {
        html: format!("<figure>{tag}<figcaption>{alt}</figcaption></figure>"),
        len: s.len() - after.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_carry_slug_ids_unique_per_document() {
        let html = to_html("# Pandoc's Port\n\n## Rust\n\n## Rust\n\n## !!!\n");
        assert!(html.contains(r#"<h1 id="pandocs-port">Pandoc’s Port</h1>"#), "{html}");
        assert!(html.contains(r#"<h2 id="rust">Rust</h2>"#), "{html}");
        assert!(html.contains(r#"<h2 id="rust-2">Rust</h2>"#), "{html}");
        assert!(html.contains(r#"<h2 id="section">!!!</h2>"#), "{html}");
        // A second document starts its own id space.
        assert!(to_html("## Rust\n").contains(r#"id="rust""#));
    }

    #[test]
    fn footnotes_render_gfm_shape() {
        let html = to_html("Hi[^x].\n\n[^x]: A note.\n");
        assert!(html.contains(r##"<sup class="footnote-ref"><a href="#fn-x" id="fnref-x""##), "{html}");
        assert!(html.contains(r#"<section class="footnotes""#), "{html}");
        assert!(html.contains(r#"class="footnote-backref""#), "{html}");
    }

    #[test]
    fn fenced_code_is_highlighted_in_the_shared_markup() {
        let html = to_html("```rust\nfn main() {}\n```\n\n```\nplain\n```\n");
        assert!(html.contains(r#"<div class="sourceCode"><pre class="sourceCode rust"><code class="sourceCode rust"><span><span class="kw">fn</span>"#), "{html}");
        assert!(html.contains("<pre><code>plain\n</code></pre>"), "{html}");
    }

    #[test]
    fn tables_smart_punctuation_autolinks_and_raw_html() {
        let html = to_html("| a | b |\n|:--|--:|\n| 1 | 2 |\n\n\"Quoted\" -- and https://example.org/x\n\n<details><summary>s</summary>t</details>\n\n<script>alert(1)</script>\n");
        assert!(html.contains(r#"<th align="left">a</th>"#), "{html}");
        assert!(html.contains(r#"<td align="right">2</td>"#), "{html}");
        assert!(html.contains("“Quoted” –"), "{html}");
        assert!(html.contains(r#"<a href="https://example.org/x">https://example.org/x</a>"#), "{html}");
        assert!(html.contains("<details>"), "raw HTML passes to the sanitizer: {html}");
        assert!(html.contains("<script>"), "and so does script, which the sanitizer removes: {html}");
    }

    #[test]
    fn lone_image_with_alt_becomes_a_figure() {
        let html = to_html("![A caption](a.png)\n\nText ![inline](b.png) more.\n\n![](c.png)\n");
        assert!(html.contains(r#"<figure><img src="a.png" alt="A caption" /><figcaption>A caption</figcaption></figure>"#), "{html}");
        assert!(html.contains(r#"<p>Text <img src="b.png" alt="inline" /> more.</p>"#), "{html}");
        assert!(html.contains(r#"<p><img src="c.png" alt="" /></p>"#), "{html}");
    }

    #[test]
    fn extensions_match_the_old_pandoc_set() {
        let html = to_html("~~gone~~ H~2~O x^2^ :tada:\n\n- [x] done\n\nTerm\n: Definition\n\n> [!NOTE]\n> Careful.\n\n$x_1$\n");
        assert!(html.contains("<del>gone</del>"), "{html}");
        assert!(html.contains("<sub>2</sub>"), "{html}");
        assert!(html.contains("<sup>2</sup>"), "{html}");
        assert!(html.contains("🎉"), "{html}");
        assert!(html.contains(r#"<input type="checkbox" checked="" disabled="" />"#), "{html}");
        assert!(html.contains("<dl>"), "{html}");
        assert!(html.contains("markdown-alert-note"), "{html}");
        assert!(html.contains(r#"<span data-math-style="inline">x_1</span>"#), "{html}");
    }
}
