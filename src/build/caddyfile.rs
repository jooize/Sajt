//! The Caddyfile adapter: render the host-neutral manifest as one generated
//! site block (sajt.md "Hosting: one manifest, adapters translate").
//! Rung 1+2 of the capability ladder: exact files via per-URL matchers over
//! the blob store, plus the declarative rule map (301s, the 410 ledger,
//! headers). The manifest is the product; this file is a rendering of it,
//! regenerated per host and never edited.
//!
//! Fail closed: a URL or header value this renderer cannot represent
//! *exactly* in Caddyfile syntax is a hard error naming the value, never a
//! best-effort escape. Caddy expands `{...}` placeholders inside quoted
//! strings and treats `*` in path matchers as a wildcard, and its path
//! matching is case-insensitive — each of those would silently change what
//! gets served, so they are rejected up front.

use crate::build::manifest::{FileEntry, Manifest};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Quote a value as a Caddyfile double-quoted token. Errors on characters
/// whose meaning Caddy would change underneath us (placeholders, control
/// characters) rather than guessing at an escape.
fn quote(value: &str, what: &str) -> Result<String, String> {
    if value.chars().any(|c| c.is_control()) {
        return Err(format!(
            "{} contains a control character and cannot be written into a Caddyfile: {:?}",
            what, value
        ));
    }
    if value.contains('{') || value.contains('}') {
        return Err(format!(
            "{} contains '{{' or '}}', which Caddy expands as a placeholder even inside \
             quotes; rename the source file: {:?}",
            what, value
        ));
    }
    Ok(format!(
        "\"{}\"",
        value.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

/// Quote a URL for a `path` matcher: everything `quote` rejects, plus `*`,
/// which the matcher would treat as a wildcard with no escape syntax.
fn quote_path(url: &str, what: &str) -> Result<String, String> {
    if url.contains('*') {
        return Err(format!(
            "{} contains '*', which a Caddy path matcher treats as a wildcard; \
             rename the source file: {:?}",
            what, url
        ));
    }
    quote(url, what)
}

/// Caddy path matching is case-insensitive, so two addresses differing only
/// by case would collapse into one. The serve lane distinguishes them; this
/// adapter cannot, and says so instead of shipping the wrong bytes.
fn reject_case_collisions(manifest: &Manifest) -> Result<(), String> {
    let mut seen: BTreeMap<String, &str> = BTreeMap::new();
    let addresses = manifest
        .files
        .keys()
        .chain(manifest.redirects.keys())
        .chain(manifest.gone.iter());
    for url in addresses {
        if let Some(prior) = seen.insert(url.to_lowercase(), url) {
            if prior != url {
                return Err(format!(
                    "addresses {:?} and {:?} differ only by case; Caddy matches paths \
                     case-insensitively and would serve one for both",
                    prior, url
                ));
            }
        }
    }
    Ok(())
}

/// The headers a file's handle block sets: everything the manifest recorded
/// except `etag`. Caddy's file_server derives its own strong ETag from the
/// blob (equally stable — blobs are content-addressed) and runs the
/// conditional-request logic against it; overriding the value would break
/// If-None-Match revalidation without making anything more correct.
fn emit_headers(out: &mut String, indent: &str, entry: &FileEntry) -> Result<(), String> {
    let kept: Vec<(&String, &String)> = entry
        .headers
        .iter()
        .filter(|(name, _)| name.as_str() != "etag")
        .collect();
    if kept.is_empty() {
        return Ok(());
    }
    writeln!(out, "{}header {{", indent).unwrap();
    for (name, value) in kept {
        writeln!(
            out,
            "{}\t{} {}",
            indent,
            name,
            quote(value, &format!("header {} value", name))?
        )
        .unwrap();
    }
    writeln!(out, "{}}}", indent).unwrap();
    Ok(())
}

fn emit_file_handle(
    out: &mut String,
    matcher: &str,
    url: &str,
    entry: &FileEntry,
) -> Result<(), String> {
    writeln!(
        out,
        "\t@{} path {}",
        matcher,
        quote_path(url, "published address")?
    )
    .unwrap();
    writeln!(out, "\thandle @{} {{", matcher).unwrap();
    emit_headers(out, "\t\t", entry)?;
    writeln!(
        out,
        "\t\trewrite * {}",
        quote(&format!("/blobs/{}", entry.hash), "blob path")?
    )
    .unwrap();
    if entry.status == 200 {
        writeln!(out, "\t\tfile_server").unwrap();
    } else {
        writeln!(out, "\t\tfile_server {{").unwrap();
        writeln!(out, "\t\t\tstatus {}", entry.status).unwrap();
        writeln!(out, "\t\t}}").unwrap();
    }
    writeln!(out, "\t}}").unwrap();
    Ok(())
}

/// Render the manifest as a complete Caddyfile. `address` is the site
/// address line (e.g. `http://localhost:8080` for the parity loop, the real
/// domain in production); `root` is the build directory holding `blobs/`.
pub fn render(manifest: &Manifest, address: &str, root: &str) -> Result<String, String> {
    reject_case_collisions(manifest)?;

    let mut out = String::new();
    writeln!(
        out,
        "# Generated by sajt-build caddyfile -- a rendering of manifest.json."
    )
    .unwrap();
    writeln!(out, "# Do not edit; regenerate instead.").unwrap();
    writeln!(
        out,
        "# {} files, {} redirects, {} gone; manifest generated {}.",
        manifest.files.len(),
        manifest.redirects.len(),
        manifest.gone.len(),
        manifest.generated
    )
    .unwrap();
    writeln!(
        out,
        "# ETags are file_server's own (derived from the content-addressed blobs);"
    )
    .unwrap();
    writeln!(
        out,
        "# manifest etag values are intentionally not forced over them."
    )
    .unwrap();
    writeln!(out).unwrap();

    if address.chars().any(|c| c.is_control()) {
        return Err("site address contains a control character".to_string());
    }
    writeln!(out, "{} {{", address).unwrap();
    writeln!(out, "\troot * {}", quote(root, "root path")?).unwrap();
    writeln!(out).unwrap();

    // Hardening headers on every response, from the one policy source.
    writeln!(out, "\theader {{").unwrap();
    for (name, value) in &manifest.universal_headers {
        writeln!(
            out,
            "\t\t{} {}",
            name,
            quote(value, &format!("universal header {} value", name))?
        )
        .unwrap();
    }
    writeln!(out, "\t}}").unwrap();
    writeln!(out).unwrap();

    // The rule map: 301s first (redir orders before handle in Caddy), then
    // the 410 ledger.
    for (from, to) in &manifest.redirects {
        writeln!(
            out,
            "\tredir {} {} 301",
            quote_path(from, "redirect source")?,
            quote(to, "redirect target")?
        )
        .unwrap();
    }
    if !manifest.redirects.is_empty() {
        writeln!(out).unwrap();
    }

    if !manifest.gone.is_empty() {
        write!(out, "\t@gone path").unwrap();
        for url in &manifest.gone {
            write!(out, " {}", quote_path(url, "gone address")?).unwrap();
        }
        writeln!(out).unwrap();
        writeln!(out, "\thandle @gone {{").unwrap();
        writeln!(
            out,
            "\t\trespond \"This address was published here once; its content has been removed.\" 410"
        )
        .unwrap();
        writeln!(out, "\t}}").unwrap();
        writeln!(out).unwrap();
    }

    // Every published file: exact-path matcher, its recorded headers, its
    // blob. Handle blocks are mutually exclusive; the matcher-less fallback
    // below catches everything outside the closure.
    for (i, (url, entry)) in manifest.files.iter().enumerate() {
        emit_file_handle(&mut out, &format!("f{}", i), url, entry)?;
    }
    writeln!(out).unwrap();

    writeln!(out, "\thandle {{").unwrap();
    emit_headers(&mut out, "\t\t", &manifest.fallback)?;
    writeln!(
        out,
        "\t\trewrite * {}",
        quote(&format!("/blobs/{}", manifest.fallback.hash), "fallback blob path")?
    )
    .unwrap();
    writeln!(out, "\t\tfile_server {{").unwrap();
    writeln!(out, "\t\t\tstatus {}", manifest.fallback.status).unwrap();
    writeln!(out, "\t\t}}").unwrap();
    writeln!(out, "\t}}").unwrap();
    writeln!(out, "}}").unwrap();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::manifest::ProvenanceEntry;
    use std::collections::{BTreeMap, BTreeSet};

    fn entry(hash: &str, status: u16, headers: &[(&str, &str)]) -> FileEntry {
        FileEntry {
            hash: hash.to_string(),
            size: 1,
            status,
            headers: headers
                .iter()
                .map(|(n, v)| (n.to_string(), v.to_string()))
                .collect(),
            provenance: None,
        }
    }

    fn tiny_manifest() -> Manifest {
        Manifest {
            version: 1,
            generated: "2026-08-30T00:00:00Z".to_string(),
            files: BTreeMap::from([
                (
                    "/".to_string(),
                    entry(
                        "sha256-aa",
                        200,
                        &[
                            ("content-type", "text/html; charset=utf-8"),
                            ("content-security-policy", "default-src 'self'"),
                            ("etag", "\"abc\""),
                        ],
                    ),
                ),
                (
                    "/fog over".to_string(),
                    entry("sha256-bb", 200, &[("content-type", "text/plain")]),
                ),
                (
                    "/photo.tif".to_string(),
                    entry("sha256-cc", 415, &[("content-type", "text/html; charset=utf-8")]),
                ),
            ]),
            redirects: BTreeMap::from([("/old".to_string(), "/".to_string())]),
            gone: BTreeSet::from(["/removed".to_string()]),
            fallback: entry("sha256-ff", 404, &[("content-type", "text/html; charset=utf-8")]),
            universal_headers: vec![("x-content-type-options".to_string(), "nosniff".to_string())],
        }
    }

    #[test]
    fn renders_the_whole_vocabulary() {
        let text = render(&tiny_manifest(), "http://localhost:8080", "/tmp/build").unwrap();
        assert!(text.contains("http://localhost:8080 {"));
        assert!(text.contains("root * \"/tmp/build\""));
        assert!(text.contains("x-content-type-options \"nosniff\""));
        assert!(text.contains("redir \"/old\" \"/\" 301"));
        assert!(text.contains("@gone path \"/removed\""));
        assert!(text.contains(" 410"));
        // Exact-path matchers, quoted (the space survives).
        assert!(text.contains("path \"/fog over\""));
        assert!(text.contains("rewrite * \"/blobs/sha256-bb\""));
        // The 415 explainer keeps its status.
        assert!(text.contains("status 415"));
        // The fallback closes the site block with the manifest's 404 page.
        assert!(text.contains("rewrite * \"/blobs/sha256-ff\""));
        assert!(text.contains("status 404"));
        // ETags are left to file_server (the explanatory comment may name
        // them; no header directive line does).
        assert!(!text.contains("\tetag "));
        assert!(!text.contains("\"abc\""));
    }

    #[test]
    fn quoting_escapes_and_refuses() {
        assert_eq!(quote("a\"b\\c", "x").unwrap(), "\"a\\\"b\\\\c\"");
        assert!(quote("has{placeholder}", "x").is_err());
        assert!(quote("ctrl\nchar", "x").is_err());
        assert!(quote_path("/wild*card", "x").is_err());
        assert!(quote_path("/fog over", "x").is_ok());
    }

    #[test]
    fn case_collisions_are_refused() {
        let mut m = tiny_manifest();
        m.files.insert(
            "/Fog Over".to_string(),
            entry("sha256-dd", 200, &[("content-type", "text/plain")]),
        );
        let err = render(&m, "http://localhost:8080", "/tmp/build").unwrap_err();
        assert!(err.contains("case"), "unexpected error: {err}");
    }

    #[test]
    fn provenance_is_ignored_by_the_renderer() {
        // The adapter serves whatever the manifest lists; the push step is
        // where provenance gates. This test just pins that a provenance entry
        // does not change the rendering.
        let mut m = tiny_manifest();
        m.files.get_mut("/").unwrap().provenance = Some(ProvenanceEntry {
            source: "hello/world.md".to_string(),
            tag: "public".to_string(),
        });
        let a = render(&m, "http://localhost:8080", "/tmp/build").unwrap();
        m.files.get_mut("/").unwrap().provenance = None;
        let b = render(&m, "http://localhost:8080", "/tmp/build").unwrap();
        assert_eq!(a, b);
    }
}
