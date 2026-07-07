# esko.bar — Design

The single summary of the site's design. Detail files live in `.claude-memory/`
and are referenced at the bottom. Items are marked **DECIDED** (with date),
**PROPOSED** (Claude's recommendation, awaiting Tilde), or **OPEN**.

## Philosophy

The filesystem is the CMS. One folder, synced through iCloud Drive, is the
whole publishing pipeline. No databases as source of truth, no frontmatter,
no build step. Metadata is native metadata — filenames, folder names,
creation dates, Finder/Files tags. Tags over folders everywhere: folders
exist only where they *are* the content (bundles).

Tagline: **"Tag it `public` — it's published."**

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
scheme guard, and default EXIF stripping with an `original` opt-in.
(`/hello-world`). This supersedes the timestamp-prefix filename convention,
birthtime dates, and the `.id`/UUID identity plan:

- **Files stay first-class**: a bare `sunset.md` is a complete post. Its
  **publish date is its mtime** (the one timestamp every sync tool preserves,
  settable on every OS — birthtime/Date Added are not portable to the Linux
  host). Trade: editing a bare file republishes it; fold it into a folder when
  the date must outlive edits. The extension gives the timeline its type
  indication for free.
- **Folders as entries** — `open-source-licenses/` *is*
  `esko.bar/open-source-licenses`. Publish date = an **empty date-named
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
  addressable as `esko.bar/<name>/<asset>`.

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
- Unpublishing (tag removed after being live) answers **410 Gone**.

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
- **One flat namespace; the oldest claim wins the bare URL.** When a name is
  claimed by more than one post/file/alias, the oldest claim keeps `/name` —
  an established URL never changes meaning (cool URIs), so a newly-dropped
  `IMG_4392` can never silently retarget an old link. The other claimants stay
  reachable at their date paths, the winning page carries a visible "this name
  is also used by …" notice, and the collision is logged loudly. Handing a name
  over is deliberate: delete the old claim.
- **Revisions** are Finder's own ` copy [n]` suffix. The unsuffixed post is
  always current (keeps its URL, tags, and timeline slot); `label copy/`
  folders (or ` copy` files inside the post) are archived snapshots, dated by
  their own mtime, reachable from the entry's revision nav and their date-path
  URLs. Deleting a copy never breaks the post.
- The **only** fail-closed *errors* are intra-post ambiguity (multiple primary
  candidates, multiple date markers) — genuinely no-right-answer cases, shown
  as an errored row and an HTTP 500 page naming the conflict.

The server's **index and caches are disposable and live outside the content
tree** (see cache location below); the content folder is the source of truth,
opened read-only. The index still records which labels were once public so
unpublishing can answer **410 Gone** vs **404** after deletion.

### Media privacy — metadata stripping

**DECIDED 2026-07-03: strip on the fly, never touch originals.** Served
images are cleaned at request time and the cleaned bytes are cached (keyed by
content hash, alongside the existing embed cache); the files on disk keep
their EXIF forever. What gets removed: EXIF (GPS, serial numbers, owner
name), IPTC, XMP, thumbnails in metadata, for JPEG/PNG/WebP/AVIF/HEIC; for
video, QuickTime/MP4 location atoms (`com.apple.quicktime.location.*`).
Color profiles and orientation are preserved (orientation is applied or kept,
never lost). **Fail closed: a format the stripper cannot confidently clean is
not served raw** — it renders through the viewer or is refused, never leaked
with metadata intact.

**Architectural guarantee (DECIDED 2026-07-04): the unstripped original is
unreachable by construction.** The HTTP layer for media has exactly one byte
source — the cleaned cache. There is no code path from a request to an
original file handle; a bug or human error in a handler can therefore serve
the wrong *cleaned* bytes at worst, never raw ones. Cache miss = strip first,
then serve from cache; strip failure = no cache entry = nothing to serve.
Enforced with a test that greps/route-audits the media handlers for direct
content-dir reads.

## URL scheme

Labels are primary; dates are for timeline filtering only.

```
esko.bar/                     timeline (newest first), tag cloud as header
esko.bar/saved                the reader's bookmarked entries (client-side)
esko.bar/+favorite            my hand-picked favorites (a plain Finder tag)
esko.bar/open-source-licenses entry (file or folder), label = URL
esko.bar/open-source-licenses/report.pdf   asset inside a bundle
esko.bar/2026-03/             listing: March 2026
esko.bar/+design+rust         tag filter (AND); comma = OR
esko.bar/sunset.md            raw source (extension = raw)
```

Old date+label URLs 301-redirect to label-only. `/best` and `/everything`
are retired (DECIDED 2026-07-04): the timeline with its filter row
(everything · notable · best · ★ favorites) covers both — see Presentation.

**No more untitled entries — SHIPPED 2026-07-06.** Under the old convention a
filename that was only a timestamp minted a bare-timestamp canonical URL that
collided with the date-filter route (the entry rendered as a timeline, so
Continue couldn't inline-load it). In the folder model a post's name is its
folder or file name, so there is no nameless entry: the one-shot migration
folded any unlabeled file into an `untitled/` folder post (label `untitled`),
and `parse_filename` — which used to mint `None` labels — is gone.

**View filters — DECIDED 2026-07-05.** Topics stay path-based (`/+design`,
`/+design+rust` AND, `/+design,rust` OR): permanent, linkable cool-URIs. The
transient view state rides in composable query params — `?grade=notable`
(**SHIPPED 2026-07-06**: renamed from `?level=`, with the scale collapsed to a
two-state everything/notable — the `best` segment and its threshold are gone;
with no grading ledger present the notable bucket is simply empty), `?fav`, and search on
the universal `?q=…` — layering onto any path (`/+design?grade=notable&fav`). Every header control is a real `<a>` / GET-form
(no JS required), so the address bar always reflects the current view and
right-click → Copy Link shares the exact filtered timeline; no separate "link
these filters" affordance is needed. `/saved` is the reader's bookmarks, a
client-side view (the server renders the full timeline, the browser filters to
what it has in `localStorage`). Tag order in a multi-tag path is left
as-composed for now (a sorted `rel="canonical"` to fold `+a+b`/`+b+a` is a
later SEO nicety).

## Authoring formats

- **CommonMark** (`.md`) — Pandoc `commonmark_x`, as today. Bare URLs on their
  own line expand to rich embed cards via the existing embed system (YouTube,
  Bluesky, Mastodon, App Store, generic OpenGraph…). That *is* the Markdown
  equivalent of `video::…[youtube]` — paste the URL, get the player/card.
- **AsciiDoc** (`.adoc`) — **DECIDED 2026-07-03: render with Asciidoctor**,
  not Pandoc. Pandoc has **no AsciiDoc reader** (verified: absent from
  `pandoc --list-input-formats`; the current `"adoc" => "asciidoc"` mapping in
  `src/render.rs` errors at runtime). Asciidoctor natively supports
  `video::RvRhUHTV_8k[youtube]`, `video::file.mp4[width=640,start=60,opts=autoplay]`,
  admonitions, includes, and source highlighting. Ship it in the Nix dev shell
  alongside Pandoc; invoke like Pandoc is invoked today (sandboxed, semaphore,
  timeout, `--safe-mode=secure`).
- **Syntax highlighting** — one visual system across both engines: Pandoc's
  kate/breezedark token classes are already themed with CSS custom properties;
  map Asciidoctor's Rouge/Pygments classes onto the same custom properties so
  code looks identical regardless of source format.
- Everything else as today: `.rst`/`.org`/`.tex` via Pandoc, `.html`
  passthrough, `.txt` in `<pre>`, `.link` via embeds, images in viewer,
  `.prompt` via Claude API, other files as downloads.

## Presentation

**DECIDED 2026-07-05: the "rows" glass variant** (supersedes the 2026-07-03
"no glass, anywhere"). After comparing plain / rows / full in
`static/timeline-glass-mockup.html`, Tilde chose *rows*: on the timeline every
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
  footnote syntax; a Pandoc Lua filter / Asciidoctor postprocess does the
  rest.
- **Heading anchors (left)** — a `#` appears in the left margin on
  hover/focus of a heading, linking to it. **DECIDED (direction)
  2026-07-03: the left margin stays otherwise empty** — anchors are the only
  thing that lives there, mirroring sidenotes on the right.
- **Mini-TOC — DECIDED 2026-07-04**: the one exception to the empty left
  margin. Very long entries only (threshold: several `h2`s / long reading
  time) get a collapsed "On this page" disclosure; short entries never show
  it. Collapsed by default, plain text links, no scroll-spy machinery.
- **Reader-adjustable width** — a discreet drag handle at the edge of the
  content column (keyboard: arrows; double-click resets; persisted).
  Sidenotes move between margin and inline automatically.
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
- **Page titles — DECIDED 2026-07-04: site first**: `esko.bar — Topics`,
  `esko.bar — <entry title>`. The domain is the brand, and tabs from the
  site cluster visually. Modern browsers (Safari included) deduplicate a
  repeated title prefix across same-site tabs and surface the distinct
  part, so the classic truncation argument for page-first no longer bites.
- **CSS conventions** — class-less (element selectors + structural
  combinators) in real templates; `color-scheme: light dark` with custom
  properties; plain-value fallbacks before modern functions so old browsers
  degrade to a readable page. **Class exceptions** (the only classes the
  templates use — JS state, JS-injected markup, or pandoc output):
  `.selected`, `.near`, `.anchor`, `.sn`, `.lit`, `.backref`, `.pill`
  (JS-driven state or JS-injected) and pandoc's `.footnote-ref` /
  `.footnote-back` / `.sourceCode` / `.footnotes` (renderer output).

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
- **Metadata stripping on the fly** — DECIDED, see Media privacy above.
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

## Roadmap to publishable v0.1

1. **Publish gate** — only `public`-tagged entries are served; everything
   else 404s (410 once the was-public ledger exists). Fail-closed tests.
   (The one blocker for going live.)
2. **Folders as entries** — **DONE 2026-07-06** (`entry-model.md`): folder
   posts with clean names, publish dates from empty date markers (mtime
   fallback), primary-file resolution, and asset URLs all serve. Remaining
   polish: `.prompt` bundle generation and auto gallery/listing for a folder
   with no primary.
3. **AsciiDoc via Asciidoctor** — fix the broken `.adoc` path; unified
   highlight theming; `video::` works.
4. **Port the plain design** — entry pages from `entry-page-mockup.html`
   (plain body, way back to root, sidenotes filter, anchors, reader width,
   quote/code actions, continue-reading) and the timeline from
   `timeline-mockup.html` (cloud header, filter row, bookmarks, quality
   meter, ★ favorites). Replaces the interim glass cards in templates.
5. **Metadata stripping** — before anything with photos goes public.
6. **Feeds + sitemap + OG meta + security headers**, then deploy behind
   Caddy; `rsync -avX` content up; go live.

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
`.esko.bar-grade-judgements.jsonl` + Bradley-Terry derivation), visitor
favorites + infinite-scroll continue, Share Extension, passkey auth for
`private`, expiring share links, `.prompt` generation polish, 404 suggestions,
dark-variant images, related entries, mini-TOC, print stylesheet.

## Design files

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
