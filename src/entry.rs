use crate::postdate::PostDate;
use crate::tags::Tag;
use chrono::NaiveDateTime;
use std::path::PathBuf;

/// Tags the site treats as machinery, not topics — never shown in the cloud or
/// the row rail. `favorite` drives the ★; `public`/`private` drive visibility.
pub fn is_reserved_tag(name: &str) -> bool {
    matches!(name.to_ascii_lowercase().as_str(), "public" | "private" | "favorite" | "public-original")
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

/// One language version of a folder post (DESIGN.md "Languages"): a sibling
/// of the primary named `<stem>.<tag>.<ext>`, in a language other than the
/// site's. It is the same post in another language, addressed at
/// `/<post>/<tag>`, and it serves its own bytes under the same per-file rule
/// as any content: only with its own `public` tag.
#[derive(Debug, Clone)]
pub struct Version {
    /// The canonical language tag (`sv`, `pt-BR`), never the site language.
    pub lang: String,
    /// The version's content file.
    pub path: PathBuf,
    /// Its lowercased extension: a version may be another format than the
    /// primary (`brev.md` next to `brev.sv.adoc`).
    pub extension: String,
    /// The file's mtime, the "edited" date shown when it is meaningfully
    /// later than the post's publish date.
    pub mtime: NaiveDateTime,
    /// The version's own display title (its first H1), if it has one.
    pub display_label: Option<String>,
}

/// Why a post failed to scan. A malformed post still becomes an `Entry` (with
/// this set) so it renders as a loud, fail-closed error row rather than silently
/// vanishing or serving the wrong bytes. Each variant carries the conflicting
/// names (relative to the post) so the error page can name the exact fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostError {
    /// A folder post carried more than one date-marker subfolder. This stays a
    /// hard error — no degraded rendering can respect an unknown publish date.
    MultipleDateMarkers(Vec<String>),
    /// The folder post has no file that can serve as primary content (it is empty
    /// of regular files, with no `index/` listing marker either).
    NoPrimary,
}

/// One entry in a folder listing: a public file (`post-model.md` §6). Ordered by
/// filename. Built at scan time from the folder's public-tagged children;
/// membership is a `public` allowlist, never a blocklist, so nothing leaks by
/// default (`.DS_Store`, drafts and markers are excluded simply by being untagged).
#[derive(Debug, Clone)]
pub struct ListItem {
    /// The file name as shown, including its extension.
    pub name: String,
    /// The stem (name without the trailing extension) — the display label.
    pub stem: String,
    /// The lowercased extension (empty for a dotless file).
    pub ext: String,
    /// Absolute path to the file. Consumed in Commit 6c (content hash + nested
    /// serving); the row href is built folder-relative from `name` today.
    #[allow(dead_code)]
    pub path: PathBuf,
    pub mtime: NaiveDateTime,
    /// Size in bytes, shown discreetly in the row (0 for a subfolder).
    pub size: u64,
    /// Whether this is an image medium (drives gallery vs. file-list).
    pub is_image: bool,
    /// Whether this row is a public subfolder — a nested listing reachable at
    /// `<parent>/<name>/` (post-model.md §6). Renders as a folder row, never a
    /// gallery tile, so any subfolder forces the file-list style.
    pub is_dir: bool,
}

/// A folder that renders as a browsable index rather than a single document — the
/// `kind = folder` case (`post-model.md` §6). A folder becomes a listing when it
/// has no single primary (several primary candidates, or media with no document),
/// or when an empty `index/` marker forces it. Membership is the same fail-closed
/// `public` allowlist used everywhere else.
#[derive(Debug, Clone)]
pub struct Listing {
    /// The public files, in filename order.
    pub items: Vec<ListItem>,
    /// Total candidate files in the folder (public or not), for the "N files,
    /// M public" count — so a reader can tell that something is withheld.
    pub total: usize,
    /// An `index`/folder-name document rendered as intro prose above the grid
    /// (the `index/`-marker "a gallery with a story" case). `None` otherwise.
    pub intro: Option<PathBuf>,
    /// The intro document's extension, for rendering it.
    pub intro_ext: String,
    /// When several files claimed the primary slot, the server declines to guess
    /// and lists instead: their names, for a prominent collision notice. Empty
    /// unless that demotion happened (an `index/` marker silences it).
    pub collision: Vec<String>,
}

