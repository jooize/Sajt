//! One-shot migration: old flat `YYYY-MM-DDTHHMMSS[_label].ext` files -> folder
//! posts. See entry-model.md "Migration". DRY-RUN by default (prints the plan,
//! writes nothing); `--apply` performs it. Never overwrites; anything ambiguous
//! is left in place and reported. Pre-1.0, run once.
//!
//! Each old file becomes:
//!   `<label>/<label>.ext`               (clean name; timestamp prefix dropped)
//!   `<label>/YYYY-MM-DDTHHMMSS/`         (empty date marker = the old timestamp)
//! Unlabeled files (timestamp only) use the label `untitled`. Same-name
//! duplicates fold into one post: the newest is current, older ones become
//! `<label> copy/`, `<label> copy 2/` revision folders whose primary file mtime
//! is set to the old filename timestamp (copies are dated by mtime, not markers).

use chrono::{Local, NaiveDateTime, TimeZone};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// An old-convention file successfully parsed into its parts.
struct OldFile {
    path: PathBuf,
    /// The timestamp prefix verbatim, e.g. "2026-03-03T143052" -- reused as both
    /// the date-marker folder name and the mtime a revision is stamped with.
    stamp: String,
    stamp_dt: NaiveDateTime,
    label: String,
    ext: String,
}

/// One planned move. Every variant only ever creates new folders and moves files
/// into them; the migration never deletes or overwrites.
enum Step {
    /// The current post: move `src` to `<folder>/<file>` and create marker `<folder>/<marker>/`.
    Current {
        src: PathBuf,
        folder: String,
        file: String,
        marker: String,
    },
    /// An archived revision: move `src` to `<folder>/<file>` and set its mtime.
    Revision {
        src: PathBuf,
        folder: String,
        file: String,
        mtime: NaiveDateTime,
    },
}

/// Run the migration. `apply` false = DRY-RUN (plan only, no writes).
pub fn run(content_dir: &Path, apply: bool) -> std::io::Result<()> {
    let mut olds: Vec<OldFile> = Vec::new();
    let mut skips: Vec<(PathBuf, String)> = Vec::new();
    let mut legacy_caches: Vec<PathBuf> = Vec::new();

    for dir_entry in std::fs::read_dir(content_dir)? {
        let dir_entry = dir_entry?;
        let path = dir_entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => {
                skips.push((path, "non-UTF-8 name".to_string()));
                continue;
            }
        };
        if name.starts_with('.') {
            continue; // dotfiles are not content
        }
        let is_dir = dir_entry.file_type()?.is_dir();
        if is_dir {
            if name.ends_with(".embed-cache") {
                legacy_caches.push(path);
            } else {
                skips.push((path, "already a folder (not old flat convention)".to_string()));
            }
            continue;
        }
        match parse_old(&path, &name) {
            Ok(old) => olds.push(old),
            Err(reason) => skips.push((path, reason)),
        }
    }

    // Group by label so same-name duplicates fold into one post + revisions.
    let mut groups: BTreeMap<String, Vec<OldFile>> = BTreeMap::new();
    for old in olds {
        groups.entry(old.label.clone()).or_default().push(old);
    }

    let mut steps: Vec<Step> = Vec::new();
    for (label, mut group) in groups {
        // Newest first: it becomes the current post, the rest become revisions.
        group.sort_by(|a, b| b.stamp_dt.cmp(&a.stamp_dt));

        // Fail closed: never overwrite an existing folder. Skip the whole family.
        if content_dir.join(&label).exists() {
            for old in &group {
                skips.push((
                    old.path.clone(),
                    format!("target folder '{}/' already exists", label),
                ));
            }
            continue;
        }

        let current = &group[0];
        steps.push(Step::Current {
            src: current.path.clone(),
            folder: label.clone(),
            file: format!("{}.{}", label, current.ext),
            marker: current.stamp.clone(),
        });
        for (i, older) in group[1..].iter().enumerate() {
            let rank = i + 1;
            let folder = if rank == 1 {
                format!("{} copy", label)
            } else {
                format!("{} copy {}", label, rank)
            };
            steps.push(Step::Revision {
                src: older.path.clone(),
                // The copy folder's primary is resolved against the base name, so
                // the inner file keeps the base label, not the copy-suffixed one.
                file: format!("{}.{}", label, older.ext),
                folder,
                mtime: older.stamp_dt,
            });
        }
    }

    print_plan(content_dir, &steps, &skips, &legacy_caches, apply);

    if apply {
        apply_steps(content_dir, &steps)?;
        println!("\nApplied {} move(s). Content converted.", steps.len());
        if !legacy_caches.is_empty() {
            println!(
                "Left {} legacy .embed-cache dir(s) in place (safe to delete; regenerated in the external cache).",
                legacy_caches.len()
            );
        }
    } else {
        println!("\nDRY RUN -- nothing changed. Re-run with `migrate --apply` to perform it.");
    }
    Ok(())
}

