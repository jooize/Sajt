//! The page layer: every URL of the site as a pure function of the content
//! store — `respond(store, path, flags)` returns the finished HTTP-shaped
//! reply with no web-framework types anywhere. crates/serve wraps this in
//! axum; crates/build walks the URL space and writes the same bytes to files
//! (sajt.md: because both link this exact code, static output cannot
//! diverge from the preview).

use crate::content::ContentStore;
use crate::entry::{Entry, Revision};
use crate::render::{render_entry, RenderedContent};
use crate::stats::{compute_cloud, ViewFilter};
use crate::templates::{self, HeaderContext};
use crate::url::{parse_url_path, ContentQuery};

/// Where a reply's bytes came from — the record the build's provenance
/// manifest is made of (sajt.md: "the build emits a provenance manifest
/// naming, for every file it intends to upload, the source path and the
/// specific `public` tag that authorized it, and refuses to upload any file
/// without that provenance").
#[derive(Debug, Clone, PartialEq)]
pub enum Provenance {
    /// Template output over already-public metadata (a timeline, a listing,
    /// an explainer, the 404 shell) — no single content file's bytes.
    Generated,
    /// The content (or a derived rendition) of one specific file, published
    /// under the named tag on that file itself.
    File {
        source: std::path::PathBuf,
        tag: &'static str,
    },
}

/// A finished reply, framework-free: status, headers (lowercase names, valid
/// HTTP values), body, and the provenance of the bytes. The one shape every
/// page, byte stream, redirect, and error takes on its way to axum, a file on
/// disk, or stdout.
#[derive(Debug)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(&'static str, String)>,
    pub body: Vec<u8>,
    pub provenance: Provenance,
}

impl Reply {
    /// A rendered HTML page (200) — template output, `Generated` provenance.
    pub fn html(body: String) -> Self {
        Self::html_status(200, body)
    }

    /// A rendered HTML page with an explicit status (404 page, 500 error page).
    pub fn html_status(status: u16, body: String) -> Self {
        Reply {
            status,
            headers: vec![("content-type", "text/html; charset=utf-8".to_string())],
            body: body.into_bytes(),
            provenance: Provenance::Generated,
        }
    }

    /// A 301 to `location` (an encoded canonical address).
    pub fn redirect(location: &str) -> Self {
        Reply {
            status: 301,
            headers: vec![("location", location.to_string())],
            body: Vec::new(),
            provenance: Provenance::Generated,
        }
    }

    /// A plain-text reply with an explicit status.
    pub fn text(status: u16, body: &str) -> Self {
        Reply {
            status,
            headers: vec![("content-type", "text/plain; charset=utf-8".to_string())],
            body: body.as_bytes().to_vec(),
            provenance: Provenance::Generated,
        }
    }

    /// Stamp this reply as carrying one specific file's (possibly derived)
    /// content, authorized by the named tag on that file.
    fn from_file(mut self, source: &std::path::Path, tag: &'static str) -> Self {
        self.provenance = Provenance::File {
            source: source.to_path_buf(),
            tag,
        };
        self
    }

    /// The redirect target, when this reply is one.
    pub fn location(&self) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| *n == "location")
            .map(|(_, v)| v.as_str())
    }
}

/// The request inputs that are not the path: the standalone-document view
/// selectors (`?embed`, `?fullscreen`) and the one query filter (`?search=`).
#[derive(Debug, Default, Clone)]
pub struct RequestFlags {
    pub embed: bool,
    pub fullscreen: bool,
    pub search: Option<String>,
}

/// Assemble the view filter: grade/favorites come off the parsed path (the
/// reserved `/notable` and `/favorites` segments), search off the query.
/// Query spellings of grade/favorites are not accepted — not even as
/// redirect inputs (sajt.md: no legacy to absorb).
fn view_of(query: &ContentQuery, flags: &RequestFlags) -> ViewFilter {
    ViewFilter::new(query.notable, query.favorites, flags.search.as_deref())
}

/// Human-readable filter description for the page title.
fn filter_title(path_desc: &str, view: &ViewFilter, saved: bool) -> String {
    let mut parts: Vec<String> = Vec::new();
    if saved {
        parts.push("Saved".to_string());
    }
    if !path_desc.is_empty() {
        parts.push(path_desc.to_string());
    }
    if view.notable {
        parts.push(view.grade_word().to_string());
    }
    if view.fav {
        parts.push("favorites".to_string());
    }
    if let Some(ref q) = view.q {
        parts.push(format!("\u{201c}{}\u{201d}", q));
    }
    parts.join(" · ")
}

/// The single active topic, if the path filters to exactly one tag.
fn active_tag(query: &ContentQuery) -> Option<&str> {
    if query.and_tags.len() == 1
        && query.or_tags.is_empty()
        && query.date_prefix.is_none()
        && query.label.is_none()
    {
        Some(query.and_tags[0].as_str())
    } else {
        None
    }
}

/// The complete URL space as one dispatcher, mirroring the server's router:
/// `/static/…` and `/_embed/…` go to their dedicated resolvers, everything
/// else to `respond`. The closure builder walks the site through this one
/// function, so what it emits is byte-for-byte what the preview serves.
pub async fn route(
    store: &ContentStore,
    static_dir: &std::path::Path,
    path: &str,
    flags: &RequestFlags,
) -> Reply {
    let bare = path.trim_start_matches('/');
    if let Some(rest) = bare.strip_prefix("static/") {
        return static_asset(static_dir, rest);
    }
    if let Some(rest) = bare.strip_prefix("_embed/") {
        return match rest.split_once('/') {
            Some((key, name)) if !name.contains('/') => embed_asset(store, key, name),
            _ => not_found(),
        };
    }
    respond(store, path, flags).await
}

/// The fallback 404 page — the shell every out-of-closure URL lands on. The
/// builder ships it as the static host's custom error page (rung 4).
pub fn fallback_404() -> Reply {
    not_found()
}

/// The whole site as one function: the decoded request `path` (leading slash
/// optional) plus `flags` resolve against the store to a finished `Reply`.
/// Every route — timeline, scopes, posts, raw files, renditions, assets,
/// saved — goes through here; axum and the closure builder are both callers.
pub async fn respond(store: &ContentStore, path: &str, flags: &RequestFlags) -> Reply {
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        // The root timeline (no scope, no view — `/notable` etc. arrive as
        // scope paths below).
        let view = ViewFilter::new(false, false, flags.search.as_deref());
        return Reply::html(timeline_root(store, &view));
    }
    if path == "saved" {
        let view = ViewFilter::new(false, false, flags.search.as_deref());
        return Reply::html(saved_page(store, &view));
    }
    resolve(store, path, flags).await
}

/// The unfiltered root timeline.
fn timeline_root(store: &ContentStore, view: &ViewFilter) -> String {
    let all: Vec<&Entry> = store.entries.iter().collect();
    let display: Vec<&Entry> = all.iter().copied().filter(|e| view.matches(e)).collect();

    let cloud = compute_cloud(&all);
    let ctx = HeaderContext {
        cloud: &cloud,
        active_tag: None,
        view,
        base_path: "/",
        date_scope: None,
        path_tags: "",
        saved_view: false,
    };
    let title = filter_title("", view, false);
    templates::timeline_page(&display, &all, &ctx, &title)
}

/// Render the saved-bookmarks page for a view (a client-side view: the page
/// carries the full timeline and the browser filters to what it has saved).
/// Serves the bare `/saved` route and the view-suffixed paths
/// (`/saved/notable`, …).
fn saved_page(store: &ContentStore, view: &ViewFilter) -> String {
    let all: Vec<&Entry> = store.entries.iter().collect();
    let display: Vec<&Entry> = all.iter().copied().filter(|e| view.matches(e)).collect();

    let cloud = compute_cloud(&all);
    let ctx = HeaderContext {
        cloud: &cloud,
        active_tag: None,
        view,
        base_path: "/saved",
        date_scope: None,
        path_tags: "",
        saved_view: true,
    };
    let title = filter_title("", view, true);
    templates::timeline_page(&display, &all, &ctx, &title)
}

/// Static assets (`/static/{path}`) with aggressive caching. Generated assets
/// (site.css / site.js / boot.js) come from the compile-time consts, not from
/// disk; their URLs carry a content-hash `?v=` token, so `immutable` is safe.
/// `static_dir` is the on-disk directory for the rest (fonts, images).
pub fn static_asset(static_dir: &std::path::Path, path: &str) -> Reply {
    if let Some(asset) = crate::assets::get(path) {
        return Reply {
            status: 200,
            headers: vec![
                ("content-type", asset.mime().to_string()),
                ("cache-control", "public, max-age=31536000, immutable".to_string()),
            ],
            body: asset.body().to_string().into_bytes(),
            provenance: Provenance::Generated,
        };
    }

    // Restrict to known safe filenames (no path traversal)
    let safe: bool = path
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
    if !safe || path.contains("..") || path.starts_with('.') {
        return not_found();
    }

    // The same byte-for-byte rule as a folder asset: the address is the
    // directory entry's name on every platform, never a case-folded spelling
    // that only APFS would answer (the build emits the on-disk name).
    let Some(file_path) = exact_path(static_dir, &[path]) else {
        return not_found();
    };
    let content = match std::fs::read(&file_path) {
        Ok(c) => c,
        Err(_) => return not_found(),
    };

    let ext = file_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let mime = mime_guess::from_ext(ext)
        .first_or_octet_stream()
        .to_string();

    Reply {
        status: 200,
        headers: vec![
            ("content-type", mime),
            ("cache-control", "public, max-age=31536000, immutable".to_string()),
        ],
        body: content,
        provenance: Provenance::Generated,
    }
}

