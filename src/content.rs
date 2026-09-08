use crate::embed::EmbedData;
use crate::entry::{is_image_ext, Entry, ListItem, Listing, PostError, Revision, COPY_KEYWORD};
use crate::postdate::PostDate;
use crate::slug::{is_reserved_slug, slug};
use crate::tags::{read_finder_comment, read_tags_colored, Tag};
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct ContentStore {
    pub entries: Vec<Entry>,
    pub content_dir: PathBuf,
    /// Root for disposable derived caches (embeds, etc.), always OUTSIDE the
    /// content tree so the server never writes into content. See `entry-model.md`.
    pub cache_dir: PathBuf,
    pub embed_cache: HashMap<PathBuf, EmbedData>,
    /// The earliest future-dated (scheduled) post's moment, as a local instant.
    /// The server wakes at this time to rescan so a scheduled post appears
    /// exactly when due, not on the next unrelated change. `None` when nothing
    /// is scheduled. See `post-model.md` §2 (future-hold).
    pub next_future: Option<NaiveDateTime>,
}

impl ContentStore {
    /// Scan the content directory into posts, newest first (by publish date).
    ///
    /// A post is a bare file (publish date = mtime) or a folder (publish date =
    /// its empty date-marker subfolder, else the primary file's mtime). The scan
    /// never writes; a malformed post becomes an errored `Entry` rather than
    /// aborting the whole scan or serving the wrong bytes. See `entry-model.md`.
    pub fn scan(content_dir: &Path, cache_dir: &Path) -> std::io::Result<Self> {
        let mut entries = scan_entries(content_dir)?;

        // Fail-closed visibility gate (post-model.md §6): a post is served only
        // when its top level (the folder, or the bare file) is tagged `public`
        // and not `private`. Dropping hidden posts here — before the timeline,
        // name resolution, listings, and embeds ever see them — makes every
        // downstream path fail-closed at once. Untagged content simply does not
        // exist to the server.
        let scanned = entries.len();
        entries.retain(|e| e.is_visible());
        let hidden = scanned - entries.len();
        if hidden > 0 {
            tracing::info!(
                "Visibility gate: {} post(s) withheld (untagged or private), {} served",
                hidden,
                entries.len()
            );
        }

        // Future-hold (post-model.md §2): a post dated in the future is held out
        // of the served set until its moment. Fail-safe — a future misdrop stays
        // hidden. Record the earliest future moment so the server can wake and
        // rescan exactly then, rather than waiting for an unrelated change.
        let now = PostDate::now();
        let mut future_wakes: Vec<NaiveDateTime> = Vec::new();
        entries.retain(|e| {
            if e.timestamp.is_future(&now) {
                if let Some(inst) = e.timestamp.to_local_instant() {
                    future_wakes.push(inst);
                }
                false
            } else {
                true
            }
        });
        if !future_wakes.is_empty() {
            tracing::info!(
                "Future-hold: {} post(s) scheduled, hidden until their date",
                future_wakes.len()
            );
        }
        let next_future = future_wakes.into_iter().min();

        tracing::info!("Scanned {} entries from {}", entries.len(), content_dir.display());

        Ok(ContentStore {
            entries,
            content_dir: content_dir.to_path_buf(),
            cache_dir: cache_dir.to_path_buf(),
            embed_cache: HashMap::new(),
            next_future,
        })
    }

    /// Re-scan the content directory, replacing all entries.
    pub fn rescan(&mut self) -> std::io::Result<()> {
        let new = Self::scan(&self.content_dir, &self.cache_dir)?;
        self.entries = new.entries;
        self.next_future = new.next_future;
        self.embed_cache.clear();
        Ok(())
    }

    /// How long until the next scheduled (future-dated) post is due, for the
    /// future-hold wake timer. `None` when nothing is scheduled. A one-second
    /// cushion ensures the moment has passed by the time the rescan runs.
    pub fn next_future_delay(&self) -> Option<std::time::Duration> {
        let inst = self.next_future?;
        let secs = (inst - Local::now().naive_local()).num_seconds().max(0) as u64;
        Some(std::time::Duration::from_secs(secs + 1))
    }

    /// Resolve embeds for entries containing recognized URLs.
    pub async fn resolve_embeds(&mut self) {
        let cache =
            crate::embed::resolve_embeds(&mut self.entries, &self.content_dir, &self.cache_dir)
                .await;
        self.embed_cache = cache;
        let count = self.embed_cache.len();
        if count > 0 {
            tracing::info!("Resolved {} link embeds", count);
        }
    }
}

// ─── Scanner ─────────────────────────────────────────────────────

/// A raw top-level item in the content directory, before it is resolved into a
/// post or attached as a revision.
struct TopItem {
    name: String,
    path: PathBuf,
    is_dir: bool,
}

fn scan_entries(content_dir: &Path) -> std::io::Result<Vec<Entry>> {
    let read_dir = match std::fs::read_dir(content_dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::warn!("Content directory does not exist: {}", content_dir.display());
            return Ok(Vec::new());
        }
        Err(e) => return Err(e),
    };

    // Split top-level items into current-post candidates and ` copy [n]`
    // revisions of some base. Dotfiles and derived embed caches are ignored.
    let mut posts: Vec<TopItem> = Vec::new();
    let mut revisions: Vec<TopItem> = Vec::new();
    for dir_entry in read_dir {
        let dir_entry = dir_entry?;
        let path = dir_entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => {
                tracing::warn!("Skipping content item with non-UTF-8 name: {}", path.display());
                continue;
            }
        };
        if name.starts_with('.') || crate::config::is_reserved_name(&name) {
            // dotfiles (.DS_Store, .claude) and the engine's own visible files
            // (Sajt.toml, the grade ledger) are never posts
            continue;
        }
        if is_cache_name(&name) {
            continue; // legacy in-tree embed caches (the server now caches outside content)
        }
        let is_dir = dir_entry.file_type()?.is_dir();

        // A ` copy [n]` name (folder name, or file stem) marks a revision of its
        // family base rather than a post of its own.
        let stem_for_copy = if is_dir { name.clone() } else { split_name(&name).0.to_string() };
        if parse_revision_suffix(&stem_for_copy).is_some() {
            revisions.push(TopItem { name, path, is_dir });
        } else {
            posts.push(TopItem { name, path, is_dir });
        }
    }

    let mut entries: Vec<Entry> = posts.iter().map(build_post).collect();
    attach_revisions(&mut entries, &revisions);

    // Newest first; the sort is stable so equal publish dates keep scan order.
    entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    assign_grades(&mut entries, content_dir);
    Ok(entries)
}

/// Derive each post's `grade` from the pairwise-judgement ledger and assign it.
///
/// Read-only: the ledger lives in the content root but is only read here; the
/// grades are held in memory, recomputed on every scan. Names in the ledger
/// (post labels or their `alias <name>/` addresses) resolve to canonical labels
/// so a rename never orphans past judgements. An absent or empty ledger leaves
/// every `grade` as `None` — the site is then simply "everything" + favorites.
fn assign_grades(entries: &mut [Entry], content_dir: &Path) {
    let judgements = crate::grade::load_judgements(content_dir);
    if judgements.is_empty() {
        return; // no ledger -> nothing to assign, every post stays ungraded
    }

    // Map every canonical label to itself first, so a real post always wins over
    // an alias of the same name; then fold in aliases that don't collide.
    let mut resolver: HashMap<String, String> = HashMap::new();
    for e in entries.iter() {
        if let Some(label) = &e.label {
            resolver.insert(label.clone(), label.clone());
        }
    }
    for e in entries.iter() {
        if let Some(label) = &e.label {
            for alias in &e.aliases {
                resolver.entry(alias.clone()).or_insert_with(|| label.clone());
            }
        }
    }

    let grades = crate::grade::derive_grades(&judgements, |name| resolver.get(name).cloned());
    let graded = grades.len();
    for e in entries.iter_mut() {
        if let Some(label) = &e.label {
            e.grade = grades.get(label).copied();
        }
    }
    if graded > 0 {
        tracing::info!("Derived grades for {} post(s) from the judgement ledger", graded);
    }
}

/// Build a post from a top-level item: a bare file or a folder.
fn build_post(item: &TopItem) -> Entry {
    build_post_as(item, None)
}

/// Build a post, optionally *claiming* a base name. `claim` is `Some(base)` only
/// when this item is a promoted orphan copy standing in for a missing base (`foo
/// copy` with no `foo`, see `attach_revisions`): it then takes the base's identity
/// (label + slug) and — for a folder — resolves its primary against the base name,
/// since Finder's Cmd-D renames only the folder and leaves the inner files named
/// after the base. A normal post passes `None` and is simply its own name.
fn build_post_as(item: &TopItem, claim: Option<&str>) -> Entry {
    if item.is_dir {
        build_folder_post(item, claim)
    } else {
        build_bare_post(item, claim)
    }
}

/// A bare-file post: the file itself is the content, its mtime is the publish
/// date, its stem is the label. Editing it moves the mtime (republishes) — by
/// design; fold it into a folder to gain a stable date, assets, or aliases.
fn build_bare_post(item: &TopItem, claim: Option<&str>) -> Entry {
    let (stem, ext) = split_name(&item.name);
    // Identity (label / slug / date-name) comes from the claimed base when this
    // file is a promoted orphan copy; otherwise from its own stem. The bytes,
    // mtime, tags, excerpt and title are always read from the file itself.
    let ident = claim.unwrap_or(stem);
    let tags = read_tags_colored(&item.path);
    let mtime = mtime_local(&item.path).unwrap_or_else(epoch);
    // Bare file: the file itself is both the commented object and the text source.
    let excerpt = row_description(&item.path, &item.path, ext);
    // Display title = the primary's first H1 if present, else the filename text
    // (foundation #4). The slug/identity stays filename-derived — never the H1.
    let h1 = extract_h1(&item.path, ext);

    // A bare file that resolves to a single URL (`.webloc`/`.url`/URL-only text)
    // IS a link post (post-model.md §4): the file's whole substance is the
    // destination, so `kind()` becomes `link` and the body is the embed card.
    let link_url = resolve_link_destination(&item.path, ext);

    // A date-named bare file is dated by its own name (any precision) and never
    // claims a bare URL; its trailing text (or an H1) shows as a display title.
    // Natural filenames otherwise publish as a normal labeled post at their
    // mtime, addressed by the derived slug — a spaced name is no error.
    let (timestamp, label, slug, display_label) = match parse_date_name(ident) {
        Some((date, title)) => (date, title.clone(), None, h1.or(title)),
        None => (
            PostDate::from_mtime(mtime),
            Some(ident.to_string()),
            derive_slug(ident),
            h1,
        ),
    };

    Entry {
        path: item.path.clone(),
        dir: None,
        timestamp,
        edited: None,
        label,
        slug,
        display_label,
        excerpt,
        extension: ext.to_string(),
        tags,
        grade: None,
        aliases: Vec::new(),
        revisions: Vec::new(),
        error: None,
        listing: None,
        attachments: Vec::new(),
        link_url,
        link_title: None,
    }
}

/// Derive a post's slug and loudly log a reserved one. A slug that is `saved` or
/// purely numeric never claims its bare URL (the router owns those segments), so
/// the post lives at its date path instead; the log makes that non-silent.
fn derive_slug(name: &str) -> Option<String> {
    let s = slug(name);
    if let Some(ref slug) = s {
        if is_reserved_slug(slug) {
            tracing::warn!(
                "Post name '{}' slugs to the reserved segment '/{}' (a route or the year view); \
                 it yields the bare URL and is addressed at its date path instead.",
                name,
                slug
            );
        }
    }
    s
}

