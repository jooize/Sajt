//! The file-privacy boundary: serve-time classification and metadata stripping
//! (`post-model.md` §8).
//!
//! Every served file passes [`classify`], an **allowlist**: raster images are
//! cleaned (segment strip or sandboxed transcode), PDFs and SVGs are rewritten
//! without their metadata, author-readable UTF-8 text is served as-is, and
//! everything else is withheld — fail closed — with the author's per-file
//! `public-original` tag as the explicit exact-bytes escape. GPS, camera, and
//! timestamp metadata is a real leak, so every served image path (rendered
//! *and* raw) runs through [`strip`] unless the author opted in.
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
//! GIF/BMP long tail) return [`StripOutcome::NeedsTranscode`]: libvips, running
//! as an **OS-sandboxed subprocess** (Seatbelt on macOS, bubblewrap on Linux),
//! re-encodes them to a metadata-free JPEG. The decoder parses attacker-
//! controlled bytes, so even a decoder compromise is confined to a per-job
//! scratch directory — no network, no view of the content tree. Without sandbox
//! tooling the transcode path is disabled and those formats are withheld
//! (`--unsandboxed-transcode` overrides, loudly). Anything the transcode cannot
//! clean is withheld — never a silent raw fallback.
//!
//! No cleaner is trusted on the way out. For raster images, every producer
//! (segment strip, transcode, thumbnail) funnels through one disk pipeline:
//! the cleaned output lands in a quarantine file, *that file* is re-read and
//! verified by independent parsers (kamadak-exif finds no sensitive metadata;
//! `imagesize` confirms it still parses as the claimed container), and only a
//! verified file is promoted — atomic rename — into the clean store
//! (`<cache_dir>/media/clean/`), the sole byte source the serving layer reads.
//! Store reads re-verify on every request, so even a corrupted or tampered
//! store file withholds loudly instead of serving. [`CleanBytes`] makes the
//! gate structural: its only constructor is the verifier, so no code path can
//! hand the serving layer unverified image bytes. PDFs and SVGs re-check their
//! output in memory the same spirit (`pdf_is_clean`, `svg_is_clean`). A strip
//! bug becomes a loud withhold, never a leak.

use img_parts::jpeg::{markers, Jpeg};
use img_parts::png::Png;
use img_parts::webp::WebP;
use img_parts::{Bytes, ImageEXIF};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
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
        // Handled by the transcode path (sandboxed libvips → clean JPEG). Fail
        // closed on the way: an image we cannot verify-clean is not served raw.
        _ => StripOutcome::NeedsTranscode,
    }
}

// --- serving-gate classification -------------------------------------------

