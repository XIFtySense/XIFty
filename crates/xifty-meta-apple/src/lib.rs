use plist::Value as PlistValue;
use std::io::Cursor as IoCursor;
use xifty_container_tiff::TiffContainer;
use xifty_core::{MetadataEntry, Provenance, TypedValue};
use xifty_source::Endian;

const APPLE_MAKER_NOTE_HEADER: &[u8] = b"Apple iOS\0";

#[derive(Debug, Clone, Copy)]
struct AppleEntry {
    tag_id: u16,
    type_id: u16,
    count: u32,
    value_or_offset: u32,
}

pub fn decode_from_tiff(
    bytes: &[u8],
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
    if !matches!(make, Some(value) if value.eq_ignore_ascii_case("Apple")) {
        return Vec::new();
    }

    let Some(maker_note) = tiff.entries.iter().find(|entry| entry.tag_id == 0x927C) else {
        return Vec::new();
    };
    let start = maker_note.value_or_offset as usize;
    let Some(end) = start.checked_add(maker_note.count as usize) else {
        return Vec::new();
    };
    let Some(maker_bytes) = bytes.get(start..end) else {
        return Vec::new();
    };
    if !maker_bytes.starts_with(APPLE_MAKER_NOTE_HEADER) {
        return Vec::new();
    }

    let payload = &maker_bytes[APPLE_MAKER_NOTE_HEADER.len() + 2..];
    let value_base = APPLE_MAKER_NOTE_HEADER.len() + 2;
    if payload.len() < 4 {
        return Vec::new();
    }
    let endian = match &payload[..2] {
        b"II" => Endian::Little,
        b"MM" => Endian::Big,
        _ => return Vec::new(),
    };
    let Some(count) = read_u16(payload, 2, endian).map(|value| value as usize) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    let mut parsed_entries = Vec::new();
    for index in 0..count {
        let offset = 4 + index * 12;
        if offset + 12 > payload.len() {
            break;
        }
        let Some(tag_id) = read_u16(payload, offset, endian) else {
            break;
        };
        let Some(type_id) = read_u16(payload, offset + 2, endian) else {
            break;
        };
        let Some(count) = read_u32(payload, offset + 4, endian) else {
            break;
        };
        let Some(value_or_offset) = read_u32(payload, offset + 8, endian) else {
            break;
        };
        parsed_entries.push(AppleEntry {
            tag_id,
            type_id,
            count,
            value_or_offset,
        });
    }

    for entry in &parsed_entries {
        match entry.tag_id {
            0x0001 => push_integer(
                &mut entries,
                container_name,
                "MakerNoteVersion",
                entry.tag_id,
                "apple_makernote",
                read_i32_value(payload, value_base, endian, entry).map(i64::from),
            ),
            0x0003 => decode_runtime_plist(
                &mut entries,
                container_name,
                entry.tag_id,
                read_bytes_value(payload, value_base, endian, entry),
            ),
            0x0004 => push_string(
                &mut entries,
                container_name,
                "AEStable",
                entry.tag_id,
                "apple_makernote",
                read_i32_value(payload, value_base, endian, entry).map(yes_no),
            ),
            0x0005 => push_integer(
                &mut entries,
                container_name,
                "AETarget",
                entry.tag_id,
                "apple_makernote",
                read_i32_value(payload, value_base, endian, entry).map(i64::from),
            ),
            0x0006 => push_integer(
                &mut entries,
                container_name,
                "AEAverage",
                entry.tag_id,
                "apple_makernote",
                read_i32_value(payload, value_base, endian, entry).map(i64::from),
            ),
            0x0007 => push_string(
                &mut entries,
                container_name,
                "AFStable",
                entry.tag_id,
                "apple_makernote",
                read_i32_value(payload, value_base, endian, entry).map(yes_no),
            ),
            0x0008 => push_string(
                &mut entries,
                container_name,
                "AccelerationVector",
                entry.tag_id,
                "apple_makernote",
                read_signed_rational_triplet(payload, value_base, endian, entry),
            ),
            0x0014 => push_string(
                &mut entries,
                container_name,
                "ImageCaptureType",
                entry.tag_id,
                "apple_makernote",
                read_i32_value(payload, value_base, endian, entry).map(image_capture_type),
            ),
            0x0017 => push_integer(
                &mut entries,
                container_name,
                "LivePhotoVideoIndex",
                entry.tag_id,
                "apple_makernote",
                read_i64_value(payload, value_base, endian, entry),
            ),
            0x001f => push_integer(
                &mut entries,
                container_name,
                "PhotosAppFeatureFlags",
                entry.tag_id,
                "apple_makernote",
                read_i32_value(payload, value_base, endian, entry).map(i64::from),
            ),
            0x0021 => push_float(
                &mut entries,
                container_name,
                "HDRHeadroom",
                entry.tag_id,
                "apple_makernote",
                read_signed_rational_value(payload, value_base, endian, entry),
            ),
            0x0023 => push_string(
                &mut entries,
                container_name,
                "AFPerformance",
                entry.tag_id,
                "apple_makernote",
                read_i32_pair(payload, value_base, endian, entry).map(|(first, second)| {
                    format!("{first} {} {}", second >> 28, second & 0x0fff_ffff)
                }),
            ),
            0x0027 => push_float(
                &mut entries,
                container_name,
                "SignalToNoiseRatio",
                entry.tag_id,
                "apple_makernote",
                read_signed_rational_value(payload, value_base, endian, entry),
            ),
            0x002b => push_string(
                &mut entries,
                container_name,
                "PhotoIdentifier",
                entry.tag_id,
                "apple_makernote",
                read_string_value(payload, value_base, endian, entry),
            ),
            0x002d => push_integer(
                &mut entries,
                container_name,
                "ColorTemperature",
                entry.tag_id,
                "apple_makernote",
                read_i32_value(payload, value_base, endian, entry).map(i64::from),
            ),
            0x002e => push_string(
                &mut entries,
                container_name,
                "CameraType",
                entry.tag_id,
                "apple_makernote",
                read_i32_value(payload, value_base, endian, entry).map(camera_type),
            ),
            0x002f => push_integer(
                &mut entries,
                container_name,
                "FocusPosition",
                entry.tag_id,
                "apple_makernote",
                read_i32_value(payload, value_base, endian, entry).map(i64::from),
            ),
            0x0030 => push_float(
                &mut entries,
                container_name,
                "HDRGain",
                entry.tag_id,
                "apple_makernote",
                read_signed_rational_value(payload, value_base, endian, entry),
            ),
            0x0038 => push_integer(
                &mut entries,
                container_name,
                "AFMeasuredDepth",
                entry.tag_id,
                "apple_makernote",
                read_i32_value(payload, value_base, endian, entry).map(i64::from),
            ),
            0x003d => push_integer(
                &mut entries,
                container_name,
                "AFConfidence",
                entry.tag_id,
                "apple_makernote",
                read_i32_value(payload, value_base, endian, entry).map(i64::from),
            ),
            _ => {}
        }
    }

    entries
}

