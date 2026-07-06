# Plan: port the mockup design into production (templates.rs)

Written 2026-07-04 after a mockup session. The mockups are now the design
source of truth; this plan turns them into served pages. Everything below
was agreed with Tilde unless marked "decide".

## Sources of truth (read these first)

- `static/timeline-mockup.html` — the timeline (site root), v4.8 + deep links
- `static/entry-page-mockup.html` — the entry page, incl. shared header
- `static/timeline-glass-mockup.html` — glass flair variants (plain/rows/full),
  **decision pending**: Tilde is comparing; DESIGN.md currently says fully
  plain. Do NOT ship glass unless Tilde has picked a variant. If a glass
  variant wins, update DESIGN.md.
- `static/filter-control-mockup.html`, `static/marks-mockup.html` — reference
- `DESIGN.md` — canonical decisions; update it as part of this work

## Core directive

**The header is one shared component.** Tilde: "we should reuse the header
code in production." One Rust function renders the site header (tag cloud,
saved count, level control, favorites, search) for BOTH the timeline and
entry pages. No duplicated markup.

## What the design is

### Shared header (on every page)
- Tag cloud IS the site header/menu. Size = entry count, ink = recency,
  dot = Finder tag color. Built from real tag stats server-side.
- Below it: saved-bookmark count (hangs left, x-aligned with entry marks),
  stepped-bars icon + segmented level control (everything/notable/best),
  ★ favorites toggle (independent axis, composes with level), search on its
  own line, its width JS-matched to the segmented control.
- **One click does the thing.** From an entry page, every header control
  navigates to the timeline with the filter already applied. Mockups fake
  this with hashes (`#/+tag`, `#saved`, `#level-N`, `#fav`, `#q=…`,
  combinable with `&`; parser at the bottom of timeline-mockup.html's
  script). Production should use real URLs — decide the scheme, e.g.
  `/+design`, `/?level=notable&fav`, `/?q=…` — server-rendered so the
  timeline arrives pre-filtered. Never make the user click twice.

### Entry page
- Header above the post, no hairline between them. On load, the page lands
  scrolled to the post (instant, in JS before paint if possible); scrolling
  up reveals the menu. Tilde loves this dynamic.
- Crumbs at top-left of the post, visible without scrolling:
  `← Timeline` (link) and `↑ Menu` (button, scrolls to top INSTANTLY —
  Tilde: "no slow smooth scrolling!!").
  - Deferred idea (Tilde, unsure): also show the previous post there like
    the Continue block at the bottom — "might get busy". Don't build.
- h1 gets a chain-icon anchor (svg, .58em) in the left margin linking to
  the entry's canonical URL — no `#` fragment. h2s keep `#` section links.
- All heading anchors behave like the timeline bookmark: resting opacity
  .32, `.near` (pointer within ~50px, rAF-throttled) lifts to full grey,
  hover on the anchor itself turns violet. Nothing appears/vanishes.
- Sidenotes: right margin when `(innerWidth - contentWidth)/2 >= 330px`
  (`data-sn="margin"`), otherwise a real footnote list at the bottom of the
  post (`data-sn="bottom"`, per-article `#footnotes` section built from the
  `.sn` spans). References are links both ways: margin mode glows the note
  (`.lit`), bottom mode scrolls to the footnote and flashes it; footnotes
  carry `↩` backlinks. URL stays clean (preventDefault).
- Entry header: mono date, hairline quality meter with notable/best ticks,
  Finder-color dot tags (link to filtered timeline).
- Continue block loads next entry inline; URL follows viewport majority;
  infinite scroll opt-in. Typeface toggle t (serif/sans) is mockup-only —
  decide whether it ships.

### Timeline
- Month headings, rows with left rail (mono date, quality meter, vertical
  Finder-color tags), hanging ★ (author favorite) and bookmark (reader,
  localStorage only) with two-stage proximity glow, keyboard j/k/Enter/b//
  /Esc/?, help dialog, saved view with "hidden by filters → show them".

### Both pages
- Reading-width edge grip, shared width store, so one width rules the site.
  Live px/rem/ch readout while dragging or keyboard-focused.
- `light-dark()` theming, OS-follows dark mode, esko violet #a123f6.
- CSS convention: element selectors, no classes — current exceptions:
  `.selected`, `.sn`, `.near`, `.lit`, `.backref` (JS state / pandoc-ish
  output). Document the exception list in DESIGN.md.

## Server-side data needed

1. Tag stats for the cloud: per tag — count, last-active date, Finder color
   (from macOS xattr; map to the light-dark tuned palette in the mockups).
2. Grade `q` per entry (pairwise placement; store as percentile/float).
   Meter width = q, ticks at the 50% / 78% thresholds (tier ≥.5 notable,
   ≥.78 best) — keep thresholds in one place, they also drive the level
   filter.
3. `favorite` tag → ★.
4. Kind/type from the file itself (note/photo/page/link/folder), searchable.
5. Month grouping, newest first.

## Client-side (privacy)

Bookmarks, search text, reading width live in localStorage ONLY — nothing
leaves the browser (help dialog says so). Rename mockup keys `mock-width`,
`mock-saved`, `mock-type`, `mock-autoload` to real names once, before
launch (no compat needed pre-v1.0.0).

**DONE 2026-07-05: keys route through a provisional `NS = "site"` constant**,
so they are `site-width`, `site-saved`, `site-type`, `site-autoload` (NOT the
`esko-*` this doc's older prose below says — the engine is generic/unbranded;
rename the one constant once the site is named, pre-1.0, no migration).

## Suggested order (one commit each)

