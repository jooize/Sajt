//! Syntax highlighting -- one highlighter for every authoring format.
//!
//! comrak (CommonMark) and Asciidoctor both leave a fenced code block as
//! `<pre><code class="language-X">escaped text</code></pre>`. [`code_blocks`]
//! rewrites every such block into the token markup the stylesheet themes:
//!
//! ```html
//! <div class="sourceCode"><pre class="sourceCode rust"><code class="sourceCode rust">
//! <span class="kw">fn</span> main() <span class="op">{</span> ...
//! </code></pre></div>
//! ```
//!
//! That is the shape Pandoc's Skylighting emits (the kate token classes:
//! `kw`, `dt`, `fu`, `st`, `co`, ...), so code looks identical whichever
//! engine parsed the post and the theme in `templates.rs` stays one system.
//! Tokens come from syntect's Sublime Text grammars; [`class_for`] folds the
//! TextMate scope names onto the kate classes.
//!
//! Everything here is fail-safe: a grammar error, an unknown language, or a
//! block that does not look exactly like engine output falls back to the
//! escaped plain text or leaves the markup untouched. The body still passes
//! through `sanitize::body` afterwards, so nothing produced here is trusted
//! on its own.

use std::fmt::Write as _;
use std::sync::LazyLock;

use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

/// The bundled grammars. Loading the dump takes tens of milliseconds, so it
/// happens once per process.
static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);

/// Code larger than this is served escaped but unhighlighted: a grammar can
/// go quadratic on adversarial input, and a listing this long is not prose.
const MAX_HIGHLIGHT_BYTES: usize = 256 * 1024;

/// A scope prefix and the kate token class it maps to. Checked in order for
/// each scope on the stack, innermost scope first, so a more specific prefix
/// must precede its general form (`comment.block.documentation` before
/// `comment`).
struct Rule {
    prefix: Scope,
    class: &'static str,
}

static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    let table: &[(&str, &str)] = &[
        ("comment.block.documentation", "do"),
        ("comment.line.documentation", "do"),
        ("comment", "co"),
        ("string.regexp", "ss"),
        ("constant.character.escape", "sc"),
        ("constant.other.placeholder", "sc"),
        ("string", "st"),
        ("keyword.control.import", "im"),
        ("keyword.control", "cf"),
        ("keyword.operator", "op"),
        ("keyword", "kw"),
        ("storage.type.primitive", "dt"),
        ("storage", "kw"),
        ("constant.numeric.float", "fl"),
        ("constant.numeric.integer.hexadecimal", "bn"),
        ("constant.numeric.integer.binary", "bn"),
        ("constant.numeric.integer.octal", "bn"),
        ("constant.numeric.hex", "bn"),
        ("constant.numeric.binary", "bn"),
        ("constant.numeric.octal", "bn"),
        ("constant.numeric", "dv"),
        ("constant.character", "ch"),
        ("constant.language", "cn"),
        ("constant", "cn"),
        ("entity.name.function", "fu"),
        ("entity.name.tag", "kw"),
        ("entity.name.constant", "cn"),
        ("entity.name.label", "ot"),
        ("entity.name.section", "ot"),
        ("entity.name", "dt"),
        ("entity.other.attribute-name", "at"),
        ("entity.other.inherited-class", "dt"),
        ("support.function", "bu"),
        ("support.type", "dt"),
        ("support.class", "dt"),
        ("support.constant", "cn"),
        ("support.variable", "va"),
        ("support.module", "im"),
        ("support", "bu"),
        ("variable.parameter", "va"),
        ("variable.language", "kw"),
        ("variable.function", "fu"),
        ("variable.annotation", "at"),
        ("variable", "va"),
        ("meta.annotation", "at"),
        ("meta.attribute", "at"),
        ("meta.preprocessor", "pp"),
        ("invalid.deprecated", "wa"),
        ("invalid", "er"),
        ("punctuation.section.interpolation", "sc"),
        ("punctuation.section.embedded", "sc"),
        ("punctuation", "op"),
    ];
    table
        .iter()
        .map(|(prefix, class)| Rule {
            prefix: Scope::new(prefix).expect("scope table entries are valid scope names"),
            class,
        })
        .collect()
});

