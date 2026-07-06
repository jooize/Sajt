# Entry model — bare files + folder posts, portable dates (Phase 2a design)

Status: DECIDED 2026-07-06 (design conversation). Supersedes the flat
timestamped-filename convention AND the 2026-07-05 folder-only/`.id`/auto-fold
draft of this document. Referenced from `DESIGN.md`. Build order: this spec ->
scanner rewrite -> one-shot migration.

## Principles (settled)

- **No software required on Mac or iPhone.** Authoring is: name a file or
  folder, optionally tag it (Finder tags), sync it. Everything durable is
  user-authored filesystem structure.
- **The server never writes into the content tree.** Content is read-only to
  it. All derived state (embed cache, index, computed grade buckets) lives
  outside the content dir and is disposable/regenerable. Sync is one-way
  (authoring devices -> server); nothing syncs back. A compromised server
  cannot alter or leak-by-modification any content.
- **Fail closed.** Ambiguity (name collisions, multiple candidates) renders an
  error page naming the conflict; never guess, never serve the wrong bytes
  under a stable URL.
- An optional companion app is a *concierge*, never a dependency: it can fold
  bare files, stamp date markers, and record grades — but the site is fully
  functional without it having ever run.

## Why not filesystem creation dates

- **macOS "Date Added"** (`kMDItemDateAdded` / `ATTR_CMN_ADDEDTIME`) is
  Apple-only, and resets to the copy moment on any transfer (iCloud, rsync) —
  it cannot travel.
- **Linux can read btime** (`statx`, ext4/xfs/btrfs) but has **no syscall to
  set it**; `rsync`/`cp -a` cannot preserve it. Birthtime cannot be canonical.
- **mtime is the one universal, settable, sync-preserved timestamp**
  (`rsync -t`, iCloud, `cp -p`, `touch -t`, all platforms). The model leans on
  it.

## The model

A post is **either a bare file or a folder** in the content tree.

### Bare-file post

`hello-world.md` at the top level is a complete post:

- label/address = filename stem (`/hello-world`), type/raw view = extension,
- **publish date = mtime** (read as local wall-clock),
- edited date = same thing, so no separate "edited" line is shown.

