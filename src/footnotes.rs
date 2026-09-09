//! Footnotes -- one markup for every authoring format.
//!
//! comrak emits footnotes in the GitHub shape, which the stylesheet themes and
//! the sidenote script pairs:
//!
//! ```html
//! text<sup class="footnote-ref"><a href="#fn-1" id="fnref-1">1</a></sup>
//! ...
//! <section class="footnotes">
//! <ol>
//! <li id="fn-1">
//! <p>The note. <a href="#fnref-1" class="footnote-backref">&#8617;</a></p>
//! </li>
//! </ol>
//! </section>
//! ```
//!
//! Asciidoctor has its own: a `<sup class="footnote">` holding a bracketed
//! link to `#_footnotedef_N`, and a `<div id="footnotes">` of `div.footnote`
//! definitions, each opening with a numbered link back to the reference.
//! [`normalize`] rewrites that into the shape above, so the client never
//! learns a second one: a note cited twice gets `fnref-N-2` and a second
//! back link, exactly as comrak numbers them.
//!
//! Like `highlight::code_blocks`, this is a post-pass over engine output and
//! fail-safe: markup that does not match the engine's exact shape is left
//! untouched, and the body still passes through `sanitize::body` afterwards,
//! so nothing produced here is trusted on its own.

use std::collections::HashMap;
use std::fmt::Write as _;

use crate::highlight::attr;

/// Rewrite Asciidoctor's footnote markup in `html` into the shared shape.
/// Output that already carries the shared shape passes through unchanged.
pub fn normalize(html: &str) -> String {
    // How many times each definition number has been cited so far, in
    // document order: the second citation of note 3 becomes `fnref-3-2`.
    let mut cited: HashMap<&str, u32> = HashMap::new();

    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(i) = rest.find("<sup class=\"footnote") {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        match parse_ref(rest) {
            Some(r) => {
                let n = cited.entry(r.number).or_insert(0);
                *n += 1;
                let _ = write!(
                    out,
                    "<sup class=\"footnote-ref\"><a href=\"#fn-{}\" id=\"{}\">{}</a></sup>",
                    r.number,
                    ref_id(r.number, *n),
                    r.number
                );
                rest = &rest[r.len..];
            }
            None => {
                out.push_str("<sup");
                rest = &rest["<sup".len()..];
            }
        }
    }
    out.push_str(rest);

    let refs_done = out;
    let mut out = String::with_capacity(refs_done.len());
    let mut rest = refs_done.as_str();
    while let Some(i) = rest.find("<div id=\"footnotes\">") {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        match parse_definitions(rest) {
            Some((defs, len)) => {
                render_definitions(&defs, &cited, &mut out);
                rest = &rest[len..];
            }
            // A definition block that is not exactly the engine's shape
            // leaves the whole body alone, rewritten references included:
            // references pointing at definitions of another shape would
            // pair with nothing.
            None => return html.to_string(),
        }
    }
    out.push_str(rest);
    out
}

/// The id comrak gives the `n`th citation of a note: `fnref-N`, then
/// `fnref-N-2`, `fnref-N-3`, ...
fn ref_id(number: &str, n: u32) -> String {
    if n == 1 {
        format!("fnref-{number}")
    } else {
        format!("fnref-{number}-{n}")
    }
}

fn is_number(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// The attribute text of a tag whose `<name` prefix is already consumed,
/// and what follows its `>`. `None` when another tag opens before it closes.
fn split_tag(s: &str) -> Option<(&str, &str)> {
    let end = s.find('>')?;
    let attrs = &s[..end];
    if attrs.contains('<') {
        return None;
    }
    Some((attrs, &s[end + 1..]))
}

/// A recognised Asciidoctor reference: the definition number it cites and
/// how many bytes of input it spans.
struct Ref<'a> {
    number: &'a str,
    len: usize,
}

/// Parse `<sup class="footnote">[<a ... href="#_footnotedef_N">N</a>]</sup>`
/// (class `footnoteref` for a repeated citation) at the start of `s`.
fn parse_ref(s: &str) -> Option<Ref<'_>> {
    let rest = s
        .strip_prefix("<sup class=\"footnote\"")
        .or_else(|| s.strip_prefix("<sup class=\"footnoteref\""))?;
    let (_, rest) = split_tag(rest)?;
    let rest = rest.strip_prefix("[<a")?;
    let (attrs, rest) = split_tag(rest)?;
    let number = attr(attrs, "href")?.strip_prefix("#_footnotedef_")?;
    if !is_number(number) {
        return None;
    }
    let end = rest.find("</a>")?;
    if &rest[..end] != number {
        return None;
    }
    let rest = rest[end..].strip_prefix("</a>]</sup>")?;
    Some(Ref {
        number,
        len: s.len() - rest.len(),
    })
}

