//! Local-only pairwise grading tool (the `grade` subcommand).
//!
//! This is a SEPARATE web server from the public site (`routes.rs`). The public
//! server is strictly read-only on the content tree; this grader is the one
//! sanctioned writer, and it writes exactly one file — the append-only judgement
//! ledger `<content_dir>/Sajt-Grade-Judgements.jsonl` that `grade.rs`
//! reads. It never mutates a post. It binds `127.0.0.1` only and must never be
//! proxied (it is deliberately absent from the Caddyfile).
//!
//! Flow (ported from `static/entries-site.html`, the "place your post" dialog):
//! pick a post to place, then compare it head-to-head against a pivot post —
//! each rendered exactly as it appears on the live site — and answer "better" or
//! "worse". A binary search over the current ranking settles the post in about
//! `ceil(log2(n+1))` comparisons; every answer records one pairwise judgement.
//! On settle, the exact ledger lines are shown and can be appended.
//!
//! State (the in-flight placement) lives in server memory: this is a single
//! local user, and a page reload simply restarts the current placement.

use axum::body::Body;
use axum::extract::{Form, Path as AxumPath, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::Router;
use chrono::SecondsFormat;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::RwLock;

use sajt::content::ContentStore;
use sajt::entry::Entry;
use sajt::render::{render_entry, RenderedContent};
use sajt::templates;

/// The single file this tool ever writes, relative to the content root. Must
/// match `grade::LEDGER_STEM` + `.jsonl` — the exact name the public server
/// reads. The write guard (`append_judgements`) asserts the full path.
const LEDGER_FILENAME: &str = "Sajt-Grade-Judgements.jsonl";

// ─── Server state ────────────────────────────────────────────────────────────

type Shared = Arc<RwLock<GraderState>>;

struct GraderState {
    /// The content root — the only directory the ledger may live in. The
    /// disposable cache root (kept OUTSIDE this tree, so scanning never writes
    /// into content) lives on `store` itself.
    content_dir: PathBuf,
    store: ContentStore,
    /// The in-flight placement, if a post is currently being graded.
    placement: Option<Placement>,
}

/// One pairwise judgement recorded during a placement: `winner` beat `loser` at
/// instant `at`. These are the exact lines the placement will append.
#[derive(Clone)]
struct Recorded {
    winner: String,
    loser: String,
    at: String,
}

/// A binary-search insertion in progress. `ranked` is the current ordering of
/// every OTHER gradable post (best first); the target is being placed into it.
/// `lo`/`hi` bound the insertion point; `mid = (lo + hi) / 2` is the pivot.
struct Placement {
    /// Canonical label of the post being placed.
    target: String,
    /// Labels of the other posts, best-first (graded desc, ungraded last).
    ranked: Vec<String>,
    lo: usize,
    hi: usize,
    /// Judgements recorded so far (one per comparison answered).
    judgements: Vec<Recorded>,
}

impl GraderState {
    /// Re-scan the content tree (which also re-derives grades from the ledger),
    /// so a placement started after an append reflects the new judgements.
    fn rescan(&mut self) {
        if let Err(e) = self.store.rescan() {
            tracing::error!("Grader rescan failed: {}", e);
        }
    }

    /// Every post that can be graded: error-free, labeled, and de-duplicated by
    /// label to the OLDEST claimant (matching the site's oldest-claim-wins and
    /// the ledger's label-keyed identity). Newest first.
    fn gradable(&self) -> Vec<&Entry> {
        use std::collections::HashMap;
        let mut by_label: HashMap<String, &Entry> = HashMap::new();
        for e in &self.store.entries {
            if e.error.is_some() {
                continue;
            }
            let Some(label) = e.label.as_deref() else {
                continue;
            };
            let key = label.to_lowercase();
            match by_label.get(&key) {
                // Keep the oldest claimant, as the site and ledger do.
                Some(prev) if prev.timestamp <= e.timestamp => {}
                _ => {
                    by_label.insert(key, e);
                }
            }
        }
        let mut v: Vec<&Entry> = by_label.into_values().collect();
        v.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        v
    }

    /// Find a gradable post by label (case-insensitive).
    fn find(&self, label: &str) -> Option<&Entry> {
        self.gradable()
            .into_iter()
            .find(|e| e.label.as_deref().map_or(false, |l| l.eq_ignore_ascii_case(label)))
    }

    /// Rank every OTHER gradable post to search against: graded posts by grade
    /// descending, then ungraded posts (which have no grade to compare) last, in
    /// timeline order. With nothing graded yet this is just the timeline.
    fn ranking(&self, target: &str) -> Vec<String> {
        let mut others: Vec<&Entry> = self
            .gradable()
            .into_iter()
            .filter(|e| e.label.as_deref().map_or(true, |l| !l.eq_ignore_ascii_case(target)))
            .collect();
        // Stable sort: ungraded posts keep their newest-first order.
        others.sort_by(|a, b| grade_key(b).partial_cmp(&grade_key(a)).unwrap_or(Ordering::Equal));
        others
            .into_iter()
            .filter_map(|e| e.label.clone())
            .collect()
    }
}

/// Sort key for ranking: the grade, or `-1.0` for ungraded so they fall last.
fn grade_key(e: &Entry) -> f32 {
    e.grade.unwrap_or(-1.0)
}

// ─── Pure binary-search step (unit-tested) ───────────────────────────────────

/// One binary-search step. Given the current `[lo, hi)` insertion bounds and
/// whether the target beat the pivot at `mid = (lo + hi) / 2`, return the new
/// bounds: "better" narrows the upper half (`hi = mid`), "worse" the lower half
/// (`lo = mid + 1`). Converges in `ceil(log2(hi - lo + 1))` steps to `lo == hi`.
fn step(lo: usize, hi: usize, target_better: bool) -> (usize, usize) {
    let mid = (lo + hi) / 2;
    if target_better {
        (lo, mid)
    } else {
        (mid + 1, hi)
    }
}

/// The (winner, loser) a comparison records: if the target being placed beat the
/// pivot, the target is the winner; otherwise the pivot is.
fn winner_of<'a>(target: &'a str, pivot: &'a str, target_better: bool) -> (&'a str, &'a str) {
    if target_better {
        (target, pivot)
    } else {
        (pivot, target)
    }
}

