use std::time::Duration;
use tokio::process::Command;
use tokio::sync::Semaphore;

/// Global semaphore to limit concurrent pandoc processes.
static PANDOC_SEMAPHORE: Semaphore = Semaphore::const_new(4);

const PANDOC_TIMEOUT: Duration = Duration::from_secs(10);

/// What kind of rendered output we produce.
#[derive(Debug)]
#[allow(dead_code)]
pub enum RenderedContent {
    /// HTML fragment — wrapped in page shell
    Html(String),
    /// Complete HTML document — served as-is
    Standalone(String),
    /// Plain text wrapped in <pre>
    PreformattedText(String),
    /// Link embed card
    Embed(String),
    /// Image to be shown in a viewer
    Image {
        mime: String,
    },
    /// Serve as download
    Download {
        mime: String,
    },
}

/// Map file extension to pandoc input format.
fn pandoc_format(ext: &str) -> Option<&'static str> {
    match ext {
        "md" => Some("commonmark_x"),
        "adoc" => Some("asciidoc"),
        "rst" => Some("rst"),
        "tex" => Some("latex"),
        "org" => Some("org"),
        _ => None,
    }
}

/// Check if extension is an image type.
fn is_image(ext: &str) -> bool {
    matches!(ext, "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" | "avif" | "ico" | "bmp")
}

/// Render content based on file extension.
pub async fn render_entry(extension: &str, file_content: &[u8]) -> Result<RenderedContent, String> {
    // Normalize aliases up front (`markdown`→`md`, `text`→`txt`, `asciidoc`→`adoc`),
    // so both the medium classifier and this render path agree — no format silently
    // falls through to a download.
    let ext = crate::entry::normalize_ext(extension);
    let ext = ext.as_str();
    if let Some(format) = pandoc_format(ext) {
        let html = render_pandoc(format, file_content).await?;
        // pandoc passes raw embedded HTML straight through (its `-raw_html`
        // reader extension is a no-op in 3.7), so the body is untrusted until
        // sanitized. This is the fail-closed boundary for author Markdown: no
        // `<script>`/`on*`/`javascript:` survives into the trusted origin, while
        // pandoc's structural markup (highlight classes, footnote ids) is kept.
        Ok(RenderedContent::Html(crate::sanitize::body(&html)))
    } else if ext == "html" || ext == "htm" {
        // Every .html post is served as its own sandboxed document (the A/B/C
        // model in routes.rs), never merged into the trusted shell -- so raw
        // author HTML (which pandoc would pass through unsanitized, and which we
        // deliberately do NOT sanitize here because running arbitrary HTML/JS
        // jailed is the whole feature) can only ever execute in the jail.
        Ok(RenderedContent::Standalone(String::from_utf8_lossy(file_content).to_string()))
    } else if ext == "txt" {
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

/// Shell out to pandoc for rendering.
async fn render_pandoc(input_format: &str, content: &[u8]) -> Result<String, String> {
    let _permit = PANDOC_SEMAPHORE
        .acquire()
        .await
        .map_err(|e| format!("Semaphore error: {}", e))?;

    let mut child = Command::new("pandoc")
        .arg("-f")
        .arg(input_format)
        .arg("-t")
        .arg("html")
        .arg("--highlight-style=kate")
        .arg("--sandbox")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn pandoc: {}", e))?;

    // Write content to stdin
    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        stdin
            .write_all(content)
            .await
            .map_err(|e| format!("Failed to write to pandoc stdin: {}", e))?;
        // Drop stdin to close it
    }

    // Wait with timeout
    let output = tokio::time::timeout(PANDOC_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| "Pandoc timed out after 10s".to_string())?
        .map_err(|e| format!("Pandoc process error: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Pandoc failed: {}", stderr));
    }

    String::from_utf8(output.stdout).map_err(|e| format!("Pandoc output not UTF-8: {}", e))
}