/// Resolve every non-root, non-bare-`/saved` path against the store.
async fn resolve(store: &ContentStore, path: &str, flags: &RequestFlags) -> Reply {
    // Dot segments never appear in a canonical address; reject them outright so
    // no crafted `..`/`.` path can reach resolution.
    if path.split('/').any(|s| s == "." || s == "..") {
        return not_found();
    }

    // `/saved` with a view suffix (`/saved/notable`, …) or a stray trailing
    // slash. `saved` is a reserved word like the view segments: it never
    // resolves as a post name, so nothing here shadows content.
    if path == "saved/" || path.starts_with("saved/") {
        let rest = &path["saved".len()..];
        let q = parse_url_path(rest);
        // Only view words may follow /saved — anything else is a miss.
        if q.malformed
            || q.date_prefix.is_some()
            || !q.and_tags.is_empty()
            || !q.or_tags.is_empty()
            || q.label.is_some()
            || q.time.is_some()
            || q.raw_extension.is_some()
        {
            return not_found();
        }
        let view = view_of(&q, flags);
        let canon = format!("/saved{}", view.path_suffix());
        let requested = format!("/{}", path);
        if requested != canon {
            return Reply::redirect(&with_search(&templates::encode_path(&canon), &view));
        }
        return Reply::html(saved_page(store, &view));
    }

    // Standalone-HTML view selectors (post-model.md §4 / the A/B/C model): `?embed`
    // appends the height reporter to the raw asset; `?fullscreen` hands a standalone
    // post the whole viewport. Both are presence flags.
    let embed = flags.embed;
    let fullscreen = flags.fullscreen;

    let all_entries: Vec<&Entry> = store.entries.iter().collect();

    // Folder-post asset (`/{folder}/{asset…}`): resolved before the query parser,
    // which would otherwise misread the multi-segment path as a label. Rendition
    // rungs (`…/photo.tif/jpeg`, `…/jpeg/thumb`) are parsed off the tail there.
    // First of all routes: everything under a folder is the folder's, so a
    // literal file always wins its own name, even one named like another
    // post's language version (`/brev/photo.sv.jpg` is brev's asset, never a
    // redirect to post photo's Swedish version).
    if let Some(resp) = try_asset(&all_entries, &path, &store.content_dir, &store.cache_dir, embed).await {
        return resp;
    }

    // Nested subfolder listing (`/{folder}/{sub…}/`): a public subfolder browsed
    // as its own page. Same precedence reason as assets — the parser can't read a
    // multi-segment nested path. `try_asset` already handled nested files.
    if let Some(resp) = try_folder_listing(&all_entries, &path, &store.content_dir) {
        return resp;
    }

    // A language version (`/{post}.{tag}`, raw at `/{post}.{tag}.{ext}`): a post
    // address plus a registered language tag, answered only when that version
    // exists (DESIGN.md "Languages"). Resolved before the query parser, which
    // would read the tag as a raw-file extension; after the folder routes, which
    // own every path under a folder.
    if let Some(resp) = try_version(store, &all_entries, &path, flags).await {
        return resp;
    }

    // The time-of-day disambiguator is a URL path segment now (`/2026/07/04/191430`),
    // parsed straight off the path — no `?time=` query string. View axes
    // (`/notable`, `/favorites`) are path segments too.
    let query = parse_url_path(&format!("/{}", path));
    // A path that is not an address of this site names nothing: no redirect to
    // the name it happens to end with, no suggestions page, just the 404 every
    // missing path gets. The folder routes above already had their say, so a
    // real file under a folder post is unaffected.
    if query.malformed {
        return not_found();
    }
    let view = view_of(&query, flags);
    let requested = format!("/{}", path);

    // Bare `/name`: one flat namespace, the OLDEST claim (a label or an
    // `alias <name>/` marker) owns it, so a URL's meaning never changes.
    if is_bare_label(&query) {
        if let Some(owner) = templates::name_owner(query.label.as_deref().unwrap(), &all_entries) {
            return serve_resolved(owner, &all_entries, &store, &requested, fullscreen).await;
        }
    }

    // Filter current entries matching the path query (label compared on slug).
    let matching: Vec<&Entry> = store
        .entries
        .iter()
        .filter(|e| query.matches(&e.timestamp, &e.slug, &e.tag_names()))
        .collect();

    // Raw file request (URL has extension like sunset.jpg), possibly with
    // rendition rungs (`/jpeg`, `/jpeg/thumb`) parsed off the path. The base
    // href (renditions stripped) anchors rendition links and redirects.
    if let Some(ref raw_ext) = query.raw_extension {
        let base_href = strip_rendition_suffix(&templates::encode_path(&requested), &query);
        if let Some(entry) = raw_current(&matching, raw_ext) {
            return serve_raw_bytes(
                entry,
                embed,
                query.rendition_jpeg,
                query.rendition_thumb,
                &base_href,
                &store.cache_dir,
            )
            .await;
        }
        // No current file answers this address; a date path may still name one
        // of a post's archived snapshots, whose bytes are its own.
        if let Some(reply) =
            serve_snapshot_bytes(&all_entries, &query, raw_ext, &requested, embed, store).await
        {
            return reply;
        }
        return not_found();
    }

    // Listing: a trailing slash, or a date / tag / view filter with no label
    // and no time segment narrowing it to one entry. With a time present we
    // fall through to entry resolution (that's how an unlabeled entry is
    // addressed). A pure scope listing is 301-normalized onto the canonical
    // spelling (date, then tags, then view; tags sorted; no trailing slash),
    // so every ordering of the same filter has exactly one address.
    let is_scope_listing = query.label.is_none()
        && query.time.is_none()
        && (query.date_prefix.is_some()
            || !query.and_tags.is_empty()
            || !query.or_tags.is_empty()
            || query.notable
            || query.favorites);
    if (query.is_listing && query.time.is_none()) || is_scope_listing {
        let base = if query.label.is_none() {
            let canon = query.canonical_scope_path();
            if requested != canon {
                return Reply::redirect(&with_search(&templates::encode_path(&canon), &view));
            }
            query.scope_base_path()
        } else {
            // A label listing (`/name/`) keeps its requested base; only the
            // view words are stripped so header links do not double them.
            strip_view_segments(&requested)
        };
        return render_listing(&matching, &all_entries, &query, &view, &base);
    }

    // Resolve the request to one current entry:
    // - exactly one match serves (or 301s to its canonical address);
    // - a bare /label carried by several entries serves the OLDEST — newer
    //   claims stay reachable at their date-disambiguated addresses.
    let target: Option<&Entry> = if matching.len() == 1 {
        Some(matching[0])
    } else if query.label.is_some() && query.date_prefix.is_none() {
        oldest_of(&matching)
    } else {
        None
    };

    if let Some(entry) = target {
        return serve_resolved(entry, &all_entries, &store, &requested, fullscreen).await;
    }

    // An archived revision addressed at its date path (+ time segment).
    if let Some((parent, rev)) = find_revision(&all_entries, &query) {
        // A coarser date can name a snapshot uniquely (`/2026/09/hej`); like
        // every other spelling it converges on the snapshot's one address.
        let view = revision_view(parent, rev);
        return serve_resolved(&view, &all_entries, &store, &requested, false).await;
    }

    // A label-free date+time deeplink with no exact match lands on the day view,
    // never a 404 — an edited-that-day post is right there. (The citation
    // survives renames: it resolves by timestamp, or degrades to the day.)
    if query.label.is_none() && query.time.is_some() && query.date_prefix.is_some() {
        let mut day_q = query.clone();
        day_q.time = None;
        let day: Vec<&Entry> = store
            .entries
            .iter()
            .filter(|e| day_q.matches(&e.timestamp, &e.slug, &e.tag_names()))
            .collect();
        if !day.is_empty() {
            let base = day_q.scope_base_path();
            return render_listing(&day, &all_entries, &day_q, &view, &base);
        }
    }

    if matching.is_empty() {
        // A dead bare label (renamed away, or a typo): offer the timeline, search,
        // and closest-slug suggestions rather than auto-redirecting a reused name.
        // Served with 404, identically to any missing path (no existence oracle).
        if is_bare_label(&query) {
            if let Some(label) = query.label.as_deref() {
                return not_found_response(templates::not_found_label_page(&requested, label, &all_entries));
            }
        }
        return not_found();
    }

    // Multiple matches — show as listing
    let base = strip_view_segments(&requested);
    render_listing(&matching, &all_entries, &query, &view, &base)
}

/// Resolve a language-version address (DESIGN.md "Languages"): the post's
/// address with the tag as a trailing "extension", mirroring the file's name.
/// `/brev.sv` is the page of `brev.sv.md` (dated `/2026/03/12/brev.sv`), its
/// bytes are at `/brev.sv.md`, and the rendition rungs hang off a raw image
/// version as off any file (`/photo.sv.tif/jpeg/thumb`). The tag must be a
/// registered language, the head must resolve to exactly one post, and that
/// post must carry a version in that language. Anything else is `None`, so the
/// caller falls through and no existing address changes meaning: `/notes.old`
/// for a post with no version in `old` is the raw request it always was. A
/// non-canonical spelling (case, an alias, a trailing slash) 301s to the
/// canonical one. The head is a post address only: the bare name, or date and
/// time segments before it. Any other prefix (`/x/brev.sv`, a folder's own
/// path) names nothing here, so the folder routes keep what is theirs and no
/// stray prefix reaches a version by the label alone.
async fn try_version(
    store: &ContentStore,
    all_entries: &[&Entry],
    path: &str,
    flags: &RequestFlags,
) -> Option<Reply> {
    let requested = format!("/{}", path);
    let mut segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let mut thumb = false;
    let mut as_jpeg = false;
    if segments.last() == Some(&"thumb") {
        thumb = true;
        segments.pop();
    }
    if segments.last() == Some(&"jpeg") {
        as_jpeg = true;
        segments.pop();
    }
    let last = segments.pop()?;

    // `<stem>.<tag>` names the page, `<stem>.<tag>.<ext>` the bytes.
    fn dotted(s: &str) -> Option<(&str, &str)> {
        s.rsplit_once('.').filter(|(head, tail)| !head.is_empty() && !tail.is_empty())
    }
    let (stem, tail) = dotted(last)?;
    let (stem, tag_token, ext) = match crate::lang::parse(tail) {
        Some(_) => (stem, tail, None),
        None => {
            let (stem, token) = dotted(stem)?;
            (stem, token, Some(tail))
        }
    };
    let tag = crate::lang::parse(tag_token)?;
    if (thumb || as_jpeg) && ext.is_none() {
        return None; // rungs hang off files, not pages
    }
    if !segments.iter().all(|s| is_date_segment(s)) {
        return None; // only the date hierarchy may precede a post's name
    }
    segments.push(stem);
    let head = segments.join("/");
    let current = resolve_one(all_entries, &head).and_then(|e| e.version(&tag).map(|v| (e, v)));
    let (entry, version) = match current {
        Some(pair) => pair,
        // No current post with that version at this address. A date path may
        // still name one of a version's archived snapshots
        // (`/2026/03/12/091500/brev.sv`) — the version's own history, served
        // exactly as the site-language file's is.
        None => {
            let (entry, version, rev) = find_version_revision(all_entries, &head, &tag)?;
            let view = revision_view(&entry.show_version(version), rev);
            // The snapshot's bytes are its own, at its own raw address.
            if let Some(e) = ext {
                if !e.eq_ignore_ascii_case(&view.extension) {
                    return None;
                }
                let base_href = templates::canonical_raw_href(&view, all_entries);
                let mut canon = crate::url::percent_decode(&base_href);
                if as_jpeg {
                    canon.push_str("/jpeg");
                }
                if thumb {
                    canon.push_str("/thumb");
                }
                if requested != canon {
                    return Some(Reply::redirect(&templates::encode_path(&canon)));
                }
                return Some(
                    serve_raw_bytes(&view, flags.embed, as_jpeg, thumb, &base_href, &store.cache_dir)
                        .await,
                );
            }
            let canon = templates::canonical(&view, all_entries);
            if requested != canon.path {
                return Some(Reply::redirect(&templates::canonical_location(&view, all_entries)));
            }
            return Some(serve_entry(&view, store, false).await);
        }
    };
    if let Some(e) = ext {
        if !e.eq_ignore_ascii_case(&version.extension) {
            return None; // `/x.sv.txt` for a `.md` version names nothing
        }
    }
    let shown = entry.show_version(version);

    // The page: canonical at `/<post>.<tag>`.
    if ext.is_none() {
        let canon = templates::canonical(&shown, all_entries);
        if requested != canon.path {
            return Some(Reply::redirect(&templates::canonical_location(&shown, all_entries)));
        }
        let reply = serve_entry(&shown, store, flags.fullscreen).await;
        return Some(with_alternates(reply, &shown, all_entries));
    }

    // The bytes: canonical at `/<post>.<tag>.<ext>`, plus any rendition rungs.
    let base_href = templates::canonical_raw_href(&shown, all_entries);
    let mut canon = crate::url::percent_decode(&base_href);
    if as_jpeg {
        canon.push_str("/jpeg");
    }
    if thumb {
        canon.push_str("/thumb");
    }
    if requested != canon {
        return Some(Reply::redirect(&templates::encode_path(&canon)));
    }
    Some(serve_raw_bytes(&shown, flags.embed, as_jpeg, thumb, &base_href, &store.cache_dir).await)
}

/// Find the one archived revision of a language version that a version
/// address's date path names (`/2026/03/12/091500/brev.sv`, head
/// `2026/03/12/091500/brev`, tag `sv`). The post is matched on its slug, as
/// [`find_revision`] matches the site-language file's; zero or several matches
/// fail closed to `None`, so an ambiguous address never guesses a snapshot.
fn find_version_revision<'a>(
    all_entries: &[&'a Entry],
    head: &str,
    tag: &str,
) -> Option<(&'a Entry, &'a crate::entry::Version, &'a Revision)> {
    let query = parse_url_path(&format!("/{}", head));
    if query.malformed {
        return None; // not an address at all
    }
    let label = query.label.as_ref()?;
    let date_prefix = query.date_prefix.as_ref()?;
    let want = crate::slug::slug(label)?;

    let mut found: Option<(&Entry, &crate::entry::Version, &Revision)> = None;
    let mut count = 0usize;
    for e in all_entries {
        if e.error.is_some() || e.slug.as_deref() != Some(want.as_str()) {
            continue;
        }
        let version = match e.version(tag) {
            Some(v) => v,
            None => continue,
        };
        for r in &version.revisions {
            if !revision_is_at(r, date_prefix, query.time.as_deref()) {
                continue;
            }
            found = Some((*e, version, r));
            count += 1;
        }
    }

    (count == 1).then_some(()).and(found)
}

/// Whether a path segment is a rung of the date hierarchy (`2026`, `03`,
/// `12`) or the time-of-day segment (`191430`): all digits. The shape check
/// that keeps a post address to `/[date/[time/]]name`; the parser judges the
/// values.
fn is_date_segment(segment: &str) -> bool {
    !segment.is_empty() && segment.bytes().all(|b| b.is_ascii_digit())
}

/// Resolve a decoded post address (no leading slash) to exactly one current
/// post, the way `resolve` does for a page request: a bare `/name` goes to its
/// owner (oldest claim), a dated address to its single match. `None` for a
/// scope, a raw file, a listing, or anything ambiguous.
fn resolve_one<'a>(all_entries: &[&'a Entry], head: &str) -> Option<&'a Entry> {
    let query = parse_url_path(&format!("/{}", head));
    if query.malformed
        || query.raw_extension.is_some()
        || query.is_listing
        || !query.and_tags.is_empty()
        || !query.or_tags.is_empty()
        || query.notable
        || query.favorites
        || query.rendition_jpeg
        || query.rendition_thumb
    {
        return None;
    }
    // A scope names no post: `/2026` is the year view even when exactly one
    // post is dated then, and `/2026/03/25` the day view. Only a name, or a
    // date narrowed to the second, addresses one post.
    if query.label.is_none() && query.time.is_none() {
        return None;
    }
    if is_bare_label(&query) {
        if let Some(owner) = templates::name_owner(query.label.as_deref()?, all_entries) {
            return Some(owner);
        }
    }
    let matching: Vec<&Entry> = all_entries
        .iter()
        .copied()
        .filter(|e| query.matches(&e.timestamp, &e.slug, &e.tag_names()))
        .collect();
    let target = if matching.len() == 1 {
        Some(matching[0])
    } else if query.label.is_some() && query.date_prefix.is_none() {
        oldest_of(&matching)
    } else {
        None
    };
    target.filter(|e| e.error.is_none())
}

