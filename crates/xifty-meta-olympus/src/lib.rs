//! Olympus MakerNote decoder (`namespace = "olympus_makernote"`).
//!
//! Status: **bounded** — emits a small starter set of well-understood Olympus
//! MakerNote tags. The canonical Olympus tag list is large and largely model
//! dependent; tags outside the curated allowlist are surfaced structurally as
//! `OlympusTag_<hex>` so probe/extract output reflects what is present without
//! inventing semantic meaning.
//!
//! ## MakerNote header variants
//!
//! Olympus has shipped three MakerNote header shapes over time. We detect
//! them in **longest-match-first** order because `OLYMP\0` is a strict prefix
//! of `OLYMP\0\x01\0`:
//!
//! 1. `OLYMPUS\0II\x03\0` (10 bytes, newer Olympus). IFD entry value offsets
//!    are **absolute file offsets**.
//! 2. `OLYMP\0\x01\0` (8 bytes, legacy Olympus). IFD entry value offsets are
//!    **relative to the MakerNote start**.
//! 3. `OLYMP\0` (6 bytes, very old Olympus). Same offset semantics as the
//!    legacy 8-byte header.
//!
//! Header detection failures degrade silently to an empty result; the EXIF
//! surface still renders.

use xifty_container_tiff::TiffContainer;
use xifty_core::{MetadataEntry, Provenance, TypedValue};
use xifty_source::{Cursor, Endian};

/// Header variants. Order matters — longest-prefix-first to avoid `OLYMP\0`
/// shadowing the longer `OLYMP\0\x01\0` form.
const OLYMPUS_HEADERS: &[(&[u8], OffsetMode)] = &[
    (b"OLYMPUS\0II\x03\0", OffsetMode::Absolute),
    (b"OLYMP\0\x01\0", OffsetMode::RelativeToMakerNote),
    (b"OLYMP\0", OffsetMode::RelativeToMakerNote),
];

#[derive(Debug, Clone, Copy)]
enum OffsetMode {
    /// Sub-IFD value offsets are absolute file offsets.
    Absolute,
    /// Sub-IFD value offsets are relative to the MakerNote payload start.
    RelativeToMakerNote,
}

#[derive(Debug, Clone, Copy)]
struct OlympusEntry {
    tag_id: u16,
    type_id: u16,
    count: u32,
    value_or_offset: u32,
}

/// Decode an Olympus MakerNote sub-IFD inside a parent TIFF buffer.
///
/// Returns an empty `Vec` when EXIF `Make` is absent / not Olympus or when
/// the MakerNote tag is not present or its header is unrecognised.
pub fn decode_from_tiff(
    bytes: &[u8],
    base_offset: u64,
    container_name: &str,
    tiff: &TiffContainer,
    exif_entries: &[MetadataEntry],
) -> Vec<MetadataEntry> {
    let make = exif_entries
        .iter()
        .find(|entry| entry.namespace == "exif" && entry.tag_name == "Make")
        .and_then(|entry| match &entry.value {
            TypedValue::String(value) => Some(value.as_str()),
            _ => None,
        });
    if !matches!(
        make,
        Some(value) if value.trim().to_ascii_uppercase().starts_with("OLYMPUS")
    ) {
        return Vec::new();
    }

    let Some(maker_note) = tiff.entries.iter().find(|entry| entry.tag_id == 0x927C) else {
        return Vec::new();
    };

    let start = maker_note.value_or_offset as usize;
    let count = maker_note.count as usize;
    let end = match start.checked_add(count) {
        Some(end) => end,
        None => return Vec::new(),
    };
    let Some(maker_bytes) = bytes.get(start..end) else {
        return Vec::new();
    };

    // Detect header in longest-prefix-first order.
    let Some((header_len, mode)) = OLYMPUS_HEADERS
        .iter()
        .find(|(prefix, _)| maker_bytes.starts_with(prefix))
        .map(|(prefix, mode)| (prefix.len(), *mode))
    else {
        return Vec::new();
    };

    let mut out = Vec::new();
    match mode {
        OffsetMode::RelativeToMakerNote => {
            // Build a cursor whose origin is the MakerNote start. Sub-IFD entry
            // value offsets are then directly indexable inside `maker_bytes`.
            let cursor = Cursor::new(maker_bytes, base_offset + start as u64);
            let entries = parse_ifd_entries(&cursor, tiff.endian, header_len);
            for entry in &entries {
                if let Some(decoded) = decode_entry(container_name, &cursor, tiff.endian, entry) {
                    out.push(decoded);
                }
            }
        }
        OffsetMode::Absolute => {
            // IFD count + entries live inside the MakerNote at `header_len`,
            // but value offsets reference the parent file, so we keep the
            // parent cursor for value reads and slice the IFD bytes manually.
            let parent_cursor = Cursor::new(bytes, base_offset);
            let ifd_offset_in_file = start + header_len;
            let entries = parse_ifd_entries(&parent_cursor, tiff.endian, ifd_offset_in_file);
            for entry in &entries {
                if let Some(decoded) =
                    decode_entry(container_name, &parent_cursor, tiff.endian, entry)
                {
                    out.push(decoded);
                }
            }
        }
    }
    out
}

