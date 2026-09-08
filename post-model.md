# Post model — links, listings, slugs, kinds

Design session 2026-07-07. This document captures a body of decisions that
**extend and, where noted, supersede** [entry-model.md](entry-model.md). The
bare-file / folder-post foundation, the ` copy [n]` revision grammar, the empty
date-marker and `alias <name>/` markers, oldest-claim-wins, and the strictly
read-only server all stand — this refines identity, timestamps, kinds, links,
listings, and image privacy on top of them.

Nothing here is shipped yet; it is the spec to build from. No backward
compatibility is owed (pre-1.0).

---

## 1. Identity, names, and slugs

A post's identity is still its filename (bare file) or folder name. Two
projections come off that one name:

- **Title text** — the display headline. The filename stem, *natural*: spaces
  and letter case preserved. `fog over the bay`.
- **URL label (slug)** — the address. Derived from the stem: lower-cased,
  spaces to hyphens, punctuation stripped. `/fog-over-the-bay`.

### Spaces are allowed (change)

Today `build_bare_post` (`content.rs:180`) and `build_folder_post`
(`content.rs:213`) reject any name containing a space as `UnparseableName` —
a conservative typo guard. **Lift it.** A "drop a file" site should accept the
natural macOS filename `fog over the bay.jpg`.

This is safe:

- **Grammar-safe.** ` copy [n]` revisions are already routed away before a post
  is built (`content.rs:102`); `alias`/date/`index` markers live *inside* posts.
  So a top-level spaced name conflicts with nothing except a trailing
  ` copy N`, which Finder itself treats as a duplicate. The only limit: you
  can't title something `… copy` — see §5 for the softening of even that.
- **Privacy-safe.** Visibility is gated by the `public`/`private` tag, never by
  the name. A mistyped name cannot leak anything a correct one wouldn't.

Identity stays "the post's name is its filename"; the slug is a derived
projection. Mirror the existing client `slugify` (`templates.rs:919`)
server-side so both agree on the same address. **Addresses are always
lowercase**; a mixed-case URL is non-canonical and 301s to the canonical
slug, like every other non-canonical URL.

### Slug algorithm (exact — collisions key on this)

1. Unicode-normalize **NFKD**; strip combining marks from Latin base
   letters (`é` → `e`). Non-Latin letters and digits are **kept**
   (Unicode-lowercased) — a Japanese or Cyrillic title keeps its script.
2. Every character that is not a letter or digit → hyphen.
3. Collapse hyphen runs; trim leading/trailing hyphens.
4. **Empty result** (a title of `!!!`) → the post is treated as
   **unlabeled** (like a date-named post, §2): addressed at its date path,
   no name claim, notice on the page.

### Reserved segments — the router always wins

Some top-level segments belong to the URL grammar, not the namespace:
`saved`, and any **purely numeric** slug (year/date filtering — `/2026`
must stay the year view; the §2 "1984 stays a label" promise holds for
*titles*, but the bare URL yields to the router). A post whose slug is
reserved never claims the bare URL: it lives at its date path, carries the
standard "also at" notice, and the server logs the reservation loudly.
Tag grammar needs no reservation — `+`/`!` are punctuation and never
survive slugification.

### Collisions fold on the slug

Slug derivation widens the collision space: `fog over the bay` and
`fog-over-the-bay` (and `Fog Over The Bay`) all reduce to `/fog-over-the-bay`.
**Key claim comparison on the slug**, not the exact label (today `name_claimants`
compares labels case-insensitively). Then the existing rule applies unchanged:

- The **oldest** claim owns the bare `/fog-over-the-bay`.
- Younger claimants carry the shortest date that distinguishes them
  (`/2026/07/04/fog-over-the-bay`, plus a time path segment
  `/2026/07/04/191430/fog-over-the-bay` for a same-day tie).
- **Both always appear on the timeline** — the timeline never hides a post; a
  collision only decides the canonical URL. The two rows are distinguishable by
  their *natural* display titles and their dates.

### Collisions are made discoverable (extend)

The name-share machinery already exists on post pages: `name_shares` +
`name_share_notice` render an "Also at" block (`templates.rs:2313`). Extend it:

