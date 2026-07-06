//! Link embed support.
//!
//! Fetches and caches link data for display as rich preview cards.
//! - oEmbed platforms (Twitter, Bluesky, Mastodon): full cached cards served locally
//! - Non-oEmbed social (Instagram, Threads): archived for personal use, styled link cards served publicly
//! - Apple platforms (App Store, Music, Podcasts, Books, TV): iTunes Lookup API preview cards
//! - Generic HTTPS URLs: OpenGraph/Twitter meta-tag preview cards as a final fallback
//!
//! Periodic liveness checks verify originals are still public; deleted/removed content stops being served.
//!
//! Trust model: the site operator authors every `.link` file, so URLs are trusted.
//! Outbound fetches still go through a single shared client that pins timeouts and
//! a generic User-Agent so we never leak request-shape detail about the operator.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

// ─── Platform detection ──────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Twitter,
    Bluesky,
    Mastodon,
    Instagram,
    Threads,
    #[serde(rename = "apple_appstore")]
    AppleAppStore,
    #[serde(rename = "apple_music")]
    AppleMusic,
    #[serde(rename = "apple_podcasts")]
    ApplePodcasts,
    #[serde(rename = "apple_books")]
    AppleBooks,
    #[serde(rename = "apple_tv")]
    AppleTV,
    /// Any HTTPS URL not matching a more specific platform. Rendered from OpenGraph/Twitter meta tags.
    #[serde(rename = "generic")]
    Generic,
}

impl Platform {
    /// Whether this platform has a public oEmbed API (= implicit license to embed).
    pub fn has_oembed(&self) -> bool {
        matches!(self, Platform::Twitter | Platform::Bluesky | Platform::Mastodon)
    }

    /// Whether this is an Apple platform (App Store, Music, Podcasts, Books, TV).
    pub fn is_apple(&self) -> bool {
        matches!(
            self,
            Platform::AppleAppStore
                | Platform::AppleMusic
                | Platform::ApplePodcasts
                | Platform::AppleBooks
                | Platform::AppleTV
        )
    }

    /// CSS accent color for this platform.
    pub fn accent_color(&self) -> &'static str {
        match self {
            Platform::Twitter => "#1d9bf0",
            Platform::Bluesky => "#0085ff",
            Platform::Mastodon => "#6364ff",
            Platform::Instagram => "#e1306c",
            Platform::Threads => "var(--color-fg)",
            Platform::AppleAppStore => "#0d84ff", // overridden by render for Mac apps
            Platform::AppleMusic => "#fa2d48",
            Platform::ApplePodcasts => "#9933cc",
            Platform::AppleBooks => "#f5813f",
            Platform::AppleTV => "#000000",
            Platform::Generic => "var(--color-link)",
        }
    }

    /// Human-readable platform name. For Generic links, the actual display is sourced
    /// from the embed's site_name/hostname; this static fallback only matters for
    /// the "deleted" card and similar generic UI.
    pub fn display_name(&self) -> &'static str {
        match self {
            Platform::Twitter => "Twitter",
            Platform::Bluesky => "Bluesky",
            Platform::Mastodon => "Mastodon",
            Platform::Instagram => "Instagram",
            Platform::Threads => "Threads",
            Platform::AppleAppStore => "App Store", // overridden by render for Mac apps
            Platform::AppleMusic => "Apple Music",
            Platform::ApplePodcasts => "Apple Podcasts",
            Platform::AppleBooks => "Apple Books",
            Platform::AppleTV => "Apple TV",
            Platform::Generic => "Link",
        }
    }
}

/// Parsed link URL with platform identification.
#[derive(Debug, Clone)]
pub struct ParsedUrl {
    pub platform: Platform,
    pub url: String,
    pub user: String,
    pub item_id: String,
    /// Mastodon instance hostname (only set for Mastodon URLs).
    pub instance: Option<String>,
    /// Country code extracted from Apple URLs (e.g. "us", "gb").
    pub country: Option<String>,
}

/// Try to parse a URL as a recognized link (social media, Apple, etc.).
pub fn parse_link_url(url: &str) -> Option<ParsedUrl> {
    let url = url.trim();

    // Twitter / X
    // https://twitter.com/{user}/status/{id}
    // https://x.com/{user}/status/{id}
    if let Some(rest) = url
        .strip_prefix("https://twitter.com/")
        .or_else(|| url.strip_prefix("https://x.com/"))
        .or_else(|| url.strip_prefix("https://www.twitter.com/"))
        .or_else(|| url.strip_prefix("https://www.x.com/"))
    {
        let parts: Vec<&str> = rest.splitn(4, '/').collect();
        if parts.len() >= 3 && parts[1] == "status" && !parts[0].is_empty() && !parts[2].is_empty() {
            // Strip query string / fragment from item_id
            let item_id = parts[2].split(['?', '#']).next().unwrap_or(parts[2]);
            return Some(ParsedUrl {
                platform: Platform::Twitter,
                url: url.to_string(),
                user: parts[0].to_string(),
                item_id: item_id.to_string(),
                instance: None,
                country: None,
            });
        }
    }

    // Bluesky
    // https://bsky.app/profile/{handle}/post/{rkey}
    if let Some(rest) = url.strip_prefix("https://bsky.app/profile/") {
        let parts: Vec<&str> = rest.splitn(4, '/').collect();
        if parts.len() >= 3 && parts[1] == "post" && !parts[0].is_empty() && !parts[2].is_empty() {
            let item_id = parts[2].split(['?', '#']).next().unwrap_or(parts[2]);
            return Some(ParsedUrl {
                platform: Platform::Bluesky,
                url: url.to_string(),
                user: parts[0].to_string(),
                item_id: item_id.to_string(),
                instance: None,
                country: None,
            });
        }
    }

    // Instagram
    // https://www.instagram.com/p/{shortcode}/
    // https://www.instagram.com/reel/{shortcode}/
    if let Some(rest) = url
        .strip_prefix("https://www.instagram.com/")
        .or_else(|| url.strip_prefix("https://instagram.com/"))
    {
        let parts: Vec<&str> = rest.splitn(3, '/').collect();
        if parts.len() >= 2 && (parts[0] == "p" || parts[0] == "reel") && !parts[1].is_empty() {
            let shortcode = parts[1]
                .split(['?', '#'])
                .next()
                .unwrap_or(parts[1])
                .trim_end_matches('/');
            return Some(ParsedUrl {
                platform: Platform::Instagram,
                url: url.to_string(),
                user: String::new(), // unknown until we fetch
                item_id: shortcode.to_string(),
                instance: None,
                country: None,
            });
        }
    }

    // Threads
    // https://www.threads.net/@{user}/post/{shortcode}
    if let Some(rest) = url
        .strip_prefix("https://www.threads.net/@")
        .or_else(|| url.strip_prefix("https://threads.net/@"))
    {
        let parts: Vec<&str> = rest.splitn(3, '/').collect();
        if parts.len() >= 3 && parts[1] == "post" && !parts[0].is_empty() && !parts[2].is_empty() {
            let shortcode = parts[2]
                .split(['?', '#'])
                .next()
                .unwrap_or(parts[2])
                .trim_end_matches('/');
            return Some(ParsedUrl {
                platform: Platform::Threads,
                url: url.to_string(),
                user: parts[0].to_string(),
                item_id: shortcode.to_string(),
                instance: None,
                country: None,
            });
        }
    }

    // ── Apple platforms ──

    // App Store: https://apps.apple.com/{country}/app/{name}/id{numeric_id}
    if let Some(rest) = url.strip_prefix("https://apps.apple.com/") {
        if let Some(parsed) = parse_apple_url(rest, Platform::AppleAppStore, url) {
            return Some(parsed);
        }
    }

    // Apple Music: https://music.apple.com/{country}/album|artist|playlist|song/...
    if let Some(rest) = url.strip_prefix("https://music.apple.com/") {
        if let Some(parsed) = parse_apple_url(rest, Platform::AppleMusic, url) {
            return Some(parsed);
        }
    }

    // Apple Podcasts: https://podcasts.apple.com/{country}/podcast/{name}/id{numeric_id}
    if let Some(rest) = url.strip_prefix("https://podcasts.apple.com/") {
        if let Some(parsed) = parse_apple_url(rest, Platform::ApplePodcasts, url) {
            return Some(parsed);
        }
    }

    // Apple Books: https://books.apple.com/{country}/book/{name}/id{numeric_id}
    if let Some(rest) = url.strip_prefix("https://books.apple.com/") {
        if let Some(parsed) = parse_apple_url(rest, Platform::AppleBooks, url) {
            return Some(parsed);
        }
    }

    // Apple TV: https://tv.apple.com/{country}/show|movie|episode/{name}/umc.cmc.{id}
    if let Some(rest) = url.strip_prefix("https://tv.apple.com/") {
        if let Some(parsed) = parse_apple_url(rest, Platform::AppleTV, url) {
            return Some(parsed);
        }
    }

    // iTunes (movies): https://itunes.apple.com/{country}/movie/{name}/id{numeric_id}
    if let Some(rest) = url.strip_prefix("https://itunes.apple.com/") {
        if let Some(parsed) = parse_apple_url(rest, Platform::AppleTV, url) {
            return Some(parsed);
        }
    }

    // Mastodon / Fediverse
    // https://{instance}/@{user}/{item_id}
    // Must be validated via oEmbed discovery later
    if url.starts_with("https://") {
        let without_scheme = &url["https://".len()..];
        let parts: Vec<&str> = without_scheme.splitn(4, '/').collect();
        if parts.len() >= 3
            && parts[1].starts_with('@')
            && parts[1].len() > 1
            && !parts[2].is_empty()
        {
            let item_id = parts[2].split(['?', '#']).next().unwrap_or(parts[2]);
            // Basic check: item_id should be numeric for Mastodon
            if item_id.chars().all(|c| c.is_ascii_digit()) {
                let instance = parts[0].to_string();
                let user = parts[1][1..].to_string(); // strip @
                return Some(ParsedUrl {
                    platform: Platform::Mastodon,
                    url: url.to_string(),
                    user,
                    item_id: item_id.to_string(),
                    instance: Some(instance),
                    country: None,
                });
            }
        }
    }

    // Generic HTTPS fallback. Any well-formed https:// URL with a hostname containing
    // a dot becomes a Generic embed (OG/Twitter meta tags). HTTP is intentionally
    // rejected — we don't want to render or download from plaintext sources.
    if let Some(host) = extract_hostname(url) {
        return Some(ParsedUrl {
            platform: Platform::Generic,
            url: url.to_string(),
            user: String::new(),
            item_id: host,
            instance: None,
            country: None,
        });
    }

    None
}

