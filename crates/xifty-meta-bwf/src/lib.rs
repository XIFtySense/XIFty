//! Broadcast Wave Format (BWF) `bext` chunk decoder.
//!
//! EBU Tech 3285 v2 layout (offsets are bytes from the start of the `bext`
//! chunk payload):
//!
//! | Offset | Size | Field                |
//! |--------|------|----------------------|
//! | 0      | 256  | Description          |
//! | 256    | 32   | Originator           |
//! | 288    | 32   | OriginatorReference  |
//! | 320    | 10   | OriginationDate      |
//! | 330    | 8    | OriginationTime      |
//! | 338    | 8    | TimeReference (u64)  |
//! | 346    | 2    | Version (u16)        |
//! | 348    | 64   | UMID                 |
//! | 412    | 190  | Reserved             |
//! | 602    | tail | CodingHistory        |
//!
//! The decoder bounds-checks each field; a truncated chunk degrades to
//! emitting only the prefix that fits.

use xifty_core::{MetadataEntry, Provenance, TypedValue};

#[derive(Debug, Clone)]
pub struct BwfPayload<'a> {
    pub bytes: &'a [u8],
    pub container: &'a str,
    pub offset_start: u64,
    pub offset_end: u64,
}

pub fn decode_payload(payload: BwfPayload<'_>) -> Vec<MetadataEntry> {
    let mut entries = Vec::new();
    let bytes = payload.bytes;

    if let Some(value) = read_string(bytes, 0, 256) {
        push_string(&mut entries, &payload, "Description", value);
    }
    if let Some(value) = read_string(bytes, 256, 32) {
        push_string(&mut entries, &payload, "Originator", value);
    }
    if let Some(value) = read_string(bytes, 288, 32) {
        push_string(&mut entries, &payload, "OriginatorReference", value);
    }
    if let Some(value) = read_string(bytes, 320, 10) {
        push_string(&mut entries, &payload, "OriginationDate", value);
    }
    if let Some(value) = read_string(bytes, 330, 8) {
        push_string(&mut entries, &payload, "OriginationTime", value);
    }
    if let Some(slice) = bytes.get(338..346) {
        if let Ok(arr) = <[u8; 8]>::try_from(slice) {
            let samples = u64::from_le_bytes(arr);
            // u64::MAX would overflow i64; clamp by emitting only when
            // representable. Practical BWF files stay well under i64::MAX.
            if samples <= i64::MAX as u64 {
                push_integer(
                    &mut entries,
                    &payload,
                    "TimeReference",
                    samples as i64,
                    "low+high 32-bit time reference, in samples",
                );
            }
        }
    }
    if let Some(slice) = bytes.get(346..348) {
        if let Ok(arr) = <[u8; 2]>::try_from(slice) {
            let version = u16::from_le_bytes(arr);
            push_integer(
                &mut entries,
                &payload,
                "Version",
                version as i64,
                "BWF version (0=raw, 1=UMID, 2+=loudness)",
            );
        }
    }
    if let Some(value) = read_hex(bytes, 348, 64) {
        push_string(&mut entries, &payload, "Umid", value);
    }
    // Reserved bytes (412..602) are ignored.
    if bytes.len() > 602 {
        if let Some(value) = read_string(bytes, 602, bytes.len() - 602) {
            if !value.is_empty() {
                push_string(&mut entries, &payload, "CodingHistory", value);
            }
        }
    }

    entries
}

fn read_string(bytes: &[u8], start: usize, len: usize) -> Option<String> {
    let slice = bytes.get(start..start + len)?;
    let trimmed = trim_nulls(slice);
    if trimmed.is_empty() {
        return None;
    }
    Some(String::from_utf8_lossy(trimmed).into_owned())
}

