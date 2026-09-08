//! The closure builder (sajt.md): generate the transitive closure of
//! the site's own link graph. Every URL a generated page emits is resolved
//! through the same `sajt::page` layer the preview server uses,
//! its reply written to a content-addressed blob store, and the whole build
//! described by one host-neutral manifest — files with hashes, the redirect
//! map, the 410 ledger, the fallback page, and the security headers. Per-host
//! adapters translate the manifest (`caddyfile` here so far); `verify`
//! replays it against any live host. The walk itself knows no provider.
//!
//! The invariant this walk enforces: **every URL the site emits is a real
//! file**. A link that resolves to a 404 is a broken build (nonzero exit),
//! not a footnote. `--allow-broken-links` downgrades that one failure to a
//! loud warning for a deliberate partial publish; internal errors still fail.

use clap::Parser;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;

use sajt::page::{self, Reply, RequestFlags};
use sajt::url::percent_decode;

use crate::{load_store, open_site, SiteArgs};

mod caddyfile;
mod manifest;
mod verify;

use manifest::{FileEntry, Manifest, ProvenanceEntry};

#[derive(Parser)]
pub struct CaddyfileArgs {
    /// The manifest to render.
    #[arg(long, default_value = "./build/manifest.json")]
    manifest: PathBuf,

    /// The site directory; its `Sajt.toml` domain names the generated site
    /// block.
    #[arg(long, default_value = ".")]
    site: PathBuf,

    /// Read the site configuration from this file instead of `Sajt.toml`
    /// inside the site directory.
    #[arg(long)]
    config: Option<PathBuf>,

    /// Site address line for the generated file, overriding the config
    /// domain (e.g. http://localhost:8080 for the local parity loop).
    /// Without either, the parity-loop address is the default.
    #[arg(long)]
    address: Option<String>,

    /// Where to write the generated Caddyfile.
    #[arg(long, default_value = "./build/Caddyfile")]
    out: PathBuf,

    /// Filesystem root the host serves blobs from. Defaults to the
    /// manifest's own directory, absolute.
    #[arg(long)]
    root: Option<PathBuf>,
}

#[derive(Parser)]
pub struct VerifyArgs {
    /// The manifest that is the reference.
    #[arg(long, default_value = "./build/manifest.json")]
    manifest: PathBuf,

    /// Base URL of the host to check, e.g. http://127.0.0.1:1234 (the
    /// preview server) or http://localhost:8080 (the Caddyfile adapter).
    #[arg(long)]
    base: String,

    /// The host runs code and personalizes its out-of-closure 404 (the
    /// preview server does). Skips the byte-exact body check on the fallback
    /// probe only; its status and headers are still checked.
    #[arg(long)]
    dynamic_fallback: bool,
}

#[derive(Parser)]
pub struct BuildArgs {
    #[command(flatten)]
    pub site: SiteArgs,

    /// On-disk static assets directory (fonts, images the shell references).
    #[arg(long, default_value = "./static")]
    static_dir: PathBuf,

    /// Output directory: `blobs/` (content-addressed bodies) + `manifest.json`.
    #[arg(long, default_value = "./build")]
    out: PathBuf,

    /// Backstop on the number of URLs walked — a runaway closure is a build
    /// bug, and this turns it into a loud failure instead of a full disk.
    #[arg(long, default_value_t = 100_000)]
    max_urls: usize,

    /// Succeed even when pages link to URLs that resolve to 404. The broken
    /// links are still listed in the report; this only changes the exit code,
    /// for a deliberate publish of a knowingly incomplete site.
    #[arg(long)]
    allow_broken_links: bool,
}

/// Where a link points, after normalization.
enum Target {
    /// A same-site path to walk (decoded, query dropped where it is not part
    /// of the resource: `?v=` cache tokens, `?search=` live input).
    Page(String),
    /// A same-site path whose query names a real variant a static host cannot
    /// key on (`?embed`, `?fullscreen`) — walked without the query, reported.
    QueryVariant(String),
    /// Off-site or non-navigational (mailto:, data:, fragments) — ignored.
    External,
}