/// The estimated number of comparisons to place a post among `n` others:
/// `ceil(log2(n + 1))`, at least 1. Purely for the progress readout.
fn comparisons_estimate(n: usize) -> usize {
    (((n + 1) as f64).log2().ceil() as usize).max(1)
}

// ─── Ledger append (the only write; guarded, unit-tested) ────────────────────

/// The one file this tool may write.
fn ledger_path(content_dir: &Path) -> PathBuf {
    content_dir.join(LEDGER_FILENAME)
}

/// One ledger line, serialized in the exact field order the format documents:
/// `{"winner":"…","loser":"…","at":"…"}`. `grade::Judgement` parses it back.
#[derive(Serialize)]
struct LedgerLine<'a> {
    winner: &'a str,
    loser: &'a str,
    at: &'a str,
}

fn render_line(r: &Recorded) -> String {
    serde_json::to_string(&LedgerLine {
        winner: &r.winner,
        loser: &r.loser,
        at: &r.at,
    })
    // A struct of three plain strings cannot fail to serialize; fall back to a
    // hand-built object rather than panic if that assumption ever breaks.
    .unwrap_or_else(|_| {
        format!(
            r#"{{"winner":{},"loser":{},"at":{}}}"#,
            json_str(&r.winner),
            json_str(&r.loser),
            json_str(&r.at)
        )
    })
}

/// A JSON string literal for a single value (fallback path only).
fn json_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

/// Append complete JSONL lines to the grade ledger — the ONLY write this tool
/// performs. `target` MUST be exactly `<content_dir>/Sajt-Grade-Judgements
/// .jsonl`; any other path is refused before a single byte is written (fail
/// closed). The file is created if absent. Each line gets a trailing newline.
fn append_judgements(content_dir: &Path, target: &Path, lines: &[String]) -> std::io::Result<()> {
    let expected = ledger_path(content_dir);
    if target != expected {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "refusing to write {}: the grader may only write the ledger {}",
                target.display(),
                expected.display()
            ),
        ));
    }
    if lines.is_empty() {
        return Ok(());
    }
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(target)?;
    for line in lines {
        f.write_all(line.as_bytes())?;
        f.write_all(b"\n")?;
    }
    f.flush()
}

// ─── HTTP handlers ───────────────────────────────────────────────────────────

