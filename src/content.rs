use crate::embed::EmbedData;
use crate::entry::{Entry, PostError, Revision, COPY_KEYWORD};
use crate::tags::{read_tags_colored, Tag};
use chrono::{DateTime, FixedOffset, Local, NaiveDate, NaiveDateTime, TimeZone};
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
}

impl ContentStore {
    /// Scan the content directory into posts, newest first (by publish date).
    ///
    /// A post is a bare file (publish date = mtime) or a folder (publish date =
    /// its empty date-marker subfolder, else the primary file's mtime). The scan
    /// never writes; a malformed post becomes an errored `Entry` rather than
    /// aborting the whole scan or serving the wrong bytes. See `entry-model.md`.
    pub fn scan(content_dir: &Path, cache_dir: &Path) -> std::io::Result<Self> {
        let entries = scan_entries(content_dir)?;
        tracing::info!("Scanned {} entries from {}", entries.len(), content_dir.display());
        Ok(ContentStore {
            entries,
            content_dir: content_dir.to_path_buf(),
            cache_dir: cache_dir.to_path_buf(),
            embed_cache: HashMap::new(),
        })
    }

    /// Re-scan the content directory, replacing all entries.
    pub fn rescan(&mut self) -> std::io::Result<()> {
        let new = Self::scan(&self.content_dir, &self.cache_dir)?;
        self.entries = new.entries;
        self.embed_cache.clear();
        Ok(())
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
        if name.starts_with('.') {
            continue; // dotfiles: .DS_Store, .claude, the grade ledger
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
    Ok(entries)
}

/// Build a post from a top-level item: a bare file or a folder.
fn build_post(item: &TopItem) -> Entry {
    if item.is_dir {
        build_folder_post(item)
    } else {
        build_bare_post(item)
    }
}

/// A bare-file post: the file itself is the content, its mtime is the publish
/// date, its stem is the label. Editing it moves the mtime (republishes) — by
/// design; fold it into a folder to gain a stable date, assets, or aliases.
fn build_bare_post(item: &TopItem) -> Entry {
    let (stem, ext) = split_name(&item.name);
    let tags = read_tags_colored(&item.path);
    let timestamp = mtime_local(&item.path).unwrap_or_else(epoch);

    // Copies were already routed away, so a space in a bare label means the name
    // parsed as no known grammar (a typo or look-alike char) — fail closed.
    let error = if stem.contains(' ') {
        Some(PostError::UnparseableName(item.name.clone()))
    } else {
        None
    };

    Entry {
        path: item.path.clone(),
        dir: None,
        timestamp,
        edited: None,
        label: Some(stem.to_string()),
        display_label: None,
        extension: ext.to_string(),
        tags,
        grade: None,
        aliases: Vec::new(),
        revisions: Vec::new(),
        error,
    }
}

/// A folder post: one primary content file, optional assets, and empty marker
/// folders (a single date marker, any number of `alias <name>/`).
fn build_folder_post(item: &TopItem) -> Entry {
    let dir = &item.path;
    let label = item.name.clone();
    // Finder tags are read at the post level — here, the folder itself.
    let tags = read_tags_colored(dir);

    // A spaced folder name is not a valid post name (aliases live inside; copies
    // were routed away) — fail closed.
    if label.contains(' ') {
        return errored_folder(item, tags, PostError::UnparseableName(label));
    }

    let scan = match scan_folder(dir, &label) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to read folder post {}: {}", dir.display(), e);
            return errored_folder(item, tags, PostError::NoPrimary);
        }
    };

    if let Some(err) = scan.error {
        return errored_folder(item, tags, err);
    }

    let primary = scan.primary.expect("a folder post with no error has a primary");
    let (_, ext) = split_name(&primary.name);

    // Publish date: the date marker, else the primary's mtime. The edited line is
    // the primary's mtime, shown only when it is meaningfully later than publish.
    let (timestamp, edited) = match scan.date_marker {
        Some(marker) => {
            let edited = (primary.mtime > marker + chrono::Duration::minutes(1)).then_some(primary.mtime);
            (marker, edited)
        }
        None => (primary.mtime, None),
    };