/// What a multi-segment path resolves to: the folder post that owns it, or the
/// canonical spelling of that address when the request came in another one.
enum FolderPath<'a, 'p> {
    /// The owning post, the canonical head (decoded, no leading slash) every
    /// crumb and item href under it hangs off, and the tail below it.
    Owner(&'a Entry, String, Vec<&'p str>),
    /// The head named the post by another spelling: 301 to this encoded
    /// location, the canonical head with the tail verbatim.
    Elsewhere(String),
}

/// Split a multi-segment path into the folder post that owns it and the tail
/// below it: the shortest leading run of segments that is a post's own address
/// (`hej`, `2026/09/10/hej`, `2026/03/25/191500` for an unlabeled one) owns
/// everything under it, exactly as the folder on disk does. The first such
/// head wins and the search stops there — a post's namespace is its own, and
/// nothing below it is read as another post's address.
///
/// A folder's namespace hangs off the post's **one canonical address**. Every
/// other spelling of the head converges on it exactly as the post's page does:
/// a differently-cased name, an `alias <name>/` marker, a dated form of a name
/// the post owns — each 301s to the canonical head with the tail verbatim (and
/// the trailing slash a nested-listing request carried). The redirect is
/// decided from the address alone, before the tail is looked for on disk, so a
/// hidden file and a missing one still answer alike in the canonical scope.
///
/// One spelling is not the folder's root at all: a date form of a *named*
/// post's address (`/2026/03/12/191430` for a post that owns `/brev`) names no
/// folder, so such a head is skipped and the whole path falls through to the
/// normal resolution that 301s the page.
///
/// `None` when no head is a post's address, or when the one that is belongs to
/// a bare file: a file has no namespace, so `/notes.md/anything` is not an
/// asset path.
fn folder_head<'a, 'p>(all_entries: &[&'a Entry], path: &'p str) -> Option<FolderPath<'a, 'p>> {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segments.len() < 2 {
        return None; // the post's own address, served as the post
    }
    for i in 1..segments.len() {
        let head = segments[..i].join("/");
        let owner = match resolve_one(all_entries, &head) {
            Some(owner) => owner,
            None => continue,
        };
        let requested = format!("/{}", head);
        let canonical = templates::canonical(owner, all_entries).path;
        if canonical != requested && parse_url_path(&requested).label.is_none() {
            continue; // a date form of a named post: not this folder's root
        }
        owner.dir.as_ref()?; // a bare-file post carries no assets
        let tail = segments[i..].to_vec();
        if canonical != requested {
            let mut location =
                templates::encode_path(&format!("{}/{}", canonical, tail.join("/")));
            if path.ends_with('/') {
                location.push('/'); // a nested listing keeps its trailing slash
            }
            return Some(FolderPath::Elsewhere(location));
        }
        return Some(FolderPath::Owner(owner, head, tail));
    }
    None
}

/// Add the `Link` header naming every language version of a post (RFC 8288
/// `rel="alternate"; hreflang=…`, the HTTP twin of the `<link>` elements in
/// the page head) to a successful reply. The static build records it with the
/// file, so a host adapter can offer the Accept-Language redirect from the
/// manifest alone, and `verify` checks it like any header. Paths, not
/// absolute URLs: the header is resolved against the request, so it is true
/// on every host the same bytes are served from.
fn with_alternates(mut reply: Reply, entry: &Entry, all_entries: &[&Entry]) -> Reply {
    if reply.status != 200 || entry.versions.is_empty() {
        return reply;
    }
    let links: Vec<String> = templates::alternates(entry, all_entries)
        .iter()
        .map(|a| format!("<{}>; rel=\"alternate\"; hreflang=\"{}\"", a.href, a.hreflang))
        .collect();
    reply.headers.push(("link", links.join(", ")));
    reply
}

/// Drop the rendition rungs off the tail of an encoded raw-file path, giving
/// the file's own base address (`/photo.tif/jpeg/thumb` → `/photo.tif`).
fn strip_rendition_suffix(encoded_path: &str, query: &ContentQuery) -> String {
    let mut out = encoded_path;
    if query.rendition_thumb {
        out = out.strip_suffix("/thumb").unwrap_or(out);
    }
    if query.rendition_jpeg {
        out = out.strip_suffix("/jpeg").unwrap_or(out);
    }
    out.to_string()
}

/// Drop the reserved view words from a path, so a label-listing base composes
/// header links without doubling them.
fn strip_view_segments(path: &str) -> String {
    let kept: Vec<&str> = path
        .split('/')
        .filter(|s| !s.is_empty() && *s != "notable" && *s != "favorites")
        .collect();
    if kept.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", kept.join("/"))
    }
}

/// A location with the view's `?search=` preserved (the path already carries
/// the view segments).
fn with_search(encoded_path: &str, view: &ViewFilter) -> String {
    format!("{}{}", encoded_path, templates::search_query_suffix(view))
}

/// Whether a request is a bare `/name` (no date, time, extension, tag, or
/// trailing slash) — the form claim resolution applies to.
fn is_bare_label(q: &ContentQuery) -> bool {
    q.label.is_some()
        && q.date_prefix.is_none()
        && q.time.is_none()
        && q.raw_extension.is_none()
        && q.and_tags.is_empty()
        && q.or_tags.is_empty()
        && !q.is_listing
}

/// The single oldest entry in a set, or None on a timestamp tie (ambiguous).
fn oldest_of<'a>(entries: &[&'a Entry]) -> Option<&'a Entry> {
    let oldest = entries.iter().map(|e| e.timestamp).min()?;
    let mut at_oldest = entries.iter().filter(|e| e.timestamp == oldest);
    let first = at_oldest.next()?;
    if at_oldest.next().is_some() {
        None
    } else {
        Some(first)
    }
}

/// Serve an entry once it is resolved: 301 to its canonical address if the
/// request is not already there, otherwise render it (or its error page).
/// Every non-canonical URL collapses onto the one bare canonical address —
/// view state is a listing concern and never rides a post URL.
async fn serve_resolved(
    entry: &Entry,
    all_entries: &[&Entry],
    store: &ContentStore,
    requested: &str,
    fullscreen: bool,
) -> Reply {
    let canon = templates::canonical(entry, all_entries);
    if requested != canon.path {
        return Reply::redirect(&templates::canonical_location(entry, all_entries));
    }
    if entry.error.is_some() {
        return error_response(entry, all_entries);
    }
    with_alternates(serve_entry(entry, store, fullscreen).await, entry, all_entries)
}

/// Render a post's fail-closed error page with HTTP 500 (loud, never hidden).
fn error_response(entry: &Entry, all_entries: &[&Entry]) -> Reply {
    Reply::html_status(500, templates::error_page(entry, all_entries))
}

/// The path `segments` names under `dir`, built by walking real directory
/// entries: each segment must equal an entry's name byte-for-byte. A
/// filesystem that folds case or Unicode normalization would otherwise answer
/// `/hej/photo.jpg` with `Photo.JPG` here and 404 it on Linux; the static
/// build emits on-disk names, so the served address is the entry name and
/// nothing else. `.`/`..` never match an entry, so the walk is bounded to
/// `dir` by construction (the canonicalize guard stays as a second line).
/// None = no such entry.
fn exact_path(dir: &std::path::Path, segments: &[&str]) -> Option<std::path::PathBuf> {
    if segments.is_empty() {
        return None; // a folder head alone names no entry inside it
    }
    let mut current = dir.to_path_buf();
    for segment in segments {
        let want = std::ffi::OsStr::new(*segment);
        let entry = std::fs::read_dir(&current)
            .ok()?
            .flatten()
            .find(|e| e.file_name().as_os_str() == want)?;
        current = entry.path();
    }
    Some(current)
}

/// Resolve a folder-post asset request (`/{folder}/{asset…}`) to bytes on disk,
/// strictly inside that post's directory. Returns None when the path is not an
/// asset (wrong shape, unknown folder, a missing or escaping file), so the
/// caller falls through to normal resolution. Rendition rungs on the tail
/// (`…/photo.tif/jpeg`, `…/jpeg/thumb`) are recognized only when no literal
/// file answers the full path, so a real file always wins its own name.
async fn try_asset(
    all_entries: &[&Entry],
    path: &str,
    content_dir: &std::path::Path,
    cache_dir: &std::path::Path,
    embed: bool,
) -> Option<Reply> {
    // The post that owns this path is the shortest head that is a post
    // address — its bare name, or its date form when a newer claimant or an
    // unlabeled post lives there. Everything after that head is the folder's,
    // and any other spelling of the head 301s onto the canonical one first.
    let (owner, head, tail) = match folder_head(all_entries, path)? {
        FolderPath::Owner(owner, head, tail) => (owner, head, tail),
        FolderPath::Elsewhere(location) => return Some(Reply::redirect(&location)),
    };
    let dir = owner.dir.as_ref()?; // only folder posts carry assets
    let canon_dir = std::fs::canonicalize(dir).ok()?;

    // Resolve the requested file, literal path first; failing that, peel the
    // canonical rendition tail (`thumb` then `jpeg`) and retry.
    let mut thumb = false;
    let mut as_jpeg = false;
    let mut file_segments: &[&str] = &tail;
    let mut canon_file =
        exact_path(dir, file_segments).and_then(|p| std::fs::canonicalize(p).ok());
    if canon_file.is_none() {
        let mut trimmed = file_segments;
        if trimmed.last() == Some(&"thumb") {
            thumb = true;
            trimmed = &trimmed[..trimmed.len() - 1];
        }
        if trimmed.last() == Some(&"jpeg") {
            as_jpeg = true;
            trimmed = &trimmed[..trimmed.len() - 1];
        }
        // A thumb rung is honest for a JPEG source without the /jpeg rung;
        // anything else must carry it. The last real segment must name a file
        // with an extension — rendition rungs hang off files, not folders.
        if (!thumb && !as_jpeg) || trimmed.is_empty() || !trimmed.last()?.contains('.') {
            return None;
        }
        file_segments = trimmed;
        canon_file = exact_path(dir, file_segments).and_then(|p| std::fs::canonicalize(p).ok());
    }
    let canon_file = canon_file?;
    if !canon_file.starts_with(&canon_dir) {
        return None; // path-traversal guard
    }
    if !std::fs::metadata(&canon_file).ok()?.is_file() {
        return None; // marker folders / subdirs are not assets
    }
    if canon_file
        .file_name()
        .and_then(|n| n.to_str())
        .map_or(true, |n| n.starts_with('.'))
    {
        return None; // dotfiles are never served
    }

    // The primary content is canonical at `/label(.ext)` — send duplicates there.
    // A file only *is* the primary when it carries its own `public` tag (the scan
    // demotes an untagged primary to a withheld listing), so this redirect never
    // routes around the per-file gate below.
    if std::fs::canonicalize(&owner.path).ok().as_deref() == Some(canon_file.as_path()) {
        // The post's own canonical raw address — which is its page address for
        // a primary with no extension. Built from the post, never from the
        // requested head: a newer claimant's file must land on ITS address,
        // not on the bare name the oldest claim owns.
        return Some(Reply::redirect(&templates::canonical_raw_href(owner, all_entries)));
    }
    // A language version's file is canonical at `/label.<tag>.<ext>` likewise.
    if let Some(version) = owner
        .versions
        .iter()
        .find(|v| std::fs::canonicalize(&v.path).ok().as_deref() == Some(canon_file.as_path()))
    {
        let shown = owner.show_version(version);
        return Some(Reply::redirect(&templates::canonical_raw_href(&shown, all_entries)));
    }

    // Per-file visibility gate (strict — post-model.md [review]): every asset
    // needs its own `public` tag, and no component of its path may be `private`.
    // A hidden or untagged asset is treated as absent (fall through to a normal
    // 404), so it is indistinguishable from a missing one — no existence oracle.
    if !crate::tags::path_visible(content_dir, &canon_file) {
        return None;
    }

    let bytes = std::fs::read(&canon_file).ok()?;
    let ext = canon_file.extension().and_then(|e| e.to_str()).unwrap_or("");
    // An in-folder HTML/XHTML asset is a standalone document too (model C): jail
    // it with the sandbox CSP, honoring `?embed` so a folder-hosted page can be
    // framed the same way a bare-file one is.
    if is_sandboxed_document(ext) {
        if thumb || as_jpeg {
            return Some(not_found()); // renditions exist for images only
        }
        return Some(
            sandbox_html_response(bytes, embed, document_content_type(ext))
                .from_file(&canon_file, "public"),
        );
    }
    // In-folder assets go through the same privacy gate as a primary (post-model.md
    // §8): images stripped/transcoded, author-readable text raw, everything else
    // withheld. The `public-original` opt-in is per file here, read from the
    // asset's own tags; on a withheld format it is the universal exact-bytes
    // escape (the author explicitly publishes whatever the file embeds).
    let stem = canon_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file")
        .to_string();
    // The tag that authorizes the raw tail below; the Withhold arm upgrades it
    // when serving exact bytes is the author's explicit public-original choice.
    let mut raw_tag = "public";
    match crate::media::classify(ext, &bytes) {
        crate::media::Disposition::Image => {
            let is_original = file_serves_original(&canon_file);
            // The asset's own address with the rendition rungs stripped: the
            // head that resolved, then the path inside the folder.
            let base_href =
                templates::encode_path(&format!("/{}/{}", head, file_segments.join("/")));
            return Some(
                serve_image(ImageRequest {
                    bytes,
                    source: &canon_file,
                    ext,
                    is_original,
                    thumb,
                    as_jpeg,
                    base_href: &base_href,
                    save_stem: &stem,
                    cache_dir,
                })
                .await,
            );
        }
        crate::media::Disposition::Pdf => {
            return Some(serve_pdf(bytes, file_serves_original(&canon_file), &canon_file, &stem));
        }
        crate::media::Disposition::Svg => {
            return Some(serve_svg(bytes, file_serves_original(&canon_file), &canon_file, &stem));
        }
        crate::media::Disposition::Withhold => {
            if !file_serves_original(&canon_file) {
                return Some(metadata_withheld());
            }
            raw_tag = "public-original";
            tracing::warn!(
                "Serving {} exact bytes (public-original) — any embedded metadata is published",
                canon_file.display()
            );
        }
        crate::media::Disposition::Raw => {}
    }
    // A rendition rung on a non-image asset names nothing — fail closed.
    if thumb || as_jpeg {
        return Some(not_found());
    }
    let mime = raw_content_type(ext);
    Some(Reply {
        provenance: Provenance::File { source: canon_file.clone(), tag: raw_tag },
        status: 200,
        headers: vec![
            ("content-type", mime),
            ("cache-control", "public, max-age=3600".to_string()),
        ],
        body: bytes,
    })
}

