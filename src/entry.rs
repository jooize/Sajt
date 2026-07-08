use crate::tags::Tag;
use chrono::NaiveDateTime;
use std::path::PathBuf;

/// Tags the site treats as machinery, not topics — never shown in the cloud or
/// the row rail. `favorite` drives the ★; `public`/`private` drive visibility.
pub fn is_reserved_tag(name: &str) -> bool {
    matches!(name.to_ascii_lowercase().as_str(), "public" | "private" | "favorite")
}

/// The Finder duplicate keyword. Finder writes it only in English ("copy"); the
/// revision grammar (`<name> copy`, `<name> copy N`) is built from this one
/// constant so a localized Finder can be supported later by changing it here.
pub const COPY_KEYWORD: &str = "copy";

/// An archived revision of a post: a frozen earlier state, made by Cmd-D before
/// editing. Either a sibling ` copy [n]` folder (a full-post snapshot) or a
/// `<stem> copy [n]` file inside a folder post (a file-level snapshot). A
/// revision is dated by its own primary/file mtime — Finder's Cmd-D preserves
/// that — and never by an inherited date marker.
#[derive(Debug, Clone)]
pub struct Revision {
    /// Revision date = the copy's primary-file mtime (read as local wall-clock).
    pub date: NaiveDateTime,
    /// The revision's primary content file, for serving at its date-path URL.
    // Consumed when routes serve revisions at their date paths (step 2).
    #[allow(dead_code)]
    pub path: PathBuf,
    /// The copy's rank within the family (1 = ` copy`, 2 = ` copy 2`, …). Newer
    /// dates sort first; rank breaks exact date ties (a later copy ranks higher).
    pub rank: u32,
}

/// Why a post failed to scan. A malformed post still becomes an `Entry` (with
/// this set) so it renders as a loud, fail-closed error row rather than silently
/// vanishing or serving the wrong bytes. Each variant carries the conflicting
/// names (relative to the post) so the error page can name the exact fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostError {
    /// A folder post carried more than one date-marker subfolder.
    MultipleDateMarkers(Vec<String>),
    /// More than one file could be the primary content, or none could be chosen
    /// from several (stem `index` or matching the folder name resolves it).
    AmbiguousPrimary(Vec<String>),
    /// The folder post has no file that can serve as primary content (it is empty
    /// of regular files).
    NoPrimary,
}

/// A post: a bare file or a folder in the content tree, resolved to the bytes it
/// serves plus the durable, user-authored metadata around it (publish date,
/// aliases, revisions). See `entry-model.md` for the model this represents.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The primary content file to render and serve. For a bare-file post this is
    /// the file itself; for a folder post it is the resolved primary inside the
    /// folder. For an errored folder post it is the folder itself (unreadable as
    /// bytes — the error page renders instead).
    pub path: PathBuf,
    /// The post's directory when it is a folder post; `None` for a bare-file post.
    /// Assets (and the alias/date markers) resolve relative to this.
    pub dir: Option<PathBuf>,
    /// Publish date: the empty date-marker subfolder for folder posts, otherwise
    /// the primary file's mtime (bare files, or folder posts with no marker).
    /// Read as local wall-clock — that is how Finder/`touch` write times.
    pub timestamp: NaiveDateTime,
    /// Edited date = the primary file's mtime, recorded only when it is
    /// meaningfully later than `timestamp` (folder posts with a date marker). A
    /// bare file's mtime *is* its publish date, so this stays `None` for them.
    // Rendered as the entry's "edited" line in step 2.
    #[allow(dead_code)]
    pub edited: Option<NaiveDateTime>,
    pub label: Option<String>,
    /// The URL slug: a lowercase, hyphenated, collision-keying projection of the
    /// name (`Fog Over The Bay` -> `fog-over-the-bay`). `None` when the name has
    /// no letters or digits (a punctuation-only title) — such a post is unlabeled
    /// and addressed at its date path. Identity is still the name (`label`); the
    /// slug is the *address* and is what claim comparison keys on. See `slug.rs`.
    pub slug: Option<String>,
    /// Auto-generated display label (e.g. "@handle · date" for social embeds).
    /// Templates use display_label.as_ref().or(label.as_ref()) for display.
    pub display_label: Option<String>,
    /// One-line row description. A user-authored macOS Finder comment when the
    /// post has one (any post kind — photos, links, pages too); otherwise the
    /// first prose paragraph of a text post, stripped to plain text and capped.
    /// `None` when the post has neither. Also folded into search.
    pub excerpt: Option<String>,
    pub extension: String,
    pub tags: Vec<Tag>,
    /// Pairwise-grade percentile in `0.0..=1.0`, or `None` until the entry has
    /// been graded. No grading flow exists yet, so this is always `None` today;
    /// the quality meter and grade filter read it when it exists.
    pub grade: Option<f32>,
    /// Extra addresses declared by `alias <name>/` marker folders. Each is an
    /// additional address for this post (served by a 301 to the canonical one).
    // Consumed when routes resolve alias URLs and the entry page lists them (step 2).
    #[allow(dead_code)]
    pub aliases: Vec<String>,
    /// Archived revisions, newest first. Kept off the timeline proper (only the
    /// current post shows); reachable through the revision nav and date-path URLs.
    pub revisions: Vec<Revision>,
    /// Set when the post scanned wrong. It still renders — as a fail-closed error
    /// page/row — so the conflict is surfaced, never hidden.
    pub error: Option<PostError>,
}

impl Entry {
    /// Whether the author marked this entry a favorite (the `favorite` tag → ★).
    pub fn is_favorite(&self) -> bool {
        self.tags.iter().any(|t| t.name.eq_ignore_ascii_case("favorite"))
    }

    /// Whether the post carries the `public` tag at its own (post) level.
    pub fn is_public(&self) -> bool {
        self.tags.iter().any(Tag::is_public)
    }

    /// Whether the post carries the `private` tag (deny-wins over `public`).
    pub fn is_private(&self) -> bool {
        self.tags.iter().any(Tag::is_private)
    }

    /// The fail-closed post-level visibility gate: a post is served only when it
    /// is tagged `public` and not `private`. Untagged is not served. This decides
    /// whether the post exists in the served set at all (timeline, name
    /// resolution, listings) — enforced once in the scanner, so every downstream
    /// consumer is fail-closed for free. See `post-model.md` §6.
    pub fn is_visible(&self) -> bool {
        self.is_public() && !self.is_private()
    }

    /// Tags shown to readers: everything that isn't machinery, in file order.
    pub fn topical_tags(&self) -> impl Iterator<Item = &Tag> {
        self.tags.iter().filter(|t| !is_reserved_tag(&t.name))
    }

    /// Tag names (all of them) for URL/query matching, which is name-based.
    pub fn tag_names(&self) -> Vec<String> {
        self.tags.iter().map(|t| t.name.clone()).collect()
    }

    /// The entry's *type*, derived from the file itself — never a tag. Search
    /// matches it, so typing "photo" finds photos with zero extra UI.
    pub fn kind(&self) -> &'static str {
        match self.extension.to_ascii_lowercase().as_str() {
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "avif" | "heic" | "heif" | "tiff" | "bmp" => "photo",
            "html" | "htm" => "page",
            "link" => "link",
            "" | "/" => "folder",
            "md" | "markdown" | "txt" | "text" | "adoc" | "asciidoc" | "rst" | "org" | "tex" => "note",
            _ => "file",
        }
    }
}