impl Listing {
    /// All public items are images → render as a gallery; mixed / any non-image →
    /// a file list. An empty listing is a (degenerate) file list.
    pub fn is_gallery(&self) -> bool {
        !self.items.is_empty() && self.items.iter().all(|i| i.is_image)
    }
}

/// Whether an extension names an image medium (drives `kind()` and gallery
/// detection). Compared after `normalize_ext`.
pub fn is_image_ext(ext: &str) -> bool {
    matches!(
        normalize_ext(ext).as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "avif" | "heic" | "heif" | "tiff" | "bmp"
    )
}

/// A post: a bare file or a folder in the content tree, resolved to the bytes it
/// serves plus the durable, user-authored metadata around it (publish date,
/// aliases, revisions). See `entry-model.md` for the model this represents.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The post's identity: the top-level content item it was scanned from —
    /// the folder of a folder post, the file of a bare-file post. Two `Entry`
    /// values are the same post exactly when their `id`s match; compare with
    /// [`Entry::same_post`]. The views a post is served through (a language
    /// version via [`Entry::show_version`], an archived revision) are clones
    /// whose `path` and `extension` point at other bytes but whose `id` is
    /// untouched, so name-claim and ownership checks still recognise them.
    /// Never compare entries by pointer: a clone is not at the scanned address.
    pub id: PathBuf,
    /// The primary content file to render and serve. For a bare-file post this is
    /// the file itself; for a folder post it is the resolved primary inside the
    /// folder. For an errored folder post it is the folder itself (unreadable as
    /// bytes — the error page renders instead).
    pub path: PathBuf,
    /// The post's directory when it is a folder post; `None` for a bare-file post.
    /// Assets (and the alias/date markers) resolve relative to this.
    pub dir: Option<PathBuf>,
    /// Publish date, precision-aware: the empty date-marker subfolder or a
    /// date-named post carries its own precision (year / month / day / minute /
    /// second, BCE possible); otherwise it is the primary file's mtime at second
    /// precision. Read as local wall-clock — that is how Finder/`touch` write it.
    pub timestamp: PostDate,
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
    /// Set when this folder post is a browsable listing rather than a single
    /// document (no primary, or an `index/` marker). Carries the public files to
    /// list and the collision notice. `None` for a normal document post or a
    /// bare-file post. See `post-model.md` §6.
    pub listing: Option<Listing>,
    /// A document post's `public` sibling files, in filename order — rendered as
    /// an attachment file list below the body (`post-model.md` §6). Empty for a
    /// bare file, a listing (its files are the listing), or a post with none.
    pub attachments: Vec<ListItem>,
    /// An outbound destination this post resolves to or cites (`post-model.md` §4),
    /// orthogonal to `kind`. Set by the scanner from a `.webloc`/`.url`/single-URL
    /// text file (the post *is* the link → `kind = "link"`) or a `link.*` sidecar
    /// (a content post that *cites* a destination → keeps its own medium). Always
    /// an `http(s)` URL that passed the scheme guard; `None` otherwise. The cite
    /// renders it with `rel="noreferrer"` and the scheme flag. See `outbound.rs`.
    pub link_url: Option<String>,
    /// The destination's own headline for the cite line, resolved from the embed
    /// cache by `resolve_embeds` (`post-model.md` §4). `None` until (or unless) that
    /// fetch succeeds — the cite then degrades to the bare domain. It never
    /// overrides the post's own `label`; a bare link with no label of its own
    /// promotes this into its heading. Not part of a post's identity.
    pub link_title: Option<String>,
    /// The language of the post's content when it differs from the site's:
    /// the canonical tag read from the primary file's name (`brev.sv.md`).
    /// `None` means the site language. Rendered as `lang` on the article.
    pub lang: Option<String>,
    /// The post's other-language versions (folder posts only), sorted by tag.
    /// Each lives at `/<post>/<tag>` and links to the others with `hreflang`.
    pub versions: Vec<Version>,
    /// Set on the value that renders one of `versions` (see [`Entry::show_version`]):
    /// the version's tag. The address then gains a `/<tag>` segment, `path`,
    /// `extension`, `lang` and `display_label` are the version's, and the rest
    /// of the post (date, tags, aliases, the other versions) is shared. `None`
    /// on a scanned post and on the site-language page.
    pub version: Option<String>,
}

impl Entry {
    /// Whether `other` is this post (possibly a version or revision view of it).
    pub fn same_post(&self, other: &Entry) -> bool {
        self.id == other.id
    }

