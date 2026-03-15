use crate::entry::Entry;

const CSS: &str = r#"
@property --spec-angle {
  syntax: '<angle>';
  inherits: true;
  initial-value: 160deg;
}

:root {
  color-scheme: light;

  --color-bg: #fff;
  --sunset-bg: linear-gradient(180deg, #e8e0f0, #f5d5c8 60%, #fce4b8) fixed;
  --color-fg: #313b3f;
  --color-muted: #555;
  --color-faint: #999;
  --color-link: #26a8ed;
  --color-link-hover: #1a8fd4;
  --color-date: green;
  --color-tag-bg: rgba(100, 155, 210, .55);
  --color-tag-hover: rgba(109, 90, 207, .65);
  --color-tag-fg: #fff;
  --color-card-bg: rgba(220, 245, 220, .18);
  --color-border: #e3e9ed;
  --color-code-bg: #e8ecf0;
  --color-blockquote-border: #e5eff5;
  --color-link-underline: #b4d5ee;
  --color-404: #ccc;

  --bg-blob-1: rgba(100, 140, 220, .12);
  --bg-blob-1-end: rgba(100, 140, 220, 0);
  --bg-blob-2: rgba(180, 100, 200, .08);
  --bg-blob-2-end: rgba(180, 100, 200, 0);
  --bg-blob-3: rgba(100, 200, 160, .06);
  --bg-blob-3-end: rgba(100, 200, 160, 0);
  --nav-pill-bg: rgba(161, 35, 246, .86);
  --nav-pill-hover: rgba(161, 35, 246, .99);
  --nav-pill-glow: rgba(161, 35, 246, .30);
  --toggle-pill-bg: rgba(220, 245, 220, .3);
  --toggle-pill-hover: rgba(200, 240, 200, .5);
  --glass-border: rgba(34, 139, 34, .2);
  --glass-highlight: rgba(34, 139, 34, .25);
  --glass-spec-hi: rgba(255, 255, 255, .55);
  --glass-spec-mid: rgba(255, 255, 255, .15);
  --glass-spec-lo: rgba(34, 139, 34, .08);
  --noise-opacity: .02;

}

:root[data-theme="dark"] {
    color-scheme: dark;
    --color-bg: #151515;
    --sunset-bg: linear-gradient(180deg, #1a1525, #2a1a2e 60%, #2d1f1a) fixed;
    --color-fg: #d4d4d4;
    --color-muted: #999;
    --color-faint: #666;
    --color-link: #6cb4ee;
    --color-link-hover: #9ccdff;
    --color-date: #5a5;
    --color-tag-bg: rgba(58, 90, 110, .6);
    --color-tag-hover: rgba(90, 74, 191, .6);
    --color-tag-fg: #c8dce8;
    --color-card-bg: rgba(25, 40, 25, .2);
    --color-border: #333;
    --color-code-bg: #232629;
    --color-blockquote-border: #333;
    --color-link-underline: #3a5a6e;
    --color-404: #444;

    --bg-blob-1: rgba(60, 80, 160, .25);
    --bg-blob-1-end: rgba(60, 80, 160, 0);
    --bg-blob-2: rgba(140, 60, 160, .18);
    --bg-blob-2-end: rgba(140, 60, 160, 0);
    --bg-blob-3: rgba(60, 160, 100, .12);
    --bg-blob-3-end: rgba(60, 160, 100, 0);
    --nav-pill-bg: rgba(161, 35, 246, .69);
    --nav-pill-hover: rgba(161, 35, 246, .84);
    --nav-pill-glow: rgba(161, 35, 246, .31);
    --toggle-pill-bg: rgba(60, 100, 60, .12);
    --toggle-pill-hover: rgba(70, 120, 70, .2);
    --glass-border: rgba(90, 170, 90, .18);
    --glass-highlight: rgba(90, 170, 90, .22);
    --glass-spec-hi: rgba(255, 255, 255, .35);
    --glass-spec-mid: rgba(255, 255, 255, .08);
    --glass-spec-lo: rgba(90, 170, 90, .05);
    --noise-opacity: .06;
    --glass-spec-opacity: .06;
}

/* Variant 2: warm glass */
:root[data-variant="2"] {
  --color-card-bg: rgba(245, 220, 200, .18);
  --glass-border: rgba(200, 140, 80, .2);
  --glass-highlight: rgba(245, 200, 150, .3);
  --glass-spec-lo: rgba(200, 140, 80, .08);
}
:root[data-theme="dark"][data-variant="2"] {
  --color-card-bg: rgba(45, 30, 25, .2);
  --glass-border: rgba(180, 120, 80, .18);
  --glass-highlight: rgba(200, 150, 100, .22);
  --glass-spec-lo: rgba(180, 120, 80, .05);
}

/* Variant 3: full sky gradient */
:root[data-variant="3"] {
  --sunset-bg: linear-gradient(180deg in oklab, #4a6fa5, #b07aa8 25%, #d4a080 50%, #d4644a 75%, #c0392b) fixed;
  --color-card-bg: rgba(245, 220, 200, .18);
  --glass-border: rgba(200, 140, 80, .2);
  --glass-highlight: rgba(245, 200, 150, .3);
  --glass-spec-lo: rgba(200, 140, 80, .08);
}
:root[data-theme="dark"][data-variant="3"] {
  --sunset-bg: linear-gradient(180deg, #0a0e1a, #1a1028 25%, #3a1a2e 50%, #3d1510 75%, #4a1a0a) fixed;
  --color-card-bg: rgba(45, 30, 25, .2);
  --glass-border: rgba(180, 120, 80, .18);
  --glass-highlight: rgba(200, 150, 100, .22);
  --glass-spec-lo: rgba(180, 120, 80, .05);
}

/* Variant 4: dynamic sun */
:root[data-variant="4"] {
  --color-card-bg: rgba(245, 220, 200, .18);
  --glass-border: rgba(200, 140, 80, .2);
  --glass-highlight: rgba(245, 200, 150, .3);
  --glass-spec-lo: rgba(200, 140, 80, .08);
  --color-faint: #666;
  --color-muted: #444;
}
:root[data-theme="dark"][data-variant="4"] {
  --color-card-bg: rgba(45, 30, 25, .2);
  --glass-border: rgba(180, 120, 80, .18);
  --glass-highlight: rgba(200, 150, 100, .22);
  --glass-spec-lo: rgba(180, 120, 80, .05);
  --color-faint: #aaa;
  --color-muted: #bbb;
}

/* Variant 6: raindrop-fx with photo background */
:root[data-variant="6"] {
  --color-card-bg: rgba(20, 20, 30, .55);
  --glass-border: rgba(120, 140, 180, .25);
  --glass-highlight: rgba(180, 200, 240, .2);
  --glass-spec-lo: rgba(100, 120, 160, .08);
  --color-fg: #e0e4ea;
  --color-muted: #bbb;
  --color-faint: #999;
  --color-date: #7ab;
  --color-tag-bg: rgba(60, 80, 120, .6);
  --color-tag-fg: #d0dae8;
}
:root[data-theme="dark"][data-variant="6"] {
  --color-card-bg: rgba(10, 10, 18, .6);
  --glass-border: rgba(80, 100, 140, .2);
  --glass-highlight: rgba(120, 140, 180, .15);
  --glass-spec-lo: rgba(60, 80, 120, .05);
}

/* Variant 7: raindrop-fx with demo background — same palette as 6 */
:root[data-variant="7"] {
  --color-card-bg: rgba(20, 20, 30, .55);
  --glass-border: rgba(120, 140, 180, .25);
  --glass-highlight: rgba(180, 200, 240, .2);
  --glass-spec-lo: rgba(100, 120, 160, .08);
  --color-fg: #e0e4ea;
  --color-muted: #bbb;
  --color-faint: #999;
  --color-date: #7ab;
  --color-tag-bg: rgba(60, 80, 120, .6);
  --color-tag-fg: #d0dae8;
}
:root[data-theme="dark"][data-variant="7"] {
  --color-card-bg: rgba(10, 10, 18, .6);
  --glass-border: rgba(80, 100, 140, .2);
  --glass-highlight: rgba(120, 140, 180, .15);
  --glass-spec-lo: rgba(60, 80, 120, .05);
}

/* Variant 5: CSS rain — reuses variant 4 palette */
:root[data-variant="5"] {
  --color-card-bg: rgba(245, 220, 200, .18);
  --glass-border: rgba(200, 140, 80, .2);
  --glass-highlight: rgba(245, 200, 150, .3);
  --glass-spec-lo: rgba(200, 140, 80, .08);
  --color-faint: #666;
  --color-muted: #444;
}
:root[data-theme="dark"][data-variant="5"] {
  --color-card-bg: rgba(45, 30, 25, .2);
  --glass-border: rgba(180, 120, 80, .18);
  --glass-highlight: rgba(200, 150, 100, .22);
  --glass-spec-lo: rgba(180, 120, 80, .05);
  --color-faint: #aaa;
  --color-muted: #bbb;
}

/* Variants 8, 9, 10, 11: winter glass — cool blue-white */
:root[data-variant="8"],
:root[data-variant="9"],
:root[data-variant="10"],
:root[data-variant="11"] {
  --color-card-bg: rgba(200, 220, 240, .18);
  --glass-border: rgba(120, 160, 200, .2);
  --glass-highlight: rgba(180, 210, 240, .3);
  --glass-spec-lo: rgba(120, 160, 200, .08);
  --color-tag-bg: rgba(100, 150, 200, .5);
  --color-date: #4a8ab5;
  --toggle-pill-bg: rgba(200, 220, 240, .3);
  --toggle-pill-hover: rgba(180, 210, 235, .5);
}
:root[data-theme="dark"][data-variant="8"],
:root[data-theme="dark"][data-variant="9"],
:root[data-theme="dark"][data-variant="10"],
:root[data-theme="dark"][data-variant="11"] {
  --color-card-bg: rgba(15, 20, 35, .25);
  --glass-border: rgba(100, 140, 190, .18);
  --glass-highlight: rgba(130, 170, 210, .22);
  --glass-spec-lo: rgba(100, 140, 190, .05);
  --color-tag-bg: rgba(50, 80, 120, .55);
  --color-date: #6aa0c5;
  --toggle-pill-bg: rgba(40, 60, 90, .15);
  --toggle-pill-hover: rgba(50, 80, 110, .25);
}

/* Snow variants: content above snow overlay, snow falls behind glass cards */
:root[data-variant="8"] nav,
:root[data-variant="9"] nav,
:root[data-variant="10"] nav,
:root[data-variant="11"] nav,
:root[data-variant="8"] main,
:root[data-variant="9"] main,
:root[data-variant="10"] main,
:root[data-variant="11"] main {
  position: relative;
  z-index: 3;
}

*, *::before, *::after {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

html {
  font-size: 62.5%;
  -webkit-text-size-adjust: 100%;
}

body {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Oxygen,
    Ubuntu, Cantarell, "Helvetica Neue", sans-serif;
  font-size: 1.6rem;
  line-height: 1.6;
  color: var(--color-fg);
  background: var(--sunset-bg);
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}


a {
  color: var(--color-link);
  text-decoration: none;
}

a:hover {
  color: var(--color-link-hover);
  text-decoration: underline;
}

/* Layout */

main {
  max-width: 70ch;
  margin-inline: auto;
}

/* Navigation bar */

body > nav {
  max-width: 70ch;
  margin-inline: auto;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: .75em 1em;
  gap: .5em;
}

body > nav > a:first-child {
  display: inline-block;
  font-size: .85em;
  padding: .3em .85em;
  border-radius: 1em;
  background: var(--color-card-bg);
  color: var(--nav-pill-fg, rgba(161, 35, 246, 1));
  font-weight: 600;
  text-decoration: none;
  -webkit-backdrop-filter: blur(10px) saturate(120%);
  backdrop-filter: blur(10px) saturate(120%);
  border: none;
  box-shadow:
    inset 0 1px 0 var(--glass-highlight),
    inset 0 -1px 0 rgba(0, 0, 0, .1);
  transition: background .15s, box-shadow .15s;
}

body > nav > a:first-child:hover {
  background: var(--color-tag-bg);
  text-decoration: none;
  box-shadow:
    0 2px 8px rgba(0, 0, 0, .06),
    inset 0 1px 0 var(--glass-highlight),
    inset 0 -1px 0 rgba(0, 0, 0, .1);
}

/* Position toggle — pure CSS checkbox hack */

#pos {
  position: absolute;
  opacity: 0;
  pointer-events: none;
}

body > nav > label[for="pos"] {
  display: inline-block;
  font-size: .85em;
  padding: .25em .7em;
  border-radius: 1em;
  background: var(--color-card-bg);
  border: none;
  -webkit-backdrop-filter: blur(10px) saturate(120%);
  backdrop-filter: blur(10px) saturate(120%);
  box-shadow:
    inset 0 1px 0 var(--glass-highlight),
    inset 0 -1px 0 rgba(0, 0, 0, .1);
  color: var(--color-faint);
  cursor: pointer;
  user-select: none;
  margin-left: auto;
  transition: all .15s;
}

body > nav > label[for="pos"]:hover {
  background: var(--color-tag-bg);
  color: var(--color-fg);
}

body > nav > label[for="pos"]::after {
  content: "\2190 left";
}

#pos:checked ~ nav {
  margin-inline: auto;
}

#pos:not(:checked) ~ nav {
  margin-inline: 0;
}

#pos:checked ~ main {
  margin-inline: auto;
}

#pos:not(:checked) ~ main {
  margin-inline: 0;
}

