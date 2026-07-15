//! Image metadata stripping at serving time (`post-model.md` §8).
//!
//! Privacy is fail-closed: GPS, camera, and timestamp metadata is a real leak, so
//! every served image path (rendered *and* raw) runs through [`strip`] unless the
//! author opted the file into the exact original with an `original` tag.
//!
//! The strip is **surgical, not a re-encode**: we drop the metadata-bearing
//! container segments (EXIF, XMP, IPTC, comments, the embedded EXIF thumbnail —
//! which can hold the un-cropped original) while leaving the compressed pixel
//! data byte-identical and keeping the ICC color profile. This preserves both
//! quality (no generational JPEG loss) and color fidelity — the choice recorded
//! in §8 over a lossy `image`-crate re-encode.
//!
//! The one piece of metadata we deliberately *keep* is image **orientation**:
//! naively dropping it renders every portrait phone photo sideways. We read the
//! orientation value with a robust, panic-free parser (the input is attacker
//! controlled) and re-emit a canonical, orientation-only EXIF block. Because that
//! block is fixed for a given orientation, the stripped output is deterministic —
//! same input bytes always produce the same output bytes, so a strong ETag /
//! content hash over the stripped result stays stable (the §8 caching rule).
//!
//! Formats we cannot segment-strip in pure Rust (HEIC/HEIF/TIFF/AVIF, and the
//! GIF/BMP long tail) return [`StripOutcome::NeedsTranscode`]: the transcode path
//! (libvips, a sandboxed subprocess) cleans them by re-encoding to a metadata-free
//! JPEG. Until that path resolves them the caller withholds the bytes — never a
//! silent raw fallback.

use img_parts::jpeg::{markers, Jpeg};
use img_parts::png::Png;
use img_parts::webp::WebP;
use img_parts::{Bytes, ImageEXIF};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::Semaphore;

/// The result of trying to strip an image's metadata for serving.
pub enum StripOutcome {
    /// Metadata removed; these bytes are safe to serve. Pixel data is
    /// byte-identical to the source; only the ICC profile and a canonical
    /// orientation tag survive. `content_type` is the served MIME (the source's
    /// own type for a segment strip; the transcode path may change it).
    Clean {
        bytes: Vec<u8>,
        content_type: &'static str,
    },
    /// A format that can only be cleaned by transcoding (HEIC/HEIF/TIFF/AVIF/…).
    /// The transcode path handles it; until then the caller withholds (fail
    /// closed), never serving the unstripped original.
    NeedsTranscode,
    /// The bytes did not parse as the claimed image format, or re-serialization
    /// failed. Withhold rather than risk leaking unstripped metadata.
    Failed,
}

/// Strip metadata from an image for serving. `ext` is the file extension (any
/// case, aliases allowed); the bytes are the file's exact contents. Callers gate
/// on [`crate::entry::is_image_ext`] and the per-file `original` opt-in before
/// calling this.
pub fn strip(ext: &str, bytes: &[u8]) -> StripOutcome {
    match crate::entry::normalize_ext(ext).as_str() {
        "jpg" | "jpeg" => strip_jpeg(bytes),
        "png" => strip_png(bytes),
        "webp" => strip_webp(bytes),
        // Handled by the transcode path (libvips → clean JPEG). Fail closed until
        // then: an image we cannot verify-clean is not served raw.
        _ => StripOutcome::NeedsTranscode,
    }
}

// --- serving-gate classification -------------------------------------------

/// How a served file should be handled at the privacy gate.
pub enum Disposition {
    /// A raster image: strip or transcode before serving.
    Image,
    /// Media that routinely embeds location/camera metadata but that we have no
    /// cleaning path for yet (video, RAW, location audio). Withheld, never served
    /// raw — the fail-closed boundary the whole feature promises applies to
    /// *files*, not just to the still-image formats we happen to clean.
    Withhold,
    /// Carries no invisible location/camera metadata that we strip (text,
    /// markdown, PDF, ...): serve as-is.
    Raw,
}

/// Extensions of media that routinely embed GPS/camera metadata and that we
/// cannot yet clean — withheld rather than served raw. The extension is
/// authoritative here (checked before content sniffing): a RAW `.dng` is
/// TIFF-structured and would otherwise sniff as an image, but it must not be
/// handed to the image path.
fn is_withheld_media_ext(ext: &str) -> bool {
    matches!(
        crate::entry::normalize_ext(ext).as_str(),
        // Video — QuickTime/MP4 carry location atoms; the rest by extension.
        "mov" | "mp4" | "m4v" | "avi" | "mkv" | "webm" | "wmv" | "flv" | "mpg"
            | "mpeg" | "3gp" | "3g2" | "mts" | "m2ts" | "ts"
        // Location-bearing audio.
            | "m4a" | "aac"
        // Camera RAW — the richest EXIF/GPS of any format.
            | "dng" | "cr2" | "cr3" | "nef" | "nrw" | "arw" | "sr2" | "srf"
            | "raf" | "orf" | "rw2" | "pef" | "srw" | "x3f" | "raw" | "rwl" | "dcr"
    )
}

