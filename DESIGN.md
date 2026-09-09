# Sajt — Design

The single summary of the site's design. Detail files live in `.claude-memory/`
and are referenced at the bottom. Items are marked **DECIDED** (with date),
**PROPOSED** (a recommendation awaiting a decision), or **OPEN**.

## Philosophy

The filesystem is the CMS. One folder, synced through iCloud Drive, is the
whole publishing pipeline. No databases as source of truth, no frontmatter,
no build step. Metadata is native metadata — filenames, folder names,
creation dates, Finder/Files tags. Tags over folders everywhere: folders
exist only where they *are* the content (bundles).

Tagline: **"Tag it `public` — it's published."**

What the tag means (DECIDED 2026-07-15, v0.22.0): **the `public` tag on an
object publishes everything that belongs to that object — metadata and
contents — and nothing it merely contains.** Tag a file → its name, date,
and insides are published. Tag a folder → the folder's name (the URL), date,
and Finder comment are published, plus the fact that a post exists; every
file inside is a separate object needing its own tag. No exceptions — see
"Visibility" below.

- Cool URIs don't change: URLs contain only labels and dates, never
  technology. Renames redirect (301); removals answer 410 Gone, not 404.
- Fail closed: nothing is published by accident. Security over convenience,
  then engineer the convenience back.
- Native approaches on every platform: Finder, Files.app, share sheet, tags,
  real file dates. The site never re-implements UI the OS provides (no theme
  switcher, no share buttons); dark mode follows the system.
- The content folder contains only visible things — **no dotfiles, ever**
  (DECIDED 2026-07-04). The server's index and caches live in the server's
  own state directory, outside the content folder. Anything the server does
  materialize alongside content is a plain visible file or symlink the user
  can see, understand, and delete in Finder.
- A high-quality, serious website: plain, typography-first presentation
  built on current web platform features (`light-dark()`, text fragments,
  `:has()`), always with graceful fallbacks so old browsers get a readable
  page rather than a broken one.

## Content model

### Entries

An entry is a file **or a folder** in the single content directory.
**SHIPPED 2026-07-06 — full spec in `entry-model.md` (canonical).** The scanner
rewrite, folder-post serving, and the one-shot flat→folder migration are all
live; the live `content/` is now folder posts served at clean URLs

**IN DESIGN 2026-07-07 — `post-model.md` (spec, not yet built).** Extends the
above: natural-spaces titles with derived URL slugs, mtime-only timestamps +
date-named unlabeled posts, `kind` as *medium* (`note`→`text`, dotless files,
`.html` distinct), the `link_url` axis (`.webloc`/`.url`/single-URL text +
`link.*` sidecars, direct-out rows, retires `.link`), families keyed by base
name (orphan promotion), slug-collision dropdowns, folder **listings** via an
`index/` marker with allowlist-by-`public`-tag membership, a tiered outbound
scheme guard, and the file-privacy boundary (allowlist classification, metadata
stripping, the `<visibility>[-original]` exact-bytes escape).
(`/hello-world`). This supersedes the timestamp-prefix filename convention,
birthtime dates, and the `.id`/UUID identity plan:

- **Files stay first-class**: a bare `sunset.md` is a complete post. Its
  **publish date is its mtime** (the one timestamp every sync tool preserves,
  settable on every OS — birthtime/Date Added are not portable to the Linux
  host). Trade: editing a bare file republishes it; fold it into a folder when
  the date must outlive edits. The extension gives the timeline its type
  indication for free.
- **Folders as entries** — `open-source-licenses/` *is*
  `example.org/open-source-licenses`. Publish date = an **empty date-named
  subfolder** (`2026-03-03T1430/`), editable on any device (Finder/iOS Files
  "New Folder"); zero markers → mtime fallback, two+ → error (fail-closed).
  Exactly one primary content file (stem `index` or = folder name, or the
  sole file); intra-post ambiguity errors the page. `alias <name>/` marker
  folders give extra addresses / rename survival — **no server-assigned
  identity, no `.id`**. Revisions: **the unsuffixed name is always current**;
  Cmd-D before editing freezes the old state into `label copy/` /
  `label copy 2/` (or ` copy` files inside the post as snapshots) — the copy
  is the archive, dated by its own mtime, never shown on the timeline;
  deleting any copy never breaks the post. Names + aliases
  share one flat namespace; **multiple claims on a name -> oldest claim keeps
  the bare URL** (established URLs never change meaning), others reachable at
  date paths, share surfaced on-page + logged. Tags on the folder apply to
  the entry.
- **The server never writes into the content tree** (read-only; all caches
  outside it, disposable). Sync is one-way, authoring devices → server. No
  companion software is required on Mac or iPhone; a future app is a
  concierge (fold, stamp dates, grade), never a dependency.
- **Bundles** — a folder-entry may contain an `index.md` / `index.adoc`
  (hand-written page), a `.prompt` (AI-generated page from sibling files), or
  neither (auto gallery/listing of its contents). Files inside a bundle
  inherit the folder's identity and need no timestamps. Assets inside are
  addressable as `example.org/<name>/<asset>`.

### Visibility — tags are the only gate

**DECIDED 2026-07-03: one flat content folder; the `public` xattr tag is the
only publish gate.** No moving files, no special folders — tags over folders,
consistently. Files.app on iPhone tags well now, so the flow is native on
both platforms.

Why this is not a recipe for disaster: the default stays fail-closed, so
every failure mode points the safe direction. An untagged entry is invisible.
A tag lost in a bad sync *unpublishes* — annoying, never a leak. There is no
state in which something private goes live without a deliberate tagging act.

- **`public` tag** = published for all. Anything else = not served.
- **`private` tag** = gated / access-controlled — a future authenticated
  area. Until that exists, `private` entries are simply never served.
- **`personal` tag** = descriptive, orthogonal to visibility (an entry can be
  personal+public).
- **`-original` suffix** composes with any visibility tier
  (`public-original`): serve this file's **exact bytes**, bypassing the
  file-privacy boundary's stripping — see "The file-privacy boundary" below.
- Unpublishing (tag removed after being live) answers **410 Gone**.

**SHIPPED 2026-07-08 (`post-model.md` §6 + [review]) — the gate is now
enforced, fail-closed at every level.** Enforced once in the scanner
(`ContentStore::scan` drops any post whose top level isn't `public`, or is
`private`) so the timeline, name resolution, listings, and embeds are all
fail-closed at once. Deny-wins: `private` beats `public` on the same item and
hides its whole subtree. Assets are gated per-file and per-path
(`tags::path_visible` ANDs `public` down the whole chain, `private` anywhere
denies): **every** file and subfolder needs its own `public` — strict, no
inline-image exception, so an untagged inline `<img>` is a broken/404 image
until its file is tagged (which also lists it, §6). A hidden path 404s
identically to a missing one. The tagline holds literally now: drop
a file, **tag it `public`**, it's published.