    /// The version in a language, matched case-insensitively (`/brev/SV` is
    /// answered by the `sv` version, then redirected to its canonical case).
    pub fn version(&self, lang: &str) -> Option<&Version> {
        self.versions.iter().find(|v| crate::lang::same(&v.lang, lang))
    }

    /// The value that renders `version` of this post: the same post with the
    /// version's file, extension, language and title, marked so its address
    /// carries the tag. Revisions belong to the site-language file and are
    /// not shown on a version page.
    pub fn show_version(&self, version: &Version) -> Entry {
        let mut shown = self.clone();
        shown.path = version.path.clone();
        shown.extension = version.extension.clone();
        shown.lang = Some(version.lang.clone());
        shown.display_label = version.display_label.clone().or_else(|| self.display_label.clone());
        shown.version = Some(version.lang.clone());
        shown.revisions = Vec::new();
        shown.edited = self
            .timestamp
            .to_local_instant()
            .filter(|inst| version.mtime > *inst + chrono::Duration::minutes(1))
            .map(|_| version.mtime);
        shown
    }

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
    /// consumer is fail-closed for free. Reachability only: file *content*
    /// (including a folder post's primary) additionally requires that file's own
    /// `public` tag, enforced in the scanner and per request. See
    /// `post-model.md` §6.
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

    /// The entry's *medium* — what sort of thing it is and how it is served —
    /// derived from the file itself, never a tag. Search matches it, so typing
    /// "photo" finds photos with zero extra UI. Not the format (that's the
    /// extension) and not the genre (that's an author tag). See `post-model.md` §3.
    pub fn kind(&self) -> &'static str {
        // A listing folder is browsable, not a document — classify it before the
        // extension (its `path` is a directory, not a readable file).
        if self.listing.is_some() {
            return "folder";
        }
        // A post whose whole substance is an outbound destination is a `link`
        // (post-model.md §4): a bare file that IS a URL (`.webloc`/`.url`/single-
        // URL text — `dir` is `None`), or a folder whose primary is a link file.
        // A *content* post that merely cites a destination keeps its own medium,
        // so this only fires when the primary itself is the link.
        if self.link_url.is_some()
            && (self.dir.is_none()
                || matches!(normalize_ext(&self.extension).as_str(), "webloc" | "url"))
        {
            return "link";
        }
        match normalize_ext(&self.extension).as_str() {
            _ if is_image_ext(&self.extension) => "photo",
            // The HTML/XHTML family: complete self-contained documents served as
            // their own sandboxed standalone page (bypassing site chrome).
            _ if is_html_document(&self.extension) => "html",
            // Rendered (md/adoc) and plain (txt/rst/org/tex) text differ in
            // *rendering*, not medium — all poured into the site shell.
            _ if is_text_ext(&self.extension) => "text",
            // Dotless bare files (README, LICENSE): UTF-8-decodable → text
            // (rendered preformatted), else an opaque download. Only a folder is
            // ever `folder` (a listing — see §6).
            "" => {
                if self.is_utf8_text() {
                    "text"
                } else {
                    "file"
                }
            }
            _ => "file",
        }
    }

    /// Whether the primary file decodes as UTF-8 text (a cheap prefix sniff), used
    /// to classify dotless bare files. A trailing multi-byte character split at the
    /// prefix boundary is treated as text (fail toward readable).
    fn is_utf8_text(&self) -> bool {
        use std::io::Read;
        let mut buf = [0u8; 512];
        match std::fs::File::open(&self.path).and_then(|mut f| f.read(&mut buf)) {
            Ok(n) => match std::str::from_utf8(&buf[..n]) {
                Ok(_) => true,
                Err(e) => e.error_len().is_none() && e.valid_up_to() > 0,
            },
            Err(_) => false,
        }
    }
}

/// Text formats the engine renders: Markdown (comrak) and AsciiDoc
/// (Asciidoctor). Aliases fold through `normalize_ext` first.
pub fn is_rendered_text_ext(ext: &str) -> bool {
    matches!(normalize_ext(ext).as_str(), "md" | "adoc")
}

/// Text formats the engine does NOT render but still treats as posts: shown as
/// written (line breaks kept, in the body face, see `templates::Body::Plain`)
/// and served raw as `text/plain`, so a browser shows the file instead of
/// downloading it. `rst`/`org`/`tex` lost their renderer with Pandoc (v0.39.0);
/// any of them can grow an engine later without changing its medium.
pub fn is_plain_text_ext(ext: &str) -> bool {
    matches!(normalize_ext(ext).as_str(), "txt" | "rst" | "org" | "tex")
}

