//! The site's identity: its name, its domain, its language. The engine is
//! generic; these are site data, read from `Sajt.toml` in the site
//! directory and derived from the directory itself when the file says
//! nothing. Everything that wants a name gets it from here: page titles and
//! the header mark, the generated Caddyfile's site block (and whatever
//! container or service unit wraps it), the embed fetcher's contact URL.
//!
//! The file lives inside the site directory, visible, so it moves, syncs,
//! and backs up with the site and can be found and edited where the site is.
//! It can never be published: its name is reserved, and the scanner skips
//! reserved names at the top level ([`is_reserved_name`]) the way it skips
//! dotfiles. No hidden data: everything the engine keeps in a site is a
//! visible file with a `Sajt` prefix.
//!
//! Serving is domain-agnostic. Every URL the engine emits is a path, so a
//! site works on any host name the moment it is served; the domain is only
//! consulted where something must name the public host (the Caddyfile
//! adapter's site address, the contact URL in the fetch user agent).

use serde::Deserialize;
use std::path::Path;
use std::sync::OnceLock;

/// The configuration file's name inside the site directory.
pub const FILE_NAME: &str = "Sajt.toml";

/// What the file may say. Every key is optional: a missing file and an empty
/// file mean the same thing, a site described entirely by its directory.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SiteConfig {
    /// The site's display name, shown in page titles and the header mark.
    /// Defaults to the site directory's name, so `~/Sites/example.org` is
    /// "example.org" without any configuration.
    pub name: Option<String>,

    /// The site's public host, a bare name like "example.org": no scheme, no
    /// path. Optional: serving never needs it, and adapters fall back to
    /// local addresses.
    pub domain: Option<String>,

    /// The language of the site's prose as a BCP 47 tag ("en", "sv",
    /// "pt-BR"), emitted as the `lang` attribute of every page so browsers,
    /// screen readers, and hyphenation treat the text correctly. Defaults to
    /// "en".
    pub language: Option<String>,
}

/// The resolved identity every page renders with: the config with its
/// defaults filled in from the site directory.
#[derive(Clone, Debug, PartialEq)]
pub struct Site {
    pub name: String,
    pub domain: Option<String>,
    pub language: String,
}

impl Default for Site {
    /// The identity of a site nobody configured or named: what tests and any
    /// caller that never called [`init`] render with.
    fn default() -> Self {
        Site {
            name: "Sajt".to_string(),
            domain: None,
            language: "en".to_string(),
        }
    }
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

/// A name is free text for humans (it is HTML-escaped wherever it is
/// rendered), but it must be a single line with something on it.
fn valid_name(name: &str) -> bool {
    let trimmed = name.trim();
    !trimmed.is_empty() && trimmed.len() <= 200 && !name.chars().any(char::is_control)
}

/// A language tag as browsers accept it: a 2-3 letter primary subtag, then
/// any number of 1-8 character alphanumeric subtags, hyphen-separated.
fn valid_language(tag: &str) -> bool {
    let mut parts = tag.split('-');
    let primary = parts.next().unwrap_or("");
    (2..=3).contains(&primary.len())
        && primary.chars().all(|c| c.is_ascii_alphabetic())
        && parts.all(|p| (1..=8).contains(&p.len()) && p.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// Top-level names the engine reserves for its own visible files: the
/// configuration and the grade ledger (with its iCloud conflict copies). The
/// scanner never treats these as posts, and nothing else may claim them.
pub fn is_reserved_name(name: &str) -> bool {
    name == FILE_NAME || crate::grade::is_ledger_name(name)
}

/// Where the configuration file lives for a site directory.
pub fn path_in(site_dir: &Path) -> std::path::PathBuf {
    site_dir.join(FILE_NAME)
}

/// Load the config at `path`. A missing file is the default config, a site
/// described by its directory, but a file that exists and cannot be parsed
/// is an error: a typo silently reverting the site to defaults would be a
/// trap. Unknown keys are errors for the same reason.
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
    if let Some(name) = &config.name {
        if !valid_name(name) {
            return Err(format!(
                "config {}: name {:?} must be one non-empty line",
                path.display(),
                name
            ));
        }
    }
    if let Some(language) = &config.language {
        if !valid_language(language) {
            return Err(format!(
                "config {}: language {:?} is not a language tag like \"en\" or \"pt-BR\"",
                path.display(),
                language
            ));
        }
    }
    Ok(config)
}

/// Fill the config's gaps from the site directory: an unnamed site is named
/// after its directory, an unlanguaged one is English.
pub fn resolve(config: SiteConfig, site_dir: &Path) -> Site {
    let defaults = Site::default();
    let dir_name = site_dir
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty());
    Site {
        name: config
            .name
            .map(|n| n.trim().to_string())
            .or(dir_name)
            .unwrap_or(defaults.name),
        domain: config.domain,
        language: config.language.unwrap_or(defaults.language),
    }
}

/// The identity this process renders with, set once at startup by whichever
/// subcommand opened the site (the same pattern as the transcode and JPEG
/// settings in `media`). Pages read it through [`site`].
static SITE: OnceLock<Site> = OnceLock::new();

/// Record the resolved identity. Call once at startup, before any page is
/// rendered; a second call is ignored, the first identity stands.
pub fn init(site: Site) {
    let _ = SITE.set(site);
}

/// The identity pages render with. Before [`init`], the unnamed default:
/// tests and library callers that render without opening a site get "Sajt".
pub fn site() -> &'static Site {
    SITE.get_or_init(Site::default)
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
    fn names_are_one_line() {
        assert!(valid_name("Example"));
        assert!(valid_name("Anna's notes"));
        assert!(!valid_name(""));
        assert!(!valid_name("   "));
        assert!(!valid_name("two\nlines"));
    }

    #[test]
    fn languages_are_tags() {
        for ok in ["en", "sv", "pt-BR", "zh-Hant-TW", "sr-Latn"] {
            assert!(valid_language(ok), "{ok}");
        }
        for bad in ["", "e", "english", "en_US", "en-", "-en", "en--US"] {
            assert!(!valid_language(bad), "{bad}");
        }
    }

    #[test]
    fn parsing_fails_closed() {
        assert!(toml::from_str::<SiteConfig>("domain = \"example.org\"").is_ok());
        // Unknown keys are refused, not ignored.
        assert!(toml::from_str::<SiteConfig>("domian = \"example.org\"").is_err());
    }

    #[test]
    fn unnamed_site_is_named_after_its_directory() {
        let site = resolve(SiteConfig::default(), Path::new("/Users/anna/Sites/example.org"));
        assert_eq!(site.name, "example.org");
        assert_eq!(site.language, "en");
        assert_eq!(site.domain, None);
    }

    #[test]
    fn config_wins_over_the_directory() {
        let config = SiteConfig {
            name: Some("  Anna's notes ".to_string()),
            domain: Some("example.org".to_string()),
            language: Some("sv".to_string()),
        };
        let site = resolve(config, Path::new("/Users/anna/Sites/notes"));
        assert_eq!(site.name, "Anna's notes");
        assert_eq!(site.domain.as_deref(), Some("example.org"));
        assert_eq!(site.language, "sv");
    }

    #[test]
    fn a_directory_without_a_name_falls_back_to_the_engine() {
        assert_eq!(resolve(SiteConfig::default(), Path::new("/")).name, "Sajt");
    }
}