/// How a served file should be handled at the privacy gate.
pub enum Disposition {
    /// A raster image: strip or transcode before serving.
    Image,
    /// A PDF: its `/Info`/XMP document metadata is stripped ([`strip_pdf`])
    /// before serving; unparseable or encrypted ones are withheld.
    Pdf,
    /// An SVG: rewritten without its metadata ([`strip_svg`]) before serving;
    /// unparseable ones (or uncleanable embedded rasters) are withheld.
    Svg,
    /// A file we cannot verify-clean: withheld, never served raw. This is the
    /// fail-closed *default* — the boundary is an allowlist (cleaned images,
    /// author-readable text), not a denylist of known-bad formats, so a format
    /// nobody thought about is a 415, not a leak. The author's per-file
    /// `public-original` tag is the universal escape: it serves the exact bytes
    /// of any withheld format, on the author's explicit say-so.
    Withhold,
    /// Author-readable text (valid UTF-8): every byte is visible in any editor,
    /// so there is nothing invisible to strip. Served as-is.
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

/// Decide how a file is served at the privacy gate. The boundary is an
/// **allowlist**: only what we can clean (raster images) or what the author can
/// fully read (UTF-8 text) is served by default; everything else — video, RAW,
/// PDF, SVG, Office documents, archives, and every format nobody listed — is
/// withheld until it has a cleaning path (PDF and SVG strips are next), with the
/// per-file `public-original` tag as the author's exact-bytes escape.
///
/// Order matters: a known metadata-bearing media extension is withheld first (so
/// a RAW `.dng` never reaches the image path via its TIFF magic); then declared
/// images; then a content sniff catches mislabeled photos/videos; then the text
/// gate; the default is Withhold, never raw.
pub fn classify(ext: &str, bytes: &[u8]) -> Disposition {
    if is_withheld_media_ext(ext) {
        return Disposition::Withhold;
    }
    if crate::entry::is_image_ext(ext) {
        return Disposition::Image;
    }
    if let Some(d) = sniff(bytes) {
        return d;
    }
    // PDF and SVG carry metadata that is invisible in the *rendered* view (XMP
    // author/tool info, editor filesystem paths) even though the bytes may be
    // valid UTF-8 — they must not slip through the text gate; each has its own
    // strip path.
    if crate::entry::normalize_ext(ext) == "pdf" || bytes.starts_with(b"%PDF-") {
        return Disposition::Pdf;
    }
    if crate::entry::normalize_ext(ext) == "svg" {
        return Disposition::Svg;
    }
    // Author-readable text: every byte visible in an editor — nothing invisible
    // to strip. (UTF-16/legacy encodings are withheld: we cannot cheaply prove
    // they are what the author read.)
    if is_readable_text(bytes) {
        return Disposition::Raw;
    }
    Disposition::Withhold
}

/// Whether bytes are text an author has actually *seen*: valid UTF-8 with no
/// control characters beyond tab/newline/CR. Plain "valid UTF-8" is not enough —
/// a ZIP local-file header or any binary framing that stays under 0x80 decodes
/// fine but is not readable text, and readability is the entire justification
/// for serving the bytes unstripped.
fn is_readable_text(bytes: &[u8]) -> bool {
    match std::str::from_utf8(bytes) {
        Ok(s) => !s.chars().any(|c| c.is_control() && !matches!(c, '\t' | '\n' | '\r')),
        Err(_) => false,
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

// --- PDF ----------------------------------------------------------------------

/// Strip a PDF's document-level metadata for serving: the `/Info` dictionary
/// (Author, Creator, Producer, creation/modification dates — the exact fields
/// that identify a person and their tooling), every XMP `/Metadata` stream (the
/// same data in RDF form, attachable to *any* object), and `/PieceInfo`
/// (application-private page data, where creative tools stash provenance).
/// Content the author can see in a PDF viewer — the rendered pages, outlines,
/// annotations — is untouched.
///
/// Because lopdf re-serializes the parsed document, **prior incremental
/// generations are dropped too**: a PDF edited in place (the usual way tools
/// update metadata) keeps its old metadata bytes in the file, invisible but
/// recoverable — a rewrite from the object table leaves that shadow history
/// behind.
///
/// Returns `None` — the caller withholds — for encrypted PDFs (we cannot see
/// what we would be serving) and for anything lopdf cannot parse or re-save.
/// Output is deterministic for a given input (no timestamps are introduced), so
/// the content-hash ETag over the stripped bytes is stable.
pub fn strip_pdf(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut doc = match lopdf::Document::load_mem(bytes) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("PDF withheld: parse failed ({e})");
            return None;
        }
    };
    if doc.trailer.get(b"Encrypt").is_ok() {
        tracing::warn!("PDF withheld: encrypted");
        return None;
    }

    // The /Info dictionary: drop the trailer key and the object it points at.
    if let Ok(info) = doc.trailer.get(b"Info") {
        if let Ok(id) = info.as_reference() {
            doc.delete_object(id);
        }
    }
    doc.trailer.remove(b"Info");

    // XMP metadata streams and application piece-info, wherever they hang:
    // remove the dict keys everywhere, remember what they referenced, then
    // delete those objects — plus any orphan whose /Type is /Metadata (an
    // unreferenced stream would otherwise still be written out).
    let mut doomed: Vec<lopdf::ObjectId> = Vec::new();
    for (&id, obj) in doc.objects.iter_mut() {
        let dict = match obj {
            lopdf::Object::Dictionary(d) => d,
            lopdf::Object::Stream(s) => &mut s.dict,
            _ => continue,
        };
        for key in [b"Metadata".as_slice(), b"PieceInfo".as_slice()] {
            if let Some(gone) = dict.remove(key) {
                if let Ok(rid) = gone.as_reference() {
                    doomed.push(rid);
                }
            }
        }
        if matches!(dict.get(b"Type"), Ok(lopdf::Object::Name(n)) if n == b"Metadata") {
            doomed.push(id);
        }
    }
    for id in doomed {
        doc.delete_object(id);
    }

    let mut out = Vec::new();
    if let Err(e) = doc.save_to(&mut out) {
        tracing::warn!("PDF withheld: re-save failed ({e})");
        return None;
    }
    // Verify before anything is served: re-parse the *output* and assert every
    // stripped channel is actually gone. The strip above is trusted for nothing —
    // a logic bug in it becomes a withhold plus a loud log, never a leak.
    if !pdf_is_clean(&out) {
        tracing::error!("PDF withheld: strip verification failed — output still carries metadata");
        return None;
    }
    Some(out)
}

/// Post-strip verification: parse the stripped bytes fresh and check that no
/// stripped metadata channel remains (`/Info` in the trailer, `/Metadata` or
/// `/PieceInfo` on any object, any object typed `/Metadata`). Unparseable
/// output is *not* clean — fail closed.
fn pdf_is_clean(bytes: &[u8]) -> bool {
    let doc = match lopdf::Document::load_mem(bytes) {
        Ok(d) => d,
        Err(_) => return false,
    };
    if doc.trailer.get(b"Info").is_ok() {
        return false;
    }
    doc.objects.values().all(|obj| {
        let dict = match obj {
            lopdf::Object::Dictionary(d) => d,
            lopdf::Object::Stream(s) => &s.dict,
            _ => return true,
        };
        dict.get(b"Metadata").is_err()
            && dict.get(b"PieceInfo").is_err()
            && !matches!(dict.get(b"Type"), Ok(lopdf::Object::Name(n)) if n == b"Metadata")
    })
}

// --- SVG ----------------------------------------------------------------------

/// Namespaces whose elements and attributes are part of the image itself and
/// survive the strip. Everything else — Dublin Core (`dc:creator`), RDF/CC
/// license blocks, and the editor namespaces (`sodipodi:docname` and
/// `inkscape:export-filename` carry the author's real filesystem paths) — is
/// dropped.
const SVG_KEEP_NAMESPACES: &[&[u8]] = &[
    b"http://www.w3.org/2000/svg",
    b"http://www.w3.org/1999/xlink",
    b"http://www.w3.org/XML/1998/namespace",
];

/// SVG-namespace elements dropped with their whole subtree: `metadata` is the
/// standard metadata container (RDF/Dublin Core — creator, license, tool);
/// `script` never belongs in a served image (the CSP already jails it — this is
/// defense in depth, not the primary control). `title`/`desc` are deliberately
/// *kept*: they are accessibility content, read out by screen readers.
const SVG_DROP_LOCAL: &[&[u8]] = &[b"metadata", b"script"];

/// Strip an SVG's metadata for serving: a streaming XML rewrite (quick-xml)
/// that copies the document through verbatim except for what it removes —
/// `<metadata>` subtrees, elements and attributes in non-SVG namespaces,
/// comments, processing instructions, `<script>` subtrees and `on*`/
/// `javascript:` attributes. A `<image>` embedding a raster as a base64 `data:`
/// URI is decoded and run through the raster [`strip`] (an embedded JPEG can
/// carry a full EXIF/GPS payload); a payload we cannot verify-clean withholds
/// the whole file.
///
/// Fail closed: any parse error, a `DOCTYPE` (entity machinery we will not
/// interpret), a non-UTF-8 document, or an uncleanable embedded image returns
/// `None` and the caller withholds. Because we only ever *remove* well-formed
/// events, well-formed input stays well-formed. Output is deterministic.
pub fn strip_svg(bytes: &[u8]) -> Option<Vec<u8>> {
    use quick_xml::events::{BytesStart, Event};

    // Non-UTF-8 documents are withheld outright (quick-xml would otherwise
    // decode per the XML declaration; we never serve what we cannot read).
    if std::str::from_utf8(bytes).is_err() {
        tracing::warn!("SVG withheld: not UTF-8");
        return None;
    }

    let mut reader = quick_xml::NsReader::from_reader(bytes);
    let mut writer = quick_xml::Writer::new(Vec::with_capacity(bytes.len()));
    let mut skip_depth = 0usize; // inside a dropped subtree when > 0

    loop {
        let event = match reader.read_resolved_event() {
            Ok((_, Event::Eof)) => break,
            Ok((resolved, event)) => {
                // Everything below decides whether this event survives.
                match &event {
                    Event::DocType(_) => {
                        tracing::warn!("SVG withheld: DOCTYPE (entity definitions) present");
                        return None;
                    }
                    Event::Comment(_) | Event::PI(_) => continue, // metadata channels
                    Event::Start(e) | Event::Empty(e) => {
                        if skip_depth > 0 {
                            if matches!(event, Event::Start(_)) {
                                skip_depth += 1;
                            }
                            continue;
                        }
                        if svg_element_dropped(&resolved, e.local_name().as_ref()) {
                            if matches!(event, Event::Start(_)) {
                                skip_depth = 1;
                            }
                            continue;
                        }
                        // Rebuild the tag with only the surviving attributes.
                        let mut clean =
                            BytesStart::new(String::from_utf8_lossy(e.name().as_ref()).into_owned());
                        for attr in e.attributes() {
                            let attr = match attr {
                                Ok(a) => a,
                                Err(e) => {
                                    tracing::warn!("SVG withheld: bad attribute ({e})");
                                    return None;
                                }
                            };
                            let key = attr.key;
                            // Namespace declarations: keep only bindings to the
                            // allowed namespaces. A leftover xmlns:inkscape or
                            // xmlns:sodipodi is unused after the strip, and the
                            // URI alone fingerprints the author's tooling.
                            if key.as_namespace_binding().is_some() {
                                let uri = attr.decode_and_unescape_value(reader.decoder()).ok()?;
                                if SVG_KEEP_NAMESPACES.contains(&uri.as_bytes()) {
                                    clean.push_attribute((
                                        String::from_utf8_lossy(key.as_ref()).into_owned().as_str(),
                                        uri.as_ref(),
                                    ));
                                }
                                continue;
                            }
                            let (ns, local) = reader.resolver().resolve_attribute(key);
                            // Foreign-namespace attributes are metadata
                            // (sodipodi:docname, inkscape:export-filename, ...).
                            if let quick_xml::name::ResolveResult::Bound(n) = &ns {
                                if !SVG_KEEP_NAMESPACES.contains(&n.as_ref()) {
                                    continue;
                                }
                            }
                            // Event handlers never survive (defense in depth
                            // under the CSP jail).
                            if local.as_ref().to_ascii_lowercase().starts_with(b"on") {
                                continue;
                            }
                            let value = attr.decode_and_unescape_value(reader.decoder()).ok()?;
                            // href / xlink:href: refuse script URLs; clean
                            // embedded raster data URIs through the image strip.
                            let value = if local.as_ref() == b"href" {
                                match clean_svg_href(&value) {
                                    SvgHref::Keep => value.into_owned(),
                                    SvgHref::Replace(v) => v,
                                    SvgHref::Drop => continue,
                                    SvgHref::WithholdFile => {
                                        tracing::warn!(
                                            "SVG withheld: embedded data: image cannot be verify-cleaned"
                                        );
                                        return None;
                                    }
                                }
                            } else {
                                value.into_owned()
                            };
                            clean.push_attribute((
                                String::from_utf8_lossy(key.as_ref()).into_owned().as_str(),
                                value.as_str(),
                            ));
                        }
                        if matches!(event, Event::Start(_)) {
                            Event::Start(clean)
                        } else {
                            Event::Empty(clean)
                        }
                    }
                    Event::End(_) => {
                        if skip_depth > 0 {
                            skip_depth -= 1;
                            continue;
                        }
                        event
                    }
                    _ => {
                        if skip_depth > 0 {
                            continue;
                        }
                        event
                    }
                }
            }
            Err(e) => {
                tracing::warn!("SVG withheld: parse failed ({e})");
                return None;
            }
        };
        writer.write_event(event).ok()?;
    }

    let out = writer.into_inner();
    if !svg_is_clean(&out) {
        tracing::error!("SVG withheld: strip verification failed — output still carries metadata");
        return None;
    }
    Some(out)
}

/// Whether an element (with its subtree) is dropped: foreign-namespace elements
/// (RDF, Dublin Core, sodipodi, inkscape) and the SVG-namespace drop list.
fn svg_element_dropped(ns: &quick_xml::name::ResolveResult, local: &[u8]) -> bool {
    if let quick_xml::name::ResolveResult::Bound(n) = ns {
        if !SVG_KEEP_NAMESPACES.contains(&n.as_ref()) {
            return true;
        }
    }
    SVG_DROP_LOCAL.contains(&local)
}

enum SvgHref {
    /// Keep the value as-is (relative path, http(s) — the CSP jail governs it).
    Keep,
    /// Replace with this cleaned value (a data: raster, re-encoded stripped).
    Replace(String),
    /// Drop the attribute (script URL).
    Drop,
    /// The embedded payload cannot be verify-cleaned: withhold the whole file.
    WithholdFile,
}

/// Decide what happens to an `href`/`xlink:href` value. A base64 `data:` raster
/// is decoded, run through the raster [`strip`], and re-embedded; any data: URI
/// we cannot verify-clean (non-base64, a format without a pure-Rust strip, or a
/// failing strip) withholds the whole SVG — fail closed, an embedded HEIC/GIF
/// is rare but its GPS is as real as anyone's.
fn clean_svg_href(value: &str) -> SvgHref {
    use base64::Engine;
    let lower = value.trim_start().to_ascii_lowercase();
    if lower.starts_with("javascript:") || lower.starts_with("vbscript:") {
        return SvgHref::Drop;
    }
    if !lower.starts_with("data:") {
        return SvgHref::Keep;
    }
    // data:<mediatype>;base64,<payload> — only base64 rasters we can strip.
    let (kind, ext) = if lower.starts_with("data:image/jpeg;base64,") {
        ("data:image/jpeg;base64,", "jpg")
    } else if lower.starts_with("data:image/png;base64,") {
        ("data:image/png;base64,", "png")
    } else if lower.starts_with("data:image/webp;base64,") {
        ("data:image/webp;base64,", "webp")
    } else {
        return SvgHref::WithholdFile;
    };
    let payload = &value[kind.len()..];
    let decoded = match base64::engine::general_purpose::STANDARD.decode(payload.trim()) {
        Ok(d) => d,
        Err(_) => return SvgHref::WithholdFile,
    };
    match strip(ext, &decoded) {
        StripOutcome::Clean { bytes, .. } => SvgHref::Replace(format!(
            "{kind}{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )),
        _ => SvgHref::WithholdFile,
    }
}

/// Post-strip verification: parse the stripped output fresh and assert nothing
/// the strip removes is still present — no DOCTYPE/comments/PIs, no foreign-
/// namespace or dropped elements, no foreign-namespace or `on*` attributes, no
/// script URLs, and every remaining `data:` href both decodes and carries no
/// sensitive metadata (checked with kamadak-exif, an independent parser from
/// the one that cleaned it). Unparseable output is not clean.
fn svg_is_clean(bytes: &[u8]) -> bool {
    use quick_xml::events::Event;
    let mut reader = quick_xml::NsReader::from_reader(bytes);
    loop {
        match reader.read_resolved_event() {
            Ok((_, Event::Eof)) => return true,
            Ok((_, Event::DocType(_) | Event::Comment(_) | Event::PI(_))) => return false,
            Ok((resolved, Event::Start(e) | Event::Empty(e))) => {
                if svg_element_dropped(&resolved, e.local_name().as_ref()) {
                    return false;
                }
                for attr in e.attributes() {
                    let attr = match attr {
                        Ok(a) => a,
                        Err(_) => return false,
                    };
                    if attr.key.as_namespace_binding().is_some() {
                        // Only bindings to allowed namespaces may remain.
                        match attr.decode_and_unescape_value(reader.decoder()) {
                            Ok(uri) if SVG_KEEP_NAMESPACES.contains(&uri.as_bytes()) => continue,
                            _ => return false,
                        }
                    }
                    let (ns, local) = reader.resolver().resolve_attribute(attr.key);
                    if let quick_xml::name::ResolveResult::Bound(n) = &ns {
                        if !SVG_KEEP_NAMESPACES.contains(&n.as_ref()) {
                            return false;
                        }
                    }
                    if local.as_ref().to_ascii_lowercase().starts_with(b"on") {
                        return false;
                    }
                    if local.as_ref() == b"href" {
                        let value = match attr.decode_and_unescape_value(reader.decoder()) {
                            Ok(v) => v,
                            Err(_) => return false,
                        };
                        if !svg_href_is_clean(&value) {
                            return false;
                        }
                    }
                }
            }
            Ok(_) => {}
            Err(_) => return false,
        }
    }
}

/// The verifier's independent judgment of one href value.
fn svg_href_is_clean(value: &str) -> bool {
    use base64::Engine;
    let lower = value.trim_start().to_ascii_lowercase();
    if lower.starts_with("javascript:") || lower.starts_with("vbscript:") {
        return false;
    }
    if !lower.starts_with("data:") {
        return true;
    }
    let Some(comma) = value.find(',') else { return false };
    if !lower[..comma + 1].ends_with(";base64,") {
        return false;
    }
    match base64::engine::general_purpose::STANDARD.decode(value[comma + 1..].trim()) {
        Ok(decoded) => !has_sensitive_metadata(&decoded),
        Err(_) => false,
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

// --- transcode sandbox policy -------------------------------------------------

/// How the libvips transcode subprocess is confined. Decided once at startup
/// ([`init_transcode`]) and read on every transcode.
#[derive(Debug, PartialEq)]
pub enum TranscodeMode {
    /// macOS: `sandbox-exec` with a deny-default Seatbelt profile — vips may
    /// read the Nix store and system volume, read/write its per-job scratch
    /// directory, and nothing else (no network, no content tree).
    Seatbelt,
    /// Linux: `bwrap` (bubblewrap) with fully unshared namespaces — read-only
    /// system binds, the scratch directory as the only writable path, no
    /// network.
    Bwrap,
    /// `--unsandboxed-transcode`: the operator explicitly accepted running
    /// libvips on untrusted images without OS confinement.
    Unsandboxed,
    /// No sandbox tooling and no override: transcodes are refused and the
    /// formats that need one are withheld. This is the fail-closed default —
    /// also what any caller that never ran [`init_transcode`] gets.
    Disabled,
}

static TRANSCODE_MODE: OnceLock<TranscodeMode> = OnceLock::new();

/// Where macOS ships the Seatbelt profile interpreter (part of the OS).
const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

/// Resolve and record how transcodes are confined, logging the decision loudly.
/// Called once at server startup; before (or without) it, the mode reads as
/// [`TranscodeMode::Disabled`] — fail closed.
pub fn init_transcode(allow_unsandboxed: bool) {
    let mode = detect_transcode_mode(allow_unsandboxed);
    match mode {
        TranscodeMode::Seatbelt => {
            tracing::info!("image transcodes run sandboxed: macOS Seatbelt (sandbox-exec)")
        }
        TranscodeMode::Bwrap => {
            tracing::info!("image transcodes run sandboxed: bubblewrap (bwrap)")
        }
        TranscodeMode::Unsandboxed => tracing::warn!(
            "SECURITY: --unsandboxed-transcode is set — libvips will decode untrusted \
             images WITHOUT an OS sandbox; a decoder exploit could read this machine, \
             including the private content tree"
        ),
        TranscodeMode::Disabled => tracing::warn!(
            "SECURITY NOTICE: no OS sandbox for image transcodes ({}). Formats that \
             need one (HEIC/TIFF/GIF/...) will be WITHHELD (HTTP 415). Install the \
             tool, or accept the risk with --unsandboxed-transcode",
            if cfg!(target_os = "macos") {
                "sandbox-exec not found"
            } else if cfg!(target_os = "linux") {
                "bwrap not on PATH"
            } else {
                "unsupported platform"
            },
        ),
    }
    let _ = TRANSCODE_MODE.set(mode);
}

fn transcode_mode() -> &'static TranscodeMode {
    TRANSCODE_MODE.get().unwrap_or(&TranscodeMode::Disabled)
}

/// Detect the platform sandbox: macOS ships `sandbox-exec` at a fixed path,
/// Linux needs `bwrap` on PATH, anything else has no supported sandbox.
fn detect_transcode_mode(allow_unsandboxed: bool) -> TranscodeMode {
    let sandbox = if cfg!(target_os = "macos") {
        Path::new(SANDBOX_EXEC)
            .is_file()
            .then_some(TranscodeMode::Seatbelt)
    } else if cfg!(target_os = "linux") {
        find_on_path("bwrap").map(|_| TranscodeMode::Bwrap)
    } else {
        None
    };
    resolve_transcode_mode(sandbox, allow_unsandboxed)
}

/// The policy itself: an available sandbox is always used — the override flag
/// cannot turn one *off* — and with none, the explicit override is the only way
/// a transcode runs at all.
fn resolve_transcode_mode(
    sandbox: Option<TranscodeMode>,
    allow_unsandboxed: bool,
) -> TranscodeMode {
    match (sandbox, allow_unsandboxed) {
        (Some(mode), _) => mode,
        (None, true) => TranscodeMode::Unsandboxed,
        (None, false) => TranscodeMode::Disabled,
    }
}

/// First executable called `name` on PATH, as an absolute path — resolved on
/// the host so a sandboxed invocation never depends on PATH lookup inside the
/// sandbox (bwrap runs with a cleared environment).
fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|c| is_executable_file(c))
}

#[cfg(unix)]
fn is_executable_file(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(p: &Path) -> bool {
    p.is_file()
}

/// The vips CLI invocation shared by every mode: sniff the real format from the
/// content of `in.<ext>`, resize down to `max_edge`, write a quality-85 JPEG.
fn vips_cli_args(tin: &Path, tout: &Path, max_edge: &str) -> Vec<OsString> {
    vec![
        "thumbnail".into(),
        tin.as_os_str().to_owned(),
        format!("{}[Q=85]", tout.display()).into(),
        max_edge.into(),
        "--size".into(),
        "down".into(),
    ]
}

/// Deny-default Seatbelt profile for one transcode. The `dyld-support.sb`
/// import (shipped with macOS; the process-bootstrap rules Apple keeps in sync
/// with dyld — without it a `(deny default)` child aborts inside dyld before
/// `main`) is what lets the process start at all. Beyond that: read the Nix
/// store and the system volume, read/write the per-job scratch directory, and
/// nothing else — no network operation is allowed anywhere in the profile.
/// Live-verified (2026-07-15, macOS 26.5): HEIC transcodes; reading a file
/// outside the scratch, writing outside the scratch, and HTTPS egress are all
/// denied.
fn seatbelt_profile(scratch: &Path) -> String {
    format!(
        r#"(version 1)
(deny default)
(import "dyld-support.sb")
(allow process-fork)
(allow process-exec (subpath "/nix/store"))
(allow file-read* (subpath "/nix/store"))
(allow file-read* (subpath "/System"))
(allow file-read* (subpath "/dev"))
(allow file-write-data (literal "/dev/null"))
(allow file-read* file-write* (subpath "{scratch}"))
(allow sysctl-read)
"#,
        scratch = seatbelt_quote(scratch)
    )
}

/// Escape a path for embedding in a Seatbelt string literal, so a hostile or
/// merely unusual cache path cannot terminate the quoted string and inject
/// profile rules.
fn seatbelt_quote(p: &Path) -> String {
    p.display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

/// Assemble the macOS invocation: `sandbox-exec -p <profile> vips ...` with
/// host paths (Seatbelt filters syscalls; the filesystem view is unchanged).
fn seatbelt_invocation(
    vips: &Path,
    scratch: &Path,
    tin: &Path,
    tout: &Path,
    max_edge: &str,
) -> (PathBuf, Vec<OsString>) {
    let mut args: Vec<OsString> = vec![
        "-p".into(),
        seatbelt_profile(scratch).into(),
        vips.as_os_str().to_owned(),
    ];
    args.extend(vips_cli_args(tin, tout, max_edge));
    (PathBuf::from(SANDBOX_EXEC), args)
}

/// Assemble the Linux invocation: `bwrap` with every namespace unshared (which
/// removes the network), a read-only view of the system paths a distro may
/// have, and the scratch directory mounted at `/scratch` as the only writable
/// mount — vips reads its input and writes its output there and can touch
/// nothing else. Unit-tested here; live verification requires a Linux host.
fn bwrap_invocation(
    bwrap: &Path,
    vips: &Path,
    scratch: &Path,
    ext: &str,
    max_edge: &str,
) -> (PathBuf, Vec<OsString>) {
    let mut args: Vec<OsString> = Vec::new();
    for flag in ["--unshare-all", "--die-with-parent", "--new-session", "--clearenv"] {
        args.push(flag.into());
    }
    // vips's env must be re-set inside (not inherited): --clearenv wipes the
    // parent environment.
    for (key, value) in [("VIPS_CONCURRENCY", "1"), ("VIPS_BLOCK_UNTRUSTED", "1")] {
        args.push("--setenv".into());
        args.push(key.into());
        args.push(value.into());
    }
    // Read-only system binds; --ro-bind-try skips paths this distro lacks.
    for dir in ["/nix/store", "/usr", "/lib", "/lib64", "/bin", "/etc/ld.so.cache"] {
        args.push("--ro-bind-try".into());
        args.push(dir.into());
        args.push(dir.into());
    }
    args.push("--bind".into());
    args.push(scratch.as_os_str().to_owned());
    args.push("/scratch".into());
    for (flag, value) in [("--proc", "/proc"), ("--dev", "/dev"), ("--chdir", "/scratch")] {
        args.push(flag.into());
        args.push(value.into());
    }
    args.push("--".into());
    args.push(vips.as_os_str().to_owned());
    args.extend(vips_cli_args(
        Path::new(&format!("/scratch/in.{ext}")),
        Path::new("/scratch/out.jpg"),
        max_edge,
    ));
    (bwrap.to_owned(), args)
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

/// Image bytes that passed the independent output verification — the ONLY form
/// the serving layer accepts. The fields are private and nothing but
/// [`verify_bytes`] constructs one, so no serving path, present or future, can
/// emit image bytes that skipped the verifier: the compiler enforces the gate.
pub struct CleanBytes {
    bytes: Vec<u8>,
    content_type: &'static str,
}

impl CleanBytes {
    pub fn content_type(&self) -> &'static str {
        self.content_type
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// The prepared bytes to serve for an image request.
pub enum Prepared {
    /// Verified-clean bytes (stripped or transcoded), read back out of the
    /// clean store: serve them.
    Ready(CleanBytes),
    /// Fail closed: the image could not be cleaned (corrupt, transcode failed
    /// or timed out, or a cleaned output failed verification). The caller
    /// withholds the bytes.
    Withheld,
}

/// Wrap a pipeline result for the serving layer.
fn ready(clean: Option<CleanBytes>) -> Prepared {
    match clean {
        Some(c) => Prepared::Ready(c),
        None => Prepared::Withheld,
    }
}

/// Version tag for the segment-strip pipeline's clean-store entries; bump when
/// the strip's behavior changes so stale outputs regenerate.
const STRIP_TAG: &str = "s1";

/// The served MIME for a segment-strippable extension — also the clean-store
/// filename extension via [`store_ext`]. `None` for formats only the transcode
/// path can clean.
fn strip_content_type(norm_ext: &str) -> Option<&'static str> {
    match norm_ext {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

/// Clean an image for serving. Every producer funnels through one disk
/// pipeline: cleaned bytes (pure-Rust segment strip, or sandboxed libvips
/// transcode) land in a quarantine file, THAT FILE is re-read and verified by
/// independent parsers, and only a verified file is promoted — atomic rename —
/// into the clean store, `<cache_dir>/media/clean/`, the sole byte source the
/// serving layer reads (re-verifying on every read). `original` files never
/// reach here — the caller serves their exact bytes directly.
pub async fn prepare(ext: &str, source: &[u8], cache_dir: &Path) -> Prepared {
    let norm = crate::entry::normalize_ext(ext);
    let key = format!("{}-{STRIP_TAG}", content_hash(source));
    // The strip is deterministic (§8), so its output is clean-store-cached
    // exactly like a transcode's — a hit skips the strip entirely.
    if let Some(content_type) = strip_content_type(&norm) {
        if let Some(clean) = read_clean_store(cache_dir, &key, content_type).await {
            return Prepared::Ready(clean);
        }
    }
    match strip(ext, source) {
        StripOutcome::Clean { bytes: cleaned, content_type } => {
            ready(promote_and_read(&cleaned, content_type, cache_dir, &key).await)
        }
        StripOutcome::Failed => Prepared::Withheld,
        StripOutcome::NeedsTranscode => ready(
            vips_clean_jpeg(ext, source, cache_dir, TRANSCODE_MAX_EDGE, TRANSCODE_TAG).await,
        ),
    }
}

/// The independent verifier — the only mint for [`CleanBytes`]. Re-checks a
/// cleaned output with parsers that did not produce it: kamadak-exif must find
/// no sensitive metadata, and the bytes must still parse as the image container
/// the content type claims (a torn write or corrupted store entry is not
/// servable merely because it carries no EXIF). Any failure is a loud None.
fn verify_bytes(bytes: Vec<u8>, content_type: &'static str) -> Option<CleanBytes> {
    if has_sensitive_metadata(&bytes) {
        tracing::error!("image withheld: cleaned output still carries metadata");
        return None;
    }
    let container = match imagesize::image_type(&bytes) {
        Ok(t) => t,
        Err(_) => {
            tracing::error!("image withheld: cleaned output no longer parses as an image");
            return None;
        }
    };
    let container_matches = matches!(
        (container, content_type),
        (imagesize::ImageType::Jpeg, "image/jpeg")
            | (imagesize::ImageType::Png, "image/png")
            | (imagesize::ImageType::Webp, "image/webp")
    );
    if !container_matches {
        tracing::error!("image withheld: cleaned output is not the {content_type} it claims");
        return None;
    }
    Some(CleanBytes { bytes, content_type })
}

/// Clean-store filename extension for a served content type. Only the types
/// [`verify_bytes`] admits ever reach the store.
fn store_ext(content_type: &str) -> &'static str {
    match content_type {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        _ => "bin",
    }
}

fn clean_store_path(cache_dir: &Path, key: &str, content_type: &str) -> PathBuf {
    cache_dir
        .join("media")
        .join("clean")
        .join(format!("{key}.{}", store_ext(content_type)))
}

/// Read one entry back from the clean store, re-verifying on EVERY read: a
/// corrupted or tampered store file is withheld — and deleted, so the next
/// request regenerates it from source — never served.
async fn read_clean_store(
    cache_dir: &Path,
    key: &str,
    content_type: &'static str,
) -> Option<CleanBytes> {
    let path = clean_store_path(cache_dir, key, content_type);
    let bytes = tokio::fs::read(&path).await.ok()?;
    match verify_bytes(bytes, content_type) {
        Some(clean) => Some(clean),
        None => {
            tracing::error!(
                "clean-store entry {} failed re-verification — removed",
                path.display()
            );
            let _ = tokio::fs::remove_file(&path).await;
            None
        }
    }
}

/// The promotion gate between a cleaner's output and anything servable: write
/// the cleaned bytes to a quarantine file, re-read THAT FILE (what is actually
/// on disk, not the buffer the cleaner handed over — a torn write fails here),
/// verify the re-read, and only on a pass rename it atomically into the clean
/// store. The served bytes then come from a verifying store read like every
/// other request — nothing is served from the pre-promotion buffer, so there is
/// no window between what was verified and what is served. Any failure deletes
/// the quarantine file and withholds; a killed process strands only a
/// dot-prefixed file that [`sweep_cache`] clears at startup.
async fn promote_and_read(
    cleaned: &[u8],
    content_type: &'static str,
    cache_dir: &Path,
    key: &str,
) -> Option<CleanBytes> {
    let media = cache_dir.join("media");
    if tokio::fs::create_dir_all(media.join("clean")).await.is_err() {
        return None;
    }
    let uniq = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let quarantine = media.join(format!(".q.{key}.{uniq}"));
    if tokio::fs::write(&quarantine, cleaned).await.is_err() {
        let _ = tokio::fs::remove_file(&quarantine).await;
        return None;
    }
    let verified = match tokio::fs::read(&quarantine).await {
        Ok(reread) => verify_bytes(reread, content_type).is_some(),
        Err(_) => false,
    };
    if !verified {
        tracing::error!("image withheld: quarantined output failed file verification ({key})");
        let _ = tokio::fs::remove_file(&quarantine).await;
        return None;
    }
    let dest = clean_store_path(cache_dir, key, content_type);
    if tokio::fs::rename(&quarantine, &dest).await.is_err() {
        tracing::error!("image withheld: could not promote {key} into the clean store");
        let _ = tokio::fs::remove_file(&quarantine).await;
        return None;
    }
    read_clean_store(cache_dir, key, content_type).await
}

/// Produce a small, clean JPEG thumbnail for a gallery tile. Unlike [`prepare`],
/// this always goes through libvips (every format, including JPEG/PNG, needs the
/// resize), and it ignores the `original` opt-in — a tile is a derived preview,
/// so it is stripped even when the full asset it links to is served exact. Cached
/// separately from the full view via [`THUMB_TAG`].
pub async fn thumbnail(ext: &str, bytes: &[u8], cache_dir: &Path) -> Prepared {
    ready(vips_clean_jpeg(ext, bytes, cache_dir, THUMB_MAX_EDGE, THUMB_TAG).await)
}

/// Decode any raster with libvips and re-encode a clean JPEG capped at
/// `max_edge`, keeping the source ICC and baking in orientation; the JPEG still
/// carries the source EXIF/GPS, so it is run back through the segment strip to
/// drop the metadata while keeping the color profile libvips preserved. The
/// result goes through the same quarantine → verify → promote gate as every
/// cleaner ([`promote_and_read`]) into the clean store keyed by source hash +
/// `tag`, so the subprocess runs at most once per (image, size). Returns the
/// store-read verified bytes, or `None` on any failure (fail closed).
async fn vips_clean_jpeg(
    ext: &str,
    bytes: &[u8],
    cache_dir: &Path,
    max_edge: &str,
    tag: &str,
) -> Option<CleanBytes> {
    // Decompression-bomb gate: read the declared dimensions from the header
    // (pure Rust, no decode) and refuse anything over the cap — or whose
    // dimensions cannot be read at all — before libvips ever sees the bytes.
    if !decode_size_allowed(bytes) {
        return None;
    }

    let media = cache_dir.join("media");
    tokio::fs::create_dir_all(&media).await.ok()?;

    let key = format!("{}-{}", content_hash(bytes), tag);
    if let Some(clean) = read_clean_store(cache_dir, &key, "image/jpeg").await {
        return Some(clean);
    }

    // Serialize heavy work behind the limiter, then re-check the store: a
    // concurrent request for the same image may have promoted it while we waited.
    let _permit = VIPS_SEMAPHORE.acquire().await.ok()?;
    if let Some(clean) = read_clean_store(cache_dir, &key, "image/jpeg").await {
        return Some(clean);
    }

    // Per-job scratch directory: the ONE place the sandboxed subprocess may
    // write (and, with the input placed inside it, the one file tree it reads
    // beyond the system). Dot-prefixed so the startup sweep ([`sweep_cache`])
    // clears anything a killed process stranded.
    let uniq = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let scratch = media.join(format!(".tx.{key}.{uniq}"));
    tokio::fs::create_dir_all(&scratch).await.ok()?;
    let norm = crate::entry::normalize_ext(ext);

    let ran = run_vips_transcode(bytes, &scratch, &norm, max_edge).await;
    let raw = if ran {
        tokio::fs::read(scratch.join("out.jpg")).await.ok()
    } else {
        None
    };
    // The whole scratch — untrusted full-metadata input included — goes away
    // before anything else happens.
    let _ = tokio::fs::remove_dir_all(&scratch).await;
    let raw = raw?;

    // libvips carried the source EXIF/GPS into the JPEG (confirmed) — strip it,
    // keeping the ICC/orientation the transcode preserved.
    let cleaned = match strip_jpeg(&raw) {
        StripOutcome::Clean { bytes, .. } => bytes,
        _ => return None,
    };

    // Quarantine → verify-the-file → promote; serve from the verified store.
    promote_and_read(&cleaned, "image/jpeg", cache_dir, &key).await
}

/// Run one libvips transcode as an OS-sandboxed subprocess. Untrusted bytes go
/// to `in.<ext>` inside the per-job `scratch` directory (libvips sniffs the real
/// format from content, not the extension); the output is `out.jpg`, capped at
/// `max_edge`. The sandbox (Seatbelt / bwrap, per [`init_transcode`]) confines
/// the decoder to that directory: read-only system paths, no network, no view of
/// the content tree — a decoder exploit on a hostile image reads pixels, not
/// files. Also: a single internal thread, the ImageMagick/untrusted loaders
/// blocked, a wall-clock timeout with kill-on-drop, and — on Unix — an
/// RLIMIT_CPU backstop (inherited through the sandbox wrapper into vips).
/// With no sandbox and no explicit override, refuses to run at all (fail
/// closed). Returns whether it succeeded.
async fn run_vips_transcode(bytes: &[u8], scratch: &Path, ext: &str, max_edge: &str) -> bool {
    let mode = transcode_mode();
    if matches!(mode, TranscodeMode::Disabled) {
        tracing::warn!(
            "transcode refused: no OS sandbox and no --unsandboxed-transcode \
             (see the startup notice); withholding"
        );
        return false;
    }

    let tin = scratch.join(format!("in.{ext}"));
    let tout = scratch.join("out.jpg");
    if tokio::fs::write(&tin, bytes).await.is_err() {
        return false;
    }

    let vips = find_on_path("vips").unwrap_or_else(|| PathBuf::from("vips"));
    let (program, args) = match mode {
        TranscodeMode::Seatbelt => seatbelt_invocation(&vips, scratch, &tin, &tout, max_edge),
        TranscodeMode::Bwrap => {
            let bwrap = find_on_path("bwrap").unwrap_or_else(|| PathBuf::from("bwrap"));
            bwrap_invocation(&bwrap, &vips, scratch, ext, max_edge)
        }
        TranscodeMode::Unsandboxed => (vips.clone(), vips_cli_args(&tin, &tout, max_edge)),
        TranscodeMode::Disabled => unreachable!("refused above"),
    };

    let mut std_cmd = std::process::Command::new(&program);
    std_cmd
        .args(&args)
        // One worker thread: predictable memory, no thread-count amplification.
        // (The bwrap invocation re-sets these two inside via --setenv, since it
        // clears the inherited environment; Seatbelt/unsandboxed inherit them.)
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
/// somehow missed. (rlimits survive exec and are inherited, so the cap set on
/// the sandbox wrapper reaches vips itself.) RLIMIT_AS is deliberately not set —
/// virtual-address limits are blunt and break legitimate large decodes; memory
/// blowup is bounded instead by the pre-decode megapixel cap
/// ([`MAX_DECODE_PIXELS`]) and confined by the OS sandbox around the subprocess.
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

/// Pixel ceiling for anything handed to the decoder — ~100 megapixels, well
/// above real photography (an iPhone panorama is ~63 MP) and well below where
/// a decode's memory hurts. A decompression bomb declares enormous dimensions
/// in a tiny file; the header is parsed in pure Rust (`imagesize`, no decode),
/// and anything over the cap — or whose dimensions cannot be read at all — is
/// withheld before libvips ever runs.
const MAX_DECODE_PIXELS: u64 = 100_000_000;

/// Whether the declared dimensions are under [`MAX_DECODE_PIXELS`]. Unreadable
/// dimensions fail closed: a container so mangled that a header parse cannot
/// size it is not something to hand a decoder.
fn decode_size_allowed(bytes: &[u8]) -> bool {
    match imagesize::blob_size(bytes) {
        Ok(dim) => {
            let px = (dim.width as u64).saturating_mul(dim.height as u64);
            if px > MAX_DECODE_PIXELS {
                tracing::warn!(
                    "image withheld: {}x{} exceeds the {} MP pre-decode cap",
                    dim.width,
                    dim.height,
                    MAX_DECODE_PIXELS / 1_000_000
                );
                return false;
            }
            true
        }
        Err(e) => {
            tracing::warn!("image withheld: could not read dimensions before decode ({e})");
            false
        }
    }
}

/// Startup hygiene: clear transcode leftovers from `<cache>/media`. Scratch
/// directories (`.tx.*`) and in-flight cache publishes (`.*.wip.jpg`) are
/// dot-prefixed exactly so that a `kill -9` mid-transcode strands nothing a
/// later run cannot recognize; everything dot-prefixed in this directory is
/// disposable by construction. The stranded *inputs* are the point of the
/// sweep — they hold untrusted, full-metadata bytes (not HTTP-reachable, but
/// no reason to keep them on disk).
pub fn sweep_cache(cache_dir: &Path) {
    let media = cache_dir.join("media");
    let entries = match std::fs::read_dir(&media) {
        Ok(e) => e,
        Err(_) => return, // no media cache yet — nothing to sweep
    };
    for entry in entries.flatten() {
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        // The verified clean store is the one thing that persists; everything
        // else under media/ is in-flight (dot-prefixed quarantine files and
        // transcode scratch dirs — possibly holding untrusted full-metadata
        // input a killed process stranded) or a legacy flat cache file nothing
        // reads anymore.
        if is_dir && entry.file_name().to_string_lossy() == "clean" {
            continue;
        }
        let path = entry.path();
        let removed = if is_dir {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        match removed {
            Ok(()) => tracing::info!("swept stale media temp {}", path.display()),
            Err(e) => tracing::warn!("could not sweep media temp {}: {e}", path.display()),
        }
    }
}

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

    /// A real, decodable 1x1 baseline JPEG (imagemagick-generated, metadata-free).
    /// Unlike [`tiny_jpeg`] — whose header-only SOS img-parts mangles on a
    /// round-trip — this survives the strip intact, so it can exercise the full
    /// strip → verify → promote pipeline.
    fn real_jpeg() -> Vec<u8> {
        vec![
            255, 216, 255, 224, 0, 16, 74, 70, 73, 70, 0, 1, 1, 0, 0, 1, 0, 1, 0, 0,
            255, 219, 0, 67, 0, 5, 3, 4, 4, 4, 3, 5, 4, 4, 4, 5, 5, 5, 6, 7,
            12, 8, 7, 7, 7, 7, 15, 11, 11, 9, 12, 17, 15, 18, 18, 17, 15, 17, 17, 19,
            22, 28, 23, 19, 20, 26, 21, 17, 17, 24, 33, 24, 26, 29, 29, 31, 31, 31, 19, 23,
            34, 36, 34, 30, 36, 28, 30, 31, 30, 255, 219, 0, 67, 1, 5, 5, 5, 7, 6, 7,
            14, 8, 8, 14, 30, 20, 17, 20, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30,
            30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30,
            30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 255, 192,
            0, 17, 8, 0, 1, 0, 1, 3, 1, 34, 0, 2, 17, 1, 3, 17, 1, 255, 196, 0,
            21, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 6,
            255, 196, 0, 20, 16, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 255, 196, 0, 21, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 7, 8, 255, 196, 0, 20, 17, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 255, 218, 0, 12, 3, 1, 0, 2, 17, 3, 17, 0, 63,
            0, 141, 1, 67, 18, 159, 255, 217,
        ]
    }

    /// [`real_jpeg`] with an APP1 EXIF segment (Make tag) spliced in after SOI —
    /// the class of metadata the pipeline must never let through.
    fn jpeg_with_make() -> Vec<u8> {
        let tiff = [
            b'I', b'I', 0x2A, 0x00, // little-endian TIFF magic
            0x08, 0, 0, 0, // IFD0 at offset 8
            0x01, 0x00, // one entry
            0x0F, 0x01, // tag 0x010F (Make)
            0x02, 0x00, // type ASCII
            0x04, 0, 0, 0, // count 4 (fits inline)
            b'C', b'a', b'm', 0, // "Cam\0"
            0, 0, 0, 0, // no next IFD
        ];
        let mut app1 = b"Exif\0\0".to_vec();
        app1.extend_from_slice(&tiff);
        let mut segment = vec![0xFF, 0xE1];
        segment.extend_from_slice(&((app1.len() + 2) as u16).to_be_bytes());
        segment.extend_from_slice(&app1);
        let mut jpeg = real_jpeg();
        jpeg.splice(2..2, segment);
        assert!(has_sensitive_metadata(&jpeg), "fixture must carry EXIF Make");
        jpeg
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
        // Author-readable UTF-8 text is served raw.
        assert!(matches!(classify("txt", b"just some text here"), Disposition::Raw));
        assert!(matches!(classify("md", "unicode är fine ✓".as_bytes()), Disposition::Raw));
        assert!(matches!(classify("", b"README body"), Disposition::Raw));

        // PDF and SVG hide metadata behind the rendered view — never through the
        // text gate even as pure ASCII. PDF routes to its strip (also under a
        // lying extension, by magic); SVG is withheld until its strip lands.
        assert!(matches!(classify("pdf", b"%PDF-1.7 ....."), Disposition::Pdf));
        assert!(matches!(classify("txt", b"%PDF-1.4 disguised"), Disposition::Pdf));
        assert!(matches!(classify("svg", b"<svg xmlns='http://www.w3.org/2000/svg'/>"), Disposition::Svg));

        // The default is fail-closed: unknown binary formats (Office, archives,
        // fonts, anything nobody listed) are withheld, never served raw.
        assert!(matches!(classify("zip", b"PK\x03\x04\x14\x00\x00\x00\x00\x00etc"), Disposition::Withhold));
        assert!(matches!(classify("docx", b"PK\x03\x04\x14\x00\x00\x00\x08\x00etc"), Disposition::Withhold));
        assert!(matches!(classify("bin", &[0u8, 159, 146, 150, 7, 8, 9, 250, 251, 252, 253, 254]), Disposition::Withhold));
        // Non-UTF-8 text encodings cannot be cheaply proven author-readable.
        let utf16 = [0xFF, 0xFE, b'h', 0, b'i', 0, b' ', 0, b't', 0, b'x', 0, b't', 0];
        assert!(matches!(classify("txt", &utf16), Disposition::Withhold));
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

    /// A minimal one-page PDF carrying every metadata channel we strip: an
    /// `/Info` dictionary, a catalog-level XMP `/Metadata` stream, and a
    /// `/PieceInfo` on the page.
    fn pdf_with_metadata() -> Vec<u8> {
        use lopdf::{dictionary, Object, Stream};
        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let piece = dictionary! {"SecretApp" => dictionary! {"Private" => Object::string_literal("SecretPiece")}};
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => Object::Reference(pages_id), "PieceInfo" => piece,
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![Object::Reference(page_id)], "Count" => 1,
            }),
        );
        let xmp = Stream::new(
            dictionary! {"Type" => "Metadata", "Subtype" => "XML"},
            b"<x:xmpmeta><dc:creator>SecretCreator</dc:creator></x:xmpmeta>".to_vec(),
        );
        let meta_id = doc.add_object(xmp);
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog", "Pages" => Object::Reference(pages_id),
            "Metadata" => Object::Reference(meta_id),
        });
        doc.trailer.set("Root", Object::Reference(catalog_id));
        let info_id = doc.add_object(dictionary! {
            "Author" => Object::string_literal("Secret Author"),
            "Producer" => Object::string_literal("SecretTool 1.0"),
        });
        doc.trailer.set("Info", Object::Reference(info_id));
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    #[test]
    fn strip_pdf_removes_every_metadata_channel_deterministically() {
        let contains = |hay: &[u8], needle: &[u8]| hay.windows(needle.len()).any(|w| w == needle);
        let bytes = pdf_with_metadata();
        for needle in [b"Secret Author".as_slice(), b"SecretCreator", b"SecretTool", b"SecretPiece"] {
            assert!(contains(&bytes, needle), "fixture must carry the metadata");
        }
        let clean = strip_pdf(&bytes).expect("valid PDF strips");
        for needle in [b"Secret Author".as_slice(), b"SecretCreator", b"SecretTool", b"SecretPiece"] {
            assert!(!contains(&clean, needle), "{:?} must be gone", String::from_utf8_lossy(needle));
        }
        // Still a valid PDF, and the strip is deterministic (stable ETag).
        assert!(lopdf::Document::load_mem(&clean).is_ok());
        assert_eq!(strip_pdf(&bytes).unwrap(), clean);
    }

    #[test]
    fn strip_pdf_fails_closed() {
        // Unparseable bytes are withheld, never passed through.
        assert!(strip_pdf(b"%PDF-1.4 not really a pdf").is_none());
        // An encrypted PDF is withheld: we cannot see what we would serve.
        use lopdf::{dictionary, Object};
        let mut doc = lopdf::Document::load_mem(&pdf_with_metadata()).unwrap();
        let enc_id = doc.add_object(dictionary! {"Filter" => "Standard"});
        doc.trailer.set("Encrypt", Object::Reference(enc_id));
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        assert!(strip_pdf(&bytes).is_none());
    }

    /// An Inkscape-flavored SVG exercising every leak channel the strip covers:
    /// editor-namespace attributes with filesystem paths, a `<metadata>` RDF
    /// block with the creator, editor-namespace elements, comments, a script,
    /// and an event handler.
    fn inkscape_svg() -> String {
        r##"<?xml version="1.0" encoding="UTF-8"?>
<!-- Made with SecretEditor on Tilde's laptop -->
<svg xmlns="http://www.w3.org/2000/svg"
     xmlns:xlink="http://www.w3.org/1999/xlink"
     xmlns:dc="http://purl.org/dc/elements/1.1/"
     xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
     xmlns:sodipodi="http://sodipodi.sourceforge.net/DTD/sodipodi-0.0.dtd"
     xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape"
     width="100" height="100"
     sodipodi:docname="/Users/tilde/Secret Projects/logo-final.svg"
     inkscape:export-filename="/Users/tilde/Desktop/secret-export.png">
  <sodipodi:namedview inkscape:window-width="1728"/>
  <metadata><rdf:RDF><dc:creator>Tilde Secret</dc:creator></rdf:RDF></metadata>
  <title>A circle</title>
  <script>alert('x')</script>
  <circle cx="50" cy="50" r="40" fill="#a123f6" onclick="alert('y')"/>
</svg>"##
            .to_string()
    }

    #[test]
    fn strip_svg_removes_every_metadata_channel() {
        let src = inkscape_svg();
        let clean = strip_svg(src.as_bytes()).expect("well-formed SVG strips");
        let clean_str = String::from_utf8(clean.clone()).unwrap();
        for leak in [
            "Secret Projects", "secret-export", "Tilde Secret", "SecretEditor",
            "sodipodi", "inkscape", "namedview", "purl.org", "rdf-syntax",
            "<metadata", "<script", "onclick",
        ] {
            assert!(!clean_str.contains(leak), "{leak:?} must be gone:\n{clean_str}");
        }
        // The image itself survives: the circle, its styling, the accessible title.
        for kept in ["<circle", "cx=\"50\"", "fill=\"#a123f6\"", "<title>A circle</title>", "width=\"100\""] {
            assert!(clean_str.contains(kept), "{kept:?} must survive:\n{clean_str}");
        }
        // Deterministic, still well-formed, and verified clean by the checker.
        assert_eq!(strip_svg(src.as_bytes()).unwrap(), clean);
        assert!(svg_is_clean(&clean));
        assert!(!svg_is_clean(src.as_bytes()));
    }

    #[test]
    fn strip_svg_fails_closed() {
        // Unparseable, DOCTYPE-carrying (entity machinery), and non-UTF-8
        // documents are all withheld.
        assert!(strip_svg(b"<svg><unclosed").is_none());
        assert!(strip_svg(b"<?xml version=\"1.0\"?><!DOCTYPE svg [<!ENTITY x \"y\">]><svg/>").is_none());
        assert!(strip_svg(&[0xFF, 0xFE, 0x3C, 0x00]).is_none());
        // An embedded data: URI in a format we cannot pure-Rust-strip (GIF)
        // withholds the whole file rather than passing the payload through.
        let gif = "<svg xmlns=\"http://www.w3.org/2000/svg\"><image href=\"data:image/gif;base64,R0lGODlh\"/></svg>";
        assert!(strip_svg(gif.as_bytes()).is_none());
        // javascript: hrefs are dropped (the file still serves).
        let js = "<svg xmlns=\"http://www.w3.org/2000/svg\"><a href=\"javascript:alert(1)\"><text>x</text></a></svg>";
        let clean = String::from_utf8(strip_svg(js.as_bytes()).unwrap()).unwrap();
        assert!(!clean.contains("javascript"));
    }

    #[test]
    fn strip_svg_cleans_embedded_raster_data_uris() {
        use base64::Engine;
        // Build a JPEG carrying sensitive EXIF (Make), embed it as a data: URI,
        // and check the strip re-embeds a cleaned version of it.
        let mut jpeg = Vec::new();
        jpeg.extend_from_slice(&[0xFF, 0xD8]); // SOI
        let tiff = {
            let mut t = vec![
                b'I', b'I', 0x2A, 0x00, 0x08, 0, 0, 0, 0x01, 0x00,
                0x0F, 0x01, 0x02, 0x00, 0x04, 0, 0, 0, b'C', b'a', b'm', 0,
            ];
            t.extend_from_slice(&[0, 0, 0, 0]);
            t
        };
        let mut app1 = b"Exif\0\0".to_vec();
        app1.extend_from_slice(&tiff);
        let len = (app1.len() + 2) as u16;
        jpeg.extend_from_slice(&[0xFF, 0xE1]);
        jpeg.extend_from_slice(&len.to_be_bytes());
        jpeg.extend_from_slice(&app1);
        jpeg.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x02, 0x11, 0x22, 0xFF, 0xD9]);
        assert!(has_sensitive_metadata(&jpeg), "fixture must carry EXIF Make");

        let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg);
        let svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><image href=\"data:image/jpeg;base64,{b64}\"/></svg>"
        );
        let clean = String::from_utf8(strip_svg(svg.as_bytes()).unwrap()).unwrap();
        let embedded = clean.split("base64,").nth(1).unwrap().split('"').next().unwrap();
        let decoded = base64::engine::general_purpose::STANDARD.decode(embedded).unwrap();
        assert!(!has_sensitive_metadata(&decoded), "embedded raster must be cleaned");
    }

