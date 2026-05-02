//! Panasonic RW2 IFD0 metadata decoder (`namespace = "panasonic"`).
//!
//! ## Tag-ID collision policy
//!
//! Panasonic RW2 IFD0 tag IDs in the range 0x0001..0x002d (and selectively
//! beyond) **collide numerically with standard TIFF / EXIF tag IDs** but
//! carry entirely different semantics. For example, 0x010F is "Make" in
//! standard TIFF/EXIF but is a Panasonic-private value in RW2 IFD0. Routing
//! RW2 IFD0 entries through `xifty-meta-exif::decode_from_tiff` (or any of
//! the vendor MakerNote decoders that consume EXIF context) would publish
//! garbage under the `exif` / `apple` / `sony` / `canon` / `fuji` /
//! `olympus_makernote` namespaces.
//!
//! Therefore:
//!
//! - This crate consumes `xifty_container_rw2::Rw2Container` records
//!   directly — never `xifty_container_tiff::TiffContainer`.
//! - Every emitted [`MetadataEntry`] carries `namespace = "panasonic"`. There
//!   is no overlap with the `exif` namespace by construction.
//! - The CLI extract path for `Format::Rw2` MUST route through this decoder
//!   only — never through `decode_from_tiff` or any TIFF/EXIF MakerNote
//!   decoder. See `xifty-cli/src/lib.rs::rw2_extract`.
//!
//! ## Bounded tag set
//!
//! Status: **bounded**. Panasonic's full RW2 tag dictionary is large and
//! partially model-dependent; this decoder ships a curated starter set.
//! Tags outside the allowlist surface structurally as `PanasonicTag_<hex>`
//! so the namespace remains honest about what the file actually carries.
//!
//! Currently decoded tags:
//!
//! | Tag    | Name                  |
//! | ------ | --------------------- |
//! | 0x0001 | PanasonicRawVersion   |
//! | 0x0002 | SensorWidth           |
//! | 0x0003 | SensorHeight          |
//! | 0x0004 | SensorTopBorder       |
//! | 0x0005 | SensorLeftBorder      |
//! | 0x0006 | SensorBottomBorder    |
//! | 0x0007 | SensorRightBorder     |
//! | 0x0009 | CFAPattern            |
//! | 0x000A | BlackLevelRed         |
//! | 0x0017 | ISO                   |
//! | 0x0024 | WBRedLevel            |
//! | 0x0025 | WBGreenLevel          |
//! | 0x0026 | WBBlueLevel           |
//! | 0x002E | JpgFromRaw            |
//!
//! ## EXIF Sub-IFD
//!
//! Real-world RW2 files may carry an embedded EXIF Sub-IFD reachable via a
//! Panasonic-specified pointer (not via IFD0's 0x8769, which in Panasonic's
//! IFD0 namespace does NOT mean "ExifIFD"). Surfacing such an embedded EXIF
//! block is intentionally out of scope for this [S] task; the decoder
//! presently exposes Panasonic IFD0 only.

use xifty_container_rw2::{Rw2Container, Rw2Entry};
use xifty_core::{MetadataEntry, Provenance, TypedValue};
use xifty_source::{Cursor, Endian};

/// Decode bounded Panasonic IFD0 tags from a parsed RW2 container.
///
/// `bytes` must be the exact buffer that was passed to
/// [`xifty_container_rw2::parse_bytes`], and `base_offset` must match.
/// Every entry in the returned vector has `namespace == "panasonic"`.
pub fn decode_from_rw2(
    bytes: &[u8],
    base_offset: u64,
    container_name: &str,
    rw2: &Rw2Container,
) -> Vec<MetadataEntry> {
    let cursor = Cursor::new(bytes, base_offset);
    let endian = rw2.endian;
    let mut out = Vec::with_capacity(rw2.entries.len());
    for entry in &rw2.entries {
        if let Some(decoded) = decode_entry(container_name, &cursor, endian, entry) {
            out.push(decoded);
        }
    }
    out
}