/// GET / — the landing page: pick a post to place. Ungraded posts are surfaced
/// first (the natural things to place); graded posts can be re-placed.
async fn landing(State(app): State<Shared>) -> Response {
    let site = sajt::config::site();
    let st = app.read().await;
    let posts = st.gradable();
    let (ungraded, graded): (Vec<&Entry>, Vec<&Entry>) =
        posts.iter().partition(|e| e.grade.is_none());

    let li = |e: &Entry| -> String {
        let label = e.label.as_deref().unwrap_or("");
        let grade = match e.grade {
            Some(g) => format!(r#" <span class="g-grade">{}%</span>"#, (g * 100.0).round() as i64),
            None => String::new(),
        };
        format!(
            r#"<li><a href="/place/{href}">{name}</a> <span class="g-kind">{kind}</span>{grade}</li>"#,
            href = href_label(label),
            name = esc(label),
            kind = esc(e.kind()),
            grade = grade,
        )
    };

    let ungraded_html: String = ungraded.iter().map(|e| li(e)).collect();
    let graded_html: String = graded.iter().map(|e| li(e)).collect();

    let body = format!(
        r#"<div id="gtop">
<h1>{site_name} grading</h1>
<span class="g-prog">{total} gradable post{plural}</span>
<form method="post" action="/refresh" class="g-right"><button class="g-btn" type="submit">Refresh</button></form>
</div>
<div id="glist">
<h2>Ungraded &mdash; place these ({u})</h2>
{ungraded_list}
<h2>Graded &mdash; re-place ({g})</h2>
{graded_list}
</div>"#,
        site_name = esc(&site.name),
        total = posts.len(),
        plural = if posts.len() == 1 { "" } else { "s" },
        u = ungraded.len(),
        g = graded.len(),
        ungraded_list = if ungraded.is_empty() {
            r#"<p class="g-empty">None &mdash; every post is graded.</p>"#.to_string()
        } else {
            format!("<ul>{}</ul>", ungraded_html)
        },
        graded_list = if graded.is_empty() {
            r#"<p class="g-empty">None yet.</p>"#.to_string()
        } else {
            format!("<ul>{}</ul>", graded_html)
        },
    );
    Html(shell(&format!("{} grading", site.name), &body)).into_response()
}

/// POST /refresh — re-scan the content tree (picks up new posts and any ledger
/// changes), then return to the landing page.
async fn refresh(State(app): State<Shared>) -> Response {
    app.write().await.rescan();
    Redirect::to("/").into_response()
}

/// GET /place/{label} — begin placing a post: build the ranking of the others
/// and reset the binary search, then hand off to the comparison view.
async fn place(State(app): State<Shared>, AxumPath(label): AxumPath<String>) -> Response {
    let mut st = app.write().await;

    // Resolve to the canonical stored label (or bail to the landing page).
    let target = match st.find(&label).and_then(|e| e.label.clone()) {
        Some(t) => t,
        None => return Redirect::to("/").into_response(),
    };
    let ranked = st.ranking(&target);
    let hi = ranked.len();
    st.placement = Some(Placement {
        target,
        ranked,
        lo: 0,
        hi,
        judgements: Vec::new(),
    });
    Redirect::to("/compare").into_response()
}

/// What the comparison GET needs to render, gathered under the read lock so the
/// (async, pandoc-shelling) render happens after the lock is released.
enum CompareView {
    None,
    Compare {
        target: Entry,
        pivot: Entry,
        target_label: String,
        step_no: usize,
        total: usize,
    },
    Settle {
        content_dir: PathBuf,
        target: String,
        ranked: Vec<String>,
        insert_at: usize,
        lines: Vec<String>,
    },
}

/// GET /compare — show the current head-to-head, or the settle screen once the
/// binary search has converged.
async fn compare_get(State(app): State<Shared>) -> Response {
    let view = {
        let st = app.read().await;
        let Some(p) = &st.placement else {
            return Redirect::to("/").into_response();
        };
        if p.lo >= p.hi {
            CompareView::Settle {
                content_dir: st.content_dir.clone(),
                target: p.target.clone(),
                ranked: p.ranked.clone(),
                insert_at: p.lo,
                lines: p.judgements.iter().map(render_line).collect(),
            }
        } else {
            let mid = (p.lo + p.hi) / 2;
            let pivot_label = p.ranked[mid].clone();
            match (st.find(&p.target).cloned(), st.find(&pivot_label).cloned()) {
                (Some(target), Some(pivot)) => CompareView::Compare {
                    target,
                    pivot,
                    target_label: p.target.clone(),
                    step_no: p.judgements.len(),
                    total: comparisons_estimate(p.ranked.len()),
                },
                _ => CompareView::None,
            }
        }
    };

    match view {
        CompareView::None => Redirect::to("/").into_response(),
        CompareView::Settle {
            content_dir,
            target,
            ranked,
            insert_at,
            lines,
        } => Html(render_settle(&content_dir, &target, &ranked, insert_at, &lines)).into_response(),
        CompareView::Compare {
            target,
            pivot,
            target_label,
            step_no,
            total,
        } => {
            let pivot_label = pivot.label.clone().unwrap_or_default();
            let left = render_post_body(&target).await;
            let right = render_post_body(&pivot).await;
            Html(render_compare(&target_label, &pivot_label, &left, &right, step_no, total))
                .into_response()
        }
    }
}

#[derive(Deserialize)]
struct ChoiceForm {
    choice: String,
}

/// POST /compare — record the answer as one judgement, advance the binary
/// search, and redirect back to GET /compare (post-redirect-get, so a reload
/// never re-submits the same comparison).
async fn compare_post(State(app): State<Shared>, Form(form): Form<ChoiceForm>) -> Response {
    {
        let mut st = app.write().await;
        let Some(p) = st.placement.as_mut() else {
            return Redirect::to("/").into_response();
        };
        if p.lo >= p.hi {
            return Redirect::to("/compare").into_response();
        }
        let mid = (p.lo + p.hi) / 2;
        let pivot = p.ranked[mid].clone();
        let target = p.target.clone();
        let better = form.choice == "better";

        let (winner, loser) = winner_of(&target, &pivot, better);
        let at = chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
        p.judgements.push(Recorded {
            winner: winner.to_string(),
            loser: loser.to_string(),
            at,
        });

        let (lo, hi) = step(p.lo, p.hi, better);
        p.lo = lo;
        p.hi = hi;
    }
    Redirect::to("/compare").into_response()
}

/// POST /append — write the placement's judgements to the ledger (the only
/// write), then confirm exactly what was written and clear the placement.
async fn append_post(State(app): State<Shared>) -> Response {
    let (content_dir, target, lines) = {
        let st = app.read().await;
        let Some(p) = &st.placement else {
            return Redirect::to("/").into_response();
        };
        let lines: Vec<String> = p.judgements.iter().map(render_line).collect();
        (st.content_dir.clone(), ledger_path(&st.content_dir), lines)
    };

    match append_judgements(&content_dir, &target, &lines) {
        Ok(()) => {
            // Re-scan so a subsequent placement reflects the new grades, and
            // clear the settled placement.
            let mut st = app.write().await;
            st.rescan();
            st.placement = None;
            Html(render_appended(&target, &lines)).into_response()
        }
        Err(e) => Html(shell(
            "Ledger write failed",
            &format!(
                r#"<div id="gtop"><h1>Ledger write failed</h1></div>
<div id="glist"><p class="g-empty">{}</p><p><a class="g-btn" href="/compare">Back</a></p></div>"#,
                esc(&e.to_string())
            ),
        ))
        .into_response(),
    }
}

/// GET /_raw/{label} — serve a post's own bytes, for image parity. Traversal-safe
/// by construction: `label` is resolved to a known entry (via the site's own
/// oldest-claim resolver) and only that entry's primary file is served — no
/// caller-supplied path ever reaches the filesystem.
async fn raw_bytes(State(app): State<Shared>, AxumPath(label): AxumPath<String>) -> Response {
    let st = app.read().await;
    let all: Vec<&Entry> = st.store.entries.iter().collect();
    let entry = match templates::name_owner(&label, &all) {
        Some(e) => e,
        None => return not_found(),
    };
    let bytes = match std::fs::read(&entry.path) {
        Ok(b) => b,
        Err(_) => return not_found(),
    };
    let mime = mime_guess::from_ext(&entry.extension)
        .first_or_octet_stream()
        .to_string();
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_str(&mime).unwrap_or(HeaderValue::from_static("application/octet-stream")),
        )
        .body(Body::from(bytes))
        .unwrap_or_else(|_| not_found())
}

