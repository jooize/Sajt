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
3. Entry page: land-on-post + crumbs + anchors + proximity JS.
4. Sidenotes/footnotes two-mode system (pandoc integration: emit `.sn`
   spans + build footnote list; or emit both server-side and hide one).
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
- **Tag cloud: center-out.** Sort tags by count desc (ties alphabetical),
  then alternate push-back/push-front so the largest lands mid-sequence and
  sizes fall off toward both edges. Presentation-only, in `render_cloud`.
  (Trade-off accepted: loses alphabetical scanning.)
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
- Does the serif/sans typeface toggle ship, and where does it live? (still open)
- Previous-post crumb: deferred as probably too busy.
