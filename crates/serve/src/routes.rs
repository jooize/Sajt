use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use staticdrop_core::content::ContentStore;
use staticdrop_core::entry::{Entry, Revision};
use staticdrop_core::render::{render_entry, RenderedContent};
use staticdrop_core::stats::{compute_cloud, ViewFilter};
use staticdrop_core::templates::{self, HeaderContext};
use staticdrop_core::url::{parse_url_path, ContentQuery};

pub type AppState = Arc<RwLock<ContentStore>>;

/// Parse the query string into a view filter (grade / favorites / search).
/// `?favorites` is presence-based: bare, empty, or any value but 0/false.
fn view_from_query(params: &HashMap<String, String>) -> ViewFilter {
    let fav = params
        .get("favorites")
        .map_or(false, |v| v != "0" && v != "false");
    ViewFilter::from_params(
        params.get("grade").map(String::as_str),
        fav,
        params.get("q").map(String::as_str),
    )
}

/// Human-readable filter description for the page title.
fn filter_title(path_desc: &str, view: &ViewFilter, saved: bool) -> String {
    let mut parts: Vec<String> = Vec::new();
    if saved {
        parts.push("Saved".to_string());
    }
    if !path_desc.is_empty() {
        parts.push(path_desc.to_string());
    }
    if view.notable {
        parts.push(view.grade_word().to_string());
    }
    if view.fav {
        parts.push("favorites".to_string());
    }
    if let Some(ref q) = view.q {
        parts.push(format!("\u{201c}{}\u{201d}", q));
    }
    parts.join(" · ")
}

/// The single active topic, if the path filters to exactly one tag.
fn active_tag(query: &ContentQuery) -> Option<&str> {
    if query.and_tags.len() == 1
        && query.or_tags.is_empty()
        && query.date_prefix.is_none()
        && query.label.is_none()
    {
        Some(query.and_tags[0].as_str())
    } else {
        None
    }
}

/// GET / — timeline
pub async fn index(
    State(store): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let store = store.read().await;
    let all: Vec<&Entry> = store.entries.iter().collect();
    let view = view_from_query(&params);

    let display: Vec<&Entry> = all.iter().copied().filter(|e| view.matches(e)).collect();

    let cloud = compute_cloud(&all);
    let ctx = HeaderContext {
        cloud: &cloud,
        active_tag: None,
        view: &view,
        base_path: "/",
        date_scope: None,
        path_tags: "",
        saved_view: false,
    };
    let title = filter_title("", &view, false);
    Html(templates::timeline_page(&display, &all, &ctx, &title))
}

/// GET /saved — the reader's saved bookmarks (a client-side view: the server
/// renders the full timeline and the browser filters to what it has saved).
pub async fn saved(
    State(store): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let store = store.read().await;
    let all: Vec<&Entry> = store.entries.iter().collect();
    let view = view_from_query(&params);

    let display: Vec<&Entry> = all.iter().copied().filter(|e| view.matches(e)).collect();

    let cloud = compute_cloud(&all);
    let ctx = HeaderContext {
        cloud: &cloud,
        active_tag: None,
        view: &view,
        base_path: "/saved",
        date_scope: None,
        path_tags: "",
        saved_view: true,
    };
    let title = filter_title("", &view, true);
    Html(templates::timeline_page(&display, &all, &ctx, &title))
}

/// GET /static/{*path} — serve static assets with aggressive caching.
pub async fn serve_static(Path(path): Path<String>) -> Response {
    // Generated assets (site.css / site.js / boot.js) are built from the
    // compile-time consts, not read from disk. Their URLs carry a content-hash
    // `?v=` token, so they are safe to serve `immutable`.
    if let Some(asset) = staticdrop_core::assets::get(&path) {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, HeaderValue::from_static(asset.mime()))
            .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
            .body(Body::from(asset.body().to_string()))
            .unwrap_or_else(|_| not_found());
    }

    // Restrict to known safe filenames (no path traversal)
    let safe: bool = path
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
    if !safe || path.contains("..") || path.starts_with('.') {
        return not_found();
    }

    let file_path = std::path::Path::new("static").join(&path);
    let content = match std::fs::read(&file_path) {
        Ok(c) => c,
        Err(_) => return not_found(),
    };

    let ext = file_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let mime = mime_guess::from_ext(ext)
        .first_or_octet_stream()
        .to_string();

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, HeaderValue::from_str(&mime).unwrap_or(HeaderValue::from_static("application/octet-stream")))
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .body(Body::from(content))
        .unwrap_or_else(|_| not_found())
}