// ─── Rendering ───────────────────────────────────────────────────────────────

/// Render one post's body exactly as the live site would: the same
/// `render_entry` pipeline and the same `main > article` structure and CSS.
/// Standalone HTML posts render inside a sandboxed iframe so their CSS cannot
/// bleed into the grader chrome; images point at the `/_raw` byte route.
async fn render_post_body(entry: &Entry) -> String {
    let label = entry.label.as_deref().unwrap_or("");
    let bytes = match std::fs::read(&entry.path) {
        Ok(b) => b,
        Err(e) => {
            return templates::post_body_fragment(
                entry,
                &format!("<p>Could not read the post file: {}</p>", esc(&e.to_string())),
            )
        }
    };

    match render_entry(&entry.extension, &bytes).await {
        Ok(RenderedContent::Html(html)) => templates::post_body_fragment(entry, &html),
        Ok(RenderedContent::Standalone(doc)) => standalone_iframe(&doc),
        Ok(RenderedContent::PreformattedText(text)) => {
            templates::post_body_fragment(entry, &format!("<pre>{}</pre>", esc(&text)))
        }
        Ok(RenderedContent::Embed(card)) => templates::post_body_fragment(entry, &card),
        Ok(RenderedContent::Image { .. }) => {
            let inner = format!(
                r#"<figure><img src="/_raw/{href}" alt="{alt}"></figure>"#,
                href = href_label(label),
                alt = esc(label),
            );
            templates::post_body_fragment(entry, &inner)
        }
        Ok(RenderedContent::Download { mime }) => {
            let inner = format!(
                r#"<p>Downloadable file ({mime}). <a href="/_raw/{href}">open</a></p>"#,
                mime = esc(&mime),
                href = href_label(label),
            );
            templates::post_body_fragment(entry, &inner)
        }
        Err(e) => templates::post_body_fragment(entry, &format!("<p>Rendering error: {}</p>", esc(&e))),
    }
}