/// A folder post: one primary content file, optional assets, and empty marker
/// folders (a single date marker, any number of `alias <name>/`).
fn build_folder_post(item: &TopItem, claim: Option<&str>) -> Entry {
    let dir = &item.path;
    // A promoted orphan copy claims the base name; its inner files keep the base's
    // names (Cmd-D renames only the folder), so the primary resolves against it.
    let label = claim.unwrap_or(&item.name).to_string();
    // Finder tags are read at the post level — here, the folder itself.
    let tags = read_tags_colored(dir);

    let scan = match scan_folder(dir, &label) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to read folder post {}: {}", dir.display(), e);
            return errored_folder(item, &label, tags, PostError::NoPrimary);
        }
    };

    if let Some(err) = scan.error {
        return errored_folder(item, &label, tags, err);
    }

    if let Some(listing) = scan.listing {
        return build_listing_post(item, &label, tags, scan.date_marker, scan.aliases, listing);
    }

    let primary = scan.primary.expect("a folder post with neither error nor listing has a primary");
    let (_, ext) = split_name(&primary.name);
    // Folder post: the comment is read at the post level (the folder, like tags),
    // the auto-excerpt from the primary file inside it.
    let excerpt = row_description(dir, &primary.path, ext);
    let h1 = extract_h1(&primary.path, ext);

    // "Edited" = the primary's mtime, shown only when meaningfully later than the
    // publish date. Compared against the publish date's starting instant.
    let edited_after = |date: &PostDate| {
        date.to_local_instant()
            .filter(|inst| primary.mtime > *inst + chrono::Duration::minutes(1))
            .map(|_| primary.mtime)
    };

    // A date-named folder is dated by its own name at that precision, unlabeled,
    // never claiming a bare URL. Otherwise: the empty date-marker subfolder pins
    // the publish date, else the primary's mtime; the slug comes from the label.
    let (timestamp, edited, post_label, post_slug, display_label) = match parse_date_name(&label) {
        Some((date, title)) => {
            let edited = edited_after(&date);
            (date, edited, title.clone(), None, h1.or(title))
        }
        None => {
            let (timestamp, edited) = match scan.date_marker {
                Some(marker) => (marker, edited_after(&marker)),
                None => (PostDate::from_mtime(primary.mtime), None),
            };
            (timestamp, edited, Some(label.clone()), derive_slug(&label), h1)
        }
    };

    Entry {
        path: primary.path,
        dir: Some(dir.clone()),
        timestamp,
        edited,
        label: post_label,
        slug: post_slug,
        display_label,
        excerpt,
        extension: ext.to_string(),
        tags,
        grade: None,
        aliases: scan.aliases,
        revisions: scan.revisions,
        error: None,
        listing: None,
        attachments: scan.attachments,
        link_url: scan.link_url,
        link_title: None,
    }
}

/// A folder post that renders as a browsable listing (no single primary, or an
/// `index/` marker). Dated by an intro/date-marker, else the folder's own mtime;
/// an `index`/folder-name intro document supplies the title and description.
/// `extension` is empty so the row shows the folder `/` marker.
fn build_listing_post(
    item: &TopItem,
    label: &str,
    tags: Vec<Tag>,
    date_marker: Option<PostDate>,
    aliases: Vec<String>,
    listing: Listing,
) -> Entry {
    let dir = &item.path;
    let folder_mtime = mtime_local(dir).unwrap_or_else(epoch);
    // An intro document (the `index/`-marker "gallery with a story") gives the
    // listing a title (H1) and one-line description; otherwise the folder-level
    // Finder comment is the only description source.
    let (excerpt, h1) = match &listing.intro {
        Some(p) => (row_description(dir, p, &listing.intro_ext), extract_h1(p, &listing.intro_ext)),
        None => (row_description(dir, dir, ""), None),
    };

    // Same dating rule as a document folder post: a date-named folder is unlabeled
    // at its own precision; else the date marker pins it, else the folder's mtime.
    let (timestamp, post_label, post_slug, display_label) = match parse_date_name(label) {
        Some((date, title)) => (date, title.clone(), None, h1.or(title)),
        None => {
            let ts = date_marker.unwrap_or_else(|| PostDate::from_mtime(folder_mtime));
            (ts, Some(label.to_string()), derive_slug(label), h1)
        }
    };

    Entry {
        path: dir.clone(),
        dir: Some(dir.clone()),
        timestamp,
        edited: None,
        label: post_label,
        slug: post_slug,
        display_label,
        excerpt,
        extension: String::new(),
        tags,
        grade: None,
        aliases,
        revisions: Vec::new(),
        error: None,
        listing: Some(listing),
        attachments: Vec::new(),
        link_url: None,
        link_title: None,
    }
}

/// A folder post that scanned wrong: still an entry (so it renders as a loud
/// error, never vanishing), dated by the folder's own mtime. `label` is the
/// effective name (the claimed base for a promoted copy, else the folder name).
fn errored_folder(item: &TopItem, label: &str, tags: Vec<Tag>, error: PostError) -> Entry {
    Entry {
        path: item.path.clone(),
        dir: Some(item.path.clone()),
        timestamp: PostDate::from_mtime(mtime_local(&item.path).unwrap_or_else(epoch)),
        edited: None,
        label: Some(label.to_string()),
        slug: slug(label),
        display_label: None,
        excerpt: None,
        extension: String::new(),
        tags,
        grade: None,
        aliases: Vec::new(),
        revisions: Vec::new(),
        error: Some(error),
        listing: None,
        attachments: Vec::new(),
        link_url: None,
        link_title: None,
    }
}

// ---------------------------------------------------------------------------
// Row description — a Finder comment (any post) or an auto-excerpt (text posts)
// ---------------------------------------------------------------------------

/// Row descriptions cap at this many characters so a row stays one line; the
/// same capped string also feeds the search haystack (`stats::haystack`).
const DESCRIPTION_MAX_LEN: usize = 200;

/// The one-line row description for a post.
///
/// A user-authored macOS Finder comment wins when present: it is deliberate
/// metadata, so it applies to *any* post kind — a photo, link, or page can carry
/// a description this way, which the text-only auto-excerpt structurally never
/// can. Otherwise fall back to the auto-excerpt (first prose paragraph, text
/// posts only). The comment is collapsed to a single line and capped the same as
/// an excerpt; it is HTML-escaped at render like every other field.
///
/// `comment_path` is the post-level object the reader comments in Finder — the
/// folder for a folder post, the bare file for a bare-file post, mirroring how
/// tags are read at the post level. `text_path` is the primary content file the
/// auto-excerpt reads.
fn row_description(comment_path: &Path, text_path: &Path, ext: &str) -> Option<String> {
    if let Some(comment) = read_finder_comment(comment_path) {
        let one_line = collapse_ws(&comment);
        if !one_line.is_empty() {
            return Some(truncate_words(&one_line, DESCRIPTION_MAX_LEN));
        }
    }
    extract_excerpt(text_path, ext)
}

/// A one-line row description: the first prose paragraph of a text post,
/// stripped to plain text and length-capped. `None` for non-text posts
/// (photos, links, folders, other files) and — deliberately — for HTML pages,
/// whose `<style>`/`<script>` text must never leak into a description.
fn extract_excerpt(path: &Path, ext: &str) -> Option<String> {
    const MAX_READ: u64 = 16 * 1024;
    let text_like = matches!(
        ext.to_ascii_lowercase().as_str(),
        "md" | "markdown" | "txt" | "text" | "adoc" | "asciidoc" | "rst" | "org" | "tex"
    );
    if !text_like {
        return None;
    }
    // A description never needs the whole file — read only a prefix.
    let mut buf = Vec::new();
    {
        use std::io::Read;
        let file = std::fs::File::open(path).ok()?;
        file.take(MAX_READ).read_to_end(&mut buf).ok()?;
    }
    let text = String::from_utf8_lossy(&buf);
    let para = collapse_ws(&first_prose_paragraph(&text)?);
    if para.is_empty() {
        None
    } else {
        Some(truncate_words(&para, DESCRIPTION_MAX_LEN))
    }
}

/// The primary's first heading as a display title (foundation #4): a markdown
/// ATX `# Title`, an AsciiDoc `= Title`, or a setext title underlined with `===`.
/// Best-effort, text formats only; `None` when there is no leading heading (the
/// display then falls back to the filename text). Never reads HTML, whose markup
/// must not leak into a title (same rule as the excerpt).
fn extract_h1(path: &Path, ext: &str) -> Option<String> {
    const MAX_READ: u64 = 16 * 1024;
    let text_like = matches!(
        ext.to_ascii_lowercase().as_str(),
        "md" | "markdown" | "txt" | "text" | "adoc" | "asciidoc" | "rst" | "org" | "tex"
    );
    if !text_like {
        return None;
    }
    let mut buf = Vec::new();
    {
        use std::io::Read;
        let file = std::fs::File::open(path).ok()?;
        file.take(MAX_READ).read_to_end(&mut buf).ok()?;
    }
    let text = String::from_utf8_lossy(&buf);
    first_heading(&text)
}

/// The first heading in a text document, or `None`. Recognizes ATX (`# `),
/// AsciiDoc (`= `), and setext (a line underlined by `===`). Only the first
/// non-blank line is considered, so prose without a heading yields no title.
fn first_heading(text: &str) -> Option<String> {
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let first = lines.next()?;
    if let Some(rest) = first.strip_prefix("# ") {
        let title = collapse_ws(&strip_inline_markup(rest));
        return (!title.is_empty()).then_some(title);
    }
    if let Some(rest) = first.strip_prefix("= ") {
        let title = collapse_ws(&strip_inline_markup(rest));
        return (!title.is_empty()).then_some(title);
    }
    // Setext H1: a plain line underlined by two or more `=` on the next line.
    if let Some(next) = lines.next() {
        let underlined = next.len() >= 2 && next.bytes().all(|b| b == b'=');
        let is_prose = !first.starts_with(['#', '>', '|', '-', '*', '+', '=']);
        if underlined && is_prose {
            let title = collapse_ws(&strip_inline_markup(first));
            return (!title.is_empty()).then_some(title);
        }
    }
    None
}

/// The first run of prose lines: skips leading blanks, ATX headings, block
/// quotes, tables, fenced code, comments and thematic breaks, then joins the
/// first paragraph and strips inline markup. Best-effort, format-agnostic.
fn first_prose_paragraph(text: &str) -> Option<String> {
    let mut in_fence = false;
    let mut para: Vec<String> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with("```") || line.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if line.is_empty() {
            if para.is_empty() {
                continue; // still skipping leading blanks
            }
            break; // a blank ends the first paragraph
        }
        if is_block_marker(line) {
            if para.is_empty() {
                continue; // skip a leading heading / marker (e.g. the title)
            }
            break;
        }
        para.push(strip_inline_markup(line));
    }
    if para.is_empty() {
        None
    } else {
        Some(para.join(" "))
    }
}

/// A trimmed, non-empty line that opens a non-prose block we skip over.
fn is_block_marker(line: &str) -> bool {
    let first = line.as_bytes()[0];
    matches!(first, b'#' | b'>' | b'|')
        || line.starts_with("- ")
        || line.starts_with("* ")
        || line.starts_with("+ ")
        || line.starts_with("<!--")
        || line == "---"
        || line == "***"
        || line == "___"
        || line.starts_with("// ")                        // asciidoc comment
        || (first == b'=' && line[1..].starts_with(' '))  // asciidoc "= Title"
}

/// Strip HTML tags, markdown links/images, and emphasis/code markers, leaving
/// readable text: `[text](url)` -> `text`, `![alt](url)` -> `alt`.
fn strip_inline_markup(s: &str) -> String {
    // 1) drop HTML tags
    let mut no_tags = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => no_tags.push(c),
            _ => {}
        }
    }
    // 2) resolve markdown links/images to their visible text
    let delinked = delink(&no_tags);
    // 3) drop emphasis / code / strikethrough markers
    delinked.chars().filter(|c| !matches!(c, '*' | '`' | '~')).collect()
}

/// Replace `[text](url)` / `![alt](url)` with their visible text. Unmatched or
/// reference-style brackets degrade to their inner text (best-effort).
fn delink(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        if out.ends_with('!') {
            out.pop(); // it was an image ![alt](...) — drop the marker
        }
        let after_open = &rest[open + 1..];
        let Some(close) = after_open.find(']') else {
            out.push_str(after_open);
            return out;
        };
        out.push_str(&after_open[..close]);
        let after_close = &after_open[close + 1..];
        if let Some(stripped) = after_close.strip_prefix('(') {
            if let Some(paren) = stripped.find(')') {
                rest = &stripped[paren + 1..];
                continue;
            }
        }
        rest = after_close;
    }
    out.push_str(rest);
    out
}