fn decode_entry(
    container_name: &str,
    cursor: &Cursor<'_>,
    endian: Endian,
    entry: &OlympusEntry,
) -> Option<MetadataEntry> {
    let tag_name = tag_name(entry.tag_id);
    let value = match entry.type_id {
        2 => read_ascii(cursor, entry).map(TypedValue::String),
        3 | 4 | 8 | 9 => read_integer(cursor, endian, entry).map(TypedValue::Integer),
        _ => Some(TypedValue::Integer(entry.value_or_offset as i64)),
    }?;
    Some(MetadataEntry {
        namespace: "olympus_makernote".into(),
        tag_id: format!("0x{:04X}", entry.tag_id),
        tag_name,
        value,
        provenance: provenance(container_name),
        notes: Vec::new(),
    })
}

/// Curated starter allowlist of Olympus MakerNote tags. The canonical tag set
/// is large and partially model-dependent — this list is intentionally small
/// and grows as fixtures land. Unknown tags fall through as
/// `OlympusTag_<hex>` so the structural surface stays honest.
fn tag_name(tag_id: u16) -> String {
    match tag_id {
        0x0000 => "MakerNoteVersion".into(),
        0x0200 => "SpecialMode".into(),
        0x0201 => "Quality".into(),
        0x0202 => "Macro".into(),
        0x0203 => "BWMode".into(),
        0x0204 => "DigitalZoom".into(),
        0x0207 => "CameraType".into(),
        0x0208 => "CameraID".into(),
        0x0209 => "EpsonImageWidth".into(),
        0x020B => "EpsonImageHeight".into(),
        0x020C => "EpsonSoftware".into(),
        0x0F00 => "DataDump".into(),
        0x1000 => "ShutterSpeed".into(),
        0x1001 => "ISO".into(),
        0x1002 => "Aperture".into(),
        0x1003 => "Brightness".into(),
        0x1004 => "FlashMode".into(),
        0x1005 => "FlashDevice".into(),
        0x1006 => "Bracket".into(),
        0x100B => "FocusMode".into(),
        0x100C => "FocusDistance".into(),
        0x100D => "Zoom".into(),
        0x101A => "SerialNumber".into(),
        other => format!("OlympusTag_{other:04X}"),
    }
}

fn parse_ifd_entries(cursor: &Cursor<'_>, endian: Endian, ifd_offset: usize) -> Vec<OlympusEntry> {
    let Ok(count) = cursor.read_u16(ifd_offset, endian) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for index in 0..count as usize {
        let offset = ifd_offset + 2 + index * 12;
        if offset + 12 > cursor.len() {
            break;
        }
        let Ok(tag_id) = cursor.read_u16(offset, endian) else {
            break;
        };
        let Ok(type_id) = cursor.read_u16(offset + 2, endian) else {
            break;
        };
        let Ok(count) = cursor.read_u32(offset + 4, endian) else {
            break;
        };
        let Ok(value_or_offset) = cursor.read_u32(offset + 8, endian) else {
            break;
        };
        entries.push(OlympusEntry {
            tag_id,
            type_id,
            count,
            value_or_offset,
        });
    }
    entries
}

