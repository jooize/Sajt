//! `sajt`: the one command. Every lane of the engine is a subcommand over the
//! same library, so a site previews (`serve`), ships (`build`), and verifies
//! (`verify`) through identical code. Shared options (where the site is, where
//! disposable caches go, the rendering knobs) live in [`SiteArgs`] and are
//! opened the same way for every subcommand by [`open_site`].

mod build;
mod serve;

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "sajt",
    // Built by `build.rs`: the crate version, plus the commit when the
    // builder passed one, so a released binary names what it was built from.
    version = env!("SAJT_VERSION"),
    about = "Publishing engine where the filesystem is the CMS"
)]
enum Cli {
    /// Serve the site: preview on a workstation, or the origin behind a proxy
    ///
    /// Watches the content directory and rescans on change.
    Serve(serve::ServeArgs),
    /// Walk the site closure; emit blobs/, manifest.json, and the report
    Build(build::BuildArgs),
    /// Render manifest.json as a complete generated Caddyfile
    Caddyfile(build::CaddyfileArgs),
    /// Verify a live host against the manifest
    ///
    /// Fetches every manifest address and compares status, body hash, and
    /// headers against the manifest.
    Verify(build::VerifyArgs),
    /// Local-only pairwise grading tool (an authoring surface, never proxied)
    ///
    /// Starts its own web UI to compare two posts side-by-side (each rendered as on the live site), binary-searches
    /// the new post into the ranking, and appends the resulting judgements to
    /// `<content_dir>/Sajt-Grade-Judgements.jsonl`. Localhost bind only;
    /// never proxy it. Runs until Ctrl-C. See `entry-model.md` (Grade section).
    Grade(serve::GradeArgs),
    /// Resolve one URL against the content tree and print the reply
    ///
    /// The whole site as a function, no server. The body goes to stdout; the
    /// status line and headers go to stderr. Exit code 0 below 400, 1 from
    /// 400 up. Accepts a path with an optional query
    /// (`/2026/+design/notable`, `/photo.tif/jpeg`, `/?search=rust`).
    Get(serve::GetArgs),
}

/// Options every subcommand that touches a site shares.
#[derive(clap::Args)]
pub struct SiteArgs {
    /// The site directory: the content files, and `Sajt.toml` if the site
    /// wants to say more about itself than its directory name does.
    #[arg(long, default_value = ".")]
    pub site: PathBuf,

    /// Read the site configuration from this file instead of `Sajt.toml`
    /// inside the site directory. A missing file is fine either way: the site
    /// is then described by its directory. A file that exists but does not
    /// parse is an error at startup.
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Root for disposable caches (embeds, the clean media store), kept
    /// OUTSIDE the content tree so the engine never writes into content.
    /// Shared by every subcommand so renditions are built once. Defaults to
    /// the platform cache dir (macOS ~/Library/Caches/bar.esko.Sajt, Linux
    /// ~/.cache/sajt).
    #[arg(long)]
    pub cache_dir: Option<PathBuf>,

    /// JPEG quality (1-100) for full-view transcoded renditions (the `/jpeg`
    /// rung). Author-side only, never a URL parameter. The value is part of
    /// the clean-store cache key, so changing it regenerates renditions on
    /// demand; gallery tiles keep their own fixed quality. The same value
    /// must be used for `serve` and `build`, or their bytes differ.
    #[arg(long, default_value_t = 85, value_parser = clap::value_parser!(u8).range(1..=100))]
    pub jpeg_quality: u8,

    /// Allow image transcodes to run WITHOUT an OS sandbox when none is
    /// available (sandbox-exec on macOS, bwrap on Linux). The default is
    /// fail-closed: with no sandbox tooling, formats that need a transcode
    /// (HEIC/TIFF/GIF/...) are withheld rather than decoded unconfined.
    #[arg(long)]
    pub unsandboxed_transcode: bool,
}

/// A site opened for work: its configuration applied, its directories
/// resolved and checked.
pub struct OpenedSite {
    pub site: sajt::config::Site,
    pub content_dir: PathBuf,
    pub cache_dir: PathBuf,
}

/// Platform cache directory used when `--cache-dir` isn't given. Falls back to
/// a project-local `./.cache` only if the OS can't provide one.
fn default_cache_dir() -> PathBuf {
    directories::ProjectDirs::from("bar", "esko", "Sajt")
        .map(|dirs| dirs.cache_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("./.cache"))
}