1. **DONE 2026-07-05.** Extract a shared header component in templates.rs;
   render on timeline + entry pages with real tag stats. Decide + implement
   filter URL scheme. Because the whole design system is one stylesheet, this
   pass also threw out the old glass-card CSS and ported the timeline visuals
   (foundation + cloud header + rows-glass rows) and put the shared header on
   the entry page. What landed:
   - `src/tags.rs` keeps the Finder color index (0-7) per tag; `src/stats.rs`
     is new — tag-cloud stats (count/recency/color), the grade thresholds
     (`NOTABLE`/`BEST`, single source of truth), and the `ViewFilter`
     (level/fav/q). `Entry` gained `grade: Option<f32>` (always `None` — no
     grading flow yet) and `tags: Vec<Tag>`, plus `is_favorite`/`kind`/
     `topical_tags` helpers.
   - `templates.rs` rewritten: shared `render_site_header`, the ported
     rows-glass timeline, plain entry post + crumbs, one client `JS` (grip,
     help, bookmarks, saved view, keyboard j/k/Enter/b//, proximity glow,
     land-on-post). `routes.rs` parses `?level/&fav/&q`, applies the view
     filter server-side, adds `/saved`.
   - URL scheme (decided): tags path-based `/+tag`; level/fav/q as composable
     query params; controls are real links so the URL bar is the shareable
     filter link. See DESIGN.md.
   - Deferred out of this pass (kept for later stages): in-place client
     re-render so level/tag/search don't reload (a JSON data island seeding
     the mockup's `paint()`); the full entry-body port (sidenotes, heading
     anchors, continue-reading — stages 3-4); the interim embed-CSS token
     bridge in templates.rs (compat `--color-*` aliases) until the entry body
     is ported. **Grades don't exist yet**, so the quality meter never draws
     and `notable`/`best` filter to empty until the grading flow lands.
2. Timeline page: rows/rail/marks/keyboard/help/saved from the mockup.
   *(Visual rows/rail/marks + keyboard/help/saved shipped in stage 1; what
   remains here is the no-reload in-place filtering via a JSON data island,
   and the saved-view "hidden by filters → show them" note it enables.)*
3. **DONE 2026-07-05.** Entry page: land-on-post + crumbs + anchors + proximity JS.
4. **DONE 2026-07-05.** Sidenotes/footnotes two-mode system (pandoc integration: emit `.sn`
   spans + build footnote list; or emit both server-side and hide one).
   *(Stages 3–4 shipped together as the entry-body port — see section A below.
   Continue is a FULL inline-load, not just a plain link; the URL-follow is
   reading-line / Discourse-style — the address reflects whichever article's
   top has crossed ~30% of the viewport, stepping through every entry including
   short link cards.)*
5. Width grip + readout, shared store, both pages.
6. Glass variant IF decided; update DESIGN.md either way.
7. Prune interim glass card styles from templates.rs (DESIGN.md notes they
   linger from the 2026-07-03 pruning).

Run: `nix develop --command cargo run` + `caddy start --config Caddyfile`,
then https://localhost. Restart server after changes (Tilde watches live).

## 2026-07-05 handoff: decisions made, implementation plan (for Opus)

Fable made the design calls below and started the code; Opus finishes it.
**The tree is MID-REFACTOR and does NOT compile** until item 1 is done —
`routes.rs` already calls the new `entry_page`/`image_page` signatures.
Verify with `nix develop --command cargo test` and by running the server.

### Decisions (made this session — implement as stated)

- **URL collisions / revisions.** One canonical address per entry:
  unique label → `/label`; duplicated label → the NEWEST entry owns bare
  `/label`, older ones get the shortest disambiguating date prefix —
  `/2026-03-12/label` when the day suffices, full `/2026-03-12T133513/label`
  only as a last resort. Single-segment ISO dates (NOT `/2026/03/12/`: one
  segment matches the existing date-filter scheme). Duplicates ARE served
  (a folder and a file may legitimately share a name; an old version stays
  readable — that's the revisions story: re-drop a file with the same label,
  the new one takes `/label`, the old one keeps its dated URL). Every
  non-canonical URL that resolves to one entry 301s to the canonical,
  preserving the query string. Bare `/label` matching several entries serves
  the newest (tie → listing). macOS Versions is NOT used (see notes at end).