fn decode_runtime_plist(
    out: &mut Vec<MetadataEntry>,
    container_name: &str,
    tag_id: u16,
    bytes: Option<Vec<u8>>,
) {
    let Some(bytes) = bytes else {
        return;
    };
    let Ok(value) = PlistValue::from_reader(IoCursor::new(bytes)) else {
        return;
    };
    let Some(dict) = value.into_dictionary() else {
        return;
    };
    let flags = dict.get("flags").and_then(plist_signed_integer);
    let runtime_value = dict.get("value").and_then(plist_signed_integer);
    let epoch = dict.get("epoch").and_then(plist_signed_integer);
    let scale = dict.get("timescale").and_then(plist_signed_integer);

    push_string(
        out,
        container_name,
        "RunTimeFlags",
        tag_id,
        "apple_runtime",
        flags.map(runtime_flags),
    );
    push_integer(
        out,
        container_name,
        "RunTimeValue",
        tag_id,
        "apple_runtime",
        runtime_value,
    );
    push_integer(
        out,
        container_name,
        "RunTimeEpoch",
        tag_id,
        "apple_runtime",
        epoch,
    );
    push_integer(
        out,
        container_name,
        "RunTimeScale",
        tag_id,
        "apple_runtime",
        scale,
    );
}

fn plist_signed_integer(value: &PlistValue) -> Option<i64> {
    match value {
        PlistValue::Integer(integer) => integer.as_signed(),
        _ => None,
    }
}

