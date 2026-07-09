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
        return seg.contents().starts_with(b"ICC_PROFILE\0");
    }
    !JPEG_DROP_MARKERS.contains(&m)
}

fn strip_jpeg(bytes: &[u8]) -> StripOutcome {
    let mut jpeg = match Jpeg::from_bytes(Bytes::copy_from_slice(bytes)) {
        Ok(j) => j,
        Err(_) => return StripOutcome::Failed,
    };

    // Read orientation before we drop the EXIF segment that carries it.
    let orientation = jpeg.exif().and_then(|tiff| read_orientation(&tiff));

    jpeg.segments_mut().retain(keep_jpeg_segment);

    // Re-attach a canonical, orientation-only EXIF so portrait photos are not
    // rendered sideways. Only when non-default (2..=8) — orientation 1 (or none)
    // needs no tag, keeping the output minimal.
    if let Some(o) = orientation {
        if (2..=8).contains(&o) {
            jpeg.set_exif(Some(Bytes::from(minimal_orientation_exif(o))));
        }
    }

    let mut out = Vec::with_capacity(bytes.len());
    match jpeg.encoder().write_to(&mut out) {
        Ok(_) => StripOutcome::Clean { bytes: out, content_type: "image/jpeg" },
        Err(_) => StripOutcome::Failed,
    }
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

    #[test]
    fn corrupt_supported_formats_fail_closed() {
        // Not valid JPEG/PNG/WebP bytes -> Failed, never a Clean passthrough.
        for ext in ["jpg", "png", "webp"] {
            assert!(
                matches!(strip(ext, b"definitely not an image"), StripOutcome::Failed),
                "{ext} garbage should fail closed"
            );
        }
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
