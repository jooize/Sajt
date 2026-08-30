mod grader;
mod migrate;
mod routes;
mod security;

use clap::{Parser, Subcommand};
use staticdrop_core::{content, embed, media};
use notify::Watcher;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

#[derive(Parser)]
#[command(name = "staticdrop", about = "StaticDrop content server")]
struct Args {
    /// Directory containing content files
    #[arg(long, default_value = "./content")]
    content_dir: PathBuf,

    /// Root for disposable caches (embeds, etc.), kept OUTSIDE the content tree
    /// so the server never writes into content. Defaults to the platform cache
    /// dir (e.g. macOS ~/Library/Caches/bar.esko.staticdrop).
    #[arg(long)]
    cache_dir: Option<PathBuf>,

    /// Port to listen on
    #[arg(long, default_value_t = 1234)]
    port: u16,

    /// Address to bind. The loopback default keeps the server reachable only
    /// through the local reverse proxy; set 0.0.0.0 only where the network
    /// boundary is elsewhere (e.g. inside a container whose host does TLS).
    /// The grading tool is NOT affected — it always binds loopback.
    #[arg(long, default_value = "127.0.0.1")]
    listen: std::net::IpAddr,

    /// Embed liveness check interval in hours (0 = disabled)
    #[arg(long, default_value_t = 24)]
    embed_check_hours: u64,

    /// JPEG quality (1-100) for full-view transcoded renditions (the `/jpeg` rung).
    /// Author-side only, never a URL parameter. The value is part of the
    /// clean-store cache key, so changing it regenerates renditions on demand;
    /// gallery tiles keep their own fixed quality.
    #[arg(long, default_value_t = 85, value_parser = clap::value_parser!(u8).range(1..=100))]
    jpeg_quality: u8,

    /// Allow image transcodes to run WITHOUT an OS sandbox when none is
    /// available (sandbox-exec on macOS, bwrap on Linux). The default is
    /// fail-closed: with no sandbox tooling, formats that need a transcode
    /// (HEIC/TIFF/GIF/...) are withheld rather than decoded unconfined.
    #[arg(long)]
    unsandboxed_transcode: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// One-shot: convert old flat `YYYY-MM-DDTHHMMSS[_label].ext` files into
    /// folder posts (see entry-model.md). DRY-RUN unless `--apply` is given;
    /// never overwrites. Run once, pre-1.0.
    Migrate {
        /// Perform the migration. Without this, only the plan is printed.
        #[arg(long)]
        apply: bool,
    },
    /// Local-only pairwise grading tool. Starts its own web UI to compare two
    /// posts side-by-side (each rendered as on the live site), binary-searches
    /// the new post into the ranking, and appends the resulting judgements to
    /// `<content_dir>/.esko.bar-grade-judgements.jsonl`. Localhost bind only;
    /// never proxy it. Runs until Ctrl-C. See `entry-model.md` (Grade section).
    Grade {
        /// Port to listen on (127.0.0.1 only).
        #[arg(long, default_value_t = 1236)]
        port: u16,
    },
    /// Resolve one URL against the content tree and print the reply — the
    /// whole site as a function, no server. The body goes to stdout; the
    /// status line and headers go to stderr. Exit code 0 below 400, 1 from
    /// 400 up. Accepts a path with an optional query
    /// (`/2026/+design/notable`, `/photo.tif/jpeg`, `/?search=rust`).
    Get {
        /// URL path (and optional `?search=`/`?embed`/`?fullscreen` query).
        url: String,
    },
}

/// Split a `get` URL into the decoded path and its request flags. The query
/// understands exactly what the server's handlers do: `search=` (with `+` as
/// space), `embed`, and `fullscreen`.
fn split_request(url: &str) -> (String, staticdrop_core::page::RequestFlags) {
    let (path, query) = match url.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (url, None),
    };
    let mut flags = staticdrop_core::page::RequestFlags::default();
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

/// Platform cache directory used when `--cache-dir` isn't given. Falls back to a
/// project-local `./.cache` only if the OS can't provide one.
fn default_cache_dir() -> PathBuf {
    directories::ProjectDirs::from("bar", "esko", "staticdrop")
        .map(|dirs| dirs.cache_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("./.cache"))
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // `get` prints the reply body on stdout, so logs must stay off it.
    if matches!(args.command, Some(Command::Get { .. })) {
        tracing_subscriber::fmt().with_writer(std::io::stderr).init();
    } else {
        tracing_subscriber::fmt::init();
    }

    // Canonicalize content dir (or use as-is if it doesn't exist yet)
    let content_dir = args
        .content_dir
        .canonicalize()
        .unwrap_or_else(|_| args.content_dir.clone());

    // One-shot subcommands run and exit before any server setup.
    if let Some(Command::Migrate { apply }) = args.command {
        if let Err(e) = migrate::run(&content_dir, apply) {
            eprintln!("Migration failed: {}", e);
            std::process::exit(1);
        }
        return;
    }

    // The grading tool runs its own local-only web server (separate from the
    // public one) until Ctrl-C, then exits. It is the ONLY sanctioned writer of
    // the content tree, and writes exactly one file: the grade ledger.
    if let Some(Command::Grade { port }) = args.command {
        grader::run(content_dir, port).await;
        return;
    }

    tracing::info!("Content directory: {}", content_dir.display());

    // Resolve the cache root (outside the content tree) and make sure it exists.
    // Creating it up front surfaces permission problems early; a failure isn't
    // fatal (the site still serves, embeds just re-fetch each run).
    let cache_dir = args.cache_dir.clone().unwrap_or_else(default_cache_dir);
    if let Err(e) = std::fs::create_dir_all(&cache_dir) {
        tracing::warn!("Could not create cache directory {}: {}", cache_dir.display(), e);
    }
    let cache_dir = cache_dir.canonicalize().unwrap_or(cache_dir);
    tracing::info!("Cache directory: {}", cache_dir.display());

    // Fail closed: the cache must never sit inside the content tree, or the
    // server's cache writes (and its stale-cache cleanup, which deletes whole
    // cache subdirectories) would mutate content. The content tree stays
    // strictly read-only.
    if cache_dir == content_dir || cache_dir.starts_with(&content_dir) {
        panic!(
            "Refusing to start: cache directory {} is inside the content directory {}. \
             Choose a --cache-dir outside the content tree.",
            cache_dir.display(),
            content_dir.display()
        );
    }

    // Clear transcode scratch stranded by a prior run that died mid-job, then
    // decide — loudly — how the vips subprocess is confined for this run.
    media::sweep_cache(&cache_dir);
    media::init_transcode(args.unsandboxed_transcode);
    media::init_jpeg_quality(args.jpeg_quality);

    let mut store = content::ContentStore::scan(&content_dir, &cache_dir)
        .expect("Failed to scan content directory");

    // Resolve link embeds (fetches uncached, reads cached)
    store.resolve_embeds().await;

    // One-shot URL resolution: the page layer as a CLI. Runs after the same
    // scan + embed resolve the server does, so the bytes match a live request.
    if let Some(Command::Get { ref url }) = args.command {
        let (path, flags) = split_request(url);
        let reply = staticdrop_core::page::respond(&store, &path, &flags).await;
        eprintln!("HTTP {}", reply.status);
        for (name, value) in &reply.headers {
            eprintln!("{}: {}", name, value);
        }
        use std::io::Write;
        std::io::stdout().write_all(&reply.body).expect("write body to stdout");
        std::process::exit(if reply.status < 400 { 0 } else { 1 });
    }

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