/// Resolve a nested subfolder listing request (`/{folder}/{sub…}/`) to a
/// browsable directory strictly inside that post's folder. Returns None when the
/// path is not a nested directory (wrong shape, a file — handled by `try_asset` —
/// a hidden/escaping path, or the top-level `/label` itself), so the caller falls
/// through. Every path component must be `public` (fail-closed, deny-wins).
fn try_folder_listing(
    all_entries: &[&Entry],
    path: &str,
    content_dir: &std::path::Path,
) -> Option<Reply> {
    // Same ownership rule as `try_asset`: the shortest post-shaped head owns
    // every path under it, whether that head is a bare name or a date form,
    // and any other spelling of it 301s onto the canonical one.
    let (owner, head, tail) = match folder_head(all_entries, path)? {
        FolderPath::Owner(owner, head, tail) => (owner, head, tail),
        FolderPath::Elsewhere(location) => return Some(Reply::redirect(&location)),
    };
    let dir = owner.dir.as_ref()?; // a bare-file post has no subfolders

    // Past this point the request is scoped *into* a real folder post, so it
    // resolves here or 404s — it never falls through to the timeline. Visible
    // files were already served by `try_asset`; a hidden or missing path 404s
    // identically (no existence oracle), and so does a hidden nested listing.
    let resolve = || -> Option<Reply> {
        let candidate = exact_path(dir, &tail)?;
        let canon_dir = std::fs::canonicalize(dir).ok()?;
        let canon_target = std::fs::canonicalize(&candidate).ok()?;
        if !canon_target.starts_with(&canon_dir) {
            return None; // path-traversal guard
        }
        if !std::fs::metadata(&canon_target).ok()?.is_dir() {
            return None; // a file is an asset (try_asset), not a listing
        }
        // Fail-closed: every component (the post, each subfolder) must be public.
        if !crate::tags::path_visible(content_dir, &canon_target) {
            return None;
        }
        // The post's own address, then the path inside its folder: the base
        // every crumb and item href hangs off.
        let base = format!("/{}/{}", head, tail.join("/"));
        Some(render_nested_listing(&canon_target, &base, all_entries))
    };
    Some(resolve().unwrap_or_else(not_found))
}

/// Render a resolved, visible nested subfolder as a listing page. `base` is the
/// decoded address of the subfolder (the owning post's address plus the path
/// inside its folder, no trailing slash): it IS the canonical URL and the base
/// each item href hangs off — mirroring the filesystem verbatim.
fn render_nested_listing(
    canon_target: &std::path::Path,
    base: &str,
    all_entries: &[&Entry],
) -> Reply {
    let listing = crate::content::build_dir_listing(canon_target);
    let title = canon_target
        .file_name()
        .and_then(|n| n.to_str())
        .or_else(|| base.rsplit('/').next())
        .unwrap_or("files")
        .to_string();
    Reply::html(templates::nested_listing_page(base, &title, &listing, all_entries))
}

/// Find the one archived revision a date-path request addresses, or None when
/// zero or several match (fail closed to normal resolution).
fn find_revision<'a>(
    all_entries: &[&'a Entry],
    query: &ContentQuery,
) -> Option<(&'a Entry, &'a Revision)> {
    let label = query.label.as_ref()?;
    let date_prefix = query.date_prefix.as_ref()?;
    let want = crate::slug::slug(label)?;

    let mut found: Option<(&Entry, &Revision)> = None;
    let mut count = 0usize;
    for e in all_entries {
        if e.error.is_some() || e.slug.as_deref() != Some(want.as_str()) {
            continue;
        }
        for r in &e.revisions {
            if !revision_is_at(r, date_prefix, query.time.as_deref()) {
                continue;
            }
            found = Some((*e, r));
            count += 1;
        }
    }

    (count == 1).then_some(()).and(found)
}

/// Whether a revision's own date is the one a date path addresses: the date
/// prefix matches its `Y-m-dTHMS` stamp, and the time segment (when the URL
/// carries one) its `HMS`.
fn revision_is_at(rev: &Revision, date_prefix: &str, time: Option<&str>) -> bool {
    if !rev.date.format("%Y-%m-%dT%H%M%S").to_string().starts_with(date_prefix) {
        return false;
    }
    match time {
        Some(t) => rev.date.format("%H%M%S").to_string().starts_with(t),
        None => true,
    }
}

/// The value that renders one archived revision of `entry`: its own bytes,
/// dated by its own mtime, under the post's chrome. A snapshot is one frozen
/// file, so it carries no history of its own (`revisions`), no extra addresses
/// (`aliases`), no listing, and no counterpart in another language — it is
/// what that one file was at that moment, and nothing says another language's
/// file was in the same state then. `snapshot` marks it, which is what puts
/// its page and its bytes at its own date address (`templates::canonical`).
fn revision_view(entry: &Entry, rev: &Revision) -> Entry {
    let mut e = entry.clone();
    e.path = rev.path.clone();
    e.timestamp = crate::postdate::PostDate::from_mtime(rev.date);
    e.snapshot = true;
    e.edited = None;
    e.revisions = Vec::new();
    e.aliases = Vec::new();
    e.listing = None; // a revision serves specific bytes, never a listing
    e.versions = Vec::new(); // no hreflang alternates on a snapshot
    e.extension = rev
        .path
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_string();
    e
}

/// Render a filtered timeline listing: apply the view filter, build the shared
/// header reflecting the active topic and view, and hand off to the template.
/// `base_path` is the scope's canonical base (date/tags, no view words) that
/// the header controls compose view links onto.
fn render_listing(
    matching: &[&Entry],
    all_entries: &[&Entry],
    query: &ContentQuery,
    view: &ViewFilter,
    base_path: &str,
) -> Reply {
    let display: Vec<&Entry> = matching.iter().copied().filter(|e| view.matches(e)).collect();
    let cloud = compute_cloud(all_entries);
    // The tag-only portion of the path, so month links can graft a date onto the
    // active topic and the date chip can clear back to just the tags.
    let path_tags = tag_suffix(base_path);
    let date_scope = query.date_prefix.as_deref().map(|prefix| templates::DateScope {
        label: human_date(prefix),
        clear_path: if path_tags.is_empty() { "/".to_string() } else { path_tags.clone() },
    });
    let ctx = HeaderContext {
        cloud: &cloud,
        active_tag: active_tag(query),
        view,
        base_path,
        date_scope,
        path_tags: &path_tags,
        saved_view: false,
    };
    let title = filter_title(&build_filter_description(query), view, false);
    Reply::html(templates::timeline_page(&display, all_entries, &ctx, &title))
}

/// The `+tag` portion of a path (e.g. "/+design"), or "" when there is none —
/// used to keep the active topic while swapping or clearing the date scope.
fn tag_suffix(path: &str) -> String {
    path.trim_matches('/')
        .split('/')
        .filter(|s| s.starts_with('+'))
        .map(|s| format!("/{}", s))
        .collect()
}

/// Humanize a compact date prefix for the scope chip and the page title:
/// "2026" → "2026", "2026-03" → "March 2026", "2026-03-25" → "March 25, 2026".
fn human_date(prefix: &str) -> String {
    let parts: Vec<&str> = prefix.split('-').collect();
    match parts.as_slice() {
        [y] => y.to_string(),
        [y, m] => format!("{} {}", month_name(m), y),
        [y, m, d] => format!("{} {}, {}", month_name(m), d.trim_start_matches('0'), y),
        _ => prefix.to_string(),
    }
}

/// Full month name for a zero-padded "01".."12"; echoes the input if unknown.
fn month_name(m: &str) -> &str {
    const NAMES: [&str; 12] = [
        "January", "February", "March", "April", "May", "June", "July", "August",
        "September", "October", "November", "December",
    ];
    m.parse::<usize>()
        .ok()
        .filter(|n| (1..=12).contains(n))
        .map_or(m, |n| NAMES[n - 1])
}

/// `/_embed/{key}/{asset_name}` — a cached embed asset (OG images, etc.).
///
/// `key` is the content-relative-path hash the cache is keyed by; it must match
/// an entry the store actually knows about, so a removed or unpublished entry
/// can never have its cached media served (privacy fails closed). `asset_name`
/// must be a simple filename (no path separators). Anything else 404s.
pub fn embed_asset(store: &ContentStore, key: &str, asset_name: &str) -> Reply {
    // Reject anything that even looks path-y. Sanitized to the same shape used when
    // writing the file (`download_media`'s safe_name filter), with no dots-only.
    if !is_safe_asset_segment(key) || !is_safe_asset_segment(asset_name) {
        return not_found();
    }

    // Find the entry whose cache key matches. Lookup, not derivation: we serve
    // assets only for entries currently in the store (i.e. public).
    let entry = store
        .entries
        .iter()
        .find(|e| crate::embed::cache_key(&store.content_dir, &e.path) == key);
    let entry = match entry {
        Some(e) => e,
        None => return not_found(),
    };

    let cache_dir =
        crate::embed::cache_dir_for(&store.cache_dir, &store.content_dir, &entry.path);
    let asset_path = cache_dir.join(asset_name);

    // Defense in depth: ensure the resolved path still sits inside cache_dir.
    if !asset_path.starts_with(&cache_dir) {
        return not_found();
    }

    let bytes = match std::fs::read(&asset_path) {
        Ok(b) => b,
        Err(_) => return not_found(),
    };

    let ext = asset_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    // `download_media` names a cached asset from the remote URL and does not vet
    // its content type, so a hostile `og:image` could land an HTML/XHTML document
    // here. Serve any such document jailed rather than as a trusted-origin page.
    if is_sandboxed_document(ext) {
        return sandbox_html_response(bytes, false, document_content_type(ext))
            .from_file(&entry.path, "public");
    }
    let mime = raw_content_type(ext);

    Reply {
        provenance: Provenance::File { source: entry.path.clone(), tag: "public" },
        status: 200,
        headers: vec![
            ("content-type", mime),
            ("cache-control", "public, max-age=86400".to_string()),
        ],
        body: bytes,
    }
}

/// Allow only simple filename characters: alphanumeric plus `. - _`.
/// No path separators, no leading dot, no `..`.
fn is_safe_asset_segment(s: &str) -> bool {
    if s.is_empty() || s.starts_with('.') || s.contains("..") {
        return false;
    }
    s.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
}

