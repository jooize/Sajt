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
- A high-quality, serious website: plain, typography-first presentation
  built on current web platform features (`light-dark()`, text fragments,
  `:has()`), always with graceful fallbacks so old browsers get a readable
  page rather than a broken one.

## Content model

### Entries

An entry is a file **or a folder** in the single content directory.

- **Files** — as today: `sunset.md`, `IMG_4392.jpg`, `talk.pdf`, `post.link`.
  Timestamp-prefix names (`2026-03-03T143052_sunset.md`) remain supported and
  act as an explicit date override.
- **Folders as entries** — **DECIDED 2026-07-03: clean names, native dates.**
  A folder named `open-source-licenses/` *is* `esko.bar/open-source-licenses`.
  No timestamp prefix: the date comes from the folder's native creation date,
  recorded into the server's index the first time it is seen (so it survives
  later syncs that mangle birthtime). Finder/Files sort it by real date
  columns; the web sorts it by the same date. Tags on the folder apply to the
  entry. A timestamp-prefixed folder name is still honored as a date override,
  same rule as files.
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

### Action tags (`Do` prefix)

xattr tags the server executes and removes: `DoRename` (stamp timestamp name),
`DoDate`, `DoPublish` (apply `public`), `DoGenerate` (run `.prompt`).

### Identity, index, renames

SHA-256 content hashing gives every entry a durable identity independent of
its name. The server index (SQLite or JSON) records per identity: content
hash(es), **first-seen date**, and **every label the entry has ever been
public under**. The index is a cache/ledger, never the source of truth for
content.

**How renames keep URLs alive:** rename a file or folder freely in Finder —
the server sees the same content hash under a new name (and FSEvents reports
the rename directly), so it updates the current label and keeps the old one
in the ledger. Every former label answers **301 Moved Permanently** to the
current URL, forever. Edge cases: if a rename and a content edit happen in
the same sync batch, the FSEvents rename event still ties old to new; if a
new entry later claims an old label, the explicit current entry wins and the
redirect is dropped in its favor (logged loudly).

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

## URL scheme

Labels are primary; dates are for timeline filtering only.

```
esko.bar/                     timeline (newest first)
esko.bar/best                 curated: highest-graded entries
esko.bar/everything           complete compact archive
esko.bar/open-source-licenses entry (file or folder), label = URL
esko.bar/open-source-licenses/report.pdf   asset inside a bundle
esko.bar/2026-03/             listing: March 2026
esko.bar/+design+rust         tag filter (AND); comma = OR
esko.bar/sunset.md            raw source (extension = raw)
```

Old date+label URLs 301-redirect to label-only.

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

**DECIDED 2026-07-03: no glass, anywhere.** Plain, typography-first, serious.
Structure carries the beauty. Reference realization:
`static/entry-page-mockup.html`.

- **Entry pages** — pure typography on the page background. No cards, no
  materials, no backdrop filters.
- **Site nav** — a static row of plain text links at the top of the content
  column (not full-bleed, not floating, not sticky): **Timeline · Best ·
  Everything**. No site title — the domain is the brand. Nothing else.
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
  thing that lives there, mirroring sidenotes on the right. (A collapsed
  "on this page" mini-TOC for very long entries is a possible later
  exception — PROPOSED, not now.)
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
- **Timeline** — visual design is **OPEN again**: the floating-glass-cards
  realization (`static/entries-site.html`) is visually obsolete now that
  glass is dropped, but its *interaction model* stands: month grouping,
  kind filter, sort, search (`/`), keyboard nav (`j`/`k`), relative pairwise
  grading at publish time (percentile → grade meter, feeding `/best`).
  Needs a plain re-realization in the mockup's language.
- **Typeface** — **OPEN**: New York (ui-serif) vs SF (system-ui) for entry
  body text; toggle in the mockup. Chrome is always system sans.
- **CSS conventions** — class-less (element selectors + structural
  combinators) in real templates; `color-scheme: light dark` with custom
  properties; plain-value fallbacks before modern functions so old browsers
  degrade to a readable page.

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

More ideas (PROPOSED):
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
  Finder. `DoRename` tag if a stamped filename is wanted. FSEvents watcher
  picks it up (500 ms debounce).
- **Later**: native macOS Share Extension (planned, not started) and an iOS
  Shortcut as accelerators — never requirements.

## Roadmap to publishable v0.1

1. **Publish gate** — only `public`-tagged entries are served; everything
   else 404s (410 once the was-public ledger exists). Fail-closed tests.
   (The one blocker for going live.)
2. **Folders as entries** — clean names, first-seen dates in index, bundle
   `index.*` rendering, asset URLs.
3. **AsciiDoc via Asciidoctor** — fix the broken `.adoc` path; unified
   highlight theming; `video::` works.
4. **Port the plain design** — entry pages from `entry-page-mockup.html`
   (plain body, top nav, sidenotes filter, anchors, reader width, quote/code
   actions) and a plain timeline (interaction model from `entries-site.html`,
   new visual language). Replaces the interim glass cards in templates.
5. **Metadata stripping** — before anything with photos goes public.
6. **Feeds + sitemap + OG meta + security headers**, then deploy behind
   Caddy; `rsync -avX` content up; go live.

Done 2026-07-03: effects/variants/theme-switcher pruned from templates;
unreferenced prototypes archived (git history keeps them).

Later: grading flow in production, `/best` page, Share Extension, passkey
auth for `private`, expiring share links, `.prompt` generation polish,
404 suggestions, dark-variant images, related entries.

## Design files

- `.claude-memory/design.md` — original full design (URL grammar, prompt
  files, hashing, action tags, sky variants history)
- `.claude-memory/entry-list-design.md` — timeline layout exploration,
  grading model, visibility tags
- `.claude-memory/design-inspiration.md` — external references
- `.claude-memory/glass-effects.md`, `ux-laws.md` — technique research
- `.claude-memory/snowfall-*.md`, `rain-*.md` — retired effects research
- `static/entries-site.html` — timeline interaction reference (visuals
  obsolete: predates the no-glass decision)
- `static/entry-page-mockup.html` — entry page reference (plain, top nav,
  sidenotes, anchors, quote/link/code actions, adjustable width)
