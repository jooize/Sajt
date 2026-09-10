//! Language tags. One module decides what a language is for the whole
//! engine: the site's language and the declared ones in `Sajt.toml`, the
//! subtag in a filename (`brev.sv.md`), the suffix of a version address
//! (`/brev.sv`), and the name a language is shown under (its autonym:
//! "svenska", "Deutsch").
//!
//! A tag is a BCP 47 (RFC 5646) tag whose subtags appear in the IANA
//! Language Subtag Registry, in canonical form (`pt-BR`, `zh-Hant`). The
//! registry, not the grammar, is the authority: `english` and `en_US` are
//! not tags, `swe` is not a tag (the registry lists `sv`, the shortest ISO
//! 639 code), and a merely well-formed `xx` is not a tag. Everything the
//! registry does not list falls closed to "not a language".
//!
//! # The filename position
//!
//! In `notes.old.md` the token before the extension could be a language:
//! `old` is in the registry (Mochi), and so are `new`, `min`, `txt`, `tex`,
//! `doc`, `the`, `and`, `jan`, `usa` and most short English words, because
//! ISO 639-3 names every language on earth (54% of two-letter and 68% of
//! three-letter dictionary words are registered tags). Yet `notes.old` is
//! the name of that file. The registry therefore does not decide the
//! filename position; the site does. [`filename_token`] reads a token as a
//! language only when the site declares it (`Sajt.toml` `languages`, or
//! its own `language`), the convention of Hugo, Zola and Apache's
//! `AddLanguage`. A site that declares nothing never misreads a name; a
//! site with Swedish posts says so once. Every registered language remains
//! available, three-letter ones (`yue`, `fil`, `haw`, `gsw`) included:
//! declaring it is the only step. An undeclared token that happens to be a
//! registered language is reported to the caller, so the scanner can log a
//! forgotten declaration, and stays part of the name.

use language_tags::LanguageTag;

/// Codes the registry lists that name no language a text can be in:
/// undetermined, multiple, uncoded, and "no linguistic content".
const SPECIAL_CODES: [&str; 4] = ["und", "mul", "mis", "zxx"];

/// Parse and validate a language tag, returning its canonical form
/// (`pt-BR`, `zh-Hant`, `sr-Latn`), or `None` when the input is not a
/// registered language tag: malformed, unregistered, a private-use or
/// grandfathered tag, a tag with extensions, or a special code.
pub fn parse(input: &str) -> Option<String> {
    let tag = LanguageTag::parse(input).ok()?;
    tag.validate().ok()?;
    // Canonical form: deprecated subtags replaced, an extended language
    // promoted (`zh-yue` -> `yue`), a suppressed script dropped (`en-Latn`
    // -> `en`), and subtag case normalized. A tag with no single canonical
    // form is refused.
    let tag = tag.canonicalize().ok()?;
    tag.validate().ok()?;
    let text = tag.as_str();
    if text.starts_with("x-") || text.contains("-x-") || tag.extension().is_some() {
        return None;
    }
    let primary = tag.primary_language();
    // Private-use primary subtags (`qaa`..`qtz`) validate, but name nothing.
    if ("qaa"..="qtz").contains(&primary) {
        return None;
    }
    if SPECIAL_CODES.contains(&primary) {
        return None;
    }
    // Private-use regions (`QM`..`QZ`, `XA`..`XZ`) and scripts (`Qaaa`..
    // `Qabx`) validate the same way, and name nothing either.
    if let Some(region) = tag.region() {
        if ("QM"..="QZ").contains(&region) || ("XA"..="XZ").contains(&region) {
            return None;
        }
    }
    if let Some(script) = tag.script() {
        if ("Qaaa"..="Qabx").contains(&script) {
            return None;
        }
    }
    // A grandfathered tag with no preferred value survives canonicalization
    // with its odd shape (`i-default`); its "primary language" is the whole
    // tag. Refuse anything whose primary subtag is not plain letters.
    if !(2..=3).contains(&primary.len()) || !primary.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    Some(text.to_string())
}

/// The primary language subtag of a canonical tag (`pt` of `pt-BR`).
pub fn primary(tag: &str) -> &str {
    tag.split('-').next().unwrap_or(tag)
}

/// Whether two tags name the same language. Tags are compared whole and
/// case-insensitively: `en` and `en-GB` are different languages here, since
/// a site in one can carry a version in the other.
pub fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// What a token in the language position of a filename (`sv` in
/// `brev.sv.md`) turns out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilenameToken {
    /// A language the site declares (or its own): the canonical tag.
    Language(String),
    /// A registered language the site does not declare: it stays part of
    /// the name, and the caller may log it (the canonical tag, for the log).
    Undeclared(String),
    /// Not a language tag at all: part of the name.
    Plain,
}

/// Read a token in the language position of a filename against the site's
/// declared languages (see the module notes).
pub fn filename_token(token: &str) -> FilenameToken {
    filename_token_in(token, crate::config::site())
}

/// [`filename_token`] against an explicit site identity.
pub fn filename_token_in(token: &str, site: &crate::config::Site) -> FilenameToken {
    match parse(token) {
        Some(tag) if site.speaks(&tag) => FilenameToken::Language(tag),
        Some(tag) => FilenameToken::Undeclared(tag),
        None => FilenameToken::Plain,
    }
}