/// Serve a single entry as a rendered HTML page. `fullscreen` selects the
/// fullscreen variant (B) for a standalone `.html` post; it is ignored otherwise.
async fn serve_entry(entry: &Entry, store: &ContentStore, fullscreen: bool) -> Reply {
    let all: Vec<&Entry> = store.entries.iter().collect();

    // A listing folder renders as a browsable index, not a document. Its optional
    // intro doc is rendered through the same pipeline the post body uses.
    if let Some(listing) = &entry.listing {
        let intro_html = match &listing.intro {
            Some(p) => match std::fs::read(p) {
                Ok(bytes) => match render_entry(&listing.intro_ext, &bytes).await {
                    Ok(RenderedContent::Html(h)) => {
                        let h = crate::embed::expand_inline_embeds(&h, &store.embed_cache);
                        Some(templates::Body::Html(crate::outbound::sanitize_body_links(&h)))
                    }
                    Ok(RenderedContent::PreformattedText(t)) => Some(templates::Body::Plain(t)),
                    _ => None,
                },
                Err(_) => None,
            },
            None => None,
        };
        return Reply::html(templates::listing_page(entry, listing, &all, intro_html))
            .from_file(&entry.path, "public");
    }

    // The next-older entry feeds the Continue block at the foot of the post.
    let next: Option<&Entry> = all
        .iter()
        .copied()
        .filter(|e| e.timestamp < entry.timestamp)
        .max_by_key(|e| e.timestamp);

    // A standalone `.html` post is shown jailed, never merged into the trusted
    // shell: the post address embeds it in the shell (A) or, with `?fullscreen`,
    // hands it the whole viewport (B). Both wrap the same-origin asset URL in a
    // sandboxed iframe; the raw bytes (C) come from that URL (serve_raw_bytes).
    if is_sandboxed_document(&entry.extension) {
        return if fullscreen {
            Reply::html(templates::standalone_fullscreen_page(entry, &all)).from_file(&entry.path, "public")
        } else {
            Reply::html(templates::standalone_embed_page(entry, &all, next)).from_file(&entry.path, "public")
        };
    }

    // A link post *is* its destination (post-model.md §4): its body is the rich
    // embed card. A content post that merely *cites* a destination keeps its own
    // body and gets a cite section below it (handled by the render path + template),
    // so the card branch is gated to link posts only — otherwise a folder post with
    // a `link.*` sidecar (also in the embed cache, keyed by its primary) would be
    // hijacked into showing the card instead of its own words.
    if entry.kind() == "link" {
        if let Some(embed_data) = store.embed_cache.get(&entry.path) {
            if !embed_data.is_upstream_deleted() {
                let cache_dir =
                    crate::embed::cache_dir_for(&store.cache_dir, &store.content_dir, &entry.path);
                let card_html = crate::embed::render_embed_card(embed_data, &cache_dir);
                return Reply::html(templates::entry_page(entry, templates::Body::Html(card_html), &all, next)).from_file(&entry.path, "public");
            }
        }
        // A link post with no usable embed (fetch failed / upstream deleted / not
        // yet fetched): render the bare destination cite rather than fall through to
        // a raw `.webloc`/`.url` byte download, so the page stays a working link.
        let body = templates::bare_link_body(entry);
        return Reply::html(templates::entry_page(entry, templates::Body::Html(body), &all, next)).from_file(&entry.path, "public");
    }

    let content = match std::fs::read(&entry.path) {
        Ok(c) => c,
        Err(_) => return not_found(),
    };

    match render_entry(&entry.extension, &content).await {
        Ok(RenderedContent::Html(html)) => {
            // Expand inline URLs to embed cards, then run the fail-closed outbound
            // scheme guard as the last transform before the body enters the shell:
            // no unsafe-scheme anchor can survive, and every external link carries
            // rel="noreferrer" (post-model.md §7).
            let html = crate::embed::expand_inline_embeds(&html, &store.embed_cache);
            let html = crate::outbound::sanitize_body_links(&html);
            Reply::html(templates::entry_page(entry, templates::Body::Html(html), &all, next)).from_file(&entry.path, "public")
        }
        Ok(RenderedContent::Standalone(html)) => {
            Reply::html(html).from_file(&entry.path, "public")
        }
        Ok(RenderedContent::PreformattedText(text)) => {
            Reply::html(templates::entry_page(entry, templates::Body::Plain(text), &all, next)).from_file(&entry.path, "public")
        }
        Ok(RenderedContent::Embed(card_html)) => {
            Reply::html(templates::entry_page(entry, templates::Body::Html(card_html), &all, next)).from_file(&entry.path, "public")
        }
        Ok(RenderedContent::Image { mime }) => {
            // The exact-bytes opt-in is per file; the loud "publishes your
            // location/camera metadata" warning fires only when an `original`
            // image actually carries something sensitive to leak.
            let is_original = file_serves_original(&entry.path);
            let publishes_metadata = is_original && crate::media::has_sensitive_metadata(&content);
            if publishes_metadata {
                tracing::warn!(
                    "Serving {} with embedded metadata intact (public-original tag)",
                    entry.path.display()
                );
            }
            Reply::html(templates::image_page(entry, &mime, is_original, publishes_metadata, &all, next))
                .from_file(&entry.path, "public")
        }
        Ok(RenderedContent::Download { .. }) => {
            let raw_href = templates::canonical_raw_href(entry, &all);
            serve_raw_bytes(entry, false, false, false, &raw_href, &store.cache_dir).await
        }
        Err(e) => {
            tracing::error!("Render error for {}: {}", entry.path.display(), e);
            let body = format!("<p>Rendering error: {}</p>", html_escape_content(&e));
            Reply::html(templates::entry_page(entry, templates::Body::Html(body), &all, next)).from_file(&entry.path, "public")
        }
    }
}

/// The current post whose bytes a raw request addresses: the one carrying the
/// requested extension, the oldest on a duplicate label — mirroring the page
/// URL's oldest-claim-wins rule. `None` when no current file answers.
fn raw_current<'a>(matching: &[&'a Entry], requested_ext: &str) -> Option<&'a Entry> {
    matching
        .iter()
        .copied()
        .filter(|e| e.extension.eq_ignore_ascii_case(requested_ext))
        .min_by_key(|e| e.timestamp)
}

/// Serve an archived snapshot's own bytes at its own raw address
/// (`/Y/M/D/HHMMSS/name.ext`, the copy's date to the second). A snapshot is
/// one frozen file, so its bytes leave through `serve_raw_bytes` like any
/// other file's — the same privacy gate, the same `public` tag on the copy
/// itself, decided at scan time. `None` when the date path names no single
/// snapshot or the extension is not the copy's, so the caller 404s.
async fn serve_snapshot_bytes(
    all_entries: &[&Entry],
    query: &ContentQuery,
    requested_ext: &str,
    requested: &str,
    embed: bool,
    store: &ContentStore,
) -> Option<Reply> {
    let (parent, rev) = find_revision(all_entries, query)?;
    let view = revision_view(parent, rev);
    if !requested_ext.eq_ignore_ascii_case(&view.extension) {
        return None; // `/…/hej.txt` for a `.md` snapshot names nothing
    }
    let base_href = templates::canonical_raw_href(&view, all_entries);
    let mut canon = crate::url::percent_decode(&base_href);
    if query.rendition_jpeg {
        canon.push_str("/jpeg");
    }
    if query.rendition_thumb {
        canon.push_str("/thumb");
    }
    if requested != canon {
        return Some(Reply::redirect(&templates::encode_path(&canon)));
    }
    Some(
        serve_raw_bytes(
            &view,
            embed,
            query.rendition_jpeg,
            query.rendition_thumb,
            &base_href,
            &store.cache_dir,
        )
        .await,
    )
}

/// The save-name stem for an entry's bytes: its label, or the timestamp stamp
/// for an unlabeled file.
fn entry_save_stem(entry: &Entry) -> String {
    match &entry.label {
        Some(label) => sanitize_filename(label),
        None => entry.timestamp.file_stamp(),
    }
}

async fn serve_raw_bytes(
    entry: &Entry,
    embed: bool,
    as_jpeg: bool,
    thumb: bool,
    base_href: &str,
    cache_dir: &std::path::Path,
) -> Reply {
    let content = match std::fs::read(&entry.path) {
        Ok(c) => c,
        Err(_) => return not_found(),
    };
    let stem = entry_save_stem(entry);

    // A standalone HTML/XHTML post's bytes are the byte-exact asset (model C):
    // served jailed by the sandbox CSP, with the height reporter appended only on
    // the `?embed` copy. Never the trusted origin, never a download prompt.
    if is_sandboxed_document(&entry.extension) {
        if as_jpeg || thumb {
            return not_found(); // renditions exist for images only
        }
        return sandbox_html_response(content, embed, document_content_type(&entry.extension))
            .from_file(&entry.path, "public");
    }

    // Privacy gate (post-model.md §8): an image is stripped/transcoded (unless
    // the author opted into the exact bytes); author-readable UTF-8 text serves
    // as-is; every other format is withheld — the boundary is an allowlist, so a
    // format nobody listed is a 415, never a metadata leak. The per-file
    // `public-original` tag serves the exact bytes of any withheld format.
    // The tag that authorizes the raw tail below; the Withhold arm upgrades it
    // when serving exact bytes is the author's explicit public-original choice.
    let mut raw_tag = "public";
    match crate::media::classify(&entry.extension, &content) {
        crate::media::Disposition::Image => {
            let is_original = file_serves_original(&entry.path);
            return serve_image(ImageRequest {
                bytes: content,
                source: &entry.path,
                ext: &entry.extension,
                is_original,
                thumb,
                as_jpeg,
                base_href,
                save_stem: &stem,
                cache_dir,
            })
            .await;
        }
        crate::media::Disposition::Pdf => {
            return serve_pdf(content, file_serves_original(&entry.path), &entry.path, &stem);
        }
        crate::media::Disposition::Svg => {
            return serve_svg(content, file_serves_original(&entry.path), &entry.path, &stem);
        }
        crate::media::Disposition::Withhold => {
            if !file_serves_original(&entry.path) {
                return metadata_withheld();
            }
            raw_tag = "public-original";
            tracing::warn!(
                "Serving {} exact bytes (public-original) — any embedded metadata is published",
                entry.path.display()
            );
        }
        crate::media::Disposition::Raw => {}
    }

    // A rendition rung on a non-image raw file names nothing — fail closed.
    if as_jpeg || thumb {
        return not_found();
    }

    let mime = raw_content_type(&entry.extension);

    let filename = format!("{}.{}", stem, entry.extension);
    let disposition = format!(r#"inline; filename="{}""#, filename);

    Reply {
        provenance: Provenance::File { source: entry.path.clone(), tag: raw_tag },
        status: 200,
        headers: vec![
            ("content-type", mime),
            ("content-disposition", disposition),
        ],
        body: content,
    }
}

/// Serve image bytes with location/camera metadata stripped for privacy
/// (`post-model.md` §8). `is_original` (the author's per-file `original` tag)
/// bypasses the strip and serves the exact source bytes. A format we cannot yet
/// clean, or one that fails to parse, is withheld — fail closed, never a silent
/// raw fallback that would leak the metadata we mean to remove.
/// Whether the exact-bytes opt-in applies to *this specific file* — read from
/// the file's own Finder tags, never inherited from a parent folder (post-model.md
/// §8). A tag that re-exposes embedded GPS must never cascade.
fn file_serves_original(path: &std::path::Path) -> bool {
    crate::tags::read_tags_colored(path)
        .iter()
        .any(crate::tags::Tag::is_original)
}

/// Serve a PDF with its document metadata stripped (`media::strip_pdf`), or the
/// exact bytes when the author's per-file `public-original` tag says so. A PDF
/// that cannot be verify-cleaned (encrypted, unparseable) is withheld — the
/// same fail-closed rule as every other format.
fn serve_pdf(bytes: Vec<u8>, is_original: bool, path: &std::path::Path, stem: &str) -> Reply {
    let name = format!("{}.pdf", stem);
    if is_original {
        tracing::warn!(
            "Serving {} exact bytes (public-original) — any embedded metadata is published",
            path.display()
        );
        return clean_bytes_response(bytes, "application/pdf", &name, path, "public-original");
    }
    match crate::media::strip_pdf(&bytes) {
        Some(clean) => clean_bytes_response(clean, "application/pdf", &name, path, "public"),
        None => metadata_withheld(),
    }
}

/// Serve an SVG with its metadata stripped (`media::strip_svg`), or the exact
/// bytes on the author's per-file `public-original` tag. Either way the
/// response is `image/svg+xml`, which the security middleware hard-jails
/// (`sandbox; default-src 'none'`) — the strip handles the metadata in the
/// bytes; the CSP handles script when the SVG is opened as a document.
fn serve_svg(bytes: Vec<u8>, is_original: bool, path: &std::path::Path, stem: &str) -> Reply {
    let name = format!("{}.svg", stem);
    if is_original {
        tracing::warn!(
            "Serving {} exact bytes (public-original) — any embedded metadata is published",
            path.display()
        );
        return clean_bytes_response(bytes, "image/svg+xml", &name, path, "public-original");
    }
    match crate::media::strip_svg(&bytes) {
        Some(clean) => clean_bytes_response(clean, "image/svg+xml", &name, path, "public"),
        None => metadata_withheld(),
    }
}

/// Everything one image request needs: the source bytes and extension, the
/// author's exact-bytes opt-in, the rendition rungs off the URL, the file's
/// base address (rungs stripped) for redirects and the explainer, and the
/// save-name stem the response's Content-Disposition names the download by —
/// so `/photo.tif/jpeg` saves as `photo.jpg`, never as `jpeg`.
struct ImageRequest<'a> {
    bytes: Vec<u8>,
    source: &'a std::path::Path,
    ext: &'a str,
    is_original: bool,
    thumb: bool,
    as_jpeg: bool,
    base_href: &'a str,
    save_stem: &'a str,
    cache_dir: &'a std::path::Path,
}