/// Collapse all runs of whitespace to single spaces and trim.
fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Truncate to at most `max` characters on a word boundary, appending an
/// ellipsis when the text was shortened.
fn truncate_words(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out = String::new();
    for word in s.split(' ') {
        let sep = usize::from(!out.is_empty());
        if out.chars().count() + sep + word.chars().count() > max {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    if out.is_empty() {
        out = s.chars().take(max).collect();
    }
    out.push('\u{2026}');
    out
}

/// The resolved primary file of a folder post.
struct PrimaryFile {
    name: String,
    path: PathBuf,
    mtime: NaiveDateTime,
}

/// A regular file child of a folder post, with its stem split out.
struct FileChild {
    name: String,
    stem: String,
    path: PathBuf,
    mtime: NaiveDateTime,
}

/// The result of scanning a folder post's contents. Exactly one of `primary`,
/// `listing`, or `error` is set (a document post, a browsable listing, or a
/// fail-closed error); markers (`date_marker`, `aliases`) apply to all three.
struct FolderScan {
    primary: Option<PrimaryFile>,
    listing: Option<Listing>,
    /// A document post's public sibling files (empty for a listing or an error).
    attachments: Vec<ListItem>,
    /// The post's outbound destination (post-model.md §4): a `link.*` sidecar or a
    /// non-primary file that resolves to a single URL. `None` for a plain post, or
    /// when several destinations claimed the slot with no `link.*` tie-break (the
    /// scanner declines to guess and emits no cite). Set only in the primary branch.
    link_url: Option<String>,
    date_marker: Option<PostDate>,
    aliases: Vec<String>,
    revisions: Vec<Revision>,
    error: Option<PostError>,
}

/// Read a text file, capped, for link/URL detection. A file larger than the cap
/// cannot be a single-URL link anyway, so the truncated read simply fails the
/// single-token test below.
fn read_text_capped(path: &Path) -> Option<String> {
    use std::io::Read;
    let f = std::fs::File::open(path).ok()?;
    let mut buf = Vec::new();
    f.take(64 * 1024).read_to_end(&mut buf).ok()?;
    String::from_utf8(buf).ok()
}

/// The `URL` value of a macOS `.webloc` bookmark (a plist with a `URL` key).
fn read_webloc_url(path: &Path) -> Option<String> {
    let val = plist::Value::from_file(path).ok()?;
    let url = val.as_dictionary()?.get("URL")?.as_string()?.trim().to_string();
    (!url.is_empty()).then_some(url)
}

/// The `URL=` value of a Windows-style `.url` shortcut (INI). Case-insensitive key.
fn read_dot_url(path: &Path) -> Option<String> {
    let content = read_text_capped(path)?;
    for line in content.lines() {
        let line = line.trim();
        if let Some(eq) = line.find('=') {
            if line[..eq].trim().eq_ignore_ascii_case("URL") {
                let u = line[eq + 1..].trim();
                if !u.is_empty() {
                    return Some(u.to_string());
                }
            }
        }
    }
    None
}

/// Resolve a file to a single outbound destination, or `None` if it is not a link
/// (`post-model.md` §4). A `.webloc`/`.url` bookmark yields its target; a plain-
/// text file yields its content **only when the entire trimmed content is exactly
/// one token** — a title line plus a URL is a note-with-a-link, not a link. The
/// destination must pass the scheme guard as an `http(s)` URL; anything else
/// (`javascript:`, a bare word, …) is not a link, so the file stays ordinary.
/// Whether a non-primary folder file may claim the post's cite. Intent must be
/// carried by the format or the name, never inferred from content alone: a
/// bookmark format (.webloc/.url) *is* a URL by construction, so it always
/// qualifies; a general text file qualifies only when its stem is `link`. This
/// keeps a file's role stable — editing `notes.txt` down to a single URL, or
/// adding a second URL-file next to it, never changes what either file means.
/// (Bare-file posts at the top level are exempt: there the file is the whole
/// post, so single-URL content only changes its own rendering, no other file's
/// role — see `build_bare_post`.)
fn cite_candidate(stem: &str, ext: &str) -> bool {
    matches!(crate::entry::normalize_ext(ext).as_str(), "webloc" | "url")
        || stem.eq_ignore_ascii_case("link")
}

fn resolve_link_destination(path: &Path, ext: &str) -> Option<String> {
    let url = match crate::entry::normalize_ext(ext).as_str() {
        "webloc" => read_webloc_url(path)?,
        "url" => read_dot_url(path)?,
        // Plain-text media (and dotless text): a whole-content single URL.
        "md" | "txt" | "" => {
            let content = read_text_capped(path)?;
            let t = content.trim();
            if t.is_empty() || t.split_whitespace().count() != 1 {
                return None;
            }
            t.to_string()
        }
        _ => return None,
    };
    // Only an http(s) destination is a link; the scheme guard's http/https verdict
    // is the single source of truth (it also normalizes and rejects obfuscation).
    match crate::outbound::classify_scheme(&url) {
        crate::outbound::Scheme::HttpsWeb | crate::outbound::Scheme::HttpWeb => Some(url),
        _ => None,
    }
}

fn scan_folder(dir: &Path, label: &str) -> std::io::Result<FolderScan> {
    let mut date_markers: Vec<(String, PostDate)> = Vec::new();
    let mut aliases: Vec<String> = Vec::new();
    let mut files: Vec<FileChild> = Vec::new();
    // Public, non-marker subfolders are nested listings (post-model.md §6): folder
    // rows in this folder's listing/attachments, browsable at `<parent>/<name>/`.
    // Untagged subfolders stay invisible (fail-closed allowlist).
    let mut subdirs: Vec<ListItem> = Vec::new();
    // An empty `index/` marker forces listing mode (post-model.md §6) — the same
    // file/folder split web servers use (`index.md` file = primary; `index/`
    // folder = listing directive). Joins the empty-folder marker vocabulary.
    let mut force_listing = false;

    for child in std::fs::read_dir(dir)? {
        let child = child?;
        let cpath = child.path();
        let cname = match cpath.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if cname.starts_with('.') || is_cache_name(&cname) {
            continue;
        }
        if child.file_type()?.is_dir() {
            // Subfolders are markers or (for now) ignored — never assets. The
            // `index/`, date, and `alias <name>/` markers must be empty.
            if cname.eq_ignore_ascii_case("index") && is_effectively_empty(&cpath) {
                force_listing = true;
            } else if let Some(dt) = parse_date_marker(&cname) {
                if is_effectively_empty(&cpath) {
                    date_markers.push((cname, dt));
                } else {
                    tracing::debug!("Ignoring non-empty date-named subfolder in {}: {}", dir.display(), cname);
                }
            } else if let Some(alias) = parse_alias_marker(&cname) {
                aliases.push(alias);
            } else if file_is_public(&cpath) {
                let mtime = mtime_local(&cpath).unwrap_or_else(epoch);
                subdirs.push(dir_list_item(&cname, &cpath, mtime));
            } else {
                tracing::debug!("Ignoring non-public subfolder in {}: {}", dir.display(), cname);
            }
        } else {
            let stem = split_name(&cname).0.to_string();
            let mtime = mtime_local(&cpath).unwrap_or_else(epoch);
            files.push(FileChild { name: cname, stem, path: cpath, mtime });
        }
    }

    // Exactly one date marker, or none. Two is a hard error at every mode — no
    // degraded rendering can respect an unknown publish date.
    if date_markers.len() > 1 {
        let mut names: Vec<String> = date_markers.into_iter().map(|(n, _)| n).collect();
        names.sort();
        return Ok(FolderScan {
            primary: None,
            listing: None,
            attachments: Vec::new(),
            link_url: None,
            date_marker: None,
            aliases,
            revisions: Vec::new(),
            error: Some(PostError::MultipleDateMarkers(names)),
        });
    }
    let date_marker = date_markers.into_iter().next().map(|(_, dt)| dt);

    // ` copy [n]` files are revision snapshots; the rest are primary/asset files.
    let mut copies: Vec<(FileChild, String, u32)> = Vec::new();
    let mut plain: Vec<FileChild> = Vec::new();
    for f in files {
        match parse_revision_suffix(&f.stem) {
            Some((base, rank)) => copies.push((f, base, rank)),
            None => plain.push(f),
        }
    }

    // Primary candidates = files whose stem is `index` or the folder name,
    // compared on the slug so `My Resumé/my-resume.md` matches (post-model.md §1).
    let is_candidate = |f: &FileChild| {
        f.stem.eq_ignore_ascii_case("index")
            || slug(&f.stem) == slug(label) && slug(label).is_some()
            || f.stem == label
    };
    let candidates: Vec<&FileChild> = plain.iter().filter(|f| is_candidate(f)).collect();

    // An `index/` marker forces a listing even when a primary could resolve; an
    // `index`/folder-name document then becomes the listing's intro prose — but
    // only when that document itself is tagged `public` (the universal per-file
    // rule: no `public` tag on a file, no served content).
    if force_listing {
        let intro = plain.iter().find(|f| is_candidate(f) && file_is_public(&f.path));
        let listing = build_listing(&plain, &subdirs, intro, Vec::new());
        return Ok(FolderScan {
            primary: None,
            listing: Some(listing),
            attachments: Vec::new(),
            link_url: None,
            date_marker,
            aliases,
            revisions: Vec::new(),
            error: None,
        });
    }

    // One candidate → primary; none but a single lone file → that file. Both
    // remaining cases demote to a listing instead of erroring (post-model.md §6):
    // several candidates → decline to guess + collision notice; no candidate with
    // several files → an automatic listing. A truly empty folder is NoPrimary.
    let primary_child: Option<&FileChild> = match candidates.len() {
        1 => Some(candidates[0]),
        0 if plain.len() == 1 => Some(&plain[0]),
        _ => None,
    };

    let primary_child = match primary_child {
        Some(p) => p,
        None => {
            // Truly empty (no files and no public subfolders) is the only NoPrimary
            // case; a folder of only public subfolders is a listing of them.
            if plain.is_empty() && subdirs.is_empty() {
                return Ok(FolderScan {
                    primary: None,
                    listing: None,
                    attachments: Vec::new(),
                    link_url: None,
                    date_marker,
                    aliases,
                    revisions: Vec::new(),
                    error: Some(PostError::NoPrimary),
                });
            }
            // Several primary candidates: never guess which gets the headline —
            // decline and list, with a prominent collision notice (silenced only
            // by an intentional `index/` marker) logged loudly.
            let collision: Vec<String> = if candidates.len() > 1 {
                let mut names: Vec<String> = candidates.iter().map(|f| f.name.clone()).collect();
                names.sort();
                tracing::warn!(
                    "Folder post {} has several primary candidates ({}); declining to pick and \
                     listing instead. Remove one, or add an empty `index/` marker to make the \
                     listing intentional.",
                    dir.display(),
                    names.join(", ")
                );
                names
            } else {
                Vec::new()
            };
            let listing = build_listing(&plain, &subdirs, None, collision);
            return Ok(FolderScan {
                primary: None,
                listing: Some(listing),
                attachments: Vec::new(),
                link_url: None,
                date_marker,
                aliases,
                revisions: Vec::new(),
                error: None,
            });
        }
    };

    // The universal per-file rule (post-model.md §6): the folder's `public` tag
    // makes the post *reachable*; only a file's own `public` tag ever serves its
    // *content*. A would-be primary without its own tag is withheld — the post
    // demotes to a listing of the folder's public files (possibly empty, with
    // the withheld count) instead of rendering the untagged document.
    if !file_is_public(&primary_child.path) {
        tracing::warn!(
            "Folder post {} withholds its primary '{}': the file is not tagged `public` \
             (the folder tag alone never serves file content). Tag the file public to \
             publish it; the post lists its public files meanwhile.",
            dir.display(),
            primary_child.name
        );
        let listing = build_listing(&plain, &subdirs, None, Vec::new());
        return Ok(FolderScan {
            primary: None,
            listing: Some(listing),
            attachments: Vec::new(),
            link_url: None,
            date_marker,
            aliases,
            revisions: Vec::new(),
            error: None,
        });
    }

    // Revisions = ` copy [n]` files that snapshot the primary (share its stem).
    // Same per-file rule: an untagged snapshot's content is never served, so it
    // never becomes an addressable revision.
    let primary_stem = primary_child.stem.clone();
    let mut revisions: Vec<Revision> = copies
        .iter()
        .filter(|(f, base, _)| *base == primary_stem && file_is_public(&f.path))
        .map(|(f, _, rank)| Revision { date: f.mtime, path: f.path.clone(), rank: *rank })
        .collect();
    sort_revisions(&mut revisions);

    let primary = PrimaryFile {
        name: primary_child.name.clone(),
        path: primary_child.path.clone(),
        mtime: primary_child.mtime,
    };

    // Outbound destination (post-model.md §4): a bookmark or a `link.*` text
    // sidecar drops out of the file set and becomes the post's cite. Candidacy
    // is intent-carried, never content-sniffed (`cite_candidate`): a bookmark
    // format (.webloc/.url) is a URL by construction, so any name qualifies —
    // dragging a Safari bookmark in cites without a rename; a general text file
    // qualifies only when its stem is `link`, so editing a note down to one URL
    // never silently changes its role. The per-file rule applies here too — the
    // cite publishes the file's content (the URL), so an untagged URL-file is no
    // destination; it stays withheld like any other untagged sibling. The stem
    // `link` is the explicit tie-break; several destinations with no single
    // `link.*` never guess — emit no cite, log loudly (the files then stay
    // ordinary listed/attachment content).
    let destinations: Vec<(&FileChild, String)> = plain
        .iter()
        .filter(|f| f.path != primary_child.path && file_is_public(&f.path))
        .filter_map(|f| {
            let (_, ext) = split_name(&f.name);
            if !cite_candidate(&f.stem, ext) {
                return None;
            }
            resolve_link_destination(&f.path, ext).map(|u| (f, u))
        })
        .collect();
    let (link_url, link_path): (Option<String>, Option<PathBuf>) = match destinations.len() {
        0 => (None, None),
        1 => (Some(destinations[0].1.clone()), Some(destinations[0].0.path.clone())),
        _ => {
            let marked: Vec<&(&FileChild, String)> = destinations
                .iter()
                .filter(|(f, _)| f.stem.eq_ignore_ascii_case("link"))
                .collect();
            if marked.len() == 1 {
                (Some(marked[0].1.clone()), Some(marked[0].0.path.clone()))
            } else {
                let mut names: Vec<String> =
                    destinations.iter().map(|(f, _)| f.name.clone()).collect();
                names.sort();
                tracing::warn!(
                    "Folder post {} claims {} outbound destinations ({}) with no single `link.*` \
                     tie-break; emitting no cite. Keep one, or name one `link.*`.",
                    dir.display(),
                    destinations.len(),
                    names.join(", ")
                );
                (None, None)
            }
        }
    };

    // Attachments = the post's public sibling files + public subfolders (folder
    // rows), in filename order — rendered below the body (post-model.md §6). The
    // promoted destination file drops out (it is the cite, not a listed sibling).
    let mut att_files: Vec<ListItem> = plain
        .iter()
        .filter(|f| {
            f.path != primary_child.path
                && Some(&f.path) != link_path.as_ref()
                && file_is_public(&f.path)
        })
        .map(to_list_item)
        .collect();
    att_files.sort_by(|a, b| a.name.cmp(&b.name));
    let mut attachments = subdirs.clone();
    attachments.sort_by(|a, b| a.name.cmp(&b.name));
    attachments.extend(att_files); // folders first, then files

    Ok(FolderScan {
        primary: Some(primary),
        listing: None,
        attachments,
        link_url,
        date_marker,
        aliases,
        revisions,
        error: None,
    })
}

/// Build a listing for a nested subfolder, read fresh from disk (post-model.md
/// §6). A nested folder has no identity or primary resolution — it is purely a
/// browsable index of its public files and public sub-subfolders. Markers,
/// dotfiles, cache dirs, and `copy [n]` snapshots are excluded; the caller has
/// already confirmed the whole path is `public` via `path_visible`.
pub fn build_dir_listing(dir: &Path) -> Listing {
    let mut plain: Vec<FileChild> = Vec::new();
    let mut subdirs: Vec<ListItem> = Vec::new();
    let read = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return build_listing(&[], &[], None, Vec::new()),
    };
    for child in read.filter_map(Result::ok) {
        let cpath = child.path();
        let cname = match cpath.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if cname.starts_with('.') || is_cache_name(&cname) {
            continue;
        }
        match child.file_type() {
            Ok(t) if t.is_dir() => {
                // Skip the empty-marker vocabulary; list public content subfolders.
                let is_marker = (cname.eq_ignore_ascii_case("index")
                    || parse_date_marker(&cname).is_some()
                    || parse_alias_marker(&cname).is_some())
                    && is_effectively_empty(&cpath);
                if !is_marker && file_is_public(&cpath) {
                    let mtime = mtime_local(&cpath).unwrap_or_else(epoch);
                    subdirs.push(dir_list_item(&cname, &cpath, mtime));
                }
            }
            Ok(_) => {
                let stem = split_name(&cname).0.to_string();
                if parse_revision_suffix(&stem).is_some() {
                    continue; // `copy [n]` snapshots are not listed
                }
                let mtime = mtime_local(&cpath).unwrap_or_else(epoch);
                plain.push(FileChild { name: cname, stem, path: cpath, mtime });
            }
            Err(_) => continue,
        }
    }
    build_listing(&plain, &subdirs, None, Vec::new())
}

/// Whether a file is `public` (and not `private`) at its own level — the listing
/// membership rule (an allowlist). The folder above it is already known public
/// (the post is visible), so this direct-child check is equivalent to the full
/// `path_visible` chain; nested listings re-walk the chain per level in Commit 6c.
fn file_is_public(path: &Path) -> bool {
    let tags = read_tags_colored(path);
    tags.iter().any(Tag::is_public) && !tags.iter().any(Tag::is_private)
}

/// Turn a folder child into a listing row (size + image classification read here).
fn to_list_item(f: &FileChild) -> ListItem {
    let ext = split_name(&f.name).1.to_ascii_lowercase();
    let size = std::fs::metadata(&f.path).map(|m| m.len()).unwrap_or(0);
    ListItem {
        name: f.name.clone(),
        stem: f.stem.clone(),
        is_image: is_image_ext(&ext),
        ext,
        path: f.path.clone(),
        mtime: f.mtime,
        size,
        is_dir: false,
    }
}

/// A subfolder listing row — a nested listing reachable at `<parent>/<name>/`.
fn dir_list_item(name: &str, path: &Path, mtime: NaiveDateTime) -> ListItem {
    ListItem {
        name: name.to_string(),
        stem: name.to_string(),
        ext: String::new(),
        path: path.to_path_buf(),
        mtime,
        size: 0,
        is_image: false,
        is_dir: true,
    }
}

/// Build a `Listing` from a folder's plain files and its public subfolders: the
/// public files become rows (an `intro` document is excluded), the subfolders
/// become nested-listing folder rows; all in filename order. `total` counts every
/// candidate file so the reader can see something is withheld. Folders sort first.
fn build_listing(
    plain: &[FileChild],
    subdirs: &[ListItem],
    intro: Option<&FileChild>,
    collision: Vec<String>,
) -> Listing {
    let intro_path = intro.map(|f| f.path.as_path());
    let listable: Vec<&FileChild> =
        plain.iter().filter(|f| Some(f.path.as_path()) != intro_path).collect();
    let mut files: Vec<ListItem> =
        listable.iter().filter(|f| file_is_public(&f.path)).map(|f| to_list_item(f)).collect();
    files.sort_by(|a, b| a.name.cmp(&b.name)); // Finder's filename order, not mtime.
    let mut dirs = subdirs.to_vec();
    dirs.sort_by(|a, b| a.name.cmp(&b.name));
    // Folders first, then files — the usual Finder-column convention.
    dirs.extend(files);
    Listing {
        items: dirs,
        total: listable.len(),
        intro: intro.map(|f| f.path.clone()),
        intro_ext: intro.map(|f| split_name(&f.name).1.to_string()).unwrap_or_default(),
        collision,
    }
}

/// Collapse each top-level `<base> copy [n]` sibling into its family. A family is
/// keyed by the base name, *not* by whether the base file exists (`post-model.md`
/// §5):
///
/// - **Base present** — a current post is named `<base>`: the copies attach to it
///   as archived revisions, preferring a kind match (a folder copy revises a
///   folder post, a file copy a bare file) when both claim the name.
/// - **Base absent** — the newest-by-mtime orphan copy is *promoted* to be the
///   post, claiming the base name and slug (so `/base` keeps resolving after the
///   base is deleted — the recovery property), and the rest become its revisions.
///
/// This ends the old *silent drop* of orphan copies. Every collapse is logged so
/// the family model is not invisible, but there is deliberately no reader-facing
/// notice: a routine Cmd-D backup and a legitimately-titled `foo copy` that a
/// later `foo` shadows are structurally identical to a stateless scan (age can't
/// separate them — Cmd-D archives are legitimately older), and both "just work" —
/// the copy stays reachable through the revision dropdown and its date-path URL.
/// Recovery from an unwanted collapse is one rename.
fn attach_revisions(entries: &mut Vec<Entry>, revisions: &[TopItem]) {
    // Copies whose base has no current post, grouped by base for promotion.
    let mut orphans: HashMap<String, Vec<(&TopItem, u32)>> = HashMap::new();

    for rev in revisions {
        let stem = if rev.is_dir { rev.name.clone() } else { split_name(&rev.name).0.to_string() };
        let (base, rank) = match parse_revision_suffix(&stem) {
            Some(v) => v,
            None => continue, // only copies reach here
        };

        match choose_current(entries, &base, rev.is_dir) {
            Some(i) => {
                let (rev_path, rev_mtime) = match revision_primary(rev) {
                    Some(v) => v,
                    None => {
                        tracing::warn!("Skipping unreadable archived revision: {}", rev.path.display());
                        continue;
                    }
                };
                // Per-file rule: an archived snapshot's content is served only
                // when its whole chain is tagged `public` (the copy item, and —
                // for a folder copy — the inner primary file too).
                if !revision_chain_public(rev, &rev_path) {
                    tracing::info!(
                        "Revision family '{}': withholding the copy '{}' — not tagged `public` \
                         (file content is only served with its own tag).",
                        base,
                        rev.name
                    );
                    continue;
                }
                entries[i].revisions.push(Revision { date: rev_mtime, path: rev_path, rank });
                sort_revisions(&mut entries[i].revisions);
                tracing::info!(
                    "Revision family '{}': archived the copy '{}' as a revision of the current post.",
                    base,
                    rev.name
                );
            }
            None => orphans.entry(base).or_default().push((rev, rank)),
        }
    }

    // Promote each base-absent family: newest copy becomes the post, rest revisions.
    for (base, group) in orphans {
        // Resolve each orphan's primary (path + mtime); drop the unreadable ones.
        let mut resolved: Vec<(&TopItem, u32, PathBuf, NaiveDateTime)> = group
            .into_iter()
            .filter_map(|(rev, rank)| match revision_primary(rev) {
                Some((path, mtime)) => Some((rev, rank, path, mtime)),
                None => {
                    tracing::warn!("Skipping unreadable orphan copy: {}", rev.path.display());
                    None
                }
            })
            .collect();
        if resolved.is_empty() {
            continue;
        }
        // Newest first; a higher copy number breaks an exact mtime tie (matching
        // `sort_revisions`). The head is promoted; the tail become its revisions.
        resolved.sort_by(|a, b| b.3.cmp(&a.3).then(b.1.cmp(&a.1)));
        let winner = resolved[0].0;
        let mut promoted = build_post_as(winner, Some(base.as_str()));
        for (rev, rank, path, mtime) in resolved.into_iter().skip(1) {
            // Same per-file rule as attached revisions: no `public` chain, no
            // served snapshot.
            if !revision_chain_public(rev, &path) {
                tracing::info!(
                    "Revision family '{}': withholding the copy '{}' — not tagged `public` \
                     (file content is only served with its own tag).",
                    base,
                    rev.name
                );
                continue;
            }
            promoted.revisions.push(Revision { date: mtime, path, rank });
        }
        sort_revisions(&mut promoted.revisions);
        tracing::info!(
            "Revision family '{}': no current post by that name; promoted the newest copy '{}' to be the post, with {} older revision(s).",
            base,
            winner.name,
            promoted.revisions.len()
        );
        entries.push(promoted);
    }
}

/// Index of the current post a `<base> copy` should revise: one whose label is
/// exactly `base`, preferring a kind match (folder copy → folder post, file copy →
/// bare file) when the name is claimed by both. `None` when no current post
/// carries the name — the copy is then an orphan for `attach_revisions` to promote.
fn choose_current(entries: &[Entry], base: &str, is_dir: bool) -> Option<usize> {
    let mut chosen: Option<usize> = None;
    for (i, e) in entries.iter().enumerate() {
        if e.error.is_some() || e.label.as_deref() != Some(base) {
            continue;
        }
        match chosen {
            None => chosen = Some(i),
            Some(c) => {
                if (e.dir.is_some() == is_dir) && (entries[c].dir.is_some() != is_dir) {
                    chosen = Some(i);
                }
            }
        }
    }
    chosen
}

/// Whether a top-level ` copy [n]` sibling's served content is fully `public`:
/// the copy item itself, and — for a folder copy — the inner primary file too
/// (the same every-component chain `path_visible` enforces at request time).
fn revision_chain_public(rev: &TopItem, primary: &Path) -> bool {
    if !file_is_public(&rev.path) {
        return false;
    }
    !rev.is_dir || file_is_public(primary)
}

/// The primary content file (and its mtime) of a top-level ` copy [n]` sibling.
/// A folder copy keeps its inner files' original names, so its primary is
/// resolved against the family base, not the copy's own (renamed) folder.
fn revision_primary(rev: &TopItem) -> Option<(PathBuf, NaiveDateTime)> {
    if rev.is_dir {
        let (base, _) = parse_revision_suffix(&rev.name)?;
        resolve_primary_file(&rev.path, &base)
    } else {
        Some((rev.path.clone(), mtime_local(&rev.path)?))
    }
}

/// Resolve just a folder's primary file (path + mtime), matching candidate stems
/// against `match_name`. Shared shape with `scan_folder` but without the marker
/// and error bookkeeping — used for revision folders.
fn resolve_primary_file(dir: &Path, match_name: &str) -> Option<(PathBuf, NaiveDateTime)> {
    let mut plain: Vec<FileChild> = Vec::new();
    for child in std::fs::read_dir(dir).ok()? {
        let child = match child {
            Ok(c) => c,
            Err(_) => continue,
        };
        let cpath = child.path();
        let cname = match cpath.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if cname.starts_with('.') || is_cache_name(&cname) {
            continue;
        }
        match child.file_type() {
            Ok(t) if t.is_dir() => continue,
            Ok(_) => {}
            Err(_) => continue,
        }
        let stem = split_name(&cname).0.to_string();
        if parse_revision_suffix(&stem).is_some() {
            continue; // a copy folder's own inner snapshots don't count as primary
        }
        let mtime = mtime_local(&cpath).unwrap_or_else(epoch);
        plain.push(FileChild { name: cname, stem, path: cpath, mtime });
    }

    let candidates: Vec<&FileChild> = plain
        .iter()
        .filter(|f| f.stem == match_name || f.stem.eq_ignore_ascii_case("index"))
        .collect();
    let chosen = match candidates.len() {
        1 => candidates[0],
        0 if plain.len() == 1 => &plain[0],
        _ => return None,
    };
    Some((chosen.path.clone(), chosen.mtime))
}

// ─── Parsing helpers ─────────────────────────────────────────────

/// Split a filename into `(stem, extension)` on the last interior dot. A dotless
/// name (or a leading-dot name) yields an empty extension.
fn split_name(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 && i + 1 < name.len() => (&name[..i], &name[i + 1..]),
        _ => (name, ""),
    }
}