#pos:not(:checked) ~ nav > label[for="pos"]::after {
  content: "center \2192";
}

/* Theme toggle — opposite color of current mode */

body > nav > button#theme {
  appearance: none;
  border: none;
  font-size: .85em;
  padding: .25em .6em;
  border-radius: 1em;
  background: var(--color-fg);
  color: var(--color-bg);
  cursor: pointer;
  font-weight: 600;
  transition: opacity .15s;
  line-height: 1;
}

body > nav > button#theme:hover {
  opacity: .8;
}

body > nav > button.variant {
  appearance: none;
  border: none;
  font-size: .75em;
  padding: .2em .55em;
  border-radius: 1em;
  background: var(--color-card-bg);
  color: var(--color-faint);
  cursor: pointer;
  -webkit-backdrop-filter: blur(10px) saturate(120%);
  backdrop-filter: blur(10px) saturate(120%);
  box-shadow:
    inset 0 1px 0 var(--glass-highlight),
    inset 0 -1px 0 rgba(0, 0, 0, .1);
  transition: opacity .15s;
  line-height: 1;
}

body > nav > button.variant:hover {
  opacity: .8;
}

body > nav > button.variant.active {
  color: var(--color-fg);
  font-weight: 700;
  box-shadow:
    inset 0 1px 0 var(--glass-highlight),
    inset 0 -1px 0 rgba(0, 0, 0, .1),
    0 0 0 1px var(--glass-highlight);
}

#theme::after { content: "\263E"; }

body > nav > button#rain {
  appearance: none;
  border: none;
  font-size: .85em;
  padding: .25em .6em;
  border-radius: 1em;
  background: var(--color-card-bg);
  color: var(--color-faint);
  cursor: pointer;
  -webkit-backdrop-filter: blur(10px) saturate(120%);
  backdrop-filter: blur(10px) saturate(120%);
  box-shadow:
    inset 0 1px 0 var(--glass-highlight),
    inset 0 -1px 0 rgba(0, 0, 0, .1);
  transition: opacity .15s, color .15s;
  line-height: 1;
}