/// A complete HTML document rendered in a sandboxed iframe: `allow-scripts` (so
/// pages that build themselves still render) but a unique opaque origin (no
/// `allow-same-origin`), so it cannot reach the grader page, and its CSS is
/// fully contained. The doc is trusted (same author pipeline as the live site).
fn standalone_iframe(doc: &str) -> String {
    format!(
        r#"<div class="g-frame-wrap"><iframe class="g-frame" sandbox="allow-scripts" srcdoc="{}"></iframe></div>"#,
        srcdoc_escape(doc)
    )
}

/// Render the head-to-head comparison page: the target on the LEFT, the pivot on
/// the RIGHT, each independently scrollable, with the answer buttons fixed on top.
fn render_compare(
    target_label: &str,
    pivot_label: &str,
    left_body: &str,
    right_body: &str,
    step_no: usize,
    total: usize,
) -> String {
    let body = format!(
        r#"<form id="gtop" method="post" action="/compare">
<h1>Placing: {target}</h1>
<span class="g-prog">Comparison {n} of ~{total}</span>
<div class="g-btns">
<button class="g-btn up" name="choice" value="better" type="submit">&#9650; LEFT is better</button>
<button class="g-btn down" name="choice" value="worse" type="submit">&#9660; LEFT is worse</button>
</div>
<a class="g-skip g-right" href="/">Skip</a>
</form>
<div id="gpair">
<div class="gcol">
<div class="g-banner">LEFT &mdash; placing <b>{target}</b></div>
{left}
</div>
<div class="gcol">
<div class="g-banner">RIGHT &mdash; compare against <b>{pivot}</b></div>
{right}
</div>
</div>"#,
        target = esc(target_label),
        pivot = esc(pivot_label),
        n = step_no + 1,
        total = total,
        left = left_body,
        right = right_body,
    );
    shell(&format!("Placing {}", target_label), &body)
}

/// Render the settle screen: where the post landed, plus the exact ledger lines
/// the comparisons produced and the append / cancel actions.
fn render_settle(
    content_dir: &Path,
    target: &str,
    ranked: &[String],
    insert_at: usize,
    lines: &[String],
) -> String {
    let len = ranked.len();
    let summary = if len == 0 {
        "This is your only gradable post &mdash; there was nothing to compare it against.".to_string()
    } else if insert_at == 0 {
        "New best &mdash; ranks above everything else.".to_string()
    } else if insert_at >= len {
        format!("Ranks at the very bottom, below <b>{}</b>.", esc(&ranked[len - 1]))
    } else {
        format!(
            "Sits between <b>{}</b> and <b>{}</b>.",
            esc(&ranked[insert_at - 1]),
            esc(&ranked[insert_at]),
        )
    };
    let pct = if len == 0 {
        100
    } else {
        (((1.0 - insert_at as f64 / len as f64) * 100.0).round() as i64).max(1)
    };

    let actions = if lines.is_empty() {
        r#"<p class="g-note">No comparisons were made &mdash; nothing to append.</p>
<p><a class="g-btn" href="/">Back to posts</a></p>"#
            .to_string()
    } else {
        format!(
            r#"<h2>Ledger lines ({k})</h2>
<pre class="g-lines">{lines}</pre>
<div class="g-actions">
<form method="post" action="/append"><button class="g-btn up" type="submit">Append to ledger</button></form>
<a class="g-btn" href="/">Skip / cancel</a>
</div>
<p class="g-note">Appends to <code>{path}</code></p>"#,
            k = lines.len(),
            lines = esc(&lines.join("\n")),
            path = esc(&ledger_path(content_dir).display().to_string()),
        )
    };

    let body = format!(
        r#"<div id="gtop"><h1>Placed: {target}</h1><span class="g-prog">Top {pct}%</span></div>
<div id="glist">
<p class="g-summary">{summary}</p>
{actions}
</div>"#,
        target = esc(target),
        pct = pct,
        summary = summary,
        actions = actions,
    );
    shell(&format!("Placed {}", target), &body)
}

