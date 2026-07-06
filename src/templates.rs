use crate::entry::{Entry, PostError, Revision};
use crate::stats::{compute_cloud, finder_color_var, CloudStats, TagStat, ViewFilter};

// ============================================================================
// Stylesheet — ported from the mockups (timeline-glass-mockup.html, the "rows"
// glass variant Tilde chose, and entry-page-mockup.html's shared header). The
// old glass-card design was thrown out. Element selectors, no classes (the JS
// state classes .selected/.near/.empty aside), light-dark() theming.
// ============================================================================

const CSS: &str = r##"
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
/* stepped bars, lit to the current level */
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
#site form p > a#favonly[aria-current="true"] { color: var(--ink); }

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
  width: calc((100% - 4px) / 3);
  border-radius: 999px;
  background: light-dark(#fff, rgba(178, 205, 184, .14));
  box-shadow: 0 1px 3px light-dark(rgba(20, 24, 20, .16), rgba(0, 0, 0, .4));
  transition: left .18s ease;
}
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

/* quality meter — a hairline with ticks at the notable/best thresholds, shared
   by the timeline rows and the entry-post header */
.meter { position: relative; width: 2.9rem; height: 2px; flex: none; border-radius: 1px; background: var(--hair); }
.meter > i { position: absolute; inset: 0 auto 0 0; border-radius: 1px; background: var(--faint); }
.meter::before, .meter::after { content: ""; position: absolute; top: -2px; width: 1px; height: 6px; background: var(--hair); }
.meter::before { left: 50%; }
.meter::after { left: 78%; }
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
main section article h3 a { color: var(--ink); }
main section article h3 a:hover { color: var(--violet); text-decoration: none; }
main section article h3 small { font-size: 1em; font-weight: inherit; color: var(--soft); }
main section article h3 i { font-weight: 450; color: var(--faint); }

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

