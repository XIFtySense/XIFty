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
}
