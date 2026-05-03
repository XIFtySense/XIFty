//! Classic `.THM` JFIF thumbnail sidecar adapter.
//!
//! Older Canon, Nikon, Olympus, Pentax, Casio, Sony, and Panasonic camcorders
//! drop a `<basename>.THM` JFIF thumbnail next to every `<basename>.MOV` /
//! `.AVI` / `.MP4` / `.MTS` clip. Those THMs frequently carry the *only* rich
//! EXIF (make/model/captured_at, sometimes GPS) the user has for archival
//! video — the video container itself is often metadata-bare in the
//! mid-2000s through ~2015 era.
//!
//! This adapter is a deliberate complement to `xifty-sidecar-gopro`, which
//! handles GoPro's specific `GH/GX`-prefix `.THM` flavor. We skip stems
//! matching `^G[HX]\d+$` (case-insensitive on the leading two letters) so
//! both adapters do not double-claim the same file. `MergePolicy::Complement`
//! does no deduplication (`xifty-sidecar::SidecarRegistry::merge_into`) — the
//! stem-skip in `discover()` is the SOLE collision-avoidance mechanism.
//! `priority: 50` is sort-order only.
//!
//! Per-tag derivations:
//! - Dimensions are read by handing the bytes to
//!   [`xifty_container_jpeg::parse_bytes`] and walking the resulting segment
//!   list for the first SOF (start-of-frame) marker. The SOF payload layout
//!   (precision/height-BE/width-BE at offsets 0..5) is shared across every
//!   SOF flavor in the JPEG spec. *Kept in sync with
//!   `xifty-sidecar-gopro::parse_thm` SOF walk.*
//! - When the JFIF carries an APP1 EXIF segment
//!   ([`xifty_container_jpeg::JpegContainer::exif_payload`]), the raw bytes
//!   are emitted as a single `thumbnail.exif_payload` `TypedValue::Bytes`
//!   entry. The CLI then dispatches that payload through the existing
//!   TIFF + EXIF decode chain and projects selected EXIF tags onto flat
//!   `thumbnail.exif.{make,model,captured_at}` fields under namespace
//!   `classic_thm`. This crate has zero awareness of EXIF tag semantics.

use std::path::{Path, PathBuf};

use xifty_container_jpeg::parse_bytes as parse_jpeg_bytes;
use xifty_core::{Issue, MetadataEntry, Provenance, Severity, TypedValue};
use xifty_sidecar::{
    MergeContext, MergePolicy, SIDECAR_PARSE_ERROR, Sidecar, SidecarPayload, SidecarRef,
};

const NAMESPACE: &str = "classic_thm";
const CONTAINER: &str = "sidecar";

const LABEL_THM: &str = "thm";

/// Classic `.THM` adapter — registered automatically by
/// [`xifty_sidecar::SidecarRegistry`] when the CLI/FFI sidecar feature is
/// enabled.
#[derive(Debug, Default)]
pub struct ClassicThmSidecar;

impl ClassicThmSidecar {
    pub const fn new() -> Self {
        Self
    }
}

impl Sidecar for ClassicThmSidecar {
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
            _ => SidecarPayload::default(),
        }
    }

    fn priority(&self) -> u8 {
        // Sort order only — MergePolicy::Complement does a plain .extend();
        // the stem-skip in discover() is the sole de-duplication mechanism.
        50
    }

    fn merge_policy(&self) -> MergePolicy {
        MergePolicy::Complement
    }
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// Classic camcorder convention: `<basename>.THM` (case-insensitive) sits
/// next to `<basename>.{mov,avi,mp4,mts}`. We skip stems claimed by the
/// GoPro adapter (`^G[HX]\d+$`) so the two adapters do not double-claim the
/// same THM file. `GP`-prefix stems are NOT skipped — the GoPro adapter does
/// not claim them today, and skipping them here would orphan the file.
fn discover_in_dir(primary: &Path) -> Vec<SidecarRef> {
    let Some(dir) = primary.parent() else {
        return Vec::new();
    };
    let Some(stem) = primary.file_stem().and_then(|s| s.to_str()) else {
        return Vec::new();
    };
    let Some(ext) = primary.extension().and_then(|e| e.to_str()) else {
        return Vec::new();
    };
    if !is_supported_video_ext(ext) {
        return Vec::new();
    }
    if is_gopro_stem(stem) {
        return Vec::new();
    }

    let read = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    };

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
        if entry_ext.eq_ignore_ascii_case("THM") && entry_stem.eq_ignore_ascii_case(stem) {
            thm_hit = Some(path);
            break;
        }
    }

    let mut hits = Vec::new();
    if let Some(path) = thm_hit {
        hits.push(SidecarRef {
            path,
            label: LABEL_THM.into(),
        });
    }
    hits
}