/// Best-effort content sniff of the leading bytes, so a *mislabeled* file is
/// classified by what it actually is rather than its extension: a raster image
/// routes to the strip/transcode, ISOBMFF audio/video is withheld. Returns `None`
/// when nothing is recognized (the caller then trusts the extension).
fn sniff(bytes: &[u8]) -> Option<Disposition> {
    if bytes.len() < 12 {
        return None;
    }
    let b = bytes;
    if b.starts_with(&[0xFF, 0xD8, 0xFF])                        // JPEG
        || b.starts_with(b"\x89PNG\r\n\x1a\n")                   // PNG
        || b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a")  // GIF
        || b.starts_with(b"BM")                                  // BMP
        || b.starts_with(&[0x49, 0x49, 0x2A, 0x00])              // TIFF (LE)
        || b.starts_with(&[0x4D, 0x4D, 0x00, 0x2A])              // TIFF (BE)
        || (b.starts_with(b"RIFF") && &b[8..12] == b"WEBP")      // WebP
    {
        return Some(Disposition::Image);
    }
    // ISOBMFF: `<size>ftyp<brand>`. HEIC/AVIF are images we transcode; MP4/MOV/
    // M4A share the same box but a different brand and are withheld.
    if &b[4..8] == b"ftyp" {
        let brand = &b[8..12];
        let is_image = matches!(
            brand,
            b"heic" | b"heix" | b"heif" | b"hevc" | b"mif1" | b"msf1" | b"avif" | b"avis"
        );
        return Some(if is_image { Disposition::Image } else { Disposition::Withhold });
    }
    None
}

/// Decide how a file is served at the privacy gate. Order matters: a known
/// metadata-bearing media extension is withheld first (so a RAW `.dng` never
/// reaches the image path via its TIFF magic); then declared images; then a
/// content sniff catches mislabeled photos/videos; anything else is served raw.
pub fn classify(ext: &str, bytes: &[u8]) -> Disposition {
    if is_withheld_media_ext(ext) {
        return Disposition::Withhold;
    }
    if crate::entry::is_image_ext(ext) {
        return Disposition::Image;
    }
    sniff(bytes).unwrap_or(Disposition::Raw)
}

// --- JPEG -------------------------------------------------------------------

/// JPEG APP markers that carry user/application metadata rather than structural
/// or color information — every one is dropped. `APP1` is EXIF *and* XMP; `APP13`
/// is Photoshop/IPTC; `APP3`–`APP12`/`APP15` hold assorted maker notes and
/// metadata. `COM` is a free-text comment.
///
/// Deliberately *kept*: `APP0` (JFIF density), `APP14` (Adobe color transform —
/// dropping it can shift CMYK/YCCK colors), and — filtered by content, not marker
/// — the ICC profile in `APP2`. Everything structural (SOF/DHT/DQT/SOS/…) is kept
/// by the catch-all in [`keep_jpeg_segment`].
const JPEG_DROP_MARKERS: &[u8] = &[
    markers::APP1,
    markers::APP3,
    markers::APP4,
    markers::APP5,
    markers::APP6,
    markers::APP7,
    markers::APP8,
    markers::APP9,
    markers::APP10,
    markers::APP11,
    markers::APP12,
    markers::APP13,
    markers::APP15,
    markers::COM,
];

/// Whether a JPEG segment survives the strip. Drops the metadata `APP` markers and
/// keeps everything else, with one content-level exception: `APP2` is kept only
/// when it is a real ICC profile, so a Multi-Picture Format (MPF) `APP2` — which
/// can embed a second, full-resolution image (a real leak) — is dropped.
fn keep_jpeg_segment(seg: &img_parts::jpeg::JpegSegment) -> bool {
    let m = seg.marker();
    if m == markers::APP2 {
        // ICC lives in APP2; an MPF/FlashPix APP2 (which can index an embedded
        // second image) is dropped.
        return seg.contents().starts_with(b"ICC_PROFILE\0");
    }
    if m == markers::APP0 {
        // Base JFIF density info is kept; a `JFXX` APP0 embeds a preview
        // thumbnail — the same un-cropped-preview leak class as the EXIF
        // thumbnail — so only a plain JFIF APP0 survives.
        return seg.contents().starts_with(b"JFIF\0");
    }
    !JPEG_DROP_MARKERS.contains(&m)
}

