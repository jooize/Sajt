use crate::entry::{Entry, ListItem, Listing, PostError, Revision};
use crate::slug::{is_reserved_slug, slug};
use crate::stats::{compute_cloud, CloudStats, TagStat, ViewFilter};

// ============================================================================
// Stylesheet — ported from the mockups (timeline-glass-mockup.html, the "rows"
// glass variant Tilde chose, and entry-page-mockup.html's shared header). The
// old glass-card design was thrown out. Element selectors, no classes (the JS
// state classes .selected/.near/.empty aside), light-dark() theming.
// ============================================================================

pub(crate) const CSS: &str = r##"
:root {
  color-scheme: light dark;

  --violet: #a123f6;
  --violet-soft: light-dark(rgba(161, 35, 246, .08), rgba(161, 35, 246, .13));

  --bg:     light-dark(#f5f4f0, #121514);
  --ink:    light-dark(#20231f, #d9ded9);
  --soft:   light-dark(#43473f, #b4bab2);
  --faint:  light-dark(#71766c, #8b918a);
  --hair:   light-dark(rgba(46, 62, 50, .18), rgba(178, 205, 184, .16));
  --pill:   light-dark(rgba(46, 62, 50, .1), rgba(178, 205, 184, .14));
  --code-bg: light-dark(#eceae4, #1c201a);
  --code-hair: light-dark(rgba(46, 62, 50, .14), rgba(178, 205, 184, .12));

  /* Finder tag colors, tuned per scheme */
  --tag-red:    light-dark(#e0383e, #ff6961);
  --tag-orange: light-dark(#e8842c, #ffb340);
  --tag-yellow: light-dark(#d9a800, #ffd426);
  --tag-green:  light-dark(#2f9e50, #30db5b);
  --tag-blue:   light-dark(#1673de, #409cff);
  --tag-purple: light-dark(#9853d2, #bf5af2);
  --tag-gray:   light-dark(#8e8e93, #98989d);

  --serif: ui-serif, "New York", Georgia, "Times New Roman", serif;
  --sans:  system-ui, -apple-system, "Helvetica Neue", sans-serif;
  --mono:  ui-monospace, "SF Mono", Menlo, Consolas, monospace;

  --content-w: 46rem;
  --rail: 5.6rem;

  /* Glass material (the "rows" variant), tuned to this palette */
  --glass:        light-dark(rgba(255, 255, 255, .5),  rgba(44, 50, 46, .44));
  --glass-strong: light-dark(rgba(252, 252, 250, .68), rgba(40, 46, 42, .64));
  --glass-edge:   light-dark(rgba(255, 255, 255, .62), rgba(255, 255, 255, .1));
  --glass-shadow:
    0 6px 20px light-dark(rgba(30, 34, 28, .09), rgba(0, 0, 0, .38)),
    0 1px 2px  light-dark(rgba(30, 34, 28, .06), rgba(0, 0, 0, .3)),
    inset 0 1px 0 light-dark(rgba(255, 255, 255, .7), rgba(255, 255, 255, .12));
  --glass-shadow-hover:
    0 12px 28px light-dark(rgba(30, 34, 28, .14), rgba(0, 0, 0, .48)),
    0 1px 2px   light-dark(rgba(30, 34, 28, .06), rgba(0, 0, 0, .3)),
    inset 0 1px 0 light-dark(rgba(255, 255, 255, .8), rgba(255, 255, 255, .16));

  /* Compatibility aliases so the embed-card CSS keeps rendering until the
     entry body is fully ported (stage 3-4). */
  --color-fg: var(--ink);
  --color-muted: var(--soft);
  --color-faint: var(--faint);
  --color-link: var(--violet);
  --color-link-hover: var(--violet);
  --color-card-bg: var(--glass);
  --glass-border: var(--glass-edge);
}

* { margin: 0; box-sizing: border-box; }

html { -webkit-text-size-adjust: 100%; }

body {
  background: var(--bg);
  color: var(--ink);
  font-family: var(--sans);
  font-size: 1rem;
  line-height: 1.5;
  padding-inline: 1.25rem;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

:focus-visible {
  outline: 2px solid var(--violet);
  outline-offset: 2px;
  border-radius: 4px;
}

::selection { background: rgba(161, 35, 246, .25); }

a { color: var(--violet); text-decoration: none; }
a:hover { text-decoration: underline; text-underline-offset: 3px; }

main {
  max-width: var(--content-w);
  margin-inline: auto;
  padding: 0 0 4rem;
}

/* ---------- reading-width handle (both pages) ---------- */

#grip {
  position: fixed;
  top: 50%;
  translate: 0 -50%;
  left: calc(50% + var(--content-w) / 2 + .55rem);
  width: 1.1rem;
  height: 3.8rem;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: .3rem;
  cursor: col-resize;
  touch-action: none;
  border-radius: 999px;
  z-index: 5;
}
#grip::before,
#grip::after {
  content: "";
  width: 4px;
  height: 1.05rem;
  border-radius: 999px;
  background: var(--hair);
  transition: background .15s ease, height .15s ease;
}
#grip:hover::before,
#grip:hover::after,
#grip:focus-visible::before,
#grip:focus-visible::after,
#grip.active::before,
#grip.active::after { background: var(--violet); height: 1.35rem; }
@media (prefers-reduced-motion: reduce) { #grip::before, #grip::after { transition: none; } }
@media (max-width: 56rem) { #grip { display: none; } }

#readout {
  position: fixed;
  top: 50%;
  left: 0;
  translate: 0 -50%;
  display: grid;
  grid-template-columns: auto auto;
  column-gap: .32rem;
  align-items: baseline;
  text-align: right;
  font: 500 .72rem/1.4 var(--mono);
  font-variant-numeric: tabular-nums;
  letter-spacing: -.01em;
  color: var(--bg);
  background: var(--ink);
  padding: .36rem .52rem;
  border-radius: .4rem;
  opacity: 0;
  pointer-events: none;
  z-index: 6;
  transition: opacity .12s ease;
}
#readout b { font-weight: 600; }
#readout i { font-style: normal; text-align: left; opacity: .55; }
body:has(#grip.active) #readout,
body:has(#grip:focus-visible) #readout { opacity: 1; }
@media (prefers-reduced-motion: reduce) { #readout { transition: none; } }
@media (max-width: 56rem) { #readout { display: none; } }
body:has(#grip.active) { -webkit-user-select: none; user-select: none; }

/* ================= shared site header (cloud + controls) ================= */

#site {
  max-width: var(--content-w);
  margin-inline: auto;
  padding: 1.4rem 0 1.3rem;
  font-family: var(--sans);
}

/* the cloud IS the header: size = entries, ink = recency, dot = Finder color.
   A flat, left-aligned wrap — predictable, no reshuffling mass. */
#cloud {
  position: relative;
  max-width: 30rem;
  margin-inline: 0 auto;
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  justify-content: center;
  column-gap: 1.4rem;
  row-gap: .5rem;
  padding: 1rem .5rem 2.6rem;
  line-height: 1.25;
}
#cloud:empty { display: none; }
#cloud a {
  position: relative;
  display: inline-flex;
  align-items: baseline;
  gap: .38rem;
  font-weight: 550;
  letter-spacing: -.011em;
}
#cloud a:hover { color: var(--violet) !important; text-decoration: none; }
/* selected topic wears a quiet Finder-grey pill, drawn out of flow so nothing
   shifts a pixel on toggle */
#cloud a[aria-current="true"] { isolation: isolate; }
#cloud a[aria-current="true"]::after {
  content: "";
  position: absolute;
  inset: calc(-.14em + 1px) -.6em calc(-.14em - 1px);
  z-index: -1;
  border-radius: 999px;
  background: var(--pill);
}
#cloud a::before {
  content: "";
  align-self: center;
  translate: 0 .09em;
  width: .5em;
  height: .5em;
  min-width: 6px;
  min-height: 6px;
  border-radius: 50%;
  background: var(--tag, var(--tag-gray));
}
/* Finder color index -> the --tag custom property the dots read. Generic: shared
   by the cloud, the post-header rail, and row tag pills. Index 0 (none) and 1
   (gray) carry no rule and fall through to the neutral --tag-gray fallback. */
[data-tag-color="2"] { --tag: var(--tag-green); }
[data-tag-color="3"] { --tag: var(--tag-purple); }
[data-tag-color="4"] { --tag: var(--tag-blue); }
[data-tag-color="5"] { --tag: var(--tag-yellow); }
[data-tag-color="6"] { --tag: var(--tag-red); }
[data-tag-color="7"] { --tag: var(--tag-orange); }
/* cloud recency: fresh topics render dark + bold, dormant ones fade. */
#cloud a[data-recency="fresh"] { color: var(--ink); font-weight: 650; }
#cloud a[data-recency="mid"] { color: var(--soft); font-weight: 550; }
#cloud a[data-recency="dormant"] { color: var(--faint); font-weight: 500; }
#cloud a > small {
  font-size: .68em;
  font-weight: 500;
  color: var(--faint);
  font-feature-settings: "tnum";
  translate: 0 -.07em;
}

/* active date scope: a removable glass chip that sits between the cloud and the
   controls, pulled up under the cloud's tall padding. Only present when a date
   filter is active, so the header is otherwise unchanged. */
#scope {
  display: flex;
  justify-content: center;
  margin: -1.7rem 0 1rem;
}
#scope span,
#scope a {
  display: inline-flex;
  align-items: center;
  height: 1.7rem;
  background: var(--glass);
  border: .5px solid var(--glass-edge);
  -webkit-backdrop-filter: blur(14px) saturate(160%);
  backdrop-filter: blur(14px) saturate(160%);
  box-shadow: inset 0 1px 0 light-dark(rgba(255, 255, 255, .55), rgba(255, 255, 255, .08));
}
#scope span {
  padding: 0 .35rem 0 .8rem;
  border-radius: 999px 0 0 999px;
  border-right: none;
  font: 550 .82rem var(--sans);
  color: var(--soft);
  font-feature-settings: "tnum";
}
#scope a {
  padding: 0 .6rem 0 .45rem;
  border-radius: 0 999px 999px 0;
  border-left: .5px solid var(--hair);
  color: var(--faint);
  font-size: 1.05rem;
  line-height: 1;
}
#scope a:hover { color: var(--violet); text-decoration: none; background: var(--violet-soft); }

/* controls: one quiet line, no rule under it */
#site form {
  position: relative;
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: .4rem 1.2rem;
  font: 550 .8rem var(--sans);
  color: var(--faint);
}

/* saved-bookmarks count hangs in the left margin, x-aligned with the rows' marks */
#navsaved {
  position: absolute;
  left: -3.4rem;
  top: .95rem;
  translate: 0 -50%;
  display: inline-flex;
  align-items: center;
  gap: .3rem;
  padding: 0 .1rem;
  color: var(--faint);
}
#navsaved:hover { color: var(--violet); text-decoration: none; }
#navsaved[aria-current="page"] { color: var(--violet); }
#navsaved svg {
  width: .85rem;
  height: .85rem;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.8;
  stroke-linejoin: round;
}
#navsaved[aria-current="page"] svg { fill: currentColor; }
#navsaved output { font: 500 .74rem var(--mono); font-feature-settings: "tnum"; }
#navsaved i { font-style: normal; font-size: .72rem; opacity: .5; }
#navsaved[hidden] { display: none; }

#site form p { display: inline-flex; gap: .55rem; align-items: center; margin: 0; }
/* stepped bars, lit to the current grade floor */
#site form p > svg { width: 1.1rem; height: 1.1rem; fill: currentColor; }
#site form p > svg rect { opacity: .3; }
#site form p > svg rect[data-on] { opacity: 1; }
#site form p > a#favonly {
  padding: 0;
  border: none;
  background: none;
  color: var(--faint);
  font: inherit;
  cursor: pointer;
}
#site form p > a#favonly > b { color: var(--violet); font-weight: 500; }
#site form p > a#favonly:hover { color: var(--violet); text-decoration: none; }
#site form p > a#favonly[aria-current="true"] { color: var(--ink); box-shadow: 0 2px 0 -.5px var(--violet); }

/* segmented control: a sliding thumb in a quiet glass capsule */
#site form p > span {
  position: relative;
  display: inline-grid;
  grid-auto-flow: column;
  grid-auto-columns: 1fr;
  padding: 2px;
  border-radius: 999px;
  background: var(--glass);
  border: .5px solid var(--glass-edge);
  -webkit-backdrop-filter: blur(14px) saturate(160%);
  backdrop-filter: blur(14px) saturate(160%);
  box-shadow: inset 0 1px 0 light-dark(rgba(255, 255, 255, .55), rgba(255, 255, 255, .08));
}
#site form p > span > u {
  position: absolute;
  inset-block: 2px;
  left: 2px;
  width: calc((100% - 4px) / 2);
  border-radius: 999px;
  background: light-dark(#fff, rgba(178, 205, 184, .14));
  box-shadow: 0 1px 3px light-dark(rgba(20, 24, 20, .16), rgba(0, 0, 0, .4));
  transition: left .18s ease;
}
/* notable segment selected: slide the thumb to the halfway mark */
#site form p > span > u[data-notable] { left: calc(2px + (100% - 4px) / 2); }
#site form p > span > a {
  position: relative;
  padding: .3em .9em;
  border-radius: 999px;
  color: var(--faint);
  text-align: center;
}
#site form p > span > a:hover { color: var(--violet); text-decoration: none; }
#site form p > span > a[aria-current="true"] { color: var(--ink); }
@media (prefers-reduced-motion: reduce) { #site form p > span > u { transition: none; } }

/* search, always visible, on its own line */
#site form search { flex-basis: 100%; margin: .4rem 0 0; display: inline-flex; align-items: center; gap: .55rem; }
#site form search button { display: inline-flex; padding: 0; border: none; background: none; color: var(--faint); cursor: pointer; }
#site form search button:hover { color: var(--violet); }
#site form search button svg { width: 1.1rem; height: 1.1rem; fill: none; stroke: currentColor; stroke-width: 1.9; stroke-linecap: round; }
#site form search input {
  width: 11rem;
  padding: .2rem 0;
  border: none;
  border-bottom: 1px solid var(--hair);
  border-radius: 0;
  background: none;
  color: var(--ink);
  font: .85rem var(--sans);
}
#site form search input::placeholder { color: var(--faint); }
#site form search input:focus { outline: none; border-bottom-color: var(--violet); }

/* ---------- crumbs (entry page): up-left on the post ---------- */
#crumbs {
  max-width: var(--content-w);
  margin-inline: auto;
  display: flex;
  gap: 1.2rem;
  margin-bottom: 1.4rem;
  font: 550 .8rem var(--sans);
}
#crumbs a, #crumbs button {
  padding: 0; border: none; background: none; cursor: pointer;
  color: var(--faint); font: inherit;
}
#crumbs a:hover, #crumbs button:hover { color: var(--violet); text-decoration: none; }

/* ================= timeline: month headings + rows ================= */

main section > h2 {
  margin: 2.3rem 0 .7rem;
  font: 650 .95rem var(--sans);
  font-feature-settings: "tnum";
  letter-spacing: -.011em;
  color: var(--soft);
}
main section > h2 > a { color: inherit; }
main section > h2 > a:hover { color: var(--violet); text-decoration: none; }
main section:first-of-type > h2 { margin-top: .9rem; }
main section > ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: .5rem; }

main section article {
  position: relative;
  display: grid;
  grid-template-columns: var(--rail) 1fr;
  column-gap: 1.25rem;
  padding: .75rem .6rem;
  margin-inline: -.6rem;
  border-radius: 14px;
  background: var(--glass);
  border: .5px solid var(--glass-edge);
  -webkit-backdrop-filter: blur(18px) saturate(170%);
  backdrop-filter: blur(18px) saturate(170%);
  box-shadow: var(--glass-shadow);
  transition: transform .16s ease, box-shadow .16s ease;
}
main section article:hover { transform: translateY(-1px); box-shadow: var(--glass-shadow-hover); }
main section article.selected { background: color-mix(in srgb, var(--violet) 9%, var(--glass)); }
@media (prefers-reduced-motion: reduce) {
  main section article { transition: none; }
  main section article:hover { transform: none; }
}

/* the row's hover zone reaches into the margin its marks hang in */
main section article::before {
  content: "";
  position: absolute;
  top: 0; bottom: 0;
  left: -3.4rem;
  width: 3.4rem;
}

/* left rail: date, quality meter, tags — one straight scan line */
main section article > aside { display: flex; flex-direction: column; align-items: flex-start; }
main section article > aside > time {
  font: 500 .74rem var(--mono);
  line-height: 1.5rem;
  font-feature-settings: "tnum";
  letter-spacing: .02em;
  color: var(--faint);
  white-space: nowrap;
}

/* quality meter — a hairline with a tick at the notable threshold, shared
   by the timeline rows and the entry-post header */
.meter { position: relative; width: 2.9rem; height: 2px; flex: none; border-radius: 1px; background: var(--hair); }
.meter > i { position: absolute; inset: 0 auto 0 0; border-radius: 1px; background: var(--faint); }
.meter::before { content: ""; position: absolute; top: -2px; left: 50%; width: 1px; height: 6px; background: var(--hair); }
main section article > aside > span.meter { margin-top: .55rem; }
main section article:hover .meter > i,
main section article.selected .meter > i { background: var(--violet); }

/* tags: vertical, in their Finder colors */
main section article > aside > nav { display: flex; flex-direction: column; align-items: flex-start; gap: .18rem; margin-top: .6rem; }
main section article > aside > nav a { display: inline-flex; align-items: center; gap: .35rem; font: 550 .74rem var(--sans); color: var(--soft); }
main section article > aside > nav a:hover { color: var(--violet); text-decoration: none; }
main section article > aside > nav a::before { content: ""; width: .42rem; height: .42rem; border-radius: 50%; background: var(--tag, var(--tag-gray)); }

/* hanging marks, like footnotes: ★ nearest, bookmark outside it */
main section article aside > b {
  position: absolute; top: .75rem; left: -1.4rem;
  color: var(--violet); font-size: .8rem; font-weight: 500; line-height: 1.5rem;
}
main section article aside > button {
  position: absolute; top: .75rem; left: -2.8rem;
  display: inline-flex; align-items: center; height: 1.5rem; padding: 0 .1rem;
  border: none; background: none; color: var(--faint); cursor: pointer;
  opacity: .32;
  transition: opacity .15s ease, color .15s ease;
}
main section article aside > button.near,
main section article aside > button:focus-visible,
main section article aside > button[aria-pressed="true"] { opacity: 1; }
main section article aside > button:hover { color: var(--violet); }
main section article aside > button[aria-pressed="true"] { color: var(--violet); }
main section article aside > button svg { width: .85rem; height: .85rem; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linejoin: round; translate: 0 -1px; }
main section article aside > button[aria-pressed="true"] svg { fill: currentColor; }
@media (prefers-reduced-motion: reduce) { main section article aside > button { transition: none; } }

/* content: every title the same size */
main section article h3 { font-size: 1rem; font-weight: 650; letter-spacing: -.011em; line-height: 1.5; margin: 0; }
/* child combinator: OUR label -> OUR page. A promoted cite (h3 > cite > a, a bare
   link with no label of its own) links out instead, and keeps the cite styling. */
main section article h3 > a { color: var(--ink); }
main section article h3 > a:hover { color: var(--violet); text-decoration: none; }
main section article h3 small { font-size: 1em; font-weight: inherit; color: var(--soft); }
main section article h3 i { font-weight: 450; color: var(--faint); }
main section article > div > p { margin: .12rem 0 0; font-size: .875rem; line-height: 1.5; color: var(--soft); }

/* ---- outbound cite: the DESTINATION (external), post-model.md §4 ---- */
/* Scoped to `article` so timeline rows and entry pages (#source / #linkcard) share
   one styling; a semantic <cite>, no class. Ported from static/link-rows-mockup.html. */