/// Render the confirmation page after a successful append: the exact path and
/// lines written.
fn render_appended(path: &Path, lines: &[String]) -> String {
    let body = format!(
        r#"<div id="gtop"><h1>Appended</h1></div>
<div id="glist">
<p class="g-summary">Wrote {k} line{plural} to <code>{path}</code>.</p>
<pre class="g-lines">{lines}</pre>
<p><a class="g-btn" href="/">Grade another post</a></p>
</div>"#,
        k = lines.len(),
        plural = if lines.len() == 1 { "" } else { "s" },
        path = esc(&path.display().to_string()),
        lines = esc(&lines.join("\n")),
    );
    shell("Appended to ledger", &body)
}

// ─── Page shell + escaping ───────────────────────────────────────────────────

/// The grader page shell. It embeds the live-site stylesheet (`templates::CSS`)
/// so post bodies render with full parity, then a small grader-only stylesheet
/// on top — scoped to `#g*` ids and `.g-*` classes the site CSS never defines,
/// so the two never collide.
fn shell(title: &str, body: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="{lang}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>{css}{grader_css}</style>
</head>
<body>
{body}
</body>
</html>"#,
        lang = esc(&sajt::config::site().language),
        title = esc(title),
        css = templates::CSS,
        grader_css = GRADER_CSS,
        body = body,
    )
}

const GRADER_CSS: &str = r#"
body { padding-inline: 0; margin: 0; }
#gtop {
  position: sticky; top: 0; z-index: 10;
  display: flex; align-items: center; flex-wrap: wrap; gap: .9rem 1.1rem;
  padding: .7rem 1.1rem;
  background: var(--bg);
  border-bottom: 1px solid var(--hair);
}
#gtop h1 { margin: 0; font: 650 1.02rem var(--sans); letter-spacing: -.01em; }
.g-prog { font: 500 .8rem var(--mono); color: var(--faint); }
.g-btns { display: flex; gap: .5rem; }
.g-right { margin-left: auto; }
.g-btn {
  display: inline-flex; align-items: center; gap: .35rem;
  font: 600 .88rem var(--sans);
  padding: .5rem .95rem;
  border-radius: .6rem;
  border: 1px solid var(--hair);
  background: var(--pill);
  color: var(--ink);
  cursor: pointer;
  text-decoration: none;
}
.g-btn:hover { border-color: var(--violet); color: var(--violet); text-decoration: none; }
.g-btn.up:hover { color: #1a8a3c; border-color: #1a8a3c; }
.g-btn.down:hover { color: var(--tag-red); border-color: var(--tag-red); }
.g-skip { font: 550 .82rem var(--sans); color: var(--faint); align-self: center; }
#gpair { display: flex; align-items: stretch; height: calc(100vh - 3.6rem); }
.gcol { flex: 1 1 0; min-width: 0; overflow: auto; border-inline-end: 1px solid var(--hair); }
.gcol:last-child { border-inline-end: 0; }
.gcol > main { padding-inline: 1.25rem; }
.g-banner {
  position: sticky; top: 0; z-index: 3;
  padding: .5rem 1.25rem;
  font: 550 .78rem var(--sans);
  color: var(--soft);
  background: color-mix(in srgb, var(--bg) 88%, transparent);
  backdrop-filter: blur(6px);
  border-bottom: 1px solid var(--hair);
}
.g-banner b { color: var(--ink); font-weight: 650; }
.g-frame-wrap { padding: 1rem 1.25rem 3rem; }
.g-frame { width: 100%; height: 78vh; border: 1px solid var(--hair); border-radius: .5rem; background: #fff; }
#glist { max-width: 46rem; margin-inline: auto; padding: 1.5rem 1.25rem 4rem; }
#glist h2 { font: 600 .95rem var(--sans); color: var(--soft); margin: 1.6rem 0 .6rem; }
#glist ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: .1rem; }
#glist li { display: flex; align-items: baseline; gap: .6rem; padding: .35rem 0; border-bottom: 1px solid var(--hair); }
#glist li a { font: 550 1rem var(--sans); }
.g-kind { font: 500 .74rem var(--mono); color: var(--faint); }
.g-grade { margin-left: auto; font: 600 .8rem var(--mono); color: var(--violet); }
.g-empty { color: var(--faint); font-size: .9rem; }
.g-summary { font-size: 1.05rem; margin: .5rem 0 1.3rem; }
.g-lines {
  white-space: pre-wrap; word-break: break-all; user-select: all;
  font: 500 .82rem var(--mono);
  background: var(--code-bg); border: 1px solid var(--code-hair);
  border-radius: .5rem; padding: .8rem 1rem; margin: .4rem 0 1rem;
}
.g-actions { display: flex; gap: .6rem; align-items: center; margin: 1rem 0; }
.g-actions form { margin: 0; }
.g-note { font-size: .82rem; color: var(--faint); }
.g-note code, .g-summary code { font: 500 .82rem var(--mono); color: var(--soft); }
"#;

/// Escape text inserted into the grader's own chrome (labels, paths, errors).
/// Post bodies come from the trusted render pipeline and are NOT run through
/// this — they are inserted as-is, exactly as the live site does.
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Escape a full HTML document for an `iframe srcdoc` attribute value. Only `&`
/// and `"` need escaping inside a double-quoted attribute; `<`/`>` must survive
/// so the document parses.
fn srcdoc_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('"', "&quot;")
}

/// Percent-encode a post label for use as a single URL path segment.
fn href_label(label: &str) -> String {
    templates::encode_path(label)
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "Not found").into_response()
}

