//! Social media embed support.
//!
//! Fetches and caches social media post data for display as cards.
//! - oEmbed platforms (Twitter, Bluesky, Mastodon): full cached cards served locally
//! - Non-oEmbed platforms (Instagram, Threads): archived for personal use, styled link cards served publicly
//!
//! Periodic liveness checks verify originals are still public; deleted posts stop being served.

use serde::{Deserialize, Serialize};
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
}

impl Platform {
    /// Whether this platform has a public oEmbed API (= implicit license to embed).
    pub fn has_oembed(&self) -> bool {
        matches!(self, Platform::Twitter | Platform::Bluesky | Platform::Mastodon)
    }

    /// CSS accent color for this platform.
    pub fn accent_color(&self) -> &'static str {
        match self {
            Platform::Twitter => "#1d9bf0",
            Platform::Bluesky => "#0085ff",
            Platform::Mastodon => "#6364ff",
            Platform::Instagram => "#e1306c",
            Platform::Threads => "var(--color-fg)",
        }
    }

    /// Human-readable platform name.
    pub fn display_name(&self) -> &'static str {
        match self {
            Platform::Twitter => "Twitter",
            Platform::Bluesky => "Bluesky",
            Platform::Mastodon => "Mastodon",
            Platform::Instagram => "Instagram",
            Platform::Threads => "Threads",
        }
    }
}

/// Parsed social media URL.
#[derive(Debug, Clone)]
pub struct SocialUrl {
    pub platform: Platform,
    pub url: String,
    pub user: String,
    pub post_id: String,
    /// Mastodon instance hostname (only set for Mastodon URLs).
    pub instance: Option<String>,
}

