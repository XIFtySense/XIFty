//! Fuji RAF container parser.
//!
//! RAF is Fuji's proprietary RAW container. It begins with the ASCII magic
//! `FUJIFILMCCD-RAW` at byte 0, followed by a fixed-layout header pointing to
//! a JPEG preview block, a CFA header block, and the raw image data. The
//! preview JPEG carries the camera's standard EXIF/MakerNote in an `APP1`
//! segment — that embedded TIFF is what XIFty exposes via
//! [`embedded_tiff_slice`].
//!
//! ## Header layout (big-endian u32 fields)
//!
//! | Offset | Size | Field                  |
//! | ------ | ---- | ---------------------- |
//! | 0      | 16   | magic "FUJIFILMCCD-RAW" + NUL |
//! | 16     | 4    | format version (ASCII) |
//! | 20     | 8    | camera id              |
//! | 28     | 32   | camera string          |
//! | 60     | 4    | dir version            |
//! | 64     | 20   | unknown / reserved     |
//! | 84     | 4    | jpeg preview offset    |
//! | 88     | 4    | jpeg preview length    |
//! | 92     | 4    | cfa-header offset      |
//! | 96     | 4    | cfa-header length      |
//! | 100    | 4    | cfa-record offset      |
//! | 104    | 4    | cfa-record length      |
//!
//! The header always uses big-endian word ordering regardless of the embedded
//! TIFF's byte order. Embedded EXIF endianness is determined by the TIFF
//! magic (`II*\0` / `MM\0*`) at the start of the EXIF payload.

use xifty_container_jpeg::parse_bytes as parse_jpeg_bytes;
use xifty_core::{ContainerNode, Issue, Severity, XiftyError, issue};
use xifty_source::SourceBytes;

const RAF_MAGIC: &[u8] = b"FUJIFILMCCD-RAW";
const HEADER_MIN_LEN: usize = 108;

#[derive(Debug, Clone, Copy)]
pub struct RafBlock {
    pub offset: u64,
    pub length: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct RafEmbeddedTiff {
    pub offset: u64,
    pub length: u64,
}

#[derive(Debug, Clone)]
pub struct RafContainer {
    pub nodes: Vec<ContainerNode>,
    pub issues: Vec<Issue>,
    pub jpeg_preview: Option<RafBlock>,
    pub cfa_header: Option<RafBlock>,
    pub raw_data: Option<RafBlock>,
    pub embedded_tiff: Option<RafEmbeddedTiff>,
}

pub fn parse(source: &SourceBytes) -> Result<RafContainer, XiftyError> {
    parse_bytes(source.bytes())
}

pub fn parse_bytes(bytes: &[u8]) -> Result<RafContainer, XiftyError> {
    if bytes.len() < HEADER_MIN_LEN {
        return Err(XiftyError::Parse {
            message: "raf header truncated".into(),
        });
    }
    if &bytes[0..RAF_MAGIC.len()] != RAF_MAGIC {
        return Err(XiftyError::Parse {
            message: "raf magic mismatch".into(),
        });
    }

    let mut nodes = vec![ContainerNode {
        kind: "container".into(),
        label: "raf".into(),
        offset_start: 0,
        offset_end: bytes.len() as u64,
        parent_label: None,
    }];
    let mut issues = Vec::new();

    nodes.push(ContainerNode {
        kind: "header".into(),
        label: "raf_header".into(),
        offset_start: 0,
        offset_end: HEADER_MIN_LEN as u64,
        parent_label: Some("raf".into()),
    });

    let jpeg_offset = read_u32_be(bytes, 84);
    let jpeg_length = read_u32_be(bytes, 88);
    let cfa_offset = read_u32_be(bytes, 92);
    let cfa_length = read_u32_be(bytes, 96);
    let raw_offset = read_u32_be(bytes, 100);
    let raw_length = read_u32_be(bytes, 104);

    let jpeg_preview = make_block(
        bytes,
        jpeg_offset,
        jpeg_length,
        "raf_jpeg_preview",
        &mut nodes,
        &mut issues,
    );
    let cfa_header = make_block(
        bytes,
        cfa_offset,
        cfa_length,
        "raf_cfa_header",
        &mut nodes,
        &mut issues,
    );
    let raw_data = make_block(
        bytes,
        raw_offset,
        raw_length,
        "raf_raw_data",
        &mut nodes,
        &mut issues,
    );

    let embedded_tiff = if let Some(block) = jpeg_preview {
        locate_embedded_tiff(bytes, block, &mut nodes, &mut issues)
    } else {
        None
    };

    if embedded_tiff.is_none() {
        issues.push(issue(
            Severity::Warning,
            "raf_embedded_tiff_missing",
            "could not locate embedded EXIF TIFF inside RAF preview",
        ));
    }

    Ok(RafContainer {
        nodes,
        issues,
        jpeg_preview,
        cfa_header,
        raw_data,
        embedded_tiff,
    })
}

/// Returns the absolute offset and a borrowed slice of the embedded EXIF TIFF
/// payload, or `None` when the RAF did not carry one.
pub fn embedded_tiff_slice<'a>(
    bytes: &'a [u8],
    container: &RafContainer,
) -> Option<(u64, &'a [u8])> {
    let tiff = container.embedded_tiff?;
    let start = usize::try_from(tiff.offset).ok()?;
    let end = start.checked_add(usize::try_from(tiff.length).ok()?)?;
    let slice = bytes.get(start..end)?;
    Some((tiff.offset, slice))
}