/// GET /{*path} — catch-all handler
pub async fn catch_all(
    State(store): State<AppState>,
    Path(path): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    // Dot segments never appear in a canonical address; reject them outright so
    // no crafted `..`/`.` path can reach resolution.
    if path.split('/').any(|s| s == "." || s == "..") {
        return not_found();
    }

    // Standalone-HTML view selectors (post-model.md §4 / the A/B/C model): `?embed`
    // appends the height reporter to the raw asset; `?fullscreen` hands a standalone
    // post the whole viewport. Both are presence flags, like `?favorites`.
    let embed = params.contains_key("embed");
    let fullscreen = params.contains_key("fullscreen");
    // `?thumb` requests a small gallery-tile rendition of an image asset
    // (post-model.md §8, C8c) instead of the full stripped/transcoded view.
    let thumb = params.contains_key("thumb");
    // `?as=jpeg` names the served representation when it is not the file's own
    // container: the clean JPEG rendition of a transcode-only format, and any
    // non-JPEG source's `?thumb` tile. The URL never lies about the bytes — the
    // bare URL of such a file answers 303 to this address instead. Unknown
    // values fail closed.
    let as_jpeg = match params.get("as").map(String::as_str) {
        None => false,
        Some("jpeg") => true,
        Some(_) => return not_found(),
    };
    // The request path re-encoded, for building the honest-address redirect.
    let raw_href = templates::encode_path(&format!("/{}", path));

    let store = store.read().await;
    let all_entries: Vec<&Entry> = store.entries.iter().collect();

    // Folder-post asset (`/{folder}/{asset…}`): resolved before the query parser,
    // which would otherwise misread the multi-segment path as a label.
    if let Some(resp) = try_asset(&all_entries, &path, &store.content_dir, &store.cache_dir, embed, thumb, as_jpeg, &raw_href).await {
        return resp;
    }

    // Nested subfolder listing (`/{folder}/{sub…}/`): a public subfolder browsed
    // as its own page. Same precedence reason as assets — the parser can't read a
    // multi-segment nested path. `try_asset` already handled nested files.
    if let Some(resp) = try_folder_listing(&all_entries, &path, &store.content_dir) {
        return resp;
    }

    // The time-of-day disambiguator is a URL path segment now (`/2026/07/04/191430`),
    // parsed straight off the path — no `?time=` query string.
    let query = parse_url_path(&format!("/{}", path));
    let view = view_from_query(&params);
    let requested = format!("/{}", path);

    // Bare `/name`: one flat namespace, the OLDEST claim (a label or an
    // `alias <name>/` marker) owns it, so a URL's meaning never changes.
    if is_bare_label(&query) {
        if let Some(owner) = templates::name_owner(query.label.as_deref().unwrap(), &all_entries) {
            return serve_resolved(owner, &all_entries, &store, &requested, &view, fullscreen).await;
        }
    }

    // Filter current entries matching the path query (label compared on slug).
    let matching: Vec<&Entry> = store
        .entries
        .iter()
        .filter(|e| query.matches(&e.timestamp, &e.slug, &e.tag_names()))
        .collect();

    // Raw file request (URL has extension like sunset.jpg)
    if let Some(ref raw_ext) = query.raw_extension {
        return serve_raw_file(&matching, raw_ext, &store.cache_dir, embed, as_jpeg, &raw_href).await;
    }

    // Listing: a trailing slash, or a date-only / tag-only filter with no label
    // and no `?time=` narrowing it to one entry. With a time present we fall
    // through to entry resolution (that's how an unlabeled entry is addressed).
    let is_scope_listing = query.label.is_none()
        && query.time.is_none()
        && (query.date_prefix.is_some() || !query.and_tags.is_empty() || !query.or_tags.is_empty());
    if (query.is_listing && query.time.is_none()) || is_scope_listing {
        return render_listing(&matching, &all_entries, &query, &view, &path);
    }

    // Resolve the request to one current entry:
    // - exactly one match serves (or 301s to its canonical address);
    // - a bare /label carried by several entries serves the OLDEST — newer
    //   claims stay reachable at their date-disambiguated addresses.
    let target: Option<&Entry> = if matching.len() == 1 {
        Some(matching[0])
    } else if query.label.is_some() && query.date_prefix.is_none() {
        oldest_of(&matching)
    } else {
        None
    };

    if let Some(entry) = target {
        return serve_resolved(entry, &all_entries, &store, &requested, &view, fullscreen).await;
    }

    // An archived revision addressed at its date path (+ time segment).
    if let Some((parent, rev)) = find_revision(&all_entries, &query) {
        return serve_revision(parent, rev, &store).await;
    }

    // A label-free date+time deeplink with no exact match lands on the day view,
    // never a 404 — an edited-that-day post is right there. (The citation
    // survives renames: it resolves by timestamp, or degrades to the day.)
    if query.label.is_none() && query.time.is_some() && query.date_prefix.is_some() {
        let mut day_q = query.clone();
        day_q.time = None;
        let day: Vec<&Entry> = store
            .entries
            .iter()
            .filter(|e| day_q.matches(&e.timestamp, &e.slug, &e.tag_names()))
            .collect();
        if !day.is_empty() {
            return render_listing(&day, &all_entries, &day_q, &view, &path);
        }
    }

    if matching.is_empty() {
        // A dead bare label (renamed away, or a typo): offer the timeline, search,
        // and closest-slug suggestions rather than auto-redirecting a reused name.
        // Served with 404, identically to any missing path (no existence oracle).
        if is_bare_label(&query) {
            if let Some(label) = query.label.as_deref() {
                return not_found_response(templates::not_found_label_page(label, &all_entries));
            }
        }
        return not_found();
    }

    // Multiple matches — show as listing
    render_listing(&matching, &all_entries, &query, &view, &path)
}

/// Whether a request is a bare `/name` (no date, time, extension, tag, or
/// trailing slash) — the form claim resolution applies to.
fn is_bare_label(q: &ContentQuery) -> bool {
    q.label.is_some()
        && q.date_prefix.is_none()
        && q.time.is_none()
        && q.raw_extension.is_none()
        && q.and_tags.is_empty()
        && q.or_tags.is_empty()
        && !q.is_listing
}

/// The single oldest entry in a set, or None on a timestamp tie (ambiguous).
fn oldest_of<'a>(entries: &[&'a Entry]) -> Option<&'a Entry> {
    let oldest = entries.iter().map(|e| e.timestamp).min()?;
    let mut at_oldest = entries.iter().filter(|e| e.timestamp == oldest);
    let first = at_oldest.next()?;
    if at_oldest.next().is_some() {
        None
    } else {
        Some(first)
    }
}

/// Serve an entry once it is resolved: 301 to its canonical address if the
/// request is not already there, otherwise render it (or its error page). Every
/// non-canonical URL collapses onto the one canonical address, query preserved.
async fn serve_resolved(
    entry: &Entry,
    all_entries: &[&Entry],
    store: &ContentStore,
    requested: &str,
    view: &ViewFilter,
    fullscreen: bool,
) -> Response {
    let canon = templates::canonical(entry, all_entries);
    if requested != canon.path {
        return redirect(&templates::canonical_location(entry, all_entries, view));
    }
    if entry.error.is_some() {
        return error_response(entry, all_entries);
    }
    serve_entry(entry, store, fullscreen).await
}

/// Render a post's fail-closed error page with HTTP 500 (loud, never hidden).
fn error_response(entry: &Entry, all_entries: &[&Entry]) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Html(templates::error_page(entry, all_entries)),
    )
        .into_response()
}