    #[test]
    fn pdf_verification_rejects_unstripped_output() {
        // The independent re-check must flag the fixture as dirty, pass the
        // stripped result, and treat unparseable bytes as NOT clean.
        assert!(!pdf_is_clean(&pdf_with_metadata()));
        assert!(pdf_is_clean(&strip_pdf(&pdf_with_metadata()).unwrap()));
        assert!(!pdf_is_clean(b"not a pdf"));
    }

    #[test]
    fn verification_gate_withholds_metadata_bearing_output() {
        // A minimal TIFF whose one IFD entry is Make ("Cam") — sensitive
        // metadata the strip should never let through. If the strip pipeline
        // ever *did* emit something like this, the verification gate is the
        // last line: it must withhold, not serve.
        let tiff: Vec<u8> = vec![
            b'I', b'I', 0x2A, 0x00, // little-endian TIFF magic
            0x08, 0, 0, 0, // IFD0 at offset 8
            0x01, 0x00, // one entry
            0x0F, 0x01, // tag 0x010F (Make)
            0x02, 0x00, // type ASCII
            0x04, 0, 0, 0, // count 4 (fits inline)
            b'C', b'a', b'm', 0, // "Cam\0"
            0, 0, 0, 0, // no next IFD
        ];
        assert!(has_sensitive_metadata(&tiff));
        assert!(verify_bytes(tiff, "image/tiff").is_none());
        // A clean image passes the gate.
        assert!(verify_bytes(tiny_jpeg(), "image/jpeg").is_some());
        // Clean bytes claiming the wrong container are rejected: a JPEG is not
        // servable as image/png no matter how metadata-free it is.
        assert!(verify_bytes(tiny_jpeg(), "image/png").is_none());
        // Garbage without metadata is still not an image.
        assert!(verify_bytes(b"no container here".to_vec(), "image/jpeg").is_none());
    }