fn make_block(
    bytes: &[u8],
    offset: u32,
    length: u32,
    label: &str,
    nodes: &mut Vec<ContainerNode>,
    issues: &mut Vec<Issue>,
) -> Option<RafBlock> {
    if offset == 0 || length == 0 {
        return None;
    }
    let start = offset as usize;
    let end = start.checked_add(length as usize)?;
    if end > bytes.len() {
        issues.push(issue(
            Severity::Warning,
            "raf_block_out_of_bounds",
            format!("{label} block exceeds file length"),
        ));
        return None;
    }
    nodes.push(ContainerNode {
        kind: "block".into(),
        label: label.into(),
        offset_start: start as u64,
        offset_end: end as u64,
        parent_label: Some("raf".into()),
    });
    Some(RafBlock {
        offset: offset as u64,
        length: length as u64,
    })
}

/// Resolve the embedded EXIF TIFF block. Two possible shapes:
///
/// 1. The RAF preview block is itself a JFIF JPEG; the EXIF lives inside its
///    `APP1/Exif\0\0` segment. This is by far the common case for production
///    Fuji bodies.
/// 2. The preview block points directly at a bare TIFF (`II*\0` / `MM\0*`).
///    Some older / minor bodies and round-tripped writers do this.
fn locate_embedded_tiff(
    bytes: &[u8],
    preview: RafBlock,
    nodes: &mut Vec<ContainerNode>,
    _issues: &mut Vec<Issue>,
) -> Option<RafEmbeddedTiff> {
    let start = usize::try_from(preview.offset).ok()?;
    let end = start.checked_add(usize::try_from(preview.length).ok()?)?;
    let preview_slice = bytes.get(start..end)?;

    // Bare TIFF preview: the block itself starts with TIFF magic.
    if preview_slice.starts_with(b"II*\0") || preview_slice.starts_with(b"MM\0*") {
        let tiff = RafEmbeddedTiff {
            offset: preview.offset,
            length: preview.length,
        };
        nodes.push(ContainerNode {
            kind: "block".into(),
            label: "raf_embedded_tiff".into(),
            offset_start: tiff.offset,
            offset_end: tiff.offset + tiff.length,
            parent_label: Some("raf_jpeg_preview".into()),
        });
        return Some(tiff);
    }

    // JPEG preview: parse it via xifty-container-jpeg and pull EXIF APP1.
    // `JpegContainer::exif_payload()` returns offsets relative to the JPEG
    // buffer start, so rebase against the preview block's absolute offset.
    if preview_slice.starts_with(&[0xFF, 0xD8]) {
        let jpeg = parse_jpeg_bytes(preview_slice, 0).ok()?;
        let (relative_exif_offset, exif_payload) = jpeg.exif_payload()?;
        let tiff = RafEmbeddedTiff {
            offset: preview.offset + relative_exif_offset,
            length: exif_payload.len() as u64,
        };
        nodes.push(ContainerNode {
            kind: "block".into(),
            label: "raf_embedded_tiff".into(),
            offset_start: tiff.offset,
            offset_end: tiff.offset + tiff.length,
            parent_label: Some("raf_jpeg_preview".into()),
        });
        return Some(tiff);
    }

    None
}

