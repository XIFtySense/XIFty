//! GoPro `.lrv` proxy + `.thm` thumbnail sidecar adapter.
//!
//! GoPro Hero cameras emit a triplet beside every recorded clip:
//!
//! - `<basename>.MP4` — primary clip (e.g. `GH010024.MP4` / `GX010024.MP4`).
//! - `<basename>.LRV` — low-resolution proxy. ISOBMFF/MP4-shaped container
//!   with the same numeric stem but a `GL` prefix shifted from the primary's
//!   `GH`/`GX`. Example: `GH010024.MP4` -> `GL010024.LRV`.
//! - `<basename>.THM` — JFIF thumbnail. Shares the *same* prefix and stem as
//!   the primary (`GH010024.MP4` -> `GH010024.THM`); does **not** use the
//!   `GL` proxy prefix.
//!
//! These siblings are not metadata in the traditional sense — they are
//! pointers to alternative renditions. The adapter surfaces them under the
//! `gopro_sidecar` namespace so downstream consumers (e.g. kstore creator
//! wedge) can ask "is there a proxy I can stream?" or "where's the
//! thumbnail?" without re-walking the directory themselves.
//!
//! Per-tag derivations:
//! - `.THM` dimensions are read by handing the bytes to
//!   [`xifty_container_jpeg::parse_bytes`] and walking the resulting
//!   `JpegSegment` list for the first SOF (start-of-frame) marker. The SOF
//!   payload layout (precision/height-BE/width-BE at offsets 0..5) is shared
//!   across every SOF flavor in the JPEG spec.
//! - `.LRV` dimensions, brand, and duration come from
//!   [`xifty_container_isobmff::parse_bytes`]. We derive `proxy.bitrate_bps`
//!   from `(file size * 8) / mvhd duration` when the duration is known and
//!   positive.

use std::path::{Path, PathBuf};

use xifty_container_isobmff::parse_bytes as parse_isobmff_bytes;
use xifty_container_jpeg::parse_bytes as parse_jpeg_bytes;
use xifty_core::{Issue, MetadataEntry, Provenance, Severity, TypedValue};
use xifty_sidecar::{
    MergeContext, MergePolicy, SIDECAR_PARSE_ERROR, Sidecar, SidecarPayload, SidecarRef,
};

const NAMESPACE: &str = "gopro_sidecar";
const CONTAINER: &str = "sidecar";

const LABEL_LRV: &str = "lrv";
const LABEL_THM: &str = "thm";

/// GoPro `.lrv` + `.thm` adapter — registered automatically by
/// [`xifty_sidecar::SidecarRegistry`] when the CLI/FFI sidecar feature is
/// enabled.
#[derive(Debug, Default)]
pub struct GoProSidecar;

impl GoProSidecar {
    pub const fn new() -> Self {
        Self
    }
}

