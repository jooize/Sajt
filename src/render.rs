//! Rendering dispatch: a file's bytes and extension become one of the
//! [`RenderedContent`] shapes the page layer knows how to serve.
//!
//! Text formats and their engines:
//!
//! * CommonMark (`.md`) -- comrak, in-process (`markdown.rs`). No subprocess,
//!   no external dependency, the common case.
//! * AsciiDoc (`.adoc`) -- Asciidoctor as a helper subprocess in its secure
//!   safe mode (no includes, no file access; an `include::` degrades to a
//!   dead link). The Nix shell carries it. Absent, those posts fail to
//!   render with a message naming the tool; nothing else is affected.
//!
//! Every engine's output ends in one markup and one trust boundary: fenced
//! code is highlighted by `highlight::code_blocks`, footnotes are brought to
//! the shared shape by `footnotes::normalize`, and the body is sanitized by
//! `sanitize::body`, the fail-closed boundary between author markup and the
//! trusted origin. Helpers run under a concurrency cap and a wall-clock
//! timeout, so a pathological document cannot pin the server.

use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::Semaphore;

/// At most this many helper subprocesses (Asciidoctor) at once.
static HELPER_SEMAPHORE: Semaphore = Semaphore::const_new(4);

const HELPER_TIMEOUT: Duration = Duration::from_secs(10);

/// What kind of rendered output we produce.
#[derive(Debug)]
#[allow(dead_code)]
pub enum RenderedContent {
    /// HTML fragment — wrapped in page shell
    Html(String),
    /// Complete HTML document — served as-is
    Standalone(String),
    /// Plain text shown as written (`templates::Body::Plain` escapes it and
    /// keeps its line breaks) — txt and the engine-less formats rst/org/tex.
    PreformattedText(String),
    /// Link embed card
    Embed(String),
    /// Image to be shown in a viewer
    Image {
        mime: String,
    },
    /// Serve as download (`kind = file`): anything without a text, image or
    /// document lane.
    Download {
        mime: String,
    },
}

/// Whether an image post renders as an image page (header + `<img>` + privacy
/// notice). Delegates to the canonical medium predicate so a transcode-only
/// format (HEIC/TIFF/...) gets the same page as a JPEG — the `<img>` src is the
/// raw route, which serves whatever clean derivative the privacy gate produces.
/// `svg`/`ico` are render-page extras: browsers display them, but they are not
/// raster media in `entry::is_image_ext` terms.
fn is_image(ext: &str) -> bool {
    crate::entry::is_image_ext(ext) || matches!(ext, "svg" | "ico")
}

/// Render content based on file extension.
pub async fn render_entry(extension: &str, file_content: &[u8]) -> Result<RenderedContent, String> {
    // Normalize aliases up front (`markdown`→`md`, `text`→`txt`, `asciidoc`→`adoc`),
    // so both the medium classifier and this render path agree — no format silently
    // falls through to a download.
    let ext = crate::entry::normalize_ext(extension);
    let ext = ext.as_str();
    if ext == "md" {
        let source = String::from_utf8_lossy(file_content).into_owned();
        // CPU-bound parse off the async threads; a long document must not
        // stall every other request in flight.
        let html = tokio::task::spawn_blocking(move || crate::markdown::to_html(&source))
            .await
            .map_err(|e| format!("Markdown render task failed: {e}"))?;
        Ok(RenderedContent::Html(crate::sanitize::body(&html)))
    } else if ext == "adoc" {
        let html = run_helper(
            "asciidoctor",
            &[
                "--safe-mode=secure",
                "--embedded",
                "--attribute=showtitle",
                "--out-file=-",
                "-",
            ],
            file_content,
        )
        .await?;
        let html = crate::highlight::code_blocks(&html);
        let html = crate::footnotes::normalize(&html);
        Ok(RenderedContent::Html(crate::sanitize::body(&html)))
    } else if crate::entry::is_html_document(ext) {
        // Every HTML/XHTML post is served as its own sandboxed document (the
        // A/B/C model in routes.rs), never merged into the trusted shell -- so
        // raw author HTML (which we deliberately do NOT sanitize here, because
        // running arbitrary HTML/JS jailed is the whole feature) can only ever
        // execute in the jail.
        Ok(RenderedContent::Standalone(String::from_utf8_lossy(file_content).to_string()))
    } else if crate::entry::is_plain_text_ext(ext) {
        // txt, and the formats without an engine (rst/org/tex): the post is the
        // file as written. `templates::Body::Plain` escapes it at the sink.
        let text = String::from_utf8_lossy(file_content).to_string();
        Ok(RenderedContent::PreformattedText(text))
    } else if is_image(ext) {
        let mime = mime_guess::from_ext(ext)
            .first_or_octet_stream()
            .to_string();
        Ok(RenderedContent::Image { mime })
    } else if ext.is_empty() {
        // Dotless bare file: UTF-8 → preformatted text, else an opaque download.
        match std::str::from_utf8(file_content) {
            Ok(text) => Ok(RenderedContent::PreformattedText(text.to_string())),
            Err(_) => Ok(RenderedContent::Download {
                mime: "application/octet-stream".to_string(),
            }),
        }
    } else {
        let mime = mime_guess::from_ext(ext)
            .first_or_octet_stream()
            .to_string();
        Ok(RenderedContent::Download { mime })
    }
}

