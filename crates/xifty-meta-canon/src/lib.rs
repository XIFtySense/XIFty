//! Canon MakerNote interpreter (`namespace = "canon"`).
//!
//! Status: **planned** — this crate currently performs only a structural pass
//! over the top-level Canon MakerNote IFD and emits a small set of
//! ASCII/short-scalar entries (CanonImageType, CanonFirmwareVersion,
//! OwnerName, SerialNumber). It does not yet decode CameraInfo IFDs, the
//! footer-anchored offset variants on newer bodies, or any encrypted blocks.
//!
//! Planned tag coverage for the eventual `bounded` set:
//! - `0x0006` CanonImageType (ASCII)
//! - `0x0007` CanonFirmwareVersion (ASCII)
//! - `0x0009` OwnerName (ASCII)
//! - `0x000C` SerialNumber (LONG)
//! - `0x000D` CameraInfo (sub-IFD, body-specific layouts — out of scope here)
//! - `0x0095` LensModel (ASCII)
//! - `0x0096` InternalSerialNumber (ASCII)
//!
//! ## MakerNote header convention
//!
//! Unlike Sony (`SONY DSC \0\0\0`) or Apple (`Apple iOS\0`), Canon's MakerNote
//! has **no leading header bytes** — the EXIF MakerNote tag (0x927C) value
//! points directly at an IFD that uses the parent TIFF's base offset and
//! endianness. Walking it is identical to walking IFD0/ExifIFD with offsets
//! resolved against the file root.

use xifty_container_tiff::TiffContainer;
use xifty_core::{MetadataEntry, Provenance, TypedValue};
use xifty_source::{Cursor, Endian};

#[derive(Debug, Clone, Copy)]
struct CanonEntry {
    tag_id: u16,
    type_id: u16,
    count: u32,
    value_or_offset: u32,
}

/// Decode a Canon MakerNote sub-IFD that lives inside a parent TIFF buffer.
///
/// Returns an empty `Vec` when the EXIF `Make` tag is absent / not Canon, or
/// when the MakerNote tag (`0x927C`) is not present in the parent IFD set.
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
    if !matches!(make, Some(value) if value.trim().to_ascii_lowercase().starts_with("canon")) {
        return Vec::new();
    }

    let Some(maker_note) = tiff.entries.iter().find(|entry| entry.tag_id == 0x927C) else {
        return Vec::new();
    };
    // Canon MakerNote: value_or_offset points directly at an IFD in the parent
    // TIFF buffer (no header bytes preceding the entry count).
    let ifd_offset = maker_note.value_or_offset as usize;
    let cursor = Cursor::new(bytes, base_offset);
    let entries = parse_ifd_entries(&cursor, tiff.endian, ifd_offset);
    let mut out = Vec::new();
    for entry in &entries {
        match entry.tag_id {
            0x0006 => push_ascii(
                &mut out,
                container_name,
                "CanonImageType",
                entry,
                &cursor,
                tiff.endian,
            ),
            0x0007 => push_ascii(
                &mut out,
                container_name,
                "CanonFirmwareVersion",
                entry,
                &cursor,
                tiff.endian,
            ),
            0x0009 => push_ascii(
                &mut out,
                container_name,
                "OwnerName",
                entry,
                &cursor,
                tiff.endian,
            ),
            0x000C => {
                if let Some(value) = read_long_scalar(&cursor, tiff.endian, entry) {
                    out.push(MetadataEntry {
                        namespace: "canon".into(),
                        tag_id: format!("0x{:04X}", entry.tag_id),
                        tag_name: "SerialNumber".into(),
                        value: TypedValue::Integer(value as i64),
                        provenance: provenance(container_name),
                        notes: Vec::new(),
                    });
                }
            }
            0x0095 => push_ascii(
                &mut out,
                container_name,
                "LensModel",
                entry,
                &cursor,
                tiff.endian,
            ),
            0x0096 => push_ascii(
                &mut out,
                container_name,
                "InternalSerialNumber",
                entry,
                &cursor,
                tiff.endian,
            ),
            _ => {}
        }
    }
    out
}

fn parse_ifd_entries(cursor: &Cursor<'_>, endian: Endian, ifd_offset: usize) -> Vec<CanonEntry> {
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
        entries.push(CanonEntry {
            tag_id,
            type_id,
            count,
            value_or_offset,
        });
    }
    entries
}

fn read_ascii(cursor: &Cursor<'_>, _endian: Endian, entry: &CanonEntry) -> Option<String> {
    if entry.type_id != 2 {
        return None;
    }
    let count = entry.count as usize;
    if count == 0 {
        return None;
    }
    if count <= 4 {
        // Inline ASCII (<=4 bytes) is rare for the planned tag set and would
        // require carrying the entry's absolute byte offset just to read it
        // honestly across endianness. Defer until needed.
        return None;
    }
    let start = entry.value_or_offset as usize;
    let end = start.checked_add(count)?;
    let raw = cursor.bytes().get(start..end)?;
    let bytes: Vec<u8> = raw.iter().take_while(|&&b| b != 0).copied().collect();
    String::from_utf8(bytes).ok()
}