fn strip_jpeg(bytes: &[u8]) -> StripOutcome {
    // A JPEG carrying data after its primary EOI cannot be safely segment-stripped:
    // img-parts re-emits everything after the scan verbatim, so an appended trailer
    // — and its own metadata — would survive. This is exactly how phones ship a
    // Multi-Picture-Format second full-resolution image (Samsung/Google HDR, dual
    // lens) and a Motion Photo (an appended MP4), each with its own GPS. Route any
    // such file to the transcode, which re-encodes a single clean frame with no
    // trailer. A file we cannot even parse to an EOI is likewise not stripped.
    match primary_jpeg_end(bytes) {
        Some(end) if end == bytes.len() => {}
        _ => return StripOutcome::NeedsTranscode,
    }

    let mut jpeg = match Jpeg::from_bytes(Bytes::copy_from_slice(bytes)) {
        Ok(j) => j,
        Err(_) => return StripOutcome::Failed,
    };

    // Read orientation before we drop the EXIF segment that carries it.
    let orientation = jpeg.exif().and_then(|tiff| read_orientation(&tiff));

    jpeg.segments_mut().retain(keep_jpeg_segment);

    // Re-attach a canonical, orientation-only EXIF so portrait photos are not
    // rendered sideways — but only when there is room for img-parts to insert the
    // segment (it inserts at a fixed index and panics on a too-short segment list),
    // and only when orientation is non-default (2..=8; 1 or none needs no tag). A
    // pathological JPEG that strips to a near-empty segment list loses orientation
    // rather than panicking the request handler.
    if let Some(o) = orientation {
        if (2..=8).contains(&o) && jpeg.segments().len() >= 3 {
            jpeg.set_exif(Some(Bytes::from(minimal_orientation_exif(o))));
        }
    }

    let mut out = Vec::with_capacity(bytes.len());
    match jpeg.encoder().write_to(&mut out) {
        Ok(_) => StripOutcome::Clean { bytes: out, content_type: "image/jpeg" },
        Err(_) => StripOutcome::Failed,
    }
}

/// Byte offset just past the first complete JPEG image's EOI marker, or `None` if
/// the bytes are not a parseable JPEG. Bytes after this offset are a trailer (a
/// Motion-Photo video, an MPF second image, any appended data) that a segment
/// strip would preserve verbatim. Walks the marker structure, so a `FF D9` pair
/// inside entropy-coded data or a segment body is never mistaken for the EOI.
fn primary_jpeg_end(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 2 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None; // no SOI
    }
    let mut i = 2;
    loop {
        // A marker is 0xFF then a non-0xFF id, allowing 0xFF fill bytes between.
        if i + 1 >= bytes.len() || bytes[i] != 0xFF {
            return None;
        }
        let mut id = bytes[i + 1];
        i += 2;
        while id == 0xFF {
            id = *bytes.get(i)?;
            i += 1;
        }
        match id {
            0xD9 => return Some(i), // EOI — end of the primary image
            0x01 | 0xD0..=0xD7 => {} // standalone markers, no payload
            0xDA => {
                // Start of scan: skip the header, then walk entropy to the next
                // real marker (0xFF + a non-stuffing, non-restart, non-fill byte).
                i += jpeg_seg_len(bytes, i)?;
                loop {
                    if bytes.get(i)? == &0xFF {
                        match bytes.get(i + 1)? {
                            0x00 | 0xD0..=0xD7 => i += 2, // byte-stuffing / restart
                            0xFF => i += 1,               // fill byte
                            _ => break,                   // a real marker
                        }
                    } else {
                        i += 1;
                    }
                }
            }
            _ => i += jpeg_seg_len(bytes, i)?, // length-bearing marker: skip it
        }
    }
}

/// Big-endian 2-byte JPEG segment length at `pos` (the length counts its own 2
/// bytes). `None` if it is degenerate or runs past the buffer.
fn jpeg_seg_len(bytes: &[u8], pos: usize) -> Option<usize> {
    let len = ((*bytes.get(pos)? as usize) << 8) | (*bytes.get(pos + 1)? as usize);
    if len < 2 || pos + len > bytes.len() {
        return None;
    }
    Some(len)
}

// --- PNG --------------------------------------------------------------------