/// Extract a display hostname from an https:// URL.
///
/// Returns `None` if the URL is not https, has no hostname, has a hostname without
/// a dot (which excludes bare hostnames and IP-style locals like `localhost`), or
/// looks like a private/loopback IP literal. Hostname is lowercased and the leading
/// `www.` is stripped for display.
fn extract_hostname(url: &str) -> Option<String> {
    let after_scheme = url.strip_prefix("https://")?;
    // Authority ends at first '/', '?', '#'
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_scheme);
    if authority.is_empty() {
        return None;
    }
    // Strip optional userinfo and port (host[:port]); reject userinfo since
    // none of our trusted URLs need it and it makes display unambiguous.
    if authority.contains('@') {
        return None;
    }
    let host_no_port = authority.split(':').next().unwrap_or(authority);
    let host = host_no_port.trim().to_lowercase();
    if host.is_empty()
        || host.contains(' ')
        || !host.contains('.')
        || host.starts_with('.')
        || host.ends_with('.')
    {
        return None;
    }
    // Reject obvious loopback / private literals so a typo can't make the server
    // fetch from itself. Full SSRF protection would require IP resolution.
    if is_local_host_literal(&host) {
        return None;
    }
    Some(host.strip_prefix("www.").unwrap_or(&host).to_string())
}

/// Cheap textual check for hostnames that resolve to loopback or private networks.
/// Not a substitute for IP-based SSRF guarding; just keeps the obvious foot-guns away.
fn is_local_host_literal(host: &str) -> bool {
    if host == "localhost" || host.ends_with(".localhost") || host == "localhost.localdomain" {
        return true;
    }
    if let Ok(addr) = host.parse::<std::net::IpAddr>() {
        return match addr {
            std::net::IpAddr::V4(v4) => {
                v4.is_loopback()
                    || v4.is_private()
                    || v4.is_link_local()
                    || v4.is_unspecified()
                    || v4.is_broadcast()
                    || v4.octets()[0] == 0
            }
            std::net::IpAddr::V6(v6) => {
                v6.is_loopback() || v6.is_unspecified() || v6.segments()[0] & 0xfe00 == 0xfc00
            }
        };
    }
    false
}

/// Parse an Apple URL path after the domain prefix.
///
/// Handles these patterns:
/// - `{country}/app/{name}/id{numeric_id}`
/// - `{country}/album/{name}/{numeric_id}`
/// - `{country}/artist/{name}/{numeric_id}`
/// - `{country}/playlist/{name}/pl.{id}`
/// - `{country}/song/{name}/{numeric_id}` (or just `{country}/song/{numeric_id}`)
/// - `{country}/podcast/{name}/id{numeric_id}`
/// - `{country}/book/{name}/id{numeric_id}`
/// - `{country}/show/{name}/umc.cmc.{id}`
/// - `{country}/movie/{name}/umc.cmc.{id}` or `id{numeric_id}`
/// - `{country}/episode/{name}/umc.cmc.{id}`
fn parse_apple_url(path: &str, platform: Platform, original_url: &str) -> Option<ParsedUrl> {
    let parts: Vec<&str> = path.split('/').collect();
    // Minimum: {country}/{type}/{name_or_id}/{id} = 4 parts
    // Some have only 3: {country}/song/{id}
    if parts.len() < 3 {
        return None;
    }

    let country = parts[0];
    // Country code should be 2 letters
    if country.len() != 2 || !country.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }

    // The ID is in the last path segment (strip query/fragment first)
    let last = parts.last()?;
    let last_clean = last.split(['?', '#']).next().unwrap_or(last).trim_end_matches('/');

    // Extract the item ID from the last segment
    let item_id = if let Some(id) = last_clean.strip_prefix("id") {
        // id{numeric} pattern (App Store, Podcasts, Books, iTunes movies)
        if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) {
            id.to_string()
        } else {
            return None;
        }
    } else if last_clean.starts_with("umc.cmc.") {
        // Apple TV+ UMC pattern
        last_clean.to_string()
    } else if last_clean.starts_with("pl.") {
        // Apple Music playlist pattern
        last_clean.to_string()
    } else if last_clean.chars().all(|c| c.is_ascii_digit()) && !last_clean.is_empty() {
        // Bare numeric ID (Apple Music albums, artists, songs)
        last_clean.to_string()
    } else {
        return None;
    };

    Some(ParsedUrl {
        platform,
        url: original_url.to_string(),
        user: String::new(),
        item_id,
        instance: None,
        country: Some(country.to_lowercase()),
    })
}

// ─── Embed data structures ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "platform")]
pub enum EmbedData {
    #[serde(rename = "twitter")]
    Twitter(TwitterEmbed),
    #[serde(rename = "bluesky")]
    Bluesky(BlueskyEmbed),
    #[serde(rename = "mastodon")]
    Mastodon(MastodonEmbed),
    #[serde(rename = "instagram")]
    Instagram(InstagramEmbed),
    #[serde(rename = "threads")]
    Threads(ThreadsEmbed),
    #[serde(rename = "apple_appstore")]
    AppleAppStore(AppleEmbed),
    #[serde(rename = "apple_music")]
    AppleMusic(AppleEmbed),
    #[serde(rename = "apple_podcasts")]
    ApplePodcasts(AppleEmbed),
    #[serde(rename = "apple_books")]
    AppleBooks(AppleEmbed),
    #[serde(rename = "apple_tv")]
    AppleTV(AppleEmbed),
    #[serde(rename = "generic")]
    Generic(GenericEmbed),
}

impl EmbedData {
    pub fn platform(&self) -> Platform {
        match self {
            EmbedData::Twitter(_) => Platform::Twitter,
            EmbedData::Bluesky(_) => Platform::Bluesky,
            EmbedData::Mastodon(_) => Platform::Mastodon,
            EmbedData::Instagram(_) => Platform::Instagram,
            EmbedData::Threads(_) => Platform::Threads,
            EmbedData::AppleAppStore(_) => Platform::AppleAppStore,
            EmbedData::AppleMusic(_) => Platform::AppleMusic,
            EmbedData::ApplePodcasts(_) => Platform::ApplePodcasts,
            EmbedData::AppleBooks(_) => Platform::AppleBooks,
            EmbedData::AppleTV(_) => Platform::AppleTV,
            EmbedData::Generic(_) => Platform::Generic,
        }
    }

    pub fn url(&self) -> &str {
        match self {
            EmbedData::Twitter(e) => &e.url,
            EmbedData::Bluesky(e) => &e.url,
            EmbedData::Mastodon(e) => &e.url,
            EmbedData::Instagram(e) => &e.url,
            EmbedData::Threads(e) => &e.url,
            EmbedData::AppleAppStore(e)
            | EmbedData::AppleMusic(e)
            | EmbedData::ApplePodcasts(e)
            | EmbedData::AppleBooks(e)
            | EmbedData::AppleTV(e) => &e.url,
            EmbedData::Generic(e) => &e.url,
        }
    }

    /// Generate a display label for timeline display.
    pub fn display_label(&self) -> String {
        match self {
            EmbedData::Twitter(e) => {
                let date = e.post_date.as_deref().unwrap_or("");
                if date.is_empty() {
                    format!("@{}", e.author_handle)
                } else {
                    format!("@{} \u{00b7} {}", e.author_handle, date)
                }
            }
            EmbedData::Bluesky(e) => {
                let date = e.post_date.as_deref().unwrap_or("");
                if date.is_empty() {
                    format!("@{}", e.author_handle)
                } else {
                    format!("@{} \u{00b7} {}", e.author_handle, date)
                }
            }
            EmbedData::Mastodon(e) => {
                let date = e.post_date.as_deref().unwrap_or("");
                if date.is_empty() {
                    format!("@{}@{}", e.author_handle, e.instance)
                } else {
                    format!("@{}@{} \u{00b7} {}", e.author_handle, e.instance, date)
                }
            }
            EmbedData::Instagram(e) => {
                if e.username.is_empty() {
                    "Instagram post".to_string()
                } else {
                    format!("@{} on Instagram", e.username)
                }
            }
            EmbedData::Threads(e) => {
                if e.username.is_empty() {
                    "Threads post".to_string()
                } else {
                    format!("@{} on Threads", e.username)
                }
            }
            EmbedData::AppleAppStore(e)
            | EmbedData::AppleMusic(e)
            | EmbedData::ApplePodcasts(e)
            | EmbedData::AppleBooks(e)
            | EmbedData::AppleTV(e) => {
                if e.artist_name.is_empty() {
                    let platform_name = match (self.platform(), e.content_type.as_deref()) {
                        (Platform::AppleAppStore, Some("mac-software")) => "Mac App Store",
                        _ => self.platform().display_name(),
                    };
                    format!("{} \u{00b7} {}", e.name, platform_name)
                } else {
                    format!("{} \u{00b7} {}", e.name, e.artist_name)
                }
            }
            EmbedData::Generic(e) => {
                let source = e.site_name.as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or(&e.hostname);
                match e.title.as_deref().filter(|t| !t.is_empty()) {
                    Some(t) => format!("{} \u{00b7} {}", t, source),
                    None => source.to_string(),
                }
            }
        }
    }

    /// Whether this embed has oEmbed-licensed content that can be served publicly.
    pub fn can_serve_content(&self) -> bool {
        self.platform().has_oembed()
    }