fn read_ascii(cursor: &Cursor<'_>, entry: &OlympusEntry) -> Option<String> {
    if entry.type_id != 2 {
        return None;
    }
    let count = entry.count as usize;
    if count == 0 {
        return None;
    }
    if count <= 4 {
        // Inline ASCII would require the raw entry byte slot; the allowlisted
        // ASCII tags in practice exceed 4 bytes so we defer rather than mis-read.
        return None;
    }
    let start = entry.value_or_offset as usize;
    let end = start.checked_add(count)?;
    let raw = cursor.bytes().get(start..end)?;
    let bytes: Vec<u8> = raw.iter().take_while(|&&b| b != 0).copied().collect();
    String::from_utf8(bytes).ok()
}

fn read_integer(cursor: &Cursor<'_>, endian: Endian, entry: &OlympusEntry) -> Option<i64> {
    match entry.type_id {
        3 if entry.count == 1 => Some((entry.value_or_offset & 0xFFFF) as i64),
        4 if entry.count == 1 => Some(entry.value_or_offset as i64),
        8 if entry.count == 1 => Some((entry.value_or_offset as i32) as i64),
        9 if entry.count == 1 => Some((entry.value_or_offset as i32) as i64),
        4 if entry.count > 1 => {
            let offset = entry.value_or_offset as usize;
            cursor.read_u32(offset, endian).ok().map(|v| v as i64)
        }
        _ => None,
    }
}