/// PNG ancillary chunks that carry text or metadata, all dropped: the textual
/// chunks (`tEXt`/`zTXt`/`iTXt`), an embedded EXIF block (`eXIf`), and the
/// last-modified timestamp (`tIME`). The ICC profile (`iCCP`) and every
/// rendering-relevant chunk (`IHDR`/`PLTE`/`IDAT`/`gAMA`/`sRGB`/…) are kept.
///
/// PNG orientation via `eXIf` exists but is vanishingly rare and widely ignored
/// by viewers, so we drop it rather than carry it — a PNG is a screenshot or
/// graphic far more often than a rotated camera capture.
const PNG_DROP_CHUNKS: &[[u8; 4]] = &[*b"tEXt", *b"zTXt", *b"iTXt", *b"eXIf", *b"tIME"];

fn strip_png(bytes: &[u8]) -> StripOutcome {
    let mut png = match Png::from_bytes(Bytes::copy_from_slice(bytes)) {
        Ok(p) => p,
        Err(_) => return StripOutcome::Failed,
    };
    for kind in PNG_DROP_CHUNKS {
        png.remove_chunks_by_type(*kind);
    }
    let mut out = Vec::with_capacity(bytes.len());
    match png.encoder().write_to(&mut out) {
        Ok(_) => StripOutcome::Clean { bytes: out, content_type: "image/png" },
        Err(_) => StripOutcome::Failed,
    }
}

// --- WebP -------------------------------------------------------------------

fn strip_webp(bytes: &[u8]) -> StripOutcome {
    let mut webp = match WebP::from_bytes(Bytes::copy_from_slice(bytes)) {
        Ok(w) => w,
        Err(_) => return StripOutcome::Failed,
    };
    // Remove the metadata chunks outright — this is what actually deletes the
    // EXIF/XMP data. The `VP8X` feature-flag bits that *advertise* those chunks
    // are left as-is (img-parts does not rewrite them); decoders treat a set flag
    // with a missing chunk as "absent", so no data leaks and rendering is
    // unaffected. The dangling bit carries no information beyond "there was
    // metadata", which the removal has already made false.
    webp.remove_chunks_by_id(*b"EXIF");
    webp.remove_chunks_by_id(*b"XMP ");
    let mut out = Vec::with_capacity(bytes.len());
    match webp.encoder().write_to(&mut out) {
        Ok(_) => StripOutcome::Clean { bytes: out, content_type: "image/webp" },
        Err(_) => StripOutcome::Failed,
    }
}

// --- shared helpers ---------------------------------------------------------

/// Lowercase-hex SHA-256 of served bytes — the strong ETag / content hash for a
/// stripped image (`post-model.md` §8: the cache identity derives from the
/// *stripped* output). Also the cache key for the transcode/thumbnail paths.
pub fn content_hash(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        out.push(char::from_digit((b >> 4) as u32, 16).unwrap());
        out.push(char::from_digit((b & 0xf) as u32, 16).unwrap());
    }
    out
}

// --- transcode (libvips subprocess) -----------------------------------------

/// Cap on concurrent libvips subprocesses. Decoding a large HEIC/HEIF is memory-
/// and CPU-heavy, so only a few run at once (mirrors the pandoc limiter).
static VIPS_SEMAPHORE: Semaphore = Semaphore::const_new(2);

/// Per-process counter that makes each transcode's temp filenames unique, so two
/// concurrent requests for the *same* image never write the same scratch files.
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Wall-clock ceiling for one transcode. A hostile image that makes libvips spin
/// or hang is killed (`kill_on_drop`) rather than pinning a worker forever.
const VIPS_TIMEOUT: Duration = Duration::from_secs(25);

/// Longest edge of the transcoded full view. Capped for size/DoS; the exact
/// full-resolution file is reachable only via the `original` opt-in.
const TRANSCODE_MAX_EDGE: &str = "4096";

/// Longest edge of a gallery thumbnail — small enough to keep a grid light, large
/// enough to stay crisp on a 2x display.
const THUMB_MAX_EDGE: &str = "600";

/// Version tags folded into the cache key, so changing a pipeline's parameters
/// (size/quality) invalidates old cache entries without a manual purge. The tag
/// also separates the full-view and thumbnail caches for the same source image.
const TRANSCODE_TAG: &str = "v1-j4096q85";
const THUMB_TAG: &str = "v1-t600q80";

/// The prepared bytes to serve for an image request.
pub enum Prepared {
    /// Serve these bytes with this content type (stripped or transcoded-clean).
    Ready {
        bytes: Vec<u8>,
        content_type: &'static str,
    },
    /// Fail closed: the image could not be cleaned (corrupt, or the transcode
    /// failed / timed out). The caller withholds the bytes.
    Withheld,
}