    /// Whether the original content has been confirmed deleted/unavailable.
    pub fn is_upstream_deleted(&self) -> bool {
        match self {
            EmbedData::Twitter(e) => e.upstream_deleted,
            EmbedData::Bluesky(e) => e.upstream_deleted,
            EmbedData::Mastodon(e) => e.upstream_deleted,
            EmbedData::Instagram(e) => e.upstream_deleted,
            EmbedData::Threads(e) => e.upstream_deleted,
            EmbedData::AppleAppStore(e)
            | EmbedData::AppleMusic(e)
            | EmbedData::ApplePodcasts(e)
            | EmbedData::AppleBooks(e)
            | EmbedData::AppleTV(e) => e.upstream_deleted,
            EmbedData::Generic(e) => e.upstream_deleted,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterEmbed {
    pub url: String,
    pub author_name: String,
    pub author_handle: String,
    pub content_html: String,
    pub post_date: Option<String>,
    pub media_files: Vec<String>,
    #[serde(default)]
    pub upstream_deleted: bool,
    #[serde(default)]
    pub last_checked: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueskyEmbed {
    pub url: String,
    pub author_name: String,
    pub author_handle: String,
    pub content_html: String,
    pub post_date: Option<String>,
    pub media_files: Vec<String>,
    #[serde(default)]
    pub upstream_deleted: bool,
    #[serde(default)]
    pub last_checked: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MastodonEmbed {
    pub url: String,
    pub instance: String,
    pub author_name: String,
    pub author_handle: String,
    pub content_html: String,
    pub post_date: Option<String>,
    pub media_files: Vec<String>,
    #[serde(default)]
    pub upstream_deleted: bool,
    #[serde(default)]
    pub last_checked: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramEmbed {
    pub url: String,
    pub username: String,
    pub shortcode: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub image_file: Option<String>,
    #[serde(default)]
    pub upstream_deleted: bool,
    #[serde(default)]
    pub last_checked: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadsEmbed {
    pub url: String,
    pub username: String,
    pub shortcode: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub image_file: Option<String>,
    #[serde(default)]
    pub upstream_deleted: bool,
    #[serde(default)]
    pub last_checked: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericEmbed {
    pub url: String,
    /// Hostname extracted from the URL (e.g. "j-fidel-505.neocities.org"). Always present.
    pub hostname: String,
    /// `og:site_name`, when the page provides it (preferred over `hostname` for display).
    pub site_name: Option<String>,
    /// `og:title` or `<title>`.
    pub title: Option<String>,
    /// `og:description` or `<meta name="description">`, with HTML stripped and length capped.
    pub description: Option<String>,
    /// Source `og:image` URL (kept for re-fetch on cache invalidation).
    pub image_url: Option<String>,
    /// Local cached filename in the sidecar directory.
    pub image_file: Option<String>,
    #[serde(default)]
    pub upstream_deleted: bool,
    #[serde(default)]
    pub last_checked: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppleEmbed {
    pub url: String,
    pub item_id: String,
    pub name: String,
    pub artist_name: String,
    pub artwork_url: Option<String>,
    pub artwork_file: Option<String>,
    pub description: Option<String>,
    pub price: Option<f64>,
    pub formatted_price: Option<String>,
    pub currency: Option<String>,
    pub rating: Option<f64>,
    pub rating_count: Option<u64>,
    pub genre: Option<String>,
    pub release_date: Option<String>,
    pub content_type: Option<String>,
    #[serde(default)]
    pub upstream_deleted: bool,
    #[serde(default)]
    pub last_checked: Option<String>,
}

// ─── Cache I/O (outside the content tree) ───────────────────────
//
// The server is strictly read-only on the content directory (see
// entry-model.md), so every derived embed cache lives under a separate cache
// root passed in from main -- by default the platform cache dir (e.g. macOS
// ~/Library/Caches/...). Each entry's cache is a directory named by a hash of
// its content-relative path; `meta.json5` records the source path and mtime so
// an edit (new mtime) forces a refetch, and any downloaded media (OG images,
// artwork) sits beside it.

/// Per-entry cache directory: `<cache_dir>/embeds/<key>`, where `key` is a hash
/// of the entry's path relative to the content root. Deterministic, so the
/// writer (resolve), the card renderer, and the `/_embed` route all agree.
pub fn cache_dir_for(cache_dir: &Path, content_dir: &Path, entry_path: &Path) -> PathBuf {
    embed_root(cache_dir).join(cache_key(content_dir, entry_path))
}

/// The embed-cache root under the general cache dir.
fn embed_root(cache_dir: &Path) -> PathBuf {
    cache_dir.join("embeds")
}

/// Stable cache key for an entry: hex SHA-256 of its content-relative path.
/// Hashing keeps the key filesystem- and URL-safe whatever the path's
/// characters, and collision-free across the whole tree.
pub fn cache_key(content_dir: &Path, entry_path: &Path) -> String {
    let rel = entry_path.strip_prefix(content_dir).unwrap_or(entry_path);
    let mut hasher = Sha256::new();
    hasher.update(rel.to_string_lossy().as_bytes());
    hex_of(hasher.finalize().as_slice())
}

/// Lower/upper-hex encode a byte slice (reuses the percent-encoding table).
fn hex_of(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

/// Source-file mtime in whole seconds since the Unix epoch, or `None` if the
/// file is gone or its time is unreadable. Pre-epoch times go negative.
pub fn file_mtime_secs(path: &Path) -> Option<i64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(match modified.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    })
}

/// On-disk cache envelope: the embed data plus the provenance that invalidates
/// it. `source` is advisory (for debugging); `mtime` is authoritative -- a
/// mismatch against the live file means the entry was edited, so refetch.
#[derive(Debug, Serialize, Deserialize)]
struct CacheEnvelope {
    source: String,
    mtime: i64,
    data: EmbedData,
}

fn meta_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join("meta.json5")
}

/// Read cached embed data, but only if it was written for the current file
/// mtime. Any staleness (missing, unparsable, or mtime drift) reads as a miss.
pub fn read_cached(cache_dir: &Path, expected_mtime: Option<i64>) -> Option<EmbedData> {
    let meta = meta_path(cache_dir);
    let content = std::fs::read_to_string(&meta).ok()?;
    let envelope: CacheEnvelope = match json5::from_str(&content) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("Failed to parse embed cache {}: {}", meta.display(), e);
            return None;
        }
    };
    if expected_mtime.is_some() && Some(envelope.mtime) != expected_mtime {
        tracing::info!(
            "Embed cache stale (mtime {} != {:?}): {}",
            envelope.mtime,
            expected_mtime,
            meta.display()
        );
        return None;
    }
    Some(envelope.data)
}

/// Write embed data to the entry's cache directory, stamping the source path
/// and mtime so a later edit invalidates it.
fn write_cache(
    cache_dir: &Path,
    source: &str,
    mtime: i64,
    data: &EmbedData,
) -> std::io::Result<()> {
    std::fs::create_dir_all(cache_dir)?;
    let envelope = CacheEnvelope {
        source: source.to_string(),
        mtime,
        data: data.clone(),
    };
    let meta = meta_path(cache_dir);
    let json = serde_json::to_string_pretty(&envelope)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(&meta, json)?;
    tracing::info!("Wrote embed cache: {}", meta.display());
    Ok(())
}

// ─── oEmbed response parsing ────────────────────────────────────

#[derive(Debug, Deserialize)]
struct OEmbedResponse {
    #[serde(default)]
    author_name: Option<String>,
    #[serde(default)]
    author_url: Option<String>,
    #[serde(default)]
    html: Option<String>,
}

/// Build the oEmbed endpoint URL for a given social URL.
fn oembed_endpoint(social: &ParsedUrl) -> Option<String> {
    let encoded = urlencod(&social.url);
    match social.platform {
        Platform::Twitter => Some(format!(
            "https://publish.twitter.com/oembed?url={}&omit_script=true",
            encoded
        )),
        Platform::Bluesky => Some(format!(
            "https://embed.bsky.app/oembed?url={}&format=json",
            encoded
        )),
        Platform::Mastodon => {
            let instance = social.instance.as_deref()?;
            Some(format!(
                "https://{}/api/oembed?url={}",
                instance, encoded
            ))
        }
        Platform::Instagram | Platform::Threads => None,
        Platform::AppleAppStore
        | Platform::AppleMusic
        | Platform::ApplePodcasts
        | Platform::AppleBooks
        | Platform::AppleTV
        // Generic links are resolved via OG tags (fetch_generic_og), never oEmbed.
        | Platform::Generic => None,
    }
}

/// Minimal URL encoding for query parameter values.
fn urlencod(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push(char::from(HEX[(b >> 4) as usize]));
                out.push(char::from(HEX[(b & 0xf) as usize]));
            }
        }
    }
    out
}

const HEX: [u8; 16] = *b"0123456789ABCDEF";

// ─── Fetching ───────────────────────────────────────────────────

/// Build a shared HTTP client with reasonable defaults.
fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("Mozilla/5.0 (compatible; esko-bar/0.1)")
        .build()
        .expect("Failed to build HTTP client")
}

/// Fetch embed data for a social URL via oEmbed.
async fn fetch_oembed(client: &reqwest::Client, social: &ParsedUrl) -> Result<EmbedData, String> {
    let endpoint = oembed_endpoint(social).ok_or("No oEmbed endpoint for this platform")?;

    tracing::info!("Fetching oEmbed: {}", endpoint);

    let resp = client
        .get(&endpoint)
        .send()
        .await
        .map_err(|e| format!("oEmbed request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("oEmbed returned status {}", resp.status()));
    }

    let oembed: OEmbedResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse oEmbed JSON: {}", e))?;

    let content_html = sanitize_oembed_html(oembed.html.as_deref().unwrap_or(""));
    let author_name = oembed.author_name.unwrap_or_default();

    // Try to extract handle from author_url or author_name
    let author_handle = extract_handle(&author_name, oembed.author_url.as_deref(), social);

    let now = chrono::Utc::now().to_rfc3339();

    match social.platform {
        Platform::Twitter => Ok(EmbedData::Twitter(TwitterEmbed {
            url: social.url.clone(),
            author_name,
            author_handle,
            content_html,
            post_date: None,
            media_files: Vec::new(),
            upstream_deleted: false,
            last_checked: Some(now),
        })),
        Platform::Bluesky => Ok(EmbedData::Bluesky(BlueskyEmbed {
            url: social.url.clone(),
            author_name,
            author_handle: social.user.clone(),
            content_html,
            post_date: None,
            media_files: Vec::new(),
            upstream_deleted: false,
            last_checked: Some(now),
        })),
        Platform::Mastodon => Ok(EmbedData::Mastodon(MastodonEmbed {
            url: social.url.clone(),
            instance: social.instance.clone().unwrap_or_default(),
            author_name,
            author_handle: social.user.clone(),
            content_html,
            post_date: None,
            media_files: Vec::new(),
            upstream_deleted: false,
            last_checked: Some(now),
        })),
        _ => Err("Platform does not support oEmbed".to_string()),
    }
}

/// Fetch OG tags for Instagram/Threads (archived for personal use, not served publicly).
async fn fetch_og_tags(client: &reqwest::Client, social: &ParsedUrl) -> Result<EmbedData, String> {
    tracing::info!("Fetching OG tags: {}", social.url);

    let resp = client
        .get(&social.url)
        .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .send()
        .await
        .map_err(|e| format!("OG tag fetch failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("OG tag fetch returned status {}", resp.status()));
    }

    let html = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    let (title, description, _image_url) = parse_og_tags(&html);

    let now = chrono::Utc::now().to_rfc3339();

    match social.platform {
        Platform::Instagram => {
            let username = extract_instagram_username(&social.url).unwrap_or_default();
            Ok(EmbedData::Instagram(InstagramEmbed {
                url: social.url.clone(),
                username,
                shortcode: social.item_id.clone(),
                title,
                description,
                image_file: None, // Media downloaded separately
                upstream_deleted: false,
                last_checked: Some(now),
            }))
        }
        Platform::Threads => Ok(EmbedData::Threads(ThreadsEmbed {
            url: social.url.clone(),
            username: social.user.clone(),
            shortcode: social.item_id.clone(),
            title,
            description,
            image_file: None,
            upstream_deleted: false,
            last_checked: Some(now),
        })),
        _ => Err("Platform does not use OG tags".to_string()),
    }
}

/// Fetch generic page metadata (OG / Twitter card / `<title>`) for any HTTPS URL.
async fn fetch_generic_og(
    client: &reqwest::Client,
    parsed: &ParsedUrl,
    cache_dir: &Path,
) -> Result<EmbedData, String> {
    tracing::info!("Fetching generic OG tags: {}", parsed.url);

    let resp = client
        .get(&parsed.url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (compatible; esko-bar/0.1; +https://esko.bar)",
        )
        // Most sites only emit OG tags to clients that accept HTML.
        .header("Accept", "text/html,application/xhtml+xml")
        .send()
        .await
        .map_err(|e| format!("Generic OG fetch failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Generic OG fetch returned status {}", resp.status()));
    }

    // Hard cap response body to avoid pulling in giant pages. 4 MiB is plenty for
    // a `<head>`-heavy page and still cheap to discard if it's a media file.
    let body = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?;
    const MAX_BYTES: usize = 4 * 1024 * 1024;
    let html_bytes = if body.len() > MAX_BYTES {
        &body[..MAX_BYTES]
    } else {
        &body[..]
    };
    let html = String::from_utf8_lossy(html_bytes);

    let (title, description, image_url, site_name) = parse_meta_tags(&html);

    let now = chrono::Utc::now().to_rfc3339();

    // Sanitize description: strip HTML tags, collapse whitespace, cap length.
    let description = description.map(|d| {
        let plain = strip_html_tags(&d);
        let collapsed: String = plain.split_whitespace().collect::<Vec<_>>().join(" ");
        truncate_for_display(&collapsed, 240)
    }).filter(|s| !s.is_empty());

    let title = title.map(|t| {
        let plain = strip_html_tags(&t);
        let collapsed: String = plain.split_whitespace().collect::<Vec<_>>().join(" ");
        truncate_for_display(&collapsed, 160)
    }).filter(|s| !s.is_empty());

    // Resolve image_url against the page URL so /path/to.png works as well as absolute.
    let resolved_image = image_url
        .as_deref()
        .and_then(|raw| resolve_url_relative_to(&parsed.url, raw));

    let mut embed = GenericEmbed {
        url: parsed.url.clone(),
        hostname: parsed.item_id.clone(), // we stashed hostname here in parse_link_url
        site_name,
        title,
        description,
        image_url: resolved_image.clone(),
        image_file: None,
        upstream_deleted: false,
        last_checked: Some(now),
    };

    if let Some(ref img_url) = resolved_image {
        match download_media(client, img_url, cache_dir).await {
            Ok(filename) => embed.image_file = Some(filename),
            Err(e) => tracing::warn!("Failed to download generic OG image {}: {}", img_url, e),
        }
    }

    Ok(EmbedData::Generic(embed))
}

/// Resolve a possibly-relative URL against a base. Returns None if the result
/// isn't a usable https URL we'd want to fetch.
fn resolve_url_relative_to(base: &str, candidate: &str) -> Option<String> {
    let candidate = candidate.trim();
    if candidate.is_empty() {
        return None;
    }
    if candidate.starts_with("https://") {
        return Some(candidate.to_string());
    }
    // We intentionally drop http:// candidates — only serve assets fetched over TLS.
    if candidate.starts_with("http://") {
        return None;
    }
    let base_no_scheme = base.strip_prefix("https://")?;
    let (authority, _path) = match base_no_scheme.find('/') {
        Some(idx) => base_no_scheme.split_at(idx),
        None => (base_no_scheme, ""),
    };
    if candidate.starts_with("//") {
        return Some(format!("https:{}", candidate));
    }
    if let Some(rest) = candidate.strip_prefix('/') {
        return Some(format!("https://{}/{}", authority, rest));
    }
    // Relative path. Resolve against base's directory.
    let base_dir = match base.rfind('/') {
        Some(idx) if idx > "https://".len() => &base[..=idx],
        _ => return Some(format!("https://{}/{}", authority, candidate)),
    };
    Some(format!("{}{}", base_dir, candidate))
}

/// Truncate a string to roughly `max_chars`, snapping back to the last space and
/// appending an ellipsis. Operates on chars, not bytes, so it's safe for non-ASCII.
fn truncate_for_display(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars).collect();
    if let Some(last_space) = out.rfind(char::is_whitespace) {
        out.truncate(last_space);
    }
    out.push_str("\u{2026}");
    out
}

/// Download media file to sidecar cache directory, preserving original filename.
async fn download_media(
    client: &reqwest::Client,
    media_url: &str,
    cache_dir: &Path,
) -> Result<String, String> {
    let filename = media_url
        .rsplit('/')
        .next()
        .unwrap_or("media")
        .split(['?', '#'])
        .next()
        .unwrap_or("media");

    // Sanitize filename — only allow safe characters
    let safe_name: String = filename
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '-' || *c == '_')
        .collect();
    let safe_name = if safe_name.is_empty() {
        "media".to_string()
    } else {
        safe_name
    };

    let dest = cache_dir.join(&safe_name);
    if dest.exists() {
        return Ok(safe_name);
    }

    tracing::info!("Downloading media: {} -> {}", media_url, dest.display());

    let resp = client
        .get(media_url)
        .send()
        .await
        .map_err(|e| format!("Media download failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Media download returned status {}", resp.status()));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to read media bytes: {}", e))?;

    std::fs::create_dir_all(cache_dir)
        .map_err(|e| format!("Failed to create cache dir: {}", e))?;
    std::fs::write(&dest, &bytes)
        .map_err(|e| format!("Failed to write media file: {}", e))?;

    Ok(safe_name)
}

// ─── iTunes Lookup API ──────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ITunesLookupResponse {
    #[serde(default, rename = "resultCount")]
    result_count: u32,
    #[serde(default)]
    results: Vec<ITunesResult>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ITunesResult {
    #[serde(default, rename = "trackName")]
    track_name: Option<String>,
    #[serde(default, rename = "collectionName")]
    collection_name: Option<String>,
    #[serde(default, rename = "artistName")]
    artist_name: Option<String>,
    #[serde(default, rename = "sellerName")]
    seller_name: Option<String>,
    #[serde(default, rename = "artworkUrl512")]
    artwork_url_512: Option<String>,
    #[serde(default, rename = "artworkUrl100")]
    artwork_url_100: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default, rename = "trackPrice")]
    track_price: Option<f64>,
    #[serde(default, rename = "collectionPrice")]
    collection_price: Option<f64>,
    #[serde(default, rename = "formattedPrice")]
    formatted_price: Option<String>,
    #[serde(default)]
    currency: Option<String>,
    #[serde(default, rename = "averageUserRating")]
    average_user_rating: Option<f64>,
    #[serde(default, rename = "userRatingCount")]
    user_rating_count: Option<u64>,
    #[serde(default, rename = "primaryGenreName")]
    primary_genre_name: Option<String>,
    #[serde(default, rename = "releaseDate")]
    release_date: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default, rename = "wrapperType")]
    wrapper_type: Option<String>,
}