article cite {
  display: inline-flex;
  align-items: center;
  gap: .45rem;
  font-style: normal;
  font-size: .8rem;
  margin-top: .4rem;
  max-width: 100%;
}
article cite > a { display: inline-flex; align-items: center; gap: .45rem; color: var(--soft); min-width: 0; }
article cite > a:hover { color: var(--violet); text-decoration: none; }
article cite b { font-weight: 550; color: var(--soft); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
article cite > a:hover b { color: var(--violet); }
article cite span { color: var(--faint); white-space: nowrap; }        /* domain */
article cite span::before { content: "\00B7\00A0"; }                   /* "· " */
/* the "leaves the site" arrow, appended to any external source link */
article cite > a::after { content: "\2197"; font-size: .82em; color: var(--faint); translate: 0 -.05em; }
/* http:// gets a caution tint on the domain (the scheme guard, echoed in CSS) */
article cite a[href^="http://"] span { color: var(--tag-red); }
article cite a[href^="http://"] span::after { content: " (not secure)"; }

/* stand-in favicon tile: the domain's first letter on a hashed color (data-tile),
   a static rule so no inline style and zero third-party requests (privacy). */
article cite i {
  flex: none;
  width: 1.05rem; height: 1.05rem;
  border-radius: 4px;
  display: inline-flex; align-items: center; justify-content: center;
  font: 700 .62rem/1 var(--sans);
  font-style: normal;
  color: #fff;
}
article cite i[data-tile="0"] { background: #d1495b; }
article cite i[data-tile="1"] { background: #2a9d8f; }
article cite i[data-tile="2"] { background: #e76f51; }
article cite i[data-tile="3"] { background: #4361ee; }
article cite i[data-tile="4"] { background: #7b2cbf; }
article cite i[data-tile="5"] { background: #386641; }

/* a quiet internal permalink for a bare link that has no label of its own (case 4) */
main section article > div > a[data-perma] {
  display: inline-block;
  margin-top: .35rem;
  font: 500 .72rem var(--mono);
  color: var(--faint);
}
main section article > div > a[data-perma]:hover { color: var(--violet); text-decoration: none; }
main section article > div > a[data-perma]::before { content: "\00B6\00A0"; }   /* "¶ " */

/* entry-page source section (below a commentary body) + bare-link fallback body */
main > article > section#source { margin-top: 1.9rem; }
main > article > section#source > h2 {
  font: 600 .72rem var(--mono);
  letter-spacing: .04em;
  text-transform: uppercase;
  color: var(--faint);
  margin: 0 0 .35rem;
}
main > article > section > div#linkcard { margin: .5rem 0; }

main > p.empty { padding: 3rem 0; text-align: center; font-size: .875rem; color: var(--soft); }
main > p.empty[hidden] { display: none; }

/* compact: no margin to hang into, marks flow inline after the date */
@media (max-width: 56rem) {
  main section article aside > b,
  main section article aside > button { position: static; height: auto; }
  main section article::before { content: none; }
  main section article { grid-template-columns: 1fr; row-gap: .15rem; }
  main section article > aside { flex-direction: row; flex-wrap: wrap; align-items: center; column-gap: .6rem; }
  main section article > aside > span.meter { margin-top: 0; }
  main section article > aside > nav { flex-direction: row; margin-top: 0; column-gap: .6rem; }
  #navsaved { position: static; translate: none; }
  #cloud { column-gap: 1.2rem; }
}

/* ================= entry post (serif typography, ported) ================= */

/* header: one flex row — mono date · quality meter · Finder-color dot tags */
main > article > header { display: flex; align-items: center; flex-wrap: wrap; gap: .9rem; }
main > article > header > time {
  font: 500 .78rem var(--mono);
  font-feature-settings: "tnum";
  letter-spacing: .02em;
  color: var(--faint);
}
main > article > header > nav { display: flex; flex-wrap: wrap; gap: .8rem; font-size: .8rem; }
main > article > header > nav a { display: inline-flex; align-items: center; gap: .35rem; color: var(--soft); font-weight: 550; }
main > article > header > nav a:hover { color: var(--violet); text-decoration: none; }
main > article > header > nav a::before { content: ""; width: .42rem; height: .42rem; border-radius: 50%; background: var(--tag, var(--tag-gray)); }

/* the body reads in serif; the typeface toggle (t / footer) swaps to sans on
   <html data-type="sans">. Chrome (header, crumbs, footer) stays sans. */
main > article > section {
  margin-top: 1.9rem;
  font-family: var(--serif);
  font-size: 1.0625rem;
  line-height: 1.72;
  color: var(--ink);
}
html[data-type="sans"] main > article > section { font-family: var(--sans); letter-spacing: .001em; }
main > article > section > :first-child { margin-top: 0; }
main > article > section p { margin: 0 0 1.15rem; }

/* headings; the left-margin anchors are injected by JS as <a class="anchor"> */
main > article > section h1 {
  position: relative;
  font-size: clamp(1.75rem, 4.5vw, 2.35rem);
  font-weight: 700;
  line-height: 1.14;
  letter-spacing: -.018em;
  margin: 0 0 1.3rem;
  text-wrap: balance;
}
main > article > section h2 {
  position: relative;
  font-size: 1.32rem;
  font-weight: 650;
  line-height: 1.25;
  letter-spacing: -.012em;
  margin: 2.6rem 0 .9rem;
}
main > article > section h3 { font-size: 1.15rem; font-weight: 600; margin: 1.8rem 0 .6rem; }
main > article > section h4, main > article > section h5, main > article > section h6 { font-size: 1rem; font-weight: 600; margin: 1.6rem 0 .5rem; }

/* anchors hang left, always faintly there; pointer nearness (.near, set in JS)
   lifts them, the anchor itself turns violet on hover. h1 carries a chain to the
   entry's own address, h2s keep # section links. */
main > article > section :is(h1, h2) > a.anchor {
  position: absolute;
  left: -1.7rem;
  top: 0;
  width: 1.4rem;
  text-align: center;
  font: 550 .85em var(--sans);
  color: var(--faint);
  opacity: .32;
  text-decoration: none;
  transition: opacity .15s ease, color .15s ease;
}
main > article > section h1 > a.anchor { font-size: inherit; display: grid; place-items: center; height: 1.14em; }
main > article > section h1 > a.anchor svg {
  width: .58em; height: .58em; fill: none; stroke: currentColor;
  stroke-width: 1.7; stroke-linecap: round; stroke-linejoin: round;
}
main > article > section :is(h1, h2) > a.anchor.near,
main > article > section :is(h1, h2) > a.anchor:focus-visible { opacity: 1; }
main > article > section :is(h1, h2) > a.anchor:hover { opacity: 1; color: var(--violet); text-decoration: none; }
@media (prefers-reduced-motion: reduce) { main > article > section :is(h1, h2) > a.anchor { transition: none; } }

main > article > section ul, main > article > section ol { padding-left: 1.3em; margin: 0 0 1.15rem; }
main > article > section li { margin: .35em 0; }
main > article > section ul { list-style: disc; }
main > article > section ol { list-style: decimal; }
main > article > section img { max-width: 100%; height: auto; border-radius: .5em; }
main > article > section hr { border: 0; border-top: 1px solid var(--hair); margin: 2.5em 0; }
main > article > section a { text-decoration: underline; text-decoration-color: color-mix(in srgb, var(--violet), transparent 55%); text-underline-offset: .15em; }
main > article > section a:hover { text-decoration-color: currentColor; }

/* tables (pandoc pipe tables). Cell alignment arrives as a data-align attribute
   -- the sanitizer rewrites pandoc's inline `text-align` style out, so nothing
   inline is left for a strict `style-src 'self'` to refuse. */
main > article > section table { width: 100%; border-collapse: collapse; margin: 0 0 1.4rem; font-size: .95em; display: block; overflow-x: auto; }
main > article > section th, main > article > section td { padding: .4em .7em; border-bottom: 1px solid var(--hair); text-align: left; }
main > article > section thead th { border-bottom: 2px solid var(--faint); font-weight: 600; }
main > article > section :is(th, td)[data-align="left"] { text-align: left; }
main > article > section :is(th, td)[data-align="center"] { text-align: center; }
main > article > section :is(th, td)[data-align="right"] { text-align: right; }

/* Outbound-scheme guard (post-model.md §7). Plaintext http links are allowed but
   flagged; unsafe schemes never reach here as anchors -- the server neutralizes
   them into a [data-unsafe-link] span before the body is served. */
main a[href^="http://"]::after { content: " \2197\FE0E (not secure)"; font-size: .82em; color: var(--tag-red); white-space: nowrap; }
main [data-unsafe-link] { color: var(--tag-red); text-decoration: line-through; cursor: not-allowed; }
main [data-unsafe-link]::before { content: "\26A0\FE0E\00A0"; }

/* inline + block code (pandoc: bare <pre> or <div class="sourceCode"><pre>) */
main > article > section code { font-family: var(--mono); font-size: .86em; background: var(--code-bg); padding: .12em .35em; border-radius: 5px; border: 1px solid var(--code-hair); }
main > article > section pre { position: relative; overflow-x: auto; padding: 1em 1.1em; margin: 0 0 1.4rem; background: var(--code-bg); border: 1px solid var(--code-hair); border-radius: 11px; font-size: .85em; line-height: 1.55; }
main > article > section pre code { background: none; padding: 0; border: 0; font-size: inherit; }
main > article > section div.sourceCode { position: relative; margin: 0 0 1.4rem; }
main > article > section div.sourceCode > pre { margin: 0; }

/* shared pill look for JS-injected copy + quote/link buttons */
main > article button.pill {
  padding: .16rem .6rem; border-radius: 999px;
  border: 1px solid var(--hair); background: var(--bg);
  color: var(--faint); font: 550 .68rem var(--sans); font-style: normal; cursor: pointer;
}
main > article button.pill:hover { color: var(--violet); border-color: var(--violet); }

/* copy pill on code blocks — appears on hover, no layout shift */
main > article > section div.sourceCode > button.pill,
main > article > section pre > button.pill {
  position: absolute; top: .55rem; right: .6rem; z-index: 1;
  opacity: 0; transition: opacity .15s ease;
}
main > article > section div.sourceCode:hover > button.pill,
main > article > section pre:hover > button.pill,
main > article > section div.sourceCode > button.pill:focus-visible,
main > article > section pre > button.pill:focus-visible { opacity: 1; }
@media (hover: none) {
  main > article > section div.sourceCode > button.pill,
  main > article > section pre > button.pill { opacity: .7; }
}

/* blockquote: violet bar, italic, discreet quote/link menu (JS-injected) */
main > article > section blockquote {
  position: relative;
  margin: 1.8rem 0;
  padding-left: 1.2rem;
  padding-right: 7.5rem;
  border-left: 2px solid var(--violet);
  font-style: italic;
  color: var(--soft);
}
main > article > section blockquote p { margin: 0 0 .6rem; }
main > article > section blockquote p:last-child { margin-bottom: 0; }
main > article > section blockquote :is(footer, cite) {
  display: block; margin-top: .35rem; font: .8rem var(--sans);
  font-style: normal; color: var(--faint);
}
main > article > section blockquote > menu {
  position: absolute; top: .05rem; right: 0;
  display: flex; gap: .35rem; margin: 0; padding: 0; list-style: none;
  opacity: 0; transition: opacity .15s ease;
}
main > article > section blockquote:hover > menu,
main > article > section blockquote > menu:focus-within { opacity: 1; }
@media (hover: none) { main > article > section blockquote > menu { opacity: .7; } }
@media (prefers-reduced-motion: reduce) {
  main > article > section blockquote > menu,
  main > article > section div.sourceCode > button.pill,
  main > article > section pre > button.pill { transition: none; }
}
@media (max-width: 40rem) {
  main > article > section blockquote { padding-right: 1rem; }
  main > article > section blockquote > menu { position: static; margin-top: .5rem; opacity: .7; }
}

/* footnote refs, right-margin sidenotes, and the bottom footnote list.
   Two modes switch on <html data-sn>: with room JS clones each note into a
   right-margin .sn and hides the bottom list; otherwise the bottom list shows
   (also the no-JS default — no data-sn attribute). */
main > article > section a.footnote-ref { color: var(--violet); }
main > article > section a.footnote-ref sup { font: 650 .72em var(--sans); line-height: 0; }

main > article .sn { display: none; }
html[data-sn="margin"] main > article .sn {
  display: block; float: right; clear: right;
  width: 15.5rem; margin: .15rem -18.5rem 1rem 1.5rem;
  font: .79rem/1.55 var(--sans); font-style: normal; color: var(--faint);
  transition: color .25s ease;
}
html[data-sn="margin"] main > article .sn.lit { color: var(--ink); }
main > article .sn > sup { margin-right: .35em; color: var(--violet); }

html[data-sn="margin"] main > article .footnotes { display: none; }
main > article .footnotes {
  margin-top: 3rem; padding-top: 1rem; border-top: 1px solid var(--hair);
  font: .85rem/1.55 var(--sans); color: var(--soft);
}
main > article .footnotes hr { display: none; }
main > article .footnotes::before {
  content: "Footnotes"; display: block; margin: 0 0 .55rem;
  font: 500 .74rem var(--mono); letter-spacing: .09em; text-transform: uppercase; color: var(--faint);
}
main > article .footnotes ol { margin: 0; padding-left: 1.4rem; display: grid; gap: .55rem; }
main > article .footnotes li { margin: 0; border-radius: 6px; padding: .1rem .4rem; margin-inline: -.4rem; transition: background .3s ease; }
main > article .footnotes li::marker { color: var(--violet); font: 650 .85em var(--sans); }
main > article .footnotes li.lit { background: var(--violet-soft); }
main > article .footnotes li p { margin: 0; }
main > article .footnotes .footnote-back { margin-left: .35em; }
@media (prefers-reduced-motion: reduce) {
  html[data-sn="margin"] main > article .sn, main > article .footnotes li { transition: none; }
}

/* mini-TOC — JS-injected after the h1 when the post has >= 3 sections */
main > article #toc { margin: 0 0 1.6rem; font: .8rem var(--sans); color: var(--faint); }
main > article #toc summary { width: fit-content; cursor: pointer; font-weight: 550; }
main > article #toc summary:hover { color: var(--violet); }
main > article #toc ol { margin: .45rem 0 0; padding-left: 1.35rem; list-style: decimal; }
main > article #toc li { margin: .1rem 0; }
main > article #toc a { color: var(--soft); text-decoration: none; }
main > article #toc a:hover { color: var(--violet); }

main > article > footer { margin-top: 2.5rem; padding-top: 1rem; border-top: 1px solid var(--hair); font: .8rem var(--sans); }
main > article > footer a { color: var(--faint); margin-right: 1rem; }
main > article > footer a:hover { color: var(--ink); text-decoration: none; }

/* ---------- standalone .html post: embedded (A) + fullscreen (B) ---------- */
/* A: the dropped-in HTML runs in a sandboxed iframe. Its height is set by the
   site JS from the frame's own reporter; a floor keeps a blank frame from
   collapsing before the first measurement lands. */
#stage { margin: .5rem 0 .5rem; }
#stage > iframe {
  display: block; width: 100%; border: 1px solid var(--hair); border-radius: 12px;
  min-height: 8rem; height: 24rem; background: var(--bg); color-scheme: light;
}
#se-controls { margin: .2rem 0 0; font: .8rem var(--sans); }
#se-controls a { color: var(--faint); margin-right: 1rem; }
#se-controls a:hover { color: var(--violet); text-decoration: none; }
/* B: fullscreen chrome -- a slim fixed bar over a viewport-filling frame. */
body[data-page="fullscreen"] { margin: 0; }
#sebar {
  position: fixed; inset: 0 0 auto 0; z-index: 2; height: 2.9rem;
  display: flex; align-items: center; gap: 1rem; padding: 0 1rem;
  background: var(--glass); -webkit-backdrop-filter: blur(14px) saturate(160%);
  backdrop-filter: blur(14px) saturate(160%); border-bottom: .5px solid var(--glass-edge);
  font: 550 .84rem var(--sans);
}
#sebar-mark { color: var(--violet); font-weight: 650; }
#sebar-title { color: var(--soft); min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
#sebar-actions { margin-left: auto; display: flex; gap: 1rem; white-space: nowrap; }
#sebar a { color: var(--faint); }
#sebar a:hover, #sebar-mark:hover { color: var(--violet); text-decoration: none; }
#se-full { position: fixed; inset: 2.9rem 0 0 0; width: 100%; height: calc(100vh - 2.9rem); border: 0; background: var(--bg); }

/* ---------- continue reading: post navigation as content ---------- */
main > article + article { margin-top: 3.2rem; padding-top: 3.2rem; border-top: 1px solid var(--hair); }
#continue { margin-top: 3rem; padding-top: 1rem; border-top: 1px solid var(--hair); }
#continue > p { margin: 0 0 .55rem; font: 500 .74rem var(--mono); letter-spacing: .09em; text-transform: uppercase; color: var(--faint); }
#continue > a { display: block; color: inherit; }
#continue > a:hover { text-decoration: none; }
#continue > a b { font: 650 1rem var(--sans); letter-spacing: -.011em; }
#continue > a:hover b { color: var(--violet); }
#continue > a time { margin-left: .6rem; font: 500 .74rem var(--mono); letter-spacing: .06em; color: var(--faint); }
#continue > label { display: inline-flex; align-items: center; gap: .45rem; margin-top: 1rem; font: 550 .78rem var(--sans); color: var(--faint); cursor: pointer; }
#continue > label[hidden] { display: none; }
#continue > label input { margin: 0; accent-color: var(--violet); }

/* ---- folder listings (post-model.md §6): gallery / file list ---- */
/* Scoped under #listing so its resets beat the prose `section ul/ol` rules. */
#listing > h1 { font-size: 1.35rem; font-weight: 680; letter-spacing: -.02em; margin: .3rem 0 1rem; }
#listing > h1 > small { font-weight: 450; font-size: .78rem; color: var(--faint); margin-left: .5rem; font-family: var(--mono); }
#listing > aside {
  display: flex; align-items: baseline; gap: .5rem;
  font-size: .82rem; color: var(--soft);
  border: .5px solid var(--hair); border-left: 3px solid var(--tag-yellow);
  border-radius: 8px; padding: .5rem .75rem; margin: 0 0 1.2rem;
  background: light-dark(rgba(245, 163, 0, .05), rgba(255, 212, 38, .06));
}
#listing > aside::before { content: "\24D8"; color: var(--tag-yellow); font-size: .9rem; }
#listing > aside code, #listing .empty code { font: 500 .95em var(--mono); color: var(--ink); }
#listing .empty { padding: 2.5rem 0; text-align: center; font-size: .875rem; color: var(--soft); }
/* gallery */
#listing section > ul {
  list-style: none; padding: 0; margin: 1rem 0 0;
  display: grid; grid-template-columns: repeat(auto-fill, minmax(9.5rem, 1fr)); gap: .6rem;
}
#listing section > ul > li { margin: 0; }
#listing section > ul > li > a {
  display: flex; flex-direction: column; border-radius: 12px; overflow: hidden;
  background: var(--glass); border: .5px solid var(--glass-edge); box-shadow: var(--glass-shadow); color: var(--ink);
}
#listing section > ul > li > a:hover { text-decoration: none; border-color: var(--violet); }
#listing figure { margin: 0; }
#listing figure > div { aspect-ratio: 4 / 3; overflow: hidden; background: var(--violet-soft); }
#listing figure img { width: 100%; height: 100%; object-fit: cover; display: block; border-radius: 0; }
#listing figcaption { padding: .4rem .55rem .5rem; font-size: .74rem; line-height: 1.35; display: flex; flex-direction: column; gap: .1rem; }
#listing figcaption b { font-weight: 560; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
#listing figcaption b small { font-weight: inherit; color: var(--soft); }
#listing figcaption span { font: 500 .68rem var(--mono); color: var(--faint); font-feature-settings: "tnum"; }
/* file list — shared by pure listings and a post's attachments (#attachments) */
#attachments { margin-top: 2.2rem; }
#attachments > h2 { font-size: .95rem; font-weight: 600; margin: 0 0 .7rem; letter-spacing: -.01em; }
:is(#listing section, #attachments) > ol { list-style: none; padding: 0; margin: 1rem 0 0; display: flex; flex-direction: column; gap: .4rem; }
:is(#listing section, #attachments) > ol > li { margin: 0; }
:is(#listing section, #attachments) > ol > li > a {
  display: grid; grid-template-columns: 2.4rem 1fr auto auto; align-items: center; column-gap: .8rem;
  padding: .55rem .7rem; border-radius: 12px;
  background: var(--glass); border: .5px solid var(--glass-edge); box-shadow: var(--glass-shadow); color: var(--ink);
}
:is(#listing section, #attachments) > ol > li > a:hover { text-decoration: none; border-color: var(--violet); }
:is(#listing section, #attachments) > ol i {
  font: 700 .58rem/1 var(--mono); font-style: normal; display: inline-flex; align-items: center; justify-content: center;
  width: 2.4rem; height: 1.55rem; border-radius: 5px; color: #fff; letter-spacing: .03em; background: var(--kind, var(--tag-gray));
}
/* file-type category -> the --kind badge tint (was an inline style; kept off
   inline for CSP). `file` carries no rule and falls to the neutral gray. */
:is(#listing section, #attachments) > ol i[data-kind="folder"]  { --kind: var(--violet); }
:is(#listing section, #attachments) > ol i[data-kind="pdf"]     { --kind: var(--tag-red); }
:is(#listing section, #attachments) > ol i[data-kind="doc"]     { --kind: var(--tag-blue); }
:is(#listing section, #attachments) > ol i[data-kind="archive"] { --kind: var(--tag-green); }
:is(#listing section, #attachments) > ol i[data-kind="media"]   { --kind: var(--tag-yellow); }
:is(#listing section, #attachments) > ol b { font-weight: 560; font-size: .875rem; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
:is(#listing section, #attachments) > ol b small { font-weight: inherit; color: var(--soft); }
:is(#listing section, #attachments) > ol span, :is(#listing section, #attachments) > ol time { font: 500 .72rem var(--mono); color: var(--faint); font-feature-settings: "tnum"; white-space: nowrap; }

/* compact: anchors go inline, sidenotes never float (no room) */
@media (max-width: 56rem) {
  main > article > section :is(h1, h2) > a.anchor {
    position: static; display: inline-grid; margin-left: .4em;
    width: auto; height: auto; vertical-align: baseline;
  }
  html[data-sn="margin"] main > article .sn { float: none; width: auto; margin: .4rem 0; }
}

/* image viewer */
figure { margin-top: 1.5rem; text-align: center; }
figure > img { max-width: 100%; max-height: 85vh; border-radius: .5em; }
figure > figcaption { margin-top: .5rem; font-size: .85rem; color: var(--faint); }

/* Privacy notice under a stripped image: quiet but always present (never
   auto-dismissed). A left accent rule and the violet code chip keep it legible
   in light and dark without shouting. */
main > article > aside[data-notice] {
  margin: 1.1rem auto 0; max-width: 46rem; text-align: left;
  padding: .55rem .8rem; font-size: .82rem; line-height: 1.5; color: var(--soft);
  background: var(--violet-soft); border-left: 2px solid var(--violet);
  border-radius: .3rem;
}
main > article > aside[data-notice] > code {
  font-size: .95em; padding: .05em .35em; border-radius: .25rem;
  background: color-mix(in oklab, var(--violet) 16%, transparent); color: var(--ink);
}
/* Danger variant: a public-original image that actually publishes GPS/camera
   metadata. The safe state is quiet; this one shouts — a warm red rule/tint and
   a heavier weight so it reads as a live warning in light and dark. */
main > article > aside[data-notice="danger"] {
  background: color-mix(in oklab, #e5484d 14%, transparent);
  border-left-color: #e5484d; color: var(--ink); font-weight: 550;
}
main > article > aside[data-notice="danger"] > code {
  background: color-mix(in oklab, #e5484d 22%, transparent);
}

/* ---------- Pandoc Skylighting: kate (light) + breezedark (dark) ---------- */
:root {
  --hl-keyword: #1f1c1b; --hl-keyword-weight: 700; --hl-datatype: #0057ae;
  --hl-function: #644a9b; --hl-string: #bf0303; --hl-char: #924c9d;
  --hl-specialchar: #3daee9; --hl-comment: #898887; --hl-annotation: #ca60ca;
  --hl-number: #b08000; --hl-operator: #1f1c1b; --hl-controlflow: #1f1c1b;
  --hl-controlflow-weight: 700; --hl-builtin: #644a9b; --hl-builtin-weight: 700;
  --hl-variable: #0057ae; --hl-preprocessor: #006e28; --hl-attribute: #0057ae;
  --hl-import: #ff5500; --hl-error: #bf0303; --hl-alert-fg: #bf0303;
  --hl-alert-bg: #f7e6e6; --hl-constant: #aa5500; --hl-specialstring: #ff5500;
  --hl-documentation: #607880; --hl-other: #006e28; --hl-information: #b08000;
}
@media (prefers-color-scheme: dark) {
  :root {
    --hl-keyword: #cfcfc2; --hl-keyword-weight: 700; --hl-datatype: #2980b9;
    --hl-function: #8e44ad; --hl-string: #f44f4f; --hl-char: #3daee9;
    --hl-specialchar: #3daee9; --hl-comment: #7a7c7d; --hl-annotation: #3f8058;
    --hl-number: #f67400; --hl-operator: #cfcfc2; --hl-controlflow: #fdbc4b;
    --hl-controlflow-weight: 700; --hl-builtin: #7f8c8d; --hl-builtin-weight: normal;
    --hl-variable: #27aeae; --hl-preprocessor: #27ae60; --hl-attribute: #2980b9;
    --hl-import: #27ae60; --hl-error: #da4453; --hl-alert-fg: #95da4c;
    --hl-alert-bg: #4d1f24; --hl-constant: #27aeae; --hl-specialstring: #da4453;
    --hl-documentation: #a43340; --hl-other: #27ae60; --hl-information: #c45b00;
  }
}
div.sourceCode { position: relative; }
pre.sourceCode { background: var(--code-bg); }
code span.kw { color: var(--hl-keyword); font-weight: var(--hl-keyword-weight); }
code span.dt { color: var(--hl-datatype); }
code span.fu { color: var(--hl-function); }
code span.st { color: var(--hl-string); }
code span.ch { color: var(--hl-char); }
code span.sc { color: var(--hl-specialchar); }
code span.co { color: var(--hl-comment); }
code span.an { color: var(--hl-annotation); }
code span.dv, code span.bn, code span.fl { color: var(--hl-number); }
code span.op { color: var(--hl-operator); }
code span.cf { color: var(--hl-controlflow); font-weight: var(--hl-controlflow-weight); }
code span.bu { color: var(--hl-builtin); font-weight: var(--hl-builtin-weight); }
code span.va { color: var(--hl-variable); }
code span.pp { color: var(--hl-preprocessor); }
code span.at { color: var(--hl-attribute); }
code span.im { color: var(--hl-import); }
code span.er { color: var(--hl-error); text-decoration: underline; }
code span.al { color: var(--hl-alert-fg); background-color: var(--hl-alert-bg); font-weight: 700; }
code span.cn { color: var(--hl-constant); }
code span.ss { color: var(--hl-specialstring); }
code span.do { color: var(--hl-documentation); }
code span.ot { color: var(--hl-other); }
code span.in { color: var(--hl-information); }
code span.wa { color: var(--hl-error); }
code span.vs { color: var(--hl-string); }
code span.ex { color: var(--hl-function); font-weight: 700; }
code > span > a { color: var(--faint); text-decoration: none; user-select: none; }

/* ================= help dialog + footer ================= */

dialog {
  min-width: min(21rem, calc(100vw - 3rem));
  margin: auto auto 4.4rem;
  border: .5px solid var(--glass-edge);
  border-radius: 14px;
  padding: 1.3rem 1.5rem 1.4rem;
  background: var(--glass-strong);
  color: var(--ink);
  -webkit-backdrop-filter: blur(26px) saturate(180%);
  backdrop-filter: blur(26px) saturate(180%);
  box-shadow: 0 12px 32px light-dark(rgba(20, 24, 20, .14), rgba(0, 0, 0, .5));
}
dialog::backdrop { background: rgba(10, 12, 11, .25); -webkit-backdrop-filter: blur(5px); backdrop-filter: blur(5px); }
dialog h2 { font-size: .95rem; font-weight: 650; margin-bottom: .8rem; }
dialog dl { display: grid; grid-template-columns: auto 1fr; gap: .4rem 1rem; font-size: .85rem; color: var(--soft); }
dialog dt { text-align: right; }
dialog p { margin-top: 1rem; padding-top: .8rem; border-top: 1px solid var(--hair); font-size: .8rem; color: var(--soft); }
dialog p b { color: var(--violet); font-weight: 500; }
dialog p b svg { width: .82em; height: .82em; fill: currentColor; vertical-align: -.08em; }
kbd { font: 500 .95em var(--mono); }

body > footer {
  max-width: var(--content-w);
  margin-inline: auto;
  padding-bottom: 3rem;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-wrap: wrap;
  gap: .35rem .9rem;
  font: .8rem var(--sans);
  color: var(--soft);
  text-align: center;
}
body > footer button { padding: 0; border: none; background: none; color: inherit; font: inherit; cursor: pointer; }
body > footer button:hover { color: var(--violet); }
body > footer #typeface span { text-transform: capitalize; }
body > footer > i { font-style: normal; opacity: .4; }

/* Post extras: revisions, aliases, and the name-share notice under a post. */
#revisions, main section article > div > details {
  margin-top: .3rem;
  font: .8rem var(--sans);
  color: var(--soft);
}
#revisions { margin-top: 1.6rem; }
#revisions > summary, main section article details > summary {
  cursor: pointer;
  color: var(--soft);
  list-style: none;
}
#revisions > summary::-webkit-details-marker,
main section article details > summary::-webkit-details-marker { display: none; }
#revisions > summary::before,
main section article details > summary::before { content: "\203A\00a0"; color: var(--faint); }
#revisions[open] > summary::before,
main section article details[open] > summary::before { content: "\2304\00a0"; }
#revisions ol, main section article details ol { margin: .4rem 0 0; padding-left: 1rem; }
#revisions time, main section article details time { font-family: var(--mono); font-size: .92em; }

#aliases, #nameshare {
  margin-top: 1.6rem;
  padding-top: .9rem;
  border-top: 1px solid var(--hair);
  font: .82rem var(--sans);
  color: var(--soft);
}
#aliases h2, #nameshare p { font-size: .82rem; font-weight: 600; color: var(--soft); margin-bottom: .35rem; }
#aliases ul, #nameshare ul { list-style: none; display: flex; flex-wrap: wrap; gap: .3rem .9rem; }
#aliases a, #nameshare a { color: var(--soft); }
#aliases a:hover, #nameshare a:hover { color: var(--violet); }
#nameshare code, #aliases code { font-family: var(--mono); color: var(--ink); }

/* Errored posts, surfaced loud and never hidden. */
[data-error] { --violet: var(--tag-red); }
article[data-error] > section h1,
main article[data-error] h3 a { color: var(--tag-red); }
li article[data-error] aside > b { color: var(--tag-red); font-size: 1rem; }
article[data-error] ul { margin: .6rem 0; padding-left: 1.3rem; }
article[data-error] code { font-family: var(--mono); background: light-dark(rgba(210,60,60,.08), rgba(255,120,120,.12)); padding: .05em .35em; border-radius: 4px; }
"##;

// ============================================================================
// Client script — ported from the mockups. The cloud, rows and filtering are
// server-rendered (controls are real links), so this only carries progressive
// enhancements: the shared reading-width grip, help dialog and proximity glow;
// on the timeline the bookmarks/saved-view/keyboard behaviors; on entry pages
// the heading anchors, mini-TOC, copy/quote pills, two-mode footnotes, typeface
// toggle, land-on-post, and inline Continue loading. No-JS stays fully usable.
// ============================================================================

/// Pre-paint bootstrap: applies the reader's saved width and typeface before the
/// first paint, so there is no flash of default layout. It is a tiny, external,
/// render-blocking `<script src>` in `<head>` (CSP `script-src 'self'` forbids
/// inline script, and we take no nonce). Setting styles via the CSSOM `.style`
/// property is script-driven, not an inline `style` attribute, so it is clean
/// under `style-src 'self'`. Keep the `NS` prefix in sync with the JS below.
pub(crate) const BOOT_JS: &str = r##"
(function () {
  try {
    var NS = "site", d = document.documentElement, s = window.localStorage;
    var w = parseFloat(s.getItem(NS + "-width"));
    if (w > 0) {
      var m = Math.min(1160, window.innerWidth - 64);
      d.style.setProperty("--content-w", Math.max(480, Math.min(m, w)) + "px");
    }
    var t = s.getItem(NS + "-type");
    if (t) d.dataset.type = t;
  } catch (e) {}
})();
"##;

pub(crate) const JS: &str = r##"
(function () {
  var store = window.localStorage;
  /* Provisional key namespace: this is a generic site engine, so the client
     storage keys avoid a brand. Rename NS once the engine is named (pre-1.0,
     no migration needed). */
  var NS = "site";
  var WIDTH = NS + "-width", SAVED = NS + "-saved", TYPE = NS + "-type", AUTOLOAD = NS + "-autoload";
  var body = document.body, page = body.dataset.page;
  var help = document.getElementById("help");
  var helpbtn = document.getElementById("helpbtn");

  /* ---------- small shared helpers ---------- */
  var CHAIN = '<svg viewBox="0 0 24 24" aria-hidden="true">' +
    '<path d="M10 13a3 3 0 0 0 4 .3l3-3a3 3 0 0 0-4.3-4.3l-1.4 1.4"/>' +
    '<path d="M14 11a3 3 0 0 0-4-.3l-3 3a3 3 0 0 0 4.3 4.3l1.4-1.4"/></svg>';
  function glow(el) {
    el.classList.add("lit");
    clearTimeout(el._lit);
    el._lit = setTimeout(function () { el.classList.remove("lit"); el._lit = 0; }, 1600);
  }
  function fallbackCopy(text) {
    var ta = document.createElement("textarea");
    ta.value = text; ta.style.position = "fixed"; ta.style.opacity = "0";
    document.body.appendChild(ta); ta.focus(); ta.select();
    try { document.execCommand("copy"); } catch (e) {}
    ta.remove();
  }
  function copyText(text) {
    if (navigator.clipboard && navigator.clipboard.writeText) {
      return navigator.clipboard.writeText(text).catch(function () { fallbackCopy(text); });
    }
    fallbackCopy(text);
    return Promise.resolve();
  }
  function flash(btn, done) {
    var original = btn.innerHTML;
    btn.textContent = done;
    setTimeout(function () { btn.innerHTML = original; }, 1400);
  }
  /* Mirror of the server slug recipe (slug.rs), for in-page heading anchors:
     NFKD, drop nonspacing marks, NFC, lowercase, join apostrophes/quotes, then
     collapse every other non-alphanumeric run to a hyphen. Falls back to
     "section" (headings are never date-addressed, so an empty id is useless). */
  function slugify(s) {
    return s.normalize("NFKD").replace(/\p{Mn}/gu, "").normalize("NFC")
      .toLowerCase()
      .replace(/['‘’‛ʼ`´"“”‟]/g, "")
      .replace(/[^\p{L}\p{N}]+/gu, "-")
      .replace(/^-+|-+$/g, "") || "section";
  }

  /* ---------- standalone .html embed: size the sandboxed frame to its content ----------
     The jailed iframe (opaque origin) posts its measured height; we trust the
     message only from that exact frame's window, clamp it (floor so a blank
     frame keeps a sensible height, ceiling against a pathological value), and
     set the height via the CSSOM (script-driven, so CSP style-src is untouched). */
  var seFrame = document.getElementById("se-frame");
  if (seFrame) {
    window.addEventListener("message", function (e) {
      if (e.source !== seFrame.contentWindow) return;
      var hgt = e.data && e.data.eskoEmbedHeight;
      if (typeof hgt !== "number" || !isFinite(hgt)) return;
      hgt = Math.max(128, Math.min(200000, Math.round(hgt)));
      seFrame.style.height = hgt + "px";
    });
  }

  /* ---------- help dialog (both pages) ---------- */
  if (helpbtn && help) helpbtn.addEventListener("click", function () { help.showModal(); });
  if (help) help.addEventListener("click", function (e) { if (e.target === help) help.close(); });

  /* ---------- reading width: shared grip + store on both pages ---------- */
  var grip = document.getElementById("grip");
  var readout = document.getElementById("readout");
  var rootEl = document.documentElement;
  var mainEl = document.querySelector("main");
  /* header refs live inside #site, which the timeline swaps wholesale on an
     in-place filter change — so acquire them (and bind the magnifier) through
     bindHeader(), re-run after every swap. */
  var segSpan, qInput, qbtn;
  function bindHeader() {
    segSpan = document.querySelector("#site form p > span");
    qInput = document.getElementById("q");
    qbtn = document.getElementById("qbtn");
    /* the magnifier focuses the field rather than submitting an empty search;
       without JS it stays a submit button, so search still works. */
    if (qbtn && qInput) qbtn.addEventListener("click", function (e) { e.preventDefault(); qInput.focus(); });
  }
  bindHeader();
  var MINW = 480;
  var maxW = function () { return Math.min(1160, window.innerWidth - 64); };
  var clampW = function (v) { return Math.max(MINW, Math.min(maxW(), v)); };
  var width = parseFloat(store.getItem(WIDTH)) || null;
  var remPx = parseFloat(getComputedStyle(rootEl).fontSize) || 16;
  var chPx = 0;

  function measureCh() {
    if (!mainEl) return;
    var probe = document.createElement("span");
    probe.textContent = "0".repeat(50);
    probe.style.cssText = "position:absolute;visibility:hidden;white-space:pre;top:-9999px";
    var cs = getComputedStyle(mainEl);
    probe.style.font = cs.font || (cs.fontSize + " " + cs.fontFamily);
    document.body.appendChild(probe);
    chPx = probe.getBoundingClientRect().width / 50;
    probe.remove();
  }
  function syncSearchWidth() {
    if (!qInput || !segSpan) return;
    if (window.matchMedia("(max-width: 56rem)").matches) { qInput.style.width = ""; return; }
    qInput.style.width = segSpan.getBoundingClientRect().width + "px";
  }
  /* margin sidenotes vs. bottom footnotes — only switches once .sn spans exist,
     so the no-JS / pre-enhancement state stays "bottom" (footnote list visible). */
  function updateSidenotes() {
    if (!document.querySelector("main .sn")) return;
    var w = mainEl.getBoundingClientRect().width;
    rootEl.dataset.sn = (window.innerWidth - w) / 2 >= 330 ? "margin" : "bottom";
  }
  function applyWidth() {
    if (width) rootEl.style.setProperty("--content-w", clampW(width) + "px");
    else rootEl.style.removeProperty("--content-w");
    if (grip && mainEl) {
      var px = Math.round(mainEl.getBoundingClientRect().width);
      grip.setAttribute("aria-valuenow", px);
      grip.setAttribute("aria-valuemax", Math.round(maxW()));
      if (!chPx) measureCh();
      var cells = "<b>" + px + "</b><i>px</i><b>" + (px / remPx).toFixed(1) + "</b><i>rem</i>";
      if (chPx) cells += "<b>" + Math.round(px / chPx) + "</b><i>ch</i>";
      if (readout) {
        readout.innerHTML = cells;
        var gr = grip.getBoundingClientRect();
        var room = window.innerWidth - readout.offsetWidth - 10;
        readout.style.left = Math.max(6, Math.min(gr.right + 6, room)) + "px";
      }
    }
    syncSearchWidth();
    updateSidenotes();
  }
  if (grip && mainEl) {
    var dragging = false, grabDX = 0;
    grip.addEventListener("pointerdown", function (e) {
      dragging = true; grip.classList.add("active"); grip.setPointerCapture(e.pointerId);
      grabDX = e.clientX - mainEl.getBoundingClientRect().right;
    });
    grip.addEventListener("pointermove", function (e) {
      if (!dragging) return;
      var right = e.clientX - grabDX;
      width = clampW((right - window.innerWidth / 2) * 2);
      applyWidth();
    });
    grip.addEventListener("pointerup", function () {
      dragging = false; grip.classList.remove("active");
      if (width) store.setItem(WIDTH, width);
    });
    grip.addEventListener("dblclick", function () {
      width = null; store.removeItem(WIDTH); applyWidth();
    });
    grip.addEventListener("keydown", function (e) {
      var cur = mainEl.getBoundingClientRect().width;
      if (e.key === "ArrowRight") width = clampW(cur + 24);
      else if (e.key === "ArrowLeft") width = clampW(cur - 24);
      else if (e.key === "Home") width = null;
      else return;
      e.preventDefault();
      if (width) store.setItem(WIDTH, width); else store.removeItem(WIDTH);
      applyWidth();
    });
  }
  window.addEventListener("resize", applyWidth);
  applyWidth();

  /* ---------- proximity glow (both pages) ----------
     a mark/anchor rests dim and lifts to full grey within ~50px of the pointer,
     turning violet only on direct hover. Timeline matches its row marks, the
     entry page matches the injected heading anchors, and articles appended by
     Continue are picked up because the query runs live each frame. */
  var NEAR = 50, nearRaf = 0, nx = -1e4, ny = -1e4;
  function updateNear() {
    nearRaf = 0;
    document.querySelectorAll("main article aside > button, main .anchor").forEach(function (el) {
      var r = el.getBoundingClientRect();
      var dx = Math.max(r.left - nx, 0, nx - r.right);
      var dy = Math.max(r.top - ny, 0, ny - r.bottom);
      el.classList.toggle("near", dx * dx + dy * dy <= NEAR * NEAR);
    });
  }
  function queueNear() { if (!nearRaf) nearRaf = requestAnimationFrame(updateNear); }
  document.addEventListener("pointermove", function (e) {
    if (e.pointerType === "touch") return;
    nx = e.clientX; ny = e.clientY; queueNear();
  });
  document.documentElement.addEventListener("pointerleave", function () { nx = ny = -1e4; queueNear(); });

  /* ---------- entry page: enhance the post(s), typeface, land-on-post, continue ---------- */
  if (page === "entry") {
    var reduced = window.matchMedia("(prefers-reduced-motion: reduce)");
    var behave = function () { return reduced.matches ? "auto" : "smooth"; };

    /* two-mode footnotes: clone pandoc's bottom footnote list into right-margin
       .sn spans (shown only when there's room — see updateSidenotes). Pairing is
       positional: the Nth ref pairs with the Nth <li> / .sn. URL stays clean. */
    function buildFootnotes(post) {
      var fnSection = post.querySelector(".footnotes");
      if (!fnSection) return;
      var refs = post.querySelectorAll("a.footnote-ref");
      var items = fnSection.querySelectorAll("ol > li");
      var sns = [];
      items.forEach(function (li, i) {
        var ref = refs[i];
        if (!ref) { sns.push(null); return; }
        var clone = li.cloneNode(true);
        var back = clone.querySelector(".footnote-back");
        if (back) back.remove();
        var lone = clone.children.length === 1 && clone.firstElementChild.tagName === "P"
          ? clone.firstElementChild : null;
        var sn = document.createElement("span");
        sn.className = "sn";
        var sup = document.createElement("sup");
        sup.textContent = String(i + 1);
        sn.appendChild(sup);
        var host = document.createElement("span");
        host.innerHTML = (lone ? lone.innerHTML : clone.innerHTML).trim();
        sn.appendChild(host);
        ref.after(sn);
        sns.push(sn);
      });
      refs.forEach(function (ref, i) {
        var sn = sns[i], li = items[i];
        ref.addEventListener("click", function (e) {
          e.preventDefault();
          if (rootEl.dataset.sn === "margin") { if (sn) glow(sn); }
          else if (li) { li.scrollIntoView({ block: "center", behavior: behave() }); glow(li); }
        });
        ref.addEventListener("mouseenter", function () {
          if (rootEl.dataset.sn === "margin" && sn) sn.classList.add("lit");
        });
        ref.addEventListener("mouseleave", function () {
          if (sn && !sn._lit) sn.classList.remove("lit");
        });
      });
      fnSection.addEventListener("click", function (e) {
        var back = e.target.closest(".footnote-back");
        if (!back) return;
        e.preventDefault();
        var li = back.closest("li");
        var i = Array.prototype.indexOf.call(items, li);
        if (i >= 0 && refs[i]) refs[i].scrollIntoView({ block: "center", behavior: behave() });
      });
    }

    /* per-post enhancements — the first #post and every article Continue loads */
    function enhancePost(post) {
      var canonical = post.dataset.canonical || "";
      var heads = post.querySelectorAll("section > h2");
      var toc = [];
      /* capture ids + clean TOC text BEFORE injecting the anchor markup */
      heads.forEach(function (h) {
        if (!h.id) {
          var base = slugify(h.textContent), id = base, n = 2;
          while (document.getElementById(id)) { id = base + "-" + (n++); }
          h.id = id;
        }
        toc.push({ id: h.id, text: h.textContent });
      });
      var h1 = post.querySelector("section > h1");
      if (h1 && canonical) {
        var chain = document.createElement("a");
        chain.className = "anchor";
        chain.setAttribute("href", canonical);
        chain.setAttribute("aria-label", "Link to this entry");
        chain.setAttribute("title", "Link to this entry");
        chain.innerHTML = CHAIN;
        h1.appendChild(chain);
      }
      heads.forEach(function (h) {
        var a = document.createElement("a");
        a.className = "anchor";
        a.setAttribute("href", "#" + h.id);
        a.setAttribute("aria-label", "Link to this section");
        a.textContent = "#";
        h.appendChild(a);
      });
      if (h1 && toc.length >= 3) {
        var details = document.createElement("details");
        details.id = "toc";
        var summary = document.createElement("summary");
        summary.textContent = "On this page";
        details.appendChild(summary);
        var ol = document.createElement("ol");
        toc.forEach(function (t) {
          var li = document.createElement("li");
          var a = document.createElement("a");
          a.setAttribute("href", "#" + t.id);
          a.textContent = t.text;
          li.appendChild(a);
          ol.appendChild(li);
        });
        details.appendChild(ol);
        h1.after(details);
      }
      buildFootnotes(post);
      /* copy pill on each code block (host = div.sourceCode wrapper if present) */
      post.querySelectorAll("section pre > code").forEach(function (code) {
        var pre = code.parentElement;
        var host = pre.parentElement && pre.parentElement.classList.contains("sourceCode")
          ? pre.parentElement : pre;
        if (host.querySelector(":scope > button.pill")) return;
        var btn = document.createElement("button");
        btn.type = "button"; btn.className = "pill"; btn.textContent = "copy";
        btn.setAttribute("aria-label", "Copy code");
        btn.addEventListener("click", function () {
          copyText(code.textContent).then(function () { flash(btn, "copied ✓"); });
        });
        host.appendChild(btn);
      });
      /* blockquote quote / deep-link menu */
      post.querySelectorAll("section blockquote").forEach(function (bq) {
        if (bq.querySelector(":scope > menu")) return;
        var lines = function () {
          return Array.prototype.map.call(bq.querySelectorAll("p"), function (p) {
            return p.textContent.trim();
          }).filter(Boolean);
        };
        var menu = document.createElement("menu");
        var quoteBtn = document.createElement("button");
        quoteBtn.type = "button"; quoteBtn.className = "pill"; quoteBtn.textContent = "quote";
        quoteBtn.addEventListener("click", function () {
          var cite = bq.querySelector("footer, cite");
          var text = "“" + lines().join("\n") + "”" + (cite ? "\n— " + cite.textContent.trim() : "");
          copyText(text).then(function () { flash(quoteBtn, "copied ✓"); });
        });
        var linkBtn = document.createElement("button");
        linkBtn.type = "button"; linkBtn.className = "pill"; linkBtn.textContent = "link";
        linkBtn.addEventListener("click", function () {
          var base = canonical ? location.origin + canonical : location.href.split("#")[0];
          var link = base + "#:~:text=" + encodeURIComponent(lines()[0] || "");
          copyText(link).then(function () { flash(linkBtn, "copied ✓"); });
        });
        menu.appendChild(quoteBtn); menu.appendChild(linkBtn);
        bq.appendChild(menu);
      });
    }

    /* typeface toggle (serif default) — t key + footer control, persisted */
    function setType(v) {
      rootEl.dataset.type = v;
      store.setItem(TYPE, v);
      var lbl = document.querySelector("#typeface span");
      if (lbl) lbl.textContent = v;
    }
    setType(store.getItem(TYPE) || "serif");
    var typeBtn = document.getElementById("typeface");
    if (typeBtn) typeBtn.addEventListener("click", function () {
      setType(rootEl.dataset.type === "sans" ? "serif" : "sans");
    });

    /* enhance the first post, then re-apply width so sidenotes can pick a mode */
    var firstPost = document.getElementById("post");
    if (firstPost) { enhancePost(firstPost); applyWidth(); }

    /* land on the post: pad main if needed so the header sits above the fold,
       then jump instantly. Re-pad on resize; never re-jump. */
    var crumbs = document.getElementById("crumbs");
    function landingTop() {
      return crumbs ? crumbs.getBoundingClientRect().top + window.pageYOffset : 0;
    }
    function recomputePad() {
      mainEl.style.minHeight = "";
      var need = landingTop() + window.innerHeight;
      var docEl = document.documentElement;
      if (docEl.scrollHeight < need) {
        mainEl.style.minHeight =
          (mainEl.getBoundingClientRect().height + (need - docEl.scrollHeight)) + "px";
      }
    }
    if (!location.hash && window.scrollY === 0) {
      recomputePad();
      window.scrollTo(0, landingTop());
    }
    window.addEventListener("resize", recomputePad);
    var toMenu = document.getElementById("tomenu");
    if (toMenu) toMenu.addEventListener("click", function () { window.scrollTo(0, 0); });

    /* keyboard: t typeface, / search, ? help */
    document.addEventListener("keydown", function (e) {
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      if (e.target.closest("input, textarea, [role=slider]")) return;
      if (help && help.open) return;
      if (e.key === "t") { setType(rootEl.dataset.type === "sans" ? "serif" : "sans"); }
      else if (e.key === "/") { e.preventDefault(); if (qInput) qInput.focus(); }
      else if (e.key === "?") { if (help) help.showModal(); }
    });

    /* Continue: load the next entry inline (no-JS falls back to a plain link) */
    var cont = document.getElementById("continue");
    if (cont) {
      var teaser = cont.querySelector("a");
      var autolabel = cont.querySelector("label");
      var autoToggle = document.getElementById("autoload");
      var loading = false, done = false;
      var lastPath = firstPost ? firstPost.dataset.canonical : null;

      /* URL-follow, Discourse-style: the address reflects the article whose top
         has crossed a reading line near the top of the viewport — the one you
         are actually reading. Stepping is monotonic, so a short entry (a link
         card) still gets its turn as its top passes the line; the old
         viewport-majority heuristic skipped anything too short to ever dominate
         the viewport. Scroll-driven, rAF-throttled. */
      var READ_LINE = 0.3;
      function updateURL() {
        var line = window.innerHeight * READ_LINE;
        var current = null;
        document.querySelectorAll("main > article").forEach(function (a) {
          if (a.getBoundingClientRect().top <= line) current = a;
        });
        if (!current) current = document.querySelector("main > article");
        if (!current || !current.dataset.canonical || current.dataset.canonical === lastPath) return;
        lastPath = current.dataset.canonical;
        try { history.replaceState(null, "", lastPath); } catch (err) {}
        if (current.dataset.title) document.title = "esko.bar — " + current.dataset.title;
      }
      var urlRaf = 0;
      function queueURL() { if (!urlRaf) urlRaf = requestAnimationFrame(function () { urlRaf = 0; updateURL(); }); }
      window.addEventListener("scroll", queueURL, { passive: true });

      function loadNext(userInitiated) {
        if (loading || done) return;
        var href = teaser && teaser.getAttribute("href");
        if (!href) return;
        loading = true;
        fetch(href, { headers: { "Accept": "text/html" } })
          .then(function (r) { return r.text(); })
          .then(function (htmlText) {
            var doc = new DOMParser().parseFromString(htmlText, "text/html");
            var art = doc.querySelector("article#post");
            if (!art) throw new Error("no post in " + href);
            art.removeAttribute("id");
            var imported = document.importNode(art, true);
            cont.parentNode.insertBefore(imported, cont);
            enhancePost(imported);
            applyWidth();
            recomputePad();
            var nextTeaser = doc.querySelector("#continue > a");
            if (nextTeaser && teaser) {
              teaser.setAttribute("href", nextTeaser.getAttribute("href"));
              teaser.innerHTML = nextTeaser.innerHTML;
            } else {
              if (teaser) teaser.remove();
              if (autolabel) autolabel.remove();
              var p = cont.querySelector("p");
              if (p) p.textContent = "That’s everything";
              done = true;
            }
            if (autolabel) autolabel.hidden = false;
            loading = false;
            updateURL();
          })
          .catch(function () {
            loading = false;
            /* inline load failed — fall back to a real navigation, but only for
               an explicit click, so autoload never yanks the reader away. */
            if (userInitiated) location.assign(href);
          });
      }
      if (teaser) teaser.addEventListener("click", function (e) { e.preventDefault(); loadNext(true); });

      if (autoToggle) {
        autoToggle.checked = store.getItem(AUTOLOAD) === "1";
        if (autoToggle.checked && autolabel) autolabel.hidden = false;
        autoToggle.addEventListener("change", function () {
          store.setItem(AUTOLOAD, autoToggle.checked ? "1" : "0");
        });
        new IntersectionObserver(function (entries) {
          if (autoToggle.checked && entries.some(function (en) { return en.isIntersecting; })) loadNext(false);
        }, { rootMargin: "200px" }).observe(cont);
      }
    }
  }

  /* ---------- timeline: bookmarks, saved view, in-place filtering, keyboard ---------- */
  if (page === "timeline") {
    var saved = new Set(JSON.parse(store.getItem(SAVED) || "[]"));
    var savedView = body.dataset.view === "saved";
    var navsaved, count;
    var arts = function () { return Array.prototype.slice.call(document.querySelectorAll("main article")); };
    var keyOf = function (a) { return a.dataset.key; };

    function refreshMarks() {
      arts().forEach(function (a) {
        var b = a.querySelector("aside > button");
        if (b) b.setAttribute("aria-pressed", saved.has(keyOf(a)));
      });
    }
    function refreshNav() {
      if (!navsaved) return;
      navsaved.hidden = !(saved.size > 0 || savedView);
      if (count) count.textContent = saved.size;
      navsaved.setAttribute("aria-current", savedView ? "page" : "false");
    }
    function applySavedView() {
      if (!savedView) return;
      var any = false;
      arts().forEach(function (a) {
        var li = a.closest("li"), vis = saved.has(keyOf(a));
        if (li) li.hidden = !vis;
        if (vis) any = true;
      });
      document.querySelectorAll("main section").forEach(function (s) {
        var vis = Array.prototype.some.call(s.querySelectorAll("li"), function (li) { return !li.hidden; });
        s.hidden = !vis;
      });
      var msg = document.getElementById("saved-empty");
      if (msg) msg.hidden = any;
    }

    /* re-read per-render state after an in-place swap: the header (hence
       #navsaved) is replaced, and body[data-view] flips between / and /saved. */
    function reinitTimeline() {
      savedView = body.dataset.view === "saved";
      navsaved = document.getElementById("navsaved");
      count = navsaved && navsaved.querySelector("output");
      refreshMarks(); refreshNav(); applySavedView();
    }
    reinitTimeline();

    /* bookmark toggle — delegated on the persistent <main>, so it survives the
       innerHTML swaps that in-place filtering does. */
    if (mainEl) mainEl.addEventListener("click", function (e) {
      var b = e.target.closest("aside > button"); if (!b) return;
      var a = b.closest("article"), k = keyOf(a);
      if (saved.has(k)) saved.delete(k); else saved.add(k);
      store.setItem(SAVED, JSON.stringify(Array.from(saved)));
      b.setAttribute("aria-pressed", saved.has(k));
      refreshNav();
      if (savedView) applySavedView();
    });

    /* ---------- in-place filtering ----------
       Every header control is a real <a> / GET-form, so this is pure
       enhancement: with JS off the same clicks navigate. We fetch the target's
       server-rendered HTML and swap the list (<main>) — and, for a control
       change, the header (#site) too — then push the URL. No row markup is ever
       built in JS, so the in-place result cannot drift from a full page load. A
       sequence token makes the latest request win and drops stale responses. */
    var swapSeq = 0;
    function localURL(href) {
      try { return new URL(href, location.href).origin === location.origin; }
      catch (e) { return false; }
    }
    function swapTo(href, mode, headerToo) {
      var seq = ++swapSeq;
      fetch(href, { headers: { "Accept": "text/html" } })
        .then(function (r) { return r.text(); })
        .then(function (t) {
          if (seq !== swapSeq) return;
          var doc = new DOMParser().parseFromString(t, "text/html");
          var newMain = doc.querySelector("main");
          if (!newMain || !mainEl) throw new Error("shape");
          mainEl.innerHTML = newMain.innerHTML;
          if (headerToo) {
            var newSite = doc.getElementById("site"), curSite = document.getElementById("site");
            if (newSite && curSite) { curSite.innerHTML = newSite.innerHTML; bindHeader(); }
          }
          body.dataset.view = doc.body.dataset.view || "";
          document.title = doc.title;
          if (mode === "push") history.pushState({ tl: 1 }, "", href);
          else if (mode === "replace") history.replaceState({ tl: 1 }, "", href);
          reinitTimeline();
          applyWidth();
          if (headerToo && mode === "push") window.scrollTo(0, 0);
        })
        .catch(function () { if (seq === swapSeq) location.assign(href); });
    }

    /* intercept the filter controls: header links (#site) + month headings +
       the saved-view "show them" link. Row links and all else navigate. */
    document.addEventListener("click", function (e) {
      if (e.metaKey || e.ctrlKey || e.shiftKey || e.altKey || e.button) return;
      var a = e.target.closest("a"); if (!a) return;
      if (!a.closest("#site") && !a.matches("main section > h2 > a") && a.id !== "unhide") return;
      var href = a.getAttribute("href");
      if (!href || !localURL(href)) return;
      e.preventDefault();
      swapTo(href, "push", true);
    });

    /* search: Enter submits, typing live-updates (debounced) — both swap only
       the list, leaving the focused field untouched. */
    function searchURL(form) {
      var u = new URL(form.getAttribute("action") || "/", location.origin);
      new FormData(form).forEach(function (v, k) {
        if (k === "q") { if (v.trim()) u.searchParams.set("q", v); }
        else if (k === "favorites") u.searchParams.set("favorites", "");
        else if (v) u.searchParams.set(k, v);
      });
      return u.pathname + u.search;
    }
    var qTimer = 0;
    document.addEventListener("submit", function (e) {
      var form = e.target.closest("#site form"); if (!form) return;
      e.preventDefault(); clearTimeout(qTimer);
      swapTo(searchURL(form), "push", false);
    });
    document.addEventListener("input", function (e) {
      if (!qInput || e.target !== qInput) return;
      var form = qInput.form; if (!form) return;
      clearTimeout(qTimer);
      qTimer = setTimeout(function () { swapTo(searchURL(form), "replace", false); }, 300);
    });
    window.addEventListener("popstate", function () {
      swapTo(location.pathname + location.search, "none", true);
    });

    /* keyboard */
    var visible = function () { return arts().filter(function (a) { var li = a.closest("li"); return !li || !li.hidden; }); };
    function move(delta) {
      var r = visible(); if (!r.length) return;
      var cur = r.findIndex(function (a) { return a.classList.contains("selected"); });
      var next = cur < 0 ? (delta > 0 ? 0 : r.length - 1) : Math.max(0, Math.min(r.length - 1, cur + delta));
      r.forEach(function (a) { a.classList.remove("selected"); });
      r[next].classList.add("selected");
      r[next].scrollIntoView({ block: "nearest" });
    }
    function clearSel() { document.querySelectorAll("main article.selected").forEach(function (a) { a.classList.remove("selected"); }); }
    document.addEventListener("keydown", function (e) {
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      if (qInput && e.target === qInput) {
        if (e.key === "Escape") {
          if (qInput.value) { qInput.value = ""; swapTo(searchURL(qInput.form), "push", false); }
          else qInput.blur();
        }
        return;
      }
      if (e.target.closest("input, textarea") || (help && help.open)) return;
      switch (e.key) {
        case "j": case "ArrowDown": e.preventDefault(); move(1); break;
        case "k": case "ArrowUp": e.preventDefault(); move(-1); break;
        case "Enter": { var a = document.querySelector("main article.selected h3 a"); if (a) a.click(); break; }
        case "b": { var b = document.querySelector("main article.selected aside > button"); if (b) b.click(); break; }
        case "/": e.preventDefault(); if (qInput) qInput.focus(); break;
        case "?": if (help) help.showModal(); break;
        case "Escape":
          /* clear a row selection first; otherwise drop any active filter. */
          if (document.querySelector("main article.selected")) clearSel();
          else if (location.pathname !== "/" || location.search) swapTo("/", "push", true);
          break;
      }
    });
  }
})();
"##;

// ---------------------------------------------------------------------------
// URL / query helpers
// ---------------------------------------------------------------------------

/// Percent-encode a query-component value (space → %20, RFC 3986 unreserved set
/// passes through). Keeps search URLs correct without a dependency.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// Build the query string for a view filter: "" when default, else
/// "?grade=notable&favorites&q=…". `favorites` is a bare presence flag.
fn query_string(view: &ViewFilter) -> String {
    let mut parts: Vec<String> = Vec::new();
    if view.notable {
        parts.push(format!("grade={}", view.grade_word()));
    }
    if view.fav {
        parts.push("favorites".to_string());
    }
    if let Some(ref q) = view.q {
        parts.push(format!("q={}", percent_encode(q)));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("?{}", parts.join("&"))
    }
}

/// A raw (un-escaped) URL: path + composed query string.
fn make_url(path: &str, view: &ViewFilter) -> String {
    format!("{}{}", path, query_string(view))
}

// ---------------------------------------------------------------------------
// Shared site header (the timeline's header, whole) — one component, both pages
// ---------------------------------------------------------------------------

/// The active date scope, shown as a removable chip in the header.
pub struct DateScope {
    /// Human-readable range, e.g. "March 2026" or "March 25, 2026".
    pub label: String,
    /// The path to return to when the date is cleared — the remaining tag path,
    /// or "/" when only the date was active. View filters are re-applied to it.
    pub clear_path: String,
}

/// Everything the shared header needs to render itself and reflect the request.
pub struct HeaderContext<'a> {
    /// Statistics for the tag cloud, built from all public entries.
    pub cloud: &'a CloudStats,
    /// The single active topic (grey pill), if the timeline is filtered to one.
    pub active_tag: Option<&'a str>,
    /// The current view filter (grade / favorites / search).
    pub view: &'a ViewFilter,
    /// The path portion the controls compose their queries onto (e.g. "/",
    /// "/+design"). Cloud topics always link from root.
    pub base_path: &'a str,
    /// The active date scope (removable chip), if the timeline is date-filtered.
    pub date_scope: Option<DateScope>,
    /// The tag-only portion of the path (e.g. "/+design" or ""), onto which
    /// month links graft a date so they preserve the active topic.
    pub path_tags: &'a str,
    /// Whether this is the reader's saved-bookmarks view.
    pub saved_view: bool,
}

impl<'a> HeaderContext<'a> {
    /// A default (unfiltered) header — used on entry/image pages, where every
    /// control simply links to the timeline with that filter applied.
    fn plain(cloud: &'a CloudStats, view: &'a ViewFilter) -> Self {
        HeaderContext {
            cloud,
            active_tag: None,
            view,
            base_path: "/",
            date_scope: None,
            path_tags: "",
            saved_view: false,
        }
    }
}

const BOOKMARK_SVG: &str =
    r#"<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6.5 3.5h11V21l-5.5-4-5.5 4z"/></svg>"#;

/// Topics in cloud order: alphabetical by name (case-insensitive), stable — a
/// topic never moves between visits, so the cloud is predictable rather than a
/// shuffling mass. Size still tracks entry count and ink still tracks recency;
/// only the left-to-right order is fixed.
fn cloud_order(tags: &[TagStat]) -> Vec<&TagStat> {
    let mut ordered: Vec<&TagStat> = tags.iter().collect();
    ordered.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.name.cmp(&b.name))
    });
    ordered
}

fn render_cloud(ctx: &HeaderContext) -> String {
    let mut out = String::new();
    for tag in cloud_order(&ctx.cloud.tags) {
        let is_active = ctx.active_tag == Some(tag.name.as_str());
        // Clicking the active topic clears it; any other selects it (from root),
        // preserving the current grade/favorites/search.
        let href = if is_active {
            make_url("/", ctx.view)
        } else {
            make_url(&format!("/+{}", tag.name), ctx.view)
        };
        let size = ctx.cloud.size_tier(tag.count);
        let recency = ctx.cloud.recency(tag.last_active);
        let entries_word = if tag.count == 1 { "entry" } else { "entries" };
        // Size (tier), recency (color+weight), and Finder color all ride data-*
        // attributes the stylesheet targets -- no inline `style` (CSP).
        out.push_str(&format!(
            r#"<a href="{href}" data-tag="{tag}" data-tag-color="{color}" data-size="{size}" data-recency="{recency}" aria-current="{cur}" title="{count} {word}, last active {last}">{tag} <small>{count}</small></a>"#,
            href = html_escape(&href),
            tag = html_escape(&tag.name),
            cur = if is_active { "true" } else { "false" },
            color = tag.color,
            size = size,
            recency = recency,
            count = tag.count,
            word = entries_word,
            last = tag.last_active.format("%Y-%m-%d"),
        ));
    }
    out
}

/// Render the whole shared header: cloud + the one quiet controls line.
fn render_site_header(ctx: &HeaderContext) -> String {
    let view = ctx.view;

    // Grade segmented control: two links (everything | notable) that set the
    // grade floor while preserving favorites/search; the thumb + stepped bars
    // reflect the state. The word doubles as the `data-grade` hook.
    let grade_link = |on: bool, label: &str| {
        let target = ViewFilter { notable: on, fav: view.fav, q: view.q.clone() };
        format!(
            r#"<a href="{href}" data-grade="{label}" aria-current="{cur}">{label}</a>"#,
            href = html_escape(&make_url(ctx.base_path, &target)),
            cur = if view.notable == on { "true" } else { "false" },
            label = label,
        )
    };
    // Two segments, so the thumb rests at the left edge or the halfway mark.
    // Its position rides a `data-notable` attribute (CSS-driven) rather than an
    // inline `style`, so the control stays clean under `style-src 'self'`.
    let thumb_attr = if view.notable { " data-notable" } else { "" };
    // The short bar is always lit ("everything"); the tall one lights at "notable".
    let bar_notable = if view.notable { " data-on" } else { "" };
    let grade_tip = if view.notable {
        "Showing notable and better — graded top 50%"
    } else {
        "Showing everything"
    };

    // Favorites toggle (independent axis): flips the fav flag, preserves the rest.
    let fav_target = ViewFilter { notable: view.notable, fav: !view.fav, q: view.q.clone() };
    let fav_href = html_escape(&make_url(ctx.base_path, &fav_target));

    // Saved-bookmarks link: to /saved, or back to root when already there.
    let saved_href = if ctx.saved_view { "/" } else { "/saved" };
    let saved_current = if ctx.saved_view { "page" } else { "false" };

    // Search preserves grade/favorites via hidden fields; action is the path.
    // (A form can't emit a valueless key, so `favorites=` stands in for the
    // bare `?favorites` the links use — the parser treats both as presence.)
    let mut hidden = String::new();
    if view.notable {
        hidden.push_str(&format!(
            r#"<input type="hidden" name="grade" value="{}">"#,
            view.grade_word()
        ));
    }
    if view.fav {
        hidden.push_str(r#"<input type="hidden" name="favorites" value="">"#);
    }
    let q_value = view.q.as_deref().map(html_escape).unwrap_or_default();
    let action = if ctx.base_path.is_empty() { "/" } else { ctx.base_path };

    // Active date scope: a quiet removable chip under the cloud. The × returns
    // to the tag path (or root), keeping the current grade/favorites/search.
    let scope = match &ctx.date_scope {
        Some(s) => format!(
            r#"<p id="scope"><span>{label}</span><a href="{clear}" aria-label="Clear date filter" title="Clear date filter">&times;</a></p>"#,
            label = html_escape(&s.label),
            clear = html_escape(&make_url(&s.clear_path, view)),
        ),
        None => String::new(),
    };

    format!(
        r#"<header id="site">
<nav id="cloud" aria-label="Topics">{cloud}</nav>
{scope}
<form method="get" action="{action}">
<a href="{saved_href}" id="navsaved" hidden aria-current="{saved_current}" aria-label="Saved for later">{bookmark}<i aria-hidden="true">&times;</i><output>0</output></a>
<p aria-label="How much to show">
<svg viewBox="0 0 24 24"><title>{tip}</title><rect x="5.5" y="12" width="5.5" height="10" rx="2.4" data-on/><rect x="13" y="4" width="5.5" height="18" rx="2.4"{b1}/></svg>
<span><u{thumb_attr}></u>{everything}{notable}</span>
<a href="{fav_href}" id="favonly" aria-current="{fav_cur}"><b>&#9733;</b> favorites</a>
</p>
<search>
<button type="submit" id="qbtn" aria-label="Search"><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="10" cy="10" r="7"/><path d="M15.2 15.2 21.5 21.5"/></svg></button>
<input type="search" name="q" id="q" value="{q_value}" placeholder="Search" aria-label="Search entries">
</search>
{hidden}
</form>
</header>"#,
        cloud = render_cloud(ctx),
        scope = scope,
        action = html_escape(action),
        saved_href = saved_href,
        saved_current = saved_current,
        bookmark = BOOKMARK_SVG,
        tip = grade_tip,
        b1 = bar_notable,
        thumb_attr = thumb_attr,
        everything = grade_link(false, "everything"),
        notable = grade_link(true, "notable"),
        fav_href = fav_href,
        fav_cur = if view.fav { "true" } else { "false" },
        q_value = q_value,
        hidden = hidden,
    )
}

// ---------------------------------------------------------------------------
// Page shell
// ---------------------------------------------------------------------------

/// Wrap page content in the full HTML document.
/// `page_kind` is "timeline" | "entry" | "plain"; `saved_view` marks /saved.
fn page_shell(title: &str, body: &str, page_kind: &str, saved_view: bool) -> String {
    let view_attr = if saved_view { r#" data-view="saved""# } else { "" };
    // The reading-typeface toggle is an entry-page affordance only (the serif
    // body is what it swaps); JS wires the `t` key and updates the label.
    let typeface = if page_kind == "entry" {
        r#" <i aria-hidden="true">&middot;</i> <button type="button" id="typeface" title="Reading typeface (press t)"><kbd>t</kbd> <span>serif</span></button>"#
    } else {
        ""
    };
    // The help dialog lists only the shortcuts that actually work on this page:
    // j/k/Enter/b are timeline-only; t (typeface) is entry-only.
    let shortcuts = if page_kind == "entry" {
        r#"<dt><kbd>t</kbd></dt><dd>reading typeface (serif / sans)</dd>
<dt><kbd>/</kbd></dt><dd>search</dd>
<dt><kbd>Esc</kbd></dt><dd>close</dd>
<dt><kbd>?</kbd></dt><dd>this help</dd>"#
    } else {
        r#"<dt><kbd>j</kbd> / <kbd>k</kbd></dt><dd>select next / previous</dd>
<dt><kbd>Enter</kbd></dt><dd>open the selected entry</dd>
<dt><kbd>b</kbd></dt><dd>save for later</dd>
<dt><kbd>/</kbd></dt><dd>search</dd>
<dt><kbd>Esc</kbd></dt><dd>clear, close</dd>
<dt><kbd>?</kbd></dt><dd>this help</dd>"#
    };
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<link rel="stylesheet" href="{css_href}">
<script src="{boot_href}"></script>
</head>
<body data-page="{page_kind}"{view_attr}>
<div id="grip" role="slider" tabindex="0" aria-label="Reading width" aria-orientation="horizontal" aria-valuemin="480" aria-valuemax="1160" aria-valuenow="736" title="Drag to set reading width. Double-click resets."></div>
<div id="readout" aria-hidden="true"></div>
{body}
<footer><button type="button" id="helpbtn"><kbd>?</kbd> shortcuts</button>{typeface}</footer>
<dialog id="help" aria-label="Keyboard shortcuts">
<h2>Keyboard</h2>
<dl>{shortcuts}</dl>
<p><b>&#9733;</b> marks my favorites. <b>{bookmark}</b> Bookmarks are yours &mdash; they never leave this browser.</p>
</dialog>
<script src="{site_href}"></script>
</body>
</html>"#,
        title = html_escape(title),
        css_href = crate::assets::SITE_CSS.url(),
        boot_href = crate::assets::BOOT_JS.url(),
        site_href = crate::assets::SITE_JS.url(),
        page_kind = page_kind,
        view_attr = view_attr,
        body = body,
        typeface = typeface,
        shortcuts = shortcuts,
        bookmark = BOOKMARK_SVG,
    )
}

// ---------------------------------------------------------------------------
// Date / label / href helpers
// ---------------------------------------------------------------------------

fn format_datetime_attr(ts: &chrono::NaiveDateTime) -> String {
    ts.format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// A canonical address: one decoded path. Any time-of-day disambiguator is a
/// path segment inline (`/2026/03/12/191430[/label]`), not a `?time=` query.
/// Dates are a slash hierarchy (`/2026/03/12`), precision-aware.
pub struct Canonical {
    /// Decoded path, e.g. `/label`, `/2026/03/12/label`, `/2026/03/12/191430`,
    /// or `/2026/03/12/191430/label` (time segment on a same-day collision).
    pub path: String,
}

/// Every entry that claims a name in the flat namespace: those whose slug matches,
/// plus those that carry it as an `alias <name>/` marker (compared on the slug —
/// so `Fog Over The Bay`, `fog-over-the-bay`, and `/FOG%20OVER...` all collide).
/// Errored posts do not claim a name — they resolve to an error page, never to
/// content under a stable URL.
fn name_claimants<'a>(name: &str, all_entries: &[&'a Entry]) -> Vec<&'a Entry> {
    let target = match slug(name) {
        Some(s) => s,
        None => return Vec::new(),
    };
    all_entries
        .iter()
        .copied()
        .filter(|e| e.error.is_none())
        .filter(|e| {
            e.slug.as_deref() == Some(target.as_str())
                || e.aliases.iter().any(|a| slug(a).as_deref() == Some(target.as_str()))
        })
        .collect()
}

/// The single oldest entry in a set, or `None` on a publish-date tie.
fn oldest_unique<'a>(entries: &[&'a Entry]) -> Option<&'a Entry> {
    let oldest = entries.iter().map(|e| e.timestamp).min()?;
    let mut at_oldest = entries.iter().filter(|e| e.timestamp == oldest);
    let first = *at_oldest.next()?;
    if at_oldest.next().is_some() {
        None
    } else {
        Some(first)
    }
}

/// The entry that owns the bare `/name` — the OLDEST claim wins, so a URL's
/// meaning never changes once established (a newer post named `IMG_4392` can
/// never silently retarget an old `/IMG_4392` link). `None` when there is no
/// claimant, or when the oldest publish date is itself tied (fail closed).
pub fn name_owner<'a>(name: &str, all_entries: &[&'a Entry]) -> Option<&'a Entry> {
    let target = slug(name)?;
    // Reserved segments (saved / numeric) belong to the router, never a post — the
    // bare URL never resolves to a post whose slug is reserved.
    if is_reserved_slug(&target) {
        return None;
    }
    oldest_unique(&name_claimants(&target, all_entries))
}

/// The other entries that claim the same bare name as `entry` (a folder vs a
/// file, an old vs new version, an alias) — surfaced on the winning page so a
/// shared name is never silent. Empty in the common unique-name case.
fn name_shares<'a>(entry: &Entry, all_entries: &[&'a Entry]) -> Vec<&'a Entry> {
    let slug = match &entry.slug {
        Some(s) => s,
        None => return Vec::new(),
    };
    name_claimants(slug, all_entries)
        .into_iter()
        .filter(|e| !std::ptr::eq(*e, entry))
        .collect()
}

