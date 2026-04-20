//! Interpret iTunes `ilst` atom payloads surfaced by `xifty-container-isobmff`
//! into normalized [`MetadataEntry`] values in the `itunes` namespace.
//!
//! Each iTunes atom wraps a `data` sub-box:
//! ```text
//!   size(4) | 'data' | version(1) | flags(3) | locale(4) | payload...
//! ```
//! The 24-bit flags field is a type indicator:
//!   * 0x01 — UTF-8 text
//!   * 0x00 — binary / integer (atom-specific interpretation)
//!   * 0x0D — JPEG cover art
//!   * 0x0E — PNG cover art
//!   * 0x15 — signed big-endian integer

use xifty_core::{MetadataEntry, Provenance, TypedValue};

#[derive(Debug, Clone)]
pub struct ItunesPayload<'a> {
    /// Four-char atom tag (e.g. `"©nam"`, `"trkn"`, `"covr"`).
    pub key: &'a str,
    /// Bytes of the inner `data` sub-box (starting at its size field).
    pub bytes: &'a [u8],
    /// Declared container (e.g. `"m4a"`).
    pub container: &'a str,
    pub offset_start: u64,
    pub offset_end: u64,
}

#[derive(Debug, Clone)]
struct DataBox<'a> {
    type_indicator: u32,
    payload: &'a [u8],
}

fn parse_data_box<'a>(bytes: &'a [u8]) -> Option<DataBox<'a>> {
    if bytes.len() < 16 {
        return None;
    }
    let size = u32::from_be_bytes(bytes.get(0..4)?.try_into().ok()?) as usize;
    if size < 16 || size > bytes.len() || bytes.get(4..8)? != b"data" {
        return None;
    }
    // version(1) | flags(3)
    let type_indicator = u32::from_be_bytes(bytes.get(8..12)?.try_into().ok()?) & 0x00FF_FFFF;
    let payload = bytes.get(16..size)?;
    Some(DataBox {
        type_indicator,
        payload,
    })
}

pub fn decode_payload(payload: ItunesPayload<'_>) -> Vec<MetadataEntry> {
    let Some(data) = parse_data_box(payload.bytes) else {
        return Vec::new();
    };

    let provenance = Provenance {
        container: payload.container.into(),
        namespace: "itunes".into(),
        path: Some("itunes_ilst".into()),
        offset_start: Some(payload.offset_start),
        offset_end: Some(payload.offset_end),
        notes: Vec::new(),
    };

    match payload.key {
        // ---- UTF-8 text atoms ----
        "\u{a9}nam" => text_entry(&data, provenance, "Title", "decoded from itunes ©nam"),
        "\u{a9}ART" => text_entry(&data, provenance, "Artist", "decoded from itunes ©ART"),
        "\u{a9}alb" => text_entry(&data, provenance, "Album", "decoded from itunes ©alb"),
        "\u{a9}day" => text_entry(&data, provenance, "Year", "decoded from itunes ©day"),
        "\u{a9}gen" => text_entry(&data, provenance, "Genre", "decoded from itunes ©gen"),
        "\u{a9}cmt" => text_entry(&data, provenance, "Comment", "decoded from itunes ©cmt"),
        "\u{a9}wrt" => text_entry(&data, provenance, "Composer", "decoded from itunes ©wrt"),
        "\u{a9}lyr" => text_entry(&data, provenance, "Lyrics", "decoded from itunes ©lyr"),
        "\u{a9}too" => text_entry(&data, provenance, "Encoder", "decoded from itunes ©too"),
        "aART" => text_entry(&data, provenance, "AlbumArtist", "decoded from itunes aART"),
        // ---- Integer-pair atoms (16-bit reserved + 16-bit index + 16-bit total + ...) ----
        "trkn" => pair_entry(&data, provenance, "TrackNumber", "decoded from itunes trkn"),
        "disk" => pair_entry(&data, provenance, "DiskNumber", "decoded from itunes disk"),
        // ---- Boolean atom ----
        "cpil" => bool_entry(&data, provenance, "Compilation", "decoded from itunes cpil"),
        // ---- Tempo (16-bit BPM) ----
        "tmpo" => integer_entry(
            &data,
            provenance,
            "BeatsPerMinute",
            "decoded from itunes tmpo",
        ),
        // ---- Binary cover art ----
        "covr" => bytes_entry(&data, provenance, "CoverArt", "decoded from itunes covr"),
        _ => Vec::new(),
    }
}

fn text_entry(
    data: &DataBox<'_>,
    provenance: Provenance,
    tag_name: &str,
    note: &str,
) -> Vec<MetadataEntry> {
    let Ok(text) = std::str::from_utf8(data.payload) else {
        return Vec::new();
    };
    let trimmed = text.trim_matches('\0');
    if trimmed.is_empty() {
        return Vec::new();
    }
    vec![MetadataEntry {
        namespace: "itunes".into(),
        tag_id: tag_name.into(),
        tag_name: tag_name.into(),
        value: TypedValue::String(trimmed.to_string()),
        provenance,
        notes: vec![note.into()],
    }]
}

fn pair_entry(
    data: &DataBox<'_>,
    provenance: Provenance,
    tag_name: &str,
    note: &str,
) -> Vec<MetadataEntry> {
    // iTunes trkn/disk body: 2-byte reserved, 2-byte index, 2-byte total, (2-byte reserved).
    if data.payload.len() < 6 {
        return Vec::new();
    }
    let index = u16::from_be_bytes([data.payload[2], data.payload[3]]);
    let total = u16::from_be_bytes([data.payload[4], data.payload[5]]);
    let text = if total > 0 {
        format!("{index}/{total}")
    } else {
        format!("{index}")
    };
    vec![MetadataEntry {
        namespace: "itunes".into(),
        tag_id: tag_name.into(),
        tag_name: tag_name.into(),
        value: TypedValue::String(text),
        provenance,
        notes: vec![note.into()],
    }]
}

