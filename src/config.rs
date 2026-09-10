//! The site's identity: its name, its domain, its language. The engine is
//! generic; these are site data, read from `Sajt.toml` in the site
//! directory and derived from the directory itself when the file says
//! nothing. Everything that wants a name gets it from here: page titles and
//! the header mark, the generated Caddyfile's site block (and whatever
//! container or service unit wraps it), the embed fetcher's contact URL.
//!
//! The file lives inside the site directory, visible, so it moves, syncs,
//! and backs up with the site and can be found and edited where the site is.
//! To the scanner it is an ordinary file: like anything else in the site it
//! is published only when it carries the public tag, and nothing about it is
//! secret. No hidden data: everything the engine keeps in a site is a
//! visible file with a `Sajt` prefix, under the same rules as the content.
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

    /// The other languages posts of this site exist in, as BCP 47 tags
    /// (`["sv", "de"]`). A dotted token before a file's extension is read as
    /// a language only when it names one of these (or the site language):
    /// `brev.sv.md` is then the Swedish version of `brev.md`, while
    /// `notes.old.md` stays the post `notes.old`, whatever the language
    /// registry says about `old`. A site that declares none never reads a
    /// name as a language. Defaults to none.
    pub languages: Option<Vec<String>>,
}

/// The resolved identity every page renders with: the config with its
/// defaults filled in from the site directory.
#[derive(Clone, Debug, PartialEq)]
pub struct Site {
    pub name: String,
    pub domain: Option<String>,
    pub language: String,
    /// The declared version languages, canonical, in declared order, without
    /// the site language (which is implied). See [`SiteConfig::languages`].
    pub languages: Vec<String>,
}

impl Default for Site {
    /// The identity of a site nobody configured or named: what any caller
    /// that never called [`init`] renders with.
    fn default() -> Self {
        Site {
            name: "Sajt".to_string(),
            domain: None,
            language: "en".to_string(),
            languages: Vec::new(),
        }
    }
}

impl Site {
    /// Whether `tag` is a language this site's posts may exist in: the site
    /// language or a declared one, compared whole and case-insensitively.
    pub fn speaks(&self, tag: &str) -> bool {
        crate::lang::same(tag, &self.language) || self.languages.iter().any(|l| crate::lang::same(l, tag))
    }

    /// The identity the test suite renders with: the unnamed default plus a
    /// few declared languages, so version fixtures (`brev.sv.md`,
    /// `brev.de.adoc`, `pt` next to `pt-BR`) pair the way a configured site
    /// pairs them. Production never sees this value.
    #[cfg(test)]
    fn for_tests() -> Self {
        Site {
            languages: ["sv", "de", "pt", "pt-BR"].iter().map(|s| s.to_string()).collect(),
            ..Site::default()
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

/// A language tag the registry knows (`lang::parse`): "en", "sv", "pt-BR".
/// Returns the canonical form, so every page's `lang` and every comparison
/// with a version's tag see one spelling.
fn canonical_language(tag: &str) -> Option<String> {
    crate::lang::parse(tag)
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
    let mut config = config;
    if let Some(language) = &config.language {
        match canonical_language(language) {
            Some(canonical) => config.language = Some(canonical),
            None => {
                return Err(format!(
                    "config {}: language {:?} is not a registered language tag like \"en\" or \"pt-BR\"",
                    path.display(),
                    language
                ));
            }
        }
    }
    if let Some(languages) = &config.languages {
        let mut canonical: Vec<String> = Vec::with_capacity(languages.len());
        for language in languages {
            let tag = canonical_language(language).ok_or_else(|| {
                format!(
                    "config {}: languages entry {:?} is not a registered language tag like \"sv\" or \"pt-BR\"",
                    path.display(),
                    language
                )
            })?;
            if canonical.iter().any(|seen| crate::lang::same(seen, &tag)) {
                return Err(format!(
                    "config {}: languages lists {:?} twice",
                    path.display(),
                    tag
                ));
            }
            canonical.push(tag);
        }
        config.languages = Some(canonical);
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
    let language = config.language.unwrap_or(defaults.language);
    // The site language is implied; listing it too is harmless, not an error.
    let languages = config
        .languages
        .unwrap_or_default()
        .into_iter()
        .filter(|l| !crate::lang::same(l, &language))
        .collect();
    Site {
        name: config
            .name
            .map(|n| n.trim().to_string())
            .or(dir_name)
            .unwrap_or(defaults.name),
        domain: config.domain,
        language,
        languages,
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
/// library callers that render without opening a site get "Sajt" (the test
/// suite gets the same with a few languages declared, [`Site::for_tests`]).
pub fn site() -> &'static Site {
    #[cfg(test)]
    {
        SITE.get_or_init(Site::for_tests)
    }
    #[cfg(not(test))]
    {
        SITE.get_or_init(Site::default)
    }
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
    fn languages_are_registered_tags_in_canonical_form() {
        for ok in ["en", "sv", "pt-BR", "zh-Hant-TW", "sr-Latn"] {
            assert_eq!(canonical_language(ok).as_deref(), Some(ok), "{ok}");
        }
        assert_eq!(canonical_language("PT-br").as_deref(), Some("pt-BR"));
        for bad in ["", "e", "english", "en_US", "en-", "-en", "en--US", "xx", "swe"] {
            assert!(canonical_language(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn parsing_fails_closed() {
        assert!(toml::from_str::<SiteConfig>("domain = \"example.org\"").is_ok());
        // Unknown keys are refused, not ignored.
        assert!(toml::from_str::<SiteConfig>("domian = \"example.org\"").is_err());
    }

    /// Load a config from text through the same validation a file gets.
    fn load_text(text: &str) -> Result<SiteConfig, String> {
        let dir = std::env::temp_dir().join(format!("sajt-config-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{:x}.toml", md5ish(text)));
        std::fs::write(&path, text).unwrap();
        let result = load(&path);
        let _ = std::fs::remove_file(&path);
        result
    }

    fn md5ish(s: &str) -> u64 {
        s.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
    }

    #[test]
    fn declared_languages_are_validated_canonical_and_unique() {
        let config = load_text("languages = [\"sv\", \"PT-br\"]").unwrap();
        assert_eq!(config.languages.unwrap(), vec!["sv", "pt-BR"]);
        assert!(load_text("languages = [\"sv\", \"old\"]").is_ok(), "old is a registered language");
        let err = load_text("languages = [\"swe\"]").err().expect("swe is not a tag");
        assert!(err.contains("\"swe\""), "{err}");
        let err = load_text("languages = [\"sv\", \"SV\"]").err().expect("a duplicate");
        assert!(err.contains("twice"), "{err}");
    }

    #[test]
    fn the_site_language_is_implied_in_the_declared_list() {
        let config = SiteConfig {
            language: Some("sv".to_string()),
            languages: Some(vec!["sv".to_string(), "de".to_string()]),
            ..SiteConfig::default()
        };
        let site = resolve(config, Path::new("/Users/anna/Sites/notes"));
        assert_eq!(site.languages, vec!["de"]);
        assert!(site.speaks("SV"));
        assert!(site.speaks("de"));
        assert!(!site.speaks("en"));
        assert!(Site::default().languages.is_empty(), "production declares nothing by default");
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
            languages: None,
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