/// Fetch Apple content metadata via the iTunes Lookup API.
///
/// For numeric IDs, uses `https://itunes.apple.com/lookup?id={id}&country={country}`.
/// For non-numeric IDs (UMC, playlists), falls back to OG tag scraping.
async fn fetch_apple_content(
    client: &reqwest::Client,
    parsed: &ParsedUrl,
    cache_dir: &Path,
) -> Result<EmbedData, String> {
    let country = parsed.country.as_deref().unwrap_or("us");
    let is_numeric = parsed.item_id.chars().all(|c| c.is_ascii_digit());

    let apple_embed = if is_numeric {
        fetch_itunes_lookup(client, &parsed.item_id, country, parsed).await?
    } else {
        // UMC IDs (Apple TV+) and playlist IDs: fall back to OG tags
        fetch_apple_og_tags(client, parsed).await?
    };

    // Download artwork into the entry's cache directory (outside the content
    // tree). The caller passes the resolved cache dir; artwork is keyed by the
    // entry, never by the remote URL, so it can be served back via /_embed.
    let mut apple_embed = apple_embed;
    if let Some(ref artwork_url) = apple_embed.artwork_url {
        match download_media(client, artwork_url, cache_dir).await {
            Ok(filename) => apple_embed.artwork_file = Some(filename),
            Err(e) => tracing::warn!("Failed to download Apple artwork: {}", e),
        }
    }

    match parsed.platform {
        Platform::AppleAppStore => Ok(EmbedData::AppleAppStore(apple_embed)),
        Platform::AppleMusic => Ok(EmbedData::AppleMusic(apple_embed)),
        Platform::ApplePodcasts => Ok(EmbedData::ApplePodcasts(apple_embed)),
        Platform::AppleBooks => Ok(EmbedData::AppleBooks(apple_embed)),
        Platform::AppleTV => Ok(EmbedData::AppleTV(apple_embed)),
        _ => Err("Not an Apple platform".to_string()),
    }
}

/// Fetch metadata from the iTunes Lookup API for numeric Apple IDs.
async fn fetch_itunes_lookup(
    client: &reqwest::Client,
    item_id: &str,
    country: &str,
    parsed: &ParsedUrl,
) -> Result<AppleEmbed, String> {
    let lookup_url = format!(
        "https://itunes.apple.com/lookup?id={}&country={}",
        urlencod(item_id),
        urlencod(country)
    );

    tracing::info!("Fetching iTunes Lookup: {}", lookup_url);

    let resp = client
        .get(&lookup_url)
        .send()
        .await
        .map_err(|e| format!("iTunes Lookup request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("iTunes Lookup returned status {}", resp.status()));
    }

    let lookup: ITunesLookupResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse iTunes Lookup JSON: {}", e))?;

    if lookup.result_count == 0 || lookup.results.is_empty() {
        return Err("iTunes Lookup returned no results".to_string());
    }

    let result = &lookup.results[0];
    let now = chrono::Utc::now().to_rfc3339();

    // Prefer the largest artwork; can swap size suffix for higher res
    let artwork_url = result
        .artwork_url_512
        .as_ref()
        .or(result.artwork_url_100.as_ref())
        .cloned();

    let name = result
        .track_name
        .as_ref()
        .or(result.collection_name.as_ref())
        .cloned()
        .unwrap_or_default();

    let artist_name = result
        .artist_name
        .as_ref()
        .or(result.seller_name.as_ref())
        .cloned()
        .unwrap_or_default();

    let price = result.track_price.or(result.collection_price);
    let formatted_price = result.formatted_price.clone().or_else(|| {
        price.map(|p| {
            let currency = result.currency.as_deref().unwrap_or("USD");
            if p <= 0.0 {
                "Free".to_string()
            } else {
                format!("{}{:.2}", currency_symbol(currency), p)
            }
        })
    });

    // Truncate description for display
    let description = result.description.as_ref().map(|d| {
        let plain = strip_html_tags(d);
        if plain.len() > 200 {
            let mut truncated = plain[..200].to_string();
            // Don't cut mid-word
            if let Some(last_space) = truncated.rfind(' ') {
                truncated.truncate(last_space);
            }
            truncated.push_str("...");
            truncated
        } else {
            plain
        }
    });

    // Parse release date to just the date portion
    let release_date = result.release_date.as_ref().map(|d| {
        // iTunes returns ISO 8601 like "2024-01-15T08:00:00Z"
        d.split('T').next().unwrap_or(d).to_string()
    });

    Ok(AppleEmbed {
        url: parsed.url.clone(),
        item_id: parsed.item_id.clone(),
        name,
        artist_name,
        artwork_url,
        artwork_file: None,
        description,
        price,
        formatted_price,
        currency: result.currency.clone(),
        rating: result.average_user_rating,
        rating_count: result.user_rating_count,
        genre: result.primary_genre_name.clone(),
        release_date,
        content_type: result.kind.clone().or(result.wrapper_type.clone()),
        upstream_deleted: false,
        last_checked: Some(now),
    })
}

/// Fall back to OG tags for Apple content not in the iTunes Lookup API
/// (Apple TV+ UMC IDs, playlists).
async fn fetch_apple_og_tags(
    client: &reqwest::Client,
    parsed: &ParsedUrl,
) -> Result<AppleEmbed, String> {
    tracing::info!("Fetching Apple OG tags: {}", parsed.url);

    let resp = client
        .get(&parsed.url)
        .send()
        .await
        .map_err(|e| format!("Apple OG tag fetch failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Apple OG tag fetch returned status {}", resp.status()));
    }

    let html = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read Apple response body: {}", e))?;

    let (title, description, image_url) = parse_og_tags(&html);
    let now = chrono::Utc::now().to_rfc3339();

    Ok(AppleEmbed {
        url: parsed.url.clone(),
        item_id: parsed.item_id.clone(),
        name: title.unwrap_or_default(),
        artist_name: String::new(),
        artwork_url: image_url,
        artwork_file: None,
        description,
        price: None,
        formatted_price: None,
        currency: None,
        rating: None,
        rating_count: None,
        genre: None,
        release_date: None,
        content_type: None,
        upstream_deleted: false,
        last_checked: Some(now),
    })
}

