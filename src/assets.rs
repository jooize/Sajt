//! Generated CSS/JS assets, served from the compile-time consts so those consts
//! stay the single source of truth (the page shell no longer inlines them — a
//! strict `script-src 'self'; style-src 'self'` CSP forbids inline `<script>`
//! and `<style>`, and we take no nonce).
//!
//! Each asset carries a short content hash used as a cache-busting token in its
//! URL (`/static/site.css?v=<hash>`): the bytes are served `immutable`, and any
//! edit to the underlying const changes the hash, so a new URL is referenced and
//! stale copies are never used. The hash is computed once, lazily, at first use.

use sha2::{Digest, Sha256};
use std::sync::LazyLock;

/// One generated asset: its filename, content type, body bytes, and the content
/// hash that versions its URL.
pub struct Asset {
    name: &'static str,
    mime: &'static str,
    body: String,
    hash: String,
}

impl Asset {
    fn new(name: &'static str, mime: &'static str, body: String) -> Self {
        // First 8 bytes of the SHA-256 as hex — plenty to detect any edit; this
        // token only busts caches, it is not a security boundary.
        let digest = Sha256::digest(body.as_bytes());
        let hash = digest[..8].iter().map(|b| format!("{:02x}", b)).collect();
        Asset { name, mime, body, hash }
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn mime(&self) -> &'static str {
        self.mime
    }

    /// The versioned URL the page references. The `?v=` token changes whenever
    /// the const changes, so `immutable` caching is safe.
    pub fn url(&self) -> String {
        format!("/static/{}?v={}", self.name, self.hash)
    }
}

/// Site stylesheet: the shared page CSS, the embed-card CSS, and the generated
/// numeric-tier rules — everything the shell used to inline, now one file.
pub static SITE_CSS: LazyLock<Asset> = LazyLock::new(|| {
    Asset::new(
        "site.css",
        "text/css; charset=utf-8",
        format!(
            "{}{}{}",
            crate::templates::CSS,
            crate::embed::EMBED_CSS,
            numeric_tiers()
        ),
    )
});

/// CSS rules for the values that used to be inline `style` and vary numerically:
/// the tag-cloud font-size tiers and the grade-meter fill widths. Generating
/// them keeps the repetitive rules DRY and out of any inline `style` (CSP
/// `style-src 'self'`). The tag-color/recency/kind rules (small, fixed sets)
/// stay hand-written in the CSS const.
fn numeric_tiers() -> String {
    let mut css = String::from("\n/* generated numeric tiers (see assets::numeric_tiers) */\n");
    // Cloud font-size tiers: quantized `0.82 + share * 0.43`rem.
    let n = crate::stats::CLOUD_SIZE_TIERS;
    for t in 0..n {
        let rem = 0.82 + (t as f32 / (n - 1) as f32) * 0.43;
        css.push_str(&format!("#cloud a[data-size=\"{t}\"] {{ font-size: {rem:.3}rem; }}\n"));
    }
    // Grade-meter fill, in 5% steps (the render side buckets to the same grid).
    let mut fill = 0;
    while fill <= 100 {
        css.push_str(&format!(".meter > i[data-fill=\"{fill}\"] {{ width: {fill}%; }}\n"));
        fill += 5;
    }
    css
}

/// Main progressive-enhancement script, run at end of body.
pub static SITE_JS: LazyLock<Asset> = LazyLock::new(|| {
    Asset::new(
        "site.js",
        "text/javascript; charset=utf-8",
        crate::templates::JS.to_string(),
    )
});

/// Pre-paint bootstrap, loaded render-blocking in `<head>`.
pub static BOOT_JS: LazyLock<Asset> = LazyLock::new(|| {
    Asset::new(
        "boot.js",
        "text/javascript; charset=utf-8",
        crate::templates::BOOT_JS.to_string(),
    )
});

/// Resolve a `/static/<name>` request to a generated asset, if it is one. The
/// server intercepts these before the on-disk static lookup.
pub fn get(name: &str) -> Option<&'static Asset> {
    match name {
        "site.css" => Some(&SITE_CSS),
        "site.js" => Some(&SITE_JS),
        "boot.js" => Some(&BOOT_JS),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_are_versioned_and_distinct() {
        let css = SITE_CSS.url();
        assert!(css.starts_with("/static/site.css?v="), "{css}");
        // Different bodies hash differently, so their cache tokens differ.
        assert_ne!(SITE_CSS.url(), SITE_JS.url());
        assert_ne!(SITE_JS.url(), BOOT_JS.url());
    }

    #[test]
    fn lookup_matches_known_names_only() {
        assert!(get("site.css").is_some());
        assert!(get("site.js").is_some());
        assert!(get("boot.js").is_some());
        assert!(get("nope.css").is_none());
        assert!(get("../secret").is_none());
    }
}