/// The language's name in its own language ("svenska", "Deutsch", "日本語"),
/// for links a reader picks a version by. Falls back to the English name,
/// then to the tag itself, so every language has a name. Subtags beyond the
/// language are appended as written (`português (BR)`), since the autonym
/// table is per language.
pub fn autonym(tag: &str) -> String {
    let primary = primary(tag);
    let language = if primary.len() == 2 {
        isolang::Language::from_639_1(primary)
    } else {
        isolang::Language::from_639_3(primary)
    };
    let name = language
        .and_then(|l| l.to_autonym().map(str::to_string))
        .or_else(|| language.map(|l| l.to_name().to_string()))
        .unwrap_or_else(|| primary.to_string());
    match tag.split_once('-') {
        Some((_, rest)) => format!("{} ({})", name, rest),
        None => name,
    }
}

/// The language's English name, for logs ("sv is Swedish") and for the
/// fallback when no autonym is known. The tag itself when unknown.
pub fn english_name(tag: &str) -> String {
    let primary = primary(tag);
    let language = if primary.len() == 2 {
        isolang::Language::from_639_1(primary)
    } else {
        isolang::Language::from_639_3(primary)
    };
    language.map(|l| l.to_name().to_string()).unwrap_or_else(|| tag.to_string())
}

/// Uppercase the first letter, for a name that opens a sentence ("Svenska.
/// Also in English"). Autonyms are cased as their language writes them
/// mid-sentence ("svenska"), so this is applied only at the sink.
pub fn sentence_case(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_tags_parse_to_canonical_form() {
        assert_eq!(parse("sv").as_deref(), Some("sv"));
        assert_eq!(parse("SV").as_deref(), Some("sv"));
        assert_eq!(parse("pt-br").as_deref(), Some("pt-BR"));
        assert_eq!(parse("zh-hant-tw").as_deref(), Some("zh-Hant-TW"));
        assert_eq!(parse("sr-latn").as_deref(), Some("sr-Latn"));
        // Suppress-Script: the default script is dropped.
        assert_eq!(parse("en-Latn").as_deref(), Some("en"));
        // An extended language promotes to its own primary subtag.
        assert_eq!(parse("zh-yue").as_deref(), Some("yue"));
        // Three-letter codes for languages without a two-letter one.
        for ok in ["haw", "yue", "fil", "gsw", "ast"] {
            assert!(parse(ok).is_some(), "{ok}");
        }
    }

    #[test]
    fn unregistered_and_special_tags_are_refused() {
        for bad in [
            "", "e", "english", "en_US", "en-", "-en", "en--US", "xx", "swe", "eng", "en-XX",
            "en-Qaaa", "en-XA", "en-QM", "x-private", "en-x-private", "en-u-co-phonebk", "i-default", "qaa",
            "und", "mul", "mis", "zxx", "en US", "en.md",
        ] {
            assert!(parse(bad).is_none(), "{bad:?} must not parse");
        }
    }

    #[test]
    fn filename_tokens_follow_the_declared_languages() {
        use crate::config::Site;
        let site = |languages: &[&str]| Site {
            languages: languages.iter().map(|s| s.to_string()).collect(),
            ..Site::default()
        };
        let none = site(&[]);
        let some = site(&["sv", "pt-BR", "haw"]);

        // Only declared languages (and the site's own) are read as languages.
        assert_eq!(filename_token_in("sv", &some), FilenameToken::Language("sv".into()));
        assert_eq!(filename_token_in("SV", &some), FilenameToken::Language("sv".into()));
        assert_eq!(filename_token_in("PT-br", &some), FilenameToken::Language("pt-BR".into()));
        assert_eq!(filename_token_in("haw", &some), FilenameToken::Language("haw".into()));
        assert_eq!(filename_token_in("en", &none), FilenameToken::Language("en".into()), "the site language is implied");
        assert_eq!(filename_token_in("pt", &some), FilenameToken::Undeclared("pt".into()), "pt-BR does not declare pt");

        // Registered but undeclared: reported, never a language.
        for word in ["sv", "old", "new", "min", "txt", "the", "in", "is", "no"] {
            assert!(parse(word).is_some(), "{word} is in the registry (the premise)");
            assert_eq!(filename_token_in(word, &none), FilenameToken::Undeclared(parse(word).unwrap()), "{word}");
        }

        // Not tags at all.
        for plain in ["md", "rst", "copy", "swe", "v2", "final", ""] {
            assert_eq!(filename_token_in(plain, &some), FilenameToken::Plain, "{plain:?}");
        }
    }

    #[test]
    fn autonyms_fall_back_to_english_then_the_tag() {
        assert_eq!(autonym("sv"), "svenska");
        assert_eq!(autonym("de"), "Deutsch");
        assert_eq!(autonym("pt-BR"), "português (BR)");
        assert_eq!(autonym("en"), "English");
        // A code the autonym table lacks still has a name.
        assert!(!autonym("haw").is_empty());
        assert_eq!(english_name("sv"), "Swedish");
        assert_eq!(sentence_case("svenska"), "Svenska");
        assert_eq!(sentence_case(""), "");
    }

    #[test]
    fn same_is_whole_tag_case_insensitive() {
        assert!(same("sv", "SV"));
        assert!(!same("en", "en-GB"));
    }
}