/// The one canonical (decoded) address for an entry.
///
/// - A unique label owns `/label`.
/// - When several posts claim a name (a folder and a file, an old and a new
///   version, an alias), the OLDEST claim owns the bare `/label`; the others
///   carry the shortest date that tells them apart: `/2026/03/12/label` when the
///   day suffices, else the day plus `?time=133513` for a same-day collision.
/// - Unlabeled entries live at their day (`/2026/03/12`) plus `?time=`.
///
/// The `path` is decoded (compare against decoded request paths); run it
/// through [`encode_path`] before emitting into an href or Location header.
pub fn canonical(entry: &Entry, all_entries: &[&Entry]) -> Canonical {
    let base = entry.timestamp.date_path();
    // An unlabeled/reserved post is addressed at its date path plus a time segment
    // when it has a real time (minute/second precision); coarse dates have none.
    let dated = || match entry.timestamp.time_seg() {
        Some(hms) => format!("{}/{}", base, hms),
        None => base.clone(),
    };

    // The address is the slug. An unlabeled post (no slug) or a reserved slug
    // (saved / numeric — the router owns those) is addressed at its date path.
    let slug = match &entry.slug {
        Some(s) if !is_reserved_slug(s) => s,
        _ => return Canonical { path: dated() },
    };

    let claimants = name_claimants(slug, all_entries);
    if claimants.len() <= 1 {
        return Canonical { path: format!("/{}", slug) };
    }

    // The oldest claim owns the bare slug; this entry owns it only if it is
    // that unique-oldest claimant (an alias or a tie sends it to a date path).
    let owns_bare = name_owner(slug, all_entries).map_or(false, |o| std::ptr::eq(o, entry));
    if owns_bare {
        return Canonical { path: format!("/{}", slug) };
    }

    let day = entry.timestamp.short_date();
    let same_day = claimants.iter().filter(|e| e.timestamp.short_date() == day).count();
    if same_day == 1 {
        Canonical { path: format!("{}/{}", base, slug) }
    } else {
        // Same-day collision: the time segment disambiguates, before the slug.
        match entry.timestamp.time_seg() {
            Some(hms) => Canonical { path: format!("{}/{}/{}", base, hms, slug) },
            None => Canonical { path: format!("{}/{}", base, slug) },
        }
    }
}