/// Parse `YYYY-MM-DDTHHMMSS[_label].ext`. Returns a human reason on no match so
/// the file is reported (left in place), never silently skipped.
fn parse_old(path: &Path, name: &str) -> Result<OldFile, String> {
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() && !e.is_empty() => (s, e),
        _ => return Err("no file extension".to_string()),
    };
    let ts = stem.get(..17).ok_or("too short for a timestamp prefix")?;
    let stamp_dt = NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H%M%S")
        .map_err(|_| "no YYYY-MM-DDTHHMMSS prefix".to_string())?;
    let rest = &stem[17..];
    let label = if rest.is_empty() {
        "untitled".to_string()
    } else if let Some(l) = rest.strip_prefix('_') {
        if l.is_empty() {
            "untitled".to_string()
        } else {
            l.to_string()
        }
    } else {
        return Err(format!("unexpected characters after timestamp: {:?}", rest));
    };
    Ok(OldFile {
        path: path.to_path_buf(),
        stamp: ts.to_string(),
        stamp_dt,
        label,
        ext: ext.to_string(),
    })
}

/// Print the plan, grouped and relative to the content root for readability.
fn print_plan(
    content_dir: &Path,
    steps: &[Step],
    skips: &[(PathBuf, String)],
    legacy_caches: &[PathBuf],
    apply: bool,
) {
    let rel = |p: &Path| {
        p.strip_prefix(content_dir)
            .unwrap_or(p)
            .display()
            .to_string()
    };
    let mode = if apply { "APPLY" } else { "DRY RUN" };
    println!("Migration plan ({}) for {}\n", mode, content_dir.display());

    let currents = steps
        .iter()
        .filter(|s| matches!(s, Step::Current { .. }))
        .count();
    let revisions = steps.len() - currents;
    println!(
        "{} post(s) to create, {} revision(s), {} skipped, {} legacy cache dir(s).\n",
        currents, revisions, skips.len(), legacy_caches.len()
    );

    for step in steps {
        match step {
            Step::Current { src, folder, file, marker } => {
                println!("  post   {}", rel(src));
                println!("         ->  {}/{}", folder, file);
                println!("         +   {}/{}/   (date marker)", folder, marker);
            }
            Step::Revision { src, folder, file, mtime } => {
                println!("  revis  {}", rel(src));
                println!(
                    "         ->  {}/{}   (mtime set to {})",
                    folder,
                    file,
                    mtime.format("%Y-%m-%d %H:%M:%S")
                );
            }
        }
    }

    if !skips.is_empty() {
        println!("\nLeft in place ({}):", skips.len());
        for (path, reason) in skips {
            println!("  {}   [{}]", rel(path), reason);
        }
    }
    if !legacy_caches.is_empty() {
        println!("\nLegacy .embed-cache dirs ({}, ignored by the server, safe to delete):", legacy_caches.len());
        for path in legacy_caches {
            println!("  {}", rel(path));
        }
    }
}

/// Perform the planned moves. Uses `create_dir` for the post folder so an
/// existing folder is a hard error (never overwrite), and `rename` so moves stay
/// atomic within the content filesystem.
fn apply_steps(content_dir: &Path, steps: &[Step]) -> std::io::Result<()> {
    for step in steps {
        match step {
            Step::Current { src, folder, file, marker } => {
                let dir = content_dir.join(folder);
                std::fs::create_dir(&dir)?;
                std::fs::rename(src, dir.join(file))?;
                std::fs::create_dir_all(dir.join(marker))?;
            }
            Step::Revision { src, folder, file, mtime } => {
                let dir = content_dir.join(folder);
                std::fs::create_dir(&dir)?;
                let dest = dir.join(file);
                std::fs::rename(src, &dest)?;
                set_mtime(&dest, *mtime)?;
            }
        }
    }
    Ok(())
}

/// Set a file's mtime to a site-local naive datetime (the old filename stamp).
fn set_mtime(path: &Path, dt: NaiveDateTime) -> std::io::Result<()> {
    let secs = Local
        .from_local_datetime(&dt)
        .single()
        .map(|z| z.timestamp())
        .unwrap_or_else(|| dt.and_utc().timestamp());
    let when = std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs.max(0) as u64);
    let file = std::fs::File::options().write(true).open(path)?;
    file.set_modified(when)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_labeled() {
        let o = parse_old(
            Path::new("/c/2026-03-03T143052_hello-world.md"),
            "2026-03-03T143052_hello-world.md",
        )
        .unwrap();
        assert_eq!(o.stamp, "2026-03-03T143052");
        assert_eq!(o.label, "hello-world");
        assert_eq!(o.ext, "md");
    }

    #[test]
    fn parse_unlabeled_is_untitled() {
        let o = parse_old(Path::new("/c/2026-03-04T091500.txt"), "2026-03-04T091500.txt").unwrap();
        assert_eq!(o.label, "untitled");
        assert_eq!(o.ext, "txt");
    }

    #[test]
    fn reject_non_convention() {
        assert!(parse_old(Path::new("/c/hello-world.md"), "hello-world.md").is_err());
        assert!(parse_old(Path::new("/c/notes"), "notes").is_err());
        // A clean folder-post name that happens to be 17+ chars but not a date.
        assert!(parse_old(Path::new("/c/my-long-post-name.md"), "my-long-post-name.md").is_err());
    }
}
