use std::collections::VecDeque;

use crate::entry::Entry;
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

  /* Finder tag colors, tuned per scheme */
  --tag-red:    light-dark(#e0383e, #ff6961);
  --tag-orange: light-dark(#e8842c, #ffb340);
  --tag-yellow: light-dark(#d9a800, #ffd426);
  --tag-green:  light-dark(#2f9e50, #30db5b);
  --tag-blue:   light-dark(#1673de, #409cff);
  --tag-purple: light-dark(#9853d2, #bf5af2);
  --tag-gray:   light-dark(#8e8e93, #98989d);

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
  display: grid;
  place-items: center;
  cursor: col-resize;
  touch-action: none;
  border-radius: 999px;
  z-index: 5;
}
#grip::before {
  content: "";
  width: 4px;
  height: 2.4rem;
  border-radius: 999px;
  background: var(--hair);
  transition: background .15s ease, height .15s ease;
}
#grip:hover::before,
#grip:focus-visible::before,
#grip.active::before { background: var(--violet); height: 3.1rem; }
@media (prefers-reduced-motion: reduce) { #grip::before { transition: none; } }
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

/* the cloud IS the header: size = entries, ink = recency, dot = Finder color */
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

/* quality: a hairline meter with ticks at the notable/best thresholds */
main section article > aside > span.meter {
  position: relative;
  width: 2.9rem;
  height: 2px;
  margin-top: .55rem;
  border-radius: 1px;
  background: var(--hair);
}
main section article > aside > span.meter > i { position: absolute; inset: 0 auto 0 0; border-radius: 1px; background: var(--faint); }
main section article:hover > aside > span.meter > i,
main section article.selected > aside > span.meter > i { background: var(--violet); }
main section article > aside > span.meter::before,
main section article > aside > span.meter::after {
  content: ""; position: absolute; top: -2px; width: 1px; height: 6px; background: var(--hair);
}
main section article > aside > span.meter::before { left: 50%; }
main section article > aside > span.meter::after { left: 78%; }

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
main section article aside > button svg { width: .85rem; height: .85rem; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linejoin: round; }
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

/* ================= entry post (plain typography) ================= */

article#post > header > time {
  display: block;
  font: 500 .78rem var(--mono);
  font-feature-settings: "tnum";
  letter-spacing: .02em;
  color: var(--faint);
}
article#post > header > nav { margin-top: .6rem; display: flex; flex-wrap: wrap; gap: .8rem; font-size: .8rem; }
article#post > header > nav a { display: inline-flex; align-items: center; gap: .35rem; color: var(--soft); font-weight: 550; }
article#post > header > nav a:hover { color: var(--violet); text-decoration: none; }
article#post > header > nav a::before { content: ""; width: .42rem; height: .42rem; border-radius: 50%; background: var(--tag, var(--tag-gray)); }

article#post > section { margin-top: 1.8rem; font-size: 1.05rem; line-height: 1.7; color: var(--ink); }
article#post > section p { margin: 0 0 1.4em; }
article#post > section h2 { font-size: 1.5em; font-weight: 650; margin: 1.6em 0 .5em; line-height: 1.2; letter-spacing: -.011em; }
article#post > section h3 { font-size: 1.2em; font-weight: 600; margin: 1.4em 0 .5em; }
article#post > section h4, article#post > section h5, article#post > section h6 { font-size: 1em; font-weight: 600; margin: 1.4em 0 .4em; }
article#post > section ul, article#post > section ol { padding-left: 1.3em; margin: 0 0 1.4em; }
article#post > section li { margin: .4em 0; }
article#post > section ul { list-style: disc; }
article#post > section ol { list-style: decimal; }
article#post > section blockquote { margin: 1.5em 0; padding: 0 1.4em; border-left: .2em solid var(--hair); color: var(--soft); }
article#post > section pre { overflow-x: auto; padding: 1em; margin: 0 0 1.4em; background: var(--code-bg); border-radius: .5em; font-size: .85em; line-height: 1.5; }
article#post > section code { font-family: var(--mono); font-size: .9em; background: var(--code-bg); padding: .15em .35em; border-radius: .3em; }
article#post > section pre > code { background: none; padding: 0; font-size: inherit; }
article#post > section img { max-width: 100%; height: auto; border-radius: .5em; }
article#post > section hr { border: 0; border-top: 1px solid var(--hair); margin: 2.5em 0; }
article#post > section a { text-decoration: underline; text-decoration-color: color-mix(in srgb, var(--violet), transparent 55%); text-underline-offset: .15em; }
article#post > section a:hover { text-decoration-color: currentColor; }

article#post > footer { margin-top: 2.5rem; padding-top: 1rem; border-top: 1px solid var(--hair); font-size: .8rem; }
article#post > footer a { color: var(--faint); margin-right: 1rem; }
article#post > footer a:hover { color: var(--ink); text-decoration: none; }

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
  font: .8rem var(--sans);
  color: var(--soft);
  text-align: center;
}
body > footer button { padding: 0; border: none; background: none; color: inherit; font: inherit; cursor: pointer; }
body > footer button:hover { color: var(--violet); }
"##;

// ============================================================================
// Client script — ported from the mockups. The cloud, rows and filtering are
// server-rendered (controls are real links), so this only carries the
// progressive enhancements: reading-width grip, help dialog, and on the
// timeline the bookmarks/saved-view/keyboard/proximity behaviors.
// ============================================================================

const JS: &str = r##"
(function () {
  var store = window.localStorage;
  var WIDTH = "esko-width", SAVED = "esko-saved";
  var body = document.body, page = body.dataset.page;
  var help = document.getElementById("help");
  var helpbtn = document.getElementById("helpbtn");

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

  /* ---------- entry page: land on the post, menu button jumps back up ---------- */
  if (page === "entry") {
    var landing = document.getElementById("crumbs") || mainEl;
    var jump = function (y) { window.scrollTo(0, y); };
    if (landing) jump(landing.getBoundingClientRect().top + window.pageYOffset);
    var toMenu = document.getElementById("tomenu");
    if (toMenu) toMenu.addEventListener("click", function () { jump(0); });
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

    /* proximity glow: the mark rests dim, lifts to full within ~50px (still
       grey), turns violet only on direct hover */
    var NEAR = 50, nearRaf = 0, nx = -1e4, ny = -1e4;
    function updateNear() {
      nearRaf = 0;
      document.querySelectorAll("main article aside > button").forEach(function (b) {
        var r = b.getBoundingClientRect();
        var dx = Math.max(r.left - nx, 0, nx - r.right);
        var dy = Math.max(r.top - ny, 0, ny - r.bottom);
        b.classList.toggle("near", dx * dx + dy * dy <= NEAR * NEAR);
      });
    }
    function queueNear() { if (!nearRaf) nearRaf = requestAnimationFrame(updateNear); }
    document.addEventListener("pointermove", function (e) {
      if (e.pointerType === "touch") return;
      nx = e.clientX; ny = e.clientY; queueNear();
    });
    document.documentElement.addEventListener("pointerleave", function () { nx = ny = -1e4; queueNear(); });

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
    /// Whether this is the reader's saved-bookmarks view.
    pub saved_view: bool,
}

impl<'a> HeaderContext<'a> {
    /// A default (unfiltered) header — used on entry/image pages, where every
    /// control simply links to the timeline with that filter applied.
    fn plain(cloud: &'a CloudStats, view: &'a ViewFilter) -> Self {
        HeaderContext { cloud, active_tag: None, view, base_path: "/", saved_view: false }
    }
}

const BOOKMARK_SVG: &str =
    r#"<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6.5 3.5h11V21l-5.5-4-5.5 4z"/></svg>"#;

/// Render the tag cloud from real stats.
/// Order the cloud "center-out": the busiest topic lands in the middle of the
/// sequence and sizes fall off toward both edges. Sort by count descending
/// (ties broken alphabetically by name), then alternate back/front into a
/// deque so the first (largest) sits centered. Presentation only.
fn center_out(tags: &[TagStat]) -> VecDeque<&TagStat> {
    let mut sorted: Vec<&TagStat> = tags.iter().collect();
    sorted.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    let mut ordered = VecDeque::with_capacity(sorted.len());
    for (i, tag) in sorted.into_iter().enumerate() {
        if i % 2 == 0 {
            ordered.push_back(tag);
        } else {
            ordered.push_front(tag);
        }
    }
    ordered
}

fn render_cloud(ctx: &HeaderContext) -> String {
    let mut out = String::new();
    for tag in center_out(&ctx.cloud.tags) {
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

    format!(
        r#"<header id="site">
<nav id="cloud" aria-label="Topics">{cloud}</nav>
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
<footer><button type="button" id="helpbtn"><kbd>?</kbd> shortcuts</button></footer>
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
        js = JS,
    )
}

// ---------------------------------------------------------------------------
// Date / label / href helpers
// ---------------------------------------------------------------------------

fn format_datetime_attr(ts: &chrono::NaiveDateTime) -> String {
    ts.format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// The one canonical (decoded) address for an entry.
///
/// - A unique label owns `/label`.
/// - When several entries share a label (a folder and a file, or an old and a
///   new version), the NEWEST owns the bare `/label`; the others carry the
///   shortest date prefix that tells them apart: `/2026-03-12/label` when the
///   day suffices, the full `/2026-03-12T133513/label` only as a last resort.
/// - Unlabeled entries live at their full timestamp.
///
/// Returned decoded (for comparing against decoded request paths); run it
/// through [`encode_path`] before emitting into an href or Location header.
pub fn canonical_path(entry: &Entry, all_entries: &[&Entry]) -> String {
    let full_ts = || entry.timestamp.format("%Y-%m-%dT%H%M%S").to_string();
    let label = match &entry.label {
        Some(label) => label,
        None => return format!("/{}", full_ts()),
    };

    let lower = label.to_lowercase();
    let twins: Vec<&&Entry> = all_entries
        .iter()
        .filter(|e| e.label.as_ref().map_or(false, |l| l.to_lowercase() == lower))
        .collect();

    if twins.len() <= 1 {
        return format!("/{}", label);
    }

    // Newest wins the bare label — unless the newest timestamp itself is tied.
    let newest = twins.iter().map(|e| e.timestamp).max().unwrap_or(entry.timestamp);
    let newest_is_unique = twins.iter().filter(|e| e.timestamp == newest).count() == 1;
    if entry.timestamp == newest && newest_is_unique {
        return format!("/{}", label);
    }

    let day = entry.timestamp.format("%Y-%m-%d").to_string();
    let same_day = twins
        .iter()
        .filter(|e| e.timestamp.format("%Y-%m-%d").to_string() == day)
        .count();
    if same_day == 1 {
        format!("/{}/{}", day, label)
    } else {
        format!("/{}/{}", full_ts(), label)
    }
}

/// Raw-file address for an entry: its canonical page address plus the
/// extension (the URL parser reads the extension back off the last segment).
fn canonical_raw_path(entry: &Entry, all_entries: &[&Entry]) -> String {
    let page = canonical_path(entry, all_entries);
    if entry.extension.is_empty() {
        page // folders have no raw file
    } else {
        format!("{}.{}", page, entry.extension)
    }
}

/// Percent-encode a decoded path for emission (href attribute, Location
/// header): each segment is encoded, slashes survive.
pub fn encode_path(path: &str) -> String {
    path.split('/')
        .map(percent_encode)
        .collect::<Vec<_>>()
        .join("/")
}

/// A stable per-entry key for the reader's localStorage bookmarks: the
/// canonical path — unique even across entries sharing a label.
fn bookmark_key(canonical: &str) -> &str {
    canonical.trim_start_matches('/')
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
    let canonical = canonical_path(entry, all_entries);
    let href = encode_path(&canonical);
    let key = bookmark_key(&canonical);

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

    format!(
        r#"<li><article data-key="{key}">
<aside>
<time datetime="{datetime}">{date}</time>
{star}<button type="button" aria-pressed="false" aria-label="Save for later (stays in this browser)" title="Save for later &mdash; stays in this browser">{bookmark}</button>
{meter}{tags}
</aside>
<div><h3><a href="{href}">{title}</a></h3></div>
</article></li>"#,
        key = html_escape(key),
        datetime = html_escape(&datetime),
        date = html_escape(&date),
        star = star,
        bookmark = BOOKMARK_SVG,
        meter = meter,
        tags = rail_tags(entry),
        href = html_escape(&href),
        title = title,
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
            rows.push_str(&format!(
                r#"<section><h2>{}</h2><ul>"#,
                html_escape(&month)
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
    let href = encode_path(&canonical_path(next, all_entries));
    let datetime = format_datetime_attr(&next.timestamp);
    let date = next.timestamp.format("%Y-%m-%d").to_string();
    let title = match next.display_label.as_deref().or(next.label.as_deref()) {
        Some(label) => html_escape(label),
        None => "(untitled)".to_string(),
    };
    format!(
        r#"<nav id="continue"><p>Continue</p><a href="{href}"><b>{title}</b><time datetime="{datetime}">{date}</time></a></nav>"#,
        href = html_escape(&href),
        title = title,
        datetime = html_escape(&datetime),
        date = html_escape(&date),
    )
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
    let canonical = canonical_path(entry, all_entries);
    let raw_href = encode_path(&canonical_raw_path(entry, all_entries));

    let body = format!(
        r#"{header}
{crumbs}
<main>
<article id="post" data-canonical="{canonical}">
{post_header}
<section>{content}</section>
<footer><a href="{raw_href}">source</a> <a href="/">timeline</a></footer>
</article>
{continue_nav}
</main>"#,
        header = render_site_header(&ctx),
        crumbs = crumbs(),
        canonical = html_escape(&encode_path(&canonical)),
        post_header = post_header(entry),
        content = rendered_html,
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
    let canonical = canonical_path(entry, all_entries);
    let src = encode_path(&canonical_raw_path(entry, all_entries));

    let body = format!(
        r#"{header}
{crumbs}
<main>
<article id="post" data-canonical="{canonical}">
{post_header}
<figure><img src="{src}" alt="{alt}"></figure>
<footer><a href="{src}">original</a> <a href="/">timeline</a></footer>
</article>
{continue_nav}
</main>"#,
        header = render_site_header(&ctx),
        crumbs = crumbs(),
        canonical = html_escape(&encode_path(&canonical)),
        post_header = post_header(entry),
        src = html_escape(&src),
        alt = html_escape(label),
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
    use chrono::NaiveDate;

    fn tag(name: &str, count: usize) -> TagStat {
        TagStat {
            name: name.to_string(),
            count,
            last_active: NaiveDate::from_ymd_opt(2026, 7, 5).unwrap(),
            color: 0,
        }
    }

    #[test]
    fn center_out_puts_largest_in_the_middle() {
        let tags = vec![
            tag("a", 1),
            tag("b", 2),
            tag("c", 3),
            tag("d", 4),
            tag("e", 5),
        ];
        // Sorted desc by count: e(5), d(4), c(3), b(2), a(1); alternating
        // back/front from the largest yields b, d, e, c, a with e centered.
        let order: Vec<&str> = center_out(&tags).iter().map(|t| t.name.as_str()).collect();
        assert_eq!(order, ["b", "d", "e", "c", "a"]);
    }

    #[test]
    fn center_out_breaks_count_ties_alphabetically() {
        let tags = vec![tag("zebra", 3), tag("apple", 3), tag("mango", 3)];
        // All equal counts: alphabetical order apple, mango, zebra, then
        // alternating back/front centers the first (apple): mango, apple, zebra.
        let order: Vec<&str> = center_out(&tags).iter().map(|t| t.name.as_str()).collect();
        assert_eq!(order, ["mango", "apple", "zebra"]);
    }
}