/// Scopes that only wrap a construct and must defer to their parent: the
/// quotes of a string are part of the string, a comment marker part of the
/// comment, and `meta.*` blocks are structure, not tokens.
static DEFER: LazyLock<Vec<Scope>> = LazyLock::new(|| {
    ["punctuation.definition", "meta", "source", "text"]
        .iter()
        .map(|s| Scope::new(s).expect("valid scope name"))
        .collect()
});

/// The kate class for a scope stack, or `None` for plain text: the innermost
/// scope that maps to a token class wins, structural scopes defer outward.
/// The `meta.*` entries in the table (annotations, preprocessor lines) are
/// the deliberate exceptions to deferring: there the wrapper is the token.
fn class_for(stack: &[Scope]) -> Option<&'static str> {
    for scope in stack.iter().rev() {
        let rule = RULES.iter().find(|r| r.prefix.is_prefix_of(*scope));
        let structural = DEFER.iter().any(|d| d.is_prefix_of(*scope));
        match rule {
            Some(rule) if !structural || matches!(rule.class, "at" | "pp") => {
                return Some(rule.class);
            }
            _ => continue,
        }
    }
    None
}

/// Resolve a fence label to a grammar. Labels that name a shell dialect the
/// bundled grammars lack fold onto bash, the nearest thing.
fn syntax_for(lang: &str) -> Option<&'static SyntaxReference> {
    let token = match lang.to_ascii_lowercase().as_str() {
        "console" | "shell" | "shell-session" | "zsh" | "fish" | "sh" => "bash".to_string(),
        "jsonc" | "json5" => "json".to_string(),
        other => other.to_string(),
    };
    SYNTAXES.find_syntax_by_token(&token)
}

/// Escape text for an HTML text node or a double-quoted attribute.
fn escape(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
}

/// Highlight `code` as `lang`: the inner HTML of the `<code>` element, with
/// kate-class spans and escaped text. `None` when the language has no grammar
/// or the grammar fails, so the caller can serve the plain escape instead.
pub fn highlight(lang: &str, code: &str) -> Option<String> {
    if code.len() > MAX_HIGHLIGHT_BYTES {
        return None;
    }
    let syntax = syntax_for(lang)?;
    let mut state = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let mut out = String::with_capacity(code.len() * 2);
    let mut open: Option<&'static str> = None;

    // Emit a run of text under one class, merging with an open span of the
    // same class and keeping line ends outside every span.
    fn emit(out: &mut String, open: &mut Option<&'static str>, text: &str, class: Option<&'static str>) {
        if text.is_empty() {
            return;
        }
        if let Some(body) = text.strip_suffix('\n') {
            emit(out, open, body, class);
            if open.take().is_some() {
                out.push_str("</span>");
            }
            out.push('\n');
            return;
        }
        if *open != class {
            if open.take().is_some() {
                out.push_str("</span>");
            }
            if let Some(c) = class {
                let _ = write!(out, "<span class=\"{c}\">");
                *open = Some(c);
            }
        }
        escape(text, out);
    }

    // Grammars from `load_defaults_newlines` expect every line to end in a
    // newline; the last line is completed and the extra newline dropped below.
    let owned;
    let source: &str = if code.ends_with('\n') {
        code
    } else {
        owned = format!("{code}\n");
        &owned
    };
    for line in LinesWithEndings::from(source) {
        let ops = state.parse_line(line, &SYNTAXES).ok()?;
        let mut at = 0;
        for (offset, op) in &ops {
            emit(&mut out, &mut open, &line[at..*offset], class_for(stack.as_slice()));
            stack.apply(op).ok()?;
            at = *offset;
        }
        emit(&mut out, &mut open, &line[at..], class_for(stack.as_slice()));
    }
    if open.is_some() {
        out.push_str("</span>");
    }
    // The fence's own trailing newline is not content; Pandoc's markup ends
    // right after the last token and the stylesheet's padding does the rest.
    if out.ends_with('\n') {
        out.pop();
    }
    Some(out)
}

