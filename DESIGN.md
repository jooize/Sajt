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
  real file dates. No custom apps required to publish (a Share Extension may
  come later as an accelerator).

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

**DECIDED 2026-07-03 (supersedes the brief `public/`-folder idea from earlier
the same day): one flat content folder; the `public` xattr tag is the only
publish gate.** No moving files, no special folders — tags over folders,
consistently. Files.app on iPhone tags well now, so the flow is native on
both platforms.

Why this is not a recipe for disaster: the default stays fail-closed, so
every failure mode points the safe direction. An untagged entry is invisible.
A tag lost in a bad sync *unpublishes* — annoying, never a leak. There is no
state in which something private goes live without a deliberate tagging act.
(The dangerous design would have been publish-by-default; that stays dead.)

- **`public` tag** = published for all. Anything else = not served.
- **`private` tag** = gated / access-controlled (shows as a "gated" state,
  not a topic pill) — for the future authenticated area.
- **`personal` tag** = descriptive, orthogonal to visibility (an entry can be
  personal+public).
- Unpublishing (tag removed after being live) answers **410 Gone**.

Deferred: per-link expiring capability URLs for sharing gated entries.

### Action tags (`Do` prefix)

xattr tags the server executes and removes: `DoRename` (stamp timestamp name),
`DoDate`, `DoPublish` (apply `public`), `DoGenerate` (run `.prompt`).

### Identity & index

SHA-256 content hashing for dedup, rename detection, version detection. The
server index (SQLite or JSON) additionally records **first-seen date per
entry** — required by clean-named folders/files — and **which labels were
ever public** (needed to answer 410 vs 404 honestly). The index is a
cache/ledger, never the source of truth for content.

## URL scheme

Labels are primary; dates are for timeline filtering only.

```
esko.bar/                     timeline (newest first)
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

Structure and typography carry the plainness; a restrained glass chrome
carries the cool. Heavy effects are gone.

- **Entry pages** — **DECIDED 2026-07-03: plain.** The entry body is pure
  typography on the page background — no card, no material. Reference
  realization: `static/entry-page-mockup.html`.
- **Header** — **DECIDED 2026-07-03**: one small sticky glass bar that stays
  **within the content column** (floating pill, not full-bleed), containing
  brand + timeline link. As few menus as possible; no sidebars — the side
  space belongs to sidenotes.
- **Reader-adjustable width** — **DECIDED 2026-07-03**: a discreet drag
  handle at the edge of the content column lets the reader set their own
  reading width (keyboard: arrow keys; double-click resets; persisted in
  localStorage). Sidenotes move between margin and inline automatically based
  on available space.
- **Sidenotes** — footnotes render as Tufte-style margin notes when there is
  room, inline note blocks otherwise. Implemented as a Pandoc Lua filter /
  Asciidoctor postprocess over standard footnote syntax — authors just write
  footnotes.
- **Copy as quote** — **DECIDED 2026-07-03**: blockquotes get a discreet
  copy button (visible on hover/focus) that copies the quote, attribution,
  and a deep link using a text fragment (`#:~:text=…`) so the link highlights
  the quoted passage for the recipient.
- **Effects** — **DECIDED 2026-07-03: drop rain, snow, WebGL glass, dynamic
  weather sky.** Keep one calm, battery-friendly glass treatment for chrome
  (tinted, sufficiently opaque, `saturate(180%)`, asymmetric rim highlights).
  Honor `prefers-reduced-transparency`, `prefers-reduced-motion`,
  `prefers-contrast`, and provide solid fallbacks where `backdrop-filter` is
  unsupported.
- **Timeline** — floating glass cards, month-grouped, website-first (no fake
  macOS chrome): `static/entries-site.html` is the reference realization.
  Esko violet `#a123f6` accent. Relative pairwise grading (percentile → grade
  meter, "Top" filter) set at publish time.
- **Typeface** — **OPEN**: New York (ui-serif) vs SF (system-ui) for entry
  body text; toggle in the mockup. Chrome is always system sans.
- **CSS conventions** — class-less (element selectors + structural
  combinators) in real templates; `color-scheme: light dark` with custom
  properties; exceptions only for JS state and renderer highlight tokens.

## Smart features — PROPOSED

Curated to fit the philosophy (native, private, cool-URIs). Each is small;
none blocks v0.1 except where noted in the roadmap.

Publishing hygiene:
- **Atom + JSON Feed** at `/feed` (+ per-tag feeds like `/+design/feed`) —
  a publishing site is not finished without feeds.
- **sitemap.xml, robots.txt** — generated from public entries only.
- **OpenGraph/Twitter meta on own entries** so esko.bar links unfurl nicely
  elsewhere (title from label, description from first paragraph, image from
  first image in entry/bundle).
- **301 on rename** (content-hash identity, already designed) and **410 Gone**
  for unpublished entries.
- **404 with suggestions** — fuzzy-match the requested label against public
  entries ("did you mean /open-source-licenses?").

Privacy & safety:
- **EXIF stripping on served images** — GPS coordinates and serials never
  leave the server; originals untouched on disk. (Roadmap item — this is a
  privacy requirement, not a nicety.)
- **Zero third-party requests** for readers, ever — embeds are already cached
  and served locally; keep it that way and add a CSP header that enforces it.

Reading experience:
- **Heading anchors** — discreet `#` link on hover for deep-linking sections.
- **Code copy button** — same discreet style as the quote button.
- **Keyboard throughout** — `j`/`k` + arrows on the timeline, `/` focuses
  search, `Esc` clears (mockup behavior, kept in production).
- **Scroll restore** on the timeline (return where you left off).

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
4. **Port the timeline** — `entries-site.html` → real templates (class-less).
5. **Entry page** — port `entry-page-mockup.html`: plain body, confined glass
   header, sidenotes filter, reader width, quote copy.
6. **EXIF stripping** — before anything with photos goes public.
7. **Prune** — remove the variant/effects system from `src/templates.rs`
   together with the remaining effect assets it references
   (`raindrop-fx.js`, `rain-bg.jpg`, `snow-*.js`). Standalone unreferenced
   prototypes were already archived (git history keeps them).
8. **Feeds + sitemap + OG meta**, then deploy behind Caddy; `rsync -avX`
   content up; go live.

Later: grading flow in production, Share Extension, gated-entry auth,
expiring share links, `.prompt` generation polish, 404 suggestions.

## Design files

- `.claude-memory/design.md` — original full design (URL grammar, prompt
  files, hashing, action tags, sky variants history)
- `.claude-memory/entry-list-design.md` — timeline layout finalists, grading
  model, visibility tags
- `.claude-memory/design-inspiration.md` — external references (siri4eu glass
  nav, lucas.love)
- `.claude-memory/glass-effects.md`, `ux-laws.md` — technique research
- `.claude-memory/snowfall-*.md`, `rain-*.md` — retired effects research
  (kept for reference)
- `static/entries-site.html` — timeline reference mockup
- `static/entry-page-mockup.html` — entry page reference (plain, confined
  header, sidenotes, quote copy, adjustable width)