/// Compose a ready-to-emit href from a decoded path, appending an optional
/// raw-file extension and encoding as it goes. Any time disambiguator is already
/// a path segment inside `path`.
fn compose_href(path: &str, ext: Option<&str>) -> String {
    let mut decoded = path.to_string();
    if let Some(e) = ext {
        if !e.is_empty() {
            decoded.push('.');
            decoded.push_str(e);
        }
    }
    encode_path(&decoded)
}

/// The full addressable href for an entry (encoded canonical path).
fn canonical_href(entry: &Entry, all_entries: &[&Entry]) -> String {
    compose_href(&canonical(entry, all_entries).path, None)
}

/// The raw-bytes href for an entry: canonical path + extension, so the parser
/// reads the extension back off the last segment. Folders have none.
fn canonical_raw_href(entry: &Entry, all_entries: &[&Entry]) -> String {
    let ext = (!entry.extension.is_empty()).then_some(entry.extension.as_str());
    compose_href(&canonical(entry, all_entries).path, ext)
}

/// The full canonical location for a redirect: encoded path + the view filters
/// (grade/favorites/search). Everything non-canonical is dropped, so every alias
/// and non-canonical URL 301s onto one address.
pub fn canonical_location(entry: &Entry, all_entries: &[&Entry], view: &ViewFilter) -> String {
    let c = canonical(entry, all_entries);
    let mut parts: Vec<String> = Vec::new();
    if view.notable {
        parts.push(format!("grade={}", view.grade_word()));
    }
    if view.fav {
        parts.push("favorites".to_string());
    }
    if let Some(ref q) = view.q {
        parts.push(format!("q={}", percent_encode(q)));
    }
    let mut out = encode_path(&c.path);
    if !parts.is_empty() {
        out.push('?');
        out.push_str(&parts.join("&"));
    }
    out
}