fn decode_entry(
    container_name: &str,
    cursor: &Cursor<'_>,
    endian: Endian,
    entry: &Rw2Entry,
) -> Option<MetadataEntry> {
    let tag_name = tag_name(entry.tag_id);
    let value = match entry.type_id {
        2 => read_ascii(cursor, entry).map(TypedValue::String),
        3 | 4 | 8 | 9 => read_integer(cursor, endian, entry).map(TypedValue::Integer),
        7 => Some(TypedValue::Bytes(read_undefined(cursor, entry))),
        _ => Some(TypedValue::Integer(entry.value_or_offset as i64)),
    }?;
    Some(MetadataEntry {
        namespace: "panasonic".into(),
        tag_id: format!("0x{:04X}", entry.tag_id),
        tag_name,
        value,
        provenance: provenance(container_name),
        notes: Vec::new(),
    })
}

/// Curated bounded Panasonic tag allowlist. Unknown tags fall through as
/// `PanasonicTag_<hex>` so the structural surface stays honest. Adding new
/// tags is additive — never remove or repurpose an existing mapping without
/// a normalization-aware migration.
fn tag_name(tag_id: u16) -> String {
    match tag_id {
        0x0001 => "PanasonicRawVersion".into(),
        0x0002 => "SensorWidth".into(),
        0x0003 => "SensorHeight".into(),
        0x0004 => "SensorTopBorder".into(),
        0x0005 => "SensorLeftBorder".into(),
        0x0006 => "SensorBottomBorder".into(),
        0x0007 => "SensorRightBorder".into(),
        0x0009 => "CFAPattern".into(),
        0x000A => "BlackLevelRed".into(),
        0x0017 => "ISO".into(),
        0x0024 => "WBRedLevel".into(),
        0x0025 => "WBGreenLevel".into(),
        0x0026 => "WBBlueLevel".into(),
        0x002E => "JpgFromRaw".into(),
        other => format!("PanasonicTag_{other:04X}"),
    }
}

fn read_ascii(cursor: &Cursor<'_>, entry: &Rw2Entry) -> Option<String> {
    if entry.type_id != 2 {
        return None;
    }
    let count = entry.count as usize;
    if count == 0 {
        return None;
    }
    if count <= 4 {
        // Inline ASCII would require the raw entry byte slot; defer rather
        // than mis-read since allowlisted ASCII tags in practice exceed 4 b.
        return None;
    }
    let start = entry.value_or_offset as usize;
    let end = start.checked_add(count)?;
    let raw = cursor.bytes().get(start..end)?;
    let bytes: Vec<u8> = raw.iter().take_while(|&&b| b != 0).copied().collect();
    String::from_utf8(bytes).ok()
}

