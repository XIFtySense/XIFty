//! Panasonic RW2 container parser (TIFF-shaped, vendor magic byte 0x55).
//!
//! ## Tag-ID collision policy
//!
//! Panasonic RW2 reuses the TIFF on-disk shape (endianness marker + 4-byte
//! magic + 4-byte first-IFD offset, then 12-byte IFD entries) but ships a
//! **private IFD0 tag-numbering scheme**. Tag IDs in the range 0x0001..0x002d
//! are assigned by Panasonic and **collide numerically with standard TIFF /
//! EXIF tag IDs**. Likewise, the standard EXIF Sub-IFD pointer (0x8769) and
//! GPS Sub-IFD pointer (0x8825) **must not** be followed from RW2 IFD0 — in
//! Panasonic's namespace those numeric tag IDs are not "ExifIFD/GPS" pointers
//! and treating them as such would dereference junk offsets.
//!
//! Consequently this crate **never invokes `xifty-meta-exif`** and **does not
//! recurse into 0x8769/0x8825 sub-IFDs**. The container surfaces every IFD0
//! entry as an `Rw2Entry` whose `ifd_name` is `"panasonic_ifd0"`, and downstream
//! consumers (specifically `xifty-meta-panasonic`) interpret them with
//! Panasonic semantics. The CLI is wired so that RW2 bytes never flow through
//! `decode_from_tiff` (the EXIF decoder), `decode_apple_from_tiff`,
//! `decode_sony_from_tiff`, `decode_canon_from_tiff`, or any other decoder
//! that assumes standard TIFF/EXIF tag-ID semantics.
//!
//! Big-endian RW2 has not been observed in the wild; only the little-endian
//! `IIU\0` magic is accepted. Anything else is rejected with
//! [`XiftyError::Parse`] rather than misparsed.

use xifty_core::{ContainerNode, Issue, Severity, XiftyError, issue};
use xifty_source::{Cursor, Endian, SourceBytes};

/// One parsed IFD0 entry from a Panasonic RW2 file. Mirrors `TiffEntry` in
/// shape (12-byte directory entry layout) but is intentionally a distinct type
/// so it cannot be passed to TIFF/EXIF interpreters.
#[derive(Debug, Clone)]
pub struct Rw2Entry {
    pub ifd_name: String,
    pub tag_id: u16,
    pub type_id: u16,
    pub count: u32,
    pub value_or_offset: u32,
    pub value_offset_absolute: Option<u64>,
    pub entry_offset: u64,
}

/// Parsed RW2 container: container nodes, raw IFD0 entries, and parser issues.
#[derive(Debug, Clone)]
pub struct Rw2Container {
    pub endian: Endian,
    pub nodes: Vec<ContainerNode>,
    pub entries: Vec<Rw2Entry>,
    pub issues: Vec<Issue>,
}

/// Parse RW2 bytes. Only the little-endian `IIU\0` magic (byte 2 == 0x55) is
/// accepted. Walks IFD0 once and emits every entry as `Rw2Entry`.
///
/// **Does not** follow standard EXIF (0x8769) or GPS (0x8825) sub-IFD
/// pointers — see the module docs for the tag-ID collision policy.
pub fn parse_bytes(
    bytes: &[u8],
    base_offset: u64,
    root_label: &str,
) -> Result<Rw2Container, XiftyError> {
    let cursor = Cursor::new(bytes, base_offset);
    if cursor.len() < 8 {
        return Err(XiftyError::Parse {
            message: "rw2 payload too small".into(),
        });
    }
    if cursor.slice(0, 2)? != b"II" {
        return Err(XiftyError::Parse {
            message: "rw2 must be little-endian (II); big-endian RW2 is not supported".into(),
        });
    }
    let endian = Endian::Little;
    let magic = cursor.read_u16(2, endian)?;
    if magic != 0x0055 {
        return Err(XiftyError::Parse {
            message: format!("unexpected rw2 magic 0x{magic:04X} (expected 0x0055)"),
        });
    }

    let mut nodes = vec![ContainerNode {
        kind: "container".into(),
        label: root_label.into(),
        offset_start: base_offset,
        offset_end: base_offset + bytes.len() as u64,
        parent_label: None,
    }];
    let mut entries = Vec::new();
    let mut issues = Vec::new();

    let first_ifd = cursor.read_u32(4, endian)? as usize;
    walk_panasonic_ifd0(
        &cursor,
        endian,
        first_ifd,
        &mut nodes,
        &mut entries,
        &mut issues,
        root_label,
    )?;

    Ok(Rw2Container {
        endian,
        nodes,
        entries,
        issues,
    })
}

