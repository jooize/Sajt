//! Language tags. One module decides what a language is for the whole
//! engine: the site's language in `Sajt.toml`, the subtag in a filename
//! (`brev.sv.md`), the segment of a version address (`/brev/sv`), and the
//! name a language is shown under (its autonym: "svenska", "Deutsch").
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
//! In `notes.txt.md` the token before the extension could be a language:
//! `txt` is in the registry (so are `tex`, `org`, `doc`, `min`, `src`,
//! `tar`, `log`, `old`, `new`, and hundreds of other three-letter English
//! abbreviations, because ISO 639-3 names every language on earth). Yet
//! `notes.txt` is the name of that file. [`filename_subtag`] settles the
//! collision by the namespace the token sits in:
//!
//! - a two-letter primary subtag is a language (`es`, `pl`, `cs` are
//!   Spanish, Polish, Czech, even though `.es`, `.pl`, `.cs` are also file
//!   extensions: BCP 47's two-letter codes are unambiguous);
//! - a three-letter primary subtag is a language only when no file format
//!   claims it (the MIME extension table plus the engine's own formats),
//!   so `txt`, `tex`, `doc`, `tar` stay part of the stem while `haw`,
//!   `yue`, `fil`, `gsw` are languages.
//!
//! The residual is named, not hidden: `old`, `new`, `min`, `src`, `tmp`,
//! `dev`, `app` are registered languages and not file formats, so
//! `notes.old.md` is a post named `notes` in Mochi. The scanner logs every
//! recognition with the language's name, and the fix is one rename.

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

/// Whether a token is a file extension some format claims: the MIME
/// extension table the raw-file routes use, or one of the engine's own
/// formats (which the table does not all know: `adoc`, `webloc`).
fn is_file_extension(token: &str) -> bool {
    mime_guess::from_ext(token).first().is_some()
        || crate::entry::is_text_ext(token)
        || crate::entry::is_image_ext(token)
        || crate::entry::is_html_document(token)
        || matches!(token.to_ascii_lowercase().as_str(), "webloc" | "url")
}

/// Read a token in the language position of a filename (`sv` in
/// `brev.sv.md`) as a language, or `None` when it is not one and stays part
/// of the stem. The registry decides, then the file-format tie-break for
/// three-letter codes (see the module notes).
pub fn filename_subtag(token: &str) -> Option<String> {
    let tag = parse(token)?;
    let primary = primary(&tag);
    if primary.len() == 3 && is_file_extension(primary) {
        return None;
    }
    Some(tag)
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
    fn filename_subtag_keeps_file_formats_in_the_stem() {
        // Registered languages that are also file formats stay in the stem.
        for ext in ["txt", "tex", "org", "doc", "tar", "log", "xml", "csv", "bin", "mov", "wav", "ogg"] {
            assert!(parse(ext).is_some(), "{ext} is in the registry (the premise of the tie-break)");
            assert!(filename_subtag(ext).is_none(), "{ext} is a file format");
        }
        // Two-letter codes are languages even where a format shares them.
        for lang in ["es", "pl", "cs", "so", "ps", "it"] {
            assert_eq!(filename_subtag(lang).as_deref(), Some(lang));
        }
        // Three-letter languages no format claims.
        for lang in ["haw", "yue", "fil", "gsw"] {
            assert_eq!(filename_subtag(lang).as_deref(), Some(lang));
        }
        assert_eq!(filename_subtag("PT-br").as_deref(), Some("pt-BR"));
        assert!(filename_subtag("rst").is_none());
        assert!(filename_subtag("md").is_none());
        assert!(filename_subtag("copy").is_none());
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