/// Percent-encode a decoded path for emission (href attribute, Location
/// header): each segment is encoded, slashes survive.
pub fn encode_path(path: &str) -> String {
    path.split('/')
        .map(percent_encode)
        .collect::<Vec<_>>()
        .join("/")
}

/// A stable per-entry key for the reader's localStorage bookmarks: the canonical
/// address (its full path, including any time segment), unique even across
/// entries sharing a label.
fn canonical_key(c: &Canonical) -> String {
    c.path.trim_start_matches('/').to_string()
}

/// Render an entry's topical tags as a vertical rail nav with Finder-color dots.
fn rail_tags(entry: &Entry) -> String {
    let pills: String = entry
        .topical_tags()
        .map(|t| {
            format!(
                r#"<a href="/+{tag}" data-tag="{tag}" data-tag-color="{color}">{tag}</a>"#,
                tag = html_escape(&t.name),
                color = t.color,
            )
        })
        .collect::<Vec<_>>()
        .join("");
    if pills.is_empty() {
        String::new()
    } else {
        format!(r#"<nav aria-label="Tags">{}</nav>"#, pills)
    }
}

// ---------------------------------------------------------------------------
// Timeline page
// ---------------------------------------------------------------------------

/// The hairline quality meter, shared by timeline rows and the entry-post
/// header. Empty until an entry is graded (no grading flow yet), so callers
/// can splice it unconditionally. `grade` is a percentile in [0, 1]: fill
/// width is `grade`, the title reads the top percentage (`1 - grade`).
fn render_meter(grade: Option<f32>) -> String {
    match grade {
        Some(q) => {
            let pct = ((1.0 - q) * 100.0).round().max(1.0) as i32;
            // Fill width rides a data-fill bucket (nearest 5%) the stylesheet
            // targets, so no inline `style` is needed (CSP `style-src 'self'`).
            let fill = ((q * 20.0).round() as i32 * 5).clamp(0, 100);
            format!(
                r#"<span class="meter" title="Graded top {pct}%" aria-label="Graded top {pct}%"><i data-fill="{fill}"></i></span>"#,
                pct = pct,
                fill = fill,
            )
        }
        None => String::new(),
    }
}

/// The display domain for a cite line: the URL's host, lowercased, `www.`-stripped,
/// port and userinfo dropped. Display-only — `link_url` was already vetted by the
/// scheme guard (and, before any fetch, the SSRF guard), so this never gates a
/// request; it only turns a stored `http(s)` URL into a readable source label.
fn link_domain(url: &str) -> String {
    let after = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let authority = after.split(['/', '?', '#']).next().unwrap_or(after);
    // Drop userinfo (`user@host`) and port (`host:443`); keep only the host.
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = host.split(':').next().unwrap_or(host).trim().to_lowercase();
    host.strip_prefix("www.").unwrap_or(&host).to_string()
}

/// A deterministic stand-in favicon (`post-model.md` §4, `static/link-rows-mockup
/// .html`): the domain's first letter on a colored tile whose color is a stable
/// hash of the domain. A real favicon fetch is deliberately deferred — a CSS-only
/// tile makes zero third-party requests, so it can never leak a reader's IP to the
/// destination. The color is bucketed into a `data-tile` attribute (a static CSS
/// rule, not an inline `style`) to keep the Content-Security-Policy clean.
fn favicon_tile(domain: &str) -> String {
    let letter = domain
        .chars()
        .find(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .unwrap_or('#');
    // A cheap, stable byte-sum bucket — no cryptographic property is needed, only
    // that the same domain always lands on the same color.
    let bucket = domain.bytes().fold(0u16, |a, b| a.wrapping_add(b as u16)) % 6;
    format!(
        r#"<i data-tile="{bucket}">{letter}</i>"#,
        bucket = bucket,
        letter = html_escape(&letter.to_string()),
    )
}

/// The external `<cite>` line for a post's outbound destination (`post-model.md`
/// §4, `static/link-rows-mockup.html`): a stand-in favicon, the destination's own
/// title, and its domain, linking out with `rel="noreferrer"`. `http://` targets
/// get a caution flag purely in CSS (the scheme guard already refused every
/// non-`http(s)` destination, so `link_url` is always safe to link here).
///
/// `own_title` is the post's own heading text, used for two degrade rules:
/// - fetch never resolved a title → show the bare domain (favicon + domain only);
/// - the destination's title equals our heading → drop it so it isn't said twice.
/// Pass `None` to always show the title (a bare link that promotes the cite into
/// its own heading, where there is no separate heading to duplicate).
fn cite_line(entry: &Entry, own_title: Option<&str>) -> String {
    let url = match entry.link_url.as_deref() {
        Some(u) => u,
        None => return String::new(),
    };
    let domain = link_domain(url);
    let tile = favicon_tile(&domain);

    let title = entry.link_title.as_deref().filter(|t| !t.is_empty());
    let duplicate =
        matches!((title, own_title), (Some(t), Some(o)) if t.eq_ignore_ascii_case(o.trim()));
    let title_html = match title {
        Some(t) if !duplicate => format!("<b>{}</b>", html_escape(t)),
        _ => String::new(),
    };

    format!(
        r#"<cite><a href="{href}" rel="noreferrer">{tile}{title}<span>{domain}</span></a></cite>"#,
        href = html_escape(url),
        tile = tile,
        title = title_html,
        domain = html_escape(&domain),
    )
}

/// Render one timeline row.
fn render_row(entry: &Entry, all_entries: &[&Entry]) -> String {
    let datetime = entry.timestamp.iso_attr();
    let date = entry.timestamp.short_date();
    let canon = canonical(entry, all_entries);
    let href = compose_href(&canon.path, None);
    let key = canonical_key(&canon);

    // A post that scanned wrong still shows — loud, never hidden — as an errored
    // row that links to its error page.
    if entry.error.is_some() {
        let label = entry.label.as_deref().unwrap_or("(unnamed)");
        return format!(
            r#"<li><article data-key="{key}" data-error>
<aside>
<time datetime="{datetime}">{date}</time>
<b title="This post needs attention">&#9888;</b>
</aside>
<div><h3><a href="{href}">{label} <small>needs attention</small></a></h3></div>
</article></li>"#,
            key = html_escape(&key),
            datetime = html_escape(&datetime),
            date = html_escape(&date),
            href = html_escape(&href),
            label = html_escape(label),
        );
    }

    let star = if entry.is_favorite() {
        r#"<b title="A favorite of mine">&#9733;</b>"#.to_string()
    } else {
        String::new()
    };

    // The meter only appears once an entry is graded (no grading flow yet).
    let meter = render_meter(entry.grade);

    let title = match entry.display_label.as_deref().or(entry.label.as_deref()) {
        Some(label) => format!(
            "{}<small>{}</small>",
            html_escape(label),
            html_escape(&ext_suffix(entry))
        ),
        None => format!(r#"<i>(untitled)</i><small>{}</small>"#, html_escape(&ext_suffix(entry))),
    };

    // The one-line description under the title (text posts only).
    let excerpt = match entry.excerpt.as_deref() {
        Some(e) => format!("<p>{}</p>", html_escape(e)),
        None => String::new(),
    };

    // A subtle, expandable note on rows that carry archived revisions.
    let revisions = if entry.revisions.is_empty() {
        String::new()
    } else {
        let n = entry.revisions.len();
        format!(
            r#"<details><summary>{n} earlier {word}</summary><ol>{items}</ol></details>"#,
            n = n,
            word = if n == 1 { "revision" } else { "revisions" },
            items = revision_items(entry),
        )
    };

    // When other posts slug to this same address, surface them inline so the
    // collision is discoverable, never silent — each links to its own
    // date-disambiguated (slug-carrying) URL. Same `<details>` idiom as revisions.
    let shares = {
        let others = name_shares(entry, all_entries);
        if others.is_empty() {
            String::new()
        } else {
            let n = others.len();
            let items: String = others
                .iter()
                .map(|e| {
                    let dt = e.timestamp.iso_attr();
                    let d = e.timestamp.short_date();
                    let t = e.display_label.as_deref().or(e.label.as_deref()).unwrap_or("(untitled)");
                    format!(
                        r#"<li><a href="{href}"><time datetime="{dt}">{d}</time> {t}</a></li>"#,
                        href = html_escape(&canonical_href(e, all_entries)),
                        dt = html_escape(&dt),
                        d = html_escape(&d),
                        t = html_escape(t),
                    )
                })
                .collect();
            format!(
                r#"<details><summary>{n} other{s} share this address</summary><ol>{items}</ol></details>"#,
                n = n,
                s = if n == 1 { "" } else { "s" },
                items = items,
            )
        }
    };

    // The content column varies with the post's link axis (post-model.md §4):
    //   1. no destination        -> heading (our page) + excerpt
    //   2/5. cites a source       -> heading (our page) + excerpt + cite
    //   3. IS a link, labeled     -> heading (our page) + cite, no prose body
    //   4. IS a link, no label    -> the destination's headline promotes into the
    //                                heading, with a quiet permalink to our card
    let own_title = entry.display_label.as_deref().or(entry.label.as_deref());
    let is_link_post = entry.kind() == "link";

    let content = if is_link_post && entry.label.is_none() {
        // Case 4: nothing of ours to title, so the target's headline is the
        // headline (links out), and a quiet permalink keeps our card reachable.
        let cite = cite_line(entry, None);
        if cite.is_empty() {
            // A link post with no resolvable destination should never occur (the
            // scanner sets kind=link only when link_url is Some); fail safe to the
            // ordinary heading rather than emit an empty row.
            format!(
                r#"<h3><a href="{href}">{title}</a></h3>{revisions}{shares}"#,
                href = html_escape(&href),
                title = title,
                revisions = revisions,
                shares = shares,
            )
        } else {
            format!(
                r#"<h3>{cite}</h3><a data-perma href="{href}">{perma}</a>{revisions}{shares}"#,
                cite = cite,
                href = html_escape(&href),
                perma = html_escape(href.trim_start_matches('/')),
                revisions = revisions,
                shares = shares,
            )
        }
    } else {
        // Cases 1/2/3/5: our own heading links to our page; a cite (when the post
        // carries a destination) and a prose excerpt (a content post, never a bare
        // link) follow it.
        let cite = if entry.link_url.is_some() {
            cite_line(entry, own_title)
        } else {
            String::new()
        };
        let body = if is_link_post { String::new() } else { excerpt };
        format!(
            r#"<h3><a href="{href}">{title}</a></h3>{body}{cite}{revisions}{shares}"#,
            href = html_escape(&href),
            title = title,
            body = body,
            cite = cite,
            revisions = revisions,
            shares = shares,
        )
    };

    format!(
        r#"<li><article data-key="{key}">
<aside>
<time datetime="{datetime}">{date}</time>
{star}<button type="button" aria-pressed="false" aria-label="Save for later (stays in this browser)" title="Save for later &mdash; stays in this browser">{bookmark}</button>
{meter}{tags}
</aside>
<div>{content}</div>
</article></li>"#,
        key = html_escape(&key),
        datetime = html_escape(&datetime),
        date = html_escape(&date),
        star = star,
        bookmark = BOOKMARK_SVG,
        meter = meter,
        tags = rail_tags(entry),
        content = content,
    )
}

/// The type suffix shown at the title's end: ".md", ".jpg", or "/" for folders.
fn ext_suffix(entry: &Entry) -> String {
    match entry.extension.as_str() {
        "" | "/" => "/".to_string(),
        ext => format!(".{}", ext),
    }
}

/// Render the timeline: shared header + month-grouped rows.
pub fn timeline_page(
    entries: &[&Entry],
    all_entries: &[&Entry],
    ctx: &HeaderContext,
    filter_desc: &str,
) -> String {
    let title = if filter_desc.is_empty() {
        "esko.bar".to_string()
    } else {
        format!("esko.bar — {}", filter_desc)
    };

    let mut rows = String::new();
    let mut current_month = String::new();
    let mut open_section = false;
    for entry in entries {
        // Group by the post's precision: dated posts by "Month Year", year-only
        // (and BCE) posts by their year label ("3000 BCE").
        let month = entry.timestamp.group_label();
        if month != current_month {
            if open_section {
                rows.push_str("</ul></section>");
            }
            // The heading links into that group's scope (the month, or the year
            // for year-precision posts), grafting the date onto any active tag
            // path and keeping the current view filters.
            let month_path = format!("{}{}", entry.timestamp.group_path(), ctx.path_tags);
            rows.push_str(&format!(
                r#"<section><h2><a href="{href}">{label}</a></h2><ul>"#,
                href = html_escape(&make_url(&month_path, ctx.view)),
                label = html_escape(&month),
            ));
            current_month = month;
            open_section = true;
        }
        rows.push_str(&render_row(entry, all_entries));
    }
    if open_section {
        rows.push_str("</ul></section>");
    }

    // Empty states. In the saved view the server sends the full set (the reader's
    // bookmarks live only in their browser), so the "nothing saved" message is a
    // JS-toggled element rather than a server verdict.
    if ctx.saved_view {
        rows.push_str(
            r#"<p class="empty" id="saved-empty" hidden>Nothing saved yet &mdash; the bookmark on any entry keeps it here, in this browser.</p>"#,
        );
    } else if rows.is_empty() {
        rows.push_str(r#"<p class="empty">Nothing matches.</p>"#);
    }

    let body = format!("{}\n<main>{}</main>", render_site_header(ctx), rows);
    page_shell(&title, &body, "timeline", ctx.saved_view)
}

// ---------------------------------------------------------------------------
// Entry / image / 404 pages
// ---------------------------------------------------------------------------

/// The crumbs row on entry pages: back to the timeline, up to the menu.
fn crumbs() -> &'static str {
    r#"<nav id="crumbs" aria-label="Site">
<a href="/">&#8592; Timeline</a>
<button type="button" id="tomenu">&#8593; Menu</button>
</nav>"#
}

/// The entry-post header: mono date, quality meter, Finder-color dot tags.
/// Shared by the document and image entry pages.
fn post_header(entry: &Entry) -> String {
    let datetime = entry.timestamp.iso_attr();
    let date = entry.timestamp.long_date();
    format!(
        r#"<header>
<time datetime="{datetime}">{date}</time>
{meter}{tags}
</header>"#,
        datetime = html_escape(&datetime),
        date = html_escape(&date),
        meter = render_meter(entry.grade),
        tags = post_tags(entry),
    )
}

/// The Continue block at the foot of a post: a teaser link to the next-older
/// entry, so the reader can keep going. Empty when there is no next entry.
/// Without JS it is a plain link to the next entry's canonical page.
fn continue_nav(next: Option<&Entry>, all_entries: &[&Entry]) -> String {
    let next = match next {
        Some(n) => n,
        None => return String::new(),
    };
    let href = canonical_href(next, all_entries);
    let datetime = next.timestamp.iso_attr();
    let date = next.timestamp.short_date();
    let title = match next.display_label.as_deref().or(next.label.as_deref()) {
        Some(label) => html_escape(label),
        None => "(untitled)".to_string(),
    };
    format!(
        r#"<nav id="continue"><p>Continue</p><a href="{href}"><b>{title}</b><time datetime="{datetime}">{date}</time></a><label hidden><input type="checkbox" id="autoload"> keep loading as I scroll</label></nav>"#,
        href = html_escape(&href),
        title = title,
        datetime = html_escape(&datetime),
        date = html_escape(&date),
    )
}

/// The date-path address of one archived revision: the current post's slug at
/// the revision's own date, always with the time segment (revisions collide with
/// the current post and each other on the day, so the time always disambiguates).
/// A revision's date is a plain mtime, so its full `/Y/M/D/HHMMSS` segments all
/// come straight off it.
fn revision_href(slug: &str, rev: &Revision) -> String {
    let decoded = format!("{}/{}", rev.date.format("/%Y/%m/%d/%H%M%S"), slug);
    compose_href(&decoded, None)
}

/// The dated `<li>` links for a post's archived revisions, newest first.
fn revision_items(entry: &Entry) -> String {
    let label = entry.slug.as_deref().unwrap_or("");
    entry
        .revisions
        .iter()
        .map(|r| {
            format!(
                r#"<li><a href="{href}"><time datetime="{dt}">{date}</time></a></li>"#,
                href = html_escape(&revision_href(label, r)),
                dt = html_escape(&format_datetime_attr(&r.date)),
                date = html_escape(&r.date.format("%Y-%m-%d %H:%M").to_string()),
            )
        })
        .collect()
}

/// The revision nav on an entry page: a small, in-place list of archived earlier
/// states, each linking to its date-path URL. Empty when there are none.
fn revision_nav(entry: &Entry) -> String {
    if entry.revisions.is_empty() {
        return String::new();
    }
    let n = entry.revisions.len();
    format!(
        r#"<details id="revisions"><summary>{n} earlier {word}</summary><ol>{items}</ol></details>"#,
        n = n,
        word = if n == 1 { "revision" } else { "revisions" },
        items = revision_items(entry),
    )
}

/// The alias list on an entry page: the extra addresses this post answers to.
/// Never fully invisible (Finder cannot warn about `alias …/` markers), so the
/// server surfaces them here. Empty when there are none.
fn alias_list(entry: &Entry) -> String {
    if entry.aliases.is_empty() {
        return String::new();
    }
    let items: String = entry
        .aliases
        .iter()
        .map(|a| {
            format!(
                r#"<li><a href="{href}">/{name}</a></li>"#,
                href = html_escape(&encode_path(&format!("/{}", a))),
                name = html_escape(a),
            )
        })
        .collect();
    format!(r#"<aside id="aliases"><h2>Also at</h2><ul>{}</ul></aside>"#, items)
}

/// The name-share notice: when several posts claim this name, link the others so
/// the collision is visible, never silent. Empty in the common unique-name case.
fn name_share_notice(entry: &Entry, all_entries: &[&Entry]) -> String {
    let shares = name_shares(entry, all_entries);
    if shares.is_empty() {
        return String::new();
    }
    let label = entry.label.as_deref().unwrap_or("");
    let links: String = shares
        .iter()
        .map(|e| {
            let title = e.display_label.as_deref().or(e.label.as_deref()).unwrap_or("(untitled)");
            format!(
                r#"<li><a href="{href}"><time datetime="{dt}">{date}</time> {title}</a></li>"#,
                href = html_escape(&canonical_href(e, all_entries)),
                dt = html_escape(&e.timestamp.iso_attr()),
                date = html_escape(&e.timestamp.short_date()),
                title = html_escape(title),
            )
        })
        .collect();
    format!(
        r#"<aside id="nameshare"><p>The name <code>{label}</code> is also used by:</p><ul>{links}</ul></aside>"#,
        label = html_escape(label),
        links = links,
    )
}

/// A persistent notice on a post whose slug is reserved by the URL grammar: it
/// explains why the post is addressed at its date path rather than the bare name.
/// Empty unless the slug is reserved. Never auto-dismissed (the condition holds
/// as long as the name does).
fn reserved_name_notice(entry: &Entry) -> String {
    let slug = match &entry.slug {
        Some(s) if is_reserved_slug(s) => s,
        _ => return String::new(),
    };
    let label = entry.label.as_deref().unwrap_or("");
    let why = if slug == "saved" {
        "a site route"
    } else {
        "the year / date view"
    };
    format!(
        r#"<aside id="nameshare"><p>The name <code>{label}</code> reduces to <code>/{slug}</code>, which the site reserves for {why}. This post keeps its date-path address above and does not claim the bare name.</p></aside>"#,
        label = html_escape(label),
        slug = html_escape(slug),
        why = why,
    )
}

/// The extra blocks under a post's content: revision nav, alias list, the
/// name-share notice, and the reserved-name notice (each empty unless it applies).
fn post_extras(entry: &Entry, all_entries: &[&Entry]) -> String {
    format!(
        "{}{}{}{}",
        revision_nav(entry),
        alias_list(entry),
        name_share_notice(entry, all_entries),
        reserved_name_notice(entry),
    )
}

/// A fail-closed error page for a post that scanned wrong: it names the exact
/// conflicting paths and the one-line fix. Served with HTTP 500 by the router.
pub fn error_page(entry: &Entry, all_entries: &[&Entry]) -> String {
    let cloud = compute_cloud(all_entries);
    let view = ViewFilter::default();
    let ctx = HeaderContext::plain(&cloud, &view);
    let label = entry.label.as_deref().unwrap_or("(unnamed)");
    let (headline, detail) = match &entry.error {
        Some(e) => error_message(e),
        None => ("This post could not be resolved.".to_string(), String::new()),
    };

    let body = format!(
        r#"{header}
{crumbs}
<main>
<article id="post" data-error>
<header><time>error</time></header>
<section>
<h1>{label}</h1>
<p><strong>{headline}</strong></p>
{detail}
</section>
</article>
</main>"#,
        header = render_site_header(&ctx),
        crumbs = crumbs(),
        label = html_escape(label),
        headline = html_escape(&headline),
        detail = detail,
    );
    page_shell(&format!("esko.bar — error: {}", label), &body, "entry", false)
}

/// The headline and detail (already-escaped HTML) for each scan failure.
fn error_message(e: &PostError) -> (String, String) {
    match e {
        PostError::MultipleDateMarkers(names) => (
            "This post has more than one date-marker folder.".to_string(),
            list_fix("Keep exactly one date folder and remove the rest:", names),
        ),
        PostError::NoPrimary => (
            "This post folder has no content file.".to_string(),
            "<p>Add a primary file named the folder name or <code>index</code>.</p>".to_string(),
        ),
    }
}

/// An intro line plus the offending names as a `<ul><li><code>…</code></li>`.
fn list_fix(intro: &str, names: &[String]) -> String {
    let items: String = names
        .iter()
        .map(|n| format!("<li><code>{}</code></li>", html_escape(n)))
        .collect();
    format!("<p>{}</p><ul>{}</ul>", html_escape(intro), items)
}

/// Render a single entry page: shared header (linking to the timeline), crumbs,
/// then the post in plain typography, and a Continue teaser for the next entry.
pub fn entry_page(
    entry: &Entry,
    rendered_html: &str,
    all_entries: &[&Entry],
    next: Option<&Entry>,
) -> String {
    let cloud = compute_cloud(all_entries);
    let view = ViewFilter::default();
    let ctx = HeaderContext::plain(&cloud, &view);

    let label = entry
        .display_label
        .as_deref()
        .or(entry.label.as_deref())
        .unwrap_or("Untitled");
    let canon_href = canonical_href(entry, all_entries);
    let raw_href = canonical_raw_href(entry, all_entries);

    let body = format!(
        r#"{header}
{crumbs}
<main>
<article id="post" data-canonical="{canonical}" data-title="{data_title}">
{post_header}
<section>{content}</section>
{cite}
{attachments}
{extras}
<footer><a href="{raw_href}">source</a> <a href="/">timeline</a></footer>
</article>
{continue_nav}
</main>"#,
        header = render_site_header(&ctx),
        crumbs = crumbs(),
        canonical = html_escape(&canon_href),
        data_title = html_escape(label),
        post_header = post_header(entry),
        content = rendered_html,
        cite = cite_section(entry),
        attachments = attachments_section(entry, all_entries),
        extras = post_extras(entry, all_entries),
        raw_href = html_escape(&raw_href),
        continue_nav = continue_nav(next, all_entries),
    );

    page_shell(&format!("esko.bar — {}", label), &body, "entry", false)
}

/// The label to title a standalone `.html` post with.
fn standalone_label(entry: &Entry) -> &str {
    entry
        .display_label
        .as_deref()
        .or(entry.label.as_deref())
        .unwrap_or("Untitled")
}

/// Model A (default): a standalone `.html` post embedded in the normal blog
/// shell. Its body is a sandboxed iframe over the `?embed` copy of the asset
/// (which carries the height reporter); the site JS sizes the frame to fit. An
/// "Expand" link opens the fullscreen view (B). The iframe is `allow-scripts
/// allow-popups` only -- never `allow-same-origin` -- so the document stays in an
/// opaque origin, unable to reach this page or its storage.
pub fn standalone_embed_page(entry: &Entry, all_entries: &[&Entry], next: Option<&Entry>) -> String {
    let cloud = compute_cloud(all_entries);
    let view = ViewFilter::default();
    let ctx = HeaderContext::plain(&cloud, &view);

    let label = standalone_label(entry);
    let canon_href = canonical_href(entry, all_entries);
    let raw_href = canonical_raw_href(entry, all_entries);
    let embed_src = format!("{}?embed", raw_href);
    let full_href = format!("{}?fullscreen", canon_href);

    let body = format!(
        r#"{header}
{crumbs}
<main>
<article id="post" data-canonical="{canonical}" data-title="{data_title}">
{post_header}
<div id="stage"><iframe id="se-frame" src="{embed_src}" sandbox="allow-scripts allow-popups" title="{data_title}" loading="lazy"></iframe></div>
<p id="se-controls"><a href="{full_href}">Expand</a> <a href="{raw_href}">view source</a></p>
{extras}
<footer><a href="{raw_href}">source</a> <a href="/">timeline</a></footer>
</article>
{continue_nav}
</main>"#,
        header = render_site_header(&ctx),
        crumbs = crumbs(),
        canonical = html_escape(&canon_href),
        data_title = html_escape(label),
        post_header = post_header(entry),
        embed_src = html_escape(&embed_src),
        full_href = html_escape(&full_href),
        raw_href = html_escape(&raw_href),
        extras = post_extras(entry, all_entries),
        continue_nav = continue_nav(next, all_entries),
    );

    page_shell(&format!("esko.bar — {}", label), &body, "entry", false)
}

/// Model B (`?fullscreen`): a standalone `.html` post given the whole viewport
/// under a slim top bar (site mark, back to the post, and "show only the HTML"
/// -> the byte-exact asset C). The frame scrolls itself, so no reporter is used.
/// A bespoke minimal chrome rather than the blog shell; it still links the shared
/// stylesheet and inherits the strict page CSP (which admits the same-origin
/// iframe via `frame-src 'self'`).
pub fn standalone_fullscreen_page(entry: &Entry, all_entries: &[&Entry]) -> String {
    let label = standalone_label(entry);
    let canon_href = canonical_href(entry, all_entries);
    let raw_href = canonical_raw_href(entry, all_entries);

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>esko.bar &mdash; {title}</title>
<link rel="stylesheet" href="{css_href}">
</head>
<body data-page="fullscreen">
<header id="sebar">
<a href="/" id="sebar-mark">esko.bar</a>
<span id="sebar-title">{title}</span>
<span id="sebar-actions"><a href="{canon}">&larr; back</a><a href="{raw}">show only the HTML</a></span>
</header>
<iframe id="se-full" src="{raw}" sandbox="allow-scripts allow-popups" title="{title}"></iframe>
</body>
</html>"#,
        title = html_escape(label),
        css_href = crate::assets::SITE_CSS.url(),
        canon = html_escape(&canon_href),
        raw = html_escape(&raw_href),
    )
}

/// Wrap an already-rendered body fragment in the exact entry-page article
/// structure the live site uses (`main > article > header + section`), so an
/// authoring tool (the grading comparison view) can show a post's body with
/// full styling parity. This is *only the post itself* — no site chrome (tag
/// cloud, crumbs, continue nav, footer). `inner_html` is trusted rendered
/// content from the same pipeline `render::render_entry` feeds the live site,
/// and is inserted as-is (exactly like `entry_page`'s content).
pub(crate) fn post_body_fragment(entry: &Entry, inner_html: &str) -> String {
    format!(
        r#"<main><article>
{post_header}
<section>{content}</section>
</article></main>"#,
        post_header = post_header(entry),
        content = inner_html,
    )
}

/// Render a browsable folder listing (`post-model.md` §6): a gallery of images,
/// or a file list for mixed content, under the same site chrome as a post. The
/// public allowlist was decided at scan time (`entry.listing`); this only renders
/// it. `intro_html` is an optional already-rendered intro document (the `index/`
/// "gallery with a story" case), inserted as-is like `entry_page`'s content.
pub fn listing_page(
    entry: &Entry,
    listing: &Listing,
    all_entries: &[&Entry],
    intro_html: Option<&str>,
) -> String {
    let cloud = compute_cloud(all_entries);
    let view = ViewFilter::default();
    let ctx = HeaderContext::plain(&cloud, &view);

    let label = entry.display_label.as_deref().or(entry.label.as_deref()).unwrap_or("Files");
    let canon = canonical(entry, all_entries);
    let canon_href = canonical_href(entry, all_entries);

    let body = listing_body(
        label,
        &canon.path,
        &canon_href,
        &post_header(entry),
        listing,
        intro_html,
        &render_site_header(&ctx),
    );
    page_shell(&format!("esko.bar — {}", label), &body, "listing", false)
}

/// A nested subfolder listing (`post-model.md` §6): `/label/sub/…` browsed as its
/// own page. It has no `Entry` (nested items carry no identity), so the title and
/// item-href base come straight from the request path — the path IS the canonical
/// URL, mirroring the filesystem verbatim. `base_path` is the decoded request path
/// without a trailing slash (e.g. `/resume/talks`).
pub fn nested_listing_page(
    base_path: &str,
    title: &str,
    listing: &Listing,
    all_entries: &[&Entry],
) -> String {
    let cloud = compute_cloud(all_entries);
    let view = ViewFilter::default();
    let ctx = HeaderContext::plain(&cloud, &view);

    // A quiet breadcrumb up to the parent path stands in for the post header.
    let parent = base_path.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
    let parent_href = if parent.is_empty() { "/".to_string() } else { encode_path(parent) };
    let header = format!(
        r#"<header><nav><a href="{href}">&larr; up</a></nav></header>"#,
        href = html_escape(&parent_href),
    );

    let body =
        listing_body(title, base_path, &encode_path(base_path), &header, listing, None, &render_site_header(&ctx));
    page_shell(&format!("esko.bar — {}", title), &body, "listing", false)
}

/// The shared `<article id="listing">` body for both a top-level listing (Entry)
/// and a nested one (path). `base_path` is the decoded path each item href hangs
/// off; `header_html` is the post header (top-level) or a breadcrumb (nested).
fn listing_body(
    title: &str,
    base_path: &str,
    canon_href: &str,
    header_html: &str,
    listing: &Listing,
    intro_html: Option<&str>,
    site_header: &str,
) -> String {
    let intro = match intro_html {
        Some(h) if !h.is_empty() => format!("<section>{}</section>", h),
        _ => String::new(),
    };
    format!(
        r#"{header}
{crumbs}
<main>
<article id="listing" data-canonical="{canonical}">
{post_header}
<h1>{title} <small>{count}</small></h1>
{collision}{intro}{grid}
<footer><a href="/">timeline</a></footer>
</article>
</main>"#,
        header = site_header,
        crumbs = crumbs(),
        canonical = html_escape(canon_href),
        post_header = header_html,
        title = html_escape(title),
        count = html_escape(&listing_count(listing)),
        collision = listing_collision_notice(listing),
        intro = intro,
        grid = listing_grid(base_path, listing),
    )
}

/// The "N files" / "N of M files public" (+ folder count) header line, so a
/// reader can tell the allowlist is withholding something. Folder rows are
/// counted separately from files.
fn listing_count(listing: &Listing) -> String {
    let files = listing.items.iter().filter(|i| !i.is_dir).count();
    let dirs = listing.items.iter().filter(|i| i.is_dir).count();
    let mut s = if listing.total > files {
        format!("{} of {} files public", files, listing.total)
    } else if files == 1 {
        "1 file".to_string()
    } else {
        format!("{} files", files)
    };
    if dirs == 1 {
        s.push_str(", 1 folder");
    } else if dirs > 1 {
        s.push_str(&format!(", {} folders", dirs));
    }
    s
}

/// The listing collision notice: several files claimed the primary slot, so the
/// server declined to guess and listed instead. Prominent and persistent (never
/// auto-dismissed); an `index/` marker silences it (the notice is then empty).
fn listing_collision_notice(listing: &Listing) -> String {
    if listing.collision.is_empty() {
        return String::new();
    }
    let names: String = listing
        .collision
        .iter()
        .map(|n| format!("<code>{}</code>", html_escape(n)))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        r#"<aside>Several files claim the primary slot ({names}), so this folder is shown as a listing rather than a single page. Remove all but one, or add an empty <code>index/</code> folder to make the listing intentional.</aside>"#,
        names = names,
    )
}

/// The listing body: a gallery (`<ul>` of figures) when every public item is an
/// image, else a file list (`<ol>`). Item links resolve as folder-relative assets
/// (`/label/file.ext`) off the listing's own canonical path.
fn listing_grid(base_path: &str, listing: &Listing) -> String {
    if listing.items.is_empty() {
        return r#"<section><p class="empty">Nothing here is public yet &mdash; tag a file <code>public</code> to list it.</p></section>"#.to_string();
    }
    if listing.is_gallery() {
        format!("<section>{}</section>", gallery_html(base_path, &listing.items))
    } else {
        format!("<section>{}</section>", file_list_html(base_path, &listing.items))
    }
}

/// A gallery `<ul>` of image tiles. The tile `<img>` loads a small `?thumb`
/// rendition (a stripped, resized JPEG built by libvips — `post-model.md` §8,
/// C8c) so a grid stays light; the surrounding link opens the full asset.
fn gallery_html(base_path: &str, items: &[ListItem]) -> String {
    let tiles: String = items
        .iter()
        .map(|it| {
            let href = html_escape(&encode_path(&format!("{}/{}", base_path, it.name)));
            format!(
                r#"<li><a href="{href}"><figure><div><img src="{href}?thumb" alt="{alt}" loading="lazy"></div><figcaption><b>{stem}<small>{ext}</small></b><span>{size}</span></figcaption></figure></a></li>"#,
                href = href,
                alt = html_escape(&it.stem),
                stem = html_escape(&it.stem),
                ext = html_escape(&dot_ext(&it.ext)),
                size = html_escape(&human_size(it.size)),
            )
        })
        .collect();
    format!("<ul>{}</ul>", tiles)
}

/// A file-list `<ol>` of typed rows (kind badge, name, size, date) — the one
/// attachment/listing idiom (`post-model.md` §6). Items link to their asset.
fn file_list_html(base_path: &str, items: &[ListItem]) -> String {
    let rows: String = items
        .iter()
        .map(|it| {
            let iso = html_escape(&it.mtime.format("%Y-%m-%dT%H:%M:%S").to_string());
            let date = html_escape(&it.mtime.format("%Y-%m-%d").to_string());
            if it.is_dir {
                // A subfolder row: a nested listing at `<base>/<name>/` (trailing
                // slash). The path IS the canonical URL, mirroring the filesystem.
                let href = format!("{}/", encode_path(&format!("{}/{}", base_path, it.name)));
                return format!(
                    r#"<li><a href="{href}"><i data-kind="folder">&#9656;</i><b>{stem}<small>/</small></b><span>folder</span><time datetime="{iso}">{date}</time></a></li>"#,
                    href = html_escape(&href),
                    stem = html_escape(&it.stem),
                    iso = iso,
                    date = date,
                );
            }
            let href = encode_path(&format!("{}/{}", base_path, it.name));
            let (badge, category) = kind_badge(&it.ext);
            format!(
                r#"<li><a href="{href}"><i data-kind="{category}">{badge}</i><b>{stem}<small>{ext}</small></b><span>{size}</span><time datetime="{iso}">{date}</time></a></li>"#,
                href = html_escape(&href),
                category = category,
                badge = html_escape(&badge),
                stem = html_escape(&it.stem),
                ext = html_escape(&dot_ext(&it.ext)),
                size = html_escape(&human_size(it.size)),
                iso = iso,
                date = date,
            )
        })
        .collect();
    format!("<ol>{}</ol>", rows)
}

/// The attachment list below a document post's body: its public sibling files as
/// the shared file-list idiom (`post-model.md` §6). Empty when the post has none.
/// Items resolve as folder-relative assets off the post's canonical path.
fn attachments_section(entry: &Entry, all_entries: &[&Entry]) -> String {
    if entry.attachments.is_empty() {
        return String::new();
    }
    let base = canonical(entry, all_entries).path;
    let n = entry.attachments.len();
    format!(
        r#"<section id="attachments"><h2>{n} {word}</h2>{list}</section>"#,
        n = n,
        word = if n == 1 { "attachment" } else { "attachments" },
        list = file_list_html(&base, &entry.attachments),
    )
}

/// The source cite shown below a content post's body on its own page (post-model
/// md §4): the same `<cite>` as the timeline, in a labeled section. Empty unless
/// the post *cites* a destination while being its own medium — a link post shows
/// its embed card instead, so it never carries a duplicate cite here.
fn cite_section(entry: &Entry) -> String {
    if entry.link_url.is_none() || entry.kind() == "link" {
        return String::new();
    }
    let own = entry.display_label.as_deref().or(entry.label.as_deref());
    let cite = cite_line(entry, own);
    if cite.is_empty() {
        return String::new();
    }
    format!(r#"<section id="source"><h2>Source</h2>{cite}</section>"#, cite = cite)
}

/// The body for a link post whose embed never resolved (fetch failed, upstream
/// deleted, or not yet fetched): just the destination cite, so the page is still a
/// working link rather than a raw `.webloc`/`.url` download. `#linkcard` scopes the
/// cite CSS the same as a timeline row and the `#source` section.
pub fn bare_link_body(entry: &Entry) -> String {
    let cite = cite_line(entry, None);
    if cite.is_empty() {
        // A link post always has a destination; this is only a defensive fallback.
        return "<p>This link has no reachable destination.</p>".to_string();
    }
    format!(r#"<div id="linkcard">{cite}</div>"#, cite = cite)
}

/// `.ext` for display, empty for a dotless file.
fn dot_ext(ext: &str) -> String {
    if ext.is_empty() {
        String::new()
    } else {
        format!(".{}", ext)
    }
}

/// A human-readable byte size for a listing row (binary units, one decimal).
fn human_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// The type badge for a file-list row: an uppercase label plus a `data-kind`
/// category. The category drives the badge tint from the stylesheet (see the
/// `[data-kind=…]` rules), so no inline `style` is needed (CSP `style-src 'self'`).
fn kind_badge(ext: &str) -> (String, &'static str) {
    let e = ext.to_ascii_lowercase();
    let category = match e.as_str() {
        "pdf" => "pdf",
        "txt" | "md" | "markdown" | "rtf" | "doc" | "docx" | "pages" => "doc",
        "zip" | "gz" | "tar" | "7z" | "dmg" | "pkg" => "archive",
        "mp3" | "wav" | "m4a" | "mp4" | "mov" | "aif" | "aiff" => "media",
        _ => "file",
    };
    let label = if e.is_empty() {
        "FILE".to_string()
    } else {
        e.to_ascii_uppercase().chars().take(4).collect()
    };
    (label, category)
}

/// The quiet, non-dismissing "metadata removed" note shown under a stripped
/// image (`post-model.md` §8, and the project rule that privacy notices stay
/// visible until the author acts). Explains what was removed and how to opt out.
fn metadata_notice() -> String {
    r#"<aside data-notice="privacy">Location and camera metadata were removed for privacy. Add the <code>public-original</code> tag to the file to publish it unchanged.</aside>"#.to_string()
}

/// The LOUD, non-dismissing warning shown under a `public-original` image that
/// actually carries embedded metadata: the *dangerous* state (publishing EXIF)
/// must shout, not the safe one (the project rule that security warnings display
/// prominently until the author acts). Names the concrete leak and the fix.
fn metadata_publish_warning() -> String {
    r#"<aside data-notice="danger">This image publishes its embedded metadata &mdash; location (GPS), camera, and capture time are visible to anyone. Remove the <code>public-original</code> tag to strip them.</aside>"#.to_string()
}

/// Render an image viewer page. `is_original` is the per-file exact-bytes opt-in;
/// `publishes_metadata` is set only when that image actually carries sensitive
/// EXIF (so the warning fires on real leaks, not on a clean original).
pub fn image_page(
    entry: &Entry,
    _mime: &str,
    is_original: bool,
    publishes_metadata: bool,
    all_entries: &[&Entry],
    next: Option<&Entry>,
) -> String {
    let cloud = compute_cloud(all_entries);
    let view = ViewFilter::default();
    let ctx = HeaderContext::plain(&cloud, &view);

    let label = entry
        .display_label
        .as_deref()
        .or(entry.label.as_deref())
        .unwrap_or("Image");
    let canon_href = canonical_href(entry, all_entries);
    let src = canonical_raw_href(entry, all_entries);

    // Three states (post-model.md §8): a stripped image carries the quiet
    // "metadata removed" note; a `public-original` image that actually embeds
    // GPS/camera data carries the LOUD publish warning; a clean original needs
    // neither. The footer link offers the "full size" stripped image, or the
    // "original" exact bytes when that is what is served.
    let (notice, raw_label) = if !is_original {
        (metadata_notice(), "full size")
    } else if publishes_metadata {
        (metadata_publish_warning(), "original")
    } else {
        (String::new(), "original")
    };

    let body = format!(
        r#"{header}
{crumbs}
<main>
<article id="post" data-canonical="{canonical}" data-title="{data_title}">
{post_header}
<figure><img src="{src}" alt="{alt}"></figure>
{notice}
{extras}
<footer><a href="{src}">{raw_label}</a> <a href="/">timeline</a></footer>
</article>
{continue_nav}
</main>"#,
        header = render_site_header(&ctx),
        crumbs = crumbs(),
        canonical = html_escape(&canon_href),
        data_title = html_escape(label),
        post_header = post_header(entry),
        src = html_escape(&src),
        alt = html_escape(label),
        notice = notice,
        raw_label = raw_label,
        extras = post_extras(entry, all_entries),
        continue_nav = continue_nav(next, all_entries),
    );

    page_shell(&format!("esko.bar — {}", label), &body, "entry", false)
}

/// Post-header tags (horizontal, Finder-color dots) for entry/image pages.
fn post_tags(entry: &Entry) -> String {
    let pills: String = entry
        .topical_tags()
        .map(|t| {
            format!(
                r#"<a href="/+{tag}" data-tag-color="{color}">{tag}</a>"#,
                tag = html_escape(&t.name),
                color = t.color,
            )
        })
        .collect::<Vec<_>>()
        .join("");
    if pills.is_empty() {
        String::new()
    } else {
        format!(r#"<nav aria-label="Tags">{}</nav>"#, pills)
    }
}

/// Character-level Levenshtein edit distance, for ranking closest slugs on a
/// dead-label not-found page. Small inputs (slugs), so the plain DP is ample.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// The posts whose slugs are closest to a mistyped/renamed bare label, for the
/// dead-label not-found page. Ranked by edit distance, thresholded so only real
/// near-misses show; deduplicated by slug; capped at `max`. Only public posts in
/// `all_entries` are considered, so this never reveals a hidden or missing post.
fn closest_slugs<'a>(query: &str, all_entries: &[&'a Entry], max: usize) -> Vec<&'a Entry> {
    let q = match slug(query) {
        Some(s) => s,
        None => return Vec::new(),
    };
    let threshold = (q.chars().count() / 2).max(2);
    let mut scored: Vec<(usize, &Entry)> = all_entries
        .iter()
        .copied()
        .filter(|e| e.error.is_none())
        .filter_map(|e| e.slug.as_deref().map(|s| (levenshtein(&q, s), e)))
        .filter(|(d, _)| *d <= threshold)
        .collect();
    scored.sort_by(|a, b| a.0.cmp(&b.0));
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    scored
        .into_iter()
        .filter(|(_, e)| e.slug.as_deref().map_or(false, |s| seen.insert(s)))
        .take(max)
        .map(|(_, e)| e)
        .collect()
}

/// The not-found page for a dead bare label (a name with no claimant — e.g. a
/// post renamed without an `alias`). It never auto-redirects (a reused name would
/// mis-resolve); it offers the timeline, search, and closest-slug suggestions.
/// Served with HTTP 404 by the router, identically to any missing path.
pub fn not_found_label_page(label: &str, all_entries: &[&Entry]) -> String {
    let cloud = compute_cloud(all_entries);
    let view = ViewFilter::default();
    let ctx = HeaderContext::plain(&cloud, &view);

    let suggestions = closest_slugs(label, all_entries, 5);
    let sugg_html = if suggestions.is_empty() {
        String::new()
    } else {
        let items: String = suggestions
            .iter()
            .map(|e| {
                let t = e.display_label.as_deref().or(e.label.as_deref()).unwrap_or("(untitled)");
                format!(
                    r#"<li><a href="{href}">{t}</a></li>"#,
                    href = html_escape(&canonical_href(e, all_entries)),
                    t = html_escape(t),
                )
            })
            .collect();
        format!("<p>Did you mean:</p><ul>{}</ul>", items)
    };

    let body = format!(
        r#"{header}
{crumbs}
<main>
<article id="post">
<header><time>404</time></header>
<section>
<h1>Nothing at that name</h1>
<p>No post is addressed <code>/{label}</code>. It may have been renamed. Try the timeline, the search above, or:</p>
{suggestions}
</section>
</article>
</main>"#,
        header = render_site_header(&ctx),
        crumbs = crumbs(),
        label = html_escape(label),
        suggestions = sugg_html,
    );
    page_shell(&format!("esko.bar — not found: {}", label), &body, "entry", false)
}

/// Render the 404 page (minimal — no cloud, just the way home).
pub fn not_found_page() -> String {
    let body = r#"<nav id="crumbs" aria-label="Site"><a href="/">&#8592; Timeline</a></nav>
<main>
<article id="post">
<header><time>404</time></header>
<section><p>Nothing here.</p></section>
</article>
</main>"#;
    page_shell("esko.bar — Not Found", body, "plain", false)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, NaiveDateTime};

    fn tag(name: &str, count: usize) -> TagStat {
        TagStat {
            name: name.to_string(),
            count,
            last_active: NaiveDate::from_ymd_opt(2026, 7, 5).unwrap(),
            color: 0,
        }
    }

    #[test]
    fn cloud_order_is_alphabetical_case_insensitive() {
        // Count is irrelevant to order — the cloud is stable/predictable.
        let tags = vec![tag("Zebra", 9), tag("apple", 1), tag("mango", 5)];
        let order: Vec<&str> = cloud_order(&tags).iter().map(|t| t.name.as_str()).collect();
        assert_eq!(order, ["apple", "mango", "Zebra"]);
    }

    fn ts(s: &str) -> crate::postdate::PostDate {
        crate::postdate::PostDate::from_mtime(
            NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H%M%S").unwrap(),
        )
    }

    fn mkentry(label: &str, at: &str) -> Entry {
        Entry {
            path: std::path::PathBuf::from(format!("/c/{}.md", label)),
            dir: None,
            timestamp: ts(at),
            edited: None,
            label: Some(label.to_string()),
            slug: slug(label),
            display_label: None,
            excerpt: None,
            extension: "md".to_string(),
            tags: Vec::new(),
            grade: None,
            aliases: Vec::new(),
            revisions: Vec::new(),
            error: None,
            listing: None,
            attachments: Vec::new(),
            link_url: None,
            link_title: None,
        }
    }

    #[test]
    fn oldest_claim_owns_bare_label() {
        let old = mkentry("foo", "2026-03-01T120000");
        let mut newer = mkentry("foo", "2026-03-10T120000");
        newer.path = "/c/foo-folder/foo.md".into();
        let all = vec![&old, &newer];

        // Oldest owns the bare label; the newer claim gets a dated URL.
        assert_eq!(canonical(&old, &all).path, "/foo");
        assert_eq!(canonical(&newer, &all).path, "/2026/03/10/foo");
        assert!(std::ptr::eq(name_owner("foo", &all).unwrap(), &old));
    }

    #[test]
    fn alias_competes_for_the_name_and_can_win() {
        // `cv` (old) aliases `resume`; a newer post is literally named `resume`.
        let mut cv = mkentry("cv", "2026-01-01T090000");
        cv.aliases = vec!["resume".to_string()];
        let resume = mkentry("resume", "2026-05-01T090000");
        let all = vec![&cv, &resume];

        // The oldest claim on `resume` is cv's alias, so cv owns `/resume`.
        assert!(std::ptr::eq(name_owner("resume", &all).unwrap(), &cv));
        // The literally-named post therefore lives at its date path.
        assert_eq!(canonical(&resume, &all).path, "/2026/05/01/resume");
    }

    #[test]
    fn tied_oldest_has_no_owner() {
        let a = mkentry("dup", "2026-03-01T120000");
        let mut b = mkentry("dup", "2026-03-01T120000");
        b.path = "/c/dup2.md".into();
        let all = vec![&a, &b];
        assert!(name_owner("dup", &all).is_none());
    }

    #[test]
    fn errored_posts_do_not_claim_names() {
        let good = mkentry("x", "2026-03-01T120000");
        let mut bad = mkentry("x", "2026-01-01T120000");
        bad.error = Some(PostError::NoPrimary);
        let all = vec![&good, &bad];
        // Even though `bad` is older, it does not claim the name — `good` owns it.
        assert!(std::ptr::eq(name_owner("x", &all).unwrap(), &good));
        assert_eq!(canonical(&good, &all).path, "/x");
    }

    #[test]
    fn natural_and_hyphenated_names_collide_on_slug() {
        // "Fog Over The Bay" and "fog-over-the-bay" reduce to one address.
        let old = mkentry("Fog Over The Bay", "2026-03-01T120000");
        let mut newer = mkentry("fog-over-the-bay", "2026-03-10T120000");
        newer.path = "/c/fog/fog-over-the-bay.md".into();
        let all = vec![&old, &newer];

        assert_eq!(canonical(&old, &all).path, "/fog-over-the-bay");
        assert_eq!(canonical(&newer, &all).path, "/2026/03/10/fog-over-the-bay");
        assert!(std::ptr::eq(name_owner("Fog Over The Bay", &all).unwrap(), &old));
        // Two others share the address as seen from each row.
        assert_eq!(name_shares(&old, &all).len(), 1);
    }

    #[test]
    fn reserved_slug_yields_the_bare_url() {
        // A post literally named "2026" (a year-in-review) slugs to a reserved
        // segment, so it is addressed at its date path, not the bare "/2026".
        let year = mkentry("2026", "2026-12-31T120000");
        let all = vec![&year];
        // Addressed at its date path (with the time segment), not the bare "/2026".
        assert_eq!(canonical(&year, &all).path, "/2026/12/31/120000");
        // The bare URL never resolves to it.
        assert!(name_owner("2026", &all).is_none());
    }

    #[test]
    fn closest_slugs_suggests_near_misses() {
        let a = mkentry("fog-over-the-bay", "2026-03-01T120000");
        let b = mkentry("smog-over-the-bay", "2026-03-02T120000");
        let c = mkentry("something-else-entirely", "2026-03-03T120000");
        let all = vec![&a, &b, &c];
        let sugg = closest_slugs("fog-over-the-bey", &all, 5);
        // The two near names are suggested; the unrelated one is filtered out.
        assert!(sugg.iter().any(|e| e.slug.as_deref() == Some("fog-over-the-bay")));
        assert!(sugg.iter().any(|e| e.slug.as_deref() == Some("smog-over-the-bay")));
        assert!(!sugg.iter().any(|e| e.slug.as_deref() == Some("something-else-entirely")));
    }

    #[test]
    fn revision_href_is_dated_with_time() {
        let mut e = mkentry("post", "2026-03-01T120000");
        e.revisions = vec![Revision {
            date: NaiveDateTime::parse_from_str("2026-02-15T091500", "%Y-%m-%dT%H%M%S").unwrap(),
            path: "/c/post/post copy.md".into(),
            rank: 1,
        }];
        // The time is a path segment now, before the slug.
        assert!(revision_items(&e).contains("/2026/02/15/091500/post"));
    }

    // ── outbound cite / link axis (post-model.md §4) ──

    #[test]
    fn link_domain_strips_scheme_www_port_and_userinfo() {
        assert_eq!(link_domain("https://www.example.com/x"), "example.com");
        assert_eq!(link_domain("https://example.com:8443/path?q=1"), "example.com");
        assert_eq!(link_domain("http://user:pass@nytimes.com/2026"), "nytimes.com");
        assert_eq!(link_domain("https://bsky.app"), "bsky.app");
    }

    #[test]
    fn favicon_tile_is_deterministic_first_letter() {
        let a = favicon_tile("github.com");
        let b = favicon_tile("github.com");
        assert_eq!(a, b, "same domain -> same tile");
        assert!(a.contains(">G<"), "first letter, uppercased: {a}");
        assert!(a.contains("data-tile=\""), "color as a data attribute, not inline style");
    }

    /// A folder post that cites a `link.*` sidecar: our label -> our page, plus a
    /// separate cite -> the destination (mockup case 2).
    fn cite_folder(label: &str) -> Entry {
        let mut e = mkentry(label, "2026-07-06T120000");
        e.dir = Some(format!("/c/{}", label).into());
        e.link_url = Some("https://nytimes.com/2026/07/road-diets".to_string());
        e.link_title = Some("Road Diets Are Quietly Reshaping American Suburbs".to_string());
        e
    }

    #[test]
    fn cite_line_shows_title_and_domain() {
        let e = cite_folder("narrow-streets");
        let out = cite_line(&e, e.label.as_deref());
        assert!(out.contains(r#"rel="noreferrer""#), "external links get noreferrer");
        assert!(out.contains("<b>Road Diets Are Quietly Reshaping American Suburbs</b>"));
        assert!(out.contains("<span>nytimes.com</span>"));
        assert!(out.contains(r#"href="https://nytimes.com/2026/07/road-diets""#));
    }

    #[test]
    fn cite_line_drops_title_equal_to_our_heading() {
        // "label == title" degrade: the source is shown once (domain only), not twice.
        let mut e = cite_folder("x");
        e.link_title = Some("My Own Heading".to_string());
        let out = cite_line(&e, Some("my own heading")); // case-insensitive match
        assert!(!out.contains("<b>"), "duplicated title suppressed: {out}");
        assert!(out.contains("<span>nytimes.com</span>"));
    }

    #[test]
    fn cite_line_degrades_to_bare_domain_without_title() {
        // Fetch never resolved a title -> the cite is just favicon + domain.
        let mut e = cite_folder("x");
        e.link_title = None;
        let out = cite_line(&e, e.label.as_deref());
        assert!(!out.contains("<b>"), "no title element without a resolved title");
        assert!(out.contains("<span>nytimes.com</span>"));
    }

    #[test]
    fn cite_line_preserves_http_for_the_css_caution_flag() {
        let mut e = cite_folder("x");
        e.link_url = Some("http://oldsite.example/gallery".to_string());
        let out = cite_line(&e, e.label.as_deref());
        // The scheme guard already vetted it; the "(not secure)" flag is pure CSS,
        // so the template only has to preserve the http:// href verbatim.
        assert!(out.contains(r#"href="http://oldsite.example/gallery""#));
        assert!(out.contains("<span>oldsite.example</span>"));
    }

    #[test]
    fn row_content_cite_keeps_our_heading_and_adds_a_source() {
        let e = cite_folder("narrow-streets");
        let all = vec![&e];
        let row = render_row(&e, &all);
        // Our label links to our page...
        assert!(row.contains(r#"<h3><a href="/narrow-streets">"#), "row: {row}");
        // ...and the destination is cited below it.
        assert!(row.contains("<cite>"));
        assert!(row.contains("<span>nytimes.com</span>"));
    }

    /// A bare link file (the post IS the destination). `webloc`/`url`/single-URL
    /// text -> kind() == "link".
    fn bare_link(label: Option<&str>) -> Entry {
        let mut e = mkentry(label.unwrap_or("placeholder"), "2026-07-05T120000");
        e.extension = "webloc".to_string();
        e.link_url = Some("https://github.com/rust-lang/rust".to_string());
        e.link_title = Some("rust-lang/rust".to_string());
        if label.is_none() {
            e.label = None;
            e.slug = None;
        }
        e
    }

    #[test]
    fn labeled_bare_link_is_a_link_post_with_our_heading() {
        let e = bare_link(Some("worth-saving"));
        assert_eq!(e.kind(), "link");
        let all = vec![&e];
        let row = render_row(&e, &all);
        // Case 3: our label -> our page (card), the source cited below.
        assert!(row.contains(r#"<h3><a href="/worth-saving">"#), "row: {row}");
        assert!(row.contains("<cite>"));
        assert!(row.contains("<b>rust-lang/rust</b>"));
    }

    #[test]
    fn unlabeled_bare_link_promotes_the_cite_and_keeps_a_permalink() {
        let e = bare_link(None);
        assert_eq!(e.kind(), "link");
        let all = vec![&e];
        let row = render_row(&e, &all);
        // Case 4: the destination's headline IS the headline (inside <h3>)...
        assert!(row.contains("<h3><cite>"), "promoted cite: {row}");
        assert!(row.contains("<b>rust-lang/rust</b>"));
        // ...and a quiet permalink keeps our own card page reachable.
        assert!(row.contains("data-perma"));
    }

    #[test]
    fn cite_section_only_renders_for_a_content_post_that_cites() {
        // A content post that cites -> a Source section on its page.
        let cite_post = cite_folder("narrow-streets");
        assert!(cite_section(&cite_post).contains(r#"<section id="source">"#));
        // A bare link post shows its card instead, so no duplicate cite section.
        let link_post = bare_link(Some("worth-saving"));
        assert!(cite_section(&link_post).is_empty());
        // A plain post with no destination -> nothing.
        let plain = mkentry("just-a-note", "2026-07-01T120000");
        assert!(cite_section(&plain).is_empty());
    }
}
