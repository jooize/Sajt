//! Replay the manifest against a live host and compare what comes back:
//! status, body hash, and every recorded header. The manifest is the single
//! reference — `crates/serve` should pass because it shares the page layer,
//! and an adapter's host should pass because the adapter rendered the same
//! manifest. The same loop later verifies a CDN after push (sajt.md).

use crate::manifest::{FileEntry, Manifest};
use sha2::{Digest, Sha256};
use sajt_core::templates::encode_path;

/// A deterministic address outside any real closure, probing the fallback
/// (custom 404) behavior.
const FALLBACK_PROBE: &str = "/sajt-verify-404-probe";

pub struct Outcome {
    pub checked: usize,
    pub mismatches: Vec<String>,
}

fn request_url(base: &str, url: &str) -> String {
    format!("{}{}", base.trim_end_matches('/'), encode_path(url))
}

/// Compare one response against a manifest file entry. `etag` is exempt:
/// hosts derive their own (equally strong, since blobs are content-addressed)
/// and the Caddyfile adapter deliberately leaves it to them.
fn check_file(
    url: &str,
    entry: &FileEntry,
    universal: &[(String, String)],
    status: u16,
    headers: &reqwest::header::HeaderMap,
    body: Option<&[u8]>,
    mismatches: &mut Vec<String>,
) {
    if status != entry.status {
        mismatches.push(format!("{}: status {} (manifest says {})", url, status, entry.status));
    }
    if let Some(body) = body {
        let digest = Sha256::digest(body);
        let hex: String = digest.iter().map(|b| format!("{:02x}", b)).collect();
        let hash = format!("sha256-{}", hex);
        if hash != entry.hash {
            mismatches.push(format!(
                "{}: body is {} bytes, {} (manifest says {} bytes, {})",
                url,
                body.len(),
                hash,
                entry.size,
                entry.hash
            ));
        }
    }
    let expected = entry
        .headers
        .iter()
        .filter(|(n, _)| n.as_str() != "etag")
        .map(|(n, v)| (n.as_str(), v.as_str()))
        .chain(universal.iter().map(|(n, v)| (n.as_str(), v.as_str())));
    for (name, want) in expected {
        match headers.get(name).and_then(|v| v.to_str().ok()) {
            Some(got) if got == want => {}
            Some(got) => mismatches.push(format!(
                "{}: header {} is {:?} (manifest says {:?})",
                url, name, got, want
            )),
            None => mismatches.push(format!("{}: header {} missing (manifest says {:?})", url, name, want)),
        }
    }
}

/// Fetch every address in the manifest from `base` and compare. Prints
/// nothing on matches; every mismatch is listed. Network failures count as
/// mismatches — an unreachable host verifies nothing.
///
/// `dynamic_fallback` relaxes exactly one check: the fallback probe's body
/// hash. A host that runs code (the preview server) personalizes its 404 —
/// the capability ladder's upper rungs — so only a static host is held to
/// the manifest's byte-exact fallback page. Status and headers are checked
/// either way.
pub async fn run(manifest: &Manifest, base: &str, dynamic_fallback: bool) -> Result<Outcome, String> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("cannot build HTTP client: {}", e))?;

    let mut mismatches = Vec::new();
    let mut checked = 0usize;

    for (url, entry) in &manifest.files {
        checked += 1;
        match client.get(request_url(base, url)).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let headers = resp.headers().clone();
                match resp.bytes().await {
                    Ok(body) => check_file(
                        url,
                        entry,
                        &manifest.universal_headers,
                        status,
                        &headers,
                        Some(&body),
                        &mut mismatches,
                    ),
                    Err(e) => mismatches.push(format!("{}: body read failed: {}", url, e)),
                }
            }
            Err(e) => mismatches.push(format!("{}: request failed: {}", url, e)),
        }
    }

    for (from, to) in &manifest.redirects {
        checked += 1;
        match client.get(request_url(base, from)).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                if status != 301 {
                    mismatches.push(format!("{}: status {} (manifest says 301)", from, status));
                }
                match resp.headers().get("location").and_then(|v| v.to_str().ok()) {
                    Some(got) if got == to => {}
                    Some(got) => mismatches.push(format!(
                        "{}: redirects to {:?} (manifest says {:?})",
                        from, got, to
                    )),
                    None => mismatches.push(format!("{}: 301 without a location header", from)),
                }
            }
            Err(e) => mismatches.push(format!("{}: request failed: {}", from, e)),
        }
    }

    for url in &manifest.gone {
        checked += 1;
        match client.get(request_url(base, url)).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                if status != 410 {
                    mismatches.push(format!("{}: status {} (manifest says 410)", url, status));
                }
            }
            Err(e) => mismatches.push(format!("{}: request failed: {}", url, e)),
        }
    }

    // The fallback: an address outside the closure must answer with the
    // manifest's 404 page, not a bare error or -- worse -- a leak.
    checked += 1;
    match client.get(request_url(base, FALLBACK_PROBE)).send().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let headers = resp.headers().clone();
            match resp.bytes().await {
                Ok(body) => check_file(
                    FALLBACK_PROBE,
                    &manifest.fallback,
                    &manifest.universal_headers,
                    status,
                    &headers,
                    (!dynamic_fallback).then_some(&body[..]),
                    &mut mismatches,
                ),
                Err(e) => mismatches.push(format!("{}: body read failed: {}", FALLBACK_PROBE, e)),
            }
        }
        Err(e) => mismatches.push(format!("{}: request failed: {}", FALLBACK_PROBE, e)),
    }

    Ok(Outcome { checked, mismatches })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn urls_are_reencoded_for_the_wire() {
        assert_eq!(request_url("http://x:1/", "/fog over"), "http://x:1/fog%20over");
        assert_eq!(request_url("http://x:1", "/+design"), "http://x:1/+design");
    }

    #[test]
    fn header_and_body_mismatches_are_named() {
        let entry = FileEntry {
            hash: "sha256-9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
                .to_string(), // sha256("test")
            size: 4,
            status: 200,
            headers: BTreeMap::from([
                ("content-type".to_string(), "text/plain".to_string()),
                ("etag".to_string(), "\"ignored\"".to_string()),
            ]),
            provenance: None,
        };
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("content-type", "text/plain".parse().unwrap());
        let mut mismatches = Vec::new();
        check_file("/t", &entry, &[], 200, &headers, Some(b"test"), &mut mismatches);
        assert!(mismatches.is_empty(), "{:?}", mismatches);

        // Wrong body, wrong status, missing universal header -- all listed;
        // the etag difference is not.
        let universal = vec![("x-content-type-options".to_string(), "nosniff".to_string())];
        check_file("/t", &entry, &universal, 500, &headers, Some(b"other"), &mut mismatches);
        assert_eq!(mismatches.len(), 3, "{:?}", mismatches);

        // A dynamic-fallback probe passes body None: the differing body is
        // not compared, the wrong status still is.
        let mut relaxed = Vec::new();
        check_file("/t", &entry, &[], 404, &headers, None, &mut relaxed);
        assert_eq!(relaxed.len(), 1, "{:?}", relaxed);
    }
}
