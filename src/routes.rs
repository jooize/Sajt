use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::content::ContentStore;
use crate::entry::Entry;
use crate::render::{render_entry, RenderedContent};
use crate::stats::{compute_cloud, ViewFilter};
use crate::templates::{self, HeaderContext};
use crate::url::{parse_url_path, ContentQuery};

pub type AppState = Arc<RwLock<ContentStore>>;

/// Parse the query string into a view filter (level / favorites / search).
fn view_from_query(params: &HashMap<String, String>) -> ViewFilter {
    let fav = params
        .get("fav")
        .map_or(false, |v| v != "0" && v != "false");
    ViewFilter::from_params(
        params.get("level").map(String::as_str),
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
    if view.level > 0 {
        parts.push(view.level_word().to_string());
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
    let query = parse_url_path(&format!("/{}", path));
    let view = view_from_query(&params);
    let store = store.read().await;

    let all_entries: Vec<&Entry> = store.entries.iter().collect();

    // Filter entries matching the path query
    let matching: Vec<&Entry> = store
        .entries
        .iter()
        .filter(|e| query.matches(&e.timestamp, &e.label, &e.tag_names()))
        .collect();

    // Raw file request (URL has extension like sunset.jpg)
    if let Some(ref raw_ext) = query.raw_extension {
        return serve_raw_file(&matching, raw_ext).await;
    }

    // Listing (trailing slash or date-only or tag-only filter)
    if query.is_listing || (query.label.is_none() && (query.date_prefix.is_some() || !query.and_tags.is_empty() || !query.or_tags.is_empty())) {
        return render_listing(&matching, &all_entries, &query, &view, &path);
    }

    // If date+label URL and a single match with a unique label, redirect to label-only URL
    if query.date_prefix.is_some() && query.label.is_some() && matching.len() == 1 {
        if let Some(ref label) = matching[0].label {
            if is_label_unique_in(label, &all_entries) {
                return redirect(&format!("/{}", label));
            }
        }
    }

    // Timestamp-only URL for an entry that has a unique label: redirect to label URL
    if query.date_prefix.is_some() && query.label.is_none() && matching.len() == 1 {
        if let Some(ref label) = matching[0].label {
            if is_label_unique_in(label, &all_entries) {
                return redirect(&format!("/{}", label));
            }
        }
    }

    // Single entry
    if matching.len() == 1 {
        let unique = matching[0]
            .label
            .as_ref()
            .map_or(false, |l| is_label_unique_in(l, &all_entries));
        return serve_entry(matching[0], &store, unique).await;
    }

    if matching.is_empty() {
        return not_found();
    }

    // Multiple matches — show as listing
    render_listing(&matching, &all_entries, &query, &view, &path)
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
    let ctx = HeaderContext {
        cloud: &cloud,
        active_tag: active_tag(query),
        view,
        base_path: &base_path,
        saved_view: false,
    };
    let title = filter_title(&build_filter_description(query), view, false);
    Html(templates::timeline_page(&display, all_entries, &ctx, &title)).into_response()
}

/// GET /_embed/{entry_name}/{asset_name} — serve a cached sidecar asset (OG images, etc.).
///
/// `entry_name` must match the filename of a content entry that the store actually knows
/// about (so this can't be used to traverse arbitrary `.embed-cache` directories on disk);
/// `asset_name` must be a simple filename (no path separators). Anything else 404s.
pub async fn serve_embed_asset(
    State(store): State<AppState>,
    Path((entry_name, asset_name)): Path<(String, String)>,
) -> Response {
    // Reject anything that even looks path-y. Sanitized to the same shape used when
    // writing the file (`download_media`'s safe_name filter), with no dots-only.
    if !is_safe_asset_segment(&entry_name) || !is_safe_asset_segment(&asset_name) {
        return not_found();
    }

    let store = store.read().await;
    // Find an entry whose filename matches the requested name. Lookup, not derivation:
    // we don't want to serve files for entries that have been removed from the store.
    let entry = store
        .entries
        .iter()
        .find(|e| e.path.file_name().and_then(|n| n.to_str()) == Some(entry_name.as_str()));
    let entry = match entry {
        Some(e) => e,
        None => return not_found(),
    };

    let cache_dir = crate::embed::cache_dir_for(&entry.path);
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

/// Check if a label is unique across all entries.
fn is_label_unique_in(label: &str, entries: &[&Entry]) -> bool {
    let lower = label.to_lowercase();
    entries
        .iter()
        .filter(|e| e.label.as_ref().map_or(false, |l| l.to_lowercase() == lower))
        .count()
        <= 1
}

/// Serve a single entry as a rendered HTML page.
async fn serve_entry(entry: &Entry, store: &ContentStore, label_unique: bool) -> Response {
    let all: Vec<&Entry> = store.entries.iter().collect();

    // Check if this entry has a cached embed
    if let Some(embed_data) = store.embed_cache.get(&entry.path) {
        if !embed_data.is_upstream_deleted() {
            let cache_dir = crate::embed::cache_dir_for(&entry.path);
            let card_html = crate::embed::render_embed_card(embed_data, &cache_dir);
            return Html(templates::entry_page(entry, &card_html, label_unique, &all)).into_response();
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
            Html(templates::entry_page(entry, &html, label_unique, &all)).into_response()
        }
        Ok(RenderedContent::Standalone(html)) => {
            Html(html).into_response()
        }
        Ok(RenderedContent::PreformattedText(text)) => {
            let pre = format!("<pre>{}</pre>", html_escape_content(&text));
            Html(templates::entry_page(entry, &pre, label_unique, &all)).into_response()
        }
        Ok(RenderedContent::Embed(card_html)) => {
            Html(templates::entry_page(entry, &card_html, label_unique, &all)).into_response()
        }
        Ok(RenderedContent::Image { mime }) => {
            Html(templates::image_page(entry, &mime, label_unique, &all)).into_response()
        }
        Ok(RenderedContent::Download { .. }) => {
            serve_raw_bytes(entry).await
        }
        Err(e) => {
            tracing::error!("Render error for {}: {}", entry.path.display(), e);
            let body = format!("<p>Rendering error: {}</p>", html_escape_content(&e));
            Html(templates::entry_page(entry, &body, label_unique, &all)).into_response()
        }
    }
}

/// Serve raw file bytes with correct MIME type and Content-Disposition.
async fn serve_raw_file(matching: &[&Entry], requested_ext: &str) -> Response {
    let entry = matching
        .iter()
        .find(|e| e.extension.eq_ignore_ascii_case(requested_ext));

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
    Response::builder()
        .status(StatusCode::MOVED_PERMANENTLY)
        .header(header::LOCATION, HeaderValue::from_str(location).unwrap())
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
        parts.push(dp.clone());
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