/// One Asciidoctor definition: its number and the note's inline HTML.
struct Definition<'a> {
    number: &'a str,
    text: &'a str,
}

/// Parse the whole `<div id="footnotes">...</div>` block at the start of
/// `s`: an `<hr>`, then one `div.footnote` per note, each opening with the
/// numbered link back to its reference. Returns the definitions and how many
/// bytes the block spans.
fn parse_definitions(s: &str) -> Option<(Vec<Definition<'_>>, usize)> {
    const CLOSE: &str = "\n</div>";
    let mut rest = s.strip_prefix("<div id=\"footnotes\">")?;
    let mut defs = Vec::new();
    loop {
        rest = rest.trim_start();
        if let Some(r) = rest.strip_prefix("<hr>") {
            rest = r;
            continue;
        }
        if let Some(r) = rest.strip_prefix("</div>") {
            rest = r;
            break;
        }
        let r = rest.strip_prefix("<div class=\"footnote\" id=\"_footnotedef_")?;
        let end = r.find('"')?;
        let number = &r[..end];
        if !is_number(number) {
            return None;
        }
        let r = r[end..].strip_prefix("\">")?.trim_start();
        let r = r
            .strip_prefix("<a href=\"#_footnoteref_")?
            .strip_prefix(number)?
            .strip_prefix("\">")?
            .strip_prefix(number)?
            .strip_prefix("</a>.")?;
        let r = r.strip_prefix(' ').unwrap_or(r);
        // A note is inline content, so the engine's own line break before
        // the closing tag is the first one in the definition.
        let end = r.find(CLOSE)?;
        defs.push(Definition {
            number,
            text: r[..end].trim_end(),
        });
        rest = &r[end + CLOSE.len()..];
    }
    Some((defs, s.len() - rest.len()))
}