fn read_u16(bytes: &[u8], offset: usize, endian: Endian) -> Option<u16> {
    let bytes: [u8; 2] = bytes.get(offset..offset + 2)?.try_into().ok()?;
    Some(match endian {
        Endian::Little => u16::from_le_bytes(bytes),
        Endian::Big => u16::from_be_bytes(bytes),
    })
}

fn read_u32(bytes: &[u8], offset: usize, endian: Endian) -> Option<u32> {
    let bytes: [u8; 4] = bytes.get(offset..offset + 4)?.try_into().ok()?;
    Some(match endian {
        Endian::Little => u32::from_le_bytes(bytes),
        Endian::Big => u32::from_be_bytes(bytes),
    })
}

fn entry_byte_len(entry: &AppleEntry) -> Option<usize> {
    let unit = match entry.type_id {
        1 | 2 | 7 => 1usize,
        3 => 2,
        4 | 9 => 4,
        5 | 10 | 16 => 8,
        _ => return None,
    };
    unit.checked_mul(entry.count as usize)
}

fn read_bytes_value(
    payload: &[u8],
    value_base: usize,
    endian: Endian,
    entry: &AppleEntry,
) -> Option<Vec<u8>> {
    let byte_len = entry_byte_len(entry)?;
    if byte_len <= 4 {
        let packed = match endian {
            Endian::Little => entry.value_or_offset.to_le_bytes(),
            Endian::Big => entry.value_or_offset.to_be_bytes(),
        };
        Some(packed[..byte_len].to_vec())
    } else {
        let offset = entry.value_or_offset as usize;
        let local_offset = offset.checked_sub(value_base)?;
        payload
            .get(local_offset..local_offset + byte_len)
            .map(|bytes| bytes.to_vec())
    }
}

fn read_string_value(
    payload: &[u8],
    value_base: usize,
    endian: Endian,
    entry: &AppleEntry,
) -> Option<String> {
    let bytes = read_bytes_value(payload, value_base, endian, entry)?;
    Some(trim_c_string_bytes(&bytes))
}

fn read_i32_value(
    payload: &[u8],
    value_base: usize,
    endian: Endian,
    entry: &AppleEntry,
) -> Option<i32> {
    let bytes = read_bytes_value(payload, value_base, endian, entry)?;
    let bytes: [u8; 4] = bytes.get(..4)?.try_into().ok()?;
    Some(match endian {
        Endian::Little => i32::from_le_bytes(bytes),
        Endian::Big => i32::from_be_bytes(bytes),
    })
}

fn read_i64_value(
    payload: &[u8],
    value_base: usize,
    endian: Endian,
    entry: &AppleEntry,
) -> Option<i64> {
    let bytes = read_bytes_value(payload, value_base, endian, entry)?;
    let bytes: [u8; 8] = bytes.get(..8)?.try_into().ok()?;
    Some(match endian {
        Endian::Little => i64::from_le_bytes(bytes),
        Endian::Big => i64::from_be_bytes(bytes),
    })
}

fn read_signed_rational_value(
    payload: &[u8],
    value_base: usize,
    endian: Endian,
    entry: &AppleEntry,
) -> Option<f64> {
    let bytes = read_bytes_value(payload, value_base, endian, entry)?;
    signed_rational_at(&bytes, 0, endian)
}

fn read_signed_rational_triplet(
    payload: &[u8],
    value_base: usize,
    endian: Endian,
    entry: &AppleEntry,
) -> Option<String> {
    let bytes = read_bytes_value(payload, value_base, endian, entry)?;
    let first = signed_rational_at(&bytes, 0, endian)?;
    let second = signed_rational_at(&bytes, 8, endian)?;
    let third = signed_rational_at(&bytes, 16, endian)?;
    Some(format!("{:.11} {:.10} {:.10}", first, second, third))
}

fn read_i32_pair(
    payload: &[u8],
    value_base: usize,
    endian: Endian,
    entry: &AppleEntry,
) -> Option<(i32, i32)> {
    let bytes = read_bytes_value(payload, value_base, endian, entry)?;
    let first: [u8; 4] = bytes.get(..4)?.try_into().ok()?;
    let second: [u8; 4] = bytes.get(4..8)?.try_into().ok()?;
    Some((
        match endian {
            Endian::Little => i32::from_le_bytes(first),
            Endian::Big => i32::from_be_bytes(first),
        },
        match endian {
            Endian::Little => i32::from_le_bytes(second),
            Endian::Big => i32::from_be_bytes(second),
        },
    ))
}