/// Render one code block in the shared markup. `lang` is the fence label as
/// written; anything but the characters a class token can safely carry is
/// dropped from the emitted class.
fn render_block(lang: &str, code: &str) -> String {
    let label: String = lang
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '#' | '.' | '-'))
        .collect();
    let class = if label.is_empty() {
        "sourceCode".to_string()
    } else {
        format!("sourceCode {label}")
    };
    let inner = highlight(lang, code).unwrap_or_else(|| {
        let mut plain = String::with_capacity(code.len());
        escape(code.strip_suffix('\n').unwrap_or(code), &mut plain);
        plain
    });
    format!(
        "<div class=\"sourceCode\"><pre class=\"{class}\"><code class=\"{class}\">{inner}</code></pre></div>"
    )
}

/// Decode the entities an engine uses inside `<code>` (the five XML escapes
/// and numeric references). Anything else is left as written.
fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest[..rest.len().min(12)].find(';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix('#')
                .and_then(|num| {
                    if let Some(hex) = num.strip_prefix(['x', 'X']) {
                        u32::from_str_radix(hex, 16).ok()
                    } else {
                        num.parse::<u32>().ok()
                    }
                })
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// A recognised engine code block: the fence language, the escaped text
/// between the `<code>` tags, and how many bytes of input it spans.
struct Block<'a> {
    lang: &'a str,
    escaped: &'a str,
    len: usize,
}

/// The value of `name="..."` inside a tag's attribute text, if present.
fn attr<'a>(attrs: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = attrs;
    while let Some(i) = rest.find(name) {
        let after = &rest[i + name.len()..];
        let boundary = i == 0 || !rest.as_bytes()[i - 1].is_ascii_alphanumeric();
        if boundary {
            if let Some(value) = after.strip_prefix("=\"") {
                return value.find('"').map(|end| &value[..end]);
            }
        }
        rest = after;
    }
    None
}

/// Parse `<pre ...><code class="language-X" ...>text</code></pre>` at the
/// start of `s`. Only exactly that shape matches: the text must hold no tag,
/// so a block an author wrote by hand with markup inside is left alone.
fn parse_block(s: &str) -> Option<Block<'_>> {
    let rest = s.strip_prefix("<pre")?;
    if !rest.starts_with(['>', ' ', '\t', '\n']) {
        return None;
    }
    let pre_end = rest.find('>')?;
    let rest = rest[pre_end + 1..].trim_start();
    let code_attrs = rest.strip_prefix("<code")?;
    if !code_attrs.starts_with(['>', ' ', '\t', '\n']) {
        return None;
    }
    let code_end = code_attrs.find('>')?;
    let attrs = &code_attrs[..code_end];
    let lang = attr(attrs, "class")?
        .split_ascii_whitespace()
        .find_map(|c| c.strip_prefix("language-"))
        .filter(|l| !l.is_empty())?;
    let text = &code_attrs[code_end + 1..];
    let text_end = text.find("</code>")?;
    let escaped = &text[..text_end];
    if escaped.contains('<') {
        return None;
    }
    let tail = text[text_end + "</code>".len()..].trim_start();
    let tail = tail.strip_prefix("</pre>")?;
    Some(Block {
        lang,
        escaped,
        len: s.len() - tail.len(),
    })
}