/// Resolve a folder-post asset request (`/{folder}/{asset…}`) to bytes on disk,
/// strictly inside that post's directory. Returns None when the path is not an
/// asset (wrong shape, unknown folder, a missing or escaping file), so the
/// caller falls through to normal resolution.
async fn try_asset(
    all_entries: &[&Entry],
    path: &str,
    content_dir: &std::path::Path,
    cache_dir: &std::path::Path,
    embed: bool,
    thumb: bool,
    as_jpeg: bool,
    raw_href: &str,
) -> Option<Response> {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segments.len() < 2 {
        return None;
    }
    let first = segments[0];
    // Never shadow the date hierarchy (`/2026/03/12/…`) with a same-named post.
    if first.len() == 4 && first.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }

    let owner = templates::name_owner(first, all_entries)?;
    let dir = owner.dir.as_ref()?; // only folder posts carry assets

    // Resolve the requested file and confirm it stays inside the post directory.
    let candidate = dir.join(segments[1..].join("/"));
    let canon_dir = std::fs::canonicalize(dir).ok()?;
    let canon_file = std::fs::canonicalize(&candidate).ok()?;
    if !canon_file.starts_with(&canon_dir) {
        return None; // path-traversal guard
    }
    if !std::fs::metadata(&canon_file).ok()?.is_file() {
        return None; // marker folders / subdirs are not assets
    }
    if canon_file
        .file_name()
        .and_then(|n| n.to_str())
        .map_or(true, |n| n.starts_with('.'))
    {
        return None; // dotfiles are never served
    }

    // The primary content is canonical at `/label(.ext)` — send duplicates there.
    // A file only *is* the primary when it carries its own `public` tag (the scan
    // demotes an untagged primary to a withheld listing), so this redirect never
    // routes around the per-file gate below.
    if std::fs::canonicalize(&owner.path).ok().as_deref() == Some(canon_file.as_path()) {
        let label = owner.label.as_deref().unwrap_or(first);
        let decoded = if owner.extension.is_empty() {
            format!("/{}", label)
        } else {
            format!("/{}.{}", label, owner.extension)
        };
        return Some(redirect(&templates::encode_path(&decoded)));
    }

    // Per-file visibility gate (strict — post-model.md [review]): every asset
    // needs its own `public` tag, and no component of its path may be `private`.
    // A hidden or untagged asset is treated as absent (fall through to a normal
    // 404), so it is indistinguishable from a missing one — no existence oracle.
    if !staticdrop_core::tags::path_visible(content_dir, &canon_file) {
        return None;
    }

    let bytes = std::fs::read(&canon_file).ok()?;
    let ext = canon_file.extension().and_then(|e| e.to_str()).unwrap_or("");
    // An in-folder HTML/XHTML asset is a standalone document too (model C): jail
    // it with the sandbox CSP, honoring `?embed` so a folder-hosted page can be
    // framed the same way a bare-file one is.
    if is_sandboxed_document(ext) {
        return Some(sandbox_html_response(bytes, embed, document_content_type(ext)));
    }
    // In-folder assets go through the same privacy gate as a primary (post-model.md
    // §8): images stripped/transcoded, author-readable text raw, everything else
    // withheld. The `public-original` opt-in is per file here, read from the
    // asset's own tags; on a withheld format it is the universal exact-bytes
    // escape (the author explicitly publishes whatever the file embeds).
    match staticdrop_core::media::classify(ext, &bytes) {
        staticdrop_core::media::Disposition::Image => {
            let is_original = file_serves_original(&canon_file);
            return Some(serve_image(bytes, ext, is_original, thumb, as_jpeg, raw_href, cache_dir).await);
        }
        staticdrop_core::media::Disposition::Pdf => {
            return Some(serve_pdf(bytes, file_serves_original(&canon_file), &canon_file));
        }
        staticdrop_core::media::Disposition::Svg => {
            return Some(serve_svg(bytes, file_serves_original(&canon_file), &canon_file));
        }
        staticdrop_core::media::Disposition::Withhold => {
            if !file_serves_original(&canon_file) {
                return Some(metadata_withheld());
            }
            tracing::warn!(
                "Serving {} exact bytes (public-original) — any embedded metadata is published",
                canon_file.display()
            );
        }
        staticdrop_core::media::Disposition::Raw => {}
    }
    let mime = raw_content_type(ext);
    Some(
        Response::builder()
            .status(StatusCode::OK)
            .header(
                header::CONTENT_TYPE,
                HeaderValue::from_str(&mime).unwrap_or(HeaderValue::from_static("application/octet-stream")),
            )
            .header(header::CACHE_CONTROL, "public, max-age=3600")
            .body(Body::from(bytes))
            .unwrap_or_else(|_| not_found()),
    )
}

/// Resolve a nested subfolder listing request (`/{folder}/{sub…}/`) to a
/// browsable directory strictly inside that post's folder. Returns None when the
/// path is not a nested directory (wrong shape, a file — handled by `try_asset` —
/// a hidden/escaping path, or the top-level `/label` itself), so the caller falls
/// through. Every path component must be `public` (fail-closed, deny-wins).
fn try_folder_listing(
    all_entries: &[&Entry],
    path: &str,
    content_dir: &std::path::Path,
) -> Option<Response> {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segments.len() < 2 {
        return None; // the top-level `/label` listing is served as its own post
    }
    let first = segments[0];
    // Never shadow the date hierarchy (`/2026/03/12/…`) with a same-named post.
    if first.len() == 4 && first.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }

    let owner = templates::name_owner(first, all_entries)?;
    let dir = match owner.dir.as_ref() {
        Some(d) => d,
        None => return None, // a bare-file post has no subfolders — fall through
    };

    // Past this point the request is scoped *into* a real folder post, so it
    // resolves here or 404s — it never falls through to the timeline. Visible
    // files were already served by `try_asset`; a hidden or missing path 404s
    // identically (no existence oracle), and so does a hidden nested listing.
    let resolve = || -> Option<Response> {
        let candidate = dir.join(segments[1..].join("/"));
        let canon_dir = std::fs::canonicalize(dir).ok()?;
        let canon_target = std::fs::canonicalize(&candidate).ok()?;
        if !canon_target.starts_with(&canon_dir) {
            return None; // path-traversal guard
        }
        if !std::fs::metadata(&canon_target).ok()?.is_dir() {
            return None; // a file is an asset (try_asset), not a listing
        }
        // Fail-closed: every component (the post, each subfolder) must be public.
        if !staticdrop_core::tags::path_visible(content_dir, &canon_target) {
            return None;
        }
        Some(render_nested_listing(&canon_target, first, &segments, all_entries))
    };
    Some(resolve().unwrap_or_else(not_found))
}