/// Whether a name is a legacy in-tree embed-cache directory. The server now
/// writes all caches outside the content tree, so these only linger from before
/// the relocation; the scanner keeps ignoring them (never a post or an asset).
fn is_cache_name(name: &str) -> bool {
    name.ends_with(".embed-cache")
}

/// Parse an `alias <name>` marker-folder name into its aliased name. The single
/// space after `alias` cannot occur in a real (hyphenated) label, so the prefix
/// is unambiguous.
fn parse_alias_marker(name: &str) -> Option<String> {
    let rest = name.strip_prefix("alias ")?.trim();
    (!rest.is_empty()).then(|| rest.to_string())
}

/// Parse a ` copy`/` copy N` revision suffix off a name (folder name or file
/// stem). Returns `(base, rank)` — rank 1 for ` copy`, N for ` copy N` (N ≥ 2).
fn parse_revision_suffix(name: &str) -> Option<(String, u32)> {
    let one = format!(" {}", COPY_KEYWORD);
    if let Some(base) = name.strip_suffix(&one) {
        if !base.is_empty() {
            return Some((base.to_string(), 1));
        }
    }
    let needle = format!(" {} ", COPY_KEYWORD);
    if let Some(pos) = name.rfind(&needle) {
        let base = &name[..pos];
        let num = &name[pos + needle.len()..];
        if !base.is_empty() && !num.is_empty() && num.bytes().all(|b| b.is_ascii_digit()) {
            if let Ok(n) = num.parse::<u32>() {
                if n >= 2 {
                    return Some((base.to_string(), n));
                }
            }
        }
    }
    None
}

