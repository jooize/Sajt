use crate::entry::Entry;

const CSS: &str = r#"
:root {
  color-scheme: light dark;

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
  --color-card-bg: rgba(224, 244, 224, .5);
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
  --glass-shadow:
    inset 0 0 0 1px var(--glass-border),
    inset 0 1px 1px rgba(255, 255, 255, .55),
    0 2px 8px rgba(40, 50, 40, .06),
    0 10px 28px rgba(40, 50, 40, .05);
}

@media (prefers-color-scheme: dark) {
  :root {
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
    --color-card-bg: rgba(28, 44, 28, .42);
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
    --glass-shadow:
      inset 0 0 0 1px var(--glass-border),
      inset 0 1px 1px rgba(255, 255, 255, .07),
      0 2px 8px rgba(0, 0, 0, .28),
      0 10px 28px rgba(0, 0, 0, .22);
  }
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
  background-color: color-mix(in srgb, var(--color-card-bg), transparent 40%);
  color: var(--nav-pill-fg, rgba(161, 35, 246, 1));
  font-weight: 600;
  text-decoration: none;
  -webkit-backdrop-filter: blur(8px) saturate(150%);
  backdrop-filter: blur(8px) saturate(150%);
  border: none;
  box-shadow: var(--glass-shadow);
  transition: background-color .15s, box-shadow .15s;
}

body > nav > a:first-child:hover {
  background-color: color-mix(in srgb, var(--color-tag-bg), transparent 30%);
  text-decoration: none;
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
  background-color: color-mix(in srgb, var(--color-card-bg), transparent 40%);
  border: none;
  -webkit-backdrop-filter: blur(8px) saturate(150%);
  backdrop-filter: blur(8px) saturate(150%);
  box-shadow: var(--glass-shadow);
  color: var(--color-faint);
  cursor: pointer;
  user-select: none;
  margin-left: auto;
  transition: all .15s;
}

body > nav > label[for="pos"]:hover {
  background-color: color-mix(in srgb, var(--color-tag-bg), transparent 30%);
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

@media (max-width: 85ch) {
  body > nav > label[for="pos"] { display: none; }
}

/* Glass cards — all viewports */

main > article {
  position: relative;
  padding: .7em 1em .6em;
  background-color: var(--color-card-bg);
  border-radius: .75em;
  border: none;
  -webkit-backdrop-filter: blur(16px) saturate(125%);
  backdrop-filter: blur(16px) saturate(125%);
  box-shadow: var(--glass-shadow);
  transition: box-shadow .15s;
}

main > article.selected {
  outline: 1.5px solid var(--nav-pill-bg);
  outline-offset: -1px;
}

@media (min-width: 70ch) {
  main {
    padding: 1em;
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
  background-color: color-mix(in srgb, var(--color-tag-bg), transparent 20%);
  border-radius: 1em;
  padding: .25em .75em;
  color: var(--color-tag-fg);
  border: none;
  -webkit-backdrop-filter: blur(6px) saturate(150%);
  backdrop-filter: blur(6px) saturate(150%);
  box-shadow: var(--glass-shadow);
  transition: background-color .15s;
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

@media (prefers-color-scheme: dark) {
  :root {
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
</head>
<body>
<input type="checkbox" id="pos" checked>
<nav>
{nav_link}
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