fn push_ascii(
    out: &mut Vec<MetadataEntry>,
    container_name: &str,
    tag_name: &str,
    entry: &CanonEntry,
    cursor: &Cursor<'_>,
    endian: Endian,
) {
    let Some(value) = read_ascii(cursor, endian, entry) else {
        return;
    };
    if value.is_empty() {
        return;
    }
    out.push(MetadataEntry {
        namespace: "canon".into(),
        tag_id: format!("0x{:04X}", entry.tag_id),
        tag_name: tag_name.into(),
        value: TypedValue::String(value),
        provenance: provenance(container_name),
        notes: Vec::new(),
    });
}

fn read_long_scalar(cursor: &Cursor<'_>, endian: Endian, entry: &CanonEntry) -> Option<u32> {
    if (entry.type_id != 4 && entry.type_id != 3) || entry.count != 1 {
        return None;
    }
    // Inline value: the value_or_offset field already holds the scalar in
    // file endianness (cursor.read_u32 normalized it).
    let _ = (cursor, endian);
    Some(entry.value_or_offset)
}

fn provenance(container_name: &str) -> Provenance {
    Provenance {
        container: container_name.into(),
        namespace: "canon".into(),
        path: Some("canon_makernote".into()),
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

    /// Build a minimal little-endian TIFF carrying:
    /// - IFD0 with Make="Canon" (0x010F) and MakerNote=0x927C pointing to a
    ///   plain Canon-style sub-IFD with one ASCII entry (CanonImageType).
    fn build_canon_tiff() -> Vec<u8> {
        // Layout:
        //   0..8   header (II*\0 + IFD0 offset = 8)
        //   8..    IFD0: 2 entries (count + 2*12 + 4 = 30 bytes), then data
        let mut out = Vec::new();
        out.extend_from_slice(b"II*\0");
        out.extend_from_slice(&8u32.to_le_bytes());

        // IFD0: 2 entries
        let make_str = b"Canon\0";
        let make_offset: u32 = 8 + 2 + 2 * 12 + 4; // == 38
        let canon_image_type = b"Canon EOS Test\0";
        // Canon MakerNote sub-IFD lives just after the Make ASCII blob.
        let maker_ifd_offset = make_offset + make_str.len() as u32; // == 44
        // Sub-IFD: count(2) + 1*12 + next(4) = 18 bytes
        let sub_data_offset = maker_ifd_offset + 18;

        out.extend_from_slice(&2u16.to_le_bytes()); // 2 IFD0 entries
        // Make tag 0x010F, ASCII (type 2), count = 6, offset
        out.extend_from_slice(&0x010Fu16.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&(make_str.len() as u32).to_le_bytes());
        out.extend_from_slice(&make_offset.to_le_bytes());
        // MakerNote tag 0x927C, UNDEFINED (type 7), count covers sub-IFD
        // bytes + the trailing CanonImageType blob, value_or_offset points
        // at the sub-IFD start.
        let maker_count: u32 = 18 + canon_image_type.len() as u32;
        out.extend_from_slice(&0x927Cu16.to_le_bytes());
        out.extend_from_slice(&7u16.to_le_bytes());
        out.extend_from_slice(&maker_count.to_le_bytes());
        out.extend_from_slice(&maker_ifd_offset.to_le_bytes());
        // next IFD = 0
        out.extend_from_slice(&0u32.to_le_bytes());

        // Make blob
        out.extend_from_slice(make_str);

        // Canon MakerNote sub-IFD: 1 entry — CanonImageType (0x0006), ASCII
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&0x0006u16.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&(canon_image_type.len() as u32).to_le_bytes());
        out.extend_from_slice(&sub_data_offset.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // next IFD = 0

        // Trailing ASCII blob for CanonImageType
        out.extend_from_slice(canon_image_type);
        out
    }

    #[test]
    fn returns_empty_when_make_is_not_canon() {
        let bytes = build_canon_tiff();
        let tiff = parse_bytes(&bytes, 0, "tiff").unwrap();
        let exif = vec![make_entry("Sony")];
        let entries = decode_from_tiff(&bytes, 0, "tiff", &tiff, &exif);
        assert!(entries.is_empty());
    }

    #[test]
    fn decodes_top_level_canon_image_type() {
        let bytes = build_canon_tiff();
        let tiff = parse_bytes(&bytes, 0, "tiff").unwrap();
        let exif = vec![make_entry("Canon")];
        let entries = decode_from_tiff(&bytes, 0, "tiff", &tiff, &exif);
        assert!(
            entries
                .iter()
                .any(|e| e.namespace == "canon" && e.tag_name == "CanonImageType"),
            "expected CanonImageType in {entries:?}",
        );
    }

    #[test]
    fn returns_empty_when_makernote_tag_absent() {
        // Build a Canon-Make TIFF but with no MakerNote tag at all.
        let mut out = Vec::new();
        out.extend_from_slice(b"II*\0");
        out.extend_from_slice(&8u32.to_le_bytes());
        let make_str = b"Canon\0";
        let make_offset: u32 = 8 + 2 + 1 * 12 + 4; // == 26
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&0x010Fu16.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&(make_str.len() as u32).to_le_bytes());
        out.extend_from_slice(&make_offset.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(make_str);
        let tiff = parse_bytes(&out, 0, "tiff").unwrap();
        let exif = vec![make_entry("Canon")];
        let entries = decode_from_tiff(&out, 0, "tiff", &tiff, &exif);
        assert!(entries.is_empty());
    }
}
