use xifty_container_jpeg::JpegContainer;
use xifty_container_tiff::{TiffContainer, TiffEntry};
use xifty_core::{MetadataEntry, Provenance, RationalValue, Severity, TypedValue, issue};
use xifty_source::{Cursor, Endian};

pub fn decode_from_tiff(
    bytes: &[u8],
    base_offset: u64,
    container_name: &str,
    tiff: &TiffContainer,
) -> Vec<MetadataEntry> {
    let cursor = Cursor::new(bytes, base_offset);
    tiff.entries
        .iter()
        .filter_map(|entry| decode_entry(&cursor, tiff.endian, container_name, entry))
        .collect()
}

pub fn exif_payload_from_jpeg(container: &JpegContainer) -> Option<(u64, &[u8])> {
    container.exif_payload()
}

fn decode_entry(
    cursor: &Cursor<'_>,
    endian: Endian,
    container_name: &str,
    entry: &TiffEntry,
) -> Option<MetadataEntry> {
    let tag_name = tag_name(entry.tag_id);
    let namespace = "exif".to_string();
    let value = match entry.type_id {
        2 => read_ascii(cursor, endian, entry),
        3 | 4 => read_integer(cursor, endian, entry).map(TypedValue::Integer),
        5 => read_rational_values(cursor, endian, entry).map(|values| {
            if values.len() == 1 {
                TypedValue::Rational {
                    numerator: values[0].numerator,
                    denominator: values[0].denominator,
                }
            } else {
                TypedValue::RationalList(values)
            }
        }),
        1 | 7 => read_bytes(cursor, endian, entry).map(TypedValue::Bytes),
        _ => read_bytes(cursor, endian, entry).map(TypedValue::Bytes),
    }?;

    let value = if matches!(entry.tag_id, 0x0132 | 0x9003 | 0x9004) {
        if let TypedValue::String(text) = value.clone() {
            TypedValue::Timestamp(text)
        } else {
            value
        }
    } else {
        value
    };

    Some(MetadataEntry {
        namespace,
        tag_id: format!("0x{:04X}", entry.tag_id),
        tag_name: tag_name.into(),
        value,
        provenance: Provenance {
            container: container_name.into(),
            namespace: "exif".into(),
            path: Some(entry.ifd_name.clone()),
            offset_start: Some(entry.entry_offset),
            offset_end: Some(entry.entry_offset + 12),
            notes: Vec::new(),
        },
        notes: Vec::new(),
    })
}

fn read_ascii(cursor: &Cursor<'_>, endian: Endian, entry: &TiffEntry) -> Option<TypedValue> {
    let bytes = raw_value_bytes(cursor, endian, entry)?;
    let text = bytes.split(|b| *b == 0).next().unwrap_or(bytes.as_slice());
    Some(TypedValue::String(
        String::from_utf8_lossy(text).trim().to_string(),
    ))
}

fn read_integer(cursor: &Cursor<'_>, endian: Endian, entry: &TiffEntry) -> Option<i64> {
    match entry.type_id {
        3 => Some(read_u16_value(cursor, endian, entry)? as i64),
        4 => Some(read_u32_value(cursor, endian, entry)? as i64),
        _ => None,
    }
}

fn read_rational_values(
    cursor: &Cursor<'_>,
    endian: Endian,
    entry: &TiffEntry,
) -> Option<Vec<RationalValue>> {
    let offset = entry.value_or_offset as usize;
    let mut values = Vec::new();
    for index in 0..entry.count as usize {
        let start = offset + index * 8;
        values.push(RationalValue {
            numerator: cursor.read_u32(start, endian).ok()? as i64,
            denominator: cursor.read_u32(start + 4, endian).ok()? as i64,
        });
    }
    Some(values)
}

fn read_bytes(cursor: &Cursor<'_>, endian: Endian, entry: &TiffEntry) -> Option<Vec<u8>> {
    raw_value_bytes(cursor, endian, entry)
}

fn read_u16_value(cursor: &Cursor<'_>, endian: Endian, entry: &TiffEntry) -> Option<u16> {
    if entry.count == 0 {
        return None;
    }
    if entry.count == 1 {
        return Some(match endian {
            Endian::Little => (entry.value_or_offset & 0xFFFF) as u16,
            Endian::Big => (entry.value_or_offset >> 16) as u16,
        });
    }
    cursor.read_u16(entry.value_or_offset as usize, endian).ok()
}

fn read_u32_value(cursor: &Cursor<'_>, endian: Endian, entry: &TiffEntry) -> Option<u32> {
    if entry.count == 0 {
        return None;
    }
    if entry.count == 1 && entry.type_id == 4 {
        return Some(entry.value_or_offset);
    }
    cursor.read_u32(entry.value_or_offset as usize, endian).ok()
}

fn raw_value_bytes(cursor: &Cursor<'_>, endian: Endian, entry: &TiffEntry) -> Option<Vec<u8>> {
    let byte_len = match entry.type_id {
        1 | 2 | 7 => entry.count as usize,
        3 => entry.count as usize * 2,
        4 | 9 => entry.count as usize * 4,
        5 | 10 => entry.count as usize * 8,
        _ => entry.count as usize,
    };
    if byte_len <= 4 {
        let packed = match endian {
            Endian::Little => entry.value_or_offset.to_le_bytes(),
            Endian::Big => entry.value_or_offset.to_be_bytes(),
        };
        Some(packed[..byte_len].to_vec())
    } else {
        Some(
            cursor
                .slice(entry.value_or_offset as usize, byte_len)
                .ok()?
                .to_vec(),
        )
    }
}

pub fn tag_name(tag_id: u16) -> &'static str {
    match tag_id {
        0x010F => "Make",
        0x0110 => "Model",
        0x0112 => "Orientation",
        0x0131 => "Software",
        0x0132 => "ModifyDate",
        0x0100 => "ImageWidth",
        0x0101 => "ImageHeight",
        0x8769 => "ExifOffset",
        0x8825 => "GPSInfo",
        0x9003 => "DateTimeOriginal",
        0x9004 => "CreateDate",
        0x0001 => "GPSLatitudeRef",
        0x0002 => "GPSLatitude",
        0x0003 => "GPSLongitudeRef",
        0x0004 => "GPSLongitude",
        _ => "UnknownTag",
    }
}

pub fn malformed_entry_issue(message: impl Into<String>) -> xifty_core::Issue {
    issue(Severity::Warning, "exif_decode_warning", message)
}
