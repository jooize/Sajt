use std::path::Path;

/// Read macOS xattr tags from a file.
///
/// Tags are stored in `com.apple.metadata:_kMDItemUserTags` as a binary plist
/// containing an array of strings like `"amusing\n6"` where `\n6` is a color suffix.
/// We strip the color suffix and return just the tag names.
pub fn read_tags(path: &Path) -> Vec<String> {
    let attr = match xattr::get(path, "com.apple.metadata:_kMDItemUserTags") {
        Ok(Some(data)) => data,
        _ => return Vec::new(),
    };

    let value: plist::Value = match plist::from_bytes(&attr) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let Some(array) = value.as_array() else {
        return Vec::new();
    };

    array
        .iter()
        .filter_map(|v| v.as_string())
        .map(|s| {
            // Strip color suffix: "tagname\n6" → "tagname"
            match s.find('\n') {
                Some(pos) => s[..pos].to_string(),
                None => s.to_string(),
            }
        })
        .collect()
}

/// Write macOS xattr tags to a file, replacing all existing tags.
pub fn write_tags(path: &Path, tags: &[String]) -> Result<(), String> {
    if tags.is_empty() {
        // Remove the attribute entirely
        match xattr::remove(path, "com.apple.metadata:_kMDItemUserTags") {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(format!("Failed to remove tags xattr: {}", e)),
        }
    }

    // Build plist array of strings (re-add color suffix \n0 for compatibility)
    let array: Vec<plist::Value> = tags
        .iter()
        .map(|t| plist::Value::String(format!("{}\n0", t)))
        .collect();
    let value = plist::Value::Array(array);

    let mut buf = Vec::new();
    plist::to_writer_binary(&mut buf, &value)
        .map_err(|e| format!("Failed to serialize tags plist: {}", e))?;

    xattr::set(path, "com.apple.metadata:_kMDItemUserTags", &buf)
        .map_err(|e| format!("Failed to write tags xattr: {}", e))?;

    Ok(())
}

/// Remove a specific tag from a file's macOS xattr tags.
/// Returns the remaining tags.
pub fn remove_tag(path: &Path, tag_to_remove: &str) -> Vec<String> {
    let tags = read_tags(path);
    let remaining: Vec<String> = tags
        .into_iter()
        .filter(|t| t != tag_to_remove)
        .collect();

    if let Err(e) = write_tags(path, &remaining) {
        tracing::warn!("Failed to update tags on {}: {}", path.display(), e);
    }

    remaining
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn read_tags_nonexistent_file() {
        let tags = read_tags(&PathBuf::from("/nonexistent/file.txt"));
        assert!(tags.is_empty());
    }
}
