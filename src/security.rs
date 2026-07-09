//! Security response headers.
//!
//! A single middleware stamps every response with the universal hardening
//! headers and a Content-Security-Policy chosen by the response's content type.
//! It never downgrades a CSP the handler already set — that lets the sandboxed
//! standalone-HTML path (S2.4) ship its own, looser-but-jailed policy while
//! everything else falls closed to the strict page policy here.

use axum::extract::Request;
use axum::http::{header, HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;

/// Strict policy for our own rendered pages: everything same-origin, no inline
/// script or style (both are externalized to `/static`), no plugins, no external
/// framing. `frame-src 'self'` admits the same-origin standalone-HTML iframe.
pub const PAGE_CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self'; \
img-src 'self' data:; frame-src 'self'; object-src 'none'; base-uri 'none'; \
form-action 'self'; frame-ancestors 'self'";

/// Fail-closed jail for any response whose content type we do not explicitly
/// recognize, and for a raw SVG served as a top-level `image/svg+xml` document (a
/// script vector otherwise). `sandbox` (no allow-* tokens) is the most
/// restrictive sandbox; `default-src 'none'` blocks any subresource. This is
/// harmless where it does not apply: a browser ignores a *subresource's* own CSP
/// (so `/static/site.js`/`.css` and `<img>`-loaded SVGs are unaffected), and it
/// only bites when the response is rendered as a top-level document — which is
/// exactly the case we must fail closed on (a stray `.xhtml`/`.xml`/`.mathml`
/// that would otherwise run script in our origin).
pub const JAIL_CSP: &str = "sandbox; default-src 'none'";

/// Policy for a dropped-in standalone `.html` document (the A/B/C model). It is
/// the one place inline script/style are *allowed* — that is the feature — but
/// `sandbox` puts the document in an opaque origin (no `allow-same-origin`), so
/// it cannot touch cookies, storage, or the parent DOM, and `default-src 'none'`
/// + `img-src 'self' data:` deny every third-party fetch (authors inline their
/// assets). The `sandbox` directive jails a direct top-level navigation too, so
/// the byte-exact asset is safe even outside an iframe. Set by the handler, so
/// the header middleware leaves it untouched.
pub const STANDALONE_CSP: &str = "sandbox allow-scripts allow-popups; \
default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; \
img-src 'self' data:";

/// Cross-Origin-Opener-Policy — no typed const exists in `http`.
const COOP: HeaderName = HeaderName::from_static("cross-origin-opener-policy");

/// Stamp hardening headers onto every response, and a content-type-appropriate
/// CSP where the handler has not already chosen one.
pub async fn headers(req: Request, next: Next) -> Response {
    let mut resp = next.run(req).await;
    let h = resp.headers_mut();

    // Universal — on every response, overriding anything upstream set.
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    h.insert(COOP, HeaderValue::from_static("same-origin"));

    // CSP by content type, unless the handler set its own (e.g. the sandboxed
    // standalone-HTML asset, which needs a bespoke sandbox policy).
    if !h.contains_key(header::CONTENT_SECURITY_POLICY) {
        let ct = h
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        // Fail closed: anything not recognized as one of our own HTML pages gets
        // the hard jail, so an unforeseen script-capable document type (e.g.
        // `application/xhtml+xml`) can never execute in our origin by default.
        let csp = if ct.starts_with("text/html") {
            PAGE_CSP
        } else {
            JAIL_CSP
        };
        h.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(csp));
    }

    resp
}