/// Render a resolved, visible nested subfolder as a listing page.
fn render_nested_listing(
    canon_target: &std::path::Path,
    first: &str,
    segments: &[&str],
    all_entries: &[&Entry],
) -> Response {
    let listing = staticdrop_core::content::build_dir_listing(canon_target);
    let title = canon_target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(first)
        .to_string();
    // The request path (no trailing slash) IS the canonical URL and the base each
    // item href hangs off — mirroring the filesystem verbatim.
    let base = format!("/{}", segments.join("/"));
    Html(templates::nested_listing_page(&base, &title, &listing, all_entries)).into_response()
}

/// Find the one archived revision a date-path request addresses, or None when
/// zero or several match (fail closed to normal resolution).
fn find_revision<'a>(
    all_entries: &[&'a Entry],
    query: &ContentQuery,
) -> Option<(&'a Entry, &'a Revision)> {
    let label = query.label.as_ref()?;
    let date_prefix = query.date_prefix.as_ref()?;
    let want = staticdrop_core::slug::slug(label)?;

    let mut found: Option<(&Entry, &Revision)> = None;
    let mut count = 0usize;
    for e in all_entries {
        if e.error.is_some() || e.slug.as_deref() != Some(want.as_str()) {
            continue;
        }
        for r in &e.revisions {
            let full = r.date.format("%Y-%m-%dT%H%M%S").to_string();
            if !full.starts_with(date_prefix.as_str()) {
                continue;
            }
            if let Some(ref t) = query.time {
                if !r.date.format("%H%M%S").to_string().starts_with(t.as_str()) {
                    continue;
                }
            }
            found = Some((*e, r));
            count += 1;
        }
    }

    (count == 1).then_some(()).and(found)
}

/// Serve an archived revision: render its own bytes, dated by its own mtime,
/// under the current post's chrome. Revisions are reachable only at date paths.
async fn serve_revision(parent: &Entry, rev: &Revision, store: &ContentStore) -> Response {
    let mut e = parent.clone();
    e.path = rev.path.clone();
    e.timestamp = staticdrop_core::postdate::PostDate::from_mtime(rev.date);
    e.edited = None;
    e.revisions = Vec::new();
    e.aliases = Vec::new();
    e.listing = None; // a revision serves specific bytes, never a listing
    e.extension = rev
        .path
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_string();
    serve_entry(&e, store, false).await
}

/// Render a filtered timeline listing: apply the view filter, build the shared
/// header reflecting the active topic and view, and hand off to the template.
fn render_listing(
    matching: &[&Entry],
    all_entries: &[&Entry],
    query: &ContentQuery,
    view: &ViewFilter,
    path: &str,
) -> Response {
    let display: Vec<&Entry> = matching.iter().copied().filter(|e| view.matches(e)).collect();
    let cloud = compute_cloud(all_entries);
    let base_path = format!("/{}", path);
    // The tag-only portion of the path, so month links can graft a date onto the
    // active topic and the date chip can clear back to just the tags.
    let path_tags = tag_suffix(path);
    let date_scope = query.date_prefix.as_deref().map(|prefix| templates::DateScope {
        label: human_date(prefix),
        clear_path: if path_tags.is_empty() { "/".to_string() } else { path_tags.clone() },
    });
    let ctx = HeaderContext {
        cloud: &cloud,
        active_tag: active_tag(query),
        view,
        base_path: &base_path,
        date_scope,
        path_tags: &path_tags,
        saved_view: false,
    };
    let title = filter_title(&build_filter_description(query), view, false);
    Html(templates::timeline_page(&display, all_entries, &ctx, &title)).into_response()
}

/// The `+tag` portion of a path (e.g. "/+design"), or "" when there is none —
/// used to keep the active topic while swapping or clearing the date scope.
fn tag_suffix(path: &str) -> String {
    path.trim_matches('/')
        .split('/')
        .filter(|s| s.starts_with('+'))
        .map(|s| format!("/{}", s))
        .collect()
}

/// Humanize a compact date prefix for the scope chip and the page title:
/// "2026" → "2026", "2026-03" → "March 2026", "2026-03-25" → "March 25, 2026".
fn human_date(prefix: &str) -> String {
    let parts: Vec<&str> = prefix.split('-').collect();
    match parts.as_slice() {
        [y] => y.to_string(),
        [y, m] => format!("{} {}", month_name(m), y),
        [y, m, d] => format!("{} {}, {}", month_name(m), d.trim_start_matches('0'), y),
        _ => prefix.to_string(),
    }
}

/// Full month name for a zero-padded "01".."12"; echoes the input if unknown.
fn month_name(m: &str) -> &str {
    const NAMES: [&str; 12] = [
        "January", "February", "March", "April", "May", "June", "July", "August",
        "September", "October", "November", "December",
    ];
    m.parse::<usize>()
        .ok()
        .filter(|n| (1..=12).contains(n))
        .map_or(m, |n| NAMES[n - 1])
}