// ─── Startup ─────────────────────────────────────────────────────────────────

/// Print the prominent, non-dismissable startup notice: the URL, that it is
/// local-only, and — loudly — that it WRITES the ledger inside the content tree.
fn print_notice(port: u16, content_dir: &Path, ledger: &Path) {
    let bar = "=".repeat(74);
    eprintln!("\n{bar}");
    eprintln!("  {} GRADING TOOL  --  local authoring surface, NOT the public site", sajt::config::site().name.to_uppercase());
    eprintln!("{bar}");
    eprintln!("  URL      http://127.0.0.1:{port}");
    eprintln!("  Content  {}", content_dir.display());
    eprintln!("  WRITES   {}   (append-only; the ONLY file it writes)", ledger.display());
    eprintln!("  Bind     127.0.0.1 only -- never proxy this, it is not in the Caddyfile.");
    eprintln!("  Stop     Ctrl-C");
    eprintln!("{bar}\n");
}

/// Run the grading tool: scan content, build the local-only router, and serve
/// until Ctrl-C.
/// `sajt grade trash`: move every ledger file to the system Trash. Lists what
/// it would move and stops without `--yes`; nothing is ever deleted outright,
/// the Trash keeps the undo.
pub fn trash(content_dir: &Path, yes: bool) {
    let files = sajt::grade::ledger_files(content_dir);
    if files.is_empty() {
        println!("No grade ledger in {}: nothing to move.", content_dir.display());
        return;
    }
    println!("Grade ledger file(s) in {}:", content_dir.display());
    for f in &files {
        println!("  {}", f.file_name().unwrap_or_default().to_string_lossy());
    }
    if !yes {
        eprintln!("Not touched. Re-run with --yes to move them to the Trash.");
        std::process::exit(1);
    }
    match trash_context().delete_all(&files) {
        Ok(()) => println!("Moved {} file(s) to the Trash.", files.len()),
        Err(e) => {
            eprintln!("Could not move the ledger to the Trash: {}", e);
            std::process::exit(1);
        }
    }
}

/// The Trash mover. On macOS the crate's default asks Finder over AppleScript,
/// which needs a running Finder, a GUI session, and an Automation permission
/// prompt; `sajt` may well run over SSH or under a sandbox. `NSFileManager`'s
/// `trashItemAtURL` moves the file into the Trash directly with none of those
/// conditions, at the cost of Finder's "Put Back" entry on some systems (the
/// file is still in the Trash, and the command listed it by name first).
fn trash_context() -> trash::TrashContext {
    let mut ctx = trash::TrashContext::new();
    #[cfg(target_os = "macos")]
    {
        use trash::macos::TrashContextExtMacos;
        ctx.set_delete_method(trash::macos::DeleteMethod::NsFileManager);
    }
    ctx
}

