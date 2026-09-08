//! The host-neutral manifest: the entire build as data. Adapters (Caddyfile,
//! `_redirects`/`_headers`, S3 sync plans) are renderings of this one
//! artifact; the verifier replays it against a live host.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The provenance record on a published file: the content-relative source
/// path and the tag on that file that authorized publishing it. Absent on
/// generated pages (template output over already-public metadata). The push
/// step refuses to upload a content-bearing file without this record.
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct ProvenanceEntry {
    pub source: String,
    pub tag: String,
}

/// One published file in the manifest: where its bytes live (the blob hash),
/// what the reply looked like, every header the host should reproduce, and
/// the provenance that authorized it.
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct FileEntry {
    pub hash: String,
    pub size: u64,
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<ProvenanceEntry>,
}

#[derive(Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub generated: String,
    pub files: BTreeMap<String, FileEntry>,
    pub redirects: BTreeMap<String, String>,
    /// URLs that were published once and are gone now — served 410, never a
    /// silent 404 (unpublishing is an answer, not an absence).
    pub gone: BTreeSet<String>,
    /// The out-of-closure fallback page (the host's custom 404).
    pub fallback: FileEntry,
    /// Hardening headers for every response, from the one policy source
    /// (`sajt_core::security`).
    pub universal_headers: Vec<(String, String)>,
}

/// Load and parse a manifest, failing with a message that names the path —
/// adapters and the verifier both start here.
pub fn load(path: &Path) -> Result<Manifest, String> {
    let bytes = std::fs::read(path)
        .map_err(|e| format!("cannot read manifest {}: {}", path.display(), e))?;
    let manifest: Manifest = serde_json::from_slice(&bytes)
        .map_err(|e| format!("cannot parse manifest {}: {}", path.display(), e))?;
    if manifest.version != 1 {
        return Err(format!(
            "manifest {} has version {}, this binary understands version 1",
            path.display(),
            manifest.version
        ));
    }
    Ok(manifest)
}