fn read_u32_be(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic RAF whose preview slot points at a bare little-endian
    /// TIFF block. Returns `(bytes, expected_tiff_offset, expected_tiff_len)`.
    fn build_raf_with_bare_tiff() -> (Vec<u8>, u64, u64) {
        let tiff = b"II*\0\x08\0\0\0\x00\x00\0\0\0\0".to_vec();
        let preview_offset: u32 = HEADER_MIN_LEN as u32; // tiff lives just after header
        let preview_len: u32 = tiff.len() as u32;

        let mut bytes = vec![0u8; HEADER_MIN_LEN];
        bytes[0..RAF_MAGIC.len()].copy_from_slice(RAF_MAGIC);
        bytes[15] = 0; // NUL after magic (16 bytes total magic field)
        bytes[16..20].copy_from_slice(b"0201");
        bytes[84..88].copy_from_slice(&preview_offset.to_be_bytes());
        bytes[88..92].copy_from_slice(&preview_len.to_be_bytes());
        bytes.extend_from_slice(&tiff);
        (bytes, preview_offset as u64, preview_len as u64)
    }

    /// Build a synthetic RAF whose preview is a real JFIF JPEG carrying an
    /// `APP1/Exif\0\0` segment with a tiny TIFF payload. Returns the bytes plus
    /// the absolute expected EXIF TIFF offset.
    fn build_raf_with_jpeg_preview() -> (Vec<u8>, u64) {
        let exif_tiff = b"II*\0\x08\0\0\0\x00\x00\0\0\0\0".to_vec();
        let mut app1_payload = b"Exif\0\0".to_vec();
        app1_payload.extend_from_slice(&exif_tiff);
        // APP1 marker + length (covers length bytes themselves + payload)
        let mut jpeg = vec![0xFF, 0xD8];
        let app1_len = app1_payload.len() + 2;
        jpeg.push(0xFF);
        jpeg.push(0xE1);
        jpeg.extend_from_slice(&(app1_len as u16).to_be_bytes());
        // EXIF TIFF lives at JPEG-relative offset = current jpeg.len()
        // (after SOI + APP1 marker + length) + 6 (skip "Exif\0\0").
        let exif_in_jpeg = jpeg.len() + 6;
        jpeg.extend_from_slice(&app1_payload);
        jpeg.extend_from_slice(&[0xFF, 0xD9]);

        let preview_offset: u32 = HEADER_MIN_LEN as u32;
        let preview_len: u32 = jpeg.len() as u32;

        let mut bytes = vec![0u8; HEADER_MIN_LEN];
        bytes[0..RAF_MAGIC.len()].copy_from_slice(RAF_MAGIC);
        bytes[16..20].copy_from_slice(b"0201");
        bytes[84..88].copy_from_slice(&preview_offset.to_be_bytes());
        bytes[88..92].copy_from_slice(&preview_len.to_be_bytes());
        bytes.extend_from_slice(&jpeg);

        let expected_exif_offset = HEADER_MIN_LEN as u64 + exif_in_jpeg as u64;
        (bytes, expected_exif_offset)
    }

    #[test]
    fn parses_bare_tiff_preview() {
        let (bytes, expected_offset, expected_len) = build_raf_with_bare_tiff();
        let raf = parse_bytes(&bytes).expect("parse");
        let (offset, slice) = embedded_tiff_slice(&bytes, &raf).expect("tiff slice");
        assert_eq!(offset, expected_offset);
        assert_eq!(slice.len(), expected_len as usize);
        assert!(slice.starts_with(b"II*\0"));
        assert!(raf.jpeg_preview.is_some());
        assert!(raf.embedded_tiff.is_some());
    }

    #[test]
    fn parses_jpeg_preview_with_app1_exif() {
        let (bytes, expected_exif_offset) = build_raf_with_jpeg_preview();
        let raf = parse_bytes(&bytes).expect("parse");
        let (offset, slice) = embedded_tiff_slice(&bytes, &raf).expect("tiff slice");
        assert_eq!(offset, expected_exif_offset);
        assert!(slice.starts_with(b"II*\0"));
    }

    #[test]
    fn rejects_magic_mismatch() {
        let mut bytes = vec![0u8; HEADER_MIN_LEN];
        bytes[..6].copy_from_slice(b"NOTRAF");
        assert!(parse_bytes(&bytes).is_err());
    }

    #[test]
    fn rejects_truncated_header() {
        let bytes = vec![0u8; 10];
        assert!(parse_bytes(&bytes).is_err());
    }

    #[test]
    fn flags_out_of_bounds_block_offsets() {
        let mut bytes = vec![0u8; HEADER_MIN_LEN];
        bytes[0..RAF_MAGIC.len()].copy_from_slice(RAF_MAGIC);
        // Point preview block past EOF.
        bytes[84..88].copy_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
        bytes[88..92].copy_from_slice(&0x10u32.to_be_bytes());
        let raf = parse_bytes(&bytes).expect("parse");
        assert!(raf.jpeg_preview.is_none());
        assert!(
            raf.issues
                .iter()
                .any(|i| i.code == "raf_block_out_of_bounds")
        );
        assert!(
            raf.issues
                .iter()
                .any(|i| i.code == "raf_embedded_tiff_missing")
        );
    }
}