/// GET /_embed/{key}/{asset_name} — serve a cached embed asset (OG images, etc.).
///
/// `key` is the content-relative-path hash the cache is keyed by; it must match
/// an entry the store actually knows about, so a removed or unpublished entry
/// can never have its cached media served (privacy fails closed). `asset_name`
/// must be a simple filename (no path separators). Anything else 404s.
pub async fn serve_embed_asset(
    State(store): State<AppState>,
    Path((key, asset_name)): Path<(String, String)>,
) -> Response {
    // Reject anything that even looks path-y. Sanitized to the same shape used when
    // writing the file (`download_media`'s safe_name filter), with no dots-only.
    if !is_safe_asset_segment(&key) || !is_safe_asset_segment(&asset_name) {
        return not_found();
    }

    let store = store.read().await;
    // Find the entry whose cache key matches. Lookup, not derivation: we serve
    // assets only for entries currently in the store (i.e. public).
    let entry = store
        .entries
        .iter()
        .find(|e| staticdrop_core::embed::cache_key(&store.content_dir, &e.path) == key);
    let entry = match entry {
        Some(e) => e,
        None => return not_found(),
    };

    let cache_dir =
        staticdrop_core::embed::cache_dir_for(&store.cache_dir, &store.content_dir, &entry.path);
    let asset_path = cache_dir.join(&asset_name);

    // Defense in depth: ensure the resolved path still sits inside cache_dir.
    if !asset_path.starts_with(&cache_dir) {
        return not_found();
    }

    let bytes = match std::fs::read(&asset_path) {
        Ok(b) => b,
        Err(_) => return not_found(),
    };

    let ext = asset_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    // `download_media` names a cached asset from the remote URL and does not vet
    // its content type, so a hostile `og:image` could land an HTML/XHTML document
    // here. Serve any such document jailed rather than as a trusted-origin page.
    if is_sandboxed_document(ext) {
        return sandbox_html_response(bytes, false, document_content_type(ext));
    }
    let mime = raw_content_type(ext);

    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_str(&mime).unwrap_or(HeaderValue::from_static("application/octet-stream")),
        )
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .body(Body::from(bytes))
        .unwrap_or_else(|_| not_found())
}

/// Allow only simple filename characters: alphanumeric plus `. - _`.
/// No path separators, no leading dot, no `..`.
fn is_safe_asset_segment(s: &str) -> bool {
    if s.is_empty() || s.starts_with('.') || s.contains("..") {
        return false;
    }
    s.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
}