fn normalize(href: &str) -> Target {
    if href.is_empty() || href.starts_with('#') {
        return Target::External;
    }
    // A scheme prefix (https:, mailto:, data:, …) means off-site.
    if href
        .split_once(':')
        .map_or(false, |(scheme, _)| {
            !scheme.is_empty()
                && scheme
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'-' || b == b'.')
                && !scheme.contains('/')
        })
        && !href.starts_with('/')
    {
        return Target::External;
    }
    if !href.starts_with('/') {
        // The site emits absolute paths only; a relative link is a template
        // bug worth seeing in the report rather than guessing a base for.
        return Target::External;
    }
    let no_frag = href.split('#').next().unwrap_or(href);
    let (path, query) = match no_frag.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (no_frag, None),
    };
    let path = percent_decode(path);
    if let Some(q) = query {
        let variant = q
            .split('&')
            .any(|kv| matches!(kv.split_once('=').map_or(kv, |(k, _)| k), "embed" | "fullscreen"));
        if variant {
            return Target::QueryVariant(path);
        }
    }
    Target::Page(path)
}

/// Every navigable URL a rendered page references.
fn extract_links(html: &str) -> Vec<String> {
    let doc = scraper::Html::parse_document(html);
    let mut out = Vec::new();
    for (sel, attr) in [
        ("a", "href"),
        ("img", "src"),
        ("link", "href"),
        ("script", "src"),
        ("iframe", "src"),
        ("form", "action"),
        ("video", "src"),
        ("audio", "src"),
        ("source", "src"),
    ] {
        let selector = scraper::Selector::parse(sel).expect("static selector");
        for el in doc.select(&selector) {
            if let Some(v) = el.value().attr(attr) {
                out.push(v.to_string());
            }
        }
    }
    out
}

/// The base file address under a rendition URL (`/photo.tif/jpeg/thumb` →
/// `/photo.tif`), if the URL carries rendition rungs. The base is a real
/// resource too — for a transcode-only format it is the explainer page a
/// hand-typed bare URL must land on — so the walk includes it even though no
/// page links it.
fn rendition_base(url: &str) -> Option<String> {
    let mut u = url;
    let mut stripped = false;
    for suffix in ["/thumb", "/jpeg"] {
        if let Some(s) = u.strip_suffix(suffix) {
            u = s;
            stripped = true;
        }
    }
    (stripped && u.rsplit('/').next().is_some_and(|seg| seg.contains('.')))
        .then(|| u.to_string())
}

fn content_type_of(reply: &Reply) -> String {
    reply
        .headers
        .iter()
        .find(|(n, _)| *n == "content-type")
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}

/// Fill the per-response gap the serve middleware fills on live responses: a
/// content-type-appropriate CSP where the handler chose none. The policy
/// itself stays in `sajt::security` (the single source); without
/// this, manifest entries would ship weaker headers than the preview server
/// sends.
fn default_csp(headers: &mut BTreeMap<String, String>) {
    if !headers.contains_key("content-security-policy") {
        let ct = headers.get("content-type").cloned().unwrap_or_default();
        headers.insert(
            "content-security-policy".to_string(),
            sajt::security::csp_for_content_type(&ct).to_string(),
        );
    }
}

/// Hash + persist a reply body into the blob store; return its manifest entry
/// with the reply's provenance recorded content-relative.
fn store_blob(
    out: &std::path::Path,
    content_dir: &std::path::Path,
    reply: &Reply,
) -> std::io::Result<FileEntry> {
    let digest = Sha256::digest(&reply.body);
    let hex: String = digest.iter().map(|b| format!("{:02x}", b)).collect();
    let hash = format!("sha256-{}", hex);
    let blob_path = out.join("blobs").join(&hash);
    if !blob_path.exists() {
        // Write-then-rename so a crashed build never leaves a half blob under
        // its final name.
        let tmp = out.join("blobs").join(format!(".tmp-{}", hex));
        std::fs::write(&tmp, &reply.body)?;
        std::fs::rename(&tmp, &blob_path)?;
    }
    let mut headers: BTreeMap<String, String> = reply
        .headers
        .iter()
        .map(|(n, v)| (n.to_string(), v.clone()))
        .collect();
    default_csp(&mut headers);
    let provenance = match &reply.provenance {
        page::Provenance::Generated => None,
        page::Provenance::File { source, tag } => Some(ProvenanceEntry {
            source: source
                .strip_prefix(content_dir)
                .unwrap_or(source)
                .display()
                .to_string(),
            tag: tag.to_string(),
        }),
    };
    Ok(FileEntry {
        hash,
        size: reply.body.len() as u64,
        status: reply.status,
        headers,
        provenance,
    })
}