- **`?fav=1` → bare `?favorites`.** Presence-based flag. Links emit
  `?favorites` with no value; the search form's hidden field emits
  `favorites=` (forms can't do valueless keys) — parser accepts both,
  rejects `0`/`false`.
- **`?level=` → `?grade=notable|best`.** It's the author's grading, so the
  URL says so: `/?grade=notable&favorites&q=…`. Internal field stays
  `ViewFilter.level` (it's a floor over grades); `level_word()` renamed
  `grade_word()`; `data-level` attr → `data-grade`.
- **Tag cloud: center-out.** ~~Sort tags by count desc (ties alphabetical),
  then alternate push-back/push-front so the largest lands mid-sequence and
  sizes fall off toward both edges. Presentation-only, in `render_cloud`.
  (Trade-off accepted: loses alphabetical scanning.)~~ **SUPERSEDED
  2026-07-05: reverted to flat, left-aligned, stable alphabetical**
  (case-insensitive). Center-out was shipped (commit 6664175) then judged
  disorienting — a cloud that rearranges/reshapes between visits loses the
  predictability that lets you find a topic by muscle memory. Size curve
  unchanged (`0.82 + share*0.43`). `static/cloud-mockup.html` keeps the
  compared, rejected alternatives (by-count, center-out, a centered "diamond
  mass", a literal-3D depth version).
- **Grip: split handle.** The single 2.4rem bar reads as a scrollbar. Make
  it two short bars with a gap (::before + ::after, 4px × ~1.05rem,
  ~.3rem gap, same radius/colors/hover-to-violet). No exact native macOS
  element exists for a vertical edge-drag; the two-bar split reads
  "handle", like the iPadOS split-view divider pill family.
- **Short posts.** Before the land-on-post jump, JS pads `main`'s
  min-height so `scrollTo(landingTop)` can actually land (header hidden
  above the fold even when the post is short). Recompute on resize with
  min-height cleared first, then re-measure.
- **Bookmark/star alignment.** `main section article aside > button svg`
  gets `translate: 0 -1px` (Tilde judged the bookmark 1–2px low vs the ★).
- **Bookmark keys.** localStorage key = canonical path sans leading slash
  (labels alone collide for duplicates). Pre-1.0, no migration.
- **Entry body typography: serif + toggle (Tilde 2026-07-05).** Post
  section defaults to serif — `--serif: ui-serif, "New York", Georgia,
  "Times New Roman", serif`, 1.0625rem/1.72; chrome stays sans. The
  typeface toggle SHIPS: `t` key + a small serif/sans control in the page
  footer (next to "? shortcuts"), entry pages only, persisted in
  localStorage as `esko-type`, applied as `html[data-type="sans"]`
  (`article#post > section` swaps to `var(--sans)`, letter-spacing .001em,
  per the mockup). Default serif; JS-applied (brief flash for sans users
  is accepted). Heading sizes per entry-page-mockup.html.
- **Continue block: FULL inline-load (Tilde 2026-07-05).** Port the
  mockup's behavior for real: the server renders
  `<nav id="continue"><p>Continue</p><a …><b>title</b><time>date</time></a></nav>`
  for the next-older entry (`next` is already computed in `serve_entry`).
  No new endpoint: activating Continue fetches the next entry's canonical
  page with `fetch()`, parses it with `DOMParser`, and appends its
  `article#post` (strip the duplicate `id`, keep `data-canonical` +
  `data-title` from `<title>` or the header) before `#continue`; the
  fetched page's own `#continue` supplies the following teaser, so the
  chain continues naturally. Re-run the per-post enhancements on each
  appended article — factor them as `enhancePost(article)` (anchors,
  proximity targets, footnotes, copy buttons, blockquote menu, TOC).
  URL follows viewport majority (IntersectionObserver over
  `main > article`s, `history.replaceState` to the owning entry's
  canonical path + `document.title` swap). Autoload is opt-in: the
  "keep loading as I scroll" checkbox appears after the first manual
  load, persisted as `esko-autoload`, IntersectionObserver with 200px
  rootMargin on `#continue`. Without JS, Continue is a plain link — that
  must keep working. Sidenote margin-vs-bottom mode stays global
  (`html[data-sn]`).

### Already implemented this session (uncommitted; keep)

- `src/stats.rs` — `from_params` documents `grade`; `grade_word()`.
- `src/routes.rs` — `view_from_query` reads `grade`/`favorites`;
  `catch_all` does the generalized canonical 301 (via
  `templates::canonical_path` + `encode_path`, query preserved) and
  newest-wins (`newest_of`); `serve_entry(entry, store)` computes
  `next: Option<&Entry>` (max timestamp < entry's) and calls
  `templates::entry_page(entry, html, &all, next)` /
  `image_page(entry, mime, &all, next)` — **new signatures, templates side
  not yet written**; `serve_raw_file` newest-wins on ties; `redirect()`
  fails closed on bad header values.
- `src/templates.rs` — `query_string` emits `grade=`/bare `favorites`;
  hidden form fields renamed; `data-grade`; new `canonical_path`,
  `canonical_raw_path`, `encode_path` (per-segment percent-encoding,
  decoded-vs-decoded comparison in routes, encoded on emission),
  `bookmark_key(&canonical)`; `render_row(entry, all_entries)` uses them;
  `timeline_page` no longer builds `label_counts` (old helpers
  `entry_href`/`entry_raw_href`/`label_counts`/`is_label_unique` deleted).

### Remaining work (in commit-sized steps)

1. **Fix the build: port `entry_page`/`image_page`** in templates.rs to
   `(entry, rendered_html | mime, all_entries: &[&Entry], next: Option<&Entry>)`.
   Inside: `let canonical = canonical_path(entry, all_entries)`; raw/source
   link = `encode_path(&canonical_raw_path(...))`; add
   `data-canonical="{encoded}"` on `<article id="post">` (the JS h1 chain
   anchor reads it); render the quality meter `<span class="meter"…>` in the
   post header when `entry.grade` is `Some` (same markup/percentile title as
   `render_row`; today always `None`, so it just must not panic); append the
   Continue nav when `next` is `Some` (title = display_label|label|
   "(untitled)", href = encoded canonical of `next`). `cargo test` green;
   run server, click around, check a duplicate-label pair if present.
2. **Cloud center-out** in `render_cloud`: collect `Vec<&TagStat>`, sort
   count desc/name asc, alternate `push_back`/`push_front` into a
   `VecDeque`, iterate that instead of `ctx.cloud.tags`.
3. **Small CSS fixes**: grip split (replace `#grip::before` block: grid →
   flex column, gap .3rem; two bars via ::before AND ::after; hover/active/
   focus-visible turns both violet and grows to ~1.35rem); bookmark svg
   `translate: 0 -1px`.
4. **Entry-body port** (the big one; port from entry-page-mockup.html,
   adapting mockup classes to pandoc's real output):
   - CSS: `--serif` token; `article#post > section` serif type; mockup h1
     (clamp size, balance) / h2 (1.32rem) / blockquote (violet bar, italic,
     hover menu) / `pre > button` copy pill / `#toc` details / `#continue` /
     `.sn` + footnotes styles. Pandoc emits
     `<section id="footnotes" class="footnotes …">` and
     `<a class="footnote-ref"><sup>N</sup></a>` refs — style
     `article#post .footnotes` (hide its `<hr>`, add a small-caps
     "Footnotes" heading via `::before`), `.lit` glow uses `--violet-soft`.
     Anchors: `.anchor` class (JS-injected — do NOT use `:last-child` like
     the mockup; a content link at a heading's end would false-match
     without JS), absolute at `left: -1.7rem`, opacity .32 base, `.near` /
     `:focus-visible` → 1, hover violet; h1 chain uses the mockup's svg at
     .58em; compact media query makes them static inline. Meter: factor the
     timeline's `span.meter` rules to also cover the post header (post
     header becomes a flex row like the mockup: time · meter · tags).
   - JS (extend the entry-page block): ensure h2 ids (slugify, uniquify —
     pandoc usually provides them); append `.anchor` links to h1 (chain →
     `data-canonical`) and h2s (`#` → `#id`) using createElement (labels can
     contain quotes — no innerHTML for attribute values); generalize the
     proximity-glow block to run on BOTH pages (timeline: `main article
     aside > button`; entry: `#post .anchor`); two-mode footnotes: if
     `#post section .footnotes` exists, build `.sn` margin spans cloned
     from each `li` (strip the `.footnote-back`), insert after each
     `a.footnote-ref`, set `html[data-sn]` from
     `(innerWidth - mainWidth)/2 >= 330 ? "margin" : "bottom"` inside
     `applyWidth`; ref clicks: margin mode glows the span (`.lit`,
     1.6s timeout), bottom mode scrollIntoView + glow, backrefs return to
     the ref — all preventDefault so the URL stays clean; no-JS must stay
     correct (footnotes section visible by default; `html[data-sn="margin"]`
     is what hides it); copy buttons on `pre > code` (append, flash
     "copied ✓"); blockquote quote/link menu (text-fragment deep link);
     mini-TOC `<details id="toc">` after the first h1 when the post has
     ≥3 h2s; land-on-post min-height pad (decision above).
5. **Docs**: DESIGN.md — URL scheme section (canonical/collision/revision
   rules, `?grade`/`?favorites`), cloud ordering, grip, entry-body port
   status, CSS class exception list gains `.anchor`, `.sn`, `.lit`,
   `.backref` (JS-injected/pandoc); PLAN.md — mark stages 3–4 done, note
   Continue is a plain link, stage 2 (data-island no-reload filtering)
   still open. Update .claude-memory (MEMORY.md bullets: URL scheme grade/
   favorites wording, collisions rule, serif body).
6. Grep for stragglers: `level=`, `fav=`, `data-level`, `entry_href`,
   `label_unique` — should be zero outside docs history.

Suggested commits: (1) build fix + canonical URLs, (2) cloud ordering,
(3) grip + bookmark nudge, (4) entry-body port, (5) docs.

### Direction (Tilde 2026-07-05): timestamp filename prefixes are going away

Clean names are the plan (per entry-page-mockup prose: folder/file name IS
the address, dates from filesystem creation time, remembered by the index
so syncs can't lose them). Fable's opinion: right call, with one structural
consequence — a directory can't hold two files with the same name, so the
FILESYSTEM stops being the version store and the SERVER must take over:

- Index persists first-seen timestamp + content hash per name (mostly
  exists already).
- On rescan, same name + changed hash = new version: snapshot the old
  content into a server-side content-addressed archive (hash-named — dedup
  is free), then serve the new.
- The canonical URL scheme implemented above survives unchanged: newest
  owns `/label`, older versions live at `/2026-03-12/label` (day) or full
  timestamp; only the SOURCE of old bytes moves from "second file on disk"
  to "server archive". Folder-vs-file same-name collisions remain real and
  keep the same disambiguation.
- Privacy fails closed: archived versions are served only while the entry
  is public; unpublish hides all versions. Never archive entries that were
  never public.
- **Untitled entries are dropped (DECIDED 2026-07-05).** A filename that is
  only a timestamp (no label) currently mints a bare-timestamp canonical URL
  (`/2026-03-04T091500`) that collides with the date-filter route — it renders
  a timeline instead of the entry, so Continue can't inline-load it. Since the
  filename becomes the name, every entry has a label: this migration stops
  `parse_filename` minting `None` labels, dropping untitled entries entirely.
- NOT this session's work — do it together with the filename-convention
  migration (one-shot conversion, pre-v1.0.0). macOS Versions still ruled
  out (below).

### Why not macOS Versions (Tilde's question)

The Versions system (NSDocument's version browser) stores revisions in a
hidden per-volume database (`/.DocumentRevisions-V100`), owned by whichever
app saved the document. It is not enumerable from other processes via any
public API, does not travel through iCloud Drive/rsync, and can be pruned
by the OS under disk pressure. It's a UI feature, not a storage contract —
unusable as a hosting substrate. Our filesystem convention (timestamped
filenames, content hashes) IS the version store, and it's portable.

## Open decisions for Tilde

- ~~Glass: plain, rows, or full?~~ **RESOLVED 2026-07-05: rows.**
- ~~Filter URL scheme for production~~ **RESOLVED 2026-07-05: tags as paths,
  view state as composable query params (`?level/&fav/&q`).**
- ~~Does the serif/sans typeface toggle ship, and where does it live?~~
  **RESOLVED 2026-07-05: ships; `t` key + a footer control on entry pages,
  persisted `esko-type`.**
- Previous-post crumb: deferred as probably too busy.

================================================================================

# 2026-07-05 SESSION 2 HANDOFF (read this first after /clear)

Written just before a context clear. This session was mostly a **design
conversation** with Tilde that produced a complete, coherent "Phase 2" (identity
+ grading + a URL-grammar redesign), plus it advanced the entry-body port. Two
buckets below: (A) finish the in-flight entry-body port, then (B) Phase 2.

## A. Entry-body port — EXACT current state

**DONE 2026-07-05 (committed): the entire entry-body port shipped** — the JS
and Rust halves specced below are all implemented. `enhancePost()` (h1
canonical-chain anchor + h2 `#` anchors via JS-injected `<a class="anchor">`,
mini-TOC when ≥ 3 h2s, code copy pills, blockquote quote/deep-link menu),
`buildFootnotes()` (clone pandoc's bottom `.footnotes` into right-margin `.sn`
notes; margin-vs-bottom via `(innerWidth - mainWidth)/2 >= 330`, set as
`html[data-sn]`), serif body + persisted typeface toggle, land-on-post +
min-height pad, and the FULL inline Continue load are all live. The URL-follow
was changed from viewport-majority to **reading-line / Discourse-style** (the
address reflects the article whose top has crossed ~30% of the viewport, so
short link cards are no longer skipped); a failed inline load falls back to a
real navigation (click only). Proximity glow now runs on BOTH the timeline and
entry pages. The entry-body selectors were generalized `article#post` →
`main > article` so appended articles render identically. Storage keys use
`NS = "site"` (see Client-side above), NOT the `esko-*` the spec below names.
The TODO spec is kept below as the record of what was built.

`src/templates.rs` reached this via **uncommitted, working** edits where the
CSS half landed first (do NOT rewrite it); the JS + Rust halves (specs below)
then landed and the whole port was committed together. Even mid-port the tree
compiled and the entry page rendered in serif with working no-JS fallbacks
(bottom footnotes visible, Continue degrades to a plain link).

### CSS that already landed (in the `CSS` const, uncommitted)
- Added `--serif` and `--code-hair` tokens.
- Factored the timeline meter into a shared `.meter` class (so the post header
  reuses it); `main section article > aside > span.meter` keeps only its
  margin/violet-on-hover specifics.
- Rewrote the `article#post` block into the ported serif design, adapted to
  **pandoc's real output** (verified live): pandoc emits `<h1 id>`/`<h2 id>`
  (auto ids), `<a class="footnote-ref"><sup>N</sup></a>`, a
  `<section class="footnotes footnotes-end-of-document"><hr><ol><li id=fnN>…
  <a class="footnote-back">↩︎</a></li></ol></section>`, and code as
  `<div class="sourceCode"><pre class="sourceCode lang"><code>` (bare ``` blocks
  are plain `<pre><code>`). CSS covers: flex post header (time·meter·tags),
  serif body + `html[data-type="sans"]` swap, h1 clamp/h2 sizes,
  `:is(h1,h2) > a.anchor` (left-margin, opacity .32, `.near`/`:focus-visible`→1,
  hover violet, h1 chain svg .58em), `button.pill` (copy + quote/link), copy pill
  on `div.sourceCode/pre`, blockquote (violet bar, hover `> menu`),
  `a.footnote-ref sup`, `.sn` two-mode (`html[data-sn="margin"]` shows margin
  notes and hides `.footnotes`; default/unset = bottom = no-JS-safe),
  `.footnotes` restyle (hide its `<hr>`, `::before` "Footnotes" heading,
  `.lit` glow), `#toc` details, `#continue` + autoload `> label`,
  `main > article + article` border, compact `@media(56rem)` anchors-inline,
  footer flex + `#typeface`.

### JS still TODO (extend the `JS` const) — full spec
localStorage keys: `esko-width`, `esko-saved`, `esko-type`, `esko-autoload`.
- **Generalize proximity glow to BOTH pages**: pull it out of the timeline
  block; one system over selector `"main article aside > button, main .anchor"`
  (rAF-throttled, `.near` within 50px; touch ignored). Timeline matches buttons,
  entry matches injected anchors; appended articles' anchors auto-included.
- **`enhancePost(post)`** (run on the first `#post` AND each appended article):
  ensure h2 ids (slugify+uniquify fallback; pandoc usually supplies); capture
  clean TOC text BEFORE injecting anchors; append h1 chain `a.anchor`
  (href = `post.dataset.canonical`, SVG via a CHAIN const, **createElement +
  setAttribute, never innerHTML for attribute values** — labels can hold quotes);
  append h2 `#id` `a.anchor`; build mini-TOC `<details id="toc">` after the h1
  when ≥3 h2s; `buildFootnotes(post)`; copy pill on each `pre > code` (host =
  `div.sourceCode` parent if present else the `pre`); blockquote quote/link
  `<menu>` (quote copies “lines”+cite; link copies
  `location.origin + article.dataset.canonical + "#:~:text=" + first line`).
- **`buildFootnotes`** (scoped per article — handles appended dup ids): for the
  `.footnotes` section, clone each `ol>li` (strip `.footnote-back`, unwrap a lone
  `<p>`) into a `<span class="sn"><sup>N</sup>…</span>` inserted after the Nth
  `a.footnote-ref`; ref click → margin mode `glow(sn)` / bottom mode
  `li.scrollIntoView + glow(li)`; hover lights the margin note; backref click →
  scroll to ref; all `preventDefault` (URL stays clean).
- **`updateSidenotes()` inside `applyWidth`**: set `html[data-sn] =
  (innerWidth - mainWidth)/2 >= 330 ? "margin" : "bottom"`, but **only once
  `.sn` spans exist** (guard on `document.querySelector("main .sn")`), so no-JS /
  pre-enhance stays bottom (footnotes visible). Call `applyWidth()` again after
  `enhancePost`.
- **Typeface**: `setType(v)` sets `html[data-type]`, stores `esko-type`, updates
  the footer `#typeface span` text; wire the footer button + the `t` key (guard
  inputs/slider). Default serif; brief flash for sans users accepted.
- **Land-on-post + pad** (`page==="entry"`): `recomputePad()` clears
  `main.style.minHeight`, measures `#crumbs` absolute top, pads `main` so
  `scrollHeight >= landingTop + innerHeight`; on load if `!location.hash &&
  scrollY===0` → pad then `scrollTo(0, landingTop)`; resize re-pads (NO re-jump);
  `#tomenu` → `scrollTo(0,0)` (instant).
- **Continue inline-load** (`#continue` present only when a next entry exists):
  teaser click → `fetch(href)` → `DOMParser` → take `article#post`,
  `removeAttribute("id")`, `importNode`, insert before `#continue`,
  `enhancePost`, `applyWidth`, `io.observe`; advance the teaser from the fetched
  doc's own `#continue > a` (copy href + innerHTML) or, if none, remove teaser +
  autolabel and set the `<p>` to "That's everything"; reveal the autoload
  `<label>` after first load. URL-follow: IntersectionObserver over
  `main > article` picks the viewport-majority article →
  `history.replaceState(null,"",a.dataset.canonical)` + `document.title` from
  `a.dataset.title`. Autoload: `esko-autoload` opt-in; IO on `#continue`
  (rootMargin 200px) calls the loader. **No-JS: Continue is a plain link — must
  keep working.**
- Helpers: `glow` (add `.lit`, clear at 1600ms), `copyText` (clipboard +
  textarea fallback), `flash` (restore innerHTML at 1400ms), `CHAIN` svg,
  `slugify`.

### Rust markup still TODO (`src/templates.rs`)
- `entry_page` + `image_page`: add `data-title="{escaped label}"` to
  `<article id="post">` (URL-follow reads it).
- `continue_nav`: add `<label hidden><input type="checkbox" id="autoload"> keep
  loading as I scroll</label>` inside `#continue`.
- `page_shell`: render a footer typeface control **on entry pages only**, e.g.
  `<button id="typeface" title="Reading typeface (press t)"><kbd>t</kbd>
  <span>serif</span></button>` with a `·` separator after the help button.

### Then
`nix develop --command cargo test`; **restart the server with the sandbox
disabled** (bind fails otherwise — kill the old instance holding :1234 first);
click a document entry (hello-world / code-highlighting has 6 h2s → TOC),
verify anchors/copy/footnotes/typeface/Continue. Commit the whole port as ONE
commit. Then PLAN items 5 (docs: DESIGN.md/PLAN.md/.claude-memory) and 6 (grep
`level=`/`fav=`/`data-level`/`entry_href`/`label_unique` — zero outside docs).

## B. PHASE 2 — identity + grading + URL grammar (all DECIDED this session)

> **SUPERSEDED IN PART, 2026-07-06 — see `entry-model.md` (canonical) and the
> SESSION 4 HANDOFF below.** What changed: **UUID/`.id` identity is dropped**
> (identity = post name + user-created `alias <name>/` marker folders;
> collisions fail closed). **Publish date** = empty date-named subfolder, or
> mtime for bare files (birthtime/Date Added ruled out — not portable to the
> Linux host). **The server never writes into the content tree** (no
> auto-fold, no DoRename; caches live outside). Ledger renamed
> **`.esko.bar-grade-judgements.jsonl`**; param renamed **`?grade=`**; scale
> collapses to **everything/notable** (+ deliberate favorites tag); site is
> fully functional with no ledger. The URL-grammar section below (2b) shipped
> and stands; the prior-art sweep stands as research record.

Backed by a web prior-art sweep (condensed below). Sequence: write a design doc
first (`identity-grading-urls.md`, ref from DESIGN.md), then build 2b→2d.

### Entry identity (unlocks the rename Tilde wants)
- **Server-assigned UUIDv7** per entry, on first sight. Time-ordered (sorts by
  creation). **NEVER stored as a tag.**
- Home, tiered: custom **xattr `bar.esko.id`** (invisible in Finder — a
  different key than `com.apple.metadata:_kMDItemUserTags`; survives an offline
  Finder rename, inode-attached) **mirrored to a hidden full-name sidecar**
  (`entry.ext.<suffix>` — survives edits/zip/iCloud where xattrs get stripped);
  **folder-entries keep the id in an inner file** (most robust — rename the
  folder = atomic). Content-hash is a **fallback re-pair signal only**, never the
  identity. A UUID is a *stored fact* → offline edit+rename works with **no
  daemon running** (the reason hash+name lost).
- **Do NOT auto-fold** dropped files into folders — it would kill the `.md`-vs-`/`
  type display and the `/name.md` raw view. File-vs-folder stays a meaningful
  user choice; folders are opt-in (Tilde's "promote a file to a folder" gesture)
  for robust identity / multi-file entries.

### Grading
- **Pairwise** ("A > B"). **Append-only ledger**: a single hidden file
  **`.esko.bar-grading-ledger.jsonl`** in the content root; server **unions +
  dedupes** all `.esko.bar-grading-ledger*.jsonl` it finds (lossless merge of
  iCloud conflict copies). The
  **disposable derived-rank cache lives in the server state dir OUTSIDE the
  content tree** (rebuildable, unsynced).
- Derive `grade` = **P(entry beats a random entry)** via **Bradley-Terry (batch
  MLE, order-independent, fully recomputable) + a Bayesian prior** (stays finite
  when deletions fragment the comparison graph — plain BT MLE is undefined on a
  disconnected graph). NOT live Elo (order-dependent, un-auditable, can't
  un-count deletions).
- Thresholds already in `stats.rs`: notable ≥ .50, best ≥ .78. URL stays
  `?grade=notable|best` (extends to `?grade=0.8` later — same "minimum grade"
  meaning). `?q=` is search (confirmed). Deletion: tombstone + skip-dangling +
  prior.
- Grading surface: a **private authenticated management API** (HTTPS-only, strong
  auth e.g. passkey/token, unreachable from the public site, fail-closed; raw
  scores/judgments never render to visitors). Web grader now / native app later
  both consume it; OpenAPI as the contract, hand-written handlers. Not this
  phase's build.

### URL grammar redesign (`src/url.rs`) — CHANGES the current scheme
- **Dates = slash hierarchy** (time IS hierarchy): `/2026/`, `/2026/03/`,
  `/2026/03/25/`. Truncatable in the URL bar with no trailing-hyphen cleanup.
  (Replaces the current hyphen-single-segment `/2026-03`.)
- **Tags = one flat segment**: `/+design+dev+!meme`. `+` = include & join,
  **`!` = exclude** (can't use `-`: it's a legal tag char, ambiguous inside a
  packed segment). `ContentQuery` gains `not_tags`. Cloud gets a **three-state
  cycle** neutral→include→exclude→neutral, each tag a real link to its next
  state (server-rendered, no JS); excluded tags render **strikethrough** +
  muted. The header must reflect the **full include/exclude SET** (upgrade from
  today's single `active_tag`, which also fixes the single-pill limitation).
- Combined: `/2026/03/25/+design+!meme`.
- **Entry canonical** `/name` (newest owns bare label). **Older version**:
  shortest disambiguating **day** prefix `/2026/03/20/name`; `?time=124522`
  ONLY for same-day collisions. Every non-canonical resolving URL 301s to
  canonical, query preserved. (Timestamps are currently naive-local — decide
  UTC/`Z` normalization.)

### Filename rename migration (Tilde's original ask — non-destructive)
- Drop the `YYYY-MM-DDTHHMMSS_` prefix → clean names. Depends on the UUID
  identity above so renames stay safe.
- **NEVER overwrite.** Collisions exist: THREE `cookie-consent-tests.html`
  (`2026-03-12T133513`, `2026-03-12T170005`, `2026-03-18T222420`); scan for
  others. Only the **newest keeps the clean name on disk**; older versions' bytes
  move to a **server-side content-addressed archive** (existing PLAN direction).
  **Save an index** of original→new name + first-seen + hash.
- Server stops parsing timestamps from filenames (`entry.rs::parse_filename`);
  reads first-seen from index/filesystem, identity from UUID. One-shot,
  pre-1.0.

### Prior-art (condensed, from the research sweep)
- **Pure sidecar-only pairwise ranking: essentially nobody does it** — a
  judgment is an *edge* between two items, so tools centralize the graph and at
  most write the *derived scalar* to sidecars (Photo Mechanic/Lightroom/Narrative
  XMP ratings; Elo cullers `rank_photos`/`elosort`/`Kura` use a central store).
  Honest file-native form = append-only ledger (source of truth) + disposable
  derived index.
- **Append-only + recompute** is the norm at scale: Chatbot Arena (Bradley-Terry
  MLE over all votes, order-independent) and gwern's `resorter` (CSV in/out,
  most-uncertain-pair querying).
- **Identity across rename+edit**: assigned opaque id wins — **Perkeep permanode**
  ("a signed random number"), **DEVONthink UUID**, **org-roam `:ID:`**. Every
  content-hash scheme breaks on edit / rename+edit-together and patches it with
  fragile heuristics — **TMSU repairs "moved OR modified, but not both"**,
  git-annex "copies metadata forward" (ambiguous). Confirms UUID over hash.
- **Sidecars**: full-name (`photo.jpg.xmp`, darktable/digiKam/Immich) beats
  base-name (collides among siblings sharing a stem). Sidecars join by filename →
  a rename orphans them unless the join is the embedded id → prefer xattr/folder
  for the durable copy, sidecar as the portable mirror.
- **Percentile**: "All Our Ideas" (Salganik–Levy) defines a score as the
  probability an item beats a random item — exactly our `grade`, robust to sparse
  counts.
- Pitfalls to avoid: content-hash-as-identity; base-name sidecar collisions; BT
  graph disconnection after deletion (→ use a prior); order-dependent online
  ratings; filename-joined sidecars.

### Execution order (next sessions)
1. **Finish entry-body port (A)** → test → commit → docs → grep. *(immediate)*
2. **Phase 2a**: design doc `identity-grading-urls.md` + DESIGN.md refs.
3. **Phase 2b**: URL grammar (`url.rs` slash-dates; flat `+/!` tags + `not_tags`;
   cloud three-state + strikethrough + full include/exclude set; `?time=`).
4. **Phase 2c**: UUID identity (xattr `bar.esko.id` + sidecar mirror / folder
   inner; hash-heal) → rename migration (non-destructive, index, server archive,
   stop parsing filename timestamps).
5. **Phase 2d**: grading (`.esko.bar-grading-ledger.jsonl`, BT+prior, management
   API, private grading surface).

================================================================================

# 2026-07-05 SESSION 3 HANDOFF (read after /clear)

**Committed this session:** the entry-body port (heading anchors, mini-TOC,
copy/quote pills, two-mode footnotes/sidenotes, serif body + persisted typeface
toggle, land-on-post, FULL inline Continue with a reading-line / Discourse-style
URL-follow) and the tag cloud reverted to **flat, left-aligned, alphabetical**
(supersedes center-out; the rejected alternatives are kept in
`static/cloud-mockup.html`). Client storage keys use `NS = "site"`. Docs
(DESIGN.md, PLAN.md) updated to match.

**Still open from stage 2:** in-place, no-reload filtering (level/tag/search
without a full page load) via a JSON data island seeding the mockup's `paint()`,
plus the saved-view "hidden by filters → show them" note it enables.

**Next: Phase 2, in the documented order —**
1. **2a** — write the design doc `identity-grading-urls.md`, referenced from
   DESIGN.md.
2. **2b** — URL grammar: slash-hierarchy dates (`/2026/03/25/`), one flat tag
   segment with `+` include / `!` exclude, `?time=` for same-day collisions,
   the full include/exclude set in the header. **Re-evaluate the cloud
   three-state cycle** (neutral→include→exclude→neutral): the cloud is now
   alphabetical/stable, so check that interaction and the strikethrough-excluded
   rendering against the new order before building.
3. **2c** — UUID identity (`bar.esko.id` xattr + sidecar mirror / folder inner
   file, content-hash as re-pair fallback) → non-destructive filename rename
   migration, which **also drops untitled entries** (see the filename-direction
   section above).
4. **2d** — grading (`.esko.bar-grading-ledger.jsonl`, Bradley-Terry + prior,
   private authenticated management API).

================================================================================

# 2026-07-06 SESSION 4 HANDOFF (read after /clear)

**Shipped earlier (committed 7008c9a, d1ce97e, 1bac760):** slash-hierarchy
date URLs (`/2026/03/25`, `src/url.rs` rewrite, 67 tests), canonical
`?time=HHMMSS` disambiguation, date-scope chip + clear + month links,
plain-filename entries read fs time as naive_local.

**DECIDED this session (canonical spec: `entry-model.md`, rewritten):**
- Post = bare file (publish date = mtime; edit republishes) OR folder
  (publish date = empty date-named subfolder `YYYY-MM-DDTHHMM[SS][zone]/`,
  exactly one, zero -> mtime fallback, two+ -> error; edited = primary mtime).
- Primary file resolution: stem `index` or = folder name; exactly one, else
  the sole non-dotfile file; ambiguity -> error page. No symlinks.
- **No `.id`, no UUID, no Mac-app requirement.** Identity = name +
  `alias <name>/` empty marker folders (rename survival, extra names,
  shortlinks). One flat namespace for posts + aliases; **multiple claims on a
  name -> oldest claim keeps the bare URL** (2026-07-06 refinement: replaces
  both-error; others reachable at date paths, share surfaced on-page +
  logged). **Revisions**: the unsuffixed name is ALWAYS current (keeps URL,
  tags, timeline position); Cmd-D before editing freezes the archive into
  `label copy/` / `label copy 2/` (or ` copy` files inside the post =
  snapshots). Copies ignore inherited date markers — dated by primary
  mtime; timeline shows only current; copies via revision nav + date URLs.
  Workflow: Cmd-D, edit the original, nothing else. Fold gesture:
  Ctrl-Cmd-N New Folder with Selection. "copy" keyword = one constant
  (English-Finder only; configurable later). Unparseable spaced names fail
  closed (incl. lookalike-char typos). Fail-closed errors otherwise only
  for intra-post ambiguity.
- **Server strictly read-only on content**; all caches (embed, index, derived
  grades) outside the content tree, disposable. One-way sync, nothing back.
  Auto-fold and DoRename are dead; a future companion app (fold/stamp/grade)
  is a concierge, never a dependency.
- Grading: optional `.esko.bar-grade-judgements.jsonl` in content root,
  author-written (app or hand), synced forward; `?grade=` (renamed from
  `?level=`) with scale collapsed to everything/**notable**; empty ledger ->
  empty bucket, site fully works. Favorites remain a deliberate Finder tag.
  Grade is NOT a tag and NOT renamed to "effort".

**Docs updated to match:** entry-model.md (rewritten), DESIGN.md (Entries
section + view-filters `?grade=`), PLAN.md (supersession banner on Phase 2).

**BUILD PLAN (approved 2026-07-06 — hand off to a coding session; spec =
`entry-model.md`, read it first and treat it as canonical over this list).**
Commit per step. Restart the server after each step so Tilde can look
(sandbox off for the bind). `nix develop --command cargo test` throughout.

*Step 1 — entry model + scanner rewrite* (`src/entry.rs`, `src/content.rs`)
- `Entry` gains: `edited: Option<NaiveDateTime>` (primary mtime when
  meaningfully later than publish), `aliases: Vec<String>`,
  `revisions: Vec<Revision>` (`Revision { date, path }`, newest first),
  `error: Option<PostError>` (a post that scans wrong still renders — as a
  fail-closed error page). `kind`/extension come from the primary file.
- Scanner: top-level regular file -> bare post (label = stem, timestamp =
  mtime as naive_local, no edited line). Top-level dir -> folder post:
  - subfolders: date marker (lenient ISO basic parse, must be empty of
    non-dotfiles; 0 markers -> primary-mtime fallback, 2+ -> PostError),
    `alias <name>/` markers, everything else ignored (`.embed-cache` etc.);
  - primary resolution per spec (stem = `index` or folder name; else sole
    non-dotfile file; ambiguity -> PostError); other files = assets;
  - ` copy [n]` sibling dirs and `<primary-stem> copy [n].*` files =
    archived revisions, dated by their primary/own mtime (inherited date
    markers IGNORED for copies), excluded from the timeline list proper;
  - names containing spaces that parse as no known grammar -> PostError
    (lookalike-char protection).
- Claim resolution across the flat namespace (post names, bare-file stems,
  alias names): unique -> resolves; multiple -> oldest publish date owns the
  bare `/name`, others keep date-path URLs; record share partners on each
  entry for the on-page notice; loud tracing::warn.
- DELETE: `parse_filename` timestamp convention, `process_do_rename`, the
  birthtime fallback in `entry_from_plain_filename` (mtime only now).
  Update/replace their tests; add scanner tests over a tempdir fixture tree
  (bare post, folder post, marker variants, copies, alias, collisions, all
  PostError cases).

*Step 2 — routes + templates* (`src/routes.rs`, `src/templates.rs`)
- Serve folder posts: `/label` = rendered primary, `/label.ext` = raw
  primary, `/label/<asset>` = assets (path-traversal-safe: resolve inside
  the post dir only). Alias names 301 to canonical. Archived revisions
  resolve ONLY at date paths (+ `?time=`), never bare.
- Entry page: revision nav (list of archived revisions w/ dates) when
  present; alias list (small); name-share notice when claims collide.
- Timeline: rows show only current revisions; rows with revisions get a
  subtle "N revisions" `<details>` expanding to dated links. Errored posts
  render as errored rows (visible, not hidden).
- Error pages: exact conflicting relative paths + one-line fix, HTTP 500.

*Step 3 — `?grade=` rename + scale collapse* (`src/stats.rs`, routes,
templates)
- `?level=` -> `?grade=`; ViewFilter field rename; segmented control
  collapses to everything | notable (drop the "best" segment; keep the
  `NOTABLE` threshold const, delete/park `BEST`). Grade buckets remain empty
  (no ledger parsing this phase — `.esko.bar-grade-judgements.jsonl` is a
  later feature; absent ledger = empty bucket by design). Grep stragglers:
  `level=`, `data-level`.

*Step 4 — caches out of the content tree* (`src/embed.rs`, `src/main.rs`)
- `--cache-dir` flag, default via `directories` crate (macOS
  `~/Library/Caches/...`, Linux `$XDG_CACHE_HOME/...`). Embed cache moves
  there (keyed by content-relative path + mtime); content dir is NEVER
  written by the server after this step — audit for any remaining write
  (fs::rename/write/create under content root must be gone).

*Step 5 — one-shot migration* (script or `cargo run -- migrate`, run ONCE
by Tilde; never overwrite; DRY-run first and show the plan)
- Each `YYYY-MM-DDTHHMMSS[_label].ext` -> folder `<label>/` (unlabeled ->
  `untitled/`) containing `<label>.ext` + date marker `YYYY-MM-DDTHHMMSS/`
  seeded from the filename timestamp; sibling `<file>.embed-cache/` -> into
  the folder (or drop it — cache regenerates in the new cache dir anyway).
- Same-name duplicates: newest = unsuffixed current; older -> `<label>
  copy/`, `<label> copy 2/` with primary mtimes set (`touch -t`) from old
  filename timestamps (copies date by mtime).
- Anything ambiguous: leave in place, report. Old SetFile scratchpad script
  is dead — do not reuse.

*Step 6 — docs*: DESIGN.md/PLAN.md/.claude-memory updated to "shipped";
grep docs for the old convention.
