use crate::embed::EmbedData;
use crate::entry::{parse_filename, Entry};
use crate::tags::{read_tags, read_tags_colored, remove_tag};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct ContentStore {
    pub entries: Vec<Entry>,
    pub content_dir: PathBuf,
    pub embed_cache: HashMap<PathBuf, EmbedData>,
}

impl ContentStore {
    /// Scan a directory for entries matching the filename convention.
    /// Returns entries sorted by timestamp descending (newest first).
    pub fn scan(content_dir: &Path) -> std::io::Result<Self> {
        let mut entries = Vec::new();

        let read_dir = match std::fs::read_dir(content_dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::warn!("Content directory does not exist: {}", content_dir.display());
                return Ok(ContentStore {
                    entries,
                    content_dir: content_dir.to_path_buf(),
                    embed_cache: HashMap::new(),
                });
            }
            Err(e) => return Err(e),
        };

        // Collect all file paths, skipping directories and dotfiles
        let mut paths_to_scan: Vec<PathBuf> = Vec::new();
        for dir_entry in read_dir {
            let dir_entry = dir_entry?;
            let path = dir_entry.path();
            if path.is_dir() {
                continue;
            }
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') {
                    continue;
                }
                paths_to_scan.push(path);
            }
        }

        // Process DoRename action tags: rename non-convention files to convention
        paths_to_scan = process_do_rename(paths_to_scan);

        // Build entries from all files
        for path in paths_to_scan {
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };

            if let Some((timestamp, label, extension)) = parse_filename(&name) {
                let tags = read_tags_colored(&path);
                entries.push(Entry {
                    path,
                    timestamp,
                    label,
                    display_label: None,
                    extension,
                    tags,
                    grade: None,
                });
            } else if let Some(entry) = entry_from_plain_filename(&path, &name) {
                entries.push(entry);
            } else {
                tracing::debug!("Skipping file (no extension): {}", name);
            }
        }

        // Sort by timestamp descending (newest first)
        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        tracing::info!("Scanned {} entries from {}", entries.len(), content_dir.display());

        Ok(ContentStore {
            entries,
            content_dir: content_dir.to_path_buf(),
            embed_cache: HashMap::new(),
        })
    }

    /// Re-scan the content directory, replacing all entries.
    pub fn rescan(&mut self) -> std::io::Result<()> {
        let new = Self::scan(&self.content_dir)?;
        self.entries = new.entries;
        self.embed_cache.clear();
        Ok(())
    }

    /// Resolve embeds for entries containing recognized URLs.
    pub async fn resolve_embeds(&mut self) {
        let cache = crate::embed::resolve_embeds(&mut self.entries, &self.content_dir).await;
        self.embed_cache = cache;
        let count = self.embed_cache.len();
        if count > 0 {
            tracing::info!("Resolved {} link embeds", count);
        }
    }
}

/// Process files tagged with "DoRename": rename to timestamp convention and remove the tag.
/// Returns updated list of paths (with renamed paths replacing originals).
fn process_do_rename(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut result = Vec::with_capacity(paths.len());

    for path in paths {
        let tags = read_tags(&path);
        if !tags.iter().any(|t| t == "DoRename") {
            result.push(path);
            continue;
        }

        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => {
                result.push(path);
                continue;
            }
        };

        // Already has timestamp prefix — just remove the tag
        if parse_filename(&name).is_some() {
            tracing::info!("DoRename: '{}' already follows convention, removing tag", name);
            remove_tag(&path, "DoRename");
            result.push(path);
            continue;
        }

        // Extract extension and label from plain filename
        let dot = match name.rfind('.') {
            Some(d) => d,
            None => {
                tracing::warn!("DoRename: '{}' has no extension, skipping", name);
                result.push(path);
                continue;
            }
        };
        let extension = &name[dot..]; // includes the dot
        let label_part = &name[..dot];

        // Get creation time from filesystem
        let metadata = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(e) => {
                tracing::error!("DoRename: failed to read metadata for '{}': {}", name, e);
                result.push(path);
                continue;
            }
        };
        let created = match metadata.created().or_else(|_| metadata.modified()) {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("DoRename: no timestamp available for '{}': {}", name, e);
                result.push(path);
                continue;
            }
        };
        let datetime: chrono::DateTime<chrono::Local> = created.into();
        let ts_prefix = datetime.format("%Y-%m-%dT%H%M%S").to_string();

        // Build new filename: YYYY-MM-DDTHHMMSS_label.ext
        let new_name = if label_part.is_empty() {
            format!("{}{}", ts_prefix, extension)
        } else {
            format!("{}_{}{}", ts_prefix, label_part, extension)
        };

        let new_path = path.with_file_name(&new_name);

        // Don't overwrite existing files
        if new_path.exists() {
            tracing::warn!(
                "DoRename: target '{}' already exists, skipping rename of '{}'",
                new_name, name
            );
            result.push(path);
            continue;
        }

        match std::fs::rename(&path, &new_path) {
            Ok(()) => {
                tracing::info!("DoRename: '{}' -> '{}'", name, new_name);
                remove_tag(&new_path, "DoRename");
                result.push(new_path);
            }
            Err(e) => {
                tracing::error!("DoRename: failed to rename '{}' -> '{}': {}", name, new_name, e);
                result.push(path);
            }
        }
    }

    result
}

/// Build an Entry from a plain filename (no timestamp prefix).
/// Uses the file's creation time (birthtime) as the timestamp, falling back to mtime.
fn entry_from_plain_filename(path: &Path, name: &str) -> Option<Entry> {
    let dot = name.rfind('.')?;
    let extension = &name[dot + 1..];
    if extension.is_empty() {
        return None;
    }

    let label = &name[..dot];
    let label = if label.is_empty() { None } else { Some(label.to_string()) };

    let metadata = std::fs::metadata(path).ok()?;
    let timestamp = metadata
        .created()
        .or_else(|_| metadata.modified())
        .ok()?;
    let datetime: chrono::DateTime<chrono::Utc> = timestamp.into();
    let naive = datetime.naive_utc();

    let tags = read_tags_colored(path);

    tracing::debug!(
        "Plain file '{}': using filesystem timestamp {}",
        name,
        naive.format("%Y-%m-%dT%H%M%S")
    );

    Some(Entry {
        path: path.to_path_buf(),
        timestamp: naive,
        label,
        display_label: None,
        extension: extension.to_string(),
        tags,
        grade: None,
    })
}