impl Sidecar for GoProSidecar {
    fn name(&self) -> &'static str {
        NAMESPACE
    }

    fn discover(&self, primary_path: Option<&Path>) -> Vec<SidecarRef> {
        let Some(primary) = primary_path else {
            return Vec::new();
        };
        discover_in_dir(primary)
    }

    fn parse(&self, sidecar: &SidecarRef, bytes: &[u8], _ctx: &MergeContext) -> SidecarPayload {
        match sidecar.label.as_str() {
            LABEL_THM => parse_thm(sidecar, bytes),
            LABEL_LRV => parse_lrv(sidecar, bytes),
            _ => SidecarPayload::default(),
        }
    }

    fn priority(&self) -> u8 {
        100
    }

    fn merge_policy(&self) -> MergePolicy {
        // Proxy/thumbnail pointers carry strictly *additional* fields the
        // primary container does not know about. No tag overlap with embedded
        // metadata is expected — but if one ever surfaces, the conflict
        // detector should flag it, not silently swallow.
        MergePolicy::Complement
    }
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// GoPro convention:
///
/// - LRV: `GH010024.MP4` -> `GL010024.LRV` (prefix shift `GH`/`GX` -> `GL`,
///   numeric stem preserved). Generic `<stem>.LRV` fallback covers renamed
///   clips.
/// - THM: `GH010024.MP4` -> `GH010024.THM` (same prefix and stem).
///
/// All matching is case-insensitive on extension and the camera-prefix
/// letters. We walk the directory once and emit at most one `lrv` + one `thm`
/// `SidecarRef`.
fn discover_in_dir(primary: &Path) -> Vec<SidecarRef> {
    let Some(dir) = primary.parent() else {
        return Vec::new();
    };
    let Some(stem) = primary.file_stem().and_then(|s| s.to_str()) else {
        return Vec::new();
    };
    // We only run when the primary has an extension — buffer-only callers
    // pass `None` and never reach here, but a path without an extension is
    // treated as "no recognizable primary file" too.
    if primary.extension().and_then(|e| e.to_str()).is_none() {
        return Vec::new();
    }

    // Compute the GoPro-shifted LRV stem if the primary stem matches
    // `^G[HX]<digits>$`. `None` if the primary does not follow the GoPro
    // naming convention.
    let lrv_shifted_stem: Option<String> = gopro_lrv_stem(stem);

    let read = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    };

    let mut lrv_hit: Option<PathBuf> = None;
    let mut thm_hit: Option<PathBuf> = None;
    for entry in read.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(dot) = name.rfind('.') else {
            continue;
        };
        let entry_stem = &name[..dot];
        let entry_ext = &name[dot + 1..];
        if entry_ext.eq_ignore_ascii_case("LRV") {
            // Prefer the GoPro-shifted stem; fall back to a generic match on
            // the primary's own stem (renamed-file case).
            if let Some(ref shifted) = lrv_shifted_stem {
                if entry_stem.eq_ignore_ascii_case(shifted) {
                    lrv_hit = Some(path.clone());
                    continue;
                }
            }
            if lrv_hit.is_none() && entry_stem.eq_ignore_ascii_case(stem) {
                lrv_hit = Some(path);
            }
        } else if entry_ext.eq_ignore_ascii_case("THM") && entry_stem.eq_ignore_ascii_case(stem) {
            thm_hit = Some(path);
        }
    }

    let mut hits = Vec::new();
    if let Some(path) = lrv_hit {
        hits.push(SidecarRef {
            path,
            label: LABEL_LRV.into(),
        });
    }
    if let Some(path) = thm_hit {
        hits.push(SidecarRef {
            path,
            label: LABEL_THM.into(),
        });
    }
    hits
}

/// Returns the `GL<digits>` stem when `stem` matches `^G[HX]<digits>$`
/// (case-insensitive on the leading two letters).
fn gopro_lrv_stem(stem: &str) -> Option<String> {
    let bytes = stem.as_bytes();
    if bytes.len() < 3 {
        return None;
    }
    if !bytes[0].eq_ignore_ascii_case(&b'G') {
        return None;
    }
    let second = bytes[1];
    if !(second.eq_ignore_ascii_case(&b'H') || second.eq_ignore_ascii_case(&b'X')) {
        return None;
    }
    if !stem[2..].chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(format!("GL{}", &stem[2..]))
}

// ---------------------------------------------------------------------------
// THM parser
// ---------------------------------------------------------------------------

