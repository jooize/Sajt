mod content;
mod embed;
mod entry;
mod grade;
mod grader;
mod migrate;
mod outbound;
mod postdate;
mod render;
mod routes;
mod sanitize;
mod slug;
mod stats;
mod tags;
mod templates;
mod url;

use clap::{Parser, Subcommand};
use notify::Watcher;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

#[derive(Parser)]
#[command(name = "esko-bar", about = "Personal content server")]
struct Args {
    /// Directory containing content files
    #[arg(long, default_value = "./content")]
    content_dir: PathBuf,

    /// Root for disposable caches (embeds, etc.), kept OUTSIDE the content tree
    /// so the server never writes into content. Defaults to the platform cache
    /// dir (e.g. macOS ~/Library/Caches/bar.esko.esko-bar).
    #[arg(long)]
    cache_dir: Option<PathBuf>,

    /// Port to listen on
    #[arg(long, default_value_t = 1234)]
    port: u16,

    /// Embed liveness check interval in hours (0 = disabled)
    #[arg(long, default_value_t = 24)]
    embed_check_hours: u64,

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
}

/// Platform cache directory used when `--cache-dir` isn't given. Falls back to a
/// project-local `./.cache` only if the OS can't provide one.
fn default_cache_dir() -> PathBuf {
    directories::ProjectDirs::from("bar", "esko", "esko-bar")
        .map(|dirs| dirs.cache_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("./.cache"))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

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

    let mut store = content::ContentStore::scan(&content_dir, &cache_dir)
        .expect("Failed to scan content directory");

    // Resolve link embeds (fetches uncached, reads cached)
    store.resolve_embeds().await;

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
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], args.port));
    tracing::info!("Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind");

    axum::serve(listener, app).await.expect("Server error");
}