const JS: &str = r##"
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
  function slugify(s) {
    return s.toLowerCase().trim()
      .replace(/[^\w\s-]/g, "").replace(/[\s_]+/g, "-").replace(/^-+|-+$/g, "") || "section";
  }

  /* ---------- help dialog (both pages) ---------- */
  if (helpbtn && help) helpbtn.addEventListener("click", function () { help.showModal(); });
  if (help) help.addEventListener("click", function (e) { if (e.target === help) help.close(); });

  /* ---------- reading width: shared grip + store on both pages ---------- */
  var grip = document.getElementById("grip");
  var readout = document.getElementById("readout");
  var rootEl = document.documentElement;
  var mainEl = document.querySelector("main");
  var segSpan = document.querySelector("#site form p > span");
  var qInput = document.getElementById("q");
  var qbtn = document.getElementById("qbtn");
  /* the magnifier focuses the field rather than submitting an empty search;
     without JS it stays a submit button, so search still works. */
  if (qbtn && qInput) qbtn.addEventListener("click", function (e) { e.preventDefault(); qInput.focus(); });
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
        summary.textContent = "Contents";
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

  /* ---------- timeline: bookmarks, saved view, keyboard, proximity ---------- */
  if (page === "timeline") {
    var saved = new Set(JSON.parse(store.getItem(SAVED) || "[]"));
    var navsaved = document.getElementById("navsaved");
    var count = navsaved && navsaved.querySelector("output");
    var savedView = body.dataset.view === "saved";
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
    refreshMarks(); refreshNav(); applySavedView();

    if (mainEl) mainEl.addEventListener("click", function (e) {
      var b = e.target.closest("aside > button"); if (!b) return;
      var a = b.closest("article"), k = keyOf(a);
      if (saved.has(k)) saved.delete(k); else saved.add(k);
      store.setItem(SAVED, JSON.stringify(Array.from(saved)));
      b.setAttribute("aria-pressed", saved.has(k));
      refreshNav();
      if (savedView) applySavedView();
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
      if (qInput && e.target === qInput) { if (e.key === "Escape") { qInput.blur(); } return; }
      if (e.target.closest("input, textarea") || (help && help.open)) return;
      switch (e.key) {
        case "j": case "ArrowDown": e.preventDefault(); move(1); break;
        case "k": case "ArrowUp": e.preventDefault(); move(-1); break;
        case "Enter": { var a = document.querySelector("main article.selected h3 a"); if (a) a.click(); break; }
        case "b": { var b = document.querySelector("main article.selected aside > button"); if (b) b.click(); break; }
        case "/": e.preventDefault(); if (qInput) qInput.focus(); break;
        case "?": if (help) help.showModal(); break;
        case "Escape": clearSel(); break;
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
    if view.level > 0 {
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
    /// The current view filter (level / favorites / search).
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
        // preserving the current level/favorites/search.
        let href = if is_active {
            make_url("/", ctx.view)
        } else {
            make_url(&format!("/+{}", tag.name), ctx.view)
        };
        let size = ctx.cloud.size_rem(tag.count);
        let (ink, weight) = ctx.cloud.ink(tag.last_active);
        let entries_word = if tag.count == 1 { "entry" } else { "entries" };
        out.push_str(&format!(
            r#"<a href="{href}" data-tag="{tag}" aria-current="{cur}" style="--tag:{color};font-size:{size:.2}rem;color:{ink};font-weight:{weight}" title="{count} {word}, last active {last}">{tag} <small>{count}</small></a>"#,
            href = html_escape(&href),
            tag = html_escape(&tag.name),
            cur = if is_active { "true" } else { "false" },
            color = finder_color_var(tag.color),
            size = size,
            ink = ink,
            weight = weight,
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

    // Grade segmented control: three links, each setting its floor while
    // preserving favorites/search; the thumb + stepped bars reflect the state.
    let level_link = |lvl: u8, label: &str| {
        let target = ViewFilter { level: lvl, fav: view.fav, q: view.q.clone() };
        format!(
            r#"<a href="{href}" data-grade="{lvl}" aria-current="{cur}">{label}</a>"#,
            href = html_escape(&make_url(ctx.base_path, &target)),
            lvl = lvl,
            cur = if view.level == lvl { "true" } else { "false" },
            label = label,
        )
    };
    let thumb_pos = format!("calc(2px + {} * (100% - 4px) / 3)", view.level);
    let bar = |i: u8| if i <= view.level { " data-on" } else { "" };
    let level_tip = match view.level {
        2 => "Showing only the best — graded top 22%",
        1 => "Showing notable and better — graded top 50%",
        _ => "Showing everything",
    };

    // Favorites toggle (independent axis): flips the fav flag, preserves the rest.
    let fav_target = ViewFilter { level: view.level, fav: !view.fav, q: view.q.clone() };
    let fav_href = html_escape(&make_url(ctx.base_path, &fav_target));

    // Saved-bookmarks link: to /saved, or back to root when already there.
    let saved_href = if ctx.saved_view { "/" } else { "/saved" };
    let saved_current = if ctx.saved_view { "page" } else { "false" };

    // Search preserves grade/favorites via hidden fields; action is the path.
    // (A form can't emit a valueless key, so `favorites=` stands in for the
    // bare `?favorites` the links use — the parser treats both as presence.)
    let mut hidden = String::new();
    if view.level > 0 {
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
    // to the tag path (or root), keeping the current level/favorites/search.
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
<svg viewBox="0 0 24 24"><title>{tip}</title><rect x="2" y="14" width="5.5" height="8" rx="2.4"{b0}/><rect x="9.25" y="8" width="5.5" height="14" rx="2.4"{b1}/><rect x="16.5" y="2" width="5.5" height="20" rx="2.4"{b2}/></svg>
<span><u style="left:{thumb}"></u>{everything}{notable}{best}</span>
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
        tip = level_tip,
        b0 = bar(0),
        b1 = bar(1),
        b2 = bar(2),
        thumb = thumb_pos,
        everything = level_link(0, "everything"),
        notable = level_link(1, "notable"),
        best = level_link(2, "best"),
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
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>{css}{embed_css}</style>
</head>
<body data-page="{page_kind}"{view_attr}>
<div id="grip" role="slider" tabindex="0" aria-label="Reading width" aria-orientation="horizontal" aria-valuemin="480" aria-valuemax="1160" aria-valuenow="736" title="Drag to set reading width. Double-click resets."></div>
<div id="readout" aria-hidden="true"></div>
{body}
<footer><button type="button" id="helpbtn"><kbd>?</kbd> shortcuts</button>{typeface}</footer>
<dialog id="help" aria-label="Keyboard shortcuts">
<h2>Keyboard</h2>
<dl>
<dt><kbd>j</kbd> / <kbd>k</kbd></dt><dd>select next / previous</dd>
<dt><kbd>Enter</kbd></dt><dd>open the selected entry</dd>
<dt><kbd>b</kbd></dt><dd>save for later</dd>
<dt><kbd>/</kbd></dt><dd>search</dd>
<dt><kbd>Esc</kbd></dt><dd>clear, close</dd>
<dt><kbd>?</kbd></dt><dd>this help</dd>
</dl>
<p><b>&#9733;</b> marks my favorites. Bookmarks are yours &mdash; they never leave this browser.</p>
</dialog>
<script>{js}</script>
</body>
</html>"#,
        title = html_escape(title),
        css = CSS,
        embed_css = crate::embed::EMBED_CSS,
        page_kind = page_kind,
        view_attr = view_attr,
        body = body,
        typeface = typeface,
        js = JS,
    )
}

// ---------------------------------------------------------------------------
// Date / label / href helpers
// ---------------------------------------------------------------------------

fn format_datetime_attr(ts: &chrono::NaiveDateTime) -> String {
    ts.format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// A canonical address: the decoded path, plus — only for same-day label
/// collisions and unlabeled entries — a `time` disambiguator carried as
/// `?time=HHMMSS`. Dates are a slash hierarchy (`/2026/03/12`).
pub struct Canonical {
    /// Decoded path, e.g. `/label`, `/2026/03/12/label`, or `/2026/03/12`.
    pub path: String,
    /// The `HHMMSS` for `?time=`, present only when the path alone is ambiguous.
    pub time: Option<String>,
}

/// Every entry that claims a name in the flat namespace: those literally labeled
/// it, plus those that carry it as an `alias <name>/` marker (all
/// case-insensitive). Errored posts do not claim a name — they resolve to an
/// error page, never to content under a stable URL.
fn name_claimants<'a>(name: &str, all_entries: &[&'a Entry]) -> Vec<&'a Entry> {
    let lower = name.to_lowercase();
    all_entries
        .iter()
        .copied()
        .filter(|e| e.error.is_none())
        .filter(|e| {
            e.label.as_ref().map_or(false, |l| l.to_lowercase() == lower)
                || e.aliases.iter().any(|a| a.to_lowercase() == lower)
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
    oldest_unique(&name_claimants(name, all_entries))
}

/// The other entries that claim the same bare name as `entry` (a folder vs a
/// file, an old vs new version, an alias) — surfaced on the winning page so a
/// shared name is never silent. Empty in the common unique-name case.
fn name_shares<'a>(entry: &Entry, all_entries: &[&'a Entry]) -> Vec<&'a Entry> {
    let label = match &entry.label {
        Some(l) => l,
        None => return Vec::new(),
    };
    name_claimants(label, all_entries)
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
    let ymd = || entry.timestamp.format("/%Y/%m/%d").to_string();
    let hms = || entry.timestamp.format("%H%M%S").to_string();

    let label = match &entry.label {
        Some(label) => label,
        None => return Canonical { path: ymd(), time: Some(hms()) },
    };

    let claimants = name_claimants(label, all_entries);
    if claimants.len() <= 1 {
        return Canonical { path: format!("/{}", label), time: None };
    }

    // The oldest claim owns the bare label; this entry owns it only if it is
    // that unique-oldest claimant (an alias or a tie sends it to a date path).
    let owns_bare = name_owner(label, all_entries).map_or(false, |o| std::ptr::eq(o, entry));
    if owns_bare {
        return Canonical { path: format!("/{}", label), time: None };
    }

    let day = entry.timestamp.format("%Y-%m-%d").to_string();
    let same_day = claimants
        .iter()
        .filter(|e| e.timestamp.format("%Y-%m-%d").to_string() == day)
        .count();
    if same_day == 1 {
        Canonical { path: format!("{}/{}", ymd(), label), time: None }
    } else {
        Canonical { path: format!("{}/{}", ymd(), label), time: Some(hms()) }
    }
}

/// Compose a ready-to-emit href from a decoded path: append an optional raw-file
/// extension and an optional `?time=` disambiguator, encoding as it goes.
fn compose_href(path: &str, ext: Option<&str>, time: Option<&str>) -> String {
    let mut decoded = path.to_string();
    if let Some(e) = ext {
        if !e.is_empty() {
            decoded.push('.');
            decoded.push_str(e);
        }
    }
    let mut out = encode_path(&decoded);
    if let Some(t) = time {
        out.push_str("?time=");
        out.push_str(&percent_encode(t));
    }
    out
}

/// The full addressable href for an entry (encoded path + `?time=` when needed).
fn canonical_href(entry: &Entry, all_entries: &[&Entry]) -> String {
    let c = canonical(entry, all_entries);
    compose_href(&c.path, None, c.time.as_deref())
}

/// The raw-bytes href for an entry: canonical path + extension (+ `?time=`), so
/// the parser reads the extension back off the last segment. Folders have none.
fn canonical_raw_href(entry: &Entry, all_entries: &[&Entry]) -> String {
    let c = canonical(entry, all_entries);
    let ext = (!entry.extension.is_empty()).then_some(entry.extension.as_str());
    compose_href(&c.path, ext, c.time.as_deref())
}

/// The full canonical location for a redirect: encoded path + `?time=` (when the
/// address needs it) + the view filters (grade/favorites/search), in that order.
/// Everything non-canonical is dropped, so every alias 301s onto one address.
pub fn canonical_location(entry: &Entry, all_entries: &[&Entry], view: &ViewFilter) -> String {
    let c = canonical(entry, all_entries);
    let mut parts: Vec<String> = Vec::new();
    if let Some(t) = &c.time {
        parts.push(format!("time={}", percent_encode(t)));
    }
    if view.level > 0 {
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
/// address (path + `?time=`), unique even across entries sharing a label.
fn canonical_key(c: &Canonical) -> String {
    let mut k = c.path.trim_start_matches('/').to_string();
    if let Some(t) = &c.time {
        k.push_str("?time=");
        k.push_str(t);
    }
    k
}

/// Render an entry's topical tags as a vertical rail nav with Finder-color dots.
fn rail_tags(entry: &Entry) -> String {
    let pills: String = entry
        .topical_tags()
        .map(|t| {
            format!(
                r#"<a href="/+{tag}" data-tag="{tag}" style="--tag:{color}">{tag}</a>"#,
                tag = html_escape(&t.name),
                color = finder_color_var(t.color),
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
            format!(
                r#"<span class="meter" title="Graded top {pct}%" aria-label="Graded top {pct}%"><i style="width:{fill}%"></i></span>"#,
                pct = pct,
                fill = (q * 100.0).round() as i32,
            )
        }
        None => String::new(),
    }
}

/// Render one timeline row.
fn render_row(entry: &Entry, all_entries: &[&Entry]) -> String {
    let datetime = format_datetime_attr(&entry.timestamp);
    let date = entry.timestamp.format("%Y-%m-%d").to_string();
    let canon = canonical(entry, all_entries);
    let href = compose_href(&canon.path, None, canon.time.as_deref());
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

    format!(
        r#"<li><article data-key="{key}">
<aside>
<time datetime="{datetime}">{date}</time>
{star}<button type="button" aria-pressed="false" aria-label="Save for later (stays in this browser)" title="Save for later &mdash; stays in this browser">{bookmark}</button>
{meter}{tags}
</aside>
<div><h3><a href="{href}">{title}</a></h3>{revisions}</div>
</article></li>"#,
        key = html_escape(&key),
        datetime = html_escape(&datetime),
        date = html_escape(&date),
        star = star,
        bookmark = BOOKMARK_SVG,
        meter = meter,
        tags = rail_tags(entry),
        href = html_escape(&href),
        title = title,
        revisions = revisions,
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
        let month = entry.timestamp.format("%B %Y").to_string();
        if month != current_month {
            if open_section {
                rows.push_str("</ul></section>");
            }
            // The month heading links into that month's view, grafting the date
            // onto any active tag path and keeping the current view filters.
            let month_path = format!("/{}{}", entry.timestamp.format("%Y/%m"), ctx.path_tags);
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
    let datetime = format_datetime_attr(&entry.timestamp);
    let date = entry.timestamp.format("%Y-%m-%d %H:%M").to_string();
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
    let datetime = format_datetime_attr(&next.timestamp);
    let date = next.timestamp.format("%Y-%m-%d").to_string();
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

/// The date-path address of one archived revision: the current post's label at
/// the revision's own date, always with `?time=` (revisions collide with the
/// current post and each other on the day, so the time always disambiguates).
fn revision_href(label: &str, rev: &Revision) -> String {
    let decoded = format!("{}/{}", rev.date.format("/%Y/%m/%d"), label);
    let hms = rev.date.format("%H%M%S").to_string();
    compose_href(&decoded, None, Some(&hms))
}

/// The dated `<li>` links for a post's archived revisions, newest first.
fn revision_items(entry: &Entry) -> String {
    let label = entry.label.as_deref().unwrap_or("");
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
                dt = html_escape(&format_datetime_attr(&e.timestamp)),
                date = html_escape(&e.timestamp.format("%Y-%m-%d").to_string()),
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

/// The extra blocks under a post's content: revision nav, alias list, and the
/// name-share notice (each empty unless it applies).
fn post_extras(entry: &Entry, all_entries: &[&Entry]) -> String {
    format!(
        "{}{}{}",
        revision_nav(entry),
        alias_list(entry),
        name_share_notice(entry, all_entries),
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
        PostError::AmbiguousPrimary(names) => (
            "This post has no single primary content file.".to_string(),
            list_fix(
                "Keep exactly one file named the folder name or index (rename or remove the others):",
                names,
            ),
        ),
        PostError::NoPrimary => (
            "This post folder has no content file.".to_string(),
            "<p>Add a primary file named the folder name or <code>index</code>.</p>".to_string(),
        ),
        PostError::UnparseableName(name) => (
            "This name is not a valid post name.".to_string(),
            format!(
                "<p>Post names are hyphenated, with no spaces: <code>{}</code>. Rename it, or use a <code>copy</code> / <code>alias</code> marker if that was the intent.</p>",
                html_escape(name),
            ),
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
        extras = post_extras(entry, all_entries),
        raw_href = html_escape(&raw_href),
        continue_nav = continue_nav(next, all_entries),
    );

    page_shell(&format!("esko.bar — {}", label), &body, "entry", false)
}

/// Render an image viewer page.
pub fn image_page(
    entry: &Entry,
    _mime: &str,
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

    let body = format!(
        r#"{header}
{crumbs}
<main>
<article id="post" data-canonical="{canonical}" data-title="{data_title}">
{post_header}
<figure><img src="{src}" alt="{alt}"></figure>
{extras}
<footer><a href="{src}">original</a> <a href="/">timeline</a></footer>
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
                r#"<a href="/+{tag}" style="--tag:{color}">{tag}</a>"#,
                tag = html_escape(&t.name),
                color = finder_color_var(t.color),
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

    fn ts(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H%M%S").unwrap()
    }

    fn mkentry(label: &str, at: &str) -> Entry {
        Entry {
            path: std::path::PathBuf::from(format!("/c/{}.md", label)),
            dir: None,
            timestamp: ts(at),
            edited: None,
            label: Some(label.to_string()),
            display_label: None,
            extension: "md".to_string(),
            tags: Vec::new(),
            grade: None,
            aliases: Vec::new(),
            revisions: Vec::new(),
            error: None,
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
        assert!(canonical(&old, &all).time.is_none());
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
    fn revision_href_is_dated_with_time() {
        let mut e = mkentry("post", "2026-03-01T120000");
        e.revisions = vec![Revision {
            date: ts("2026-02-15T091500"),
            path: "/c/post/post copy.md".into(),
            rank: 1,
        }];
        assert!(revision_items(&e).contains("/2026/02/15/post?time=091500"));
    }
}