    #[test]
    fn transcode_mode_resolution_is_fail_closed() {
        // No sandbox, no override: Disabled (withhold). The override is the
        // only way to run without one; an available sandbox always wins, even
        // with the override set.
        assert_eq!(resolve_transcode_mode(None, false), TranscodeMode::Disabled);
        assert_eq!(resolve_transcode_mode(None, true), TranscodeMode::Unsandboxed);
        assert_eq!(
            resolve_transcode_mode(Some(TranscodeMode::Seatbelt), true),
            TranscodeMode::Seatbelt
        );
        assert_eq!(
            resolve_transcode_mode(Some(TranscodeMode::Bwrap), false),
            TranscodeMode::Bwrap
        );
    }

    #[tokio::test]
    async fn transcode_refused_when_mode_is_disabled() {
        // TRANSCODE_MODE is never initialized in tests, so it reads Disabled —
        // and a transcode must refuse to run (fail closed), touching nothing.
        let scratch = std::env::temp_dir().join(format!("esko-tx-refuse-{}", std::process::id()));
        std::fs::create_dir_all(&scratch).unwrap();
        assert!(!run_vips_transcode(b"bytes", &scratch, "heic", "4096").await);
        assert!(!scratch.join("in.heic").exists(), "input must not be written");
        std::fs::remove_dir_all(&scratch).unwrap();
    }