/// Map currency code to symbol for display.
fn currency_symbol(code: &str) -> &str {
    match code {
        "USD" => "$",
        "EUR" => "\u{20ac}",
        "GBP" => "\u{00a3}",
        "JPY" | "CNY" => "\u{00a5}",
        "SEK" | "NOK" | "DKK" => "kr ",
        "CAD" | "AUD" | "NZD" | "SGD" | "HKD" => "$",
        "CHF" => "CHF ",
        "KRW" => "\u{20a9}",
        "INR" => "\u{20b9}",
        "BRL" => "R$",
        "MXN" => "MX$",
        "RUB" => "\u{20bd}",
        "TRY" => "\u{20ba}",
        "PLN" => "z\u{0142} ",
        "THB" => "\u{0e3f}",
        _ => "",
    }
}

/// Strip HTML tags from a string (for plaintext descriptions).
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            result.push(c);
        }
    }
    result
}

// ─── HTML helpers ───────────────────────────────────────────────

/// Strip `<script>` tags and event handlers from oEmbed HTML.
fn sanitize_oembed_html(html: &str) -> String {
    // Remove script tags and their content
    let mut result = String::with_capacity(html.len());
    let mut chars = html.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '<' {
            // Peek ahead for <script or </script
            let mut tag = String::new();
            while let Some(&nc) = chars.peek() {
                if nc == '>' {
                    tag.push(chars.next().unwrap());
                    break;
                }
                tag.push(chars.next().unwrap());
            }
            let lower = tag.to_lowercase();
            if lower.starts_with("script") || lower.starts_with("/script") {
                // Skip: don't output this tag
                // For opening <script...>, also skip until </script>
                if lower.starts_with("script") {
                    // Skip everything until </script>
                    let mut buf = String::new();
                    for c2 in chars.by_ref() {
                        buf.push(c2);
                        if buf.ends_with("</script>") || buf.ends_with("</SCRIPT>") {
                            break;
                        }
                    }
                }
            } else {
                // Remove on* event handlers from the tag
                let cleaned = remove_event_handlers(&tag);
                result.push('<');
                result.push_str(&cleaned);
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Remove on* event handler attributes from an HTML tag body.
fn remove_event_handlers(tag_body: &str) -> String {
    // Simple approach: remove attributes starting with "on"
    let mut result = String::with_capacity(tag_body.len());
    let mut i = 0;
    let bytes = tag_body.as_bytes();
    while i < bytes.len() {
        // Look for whitespace + "on" pattern
        if i > 0
            && bytes[i - 1].is_ascii_whitespace()
            && i + 2 < bytes.len()
            && bytes[i] == b'o'
            && bytes[i + 1] == b'n'
        {
            // Skip this attribute (find the = and then the quoted value)
            let start = i;
            let mut j = i + 2;
            // Find =
            while j < bytes.len() && bytes[j] != b'=' && bytes[j] != b'>' {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'=' {
                j += 1;
                // Skip whitespace
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                // Skip quoted value
                if j < bytes.len() && (bytes[j] == b'"' || bytes[j] == b'\'') {
                    let quote = bytes[j];
                    j += 1;
                    while j < bytes.len() && bytes[j] != quote {
                        j += 1;
                    }
                    if j < bytes.len() {
                        j += 1; // skip closing quote
                    }
                }
                i = j;
                let _ = start; // attribute skipped
                continue;
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

/// Parse OG meta tags from HTML.
///
/// Returns (title, description, image_url). Twitter card / `<title>` / standard
/// `<meta name="description">` are used as fallbacks when OG tags are missing.
fn parse_og_tags(html: &str) -> (Option<String>, Option<String>, Option<String>) {
    let (title, description, image, _site_name) = parse_meta_tags(html);
    (title, description, image)
}

/// Parse OG and Twitter card meta tags plus `<title>` and `<meta name="description">`.
///
/// Returns (title, description, image_url, site_name). Each field uses a fallback
/// chain: prefer `og:` then `twitter:` then plain `<title>`/`<meta name>`.
fn parse_meta_tags(
    html: &str,
) -> (Option<String>, Option<String>, Option<String>, Option<String>) {
    let doc = scraper::Html::parse_document(html);

    // og:* and twitter:* are sometimes mistakenly emitted with name= instead of property=,
    // so accept either attribute.
    let meta_selector = scraper::Selector::parse("meta").unwrap();

    let mut og_title = None;
    let mut og_description = None;
    let mut og_image = None;
    let mut og_site_name = None;
    let mut tw_title = None;
    let mut tw_description = None;
    let mut tw_image = None;
    let mut tw_site = None;
    let mut name_description = None;

    for el in doc.select(&meta_selector) {
        let prop = el.value().attr("property").unwrap_or("").to_ascii_lowercase();
        let name = el.value().attr("name").unwrap_or("").to_ascii_lowercase();
        let key = if !prop.is_empty() { prop } else { name };
        let content = match el.value().attr("content") {
            Some(c) if !c.is_empty() => c.to_string(),
            _ => continue,
        };
        match key.as_str() {
            "og:title" => {
                og_title.get_or_insert(content);
            }
            "og:description" => {
                og_description.get_or_insert(content);
            }
            "og:image" | "og:image:url" | "og:image:secure_url" => {
                og_image.get_or_insert(content);
            }
            "og:site_name" => {
                og_site_name.get_or_insert(content);
            }
            "twitter:title" => {
                tw_title.get_or_insert(content);
            }
            "twitter:description" => {
                tw_description.get_or_insert(content);
            }
            "twitter:image" | "twitter:image:src" => {
                tw_image.get_or_insert(content);
            }
            "twitter:site" => {
                tw_site.get_or_insert(content);
            }
            "description" => {
                name_description.get_or_insert(content);
            }
            _ => continue,
        }
    }

    let title_tag = scraper::Selector::parse("title").unwrap();
    let html_title = doc
        .select(&title_tag)
        .next()
        .map(|n| n.text().collect::<String>().trim().to_string())
        .filter(|s| !s.is_empty());

    let title = og_title.or(tw_title).or(html_title);
    let description = og_description.or(tw_description).or(name_description);
    let image = og_image.or(tw_image);
    let site_name = og_site_name.or_else(|| tw_site.map(|s| s.trim_start_matches('@').to_string()));

    (title, description, image, site_name)
}

/// Extract handle from oEmbed author info.
fn extract_handle(
    author_name: &str,
    author_url: Option<&str>,
    social: &ParsedUrl,
) -> String {
    // For Twitter, author_name often starts with "@"
    if let Some(handle) = author_name.strip_prefix('@') {
        return handle.to_string();
    }
    // Try to extract from author_url
    if let Some(url) = author_url {
        if let Some(handle) = url.rsplit('/').next() {
            if !handle.is_empty() {
                return handle.trim_start_matches('@').to_string();
            }
        }
    }
    // Fall back to the user from the URL
    social.user.clone()
}

/// Extract Instagram username from URL (path-based, not from page content).
fn extract_instagram_username(url: &str) -> Option<String> {
    // Instagram URLs don't contain the username in /p/ or /reel/ URLs.
    // We'd need to parse the page for this. Return None for now.
    let _ = url;
    None
}

// ─── Public API ─────────────────────────────────────────────────

/// Resolve embeds for entries that contain recognized URLs.
///
/// For each `.link` entry:
/// 1. Check sidecar cache -- if `meta.json5` exists, load it
/// 2. Otherwise, fetch via oEmbed, OG tags, or iTunes Lookup API
/// 3. Write cache to sidecar directory
/// 4. Set `display_label` on the entry
pub async fn resolve_embeds(
    entries: &mut [crate::entry::Entry],
    content_dir: &Path,
    cache_dir: &Path,
) -> HashMap<PathBuf, EmbedData> {
    let client = http_client();
    let mut cache = HashMap::new();

    for entry in entries.iter_mut() {
        // .link files contain a single URL
        if entry.extension != "link" {
            continue;
        }

        let content = match std::fs::read_to_string(&entry.path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let trimmed = content.trim();

        let parsed = match parse_link_url(trimmed) {
            Some(s) => s,
            None => {
                tracing::debug!(
                    "File {} has .link extension but content is not a recognized URL",
                    entry.path.display()
                );
                continue;
            }
        };

        let entry_cache_dir = cache_dir_for(cache_dir, content_dir, &entry.path);
        let mtime = file_mtime_secs(&entry.path);
        let source = entry
            .path
            .strip_prefix(content_dir)
            .unwrap_or(&entry.path)
            .to_string_lossy()
            .into_owned();

        // Reuse the cache only when it was written for the current mtime; an
        // edited .link (new mtime) reads as a miss and refetches below.
        if let Some(data) = read_cached(&entry_cache_dir, mtime) {
            if !data.is_upstream_deleted() {
                // Only set display_label if entry has no custom label
                if entry.label.is_none() {
                    entry.display_label = Some(data.display_label());
                }
                cache.insert(entry.path.clone(), data);
                continue;
            }
        }

        // Miss or stale -> refetch. Clear any stale directory first so old media
        // never lingers beside the new metadata.
        let _ = std::fs::remove_dir_all(&entry_cache_dir);

        let result = if parsed.platform.has_oembed() {
            fetch_oembed(&client, &parsed).await
        } else if parsed.platform.is_apple() {
            fetch_apple_content(&client, &parsed, &entry_cache_dir).await
        } else if parsed.platform == Platform::Generic {
            fetch_generic_og(&client, &parsed, &entry_cache_dir).await
        } else {
            fetch_og_tags(&client, &parsed).await
        };

        match result {
            Ok(data) => {
                // Cache the result, stamped with the mtime it was fetched for.
                // Without a readable mtime we can't invalidate safely, so skip
                // the write and let the next scan refetch.
                match mtime {
                    Some(mt) => {
                        if let Err(e) = write_cache(&entry_cache_dir, &source, mt, &data) {
                            tracing::error!(
                                "Failed to write embed cache for {}: {}",
                                entry.path.display(),
                                e
                            );
                        }
                    }
                    None => tracing::warn!(
                        "Not caching embed for {} (source mtime unreadable)",
                        entry.path.display()
                    ),
                }

                // Set display_label if no custom label
                if entry.label.is_none() {
                    entry.display_label = Some(data.display_label());
                }

                cache.insert(entry.path.clone(), data);
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to fetch embed for {} ({}): {}",
                    entry.path.display(),
                    parsed.url,
                    e
                );
            }
        }
    }

    cache
}

/// Check liveness of cached embeds. Mark deleted posts.
pub async fn check_liveness(
    embed_cache: &mut HashMap<PathBuf, EmbedData>,
    content_dir: &Path,
    cache_dir: &Path,
    check_interval: Duration,
) {
    let client = http_client();
    let now = chrono::Utc::now();

    for (path, data) in embed_cache.iter_mut() {
        if data.is_upstream_deleted() {
            continue;
        }

        // Check if enough time has passed since last check
        let last_checked = match data {
            EmbedData::Twitter(e) => e.last_checked.as_deref(),
            EmbedData::Bluesky(e) => e.last_checked.as_deref(),
            EmbedData::Mastodon(e) => e.last_checked.as_deref(),
            EmbedData::Instagram(e) => e.last_checked.as_deref(),
            EmbedData::Threads(e) => e.last_checked.as_deref(),
            EmbedData::AppleAppStore(e)
            | EmbedData::AppleMusic(e)
            | EmbedData::ApplePodcasts(e)
            | EmbedData::AppleBooks(e)
            | EmbedData::AppleTV(e) => e.last_checked.as_deref(),
            EmbedData::Generic(e) => e.last_checked.as_deref(),
        };

        if let Some(checked) = last_checked {
            if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(checked) {
                let elapsed = now.signed_duration_since(ts);
                if elapsed < chrono::Duration::from_std(check_interval).unwrap_or(chrono::Duration::hours(24)) {
                    continue;
                }
            }
        }

        let url = data.url().to_string();
        tracing::info!("Liveness check: {}", url);

        let result = client
            .head(&url)
            .send()
            .await;

        let now_str = now.to_rfc3339();
        let is_deleted = match result {
            Ok(resp) => {
                let status = resp.status();
                status == reqwest::StatusCode::NOT_FOUND
                    || status == reqwest::StatusCode::GONE
                    || status == reqwest::StatusCode::FORBIDDEN
            }
            Err(e) => {
                tracing::warn!("Liveness check failed for {}: {}", url, e);
                false // Network error — don't mark as deleted
            }
        };

        // Update last_checked and upstream_deleted
        match data {
            EmbedData::Twitter(e) => {
                e.last_checked = Some(now_str.clone());
                e.upstream_deleted = is_deleted;
            }
            EmbedData::Bluesky(e) => {
                e.last_checked = Some(now_str.clone());
                e.upstream_deleted = is_deleted;
            }
            EmbedData::Mastodon(e) => {
                e.last_checked = Some(now_str.clone());
                e.upstream_deleted = is_deleted;
            }
            EmbedData::Instagram(e) => {
                e.last_checked = Some(now_str.clone());
                e.upstream_deleted = is_deleted;
            }
            EmbedData::Threads(e) => {
                e.last_checked = Some(now_str.clone());
                e.upstream_deleted = is_deleted;
            }
            EmbedData::AppleAppStore(e)
            | EmbedData::AppleMusic(e)
            | EmbedData::ApplePodcasts(e)
            | EmbedData::AppleBooks(e)
            | EmbedData::AppleTV(e) => {
                e.last_checked = Some(now_str.clone());
                e.upstream_deleted = is_deleted;
            }
            EmbedData::Generic(e) => {
                e.last_checked = Some(now_str.clone());
                e.upstream_deleted = is_deleted;
            }
        }

        if is_deleted {
            tracing::warn!("Upstream content removed: {}", url);
        }

        // Persist the updated liveness state to the entry's cache directory,
        // re-stamping the current source mtime so the write stays valid.
        let entry_cache_dir = cache_dir_for(cache_dir, content_dir, path);
        let source = path
            .strip_prefix(content_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();
        match file_mtime_secs(path) {
            Some(mt) => {
                if let Err(e) = write_cache(&entry_cache_dir, &source, mt, data) {
                    tracing::error!("Failed to update embed cache after liveness check: {}", e);
                }
            }
            None => tracing::warn!(
                "Skipping embed cache update for {} (source mtime unreadable)",
                path.display()
            ),
        }
    }
}

// ─── Card rendering ─────────────────────────────────────────────

/// Render an embed as an HTML card.
///
/// `cache_dir` is the entry's cache directory (`<cache>/embeds/<key>`, outside
/// the content tree). It's used to construct local `/_embed/...` URLs for cached
/// assets so visitors don't load images from third-party CDNs (preserves privacy).
pub fn render_embed_card(data: &EmbedData, cache_dir: &Path) -> String {
    if data.is_upstream_deleted() {
        return render_deleted_card(data);
    }

    match data {
        EmbedData::Twitter(e) => render_oembed_card(
            Platform::Twitter,
            &e.author_name,
            &format!("@{}", e.author_handle),
            &e.content_html,
            e.post_date.as_deref(),
            &e.url,
        ),
        EmbedData::Bluesky(e) => render_oembed_card(
            Platform::Bluesky,
            &e.author_name,
            &format!("@{}", e.author_handle),
            &e.content_html,
            e.post_date.as_deref(),
            &e.url,
        ),
        EmbedData::Mastodon(e) => render_oembed_card(
            Platform::Mastodon,
            &e.author_name,
            &format!("@{}@{}", e.author_handle, e.instance),
            &e.content_html,
            e.post_date.as_deref(),
            &e.url,
        ),
        // Non-oEmbed: styled link cards only (no cached content served)
        EmbedData::Instagram(e) => render_link_card(
            Platform::Instagram,
            &format!("@{}", e.username),
            e.description.as_deref(),
            &e.url,
        ),
        EmbedData::Threads(e) => render_link_card(
            Platform::Threads,
            &format!("@{}", e.username),
            e.description.as_deref(),
            &e.url,
        ),
        EmbedData::AppleAppStore(e) => render_apple_card(Platform::AppleAppStore, e, cache_dir),
        EmbedData::AppleMusic(e) => render_apple_card(Platform::AppleMusic, e, cache_dir),
        EmbedData::ApplePodcasts(e) => render_apple_card(Platform::ApplePodcasts, e, cache_dir),
        EmbedData::AppleBooks(e) => render_apple_card(Platform::AppleBooks, e, cache_dir),
        EmbedData::AppleTV(e) => render_apple_card(Platform::AppleTV, e, cache_dir),
        EmbedData::Generic(e) => render_generic_card(e, cache_dir),
    }
}

/// Build a local URL for an asset cached in an entry's cache directory.
///
/// The cache dir's own name is the entry's cache key, so the URL is
/// `/_embed/<key>/<asset>` (the `/_embed` route recomputes the key to find the
/// owning entry). Returns `None` when there's no cache dir to key on (e.g.
/// inline-embed expansion passes an empty path); callers omit the asset then.
fn local_asset_url(cache_dir: &Path, asset_filename: &str) -> Option<String> {
    let key = cache_dir.file_name().and_then(|n| n.to_str())?;
    if key.is_empty() || asset_filename.is_empty() {
        return None;
    }
    Some(format!("/_embed/{}/{}", key, asset_filename))
}

fn render_generic_card(e: &GenericEmbed, cache_dir: &Path) -> String {
    let accent = Platform::Generic.accent_color();

    let source_label = e
        .site_name
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(&e.hostname);

    let image_html = match e
        .image_file
        .as_deref()
        .and_then(|f| local_asset_url(cache_dir, f))
    {
        Some(local) => format!(
            r#"<img class="generic-image" src="{}" alt="" loading="lazy">"#,
            html_escape(&local),
        ),
        None => String::new(),
    };

    let title = e.title.as_deref().unwrap_or("").trim();
    let title_html = if title.is_empty() {
        // If there's no title, fall back to the URL path so the card isn't blank.
        format!(
            r#"<div class="generic-title">{}</div>"#,
            html_escape(&e.url)
        )
    } else {
        format!(
            r#"<div class="generic-title">{}</div>"#,
            html_escape(title)
        )
    };

    let desc_html = match e.description.as_deref() {
        Some(d) if !d.is_empty() => format!(
            r#"<p class="generic-desc">{}</p>"#,
            html_escape(d)
        ),
        _ => String::new(),
    };

    format!(
        r#"<a href="{url}" class="embed-card generic-card" style="--embed-accent: {accent}" rel="noopener noreferrer" target="_blank">
{image_html}
<div class="generic-body">
<div class="generic-source">{source}</div>
{title_html}
{desc_html}
</div>
</a>"#,
        url = html_escape(&e.url),
        accent = accent,
        image_html = image_html,
        source = html_escape(source_label),
        title_html = title_html,
        desc_html = desc_html,
    )
}

fn render_oembed_card(
    platform: Platform,
    author_name: &str,
    author_handle: &str,
    content_html: &str,
    post_date: Option<&str>,
    original_url: &str,
) -> String {
    let accent = platform.accent_color();
    let platform_name = platform.display_name();

    let date_html = match post_date {
        Some(d) => format!(r#"<time>{}</time>"#, html_escape(d)),
        None => String::new(),
    };

    format!(
        r#"<div class="embed-card" style="--embed-accent: {accent}">
<div class="embed-platform">{platform_name}</div>
<div class="embed-author">
<strong>{author_name}</strong> <span>{author_handle}</span>
</div>
<div class="embed-content">{content_html}</div>
{date_html}
<a href="{url}" class="embed-link" rel="noopener noreferrer" target="_blank">View original</a>
</div>"#,
        accent = accent,
        platform_name = html_escape(platform_name),
        author_name = html_escape(author_name),
        author_handle = html_escape(author_handle),
        content_html = content_html,
        date_html = date_html,
        url = html_escape(original_url),
    )
}

fn render_link_card(
    platform: Platform,
    author: &str,
    description: Option<&str>,
    original_url: &str,
) -> String {
    let accent = platform.accent_color();
    let platform_name = platform.display_name();

    let desc_html = match description {
        Some(d) if !d.is_empty() => format!(r#"<p class="embed-desc">{}</p>"#, html_escape(d)),
        _ => String::new(),
    };

    format!(
        r#"<a href="{url}" class="embed-card embed-link-only" style="--embed-accent: {accent}" rel="noopener noreferrer" target="_blank">
<div class="embed-platform">{platform_name}</div>
<div class="embed-author"><strong>{author}</strong></div>
{desc_html}
<span class="embed-link">View on {platform_name}</span>
</a>"#,
        url = html_escape(original_url),
        accent = accent,
        platform_name = html_escape(platform_name),
        author = html_escape(author),
        desc_html = desc_html,
    )
}

fn render_apple_card(platform: Platform, e: &AppleEmbed, cache_dir: &Path) -> String {
    // For App Store, differentiate iOS vs Mac based on content_type from iTunes API
    let (accent, platform_name) = if platform == Platform::AppleAppStore {
        match e.content_type.as_deref() {
            Some("mac-software") => ("#1e88e5", "Mac App Store"),
            _ => ("#0d84ff", "App Store"),
        }
    } else {
        (platform.accent_color(), platform.display_name())
    };

    // Prefer the locally cached artwork so visitors never hit Apple's CDN (preserves
    // visitor privacy). Fall back to the remote URL only when there's no local copy --
    // inline embeds (empty cache_dir) or a cache written before the file was fetched
    // into this sidecar. The existence check keeps stale caches from emitting a broken
    // <img> that 404s against /_embed.
    let artwork_src = e
        .artwork_file
        .as_deref()
        .filter(|f| cache_dir.join(f).exists())
        .and_then(|f| local_asset_url(cache_dir, f))
        .or_else(|| e.artwork_url.clone());

    let artwork_html = match artwork_src {
        Some(src) => format!(
            r#"<img class="apple-artwork" src="{}" alt="" loading="lazy">"#,
            html_escape(&src)
        ),
        None => String::new(),
    };

    let artist_html = if !e.artist_name.is_empty() {
        format!(
            r#"<div class="apple-artist">{}</div>"#,
            html_escape(&e.artist_name)
        )
    } else {
        String::new()
    };

    let mut meta_parts: Vec<String> = Vec::new();
    if let Some(ref price) = e.formatted_price {
        meta_parts.push(html_escape(price));
    }
    if let Some(rating) = e.rating {
        let stars = render_star_rating(rating);
        let count_str = e
            .rating_count
            .map(|c| format!(" ({})", format_count(c)))
            .unwrap_or_default();
        meta_parts.push(format!("{}{}", stars, count_str));
    }
    if let Some(ref genre) = e.genre {
        meta_parts.push(html_escape(genre));
    }

    let meta_html = if meta_parts.is_empty() {
        String::new()
    } else {
        format!(
            r#"<div class="apple-meta">{}</div>"#,
            meta_parts.join(" &middot; ")
        )
    };

    let desc_html = match e.description.as_deref() {
        Some(d) if !d.is_empty() => {
            format!(r#"<p class="apple-desc">{}</p>"#, html_escape(d))
        }
        _ => String::new(),
    };

    format!(
        r#"<a href="{url}" class="embed-card apple-card" style="--embed-accent: {accent}" rel="noopener noreferrer" target="_blank">
<div class="apple-content">
{artwork_html}
<div class="apple-info">
<div class="apple-name">{name}</div>
{artist_html}
{meta_html}
{desc_html}
</div>
</div>
<span class="embed-link">{platform_name}</span>
</a>"#,
        url = html_escape(&e.url),
        accent = accent,
        platform_name = html_escape(platform_name),
        artwork_html = artwork_html,
        name = html_escape(&e.name),
        artist_html = artist_html,
        meta_html = meta_html,
        desc_html = desc_html,
    )
}

/// Render a star rating as HTML (filled/empty stars).
fn render_star_rating(rating: f64) -> String {
    let mut stars = String::new();
    let rounded = (rating * 2.0).round() / 2.0; // round to nearest 0.5
    for i in 1..=5 {
        let i_f = i as f64;
        if i_f <= rounded {
            stars.push_str("<span class=\"star-full\">\u{2605}</span>");
        } else if i_f - 0.5 <= rounded {
            stars.push_str("<span class=\"star-half\">\u{2605}</span>");
        } else {
            stars.push_str("<span class=\"star-empty\">\u{2606}</span>");
        }
    }
    format!(r#"<span class="apple-stars">{}</span>"#, stars)
}

/// Format a count for display (e.g. 1500 -> "1.5K", 2300000 -> "2.3M").
fn format_count(count: u64) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}K", count as f64 / 1_000.0)
    } else {
        count.to_string()
    }
}

fn render_deleted_card(data: &EmbedData) -> String {
    let platform = data.platform();
    let accent = platform.accent_color();
    let platform_name = platform.display_name();

    let removed_text = if platform.is_apple() {
        format!("This content is no longer available on {}.", platform_name)
    } else {
        format!("This post has been removed from {}.", platform_name)
    };

    format!(
        r#"<div class="embed-card embed-deleted" style="--embed-accent: {accent}">
<div class="embed-platform">{platform_name}</div>
<p>{removed_text}</p>
</div>"#,
        accent = accent,
        platform_name = html_escape(platform_name),
        removed_text = removed_text,
    )
}

/// Expand inline URLs in rendered HTML to embed cards.
///
/// Looks for `<a href="...">URL</a>` where the link text equals the href
/// and the href is a recognized social media URL.
pub fn expand_inline_embeds(html: &str, cache: &HashMap<PathBuf, EmbedData>) -> String {
    // Build a lookup from URL -> EmbedData
    let url_lookup: HashMap<&str, &EmbedData> = cache
        .values()
        .map(|d| (d.url(), d))
        .collect();

    if url_lookup.is_empty() {
        return html.to_string();
    }

    // Find bare URL links: <a href="URL">URL</a> (where text == href)
    // Using a simple approach — find <a> tags where content matches href
    let doc = scraper::Html::parse_fragment(html);
    let selector = scraper::Selector::parse("a[href]").unwrap();

    let mut result = html.to_string();

    // Collect replacements (work backwards to not invalidate offsets)
    let mut replacements: Vec<(String, String)> = Vec::new();

    for el in doc.select(&selector) {
        let href = el.value().attr("href").unwrap_or("");
        let text: String = el.text().collect();
        let text = text.trim();

        // Only replace when the link text IS the URL (bare URL auto-linked by Pandoc)
        if text == href {
            if let Some(embed_data) = url_lookup.get(href) {
                if !embed_data.is_upstream_deleted() {
                    let card = render_embed_card(embed_data, Path::new(""));
                    // Build the original <a> tag string to replace
                    let original = format!(
                        "<a href=\"{}\">{}</a>",
                        href, text
                    );
                    replacements.push((original, card));
                }
            }
        }
    }

    for (original, replacement) in replacements {
        result = result.replacen(&original, &replacement, 1);
    }

    result
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ─── CSS ────────────────────────────────────────────────────────

pub const EMBED_CSS: &str = r#"
/* Embed cards */

.embed-card {
  border-left: 3px solid var(--embed-accent, var(--color-link));
  padding: 1em 1.2em;
  margin: 1.5em 0;
  background: var(--color-card-bg);
  border-radius: 0 .5em .5em 0;
  border-top: 1px solid var(--glass-border);
  border-right: 1px solid var(--glass-border);
  border-bottom: 1px solid var(--glass-border);
}

.embed-card .embed-platform {
  font-size: .75em;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: .05em;
  color: var(--embed-accent, var(--color-link));
  margin-bottom: .4em;
}

.embed-card .embed-author {
  margin-bottom: .5em;
}

.embed-card .embed-author strong {
  font-size: .95em;
}

.embed-card .embed-author span {
  color: var(--color-faint);
  font-size: .85em;
}

.embed-card .embed-content {
  margin-bottom: .6em;
  line-height: 1.5;
}

.embed-card .embed-content blockquote {
  margin: 0;
  padding: 0;
  border: none;
}

.embed-card .embed-content p {
  margin: 0 0 .5em;
}

.embed-card time {
  display: block;
  font-size: .8em;
  color: var(--color-faint);
  margin-bottom: .4em;
}

.embed-card .embed-link {
  font-size: .8em;
  color: var(--embed-accent, var(--color-link));
}

.embed-card .embed-desc {
  color: var(--color-muted);
  font-size: .9em;
  margin-bottom: .5em;
}

.embed-card.embed-link-only {
  display: block;
  text-decoration: none;
  transition: border-color .15s;
}

.embed-card.embed-link-only:hover {
  border-left-color: var(--color-link-hover);
  text-decoration: none;
}

.embed-card.embed-deleted {
  opacity: .6;
  font-style: italic;
}

.embed-card.embed-deleted p {
  color: var(--color-faint);
  margin: 0;
}

/* Apple content cards */

.apple-card {
  display: block;
  text-decoration: none;
  transition: border-color .15s;
  padding: .7em 1em;
}

.apple-card:hover {
  border-left-color: var(--color-link-hover);
  text-decoration: none;
}

.apple-content {
  display: flex;
  gap: .8em;
  align-items: center;
}

.apple-artwork {
  width: 64px;
  height: 64px;
  border-radius: 14px;
  object-fit: cover;
  flex-shrink: 0;
}

.apple-info {
  flex: 1;
  min-width: 0;
}

.apple-name {
  font-weight: 700;
  font-size: .9em;
  line-height: 1.2;
}

.apple-artist {
  color: var(--color-faint);
  font-size: .8em;
}

.apple-meta {
  font-size: .75em;
  color: var(--color-faint);
  display: flex;
  flex-wrap: wrap;
  gap: .1em;
  align-items: center;
}

.apple-desc {
  color: var(--color-muted);
  font-size: .8em;
  line-height: 1.3;
  margin: .2em 0 0;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.apple-card > .embed-link {
  display: none;
}

.apple-stars {
  letter-spacing: -.05em;
}

.apple-stars .star-full {
  color: var(--embed-accent, #f5a623);
}

.apple-stars .star-half {
  color: var(--embed-accent, #f5a623);
  opacity: .5;
}

.apple-stars .star-empty {
  color: var(--color-faint);
  opacity: .3;
}

/* Generic OG card (any HTTPS URL not matching a more specific platform) */

.generic-card {
  display: block;
  text-decoration: none;
  transition: border-color .15s;
  padding: 0;
  overflow: hidden;
}

.generic-card:hover {
  border-left-color: var(--color-link-hover);
  text-decoration: none;
}

.generic-card .generic-image {
  display: block;
  width: 100%;
  max-height: 18em;
  object-fit: cover;
  background: var(--color-card-bg);
}

.generic-card .generic-body {
  padding: .9em 1.1em;
}

.generic-card .generic-source {
  font-size: .72em;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: .05em;
  color: var(--embed-accent, var(--color-link));
  margin-bottom: .3em;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.generic-card .generic-title {
  font-weight: 600;
  font-size: 1em;
  line-height: 1.3;
  margin-bottom: .25em;
  color: var(--color-fg);
}

.generic-card .generic-desc {
  color: var(--color-muted);
  font-size: .88em;
  line-height: 1.4;
  margin: 0;
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
"#;

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_twitter_url() {
        let s = parse_link_url("https://twitter.com/elonmusk/status/1234567890").unwrap();
        assert_eq!(s.platform, Platform::Twitter);
        assert_eq!(s.user, "elonmusk");
        assert_eq!(s.item_id, "1234567890");
    }

    #[test]
    fn parse_x_url() {
        let s = parse_link_url("https://x.com/user/status/999?s=20").unwrap();
        assert_eq!(s.platform, Platform::Twitter);
        assert_eq!(s.user, "user");
        assert_eq!(s.item_id, "999");
    }

    #[test]
    fn parse_bluesky_url() {
        let s = parse_link_url("https://bsky.app/profile/alice.bsky.social/post/3abc123").unwrap();
        assert_eq!(s.platform, Platform::Bluesky);
        assert_eq!(s.user, "alice.bsky.social");
        assert_eq!(s.item_id, "3abc123");
    }

    #[test]
    fn parse_mastodon_url() {
        let s = parse_link_url("https://mastodon.social/@user/123456789").unwrap();
        assert_eq!(s.platform, Platform::Mastodon);
        assert_eq!(s.user, "user");
        assert_eq!(s.item_id, "123456789");
        assert_eq!(s.instance.as_deref(), Some("mastodon.social"));
    }

    #[test]
    fn parse_instagram_url() {
        let s = parse_link_url("https://www.instagram.com/p/ABC123xyz/").unwrap();
        assert_eq!(s.platform, Platform::Instagram);
        assert_eq!(s.item_id, "ABC123xyz");
    }

    #[test]
    fn parse_instagram_reel() {
        let s = parse_link_url("https://www.instagram.com/reel/XYZ789/").unwrap();
        assert_eq!(s.platform, Platform::Instagram);
        assert_eq!(s.item_id, "XYZ789");
    }

    #[test]
    fn parse_threads_url() {
        let s = parse_link_url("https://www.threads.net/@user/post/ABC123").unwrap();
        assert_eq!(s.platform, Platform::Threads);
        assert_eq!(s.user, "user");
        assert_eq!(s.item_id, "ABC123");
    }

    #[test]
    fn parse_apple_appstore_url() {
        let s = parse_link_url("https://apps.apple.com/us/app/things-3/id904237743").unwrap();
        assert_eq!(s.platform, Platform::AppleAppStore);
        assert_eq!(s.item_id, "904237743");
        assert_eq!(s.country.as_deref(), Some("us"));
    }

    #[test]
    fn parse_apple_music_album_url() {
        let s = parse_link_url("https://music.apple.com/us/album/random-access-memories/617154241").unwrap();
        assert_eq!(s.platform, Platform::AppleMusic);
        assert_eq!(s.item_id, "617154241");
        assert_eq!(s.country.as_deref(), Some("us"));
    }

    #[test]
    fn parse_apple_music_playlist_url() {
        let s = parse_link_url("https://music.apple.com/us/playlist/todays-hits/pl.f4d106fed2bd41149aaacabb233eb5eb").unwrap();
        assert_eq!(s.platform, Platform::AppleMusic);
        assert_eq!(s.item_id, "pl.f4d106fed2bd41149aaacabb233eb5eb");
    }

    #[test]
    fn parse_apple_podcasts_url() {
        let s = parse_link_url("https://podcasts.apple.com/us/podcast/the-daily/id1200361736").unwrap();
        assert_eq!(s.platform, Platform::ApplePodcasts);
        assert_eq!(s.item_id, "1200361736");
    }

    #[test]
    fn parse_apple_books_url() {
        let s = parse_link_url("https://books.apple.com/us/book/the-great-gatsby/id498685929").unwrap();
        assert_eq!(s.platform, Platform::AppleBooks);
        assert_eq!(s.item_id, "498685929");
    }

    #[test]
    fn parse_apple_tv_show_url() {
        let s = parse_link_url("https://tv.apple.com/us/show/severance/umc.cmc.1srk2goyh2q2zdxcx605w8vtx").unwrap();
        assert_eq!(s.platform, Platform::AppleTV);
        assert_eq!(s.item_id, "umc.cmc.1srk2goyh2q2zdxcx605w8vtx");
    }

    #[test]
    fn parse_itunes_movie_url() {
        let s = parse_link_url("https://itunes.apple.com/gb/movie/inception/id400763833").unwrap();
        assert_eq!(s.platform, Platform::AppleTV);
        assert_eq!(s.item_id, "400763833");
        assert_eq!(s.country.as_deref(), Some("gb"));
    }

    #[test]
    fn unrecognized_https_url_falls_back_to_generic() {
        // Any well-formed https:// URL that doesn't match a more specific platform
        // becomes a Generic embed — title/description/image come from OG tags at fetch time.
        let s = parse_link_url("https://j-fidel-505.neocities.org/roundtables").unwrap();
        assert_eq!(s.platform, Platform::Generic);
        assert_eq!(s.url, "https://j-fidel-505.neocities.org/roundtables");
        assert_eq!(s.item_id, "j-fidel-505.neocities.org"); // hostname stashed here

        let s = parse_link_url("https://example.com/page").unwrap();
        assert_eq!(s.platform, Platform::Generic);
        assert_eq!(s.item_id, "example.com");
    }

    #[test]
    fn reject_unparseable_url() {
        assert!(parse_link_url("not a url").is_none());
        // No dot in hostname — bare names aren't useful as OG sources
        assert!(parse_link_url("https://localhost/").is_none());
        // http:// is intentionally not supported — we don't render plaintext sources
        assert!(parse_link_url("http://example.com/").is_none());
        // Loopback / private literals
        assert!(parse_link_url("https://127.0.0.1/").is_none());
        assert!(parse_link_url("https://192.168.1.1/").is_none());
    }

    #[test]
    fn extract_hostname_strips_www() {
        assert_eq!(extract_hostname("https://www.example.com/x"), Some("example.com".to_string()));
        assert_eq!(extract_hostname("https://example.com:8443/x"), Some("example.com".to_string()));
        assert_eq!(extract_hostname("https://user@example.com/"), None);
    }

    #[test]
    fn resolve_url_relative_handles_common_cases() {
        let base = "https://example.com/a/b.html";
        assert_eq!(
            resolve_url_relative_to(base, "https://cdn.example.com/img.png"),
            Some("https://cdn.example.com/img.png".to_string())
        );
        assert_eq!(
            resolve_url_relative_to(base, "//cdn.example.com/img.png"),
            Some("https://cdn.example.com/img.png".to_string())
        );
        assert_eq!(
            resolve_url_relative_to(base, "/static/img.png"),
            Some("https://example.com/static/img.png".to_string())
        );
        assert_eq!(
            resolve_url_relative_to(base, "img.png"),
            Some("https://example.com/a/img.png".to_string())
        );
        // Drop http:// candidates
        assert_eq!(
            resolve_url_relative_to(base, "http://cdn.example.com/img.png"),
            None
        );
    }

    #[test]
    fn parse_meta_tags_prefers_og_over_twitter_over_html() {
        let html = r#"<html><head>
            <title>HTML Title</title>
            <meta property="og:title" content="OG Title">
            <meta name="twitter:title" content="Twitter Title">
            <meta property="og:description" content="OG Desc">
            <meta property="og:image" content="https://x/og.png">
            <meta property="og:site_name" content="Site Name">
        </head><body></body></html>"#;
        let (title, desc, image, site) = parse_meta_tags(html);
        assert_eq!(title.as_deref(), Some("OG Title"));
        assert_eq!(desc.as_deref(), Some("OG Desc"));
        assert_eq!(image.as_deref(), Some("https://x/og.png"));
        assert_eq!(site.as_deref(), Some("Site Name"));
    }

    #[test]
    fn parse_meta_tags_falls_back_to_twitter_and_title_tag() {
        let html = r#"<html><head>
            <title>Just Title</title>
            <meta name="twitter:description" content="Tweet Desc">
        </head></html>"#;
        let (title, desc, image, _) = parse_meta_tags(html);
        assert_eq!(title.as_deref(), Some("Just Title"));
        assert_eq!(desc.as_deref(), Some("Tweet Desc"));
        assert!(image.is_none());
    }

    #[test]
    fn truncate_for_display_handles_unicode() {
        let s = "café".repeat(100);
        let t = truncate_for_display(&s, 10);
        assert!(t.chars().count() <= 11); // 10 + ellipsis
        assert!(t.ends_with('\u{2026}'));
    }

    #[test]
    fn generic_display_label_combines_title_and_source() {
        let data = EmbedData::Generic(GenericEmbed {
            url: "https://example.com/x".to_string(),
            hostname: "example.com".to_string(),
            site_name: Some("Example Site".to_string()),
            title: Some("Hello".to_string()),
            description: None,
            image_url: None,
            image_file: None,
            upstream_deleted: false,
            last_checked: None,
        });
        assert_eq!(data.display_label(), "Hello \u{00b7} Example Site");

        // Without site_name, falls back to hostname
        let data = EmbedData::Generic(GenericEmbed {
            url: "https://example.com/x".to_string(),
            hostname: "example.com".to_string(),
            site_name: None,
            title: Some("Hello".to_string()),
            description: None,
            image_url: None,
            image_file: None,
            upstream_deleted: false,
            last_checked: None,
        });
        assert_eq!(data.display_label(), "Hello \u{00b7} example.com");
    }

    #[test]
    fn local_asset_url_uses_cache_dir_name_as_key() {
        let dir = PathBuf::from("/cache/embeds/deadbeef");
        assert_eq!(
            local_asset_url(&dir, "og.png"),
            Some("/_embed/deadbeef/og.png".to_string())
        );
        // No cache dir name to key on (inline-embed path) -> no URL.
        assert!(local_asset_url(Path::new(""), "x.png").is_none());
        // Empty asset -> no URL.
        assert!(local_asset_url(&dir, "").is_none());
    }

    #[test]
    fn sanitize_removes_scripts() {
        let input = r#"<blockquote>hello</blockquote><script>alert("xss")</script>"#;
        let output = sanitize_oembed_html(input);
        assert!(!output.contains("script"));
        assert!(output.contains("<blockquote>hello</blockquote>"));
    }

    #[test]
    fn cache_dir_is_outside_content_and_keyed_by_rel_path() {
        let content = Path::new("/content");
        let cache = Path::new("/cache");
        let dir = cache_dir_for(cache, content, Path::new("/content/a/b.link"));
        // Lives under <cache>/embeds/, never inside the content tree.
        assert!(dir.starts_with("/cache/embeds"));
        assert!(!dir.starts_with("/content"));
        // The leaf is the hex key of the content-relative path.
        assert_eq!(
            dir.file_name().unwrap().to_str().unwrap(),
            cache_key(content, Path::new("/content/a/b.link"))
        );
    }

    #[test]
    fn cache_key_is_deterministic_and_path_sensitive() {
        let content = Path::new("/content");
        let k1 = cache_key(content, Path::new("/content/x.link"));
        let k2 = cache_key(content, Path::new("/content/x.link"));
        let k3 = cache_key(content, Path::new("/content/y.link"));
        assert_eq!(k1, k2);
        assert_ne!(k1, k3);
        // 32-byte SHA-256 -> 64 hex chars.
        assert_eq!(k1.len(), 64);
        assert!(k1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn cache_roundtrip_honors_mtime() {
        let mut dir = std::env::temp_dir();
        dir.push(format!("esko-embed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let data = EmbedData::Generic(GenericEmbed {
            url: "https://example.com/x".to_string(),
            hostname: "example.com".to_string(),
            site_name: None,
            title: Some("Hi".to_string()),
            description: None,
            image_url: None,
            image_file: None,
            upstream_deleted: false,
            last_checked: None,
        });

        write_cache(&dir, "x.link", 100, &data).unwrap();

        // Matching mtime -> hit.
        assert!(read_cached(&dir, Some(100)).is_some());
        // Drifted mtime (entry edited) -> miss.
        assert!(read_cached(&dir, Some(101)).is_none());
        // No expectation -> returns whatever is stored.
        assert!(read_cached(&dir, None).is_some());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn display_label_twitter() {
        let data = EmbedData::Twitter(TwitterEmbed {
            url: "https://twitter.com/user/status/1".to_string(),
            author_name: "User Name".to_string(),
            author_handle: "user".to_string(),
            content_html: String::new(),
            post_date: Some("2026-03-10".to_string()),
            media_files: Vec::new(),
            upstream_deleted: false,
            last_checked: None,
        });
        assert_eq!(data.display_label(), "@user \u{00b7} 2026-03-10");
    }

    #[test]
    fn display_label_mastodon() {
        let data = EmbedData::Mastodon(MastodonEmbed {
            url: "https://mastodon.social/@user/1".to_string(),
            instance: "mastodon.social".to_string(),
            author_name: "User".to_string(),
            author_handle: "user".to_string(),
            content_html: String::new(),
            post_date: None,
            media_files: Vec::new(),
            upstream_deleted: false,
            last_checked: None,
        });
        assert_eq!(data.display_label(), "@user@mastodon.social");
    }
}