pub fn parse(source: &SourceBytes) -> Result<Rw2Container, XiftyError> {
    parse_bytes(source.bytes(), 0, "rw2")
}

fn walk_panasonic_ifd0(
    cursor: &Cursor<'_>,
    endian: Endian,
    offset: usize,
    nodes: &mut Vec<ContainerNode>,
    entries: &mut Vec<Rw2Entry>,
    issues: &mut Vec<Issue>,
    root_label: &str,
) -> Result<(), XiftyError> {
    let ifd_name = "panasonic_ifd0";
    if offset == 0 {
        return Ok(());
    }
    if offset + 2 > cursor.len() {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "rw2_ifd_out_of_bounds".into(),
            message: format!("IFD {ifd_name} offset out of bounds"),
            offset: Some(cursor.absolute_offset(offset)),
            context: Some(ifd_name.into()),
        });
        return Ok(());
    }

    let count = cursor.read_u16(offset, endian)? as usize;
    nodes.push(ContainerNode {
        kind: "ifd".into(),
        label: ifd_name.into(),
        offset_start: cursor.absolute_offset(offset),
        offset_end: cursor.absolute_offset(offset + 2 + (count * 12)),
        parent_label: Some(root_label.into()),
    });

    for index in 0..count {
        let entry_offset = offset + 2 + index * 12;
        if entry_offset + 12 > cursor.len() {
            issues.push(issue(
                Severity::Warning,
                "rw2_entry_out_of_bounds",
                format!("entry {index} in {ifd_name} exceeds payload"),
            ));
            break;
        }
        let tag_id = cursor.read_u16(entry_offset, endian)?;
        let type_id = cursor.read_u16(entry_offset + 2, endian)?;
        let value_count = cursor.read_u32(entry_offset + 4, endian)?;
        let value_or_offset = cursor.read_u32(entry_offset + 8, endian)?;
        let byte_len = value_size(type_id).saturating_mul(value_count as usize);
        let value_offset_absolute = if byte_len > 4 {
            let local = value_or_offset as usize;
            if local + byte_len > cursor.len() {
                issues.push(issue(
                    Severity::Warning,
                    "rw2_value_out_of_bounds",
                    format!("tag 0x{tag_id:04X} points outside RW2 payload"),
                ));
                None
            } else {
                Some(cursor.absolute_offset(local))
            }
        } else {
            None
        };

        // Intentionally NO sub-IFD recursion: 0x8769 / 0x8825 are not
        // ExifIFD/GPS pointers in the Panasonic IFD0 namespace.
        entries.push(Rw2Entry {
            ifd_name: ifd_name.into(),
            tag_id,
            type_id,
            count: value_count,
            value_or_offset,
            value_offset_absolute,
            entry_offset: cursor.absolute_offset(entry_offset),
        });
    }
    Ok(())
}

