//! Fixture helpers shared by the unit tests: a throwaway content directory
//! and Finder-shaped xattrs (tags, comments) written the way Finder writes
//! them, so the readers and the visibility gate see real input. Compiled only
//! under `cfg(test)`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// A throwaway directory under the OS temp dir, removed on drop. Avoids a
/// dev-dependency; uniqueness is pid + a process-wide counter.
pub struct TmpDir(PathBuf);
impl TmpDir {
    pub fn new() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut p = std::env::temp_dir();
        p.push(format!("sajt-scan-{}-{}", std::process::id(), n));
        std::fs::create_dir_all(&p).unwrap();
        TmpDir(p)
    }
    pub fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TmpDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn touch(dir: &Path, rel: &str, body: &str) {
    let p = dir.join(rel);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(p, body).unwrap();
}
pub fn mkdir(dir: &Path, rel: &str) {
    std::fs::create_dir_all(dir.join(rel)).unwrap();
}

/// Set an xattr under the first platform-accepted name from `names` (macOS
/// takes the native Apple name; Linux rejects that namespace and takes the
/// `user.`-mapped one — the same list the readers try). Returns false when
/// every name is rejected, so a test can skip on an xattr-less filesystem.
#[must_use]
pub fn set_mapped_xattr(path: &Path, names: &[&str], buf: &[u8]) -> bool {
    names.iter().any(|name| xattr::set(path, name, buf).is_ok())
}

/// Write Finder tags the way Finder stores them — a binary-plist array of
/// `"name\nN"` strings in the `_kMDItemUserTags` xattr — so `read_tags_colored`
/// (and the visibility gate) see them. Returns false when the filesystem
/// rejects xattrs, so a test can skip rather than fail on an unsupported FS.
#[must_use]
pub fn set_tags(path: &Path, tags: &[&str]) -> bool {
    let arr: Vec<plist::Value> =
        tags.iter().map(|t| plist::Value::String((*t).to_string())).collect();
    let mut buf = Vec::new();
    plist::to_writer_binary(&mut buf, &plist::Value::Array(arr)).unwrap();
    set_mapped_xattr(path, crate::tags::USER_TAGS_XATTR_NAMES, &buf)
}

/// Write a Finder comment the way Finder stores it — a binary-plist string in
/// the `kMDItemFinderComment` xattr — so `read_finder_comment` sees it. Returns
/// false when the filesystem rejects xattrs, so the test skips rather than
/// failing on an unsupported FS.
#[must_use]
pub fn set_finder_comment(path: &Path, comment: &str) -> bool {
    let mut buf = Vec::new();
    plist::to_writer_binary(&mut buf, &plist::Value::String(comment.to_string())).unwrap();
    set_mapped_xattr(path, crate::tags::FINDER_COMMENT_XATTR_NAMES, &buf)
}

