//! Slug derivation — the address projection of a post's name.
//!
//! A post's identity is its filename/folder name; the slug is a derived,
//! lowercase, URL-safe projection of it (`Fog Over The Bay` -> `fog-over-the-bay`).
//! Collisions key on the slug, so two names that reduce to the same slug share
//! one address (see `post-model.md` §1). The client `slugify` in
//! `templates.rs` mirrors this recipe so in-page heading anchors agree with it.
//!
//! Recipe (exact — collisions depend on it):
//!   1. Unicode-normalize **NFKD** (so `é` -> `e` + combining accent, `ﬁ` -> `fi`,
//!      circled/compat digits -> plain, Hangul syllables -> conjoining jamo).
//!   2. **Drop combining marks** — this folds Latin accents (`é` -> `e`) while
//!      the conjoining jamo (letters, not marks) survive.
//!   3. Re-normalize **NFC** (so decomposed Hangul recomposes to its syllable,
//!      staying one composed character rather than a run of jamo).
//!   4. Unicode-lowercase, keeping letters/digits of any script (a Cyrillic or
//!      Japanese title keeps its script).
//!   5. Apostrophes/quotes **join** (`it's` -> `its`); every other non-alphanumeric
//!      run becomes a single hyphen; leading/trailing hyphens are trimmed.
//!   6. An empty result (a title like `!!!`) yields `None` — the post is then
//!      unlabeled and addressed at its date path, never claiming a bare URL.

use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

/// Whether a character is an apostrophe or quote that should *join* the letters
/// around it rather than split them (`it's` -> `its`, not `it-s`).
fn is_joining_quote(c: char) -> bool {
    matches!(
        c,
        '\'' | '\u{2018}' | '\u{2019}' | '\u{201B}' // ' ' ' ‛  single quotes
            | '\u{02BC}'                              // ʼ modifier letter apostrophe
            | '\u{0060}' | '\u{00B4}'                 // ` ´ grave / acute standalone
            | '"' | '\u{201C}' | '\u{201D}' | '\u{201F}' // " " " ‟  double quotes
    )
}

/// Derive the URL slug for a name stem. Returns `None` for a name with no
/// letters or digits (an all-punctuation title), which addresses at its date.
pub fn slug(stem: &str) -> Option<String> {
    // Steps 1-3: NFKD, strip combining marks, NFC. Collecting between the two
    // normalization passes is necessary — the filter sits between them.
    let stripped: String = stem.nfkd().filter(|c| !is_combining_mark(*c)).collect();

    let mut out = String::with_capacity(stripped.len());
    let mut pending_hyphen = false; // a separator is owed once real content follows
    for c in stripped.nfc() {
        if is_joining_quote(c) {
            continue; // join across apostrophes/quotes, emit nothing
        }
        // Unicode-lowercase (may expand to several chars, e.g. ß -> ss).
        for lc in c.to_lowercase() {
            if lc.is_alphanumeric() {
                if pending_hyphen && !out.is_empty() {
                    out.push('-');
                }
                pending_hyphen = false;
                out.push(lc);
            } else {
                // Any other character is a separator; collapse runs to one hyphen,
                // deferred so a trailing run never leaves a dangling hyphen.
                pending_hyphen = true;
            }
        }
    }

    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Whether a slug is reserved by the URL grammar and so never claims the bare
/// `/slug` address: `saved` (a route) or any purely-numeric slug (year/date
/// filtering — `/2026` must stay the year view). A post with a reserved slug is
/// still reachable at its date path and carries the reserved-name notice.
pub fn is_reserved_slug(s: &str) -> bool {
    s == "saved" || (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin_accents_fold() {
        assert_eq!(slug("café").as_deref(), Some("cafe"));
        assert_eq!(slug("résumé").as_deref(), Some("resume"));
        assert_eq!(slug("Über den Wolken").as_deref(), Some("uber-den-wolken"));
    }

    #[test]
    fn compatibility_forms_fold() {
        // ﬁ ligature -> fi, circled digit -> 2
        assert_eq!(slug("ﬁle").as_deref(), Some("file"));
        assert_eq!(slug("item ②").as_deref(), Some("item-2"));
    }

    #[test]
    fn non_latin_scripts_are_kept() {
        // Hangul stays composed (NFKD decomposes to jamo, NFC recomposes).
        assert_eq!(slug("한국어").as_deref(), Some("한국어"));
        // Cyrillic + Japanese survive, lowercased where applicable.
        assert_eq!(slug("Привет").as_deref(), Some("привет"));
        assert_eq!(slug("日本語").as_deref(), Some("日本語"));
    }

    #[test]
    fn apostrophes_join() {
        assert_eq!(slug("it's").as_deref(), Some("its"));
        assert_eq!(slug("don\u{2019}t stop").as_deref(), Some("dont-stop"));
        assert_eq!(slug("rock 'n' roll").as_deref(), Some("rock-n-roll"));
    }

    #[test]
    fn spaces_and_case_and_punctuation() {
        assert_eq!(slug("Fog Over The Bay").as_deref(), Some("fog-over-the-bay"));
        // Stop-words are kept — no stripping.
        assert_eq!(slug("A Tale").as_deref(), Some("a-tale"));
        assert_eq!(slug("hello, world!").as_deref(), Some("hello-world"));
        assert_eq!(slug("  trim  me  ").as_deref(), Some("trim-me"));
        assert_eq!(slug("multi---hyphen").as_deref(), Some("multi-hyphen"));
    }

    #[test]
    fn empty_and_punctuation_only_is_none() {
        assert_eq!(slug("!!!"), None);
        assert_eq!(slug(""), None);
        assert_eq!(slug("   "), None);
        assert_eq!(slug("---"), None);
    }

    #[test]
    fn idempotent_on_a_slug() {
        let s = slug("Fog Over The Bay").unwrap();
        assert_eq!(slug(&s).as_deref(), Some(s.as_str()));
    }

    #[test]
    fn reserved_slugs() {
        assert!(is_reserved_slug("saved"));
        assert!(is_reserved_slug("2026"));
        assert!(is_reserved_slug("100"));
        assert!(!is_reserved_slug("fog-over-the-bay"));
        assert!(!is_reserved_slug("2026-review"));
        assert!(!is_reserved_slug(""));
    }
}