/// POST /_rescan — re-scan content directory
pub async fn rescan(State(store): State<AppState>) -> impl IntoResponse {
    let mut store = store.write().await;
    match store.rescan() {
        Ok(()) => {
            store.resolve_embeds().await;
            (StatusCode::OK, format!("Rescanned. {} entries.", store.entries.len()))
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Rescan failed: {}", e)),
    }
}

/// Serve a single entry as a rendered HTML page. `fullscreen` selects the
/// fullscreen variant (B) for a standalone `.html` post; it is ignored otherwise.
async fn serve_entry(entry: &Entry, store: &ContentStore, fullscreen: bool) -> Response {
    let all: Vec<&Entry> = store.entries.iter().collect();

    // A listing folder renders as a browsable index, not a document. Its optional
    // intro doc is rendered through the same pipeline the post body uses.
    if let Some(listing) = &entry.listing {
        let intro_html = match &listing.intro {
            Some(p) => match std::fs::read(p) {
                Ok(bytes) => match render_entry(&listing.intro_ext, &bytes).await {
                    Ok(RenderedContent::Html(h)) => {
                        let h = staticdrop_core::embed::expand_inline_embeds(&h, &store.embed_cache);
                        Some(staticdrop_core::outbound::sanitize_body_links(&h))
                    }
                    Ok(RenderedContent::PreformattedText(t)) => {
                        Some(format!("<pre>{}</pre>", html_escape_content(&t)))
                    }
                    _ => None,
                },
                Err(_) => None,
            },
            None => None,
        };
        return Html(templates::listing_page(entry, listing, &all, intro_html.as_deref()))
            .into_response();
    }

    // The next-older entry feeds the Continue block at the foot of the post.
    let next: Option<&Entry> = all
        .iter()
        .copied()
        .filter(|e| e.timestamp < entry.timestamp)
        .max_by_key(|e| e.timestamp);

    // A standalone `.html` post is shown jailed, never merged into the trusted
    // shell: the post address embeds it in the shell (A) or, with `?fullscreen`,
    // hands it the whole viewport (B). Both wrap the same-origin asset URL in a
    // sandboxed iframe; the raw bytes (C) come from that URL (serve_raw_bytes).
    if is_sandboxed_document(&entry.extension) {
        return if fullscreen {
            Html(templates::standalone_fullscreen_page(entry, &all)).into_response()
        } else {
            Html(templates::standalone_embed_page(entry, &all, next)).into_response()
        };
    }

    // A link post *is* its destination (post-model.md §4): its body is the rich
    // embed card. A content post that merely *cites* a destination keeps its own
    // body and gets a cite section below it (handled by the render path + template),
    // so the card branch is gated to link posts only — otherwise a folder post with
    // a `link.*` sidecar (also in the embed cache, keyed by its primary) would be
    // hijacked into showing the card instead of its own words.
    if entry.kind() == "link" {
        if let Some(embed_data) = store.embed_cache.get(&entry.path) {
            if !embed_data.is_upstream_deleted() {
                let cache_dir =
                    staticdrop_core::embed::cache_dir_for(&store.cache_dir, &store.content_dir, &entry.path);
                let card_html = staticdrop_core::embed::render_embed_card(embed_data, &cache_dir);
                return Html(templates::entry_page(entry, &card_html, &all, next)).into_response();
            }
        }
        // A link post with no usable embed (fetch failed / upstream deleted / not
        // yet fetched): render the bare destination cite rather than fall through to
        // a raw `.webloc`/`.url` byte download, so the page stays a working link.
        let body = templates::bare_link_body(entry);
        return Html(templates::entry_page(entry, &body, &all, next)).into_response();
    }

    let content = match std::fs::read(&entry.path) {
        Ok(c) => c,
        Err(_) => return not_found(),
    };

    match render_entry(&entry.extension, &content).await {
        Ok(RenderedContent::Html(html)) => {
            // Expand inline URLs to embed cards, then run the fail-closed outbound
            // scheme guard as the last transform before the body enters the shell:
            // no unsafe-scheme anchor can survive, and every external link carries
            // rel="noreferrer" (post-model.md §7).
            let html = staticdrop_core::embed::expand_inline_embeds(&html, &store.embed_cache);
            let html = staticdrop_core::outbound::sanitize_body_links(&html);
            Html(templates::entry_page(entry, &html, &all, next)).into_response()
        }
        Ok(RenderedContent::Standalone(html)) => {
            Html(html).into_response()
        }
        Ok(RenderedContent::PreformattedText(text)) => {
            let pre = format!("<pre>{}</pre>", html_escape_content(&text));
            Html(templates::entry_page(entry, &pre, &all, next)).into_response()
        }
        Ok(RenderedContent::Embed(card_html)) => {
            Html(templates::entry_page(entry, &card_html, &all, next)).into_response()
        }
        Ok(RenderedContent::Image { mime }) => {
            // The exact-bytes opt-in is per file; the loud "publishes your
            // location/camera metadata" warning fires only when an `original`
            // image actually carries something sensitive to leak.
            let is_original = file_serves_original(&entry.path);
            let publishes_metadata = is_original && staticdrop_core::media::has_sensitive_metadata(&content);
            if publishes_metadata {
                tracing::warn!(
                    "Serving {} with embedded metadata intact (public-original tag)",
                    entry.path.display()
                );
            }
            Html(templates::image_page(entry, &mime, is_original, publishes_metadata, &all, next))
                .into_response()
        }
        Ok(RenderedContent::Download { .. }) => {
            let raw_href = templates::canonical_raw_href(entry, &all);
            serve_raw_bytes(entry, false, false, &raw_href, &store.cache_dir).await
        }
        Err(e) => {
            tracing::error!("Render error for {}: {}", entry.path.display(), e);
            let body = format!("<p>Rendering error: {}</p>", html_escape_content(&e));
            Html(templates::entry_page(entry, &body, &all, next)).into_response()
        }
    }
}

/// Serve raw file bytes with correct MIME type and Content-Disposition.
/// With duplicate labels, the oldest entry carrying the extension wins,
/// mirroring the page URL's oldest-claim-wins rule.
async fn serve_raw_file(
    matching: &[&Entry],
    requested_ext: &str,
    cache_dir: &std::path::Path,
    embed: bool,
    as_jpeg: bool,
    raw_href: &str,
) -> Response {
    let entry = matching
        .iter()
        .filter(|e| e.extension.eq_ignore_ascii_case(requested_ext))
        .min_by_key(|e| e.timestamp);

    match entry {
        Some(entry) => serve_raw_bytes(entry, embed, as_jpeg, raw_href, cache_dir).await,
        None => not_found(),
    }
}

async fn serve_raw_bytes(
    entry: &Entry,
    embed: bool,
    as_jpeg: bool,
    raw_href: &str,
    cache_dir: &std::path::Path,
) -> Response {
    let content = match std::fs::read(&entry.path) {
        Ok(c) => c,
        Err(_) => return not_found(),
    };

    // A standalone HTML/XHTML post's bytes are the byte-exact asset (model C):
    // served jailed by the sandbox CSP, with the height reporter appended only on
    // the `?embed` copy. Never the trusted origin, never a download prompt.
    if is_sandboxed_document(&entry.extension) {
        return sandbox_html_response(content, embed, document_content_type(&entry.extension));
    }

    // Privacy gate (post-model.md §8): an image is stripped/transcoded (unless
    // the author opted into the exact bytes); author-readable UTF-8 text serves
    // as-is; every other format is withheld — the boundary is an allowlist, so a
    // format nobody listed is a 415, never a metadata leak. The per-file
    // `public-original` tag serves the exact bytes of any withheld format.
    match staticdrop_core::media::classify(&entry.extension, &content) {
        staticdrop_core::media::Disposition::Image => {
            let is_original = file_serves_original(&entry.path);
            return serve_image(content, &entry.extension, is_original, false, as_jpeg, raw_href, cache_dir)
                .await;
        }
        staticdrop_core::media::Disposition::Pdf => {
            return serve_pdf(content, file_serves_original(&entry.path), &entry.path);
        }
        staticdrop_core::media::Disposition::Svg => {
            return serve_svg(content, file_serves_original(&entry.path), &entry.path);
        }
        staticdrop_core::media::Disposition::Withhold => {
            if !file_serves_original(&entry.path) {
                return metadata_withheld();
            }
            tracing::warn!(
                "Serving {} exact bytes (public-original) — any embedded metadata is published",
                entry.path.display()
            );
        }
        staticdrop_core::media::Disposition::Raw => {}
    }

    let mime = raw_content_type(&entry.extension);

    let filename = match &entry.label {
        Some(label) => format!("{}.{}", sanitize_filename(label), entry.extension),
        None => format!("{}.{}", entry.timestamp.file_stamp(), entry.extension),
    };

    let disposition = format!(r#"inline; filename="{}""#, filename);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, HeaderValue::from_str(&mime).unwrap_or(HeaderValue::from_static("application/octet-stream")))
        .header(header::CONTENT_DISPOSITION, HeaderValue::from_str(&disposition).unwrap_or(HeaderValue::from_static("inline")))
        .body(Body::from(content))
        .unwrap_or_else(|_| not_found())
}

/// Serve image bytes with location/camera metadata stripped for privacy
/// (`post-model.md` §8). `is_original` (the author's per-file `original` tag)
/// bypasses the strip and serves the exact source bytes. A format we cannot yet
/// clean, or one that fails to parse, is withheld — fail closed, never a silent
/// raw fallback that would leak the metadata we mean to remove.
/// Whether the exact-bytes opt-in applies to *this specific file* — read from
/// the file's own Finder tags, never inherited from a parent folder (post-model.md
/// §8). A tag that re-exposes embedded GPS must never cascade.
fn file_serves_original(path: &std::path::Path) -> bool {
    staticdrop_core::tags::read_tags_colored(path)
        .iter()
        .any(staticdrop_core::tags::Tag::is_original)
}

/// Serve a PDF with its document metadata stripped (`media::strip_pdf`), or the
/// exact bytes when the author's per-file `public-original` tag says so. A PDF
/// that cannot be verify-cleaned (encrypted, unparseable) is withheld — the
/// same fail-closed rule as every other format.
fn serve_pdf(bytes: Vec<u8>, is_original: bool, path: &std::path::Path) -> Response {
    if is_original {
        tracing::warn!(
            "Serving {} exact bytes (public-original) — any embedded metadata is published",
            path.display()
        );
        return clean_bytes_response(bytes, "application/pdf");
    }
    match staticdrop_core::media::strip_pdf(&bytes) {
        Some(clean) => clean_bytes_response(clean, "application/pdf"),
        None => metadata_withheld(),
    }
}

/// Serve an SVG with its metadata stripped (`media::strip_svg`), or the exact
/// bytes on the author's per-file `public-original` tag. Either way the
/// response is `image/svg+xml`, which the security middleware hard-jails
/// (`sandbox; default-src 'none'`) — the strip handles the metadata in the
/// bytes; the CSP handles script when the SVG is opened as a document.
fn serve_svg(bytes: Vec<u8>, is_original: bool, path: &std::path::Path) -> Response {
    if is_original {
        tracing::warn!(
            "Serving {} exact bytes (public-original) — any embedded metadata is published",
            path.display()
        );
        return clean_bytes_response(bytes, "image/svg+xml");
    }
    match staticdrop_core::media::strip_svg(&bytes) {
        Some(clean) => clean_bytes_response(clean, "image/svg+xml"),
        None => metadata_withheld(),
    }
}

async fn serve_image(
    bytes: Vec<u8>,
    ext: &str,
    is_original: bool,
    thumb: bool,
    as_jpeg: bool,
    raw_href: &str,
    cache_dir: &std::path::Path,
) -> Response {
    let norm = staticdrop_core::entry::normalize_ext(ext);
    let source_is_jpeg = matches!(norm.as_str(), "jpg" | "jpeg");

    // A gallery-tile thumbnail is a derived preview: always stripped/resized,
    // even for an `original`-tagged file (the exact bytes stay at its non-thumb
    // URL). Tiles are JPEG renditions, so a non-JPEG source's tile is served
    // only at its honest `?as=jpeg&thumb` address.
    if thumb {
        if !source_is_jpeg && !as_jpeg {
            return jpeg_rendition_redirect(raw_href, true);
        }
        return match staticdrop_core::media::thumbnail(ext, &bytes, cache_dir).await {
            staticdrop_core::media::Prepared::Ready(clean) => {
                let content_type = clean.content_type();
                clean_bytes_response(clean.into_bytes(), content_type)
            }
            staticdrop_core::media::Prepared::Withheld => metadata_withheld(),
        };
    }

    // `?as=jpeg` exists only where a JPEG rendition is the pipeline's own
    // output (transcode-only formats, or a JPEG source). No on-demand format
    // conversion for PNG/WebP — fail closed on the unexpected.
    if as_jpeg && !source_is_jpeg && !staticdrop_core::media::is_transcode_only_ext(ext) {
        return not_found();
    }

    // `public-original` exact bytes at the bare URL: the container matches the
    // extension, so the name is honest. `?as=jpeg` on an original still serves
    // the clean rendition (it is a derived representation, never a leak).
    if is_original && !as_jpeg {
        return clean_bytes_response(bytes, &raw_content_type(ext));
    }

    // The bare URL of a transcode-only format never serves JPEG bytes under a
    // foreign extension: answer with the honest address instead.
    if staticdrop_core::media::is_transcode_only_ext(ext) && !as_jpeg {
        return jpeg_rendition_redirect(raw_href, false);
    }

    match staticdrop_core::media::prepare(ext, &bytes, cache_dir).await {
        staticdrop_core::media::Prepared::Ready(clean) => {
            let content_type = clean.content_type();
            clean_bytes_response(clean.into_bytes(), content_type)
        }
        staticdrop_core::media::Prepared::Withheld => metadata_withheld(),
    }
}

/// 303 See Other to the honest address of a clean JPEG rendition. The plain-text
/// body explains what happened, so a client that does not follow redirects still
/// learns where the bytes are and how to publish the original instead.
fn jpeg_rendition_redirect(raw_href: &str, thumb: bool) -> Response {
    let location = if thumb {
        format!("{raw_href}?as=jpeg&thumb")
    } else {
        format!("{raw_href}?as=jpeg")
    };
    let value = match HeaderValue::from_str(&location) {
        Ok(v) => v,
        Err(_) => return not_found(), // fail closed on an unencodable path
    };
    let body = format!(
        "This file's format embeds metadata that cannot be removed in place, so its exact bytes are not served.\n\
         A clean JPEG rendition is at {location}\n\
         The author can tag the file public-original to publish the exact bytes instead.\n"
    );
    Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(header::LOCATION, value)
        .header(header::CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"))
        .body(Body::from(body))
        .unwrap_or_else(|_| not_found())
}

/// Build the HTTP response for verify-cleaned bytes (a stripped image or PDF,
/// or an author-opted exact original): a strong ETag derived from the *served*
/// bytes (so it changes iff the served bytes change) and a revalidatable cache
/// window. The strips are deterministic, so identical source bytes always
/// yield the same ETag.
fn clean_bytes_response(bytes: Vec<u8>, content_type: &str) -> Response {
    let etag = format!("\"{}\"", staticdrop_core::media::content_hash(&bytes));
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_str(content_type)
                .unwrap_or(HeaderValue::from_static("application/octet-stream")),
        )
        .header(header::ETAG, HeaderValue::from_str(&etag).unwrap_or(HeaderValue::from_static("\"0\"")))
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(Body::from(bytes))
        .unwrap_or_else(|_| not_found())
}

/// Fail-closed response when a file cannot be verify-cleaned (a format without a
/// strip path, or corrupt bytes). The page around it still renders; only the
/// bytes are withheld, so nothing with unvetted embedded metadata is ever
/// served. The message names the per-file opt-out so the author knows the way
/// through, without revealing anything to a reader beyond "not served".
fn metadata_withheld() -> Response {
    Response::builder()
        .status(StatusCode::UNSUPPORTED_MEDIA_TYPE)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from(
            "Withheld for privacy: this file may embed metadata (location, device, author) \
             that cannot be verified or removed. The site owner can publish its exact bytes \
             by tagging the file public-original.\n",
        ))
        .unwrap_or_else(|_| not_found())
}

/// Whether a served document must be jailed as a sandboxed standalone page: the
/// deliberate `.html` drop-in feature plus every HTML/XHTML-family document that
/// would otherwise render as a script-capable top-level document (`.htm`,
/// `.shtml`, `.xhtml`, `.xht`, ...). Delegates to the one canonical predicate
/// (`entry::is_html_document`, MIME-keyed) so the sandbox gate can never drift
/// from the `kind()`/render classification — the drift the old `html`/`htm`-only
/// check caused (`.xhtml` ran script in our origin). Non-HTML document types
/// (`.xml`, `.mathml`, ...) are not served functional-jailed here; the header
/// middleware still hard-jails them via the fail-closed default.
fn is_sandboxed_document(ext: &str) -> bool {
    staticdrop_core::entry::is_html_document(ext)
}

/// The height reporter injected into the `?embed` copy of a standalone document.
/// It runs inside the jailed iframe (whose CSP permits inline script) and posts
/// its measured height to the parent, which sizes the frame (see the site JS).
/// Only the `?embed` copy carries it; the bare asset URL stays byte-exact.
///
/// The `<script>` *content* is kept free of `<`, `>`, and `&`, so the element is
/// well-formed when the document is served as XML (XHTML) — no CDATA needed.
const EMBED_REPORTER: &str = r#"<script>(function(){
  function h(){var d=document;return Math.max(d.documentElement.scrollHeight, d.body?d.body.scrollHeight:0);}
  function send(){try{parent.postMessage({eskoEmbedHeight:h()},"*");}catch(e){}}
  if(window.ResizeObserver){new ResizeObserver(send).observe(document.documentElement);}
  window.addEventListener("load",send);send();
})();</script>
"#;