- Surface it on the **timeline row** as a `<details>` disclosure, mirroring the
  revision dropdown — "N others share this address", each linking to its
  date-disambiguated URL.
- Each colliding post is reachable and mutually discoverable; the collision is
  obvious, never silent.

---

## 2. Timestamps

**mtime is the timestamp signal everywhere. Creation time (birthtime) is never
used** — it is not portable (`rsync -t` carries mtime, not birthtime; copy and
restore reset it) and is not user-settable, whereas mtime is (`touch`, Finder).
This is the same reasoning that rejected birthtime as a revision tie-break.

- **Bare file** — publish date = its mtime. Editing moves the mtime, which
  *republishes* (bumps it up the timeline). Intentional (`entry.rs:71`).
- **Folder post** — publish date = the empty date-marker subfolder if present,
  else the primary's mtime. With a marker, the primary's mtime becomes the
  *edited* date, shown only when meaningfully later.

To pin a stable publish date, add the date-marker folder. That is its whole job.

### Date-named posts = unlabeled (new)

A top-level folder (or bare file) whose name is a valid date is an **unlabeled**
post — no label, dated by its own name, addressed at its date path + `?time=`.
This is the clean "publish now, name later" gesture; a Share Extension / Shortcut
can stamp the name automatically.

Grammar — extend `parse_date_marker` (`content.rs:761`), which today *requires*
a `T`. Accept, minimum a **complete date**, then optional precision:

```
2026-07-07                 → that day (midnight)
2026-07-07T1914            → + time
2026-07-07T191430          → + seconds
…optional Z or ±HHMM zone
```

- **Require all three date components.** A full `2026-07-07` is not a plausible
  title (and if you *do* title by date, unlabeled-dated is what you meant). Bare
  `2026` or `2026-07` **stay labels**, so "1984" and a year-in-review "2026"
  survive.
- **Keep the hyphens** (not strictly necessary — 8 digits parse — but they read
  as a date, resist being mistaken for a plain number, and mirror the URL
  `/2026/07/07/`). Pick one canonical form; don't accept both.
- **Progressive precision by hand is fine**: bare date first, add `T1914` for
  the next, add seconds if many in one minute. The filesystem enforces this —
  two same-named folders can't coexist, so the second is *forced* to carry a
  time.

The same grammar serves both positions: an *empty* date-named folder *inside* a
post is the publish marker; the *post itself* named a date is an unlabeled dated
post. "A date-named thing is a date."

### Date-time deeplinks (new)

Every post page shows a quiet permalink of its publish moment —
`¶ example.org/2026/07/04?time=191430` — mono, ¶-prefixed, the generalization
of the bare-link row's permalink (`static/link-rows-mockup.html`, case 4).

This is a **name-independent citation**: it resolves by timestamp and 301s
to whatever the canonical URL is *now*, so it survives renames that would
orphan a label link (the accepted rename cost in entry-model.md no longer
applies to anyone who cited the deeplink).

- **Resolution:** exact date+time match → 301 to canonical. No match (or
  two posts in the same second) → the **day view**, never a 404 — the
  reader lands somewhere useful; an edited-that-day post is right there.
- **Stability = publish-date stability.** A date-marker post's deeplink is
  permanent. A bare file's moves when the file is edited (mtime →
  republish) — the same documented trade as its timeline position, one
  more reason folding is the upgrade path. Folding itself is lossless:
  Ctrl-Cmd-N preserves the primary's mtime, so the deeplink resolves
  identically before and after; add the date marker to pin it forever.

---

## 3. Kinds — medium, not format, not genre