fn read_hex(bytes: &[u8], start: usize, len: usize) -> Option<String> {
    let slice = bytes.get(start..start + len)?;
    if slice.iter().all(|b| *b == 0) {
        return None;
    }
    let mut out = String::with_capacity(slice.len() * 2);
    for b in slice {
        out.push_str(&format!("{:02x}", b));
    }
    Some(out)
}

fn trim_nulls(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    let slice = &bytes[..end];
    let mut start = 0;
    while start < slice.len() && slice[start].is_ascii_whitespace() {
        start += 1;
    }
    let mut stop = slice.len();
    while stop > start && slice[stop - 1].is_ascii_whitespace() {
        stop -= 1;
    }
    &slice[start..stop]
}

fn provenance(payload: &BwfPayload<'_>) -> Provenance {
    Provenance {
        container: payload.container.into(),
        namespace: "bwf".into(),
        path: Some("bext".into()),
        offset_start: Some(payload.offset_start),
        offset_end: Some(payload.offset_end),
        notes: Vec::new(),
    }
}

fn push_string(
    entries: &mut Vec<MetadataEntry>,
    payload: &BwfPayload<'_>,
    tag: &str,
    value: String,
) {
    entries.push(MetadataEntry {
        namespace: "bwf".into(),
        tag_id: tag.into(),
        tag_name: tag.into(),
        value: TypedValue::String(value),
        provenance: provenance(payload),
        notes: vec!["decoded from BWF bext chunk".into()],
    });
}

fn push_integer(
    entries: &mut Vec<MetadataEntry>,
    payload: &BwfPayload<'_>,
    tag: &str,
    value: i64,
    note: &str,
) {
    entries.push(MetadataEntry {
        namespace: "bwf".into(),
        tag_id: tag.into(),
        tag_name: tag.into(),
        value: TypedValue::Integer(value),
        provenance: provenance(payload),
        notes: vec![note.into()],
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_bext(description: &[u8], time_reference: u64) -> Vec<u8> {
        let mut out = vec![0u8; 602];
        let copy_len = description.len().min(256);
        out[..copy_len].copy_from_slice(&description[..copy_len]);
        let originator = b"XIFty";
        out[256..256 + originator.len()].copy_from_slice(originator);
        let date = b"2026-04-20";
        out[320..330].copy_from_slice(date);
        let time = b"10:00:00";
        out[330..338].copy_from_slice(time);
        out[338..346].copy_from_slice(&time_reference.to_le_bytes());
        out[346..348].copy_from_slice(&1u16.to_le_bytes());
        out
    }

    #[test]
    fn decodes_bext_description() {
        let bytes = build_bext(b"hello world", 0);
        let entries = decode_payload(BwfPayload {
            bytes: &bytes,
            container: "wav",
            offset_start: 0,
            offset_end: bytes.len() as u64,
        });
        let description = entries
            .iter()
            .find(|e| e.tag_name == "Description")
            .expect("description present");
        assert!(matches!(&description.value, TypedValue::String(s) if s == "hello world"));
        assert_eq!(description.provenance.container, "wav");
        assert_eq!(description.provenance.namespace, "bwf");
    }

    #[test]
    fn decodes_time_reference() {
        let bytes = build_bext(b"sample", 12345678);
        let entries = decode_payload(BwfPayload {
            bytes: &bytes,
            container: "wav",
            offset_start: 0,
            offset_end: bytes.len() as u64,
        });
        let entry = entries
            .iter()
            .find(|e| e.tag_name == "TimeReference")
            .expect("time reference present");
        assert!(matches!(entry.value, TypedValue::Integer(12345678)));
    }

    #[test]
    fn truncated_chunk_decodes_only_prefix() {
        let bytes = vec![0u8; 100];
        // Description region only — everything else missing.
        let entries = decode_payload(BwfPayload {
            bytes: &bytes,
            container: "wav",
            offset_start: 0,
            offset_end: bytes.len() as u64,
        });
        assert!(entries.iter().all(|e| e.tag_name != "TimeReference"));
    }
}