pub fn render_caddyfile(args: CaddyfileArgs) {
    let manifest = manifest::load(&args.manifest).unwrap_or_else(|e| {
        eprintln!("{}", e);
        std::process::exit(1);
    });
    let config_path = args
        .config
        .clone()
        .unwrap_or_else(|| sajt::config::path_in(&args.site));
    let config = sajt::config::load(&config_path).unwrap_or_else(|e| {
        eprintln!("{}", e);
        std::process::exit(1);
    });
    // Explicit flag > config domain (Caddy provisions HTTPS for a bare
    // domain) > the local parity-loop address.
    let address = args
        .address
        .clone()
        .or_else(|| config.domain.clone())
        .unwrap_or_else(|| "http://localhost:8080".to_string());
    let root = args
        .root
        .unwrap_or_else(|| args.manifest.parent().unwrap_or(std::path::Path::new(".")).to_path_buf());
    let root = root.canonicalize().unwrap_or(root);
    let text = caddyfile::render(&manifest, &address, &root.display().to_string())
        .unwrap_or_else(|e| {
            eprintln!("Cannot render Caddyfile: {}", e);
            std::process::exit(1);
        });
    // Write-then-rename: the file on disk is always a complete rendering.
    let tmp = args.out.with_extension("tmp");
    std::fs::write(&tmp, &text).expect("write Caddyfile");
    std::fs::rename(&tmp, &args.out).expect("rename Caddyfile");
    println!(
        "Caddyfile written to {} ({} files, {} redirects, {} gone; root {})",
        args.out.display(),
        manifest.files.len(),
        manifest.redirects.len(),
        manifest.gone.len(),
        root.display()
    );
}

pub async fn run_verify(args: VerifyArgs) {
    let manifest = manifest::load(&args.manifest).unwrap_or_else(|e| {
        eprintln!("{}", e);
        std::process::exit(1);
    });
    let outcome = verify::run(&manifest, &args.base, args.dynamic_fallback)
        .await
        .unwrap_or_else(|e| {
        eprintln!("{}", e);
        std::process::exit(1);
    });
    println!("Sajt verify report ({})", args.base);
    println!("  addresses checked  {}", outcome.checked);
    if args.dynamic_fallback {
        println!("  note               fallback probe body not compared (--dynamic-fallback)");
    }
    if outcome.mismatches.is_empty() {
        println!("  result             OK -- host matches the manifest exactly");
        std::process::exit(0);
    }
    println!("  MISMATCHES         {}", outcome.mismatches.len());
    for m in &outcome.mismatches {
        println!("    {}", m);
    }
    std::process::exit(1);
}