async fn serve_image(req: ImageRequest<'_>) -> Reply {
    let norm = crate::entry::normalize_ext(req.ext);
    let source_is_jpeg = matches!(norm.as_str(), "jpg" | "jpeg");
    let transcode_only = crate::media::is_transcode_only_ext(req.ext);

    // A `/jpeg` rung on a file that already IS a JPEG adds nothing: 301 to the
    // parent (the closure model collapses no-op rungs, sajt.md).
    if req.as_jpeg && source_is_jpeg {
        let target = if req.thumb {
            format!("{}/thumb", req.base_href)
        } else {
            req.base_href.to_string()
        };
        return Reply::redirect(&target);
    }

    // A gallery-tile thumbnail is a derived preview: always stripped/resized,
    // even for an `original`-tagged file (the exact bytes stay at its non-thumb
    // URL). Tiles are JPEG renditions, so a non-JPEG source's tile lives only
    // at its honest `…/jpeg/thumb` address — a bare `…/thumb` 301s there.
    if req.thumb {
        if !source_is_jpeg && !req.as_jpeg {
            if !transcode_only {
                return not_found(); // no JPEG rendition exists for PNG/WebP
            }
            return Reply::redirect(&format!("{}/jpeg/thumb", req.base_href));
        }
        let name = format!("{}-thumb.jpg", req.save_stem);
        return match crate::media::thumbnail(req.ext, &req.bytes, req.cache_dir).await {
            crate::media::Prepared::Ready(clean) => {
                let content_type = clean.content_type();
                clean_bytes_response(clean.into_bytes(), content_type, &name, req.source, "public")
            }
            crate::media::Prepared::Withheld => metadata_withheld(),
        };
    }

    // The `/jpeg` rung exists only where a JPEG rendition is the pipeline's
    // own output (transcode-only formats, or a JPEG source). No on-demand
    // format conversion for PNG/WebP — fail closed on the unexpected.
    if req.as_jpeg && !source_is_jpeg && !transcode_only {
        return not_found();
    }

    // `public-original` exact bytes at the bare URL: the container matches the
    // extension, so the name is honest. The `/jpeg` rung on an original still
    // serves the clean rendition (a derived representation, never a leak).
    if req.is_original && !req.as_jpeg {
        let name = format!("{}.{}", req.save_stem, req.ext);
        return clean_bytes_response(req.bytes, &raw_content_type(req.ext), &name, req.source, "public-original");
    }

    // The bare URL of a transcode-only format never serves JPEG bytes under a
    // foreign extension: a styled page explains and links the honest rendition
    // address (was a 303 with a text body).
    if transcode_only && !req.as_jpeg {
        return Reply::html(templates::transcode_explainer_page(req.base_href, req.ext));
    }

    let name = if req.as_jpeg || transcode_only {
        format!("{}.jpg", req.save_stem)
    } else {
        format!("{}.{}", req.save_stem, norm)
    };
    match crate::media::prepare(req.ext, &req.bytes, req.cache_dir).await {
        crate::media::Prepared::Ready(clean) => {
            let content_type = clean.content_type();
            clean_bytes_response(clean.into_bytes(), content_type, &name, req.source, "public")
        }
        crate::media::Prepared::Withheld => metadata_withheld(),
    }
}

/// Build the HTTP response for verify-cleaned bytes (a stripped image or PDF,
/// or an author-opted exact original): a strong ETag derived from the *served*
/// bytes (so it changes iff the served bytes change), a revalidatable cache
/// window, and an honest save-name — the URL's last segment may be a rendition
/// rung (`jpeg`, `thumb`), so the filename must come from the header, not the
/// address. The strips are deterministic, so identical source bytes always
/// yield the same ETag.
fn clean_bytes_response(
    bytes: Vec<u8>,
    content_type: &str,
    filename: &str,
    source: &std::path::Path,
    tag: &'static str,
) -> Reply {
    let etag = format!("\"{}\"", crate::media::content_hash(&bytes));
    let disposition = format!(r#"inline; filename="{}""#, sanitize_filename(filename));
    Reply {
        provenance: Provenance::File { source: source.to_path_buf(), tag },
        status: 200,
        headers: vec![
            ("content-type", content_type.to_string()),
            ("content-disposition", disposition),
            ("etag", etag),
            ("cache-control", "public, max-age=3600".to_string()),
        ],
        body: bytes,
    }
}

/// Fail-closed response when a file cannot be verify-cleaned (a format without a
/// strip path, or corrupt bytes). The page around it still renders; only the
/// bytes are withheld, so nothing with unvetted embedded metadata is ever
/// served. The message names the per-file opt-out so the author knows the way
/// through, without revealing anything to a reader beyond "not served".
fn metadata_withheld() -> Reply {
    Reply::text(
        415,
        "Withheld for privacy: this file may embed metadata (location, device, author) \
         that cannot be verified or removed. The site owner can publish its exact bytes \
         by tagging the file public-original.\n",
    )
}

/// Whether a served document must be jailed as a sandboxed standalone page: the
/// deliberate `.html` drop-in feature plus every HTML/XHTML-family document that
/// would otherwise render as a script-capable top-level document (`.htm`,
/// `.shtml`, `.xhtml`, `.xht`, ...). Delegates to the one canonical predicate
/// (`entry::is_html_document`, MIME-keyed) so the sandbox gate can never drift
/// from the `kind()`/render classification — the drift the old `html`/`htm`-only
/// check caused (`.xhtml` ran script in our origin). Non-HTML document types
/// (`.xml`, `.mathml`, ...) are not served functional-jailed here; the header
/// middleware still hard-jails them via the fail-closed default.
fn is_sandboxed_document(ext: &str) -> bool {
    crate::entry::is_html_document(ext)
}

/// The height reporter injected into the `?embed` copy of a standalone document.
/// It runs inside the jailed iframe (whose CSP permits inline script) and posts
/// its measured height to the parent, which sizes the frame (see the site JS).
/// Only the `?embed` copy carries it; the bare asset URL stays byte-exact.
///
/// The `<script>` *content* is kept free of `<`, `>`, and `&`, so the element is
/// well-formed when the document is served as XML (XHTML) — no CDATA needed.
const EMBED_REPORTER: &str = r#"<script>(function(){
  function h(){var d=document;return Math.max(d.documentElement.scrollHeight, d.body?d.body.scrollHeight:0);}
  function send(){try{parent.postMessage({sajtEmbedHeight:h()},"*");}catch(e){}}
  if(window.ResizeObserver){new ResizeObserver(send).observe(document.documentElement);}
  window.addEventListener("load",send);send();
})();</script>
"#;

/// Insert the reporter *before* the document's closing `</body>` (or `</html>`),
/// so it stays valid even in XML mode (XHTML), where any content after the root
/// element is a fatal parse error — appending after `</html>` would break strict
/// XHTML. Falls back to appending only when neither tag is present (a well-formed
/// XHTML always has them; a malformed one is the author's responsibility).
fn inject_reporter(bytes: Vec<u8>) -> Vec<u8> {
    // Locate the splice point (byte index of the closing tag) while borrowing,
    // then release the borrow before taking ownership of `bytes`.
    let insert_at = match std::str::from_utf8(&bytes) {
        Ok(s) => {
            let lower = s.to_ascii_lowercase();
            lower.rfind("</body>").or_else(|| lower.rfind("</html>"))
        }
        Err(_) => None,
    };
    match insert_at {
        Some(idx) => {
            let mut out = Vec::with_capacity(bytes.len() + EMBED_REPORTER.len());
            out.extend_from_slice(&bytes[..idx]);
            out.extend_from_slice(EMBED_REPORTER.as_bytes());
            out.extend_from_slice(&bytes[idx..]);
            out
        }
        None => {
            let mut b = bytes;
            b.extend_from_slice(EMBED_REPORTER.as_bytes());
            b
        }
    }
}

/// The content type for a sandboxed standalone document: XHTML is served as the
/// strict XML type it asks for (`application/xhtml+xml`, so the browser XML-parses
/// it exactly as the author intended — choosing `.xhtml` is the opt-in), while
/// HTML gets the lenient `text/html`. The file extension is the switch.
fn document_content_type(ext: &str) -> &'static str {
    match mime_guess::from_ext(ext).first_or_octet_stream().essence_str() {
        "application/xhtml+xml" => "application/xhtml+xml; charset=utf-8",
        _ => "text/html; charset=utf-8",
    }
}

/// The content type for a raw file that is NOT served as a sandboxed standalone
/// document. Any type `mime_guess` would render as HTML/XHTML but that we do not
/// treat as a document (`.shtml` and other SSI/HTML-help extensions — we do not
/// process Server-Side Includes) is downgraded to `text/plain`, so it shows as
/// source rather than rendering un-jailed in our origin. Everything else keeps
/// its guessed type (non-HTML documents like SVG/XML are still hard-jailed by the
/// header middleware's fail-closed default).
fn raw_content_type(ext: &str) -> String {
    // The plain-text post formats are declared text/plain outright: the guess
    // table has no entry for `rst`, files `org` under Lotus Organizer and `tex`
    // under an application type, all of which make a browser save the file
    // instead of showing it.
    if crate::entry::is_plain_text_ext(ext) {
        return "text/plain; charset=utf-8".to_string();
    }
    let mime = mime_guess::from_ext(ext).first_or_octet_stream();
    match mime.essence_str() {
        "text/html" | "application/xhtml+xml" => "text/plain; charset=utf-8".to_string(),
        _ => mime.to_string(),
    }
}

/// Serve a standalone HTML/XHTML document's bytes, jailed by the sandbox CSP and
/// served with its intended content type (`content_type`). The `?embed` copy gets
/// the reporter injected well-formed; the bare copy is byte-exact. The CSP is set
/// here so the header middleware leaves it alone; nosniff is added by the
/// middleware (and, on the strict XML type, keeps the browser XML-parsing it).
fn sandbox_html_response(bytes: Vec<u8>, embed: bool, content_type: &'static str) -> Reply {
    let body = if embed { inject_reporter(bytes) } else { bytes };
    // Callers stamp File provenance; the default only stands for the
    // never-reached fallback paths.
    Reply {
        provenance: Provenance::Generated,
        status: 200,
        headers: vec![
            ("content-type", content_type.to_string()),
            ("content-security-policy", crate::security::STANDALONE_CSP.to_string()),
            ("cache-control", "public, max-age=3600".to_string()),
        ],
        body,
    }
}

fn not_found() -> Reply {
    not_found_response(templates::not_found_page())
}

/// A 404 reply wrapping a specific not-found page body. Every not-found path
/// shares this shape, so a hidden path is indistinguishable from a missing one.
fn not_found_response(body: String) -> Reply {
    Reply::html_status(404, body)
}

fn build_filter_description(query: &crate::url::ContentQuery) -> String {
    let mut parts = Vec::new();
    if let Some(ref dp) = query.date_prefix {
        parts.push(human_date(dp));
    }
    for tag in &query.and_tags {
        parts.push(format!("+{}", tag));
    }
    for tag in &query.or_tags {
        parts.push(format!("+{}", tag));
    }
    if let Some(ref label) = query.label {
        parts.push(label.clone());
    }
    parts.join(" ")
}

fn sanitize_filename(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control() && *c != '"' && *c != '\\')
        .collect()
}