    #[test]
    fn seatbelt_profile_is_deny_default_and_scratch_scoped() {
        let p = seatbelt_profile(Path::new("/cache/media/.tx.abc.0"));
        assert!(p.contains("(deny default)"));
        assert!(p.contains("(import \"dyld-support.sb\")"));
        assert!(p.contains("(subpath \"/cache/media/.tx.abc.0\")"));
        // No network operation is allowed anywhere in the profile.
        assert!(!p.contains("network"));
        // A path cannot terminate the string literal and inject profile rules.
        let q = seatbelt_profile(Path::new("/cache/we\")(allow network*)(\""));
        assert!(q.contains(r#"we\")(allow network*)(\""#)); // escaped form present
        assert!(!q.contains("we\")")); // the unescaped quote never survives
    }

    #[test]
    fn bwrap_invocation_is_isolated() {
        let (prog, args) = bwrap_invocation(
            Path::new("/usr/bin/bwrap"),
            Path::new("/nix/store/x/bin/vips"),
            Path::new("/cache/media/.tx.k.1"),
            "heic",
            "4096",
        );
        assert_eq!(prog, Path::new("/usr/bin/bwrap"));
        let a: Vec<String> = args.iter().map(|s| s.to_string_lossy().into_owned()).collect();
        // Namespaces fully unshared (that removes the network), tied to our
        // lifetime, with a cleared environment.
        for flag in ["--unshare-all", "--die-with-parent", "--new-session", "--clearenv"] {
            assert!(a.contains(&flag.to_string()), "{flag} missing");
        }
        assert!(!a.iter().any(|s| s.contains("--share-net")));
        // The scratch is the only writable bind; system binds are read-only.
        assert_eq!(a.iter().filter(|s| *s == "--bind").count(), 1);
        assert!(a.windows(3).any(|w| w == ["--bind", "/cache/media/.tx.k.1", "/scratch"]));
        assert!(a.windows(3).any(|w| w == ["--ro-bind-try", "/nix/store", "/nix/store"]));
        // vips reads and writes only inside /scratch.
        assert!(a.contains(&"/scratch/in.heic".to_string()));
        assert!(a.contains(&"/scratch/out.jpg[Q=85]".to_string()));
        // The env vips relies on is re-set inside the cleared environment.
        assert!(a.windows(3).any(|w| w == ["--setenv", "VIPS_BLOCK_UNTRUSTED", "1"]));
        assert!(a.windows(3).any(|w| w == ["--setenv", "VIPS_CONCURRENCY", "1"]));
    }

    /// A minimal PNG header declaring the given dimensions (imagesize parses
    /// the IHDR only; no pixel data or valid CRC is needed).
    fn png_header(width: u32, height: u32) -> Vec<u8> {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend_from_slice(&13u32.to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&width.to_be_bytes());
        png.extend_from_slice(&height.to_be_bytes());
        png.extend_from_slice(&[8, 2, 0, 0, 0]); // bit depth, color, ...
        png.extend_from_slice(&[0, 0, 0, 0]); // (unchecked) CRC
        png
    }

    #[test]
    fn decode_cap_refuses_bombs_and_unknown_dimensions() {
        // 100000 x 100000 = 10 gigapixels declared in a ~30-byte file: the
        // classic decompression bomb. Refused before any decode.
        assert!(!decode_size_allowed(&png_header(100_000, 100_000)));
        // Dimensions we cannot read at all: fail closed.
        assert!(!decode_size_allowed(b"not an image at all"));
        // A real-world size passes.
        assert!(decode_size_allowed(&png_header(1600, 1200)));
    }

    #[test]
    fn sweep_clears_everything_but_the_clean_store() {
        let root = std::env::temp_dir().join(format!("esko-sweep-test-{}", std::process::id()));
        let media = root.join("media");
        std::fs::create_dir_all(media.join(".tx.deadbeef.3")).unwrap();
        std::fs::write(media.join(".tx.deadbeef.3/in.heic"), b"stranded input").unwrap();
        std::fs::write(media.join(".q.deadbeef-s1.0"), b"stranded quarantine").unwrap();
        std::fs::write(media.join("deadbeef-v1-j4096q85.jpg"), b"legacy flat cache").unwrap();
        std::fs::create_dir_all(media.join("clean")).unwrap();
        std::fs::write(media.join("clean/deadbeef-s1.jpg"), b"verified store entry").unwrap();
        sweep_cache(&root);
        assert!(!media.join(".tx.deadbeef.3").exists(), "scratch dir swept");
        assert!(!media.join(".q.deadbeef-s1.0").exists(), "quarantine file swept");
        assert!(!media.join("deadbeef-v1-j4096q85.jpg").exists(), "legacy flat cache swept");
        assert!(media.join("clean/deadbeef-s1.jpg").exists(), "clean store kept");
        // A missing cache dir is fine (fresh install).
        sweep_cache(Path::new("/nonexistent-esko-cache"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn promotion_gate_populates_store_and_rejects_dirty_output() {
        let root =
            std::env::temp_dir().join(format!("esko-promote-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);

        // A verified-clean output is promoted and read back from the store.
        let clean = promote_and_read(&tiny_jpeg(), "image/jpeg", &root, "aaaa-s1").await;
        let clean = clean.expect("clean output must promote");
        assert_eq!(clean.content_type(), "image/jpeg");
        assert!(
            clean_store_path(&root, "aaaa-s1", "image/jpeg").exists(),
            "promoted file lives in the clean store"
        );
        assert!(
            read_clean_store(&root, "aaaa-s1", "image/jpeg").await.is_some(),
            "store read verifies and returns the entry"
        );

        // A dirty output — pretend a broken strip let EXIF through — is refused:
        // nothing lands in the store, no quarantine file is left behind.
        assert!(
            promote_and_read(&jpeg_with_make(), "image/jpeg", &root, "bbbb-s1").await.is_none(),
            "dirty output must not promote"
        );
        assert!(
            !clean_store_path(&root, "bbbb-s1", "image/jpeg").exists(),
            "nothing in the clean store"
        );
        // Garbage that parses as no image container is refused too.
        assert!(
            promote_and_read(b"not an image", "image/jpeg", &root, "cccc-s1").await.is_none()
        );
        let leftovers: Vec<_> = std::fs::read_dir(root.join("media"))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with('.'))
            .collect();
        assert!(leftovers.is_empty(), "no quarantine files left behind: {leftovers:?}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn poisoned_store_entry_is_withheld_and_removed() {
        let root =
            std::env::temp_dir().join(format!("esko-poison-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        // A file placed in the store without going through the promotion gate
        // (disk corruption, tampering, a version-drift bug) must never serve:
        // the per-read verification withholds it and removes the entry so the
        // next request regenerates from source.
        let poisoned = clean_store_path(&root, "dddd-s1", "image/jpeg");
        std::fs::create_dir_all(poisoned.parent().unwrap()).unwrap();
        std::fs::write(&poisoned, jpeg_with_make()).unwrap();
        assert!(
            read_clean_store(&root, "dddd-s1", "image/jpeg").await.is_none(),
            "poisoned entry must withhold"
        );
        assert!(!poisoned.exists(), "poisoned entry removed for regeneration");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn prepare_strip_path_serves_from_the_clean_store() {
        let root =
            std::env::temp_dir().join(format!("esko-prepare-store-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let source = jpeg_with_make();
        let key = format!("{}-{STRIP_TAG}", content_hash(&source));
        let first = prepare("jpg", &source, &root).await;
        let Prepared::Ready(clean) = first else {
            panic!("strippable JPEG must serve")
        };
        assert!(!has_sensitive_metadata(&clean.into_bytes()));
        assert!(
            clean_store_path(&root, &key, "image/jpeg").exists(),
            "strip output was promoted into the clean store"
        );
        // Second request is a store hit (still verified per read).
        assert!(matches!(prepare("jpg", &source, &root).await, Prepared::Ready(_)));
        std::fs::remove_dir_all(&root).unwrap();
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
