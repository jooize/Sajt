//! The site configuration file (`sajt.toml`): the one place the
//! site's own identity lives. The engine is generic; the domain is site
//! data, and everything that wants a name derives it from here — the
//! generated Caddyfile's site block (and whatever container or service unit
//! wraps it), the embed fetcher's contact URL.
//!
//! The file lives next to the content directory, never inside it: the
//! content tree is the published surface and stays pure content.

use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SiteConfig {
    /// The site's domain, a bare host name like "example.org" — no scheme, no
    /// path. Optional: without it the engine stays unbranded and adapters
    /// fall back to local addresses.
    pub domain: Option<String>,
}

/// A domain must be a plausible bare host: rejecting schemes, slashes,
/// whitespace, and control characters up front keeps every consumer (UA
/// strings, Caddyfile site addresses) from having to re-validate.
fn valid_domain(domain: &str) -> bool {
    !domain.is_empty()
        && domain.len() <= 253
        && domain
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
}

/// Load the config at `path`. A missing file is the default config — the
/// engine runs unconfigured — but a file that exists and cannot be parsed is
/// an error: a typo silently reverting the site to defaults would be a trap.
/// Unknown keys are errors for the same reason.
pub fn load(path: &Path) -> Result<SiteConfig, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(SiteConfig::default()),
        Err(e) => return Err(format!("cannot read config {}: {}", path.display(), e)),
    };
    let config: SiteConfig = toml::from_str(&text)
        .map_err(|e| format!("cannot parse config {}: {}", path.display(), e))?;
    if let Some(domain) = &config.domain {
        if !valid_domain(domain) {
            return Err(format!(
                "config {}: domain {:?} is not a bare host name (letters, digits, '-', '.')",
                path.display(),
                domain
            ));
        }
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domains_are_bare_hosts_only() {
        assert!(valid_domain("example.org"));
        assert!(valid_domain("xn--sthlm-nua.example"));
        assert!(!valid_domain(""));
        assert!(!valid_domain("https://example.org"));
        assert!(!valid_domain("example.org/path"));
        assert!(!valid_domain("example org"));
        assert!(!valid_domain("example.org\n"));
    }

    #[test]
    fn parsing_fails_closed() {
        assert!(toml::from_str::<SiteConfig>("domain = \"example.org\"").is_ok());
        // Unknown keys are refused, not ignored.
        assert!(toml::from_str::<SiteConfig>("domian = \"example.org\"").is_err());
    }
}