/// Clean an image for serving: a pure-Rust segment strip when we can ([`strip`]),
/// else a libvips transcode to a metadata-free JPEG. The transcode output is
/// disk-cached out-of-tree under `<cache_dir>/media`, keyed by the source content
/// hash, so the subprocess runs at most once per unique image. `original` files
/// never reach here — the caller serves their exact bytes directly.
pub async fn prepare(ext: &str, bytes: &[u8], cache_dir: &Path) -> Prepared {
    match strip(ext, bytes) {
        StripOutcome::Clean { bytes, content_type } => Prepared::Ready { bytes, content_type },
        StripOutcome::Failed => Prepared::Withheld,
        StripOutcome::NeedsTranscode => {
            match vips_clean_jpeg(ext, bytes, cache_dir, TRANSCODE_MAX_EDGE, TRANSCODE_TAG).await {
                Some(bytes) => Prepared::Ready { bytes, content_type: "image/jpeg" },
                None => Prepared::Withheld,
            }
        }
    }
}

/// Produce a small, clean JPEG thumbnail for a gallery tile. Unlike [`prepare`],
/// this always goes through libvips (every format, including JPEG/PNG, needs the
/// resize), and it ignores the `original` opt-in — a tile is a derived preview,
/// so it is stripped even when the full asset it links to is served exact. Cached
/// separately from the full view via [`THUMB_TAG`].
pub async fn thumbnail(ext: &str, bytes: &[u8], cache_dir: &Path) -> Prepared {
    match vips_clean_jpeg(ext, bytes, cache_dir, THUMB_MAX_EDGE, THUMB_TAG).await {
        Some(bytes) => Prepared::Ready { bytes, content_type: "image/jpeg" },
        None => Prepared::Withheld,
    }
}

/// Decode any raster with libvips and re-encode a clean JPEG capped at
/// `max_edge`, keeping the source ICC and baking in orientation; the JPEG still
/// carries the source EXIF/GPS, so it is run back through the segment strip to
/// drop the metadata while keeping the color profile libvips preserved. Result is
/// cached out-of-tree keyed by source hash + `tag`, so the subprocess runs at
/// most once per (image, size). Returns the cleaned bytes, or `None` on any
/// failure (fail closed).
async fn vips_clean_jpeg(
    ext: &str,
    bytes: &[u8],
    cache_dir: &Path,
    max_edge: &str,
    tag: &str,
) -> Option<Vec<u8>> {
    let media = cache_dir.join("media");
    tokio::fs::create_dir_all(&media).await.ok()?;

    let key = format!("{}-{}", content_hash(bytes), tag);
    let cached = media.join(format!("{key}.jpg"));
    if let Ok(b) = tokio::fs::read(&cached).await {
        return Some(b);
    }

    // Serialize heavy work behind the limiter, then re-check the cache: a
    // concurrent request for the same image may have produced it while we waited.
    let _permit = VIPS_SEMAPHORE.acquire().await.ok()?;
    if let Ok(b) = tokio::fs::read(&cached).await {
        return Some(b);
    }

    let uniq = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let norm = crate::entry::normalize_ext(ext);
    let tin = media.join(format!(".{key}.{uniq}.in.{norm}"));
    let traw = media.join(format!(".{key}.{uniq}.raw.jpg"));

    let ran = run_vips_transcode(bytes, &tin, &traw, max_edge).await;
    let _ = tokio::fs::remove_file(&tin).await; // never leave untrusted input around
    let raw = if ran { tokio::fs::read(&traw).await.ok() } else { None };
    let _ = tokio::fs::remove_file(&traw).await;
    let raw = raw?;

    // libvips carried the source EXIF/GPS into the JPEG (confirmed) — strip it,
    // keeping the ICC/orientation the transcode preserved.
    let cleaned = match strip_jpeg(&raw) {
        StripOutcome::Clean { bytes, .. } => bytes,
        _ => return None,
    };

    // Publish to the cache atomically so a concurrent reader never sees a partial
    // file (write a sibling temp, then rename).
    let wip = media.join(format!(".{key}.{uniq}.wip.jpg"));
    if tokio::fs::write(&wip, &cleaned).await.is_ok() {
        let _ = tokio::fs::rename(&wip, &cached).await;
    }
    Some(cleaned)
}