**AMENDED 2026-07-15 (v0.22.0) — no primary exception.** The folder tag makes
a post *reachable*; only a file's **own** `public` tag ever serves its
*content*. A folder post's primary (name-match, `index`, or a lone file) must
itself be tagged `public` — otherwise the post demotes to a listing of the
folder's public files (the withheld count stays visible) and the scanner logs
why. The same one rule gates the listing intro document, ` copy [n]` revision
snapshots (in-folder and top-level; a folder copy needs the copy folder *and*
its inner primary tagged), and `link.*` cite destinations (the published URL
is that file's content). Nothing untagged is ever served, with no exceptions
to remember.

**What the folder's own tag publishes (decided 2026-07-15):** tagging an
object publishes *that object's own* metadata — its name (the URL), its date
(marker or mtime), and its **Finder comment** — never its children's content.
So a comment on a public folder is served as the post description (know this:
Finder doesn't show comments at tag time — check Get Info before tagging).
Comments on files *inside* a folder are never read on any code path: a
description is either metadata on the tagged post object (folder comment) or
content excerpted from inside a file (first paragraph / H1 — served only with
that file's own `public` tag). There is no third channel.

**Access to `private` entries — ideas (PROPOSED, for later):**
- **Passkeys (WebAuthn)** — the native answer: no passwords, synced by
  iCloud Keychain, phishing-resistant. A tiny allowlist of enrolled passkeys
  unlocks gated entries. Best long-term fit.
- **Expiring capability links** — per-entry signed URLs (1 day / 1 week /
  permanent) to hand to one person without them creating an account.
  Previously discussed and deferred; still the right second step.
- **Network-scoped preview** — requests from localhost (or the tailnet/VPN)
  may see unpublished entries, clearly bannered as "unpublished preview".
  Zero auth machinery; useful for preflighting a post before tagging it.

### Action tags (`Do` prefix) — REMOVED 2026-07-06

Superseded by the read-only content model (`entry-model.md`). The server no
longer writes into the content tree, so the action tags that mutated content
are gone: `DoRename` (which stamped a timestamp filename — that whole
timestamp-name convention is retired), `DoDate`, `DoPublish`, `DoGenerate`.
Date-stamping and folding are now native Finder gestures (an empty date-named
marker folder; Ctrl-Cmd-N "New Folder with Selection"); `.prompt` generation,
when built, will render into the outside-the-tree cache, never back into
content.

### Identity, index, renames

**SHIPPED 2026-07-06 — full spec in `entry-model.md` (canonical).** Identity is
the post's **name**, not a content hash or a server-assigned id. There is no
UUID, no `.id`, and no symlink ledger — the earlier hash-as-identity plan and
the DECIDED-2026-07-04 "old labels become symlinks" ledger are both superseded
by user-authored `alias` marker folders, which travel through iCloud/rsync
where symlinks and xattrs do not.

- **Renames survive via `alias <name>/` marker folders.** An empty folder
  named `alias old-label` inside a post makes `/old-label` a **301** to the
  post's canonical URL, forever. The same gesture adds alternative names and
  shortlinks. A post's aliases are listed on its entry page (server-rendered),
  so they are never fully invisible. A bare file carries no aliases — fold it
  into a folder first.
- **The address is a derived slug (SHIPPED 2026-07-08 — `post-model.md` §1).**
  A post's name stays natural (spaces, case, punctuation — `Fog Over The Bay.jpg`
  just publishes); the URL is a lowercase, hyphenated *projection* of it
  (`/fog-over-the-bay`). Recipe (`slug.rs`): NFKD, drop combining marks (folds
  `é`→`e`), NFC, Unicode-lowercase, apostrophes join (`it's`→`its`), every other
  non-alphanumeric run → one hyphen; non-Latin scripts are kept (Hangul, Cyrillic,
  Japanese). A punctuation-only name (`!!!`) has no slug and is date-addressed.
  **Collisions key on the slug**, so `Fog Over The Bay`, `fog-over-the-bay`, and a
  mixed-case URL all fold to one address (mixed case 301s to it). Reserved slugs
  (`saved`, purely-numeric) never claim the bare URL — the router owns those — so
  the post is date-addressed and carries a persistent reserved-name notice.
- **One flat namespace; the oldest claim wins the bare URL.** When a name is
  claimed by more than one post/file/alias, the oldest claim keeps `/name` —
  an established URL never changes meaning (cool URIs), so a newly-dropped
  `IMG_4392` can never silently retarget an old link. The other claimants stay
  reachable at their date paths, the winning page carries a visible "this name
  is also used by …" notice (and each colliding **timeline row** now expands a
  "N others share this address" disclosure), and the collision is logged loudly.
  A **dead bare label** (renamed away, no `alias`) returns a 404 with closest-slug
  suggestions — never an auto-redirect, which a reused name would mis-resolve.
  Handing a name over is deliberate: delete the old claim.
- **Revisions & families** are Finder's own ` copy [n]` suffix. A **family** is
  every item sharing a base name (`X`, `X copy`, `X copy 2`, …) and always
  collapses to **one current post + a revision stack, keyed by the base name —
  not by whether the base file exists** (SHIPPED 2026-07-08, `post-model.md`
  §5). Base present → it is current, the copies are archived snapshots dated by
  their own mtime (reachable from the entry's revision nav and their date-path
  URLs). **Base absent → the newest-by-mtime orphan copy is promoted to be the
  post** (claiming the base name and slug, so `/X` keeps resolving after the
  base is deleted — a recovery property), the rest its revisions. This ends the
  old *silent drop* of orphan copies; deleting a copy — or the base — never
  breaks the post. A routine Cmd-D backup and a `foo copy` that a later `foo`
  shadows are indistinguishable to a stateless scan (age can't separate them),
  so each collapse is **logged** but carries no reader-facing notice; both just
  work, and recovery from an unwanted collapse is one rename.
- **Listings (SHIPPED 2026-07-08 — `post-model.md` §6).** A folder with **no
  single document primary** renders as a **browsable index** rather than
  erroring: all-images → a gallery, mixed → a file list. Primary candidates are
  compared on the **slug** (`My Resumé/my-resume.md` matches). Two cases that
  used to be an `AmbiguousPrimary` error now **demote to a listing**: several
  candidates → the server declines to guess, lists, and shows a **prominent
  collision notice** (+ loud log); no candidate with several files → an automatic
  listing. An empty **`index/`** marker forces listing mode and silences the
  notice; a coexisting `index`/folder-name document becomes the listing's **intro
  prose**. **Membership is a `public` allowlist** — a file lists (and serves)
  only if tagged `public`, so `.DS_Store`, drafts and markers never leak; the
  header shows "N of M files public" so a reader can tell something is withheld.
  **Attachments** are the same rule with a primary present: a document post's
  `public` sibling files render as a file list **below the body** (filename
  order) — one idiom, drop-and-tag to add, untag to remove. Listing and post are
  one gradient (primary + public files = post with attachments; no primary +
  public files = pure listing). **Public subfolders** are folder rows in the
  list and are themselves **nested listings** browsable at `/label/sub/…` to any
  depth — a nested item's **path IS its canonical URL**, mirroring the filesystem
  verbatim (no flat-namespace claim, no identity). Visibility is gated at **every**
  level (fail-closed AND, deny-wins): once a request is scoped into a folder post
  it resolves to an asset, a nested listing, or a clean 404 — a hidden nested path
  404s identically to a missing one (no existence oracle), never falling through.
- The **only** remaining fail-closed *error* is **multiple date markers** — a
  genuinely no-right-answer case (an unknown publish date), shown as an errored
  row and an HTTP 500 page naming the conflict.

The server's **index and caches are disposable and live outside the content
tree** (see cache location below); the content folder is the source of truth,
opened read-only. The index still records which labels were once public so
unpublishing can answer **410 Gone** vs **404** after deletion.

### The file-privacy boundary — what a served byte may contain

**REWRITTEN 2026-07-15 (v0.18.0–v0.21.0; supersedes the image-only "media
privacy" section).** The boundary grew from "strip images" into one serve-time
rule for *every* file: **nothing is served unless we can say why it is safe** —
an allowlist, not a denylist. Originals on disk are never touched; cleaning
happens at request time.

**Threat model in one line: strip what the author can't see (EXIF, XMP,
document info, editor traces), keep what they can (the filename, the visible
date, the pixels and prose they chose to publish).** An author publishing
`mountain.heic` saw the mountain; they did not see the GPS fix, the camera
serial, or the editor's record of their home directory. Code failure or human
error must never leak those.

#### The allowlist (`media::classify`)

Every content-tree byte that would leave the server as a file is classified
first. The default arm is **Withhold** — a format nobody thought about is an
HTTP 415, not a leak.

| Class | Formats | Treatment |
|---|---|---|
| Raster image | JPEG, PNG, WebP | Surgical **segment strip** in pure Rust (`img-parts`): pixels stay byte-identical, ICC and a canonical re-emitted orientation tag are kept; EXIF/XMP/IPTC, comments, text chunks, embedded preview thumbnails (EXIF + `JFXX`), and MPF second images are gone. Deterministic output → stable content-hash ETag; recomputed per request (cheap). |
| Raster image, hard containers | HEIC/HEIF/AVIF, TIFF, GIF; JPEGs with post-EOI trailers (Motion Photo / MPF); mislabeled images caught by content sniff | **Transcode to a clean JPEG** by libvips in an OS sandbox (below), then that JPEG runs back through the segment strip (libvips carries source EXIF through; color survives, location does not). Disk-cached out of tree, keyed by source content hash. |
| PDF | `.pdf`, or `%PDF-` magic under any extension | `strip_pdf` (lopdf): drops `/Info` (author/tool/dates), every XMP `/Metadata` stream, `/PieceInfo`; re-serialization from the object table also drops **incremental-update shadow history** (a PDF edited in place keeps its old metadata bytes, invisible but recoverable). Encrypted or unparseable → withheld. |
| SVG | `.svg` | `strip_svg` (quick-xml streaming rewrite): `<metadata>` subtrees, foreign-namespace elements/attributes **and their `xmlns` bindings** (the URI alone fingerprints tooling — `sodipodi:docname` carries real filesystem paths), comments, processing instructions, `<script>`, `on*`/`javascript:` all removed; `DOCTYPE` → withhold; embedded base64 raster `data:` URIs are decoded, run through the image strip, re-embedded — uncleanable payload withholds the whole file. `<title>`/`<desc>` kept (accessibility). |
| Readable text | Valid UTF-8, no control characters beyond tab/newline/CR | Served raw: every byte is visible in any editor, so there is nothing invisible to strip. (Plain "valid UTF-8" is not the test — ZIP framing can decode as UTF-8; the control-character rule is what makes it *readable*.) |
| Everything else | Video, camera RAW, audio, Office, archives, fonts, UTF-16 text, unknown binaries | **Withheld (415).** The 415 page names the escape hatch below. |

Ordering matters: metadata-heavy media extensions are withheld *before* the
image gate (camera RAW `.dng` is TIFF-structured and would otherwise sniff as
an image), and PDF/SVG are caught *before* the text gate (their bytes can be
pure ASCII while the metadata hides behind the rendered view).

#### Tag grammar: `<visibility>[-original]`

Visibility tags gate publication (`public` today; `private` reserved — see
"Visibility" above). The **`-original` suffix composes with any visibility
tier**: a file tagged `public-original` is served as its **exact bytes** —
EXIF, XMP, trailers and all. It is the author's *universal*, per-file escape
hatch: it applies to every class in the table, including withheld formats
(video, archives — "host this exact file" is a legitimate wish). A future
secret-link tier would compose as `unlisted-original` without new machinery.

Safety around the escape: both serving choke points honor it identically; the
server logs a warning at serve time; stripped image pages carry a prominent,
non-dismissing "metadata removed for privacy" notice naming the opt-out; and an
`-original` image page warns **only when there is actually something to leak**
(GPS/camera/timestamp present) — an inverted, meaningful warning instead of
boilerplate.

#### Verify gates — the strip trusts nothing, including itself

Every cleaning path re-checks its **own output** before serving: images are
re-read with `kamadak-exif` (an independent parser from the `img-parts` code
that produced them), PDFs with `pdf_is_clean` (re-parse; assert every stripped
channel is gone), SVGs with `svg_is_clean` (re-parse; assert no dropped
construct remains, embedded rasters re-verified). Dirty *or unparseable*
output → loud error and withhold. A logic bug in a strip becomes a 415, never
a leak.

#### The clean store — quarantine, verify the file, promote (v0.25.0, 2026-08-24)

For raster images the verify gate is **structural**, not just a call in the
right place. Every producer — segment strip, sandboxed transcode, thumbnail —
funnels through one disk pipeline:

1. the cleaned output is written to a **quarantine file**
   (`<cache>/media/.q.<key>.<n>`, dot-prefixed so the startup sweep clears
   strands);
2. **that file is re-read from disk** and verified by independent parsers —
   `kamadak-exif` must find no sensitive metadata AND `imagesize` must confirm
   the bytes still parse as the container the content type claims (a torn
   write or a strip that mangled the file is refused, not served);
3. only a verified file is **promoted** — atomic rename — into the clean store
   `<cache>/media/clean/`, the **sole byte source** the serving layer reads;
4. store reads **re-verify on every request**: a corrupted or tampered store
   entry withholds loudly and is deleted for regeneration, never served.

Nothing is served from a pre-promotion buffer, so there is no window between
what was verified and what is served. The gate is compile-enforced:
`media::CleanBytes` has private fields and its only constructor is the
verifier (`verify_bytes`), so no serving path — present or future — can emit
image bytes that skipped it. Deterministic strips (§8) make the store a cache:
strip outputs are now disk-cached alongside transcodes, keyed by source
content hash + pipeline version tag. The sweep keeps `media/clean/` and
removes everything else under `media/` at startup (in-flight temp files and
the legacy flat cache layout). PDFs and SVGs keep their in-memory verify gates
(above); folding them into the same store is a candidate follow-up.

#### Honest representation URLs — the `/jpeg` and `/thumb` rungs (v0.30.0,
2026-08-30; path form of v0.26.0's `?as=jpeg`, per sajt.md)

A URL's extension never lies about the bytes it returns (DECIDED 2026-08-25).
Strippable formats (JPEG/PNG/WebP) serve their own
container at the bare URL — stripped, pixels intact — so nothing changes
there. A transcode-only format (HEIC/TIFF/AVIF/GIF/BMP) serves a JPEG
rendition, which lives only at a path that says so — renditions are
subresources of the file, addressed by rungs in canonical order only
(`/jpeg`, then `/thumb`):

- `/photo.tif` (bare, no `public-original`) → a **styled explainer page**
  (was a 303 + text body pre-0.30): why the exact bytes are not served, a
  link to the rendition, and the `public-original` way through. A real page,
  so a static host ships it as a plain file.
- `/photo.tif/jpeg` → the clean JPEG rendition (`image/jpeg`).
- `/photo.tif` with `public-original` → the exact bytes; container matches
  the name, so the bare URL is honest by definition. `/jpeg` still serves
  the clean rendition (it is derived, never a leak).
- Gallery tiles are always JPEG renditions: a non-JPEG source's tile is
  `/photo.tif/jpeg/thumb`; a JPEG source's tile is `/photo.jpg/thumb`. A
  bare `/photo.tif/thumb` 301s to the honest `/jpeg/thumb`; a no-op
  `/photo.jpg/jpeg` 301s to its parent (the closure model collapses no-op
  rungs).
- The rungs name the pipeline's own output, never an on-demand converter:
  `/jpeg` on a PNG/WebP is a 404 (fail closed), rungs on non-image files are
  404, and only the canonical rung order exists (`/thumb/jpeg` is a miss).
- Every derived rendition carries `Content-Disposition: inline;
  filename="photo.jpg"` (tiles: `photo-thumb.jpg`), so a save never lands as
  "jpeg" from the rung name — and never as JPEG bytes under a `.tif` name,
  which the query form silently did.
- On a folder-post asset the rungs compose the same way
  (`/{post}/photo.tif/jpeg/thumb`); a literal file always wins its own name
  (rungs are recognized only when no real file answers the full path).
- The image page displays the rendition (`<img src="…/jpeg">`) and its
  notice says so: "Shown as a JPEG rendition …".

Transcode quality is author-side only, never a URL parameter: a visitor
`?quality=` would make every distinct value a fresh sandboxed vips run plus a
permanent clean-store entry — attacker-scalable CPU and disk — for no visitor
benefit over a well-chosen constant. **`--jpeg-quality` (1-100, default 85,
SHIPPED v0.27.0)** sets the full-view rendition quality; the value is part of
the clean-store cache key, so changing it regenerates renditions on demand and
can never serve a stale quality. Sizes stay fixed (full view: max edge 4096;
tiles: max edge 600 at Q=80) — the `/thumb` rung selects between exactly these two
renditions, it is not a size dial, for the same reason quality is not one.

#### The sandboxed transcode (v0.21.0, 2026-07-15)

libvips decodes attacker-controlled bytes, so the subprocess is OS-confined —
a decoder exploit reads pixels, not files:

- **Per-job scratch directory** `<cache>/media/.tx.<key>.<n>/` holds the input
  copy and the output JPEG: the only writable path in the sandbox, and (input
  inside it) the only file tree read beyond the system. Deleted, untrusted
  input included, the moment the job ends; a startup sweep clears anything a
  killed process stranded.
- **macOS: `sandbox-exec`** with a deny-default Seatbelt profile. Finding: a
  `(deny default)` child aborts inside dyld on modern macOS unless the profile
  imports Apple's `dyld-support.sb`; with it the allow list is just
  `/nix/store` (read+exec), `/System`, `/dev`, and the scratch. Live-verified
  (macOS 26.5): HEIC transcodes; reading outside the scratch, writing outside
  the scratch, and HTTPS egress are all denied.
- **Linux: `bwrap`** — `--unshare-all` (no network) `--die-with-parent`
  `--new-session` `--clearenv`, read-only system binds, scratch mounted at
  `/scratch` as the only writable mount. Command assembly is unit-tested and
  live-verified (2026-08-24, aarch64 Linux 6.18 in an Apple `container` VM,
  flake devshell): a GPS-laden TIFF transcodes to a clean JPEG through the
  sandboxed vips, and controlled probes of the same invocation shape confirm
  reads outside the scratch (path not even visible), writes outside the
  scratch, and TCP egress are all denied while the identical operations
  succeed unsandboxed.
- **Fail-closed policy:** no sandbox tooling → transcodes are refused
  (affected formats withheld, 415) with a loud startup warning;
  `--unsandboxed-transcode` is the explicit operator opt-out. An available
  sandbox always wins — the flag cannot disable one — and an uninitialized
  mode reads as disabled, so nothing ever transcodes unconfined by accident.
- **Decoder hygiene, independent of the sandbox:** `VIPS_BLOCK_UNTRUSTED=1`
  refuses libvips's bundled ImageMagick fallback loader (its hostile-image RCE
  history is why we never shell out to ImageMagick) and the other loaders vips
  flags untrusted — so the ImageMagick-only tail (BMP, JXL, JP2K) is withheld,
  not decoded by untrusted code. One worker thread, a 25 s timeout with
  kill-on-drop, an RLIMIT_CPU backstop, a concurrency limiter.
- **Pre-decode megapixel cap:** declared dimensions are read with a pure-Rust
  header parse (`imagesize`) and anything over **100 MP** — or whose
  dimensions cannot be read at all — is withheld before libvips runs
  (decompression-bomb defense; an iPhone panorama is ~63 MP).

Thumbnails (the `/thumb` rung, 600 px gallery tiles) ride the same pipeline and always
strip, even for `-original` files — a tile is a derived preview; the exact
bytes stay at the rung-free URL.

#### Route guarantee — two choke points

Content-tree bytes leave the server through exactly two functions:
`serve_raw_bytes` (a bare-file / folder primary) and `try_asset` (an in-folder
gallery/attachment asset). Both run `classify()` and match its `Disposition`
enum **exhaustively** — adding a class forces both call sites to handle it at
compile time. There is no third path: archived revisions render the current
entry's stripped URL, not raw revision bytes; the grader's `/_raw` route is a
separate localhost-only authoring tool showing authors their own bytes;
`serve_embed_asset` is the out-of-tree remote embed cache (residual below);
`serve_static` serves operator-owned assets.

#### Shipped history

C8a segment strip + `original` opt-in (2026-07-09, v0.13.0); C8b vips
transcode (v0.14.0); C8c thumbnails (v0.15.0); adversarial-review fixes
(2026-07-11, v0.15.1: MPF/Motion-Photo trailer walk to the primary EOI,
`.tif`/`.jpe`/`.jfif` alias gap, `set_exif` panic guard + `CatchPanic` layer,
`JFXX` drop); class-boundary fail-close for video/RAW/audio (v0.16.0);
`public-original` + inverted EXIF warning (v0.17.0); the full allowlist
boundary (2026-07-15, v0.18.0); PDF strip + verify gates (v0.19.0); SVG strip
(v0.20.0); sandboxed transcode + MP cap + temp sweep (v0.21.0).

#### Honest residuals (accepted or deferred, in the open)

- The image verify gate reads EXIF-class metadata (`kamadak-exif`), not XMP —
  a hypothetical strip bug that left *only* XMP behind would pass it. (Both
  strip paths do drop XMP; the gate is a narrower backstop than the strip.)
- `pdf_is_clean` re-parses with the same library (lopdf) that wrote the output:
  it catches our logic bugs, not a lopdf serialization blind spot. The SVG
  gate's embedded-raster check *does* use an independent parser.
- Nothing mechanical stops a future third serving path from skipping
  `classify()` — the exhaustive-match protection covers the two existing call
  sites; the route guarantee is held by convention and review.
- The readable-text gate withholds UTF-16/legacy-encoding text files (we
  cannot cheaply prove the author read what we would serve).
- GIF→JPEG transcode kills animation; a clean animated path would need a
  GIF-specific comment/extension strip.
- `Cache-Control: max-age=3600` on image bytes = up to one hour of un-publish
  latency at clients/proxies.
- **Embed-cache images are not stripped** (`serve_embed_asset`): remote
  og:image/oEmbed media cached at fetch time — a separate subsystem; routing
  it through `prepare()` would make "every served image passed one strip gate"
  literally true. Deferred.
- quick-xml 0.38 (pulled by `plist`, itself latest) has two DoS-class
  advisories; its input here is local xattr plists, not attacker bytes. Our
  SVG strip uses quick-xml 0.41 side by side.
- The Seatbelt profile allows exec only from `/nix/store` — a non-Nix vips
  (e.g. Homebrew) will not run under it. The deploy contract is the Nix
  devshell; a failure is loud, not silent.
- Habit worth keeping: run `cargo audit` periodically (or in CI) — the strip
  crates parse attacker-controlled bytes by design.
- Video stays withheld (no ffmpeg path yet); `public-original` serves exact
  bytes when the author explicitly chooses.

## URL scheme

Labels are primary; dates are for timeline filtering only.

```
example.org/                     timeline (newest first), tag cloud as header
example.org/saved                the reader's bookmarked entries (client-side)
example.org/+favorite            my hand-picked favorites (a plain Finder tag)
example.org/open-source-licenses entry (file or folder), label = URL
example.org/open-source-licenses/report.pdf   asset inside a bundle
example.org/2026-03/             listing: March 2026
example.org/+design+rust         tag filter (AND); comma = OR
example.org/sunset.md            raw source (extension = raw)
```

Old date+label URLs 301-redirect to label-only. `/best` and `/everything`
are retired (DECIDED 2026-07-04): the timeline with its filter row
(everything · notable · best · ★ favorites) covers both — see Presentation.

**SHIPPED 2026-07-08 (`post-model.md` §2) — precision-aware dates, time as a
path segment, BCE, scheduled publish.** Publish dates are now a precision-aware
`PostDate` (year / month / day / minute / second, with signed BCE years —
`-3000` is literally 3000 BCE). The `?time=` disambiguator is gone: the
time-of-day is a **path segment** after a full day, `/2026/07/04/191430`
(`/2026/07/04/191430/label` on a same-day slug collision). A label-free
`/Y/M/D[/HHMMSS]` is a **date-time deeplink** — a rename-durable citation that
resolves by timestamp (301 to whatever the canonical URL is now) and degrades
to the **day view** on no/ambiguous match, never a 404. Date-marker folders and
date-**named** posts share one grammar `[-]YYYY[-MM[-DD[Thhmm[ss]]]]` (the
mandatory `T` is gone — a bare date is a valid marker); a whole-date name is
unlabeled and date-addressed, a `<date> <text>` name is date-addressed with the
text as a **display title** (never a slug/bare-URL claim). A post's display
title is its primary's first **H1** if present, else the filename text
(foundation #4) — but the **slug/identity stays filename-derived, never the
H1**. A **future-dated** post is held out of the served set until its moment;
the server wakes at the next scheduled timestamp to rescan (a fail-safe:
a future misdrop stays hidden). The timeline groups by precision — "Month Year",
a bare year, or "3000 BCE".

**No more untitled entries — SHIPPED 2026-07-06.** Under the old convention a
filename that was only a timestamp minted a bare-timestamp canonical URL that
collided with the date-filter route (the entry rendered as a timeline, so
Continue couldn't inline-load it). In the folder model a post's name is its
folder or file name, so there is no nameless entry: the one-shot migration
folded any unlabeled file into an `untitled/` folder post (label `untitled`),
and `parse_filename` — which used to mint `None` labels — is gone.

**View filters — path segments (SHIPPED 2026-08-30, sajt.md).** Topics
stay path-based (`/+design`, `/+design+rust` AND, `/+design,rust` OR):
permanent, linkable cool-URIs. The view axes moved from query params into
**reserved path segments** — `/notable` (the two-state everything/notable
floor; with no grading ledger the notable bucket is simply empty) and
`/favorites` — because they are author-side content selection, so every
combination is a real page the static build can emit. Canonical scope order
is **date, then tags, then view** (`/2026/+design/notable/favorites`); every
ordering of the same filters is accepted and 301s to the canonical spelling
(tags sorted case-insensitively, no trailing slash) — redirects are shock
absorbers, not a second grammar, and the site itself only ever emits
canonical URLs. The old query spellings (`?grade=`, `?favorites`) are gone
without redirect absorption (never really online, no legacy). Search is the
one filter that stays a query — `?search=…` (renamed from `?q=` 2026-08-30;
per-request input, not a resource) — layering onto any scope path. Every
header control is a real `<a>` / GET-form (no JS required), so the address
bar always reflects the current view and right-click → Copy Link shares the
exact filtered timeline. `/saved` is the reader's bookmarks, a client-side
view (the server renders the full timeline, the browser filters to what it
has in `localStorage`); it composes with the view axes (`/saved/notable`).
Post URLs never carry view state: opening a post drops the view, and every
alias 301s to the one bare canonical address. `notable`, `favorites`, and
`saved` are reserved words — a post so named stays reachable at its date
address, never at the bare label.

## Authoring formats

**Kinds are a medium, not a format (SHIPPED 2026-07-08, `post-model.md` §3).**
`kind()` classifies what a post *is* and how it is served — `photo`, `html`
(self-contained document), `text` (poured into the shell), `link`, `folder`
(a listing), or `file` (opaque download) — never the format (the extension) or
genre (an author tag). `note`→`text` and `page`→`html` were renamed. Extension
**aliases** normalize once (`.markdown`→`md`, `.text`→`txt`, `.asciidoc`→`adoc`)
feeding both `kind()` and the render path, closing the old gap where a
`.markdown`/`.text` file silently *downloaded*. A **dotless** bare file
(`README`) is `text` when UTF-8-decodable (rendered preformatted), else an
opaque `file`.

- **CommonMark** (`.md`) — **comrak, in-process (SHIPPED 2026-09-09, v0.38.0;
  replaces Pandoc `commonmark_x`)**. No subprocess and no external dependency
  for the common case. The extension set mirrors what `commonmark_x` gave
  posts: tables, footnotes, strikeout, task lists, definition lists,
  super/subscript, `$` math, `:emoji:` codes, GitHub alerts (`> [!NOTE]`),
  autolinked bare URLs, smart punctuation, implicit figures (an image alone in
  its paragraph becomes a `<figure>` captioned by its alt text). Heading ids
  come from the site's own slug recipe (`slug.rs`), the one the in-page anchor
  script mirrors, so `#section` links resolve without JavaScript. Bare URLs on
  their own line expand to rich embed cards via the existing embed system
  (YouTube, Bluesky, Mastodon, App Store, generic OpenGraph…). That *is* the
  Markdown equivalent of `video::…[youtube]` — paste the URL, get the
  player/card.
- **AsciiDoc** (`.adoc`) — **Asciidoctor as a helper subprocess (SHIPPED
  2026-09-09)**, in its `secure` safe mode: no includes, no file access (an
  `include::` degrades to a dead link, verified), invoked like the other
  helpers (concurrency cap, wall-clock timeout, stdin/stdout). Pandoc was
  never an option: it has **no AsciiDoc reader**, so `.adoc` had never worked.
  Asciidoctor natively supports `video::RvRhUHTV_8k[youtube]`, admonitions,
  and source blocks. Absent, only `.adoc` posts fail, with a message naming
  the tool.
- **Footnotes — one shape for every format (SHIPPED 2026-09-09, v0.39.0).**
  comrak emits the GitHub shape (`sup.footnote-ref > a[href="#fn-N"]`,
  `section.footnotes > ol > li#fn-N`, `a.footnote-backref`); Asciidoctor has
  its own (`sup.footnote`, `div#footnotes`). `src/footnotes.rs` rewrites the
  latter into the former as a post-pass, down to comrak's numbering of a note
  cited twice (`fnref-N-2`, a second back link), so the stylesheet and the
  sidenote script know one shape. Pairing on the client follows the links
  (`href` to `id`, scoped to the post), never position, so a repeated
  citation gets its own sidenote and Continue-appended articles with
  repeated ids cannot cross-pair. Verified byte-identical to comrak's output
  for the same notes after sanitizing.
- **Syntax highlighting — one highlighter for every format (SHIPPED
  2026-09-09).** `src/highlight.rs` tokenizes fenced code with syntect
  (pure-Rust regexes, no C library) and emits the kate token classes the
  stylesheet already themes (`kw`, `dt`, `fu`, `st`, `co`, …) in the
  `<div class="sourceCode"><pre class="sourceCode X">` shape. It runs as a
  post-pass over engine output, so comrak's and Asciidoctor's
  `<pre><code class="language-X">` blocks come out identical. Unknown
  languages get the wrapper unhighlighted; blocks over 256 KB are escaped,
  not tokenized. Every line is a bare `<span>` child of `<code>` holding its
  tokens and its own line end (the fence newline dropped), so the stylesheet
  gives the line under the pointer a faint full-width row without a class
  (`pre > code > span:hover`); a bare unlabeled `<pre><code>` block is left
  untouched and has no rows.
- **Pandoc is gone (DECIDED 2026-09-09, v0.39.0).** It had been kept as an
  optional helper for `.rst`/`.org`/`.tex` only. More formats are a later
  exploration, each with its own engine and the same three post-passes.
- **Plain-text posts (v0.40.0).** `.txt`, and `.rst`/`.org`/`.tex` until each
  gets an engine, are posts shown as written: HTML-escaped at the sink
  (`templates::Body::Plain`), set monospace with line breaks kept and long
  lines wrapped (`article[data-body="plain"]`), so hand-made tables survive,
  but without the code-block box. Their raw URL is
  served `text/plain; charset=utf-8` outright (the MIME guess table would
  make a browser save them), so the source opens in the browser. One
  predicate, `entry::is_plain_text_ext`, drives the medium (`text`), the
  render lane, the excerpt/title extractors and the content type.
- Everything else as today: `.html` passthrough, images in viewer, `.prompt`
  via Claude API, other files as downloads. (The old `.link`
  extension is **retired** — a link is now the `link_url` axis below, not a
  special format.)

### Languages (DECIDED 2026-09-09, not yet built; lands before or with the comrak port)

The site has one language, from `Sajt.toml` (`language`, default `en`),
emitted as `<html lang>` on every page (shipped 2026-09-08). Posts may differ
from it, and a post may exist in several languages. Everything below is
filename grammar and static markup; nothing needs a server decision.

- **A post in another language**: a language subtag before the extension,
  `brev.sv.md`. The post's `lang` is `sv`; the page still belongs to the site.
  The subtag is a BCP 47 tag as `Sajt.toml` accepts it (`sv`, `pt-BR`).
- **One post, several versions**: `hello-world.md` and `hello-world.sv.md`
  side by side in the post folder. The unsuffixed file is the site-language
  version and keeps the bare URL `/hello-world`; each other version lives at
  `/hello-world/<tag>` (`/hello-world/sv`). Those URLs are canonical, stable,
  and shareable: they never redirect. Every version carries
  `<link rel="alternate" hreflang>` for all versions plus `x-default` for the
  bare URL. The timeline shows one row per post (the site-language version)
  with a quiet "also in svenska" line naming the other versions in their own
  language. A version is served under the same per-file rule as any other
  file content: its own `public` tag.
- **Inline language**: the sanitizer allows `lang` and `dir` on elements, so
  a quoted paragraph in another language is marked correctly (this is part
  of the comrak port).
- **Automatic choice is an edge feature, never a page feature.** A static host
  serves `/hello-world` as is. The CDN adapter may add a redirect: a request
  for the bare URL whose `Accept-Language` prefers a language the post exists
  in gets a `302` to that version with `Vary: Accept-Language`. It only ever
  fires on the bare URL and only when that version exists, so it is
  dead-front-safe (edge logic over the same bytes, no content decision).
- **The "you were redirected" notice, without JavaScript.** The redirect's
  `Location` carries a fragment: `/hello-world/sv#redirected-for-language`.
  Browsers keep the fragment, and every multi-version page has
  `<aside id="redirected-for-language">` at the top of `<main>` (so the
  fragment scroll is a no-op). CSS `:target` makes it prominent; otherwise it
  is the quiet version line. Its text is true for anyone who lands on the
  URL, including the recipient of a copied link: "Svenska. Also in English",
  each name an autonym linking to that version. Nothing is one-time state:
  the fragment simply goes away on the next navigation.
- **Progressive enhancement**: the entry-page script, on load, when the hash
  is `#redirected-for-language`, sets `data-redirected` on the aside and
  calls `history.replaceState` to drop the fragment from the address bar, so
  a copied URL is clean. The CSS keys on `:target` *or* `[data-redirected]`,
  because some browsers stop matching `:target` once the URL changes.
  Without JavaScript the fragment stays until the next click; accepted.
- No id needs reserving: the heading-anchor script already suffixes an id
  that is taken.

**The `link_url` axis + outbound cites (SHIPPED 2026-07-09, `post-model.md` §4).**
A post's outbound destination is orthogonal to its `kind`. The scanner resolves
`Entry.link_url` (always an `http(s)` URL that passed the scheme guard, or `None`)
from a `.webloc`/`.url` bookmark, a text file whose **entire** content is a single
URL, or a folder's `link.*` sidecar; anything else — a `javascript:` target, a
title line *plus* a URL — is refused and the file stays an ordinary post (fail
closed, no clickable link). **Amended 2026-07-15 (v0.23.0): inside a folder,
cite candidacy is intent-carried, never content-sniffed** — a bookmark format
(`.webloc`/`.url`) qualifies under any name (the format is the intent), a
general text file only when its stem is `link`; a `notes.txt` that happens to
hold one URL stays an ordinary attachment, so content edits never change a
file's role. Bare-file posts at the top level keep the single-URL predicate
(the file is the whole post — only its own rendering changes). Two shapes
follow:
- **The post IS the link** (`kind() == "link"`: a bare bookmark / single-URL
  text) — its body is the rich embed card; its timeline row is the destination's
  own headline (a labeled bookmark keeps *our* label linking to *our* page and
  cites the source beneath; an unlabeled one promotes the cite into the heading
  with a quiet `¶` permalink back to our card page).
- **The post CITES a link** (a document with a `link.*` sidecar) — it keeps its
  own medium and words, and a `<cite>` source line renders beneath the body
  (a `#source` section on its page, an inline cite in its row).
The cite is a semantic `<cite>` (favicon tile · title · domain, external `↗`,
`rel="noreferrer"`; `http://` flagged in pure CSS) per `static/link-rows-mockup
.html`. Title comes from the embed cache (`link_title`), degrading to the bare
domain on a fetch miss, and is dropped when it merely repeats our own heading.
The **favicon is a CSS-only letter tile** (a hashed color via `data-tile`, no
inline style) — a real favicon fetch is deferred precisely so a link row makes
**zero third-party requests**, never leaking a reader's IP to the destination.

**Outbound safety — scheme guard + SSRF-hardened fetcher (SHIPPED 2026-07-08,
`post-model.md` §7; the fetcher half of §4).** Two fail-closed choke points, both
security-critical:

- **Scheme allowlist (`src/outbound.rs`).** Every `<a href>` in rendered body
  HTML passes through `classify_scheme` (an allowlist, not a blocklist): only
  `https`, `http`, `mailto`, `tel` reach the reader. `http` is allowed but
  **flagged** by the pure-CSS `main a[href^="http://"]` rule (no server class);
  everything else (`javascript:`, `data:`, `vbscript:`, `file:`, custom handlers,
  protocol-relative `//host`) is **refused at render time** and rendered as a
  neutralized `[data-unsafe-link]` span — never a clickable anchor, because CSS
  cannot stop navigation. Classification defeats browser-style obfuscation
  (tab/newline stripping, entity decoding, case). External web links additionally
  gain `rel="noreferrer"` for reader privacy. Applied as the last transform
  before the body enters the shell, so nothing an embed expansion produced can
  reintroduce an unsafe link. (Cites reuse the same guard — see §4, C7b.)
- **SSRF-hardened fetcher (`src/embed.rs`).** All scan-time outbound requests
  (oEmbed, OG scrape, iTunes, media download, liveness) funnel through one
  `guarded_fetch`. reqwest auto-redirect is disabled; each of up to 5 hops is
  followed by hand, its host re-resolved and **every** resolved IP checked
  against `is_public_ip` (rejects loopback / RFC1918 / link-local incl. the
  `169.254.169.254` metadata address / ULA / CGNAT / multicast / reserved /
  documentation / IPv4-mapped IPv6 / 6to4 / Teredo), and the connection
  **pinned** to a vetted address so a DNS rebind between check and connect cannot
  slip through (non-ASCII/IDN hosts are refused so the pin key can't be dodged).
  Bodies are capped while streaming (HTML ~1 MB, media ~8 MB) under a 5 s per-hop
  timeout. This closes the prior hole where reqwest's default auto-redirect could
  follow a `Location:` into an internal address unchecked.

### Content security — CSP, raw-HTML, standalone-`.html` sandboxing (DECIDED 2026-07-08, REVISED 2026-07-09, SHIPPED v0.12.0 2026-07-09)

The outbound scheme guard above covers `<a href>` navigation only. Before
v0.12.0, Pandoc passed **raw HTML verbatim** (`<script>`, `on*` handlers,
`<iframe>`) and there was **no Content-Security-Policy**, so a content author —
or anything upstream of the content tree (a stray/synced file, a pasted quote,
the third-party oEmbed HTML the embed system injects) — could execute stored
script in our origin without touching a link. Single-author + no-cookies made the
blast radius "deface / phish the visitor", not account theft; but the stated bar
(a hospital or law firm, hence future auth + multiple contributors) meant closing
it durably. v0.12.0 does, in five commits (S2.1–S2.4 + tiers).

**The one invariant: author-supplied HTML never executes in the site's own
origin.** Every serving mode below jails it in an opaque origin, so choosing a
mode is never a security decision. What shipped:

- **Author HTML sanitized at the source with `ammonia`, NOT an engine flag.**
  Pandoc 3.7's `-raw_html` reader extension is a **no-op** — `commonmark-raw_html`
  (and `gfm`/`commonmark_x` variants) still emit `<script>`/`on*`/`javascript:`
  verbatim (verified), and comrak is run with raw HTML deliberately passed
  through. So instead of trusting a reader flag, every engine *output*
  (markdown/rst/adoc/org/tex → `RenderedContent::Html`) is run through
  `sanitize::body` (`src/sanitize.rs`, ammonia/html5ever): scripts, event
  handlers, `<iframe>`, `<style>`, inline `style=`, and unsafe-scheme URLs are
  stripped, while the structural markup the engines rely on (syntax-highlight
  classes, footnote/heading ids, `<section>`, a disabled task-list checkbox,
  `lang`/`dir`) is kept. Table alignment — Pandoc's one inline style, comrak's
  legacy `align` attribute — is rewritten to a `data-align` attribute first. This is strictly better
  than a per-format flag (parser-based, no denylist gaps, format-agnostic) and
  the strict CSP below is still the backstop. `.html`/`.htm` is the **one**
  deliberate raw surface — and it is jailed (next point).
- **Standalone `.html` — three modes, one URL each (no srcdoc, no sniffing, no
  author tag).** NOTE: the 2026-07-08 `srcdoc` plan was WRONG — an
  `about:srcdoc` document has no HTTP response and *always* inherits the parent
  CSP (a `<meta>` CSP can only tighten), so the parent's `script-src 'self'`
  would block every inline `<script>` in the dropped-in document and kill the
  feature. Instead the iframe loads a real URL that carries its own headers:
  - `/slug` — **embedded (default)**: the normal blog shell; the body is
    `<iframe src="{asset-URL}?embed" sandbox="allow-scripts allow-popups">`
    (never `allow-same-origin`, which would let the frame unsandbox itself).
    Auto-height via a ~15-line reporter (`ResizeObserver` → `postMessage`)
    injected into the `?embed` copy only; the parent treats the number as
    untrusted — a min floor (~8rem) and a sanity ceiling against pathological
    values, but otherwise **uncapped**: the post is as tall as it wants, like
    any `.md` post. Reading-width grabber works (width is the parent's
    property; the inner document reflows and re-reports).
  - `/slug?fullscreen` — full-viewport frame + slim top bar (site mark, back,
    "show only the HTML" link). No injection; the frame scrolls itself. The
    expand link on every embedded frame targets this — a stateless real-link
    control like the `/notable` view segment.
  - `/{slug}/{file}.html` — the **exact bytes** at the folder-post asset path
    that already exists (bare-file `.html` posts gain an asset-style address).
    No iframe needed: the response's own `Content-Security-Policy: sandbox
    allow-scripts allow-popups; default-src 'none'; script-src 'unsafe-inline';
    style-src 'unsafe-inline'; img-src 'self' data:` jails even a top-level
    navigation (fail-closed: the jailed doc cannot fetch third-party — authors
    inline assets). Byte-exact: content hash and ETag hold; this is the floor
    every unrecognized client gets. **JS runs in all three modes** (canvas,
    widgets, games); the opaque origin just can't read our cookies/DOM/storage
    or act as the user.
- **Strict CSP, no `unsafe-inline`, no nonces — via externalized JS/CSS.** Move
  our inline `<script>`/`<style>` to files served by the site itself. Page policy:
  `default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self'
  data:; frame-src 'self'; object-src 'none'; base-uri 'none'; form-action
  'self'; frame-ancestors 'self'`. Companion headers everywhere:
  `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`,
  `Cross-Origin-Opener-Policy: same-origin`; HSTS at the Caddy layer. The CSP
  selector **fails closed**: only our own `text/html` pages get the page policy;
  every *other* content type gets `sandbox; default-src 'none'` (harmless on
  images/JS/CSS subresources, whose own CSP a browser ignores, but a hard jail on
  any script-capable document — **an uploaded SVG, or a `.xhtml`, is a same-origin
  script vector when navigated to directly**). HTML/XHTML-family documents served
  raw (the drop-in feature) instead get the functional sandbox CSP (below), keyed
  on the resolved MIME (`text/html`/`application/xhtml+xml`) so no extension slips
  the jail. (An earlier fail-*open* default let `.xhtml` run script in our origin;
  caught in the S2 adversarial review and fixed before release.) External JS/CSS
  is cached across pages;
  the no-FOUC prefs bootstrap (typeface, reading width) is a tiny **external**
  render-blocking `<script src>` in `<head>` — equally render-blocking,
  CSP-clean, cacheable, so no nonce exists anywhere. (The light/dark *theme* is
  already flash-free: pure CSS `light-dark()`, OS-driven, no JS.)
- **Replace the hand-rolled oEmbed sanitizer with `ammonia`** (allowlist on
  html5ever, strips `<style>`/`style=`/`javascript:`/`<iframe>` by default).
  The current denylist code has real bugs (mixed-case `</Script>` terminator
  miss; UTF-8 mangled byte-by-byte in `remove_event_handlers`; unquoted `on*=`
  values leak). Re-sanitize on cache read so already-cached embeds are covered.
  This also resolves the oEmbed-vs-`style-src 'self'` tension: embed cards are
  styled by our own `EMBED_CSS` (Twitter fetched `omit_script=true`), so
  stripping foreign inline styles costs nothing and no embed iframes are needed.
- **Every inline `style=` removed so `style-src 'self'` stays strict.** More
  survived than the embed accents: the tag cloud (font-size weighted by
  frequency, recency ink/weight, Finder color), grade meters (fill width), row
  tag pills, and listing badges all emitted inline styles. All converted to
  CSS-driven `data-*` attributes (matching the no-classes/attribute-selector
  convention): embed accent → `data-platform` (+ `data-variant="mac"`); cloud
  size → `data-size` (8 tiers), recency → `data-recency`, Finder color →
  `data-tag-color`; meter fill → `data-fill` (5% steps); badges → `data-kind`.
  The repetitive numeric tiers are generated in `assets::numeric_tiers`. Pandoc's
  table-alignment and image inline styles are handled by `sanitize::body` (align
  → `data-align`; percentage image widths dropped → responsive). Result: served
  pages carry **zero** inline `style=`. (The strict `style-src 'self'` was
  weighed against `'unsafe-inline'` — chosen strict for the hospital/law-firm
  bar, since script-src stays locked and default/img/font-src 'self' already
  close CSS-based exfiltration.)
- **CSP delivers the "zero third-party requests" reader-privacy goal** (below):
  the browser refuses any request to any other host, so a reader's IP/UA/
  Referer never leak to an outside server; the jailed `.html` responses carry
  their own third-party-blocking policy. **Safety verdict:** the jail makes
  `.html` XSS-safe unconditionally; the CSP makes every page privacy-safe.

## Presentation

**DECIDED 2026-07-05: the "rows" glass variant** (supersedes the 2026-07-03
"no glass, anywhere"). After comparing plain / rows / full in
`static/timeline-glass-mockup.html`, *rows* won: on the timeline every
entry floats as a glass card, and the segmented level control and the help
dialog pick up the same glass material. The tag-cloud header and the entry-post
body stay plain — the "full" variant that also glasses the cloud header was
passed over. The glass sits directly on the plain background (no tinted wall).
Reference realizations: `static/timeline-glass-mockup.html` (rows) and
`static/entry-page-mockup.html`.

- **Entry pages** — the *post body* stays pure typography on the page
  background: no card, no material, no backdrop filter. Only the timeline rows
  and the two chrome pieces above wear glass.
- **Site nav — DECIDED 2026-07-04: there isn't one.** The timeline is the
  site root, and its header is the tag cloud itself — a nav row saying
  "Timeline" on the timeline is noise, and **Everything is dropped** (the
  timeline with the everything filter *is* everything), and the standalone
  Topics page is removed — the cloud on the timeline replaced it. No site
  title — the domain is the brand. Entry pages keep a single quiet way back
  to the root. (DECIDED 2026-07-04: tag pages are called **Topics** in
  copy — "tags" is the mechanism, "topics" is what a reader is actually
  browsing.)
- **Dark mode** — follows the OS via `prefers-color-scheme` /
  `light-dark()`. **No theme UI** (DECIDED 2026-07-03; the switcher was
  removed from the live templates). The site never duplicates UI the OS
  provides — no share buttons either.
- **Sidenotes (right)** — footnotes render as Tufte-style margin notes when
  there is room, inline note blocks otherwise. Authors just write standard
  footnote syntax; the entry-page script clones the engine's footnote list
  into the margin (one shape for every engine, see "Authoring formats").
- **Heading anchors (left)** — a `#` appears in the left margin on
  hover/focus of a heading, linking to it. **DECIDED (direction)
  2026-07-03: the left margin stays otherwise empty** — anchors are the only
  thing that lives there, mirroring sidenotes on the right.
- **Mini-TOC — DECIDED 2026-07-04**: the one exception to the empty left
  margin. Very long entries only (threshold: several `h2`s / long reading
  time) get a collapsed "On this page" disclosure; short entries never show
  it. Collapsed by default, plain text links, no scroll-spy machinery.
- **Reader-adjustable width (fading segment, DECIDED 2026-09-09)** — a
  full-height grab zone down the right edge of the content column, 1.5rem
  out, that draws nothing at rest. As the pointer nears the edge (56px) a
  hairline window fades in around the pointer's height, soft over 3rem at
  both ends, violet on direct hover and while dragging. It scrolls with the
  page (the earlier two-bar grip was fixed to the viewport's middle and
  followed the reader; a full-height resting line was tried the same day
  and fought the timeline's glass rows). Keyboard: arrows step, Home
  resets; double-click resets; persisted per site, shared by timeline and
  post. The script creates the element, since it does nothing without JS.
  Sidenotes move between margin and inline automatically; they start 3rem
  out, clear of the zone.
- **Rules fade at both ends — DECIDED 2026-09-09**: every hairline that
  spans a column (search underline, `hr`, footnotes, post footer, Continue
  seams, aliases, the shortcuts dialog) fades out over its outer 14% on each
  side, the same softness as the handle window, so no rule ends in a hard
  stop. Painted with `border-image` over an ordinary border; the search
  underline turns violet on focus through the same gradient.
- **Quote actions** — two discreet buttons on blockquote hover/focus:
  **quote** copies the quotation with attribution; **link** copies a deep
  link using a text fragment (`#:~:text=…`) that highlights the passage for
  the recipient. Same pill style for the **code copy** button on source
  blocks.
- **Effects** — rain, snow, WebGL glass, gyro tilt, dynamic weather sky:
  removed from the live templates 2026-07-03. Honor
  `prefers-reduced-motion` and `prefers-contrast` in what remains.
- **Timeline — DECIDED (v4) 2026-07-04: the tag cloud is the header.**
  The cloud (same three axes as the Topics page) sits at the top of the
  timeline; clicking a topic filters the timeline in place (click again or
  `Esc` to clear). Below it, quiet controls with **no rule under them**:
  the saved-bookmarks count hangs in the *left margin*, x-aligned with the
  entries' own bookmark marks; the level control is **stepped bars + a
  segmented control** (DECIDED 2026-07-04 from the shape comparison —
  the funnel glyph, the macOS filter mark, and the plain word row were
  all passed over): a staircase of solid bars — chunky enough to read,
  not a hairline glyph — that light up to the current level, beside a
  quiet macOS-style segmented capsule for
  **everything · notable · best**; hovering the bars explains the current
  level in words. **★ favorites is an independent toggle**, not a fourth
  segment — it is a different axis, so it composes with the level ("best"
  + ★ = my favorite best things). **Search is always visible** (the
  expanding-icon version was tried and rejected same-day): magnifier and
  field on their own line, the field starting exactly under the segmented
  choices, `/` focuses. **The saved view never silently hides**: when
  level/topic/search filters exclude bookmarked entries, a note names
  them (linked) and offers a one-click "show them" that drops the
  filters. **A row's hover zone extends into the left margin** its marks
  hang in, so the bookmark appears when the pointer is anywhere that
  belongs to the entry. Rows otherwise carry over from v3.
  Reference: `static/timeline-mockup.html`;
  `static/filter-control-mockup.html` keeps the live shape comparison
  that led here (words ± icon, hairline slider, segmented, stepped bars,
  native range).
  - **Uniform rows, quality as a hairline meter** (v3, kept): every title
    the same size (v2's graded-density sizes rejected); quality is a 2px
    hairline under the date whose fill is the pairwise-grade percentile,
    with ticks at the thresholds the filter control uses.
  - **Left rail**: ISO date (`2026-07-04`), meter, then tags stacked
    vertically — the date baseline aligns with the title baseline.
  - **Hanging marks, like footnotes**: my ★ (the `favorite` tag) hangs in
    the left margin outside the column; the reader's bookmark hangs outside
    the star and shows only on hover/selection (always when set).
  - **Month headings**: sentence case ("May 2026" — the v3 all-caps mono
    label was rejected 2026-07-04), slightly larger and bold, quiet color.
  - Titles show the extension (or folder `/`) at the **same size, slightly
    greyed** — filesystem-native type indication for free.
  - **One-line description** under the title. A native macOS **Finder
    comment** (`kMDItemFinderComment` xattr, set in Get Info) wins when
    present — deliberate metadata, so it works on *any* kind (a photo, link
    or page can carry one). Otherwise an auto-excerpt: the first prose
    paragraph of a text post, markup-stripped and capped (HTML pages
    excluded so `<style>`/`<script>` can never leak). Read at the post level
    (the folder, like tags — never `.DS_Store`), escaped at render, folded
    into search. **SHIPPED 2026-07-06.**
  - The everything/notable/best control replaces `/best`; the v1
    kind-filter menu, range slider, and the always-open search box stay
    rejected as chrome.
  - Month groups, keyboard (`j`/`k`, `Enter`, `b` bookmark, `/` search)
    carry over, plus relative pairwise grading at publish time. **Keyboard
    help lives behind `?`** — a small plain dialog above its trigger —
    instead of a cluttered hint line in the footer.
- **Types vs tags — DECIDED 2026-07-04**: photo, note, page, link, folder
  are **types**, derived from the file itself (extension/content type) —
  never tags. Search matches types, so typing "photo" filters to photos
  with zero UI. Whether dedicated type filters belong somewhere is OPEN —
  with `/everything` retired, the timeline's filter icon is the natural
  door if they ever earn a place.
- **Finder tag colors — DECIDED 2026-07-04**: tags render with the color
  they carry in Finder (read from the macOS tag xattr, which stores a color
  index 0–7 per tag). The seven Finder colors are mapped to CSS custom
  properties tuned for light and dark. Tag = small colored dot + name;
  text stays ink for legibility.
- **Tag cloud — DECIDED (v2) 2026-07-04: legible, three readable axes.**
  **Size** = entry count, **ink** (weight/contrast) = recency of last
  activity, **color dot** = the tag's Finder color (dots ride a touch
  below center, on the label's optical midline). **Position — DECIDED
  2026-07-05: flat, left-aligned, alphabetical** (case-insensitive) — a
  stable, predictable order so a topic never moves or reshapes between
  visits. This supersedes the briefly-shipped center-out ordering (a cloud
  that rearranges was judged disorienting); `static/cloud-mockup.html` keeps
  the compared, rejected alternatives (by-count, center-out, a centered
  "diamond mass", a literal-3D depth version). Scatter clouds read terribly;
  legibility beats cleverness. The cloud is deliberately narrow so it wraps
  into a few lines; the block sits at the left edge of the content column,
  its lines left-aligned within it (revised 2026-07-05 from the earlier
  centered cloud shape). Since v4 it lives at the top of the timeline as the site
  header (`static/timeline-mockup.html`). **The standalone `/topics` page
  is removed** (DECIDED 2026-07-04) — the cloud on the timeline replaced
  it; its by-latest-activity list goes with it (git history is the
  archive).
  - **Selected topic wears a Finder-grey pill** (DECIDED 2026-07-04,
    after two rejected takes the same day: an underline, a soft violet
    tint, then a solid-violet two-tone capsule — like Finder, a quiet
    grey capsule won): text, dot, and count all keep their colors; only
    the background says "selected". The pill is an absolutely positioned
    `::after` behind the tag (`isolation: isolate`, negative z-index):
    out of flow, so toggling cannot shift layout by even a pixel — the
    padding/negative-margin mirror moved a rounding pixel, and box-shadow
    spread flattened the capsule's ends; this construction has neither
    flaw, with the radius rounding the pill's own box into true
    semicircular ends. The count rides a touch above the baseline (a
    visual-only nudge shared by all cloud counts). `Esc` or a second
    click clears — the margin "Esc clears" hint was tried and removed
    same-day (clutter).
- **Three separate signals — DECIDED 2026-07-04** (the hybrid `/favorites`
  is rejected: one address showing different people different content is two
  features wearing one name):
  - **★ Author favorites** — the `favorite` Finder tag, zero machinery: a
    violet ★ on the timeline rail, a topic on the Topics page, linkable as
    `/+favorite`. My taste, hand-picked.
  - **Reader bookmarks** — a bookmark icon on each row lets a reader keep
    a read-later list; `localStorage` only, never transmitted. Explicitly
    **not cookies** (DECIDED 2026-07-04): cookies ride along on every HTTP
    request, which would hand the server exactly the list it must never
    see — with localStorage the promise is structural, not policy. (Known
    trade-off: Safari purges script-writable storage after ~7 days without
    a visit; losing a read-later list is acceptable, leaking it is not.)
    The server cannot know what anyone saved and no consent banner is needed
    (ePrivacy exempts storage strictly necessary for a function the user
    explicitly requested; no tracking, no identifiers). In the nav, a
    small **bookmark icon with a count** (not a "Saved" text link) appears
    once something is saved and opens the list.
  - **Quality** — the dynamic pairwise grade; expressed as *prominence*,
    see graded density below.
- **Continue reading (replaces the footer "timeline" link) — DECIDED
  2026-07-04**: post navigation is content, not chrome. After an entry's
  footer, a quiet block teases the next (older) entry — label, date, first
  line. Activating it loads that entry inline below, so multiple posts can
  flow on one page. The URL bar follows the reading position: an
  IntersectionObserver + `history.replaceState` sets the address to whichever
  entry currently owns the most viewport (a URL can only name one entry, so
  majority-of-viewport wins; title updates with it). After the first inline
  load, an unobtrusive "keep loading as I scroll" toggle opts into infinite
  scroll (opt-in, persisted, never default). The entry footer keeps only
  quiet entry metadata: `source`, "more like this".
- **Typeface** — **OPEN**: New York (ui-serif) vs SF (system-ui) for entry
  body text; toggle in the mockup. Chrome is always system sans.
- **Page titles — DECIDED 2026-07-04: site first**: `<site name> — Topics`,
  `<site name> — <entry title>` (the name comes from `Sajt.toml`, default the
  site directory's name; since 2026-09-08). The site name is the brand, and tabs from the
  site cluster visually. Modern browsers (Safari included) deduplicate a
  repeated title prefix across same-site tabs and surface the distinct
  part, so the classic truncation argument for page-first no longer bites.
- **CSS conventions** — class-less (element selectors + structural
  combinators) in real templates; `color-scheme: light dark` with custom
  properties; plain-value fallbacks before modern functions so old browsers
  degrade to a readable page. **Class exceptions** (the only classes the
  templates use — JS state, JS-injected markup, or engine output):
  `.selected`, `.near`, `.anchor`, `.sn`, `.lit`, `.backref`, `.pill`
  (JS-driven state or JS-injected) and the engines' `.footnote-ref` /
  `.footnote-backref` / `.footnotes` / `.sourceCode` / `.markdown-alert` /
  Asciidoctor's `.admonitionblock`, `.halign-*` (renderer output).

## Smart features

Curated to fit the philosophy (native, private, cool-URIs).

Publishing hygiene (PROPOSED, accepted in spirit 2026-07-03):
- **Atom + JSON Feed** at `/feed` (+ per-tag feeds like `/+design/feed`).
- **sitemap.xml, robots.txt** — generated from public entries only.
- **OpenGraph/Twitter meta on own entries** (title from label, description
  from first paragraph, image from first image in entry/bundle).
- **301 on rename, 410 Gone on unpublish** (see Identity above).
- **404 with suggestions** — fuzzy-match against public labels.

Privacy & safety:
- **Metadata stripping on the fly** — SHIPPED, see "The file-privacy
  boundary" above.
- **Zero third-party requests** for readers, ever — embeds are already
  cached and served locally; enforce with a Content-Security-Policy header.
- **Security headers** throughout (CSP, HSTS, X-Content-Type-Options).

Reading experience:
- **Heading anchors, code copy, quote/link actions** — DECIDED, in mockup.
- **Keyboard throughout** — `j`/`k` + arrows on the timeline (already live),
  `/` for search, `Esc` clears.
- **Scroll restore** on the timeline.

Content-hash dividends and quiet touches (DECIDED 2026-07-04):
- **Content-addressed caching for free** — the SHA-256 already computed per
  entry is a perfect strong ETag; raw files get `immutable` caching and
  change URLs only when content changes.
- **Dark-variant images** — if a bundle contains `diagram.png` and
  `diagram-dark.png`, serve a `<picture>` that switches with the OS theme.
  Native-feeling, zero configuration.
- **Related entries** — a quiet "more like this" line in the entry footer,
  from shared tags. No engagement machinery, just wayfinding.
- **Updated dates** — when a file's content hash changes after first
  publish, show "updated <date>" beside the original date.
- **Print stylesheet** — sidenotes become real footnotes, nav disappears;
  a serious site prints well.

## Publishing workflows (native only)

- **iPhone**: share sheet → Save to Files → the iCloud Drive content folder →
  long-press → Tags → `public`. Published when iCloud syncs.
- **macOS**: drop the file/folder into the content folder, tag `public` in
  Finder. To upgrade a bare file to a folder post (for assets, aliases, a
  stable date, or revisions), select it and press Ctrl-Cmd-N ("New Folder with
  Selection"); drop in an empty `2026-03-03T1430/` marker folder to pin the
  publish date. The FSEvents watcher picks changes up (500 ms debounce).
- **Later**: native macOS Share Extension (planned, not started) and an iOS
  Shortcut as accelerators — never requirements.

## Platform portability (DECIDED 2026-07-16)

**AMENDED 2026-08-30 — the deployment story moved to `sajt.md`
(canonical): the site is built entirely on the Mac and shipped as static
bytes; no dynamic server runs on the public host.** The portable binary
below remains true for local preview and the optional VPS serving lane.

**The core is one portable Rust binary: macOS-native authoring, Linux-native
serving.** Authoring stays a Finder/Files.app gesture on Apple platforms;
deployment targets include ordinary Linux servers with content arriving via
`rsync -X`. No containers in the local dev loop — the server reads Finder
metadata off the live filesystem, which a Linux-guest bind mount would not
carry (virtiofs does not pass macOS xattrs through).

What portability rests on, per subsystem:

- **Tags/comments** — Linux confines unprivileged xattrs to the `user.`
  namespace, so the Apple attr names arrive mapped. Readers try, in order:
  the native name, `user.`-prefixed, and `user.rsync.%`-wrapped (rsync
  `--fake-super`) — `tags.rs` `USER_TAGS_XATTR_NAMES` /
  `FINDER_COMMENT_XATTR_NAMES`. Payload is the same binary plist everywhere.
  The exact name a given rsync version produces is unverified against a real
  Linux box; the list covers the documented forms and extends trivially.
- **Watcher** — `notify` selects FSEvents on macOS and inotify on Linux by
  target, not by feature; no configuration needed.
- **Transcode sandbox** — Seatbelt (`sandbox-exec`) on macOS, bubblewrap on
  Linux, fail-closed without either (shipped v0.21.0). The flake devshell
  carries `bubblewrap` on Linux.
- **CI** — `.github/workflows/ci.yml` runs `cargo test` in the flake
  devshell on ubuntu and macos, so the Linux path cannot silently rot. The
  gate tests write tags under whichever mapped name the platform accepts,
  so fail-closed visibility is exercised on both.

**Deliberately rejected:** running the backend as a local Linux container
behind the machine-wide shared proxy (the `local-serving` scheme). The
Finder-metadata coupling above makes that lane wrong for *this* backend;
localhost dev keeps the repo `Caddyfile` + host Caddy, which exposes nothing.
LAN/iPhone access, when wanted, is one mDNS name pointed at the host Caddy —
an explicit, reversible step.

**Deferred to ~1.0 (distribution question, not architecture):** a sandboxed
macOS app shell over the same core. App Sandbox is compatible with the design
(network-server entitlement, security-scoped bookmark for the content folder,
FSEvents and xattrs work in-sandbox), with one real design consequence:
nested `sandbox-exec` is unavailable inside App Sandbox, so the vips jail
would become an XPC service with minimal entitlements. App Store vs.
Developer ID + notarization stays open until then.

## Roadmap to publishable v0.1

1. **Publish gate** — only `public`-tagged entries are served; everything
   else 404s (410 once the was-public ledger exists). Fail-closed tests.
   (The one blocker for going live.)
2. **Folders as entries** — **DONE 2026-07-06** (`entry-model.md`): folder
   posts with clean names, publish dates from empty date markers (mtime
   fallback), primary-file resolution, and asset URLs all serve. Remaining
   polish: `.prompt` bundle generation and auto gallery/listing for a folder
   with no primary.
3. **AsciiDoc via Asciidoctor** — **DONE 2026-09-09** (v0.38.0, with the
   comrak port of Markdown and the shared syntect highlighter; see
   "Authoring formats").
4. **Port the plain design** — entry pages from `entry-page-mockup.html`
   (plain body, way back to root, sidenotes filter, anchors, reader width,
   quote/code actions, continue-reading) and the timeline from
   `timeline-mockup.html` (cloud header, filter row, bookmarks, quality
   meter, ★ favorites). Replaces the interim glass cards in templates.
5. **Metadata stripping** — before anything with photos goes public.
6. **Feeds + sitemap + OG meta + security headers**, then go live.
   (Deployment redesigned 2026-08-30: static build shipped to object
   storage/CDN, never content rsynced to a live server — `sajt.md`.)

Done 2026-07-03: effects/variants/theme-switcher pruned from templates;
unreferenced prototypes archived (git history keeps them).

Done 2026-07-05: the mockup design is ported into `src/templates.rs`. Stage 1
shipped the shared `render_site_header` (tag cloud + controls) on both the
timeline and entry pages plus the rows-glass timeline. The **entry-body port**
then shipped in full: JS-injected heading anchors (h1 chains to the entry's
canonical URL, h2s get `#` section links), a mini-TOC `<details>` when a post
has ≥ 3 `h2`s, copy pills on code blocks and quote/deep-link pills on
blockquotes, the two-mode footnote/sidenote system (right-margin `.sn` notes
when there is room, a bottom footnote list otherwise — no-JS shows the bottom
list), serif body with a persisted typeface toggle (`t` key + footer control),
land-on-post scrolling, and an inline Continue that fetches the next entry and
appends it below with a reading-line (Discourse-style) URL that reflects the
article whose top has crossed ~30% of the viewport. The entry-body selectors
were generalized from `article#post` to `main > article` so Continue-appended
posts render identically. Client storage keys route through a provisional
`NS = "site"` namespace (`site-width` / `-saved` / `-type` / `-autoload`) — the
engine is generic, so the one constant is renamed once the site is named
(pre-1.0, no migration).

Done 2026-07-06: the entry model shipped end to end (`entry-model.md` is
canonical). The scanner was rewritten around bare-file and folder posts (mtime
or empty date-marker publish dates, `alias <name>/` markers, ` copy [n]`
revisions, fail-closed `PostError` pages); routes/templates serve folder posts,
their assets, aliases (301), and dated revisions; `?level=` became `?grade=`
with the scale collapsed to a two-state everything/notable; the embed cache and
index moved out of the content tree into a `--cache-dir` (platform cache dir by
default), so the server is now strictly read-only on content; and a one-shot
`migrate` subcommand (DRY-run by default) converted the live flat
timestamp-named files into folder posts. `content/` is now folder posts served
at clean URLs. The `Do`-prefix action tags and the timestamp-filename
convention are retired.

Later: grading flow in production (author-written
`Sajt-Grade-Judgements.jsonl` + Bradley-Terry derivation), visitor
favorites + infinite-scroll continue, Share Extension, passkey auth for
`private`, expiring share links, `.prompt` generation polish, 404 suggestions,
dark-variant images, related entries, mini-TOC, print stylesheet.

## Design files

- The design mockups that lived in `static/` (timeline, entry page, listings,
  filter controls, marks, cloud, link rows) were removed on 2026-09-08; they
  are in history at commit `8bc77c8` and earlier. New mockups showing the
  settled design are planned.
- `sajt.md` — **static-publish architecture + macOS app (DESIGNED
  2026-08-30, not yet built; canonical for deployment)**: build local,
  ship bytes; closure model; manifest + adapters; the Sajt name.
- `post-model.md` — **post-model refinement spec (2026-07-07, not yet built)**:
  slugs, timestamps, kinds, link posts, revisions/families, listings, scheme
  guard, EXIF. Extends `entry-model.md`.
- `static/link-rows-mockup.html` — link-row layout reference (label→our page,
  cited source→destination ↗; bare-link, commentary, photo-credit, unsafe cases)
- `.claude-memory/design.md` — original full design (URL grammar, prompt
  files, hashing, action tags, sky variants history)
- `.claude-memory/entry-list-design.md` — timeline layout exploration,
  grading model, visibility tags
- `.claude-memory/design-inspiration.md` — external references
- `.claude-memory/glass-effects.md`, `ux-laws.md` — technique research
- `.claude-memory/snowfall-*.md`, `rain-*.md` — retired effects research
- `static/entries-site.html` — timeline interaction reference (visuals
  obsolete: predates the no-glass decision; kept for the grading dialog)
- `static/entry-page-mockup.html` — entry page reference (plain, top nav,
  sidenotes, anchors, quote/link/code actions, adjustable width, star,
  continue-reading flow)
- `static/timeline-mockup.html` — timeline reference v4.5 (tag cloud as
  header, middle-grouped, with in-place topic filtering and a Finder-grey
  pill on the selected topic; no site nav, margin-hung bookmark count,
  stepped bars + segmented control for everything/notable/best with an
  independent ★ favorites toggle, a × between the bookmark mark and its
  count, search on its own line with the field
  width-matched to the segmented control so it sits centered under the
  choices, sentence-case month headings; v3 rows kept: uniform titles,
  hairline quality meter with threshold ticks, ISO-date rail with
  vertical tags, hanging ★/bookmark marks, `?` help dialog. Marks changed:
  the bookmark is now a dim-always outline that brightens only by pointer
  proximity, in two stages: within ~50px it lifts to full opacity but
  stays grey, and landing directly on the icon turns it violet — never on
  plain row hover — so nothing appears or vanishes and it stays
  discoverable in compact mode where there is no hover margin. The
  non-favorite rows carry no empty star (an empty ☆ was tried and rejected
  as clutter — DECIDED 2026-07-04). A reading-width edge grip drives
  `--content-w` and shares the entry page's `mock-width` store, so a width
  set on either page is the site's width on both)
- `static/filter-control-mockup.html` — the live shape comparison behind
  the stepped-bars + segmented decision (words ± icon, hairline slider,
  segmented, stepped bars, native range)
- `static/marks-mockup.html` — the live comparison behind two mark
  questions, now RESOLVED (2026-07-04): saved-count separator → × chosen;
  empty ☆ on non-favorite rows → rejected as clutter (kept off)
- Topics page mockup removed 2026-07-04 (cloud lives on the timeline; git
  history is the archive)