/// Every text format that is a post in the site shell (rendered or plain).
/// The medium classifier, the excerpt/title extractors and the render path all
/// key on this one predicate, so no text format can fall through to a download.
pub fn is_text_ext(ext: &str) -> bool {
    is_rendered_text_ext(ext) || is_plain_text_ext(ext)
}

/// Normalize extension aliases to their canonical form (case-insensitive):
/// `markdown`→`md`, `text`→`txt`, `asciidoc`→`adoc`; everything else lowercases
/// through unchanged. Both `kind()` and the render path key on this, so the two
/// never disagree about a `.markdown`, `.text`, or `.asciidoc` file (which used
/// to silently fall through to a download).
pub fn normalize_ext(ext: &str) -> String {
    match ext.to_ascii_lowercase().as_str() {
        "markdown" => "md".to_string(),
        "text" => "txt".to_string(),
        "asciidoc" => "adoc".to_string(),
        // Image-format aliases fold to their canonical extension so the metadata
        // strip gate (`is_image_ext`) and the strip dispatch agree — otherwise a
        // `.tif`/`.jpe`/`.jfif` slips past both and serves raw with full EXIF/GPS.
        "tif" => "tiff".to_string(),
        "jpe" | "jfif" | "jif" => "jpg".to_string(),
        other => other.to_string(),
    }
}

/// The HTML/XHTML document formats we render as sandboxed standalone pages (the
/// drop-in feature). An explicit allowlist, deliberately *narrower* than "every
/// extension `mime_guess` calls `text/html`": `.shtml`/`.shtm`/`.stm` (Server-Side
/// Includes) and the like are excluded, because we do not process SSI and serving
/// them as HTML would falsely imply we do. Those still serve safely — as plain
/// source text, never rendered un-jailed (see `routes::raw_content_type`).
///
/// This is the single source of truth used by three consumers that MUST agree:
/// `kind()` (medium classification), `render::render_entry` (the standalone
/// render path), and the server's sandbox gate (`routes.rs`) — if they diverged,
/// a document could be served jailed but misclassified, or classified as a page
/// but not jailed. `.xhtml`/`.xht` are included (a past `html`/`htm`-only check
/// let `.xhtml` run script in our origin — the S2 review finding).
pub fn is_html_document(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "html" | "htm" | "xhtml" | "xht"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_ext_folds_aliases_case_insensitively() {
        assert_eq!(normalize_ext("markdown"), "md");
        assert_eq!(normalize_ext("Markdown"), "md");
        assert_eq!(normalize_ext("text"), "txt");
        assert_eq!(normalize_ext("ASCIIDOC"), "adoc");
        assert_eq!(normalize_ext("JPG"), "jpg");
        assert_eq!(normalize_ext("md"), "md");
        assert_eq!(normalize_ext(""), "");
    }

    #[test]
    fn text_formats_split_into_rendered_and_plain() {
        for ext in ["md", "markdown", "adoc", "ASCIIDOC"] {
            assert!(is_rendered_text_ext(ext), "{ext} is rendered");
            assert!(!is_plain_text_ext(ext), "{ext} is not plain");
            assert!(is_text_ext(ext));
        }
        for ext in ["txt", "text", "rst", "org", "tex", "TEX"] {
            assert!(is_plain_text_ext(ext), "{ext} is plain text");
            assert!(!is_rendered_text_ext(ext), "{ext} has no renderer");
            assert!(is_text_ext(ext));
        }
        for ext in ["html", "pdf", "jpg", "zip", "", "latex", "rest"] {
            assert!(!is_text_ext(ext), "{ext} is not a text post format");
        }
    }

    #[test]
    fn html_document_family_is_html_and_xhtml_only() {
        for ext in ["html", "htm", "HTML", "xhtml", "xht", "XHTML"] {
            assert!(is_html_document(ext), "{ext} should be an HTML document");
        }
        // SSI (.shtml) and other text/html-mapped types are deliberately excluded
        // — we do not process them, so they must not render as HTML.
        for ext in ["shtml", "shtm", "stm", "hxt", "htt", "md", "txt", "svg", "xml", "png", ""] {
            assert!(!is_html_document(ext), "{ext} should NOT be an HTML document");
        }
    }
}