/// Run one libvips transcode as an isolated subprocess. Untrusted bytes go to a
/// temp file we name (libvips sniffs the real format from content, not the
/// extension); the output is a JPEG capped at `max_edge`. Hardening: only a local
/// file path is ever passed (libvips makes no network request), a single internal
/// thread, the ImageMagick/untrusted loaders blocked, a wall-clock timeout with
/// kill-on-drop, and — on Unix — an RLIMIT_CPU backstop. Returns whether it
/// succeeded.
async fn run_vips_transcode(bytes: &[u8], tin: &Path, tout: &Path, max_edge: &str) -> bool {
    if tokio::fs::write(tin, bytes).await.is_err() {
        return false;
    }

    let mut std_cmd = std::process::Command::new("vips");
    std_cmd
        .arg("thumbnail")
        .arg(tin)
        .arg(format!("{}[Q=85]", tout.display()))
        .arg(max_edge)
        .arg("--size")
        .arg("down")
        // One worker thread: predictable memory, no thread-count amplification.
        .env("VIPS_CONCURRENCY", "1")
        // Refuse every loader libvips flags "untrusted" -- crucially its bundled
        // ImageMagick fallback (`magickload`), whose long RCE history on hostile
        // images is exactly why we did not shell out to ImageMagick directly. Our
        // formats decode via vetted native loaders (heifload/tiffload/gifload/...),
        // so this only fail-closes the ImageMagick-only tail (BMP, JXL, JP2K):
        // those are withheld rather than decoded by untrusted code. Verified: a
        // BMP is refused with the flag set, HEIC still transcodes.
        .env("VIPS_BLOCK_UNTRUSTED", "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    apply_rlimits(&mut std_cmd);

    let mut cmd = Command::from(std_cmd);
    cmd.kill_on_drop(true);
    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("could not spawn vips (is it on PATH?): {e}");
            return false;
        }
    };
    match tokio::time::timeout(VIPS_TIMEOUT, child.wait_with_output()).await {
        Ok(Ok(out)) if out.status.success() => true,
        Ok(Ok(out)) => {
            tracing::warn!("vips transcode failed: {}", String::from_utf8_lossy(&out.stderr).trim());
            false
        }
        Ok(Err(e)) => {
            tracing::warn!("vips process error: {e}");
            false
        }
        Err(_) => {
            tracing::warn!("vips transcode timed out after {}s", VIPS_TIMEOUT.as_secs());
            false
        }
    }
}

