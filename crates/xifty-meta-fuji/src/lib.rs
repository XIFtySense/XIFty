//! Fuji MakerNote interpreter (`namespace = "fuji"`).
//!
//! Status: **bounded** — the decoder parses the top-level Fuji MakerNote
//! sub-IFD and emits a curated allowlist of well-understood tags as named
//! entries. Tags outside the allowlist are surfaced structurally as
//! `FujiTag_<hex>` so probe/extract output reflects what is actually present
//! without inventing semantic meaning.
//!
//! ## MakerNote header
//!
//! Fuji's MakerNote (EXIF tag `0x927C`) begins with the 8-byte ASCII signature
//! `b"FUJIFILM"`, followed by a little-endian u32 holding the byte offset to
//! the sub-IFD **relative to the MakerNote start** (not the file root). This
//! local-base addressing differs from Canon (file-relative) and matches
//! Sony's encrypted variants, so all sub-IFD reads in this module use a
//! locally-rebased view of the MakerNote bytes.
//!
//! IFD entries themselves use the parent TIFF's endianness; only the Fuji
//! MakerNote header's offset field is fixed little-endian per Fuji's spec.

use xifty_container_tiff::TiffContainer;
use xifty_core::{MetadataEntry, Provenance, TypedValue};
use xifty_source::{Cursor, Endian};

const FUJI_MAKER_NOTE_HEADER: &[u8] = b"FUJIFILM";

#[derive(Debug, Clone, Copy)]
struct FujiEntry {
    tag_id: u16,
    type_id: u16,
    count: u32,
    value_or_offset: u32,
}

/// Decode a Fuji MakerNote sub-IFD inside a parent TIFF buffer.
///
/// Returns an empty `Vec` when EXIF `Make` is absent / not Fuji or when the
/// MakerNote tag is not present.
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
        Some(value) if value.trim().to_ascii_uppercase().starts_with("FUJIFILM")
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
    if !maker_bytes.starts_with(FUJI_MAKER_NOTE_HEADER) {
        return Vec::new();
    }
    if maker_bytes.len() < FUJI_MAKER_NOTE_HEADER.len() + 4 {
        return Vec::new();
    }

    // The 4-byte LE u32 at offset +8 of the MakerNote payload is the sub-IFD
    // offset, **relative to the MakerNote start** (not the file). We build a
    // locally-based cursor so all subsequent IFD reads stay correctly bounded.
    let local_ifd_offset = u32::from_le_bytes([
        maker_bytes[8],
        maker_bytes[9],
        maker_bytes[10],
        maker_bytes[11],
    ]) as usize;
    let cursor = Cursor::new(maker_bytes, base_offset + start as u64);

    let entries = parse_ifd_entries(&cursor, tiff.endian, local_ifd_offset);
    let mut out = Vec::new();
    for entry in &entries {
        if let Some(decoded) = decode_entry(container_name, &cursor, tiff.endian, entry) {
            out.push(decoded);
        }
    }
    out
}

fn decode_entry(
    container_name: &str,
    cursor: &Cursor<'_>,
    endian: Endian,
    entry: &FujiEntry,
) -> Option<MetadataEntry> {
    let tag_name = tag_name(entry.tag_id);
    let value = match entry.type_id {
        2 => read_ascii(cursor, entry).map(TypedValue::String),
        3 | 4 | 8 | 9 => read_integer(cursor, endian, entry).map(TypedValue::Integer),
        _ => Some(TypedValue::Integer(entry.value_or_offset as i64)),
    }?;
    Some(MetadataEntry {
        namespace: "fuji".into(),
        tag_id: format!("0x{:04X}", entry.tag_id),
        tag_name,
        value,
        provenance: provenance(container_name),
        notes: Vec::new(),
    })
}

/// Allowlist of Fuji MakerNote tags decoded with semantic names. Anything not
/// listed here is emitted as `FujiTag_<hex>` so the structural pass still
/// surfaces it without inventing meaning. The selection covers tags that are
/// stable across Fuji bodies and well-understood in public references.
fn tag_name(tag_id: u16) -> String {
    match tag_id {
        0x1000 => "Quality".into(),
        0x1001 => "Sharpness".into(),
        0x1002 => "WhiteBalance".into(),
        0x1003 => "Saturation".into(),
        0x1004 => "Contrast".into(),
        0x1010 => "FlashMode".into(),
        0x1011 => "FlashStrength".into(),
        0x1020 => "Macro".into(),
        0x1021 => "FocusMode".into(),
        0x1030 => "SlowSync".into(),
        0x1031 => "PictureMode".into(),
        0x1100 => "ContinuousBracket".into(),
        0x1400 => "DynamicRange".into(),
        0x1401 => "FilmMode".into(),
        other => format!("FujiTag_{other:04X}"),
    }
}

