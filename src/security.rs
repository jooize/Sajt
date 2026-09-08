//! Security header policy.
//!
//! Pure policy, no web-framework types: the CSPs and the universal hardening
//! headers live here so crates/serve (the axum middleware in its `security`
//! module) and the future crates/build (manifest `headers` section) stamp the
//! exact same values and can never diverge. The handler-chosen standalone CSP
//! is never downgraded — that lets the sandboxed standalone-HTML path (S2.4)
//! ship its own, looser-but-jailed policy while everything else falls closed
//! to the strict page policy here.

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

/// Hardening headers stamped on every response, overriding anything upstream
/// set. Plain name/value pairs (lowercase names, valid `from_static` input)
/// so a host adapter can copy them into `_headers`/Caddyfile output verbatim.
pub const UNIVERSAL_HEADERS: &[(&str, &str)] = &[
    ("x-content-type-options", "nosniff"),
    ("referrer-policy", "no-referrer"),
    ("cross-origin-opener-policy", "same-origin"),
];

/// The default CSP for a response of the given content type, applied only
/// where the handler did not choose one itself. Fail closed: anything not
/// recognized as one of our own HTML pages gets the hard jail, so an
/// unforeseen script-capable document type (e.g. `application/xhtml+xml`) can
/// never execute in our origin by default.
pub fn csp_for_content_type(content_type: &str) -> &'static str {
    if content_type.starts_with("text/html") {
        PAGE_CSP
    } else {
        JAIL_CSP
    }
}