pub async fn build(args: BuildArgs) {
    let site = open_site(&args.site);
    let content_dir = site.content_dir.clone();
    if args.out.starts_with(&content_dir) {
        eprintln!("Refusing to build: output directory is inside the content directory.");
        std::process::exit(1);
    }
    let store = load_store(&site).await;

    std::fs::create_dir_all(args.out.join("blobs")).expect("create output directory");

    // The previous manifest (if any) feeds the diff and the 410 ledger.
    let previous: Option<Manifest> = std::fs::read(args.out.join("manifest.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());

    // ---- The walk ----------------------------------------------------------
    let flags = RequestFlags::default();
    let mut queue: VecDeque<(String, String)> = VecDeque::new(); // (url, referrer)
    queue.push_back(("/".to_string(), "(seed)".to_string()));
    queue.push_back(("/saved".to_string(), "(seed)".to_string()));

    let mut visited: BTreeSet<String> = BTreeSet::new();
    let mut files: BTreeMap<String, FileEntry> = BTreeMap::new();
    let mut redirects: BTreeMap<String, String> = BTreeMap::new();
    let mut broken: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut withheld: BTreeSet<String> = BTreeSet::new();
    let mut query_variants: BTreeSet<String> = BTreeSet::new();
    let mut errors: BTreeMap<String, u16> = BTreeMap::new();

    while let Some((url, referrer)) = queue.pop_front() {
        if !visited.insert(url.clone()) {
            continue;
        }
        if visited.len() > args.max_urls {
            eprintln!(
                "Closure exceeded --max-urls={} — a runaway link graph. Aborting.",
                args.max_urls
            );
            std::process::exit(1);
        }

        if let Some(base) = rendition_base(&url) {
            queue.push_back((base, url.clone()));
        }

        let reply = page::route(&store, &args.static_dir, &url, &flags).await;
        match reply.status {
            200 => {
                let ct = content_type_of(&reply);
                let entry = store_blob(&args.out, &content_dir, &reply).expect("write blob");
                if ct.starts_with("text/html") {
                    let body = String::from_utf8_lossy(&reply.body).into_owned();
                    for href in extract_links(&body) {
                        match normalize(&href) {
                            Target::Page(p) => queue.push_back((p, url.clone())),
                            Target::QueryVariant(p) => {
                                query_variants.insert(href.clone());
                                queue.push_back((p, url.clone()));
                            }
                            Target::External => {}
                        }
                    }
                }
                files.insert(url, entry);
            }
            301 => {
                let location = reply.location().unwrap_or("").to_string();
                if location.is_empty() {
                    errors.insert(url, 301);
                } else {
                    let target = percent_decode(location.split('?').next().unwrap_or(&location));
                    queue.push_back((target, url.clone()));
                    redirects.insert(url, location);
                }
            }
            404 => {
                broken.entry(url).or_default().insert(referrer);
            }
            415 => {
                // A withheld-format reply is a real resource (the explanation
                // text); it ships as a file, and the report names it.
                let entry = store_blob(&args.out, &content_dir, &reply).expect("write blob");
                withheld.insert(url.clone());
                files.insert(url, entry);
            }
            other => {
                errors.insert(url, other);
            }
        }
    }

    // ---- Fallback + 410 ledger ---------------------------------------------
    let fallback = store_blob(&args.out, &content_dir, &page::fallback_404()).expect("write fallback blob");

    // Once published, an address answers 410 when its content goes — unless it
    // now redirects (a rename keeps the address alive).
    let mut gone: BTreeSet<String> = BTreeSet::new();
    if let Some(prev) = &previous {
        for url in prev.files.keys().chain(prev.gone.iter()) {
            if !files.contains_key(url) && !redirects.contains_key(url) {
                gone.insert(url.clone());
            }
        }
    }

    let manifest = Manifest {
        version: 1,
        generated: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        files,
        redirects,
        gone,
        fallback,
        universal_headers: sajt::security::UNIVERSAL_HEADERS
            .iter()
            .map(|(n, v)| (n.to_string(), v.to_string()))
            .collect(),
    };

    // Write-then-rename: the manifest on disk is always complete.
    let json = serde_json::to_vec_pretty(&manifest).expect("serialize manifest");
    let tmp = args.out.join("manifest.json.tmp");
    std::fs::write(&tmp, &json).expect("write manifest");
    std::fs::rename(&tmp, args.out.join("manifest.json")).expect("rename manifest");

    // ---- Report ------------------------------------------------------------
    let total_bytes: u64 = manifest.files.values().map(|f| f.size).sum();
    println!("Sajt build report");
    println!("  urls walked      {}", visited.len());
    println!("  files            {} ({} bytes)", manifest.files.len(), total_bytes);
    println!("  redirects        {}", manifest.redirects.len());
    println!("  gone (410)       {}", manifest.gone.len());
    println!("  withheld (415)   {}", withheld.len());
    for url in &withheld {
        println!("    {}", url);
    }
    if !query_variants.is_empty() {
        println!(
            "  query variants   {} (served without their query by a static host)",
            query_variants.len()
        );
        for v in &query_variants {
            println!("    {}", v);
        }
    }

    match &previous {
        Some(prev) => {
            let added = manifest.files.keys().filter(|u| !prev.files.contains_key(*u)).count();
            let removed = prev.files.keys().filter(|u| !manifest.files.contains_key(*u)).count();
            let changed = manifest
                .files
                .iter()
                .filter(|(u, e)| prev.files.get(*u).is_some_and(|p| p.hash != e.hash))
                .count();
            println!("  diff             {} added / {} changed / {} unpublished", added, changed, removed);
        }
        None => println!("  diff             (first build — no previous manifest)"),
    }

    let mut failed = false;
    if !errors.is_empty() {
        failed = true;
        println!("  ERRORS           {}", errors.len());
        for (url, status) in &errors {
            println!("    {} -> {}", url, status);
        }
    }
    if !broken.is_empty() {
        if args.allow_broken_links {
            println!(
                "  BROKEN LINKS     {} (ALLOWED by --allow-broken-links; these URLs will 404 on the published site)",
                broken.len()
            );
        } else {
            failed = true;
            println!("  BROKEN LINKS     {} (every emitted URL must be a real file)", broken.len());
        }
        for (url, referrers) in &broken {
            for r in referrers {
                println!("    {} (linked from {})", url, r);
            }
        }
    }

    std::process::exit(if failed { 1 } else { 0 });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(href: &str) -> Option<String> {
        match normalize(href) {
            Target::Page(p) => Some(p),
            _ => None,
        }
    }

    #[test]
    fn normalize_classifies_links() {
        assert_eq!(page("/2026/+design/notable"), Some("/2026/+design/notable".into()));
        // Fragments and cache tokens drop; the path is the resource.
        assert_eq!(page("/static/site.css?v=abc123"), Some("/static/site.css".into()));
        assert_eq!(page("/post#section"), Some("/post".into()));
        assert_eq!(page("/?search=rust"), Some("/".into()));
        // Encoded paths decode for the page layer.
        assert_eq!(page("/fog%20over"), Some("/fog over".into()));
        // Off-site and non-navigational are ignored.
        assert!(matches!(normalize("https://example.com/x"), Target::External));
        assert!(matches!(normalize("mailto:a@b.c"), Target::External));
        assert!(matches!(normalize("#top"), Target::External));
        assert!(matches!(normalize("relative/path"), Target::External));
        // Standalone-document selectors are query variants: walked without
        // the query, reported.
        assert!(matches!(normalize("/doc.html?embed"), Target::QueryVariant(p) if p == "/doc.html"));
        assert!(matches!(normalize("/doc?fullscreen"), Target::QueryVariant(p) if p == "/doc"));
    }

    #[test]
    fn default_csp_mirrors_the_serve_middleware() {
        // HTML gets the page policy; everything else the jail; a handler's
        // own choice is never overridden.
        let mut h: BTreeMap<String, String> =
            [("content-type".into(), "text/html; charset=utf-8".into())].into();
        default_csp(&mut h);
        assert_eq!(
            h["content-security-policy"],
            sajt::security::csp_for_content_type("text/html")
        );

        let mut h: BTreeMap<String, String> =
            [("content-type".into(), "image/jpeg".into())].into();
        default_csp(&mut h);
        assert_eq!(
            h["content-security-policy"],
            sajt::security::csp_for_content_type("image/jpeg")
        );

        let mut h: BTreeMap<String, String> = [
            ("content-type".into(), "text/html; charset=utf-8".into()),
            ("content-security-policy".into(), "sandbox".into()),
        ]
        .into();
        default_csp(&mut h);
        assert_eq!(h["content-security-policy"], "sandbox");
    }

    #[test]
    fn rendition_bases() {
        assert_eq!(rendition_base("/photo.tif/jpeg"), Some("/photo.tif".into()));
        assert_eq!(rendition_base("/photo.tif/jpeg/thumb"), Some("/photo.tif".into()));
        assert_eq!(rendition_base("/photo.jpg/thumb"), Some("/photo.jpg".into()));
        assert_eq!(rendition_base("/post/photo.tif/jpeg"), Some("/post/photo.tif".into()));
        assert_eq!(rendition_base("/photo.jpg"), None);
        // The rung must hang off a file, not a folder segment.
        assert_eq!(rendition_base("/folder/jpeg"), None);
    }

    #[test]
    fn link_extraction_covers_the_shell_vocabulary() {
        let html = r#"<a href="/a">x</a><img src="/i.jpg"><link href="/static/site.css?v=1">
            <script src="/static/site.js?v=2"></script><iframe src="/d.html?embed"></iframe>
            <form action="/notable"></form>"#;
        let links = extract_links(html);
        for want in ["/a", "/i.jpg", "/static/site.css?v=1", "/static/site.js?v=2", "/d.html?embed", "/notable"] {
            assert!(links.iter().any(|l| l == want), "missing {want}");
        }
    }
}
