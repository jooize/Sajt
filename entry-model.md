# Entry model — folder-per-post, portable dates (Phase 2a design)

Status: DECIDED 2026-07-05 (design conversation). Supersedes the flat
timestamped-filename convention. Referenced from `DESIGN.md`. Build order:
this spec → scanner rewrite → one-shot migration.

## Why this exists

Clean filenames were the goal (`/hello-world`, not `/2026-03-03T143052_hello-world`).
The URLs were *already* clean (label-based); the timestamps only lived in the
on-disk filenames. Removing them means the publish date must live somewhere
durable and **portable to a cloud Linux host** — because the site will be served
there.

Two load-bearing facts from research settled the shape:

- **macOS "Date Added"** (`ATTR_CMN_ADDEDTIME`) is the perfect "when did this get
  published" signal locally, but it is Apple-only, needs `getattrlist`/Spotlight
  to read, and **resets to the copy moment on any other volume** (iCloud/rsync).
- **Linux can *read* a birth/creation time (`statx` BTIME on ext4/xfs/btrfs) but
  has no syscall to *set* it**, and `rsync`/`cp -a` cannot preserve it. FreeBSD
  can; Linux cannot. (rsync issue #166; kernel btime notes.)

Conclusion: **no filesystem "created/added" attribute can be the portable source
of truth.** The date must be a *thing that travels inside the content*. The only
timestamp that is universal, settable, and sync-preserved everywhere is **mtime**
— which we use for "edited", not "published".

## The model

**A post is a folder.** The folder name is the label and the address:
`hello-world/` → `/hello-world`.

Inside a post folder:

- exactly one **primary content file** (carries the bytes, extension, type, and
  the `/label.ext` raw view),
- zero or more **asset files** (images, etc. for multi-file posts),
- optional **metadata**, all of which is either dotfiles or the date-marker
  folder (below), so it never competes with the content and is never served.

A bare file dropped into the content root is **auto-folded** into this shape (see
Auto-fold). Single-file posts and multi-file posts are the same model; the
timeline still shows the primary file's extension, so `.md`-vs-`/` type display
and the raw view survive foldering (the objection that previously blocked
folders).

### Publish date — an empty, date-named folder

The publish date (timeline position) is an **empty subfolder whose name is a
date**:

```
hello-world/
  hello-world.md
  2026-03-03T1430/        <- publish date marker (empty)
```

- **Editable on any device.** "New Folder → rename" is a first-class gesture in
  Finder *and* the iOS Files app; dotfiles are not. Retitle the folder to change
  the date, from anywhere.
- **Format:** ISO-8601 *basic* (colon-free — `:` is illegal on FAT/exFAT/Windows
  and maps to `/` in Finder): `YYYY-MM-DDTHHMM[SS]` with an optional zone
  (`Z` or `±HHMM`). Parsing is lenient: seconds optional, zone optional
  (defaults to site-local). Examples: `2026-03-03T1430`, `2026-03-03T143052Z`,
  `2026-03-03T1430+0200`.
- **"Empty"** = no non-dotfile children (a stray `.DS_Store` does not disqualify
  it).
- **Exactly one** empty date-named folder per post. **Zero** → the server creates
  one = now on first sight (so a fresh drop is newest). **Two or more** → the
  page errors (fail-closed; never guess).
- Birthtime is **not** consulted. On macOS the server *may* mirror the value onto
  the folder's creation date for Finder sorting, but that is cosmetic only.

### Edited date — content mtime

"Last edited" is the **primary content file's mtime**. Universal, automatic,
preserved by `rsync -t`/`cp -p`. Shown as a secondary "edited" line when it is
meaningfully later than the publish date. No metadata required.

### Primary content resolution

Within a post folder, considering only **regular files** (dotfiles and all
subfolders — including the date marker and `.embed-cache` — excluded):

1. **Candidates** = files whose stem is `index` **or** equals the folder name
   (any extension). Examples in `foo/`: `index.md`, `foo.md`.
2. If **exactly one** candidate → it is the primary.
3. If **more than one** candidate (e.g. `index.md` + `index.html`, or
   `index.md` + `foo.md`) → **error the page.**
4. If **no** candidate but **exactly one** non-dotfile file exists → that file is
   the primary (lets you copy a file in under its real name, no rename needed).
5. Otherwise (no candidate and 0 or ≥2 files) → **error the page.**

All non-primary regular files are assets. Type/extension and the `/label.ext`
raw view come from the primary. `index` with no extension is allowed but untyped
(served as plain/download) and discouraged.

**No symlinks.** The rules above let you keep a file's real name without an
`index → file` symlink, and symlinks do not survive iCloud/zip/`rsync` without
`-l`.

### Identity (fuses with the plan)

The server-assigned **UUIDv7** lives in a `.id` dotfile inside the post folder
(the plan's "folder-entries keep the id in an inner file — rename the folder =
atomic"). Identity is server-managed, never user-edited, so a dotfile is fine
here (unlike the user-edited date). Content hash stays a re-pair fallback only.

## Auto-fold (bare drop → folder)

When the server sees a bare regular file `foo.md` in the content root:

1. create `foo/` (never overwrite: if `foo/` exists, leave the file and warn),
2. move `foo.md` → `foo/foo.md`,
3. create the date marker `foo/<now>/` (or honor a date the user pre-placed),
4. write `.id`.

Constraints (per the "fail-closed, protect the content" directive):

- **Idempotent** and **never destructive** — no overwrite, ever; on any doubt,
  leave the file untouched and log loudly.
- **Race-safe with sync** — fold only after the FSEvents debounce settles, so we
  never grab a file mid-iCloud-write. Skip files that look partial.
- Auto-fold is the convenience path; explicitly creating a folder yourself is
  always fine.

## URLs (already implemented)

The slash-hierarchy URL grammar is live (see `src/url.rs`): `/2026/03/25`,
`/+tag`, newest owns `/label`, older same-label versions at `/2026/03/25/label`
with `?time=HHMMSS` only for same-day collisions. Nothing here changes it —
`Entry.timestamp` is now sourced from the date-marker folder instead of the
filename, but the addressing is unchanged.

## Migration (flat files → folders, one-shot, pre-1.0)

For each old-convention file `YYYY-MM-DDTHHMMSS[_label].ext`:

- new folder = the label (unlabeled → `untitled`); move file inside under its
  clean name; create the date-marker folder seeded from the **filename
  timestamp** (not the wrong fs birthtime); write `.id`.
- a sibling `<file>.embed-cache/` (regenerable `.link` cache) moves inside as
  `<folder>/.embed-cache/`.
- **Collisions** (three `cookie-consent-tests.html`, two on 2026-03-12): folder
  names collide the same way filenames did → newest keeps `cookie-consent-tests/`,
  older get `cookie-consent-tests-YYYY-MM-DD-HHMMSS/` (per "keep all,
  disambiguate"); each carries its own date marker.
- never overwrite; anything ambiguous is left in place and reported.

## Scanner changes required

- Treat **directories as posts** (today's scanner skips dirs); recurse one level
  for primary/assets/date-marker/metadata.
- Drop filename-timestamp parsing (`entry.rs::parse_filename`); source
  `Entry.timestamp` from the date marker, `edited` from primary mtime.
- Keep Finder-tag xattr reading (unchanged) at the folder level.

## Open questions

- Timezone normalization: store/display in site-local vs UTC `Z`. Lenient parse
  either way; pick a canonical display.
- Whether to also expose an "edited" line in the UI now or later.
- Multi-`index` error surface: a clear on-page message telling the author which
  files collide.