/// Parse an empty date-marker folder name into a precision-aware publish date.
/// Format: `[-]YYYY[-MM[-DD[Thhmm[ss]]]]` with an optional trailing zone (`Z` or
/// `±HHMM`, only after a time); a bare (zoneless) marker is already local. The
/// mandatory `T` is gone — a bare date is a valid, day/month/year-precision
/// marker. Returns `None` for any name that is not such a date. See `PostDate`.
fn parse_date_marker(name: &str) -> Option<PostDate> {
    PostDate::parse(name)
}

/// If a top-level post name is itself a date, or a date followed by descriptive
/// text, return its publish date and an optional display title. A whole-date
/// name is unlabeled (date-addressed, no slug claim); `<date> <text>` is
/// date-addressed with the trailing text shown for recognition — never a slug.
/// See `post-model.md` §2 (date-named posts) and the [review] amendment.
fn parse_date_name(name: &str) -> Option<(PostDate, Option<String>)> {
    if let Some(d) = PostDate::parse(name) {
        return Some((d, None)); // whole name is a date -> unlabeled
    }
    if let Some((head, tail)) = name.split_once(' ') {
        if let Some(d) = PostDate::parse(head) {
            let title = tail.trim();
            return Some((d, (!title.is_empty()).then(|| title.to_string())));
        }
    }
    None
}

/// Whether a directory has no non-dotfile children (`.DS_Store` does not count).
/// An unreadable directory is treated as non-empty — fail closed, don't mistake
/// it for an empty marker.
fn is_effectively_empty(dir: &Path) -> bool {
    match std::fs::read_dir(dir) {
        Ok(rd) => !rd
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_str().map_or(true, |n| !n.starts_with('.'))),
        Err(_) => false,
    }
}

/// Sort revisions newest first; a later copy (higher rank) breaks an exact tie.
fn sort_revisions(revs: &mut [Revision]) {
    revs.sort_by(|a, b| b.date.cmp(&a.date).then(b.rank.cmp(&a.rank)));
}

/// A file's mtime as local wall-clock — how Finder/`touch`/`rsync -t` write it.
fn mtime_local(path: &Path) -> Option<NaiveDateTime> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let dt: DateTime<Local> = modified.into();
    Some(dt.naive_local())
}

