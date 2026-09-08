//! The axum skin: router, handlers, watcher wiring, and the local-only
//! grading tool. All content logic lives in the library; this module only
//! speaks HTTP.

mod grader;
mod routes;
mod security;

use clap::Parser;
use notify::Watcher;
use sajt::embed;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use crate::{load_store, open_site, SiteArgs};

#[derive(Parser)]
pub struct ServeArgs {
    #[command(flatten)]
    pub site: SiteArgs,

    /// Port to listen on
    #[arg(long, default_value_t = 1234)]
    port: u16,

    /// Address to bind. The loopback default keeps the server reachable only
    /// through the local reverse proxy; set 0.0.0.0 only where the network
    /// boundary is elsewhere (e.g. inside a container whose host does TLS).
    /// The grading tool is NOT affected: it always binds loopback.
    #[arg(long, default_value = "127.0.0.1")]
    listen: std::net::IpAddr,

    /// Embed liveness check interval in hours (0 = disabled)
    #[arg(long, default_value_t = 24)]
    embed_check_hours: u64,
}

#[derive(Parser)]
pub struct GradeArgs {
    #[command(flatten)]
    pub site: SiteArgs,

    /// Port to listen on (127.0.0.1 only).
    #[arg(long, default_value_t = 1236)]
    port: u16,

    #[command(subcommand)]
    action: Option<GradeAction>,
}