body > nav > button#rain:hover {
  opacity: .8;
}

body > nav > button#rain.active {
  color: var(--color-link);
}

#rain::after { content: "\1F4A7"; }

@media (max-width: 85ch) {
  body > nav > label[for="pos"] { display: none; }
}

/* Glass cards — all viewports */

main > article {
  position: relative;
  padding: .7em 1em .6em;
  background: var(--color-card-bg);
  border-radius: .75em;
  border: none;
  -webkit-backdrop-filter: blur(10px) saturate(120%);
  backdrop-filter: blur(10px) saturate(120%);
  box-shadow: 0 4px 16px rgba(0, 0, 0, .04);
  transition: box-shadow .15s, transform .3s ease-out;
  will-change: transform;
  transform: rotateX(0deg) rotateY(0deg);
  transform-style: flat;
}

main > article::before {
  content: "";
  position: absolute;
  inset: 0;
  border-radius: inherit;
  background: #fff;
  filter: url(#glass-spec);
  mix-blend-mode: screen;
  opacity: var(--glass-spec-opacity, .12);
  z-index: -1;
  pointer-events: none;
}

main > article::after {
  content: "";
  position: absolute;
  inset: 0;
  border-radius: inherit;
  padding: 1px;
  background: linear-gradient(
    var(--spec-angle),
    var(--glass-spec-hi) 0%,
    var(--glass-spec-mid) 35%,
    var(--glass-spec-lo) 65%,
    transparent 100%
  );
  -webkit-mask:
    linear-gradient(#fff 0 0) content-box,
    linear-gradient(#fff 0 0);
  -webkit-mask-composite: xor;
  mask:
    linear-gradient(#fff 0 0) content-box,
    linear-gradient(#fff 0 0);
  mask-composite: exclude;
  pointer-events: none;
}

main > article.selected {
  box-shadow:
    0 4px 16px rgba(0, 0, 0, .04),
    0 0 0 1px var(--nav-pill-bg);
}

main > article.selected::after {
  background: linear-gradient(
    var(--spec-angle),
    var(--nav-pill-bg) 0%,
    rgba(161, 35, 246, .3) 35%,
    transparent 100%
  );
}

@media (min-width: 70ch) {
  main {
    padding: 1em;
    perspective: 800px;
  }

  main > article.selected::before {
    content: "\276F";
    position: absolute;
    left: -.5em;
    top: 50%;
    transform: translate(-100%, -50%);
    color: var(--nav-pill-bg);
    font-size: 1.2em;
    font-weight: 700;
    text-shadow: 0 0 6px var(--nav-pill-glow), 0 0 14px var(--nav-pill-glow);
  }

  @keyframes arrow-nudge {
    0%, 100% { transform: translate(-100%, -50%); }
    50% { transform: translate(-50%, -50%); }
  }

  main > article.selected.entering::before {
    animation: arrow-nudge .15s ease-in-out;
  }
}

@media (max-width: 70ch) {
  main {
    padding: .5em 1em;
  }
}

/* Article */

article + article {
  margin-top: 1.2em;
}

article > header > time {
  display: block;
  color: var(--color-date);
  font-size: .8em;
  font-weight: 700;
  font-variant-numeric: tabular-nums;
}

article > header > h1 {
  margin: .1em 0 .3em;
  font-size: 2.5em;
  font-weight: 600;
  line-height: 1.15;
}

article > header > h2 {
  margin-top: .1em;
  margin-bottom: 0;
  font-size: 1.5em;
  font-weight: 600;
  line-height: 1.3;
}

article > header > h2 > a {
  color: inherit;
  text-decoration: none;
}

article > header > h2 > a:hover {
  color: var(--color-link);
  text-decoration: none;
}

/* Tags */

article > header > nav {
  font-size: .8em;
  padding-top: .5em;
}

article > header > nav > a {
  display: inline-block;
  background: var(--color-tag-bg);
  border-radius: 1em;
  padding: .25em .75em;
  color: var(--color-tag-fg);
  border: 1px solid var(--glass-border);
  -webkit-backdrop-filter: blur(6px) saturate(120%);
  backdrop-filter: blur(6px) saturate(120%);
  box-shadow: inset 0 1px 0 var(--glass-highlight);
  transition: background .15s;
}

article > header > nav > a:hover {
  text-decoration: none;
  background: var(--color-tag-hover);
}

/* Timeline entry preview */

article > a {
  color: var(--color-muted);
  text-decoration: none;
}

article > a:hover {
  color: var(--color-muted);
  text-decoration: none;
}

article > a > p {
  display: inline;
  font-size: .95em;
  line-height: 1.5;
}

/* Content (rendered entry) */

article > section p {
  margin: 0 0 1.5em;
}

article > section h2 {
  font-size: 1.8em;
  font-weight: 600;
  margin: .5em 0 .5em;
  line-height: 1.2;
}

article > section h3 {
  font-size: 1.4em;
  font-weight: 500;
  margin: .5em 0 .5em;
}

article > section h4,
article > section h5,
article > section h6 {
  font-size: 1.1em;
  font-weight: 500;
  margin: .5em 0 .5em;
}

article > section ul,
article > section ol {
  padding-left: 1.3em;
  margin: 0 0 1.5em;
}

article > section li {
  margin: .4em 0;
  line-height: 1.6;
}

article > section ul {
  list-style: disc;
}

article > section ol {
  list-style: decimal;
}

article > section blockquote {
  margin: 1.5em 0;
  padding: 0 1.6em;
  border-left: .3em solid var(--color-blockquote-border);
}

article > section blockquote p {
  font-size: 1.1em;
  font-weight: 300;
}

article > section pre {
  overflow-x: auto;
  padding: 1em;
  margin: 0 0 1.5em;
  background: var(--color-code-bg);
  border-radius: .4em;
  font-size: .85em;
  line-height: 1.5;
}

article > section code {
  font-family: "SF Mono", Menlo, Consolas, "Liberation Mono", monospace;
  font-size: .9em;
  background: var(--color-code-bg);
  padding: .15em .35em;
  border-radius: .25em;
}

article > section pre > code {
  background: none;
  padding: 0;
  font-size: inherit;
}

/* Pandoc Skylighting — syntax highlighting tokens */
/* kate theme (light), breezedark theme (dark) */

:root {
  --hl-keyword: #1f1c1b;
  --hl-keyword-weight: 700;
  --hl-datatype: #0057ae;
  --hl-function: #644a9b;
  --hl-string: #bf0303;
  --hl-char: #924c9d;
  --hl-specialchar: #3daee9;
  --hl-comment: #898887;
  --hl-annotation: #ca60ca;
  --hl-number: #b08000;
  --hl-operator: #1f1c1b;
  --hl-controlflow: #1f1c1b;
  --hl-controlflow-weight: 700;
  --hl-builtin: #644a9b;
  --hl-builtin-weight: 700;
  --hl-variable: #0057ae;
  --hl-preprocessor: #006e28;
  --hl-attribute: #0057ae;
  --hl-import: #ff5500;
  --hl-error: #bf0303;
  --hl-alert-fg: #bf0303;
  --hl-alert-bg: #f7e6e6;
  --hl-constant: #aa5500;
  --hl-specialstring: #ff5500;
  --hl-documentation: #607880;
  --hl-other: #006e28;
  --hl-information: #b08000;
}

:root[data-theme="dark"] {
    --hl-keyword: #cfcfc2;
    --hl-keyword-weight: 700;
    --hl-datatype: #2980b9;
    --hl-function: #8e44ad;
    --hl-string: #f44f4f;
    --hl-char: #3daee9;
    --hl-specialchar: #3daee9;
    --hl-comment: #7a7c7d;
    --hl-annotation: #3f8058;
    --hl-number: #f67400;
    --hl-operator: #cfcfc2;
    --hl-controlflow: #fdbc4b;
    --hl-controlflow-weight: 700;
    --hl-builtin: #7f8c8d;
    --hl-builtin-weight: normal;
    --hl-variable: #27aeae;
    --hl-preprocessor: #27ae60;
    --hl-attribute: #2980b9;
    --hl-import: #27ae60;
    --hl-error: #da4453;
    --hl-alert-fg: #95da4c;
    --hl-alert-bg: #4d1f24;
    --hl-constant: #27aeae;
    --hl-specialstring: #da4453;
    --hl-documentation: #a43340;
    --hl-other: #27ae60;
    --hl-information: #c45b00;
}

div.sourceCode { position: relative; }
pre.sourceCode { background: var(--color-code-bg); }

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

/* Line numbers in code blocks */
code > span > a { color: var(--color-faint); text-decoration: none; user-select: none; }

article > section img {
  max-width: 100%;
  height: auto;
  border-radius: .4em;
}

article > section hr {
  border: 0;
  border-top: 1px solid var(--color-border);
  margin: 2.5em 0 3.5em;
}

article > section a {
  text-decoration: underline;
  text-decoration-color: var(--color-link-underline);
  text-underline-offset: .15em;
}

article > section a:hover {
  text-decoration-color: currentColor;
}

/* Image viewer */

figure {
  margin-top: 1.5em;
  text-align: center;
}

figure > img {
  max-width: 100%;
  max-height: 85vh;
  border-radius: .4em;
}

figure > figcaption {
  margin-top: .5em;
  font-size: .85em;
  color: var(--color-faint);
}

/* Footer links */

article > footer {
  margin-top: 1.5em;
  font-size: .8em;
}

article > footer > a {
  color: var(--color-faint);
  margin-right: .75em;
}

article > footer > a:hover {
  color: var(--color-fg);
  text-decoration: none;
}

/* 404 */

.not-found {
  text-align: center;
  padding: 6em 0;
}

.not-found > h1 {
  font-size: 4em;
  font-weight: 600;
  color: var(--color-404);
}

.not-found > p {
  margin-top: .5em;
  color: var(--color-faint);
  font-size: 1.1em;
}

/* Weather overlays (variant 4, applied via JS body classes) */

/* Rain FX canvas — behind content, replaces body background */
#rain-fx {
  display: none;
  position: fixed;
  inset: 0;
  width: 100vw;
  height: 100vh;
  z-index: -1;
  pointer-events: none;
}

body.wx-rainfx {
  background: none !important;
}

body.wx-rainfx #rain-fx {
  display: block;
}

body.wx-snow::after {
  content: "";
  position: fixed;
  inset: -150px;
  pointer-events: none;
  z-index: 2;
  background-image:
    radial-gradient(2px 2px at 10% 20%, rgba(255,255,255,.7) 50%, transparent),
    radial-gradient(3px 3px at 40% 50%, rgba(255,255,255,.5) 50%, transparent),
    radial-gradient(1.5px 1.5px at 70% 15%, rgba(255,255,255,.6) 50%, transparent),
    radial-gradient(2.5px 2.5px at 25% 70%, rgba(255,255,255,.45) 50%, transparent),
    radial-gradient(2px 2px at 85% 40%, rgba(255,255,255,.5) 50%, transparent),
    radial-gradient(1px 1px at 55% 85%, rgba(255,255,255,.55) 50%, transparent);
  background-size: 200px 200px, 250px 300px, 180px 250px, 220px 280px, 190px 240px, 170px 220px;
  animation: wx-snow 4s linear infinite;
}

@keyframes wx-snow {
  from { transform: translateY(-150px) translateX(-15px); }
  to { transform: translateY(150px) translateX(15px); }
}

/* Rain: falling drops overlay (two parallax layers)
   Uniform tile height per layer so translateY = tile height = seamless loop.
   Coprime tile widths prevent visible horizontal repetition. */
body.wx-rain::before {
  content: "";
  position: fixed;
  inset: -150px;
  pointer-events: none;
  z-index: 1;
  background-image:
    radial-gradient(ellipse 1px 35px, rgba(180,200,225,.35), transparent),
    radial-gradient(ellipse 1px 28px, rgba(180,200,225,.25), transparent),
    radial-gradient(ellipse 1px 32px, rgba(180,200,225,.30), transparent),
    radial-gradient(ellipse 1px 22px, rgba(180,200,225,.20), transparent),
    radial-gradient(ellipse 1px 38px, rgba(180,200,225,.28), transparent);
  background-size: 90px 150px, 75px 150px, 120px 150px, 105px 150px, 60px 150px;
  background-position: 20px 0, 50px 30px, 85px 15px, 10px 60px, 65px 45px;
  opacity: .35;
  animation: wx-rain-back 2.5s linear infinite;
}

body.wx-rain::after {
  content: "";
  position: fixed;
  inset: -200px;
  pointer-events: none;
  z-index: 2;
  background-image:
    radial-gradient(ellipse 1.5px 55px, rgba(180,200,225,.4), transparent),
    radial-gradient(ellipse 1.5px 45px, rgba(180,200,225,.35), transparent),
    radial-gradient(ellipse 2px 60px, rgba(180,200,225,.38), transparent),
    radial-gradient(ellipse 1.5px 40px, rgba(180,200,225,.30), transparent),
    radial-gradient(ellipse 2px 50px, rgba(180,200,225,.33), transparent);
  background-size: 100px 180px, 80px 180px, 140px 180px, 60px 180px, 120px 180px;
  background-position: 30px 0, 70px 25px, 15px 50px, 95px 10px, 50px 70px;
  opacity: .5;
  animation: wx-rain-front 1.2s linear infinite;
}

@keyframes wx-rain-back {
  from { transform: translateY(-150px); }
  to   { transform: translateY(0); }
}

@keyframes wx-rain-front {
  from { transform: translateY(-180px); }
  to   { transform: translateY(0); }
}

/* Variant 8: CSS snow — two parallax layers
   Far layer (::before): 8 small dots, slow fall, blur
   Near layer (::after): 6 larger dots, faster fall
   Coprime tile widths prevent visible horizontal repetition.
   Uniform tile height per layer for seamless loop. */
body.wx-snow-v8::before {
  content: "";
  position: fixed;
  inset: -300px -50px;
  pointer-events: none;
  z-index: 1;
  opacity: .45;
  filter: blur(1px);
  background-image:
    radial-gradient(1.5px 1.5px at 15% 12%, #fff 50%, transparent),
    radial-gradient(1px 1px at 35% 38%, #fff 50%, transparent),
    radial-gradient(2px 2px at 55% 65%, #fff 50%, transparent),
    radial-gradient(1.2px 1.2px at 75% 22%, #fff 50%, transparent),
    radial-gradient(1.8px 1.8px at 22% 78%, #fff 50%, transparent),
    radial-gradient(1px 1px at 88% 48%, #fff 50%, transparent),
    radial-gradient(1.5px 1.5px at 45% 90%, #fff 50%, transparent),
    radial-gradient(1.3px 1.3px at 65% 5%, #fff 50%, transparent);
  background-size: 170px 300px, 190px 300px, 210px 300px, 230px 300px, 180px 300px, 200px 300px, 220px 300px, 160px 300px;
  animation: wx-snow-v8-far 12s linear infinite;
}
body.wx-snow-v8::after {
  content: "";
  position: fixed;
  inset: -200px -50px;
  pointer-events: none;
  z-index: 2;
  opacity: .7;
  background-image:
    radial-gradient(2.5px 2.5px at 20% 18%, #fff 50%, transparent),
    radial-gradient(3px 3px at 50% 55%, #fff 50%, transparent),
    radial-gradient(2.8px 2.8px at 80% 35%, #fff 50%, transparent),
    radial-gradient(2.5px 2.5px at 35% 75%, #fff 50%, transparent),
    radial-gradient(3px 3px at 65% 8%, #fff 50%, transparent),
    radial-gradient(2.6px 2.6px at 10% 50%, #fff 50%, transparent);
  background-size: 190px 200px, 210px 200px, 170px 200px, 230px 200px, 200px 200px, 180px 200px;
  animation: wx-snow-v8-near 7s linear infinite;
}
@keyframes wx-snow-v8-far {
  from { transform: translateY(-300px) translateX(-20px); }
  to   { transform: translateY(0) translateX(20px); }
}
@keyframes wx-snow-v8-near {
  from { transform: translateY(-200px) translateX(-15px); }
  to   { transform: translateY(0) translateX(15px); }
}
"#;

/// Wrap content in a full HTML page shell.
pub fn page_shell(title: &str, body: &str, show_timeline: bool) -> String {
    let nav_link = if show_timeline {
        r#"<a href="/">&#x2196; Timeline</a>"#
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
<script>!function(){{var t=localStorage.getItem("theme");if(!t)t=matchMedia("(prefers-color-scheme:dark)").matches?"dark":"light";document.documentElement.dataset.theme=t}}()</script>
</head>
<body>
<svg style="position:absolute;width:0;height:0" aria-hidden="true">
<filter id="glass-spec">
<feTurbulence type="fractalNoise" baseFrequency="0.012" numOctaves="2" seed="42" result="noise"/>
<feGaussianBlur in="noise" stdDeviation="3" result="smooth"/>
<feSpecularLighting in="smooth" surfaceScale="1.2" specularConstant="0.5" specularExponent="25" lighting-color="white" result="spec">
<feDistantLight azimuth="225" elevation="50"/>
</feSpecularLighting>
<feComposite in="spec" in2="SourceAlpha" operator="in"/>
</filter>
</svg>
<input type="checkbox" id="pos" checked>
<nav>
{nav_link}
<button id="theme" aria-label="Toggle theme"></button>
<button id="rain" aria-label="Toggle rain"></button>
<button class="variant" data-v="1">1</button>
<button class="variant" data-v="2">2</button>
<button class="variant" data-v="3">3</button>
<button class="variant" data-v="6">6</button>
<button class="variant" data-v="7">7</button>
<button class="variant" data-v="9">9</button>
<button class="variant" data-v="11">11</button>
<label for="pos"></label>
</nav>
<main>
{body}
</main>
<script>
!function(){{
var p=document.getElementById("pos"),s=localStorage.getItem("pos");
if(s!==null)p.checked=s==="1";
p.onchange=function(){{localStorage.setItem("pos",p.checked?"1":"0")}};
var btn=document.getElementById("theme");btn.onclick=function(){{var d=document.documentElement,n=d.dataset.theme==="dark"?"light":"dark";d.dataset.theme=n;localStorage.setItem("theme",n);sunTick();var dk=n==="dark";if(snowCanvas)snowCanvas.setDark(dk);if(snowGL)snowGL.setDark(dk);if(snowShader)snowShader.setDark(dk);if(cv==="8"||cv==="9"||cv==="10"||cv==="11")document.documentElement.style.setProperty("--sunset-bg",winterSky())}};
var manualRain=localStorage.getItem("rain")==="1";var rbtn=document.getElementById("rain");
var rainFx=null,rainLoaded=false;
function rainFxBg(){{
  var dk=document.documentElement.dataset.theme==="dark";
  var c=document.createElement("canvas");c.width=256;c.height=512;
  var ctx=c.getContext("2d");
  var g=ctx.createLinearGradient(0,0,0,512);
  if(dk){{
    g.addColorStop(0,"rgb(15,18,35)");g.addColorStop(.25,"rgb(30,35,55)");g.addColorStop(.5,"rgb(50,45,65)");g.addColorStop(.75,"rgb(60,40,50)");g.addColorStop(1,"rgb(40,30,40)");
  }}else{{
    g.addColorStop(0,"rgb(110,135,175)");g.addColorStop(.25,"rgb(155,170,195)");g.addColorStop(.5,"rgb(190,195,210)");g.addColorStop(.75,"rgb(210,195,185)");g.addColorStop(1,"rgb(195,185,175)");
  }}
  ctx.fillStyle=g;ctx.fillRect(0,0,256,512);
  return c;
}}
var rainGen=0,rainMode="";
function rainFxBgSrc(mode){{
  return mode==="photo"?"/static/rain-bg.jpg":mode==="demo"?"/static/rain-bg-demo.jpg":null;
}}
function rainFxStart(mode){{
  var m=mode||"gradient";
  if(rainFx&&rainMode!==m){{
    if(rainMode!=="gradient"&&m!=="gradient"){{
      // Photo <-> demo: swap background texture, keep WebGL context alive
      var src=rainFxBgSrc(m);
      if(src){{
        var img=new Image();img.onload=function(){{if(rainFx)rainFx.setBackground(img)}};img.src=src;
      }}else{{
        rainFx.setBackground(rainFxBg());
      }}
      rainMode=m;
      return;
    }}
    // Gradient <-> photo/demo: different options, full recreate with delay for WebGL cleanup
    rainFxStop();
    rainMode=m;
    var gen=rainGen;
    setTimeout(function(){{if(gen===rainGen)rainFxInit(m,gen)}},100);
    return;
  }}
  if(rainFx)return;
  var gen=++rainGen;
  rainMode=m;
  if(!rainLoaded){{
    var sc=document.createElement("script");sc.src="/static/raindrop-fx.js";
    sc.onload=function(){{rainLoaded=true;if(gen===rainGen)rainFxInit(m,gen)}};
    document.head.appendChild(sc);
  }}else{{rainFxInit(m,gen)}}
}}
function rainFxInit(mode,gen){{
  if(typeof RaindropFX==="undefined"||gen!==rainGen)return;
  var src=rainFxBgSrc(mode);
  if(src){{
    var img=new Image();
    img.onload=function(){{if(gen===rainGen)rainFxCreate(mode,img,gen)}};
    img.src=src;
  }}else{{
    rainFxCreate(mode,rainFxBg(),gen);
  }}
}}
function rainFxCreate(mode,bg,gen){{
  if(gen!==rainGen)return;
  var cv=document.createElement("canvas");cv.id="rain-fx";
  cv.style.display="none";
  cv.width=window.innerWidth;cv.height=window.innerHeight;
  document.body.insertBefore(cv,document.body.firstChild);
  rainFx=new RaindropFX({{canvas:cv,background:bg}});
  if(mode==="gradient"){{
    rainFx.options.spawnInterval=[0.05,0.12];
    rainFx.options.spawnSize=[40,100];
    rainFx.options.spawnLimit=1500;
    rainFx.options.mist=true;
    rainFx.options.mistColor=[0.5,0.55,0.6,0.3];
    rainFx.options.backgroundBlurSteps=4;
  }}
  rainFx.start();
  setTimeout(function(){{
    if(gen!==rainGen)return;
    cv.style.display="";
    document.body.classList.add("wx-rainfx");
  }},150);
  window.addEventListener("resize",rainFxResize);
}}
function rainFxResize(){{
  var cv=document.getElementById("rain-fx");
  if(rainFx&&cv){{cv.width=window.innerWidth;cv.height=window.innerHeight;rainFx.resize(window.innerWidth,window.innerHeight)}}
}}
function rainFxStop(){{
  rainGen++;
  rainMode="";
  window.removeEventListener("resize",rainFxResize);
  var el=document.getElementById("rain-fx");
  if(el){{
    try{{var gl=el.getContext("webgl2");if(gl){{var ext=gl.getExtension("WEBGL_lose_context");if(ext)ext.loseContext()}}}}catch(e){{}}
    el.remove();
  }}
  document.body.classList.remove("wx-rainfx");
  rainFx=null;
}}
/* Snow Canvas 2D (variant 9) */
var snowCanvas=null,snowCanvasLoaded=false;
function snowCanvasStart(){{
  if(snowCanvas)return;
  if(!snowCanvasLoaded){{
    var sc=document.createElement("script");sc.src="/static/snow-canvas.js";
    sc.onload=function(){{snowCanvasLoaded=true;snowCanvasCreate()}};
    document.head.appendChild(sc);
  }}else{{snowCanvasCreate()}}
}}
function snowCanvasCreate(){{
  if(typeof SnowCanvas==="undefined")return;
  var cv=document.createElement("canvas");cv.id="snow-canvas";
  cv.style.cssText="position:fixed;inset:0;z-index:2;pointer-events:none";
  cv.width=window.innerWidth;cv.height=window.innerHeight;
  document.body.appendChild(cv);
  var dk=document.documentElement.dataset.theme==="dark";
  var wind=wxData?wxData.wind_speed_10m:0;
  snowCanvas=new SnowCanvas(cv,{{dark:dk,wind:wind}});
  snowCanvas.start();
  window.addEventListener("resize",snowCanvasResize);
}}
function snowCanvasResize(){{
  var cv=document.getElementById("snow-canvas");
  if(snowCanvas&&cv){{cv.width=window.innerWidth;cv.height=window.innerHeight;snowCanvas.resize(window.innerWidth,window.innerHeight)}}
}}
function snowCanvasStop(){{
  window.removeEventListener("resize",snowCanvasResize);
  if(snowCanvas){{snowCanvas.stop();snowCanvas=null}}
  var el=document.getElementById("snow-canvas");if(el)el.remove();
}}
/* Snow WebGL (variant 10) */
var snowGL=null,snowGLLoaded=false;
function snowGLStart(){{
  if(snowGL)return;
  if(!snowGLLoaded){{
    var sc=document.createElement("script");sc.src="/static/snow-webgl.js";
    sc.onload=function(){{snowGLLoaded=true;snowGLCreate()}};
    document.head.appendChild(sc);
  }}else{{snowGLCreate()}}
}}
function snowGLCreate(){{
  if(typeof SnowGL==="undefined")return;
  var cv=document.createElement("canvas");cv.id="snow-webgl";
  cv.style.cssText="position:fixed;inset:0;z-index:2;pointer-events:none";
  cv.width=window.innerWidth;cv.height=window.innerHeight;
  document.body.appendChild(cv);
  var dk=document.documentElement.dataset.theme==="dark";
  var wind=wxData?wxData.wind_speed_10m:0;
  snowGL=new SnowGL(cv,{{dark:dk,wind:wind}});
  snowGL.start();
  window.addEventListener("resize",snowGLResize);
}}
function snowGLResize(){{
  var cv=document.getElementById("snow-webgl");
  if(snowGL&&cv){{cv.width=window.innerWidth;cv.height=window.innerHeight;snowGL.resize(window.innerWidth,window.innerHeight)}}
}}
function snowGLStop(){{
  window.removeEventListener("resize",snowGLResize);
  if(snowGL){{snowGL.stop();snowGL=null}}
  var el=document.getElementById("snow-webgl");
  if(el){{
    try{{var gl=el.getContext("webgl2");if(gl){{var ext=gl.getExtension("WEBGL_lose_context");if(ext)ext.loseContext()}}}}catch(e){{}}
    el.remove();
  }}
}}
/* Snow Shader 3D particles (variant 11) */
var snowShader=null,snowShaderLoaded=false;
function snowShaderStart(){{
  if(snowShader)return;
  if(!snowShaderLoaded){{
    var sc=document.createElement("script");sc.src="/static/snow-shader.js?v=7";
    sc.onload=function(){{snowShaderLoaded=true;snowShaderCreate()}};
    document.head.appendChild(sc);
  }}else{{snowShaderCreate()}}
}}
function snowShaderCreate(){{
  if(typeof SnowShader==="undefined")return;
  var holder=document.createElement("div");holder.id="snow-shader";
  holder.style.cssText="position:fixed;inset:0;width:100vw;height:100vh;z-index:2;pointer-events:none;overflow:hidden";
  document.body.appendChild(holder);
  var dk=document.documentElement.dataset.theme==="dark";
  var wind=wxData?wxData.wind_speed_10m:0;
  snowShader=new SnowShader(holder,{{dark:dk,wind:wind}});
  snowShader.start();
  window.addEventListener("resize",snowShaderResize);
}}
function snowShaderResize(){{
  if(snowShader)snowShader.resize();
}}
function snowShaderStop(){{
  window.removeEventListener("resize",snowShaderResize);
  if(snowShader){{snowShader.stop();snowShader=null}}
  var el=document.getElementById("snow-shader");if(el)el.remove();
}}
if(manualRain){{rbtn.classList.add("active");rainFxStart()}}
rbtn.onclick=function(){{manualRain=!manualRain;localStorage.setItem("rain",manualRain?"1":"0");rbtn.classList.toggle("active",manualRain);if(manualRain){{document.body.classList.remove("wx-snow");rainFxStart()}}else{{rainFxStop();wxUpdate()}}}};
var vbs=document.querySelectorAll("button.variant"),cv=localStorage.getItem("variant")||"1";
document.documentElement.dataset.variant=cv;
function winterSky(){{var dk=document.documentElement.dataset.theme==="dark";return dk?"linear-gradient(180deg, #1a1e2e, #202838 40%, #252d38 70%, #1e2228) fixed":"linear-gradient(180deg, #d0d8e8, #c5cfe0 40%, #b8c8d8 70%, #d0d0d5) fixed"}}
function snowAllStop(){{snowCanvasStop();snowGLStop();snowShaderStop();document.body.classList.remove("wx-snow-v8")}}
function setV(v){{cv=v;document.documentElement.dataset.variant=v;localStorage.setItem("variant",v);for(var i=0;i<vbs.length;i++)vbs[i].classList.toggle("active",vbs[i].dataset.v===v);if(v==="4"||v==="5"){{snowAllStop();rainFxStop();sunTick();if(v==="5")document.body.classList.add("wx-rain")}}else if(v==="6"){{snowAllStop();document.body.classList.remove("wx-snow","wx-rain");document.documentElement.style.removeProperty("--sunset-bg");rainFxStart("photo")}}else if(v==="7"){{snowAllStop();document.body.classList.remove("wx-snow","wx-rain");document.documentElement.style.removeProperty("--sunset-bg");rainFxStart("demo")}}else if(v==="8"){{snowAllStop();document.body.classList.remove("wx-rain","wx-snow");if(!manualRain)rainFxStop();document.body.classList.add("wx-snow-v8");document.documentElement.style.setProperty("--sunset-bg",winterSky())}}else if(v==="9"){{snowAllStop();document.body.classList.remove("wx-rain","wx-snow");if(!manualRain)rainFxStop();document.documentElement.style.setProperty("--sunset-bg",winterSky());snowCanvasStart()}}else if(v==="10"){{snowAllStop();document.body.classList.remove("wx-rain","wx-snow");if(!manualRain)rainFxStop();document.documentElement.style.setProperty("--sunset-bg",winterSky());snowGLStart()}}else if(v==="11"){{snowAllStop();document.body.classList.remove("wx-rain","wx-snow");if(!manualRain)rainFxStop();document.documentElement.style.setProperty("--sunset-bg",winterSky());snowShaderStart()}}else{{snowAllStop();document.documentElement.style.removeProperty("--sunset-bg");document.body.classList.remove("wx-snow","wx-rain");if(!manualRain)rainFxStop()}}}}
setV(cv);
for(var vi=0;vi<vbs.length;vi++)vbs[vi].onclick=function(){{setV(this.dataset.v)}};
var arts=Array.from(document.querySelectorAll("main > article")),sel=-1;
function pick(i){{
if(sel>=0&&sel<arts.length){{arts[sel].classList.remove("selected");arts[sel].classList.remove("entering")}}
sel=i;
if(sel>=0&&sel<arts.length){{arts[sel].classList.add("selected");arts[sel].scrollIntoView({{block:"nearest",behavior:"smooth"}})}}
}}
document.addEventListener("keydown",function(e){{
if(e.target.tagName==="INPUT"||e.target.tagName==="TEXTAREA"||e.target.isContentEditable)return;
var k=e.key;
if(k==="j"||k==="ArrowDown"){{
e.preventDefault();
if(arts.length<=1){{window.scrollBy(0,100);return}}
if(sel<0){{pick(0);return}}
if(sel>=arts.length-1){{window.scrollBy(0,100);return}}
pick(sel+1)
}}else if(k==="k"||k==="ArrowUp"){{
e.preventDefault();
if(arts.length<=1){{window.scrollBy(0,-100);return}}
if(sel<0){{pick(arts.length-1);return}}
if(sel<=0){{window.scrollBy(0,-100);return}}
pick(sel-1)
}}else if(k==="l"||k==="Enter"){{
if(sel>=0){{var a=arts[sel].querySelector("a[href]");if(a){{e.preventDefault();arts[sel].classList.add("entering");setTimeout(function(){{window.location.href=a.href}},120)}}}}
}}else if(k==="h"||k==="Backspace"){{
e.preventDefault();history.back()
}}
}});
window.addEventListener("pageshow",function(e){{if(e.persisted){{var el=document.querySelector(".entering");if(el)el.classList.remove("entering")}}}});

/* Dynamic glass pane effect */
var root=document.documentElement,raf=0,mql=matchMedia("(min-width:70ch)");
function glassMove(cx,cy,ww,wh){{
  var angle=Math.atan2(cy-wh/2,cx-ww/2)*180/Math.PI+90;
  root.style.setProperty("--spec-angle",angle+"deg");
  for(var i=0;i<arts.length;i++){{
    var r=arts[i].getBoundingClientRect();
    var rx=(cx-(r.left+r.width/2))/r.width;
    var ry=(cy-(r.top+r.height/2))/r.height;
    rx=Math.max(-1,Math.min(1,rx));
    ry=Math.max(-1,Math.min(1,ry));
    arts[i].style.transform="rotateY("+(rx*.5)+"deg) rotateX("+(-ry*.5)+"deg)";
  }}
}}
function glassReset(){{
  root.style.removeProperty("--spec-angle");
  for(var i=0;i<arts.length;i++){{
    arts[i].style.transform="";
  }}
}}

/* Mobile gyro glass effect */
var gyroActive=false;
function initGyro(){{
  if(gyroActive)return;
  gyroActive=true;
  window.addEventListener("deviceorientation",function(e){{
    if(raf)return;
    raf=requestAnimationFrame(function(){{
      raf=0;
      var g=Math.max(-45,Math.min(45,e.gamma||0));
      var b=Math.max(-45,Math.min(45,(e.beta||0)-45));
      var nx=g/45,ny=b/45;
      var angle=Math.atan2(ny,nx)*180/Math.PI+90;
      root.style.setProperty("--spec-angle",angle+"deg");
      for(var i=0;i<arts.length;i++){{
        arts[i].style.transform="rotateY("+(nx*.5)+"deg) rotateX("+(-ny*.5)+"deg)";
      }}
    }});
  }});
}}
if(typeof DeviceOrientationEvent!=="undefined"&&typeof DeviceOrientationEvent.requestPermission==="function"){{
  document.addEventListener("click",function once(){{
    DeviceOrientationEvent.requestPermission().then(function(s){{if(s==="granted")initGyro()}});
    document.removeEventListener("click",once);
  }});
}}else if("DeviceOrientationEvent" in window){{
  initGyro();
}}

/* Dynamic Sun — Luleå solar position */
function sunCalc(){{
  var now=new Date();
  var utcH=now.getUTCHours()+now.getUTCMinutes()/60+now.getUTCSeconds()/3600;
  var jan1=Date.UTC(now.getUTCFullYear(),0,1);
  var doy=Math.floor((Date.UTC(now.getUTCFullYear(),now.getUTCMonth(),now.getUTCDate())-jan1)/864e5)+1;
  var B=2*Math.PI/365*(doy-81);
  var declDeg=23.45*Math.sin(B);
  var declR=declDeg*Math.PI/180;
  var eot=9.87*Math.sin(2*B)-7.53*Math.cos(B)-1.5*Math.sin(B);
  var solarTime=utcH+22.1/15+eot/60;
  var ha=(solarTime-12)*15;
  var haR=ha*Math.PI/180;
  var lat=65.6*Math.PI/180;
  var sinEl=Math.sin(lat)*Math.sin(declR)+Math.cos(lat)*Math.cos(declR)*Math.cos(haR);
  var elev=Math.asin(sinEl)*180/Math.PI;
  var cosAz=(Math.sin(declR)-Math.sin(lat)*sinEl)/(Math.cos(lat)*Math.cos(Math.asin(sinEl)));
  cosAz=Math.max(-1,Math.min(1,cosAz));
  var az=Math.acos(cosAz)*180/Math.PI;
  if(ha>0)az=360-az;
  return{{elev:elev,azimuth:az}};
}}
function sunSky(elev,dk,wx){{
  /* Muted Nordic palette — misty whites, pale blue-greys */
  var S=[
    [-12,[15,17,35],[20,22,40],[25,22,38]],
    [-3,[40,45,70],[60,55,75],[80,65,65]],
    [0,[90,100,130],[150,145,155],[190,175,170]],
    [8,[160,175,200],[210,210,215],[230,225,220]],
    [20,[190,205,225],[230,232,235],[242,240,238]],
    [40,[200,215,235],[238,240,242],[248,247,245]]
  ];
  var lo=0,hi=0;
  if(elev<=S[0][0]){{lo=hi=0}}
  else if(elev>=S[S.length-1][0]){{lo=hi=S.length-1}}
  else{{for(var i=0;i<S.length-1;i++){{if(elev>=S[i][0]&&elev<S[i+1][0]){{lo=i;hi=i+1;break}}}}}}
  var t=(lo===hi)?0:(elev-S[lo][0])/(S[hi][0]-S[lo][0]);
  function lr(a,b,t){{return Math.round(a+(b-a)*t)}}
  function lc(a,b,t){{return[lr(a[0],b[0],t),lr(a[1],b[1],t),lr(a[2],b[2],t)]}}
  function dm(c){{return[Math.round(c[0]*.55),Math.round(c[1]*.55),Math.round(c[2]*.58)]}}
  function lift(c){{var l=c[0]*.2126+c[1]*.7152+c[2]*.0722;if(l>=160)return c;var f=(160-l)/160;return[Math.round(c[0]+(235-c[0])*f),Math.round(c[1]+(233-c[1])*f),Math.round(c[2]+(230-c[2])*f)]}}
  function bl(c,g,f){{return[Math.round(c[0]+(g[0]-c[0])*f),Math.round(c[1]+(g[1]-c[1])*f),Math.round(c[2]+(g[2]-c[2])*f)]}}
  var top=lc(S[lo][1],S[hi][1],t),mid=lc(S[lo][2],S[hi][2],t),bot=lc(S[lo][3],S[hi][3],t);
  if(dk){{top=dm(top);mid=dm(mid);bot=dm(bot)}}else{{top=lift(top);mid=lift(mid);bot=lift(bot)}}
  if(wx){{var cc=wx.cloud_cover/100,hum=wx.relative_humidity_2m/100;var haze=cc*.6+Math.max(0,hum-.7)*.3;var gr=dk?[120,122,125]:[210,212,215];top=bl(top,gr,haze);mid=bl(mid,gr,haze);bot=bl(bot,gr,haze)}}
  return "linear-gradient(180deg,rgb("+top+"),rgb("+mid+") 50%,rgb("+bot+")) fixed";
}}
var wxData=null,wxTime=0;
function wxFetch(){{
  var now=Date.now();
  if(wxData&&now-wxTime<900000)return;
  fetch("https://api.open-meteo.com/v1/forecast?latitude=65.6&longitude=22.1&current=weather_code,cloud_cover,relative_humidity_2m,precipitation,rain,snowfall,wind_speed_10m,temperature_2m")
  .then(function(r){{return r.json()}})
  .then(function(d){{wxData=d.current;wxTime=now;sunTick()}})
  .catch(function(){{}});
}}
function wxUpdate(){{
  document.body.classList.remove("wx-snow");
  if(cv==="5"){{document.body.classList.add("wx-rain");rainFxStop();return}}
  if(manualRain){{rainFxStart();return}}
  rainFxStop();
  if(cv!=="4"||!wxData)return;
  if(wxData.snowfall>0)document.body.classList.add("wx-snow");
  else if(wxData.rain>0||wxData.precipitation>0)rainFxStart();
}}
function sunTick(){{
  if(cv!=="4"&&cv!=="5")return;
  wxFetch();
  var s=sunCalc();
  var bg=sunSky(s.elev,document.documentElement.dataset.theme==="dark",wxData);
  document.documentElement.style.setProperty("--sunset-bg",bg);
  wxUpdate();
}}
setInterval(sunTick,60000);

}}();
</script>
</body>
</html>"#,
        title = html_escape(title),
        css = CSS,
        embed_css = crate::embed::EMBED_CSS,
        nav_link = nav_link,
        body = body,
    )
}

/// Format date with time, month name, and day name: "2026-03-03 17:00 (March, Tuesday)"
fn format_date_display(ts: &chrono::NaiveDateTime) -> String {
    ts.format("%Y-%m-%d %H:%M (%B, %A)").to_string()
}

/// Format datetime for the HTML `datetime` attribute (ISO 8601).
fn format_datetime_attr(ts: &chrono::NaiveDateTime) -> String {
    ts.format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// Permalink for an entry, using timestamp to disambiguate if label is not unique.
fn entry_href(entry: &Entry, unique_label: bool) -> String {
    match &entry.label {
        Some(label) if unique_label => format!("/{}", label),
        Some(label) => format!("/{}/{}", entry.timestamp.format("%Y-%m-%dT%H%M%S"), label),
        None => format!("/{}", entry.timestamp.format("%Y-%m-%dT%H%M%S")),
    }
}

/// Raw file URL for an entry.
fn entry_raw_href(entry: &Entry, unique_label: bool) -> String {
    match &entry.label {
        Some(label) if unique_label => format!("/{}.{}", label, entry.extension),
        Some(label) => format!("/{}/{}.{}", entry.timestamp.format("%Y-%m-%dT%H%M%S"), label, entry.extension),
        None => format!("/{}.{}", entry.timestamp.format("%Y-%m-%dT%H%M%S"), entry.extension),
    }
}

/// Count how many times each label appears in a set of entries.
fn label_counts(all_entries: &[&Entry]) -> std::collections::HashMap<String, usize> {
    let mut counts = std::collections::HashMap::new();
    for entry in all_entries {
        if let Some(ref label) = entry.label {
            *counts.entry(label.to_lowercase()).or_insert(0) += 1;
        }
    }
    counts
}

/// Check if an entry's label is unique across all entries.
fn is_label_unique(entry: &Entry, counts: &std::collections::HashMap<String, usize>) -> bool {
    match &entry.label {
        Some(label) => counts.get(&label.to_lowercase()).copied().unwrap_or(0) <= 1,
        None => false,
    }
}

/// Render tags as a <nav> with pill links. Empty string if no tags.
fn tags_nav(tags: &[String]) -> String {
    if tags.is_empty() {
        return String::new();
    }
    let pills: String = tags
        .iter()
        .map(|t| format!(r#"<a href="/+{tag}">{tag}</a>"#, tag = html_escape(t)))
        .collect::<Vec<_>>()
        .join(" ");
    format!("<nav>{}</nav>\n", pills)
}

/// Render the timeline page.
/// `all_entries` is used to determine label uniqueness for URL generation.
pub fn timeline_page(entries: &[&Entry], filter_desc: &str, all_entries: &[&Entry]) -> String {
    let title = if filter_desc.is_empty() {
        "Esko".to_string()
    } else {
        format!("{} — Esko", filter_desc)
    };

    let counts = label_counts(all_entries);

    let mut html = String::new();
    for entry in entries {
        let datetime = format_datetime_attr(&entry.timestamp);
        let date_display = format_date_display(&entry.timestamp);
        let unique = is_label_unique(entry, &counts);
        let href = entry_href(entry, unique);
        let tags = tags_nav(&entry.tags);
        let label = entry.display_label.as_deref()
            .or(entry.label.as_deref())
            .unwrap_or("(Untitled)");

        html.push_str(&format!(
            r#"<article>
<header>
<time datetime="{datetime}">{date_display}</time>
{tags}<h2><a href="{href}">{label}</a></h2>
</header>
</article>
"#,
            datetime = html_escape(&datetime),
            date_display = html_escape(&date_display),
            tags = tags,
            href = html_escape(&href),
            label = html_escape(label),
        ));
    }

    if html.is_empty() {
        html = "<p>No entries found.</p>".to_string();
    }

    page_shell(&title, &html, false)
}

/// Render a single entry page with rendered content.
pub fn entry_page(entry: &Entry, rendered_html: &str, label_unique: bool) -> String {
    let label = entry.display_label.as_deref()
        .or(entry.label.as_deref())
        .unwrap_or("Untitled");
    let datetime = format_datetime_attr(&entry.timestamp);
    let date_display = format_date_display(&entry.timestamp);
    let tags = tags_nav(&entry.tags);
    let raw_href = entry_raw_href(entry, label_unique);

    let body = format!(
        r#"<article>
<header>
<time datetime="{datetime}">{date_display}</time>
{tags}
</header>
<section>{content}</section>
<footer><a href="{raw_href}">source</a> <a href="/">timeline</a></footer>
</article>"#,
        datetime = html_escape(&datetime),
        date_display = html_escape(&date_display),
        tags = tags,
        content = rendered_html,
        raw_href = html_escape(&raw_href),
    );

    page_shell(label, &body, true)
}

/// Render an image viewer page.
pub fn image_page(entry: &Entry, _mime: &str, label_unique: bool) -> String {
    let label = entry.label.as_deref().unwrap_or("Image");
    let datetime = format_datetime_attr(&entry.timestamp);
    let date_display = format_date_display(&entry.timestamp);
    let tags = tags_nav(&entry.tags);
    let src = entry_raw_href(entry, label_unique);

    let body = format!(
        r#"<article>
<header>
<time datetime="{datetime}">{date_display}</time>
{tags}
</header>
<figure>
<img src="{src}" alt="{alt}">
</figure>
<footer><a href="{src}">original</a> <a href="/">timeline</a></footer>
</article>"#,
        datetime = html_escape(&datetime),
        date_display = html_escape(&date_display),
        tags = tags,
        src = html_escape(&src),
        alt = html_escape(label),
    );

    page_shell(label, &body, true)
}

/// Render the 404 page.
pub fn not_found_page() -> String {
    let body = r#"<article>
<div class="not-found">
<h1>404</h1>
<p>Nothing here.</p>
</div>
</article>"#;
    page_shell("Not Found — Esko", body, true)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