/// TIFF type-id byte width table. Identical to TIFF since the on-disk shape
/// of the directory entries is the same — only the tag-ID semantics differ.
fn value_size(type_id: u16) -> usize {
    match type_id {
        1 | 2 | 7 => 1,
        3 => 2,
        4 | 9 => 4,
        5 | 10 => 8,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal little-endian RW2 with a single IFD0 entry of type 4
    /// (LONG, count=1) carrying `tag` and inline value `value`.
    fn build_single_entry_rw2(tag: u16, value: u32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"IIU\0");
        out.extend_from_slice(&8u32.to_le_bytes()); // IFD0 at offset 8
        out.extend_from_slice(&1u16.to_le_bytes()); // one entry
        out.extend_from_slice(&tag.to_le_bytes());
        out.extend_from_slice(&4u16.to_le_bytes()); // type = LONG
        out.extend_from_slice(&1u32.to_le_bytes()); // count = 1
        out.extend_from_slice(&value.to_le_bytes()); // inline value
        out.extend_from_slice(&0u32.to_le_bytes()); // next IFD = 0
        out
    }

    #[test]
    fn parses_minimal_rw2() {
        let bytes = build_single_entry_rw2(0x0001, 0x00010000);
        let parsed = parse_bytes(&bytes, 0, "rw2").unwrap();
        assert_eq!(parsed.entries.len(), 1);
        assert_eq!(parsed.entries[0].tag_id, 0x0001);
        assert_eq!(parsed.entries[0].ifd_name, "panasonic_ifd0");
    }

    #[test]
    fn rejects_standard_tiff_magic() {
        // Standard TIFF magic 0x002A must NOT be accepted by the RW2 parser.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"II*\0");
        bytes.extend_from_slice(&8u32.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let err = parse_bytes(&bytes, 0, "rw2").unwrap_err();
        assert!(matches!(err, XiftyError::Parse { .. }));
    }

    #[test]
    fn rejects_big_endian_rw2() {
        // Big-endian RW2 has not been observed in the wild; reject explicitly.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"MMU\0");
        bytes.extend_from_slice(&8u32.to_be_bytes());
        bytes.extend_from_slice(&0u16.to_be_bytes());
        bytes.extend_from_slice(&0u32.to_be_bytes());
        let err = parse_bytes(&bytes, 0, "rw2").unwrap_err();
        assert!(matches!(err, XiftyError::Parse { .. }));
    }

    #[test]
    fn walks_ifd0_with_panasonic_tag_ids() {
        // Tag 0x010F means "Make" in standard TIFF / EXIF but is a Panasonic
        // private value here. The container must surface it under
        // `panasonic_ifd0`, not `ifd0`/`exif_ifd`.
        let bytes = build_single_entry_rw2(0x010F, 0x12345678);
        let parsed = parse_bytes(&bytes, 0, "rw2").unwrap();
        assert_eq!(parsed.entries.len(), 1);
        assert_eq!(parsed.entries[0].tag_id, 0x010F);
        assert_eq!(parsed.entries[0].ifd_name, "panasonic_ifd0");
        // The Sub-IFD nodes for `exif_ifd` / `gps_ifd` must not exist.
        assert!(parsed.nodes.iter().all(|n| n.label != "exif_ifd"));
        assert!(parsed.nodes.iter().all(|n| n.label != "gps_ifd"));
    }

    #[test]
    fn does_not_recurse_into_0x8769_or_0x8825() {
        // Even when an entry numerically matches the standard EXIF or GPS
        // sub-IFD pointer tag IDs, the RW2 parser must not follow them.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"IIU\0");
        bytes.extend_from_slice(&8u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes()); // two entries
        // 0x8769 with a junk offset value
        bytes.extend_from_slice(&0x8769u16.to_le_bytes());
        bytes.extend_from_slice(&4u16.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&0xDEADBEEFu32.to_le_bytes());
        // 0x8825 with a junk offset value
        bytes.extend_from_slice(&0x8825u16.to_le_bytes());
        bytes.extend_from_slice(&4u16.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&0xCAFEBABEu32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let parsed = parse_bytes(&bytes, 0, "rw2").unwrap();
        // Two entries surface, no sub-IFD nodes were added, and no error.
        assert_eq!(parsed.entries.len(), 2);
        assert!(parsed.nodes.iter().all(|n| n.label != "exif_ifd"));
        assert!(parsed.nodes.iter().all(|n| n.label != "gps_ifd"));
    }
}