#[derive(clap::Subcommand)]
enum GradeAction {
    /// Move the grade ledger (and any iCloud conflict copies) to the Trash
    ///
    /// The ledger is the only thing the engine ever writes into a site. Lists
    /// what it would remove and stops unless --yes is given. Files go to the
    /// system Trash, never straight to deletion.
    Purge {
        /// Actually do it.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Parser)]
pub struct GetArgs {
    #[command(flatten)]
    pub site: SiteArgs,

    /// URL path (and optional `?search=`/`?embed`/`?fullscreen` query).
    url: String,
}

/// The grading tool runs its own local-only web server (separate from the
/// public one) until Ctrl-C, then exits. It is the ONLY sanctioned writer of
/// the content tree, and writes exactly one file: the grade ledger.
pub async fn grade(args: GradeArgs) {
    let site = open_site(&args.site);
    match args.action {
        Some(GradeAction::Purge { yes }) => grader::purge(&site.content_dir, yes),
        None => grader::run(site.content_dir, args.port).await,
    }
}

/// One-shot URL resolution: the page layer as a CLI. Runs the same scan and
/// embed resolve the server does, so the bytes match a live request.
pub async fn get(args: GetArgs) {
    let site = open_site(&args.site);
    let store = load_store(&site).await;
    let (path, flags) = split_request(&args.url);
    let reply = sajt::page::respond(&store, &path, &flags).await;
    eprintln!("HTTP {}", reply.status);
    for (name, value) in &reply.headers {
        eprintln!("{}: {}", name, value);
    }
    use std::io::Write;
    std::io::stdout().write_all(&reply.body).expect("write body to stdout");
    std::process::exit(if reply.status < 400 { 0 } else { 1 });
}

/// Split a `get` URL into the decoded path and its request flags. The query
/// understands exactly what the server's handlers do: `search=` (with `+` as
/// space), `embed`, and `fullscreen`.
fn split_request(url: &str) -> (String, sajt::page::RequestFlags) {
    let (path, query) = match url.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (url, None),
    };
    let mut flags = sajt::page::RequestFlags::default();
    if let Some(q) = query {
        for pair in q.split('&') {
            let (k, v) = match pair.split_once('=') {
                Some((k, v)) => (k, Some(v)),
                None => (pair, None),
            };
            match k {
                "embed" => flags.embed = true,
                "fullscreen" => flags.fullscreen = true,
                "search" => flags.search = v.map(|v| percent_decode(&v.replace('+', " "))),
                _ => {}
            }
        }
    }
    (percent_decode(path), flags)
}

/// Minimal percent-decoding for the `get` subcommand (the server side gets
/// this from axum). Invalid escapes pass through literally; invalid UTF-8 is
/// replaced rather than trusted.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(b) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}


pub async fn run(args: ServeArgs) {
    let site = open_site(&args.site);
    let content_dir = site.content_dir.clone();
    let store = load_store(&site).await;

    let state = Arc::new(RwLock::new(store));

    // Watch content directory for changes (macOS FSEvents)
    let state_for_watcher = Arc::clone(&state);
    let (watcher_tx, mut watcher_rx) = tokio::sync::mpsc::channel::<()>(1);
    let content_dir_watch = content_dir.clone();

    // The watcher must live for the duration of the program
    let _watcher = {
        let tx = watcher_tx.clone();
        let mut watcher = notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
            match res {
                Ok(event) => {
                    use notify::EventKind;
                    match event.kind {
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                            // Debounce: try_send drops if channel is full (already pending rescan)
                            let _ = tx.try_send(());
                        }
                        _ => {}
                    }
                }
                Err(e) => tracing::warn!("Filesystem watch error: {}", e),
            }
        })
        .expect("Failed to create filesystem watcher");

        // Recursive: folder posts hold their content and markers in subfolders,
        // so edits inside a post must trigger a rescan too.
        watcher
            .watch(&content_dir_watch, notify::RecursiveMode::Recursive)
            .expect("Failed to watch content directory");

        tracing::info!("Watching content directory for changes (FSEvents, recursive)");
        watcher
    };

    // Spawn task to handle filesystem change notifications
    tokio::spawn(async move {
        // Debounce: wait a short period after the first event to batch rapid changes
        while watcher_rx.recv().await.is_some() {
            // Drain any queued events and wait for things to settle
            tokio::time::sleep(Duration::from_millis(500)).await;
            while watcher_rx.try_recv().is_ok() {}

            tracing::info!("Content directory changed, rescanning");
            let mut store = state_for_watcher.write().await;
            match store.rescan() {
                Ok(()) => {
                    store.resolve_embeds().await;
                    tracing::info!("Rescanned after change: {} entries", store.entries.len());
                }
                Err(e) => tracing::error!("Rescan after change failed: {}", e),
            }
        }
    });

    // Future-hold waker: wake at the next scheduled (future-dated) post's moment
    // and rescan, so a scheduled post appears exactly when due rather than on the
    // next unrelated change. Idle-polls hourly when nothing is scheduled. Reuses
    // the same write+rescan path as the FS watcher.
    {
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            loop {
                let delay = { state.read().await.next_future_delay() };
                match delay {
                    Some(d) => {
                        tokio::time::sleep(d).await;
                        let mut store = state.write().await;
                        match store.rescan() {
                            Ok(()) => {
                                store.resolve_embeds().await;
                                tracing::info!("Rescanned for a scheduled (future-dated) post");
                            }
                            Err(e) => tracing::error!("Scheduled rescan failed: {}", e),
                        }
                    }
                    None => tokio::time::sleep(Duration::from_secs(3600)).await,
                }
            }
        });
    }

    // Spawn periodic liveness check for embeds
    if args.embed_check_hours > 0 {
        let check_interval = Duration::from_secs(args.embed_check_hours * 3600);
        let state_clone = Arc::clone(&state);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(check_interval).await;
                tracing::info!("Running periodic embed liveness check");
                let mut store = state_clone.write().await;
                let store = &mut *store; // disjoint field borrows below
                embed::check_liveness(
                    &mut store.embed_cache,
                    &store.content_dir,
                    &store.cache_dir,
                    check_interval,
                )
                .await;
            }
        });
        tracing::info!(
            "Embed liveness checks scheduled every {} hours",
            args.embed_check_hours
        );
    }

    let app = axum::Router::new()
        .route("/", axum::routing::get(routes::index))
        .route("/saved", axum::routing::get(routes::saved))
        .route("/_rescan", axum::routing::post(routes::rescan))
        .route(
            "/_embed/{key}/{asset_name}",
            axum::routing::get(routes::serve_embed_asset),
        )
        .route("/static/{*path}", axum::routing::get(routes::serve_static))
        .route("/{*path}", axum::routing::get(routes::catch_all))
        .layer(axum::middleware::from_fn(security::headers))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        // Outermost: a panic in any handler on untrusted input becomes a clean
        // 500 instead of a reset connection or a downed worker — defense in depth
        // behind the fail-closed handlers (a panic leaks nothing, but this keeps
        // the server serving).
        .layer(tower_http::catch_panic::CatchPanicLayer::new())
        .with_state(state);

    let addr = SocketAddr::from((args.listen, args.port));
    tracing::info!("Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind");

    axum::serve(listener, app).await.expect("Server error");
}