fn read_integer(cursor: &Cursor<'_>, endian: Endian, entry: &Rw2Entry) -> Option<i64> {
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

fn read_undefined(cursor: &Cursor<'_>, entry: &Rw2Entry) -> Vec<u8> {
    let count = entry.count as usize;
    if count == 0 {
        return Vec::new();
    }
    if count <= 4 {
        let bytes = entry.value_or_offset.to_le_bytes();
        return bytes[..count].to_vec();
    }
    let start = entry.value_or_offset as usize;
    let Some(end) = start.checked_add(count) else {
        return Vec::new();
    };
    cursor
        .bytes()
        .get(start..end)
        .map(<[u8]>::to_vec)
        .unwrap_or_default()
}

fn provenance(container_name: &str) -> Provenance {
    Provenance {
        container: container_name.into(),
        namespace: "panasonic".into(),
        path: Some("panasonic_ifd0".into()),
        offset_start: None,
        offset_end: None,
        notes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xifty_container_rw2::parse_bytes;

    /// Build a minimal little-endian RW2 with a single LONG (type=4, count=1)
    /// IFD0 entry carrying `tag` and inline `value`.
    fn build_single_entry_rw2(tag: u16, value: u32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"IIU\0");
        out.extend_from_slice(&8u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&tag.to_le_bytes());
        out.extend_from_slice(&4u16.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&value.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out
    }

    /// Build an RW2 with one UNDEFINED (type=7) IFD0 entry carrying `payload`.
    /// Payloads of <=4 bytes are placed inline; longer payloads are stored
    /// out-of-line directly after the IFD directory.
    fn build_single_undefined_entry(tag: u16, payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"IIU\0");
        out.extend_from_slice(&8u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&tag.to_le_bytes());
        out.extend_from_slice(&7u16.to_le_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        if payload.len() <= 4 {
            let mut padded = [0u8; 4];
            padded[..payload.len()].copy_from_slice(payload);
            out.extend_from_slice(&padded);
            out.extend_from_slice(&0u32.to_le_bytes());
        } else {
            // header(8) + count(2) + entry(12) + next(4) = 26
            let payload_off: u32 = 26;
            out.extend_from_slice(&payload_off.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(payload);
        }
        out
    }

    #[test]
    fn decodes_panasonic_raw_version_tag_inline() {
        // PanasonicRawVersion is conventionally a 4-byte payload, so it lives
        // inline in the entry's value field rather than at an out-of-line offset.
        let payload = b"0001";
        let bytes = build_single_undefined_entry(0x0001, payload);
        let rw2 = parse_bytes(&bytes, 0, "rw2").unwrap();
        let entries = decode_from_rw2(&bytes, 0, "rw2", &rw2);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].tag_name, "PanasonicRawVersion");
        assert_eq!(entries[0].namespace, "panasonic");
        assert_eq!(entries[0].value, TypedValue::Bytes(payload.to_vec()));
    }

    #[test]
    fn decodes_panasonic_raw_version_tag_out_of_line() {
        // Longer UNDEFINED payloads round-trip via the out-of-line offset path.
        let payload = b"longer-version-string";
        let bytes = build_single_undefined_entry(0x0001, payload);
        let rw2 = parse_bytes(&bytes, 0, "rw2").unwrap();
        let entries = decode_from_rw2(&bytes, 0, "rw2", &rw2);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].value, TypedValue::Bytes(payload.to_vec()));
    }

    #[test]
    fn decodes_sensor_dimensions_tags() {
        // SensorWidth (0x0002 = 4096) and SensorHeight (0x0003 = 3072) live
        // in two entries. Build with type=4 (LONG) inline.
        let mut out = Vec::new();
        out.extend_from_slice(b"IIU\0");
        out.extend_from_slice(&8u32.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&0x0002u16.to_le_bytes());
        out.extend_from_slice(&4u16.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&4096u32.to_le_bytes());
        out.extend_from_slice(&0x0003u16.to_le_bytes());
        out.extend_from_slice(&4u16.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&3072u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());

        let rw2 = parse_bytes(&out, 0, "rw2").unwrap();
        let entries = decode_from_rw2(&out, 0, "rw2", &rw2);
        let width = entries
            .iter()
            .find(|e| e.tag_name == "SensorWidth")
            .expect("SensorWidth present");
        let height = entries
            .iter()
            .find(|e| e.tag_name == "SensorHeight")
            .expect("SensorHeight present");
        assert_eq!(width.value, TypedValue::Integer(4096));
        assert_eq!(height.value, TypedValue::Integer(3072));
        assert!(entries.iter().all(|e| e.namespace == "panasonic"));
    }

    #[test]
    fn entries_use_panasonic_namespace() {
        // Regression for the tag-ID collision policy: tag 0x010F is "Make"
        // in standard TIFF / EXIF, but in Panasonic RW2 IFD0 it is a
        // Panasonic-private value. It MUST land in `panasonic`, never `exif`.
        let bytes = build_single_entry_rw2(0x010F, 0x12345678);
        let rw2 = parse_bytes(&bytes, 0, "rw2").unwrap();
        let entries = decode_from_rw2(&bytes, 0, "rw2", &rw2);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].namespace, "panasonic");
        assert_ne!(entries[0].namespace, "exif");
        // Tag is not in the allowlist so it falls through structurally.
        assert_eq!(entries[0].tag_name, "PanasonicTag_010F");
        assert_eq!(entries[0].tag_id, "0x010F");
    }

    #[test]
    fn unknown_tag_falls_through_with_structural_name() {
        let bytes = build_single_entry_rw2(0x4242, 7);
        let rw2 = parse_bytes(&bytes, 0, "rw2").unwrap();
        let entries = decode_from_rw2(&bytes, 0, "rw2", &rw2);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].tag_name, "PanasonicTag_4242");
        assert_eq!(entries[0].value, TypedValue::Integer(7));
    }
}