/// Rewrite every engine-shaped code block in `html` into the shared
/// highlighted markup. Blocks with no language, and anything that is not
/// exactly engine output, pass through unchanged.
pub fn code_blocks(html: &str) -> String {
    let mut out = String::with_capacity(html.len() + html.len() / 4);
    let mut rest = html;
    while let Some(i) = rest.find("<pre") {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        match parse_block(rest) {
            Some(block) => {
                out.push_str(&render_block(block.lang, &decode_entities(block.escaped)));
                rest = &rest[block.len..];
            }
            None => {
                out.push_str("<pre");
                rest = &rest["<pre".len()..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_tokens_land_on_kate_classes() {
        let html = highlight("rust", "fn main() {\n    let s = \"hi\\n\"; // note\n}\n").unwrap();
        assert!(html.contains(r#"<span class="kw">fn</span>"#), "{html}");
        assert!(html.contains(r#"<span class="fu">main</span>"#), "{html}");
        assert!(html.contains(r#"<span class="kw">let</span>"#), "{html}");
        assert!(html.contains(r#"<span class="st">&quot;hi"#), "{html}");
        assert!(html.contains(r#"<span class="sc">\n</span>"#), "{html}");
        assert!(html.contains(r#"<span class="co">// note</span>"#), "{html}");
        // Line ends stay outside spans and the fence newline is not content.
        assert!(!html.contains("\n</span>"), "{html}");
        assert!(!html.ends_with('\n'), "{html}");
    }

    #[test]
    fn unknown_language_is_none_and_shell_aliases_resolve() {
        assert!(highlight("no-such-language", "x").is_none());
        assert!(highlight("console", "$ ls\n").is_some());
        assert!(highlight("zsh", "echo hi\n").is_some());
    }

    #[test]
    fn engine_block_becomes_shared_markup() {
        // comrak's shape, then Asciidoctor's.
        for input in [
            "<p>a</p>\n<pre><code class=\"language-rust\">fn x() -&gt; u8 { 1 }\n</code></pre>\n<p>b</p>",
            "<div class=\"content\">\n<pre class=\"highlight\"><code class=\"language-rust\" data-lang=\"rust\">fn x() -&gt; u8 { 1 }</code></pre>\n</div>",
        ] {
            let out = code_blocks(input);
            assert!(
                out.contains(r#"<div class="sourceCode"><pre class="sourceCode rust"><code class="sourceCode rust">"#),
                "{out}"
            );
            assert!(out.contains(r#"<span class="kw">fn</span>"#), "{out}");
            assert!(out.contains("-&gt;"), "escaped again: {out}");
            assert!(out.ends_with("</code></pre></div>\n<p>b</p>") || out.ends_with("</code></pre></div>\n</div>"), "{out}");
        }
    }

    #[test]
    fn unlabeled_and_foreign_blocks_pass_through() {
        let bare = "<pre><code>plain\n</code></pre>";
        assert_eq!(code_blocks(bare), bare);
        let marked_up = "<pre><code class=\"language-rust\">a <b>bold</b></code></pre>";
        assert_eq!(code_blocks(marked_up), marked_up);
        let prefix_tag = "<prefix><code class=\"language-x\">y</code></prefix>";
        assert_eq!(code_blocks(prefix_tag), prefix_tag);
    }

    #[test]
    fn unknown_language_keeps_the_wrapper_without_spans() {
        let out = code_blocks("<pre><code class=\"language-nosuch\">a &lt; b\n</code></pre>");
        assert_eq!(
            out,
            r#"<div class="sourceCode"><pre class="sourceCode nosuch"><code class="sourceCode nosuch">a &lt; b</code></pre></div>"#
        );
    }

    #[test]
    fn class_token_is_filtered() {
        let out = code_blocks("<pre><code class=\"language-x&quot;onload\">1</code></pre>");
        assert!(out.contains(r#"class="sourceCode xquotonload""#), "{out}");
    }

    #[test]
    fn entities_decode() {
        assert_eq!(decode_entities("a &lt;b&gt; &amp; &quot;c&quot; &#39;d&#x27; &bogus; &"), "a <b> & \"c\" 'd' &bogus; &");
    }
}