/// Insert the reporter *before* the document's closing `</body>` (or `</html>`),
/// so it stays valid even in XML mode (XHTML), where any content after the root
/// element is a fatal parse error — appending after `</html>` would break strict
/// XHTML. Falls back to appending only when neither tag is present (a well-formed
/// XHTML always has them; a malformed one is the author's responsibility).
fn inject_reporter(bytes: Vec<u8>) -> Vec<u8> {
    // Locate the splice point (byte index of the closing tag) while borrowing,
    // then release the borrow before taking ownership of `bytes`.
    let insert_at = match std::str::from_utf8(&bytes) {
        Ok(s) => {
            let lower = s.to_ascii_lowercase();
            lower.rfind("</body>").or_else(|| lower.rfind("</html>"))
        }
        Err(_) => None,
    };
    match insert_at {
        Some(idx) => {
            let mut out = Vec::with_capacity(bytes.len() + EMBED_REPORTER.len());
            out.extend_from_slice(&bytes[..idx]);
            out.extend_from_slice(EMBED_REPORTER.as_bytes());
            out.extend_from_slice(&bytes[idx..]);
            out
        }
        None => {
            let mut b = bytes;
            b.extend_from_slice(EMBED_REPORTER.as_bytes());
            b
        }
    }
}

/// The content type for a sandboxed standalone document: XHTML is served as the
/// strict XML type it asks for (`application/xhtml+xml`, so the browser XML-parses
/// it exactly as the author intended — choosing `.xhtml` is the opt-in), while
/// HTML gets the lenient `text/html`. The file extension is the switch.
fn document_content_type(ext: &str) -> &'static str {
    match mime_guess::from_ext(ext).first_or_octet_stream().essence_str() {
        "application/xhtml+xml" => "application/xhtml+xml; charset=utf-8",
        _ => "text/html; charset=utf-8",
    }
}