`kind` is a **medium** classifier — what sort of thing a post *is* and how it is
served. It is not the format (that's the extension, which drives exact
rendering) and not the genre (that's an author-set tag, e.g. "essay" vs "note").

| kind | what | why its own bucket |
|------|------|--------------------|
| `photo` | images | distinct rendering (viewer) |
| `html` | raw HTML | a self-contained document (`is_complete_html`) serves jailed in an opaque origin — embedded in the chrome by default, `?fullscreen`, or byte-exact at its asset URL (see DESIGN.md "Content security") |
| `text` | md, markdown, txt, text, rst, org, adoc, asciidoc, tex | poured into the site shell |
| `link` | resolves to a single URL (see §4) | primary affordance points *out* |
| `folder` | a listing (see §6) | a browsable directory index |
| `file` | anything else | opaque → download |

Decisions folded in:

- **Rename `note` → `text`** (`entry.rs:129`). "note" wrongly implied
  length/genre; a 5000-word `.md` is not a note.
- **`.text` is first-class**, and while wiring it, close two gaps in
  `render.rs`: `pandoc_format` maps only `md` (add `markdown`); `render_entry`
  preformats only `txt` (add `text`). Today a `.markdown`/`.text` file silently
  falls through to *download*. Assume `.text` = plain text (pairs with `.txt`).
- **`.html` stays distinct from `text`** — the one format that can be a
  complete, self-contained document (own CSS/JS, bypasses site chrome). Named
  `html`, not `page` — "page" over-claims, since rendered text is also a page.
- **`.md` and `.txt` share `text`** — they differ in *rendering* (formatted vs
  preformatted), not in medium. That difference is the extension's job.
- **Dotless bare files** (`README`, `LICENSE`) are not folders. Disambiguate on
  `Entry.dir` (`None` = bare file). A dotless bare file: UTF-8 decodable →
  `text` (render preformatted; we already read it for the excerpt), else
  `file`. Only a *folder* is ever `kind = folder`.

---

## 4. Link posts — the `link_url` axis

A post may carry an optional **`link_url`**, computed by the scanner (no
frontmatter). It is orthogonal to `kind`: a post *has a destination* or not.

### Where a destination comes from

- **A file that resolves to a single URL** — `.webloc` (parse the plist, read
  the `URL` key), `.url` (INI, `URL=`), or any text file whose *entire trimmed
  content is exactly one http(s) URL*. A title line plus a URL is a
  note-with-a-link, not a link post — the predicate is crisp, not fuzzy.
- **A `link.*` sidecar** in a folder post — `link.webloc`, `link.url`,
  `link.md`, `link.txt`, `link.text`. This is how a *commentary* post gets a
  destination: the primary is your writing, the sidecar is the target.

**Amended (2026-07-15, v0.23.0) — cite candidacy is intent-carried, never
content-sniffed.** Inside a folder post, a file may claim the cite only when
the *format* or the *name* says so: a bookmark format (`.webloc`/`.url`) is a
URL by construction, so any name qualifies (drag a Safari bookmark in, it
cites, no rename); a general text file qualifies only when its stem is
`link`. A `notes.txt` whose content happens to be one URL stays an ordinary
attachment — editing a file's contents, or adding a second URL-file beside
it, never changes another file's role. (Bare-file posts at the top level are
exempt: there the file *is* the post, so the single-URL predicate only
changes its own rendering to a link card — no other file's meaning moves.)

Resolution: a qualifying file **drops out of primary candidacy** and
becomes the destination (the way date/`alias` markers are set aside). The
stem `link` (like `index` for primary) is the explicit marker and the
tie-break. **More than one destination with no tie-break** never guesses —
but it demotes rather than erroring, matching the primary-collision rule
(§6): the post renders normally with **no outbound cite at all**, plus a
prominent notice ("two destinations claimed — keep one, or name one
`link.*`") and a loud log. No wrong link can ever be emitted; the body
still publishes; the fix is one rename. **The unpromoted destinations fall
back to ordinary listed files** (§6): each renders under the post body as a
link row in the attachment listing — the same timeline-row treatment any
listed file gets, carrying its cite (favicon · target title · domain ↗)
instead of a bare filename — beneath the notice that says why none became
the headline. The post's own timeline row carries no cite.

This is the one unifying idea, one level down: resolving a file to *the*
headline destination is an **optional promotion**. When it can't happen
cleanly, the file is not an error and not a special "dupe list" — it just
stays what it already was, **ordinary listed content**, the same fallback
as "drops out of primary candidacy" above. (Listing membership still obeys
the `public` allowlist, §6 — the notice always shows; the rows show the
public-tagged candidates.) The one hard error left: `kind = link` with
nothing *but* ambiguous destinations — no body to fall back to, nothing
safe to render except the notice itself.

### Destination metadata fetch (title + favicon)

The `<cite>` line needs the target's title and favicon; that fetch is a
server-side request, so it gets the full treatment:

- **Scan-time only**, never per-request; cached out-of-tree with the
  embeds. Failure degrades to bare domain (above), retried on the embed
  liveness schedule.
- **SSRF-guarded**: resolve DNS first, refuse loopback/private/link-local
  ranges — re-checked on **every redirect hop**. Hard timeout (5 s), size
  caps (favicon ~512 KB; title read from the first ~1 MB of HTML).
- **Favicons re-serve locally, never hotlinked** — hotlinking would leak
  reader IPs to the destination, defeating `rel="noreferrer"`.

This retires the `.link` extension: a bare-URL `.txt`/`.md` does the same job
and is viewable in Finder (the original complaint), so **drop `.link`**.

### Behavior — direct-out vs permalink

- **`kind = link`** is the degenerate case: the post *is* only a destination.
  Body = the embed card.
- **A content post with `link_url`** (folder post: commentary + sidecar) renders
  its own body; the destination is cited. `kind` stays the primary's medium
  (`text`, `photo`, …).

### Row layout (settled via mockup — `static/link-rows-mockup.html`)

Two visually distinct targets, so nothing is ambiguous:

- **Our label → our page** (internal). It's *our* title; it goes to *our* page.
- **The cited source → the destination** (external, marked ↗, `rel="noreferrer"`).
  Rendered as a semantic `<cite>` (favicon · target title · domain) — no class,
  fits the element-selector rule.
- **No label** (bare link) is the one case the target's headline *is* the
  headline → destination, with a quiet `¶ example.org/2026/07/04` permalink → the
  card page.
- Degrade cleanly: label == target title → show once; embed/title fetch failed →
  bare domain.

`link_url` is orthogonal to kind, so a `photo` can credit a source the same way.

---

## 5. Revisions and families

A **family** is the set of items sharing a base name: `X`, `X copy`,
`X copy 2`, … (`parse_revision_suffix` strips the trailing ` copy [n]`). A family
always collapses to **one current post + a revision stack**, keyed by the base
name — *not* by whether the base file exists.

- **Base present** → it is the current post; the copies are revisions.
- **Base absent** → the **newest-by-mtime** member becomes current; the rest are
  its revisions. So a lone orphan is a standalone post (family of one); many
  orphans stay bound as one row + a revision dropdown.

This fixes two things at once: it lets you *title* something `… copy` when no
base exists (it's a real post, not a phantom revision), and it stops the current
**silent drop** of orphan copies (`attach_revisions` discards them today). It
also gives a recovery property: delete the base and its archive promotes rather
than vanishing.

Ordering is unchanged: **mtime first, `rank` (the copy number) only as an
exact-tie break** (`sort_revisions`, `content.rs:824`). The number is never an
identity or a URL — deleting a revision and letting Finder refill the gap
reorders nothing. Birthtime was considered and rejected (§2).

**Known hazard, accepted**: a standalone post *titled* `foo copy` is only
standalone while no `foo` exists — create `foo` later and the family rule
absorbs it as a revision (and conversely, a promoted orphan demotes if its
base reappears). Age can't disambiguate (Cmd-D archives are legitimately
*older* than their edited base), so instead of a clever rule: the server
**logs the absorption loudly** and the new base's page shows a one-time
notice ("absorbed existing 'foo copy' as a revision"); the absorbed post
stays reachable via the revision dropdown, and the recovery is one rename.
Same class of accepted cost as rename-without-alias in entry-model.md.

---

## 6. Listings — folders you browse

A folder with **no single document primary** renders as a **listing**: a
browsable index of its files. This is what gives `kind = folder` / the `/`
suffix a real, non-error meaning. Visual:
`static/listing-mockup.html` (gallery, file list, timeline row).

### Choosing it

- **Automatic** — a folder holding only media with no document: all images → a
  gallery; mixed downloads/PDFs → a file list. Drop a folder of photos, get an
  album. Zero config.
- **Explicit** — an empty **`index/`** marker folder forces listing mode. It
  both lists a folder that *could* have chosen a primary and turns what is now an
  `AmbiguousPrimary` **error** (two documents) into an intentional listing. Joins
  the empty-folder-directive vocabulary (date, `alias`, `index`). URL/filesystem
  parity: `/label/file.ext` matches the folder on disk.

*(Open choice: automatic + explicit both, or explicit-only. Lean both.)*

Note the `index` overload, resolved by shape: a *file* `index.md` is the primary
page; an empty *folder* `index/` is the listing directive — the same split web
servers use.

### Attachments — a listing alongside a primary (new, 2026-07-07)

Listing and post are not two folder species; they are one rule with the
primary as the only variable. **A file tagged `public` inside a post is
listed.** A document post's public-tagged sibling files render as an
**attachment list below the body** — the same file-list component a pure
listing uses. `My Resumé/` = `My Resumé.md` (the intro, primary) +
`cv.pdf`, `references.pdf` tagged `public` (the attachments). Adding a doc
is drop + tag; removing it is untag. No markdown list to write or parse.

The whole space is one gradient:

- primary, no public-tagged files → plain post (today's behavior)
- primary + public-tagged files → post with attachments
- no primary, public-tagged files → pure listing (the base §6 case)
- `index/` marker → forces the no-primary case

Decisions folded in:

- **Ordering: filename** (Finder's order), not mtime — attachments are a
  curated set, not a feed; fixing a typo in a PDF must not reshuffle it.
- **Placement: always below the body**, one attachment idiom everywhere.
- **Public-tagged subfolders are listed too**: a non-marker subfolder tagged
  `public` appears in the list as a folder row and is itself a **nested
  listing** at `/label/sub/`, gated by the same rule at every level
  (fail-closed AND up the chain: every ancestor and the file itself must be
  `public`). Untagged subfolders stay invisible, exactly like untagged
  files. Marker folders (date, `alias`, `index`) are never listed,
  tagged or not. This gives named groups of attachments for free —
  `My Resumé/talks/` — without new grammar.
- **Primary resolution refines entry-model.md.** Candidates = files whose
  stem is `index` or the folder name, **compared on the slug** (§1) — so
  `My Resumé/my-resume.md` matches; no need to reproduce capitalization or
  accents. One candidate → primary; none but a single lone file → that
  file. Both remaining cases **demote to a listing instead of erroring**
  (supersedes `AmbiguousPrimary`):
  - **Several candidates** (`index.md` + `my-resume.md`): the server never
    guesses which file gets the headline — it declines to pick and lists.
    The page carries a **prominent collision notice** ("two files claim the
    primary slot; remove one, or add `index/` to make the listing
    intentional"), and the server logs it loudly. An intentional `index/`
    marker is exactly what silences the notice. The notice shows regardless
    of tags, but the listing still shows only public-tagged files — two
    untagged documents yield an empty listing with a loud notice.
  - **No candidate, several files** → the automatic listing above.
  Intra-post *date-marker* ambiguity (two date markers) stays a hard error —
  no degraded rendering respects an unknown date.
- **Nested listings recurse to any depth** — `/label/sub/sub2/file.ext`
  just works, each level `public` (fail-closed AND). **A nested item's
  path IS its clean canonical URL** (`/my-resume/talks/`); "no slug claim"
  means only that it never competes for the bare `/talks` in the flat
  namespace.
- **Nested items get the full row treatment**: the timeline-row visual
  language (date, tags, description), **expand-in-place** — a listing's
  timeline row (and each folder row inside a listing) is a server-rendered
  `<details>` disclosure opening to its public children, recursing, same
  idiom as the revision dropdown, no JS — and **revision collapsing**:
  ` copy [n]` is lexical, so `talk copy.pdf` folds under `talk.pdf` as one
  row with the same revision disclosure (`parse_revision_suffix` reused at
  display level, ordered by mtime).
- What nested items never have is **identity**: no flat-namespace claim,
  no aliases, no grades, no own main-timeline row. Identity stays "a
  post's name is its top-level name"; promoting a nested thing to a real
  post is the move-to-top-level gesture.

### Membership is an allowlist, never a blocklist

**A file appears in a listing only if it is tagged `public`.** Consequences:

- `.DS_Store`, markers, and any junk are excluded *because they are untagged* —
  no fragile filename filter to maintain, nothing leaks by default. This is the
  fail-closed principle applied directly.
- The `+dotfiles` marker idea is unnecessary — tag a dotfile `public` if you
  genuinely want it listed.
- Finder multi-select makes tagging a whole album one gesture.

### Serving and security

- Each item is reachable at `/label/file.ext` via folder-relative asset serving
  (already present); nested listings extend it one path level per public
  subfolder (`/label/sub/file.ext`). The listing page is links to them.
- Visibility is gated at **every** level, fail-closed AND: the post must be
  `public` for anything to exist, each subfolder on the path must be `public`,
  and the file must be `public` to be **listed**.
- ~~`public` on a file means *listed*; it does not change asset serving.
  Untagged assets keep serving at `/label/file.ext` as part of a public post
  (inline images need no tagging) — they just never appear in any generated
  list.~~ **Amended (2026-07-08 strict gate; extended 2026-07-15, v0.22.0):**
  `public` on a file means *served* — one rule, no exceptions. The tag on an
  object publishes everything that belongs to that object — metadata and
  contents — and nothing it merely contains: the folder tag makes the post
  reachable (name, date, comment); only the file's own `public` tag serves
  its content. This covers assets (an untagged inline `<img>` 404s until tagged),
  **the primary itself** (untagged primary → the post demotes to a listing of
  its public files, withheld count visible, loud log), the listing intro
  document, ` copy [n]` revision snapshots (a folder copy needs the copy
  folder *and* its inner primary tagged), and `link.*` cite destinations (the
  published URL is that file's content). Nothing is ever served or listed by
  default.
- Asset serving stays path-traversal-safe; a listing never reaches outside its
  own folder, and never through a non-public subfolder.

---

## 7. Outbound-scheme guard

Every destination — from any link format — funnels through one resolution point,
which is where the guard lives. It is written once.

**It is an allowlist, not a blocklist** (fail closed — blocklists miss
obfuscations and future dangerous schemes). Normalize first: trim, strip
control characters, lowercase the scheme; a destination must be an
absolute URL with an explicit scheme (protocol-relative `//host` never
occurs — the §4 predicate requires http(s) — but the guard rejects it
anyway). Then:

- **Allow silently:** `https:`, `mailto:`, `tel:`.
- **Allow but flag prominently:** `http:` (unencrypted). Pure CSS, no
  server class — `main a[href^="http://"]`. Fits the no-classes rule.
- **Everything else is refused, server-side** — `javascript:`, `data:`,
  `vbscript:`, `file:`, and any scheme not on the allowlist (custom URL
  handlers are a recurring exploit class; "inert" cannot be verified).
  Refused schemes execute in our origin, read the reader's disk, or
  invoke arbitrary local handlers — the danger is what they *run*, so a
  prompt is the wrong tool. Render as flagged plain text ("unsafe link
  removed"), never a clickable anchor. CSS can't stop navigation; this
  must be render-time. Widening the allowlist is a one-line, deliberate
  edit.

No `/out?` interstitial is needed: safe links just render (scheme visible),
referrer is dropped by `rel="noreferrer"`, and unsafe links are refused at the
source. (A CSS-only `:target` confirm is *possible* if ever wanted, but isn't.)

---

## 8. Images and EXIF

Not built yet — the `image` crate is absent from `Cargo.toml`; images serve
**raw/exact today**. The model to build:

- **Strip metadata by default, on every served path** (rendered *and* raw). GPS,
  camera, and timestamps are a real privacy leak; fail closed. Stripping only the
  rendered view while raw serves the original would defeat the point.
- **Opt in to the exact original** per file via an **`original`** tag — EXIF
  intact, exact bytes and hash. This is the "host an exact image file" case, and
  it composes with listings meant as file drops.
- **Transparent, never silent** — a stripped image shows a small, non-dismissing
  "metadata removed for privacy" note with the hint to tag `original`
  (per the "display notices prominently" rule).
- **Prefer a metadata-only strip over full re-encode.** Re-encoding via `image`
  is lossy for JPEG (generational loss) and drops ICC color profiles. Dropping
  the APP/EXIF segments while keeping the pixel data preserves quality *and*
  privacy. *(Revisits the memo'd "image crate re-encode" decision.)*
- **What the strip keeps and drops — exactly:**
  - **Keep orientation** (preserve the EXIF orientation tag, or transpose
    the pixels and drop it) — stripping it naively renders every portrait
    phone photo sideways, the classic bug.
  - **Keep the ICC profile** (JPEG APP2) — color fidelity, no meaningful
    leak.
  - **Drop everything else**: EXIF (including the **embedded thumbnail**,
    which can contain the un-cropped original — a real leak), XMP, IPTC,
    JPEG comment segments; PNG `tEXt`/`zTXt`/`iTXt`/`eXIf` chunks; the
    equivalent chunks in WebP/TIFF/HEIC.
  - **A format we cannot strip is not served** (except under `original`) —
    fail closed, rendered as the standard notice, never silently raw.
- **Caching:** the strip must be **deterministic** (same input → identical
  bytes) so the immutable-cache story holds; the served variant's
  ETag/hash derives from the **stripped** output, cached out-of-tree.
  `original` serves exact bytes and the original hash.

---

## 9. Open choices for the author to confirm

All four settled in the 2026-07-07 review session:

1. **Listings:** both — automatic-for-media *and* explicit `index/` (which
   also silences the primary-collision notice).
2. **Family "current" when the base is deleted:** newest-by-mtime, with
   the absorption hazard documented in §5.
3. **`.text`** = plain text.
4. **EXIF:** metadata-only strip confirmed, amended with the exact
   keep/drop list and deterministic-caching rule in §8.

## 10. Implementation map (files touched)

- `content.rs` — lift the space rejection (`build_bare_post`/`build_folder_post`);
  slug derivation (exact §1 algorithm) + claim-on-slug + reserved-segment
  yield; extend `parse_date_marker` (bare date,
  drop the mandatory `T`); date-named → unlabeled; family-by-base-name with
  orphan promotion; `link_url` resolution + `link.*` sidecar; listing detection +
  `index/` marker; per-file `public` gating for listings; attachment
  collection (public-tagged siblings + subfolders, one nesting rule);
  slug-compared primary candidates.
- `entry.rs` — `note`→`text`; `kind` on `dir`-ness + UTF-8 sniff; `link_url`
  field; listing/`folder` kind.
- `render.rs` — `markdown`/`text` gaps; drop `.link`; `.webloc`/`.url` → URL;
  single-URL text → link card; scheme allowlist guard (normalize +
  refuse-unknown); listing page; image metadata strip (orientation + ICC
  kept, deterministic) + `original` opt-in + notice; unresolved-destination
  list under the body.
- **fetcher (scan-time)** — title/favicon fetch with SSRF guard
  (DNS-resolve → refuse private ranges, per redirect hop), timeouts, size
  caps; favicons cached out-of-tree and re-served locally. Unpromoted
  ambiguous destinations fall through to the listing as link rows (reuse
  the §6 component), not a bespoke list.
- `url.rs` — slug in `ContentQuery` matching; label-free date+time deeplink
  resolution (exact match → 301 canonical, else day view).
- `templates.rs` — row: label-internal + `<cite>` external ↗; name-share row
  dropdown; slug in `name_claimants`; scheme CSS; attachment file-list
  component (shared by listings and post pages; folder rows for nested
  listings; `<details>` expand-in-place; nested ` copy [n]` collapse) —
  visual: `static/listing-mockup.html`; the ¶ deeplink on post pages.
- `routes.rs` — outbound refusal; image serving (strip vs `original`); listing
  routes over folder-relative serving, extended through public-tagged
  subfolders (`/label/sub/file.ext`).
- `Cargo.toml` — `plist` (webloc); a JPEG/PNG metadata editor (strip).