fn bool_entry(
    data: &DataBox<'_>,
    provenance: Provenance,
    tag_name: &str,
    note: &str,
) -> Vec<MetadataEntry> {
    if data.payload.is_empty() {
        return Vec::new();
    }
    let value = data.payload[0] != 0;
    vec![MetadataEntry {
        namespace: "itunes".into(),
        tag_id: tag_name.into(),
        tag_name: tag_name.into(),
        value: TypedValue::Integer(value as i64),
        provenance,
        notes: vec![note.into()],
    }]
}

fn integer_entry(
    data: &DataBox<'_>,
    provenance: Provenance,
    tag_name: &str,
    note: &str,
) -> Vec<MetadataEntry> {
    let value = match data.payload.len() {
        1 => i64::from(i8::from_be_bytes([data.payload[0]])),
        2 => i64::from(i16::from_be_bytes([data.payload[0], data.payload[1]])),
        4 => i64::from(i32::from_be_bytes([
            data.payload[0],
            data.payload[1],
            data.payload[2],
            data.payload[3],
        ])),
        8 => i64::from_be_bytes([
            data.payload[0],
            data.payload[1],
            data.payload[2],
            data.payload[3],
            data.payload[4],
            data.payload[5],
            data.payload[6],
            data.payload[7],
        ]),
        _ => return Vec::new(),
    };
    vec![MetadataEntry {
        namespace: "itunes".into(),
        tag_id: tag_name.into(),
        tag_name: tag_name.into(),
        value: TypedValue::Integer(value),
        provenance,
        notes: vec![note.into()],
    }]
}

fn bytes_entry(
    data: &DataBox<'_>,
    provenance: Provenance,
    tag_name: &str,
    note: &str,
) -> Vec<MetadataEntry> {
    if data.payload.is_empty() {
        return Vec::new();
    }
    let image_kind = match data.type_indicator {
        0x0D => "jpeg",
        0x0E => "png",
        _ => "unknown",
    };
    vec![MetadataEntry {
        namespace: "itunes".into(),
        tag_id: tag_name.into(),
        tag_name: tag_name.into(),
        value: TypedValue::Bytes(data.payload.to_vec()),
        provenance,
        notes: vec![format!("{note} ({image_kind} cover art)")],
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data_box(type_indicator: u32, payload: &[u8]) -> Vec<u8> {
        let size = 16 + payload.len();
        let mut out = Vec::new();
        out.extend_from_slice(&(size as u32).to_be_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&type_indicator.to_be_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(payload);
        out
    }

    #[test]
    fn decodes_text_atom() {
        let bytes = data_box(0x01, b"Album Name");
        let entries = decode_payload(ItunesPayload {
            key: "\u{a9}alb",
            bytes: &bytes,
            container: "m4a",
            offset_start: 0,
            offset_end: bytes.len() as u64,
        });
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].tag_name, "Album");
        assert!(matches!(entries[0].value, TypedValue::String(ref s) if s == "Album Name"));
    }

    #[test]
    fn decodes_pair_atom() {
        // reserved(2) + index(2)=3 + total(2)=10
        let payload = [0, 0, 0, 3, 0, 10];
        let bytes = data_box(0x00, &payload);
        let entries = decode_payload(ItunesPayload {
            key: "trkn",
            bytes: &bytes,
            container: "m4a",
            offset_start: 0,
            offset_end: bytes.len() as u64,
        });
        assert_eq!(entries.len(), 1);
        assert!(matches!(entries[0].value, TypedValue::String(ref s) if s == "3/10"));
    }

    #[test]
    fn decodes_boolean_atom() {
        let bytes = data_box(0x15, &[1u8]);
        let entries = decode_payload(ItunesPayload {
            key: "cpil",
            bytes: &bytes,
            container: "m4a",
            offset_start: 0,
            offset_end: bytes.len() as u64,
        });
        assert_eq!(entries.len(), 1);
        assert!(matches!(entries[0].value, TypedValue::Integer(1)));
    }

    #[test]
    fn decodes_tempo_atom() {
        let bytes = data_box(0x15, &[0, 120]);
        let entries = decode_payload(ItunesPayload {
            key: "tmpo",
            bytes: &bytes,
            container: "m4a",
            offset_start: 0,
            offset_end: bytes.len() as u64,
        });
        assert!(matches!(entries[0].value, TypedValue::Integer(120)));
    }

    #[test]
    fn decodes_cover_art() {
        let bytes = data_box(0x0D, &[0x89, b'P', b'N', b'G']);
        let entries = decode_payload(ItunesPayload {
            key: "covr",
            bytes: &bytes,
            container: "m4a",
            offset_start: 0,
            offset_end: bytes.len() as u64,
        });
        assert_eq!(entries.len(), 1);
        assert!(matches!(entries[0].value, TypedValue::Bytes(_)));
    }

    #[test]
    fn ignores_unknown_key() {
        let bytes = data_box(0x01, b"anything");
        let entries = decode_payload(ItunesPayload {
            key: "xxxx",
            bytes: &bytes,
            container: "m4a",
            offset_start: 0,
            offset_end: bytes.len() as u64,
        });
        assert!(entries.is_empty());
    }

    #[test]
    fn ignores_malformed_data_box() {
        let bytes = [0u8, 0, 0, 5, b'n', b'o', b'p', b'e'];
        let entries = decode_payload(ItunesPayload {
            key: "\u{a9}nam",
            bytes: &bytes,
            container: "m4a",
            offset_start: 0,
            offset_end: 0,
        });
        assert!(entries.is_empty());
    }
}