/// Run a rendering helper: `content` on its stdin, its stdout as UTF-8. The
/// write and the wait proceed together, so a helper that streams output
/// before it has read all of its input cannot deadlock on a full pipe.
async fn run_helper(program: &str, args: &[&str], content: &[u8]) -> Result<String, String> {
    let _permit = HELPER_SEMAPHORE
        .acquire()
        .await
        .map_err(|e| format!("Semaphore error: {e}"))?;

    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                format!("{program} is not installed (it renders this format); nothing else is affected")
            }
            _ => format!("Failed to spawn {program}: {e}"),
        })?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| format!("{program}: stdin was not piped"))?;
    let feed = async move {
        // A helper that exits early closes the pipe; that is its error to
        // report via the exit status, not ours to surface twice.
        let _ = stdin.write_all(content).await;
        drop(stdin);
    };
    let (_, output) = tokio::join!(
        feed,
        tokio::time::timeout(HELPER_TIMEOUT, child.wait_with_output())
    );
    let output = output
        .map_err(|_| format!("{program} timed out after {}s", HELPER_TIMEOUT.as_secs()))?
        .map_err(|e| format!("{program} process error: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("{program} failed: {}", stderr.trim()));
    }

    String::from_utf8(output.stdout).map_err(|e| format!("{program} output not UTF-8: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn transcode_only_formats_render_the_image_page() {
        // Regression: is_image once had a stale private list without heic/tiff,
        // so those posts fell into the Download branch and served bare bytes at
        // their canonical URL instead of rendering the image page like a JPEG.
        for ext in ["tif", "tiff", "heic", "heif", "jpg", "png", "webp", "gif", "avif", "bmp"] {
            assert!(
                matches!(
                    render_entry(ext, b"irrelevant").await,
                    Ok(RenderedContent::Image { .. })
                ),
                "{ext} post must render as an image page"
            );
        }
    }

    #[tokio::test]
    async fn plain_text_formats_are_posts_shown_as_written() {
        // txt, and the formats that lost their engine with Pandoc: the post is
        // the file, escaped later by templates::Body::Plain, never a download.
        for ext in ["txt", "text", "rst", "org", "tex"] {
            match render_entry(ext, b"Title\n=====\n\n<b>as written</b>\n").await {
                Ok(RenderedContent::PreformattedText(t)) => assert!(t.contains("<b>as written</b>"), "{ext}"),
                other => panic!("{ext} must be plain text, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn markdown_renders_in_process_and_is_sanitized() {
        let md = b"# Title\n\n<script>alert(1)</script>\n\n```rust\nfn x() {}\n```\n\nSee <span lang=\"sv\">hej</span>.\n";
        let Ok(RenderedContent::Html(html)) = render_entry("markdown", md).await else {
            panic!("markdown must render as an HTML fragment");
        };
        assert!(html.contains(r#"<h1 id="title">Title</h1>"#), "{html}");
        assert!(!html.contains("<script"), "{html}");
        assert!(html.contains(r#"<span class="kw">fn</span>"#), "{html}");
        assert!(html.contains(r#"<span lang="sv">hej</span>"#), "inline language survives: {html}");
    }

    /// Asciidoctor is part of the Nix shell, so this is a real render, not a
    /// mock: secure mode must hold, and code and footnotes must land in the
    /// shared markup after sanitizing.
    #[tokio::test]
    async fn asciidoc_renders_through_asciidoctor_secure_mode() {
        let adoc = b"= Title\n\nHello *there*.footnote:[A note.] Again.footnote:[Two.]\n\n[source,rust]\n----\nfn x() {}\n----\n\ninclude::/etc/passwd[]\n";
        let Ok(RenderedContent::Html(html)) = render_entry("asciidoc", adoc).await else {
            panic!("asciidoc must render as an HTML fragment (is asciidoctor on PATH?)");
        };
        assert!(html.contains("<h1>Title</h1>"), "{html}");
        assert!(html.contains("<strong>there</strong>"), "{html}");
        assert!(html.contains(r#"<pre class="sourceCode rust">"#), "{html}");
        assert!(html.contains(r#"<span class="kw">fn</span>"#), "{html}");
        assert!(!html.contains("root:"), "secure mode: no include: {html}");
        assert!(
            html.contains(r##"<sup class="footnote-ref"><a href="#fn-1" id="fnref-1">1</a></sup>"##),
            "footnote references in the shared shape: {html}"
        );
        assert!(html.contains(r#"<section class="footnotes">"#), "{html}");
        assert!(
            html.contains("<li id=\"fn-2\">\n<p>Two. <a href=\"#fnref-2\" class=\"footnote-backref\">\u{21a9}</a></p>"),
            "footnote list in the shared shape: {html}"
        );
        assert!(!html.contains("_footnote"), "no Asciidoctor footnote ids remain: {html}");
    }

    #[tokio::test]
    async fn missing_helper_names_the_tool() {
        // A helper that is not installed fails only the post that needs it,
        // with a message naming the tool.
        let err = run_helper("sajt-no-such-helper", &[], b"x").await.unwrap_err();
        assert!(err.contains("sajt-no-such-helper is not installed"), "{err}");
    }
}