/// The content type for a raw file that is NOT served as a sandboxed standalone
/// document. Any type `mime_guess` would render as HTML/XHTML but that we do not
/// treat as a document (`.shtml` and other SSI/HTML-help extensions — we do not
/// process Server-Side Includes) is downgraded to `text/plain`, so it shows as
/// source rather than rendering un-jailed in our origin. Everything else keeps
/// its guessed type (non-HTML documents like SVG/XML are still hard-jailed by the
/// header middleware's fail-closed default).
fn raw_content_type(ext: &str) -> String {
    let mime = mime_guess::from_ext(ext).first_or_octet_stream();
    match mime.essence_str() {
        "text/html" | "application/xhtml+xml" => "text/plain; charset=utf-8".to_string(),
        _ => mime.to_string(),
    }
}

/// Serve a standalone HTML/XHTML document's bytes, jailed by the sandbox CSP and
/// served with its intended content type (`content_type`). The `?embed` copy gets
/// the reporter injected well-formed; the bare copy is byte-exact. The CSP is set
/// here so the header middleware leaves it alone; nosniff is added by the
/// middleware (and, on the strict XML type, keeps the browser XML-parsing it).
fn sandbox_html_response(bytes: Vec<u8>, embed: bool, content_type: &'static str) -> Response {
    let body = if embed { inject_reporter(bytes) } else { bytes };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_SECURITY_POLICY, staticdrop_core::security::STANDALONE_CSP)
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(Body::from(body))
        .unwrap_or_else(|_| not_found())
}

fn redirect(location: &str) -> Response {
    // Canonical paths are percent-encoded (ASCII), so this only fails on a
    // hostile/broken query string — in which case fail closed with a 404
    // rather than panic the worker.
    let value = match HeaderValue::from_str(location) {
        Ok(v) => v,
        Err(_) => return not_found(),
    };
    Response::builder()
        .status(StatusCode::MOVED_PERMANENTLY)
        .header(header::LOCATION, value)
        .body(Body::empty())
        .unwrap()
}

fn not_found() -> Response {
    not_found_response(templates::not_found_page())
}

/// A 404 response wrapping a specific not-found page body. Every not-found path
/// shares this shape, so a hidden path is indistinguishable from a missing one.
fn not_found_response(body: String) -> Response {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(body))
        .unwrap()
}

fn build_filter_description(query: &staticdrop_core::url::ContentQuery) -> String {
    let mut parts = Vec::new();
    if let Some(ref dp) = query.date_prefix {
        parts.push(human_date(dp));
    }
    for tag in &query.and_tags {
        parts.push(format!("+{}", tag));
    }
    for tag in &query.or_tags {
        parts.push(format!("+{}", tag));
    }
    if let Some(ref label) = query.label {
        parts.push(label.clone());
    }
    parts.join(" ")
}

fn sanitize_filename(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control() && *c != '"' && *c != '\\')
        .collect()
}

fn html_escape_content(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
