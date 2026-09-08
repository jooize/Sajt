//! Axum middleware stamping the security header policy from
//! `sajt::security` onto every response. Policy values live in
//! core so the future static build emits identical headers; this module only
//! does the HTTP plumbing.

use axum::extract::Request;
use axum::http::{header, HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;
use sajt::security;

/// Stamp hardening headers onto every response, and a content-type-appropriate
/// CSP where the handler has not already chosen one.
pub async fn headers(req: Request, next: Next) -> Response {
    let mut resp = next.run(req).await;
    let h = resp.headers_mut();

    // Universal — on every response, overriding anything upstream set. The
    // names and values are compile-time constants vetted for `from_static`.
    for (name, value) in security::UNIVERSAL_HEADERS {
        h.insert(HeaderName::from_static(name), HeaderValue::from_static(value));
    }

    // CSP by content type, unless the handler set its own (e.g. the sandboxed
    // standalone-HTML asset, which needs a bespoke sandbox policy).
    if !h.contains_key(header::CONTENT_SECURITY_POLICY) {
        let ct = h
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let csp = security::csp_for_content_type(ct);
        h.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(csp));
    }

    resp
}