fn html_escape_content(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::ContentStore;
    use crate::testutil::{mkdir, set_tags, touch, TmpDir};

    /// Scan a fixture tree into a store, or `None` when the filesystem takes
    /// no xattrs (the tests skip: every served file needs its own tag).
    fn store_of(root: &std::path::Path, public: &[&str]) -> Option<ContentStore> {
        for rel in public {
            if !set_tags(&root.join(rel), &["public"]) {
                return None;
            }
        }
        Some(ContentStore::scan(root, &std::env::temp_dir()).unwrap())
    }

    async fn get(store: &ContentStore, path: &str) -> Reply {
        respond(store, path, &RequestFlags::default()).await
    }

    fn header<'a>(reply: &'a Reply, name: &str) -> Option<&'a str> {
        reply.headers.iter().find(|(k, _)| *k == name).map(|(_, v)| v.as_str())
    }

    /// A folder's file named like another post's language version is the
    /// folder's own file: served in place, never redirected to that version
    /// (DESIGN.md "Languages": a literal file always wins its own name).
    #[tokio::test]
    async fn folder_asset_named_like_a_version_is_the_folders() {
        let t = TmpDir::new();
        touch(t.path(), "photo/photo.md", "# Photo\n\nEnglish.");
        touch(t.path(), "photo/photo.sv.md", "# Foto\n\nSvenska.");
        touch(t.path(), "brev/brev.md", "# Brev\n\nEnglish.");
        touch(t.path(), "brev/photo.sv.md", "an asset of brev, not photo's version");
        let store = match store_of(
            t.path(),
            &["photo", "photo/photo.md", "photo/photo.sv.md", "brev", "brev/brev.md", "brev/photo.sv.md"],
        ) {
            Some(s) => s,
            None => return, // xattr unsupported — skip
        };

        let asset = get(&store, "/brev/photo.sv.md").await;
        assert_eq!(asset.status, 200, "the folder's own file is served in place");
        assert_eq!(asset.body, b"an asset of brev, not photo's version");

        let version = get(&store, "/photo.sv.md").await;
        assert_eq!(version.status, 200);
        assert_eq!(version.body, b"# Foto\n\nSvenska.");
    }

    /// The version address is `/[date/]name.tag`: any other prefix names
    /// nothing, so a folder path or a stray segment never reaches a version.
    #[tokio::test]
    async fn version_head_must_be_a_post_address() {
        let t = TmpDir::new();
        touch(t.path(), "brev/brev.md", "# Brev\n\nEnglish.");
        touch(t.path(), "brev/brev.sv.md", "# Brev\n\nSvenska.");
        let store = match store_of(t.path(), &["brev", "brev/brev.md", "brev/brev.sv.md"]) {
            Some(s) => s,
            None => return, // xattr unsupported — skip
        };

        assert_eq!(get(&store, "/brev.sv").await.status, 200);
        assert_eq!(get(&store, "/x/brev.sv").await.status, 404);
        assert_eq!(get(&store, "/brev/brev.sv").await.status, 404);
        assert_eq!(get(&store, "/x/brev.sv.md").await.status, 404);
    }

    /// A version's file addressed under its folder redirects to the version's
    /// own raw address, like the primary's file does.
    #[tokio::test]
    async fn version_file_under_its_folder_redirects_to_its_address() {
        let t = TmpDir::new();
        touch(t.path(), "brev/brev.md", "# Brev\n\nEnglish.");
        touch(t.path(), "brev/brev.sv.md", "# Brev\n\nSvenska.");
        let store = match store_of(t.path(), &["brev", "brev/brev.md", "brev/brev.sv.md"]) {
            Some(s) => s,
            None => return, // xattr unsupported — skip
        };

        let reply = get(&store, "/brev/brev.sv.md").await;
        assert_eq!(reply.status, 301);
        assert_eq!(header(&reply, "location"), Some("/brev.sv.md"));
        let reply = get(&store, "/brev/brev.md").await;
        assert_eq!(reply.status, 301);
        assert_eq!(header(&reply, "location"), Some("/brev.md"));
    }

    /// A versioned post that is the newer claimant of its name lives at its
    /// date address, and so does its version: `/2026/03/10/brev.sv`, raw at
    /// `/2026/03/10/brev.sv.md`. The bare `/brev.sv` belongs to the owner of
    /// `/brev`, which has no Swedish version, so it is a miss.
    #[tokio::test]
    async fn dated_version_address_of_a_newer_claimant() {
        let t = TmpDir::new();
        // The oldest claim on `brev`: a folder post dated by its marker.
        touch(t.path(), "brev/brev.md", "# Brev\n\nThe old one, English only.");
        mkdir(t.path(), "brev/2026-03-10T1200");
        // The newer claim: a top-level pair, dated by mtime (now).
        touch(t.path(), "brev.md", "# Brev\n\nThe new one.");
        touch(t.path(), "brev.sv.md", "# Brev\n\nDen nya.");
        let store = match store_of(t.path(), &["brev", "brev/brev.md", "brev.md", "brev.sv.md"]) {
            Some(s) => s,
            None => return, // xattr unsupported — skip
        };
        let newer = store
            .entries
            .iter()
            .find(|e| e.dir.is_none() && e.label.as_deref() == Some("brev"))
            .expect("the top-level pair is a post");
        assert_eq!(newer.versions.len(), 1);
        let date = newer.timestamp.date_path();

        let owner = get(&store, "/brev").await;
        assert_eq!(owner.status, 200, "the oldest claim owns the bare name");
        assert_eq!(get(&store, "/brev.sv").await.status, 404, "the owner has no Swedish version");

        let page = get(&store, &format!("{date}/brev.sv")).await;
        assert_eq!(page.status, 200, "the newer claimant's version at its date address");
        assert_eq!(
            header(&page, "link"),
            Some(format!("<{date}/brev>; rel=\"alternate\"; hreflang=\"en\", <{date}/brev.sv>; rel=\"alternate\"; hreflang=\"sv\"").as_str())
        );
        let raw = get(&store, &format!("{date}/brev.sv.md")).await;
        assert_eq!(raw.status, 200);
        assert_eq!(raw.body, b"# Brev\n\nDen nya.");

        // The non-canonical spellings converge on the dated address.
        let reply = get(&store, &format!("{date}/BREV.sv")).await;
        assert_eq!(reply.status, 301);
        assert_eq!(header(&reply, "location"), Some(format!("{date}/brev.sv").as_str()));
    }

    /// A post address is the name, or the name under its date — nothing else.
    /// A stray segment used to be swallowed (`/x/brev` 301'd to `/brev`);
    /// it is a plain 404 now, the same reply as any missing path.
    #[tokio::test]
    async fn a_post_address_is_the_name_or_its_date_form() {
        let t = TmpDir::new();
        touch(t.path(), "brev/brev.md", "# Brev\n\nEnglish.");
        mkdir(t.path(), "brev/2026-03-12T191430");
        let store = match store_of(t.path(), &["brev", "brev/brev.md"]) {
            Some(s) => s,
            None => return, // xattr unsupported — skip
        };

        assert_eq!(get(&store, "/brev").await.status, 200);
        assert_eq!(get(&store, "/2026/brev").await.status, 301, "a date form of the address");
        assert_eq!(get(&store, "/2026/03/12/brev").await.status, 301);
        assert_eq!(get(&store, "/2026/03/12/191430/brev").await.status, 301);
        for at in ["/2026/brev", "/2026/03/12/191430/brev"] {
            assert_eq!(header(&get(&store, at).await, "location"), Some("/brev"));
        }

        // Not addresses: a stray word, a second name, an invalid rung.
        for miss in ["/x/brev", "/foo/bar/brev", "/2026/99/brev", "/2026/13", "/brev/brev"] {
            let reply = get(&store, miss).await;
            assert_eq!(reply.status, 404, "{miss} names nothing");
            assert!(reply.location().is_none(), "{miss} redirects nowhere");
        }
    }

    /// A folder post's namespace hangs off its own address, whatever that is:
    /// the newer claimant of a name is addressed at its date path, so its
    /// assets, its primary redirect and its nested listings live there too.
    /// They used to 404 — the folder routes only ever read a bare first
    /// segment, and refused a four-digit one outright.
    #[tokio::test]
    async fn a_dated_post_owns_its_assets() {
        let t = TmpDir::new();
        // Two folders whose names slug alike: the oldest claim owns `/hej`.
        touch(t.path(), "hej/hej.md", "# Hej\n\nThe old one.");
        touch(t.path(), "hej/photo.txt", "the old one's asset");
        mkdir(t.path(), "hej/2026-03-10T1200");
        touch(t.path(), "Hej!/hej.md", "# Hej\n\nThe new one.");
        touch(t.path(), "Hej!/photo.txt", "the new one's asset");
        touch(t.path(), "Hej!/sub/file.txt", "nested");
        mkdir(t.path(), "Hej!/2026-09-10T1200");
        let store = match store_of(
            t.path(),
            &[
                "hej", "hej/hej.md", "hej/photo.txt",
                "Hej!", "Hej!/hej.md", "Hej!/photo.txt", "Hej!/sub", "Hej!/sub/file.txt",
            ],
        ) {
            Some(s) => s,
            None => return, // xattr unsupported — skip
        };

        assert_eq!(get(&store, "/hej").await.status, 200, "the oldest claim owns the name");
        let old_asset = get(&store, "/hej/photo.txt").await;
        assert_eq!(old_asset.body, b"the old one's asset");

        // The newer claimant lives at its date path, and so does everything
        // under its folder.
        assert_eq!(get(&store, "/2026/09/10/hej").await.status, 200);
        let asset = get(&store, "/2026/09/10/hej/photo.txt").await;
        assert_eq!(asset.status, 200);
        assert_eq!(asset.body, b"the new one's asset");

        // Its primary is canonical at its own raw address — never at the bare
        // name, which is the OLDER post's bytes.
        let primary = get(&store, "/2026/09/10/hej/hej.md").await;
        assert_eq!(primary.status, 301);
        assert_eq!(header(&primary, "location"), Some("/2026/09/10/hej.md"));
        assert_eq!(get(&store, "/hej.md").await.body, b"# Hej\n\nThe old one.");

        // A nested listing under the dated head lists items under that head.
        let nested = get(&store, "/2026/09/10/hej/sub/").await;
        assert_eq!(nested.status, 200);
        let html = String::from_utf8_lossy(&nested.body).into_owned();
        assert!(html.contains("/2026/09/10/hej/sub/file.txt"), "item hrefs hang off the head");
        assert_eq!(get(&store, "/2026/09/10/hej/sub/file.txt").await.body, b"nested");
    }

    /// Inside a folder, the address IS the directory entry's name, byte for
    /// byte. Only a case-folding filesystem (APFS, NTFS) can fail this: there
    /// `dir.join(tail)` used to resolve `Photo.TXT` for a request spelled
    /// `photo.txt` and serve it 200, while Linux 404'd the same URL and the
    /// static build — which emits on-disk names — had no such address at all.
    #[tokio::test]
    async fn asset_address_is_the_entry_name_byte_for_byte() {
        let t = TmpDir::new();
        touch(t.path(), "hej/hej.md", "# Hej");
        touch(t.path(), "hej/Photo.TXT", "the asset");
        let store = match store_of(t.path(), &["hej", "hej/hej.md", "hej/Photo.TXT"]) {
            Some(s) => s,
            None => return, // xattr unsupported — skip
        };

        let asset = get(&store, "/hej/Photo.TXT").await;
        assert_eq!(asset.status, 200, "the on-disk spelling is the address");
        assert_eq!(asset.body, b"the asset");

        for miss in ["/hej/photo.txt", "/hej/PHOTO.TXT", "/hej/Photo.txt"] {
            let reply = get(&store, miss).await;
            assert_eq!(reply.status, 404, "{miss} names no entry");
            assert!(reply.location().is_none(), "{miss} is a miss, never a redirect");
        }
    }

    /// The same rule under Unicode normalization: a name stored in NFD is
    /// served at its NFD spelling only. APFS preserves the bytes but compares
    /// folded, so the NFC spelling used to resolve the NFD file there and 404
    /// everywhere else.
    #[tokio::test]
    async fn asset_address_is_not_normalization_folded() {
        let nfd = "cafe\u{301}.txt"; // c a f e + COMBINING ACUTE
        let nfc = "caf\u{e9}.txt"; // c a f + LATIN SMALL LETTER E WITH ACUTE
        let t = TmpDir::new();
        touch(t.path(), "hej/hej.md", "# Hej");
        touch(t.path(), &format!("hej/{nfd}"), "the asset");
        let store = match store_of(t.path(), &["hej", "hej/hej.md", &format!("hej/{nfd}")]) {
            Some(s) => s,
            None => return, // xattr unsupported — skip
        };

        let asset = get(&store, &format!("/hej/{nfd}")).await;
        assert_eq!(asset.status, 200, "the on-disk bytes are the address");
        assert_eq!(asset.body, b"the asset");

        let reply = get(&store, &format!("/hej/{nfc}")).await;
        assert_eq!(reply.status, 404, "the NFC spelling names no entry");
        assert!(reply.location().is_none(), "a miss, never a redirect");
    }

    /// An operator-owned static file follows the same rule: its address is the
    /// directory entry's name, and a case-folded spelling is a plain 404.
    #[test]
    fn static_asset_address_is_the_entry_name_byte_for_byte() {
        let t = TmpDir::new();
        touch(t.path(), "rain.jpg", "not really a jpeg");

        let hit = static_asset(t.path(), "rain.jpg");
        assert_eq!(hit.status, 200, "the on-disk spelling is the address");
        assert_eq!(hit.body, b"not really a jpeg");

        for miss in ["Rain.jpg", "RAIN.JPG", "rain.JPG"] {
            let reply = static_asset(t.path(), miss);
            assert_eq!(reply.status, 404, "{miss} names no entry");
            assert!(reply.location().is_none(), "{miss} is a miss, never a redirect");
        }
    }

    /// A nested listing is addressed by its directory entry's name too — a
    /// differently-cased subfolder is a plain 404, on every filesystem.
    #[tokio::test]
    async fn nested_listing_address_is_the_entry_name_byte_for_byte() {
        let t = TmpDir::new();
        touch(t.path(), "hej/hej.md", "# Hej");
        touch(t.path(), "hej/Pics/file.txt", "nested");
        let store = match store_of(
            t.path(),
            &["hej", "hej/hej.md", "hej/Pics", "hej/Pics/file.txt"],
        ) {
            Some(s) => s,
            None => return, // xattr unsupported — skip
        };

        let listing = get(&store, "/hej/Pics/").await;
        assert_eq!(listing.status, 200, "the on-disk spelling is the address");
        let html = String::from_utf8_lossy(&listing.body).into_owned();
        assert!(html.contains("/hej/Pics/file.txt"), "item hrefs hang off the entry name");

        for miss in ["/hej/pics/", "/hej/pics", "/hej/PICS/"] {
            let reply = get(&store, miss).await;
            assert_eq!(reply.status, 404, "{miss} names no entry");
            assert!(reply.location().is_none(), "{miss} is a miss, never a redirect");
        }
    }

    /// A folder's namespace hangs off the post's ONE canonical address: every
    /// other spelling of the head — a differently-cased name, an alias marker,
    /// a dated form of a name the post owns, the folder's own on-disk name
    /// when another post owns that address — 301s onto it with the tail
    /// verbatim, exactly as the post's page does. They used to serve the bytes
    /// in place, giving one file several addresses.
    #[tokio::test]
    async fn a_non_canonical_head_redirects_to_the_canonical_one() {
        let t = TmpDir::new();
        touch(t.path(), "hej/hej.md", "# Hej\n\nThe old one.");
        touch(t.path(), "hej/photo.txt", "the old one's asset");
        touch(t.path(), "hej/sub/file.txt", "nested");
        mkdir(t.path(), "hej/2026-03-12T1914");
        mkdir(t.path(), "hej/alias gammal");
        touch(t.path(), "Hej!/hej.md", "# Hej\n\nThe new one.");
        touch(t.path(), "Hej!/photo.txt", "the new one's asset");
        mkdir(t.path(), "Hej!/2026-09-10T1200");
        let store = match store_of(
            t.path(),
            &[
                "hej", "hej/hej.md", "hej/photo.txt", "hej/sub", "hej/sub/file.txt",
                "Hej!", "Hej!/hej.md", "Hej!/photo.txt",
            ],
        ) {
            Some(s) => s,
            None => return, // xattr unsupported — skip
        };
        let all: Vec<&Entry> = store.entries.iter().collect();
        let owner = templates::name_owner("Hej!", &all).expect("the name resolves");
        assert_eq!(
            owner.id.file_name().and_then(|n| n.to_str()),
            Some("hej"),
            "both folders slug to `hej`, and the oldest claim owns it",
        );

        for (requested, canonical) in [
            ("/HEJ/photo.txt", "/hej/photo.txt"),
            ("/2026/03/12/hej/photo.txt", "/hej/photo.txt"),
            ("/gammal/photo.txt", "/hej/photo.txt"),
            ("/Hej!/photo.txt", "/hej/photo.txt"),
            ("/HEJ/sub/", "/hej/sub/"),
            // Decided from the address alone, so a missing file redirects too
            // and 404s in the canonical scope like any other missing path.
            ("/HEJ/missing.txt", "/hej/missing.txt"),
        ] {
            let reply = get(&store, requested).await;
            assert_eq!(reply.status, 301, "{requested} converges on one address");
            assert_eq!(header(&reply, "location"), Some(canonical), "{requested}");
        }
        assert_eq!(get(&store, "/hej/missing.txt").await.status, 404);

        // The canonical spellings serve in place, each post its own files.
        assert_eq!(get(&store, "/hej/photo.txt").await.body, b"the old one's asset");
        assert_eq!(
            get(&store, "/2026/09/10/hej/photo.txt").await.body,
            b"the new one's asset"
        );
        assert_eq!(get(&store, "/hej/sub/").await.status, 200);
    }

    /// An unlabeled post is addressed at its date and time, and its folder's
    /// namespace hangs off that address like any other post's.
    #[tokio::test]
    async fn an_unlabeled_post_owns_its_assets() {
        let t = TmpDir::new();
        touch(t.path(), "2026-03-25T1915 Trip/index.md", "# Trip\n\nWhere we went.");
        touch(t.path(), "2026-03-25T1915 Trip/notes.txt", "what happened");
        let store = match store_of(
            t.path(),
            &[
                "2026-03-25T1915 Trip",
                "2026-03-25T1915 Trip/index.md",
                "2026-03-25T1915 Trip/notes.txt",
            ],
        ) {
            Some(s) => s,
            None => return, // xattr unsupported — skip
        };
        assert_eq!(store.entries[0].slug, None, "a date-named post claims no name");

        assert_eq!(get(&store, "/2026/03/25/191500").await.status, 200);
        let asset = get(&store, "/2026/03/25/191500/notes.txt").await;
        assert_eq!(asset.status, 200);
        assert_eq!(asset.body, b"what happened");
    }

    /// A post may simply be named `42`: it owns the bare address, its assets
    /// hang off it, and the date form of the address 301s there.
    #[tokio::test]
    async fn a_numeric_name_is_an_ordinary_name() {
        let t = TmpDir::new();
        touch(t.path(), "42/42.md", "# 42\n\nThe answer.");
        touch(t.path(), "42/notes.txt", "the question");
        mkdir(t.path(), "42/2026-03-25");
        let store = match store_of(t.path(), &["42", "42/42.md", "42/notes.txt"]) {
            Some(s) => s,
            None => return, // xattr unsupported — skip
        };

        assert_eq!(get(&store, "/42").await.status, 200);
        let dated = get(&store, "/2026/03/25/42").await;
        assert_eq!(dated.status, 301, "the date form of the address");
        assert_eq!(header(&dated, "location"), Some("/42"));
        assert_eq!(get(&store, "/42/notes.txt").await.body, b"the question");
        // A four-digit name stays the year view's, and is date-addressed.
        assert_eq!(get(&store, "/2026").await.status, 200, "the year view");
    }

    /// A scope names no post: `/2026` is the year view even when exactly one
    /// post is dated then, so a version address built on it resolves nothing.
    #[tokio::test]
    async fn a_scope_never_resolves_to_a_post() {
        let t = TmpDir::new();
        touch(t.path(), "brev/brev.md", "# Brev\n\nEnglish.");
        touch(t.path(), "brev/brev.sv.md", "# Brev\n\nSvenska.");
        mkdir(t.path(), "brev/2026-03-12T1914");
        let store = match store_of(t.path(), &["brev", "brev/brev.md", "brev/brev.sv.md"]) {
            Some(s) => s,
            None => return, // xattr unsupported — skip
        };

        assert_eq!(get(&store, "/brev.sv").await.status, 200);
        assert_eq!(get(&store, "/2026.sv").await.status, 404, "a year is no post");
        assert_eq!(get(&store, "/2026/03/12.sv").await.status, 404);
    }

    /// A post with no address of its own is a row that states the problem
    /// where a reader will see it — on the timeline and in the listing its
    /// address collides with — and links nowhere, because there is no page.
    #[tokio::test]
    async fn a_post_with_no_address_is_an_error_row() {
        let t = TmpDir::new();
        touch(t.path(), "2026-03-25 Trip/index.md", "# Trip\n\nDay precision.");
        let store = match store_of(t.path(), &["2026-03-25 Trip", "2026-03-25 Trip/index.md"]) {
            Some(s) => s,
            None => return, // xattr unsupported — skip
        };

        for at in ["/", "/2026/03/25"] {
            let body = String::from_utf8_lossy(&get(&store, at).await.body).into_owned();
            let row = body
                .split("<article")
                .find(|part| part.contains("needs attention"))
                .unwrap_or_else(|| panic!("{at} shows the errored post"));
            assert!(row.contains("data-error"), "{at} marks the row as an error");
            assert!(row.contains("<code>/2026/03/25</code>"), "{at} names the address");
            assert!(!row.contains("<a href"), "{at} links nowhere — there is no page");
        }
    }

    /// An archived snapshot owns its address and its bytes: its page at its
    /// own date path, its source at that address plus the copy's extension.
    /// The source link used to point at the CURRENT file's bytes, and no raw
    /// address served a snapshot at all — which made `sajt build` report a
    /// broken link on any site holding a ` copy` file.
    #[tokio::test]
    async fn a_snapshot_serves_its_own_bytes_at_its_own_address() {
        let t = TmpDir::new();
        // A date marker fixes the post's own date, so the copy's mtime (now)
        // can never be mistaken for the current post's address.
        touch(t.path(), "hej/hej.md", "# Hej\n\nNu.");
        touch(t.path(), "hej/hej copy.md", "# Hej\n\nTidigare.");
        mkdir(t.path(), "hej/2026-03-03T1430");
        let store = match store_of(t.path(), &["hej", "hej/hej.md", "hej/hej copy.md"]) {
            Some(s) => s,
            None => return, // xattr unsupported — skip
        };
        let post = &store.entries[0];
        assert_eq!(post.revisions.len(), 1);
        let at = post.revisions[0].date.format("/%Y/%m/%d/%H%M%S").to_string();

        let page = get(&store, &format!("{at}/hej")).await;
        assert_eq!(page.status, 200, "the snapshot's own address");
        let html = String::from_utf8_lossy(&page.body).into_owned();
        assert!(html.contains("Tidigare"), "the copy's own words");
        assert!(html.contains(&format!("{at}/hej.md")), "its source link is its own bytes");

        let raw = get(&store, &format!("{at}/hej.md")).await;
        assert_eq!(raw.status, 200);
        assert_eq!(raw.body, b"# Hej\n\nTidigare.");
        // The current file keeps its own address and bytes.
        assert_eq!(get(&store, "/hej.md").await.body, b"# Hej\n\nNu.");

        // A coarser spelling that still names one snapshot converges on the
        // full address.
        let month = post.revisions[0].date.format("/%Y/%m").to_string();
        let coarse = get(&store, &format!("{month}/hej")).await;
        assert_eq!(coarse.status, 301);
        assert_eq!(header(&coarse, "location"), Some(format!("{at}/hej").as_str()));
    }

    /// A version's archived snapshot is served at the version's date-path
    /// address (`/Y/M/D/HHMMSS/brev.sv`), exactly as the site-language file's
    /// snapshot is at its own — and, being one frozen file, it carries no
    /// language alternates.
    #[tokio::test]
    async fn version_revision_serves_at_its_dated_address() {
        let t = TmpDir::new();
        // A date marker fixes the post's own date, so the snapshot's mtime
        // (now) can never be mistaken for the current post's address.
        touch(t.path(), "brev/brev.md", "# Brev\n\nEnglish.");
        mkdir(t.path(), "brev/2026-03-03T1430");
        touch(t.path(), "brev/brev.sv.md", "# Brev\n\nSvenska nu.");
        touch(t.path(), "brev/brev.sv copy.md", "# Brev\n\nSvenska tidigare.");
        let store = match store_of(
            t.path(),
            &["brev", "brev/brev.md", "brev/brev.sv.md", "brev/brev.sv copy.md"],
        ) {
            Some(s) => s,
            None => return, // xattr unsupported — skip
        };
        let version = store.entries[0].version("sv").expect("the Swedish version");
        assert_eq!(version.revisions.len(), 1);
        let at = version.revisions[0].date.format("/%Y/%m/%d/%H%M%S").to_string();

        let page = get(&store, &format!("{at}/brev.sv")).await;
        assert_eq!(page.status, 200, "the snapshot's own address");
        assert!(String::from_utf8_lossy(&page.body).contains("Svenska tidigare"));
        assert_eq!(header(&page, "link"), None, "a snapshot has no counterpart in another language");
        // The snapshot's bytes are its own, at its own raw address.
        let raw = get(&store, &format!("{at}/brev.sv.md")).await;
        assert_eq!(raw.status, 200);
        assert_eq!(raw.body, b"# Brev\n\nSvenska tidigare.");
        // The extension must be the copy's; another one names nothing.
        assert_eq!(get(&store, &format!("{at}/brev.sv.txt")).await.status, 404);

        // A date that names no snapshot, and a head that is not a post
        // address, both name nothing.
        assert_eq!(get(&store, "/2026/01/01/000000/brev.sv").await.status, 404);
        assert_eq!(get(&store, &format!("/x{at}/brev.sv")).await.status, 404);

        // Non-canonical spelling converges on the one address.
        let reply = get(&store, &format!("{at}/BREV.sv")).await;
        assert_eq!(reply.status, 301);
        assert_eq!(header(&reply, "location"), Some(format!("{at}/brev.sv").as_str()));

        // The version page carries the nav that leads there; the
        // site-language page has no revisions of its own to show.
        let version_page = get(&store, "/brev.sv").await;
        assert!(String::from_utf8_lossy(&version_page.body).contains(&format!("{at}/brev.sv")));
        assert!(!String::from_utf8_lossy(&get(&store, "/brev").await.body).contains("earlier revision"));
    }
}
