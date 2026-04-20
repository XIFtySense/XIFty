use xifty_core::{ContainerNode, Issue, Severity, XiftyError, issue};
use xifty_source::{Cursor, Endian, SourceBytes};

#[derive(Debug, Clone)]
pub struct TiffEntry {
    pub ifd_name: String,
    pub tag_id: u16,
    pub type_id: u16,
    pub count: u32,
    pub value_or_offset: u32,
    pub value_offset_absolute: Option<u64>,
    pub entry_offset: u64,
}

#[derive(Debug, Clone)]
pub struct TiffContainer {
    pub endian: Endian,
    pub nodes: Vec<ContainerNode>,
    pub entries: Vec<TiffEntry>,
    pub issues: Vec<Issue>,
}

pub fn parse_bytes(
    bytes: &[u8],
    base_offset: u64,
    root_label: &str,
) -> Result<TiffContainer, XiftyError> {
    let cursor = Cursor::new(bytes, base_offset);
    if cursor.len() < 8 {
        return Err(XiftyError::Parse {
            message: "tiff payload too small".into(),
        });
    }
    let endian = match cursor.slice(0, 2)? {
        b"II" => Endian::Little,
        b"MM" => Endian::Big,
        _ => {
            return Err(XiftyError::Parse {
                message: "invalid tiff endianness marker".into(),
            });
        }
    };
    let magic = cursor.read_u16(2, endian)?;
    if magic != 42 {
        return Err(XiftyError::Parse {
            message: format!("unexpected tiff magic {magic}"),
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
    walk_ifd(
        &cursor,
        endian,
        first_ifd,
        "ifd0",
        &mut nodes,
        &mut entries,
        &mut issues,
        root_label,
    )?;

    Ok(TiffContainer {
        endian,
        nodes,
        entries,
        issues,
    })
}

pub fn parse(source: &SourceBytes) -> Result<TiffContainer, XiftyError> {
    parse_bytes(source.bytes(), 0, "tiff")
}

fn walk_ifd(
    cursor: &Cursor<'_>,
    endian: Endian,
    offset: usize,
    ifd_name: &str,
    nodes: &mut Vec<ContainerNode>,
    entries: &mut Vec<TiffEntry>,
    issues: &mut Vec<Issue>,
    root_label: &str,
) -> Result<(), XiftyError> {
    if offset == 0 {
        return Ok(());
    }
    if offset + 2 > cursor.len() {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "tiff_ifd_out_of_bounds".into(),
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

    let mut exif_ifd_offset = None;
    let mut gps_ifd_offset = None;
    for index in 0..count {
        let entry_offset = offset + 2 + index * 12;
        if entry_offset + 12 > cursor.len() {
            issues.push(issue(
                Severity::Warning,
                "tiff_entry_out_of_bounds",
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
                    "tiff_value_out_of_bounds",
                    format!("tag 0x{tag_id:04X} points outside TIFF payload"),
                ));
                None
            } else {
                Some(cursor.absolute_offset(local))
            }
        } else {
            None
        };

        if tag_id == 0x8769 {
            exif_ifd_offset = Some(value_or_offset as usize);
        }
        if tag_id == 0x8825 {
            gps_ifd_offset = Some(value_or_offset as usize);
        }
        entries.push(TiffEntry {
            ifd_name: ifd_name.into(),
            tag_id,
            type_id,
            count: value_count,
            value_or_offset,
            value_offset_absolute,
            entry_offset: cursor.absolute_offset(entry_offset),
        });
    }

    if let Some(exif_offset) = exif_ifd_offset {
        walk_ifd(
            cursor,
            endian,
            exif_offset,
            "exif_ifd",
            nodes,
            entries,
            issues,
            root_label,
        )?;
    }
    if let Some(gps_offset) = gps_ifd_offset {
        walk_ifd(
            cursor, endian, gps_offset, "gps_ifd", nodes, entries, issues, root_label,
        )?;
    }
    Ok(())
}

/// Returns the absolute byte offset and a borrowed slice for the first IFD
/// entry in `tiff` whose tag id matches `tag_id`. The returned slice is taken
/// from `bytes`, which must be the same byte buffer that was passed to
/// [`parse_bytes`] when producing `tiff`.
///
/// Only out-of-line (> 4 byte) payloads are supported; inline values return
/// `None`. In practice XMP / ICC / IPTC payloads are always out-of-line, so
/// this matches production usage. Returns `None` when the resolved slice
/// would exceed `bytes.len()` — never panics.
fn payload_for_tag<'a>(
    bytes: &'a [u8],
    tiff: &TiffContainer,
    tag_id: u16,
) -> Option<(u64, &'a [u8])> {
    let base_offset = tiff
        .nodes
        .first()
        .map(|node| node.offset_start)
        .unwrap_or(0);
    let entry = tiff.entries.iter().find(|entry| entry.tag_id == tag_id)?;
    let byte_len = value_size(entry.type_id).saturating_mul(entry.count as usize);
    if byte_len == 0 || byte_len <= 4 {
        return None;
    }
    let absolute = entry.value_offset_absolute?;
    let local = absolute.checked_sub(base_offset)?;
    let local = usize::try_from(local).ok()?;
    let end = local.checked_add(byte_len)?;
    let slice = bytes.get(local..end)?;
    Some((absolute, slice))
}

/// Returns the absolute offset and raw XMP packet bytes for the first IFD
/// entry with tag `0x02BC` (XMLPacket). See [`payload_for_tag`] for semantics.
pub fn xmp_payload<'a>(bytes: &'a [u8], tiff: &TiffContainer) -> Option<(u64, &'a [u8])> {
    payload_for_tag(bytes, tiff, 0x02BC)
}

/// Returns the absolute offset and raw ICC profile bytes for the first IFD
/// entry with tag `0x8773` (InterColorProfile).
pub fn icc_payload<'a>(bytes: &'a [u8], tiff: &TiffContainer) -> Option<(u64, &'a [u8])> {
    payload_for_tag(bytes, tiff, 0x8773)
}

