mod content;
mod embed;
mod entry;
mod render;
mod routes;
mod stats;
mod tags;
mod templates;
mod url;

use clap::Parser;
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

    /// Port to listen on
    #[arg(long, default_value_t = 1234)]
    port: u16,

    /// Embed liveness check interval in hours (0 = disabled)
    #[arg(long, default_value_t = 24)]
    embed_check_hours: u64,
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

    tracing::info!("Content directory: {}", content_dir.display());

    let mut store = content::ContentStore::scan(&content_dir).expect("Failed to scan content directory");

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

    // Spawn periodic liveness check for embeds
    if args.embed_check_hours > 0 {
        let check_interval = Duration::from_secs(args.embed_check_hours * 3600);
        let state_clone = Arc::clone(&state);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(check_interval).await;
                tracing::info!("Running periodic embed liveness check");
                let mut store = state_clone.write().await;
                embed::check_liveness(&mut store.embed_cache, check_interval).await;
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
            "/_embed/{entry_name}/{asset_name}",
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