fn provenance(container_name: &str) -> Provenance {
    Provenance {
        container: container_name.into(),
        namespace: "olympus_makernote".into(),
        path: Some("ifd0_makernote".into()),
        offset_start: None,
        offset_end: None,
        notes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xifty_container_tiff::parse_bytes;

    fn make_entry(value: &str) -> MetadataEntry {
        MetadataEntry {
            namespace: "exif".into(),
            tag_id: "0x010F".into(),
            tag_name: "Make".into(),
            value: TypedValue::String(value.into()),
            provenance: Provenance {
                container: "test".into(),
                namespace: "exif".into(),
                path: None,
                offset_start: None,
                offset_end: None,
                notes: Vec::new(),
            },
            notes: Vec::new(),
        }
    }

    /// Build a minimal little-endian TIFF whose IFD0 carries Make="OLYMPUS"
    /// plus a MakerNote entry whose payload starts with `header` followed by
    /// a sub-IFD with two entries: Quality (0x0201, LONG = 2) and a
    /// fall-through tag 0x1234 (LONG = 99).
    fn build_olympus_tiff(header: &[u8]) -> Vec<u8> {
        // Sub-IFD: count(2) + 2*12 + next(4) = 30
        let sub_ifd: Vec<u8> = {
            let mut buf = Vec::new();
            buf.extend_from_slice(&2u16.to_le_bytes());
            buf.extend_from_slice(&0x0201u16.to_le_bytes());
            buf.extend_from_slice(&4u16.to_le_bytes());
            buf.extend_from_slice(&1u32.to_le_bytes());
            buf.extend_from_slice(&2u32.to_le_bytes());
            buf.extend_from_slice(&0x1234u16.to_le_bytes());
            buf.extend_from_slice(&4u16.to_le_bytes());
            buf.extend_from_slice(&1u32.to_le_bytes());
            buf.extend_from_slice(&99u32.to_le_bytes());
            buf.extend_from_slice(&0u32.to_le_bytes());
            buf
        };

        let mut maker_note = Vec::new();
        maker_note.extend_from_slice(header);
        maker_note.extend_from_slice(&sub_ifd);

        let make_blob = b"OLYMPUS\0";
        // Header(8) + count(2) + 2 entries * 12 + next(4) = 38
        let ifd0_size = 2 + 2 * 12 + 4;
        let make_offset: u32 = 8 + ifd0_size as u32;
        let maker_note_offset: u32 = make_offset + make_blob.len() as u32;

        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"II*\0");
        bytes.extend_from_slice(&8u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&0x010Fu16.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&(make_blob.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&make_offset.to_le_bytes());
        bytes.extend_from_slice(&0x927Cu16.to_le_bytes());
        bytes.extend_from_slice(&7u16.to_le_bytes());
        bytes.extend_from_slice(&(maker_note.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&maker_note_offset.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(make_blob);
        bytes.extend_from_slice(&maker_note);
        bytes
    }

    /// Build the absolute-offset variant: header `OLYMPUS\0II\x03\0`, sub-IFD
    /// inline immediately after the header inside the MakerNote payload, but
    /// any out-of-line value offsets are file-absolute. Our test sub-IFD only
    /// uses inline values, so this just exercises the absolute-mode parser
    /// reading the IFD count + entries at `start + header_len`.
    fn build_olympus_tiff_absolute() -> Vec<u8> {
        build_olympus_tiff(b"OLYMPUS\0II\x03\0")
    }

    #[test]
    fn returns_empty_when_make_is_not_olympus() {
        let bytes = build_olympus_tiff(b"OLYMP\0\x01\0");
        let tiff = parse_bytes(&bytes, 0, "tiff").unwrap();
        let exif = vec![make_entry("Sony")];
        assert!(decode_from_tiff(&bytes, 0, "orf", &tiff, &exif).is_empty());
    }

    #[test]
    fn decodes_quality_with_legacy_8byte_header() {
        let bytes = build_olympus_tiff(b"OLYMP\0\x01\0");
        let tiff = parse_bytes(&bytes, 0, "tiff").unwrap();
        let exif = vec![make_entry("OLYMPUS IMAGING CORP.")];
        let entries = decode_from_tiff(&bytes, 0, "orf", &tiff, &exif);
        assert!(
            entries
                .iter()
                .any(|e| e.namespace == "olympus_makernote" && e.tag_name == "Quality"),
            "expected Quality in {entries:?}",
        );
    }

    #[test]
    fn decodes_quality_with_short_6byte_header() {
        let bytes = build_olympus_tiff(b"OLYMP\0");
        let tiff = parse_bytes(&bytes, 0, "tiff").unwrap();
        let exif = vec![make_entry("OLYMPUS")];
        let entries = decode_from_tiff(&bytes, 0, "orf", &tiff, &exif);
        assert!(
            entries
                .iter()
                .any(|e| e.namespace == "olympus_makernote" && e.tag_name == "Quality"),
            "expected Quality in {entries:?}",
        );
    }

    #[test]
    fn decodes_quality_with_newer_absolute_header() {
        let bytes = build_olympus_tiff_absolute();
        let tiff = parse_bytes(&bytes, 0, "tiff").unwrap();
        let exif = vec![make_entry("OLYMPUS")];
        let entries = decode_from_tiff(&bytes, 0, "orf", &tiff, &exif);
        assert!(
            entries
                .iter()
                .any(|e| e.namespace == "olympus_makernote" && e.tag_name == "Quality"),
            "expected Quality in {entries:?}",
        );
    }

    #[test]
    fn longer_prefix_wins_over_olymp() {
        // Ensure `OLYMP\0` does not shadow `OLYMP\0\x01\0` — if it did, the
        // legacy parser would treat the `\x01\0` as part of the IFD count and
        // mis-read entries. We assert decode succeeds and yields Quality.
        let bytes = build_olympus_tiff(b"OLYMP\0\x01\0");
        let tiff = parse_bytes(&bytes, 0, "tiff").unwrap();
        let exif = vec![make_entry("OLYMPUS")];
        let entries = decode_from_tiff(&bytes, 0, "orf", &tiff, &exif);
        assert!(
            entries.iter().any(|e| e.tag_name == "Quality"),
            "longest prefix must be selected, got {entries:?}",
        );
    }

    #[test]
    fn falls_through_unknown_tag_with_structural_name() {
        let bytes = build_olympus_tiff(b"OLYMP\0\x01\0");
        let tiff = parse_bytes(&bytes, 0, "tiff").unwrap();
        let exif = vec![make_entry("OLYMPUS")];
        let entries = decode_from_tiff(&bytes, 0, "orf", &tiff, &exif);
        assert!(
            entries.iter().any(|e| e.tag_name == "OlympusTag_1234"),
            "expected OlympusTag_1234 in {entries:?}",
        );
    }

    #[test]
    fn returns_empty_when_header_unknown() {
        let bytes = build_olympus_tiff(b"NOPE!!\0\0");
        let tiff = parse_bytes(&bytes, 0, "tiff").unwrap();
        let exif = vec![make_entry("OLYMPUS")];
        assert!(decode_from_tiff(&bytes, 0, "orf", &tiff, &exif).is_empty());
    }
}