/// The Unix epoch, used only as a last-resort timestamp when a file's mtime is
/// unreadable (so a broken post sinks to the bottom rather than panicking).
fn epoch() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(1970, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn excerpt_skips_heading_takes_first_paragraph() {
        assert_eq!(
            first_prose_paragraph("# Title\n\nFirst real paragraph here.\n\nSecond."),
            Some("First real paragraph here.".to_string())
        );
    }

    #[test]
    fn excerpt_strips_markup_links_and_images() {
        assert_eq!(
            first_prose_paragraph("A **bold** word, a [link](https://x.test) and `code`."),
            Some("A bold word, a link and code.".to_string())
        );
        assert_eq!(
            first_prose_paragraph("See ![alt text](img.png) inline."),
            Some("See alt text inline.".to_string())
        );
    }

    #[test]
    fn excerpt_skips_fenced_code_and_block_markers() {
        let src = "```\ncode line\n```\n\n> a quote\n\nProse at last.";
        assert_eq!(first_prose_paragraph(src), Some("Prose at last.".to_string()));
    }

    #[test]
    fn excerpt_none_without_prose() {
        assert_eq!(first_prose_paragraph("# Only a heading"), None);
        assert_eq!(first_prose_paragraph(""), None);
    }

    #[test]
    fn excerpt_truncates_on_word_boundary() {
        let out = truncate_words("one two three four five", 12);
        assert!(out.ends_with('\u{2026}'), "got {out:?}");
        assert!(out.starts_with("one two"));
        assert_eq!(truncate_words("short", 12), "short");
    }

    #[test]
    fn extract_excerpt_text_only_never_html() {
        let d = TmpDir::new();
        touch(d.path(), "note.md", "# Hi\n\nHello world, this is the body.");
        touch(d.path(), "pic.jpg", "not read");
        touch(d.path(), "page.html", "<style>body{color:red}</style><p>Body</p>");
        assert_eq!(
            extract_excerpt(&d.path().join("note.md"), "md"),
            Some("Hello world, this is the body.".to_string())
        );
        assert_eq!(extract_excerpt(&d.path().join("pic.jpg"), "jpg"), None);
        // HTML is deliberately excluded so <style>/<script> text can never leak.
        assert_eq!(extract_excerpt(&d.path().join("page.html"), "html"), None);
    }

    /// Set an xattr under the first platform-accepted name from `names` (macOS
    /// takes the native Apple name; Linux rejects that namespace and takes the
    /// `user.`-mapped one — the same list the readers try). Returns false when
    /// every name is rejected, so a test can skip on an xattr-less filesystem.
    #[must_use]
    fn set_mapped_xattr(path: &Path, names: &[&str], buf: &[u8]) -> bool {
        names.iter().any(|name| xattr::set(path, name, buf).is_ok())
    }

    /// Write Finder tags the way Finder stores them — a binary-plist array of
    /// `"name\nN"` strings in the `_kMDItemUserTags` xattr — so `read_tags_colored`
    /// (and the visibility gate) see them. Returns false when the filesystem
    /// rejects xattrs, so a test can skip rather than fail on an unsupported FS.
    #[must_use]
    fn set_tags(path: &Path, tags: &[&str]) -> bool {
        let arr: Vec<plist::Value> =
            tags.iter().map(|t| plist::Value::String((*t).to_string())).collect();
        let mut buf = Vec::new();
        plist::to_writer_binary(&mut buf, &plist::Value::Array(arr)).unwrap();
        set_mapped_xattr(path, crate::tags::USER_TAGS_XATTR_NAMES, &buf)
    }

    /// Write a Finder comment the way Finder stores it — a binary-plist string in
    /// the `kMDItemFinderComment` xattr — so `read_finder_comment` sees it. Returns
    /// false when the filesystem rejects xattrs, so the test skips rather than
    /// failing on an unsupported FS.
    #[must_use]
    fn set_finder_comment(path: &Path, comment: &str) -> bool {
        let mut buf = Vec::new();
        plist::to_writer_binary(&mut buf, &plist::Value::String(comment.to_string())).unwrap();
        set_mapped_xattr(path, crate::tags::FINDER_COMMENT_XATTR_NAMES, &buf)
    }

    #[test]
    fn finder_comment_overrides_excerpt_on_any_kind() {
        let d = TmpDir::new();
        // A photo has no auto-excerpt, yet a Finder comment gives it a description.
        touch(d.path(), "pic.jpg", "binary-ish bytes, never read as prose");
        let jpg = d.path().join("pic.jpg");
        if !set_finder_comment(&jpg, "A sunset over the harbour") {
            return; // filesystem without xattr support — skip
        }
        assert_eq!(
            row_description(&jpg, &jpg, "jpg"),
            Some("A sunset over the harbour".to_string())
        );

        // On a text post the comment still wins over the first paragraph.
        touch(d.path(), "note.md", "# Title\n\nAuto first paragraph.");
        let md = d.path().join("note.md");
        assert!(set_finder_comment(&md, "Hand-written override"));
        assert_eq!(
            row_description(&md, &md, "md"),
            Some("Hand-written override".to_string())
        );
    }

    #[test]
    fn empty_finder_comment_falls_back_to_excerpt() {
        let d = TmpDir::new();
        touch(d.path(), "note.md", "# Title\n\nThe real first paragraph.");
        let md = d.path().join("note.md");
        if !set_finder_comment(&md, "   \n  \t") {
            return; // xattr unsupported — skip
        }
        // A whitespace-only comment is ignored; the auto-excerpt shows instead.
        assert_eq!(
            row_description(&md, &md, "md"),
            Some("The real first paragraph.".to_string())
        );
    }

    #[test]
    fn finder_comment_collapsed_to_one_line() {
        let d = TmpDir::new();
        touch(d.path(), "pic.jpg", "x");
        let jpg = d.path().join("pic.jpg");
        if !set_finder_comment(&jpg, "line one\n\nline two\tindented") {
            return;
        }
        assert_eq!(
            row_description(&jpg, &jpg, "jpg"),
            Some("line one line two indented".to_string())
        );
    }

    #[test]
    fn folder_post_reads_comment_at_folder_level() {
        let d = TmpDir::new();
        mkdir(d.path(), "story");
        let dir = d.path().join("story");
        touch(&dir, "story.md", "# Story\n\nInner auto paragraph.");
        let primary = dir.join("story.md");
        // A comment on the inner file must be ignored — comments (like tags) are
        // read at the post level (the folder), never on the primary inside it.
        if !set_finder_comment(&primary, "inner-file comment, ignored") {
            return;
        }
        assert!(set_finder_comment(&dir, "post-level description"));
        assert_eq!(
            row_description(&dir, &primary, "md"),
            Some("post-level description".to_string())
        );
    }

    /// A throwaway directory under the OS temp dir, removed on drop. Avoids a
    /// dev-dependency; uniqueness is pid + a process-wide counter.
    struct TmpDir(PathBuf);
    impl TmpDir {
        fn new() -> Self {
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let mut p = std::env::temp_dir();
            p.push(format!("sajt-scan-{}-{}", std::process::id(), n));
            std::fs::create_dir_all(&p).unwrap();
            TmpDir(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TmpDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn touch(dir: &Path, rel: &str, body: &str) {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, body).unwrap();
    }
    fn mkdir(dir: &Path, rel: &str) {
        std::fs::create_dir_all(dir.join(rel)).unwrap();
    }
    fn find<'a>(entries: &'a [Entry], label: &str) -> &'a Entry {
        entries
            .iter()
            .find(|e| e.label.as_deref() == Some(label))
            .unwrap_or_else(|| panic!("no entry labeled {label}"))
    }

    // ── pure helpers ──

    #[test]
    fn split_name_cases() {
        assert_eq!(split_name("a.md"), ("a", "md"));
        assert_eq!(split_name("hello-world.tar.gz"), ("hello-world.tar", "gz"));
        assert_eq!(split_name("noext"), ("noext", ""));
        assert_eq!(split_name("trailing."), ("trailing.", ""));
    }

    #[test]
    fn revision_suffix_parsing() {
        assert_eq!(parse_revision_suffix("foo copy"), Some(("foo".into(), 1)));
        assert_eq!(parse_revision_suffix("foo copy 2"), Some(("foo".into(), 2)));
        assert_eq!(parse_revision_suffix("bacon-stuff copy 12"), Some(("bacon-stuff".into(), 12)));
        assert_eq!(parse_revision_suffix("foo"), None);
        assert_eq!(parse_revision_suffix("copy"), None);
        assert_eq!(parse_revision_suffix("foocopy"), None);
        assert_eq!(parse_revision_suffix("foo copy x"), None);
        assert_eq!(parse_revision_suffix("foo copy 0"), None); // N must be >= 2
        assert_eq!(parse_revision_suffix("foo copy 1"), None); // rank 1 is the bare ` copy`
    }

    #[test]
    fn alias_marker_parsing() {
        assert_eq!(parse_alias_marker("alias resume"), Some("resume".into()));
        assert_eq!(parse_alias_marker("alias t3"), Some("t3".into()));
        assert_eq!(parse_alias_marker("alias "), None);
        assert_eq!(parse_alias_marker("alias"), None);
        assert_eq!(parse_alias_marker("resume"), None);
    }

    #[test]
    fn date_marker_parsing() {
        use crate::postdate::Precision;
        let sec = parse_date_marker("2026-03-03T143052").unwrap();
        assert_eq!(sec.precision, Precision::Second);
        assert_eq!(sec.short_date(), "2026-03-03");
        assert_eq!(sec.hms(), "143052");
        assert_eq!(parse_date_marker("2026-03-03T1430").unwrap().precision, Precision::Minute);
        // Bare dates are valid markers now — the mandatory `T` is gone.
        assert_eq!(parse_date_marker("2026-03-03").unwrap().precision, Precision::Day);
        assert_eq!(parse_date_marker("2026-03").unwrap().precision, Precision::Month);
        assert_eq!(parse_date_marker("2026").unwrap().precision, Precision::Year);
        // Zoned markers parse (exact local value depends on the machine tz).
        assert!(parse_date_marker("2026-03-03T143052Z").is_some());
        assert!(parse_date_marker("2026-03-03T1430+0200").is_some());
        assert!(parse_date_marker("2026-03-03T1430-0500").is_some());
        // Non-dates.
        assert!(parse_date_marker("hello-world").is_none());
        assert!(parse_date_marker("alias resume").is_none());
        assert!(parse_date_marker("2026-13-03T1430").is_none()); // bad month
    }

    // ── scanner ──

    #[test]
    fn bare_file_post() {
        let t = TmpDir::new();
        touch(t.path(), "hello-world.md", "# hi");
        let entries = scan_entries(t.path()).unwrap();
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.label.as_deref(), Some("hello-world"));
        assert_eq!(e.extension, "md");
        assert!(e.dir.is_none());
        assert!(e.edited.is_none());
        assert!(e.error.is_none());
    }

    #[test]
    fn folder_post_with_date_marker() {
        let t = TmpDir::new();
        touch(t.path(), "hello-world/hello-world.md", "# hi");
        mkdir(t.path(), "hello-world/2026-03-03T1430");
        if !set_tags(&t.path().join("hello-world/hello-world.md"), &["public"]) {
            return; // xattr unsupported — skip
        }
        let entries = scan_entries(t.path()).unwrap();
        assert_eq!(entries.len(), 1);
        let e = find(&entries, "hello-world");
        assert_eq!(e.extension, "md");
        assert!(e.dir.is_some());
        assert!(e.error.is_none());
        assert_eq!(e.timestamp, PostDate::parse("2026-03-03T1430").unwrap());
        // The primary was written just now, well after the 2026 marker → edited.
        assert!(e.edited.is_some());
    }

    #[test]
    fn folder_post_index_primary() {
        let t = TmpDir::new();
        touch(t.path(), "notes/index.md", "body");
        if !set_tags(&t.path().join("notes/index.md"), &["public"]) {
            return; // xattr unsupported — skip
        }
        let entries = scan_entries(t.path()).unwrap();
        let e = find(&entries, "notes");
        assert_eq!(e.extension, "md");
        assert!(e.path.ends_with("index.md"));
        assert!(e.error.is_none());
    }

    #[test]
    fn folder_post_sole_file_is_primary() {
        let t = TmpDir::new();
        touch(t.path(), "shot/whatever.jpg", "bytes");
        if !set_tags(&t.path().join("shot/whatever.jpg"), &["public"]) {
            return; // xattr unsupported — skip
        }
        let entries = scan_entries(t.path()).unwrap();
        let e = find(&entries, "shot");
        assert_eq!(e.extension, "jpg");
        assert!(e.path.ends_with("whatever.jpg"));
        assert!(e.error.is_none());
    }

    #[test]
    fn several_primary_candidates_demote_to_a_listing_with_collision() {
        // `p.md` + `index.md` both claim the primary slot: never guess — list, and
        // record the collision for a prominent notice (post-model.md §6).
        let t = TmpDir::new();
        touch(t.path(), "p/p.md", "a");
        touch(t.path(), "p/index.md", "b");
        let entries = scan_entries(t.path()).unwrap();
        let e = find(&entries, "p");
        assert!(e.error.is_none(), "a collision demotes to a listing, not an error");
        let listing = e.listing.as_ref().expect("should be a listing");
        assert_eq!(listing.collision.len(), 2, "both candidates named in the notice");
        assert_eq!(e.kind(), "folder");
    }

    #[test]
    fn no_candidate_several_files_is_an_automatic_listing() {
        // Two documents, neither named the folder/`index`: an automatic listing,
        // no collision notice (nothing claimed the headline).
        let t = TmpDir::new();
        touch(t.path(), "p/a.txt", "a");
        touch(t.path(), "p/b.txt", "b");
        let entries = scan_entries(t.path()).unwrap();
        let e = find(&entries, "p");
        assert!(e.error.is_none());
        let listing = e.listing.as_ref().expect("should be a listing");
        assert!(listing.collision.is_empty(), "no candidate claimed the primary");
        assert_eq!(listing.total, 2);
    }

    #[test]
    fn index_marker_forces_listing_with_intro_and_no_collision() {
        // An empty `index/` marker forces a listing even though `index.md` could
        // be the primary; the doc becomes the intro and the collision is silenced.
        let t = TmpDir::new();
        touch(t.path(), "album/index.md", "# My Album\n\nA story.");
        touch(t.path(), "album/photo.jpg", "img-bytes");
        mkdir(t.path(), "album/index"); // the listing directive
        if !set_tags(&t.path().join("album/photo.jpg"), &["public"]) {
            return; // xattr unsupported — skip
        }
        assert!(set_tags(&t.path().join("album/index.md"), &["public"])); // the intro serves content
        let entries = scan_entries(t.path()).unwrap();
        let e = find(&entries, "album");
        let listing = e.listing.as_ref().expect("index/ forces a listing");
        assert!(listing.collision.is_empty(), "index/ silences the collision notice");
        assert!(listing.intro.is_some(), "index.md is the intro prose");
        assert_eq!(listing.items.len(), 1, "only the public non-intro file lists");
        assert_eq!(listing.items[0].name, "photo.jpg");
        assert!(listing.is_gallery(), "a lone image -> gallery");
        assert_eq!(e.display_label.as_deref(), Some("My Album"), "title from the intro H1");
    }

    #[test]
    fn listing_membership_is_a_public_allowlist() {
        // Only `public` files list; an untagged sibling is excluded but counted in
        // the total so the reader can tell something is withheld.
        let t = TmpDir::new();
        touch(t.path(), "gal/a.jpg", "1");
        touch(t.path(), "gal/b.jpg", "2");
        touch(t.path(), "gal/secret.jpg", "3");
        if !set_tags(&t.path().join("gal/a.jpg"), &["public"]) {
            return; // xattr unsupported — skip
        }
        assert!(set_tags(&t.path().join("gal/b.jpg"), &["public"]));
        // secret.jpg stays untagged → never listed.
        let entries = scan_entries(t.path()).unwrap();
        let e = find(&entries, "gal");
        let listing = e.listing.as_ref().expect("no primary -> listing");
        assert_eq!(listing.total, 3, "all candidate files counted");
        assert_eq!(listing.items.len(), 2, "only the two public files list");
        assert!(listing.items.iter().all(|i| i.name != "secret.jpg"));
        assert!(listing.is_gallery(), "all-image listing -> gallery");
    }

    #[test]
    fn untagged_primary_is_withheld_and_demotes_to_listing() {
        // The folder tag makes the post reachable; only the file's own `public`
        // tag serves its content. An untagged would-be primary is withheld and
        // the post lists its public files instead.
        let t = TmpDir::new();
        touch(t.path(), "essay/essay.md", "not tagged -> never served");
        touch(t.path(), "essay/appendix.pdf", "pdf");
        if !set_tags(&t.path().join("essay/appendix.pdf"), &["public"]) {
            return; // xattr unsupported — skip
        }
        let entries = scan_entries(t.path()).unwrap();
        let e = find(&entries, "essay");
        assert!(e.error.is_none(), "withholding is not an error state");
        let listing = e.listing.as_ref().expect("untagged primary -> listing");
        assert!(listing.intro.is_none(), "the withheld document is no intro either");
        assert_eq!(listing.total, 2, "the withheld primary still counts as withheld");
        let names: Vec<&str> = listing.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, vec!["appendix.pdf"], "only the public sibling lists");
    }

    #[test]
    fn untagged_lone_file_is_withheld() {
        // A public folder with one untagged file publishes nothing of the file:
        // the post is an empty listing with a visible withheld count.
        let t = TmpDir::new();
        touch(t.path(), "solo/secret.txt", "never served");
        // No tag on secret.txt at all. (Folder tags are read at the store gate,
        // not in scan_entries, so none is needed here.)
        let entries = scan_entries(t.path()).unwrap();
        let e = find(&entries, "solo");
        let listing = e.listing.as_ref().expect("untagged lone file -> empty listing");
        assert_eq!(listing.total, 1, "the reader can tell something is withheld");
        assert!(listing.items.is_empty(), "the untagged file never lists");
    }

    #[test]
    fn untagged_intro_is_not_used() {
        // An `index/`-forced listing only renders its intro document when that
        // document itself is tagged `public`.
        let t = TmpDir::new();
        touch(t.path(), "album/index.md", "# Story\n\nnot tagged -> withheld");
        touch(t.path(), "album/photo.jpg", "img");
        mkdir(t.path(), "album/index");
        if !set_tags(&t.path().join("album/photo.jpg"), &["public"]) {
            return; // xattr unsupported — skip
        }
        let entries = scan_entries(t.path()).unwrap();
        let e = find(&entries, "album");
        let listing = e.listing.as_ref().expect("index/ forces a listing");
        assert!(listing.intro.is_none(), "an untagged intro is withheld");
        assert!(e.display_label.is_none(), "no title read from withheld content");
        let names: Vec<&str> = listing.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, vec!["photo.jpg"], "the untagged document never lists");
    }

    #[test]
    fn untagged_snapshots_are_not_revisions() {
        // ` copy [n]` snapshots (in-folder and top-level) serve content, so they
        // need their own `public` tag too.
        let t = TmpDir::new();
        touch(t.path(), "post/post.md", "current");
        touch(t.path(), "post/post copy.md", "untagged in-folder snapshot");
        touch(t.path(), "note.md", "current");
        touch(t.path(), "note copy.md", "untagged top-level snapshot");
        if !set_tags(&t.path().join("post/post.md"), &["public"]) {
            return; // xattr unsupported — skip
        }
        let entries = scan_entries(t.path()).unwrap();
        assert!(find(&entries, "post").revisions.is_empty(), "in-folder copy withheld");
        assert!(find(&entries, "note").revisions.is_empty(), "top-level copy withheld");
    }

    #[test]
    fn untagged_link_destination_is_no_cite() {
        // The cite publishes the destination file's content (its URL), so an
        // untagged `link.*` sidecar yields no cite and stays withheld.
        let t = TmpDir::new();
        touch(t.path(), "narrow/index.md", "# Narrow\n\ncommentary");
        touch(t.path(), "narrow/link.webloc", &webloc("https://example.com/secret-source"));
        if !set_tags(&t.path().join("narrow/index.md"), &["public"]) {
            return; // xattr unsupported — skip
        }
        let entries = scan_entries(t.path()).unwrap();
        let e = find(&entries, "narrow");
        assert!(e.link_url.is_none(), "an untagged destination publishes no URL");
        assert!(e.attachments.iter().all(|a| a.name != "link.webloc"), "and never lists");
    }

    #[test]
    fn mixed_media_listing_is_a_file_list_not_a_gallery() {
        let t = TmpDir::new();
        touch(t.path(), "kit/logo.png", "img");
        touch(t.path(), "kit/bio.pdf", "doc");
        if !set_tags(&t.path().join("kit/logo.png"), &["public"]) {
            return;
        }
        assert!(set_tags(&t.path().join("kit/bio.pdf"), &["public"]));
        let entries = scan_entries(t.path()).unwrap();
        let e = find(&entries, "kit");
        let listing = e.listing.as_ref().expect("no primary -> listing");
        assert_eq!(listing.items.len(), 2);
        assert!(!listing.is_gallery(), "a non-image present -> file list");
    }

    #[test]
    fn document_post_lists_public_sibling_attachments() {
        // A doc post (primary `resume.md`) with public siblings gets them as
        // attachments, filename-ordered; an untagged sibling is excluded.
        let t = TmpDir::new();
        touch(t.path(), "resume/resume.md", "# CV\n\nthe body");
        touch(t.path(), "resume/cv.pdf", "pdf");
        touch(t.path(), "resume/refs.pdf", "pdf");
        touch(t.path(), "resume/draft.txt", "not public");
        if !set_tags(&t.path().join("resume/cv.pdf"), &["public"]) {
            return; // xattr unsupported — skip
        }
        assert!(set_tags(&t.path().join("resume/refs.pdf"), &["public"]));
        assert!(set_tags(&t.path().join("resume/resume.md"), &["public"])); // the primary needs its own tag
        let entries = scan_entries(t.path()).unwrap();
        let e = find(&entries, "resume");
        assert!(e.error.is_none() && e.listing.is_none(), "still a document post");
        assert!(e.path.ends_with("resume.md"), "primary is the body");
        let names: Vec<&str> = e.attachments.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["cv.pdf", "refs.pdf"], "public siblings, filename order");
        assert!(!names.contains(&"resume.md"), "the primary is not an attachment");
        assert!(!names.contains(&"draft.txt"), "untagged sibling excluded");
    }

    #[test]
    fn listing_includes_public_subfolders_as_rows_untagged_excluded() {
        let t = TmpDir::new();
        touch(t.path(), "gal/a.jpg", "1");
        touch(t.path(), "gal/b.jpg", "2");
        touch(t.path(), "gal/extra/c.jpg", "3");
        touch(t.path(), "gal/hidden/d.jpg", "4");
        if !set_tags(&t.path().join("gal/a.jpg"), &["public"]) {
            return; // xattr unsupported — skip
        }
        assert!(set_tags(&t.path().join("gal/b.jpg"), &["public"]));
        assert!(set_tags(&t.path().join("gal/extra"), &["public"]));
        // gal/hidden left untagged → never a row.
        let entries = scan_entries(t.path()).unwrap();
        let e = find(&entries, "gal");
        let listing = e.listing.as_ref().expect("no primary -> listing");
        let dirs: Vec<&str> = listing.items.iter().filter(|i| i.is_dir).map(|i| i.name.as_str()).collect();
        assert_eq!(dirs, vec!["extra"], "only the public subfolder rows; hidden excluded");
        assert!(listing.items[0].is_dir, "folders sort before files");
        assert!(!listing.is_gallery(), "a folder row present -> file list, not gallery");
    }

    #[test]
    fn folder_of_only_public_subfolders_is_a_listing_not_error() {
        let t = TmpDir::new();
        touch(t.path(), "photos/travel/x.jpg", "x");
        touch(t.path(), "photos/food/y.jpg", "y");
        if !set_tags(&t.path().join("photos/travel"), &["public"]) {
            return; // xattr unsupported — skip
        }
        assert!(set_tags(&t.path().join("photos/food"), &["public"]));
        let entries = scan_entries(t.path()).unwrap();
        let e = find(&entries, "photos");
        assert!(e.error.is_none(), "subfolders-only is a listing, not NoPrimary");
        let listing = e.listing.as_ref().expect("subfolders-only -> listing");
        let names: Vec<&str> = listing.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, vec!["food", "travel"], "both subfolders, filename order");
        assert!(listing.items.iter().all(|i| i.is_dir));
    }

    #[test]
    fn folder_post_empty_is_no_primary() {
        let t = TmpDir::new();
        mkdir(t.path(), "empty");
        let entries = scan_entries(t.path()).unwrap();
        let e = find(&entries, "empty");
        assert!(matches!(e.error, Some(PostError::NoPrimary)));
    }

    #[test]
    fn folder_post_two_date_markers_error() {
        let t = TmpDir::new();
        touch(t.path(), "p/p.md", "a");
        mkdir(t.path(), "p/2026-03-03T1430");
        mkdir(t.path(), "p/2026-03-04T1200");
        let entries = scan_entries(t.path()).unwrap();
        let e = find(&entries, "p");
        assert!(matches!(e.error, Some(PostError::MultipleDateMarkers(_))));
    }

    #[test]
    fn alias_marker_recorded() {
        let t = TmpDir::new();
        touch(t.path(), "cv/cv.md", "resume");
        mkdir(t.path(), "cv/alias resume");
        let entries = scan_entries(t.path()).unwrap();
        let e = find(&entries, "cv");
        assert!(e.error.is_none());
        assert_eq!(e.aliases, vec!["resume".to_string()]);
    }

    #[test]
    fn non_empty_date_named_subfolder_is_not_a_marker() {
        let t = TmpDir::new();
        touch(t.path(), "p/p.md", "a");
        mkdir(t.path(), "p/2026-03-03T1430");
        touch(t.path(), "p/2026-03-03T1430/stray.txt", "x");
        let entries = scan_entries(t.path()).unwrap();
        let e = find(&entries, "p");
        // Not a valid marker → no error, and no date marker → no edited line.
        assert!(e.error.is_none());
        assert!(e.edited.is_none());
    }

    #[test]
    fn folder_copy_is_a_revision_not_a_post() {
        let t = TmpDir::new();
        touch(t.path(), "post/post.md", "current");
        mkdir(t.path(), "post/2026-03-03T1430");
        touch(t.path(), "post copy/post.md", "archived");
        if !set_tags(&t.path().join("post/post.md"), &["public"]) {
            return; // xattr unsupported — skip
        }
        assert!(set_tags(&t.path().join("post copy"), &["public"]));
        assert!(set_tags(&t.path().join("post copy/post.md"), &["public"]));
        let entries = scan_entries(t.path()).unwrap();
        assert_eq!(entries.len(), 1, "the copy must not be its own post");
        let e = find(&entries, "post");
        assert_eq!(e.revisions.len(), 1);
        assert!(e.revisions[0].path.ends_with("post.md"));
    }

    #[test]
    fn file_snapshot_inside_folder_is_a_revision() {
        let t = TmpDir::new();
        touch(t.path(), "post/post.md", "current");
        touch(t.path(), "post/post copy.md", "older");
        if !set_tags(&t.path().join("post/post.md"), &["public"]) {
            return; // xattr unsupported — skip
        }
        assert!(set_tags(&t.path().join("post/post copy.md"), &["public"]));
        let entries = scan_entries(t.path()).unwrap();
        assert_eq!(entries.len(), 1);
        let e = find(&entries, "post");
        assert!(e.path.ends_with("post.md"));
        assert!(!e.path.to_string_lossy().contains("copy"));
        assert_eq!(e.revisions.len(), 1);
    }

    #[test]
    fn two_file_snapshots_carry_both_ranks() {
        let t = TmpDir::new();
        touch(t.path(), "post/post.md", "current");
        touch(t.path(), "post/post copy.md", "rev1");
        touch(t.path(), "post/post copy 2.md", "rev2");
        if !set_tags(&t.path().join("post/post.md"), &["public"]) {
            return; // xattr unsupported — skip
        }
        assert!(set_tags(&t.path().join("post/post copy.md"), &["public"]));
        assert!(set_tags(&t.path().join("post/post copy 2.md"), &["public"]));
        let entries = scan_entries(t.path()).unwrap();
        let e = find(&entries, "post");
        assert_eq!(e.revisions.len(), 2);
        let mut ranks: Vec<u32> = e.revisions.iter().map(|r| r.rank).collect();
        ranks.sort();
        assert_eq!(ranks, vec![1, 2]);
    }

    #[test]
    fn bare_file_copy_attaches_to_bare_post() {
        let t = TmpDir::new();
        touch(t.path(), "note.md", "current");
        touch(t.path(), "note copy.md", "archived");
        if !set_tags(&t.path().join("note copy.md"), &["public"]) {
            return; // xattr unsupported — skip
        }
        let entries = scan_entries(t.path()).unwrap();
        assert_eq!(entries.len(), 1);
        let e = find(&entries, "note");
        assert_eq!(e.revisions.len(), 1);
    }

    #[test]
    fn lone_orphan_copy_becomes_a_standalone_post() {
        // A ` copy` with no base is a real post now (family of one), not dropped.
        let t = TmpDir::new();
        touch(t.path(), "draft copy.md", "the only one");
        let entries = scan_entries(t.path()).unwrap();
        assert_eq!(entries.len(), 1, "the orphan must not vanish");
        let e = &entries[0];
        // It claims the base name/slug — so `/draft` keeps resolving.
        assert_eq!(e.label.as_deref(), Some("draft"));
        assert_eq!(e.slug.as_deref(), Some("draft"));
        assert!(e.revisions.is_empty());
        assert!(e.error.is_none());
        assert!(e.path.ends_with("draft copy.md"), "serves the copy's bytes");
    }

    #[test]
    fn orphan_copies_promote_newest_and_keep_the_rest() {
        // No `draft` base: the newest copy promotes; older copies stay as its
        // revisions. Nothing is dropped. The rank breaks any same-second mtime tie
        // (a higher copy number is the newer one), so `draft copy 2` wins.
        let t = TmpDir::new();
        touch(t.path(), "draft copy.md", "older");
        touch(t.path(), "draft copy 2.md", "newer");
        if !set_tags(&t.path().join("draft copy.md"), &["public"]) {
            return; // xattr unsupported — skip
        }
        let entries = scan_entries(t.path()).unwrap();
        assert_eq!(entries.len(), 1, "one family row, not two posts");
        let e = find(&entries, "draft");
        assert_eq!(e.slug.as_deref(), Some("draft"), "recovery: claims the base slug");
        assert_eq!(e.revisions.len(), 1, "the older copy is a revision, not gone");
        let body = std::fs::read_to_string(&e.path).unwrap();
        assert_eq!(body, "newer", "the newest copy is the current post");
    }

    #[test]
    fn reappearing_base_absorbs_all_copies() {
        // Create `draft` (the base): both copies become its revisions and nothing
        // promotes — the family collapses to one current post + a two-deep stack.
        let t = TmpDir::new();
        touch(t.path(), "draft.md", "the real base");
        touch(t.path(), "draft copy.md", "older");
        touch(t.path(), "draft copy 2.md", "newer");
        if !set_tags(&t.path().join("draft copy.md"), &["public"]) {
            return; // xattr unsupported — skip
        }
        assert!(set_tags(&t.path().join("draft copy 2.md"), &["public"]));
        let entries = scan_entries(t.path()).unwrap();
        assert_eq!(entries.len(), 1, "the base absorbs the copies");
        let e = find(&entries, "draft");
        let body = std::fs::read_to_string(&e.path).unwrap();
        assert_eq!(body, "the real base", "the base is the current post");
        assert_eq!(e.revisions.len(), 2);
    }

    #[test]
    fn folder_orphan_copy_promotes_to_a_folder_post() {
        // A `<base> copy/` folder with no `<base>/`: Cmd-D renamed only the folder,
        // so its inner primary is still `draft.md`. It promotes to a folder post
        // that claims `draft` and resolves that inner file as its primary.
        let t = TmpDir::new();
        touch(t.path(), "draft copy/draft.md", "archived-turned-current");
        if !set_tags(&t.path().join("draft copy/draft.md"), &["public"]) {
            return; // xattr unsupported — skip
        }
        let entries = scan_entries(t.path()).unwrap();
        assert_eq!(entries.len(), 1);
        let e = find(&entries, "draft");
        assert_eq!(e.slug.as_deref(), Some("draft"));
        assert!(e.dir.is_some(), "it is a folder post");
        assert!(e.path.ends_with("draft.md"), "inner primary resolved against the base");
        assert!(e.error.is_none());
        assert!(e.revisions.is_empty());
    }

    #[test]
    fn spaced_name_publishes_with_a_slug() {
        let t = TmpDir::new();
        touch(t.path(), "Fog Over The Bay.md", "natural filename");
        let entries = scan_entries(t.path()).unwrap();
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        // Natural macOS filenames publish now; the address is the derived slug.
        assert!(e.error.is_none());
        assert_eq!(e.label.as_deref(), Some("Fog Over The Bay"));
        assert_eq!(e.slug.as_deref(), Some("fog-over-the-bay"));
    }

    #[test]
    fn embed_cache_dir_is_ignored() {
        let t = TmpDir::new();
        touch(t.path(), "x.link", "https://example.com");
        touch(t.path(), "x.link.embed-cache/meta.json", "{}");
        let entries = scan_entries(t.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].label.as_deref(), Some("x"));
        assert_eq!(entries[0].extension, "link");
    }

    // ── link axis: destinations & cites (post-model.md §4) ──

    /// A macOS `.webloc` bookmark: an XML plist with a single `URL` key.
    fn webloc(url: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>URL</key><string>{}</string></dict></plist>"#,
            url
        )
    }

    #[test]
    fn bare_webloc_is_a_link_post() {
        let t = TmpDir::new();
        touch(t.path(), "worth-saving.webloc", &webloc("https://github.com/rust-lang/rust"));
        let entries = scan_entries(t.path()).unwrap();
        let e = find(&entries, "worth-saving");
        assert_eq!(e.kind(), "link");
        assert_eq!(e.link_url.as_deref(), Some("https://github.com/rust-lang/rust"));
    }

    #[test]
    fn single_url_text_is_a_link_but_a_sentence_is_not() {
        let t = TmpDir::new();
        touch(t.path(), "read-this.txt", "https://example.com/article");
        touch(t.path(), "note.txt", "read this https://example.com/article");
        let entries = scan_entries(t.path()).unwrap();

        let link = find(&entries, "read-this");
        assert_eq!(link.kind(), "link");
        assert_eq!(link.link_url.as_deref(), Some("https://example.com/article"));

        // A title line plus a URL is a note *with* a link, not a link.
        let note = find(&entries, "note");
        assert!(note.link_url.is_none());
        assert_ne!(note.kind(), "link");
    }

    #[test]
    fn unsafe_scheme_is_never_a_link() {
        // A `javascript:` (or `data:`/`file:`) destination is refused at scan time —
        // link_url stays None and the file is an ordinary post, never a clickable link.
        let t = TmpDir::new();
        touch(t.path(), "sneaky.webloc", &webloc("javascript:alert(document.cookie)"));
        let entries = scan_entries(t.path()).unwrap();
        let e = find(&entries, "sneaky");
        assert!(e.link_url.is_none(), "javascript: must be refused");
        assert_ne!(e.kind(), "link");
    }

    #[test]
    fn folder_with_link_sidecar_cites_but_keeps_its_medium() {
        // `narrow/index.md` (the commentary, primary) + `narrow/link.webloc` (the
        // cited destination): the folder post carries the link_url but stays a text
        // post, and the destination file drops out of the attachment list.
        let t = TmpDir::new();
        touch(t.path(), "narrow/index.md", "# Narrow streets\n\nMy take on road diets.");
        touch(t.path(), "narrow/link.webloc", &webloc("https://nytimes.com/road-diets"));
        if !set_tags(&t.path().join("narrow/index.md"), &["public"]) {
            return; // xattr unsupported — skip
        }
        assert!(set_tags(&t.path().join("narrow/link.webloc"), &["public"])); // the cite publishes the URL
        let entries = scan_entries(t.path()).unwrap();
        let e = find(&entries, "narrow");
        assert_eq!(e.link_url.as_deref(), Some("https://nytimes.com/road-diets"));
        assert_ne!(e.kind(), "link", "a content post that cites keeps its own medium");
        assert!(
            e.attachments.iter().all(|a| a.name != "link.webloc"),
            "the cited destination is not also listed as an attachment"
        );
    }

    #[test]
    fn cite_intent_is_format_or_name_never_content() {
        // A bookmark format cites under any name (the format is the intent);
        // a general text file holding one URL does NOT cite unless named
        // `link` — it stays an ordinary attachment (role never content-sniffed).
        let t = TmpDir::new();
        touch(t.path(), "roads/roads.md", "# Roads\n\ncommentary");
        touch(t.path(), "roads/Road Diets - NYT.webloc", &webloc("https://nytimes.com/road-diets"));
        touch(t.path(), "cafes/cafes.md", "# Cafes\n\ncommentary");
        touch(t.path(), "cafes/source.txt", "https://example.com/cafes");
        if !set_tags(&t.path().join("roads/roads.md"), &["public"]) {
            return; // xattr unsupported — skip
        }
        assert!(set_tags(&t.path().join("roads/Road Diets - NYT.webloc"), &["public"]));
        assert!(set_tags(&t.path().join("cafes/cafes.md"), &["public"]));
        assert!(set_tags(&t.path().join("cafes/source.txt"), &["public"]));
        let entries = scan_entries(t.path()).unwrap();

        let roads = find(&entries, "roads");
        assert_eq!(roads.link_url.as_deref(), Some("https://nytimes.com/road-diets"));
        assert!(roads.attachments.iter().all(|a| !a.name.ends_with(".webloc")));

        let cafes = find(&entries, "cafes");
        assert!(cafes.link_url.is_none(), "a text file only cites when named `link`");
        assert!(
            cafes.attachments.iter().any(|a| a.name == "source.txt"),
            "the URL-holding text file stays an ordinary attachment"
        );
    }

    #[test]
    fn link_named_text_file_cites() {
        // `link.txt` (and `link.text` via normalize_ext) carries the intent in
        // the name, so it cites like a bookmark would.
        let t = TmpDir::new();
        touch(t.path(), "narrow/narrow.md", "# Narrow\n\ncommentary");
        touch(t.path(), "narrow/link.txt", "https://example.com/article");
        if !set_tags(&t.path().join("narrow/narrow.md"), &["public"]) {
            return; // xattr unsupported — skip
        }
        assert!(set_tags(&t.path().join("narrow/link.txt"), &["public"]));
        let entries = scan_entries(t.path()).unwrap();
        let e = find(&entries, "narrow");
        assert_eq!(e.link_url.as_deref(), Some("https://example.com/article"));
        assert!(e.attachments.iter().all(|a| a.name != "link.txt"));
    }

    // ── date-named posts & future-hold ──

    #[test]
    fn whole_date_name_is_unlabeled_and_dated() {
        use crate::postdate::Precision;
        let t = TmpDir::new();
        touch(t.path(), "2026-07-04.md", "body");
        let entries = scan_entries(t.path()).unwrap();
        let e = &entries[0];
        assert!(e.label.is_none(), "a whole-date name is unlabeled");
        assert!(e.slug.is_none(), "and never claims a bare URL");
        assert_eq!(e.timestamp.precision, Precision::Day);
        assert_eq!(e.timestamp.short_date(), "2026-07-04");
    }

    #[test]
    fn date_prefixed_name_shows_title_but_claims_no_slug() {
        let t = TmpDir::new();
        touch(t.path(), "1980-08-11 that thing about stuff.md", "no heading here");
        let entries = scan_entries(t.path()).unwrap();
        let e = &entries[0];
        // Date-addressed; the trailing text is a display title, never a slug.
        assert!(e.slug.is_none());
        assert_eq!(e.timestamp.short_date(), "1980-08-11");
        assert_eq!(e.display_label.as_deref(), Some("that thing about stuff"));
    }

    #[test]
    fn h1_overrides_filename_for_display_title() {
        let t = TmpDir::new();
        touch(t.path(), "my-trip.md", "# Summer in Rome\n\nWe went south.");
        let entries = scan_entries(t.path()).unwrap();
        let e = &entries[0];
        // Identity stays the filename; the display title is the H1.
        assert_eq!(e.label.as_deref(), Some("my-trip"));
        assert_eq!(e.slug.as_deref(), Some("my-trip"));
        assert_eq!(e.display_label.as_deref(), Some("Summer in Rome"));
    }

    #[test]
    fn future_dated_post_is_held() {
        let t = TmpDir::new();
        touch(t.path(), "2999-01-01.md", "from the future");
        if !set_tags(&t.path().join("2999-01-01.md"), &["public"]) {
            return; // xattr unsupported — skip
        }
        let store = ContentStore::scan(t.path(), &std::env::temp_dir()).unwrap();
        assert!(store.entries.is_empty(), "a future-dated post is withheld until its date");
        assert!(store.next_future.is_some(), "and a wake is scheduled for it");
    }

    // ── visibility gate (fail-closed) ──

    #[test]
    fn visibility_untagged_and_private_are_withheld() {
        let t = TmpDir::new();
        touch(t.path(), "shown.md", "public body");
        touch(t.path(), "untagged.md", "no tag → hidden");
        touch(t.path(), "secret.md", "public + private → private wins");
        if !set_tags(&t.path().join("shown.md"), &["public"]) {
            return; // xattr unsupported — skip
        }
        // `private` beats `public` (deny-wins), so `secret` is still withheld.
        assert!(set_tags(&t.path().join("secret.md"), &["public", "private"]));
        let store = ContentStore::scan(t.path(), &std::env::temp_dir()).unwrap();
        let labels: Vec<&str> = store.entries.iter().filter_map(|e| e.label.as_deref()).collect();
        assert_eq!(labels, vec!["shown"], "only the public, non-private post serves");
    }

    #[test]
    fn private_folder_withholds_even_a_public_primary() {
        let t = TmpDir::new();
        touch(t.path(), "album/album.md", "body");
        if !set_tags(&t.path().join("album"), &["private"]) {
            return; // xattr unsupported — skip
        }
        // Tagging the inner primary public cannot rescue a private folder post.
        let _ = set_tags(&t.path().join("album/album.md"), &["public"]);
        let store = ContentStore::scan(t.path(), &std::env::temp_dir()).unwrap();
        assert!(store.entries.is_empty(), "a private post is never served");
    }

    #[test]
    fn path_visible_needs_every_component_public() {
        let t = TmpDir::new();
        touch(t.path(), "album/sub/pic.jpg", "bytes");
        let root = t.path();
        let pic = root.join("album/sub/pic.jpg");
        if !set_tags(&root.join("album"), &["public"]) {
            return; // xattr unsupported — skip
        }
        // A gap anywhere on the chain (sub/ untagged) fails closed.
        assert!(!crate::tags::path_visible(root, &pic));
        assert!(set_tags(&root.join("album/sub"), &["public"]));
        assert!(!crate::tags::path_visible(root, &pic), "the file itself still needs `public`");
        assert!(set_tags(&pic, &["public"]));
        assert!(crate::tags::path_visible(root, &pic), "every component public → served");
        // `private` on the file wins even with a fully public chain.
        assert!(set_tags(&pic, &["public", "private"]));
        assert!(!crate::tags::path_visible(root, &pic));
    }

    #[test]
    fn dotfiles_and_missing_dir() {
        // Missing content dir → empty, no error.
        let missing = std::env::temp_dir().join(format!("sajt-missing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&missing);
        assert!(scan_entries(&missing).unwrap().is_empty());

        // Dotfiles are skipped.
        let t = TmpDir::new();
        touch(t.path(), ".DS_Store", "junk");
        touch(t.path(), "real.md", "hi");
        let entries = scan_entries(t.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].label.as_deref(), Some("real"));
    }
}