/// Emit the definitions as comrak's `<section class="footnotes">`, with one
/// back link per citation recorded in `cited`.
fn render_definitions(defs: &[Definition<'_>], cited: &HashMap<&str, u32>, out: &mut String) {
    out.push_str("<section class=\"footnotes\">\n<ol>\n");
    for def in defs {
        let _ = write!(out, "<li id=\"fn-{}\">\n<p>{}", def.number, def.text);
        let citations = cited.get(def.number).copied().unwrap_or(0);
        for n in 1..=citations {
            let _ = write!(
                out,
                " <a href=\"#{}\" class=\"footnote-backref\">\u{21a9}",
                ref_id(def.number, n)
            );
            if n > 1 {
                let _ = write!(out, "<sup class=\"footnote-ref\">{n}</sup>");
            }
            out.push_str("</a>");
        }
        out.push_str("</p>\n</li>\n");
    }
    out.push_str("</ol>\n</section>");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Asciidoctor's output for a post with two plain footnotes and one
    /// named note cited twice (`footnote:sec[Named note.]` then
    /// `footnote:sec[]`), verbatim.
    const ASCIIDOCTOR: &str = r##"<h1>T</h1>
<div class="paragraph">
<p>Text.<sup class="footnote">[<a id="_footnoteref_1" class="footnote" href="#_footnotedef_1" title="View footnote.">1</a>]</sup> More.<sup class="footnote">[<a id="_footnoteref_2" class="footnote" href="#_footnotedef_2" title="View footnote.">2</a>]</sup></p>
</div>
<div class="paragraph">
<p>Repeat.<sup class="footnote" id="_footnote_sec">[<a id="_footnoteref_3" class="footnote" href="#_footnotedef_3" title="View footnote.">3</a>]</sup> Again.<sup class="footnoteref">[<a class="footnote" href="#_footnotedef_3" title="View footnote.">3</a>]</sup></p>
</div>
<div id="footnotes">
<hr>
<div class="footnote" id="_footnotedef_1">
<a href="#_footnoteref_1">1</a>. A note with <strong>bold</strong> and a <a href="https://example.org">link</a>.
</div>
<div class="footnote" id="_footnotedef_2">
<a href="#_footnotedef_2">2</a>. Second.
</div>
<div class="footnote" id="_footnotedef_3">
<a href="#_footnoteref_3">3</a>. Named note.
</div>
</div>
"##;

    #[test]
    fn asciidoctor_footnotes_take_the_shared_shape() {
        // The fixture's second definition deliberately links to the wrong
        // target (`_footnotedef_2` instead of `_footnoteref_2`): a block that
        // is not exactly the engine's shape leaves the whole body untouched,
        // references included. Pin that, then fix the fixture and check the
        // rewrite.
        assert_eq!(normalize(ASCIIDOCTOR), ASCIIDOCTOR);

        let fixture = ASCIIDOCTOR.replace(r##"<a href="#_footnotedef_2">2</a>."##, r##"<a href="#_footnoteref_2">2</a>."##);
        let html = normalize(&fixture);
        assert!(!html.contains("_footnote"), "{html}");
        assert!(html.contains(r##"More.<sup class="footnote-ref"><a href="#fn-2" id="fnref-2">2</a></sup>"##), "{html}");
        assert!(html.contains(r##"Repeat.<sup class="footnote-ref"><a href="#fn-3" id="fnref-3">3</a></sup>"##), "{html}");
        assert!(html.contains(r##"Again.<sup class="footnote-ref"><a href="#fn-3" id="fnref-3-2">3</a></sup>"##), "{html}");
        let expected = "<section class=\"footnotes\">\n<ol>\n\
<li id=\"fn-1\">\n<p>A note with <strong>bold</strong> and a <a href=\"https://example.org\">link</a>. <a href=\"#fnref-1\" class=\"footnote-backref\">\u{21a9}</a></p>\n</li>\n\
<li id=\"fn-2\">\n<p>Second. <a href=\"#fnref-2\" class=\"footnote-backref\">\u{21a9}</a></p>\n</li>\n\
<li id=\"fn-3\">\n<p>Named note. <a href=\"#fnref-3\" class=\"footnote-backref\">\u{21a9}</a> <a href=\"#fnref-3-2\" class=\"footnote-backref\">\u{21a9}<sup class=\"footnote-ref\">2</sup></a></p>\n</li>\n\
</ol>\n</section>\n";
        assert!(html.ends_with(expected), "{html}");
    }

    /// The same notes written in Markdown come out byte-identical once both
    /// bodies are sanitized: the client sees one shape, whichever engine
    /// parsed the post.
    #[test]
    fn matches_comrak_after_sanitizing() {
        let md = "Text.[^1] More.[^2]\n\nRepeat.[^3] Again.[^3]\n\n\
[^1]: A note with **bold** and a [link](https://example.org).\n\
[^2]: Second.\n\
[^3]: Named note.\n";
        let from_md = crate::sanitize::body(&crate::markdown::to_html(md));
        let fixture = ASCIIDOCTOR.replace(r##"<a href="#_footnotedef_2">2</a>."##, r##"<a href="#_footnoteref_2">2</a>."##);
        let from_adoc = crate::sanitize::body(&normalize(&fixture));

        fn section(html: &str) -> &str {
            let start = html.find("<section").expect("a footnote section");
            let end = html.find("</section>").expect("closed") + "</section>".len();
            &html[start..end]
        }
        assert_eq!(section(&from_md), section(&from_adoc));
        for reference in [
            r##"<sup class="footnote-ref"><a href="#fn-1" id="fnref-1">1</a></sup>"##,
            r##"<sup class="footnote-ref"><a href="#fn-3" id="fnref-3-2">3</a></sup>"##,
        ] {
            assert!(from_md.contains(reference), "{from_md}");
            assert!(from_adoc.contains(reference), "{from_adoc}");
        }
    }

    #[test]
    fn shared_shape_and_unrecognised_markup_pass_through() {
        let comrak = crate::markdown::to_html("Hi[^a].\n\n[^a]: Note.\n");
        assert_eq!(normalize(&comrak), comrak);
        for odd in [
            r#"<sup class="footnote">loose text</sup>"#,
            r##"<sup class="footnote">[<a href="#_footnotedef_x">x</a>]</sup>"##,
            r##"<sup class="footnote">[<a href="#_footnotedef_1">2</a>]</sup>"##,
            r#"<div id="footnotes"><p>hand-written</p></div>"#,
            r#"<div id="footnotes"><div class="footnote" id="_footnotedef_1">no back link</div></div>"#,
        ] {
            assert_eq!(normalize(odd), odd);
        }
    }
}