/// Returns the absolute offset and raw IPTC-IIM bytes for the first IFD entry
/// with tag `0x83BB` (IPTC-NAA).
pub fn iptc_payload<'a>(bytes: &'a [u8], tiff: &TiffContainer) -> Option<(u64, &'a [u8])> {
    payload_for_tag(bytes, tiff, 0x83BB)
}

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

    #[test]
    fn parses_minimal_tiff() {
        let bytes = b"II*\0\x08\0\0\0\x00\x00\0\0\0\0";
        let parsed = parse_bytes(bytes, 0, "tiff").unwrap();
        assert_eq!(parsed.entries.len(), 0);
        assert!(!parsed.nodes.is_empty());
    }

    /// Build a TIFF file with exactly one IFD0 entry of type 7 (UNDEFINED)
    /// whose payload lives out-of-line directly after the IFD.
    fn build_single_entry_tiff(endian: Endian, tag_id: u16, payload: &[u8]) -> Vec<u8> {
        let (marker, pack16, pack32): ([u8; 2], fn(u16) -> [u8; 2], fn(u32) -> [u8; 4]) =
            match endian {
                Endian::Little => ([b'I', b'I'], |v| v.to_le_bytes(), |v| v.to_le_bytes()),
                Endian::Big => ([b'M', b'M'], |v| v.to_be_bytes(), |v| v.to_be_bytes()),
            };
        let magic: u16 = 42;
        let first_ifd_off: u32 = 8;
        let ifd_count: u16 = 1;
        // Header (8) + count (2) + entry (12) + next_ifd (4) = 26
        let payload_off: u32 = 8 + 2 + 12 + 4;

        let mut out = Vec::new();
        out.extend_from_slice(&marker);
        out.extend_from_slice(&pack16(magic));
        out.extend_from_slice(&pack32(first_ifd_off));
        out.extend_from_slice(&pack16(ifd_count));
        // entry: tag, type=7 (UNDEFINED), count=len, value_or_offset
        out.extend_from_slice(&pack16(tag_id));
        out.extend_from_slice(&pack16(7));
        out.extend_from_slice(&pack32(payload.len() as u32));
        out.extend_from_slice(&pack32(payload_off));
        // next IFD pointer
        out.extend_from_slice(&pack32(0));
        // payload bytes
        out.extend_from_slice(payload);
        out
    }

    #[test]
    fn xmp_payload_reads_tag_02bc_little_endian() {
        let payload = b"<x:xmpmeta>hello</x:xmpmeta>";
        let bytes = build_single_entry_tiff(Endian::Little, 0x02BC, payload);
        let tiff = parse_bytes(&bytes, 0, "tiff").unwrap();
        let (offset, slice) = xmp_payload(&bytes, &tiff).expect("xmp payload present");
        assert_eq!(slice, payload);
        assert_eq!(offset, 26);
    }

    #[test]
    fn xmp_payload_reads_tag_02bc_big_endian() {
        let payload = b"<x:xmpmeta>big-endian</x:xmpmeta>";
        let bytes = build_single_entry_tiff(Endian::Big, 0x02BC, payload);
        let tiff = parse_bytes(&bytes, 0, "tiff").unwrap();
        let (offset, slice) = xmp_payload(&bytes, &tiff).expect("xmp payload present");
        assert_eq!(slice, payload);
        assert_eq!(offset, 26);
    }

    #[test]
    fn icc_payload_reads_tag_8773_little_endian() {
        let payload: Vec<u8> = (0u8..64).collect();
        let bytes = build_single_entry_tiff(Endian::Little, 0x8773, &payload);
        let tiff = parse_bytes(&bytes, 0, "tiff").unwrap();
        let (_offset, slice) = icc_payload(&bytes, &tiff).expect("icc payload present");
        assert_eq!(slice, payload.as_slice());
    }

    #[test]
    fn iptc_payload_reads_tag_83bb_little_endian() {
        let payload = b"\x1c\x02\x69\x00\x05HELLO";
        let bytes = build_single_entry_tiff(Endian::Little, 0x83BB, payload);
        let tiff = parse_bytes(&bytes, 0, "tiff").unwrap();
        let (_offset, slice) = iptc_payload(&bytes, &tiff).expect("iptc payload present");
        assert_eq!(slice, payload);
    }

    #[test]
    fn payload_returns_none_when_absent() {
        // Valid TIFF with a single non-matching tag.
        let payload = b"irrelevant";
        let bytes = build_single_entry_tiff(Endian::Little, 0x0100, payload);
        let tiff = parse_bytes(&bytes, 0, "tiff").unwrap();
        assert!(xmp_payload(&bytes, &tiff).is_none());
        assert!(icc_payload(&bytes, &tiff).is_none());
        assert!(iptc_payload(&bytes, &tiff).is_none());
    }

    #[test]
    fn payload_returns_none_for_out_of_bounds_offset() {
        // Build a tiff, then truncate trailing payload bytes. parse_bytes
        // will flag `tiff_value_out_of_bounds` and set value_offset_absolute
        // to None, so the helper must return None rather than panic.
        let payload = b"<x:xmpmeta>hello</x:xmpmeta>";
        let mut bytes = build_single_entry_tiff(Endian::Little, 0x02BC, payload);
        bytes.truncate(bytes.len() - 5);
        let tiff = parse_bytes(&bytes, 0, "tiff").unwrap();
        assert!(xmp_payload(&bytes, &tiff).is_none());
    }
}