/// Try to parse a URL as a known social media post link.
pub fn parse_social_url(url: &str) -> Option<SocialUrl> {
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
            // Strip query string / fragment from post_id
            let post_id = parts[2].split(['?', '#']).next().unwrap_or(parts[2]);
            return Some(SocialUrl {
                platform: Platform::Twitter,
                url: url.to_string(),
                user: parts[0].to_string(),
                post_id: post_id.to_string(),
                instance: None,
            });
        }
    }

    // Bluesky
    // https://bsky.app/profile/{handle}/post/{rkey}
    if let Some(rest) = url.strip_prefix("https://bsky.app/profile/") {
        let parts: Vec<&str> = rest.splitn(4, '/').collect();
        if parts.len() >= 3 && parts[1] == "post" && !parts[0].is_empty() && !parts[2].is_empty() {
            let post_id = parts[2].split(['?', '#']).next().unwrap_or(parts[2]);
            return Some(SocialUrl {
                platform: Platform::Bluesky,
                url: url.to_string(),
                user: parts[0].to_string(),
                post_id: post_id.to_string(),
                instance: None,
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
            return Some(SocialUrl {
                platform: Platform::Instagram,
                url: url.to_string(),
                user: String::new(), // unknown until we fetch
                post_id: shortcode.to_string(),
                instance: None,
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
            return Some(SocialUrl {
                platform: Platform::Threads,
                url: url.to_string(),
                user: parts[0].to_string(),
                post_id: shortcode.to_string(),
                instance: None,
            });
        }
    }

    // Mastodon / Fediverse
    // https://{instance}/@{user}/{post_id}
    // Must be validated via oEmbed discovery later
    if url.starts_with("https://") {
        let without_scheme = &url["https://".len()..];
        let parts: Vec<&str> = without_scheme.splitn(4, '/').collect();
        if parts.len() >= 3
            && parts[1].starts_with('@')
            && parts[1].len() > 1
            && !parts[2].is_empty()
        {
            let post_id = parts[2].split(['?', '#']).next().unwrap_or(parts[2]);
            // Basic check: post_id should be numeric for Mastodon
            if post_id.chars().all(|c| c.is_ascii_digit()) {
                let instance = parts[0].to_string();
                let user = parts[1][1..].to_string(); // strip @
                return Some(SocialUrl {
                    platform: Platform::Mastodon,
                    url: url.to_string(),
                    user,
                    post_id: post_id.to_string(),
                    instance: Some(instance),
                });
            }
        }
    }

    None
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
}

impl EmbedData {
    pub fn platform(&self) -> Platform {
        match self {
            EmbedData::Twitter(_) => Platform::Twitter,
            EmbedData::Bluesky(_) => Platform::Bluesky,
            EmbedData::Mastodon(_) => Platform::Mastodon,
            EmbedData::Instagram(_) => Platform::Instagram,
            EmbedData::Threads(_) => Platform::Threads,
        }
    }

    pub fn url(&self) -> &str {
        match self {
            EmbedData::Twitter(e) => &e.url,
            EmbedData::Bluesky(e) => &e.url,
            EmbedData::Mastodon(e) => &e.url,
            EmbedData::Instagram(e) => &e.url,
            EmbedData::Threads(e) => &e.url,
        }
    }

    /// Generate a display label like "@handle · date" for timeline display.
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
        }
    }

    /// Whether this embed has oEmbed-licensed content that can be served publicly.
    pub fn can_serve_content(&self) -> bool {
        self.platform().has_oembed()
    }

    /// Whether the original post has been confirmed deleted/unavailable.
    pub fn is_upstream_deleted(&self) -> bool {
        match self {
            EmbedData::Twitter(e) => e.upstream_deleted,
            EmbedData::Bluesky(e) => e.upstream_deleted,
            EmbedData::Mastodon(e) => e.upstream_deleted,
            EmbedData::Instagram(e) => e.upstream_deleted,
            EmbedData::Threads(e) => e.upstream_deleted,
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

// ─── Sidecar cache I/O ──────────────────────────────────────────

/// Get the sidecar cache directory path for an entry file.
/// e.g. `content/2026-03-10T120000.txt` -> `content/2026-03-10T120000.txt.embed-cache/`
pub fn cache_dir_for(entry_path: &Path) -> PathBuf {
    let mut dir = entry_path.as_os_str().to_owned();
    dir.push(".embed-cache");
    PathBuf::from(dir)
}

fn meta_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join("meta.json5")
}

/// Read cached embed data from sidecar directory, if it exists.
pub fn read_cached(entry_path: &Path) -> Option<EmbedData> {
    let dir = cache_dir_for(entry_path);
    let meta = meta_path(&dir);
    let content = std::fs::read_to_string(&meta).ok()?;
    match json5::from_str::<EmbedData>(&content) {
        Ok(data) => Some(data),
        Err(e) => {
            tracing::warn!(
                "Failed to parse embed cache {}: {}",
                meta.display(),
                e
            );
            None
        }
    }
}

/// Write embed data to sidecar cache directory.
fn write_cache(entry_path: &Path, data: &EmbedData) -> std::io::Result<()> {
    let dir = cache_dir_for(entry_path);
    std::fs::create_dir_all(&dir)?;
    let meta = meta_path(&dir);
    let json = serde_json::to_string_pretty(data)
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
fn oembed_endpoint(social: &SocialUrl) -> Option<String> {
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
async fn fetch_oembed(client: &reqwest::Client, social: &SocialUrl) -> Result<EmbedData, String> {
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
async fn fetch_og_tags(client: &reqwest::Client, social: &SocialUrl) -> Result<EmbedData, String> {
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
                shortcode: social.post_id.clone(),
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
            shortcode: social.post_id.clone(),
            title,
            description,
            image_file: None,
            upstream_deleted: false,
            last_checked: Some(now),
        })),
        _ => Err("Platform does not use OG tags".to_string()),
    }
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
fn parse_og_tags(html: &str) -> (Option<String>, Option<String>, Option<String>) {
    let doc = scraper::Html::parse_document(html);
    let selector = scraper::Selector::parse("meta[property]").unwrap();

    let mut title = None;
    let mut description = None;
    let mut image = None;

    for el in doc.select(&selector) {
        let prop = el.value().attr("property").unwrap_or("");
        let content = el.value().attr("content");
        match prop {
            "og:title" => title = content.map(|s| s.to_string()),
            "og:description" => description = content.map(|s| s.to_string()),
            "og:image" => image = content.map(|s| s.to_string()),
            _ => {}
        }
    }

    (title, description, image)
}

/// Extract handle from oEmbed author info.
fn extract_handle(
    author_name: &str,
    author_url: Option<&str>,
    social: &SocialUrl,
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

/// Resolve embeds for entries that contain social media URLs.
///
/// For each entry whose content is a bare social URL:
/// 1. Check sidecar cache — if `meta.json5` exists, load it
/// 2. Otherwise, fetch via oEmbed or OG tags
/// 3. Write cache to sidecar directory
/// 4. Set `display_label` on the entry
pub async fn resolve_embeds(
    entries: &mut [crate::entry::Entry],
    content_dir: &Path,
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

        let social = match parse_social_url(trimmed) {
            Some(s) => s,
            None => {
                tracing::debug!(
                    "File {} has .link extension but content is not a recognized social URL",
                    entry.path.display()
                );
                continue;
            }
        };

        // Check sidecar cache first
        if let Some(data) = read_cached(&entry.path) {
            if !data.is_upstream_deleted() {
                // Only set display_label if entry has no custom label
                if entry.label.is_none() {
                    entry.display_label = Some(data.display_label());
                }
                cache.insert(entry.path.clone(), data);
                continue;
            }
        }

        // Fetch fresh data
        let result = if social.platform.has_oembed() {
            fetch_oembed(&client, &social).await
        } else {
            fetch_og_tags(&client, &social).await
        };

        match result {
            Ok(data) => {
                // Write cache
                if let Err(e) = write_cache(&entry.path, &data) {
                    tracing::error!("Failed to write embed cache for {}: {}", entry.path.display(), e);
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
                    social.url,
                    e
                );
            }
        }
    }

    let _ = content_dir; // used for relative path calculations if needed later
    cache
}

/// Check liveness of cached embeds. Mark deleted posts.
pub async fn check_liveness(
    embed_cache: &mut HashMap<PathBuf, EmbedData>,
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
        }

        if is_deleted {
            tracing::warn!("Upstream post deleted: {}", url);
        }

        // Update the cache file on disk
        if let Err(e) = write_cache(path, data) {
            tracing::error!("Failed to update embed cache after liveness check: {}", e);
        }
    }
}

// ─── Card rendering ─────────────────────────────────────────────

/// Render an embed as an HTML card.
pub fn render_embed_card(data: &EmbedData, _cache_dir: &Path) -> String {
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
    }
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

fn render_deleted_card(data: &EmbedData) -> String {
    let platform = data.platform();
    let accent = platform.accent_color();
    let platform_name = platform.display_name();

    format!(
        r#"<div class="embed-card embed-deleted" style="--embed-accent: {accent}">
<div class="embed-platform">{platform_name}</div>
<p>This post has been removed from {platform_name}.</p>
</div>"#,
        accent = accent,
        platform_name = html_escape(platform_name),
    )
}

/// Expand inline social media URLs in rendered HTML to embed cards.
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
/* Social media embed cards */

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
"#;

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_twitter_url() {
        let s = parse_social_url("https://twitter.com/elonmusk/status/1234567890").unwrap();
        assert_eq!(s.platform, Platform::Twitter);
        assert_eq!(s.user, "elonmusk");
        assert_eq!(s.post_id, "1234567890");
    }

    #[test]
    fn parse_x_url() {
        let s = parse_social_url("https://x.com/user/status/999?s=20").unwrap();
        assert_eq!(s.platform, Platform::Twitter);
        assert_eq!(s.user, "user");
        assert_eq!(s.post_id, "999");
    }

    #[test]
    fn parse_bluesky_url() {
        let s = parse_social_url("https://bsky.app/profile/alice.bsky.social/post/3abc123").unwrap();
        assert_eq!(s.platform, Platform::Bluesky);
        assert_eq!(s.user, "alice.bsky.social");
        assert_eq!(s.post_id, "3abc123");
    }

    #[test]
    fn parse_mastodon_url() {
        let s = parse_social_url("https://mastodon.social/@user/123456789").unwrap();
        assert_eq!(s.platform, Platform::Mastodon);
        assert_eq!(s.user, "user");
        assert_eq!(s.post_id, "123456789");
        assert_eq!(s.instance.as_deref(), Some("mastodon.social"));
    }

    #[test]
    fn parse_instagram_url() {
        let s = parse_social_url("https://www.instagram.com/p/ABC123xyz/").unwrap();
        assert_eq!(s.platform, Platform::Instagram);
        assert_eq!(s.post_id, "ABC123xyz");
    }

    #[test]
    fn parse_instagram_reel() {
        let s = parse_social_url("https://www.instagram.com/reel/XYZ789/").unwrap();
        assert_eq!(s.platform, Platform::Instagram);
        assert_eq!(s.post_id, "XYZ789");
    }

    #[test]
    fn parse_threads_url() {
        let s = parse_social_url("https://www.threads.net/@user/post/ABC123").unwrap();
        assert_eq!(s.platform, Platform::Threads);
        assert_eq!(s.user, "user");
        assert_eq!(s.post_id, "ABC123");
    }

    #[test]
    fn reject_non_social_url() {
        assert!(parse_social_url("https://example.com/page").is_none());
        assert!(parse_social_url("not a url").is_none());
    }

    #[test]
    fn sanitize_removes_scripts() {
        let input = r#"<blockquote>hello</blockquote><script>alert("xss")</script>"#;
        let output = sanitize_oembed_html(input);
        assert!(!output.contains("script"));
        assert!(output.contains("<blockquote>hello</blockquote>"));
    }

    #[test]
    fn cache_dir_naming() {
        let path = PathBuf::from("/content/2026-03-10T120000.txt");
        let dir = cache_dir_for(&path);
        assert_eq!(dir, PathBuf::from("/content/2026-03-10T120000.txt.embed-cache"));
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