/// Apply the site config, resolve the content and cache directories, refuse
/// a cache inside the content tree, and set the process-wide rendering knobs.
/// Every subcommand goes through here, so they cannot disagree about where
/// the site is or how it renders. Exits the process on a configuration error:
/// a typo silently reverting a site to defaults would be a trap.
pub fn open_site(args: &SiteArgs) -> OpenedSite {
    // Canonicalize the site dir (or use it as-is if it doesn't exist yet) for
    // the site name: an unnamed site is named after its directory, so the real
    // name matters. The store canonicalizes again at scan time for its own
    // fail-closed guards, which is where that invariant belongs.
    let content_dir = args
        .site
        .canonicalize()
        .unwrap_or_else(|_| args.site.clone());

    let config_path = args
        .config
        .clone()
        .unwrap_or_else(|| sajt::config::path_in(&content_dir));
    let config = sajt::config::load(&config_path).unwrap_or_else(|e| {
        eprintln!("{}", e);
        std::process::exit(1);
    });
    let site = sajt::config::resolve(config, &content_dir);
    if let Some(domain) = &site.domain {
        sajt::embed::init_contact(domain);
    }
    tracing::info!(
        "Site: {} ({})",
        site.name,
        site.domain.as_deref().unwrap_or("no domain configured")
    );
    sajt::config::init(site.clone());

    // Resolve the cache root and make sure it exists. Creating it up front
    // surfaces permission problems early; a failure isn't fatal (the site
    // still works, embeds just re-fetch each run).
    let cache_dir = args.cache_dir.clone().unwrap_or_else(default_cache_dir);
    if let Err(e) = std::fs::create_dir_all(&cache_dir) {
        tracing::warn!("Could not create cache directory {}: {}", cache_dir.display(), e);
    }
    let cache_dir = cache_dir.canonicalize().unwrap_or(cache_dir);

    // Fail closed: the cache must never sit inside the content tree, or cache
    // writes (and the stale-cache cleanup, which deletes whole cache
    // subdirectories) would mutate content. The content tree stays strictly
    // read-only.
    if cache_dir == content_dir || cache_dir.starts_with(&content_dir) {
        eprintln!(
            "Refusing to start: cache directory {} is inside the content directory {}. \
             Choose a --cache-dir outside the content tree.",
            cache_dir.display(),
            content_dir.display()
        );
        std::process::exit(1);
    }

    tracing::info!("Content directory: {}", content_dir.display());
    tracing::info!("Cache directory: {}", cache_dir.display());

    // Clear transcode scratch stranded by a prior run that died mid-job, then
    // decide, loudly, how the vips subprocess is confined for this run.
    sajt::media::sweep_cache(&cache_dir);
    sajt::media::init_transcode(args.unsandboxed_transcode);
    sajt::media::init_jpeg_quality(args.jpeg_quality);

    OpenedSite { site, content_dir, cache_dir }
}

/// Scan the content tree and resolve link embeds (fetching uncached, reading
/// cached): the same two steps every lane starts from, so `get`, `serve`,
/// and `build` see identical bytes.
pub async fn load_store(site: &OpenedSite) -> sajt::content::ContentStore {
    // A root that cannot be scanned (missing, unreadable, unresolvable) is a
    // configuration error like a bad config file: say which directory and
    // why, then exit. A panic here would bury the one line that matters.
    let mut store = sajt::content::ContentStore::scan(&site.content_dir, &site.cache_dir)
        .unwrap_or_else(|e| {
            eprintln!(
                "Cannot scan the content directory {}: {}",
                site.content_dir.display(),
                e
            );
            std::process::exit(1);
        });
    store.resolve_embeds().await;
    store
}

#[tokio::main]
async fn main() {
    // Logs go to stderr for every subcommand: `get` and `build` put their
    // product on stdout, and a server has nothing to say there.
    tracing_subscriber::fmt().with_writer(std::io::stderr).init();
    match Cli::parse() {
        Cli::Serve(args) => serve::run(args).await,
        Cli::Build(args) => build::build(args).await,
        Cli::Caddyfile(args) => build::render_caddyfile(args),
        Cli::Verify(args) => build::run_verify(args).await,
        Cli::Grade(args) => serve::grade(args).await,
        Cli::Get(args) => serve::get(args).await,
    }
}