/// Apply an RLIMIT_CPU backstop to the child before exec (Unix only): a hostile
/// image that makes libvips burn CPU is capped even if the async timeout is
/// somehow missed. RLIMIT_AS is deliberately not set — virtual-address limits are
/// blunt and can break legitimate large decodes; container memory limits
/// (cgroups) are the right deployment control.
#[cfg(unix)]
fn apply_rlimits(cmd: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;
    // SAFETY: `pre_exec` runs in the forked child before `exec`. `setrlimit` is
    // async-signal-safe and the closure allocates nothing.
    unsafe {
        cmd.pre_exec(|| {
            let secs: libc::rlim_t = 30;
            let rl = libc::rlimit { rlim_cur: secs, rlim_max: secs };
            libc::setrlimit(libc::RLIMIT_CPU, &rl);
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn apply_rlimits(_cmd: &mut std::process::Command) {}

/// Whether an image carries embedded metadata a viewer would consider sensitive —
/// GPS coordinates, camera make/model, or the original capture timestamp. Used to
/// decide whether a `public-original` image needs the loud "this publishes your
/// location/camera metadata" warning (post-model.md §8): the warning fires only
/// when there is actually something to leak. Reads the whole container (JPEG,
/// TIFF, PNG, WebP, HEIF), never panics on malformed input.
pub fn has_sensitive_metadata(bytes: &[u8]) -> bool {
    let mut cursor = std::io::Cursor::new(bytes);
    match exif::Reader::new().read_from_container(&mut cursor) {
        Ok(exif) => {
            exif.get_field(exif::Tag::GPSLatitude, exif::In::PRIMARY).is_some()
                || exif.get_field(exif::Tag::GPSLongitude, exif::In::PRIMARY).is_some()
                || exif.get_field(exif::Tag::Make, exif::In::PRIMARY).is_some()
                || exif.get_field(exif::Tag::Model, exif::In::PRIMARY).is_some()
                || exif.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY).is_some()
        }
        Err(_) => false,
    }
}

/// Read the EXIF orientation tag (1–8) from a raw TIFF/EXIF block. Uses the
/// kamadak-exif parser, which returns an error (never panics) on malformed input —
/// important because the block is attacker-controlled. Returns `None` when EXIF is
/// absent, unparseable, or has no orientation.
fn read_orientation(tiff: &[u8]) -> Option<u16> {
    let reader = exif::Reader::new();
    let exif = reader.read_raw(tiff.to_vec()).ok()?;
    let field = exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)?;
    u16::try_from(field.value.get_uint(0)?).ok()
}

/// Build a canonical little-endian TIFF/EXIF block carrying a single IFD0 entry:
/// Orientation (tag `0x0112`, type SHORT, count 1). Fixed layout, so the output is
/// deterministic for a given orientation — the property the immutable-cache /
/// content-hash story relies on. 26 bytes; img-parts prepends the `Exif\0\0`
/// application marker when it writes the segment.
fn minimal_orientation_exif(orientation: u16) -> Vec<u8> {
    let o = orientation.to_le_bytes();
    vec![
        b'I', b'I', // byte order: little-endian
        0x2A, 0x00, // TIFF magic (42)
        0x08, 0x00, 0x00, 0x00, // offset to IFD0 = 8
        0x01, 0x00, // IFD0 entry count = 1
        0x12, 0x01, // tag 0x0112 (Orientation)
        0x03, 0x00, // type 3 (SHORT)
        0x01, 0x00, 0x00, 0x00, // count = 1
        o[0], o[1], 0x00, 0x00, // value: SHORT in the first 2 bytes, padded
        0x00, 0x00, 0x00, 0x00, // next-IFD offset = 0 (none)
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_exif_roundtrips_orientation() {
        // The block we emit must be parseable back to the same orientation by an
        // independent EXIF reader — proves it is well-formed for real decoders.
        for o in 1u16..=8 {
            let tiff = minimal_orientation_exif(o);
            assert_eq!(read_orientation(&tiff), Some(o), "orientation {o}");
        }
    }

    #[test]
    fn minimal_exif_is_deterministic_and_fixed_size() {
        assert_eq!(minimal_orientation_exif(6).len(), 26);
        assert_eq!(minimal_orientation_exif(6), minimal_orientation_exif(6));
        assert_ne!(minimal_orientation_exif(1), minimal_orientation_exif(6));
    }

    #[test]
    fn read_orientation_rejects_garbage_without_panicking() {
        assert_eq!(read_orientation(&[]), None);
        assert_eq!(read_orientation(b"not a tiff at all"), None);
        assert_eq!(read_orientation(&[0xFF; 64]), None);
    }

    #[test]
    fn unsupported_formats_need_transcode() {
        for ext in ["heic", "heif", "tiff", "avif", "gif", "bmp"] {
            assert!(
                matches!(strip(ext, b"\x00\x01\x02\x03"), StripOutcome::NeedsTranscode),
                "{ext} should route to transcode"
            );
        }
    }

    #[tokio::test]
    async fn prepare_fails_closed_on_corrupt_supported_format() {
        // A corrupt JPEG fails the segment strip and never reaches the
        // transcoder, so `prepare` withholds it (the cache dir is untouched).
        let out = prepare("jpg", b"not a jpeg", std::path::Path::new("/nonexistent")).await;
        assert!(matches!(out, Prepared::Withheld));
    }

    #[test]
    fn corrupt_supported_formats_never_pass_through_clean() {
        // Garbage must never be served as a clean strip. An unparseable JPEG
        // routes to the transcode (it fails closed there); PNG/WebP fail directly.
        // Either way: never Clean.
        for ext in ["jpg", "png", "webp"] {
            assert!(
                !matches!(strip(ext, b"definitely not an image"), StripOutcome::Clean { .. }),
                "{ext} garbage must not strip clean"
            );
        }
    }

    /// A minimal but structurally valid baseline JPEG: SOI, an empty APP0, a SOS
    /// with a short header, three entropy bytes, then EOI.
    fn tiny_jpeg() -> Vec<u8> {
        vec![
            0xFF, 0xD8, // SOI
            0xFF, 0xE0, 0x00, 0x02, // APP0, length 2 (no payload)
            0xFF, 0xDA, 0x00, 0x02, // SOS, header length 2 (no payload)
            0x11, 0x22, 0x33, // entropy
            0xFF, 0xD9, // EOI
        ]
    }

    #[test]
    fn primary_jpeg_end_finds_the_eoi_and_detects_trailers() {
        let jpeg = tiny_jpeg();
        // A clean single-frame JPEG ends exactly at its length.
        assert_eq!(primary_jpeg_end(&jpeg), Some(jpeg.len()));

        // An appended trailer (MPF second image / Motion-Photo MP4) is detected:
        // the primary EOI is before the end of the buffer.
        let mut with_trailer = jpeg.clone();
        with_trailer.extend_from_slice(b"TRAILER_WITH_GPS");
        assert_eq!(primary_jpeg_end(&with_trailer), Some(jpeg.len()));
        assert!(primary_jpeg_end(&with_trailer).unwrap() < with_trailer.len());

        // A `FF D9` byte pair *inside* entropy is byte-stuffed (`FF 00`) or a
        // restart marker in real files, never a bare EOI; a stray pair in the
        // scan must not be read as the end. Here entropy contains `FF 00`
        // (stuffing) then the real EOI.
        let stuffed = vec![
            0xFF, 0xD8, 0xFF, 0xDA, 0x00, 0x02, 0xFF, 0x00, 0x44, 0xFF, 0xD9,
        ];
        assert_eq!(primary_jpeg_end(&stuffed), Some(stuffed.len()));

        // Not a JPEG.
        assert_eq!(primary_jpeg_end(b"not a jpeg"), None);
        assert_eq!(primary_jpeg_end(&[0xFF, 0xD8]), None); // SOI only, no EOI
    }

    #[test]
    fn trailered_jpeg_routes_to_transcode_not_a_raw_leak() {
        let mut jpeg = tiny_jpeg();
        jpeg.extend_from_slice(b"SECRET_TRAILER");
        // A trailered JPEG must never be segment-stripped (which would re-serve
        // the trailer) — it routes to the transcode path instead.
        assert!(matches!(strip("jpg", &jpeg), StripOutcome::NeedsTranscode));
    }

    #[test]
    fn classify_gates_the_whole_file_boundary() {
        // Declared images -> Image.
        assert!(matches!(classify("jpg", b""), Disposition::Image));
        assert!(matches!(classify("heic", b""), Disposition::Image));
        // Metadata-bearing media we cannot clean -> Withhold (never raw), even
        // though a `.dng` is TIFF-structured and would otherwise sniff as image.
        for ext in ["mov", "mp4", "m4a", "dng", "cr2", "nef", "arw"] {
            assert!(matches!(classify(ext, b""), Disposition::Withhold), "{ext}");
        }
        // A mislabeled photo is caught by content sniffing, not its extension.
        let jpeg_magic = [0xFF, 0xD8, 0xFF, 0xE0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert!(matches!(classify("bin", &jpeg_magic), Disposition::Image));
        // A mislabeled MP4 (ISOBMFF, non-image brand) is withheld.
        let mut mp4 = vec![0, 0, 0, 0x18];
        mp4.extend_from_slice(b"ftypmp42____");
        assert!(matches!(classify("txt", &mp4), Disposition::Withhold));
        // A HEIC-branded ISOBMFF is an image.
        let mut heic = vec![0, 0, 0, 0x18];
        heic.extend_from_slice(b"ftypheic____");
        assert!(matches!(classify("bin", &heic), Disposition::Image));
        // Ordinary text/docs are served raw.
        assert!(matches!(classify("txt", b"just some text here"), Disposition::Raw));
        assert!(matches!(classify("pdf", b"%PDF-1.7 ....."), Disposition::Raw));
    }

    #[test]
    fn image_extension_aliases_are_gated() {
        // The strip gate and dispatch must recognize the alias spellings, or a
        // `.tif`/`.jpe`/`.jfif` would slip past and serve raw with EXIF.
        for ext in ["tif", "jpe", "jfif", "jif"] {
            assert!(crate::entry::is_image_ext(ext), "{ext} should be an image");
        }
        // `.tif` -> tiff -> transcode; `.jpe` -> jpeg, and an unparseable stream
        // routes to the transcode (which fails closed there), never a clean pass.
        assert!(matches!(strip("tif", b"x"), StripOutcome::NeedsTranscode));
        assert!(matches!(strip("jpe", b"x"), StripOutcome::NeedsTranscode));
    }

    #[test]
    fn keep_list_drops_metadata_keeps_structure() {
        use img_parts::jpeg::JpegSegment;
        // APP1 (EXIF/XMP), APP13 (IPTC), COM are dropped; APP0 (JFIF), a real ICC
        // APP2, APP14 (Adobe), and structural SOF0 are kept.
        let drop = |m: u8, body: &[u8]| {
            !keep_jpeg_segment(&JpegSegment::new_with_contents(m, Bytes::copy_from_slice(body)))
        };
        assert!(drop(markers::APP1, b"Exif\0\0"));
        assert!(drop(markers::APP13, b"Photoshop 3.0\0"));
        assert!(drop(markers::COM, b"a comment"));
        // MPF rides in APP2 too — must be dropped (only ICC APP2 survives).
        assert!(drop(markers::APP2, b"MPF\0"));

        let keep = |m: u8, body: &[u8]| {
            keep_jpeg_segment(&JpegSegment::new_with_contents(m, Bytes::copy_from_slice(body)))
        };
        assert!(keep(markers::APP0, b"JFIF\0"));
        assert!(keep(markers::APP2, b"ICC_PROFILE\0"));
        assert!(keep(markers::APP14, b"Adobe"));
        assert!(keep(markers::SOF0, b""));
    }
}