fn parse_thm(sidecar: &SidecarRef, bytes: &[u8]) -> SidecarPayload {
    let mut entries: Vec<MetadataEntry> = Vec::new();
    let mut issues: Vec<Issue> = Vec::new();

    push_string(
        &mut entries,
        sidecar,
        "thumbnail.path",
        "thumbnail.path",
        sidecar.path.to_string_lossy().into_owned(),
    );
    push_integer(
        &mut entries,
        sidecar,
        "thumbnail.size_bytes",
        "thumbnail.size_bytes",
        bytes.len() as i64,
    );
    push_string(
        &mut entries,
        sidecar,
        "thumbnail.format",
        "thumbnail.format",
        "jfif".into(),
    );

    match parse_jpeg_bytes(bytes, 0) {
        Ok(container) => {
            // Forward parser-level issues so callers can audit malformed
            // segments without losing the structural fields above.
            issues.extend(container.issues.iter().cloned());

            let sof = container.segments.iter().find(|seg| {
                let m = seg.marker;
                (m & 0xF0) == 0xC0 && !matches!(m, 0xC4 | 0xC8 | 0xCC)
            });
            match sof {
                Some(seg) if seg.payload.len() >= 5 => {
                    // SOF payload (length prefix already stripped):
                    //   [0]    sample precision (bits)
                    //   [1..3] image height, big-endian u16
                    //   [3..5] image width,  big-endian u16
                    let height = u16::from_be_bytes([seg.payload[1], seg.payload[2]]) as i64;
                    let width = u16::from_be_bytes([seg.payload[3], seg.payload[4]]) as i64;
                    push_integer(
                        &mut entries,
                        sidecar,
                        "thumbnail.dimensions.width",
                        "thumbnail.dimensions.width",
                        width,
                    );
                    push_integer(
                        &mut entries,
                        sidecar,
                        "thumbnail.dimensions.height",
                        "thumbnail.dimensions.height",
                        height,
                    );
                }
                Some(_) => {
                    issues.push(Issue {
                        severity: Severity::Warning,
                        code: SIDECAR_PARSE_ERROR.into(),
                        message: "GoPro THM SOF segment payload too short for dimensions".into(),
                        offset: None,
                        context: Some(NAMESPACE.into()),
                    });
                }
                None => {
                    issues.push(Issue {
                        severity: Severity::Warning,
                        code: SIDECAR_PARSE_ERROR.into(),
                        message: "GoPro THM has no SOF segment; cannot derive dimensions".into(),
                        offset: None,
                        context: Some(NAMESPACE.into()),
                    });
                }
            }
        }
        Err(error) => {
            issues.push(Issue {
                severity: Severity::Warning,
                code: SIDECAR_PARSE_ERROR.into(),
                message: format!("GoPro THM JPEG parse error: {error}"),
                offset: None,
                context: Some(NAMESPACE.into()),
            });
        }
    }

    SidecarPayload { entries, issues }
}

// ---------------------------------------------------------------------------
// LRV parser
// ---------------------------------------------------------------------------