fn is_supported_video_ext(ext: &str) -> bool {
    ext.eq_ignore_ascii_case("mov")
        || ext.eq_ignore_ascii_case("avi")
        || ext.eq_ignore_ascii_case("mp4")
        || ext.eq_ignore_ascii_case("mts")
}

/// Returns `true` when `stem` matches `^G[HX]\d+$` (case-insensitive on the
/// leading two letters) — the exact coverage of `gopro_lrv_stem` in
/// `xifty-sidecar-gopro`. `GP` stems intentionally do NOT match.
fn is_gopro_stem(stem: &str) -> bool {
    let bytes = stem.as_bytes();
    if bytes.len() < 3 {
        return false;
    }
    if !bytes[0].eq_ignore_ascii_case(&b'G') {
        return false;
    }
    let second = bytes[1];
    if !(second.eq_ignore_ascii_case(&b'H') || second.eq_ignore_ascii_case(&b'X')) {
        return false;
    }
    stem[2..].chars().all(|c| c.is_ascii_digit())
}

// ---------------------------------------------------------------------------
// THM parser
// ---------------------------------------------------------------------------

// kept in sync with xifty-sidecar-gopro::parse_thm SOF walk
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
                        message: "classic THM SOF segment payload too short for dimensions".into(),
                        offset: None,
                        context: Some(NAMESPACE.into()),
                    });
                }
                None => {
                    issues.push(Issue {
                        severity: Severity::Warning,
                        code: SIDECAR_PARSE_ERROR.into(),
                        message: "classic THM has no SOF segment; cannot derive dimensions".into(),
                        offset: None,
                        context: Some(NAMESPACE.into()),
                    });
                }
            }

            // Boundary-clean handoff: emit the raw APP1 EXIF payload (if any)
            // as a single `thumbnail.exif_payload` Bytes entry. The CLI takes
            // care of TIFF + EXIF decoding and projects the flat
            // `thumbnail.exif.{make,model,captured_at}` fields. No payload =
            // no entry, no warning (pure-JFIF THMs are normal).
            if let Some((base_offset, payload)) = container.exif_payload() {
                entries.push(MetadataEntry {
                    namespace: NAMESPACE.into(),
                    tag_id: "thumbnail.exif_payload".into(),
                    tag_name: "thumbnail.exif_payload".into(),
                    value: TypedValue::Bytes(payload.to_vec()),
                    provenance: Provenance {
                        container: CONTAINER.into(),
                        namespace: NAMESPACE.into(),
                        path: Some(sidecar.path.to_string_lossy().into_owned()),
                        offset_start: Some(base_offset),
                        offset_end: Some(base_offset + payload.len() as u64),
                        notes: vec![format!("classic thm app1 exif ({})", sidecar.label)],
                    },
                    notes: Vec::new(),
                });
            }
        }
        Err(error) => {
            issues.push(Issue {
                severity: Severity::Warning,
                code: SIDECAR_PARSE_ERROR.into(),
                message: format!("classic THM JPEG parse error: {error}"),
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
        notes: vec![format!("classic thm sidecar ({})", sidecar.label)],
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

    fn unique_dir(name: &str) -> PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("xifty-sidecar-classic-thm-{name}-{stamp}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // ---------- Discovery -----------------------------------------------------

    #[test]
    fn discover_buffer_only_returns_empty() {
        let s = ClassicThmSidecar::new();
        assert!(s.discover(None).is_empty());
    }

    #[test]
    fn discover_mov_finds_uppercase_and_lowercase_thm() {
        let dir = unique_dir("mov-case");
        let primary = dir.join("clip.MOV");
        std::fs::write(&primary, b"x").unwrap();
        std::fs::write(dir.join("clip.THM"), b"x").unwrap();
        let s = ClassicThmSidecar::new();
        let hits = s.discover(Some(&primary));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].label, "thm");
        let _ = std::fs::remove_dir_all(&dir);

        let dir = unique_dir("mov-case-lower");
        let primary = dir.join("clip.MOV");
        std::fs::write(&primary, b"x").unwrap();
        std::fs::write(dir.join("clip.thm"), b"x").unwrap();
        let s = ClassicThmSidecar::new();
        let hits = s.discover(Some(&primary));
        assert_eq!(hits.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn discover_mp4_avi_mts_primaries_find_thm() {
        for ext in ["mp4", "AVI", "MTS"] {
            let dir = unique_dir(&format!("ext-{ext}"));
            let primary = dir.join(format!("clip.{ext}"));
            std::fs::write(&primary, b"x").unwrap();
            std::fs::write(dir.join("clip.THM"), b"x").unwrap();
            let s = ClassicThmSidecar::new();
            let hits = s.discover(Some(&primary));
            assert_eq!(hits.len(), 1, "expected hit for .{ext}");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn discover_skips_gopro_gh_and_gx_stems() {
        for stem in ["GH010024", "GX010024", "gh010024"] {
            let dir = unique_dir(&format!("gopro-{stem}"));
            let primary = dir.join(format!("{stem}.MP4"));
            std::fs::write(&primary, b"x").unwrap();
            std::fs::write(dir.join(format!("{stem}.THM")), b"x").unwrap();
            let s = ClassicThmSidecar::new();
            let hits = s.discover(Some(&primary));
            assert!(
                hits.is_empty(),
                "classic_thm must NOT claim GoPro stem {stem}"
            );
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn discover_finds_gp_stem_so_file_is_not_orphaned() {
        // GoPro adapter does NOT claim GP-prefix stems today, so classic_thm
        // must surface them — otherwise the file is orphaned.
        let dir = unique_dir("gp");
        let primary = dir.join("GP010024.MP4");
        std::fs::write(&primary, b"x").unwrap();
        std::fs::write(dir.join("GP010024.THM"), b"x").unwrap();
        let s = ClassicThmSidecar::new();
        let hits = s.discover(Some(&primary));
        assert_eq!(hits.len(), 1, "GP stems must be claimed by classic_thm");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn discover_skips_non_video_primaries() {
        for ext in ["jpg", "cr3", "png", "txt"] {
            let dir = unique_dir(&format!("non-video-{ext}"));
            let primary = dir.join(format!("clip.{ext}"));
            std::fs::write(&primary, b"x").unwrap();
            std::fs::write(dir.join("clip.THM"), b"x").unwrap();
            let s = ClassicThmSidecar::new();
            let hits = s.discover(Some(&primary));
            assert!(hits.is_empty(), "must not claim THM for .{ext} primary");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn discover_no_thm_sibling_returns_empty() {
        let dir = unique_dir("no-sibling");
        let primary = dir.join("clip.MOV");
        std::fs::write(&primary, b"x").unwrap();
        let s = ClassicThmSidecar::new();
        assert!(s.discover(Some(&primary)).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---------- THM parsing ---------------------------------------------------

    /// Build a tiny 320x180 SOF0 grayscale JFIF byte slice.
    fn synthetic_thm_320_180() -> Vec<u8> {
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
        let bytes = b"not a jpeg".to_vec();
        let payload = parse_thm(&fake_sidecar("thm", "/tmp/fake.THM"), &bytes);
        assert!(tag(&payload, "thumbnail.path").is_some());
        assert!(tag(&payload, "thumbnail.size_bytes").is_some());
        assert!(tag(&payload, "thumbnail.dimensions.width").is_none());
        assert!(payload.issues.iter().any(|i| i.code == SIDECAR_PARSE_ERROR));
    }

    /// Synthetic JFIF carrying APP1 `Exif\0\0` + a minimal little-endian TIFF
    /// header. We only need the parser to recognize the segment — TIFF
    /// validity is not asserted at the sidecar layer (CLI projection step
    /// takes care of TIFF + EXIF decode).
    fn synthetic_thm_with_app1_exif() -> (Vec<u8>, Vec<u8>) {
        // Minimal TIFF bytes: II + 42 + IFD offset 8 + zero-entry IFD.
        let mut tiff: Vec<u8> = Vec::new();
        tiff.extend_from_slice(b"II");
        tiff.extend_from_slice(&42u16.to_le_bytes());
        tiff.extend_from_slice(&8u32.to_le_bytes());
        tiff.extend_from_slice(&0u16.to_le_bytes()); // 0 IFD entries
        tiff.extend_from_slice(&0u32.to_le_bytes()); // next IFD = 0

        // APP1 segment payload = "Exif\0\0" + tiff.
        let mut app1_payload: Vec<u8> = Vec::new();
        app1_payload.extend_from_slice(b"Exif\0\0");
        app1_payload.extend_from_slice(&tiff);
        let seg_len = (app1_payload.len() + 2) as u16;

        let mut out: Vec<u8> = Vec::new();
        out.extend_from_slice(&[0xFF, 0xD8]); // SOI
        out.extend_from_slice(&[0xFF, 0xE1]);
        out.extend_from_slice(&seg_len.to_be_bytes());
        out.extend_from_slice(&app1_payload);
        // SOF0 320x180
        out.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x0B]);
        out.extend_from_slice(&[0x08, 0x00, 0xB4, 0x01, 0x40, 0x01, 0x01, 0x11, 0x00]);
        out.extend_from_slice(&[0xFF, 0xD9]); // EOI
        (out, tiff)
    }

    #[test]
    fn emits_raw_app1_exif_payload_as_bytes_with_offsets() {
        let (bytes, expected_tiff) = synthetic_thm_with_app1_exif();
        let payload = parse_thm(&fake_sidecar("thm", "/tmp/fake.THM"), &bytes);
        let entry = tag(&payload, "thumbnail.exif_payload").expect("payload entry present");
        assert_eq!(entry.namespace, NAMESPACE);
        match &entry.value {
            TypedValue::Bytes(b) => {
                assert_eq!(b, &expected_tiff, "raw TIFF bytes preserved verbatim");
            }
            other => panic!("expected TypedValue::Bytes, got {other:?}"),
        }
        // Offsets bracket the TIFF region exactly (post-`Exif\0\0`).
        let start = entry.provenance.offset_start.unwrap();
        let end = entry.provenance.offset_end.unwrap();
        assert_eq!((end - start) as usize, expected_tiff.len());
        // No flat `thumbnail.exif.*` fields at the sidecar layer — those are
        // CLI-projected.
        assert!(tag(&payload, "thumbnail.exif.make").is_none());
        assert!(tag(&payload, "thumbnail.exif.model").is_none());
        assert!(tag(&payload, "thumbnail.exif.captured_at").is_none());
    }

    #[test]
    fn jfif_without_app1_emits_no_exif_payload_and_no_warning() {
        let bytes = synthetic_thm_320_180();
        let payload = parse_thm(&fake_sidecar("thm", "/tmp/fake.THM"), &bytes);
        assert!(tag(&payload, "thumbnail.exif_payload").is_none());
        // Pure JFIF is normal — must not raise sidecar_parse_error for the
        // missing APP1.
        assert!(
            !payload.issues.iter().any(|i| i.code == SIDECAR_PARSE_ERROR),
            "pure-JFIF THM should not raise sidecar_parse_error: {:?}",
            payload.issues
        );
    }

    #[test]
    fn thm_without_sof_segment_emits_warning() {
        let bytes = vec![
            0xFF, 0xD8, // SOI
            0xFF, 0xE0, 0x00, 0x06, b'A', b'B', b'C', b'D', // APP0 with payload
            0xFF, 0xD9, // EOI
        ];
        let payload = parse_thm(&fake_sidecar("thm", "/tmp/fake.THM"), &bytes);
        assert!(tag(&payload, "thumbnail.dimensions.width").is_none());
        assert!(payload.issues.iter().any(|i| i.code == SIDECAR_PARSE_ERROR));
    }
}
