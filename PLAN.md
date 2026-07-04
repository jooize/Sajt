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

1. Extract a shared header component in templates.rs; render on timeline +
   entry pages with real tag stats. Decide + implement filter URL scheme.
2. Timeline page: rows/rail/marks/keyboard/help/saved from the mockup.
3. Entry page: land-on-post + crumbs + anchors + proximity JS.
4. Sidenotes/footnotes two-mode system (pandoc integration: emit `.sn`
   spans + build footnote list; or emit both server-side and hide one).
5. Width grip + readout, shared store, both pages.
6. Glass variant IF decided; update DESIGN.md either way.
7. Prune interim glass card styles from templates.rs (DESIGN.md notes they
   linger from the 2026-07-03 pruning).

Run: `nix develop --command cargo run` + `caddy start --config Caddyfile`,
then https://localhost. Restart server after changes (Tilde watches live).

## Open decisions for Tilde

- Glass: plain, rows, or full? (timeline-glass-mockup.html, `g` cycles)
- Filter URL scheme for production (`/+tag` pattern vs query params)
- Does the serif/sans typeface toggle ship, and where does it live?
- Previous-post crumb: deferred as probably too busy.