fn parse_lrv(sidecar: &SidecarRef, bytes: &[u8]) -> SidecarPayload {
    let mut entries: Vec<MetadataEntry> = Vec::new();
    let mut issues: Vec<Issue> = Vec::new();

    push_string(
        &mut entries,
        sidecar,
        "proxy.path",
        "proxy.path",
        sidecar.path.to_string_lossy().into_owned(),
    );
    push_integer(
        &mut entries,
        sidecar,
        "proxy.size_bytes",
        "proxy.size_bytes",
        bytes.len() as i64,
    );
    push_string(
        &mut entries,
        sidecar,
        "proxy.format",
        "proxy.format",
        "mp4".into(),
    );

    match parse_isobmff_bytes(bytes, 0) {
        Ok(container) => {
            issues.extend(container.issues.iter().cloned());

            // major_brand is a 4-byte ASCII tag (`isom`, `mp42`, `qt  `, etc.);
            // we surface it as `proxy.codec` to give consumers a hint about
            // the proxy's container family.
            let brand = std::str::from_utf8(&container.major_brand)
                .map(|s| s.trim_end_matches('\0').trim().to_string())
                .ok();
            if let Some(brand) = brand.filter(|s| !s.is_empty()) {
                push_string(&mut entries, sidecar, "proxy.codec", "proxy.codec", brand);
            }

            if let Some(dim) = container.primary_visual_dimensions.as_ref() {
                push_integer(
                    &mut entries,
                    sidecar,
                    "proxy.dimensions.width",
                    "proxy.dimensions.width",
                    dim.width as i64,
                );
                push_integer(
                    &mut entries,
                    sidecar,
                    "proxy.dimensions.height",
                    "proxy.dimensions.height",
                    dim.height as i64,
                );
            }

            // Bitrate derivation — guarded against the malformed /
            // zero-duration cases (`None` or `<= 0.0`) that would otherwise
            // panic on division or emit a meaningless `0`.
            if let Some(dur) = container.media_duration_seconds {
                if dur > 0.0 {
                    let bitrate = ((bytes.len() as f64 * 8.0) / dur).round() as i64;
                    push_integer(
                        &mut entries,
                        sidecar,
                        "proxy.bitrate_bps",
                        "proxy.bitrate_bps",
                        bitrate,
                    );
                }
            }
        }
        Err(error) => {
            issues.push(Issue {
                severity: Severity::Warning,
                code: SIDECAR_PARSE_ERROR.into(),
                message: format!("GoPro LRV ISOBMFF parse error: {error}"),
                offset: None,
                context: Some(NAMESPACE.into()),
            });
        }
    }

    SidecarPayload { entries, issues }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn provenance(sidecar: &SidecarRef) -> Provenance {
    Provenance {
        container: CONTAINER.into(),
        namespace: NAMESPACE.into(),
        path: Some(sidecar.path.to_string_lossy().into_owned()),
        offset_start: None,
        offset_end: None,
        notes: vec![format!("gopro sidecar ({})", sidecar.label)],
    }
}

fn push_string(
    entries: &mut Vec<MetadataEntry>,
    sidecar: &SidecarRef,
    tag_id: &str,
    tag_name: &str,
    value: String,
) {
    entries.push(MetadataEntry {
        namespace: NAMESPACE.into(),
        tag_id: tag_id.into(),
        tag_name: tag_name.into(),
        value: TypedValue::String(value),
        provenance: provenance(sidecar),
        notes: Vec::new(),
    });
}

fn push_integer(
    entries: &mut Vec<MetadataEntry>,
    sidecar: &SidecarRef,
    tag_id: &str,
    tag_name: &str,
    value: i64,
) {
    entries.push(MetadataEntry {
        namespace: NAMESPACE.into(),
        tag_id: tag_id.into(),
        tag_name: tag_name.into(),
        value: TypedValue::Integer(value),
        provenance: provenance(sidecar),
        notes: Vec::new(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_sidecar(label: &str, path: &str) -> SidecarRef {
        SidecarRef {
            path: PathBuf::from(path),
            label: label.into(),
        }
    }

    fn tag<'a>(payload: &'a SidecarPayload, tag_name: &str) -> Option<&'a MetadataEntry> {
        payload.entries.iter().find(|e| e.tag_name == tag_name)
    }

    // ---------- Discovery -----------------------------------------------------

    #[test]
    fn discover_buffer_only_returns_empty() {
        let s = GoProSidecar::new();
        assert!(s.discover(None).is_empty());
    }

    #[test]
    fn discover_missing_extension_returns_empty() {
        let dir = std::env::temp_dir().join("xifty-sidecar-gopro-no-ext");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // Primary has no extension at all.
        let primary = dir.join("GH010024");
        std::fs::write(&primary, b"x").unwrap();
        std::fs::write(dir.join("GL010024.LRV"), b"x").unwrap();
        let s = GoProSidecar::new();
        assert!(s.discover(Some(&primary)).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn discover_gh_prefix_finds_gl_lrv_and_gh_thm() {
        let dir = std::env::temp_dir().join("xifty-sidecar-gopro-gh");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let primary = dir.join("GH010024.MP4");
        std::fs::write(&primary, b"x").unwrap();
        std::fs::write(dir.join("GL010024.LRV"), b"x").unwrap();
        std::fs::write(dir.join("GH010024.THM"), b"x").unwrap();
        // Decoy that must NOT be picked up as the THM.
        std::fs::write(dir.join("GL010024.THM"), b"x").unwrap();

        let s = GoProSidecar::new();
        let hits = s.discover(Some(&primary));
        assert_eq!(hits.len(), 2);
        let lrv = hits.iter().find(|h| h.label == "lrv").unwrap();
        let thm = hits.iter().find(|h| h.label == "thm").unwrap();
        assert!(
            lrv.path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .eq_ignore_ascii_case("GL010024.LRV")
        );
        assert!(
            thm.path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .eq_ignore_ascii_case("GH010024.THM")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn discover_gx_prefix_finds_gl_lrv_and_gx_thm() {
        let dir = std::env::temp_dir().join("xifty-sidecar-gopro-gx");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let primary = dir.join("GX010099.MP4");
        std::fs::write(&primary, b"x").unwrap();
        std::fs::write(dir.join("GL010099.LRV"), b"x").unwrap();
        std::fs::write(dir.join("GX010099.THM"), b"x").unwrap();

        let s = GoProSidecar::new();
        let hits = s.discover(Some(&primary));
        assert_eq!(hits.len(), 2);
        let thm = hits.iter().find(|h| h.label == "thm").unwrap();
        // Must keep GX prefix on the THM, not shift to GL.
        assert!(
            thm.path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .eq_ignore_ascii_case("GX010099.THM")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn discover_generic_renamed_file_falls_back_to_stem_match() {
        // Renamed primary that no longer follows GoPro's GH/GX naming —
        // generic <stem>.LRV / <stem>.THM fallback must still pick it up.
        let dir = std::env::temp_dir().join("xifty-sidecar-gopro-renamed");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let primary = dir.join("vacation_clip.mp4");
        std::fs::write(&primary, b"x").unwrap();
        std::fs::write(dir.join("vacation_clip.LRV"), b"x").unwrap();
        std::fs::write(dir.join("vacation_clip.thm"), b"x").unwrap();

        let s = GoProSidecar::new();
        let hits = s.discover(Some(&primary));
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().any(|h| h.label == "lrv"));
        assert!(hits.iter().any(|h| h.label == "thm"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---------- THM parsing ---------------------------------------------------

    /// Build a tiny 320x180 SOF0 grayscale JFIF byte slice. We do *not* need
    /// a decodable scan — `xifty_container_jpeg::parse_bytes` only walks
    /// markers, so SOI + APP0 (optional) + SOF0 + EOI is enough for the
    /// dimension reader.
    fn synthetic_thm_320_180() -> Vec<u8> {
        // SOF0 segment for 320x180, 8-bit precision, 1 component:
        //   marker FFC0
        //   length: 2 (length itself) + 1 (precision) + 4 (h+w) + 1 (Nf) + 3 (component) = 11
        //   precision: 0x08
        //   height: 0x00B4 (180)
        //   width:  0x0140 (320)
        //   Nf: 1
        //   component: id=1, sampling=0x11, qt=0
        let mut out: Vec<u8> = Vec::new();
        out.extend_from_slice(&[0xFF, 0xD8]); // SOI
        out.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x0B]); // SOF0, len=11
        out.extend_from_slice(&[0x08, 0x00, 0xB4, 0x01, 0x40, 0x01, 0x01, 0x11, 0x00]);
        out.extend_from_slice(&[0xFF, 0xD9]); // EOI
        out
    }

    #[test]
    fn parses_synthetic_thm_dimensions_320_180() {
        let bytes = synthetic_thm_320_180();
        let payload = parse_thm(&fake_sidecar("thm", "/tmp/fake.THM"), &bytes);
        assert_eq!(
            tag(&payload, "thumbnail.dimensions.width").unwrap().value,
            TypedValue::Integer(320)
        );
        assert_eq!(
            tag(&payload, "thumbnail.dimensions.height").unwrap().value,
            TypedValue::Integer(180)
        );
        assert_eq!(
            tag(&payload, "thumbnail.format").unwrap().value,
            TypedValue::String("jfif".into())
        );
        assert_eq!(
            tag(&payload, "thumbnail.size_bytes").unwrap().value,
            TypedValue::Integer(bytes.len() as i64)
        );
        assert!(tag(&payload, "thumbnail.path").is_some());
        let entry = tag(&payload, "thumbnail.dimensions.width").unwrap();
        assert_eq!(entry.namespace, NAMESPACE);
        assert_eq!(entry.provenance.container, "sidecar");
    }

    #[test]
    fn malformed_thm_surfaces_parse_error_but_still_emits_path() {
        // Junk bytes — not a JPEG. Path/size still surface for the consumer.
        let bytes = b"not a jpeg".to_vec();
        let payload = parse_thm(&fake_sidecar("thm", "/tmp/fake.THM"), &bytes);
        assert!(tag(&payload, "thumbnail.path").is_some());
        assert!(tag(&payload, "thumbnail.size_bytes").is_some());
        assert!(tag(&payload, "thumbnail.dimensions.width").is_none());
        assert!(payload.issues.iter().any(|i| i.code == SIDECAR_PARSE_ERROR));
    }

    #[test]
    fn thm_without_sof_segment_emits_warning() {
        // SOI + APP0 (no SOF) + EOI — a JPEG-shaped file with no frame.
        let bytes = vec![
            0xFF, 0xD8, // SOI
            0xFF, 0xE0, 0x00, 0x06, b'A', b'B', b'C', b'D', // APP0 with payload
            0xFF, 0xD9, // EOI
        ];
        let payload = parse_thm(&fake_sidecar("thm", "/tmp/fake.THM"), &bytes);
        assert!(tag(&payload, "thumbnail.dimensions.width").is_none());
        assert!(payload.issues.iter().any(|i| i.code == SIDECAR_PARSE_ERROR));
    }

    // ---------- LRV parsing ---------------------------------------------------

    /// Build a minimal ISOBMFF byte stream: ftyp(major=mp42) + moov containing
    /// an mvhd with a known duration (3 seconds at timescale 1000) and a
    /// single video track with 640x360 tkhd dimensions.
    fn synthetic_lrv_640_360_3s() -> Vec<u8> {
        fn iso_box(box_type: &[u8; 4], payload: &[u8]) -> Vec<u8> {
            let mut out = Vec::with_capacity(8 + payload.len());
            let size = (8 + payload.len()) as u32;
            out.extend_from_slice(&size.to_be_bytes());
            out.extend_from_slice(box_type);
            out.extend_from_slice(payload);
            out
        }
        fn full_box(box_type: &[u8; 4], payload: &[u8]) -> Vec<u8> {
            let mut p = vec![0u8; 4]; // version + flags
            p.extend_from_slice(payload);
            iso_box(box_type, &p)
        }

        // ftyp: major=mp42, minor=0, compatible=mp42
        let ftyp_payload = {
            let mut p = Vec::new();
            p.extend_from_slice(b"mp42");
            p.extend_from_slice(&0u32.to_be_bytes());
            p.extend_from_slice(b"mp42");
            p
        };
        let ftyp = iso_box(b"ftyp", &ftyp_payload);

        // mvhd v0: creation(4) + modification(4) + timescale(4)=1000 + duration(4)=3000 + 80 bytes pad
        let mut mvhd_payload = Vec::new();
        mvhd_payload.extend_from_slice(&0u32.to_be_bytes());
        mvhd_payload.extend_from_slice(&0u32.to_be_bytes());
        mvhd_payload.extend_from_slice(&1000u32.to_be_bytes());
        mvhd_payload.extend_from_slice(&3000u32.to_be_bytes());
        mvhd_payload.extend_from_slice(&[0u8; 80]);
        let mvhd = full_box(b"mvhd", &mvhd_payload);

        // tkhd v0: creation(4) + modification(4) + track_id(4)=1 + reserved(4) + duration(4)=3000
        //         + reserved(8) + layer(2) + alternate_group(2) + volume(2) + reserved(2)
        //         + matrix(36) + width(4) + height(4)
        // Total payload after version+flags: 4+4+4+4+4+8+2+2+2+2+36+4+4 = 80
        let mut tkhd_payload = Vec::new();
        tkhd_payload.extend_from_slice(&0u32.to_be_bytes()); // creation
        tkhd_payload.extend_from_slice(&0u32.to_be_bytes()); // modification
        tkhd_payload.extend_from_slice(&1u32.to_be_bytes()); // track_id
        tkhd_payload.extend_from_slice(&0u32.to_be_bytes()); // reserved
        tkhd_payload.extend_from_slice(&3000u32.to_be_bytes()); // duration
        tkhd_payload.extend_from_slice(&[0u8; 8]); // reserved
        tkhd_payload.extend_from_slice(&[0u8; 2]); // layer
        tkhd_payload.extend_from_slice(&[0u8; 2]); // alternate_group
        tkhd_payload.extend_from_slice(&[0u8; 2]); // volume
        tkhd_payload.extend_from_slice(&[0u8; 2]); // reserved
        tkhd_payload.extend_from_slice(&[0u8; 36]); // matrix
        // width/height are 16.16 fixed-point.
        tkhd_payload.extend_from_slice(&((640u32) << 16).to_be_bytes());
        tkhd_payload.extend_from_slice(&((360u32) << 16).to_be_bytes());
        let tkhd = full_box(b"tkhd", &tkhd_payload);

        // hdlr: handler=vide
        let mut hdlr_payload = Vec::new();
        hdlr_payload.extend_from_slice(&0u32.to_be_bytes());
        hdlr_payload.extend_from_slice(b"vide");
        hdlr_payload.extend_from_slice(&[0u8; 12]);
        let hdlr = full_box(b"hdlr", &hdlr_payload);

        // mdhd v0: creation(4)+modification(4)+timescale(4)+duration(4)+lang(2)+pre(2) = 20
        let mut mdhd_payload = Vec::new();
        mdhd_payload.extend_from_slice(&0u32.to_be_bytes());
        mdhd_payload.extend_from_slice(&0u32.to_be_bytes());
        mdhd_payload.extend_from_slice(&1000u32.to_be_bytes());
        mdhd_payload.extend_from_slice(&3000u32.to_be_bytes());
        mdhd_payload.extend_from_slice(&[0u8; 4]);
        let mdhd = full_box(b"mdhd", &mdhd_payload);

        let mut mdia_payload = Vec::new();
        mdia_payload.extend_from_slice(&mdhd);
        mdia_payload.extend_from_slice(&hdlr);
        let mdia = iso_box(b"mdia", &mdia_payload);

        let mut trak_payload = Vec::new();
        trak_payload.extend_from_slice(&tkhd);
        trak_payload.extend_from_slice(&mdia);
        let trak = iso_box(b"trak", &trak_payload);

        let mut moov_payload = Vec::new();
        moov_payload.extend_from_slice(&mvhd);
        moov_payload.extend_from_slice(&trak);
        let moov = iso_box(b"moov", &moov_payload);

        let mut out = Vec::new();
        out.extend_from_slice(&ftyp);
        out.extend_from_slice(&moov);
        out
    }

    #[test]
    fn parses_synthetic_lrv_dimensions_codec_and_bitrate() {
        let bytes = synthetic_lrv_640_360_3s();
        let payload = parse_lrv(&fake_sidecar("lrv", "/tmp/fake.LRV"), &bytes);

        assert_eq!(
            tag(&payload, "proxy.format").unwrap().value,
            TypedValue::String("mp4".into())
        );
        assert_eq!(
            tag(&payload, "proxy.size_bytes").unwrap().value,
            TypedValue::Integer(bytes.len() as i64)
        );
        assert_eq!(
            tag(&payload, "proxy.codec").unwrap().value,
            TypedValue::String("mp42".into())
        );
        assert_eq!(
            tag(&payload, "proxy.dimensions.width").unwrap().value,
            TypedValue::Integer(640)
        );
        assert_eq!(
            tag(&payload, "proxy.dimensions.height").unwrap().value,
            TypedValue::Integer(360)
        );
        // bitrate = (size * 8) / 3.0 — known duration is 3.0s.
        let expected = ((bytes.len() as f64 * 8.0) / 3.0).round() as i64;
        assert_eq!(
            tag(&payload, "proxy.bitrate_bps").unwrap().value,
            TypedValue::Integer(expected)
        );
    }

    #[test]
    fn lrv_with_no_duration_omits_bitrate_silently() {
        // ftyp-only file — ISOBMFF parser will leave media_duration_seconds None.
        let mut bytes = Vec::new();
        let ftyp_payload = {
            let mut p = Vec::new();
            p.extend_from_slice(b"mp42");
            p.extend_from_slice(&0u32.to_be_bytes());
            p.extend_from_slice(b"mp42");
            p
        };
        let size = (8 + ftyp_payload.len()) as u32;
        bytes.extend_from_slice(&size.to_be_bytes());
        bytes.extend_from_slice(b"ftyp");
        bytes.extend_from_slice(&ftyp_payload);

        let payload = parse_lrv(&fake_sidecar("lrv", "/tmp/fake.LRV"), &bytes);
        assert!(tag(&payload, "proxy.bitrate_bps").is_none());
        // No spurious warnings about missing duration — that's a benign case.
        assert!(
            !payload.issues.iter().any(|i| i.code == SIDECAR_PARSE_ERROR),
            "ftyp-only proxy should not raise sidecar_parse_error: {:?}",
            payload.issues
        );
    }
}