    Entry {
        path: primary.path,
        dir: Some(dir.clone()),
        timestamp,
        edited,
        label: Some(label),
        display_label: None,
        extension: ext.to_string(),
        tags,
        grade: None,
        aliases: scan.aliases,
        revisions: scan.revisions,
        error: None,
    }
}

/// A folder post that scanned wrong: still an entry (so it renders as a loud
/// error, never vanishing), dated by the folder's own mtime.
fn errored_folder(item: &TopItem, tags: Vec<Tag>, error: PostError) -> Entry {
    Entry {
        path: item.path.clone(),
        dir: Some(item.path.clone()),
        timestamp: mtime_local(&item.path).unwrap_or_else(epoch),
        edited: None,
        label: Some(item.name.clone()),
        display_label: None,
        extension: String::new(),
        tags,
        grade: None,
        aliases: Vec::new(),
        revisions: Vec::new(),
        error: Some(error),
    }
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

/// The result of scanning a folder post's contents.
struct FolderScan {
    primary: Option<PrimaryFile>,
    date_marker: Option<NaiveDateTime>,
    aliases: Vec<String>,
    revisions: Vec<Revision>,
    error: Option<PostError>,
}

fn scan_folder(dir: &Path, label: &str) -> std::io::Result<FolderScan> {
    let mut date_markers: Vec<(String, NaiveDateTime)> = Vec::new();
    let mut aliases: Vec<String> = Vec::new();
    let mut files: Vec<FileChild> = Vec::new();

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
            // Subfolders are markers or ignored — never assets, never served.
            if let Some(dt) = parse_date_marker(&cname) {
                if is_effectively_empty(&cpath) {
                    date_markers.push((cname, dt));
                } else {
                    tracing::debug!("Ignoring non-empty date-named subfolder in {}: {}", dir.display(), cname);
                }
            } else if let Some(alias) = parse_alias_marker(&cname) {
                aliases.push(alias);
            } else {
                tracing::debug!("Ignoring subfolder in {}: {}", dir.display(), cname);
            }
        } else {
            let stem = split_name(&cname).0.to_string();
            let mtime = mtime_local(&cpath).unwrap_or_else(epoch);
            files.push(FileChild { name: cname, stem, path: cpath, mtime });
        }
    }

    // Exactly one date marker, or none.
    if date_markers.len() > 1 {
        let mut names: Vec<String> = date_markers.into_iter().map(|(n, _)| n).collect();
        names.sort();
        return Ok(FolderScan {
            primary: None,
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

    // Primary = the file whose stem is `index` or equals the folder name; failing
    // that, the sole plain file. Anything else is a fail-closed ambiguity.
    let candidates: Vec<&FileChild> = plain
        .iter()
        .filter(|f| f.stem == label || f.stem.eq_ignore_ascii_case("index"))
        .collect();
    let primary_child: Option<&FileChild> = match candidates.len() {
        1 => Some(candidates[0]),
        0 if plain.len() == 1 => Some(&plain[0]),
        _ => None,
    };

    let primary_child = match primary_child {
        Some(p) => p,
        None => {
            let error = if plain.is_empty() {
                PostError::NoPrimary
            } else {
                let mut names: Vec<String> = if candidates.len() > 1 {
                    candidates.iter().map(|f| f.name.clone()).collect()
                } else {
                    plain.iter().map(|f| f.name.clone()).collect()
                };
                names.sort();
                PostError::AmbiguousPrimary(names)
            };
            return Ok(FolderScan { primary: None, date_marker, aliases, revisions: Vec::new(), error: Some(error) });
        }
    };

    // Revisions = ` copy [n]` files that snapshot the primary (share its stem).
    let primary_stem = primary_child.stem.clone();
    let mut revisions: Vec<Revision> = copies
        .iter()
        .filter(|(_, base, _)| *base == primary_stem)
        .map(|(f, _, rank)| Revision { date: f.mtime, path: f.path.clone(), rank: *rank })
        .collect();
    sort_revisions(&mut revisions);

    let primary = PrimaryFile {
        name: primary_child.name.clone(),
        path: primary_child.path.clone(),
        mtime: primary_child.mtime,
    };

    Ok(FolderScan { primary: Some(primary), date_marker, aliases, revisions, error: None })
}

/// Attach each top-level ` copy [n]` sibling to the post it revises (same label,
/// preferring the matching kind: a folder copy revises a folder post, a file
/// copy a bare file). An orphan copy — one with no current post — is logged and
/// dropped, since the timeline only shows the current revision.
fn attach_revisions(entries: &mut [Entry], revisions: &[TopItem]) {
    for rev in revisions {
        let stem = if rev.is_dir { rev.name.clone() } else { split_name(&rev.name).0.to_string() };
        let (base, rank) = match parse_revision_suffix(&stem) {
            Some(v) => v,
            None => continue, // only copies reach here
        };
        let (rev_path, rev_mtime) = match revision_primary(rev) {
            Some(v) => v,
            None => {
                tracing::warn!("Skipping unreadable archived revision: {}", rev.path.display());
                continue;
            }
        };

        let mut chosen: Option<usize> = None;
        for (i, e) in entries.iter().enumerate() {
            if e.error.is_some() || e.label.as_deref() != Some(base.as_str()) {
                continue;
            }
            match chosen {
                None => chosen = Some(i),
                Some(c) => {
                    let cur_matches_kind = entries[c].dir.is_some() == rev.is_dir;
                    let e_matches_kind = e.dir.is_some() == rev.is_dir;
                    if e_matches_kind && !cur_matches_kind {
                        chosen = Some(i);
                    }
                }
            }
        }

        match chosen {
            Some(i) => {
                entries[i].revisions.push(Revision { date: rev_mtime, path: rev_path, rank });
                sort_revisions(&mut entries[i].revisions);
            }
            None => tracing::warn!(
                "Archived revision '{}' has no current post named '{}'; not shown",
                rev.name,
                base
            ),
        }
    }
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

/// Parse an empty date-marker folder name into a publish date, normalized to
/// site-local. Format: ISO-8601 basic, colon-free `YYYY-MM-DDTHHMM[SS]` with an
/// optional zone (`Z` or `±HHMM`); a bare (zoneless) marker is already local.
/// Returns `None` for any name that is not such a date.
fn parse_date_marker(name: &str) -> Option<NaiveDateTime> {
    let (body, offset) = split_zone(name)?;
    let naive = NaiveDateTime::parse_from_str(body, "%Y-%m-%dT%H%M%S")
        .or_else(|_| NaiveDateTime::parse_from_str(body, "%Y-%m-%dT%H%M"))
        .ok()?;
    match offset {
        None => Some(naive),
        Some(off) => off
            .from_local_datetime(&naive)
            .single()
            .map(|dt| dt.with_timezone(&Local).naive_local()),
    }
}

/// Split an optional trailing timezone (`Z` or `±HHMM`) off a datetime marker.
/// Returns `None` only when the name has no `T` at all (so it cannot be a marker).
fn split_zone(name: &str) -> Option<(&str, Option<FixedOffset>)> {
    let t_idx = name.find('T')?;

    if let Some(body) = name.strip_suffix('Z') {
        return Some((body, Some(FixedOffset::east_opt(0)?)));
    }

    // A numeric `±HHMM` zone sits after the time, so its sign is past the `T`;
    // the date's own hyphens are before it and never match.
    if name.len() >= 5 {
        let sign_idx = name.len() - 5;
        let bytes = name.as_bytes();
        if sign_idx > t_idx && (bytes[sign_idx] == b'+' || bytes[sign_idx] == b'-') {
            let digits = &name[sign_idx + 1..];
            if digits.bytes().all(|b| b.is_ascii_digit()) {
                let h: i32 = name[sign_idx + 1..sign_idx + 3].parse().ok()?;
                let m: i32 = name[sign_idx + 3..sign_idx + 5].parse().ok()?;
                if h <= 23 && m <= 59 {
                    let secs = h * 3600 + m * 60;
                    let off = if bytes[sign_idx] == b'+' {
                        FixedOffset::east_opt(secs)
                    } else {
                        FixedOffset::west_opt(secs)
                    }?;
                    return Some((&name[..sign_idx], Some(off)));
                }
            }
        }
    }

    Some((name, None))
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

    /// A throwaway directory under the OS temp dir, removed on drop. Avoids a
    /// dev-dependency; uniqueness is pid + a process-wide counter.
    struct TmpDir(PathBuf);
    impl TmpDir {
        fn new() -> Self {
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let mut p = std::env::temp_dir();
            p.push(format!("esko-scan-{}-{}", std::process::id(), n));
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
        let hms = NaiveDate::from_ymd_opt(2026, 3, 3).unwrap().and_hms_opt(14, 30, 52).unwrap();
        let hm = NaiveDate::from_ymd_opt(2026, 3, 3).unwrap().and_hms_opt(14, 30, 0).unwrap();
        assert_eq!(parse_date_marker("2026-03-03T143052"), Some(hms));
        assert_eq!(parse_date_marker("2026-03-03T1430"), Some(hm));
        // Zoned markers parse (exact local value depends on the machine tz).
        assert!(parse_date_marker("2026-03-03T143052Z").is_some());
        assert!(parse_date_marker("2026-03-03T1430+0200").is_some());
        assert!(parse_date_marker("2026-03-03T1430-0500").is_some());
        // Non-dates.
        assert_eq!(parse_date_marker("hello-world"), None);
        assert_eq!(parse_date_marker("alias resume"), None);
        assert_eq!(parse_date_marker("2026-13-03T1430"), None); // bad month
        assert_eq!(parse_date_marker("2026-03-03"), None); // no time
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
        let entries = scan_entries(t.path()).unwrap();
        assert_eq!(entries.len(), 1);
        let e = find(&entries, "hello-world");
        assert_eq!(e.extension, "md");
        assert!(e.dir.is_some());
        assert!(e.error.is_none());
        let expect = NaiveDate::from_ymd_opt(2026, 3, 3).unwrap().and_hms_opt(14, 30, 0).unwrap();
        assert_eq!(e.timestamp, expect);
        // The primary was written just now, well after the 2026 marker → edited.
        assert!(e.edited.is_some());
    }

    #[test]
    fn folder_post_index_primary() {
        let t = TmpDir::new();
        touch(t.path(), "notes/index.md", "body");
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
        let entries = scan_entries(t.path()).unwrap();
        let e = find(&entries, "shot");
        assert_eq!(e.extension, "jpg");
        assert!(e.path.ends_with("whatever.jpg"));
        assert!(e.error.is_none());
    }

    #[test]
    fn folder_post_multiple_primaries_error() {
        let t = TmpDir::new();
        touch(t.path(), "p/p.md", "a");
        touch(t.path(), "p/index.md", "b");
        let entries = scan_entries(t.path()).unwrap();
        let e = find(&entries, "p");
        assert!(matches!(e.error, Some(PostError::AmbiguousPrimary(_))));
    }

    #[test]
    fn folder_post_ambiguous_no_candidate_error() {
        let t = TmpDir::new();
        touch(t.path(), "p/a.txt", "a");
        touch(t.path(), "p/b.txt", "b");
        let entries = scan_entries(t.path()).unwrap();
        let e = find(&entries, "p");
        assert!(matches!(e.error, Some(PostError::AmbiguousPrimary(_))));
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
        let entries = scan_entries(t.path()).unwrap();
        assert_eq!(entries.len(), 1);
        let e = find(&entries, "note");
        assert_eq!(e.revisions.len(), 1);
    }

    #[test]
    fn spaced_name_fails_closed() {
        let t = TmpDir::new();
        touch(t.path(), "my file.md", "oops");
        let entries = scan_entries(t.path()).unwrap();
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert!(matches!(e.error, Some(PostError::UnparseableName(_))));
    }

    #[test]
    fn embed_cache_dir_is_ignored() {
        let t = TmpDir::new();
        touch(t.path(), "x.link", "https://example.com");
        touch(t.path(), "x.link.embed-cache/meta.json5", "{}");
        let entries = scan_entries(t.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].label.as_deref(), Some("x"));
        assert_eq!(entries[0].extension, "link");
    }

    #[test]
    fn dotfiles_and_missing_dir() {
        // Missing content dir → empty, no error.
        let missing = std::env::temp_dir().join(format!("esko-missing-{}", std::process::id()));
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
