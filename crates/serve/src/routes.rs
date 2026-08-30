//! The axum skin over `staticdrop_core::page`: every handler decodes the
//! request into a path + `RequestFlags`, calls the page layer, and converts
//! its framework-free `Reply` into an axum response. All resolution and
//! rendering logic lives in core, so the closure builder walks the exact
//! same code (staticdrop.md).

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use staticdrop_core::content::ContentStore;
use staticdrop_core::page::{self, Reply, RequestFlags};

pub type AppState = Arc<RwLock<ContentStore>>;

/// Decode the query string into the page layer's request flags: the
/// standalone-document selectors (`?embed`, `?fullscreen`, presence flags)
/// and the one query filter (`?search=`).
fn flags_of(params: &HashMap<String, String>) -> RequestFlags {
    RequestFlags {
        embed: params.contains_key("embed"),
        fullscreen: params.contains_key("fullscreen"),
        search: params.get("search").cloned(),
    }
}

/// Convert a page-layer `Reply` into an axum response. Header names are
/// compile-time constants and values are built from vetted inputs, so an
/// unencodable value is a bug — fail closed to a plain 500 rather than serve
/// a half-headed response.
fn to_response(reply: Reply) -> Response {
    let mut builder = Response::builder()
        .status(StatusCode::from_u16(reply.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR));
    for (name, value) in &reply.headers {
        let name = match HeaderName::from_bytes(name.as_bytes()) {
            Ok(n) => n,
            Err(_) => return internal_error(),
        };
        let value = match HeaderValue::from_str(value) {
            Ok(v) => v,
            Err(_) => return internal_error(),
        };
        builder = builder.header(name, value);
    }
    builder
        .body(Body::from(reply.body))
        .unwrap_or_else(|_| internal_error())
}

fn internal_error() -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
}

/// GET / — timeline.
pub async fn index(
    State(store): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let store = store.read().await;
    to_response(page::respond(&store, "/", &flags_of(&params)).await)
}

/// GET /saved — the reader's saved bookmarks.
pub async fn saved(
    State(store): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let store = store.read().await;
    to_response(page::respond(&store, "/saved", &flags_of(&params)).await)
}

/// GET /static/{*path} — static assets, served from `./static` beside the
/// generated compile-time assets.
pub async fn serve_static(Path(path): Path<String>) -> Response {
    to_response(page::static_asset(std::path::Path::new("static"), &path))
}

/// GET /_embed/{key}/{asset_name} — cached embed assets (OG images, etc.).
pub async fn serve_embed_asset(
    State(store): State<AppState>,
    Path((key, asset_name)): Path<(String, String)>,
) -> Response {
    let store = store.read().await;
    to_response(page::embed_asset(&store, &key, &asset_name))
}

/// GET /{*path} — everything else: scopes, posts, raw files, renditions,
/// folder assets, nested listings.
pub async fn catch_all(
    State(store): State<AppState>,
    Path(path): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let store = store.read().await;
    to_response(page::respond(&store, &path, &flags_of(&params)).await)
}

/// POST /_rescan — re-scan content directory.
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header;

    #[test]
    fn reply_conversion_preserves_status_and_headers() {
        let reply = Reply {
            provenance: staticdrop_core::page::Provenance::Generated,
            status: 301,
            headers: vec![("location", "/2026/+design/notable".to_string())],
            body: Vec::new(),
        };
        let resp = to_response(reply);
        assert_eq!(resp.status(), StatusCode::MOVED_PERMANENTLY);
        assert_eq!(
            resp.headers().get(header::LOCATION).and_then(|v| v.to_str().ok()),
            Some("/2026/+design/notable")
        );
    }

    #[test]
    fn invalid_header_value_fails_closed() {
        let reply = Reply {
            provenance: staticdrop_core::page::Provenance::Generated,
            status: 200,
            headers: vec![("location", "bad\nvalue".to_string())],
            body: Vec::new(),
        };
        assert_eq!(to_response(reply).status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