fn signed_rational_at(bytes: &[u8], offset: usize, endian: Endian) -> Option<f64> {
    let numerator: [u8; 4] = bytes.get(offset..offset + 4)?.try_into().ok()?;
    let denominator: [u8; 4] = bytes.get(offset + 4..offset + 8)?.try_into().ok()?;
    let numerator = match endian {
        Endian::Little => i32::from_le_bytes(numerator),
        Endian::Big => i32::from_be_bytes(numerator),
    } as f64;
    let denominator = match endian {
        Endian::Little => i32::from_le_bytes(denominator),
        Endian::Big => i32::from_be_bytes(denominator),
    } as f64;
    if denominator == 0.0 {
        return None;
    }
    Some(numerator / denominator)
}

fn trim_c_string_bytes(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).trim().to_string()
}

fn provenance(container_name: &str, path: &str) -> Provenance {
    Provenance {
        container: container_name.into(),
        namespace: "apple".into(),
        path: Some(path.into()),
        offset_start: None,
        offset_end: None,
        notes: Vec::new(),
    }
}

fn push_string(
    out: &mut Vec<MetadataEntry>,
    container_name: &str,
    tag_name: &str,
    tag_id: u16,
    path: &str,
    value: Option<String>,
) {
    let Some(value) = value else {
        return;
    };
    out.push(MetadataEntry {
        namespace: "apple".into(),
        tag_id: format!("0x{:04X}", tag_id),
        tag_name: tag_name.into(),
        value: TypedValue::String(value),
        provenance: provenance(container_name, path),
        notes: Vec::new(),
    });
}

fn push_integer(
    out: &mut Vec<MetadataEntry>,
    container_name: &str,
    tag_name: &str,
    tag_id: u16,
    path: &str,
    value: Option<i64>,
) {
    let Some(value) = value else {
        return;
    };
    out.push(MetadataEntry {
        namespace: "apple".into(),
        tag_id: format!("0x{:04X}", tag_id),
        tag_name: tag_name.into(),
        value: TypedValue::Integer(value),
        provenance: provenance(container_name, path),
        notes: Vec::new(),
    });
}

fn push_float(
    out: &mut Vec<MetadataEntry>,
    container_name: &str,
    tag_name: &str,
    tag_id: u16,
    path: &str,
    value: Option<f64>,
) {
    let Some(value) = value else {
        return;
    };
    out.push(MetadataEntry {
        namespace: "apple".into(),
        tag_id: format!("0x{:04X}", tag_id),
        tag_name: tag_name.into(),
        value: TypedValue::Float(value),
        provenance: provenance(container_name, path),
        notes: Vec::new(),
    });
}

fn yes_no(value: i32) -> String {
    if value == 0 {
        "No".into()
    } else {
        "Yes".into()
    }
}

fn runtime_flags(value: i64) -> String {
    let mut parts = Vec::new();
    if value & 1 != 0 {
        parts.push("Valid");
    }
    if value & 2 != 0 {
        parts.push("Has been rounded");
    }
    if value & 4 != 0 {
        parts.push("Positive infinity");
    }
    if value & 8 != 0 {
        parts.push("Negative infinity");
    }
    if value & 16 != 0 {
        parts.push("Indefinite");
    }
    if parts.is_empty() {
        value.to_string()
    } else {
        parts.join(", ")
    }
}

fn image_capture_type(value: i32) -> String {
    match value {
        1 => "ProRAW".into(),
        2 => "Portrait".into(),
        10 => "Photo".into(),
        11 => "Manual Focus".into(),
        12 => "Scene".into(),
        _ => value.to_string(),
    }
}

fn camera_type(value: i32) -> String {
    match value {
        0 => "Back Wide Angle".into(),
        1 => "Back Normal".into(),
        6 => "Front".into(),
        _ => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_flags_decodes_valid_bit() {
        assert_eq!(runtime_flags(1), "Valid");
    }
}