fn parse_ifd_entries(cursor: &Cursor<'_>, endian: Endian, ifd_offset: usize) -> Vec<FujiEntry> {
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
        entries.push(FujiEntry {
            tag_id,
            type_id,
            count,
            value_or_offset,
        });
    }
    entries
}

fn read_ascii(cursor: &Cursor<'_>, entry: &FujiEntry) -> Option<String> {
    if entry.type_id != 2 {
        return None;
    }
    let count = entry.count as usize;
    if count == 0 {
        return None;
    }
    let raw = if count <= 4 {
        // Inline ASCII lives in the value_or_offset field, byte-ordered as
        // the file's endianness wrote it. Pull from the entry slot directly
        // would require carrying the entry offset; instead, since Fuji ASCII
        // tags in the allowlist are all > 4 bytes in practice, defer.
        return None;
    } else {
        let start = entry.value_or_offset as usize;
        let end = start.checked_add(count)?;
        cursor.bytes().get(start..end)?
    };
    let bytes: Vec<u8> = raw.iter().take_while(|&&b| b != 0).copied().collect();
    String::from_utf8(bytes).ok()
}

fn read_integer(cursor: &Cursor<'_>, endian: Endian, entry: &FujiEntry) -> Option<i64> {
    match entry.type_id {
        3 if entry.count == 1 => {
            // Inline u16 — value_or_offset already normalized to file endianness.
            Some((entry.value_or_offset & 0xFFFF) as i64)
        }
        4 if entry.count == 1 => Some(entry.value_or_offset as i64),
        8 if entry.count == 1 => Some((entry.value_or_offset as i32) as i64),
        9 if entry.count == 1 => Some((entry.value_or_offset as i32) as i64),
        // Multi-element: dereference offset for type 4 LONG arrays where
        // the byte length forces an out-of-line read.
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
        namespace: "fuji".into(),
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

    /// Build a TIFF whose IFD0 carries Make="FUJIFILM" plus a MakerNote
    /// containing a Fuji header + sub-IFD with two allowlisted tags
    /// (Quality=2 LONG, PictureMode=3 LONG) and one fall-through tag
    /// (0x1234 LONG) so the structural pass is exercised.
    fn build_fuji_tiff() -> Vec<u8> {
        // Sub-IFD layout (3 entries): count(2) + 3*12 + next(4) = 42 bytes.
        let sub_ifd: Vec<u8> = {
            let mut buf = Vec::new();
            buf.extend_from_slice(&3u16.to_le_bytes());
            // 0x1000 Quality, type=4 LONG, count=1, value=2
            buf.extend_from_slice(&0x1000u16.to_le_bytes());
            buf.extend_from_slice(&4u16.to_le_bytes());
            buf.extend_from_slice(&1u32.to_le_bytes());
            buf.extend_from_slice(&2u32.to_le_bytes());
            // 0x1031 PictureMode, type=4 LONG, count=1, value=3
            buf.extend_from_slice(&0x1031u16.to_le_bytes());
            buf.extend_from_slice(&4u16.to_le_bytes());
            buf.extend_from_slice(&1u32.to_le_bytes());
            buf.extend_from_slice(&3u32.to_le_bytes());
            // 0x1234 fall-through, type=4 LONG, count=1, value=99
            buf.extend_from_slice(&0x1234u16.to_le_bytes());
            buf.extend_from_slice(&4u16.to_le_bytes());
            buf.extend_from_slice(&1u32.to_le_bytes());
            buf.extend_from_slice(&99u32.to_le_bytes());
            buf.extend_from_slice(&0u32.to_le_bytes());
            buf
        };

        // Fuji MakerNote bytes: header(8) + LE u32 sub-IFD offset(4) + sub_ifd
        let mut maker_note = Vec::new();
        maker_note.extend_from_slice(FUJI_MAKER_NOTE_HEADER);
        let sub_ifd_local_offset: u32 = 12; // immediately after header+offset
        maker_note.extend_from_slice(&sub_ifd_local_offset.to_le_bytes());
        maker_note.extend_from_slice(&sub_ifd);

        // Now build a TIFF: IFD0 has Make + MakerNote (2 entries) and
        // points the MakerNote at an absolute offset where maker_note lives.
        let make_blob = b"FUJIFILM\0";
        // Header(8) + count(2) + 2*12 + next(4) = 38
        let ifd0_size = 2 + 2 * 12 + 4;
        let make_offset: u32 = 8 + ifd0_size as u32; // = 38
        let maker_note_offset: u32 = make_offset + make_blob.len() as u32;

        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"II*\0");
        bytes.extend_from_slice(&8u32.to_le_bytes()); // IFD0 at 8
        bytes.extend_from_slice(&2u16.to_le_bytes()); // 2 entries
        // Make 0x010F ASCII
        bytes.extend_from_slice(&0x010Fu16.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&(make_blob.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&make_offset.to_le_bytes());
        // MakerNote 0x927C UNDEFINED
        bytes.extend_from_slice(&0x927Cu16.to_le_bytes());
        bytes.extend_from_slice(&7u16.to_le_bytes());
        bytes.extend_from_slice(&(maker_note.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&maker_note_offset.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes()); // next IFD = 0
        bytes.extend_from_slice(make_blob);
        bytes.extend_from_slice(&maker_note);
        bytes
    }

    #[test]
    fn returns_empty_when_make_is_not_fuji() {
        let bytes = build_fuji_tiff();
        let tiff = parse_bytes(&bytes, 0, "tiff").unwrap();
        let exif = vec![make_entry("Sony")];
        let entries = decode_from_tiff(&bytes, 0, "raf_exif", &tiff, &exif);
        assert!(entries.is_empty());
    }

    #[test]
    fn decodes_allowlisted_quality_tag() {
        let bytes = build_fuji_tiff();
        let tiff = parse_bytes(&bytes, 0, "tiff").unwrap();
        let exif = vec![make_entry("FUJIFILM")];
        let entries = decode_from_tiff(&bytes, 0, "raf_exif", &tiff, &exif);
        assert!(
            entries
                .iter()
                .any(|e| e.namespace == "fuji" && e.tag_name == "Quality"),
            "expected Quality in {entries:?}",
        );
    }

    #[test]
    fn decodes_allowlisted_picture_mode_tag() {
        let bytes = build_fuji_tiff();
        let tiff = parse_bytes(&bytes, 0, "tiff").unwrap();
        let exif = vec![make_entry("FUJIFILM")];
        let entries = decode_from_tiff(&bytes, 0, "raf_exif", &tiff, &exif);
        assert!(
            entries
                .iter()
                .any(|e| e.namespace == "fuji" && e.tag_name == "PictureMode"),
            "expected PictureMode in {entries:?}",
        );
    }

    #[test]
    fn falls_through_unknown_tag_with_structural_name() {
        let bytes = build_fuji_tiff();
        let tiff = parse_bytes(&bytes, 0, "tiff").unwrap();
        let exif = vec![make_entry("FUJIFILM")];
        let entries = decode_from_tiff(&bytes, 0, "raf_exif", &tiff, &exif);
        assert!(
            entries
                .iter()
                .any(|e| e.namespace == "fuji" && e.tag_name == "FujiTag_1234"),
            "expected FujiTag_1234 in {entries:?}",
        );
    }

    #[test]
    fn returns_empty_when_makernote_header_missing() {
        // Build a Fuji-Make TIFF whose MakerNote does NOT start with "FUJIFILM".
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"II*\0");
        bytes.extend_from_slice(&8u32.to_le_bytes());
        let make_blob = b"FUJIFILM\0";
        let ifd0_size = 2 + 2 * 12 + 4;
        let make_offset: u32 = 8 + ifd0_size as u32;
        let bogus_maker = b"NOT_FUJI_HEADER\0\0\0\0";
        let maker_offset: u32 = make_offset + make_blob.len() as u32;
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&0x010Fu16.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&(make_blob.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&make_offset.to_le_bytes());
        bytes.extend_from_slice(&0x927Cu16.to_le_bytes());
        bytes.extend_from_slice(&7u16.to_le_bytes());
        bytes.extend_from_slice(&(bogus_maker.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&maker_offset.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(make_blob);
        bytes.extend_from_slice(bogus_maker);
        let tiff = parse_bytes(&bytes, 0, "tiff").unwrap();
        let exif = vec![make_entry("FUJIFILM")];
        let entries = decode_from_tiff(&bytes, 0, "raf_exif", &tiff, &exif);
        assert!(entries.is_empty());
    }
}
