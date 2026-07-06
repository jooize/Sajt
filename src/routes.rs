use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::content::ContentStore;
use crate::entry::{Entry, Revision};
use crate::render::{render_entry, RenderedContent};
use crate::stats::{compute_cloud, ViewFilter};
use crate::templates::{self, HeaderContext};
use crate::url::{parse_url_path, ContentQuery};

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

    let store = store.read().await;
    let all_entries: Vec<&Entry> = store.entries.iter().collect();

    // Folder-post asset (`/{folder}/{asset…}`): resolved before the query parser,
    // which would otherwise misread the multi-segment path as a label.
    if let Some(resp) = try_asset(&all_entries, &path) {
        return resp;
    }

    let mut query = parse_url_path(&format!("/{}", path));
    // The `?time=` disambiguator lives in the query string, not the path. Accept
    // only a left-anchored HHMMSS prefix of digits; anything else is ignored
    // (an unmatched time simply yields no results — fail closed).
    query.time = params
        .get("time")
        .map(|t| t.trim())
        .filter(|t| !t.is_empty() && t.len() <= 6 && t.bytes().all(|b| b.is_ascii_digit()))
        .map(|t| t.to_string());
    let view = view_from_query(&params);
    let requested = format!("/{}", path);

    // Bare `/name`: one flat namespace, the OLDEST claim (a label or an
    // `alias <name>/` marker) owns it, so a URL's meaning never changes.
    if is_bare_label(&query) {
        if let Some(owner) = templates::name_owner(query.label.as_deref().unwrap(), &all_entries) {
            return serve_resolved(owner, &all_entries, &store, &requested, &query.time, &view).await;
        }
    }

    // Filter current entries matching the path query.
    let matching: Vec<&Entry> = store
        .entries
        .iter()
        .filter(|e| query.matches(&e.timestamp, &e.label, &e.tag_names()))
        .collect();

    // Raw file request (URL has extension like sunset.jpg)
    if let Some(ref raw_ext) = query.raw_extension {
        return serve_raw_file(&matching, raw_ext).await;
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
        return serve_resolved(entry, &all_entries, &store, &requested, &query.time, &view).await;
    }

    // An archived revision addressed at its date path (+ `?time=`).
    if let Some((parent, rev)) = find_revision(&all_entries, &query) {
        return serve_revision(parent, rev, &store).await;
    }

    if matching.is_empty() {
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
    req_time: &Option<String>,
    view: &ViewFilter,
) -> Response {
    let canon = templates::canonical(entry, all_entries);
    if requested != canon.path || *req_time != canon.time {
        return redirect(&templates::canonical_location(entry, all_entries, view));
    }
    if entry.error.is_some() {
        return error_response(entry, all_entries);
    }
    serve_entry(entry, store).await
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
fn try_asset(all_entries: &[&Entry], path: &str) -> Option<Response> {
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
    if std::fs::canonicalize(&owner.path).ok().as_deref() == Some(canon_file.as_path()) {
        let label = owner.label.as_deref().unwrap_or(first);
        let decoded = if owner.extension.is_empty() {
            format!("/{}", label)
        } else {
            format!("/{}.{}", label, owner.extension)
        };
        return Some(redirect(&templates::encode_path(&decoded)));
    }

    let bytes = std::fs::read(&canon_file).ok()?;
    let ext = canon_file.extension().and_then(|e| e.to_str()).unwrap_or("");
    let mime = mime_guess::from_ext(ext).first_or_octet_stream().to_string();
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

/// Find the one archived revision a date-path request addresses, or None when
/// zero or several match (fail closed to normal resolution).
fn find_revision<'a>(
    all_entries: &[&'a Entry],
    query: &ContentQuery,
) -> Option<(&'a Entry, &'a Revision)> {
    let label = query.label.as_ref()?;
    let date_prefix = query.date_prefix.as_ref()?;
    let lower = label.to_lowercase();

    let mut found: Option<(&Entry, &Revision)> = None;
    let mut count = 0usize;
    for e in all_entries {
        if e.error.is_some() || e.label.as_ref().map_or(true, |l| l.to_lowercase() != lower) {
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
    e.timestamp = rev.date;
    e.edited = None;
    e.revisions = Vec::new();
    e.aliases = Vec::new();
    e.extension = rev
        .path
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_string();
    serve_entry(&e, store).await
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
        .find(|e| crate::embed::cache_key(&store.content_dir, &e.path) == key);
    let entry = match entry {
        Some(e) => e,
        None => return not_found(),
    };

    let cache_dir =
        crate::embed::cache_dir_for(&store.cache_dir, &store.content_dir, &entry.path);
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
    let mime = mime_guess::from_ext(ext)
        .first_or_octet_stream()
        .to_string();

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

/// Serve a single entry as a rendered HTML page.
async fn serve_entry(entry: &Entry, store: &ContentStore) -> Response {
    let all: Vec<&Entry> = store.entries.iter().collect();

    // The next-older entry feeds the Continue block at the foot of the post.
    let next: Option<&Entry> = all
        .iter()
        .copied()
        .filter(|e| e.timestamp < entry.timestamp)
        .max_by_key(|e| e.timestamp);

    // Check if this entry has a cached embed
    if let Some(embed_data) = store.embed_cache.get(&entry.path) {
        if !embed_data.is_upstream_deleted() {
            let cache_dir =
                crate::embed::cache_dir_for(&store.cache_dir, &store.content_dir, &entry.path);
            let card_html = crate::embed::render_embed_card(embed_data, &cache_dir);
            return Html(templates::entry_page(entry, &card_html, &all, next)).into_response();
        }
    }

    let content = match std::fs::read(&entry.path) {
        Ok(c) => c,
        Err(_) => return not_found(),
    };

    match render_entry(&entry.extension, &content).await {
        Ok(RenderedContent::Html(html)) => {
            // Expand inline URLs in rendered HTML to embed cards
            let html = crate::embed::expand_inline_embeds(&html, &store.embed_cache);
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
            Html(templates::image_page(entry, &mime, &all, next)).into_response()
        }
        Ok(RenderedContent::Download { .. }) => {
            serve_raw_bytes(entry).await
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
async fn serve_raw_file(matching: &[&Entry], requested_ext: &str) -> Response {
    let entry = matching
        .iter()
        .filter(|e| e.extension.eq_ignore_ascii_case(requested_ext))
        .min_by_key(|e| e.timestamp);

    match entry {
        Some(entry) => serve_raw_bytes(entry).await,
        None => not_found(),
    }
}

async fn serve_raw_bytes(entry: &Entry) -> Response {
    let content = match std::fs::read(&entry.path) {
        Ok(c) => c,
        Err(_) => return not_found(),
    };

    let mime = mime_guess::from_ext(&entry.extension)
        .first_or_octet_stream()
        .to_string();

    let filename = match &entry.label {
        Some(label) => format!("{}.{}", sanitize_filename(label), entry.extension),
        None => format!(
            "{}.{}",
            entry.timestamp.format("%Y-%m-%dT%H%M%S"),
            entry.extension
        ),
    };

    let disposition = format!(r#"inline; filename="{}""#, filename);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, HeaderValue::from_str(&mime).unwrap_or(HeaderValue::from_static("application/octet-stream")))
        .header(header::CONTENT_DISPOSITION, HeaderValue::from_str(&disposition).unwrap_or(HeaderValue::from_static("inline")))
        .body(Body::from(content))
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
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(templates::not_found_page()))
        .unwrap()
}

fn build_filter_description(query: &crate::url::ContentQuery) -> String {
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