Trade, by design: **editing a bare file republishes it** (mtime moves, the
post jumps to the top of the timeline). When that is not wanted — or the post
needs assets, aliases, or revisions — fold it into a folder (below). Bare
files are the zero-friction entry path; folders are the upgrade path.
**Folding is itself a native gesture**: select the file, Ctrl-Cmd-N ("New
Folder with Selection") — Finder creates the folder around it; name it after
the file's stem. No app needed.

`foo.md` and `foo/` both present -> both are claims on `/foo`; the oldest
claim wins the bare URL (see Name resolution below).

### Folder post

The folder name is the label and the address: `hello-world/` -> `/hello-world`.
Inside:

- exactly one **primary content file** (bytes, extension, type, `/label.ext`
  raw view),
- zero or more **asset files** (images etc.),
- optional **marker folders** (date, alias — below) and dotfiles; never served,
  never competing with content.

```
hello-world/
  hello-world.md
  2026-03-03T1430/        <- publish date marker (empty folder)
  alias hi-world/         <- extra address (empty folder)
```

### Publish date — an empty, date-named folder

- **Editable on any device**: "New Folder -> rename" is first-class in Finder
  and the iOS Files app; dotfiles and xattrs are not.
- **Format:** ISO-8601 basic, colon-free (`:` is illegal on FAT/exFAT/Windows):
  `YYYY-MM-DDTHHMM[SS]` with optional zone (`Z` or `+-HHMM`). Lenient parse;
  zone defaults to site-local. Examples: `2026-03-03T1430`,
  `2026-03-03T143052Z`, `2026-03-03T1430+0200`.
- **"Empty"** = no non-dotfile children (`.DS_Store` does not disqualify).
- **Exactly one** per folder post. **Zero** -> the post has no stable date and
  falls back to the primary file's mtime (same semantics as a bare file).
  **Two or more** -> error page.
- Birthtime / Date Added are never consulted.

### Edited date — primary content mtime

Shown as a secondary "edited" line when meaningfully later than the publish
date (folder posts only; a bare file's mtime *is* its publish date).

### Primary content resolution (folder posts)

Considering only **regular files** (dotfiles and all subfolders excluded):

1. **Candidates** = files whose stem is `index` **or** equals the folder name
   (any extension), e.g. in `foo/`: `index.md`, `foo.md`.
2. Exactly one candidate -> primary.
3. More than one candidate -> **error page**.
4. No candidate but exactly one non-dotfile file -> that file is primary
   (copy a file in under its real name; no rename needed).
5. Otherwise -> **error page**.

All other regular files are assets. **No symlinks** (they do not survive
iCloud/zip/rsync-without-`-l`, and rule 4 removes the need).

## Identity — no `.id`; `alias` marker folders

There is **no server-assigned identity**. Identity = the post's name, plus
user-declared aliases:

- An **empty folder named `alias <name>`** inside a post makes `/<name>` an
  additional address for it (301 to canonical). Same gesture as the date
  marker; the space cannot occur in real labels (labels are hyphenated), so
  the verb prefix is unambiguous. This one mechanism covers:
  1. rename survival (`alias cookie-tests/` after renaming the post),
  2. alternative names that never were the name (`alias resume/` on `cv/`),
  3. shortlinks (`alias t3/`).
- Bare-file posts cannot carry aliases (nowhere to put the marker) — fold
  first.
- A post's aliases are listed on its entry page (small, server-rendered), so
  aliases are never fully invisible even though Finder cannot warn about them
  at folder-creation time.
- Cost accepted: renaming without leaving an alias orphans that post's grades
  and inbound links. Single-author site; the author knows the rule.

The verb-prefix grammar (`alias <name>/`; the date marker is the prefixless
case) leaves room for future markers (`draft/`, `unlisted/`, ...).

## Revisions — Finder's own ` copy [n]` suffix

**The unsuffixed name is always the current post.** Before editing, Cmd-D
freezes the old state: `bacon-stuff copy/`, `bacon-stuff copy 2/`, ... are
**archived revisions of `bacon-stuff/`** — the copy is the archive, never
the successor. The duplicate gesture IS the revision gesture; the workflow
is **Cmd-D, then edit the original — nothing else** (no renames, no marker
edits). One family shares the label `bacon-stuff`.

- **Current = the unsuffixed entry, structurally.** It keeps the bare
  `/bacon-stuff` URL, its tags, and its timeline position (its date marker
  stays put — an edited post does not move on the timeline; the "edited"
  line covers that).
- **Copies' dates need no attention**: a ` copy` entry **ignores its
  (Cmd-D-inherited) date marker**; its revision date = its primary file's
  mtime, which Finder copy preserves = the last time that revision's content
  was edited. Exactly right, fully automatic. (Corollary: don't edit
  archives — touching a copy's file moves its revision date.)
- Among copies, ordering = revision date; suffix rank (`copy` < `copy 2` <
  ...) breaks exact ties. Deleting any copy never breaks the family.
- **File-level snapshots, same gesture and direction**: inside a post
  folder, Cmd-D on the primary file yields `<stem> copy.md` — files whose
  stem is `<primary-stem> copy [n]` are **revision snapshots** (not assets,
  not primary-resolution errors), dated by their own mtime. The cheap 90%
  path; a sibling folder copy (frozen tags + assets) is for revisions that
  deserve full-post status.
- **Timeline shows only the current revision**, with a subtle "3 revisions"
  note on rows that have them, expandable in place (`<details>`-style, no
  JS required) to list the archived revisions with their dates, each
  linking to its date-path URL (`/2026/03/12/bacon-stuff`, `?time=`
  same-day) — NOT `/bacon-stuff/2`, which would squat the `/name/asset`
  namespace. The current revision's entry page carries the same list as a
  small revision nav.
- **Localization caveat**: Finder writes "copy" only in English ("kopia",
  "Kopie", ...). The keyword lives in one constant, configurable later.
- **Unparseable spaced names fail closed**: labels are hyphenated, spaces
  only occur in marker/suffix grammar (`copy [n]`, `alias ...`, dates). A
  spaced name that parses as none of these (e.g. a Cyrillic-lookalike
  `сору` typo) -> error page, so a botched gesture is surfaced, never
  silently published as a weird sibling post.

## Name resolution — one namespace, oldest claim wins

All names live in one flat namespace. A **claim** on name `X` is any of:
a post folder `X/`, a bare file `X.ext`, an alias marker `alias X/`
(revision folders `X copy [n]/` claim `X` collectively through the revision
rule).

- Exactly one claimant -> it resolves.
- **Multiple claimants -> the oldest claim wins the bare `/X`** (oldest =
  publish date of the claiming post; for an alias, the post carrying it).
  Rationale: **a URL's meaning never changes once established** (cool URIs)
  — dropping a new post named `IMG_4392` can never silently retarget an old
  `/IMG_4392` link, and unlike erroring, the old link keeps working.
- Every non-winning claimant remains fully reachable at its date path
  (+ `?time=` same-day) and appears normally on the timeline.
- **Visible, never silent**: the winning page carries a notice linking the
  other claimants ("this name is also used by ..."), and the server logs the
  share loudly. Nothing hidden, nothing broken.
- **Handing a name over is deliberate and cheap**: delete the old claim
  (remove the `alias X/` marker or rename the old post) and the new claim
  stands alone on next scan. If the new thing is a new *version* of the old,
  the right gesture is Cmd-D + edit the original (Revisions) — the bare URL
  never moves. No override ever happens by accident.
- The only fail-closed *errors* are intra-post ambiguity (multiple primary
  candidates, multiple date markers) — cases with genuinely no right answer.

## Grade — optional ledger, site works without it

- **Not a tag, not per-post metadata** — per-post grading ceremony is a burden;
  this must stay simple.
- The pairwise-judgement ledger is
  **`.esko.bar-grade-judgements.jsonl` in the content root**, append-only,
  written by the author (future companion/grading app, or by hand), synced
  *forward* with the content like everything else. The server only reads it
  and derives `?grade=` buckets into its outside-the-tree cache.
- **Absent or empty ledger -> `?grade=` buckets are empty and the site is
  "everything" + favorites**, fully functional. Grading is a layer to add
  whenever — or never.
- **Favorites stay a deliberate Finder tag**, independent of grade: the
  hand-picked personal statement vs the derived quality ranking.
- Keyed by post name; `alias` markers re-key judgements across renames.

## URLs (already implemented)

The slash-hierarchy grammar is live (`src/url.rs`): `/2026/03/25`, `/+tag`,
newest owns `/label`, older same-label versions at `/2026/03/25/label`,
`?time=HHMMSS` for same-day collisions, `?grade=` / `&fav` / `&q` for view
state. Only `Entry.timestamp` sourcing changes (date marker or mtime instead
of filename); addressing is unchanged.

## Migration (old flat convention -> this model, one-shot, pre-1.0)

For each old-convention file `YYYY-MM-DDTHHMMSS[_label].ext`:

- new folder = label (unlabeled -> `untitled`); move the file inside under its
  clean name; create the date marker seeded from the **filename timestamp**
  (not the unreliable fs birthtime). No `.id`.
- a sibling `<file>.embed-cache/` moves inside as `<folder>/.embed-cache/`
  (until caches relocate outside the tree entirely).
- **Same-name duplicates become revisions**: the newest becomes the current
  post (`cookie-consent-tests/`, date marker from its filename timestamp);
  older ones become `cookie-consent-tests copy/`, `cookie-consent-tests
  copy 2/` with their primary files' mtimes set to the old filename
  timestamps (copies are dated by mtime, not markers).
- Never overwrite; anything ambiguous is left in place and reported.

(Files whose date marker would equal their mtime could stay bare; folding all
migrated posts is simpler and keeps their dates stable — migrate to folders.)

## Scanner changes required

- Treat **directories as posts** (today's scanner skips dirs); recurse one
  level for primary/assets/date and alias markers.
- Keep bare files as posts with **mtime as timestamp** (today's
  `entry_from_plain_filename` already does this — birthtime fallback must go).
- Drop filename-timestamp parsing (`entry.rs::parse_filename`) and the
  `DoRename` action tag (superseded).
- Implement claim resolution (oldest wins bare name; revision suffix; alias
  markers; share-notices on winning pages) and intra-post ambiguity errors.
- Finder-tag xattr reading unchanged, read at the post level (folder or file).
- Move all caches (embed cache, index) **outside the content dir**; the
  content tree is opened read-only in intent and never written.

## Remaining decisions (settled 2026-07-06, buildable)

- **Timezone**: site-local, naive — matches existing behavior and how
  Finder/`touch` write times. Date markers may carry an explicit zone;
  normalized to site-local for ordering. Revisit UTC canonicalization only
  if the site ever has multi-timezone authors.
- **Error pages**: list the exact conflicting paths (relative to content
  root) and the one-line fix ("remove one of: ..."). Served with HTTP 500,
  shown on the timeline as an errored row (fail-closed, loud).
- **Cache location**: a server `--cache-dir` flag; default = the platform
  cache dir (`directories` crate: `~/Library/Caches/<app>` on macOS,
  `$XDG_CACHE_HOME/<app>` on Linux). Contents: embed cache, derived grade
  buckets, any index. Keyed by content path+mtime; safe to delete anytime.
- **Companion app scope** (later, nothing blocks on it): grading UI, bulk
  cleanup/renaming. Folding and date-stamping turned out native
  (Ctrl-Cmd-N, New Folder) — no longer app duties.