pub async fn run(content_dir: PathBuf, port: u16) {
    // A disposable cache root OUTSIDE the content tree, so scanning never writes
    // into content. Fail closed if it would land inside the content tree.
    let cache_dir = std::env::temp_dir().join("sajt-grader-cache");
    if let Err(e) = std::fs::create_dir_all(&cache_dir) {
        tracing::warn!("Could not create grader cache dir {}: {}", cache_dir.display(), e);
    }
    let cache_dir = cache_dir.canonicalize().unwrap_or(cache_dir);
    if cache_dir == content_dir || cache_dir.starts_with(&content_dir) {
        panic!(
            "Refusing to start: grader cache dir {} is inside the content tree {}.",
            cache_dir.display(),
            content_dir.display()
        );
    }

    let store = match ContentStore::scan(&content_dir, &cache_dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to scan content directory {}: {}", content_dir.display(), e);
            std::process::exit(1);
        }
    };

    let ledger = ledger_path(&content_dir);
    print_notice(port, &content_dir, &ledger);
    tracing::info!(
        "Grading tool scanned {} entries; ledger target {}",
        store.entries.len(),
        ledger.display()
    );

    let state: Shared = Arc::new(RwLock::new(GraderState {
        content_dir,
        store,
        placement: None,
    }));

    let app = Router::new()
        .route("/", get(landing))
        .route("/refresh", post(refresh))
        .route("/place/{label}", get(place))
        .route("/compare", get(compare_get).post(compare_post))
        .route("/append", post(append_post))
        .route("/_raw/{label}", get(raw_bytes))
        .with_state(state);

    // Localhost only, exactly like the public server. Never proxied.
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to bind grading tool to {}: {}", addr, e);
            std::process::exit(1);
        }
    };
    tracing::info!("Grading tool listening on http://{}", addr);
    if let Err(e) = axum::serve(listener, app).await {
        eprintln!("Grading tool server error: {}", e);
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The binary-search step reducer: lo/hi update in both directions, and the
    /// winner mapping of a single comparison.
    #[test]
    fn binary_search_step_and_winner() {
        // "better" narrows to the upper half; "worse" to the lower half.
        assert_eq!(step(0, 8, true), (0, 4));
        assert_eq!(step(0, 8, false), (5, 8));
        assert_eq!(step(2, 3, true), (2, 2)); // converges: lo == hi
        assert_eq!(step(2, 3, false), (3, 3));

        // The target wins iff it was judged better than the pivot.
        assert_eq!(winner_of("target", "pivot", true), ("target", "pivot"));
        assert_eq!(winner_of("target", "pivot", false), ("pivot", "target"));

        // Placing among n=5 others always settles within ceil(log2(6)) = 3 steps.
        for &better in &[true, false] {
            let (mut lo, mut hi) = (0usize, 5usize);
            let mut steps = 0;
            while lo < hi {
                let (l, h) = step(lo, hi, better);
                lo = l;
                hi = h;
                steps += 1;
                assert!(steps <= 10, "must terminate");
            }
            assert_eq!(lo, hi);
            assert!(steps <= comparisons_estimate(5), "within the estimate: {steps}");
        }
        assert_eq!(comparisons_estimate(0), 1);
        assert_eq!(comparisons_estimate(5), 3);
    }

    /// The ledger-append guard: any target that is not exactly the content
    /// root's ledger is refused, fail-closed, before anything is written; the
    /// real ledger target is accepted and the lines land with trailing newlines.
    #[test]
    fn ledger_append_path_guard() {
        let dir = std::env::temp_dir().join(format!("sajt-grader-guard-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // A non-ledger target is refused and writes nothing.
        let bad = dir.join("evil.jsonl");
        let err = append_judgements(&dir, &bad, &["{}".to_string()]).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(!bad.exists(), "nothing must be written to a rejected target");

        // A ledger under a *different* content root is also refused (the guard
        // is against `content_dir`, not just the file name).
        let other = dir.join("sub");
        std::fs::create_dir_all(&other).unwrap();
        let foreign = ledger_path(&other);
        assert!(append_judgements(&dir, &foreign, &["{}".to_string()]).is_err());

        // The one correct target is accepted and appends real lines.
        let good = ledger_path(&dir);
        let lines = vec![
            render_line(&Recorded {
                winner: "a".into(),
                loser: "b".into(),
                at: "2026-07-06T18:05:00Z".into(),
            }),
            render_line(&Recorded {
                winner: "c".into(),
                loser: "a".into(),
                at: "2026-07-06T18:05:01Z".into(),
            }),
        ];
        append_judgements(&dir, &good, &lines).unwrap();
        // Appending again must add, not truncate.
        append_judgements(&dir, &good, &lines).unwrap();

        let body = std::fs::read_to_string(&good).unwrap();
        let read: Vec<&str> = body.lines().collect();
        assert_eq!(read.len(), 4, "two appends of two lines each");
        assert_eq!(read[0], r#"{"winner":"a","loser":"b","at":"2026-07-06T18:05:00Z"}"#);
        // Every emitted line parses back as a grade::Judgement (format contract).
        for line in &read {
            let j: sajt::grade::Judgement = serde_json::from_str(line).unwrap();
            assert!(!j.winner.is_empty() && !j.loser.is_empty() && j.at.is_some());
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
