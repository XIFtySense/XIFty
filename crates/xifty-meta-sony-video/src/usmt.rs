//! USMT (User SMPTE-style metadata) UUID atom decoder.
//!
//! ## Wire format (per ExifTool `lib/Image/ExifTool/QuickTime.pm`)
//!
//! - The Sony `USMT` UUID box wraps a 16-byte usertype
//!   `55 53 4d 54 21 d2 4f ce bb 88 69 5c fa c9 c7 40` (USMT + Sony tail).
//!   See `QuickTime.pm` line ~1223-1231 (`UUID-USMT` dispatch:
//!   `Start => 16` — i.e. no header bytes between usertype and sub-boxes).
//! - The payload (post-usertype) is a sequence of standard ISOBMFF boxes
//!   (`size(4) + FourCC(4) + body`). The only known dispatched FourCC is
//!   `MTDT` (MetaData) — see `%QuickTime::UserMedia` at `QuickTime.pm:2589-2597`.
//! - Each `MTDT` body is parsed by `ProcessMetaData` (`QuickTime.pm:9573-9619`):
//!   `count(2 BE) + N × { size(2) + tag(4) + lang(2) + enc(2) + value(size-10) }`.
//! - Tag dispatch is `%QuickTime::MetaData` (`QuickTime.pm:2599-2658`):
//!   - `0x01` Title          (string)
//!   - `0x03` ProductionDate (string `YYYY/mm/dd HH:MM:SS`)
//!   - `0x04` Software       (string)
//!   - `0x05` Product        (string)
//!   - `0x0a` TrackProperty  (struct `Nnn` → flags(4) + attr(2) + priority(2))
//!   - `0x0b` TimeZone       (i16 BE, minutes from UTC)
//!   - `0x0c` ModifyDate     (string `YYYY/mm/dd HH:MM:SS`)
//! - Encoding word: `0` = 8-bit (Latin-1/ASCII), `1` = UTF-16 BE.
//!
//! Reference material only — read to understand, port to Rust, ship Rust.
//! Nothing from ExifTool ships in the artifact.

use xifty_core::{MetadataEntry, Provenance, TypedValue};
use xifty_source::{Cursor, Endian};

/// Decode the inner bytes of a Sony `USMT` UUID atom (post-usertype slice).
///
/// `bytes` is the box payload that follows the 16-byte usertype: a tree of
/// ISOBMFF sub-boxes containing zero or more `MTDT` MetaData blobs.
pub fn decode_usmt(bytes: &[u8], base_offset: u64, container_name: &str) -> Vec<MetadataEntry> {
    let mut out = Vec::new();
    let cursor = Cursor::new(bytes, base_offset);
    let mut pos = 0usize;
    while pos + 8 <= bytes.len() {
        let Ok(box_size) = cursor.read_u32(pos, Endian::Big) else {
            return out;
        };
        let box_size = box_size as usize;
        if box_size < 8 || pos + box_size > bytes.len() {
            return out;
        }
        let fourcc: [u8; 4] = match bytes.get(pos + 4..pos + 8) {
            Some(slice) => [slice[0], slice[1], slice[2], slice[3]],
            None => return out,
        };
        let body_start = pos + 8;
        let body_end = pos + box_size;
        if &fourcc == b"MTDT" {
            decode_mtdt(
                &bytes[body_start..body_end],
                base_offset + body_start as u64,
                container_name,
                &mut out,
            );
        }
        pos += box_size;
    }
    out
}

/// Walk one MTDT body. Per `ProcessMetaData` (`QuickTime.pm:9573-9619`).
fn decode_mtdt(body: &[u8], base_offset: u64, container: &str, out: &mut Vec<MetadataEntry>) {
    if body.len() < 2 {
        return;
    }
    let cursor = Cursor::new(body, base_offset);
    let Ok(count) = cursor.read_u16(0, Endian::Big) else {
        return;
    };
    let mut pos = 2usize;
    for _ in 0..count {
        if pos + 10 > body.len() {
            return;
        }
        let Ok(size) = cursor.read_u16(pos, Endian::Big) else {
            return;
        };
        let size = size as usize;
        if size < 10 || pos + size > body.len() {
            return;
        }
        let Ok(tag) = cursor.read_u32(pos + 2, Endian::Big) else {
            return;
        };
        // pos+6..pos+8 lang (UnpackLang in ExifTool); pos+8..pos+10 encoding.
        let Ok(enc) = cursor.read_u16(pos + 8, Endian::Big) else {
            return;
        };
        let value_bytes = &body[pos + 10..pos + size];
        if let Some(entry) =
            decode_record(tag, enc, value_bytes, container, base_offset + pos as u64)
        {
            out.push(entry);
        }
        pos += size;
    }
}

fn decode_record(
    tag: u32,
    enc: u16,
    value: &[u8],
    container: &str,
    offset_start: u64,
) -> Option<MetadataEntry> {
    let (tag_name, typed) = match tag {
        // Per QuickTime.pm:2604 — Title (string).
        0x01 => ("Title", string_value(enc, value)?),
        // Per QuickTime.pm:2605-2617 — ProductionDate (YYYY/mm/dd HH:MM:SS).
        0x03 => ("ProductionDate", string_value(enc, value)?),
        // Per QuickTime.pm:2618 — Software (string).
        0x04 => ("Software", string_value(enc, value)?),
        // Per QuickTime.pm:2619 — Product (string).
        0x05 => ("Product", string_value(enc, value)?),
        // Per QuickTime.pm:2620-2628 — TrackProperty (flags(N) + attr(n) + priority(n)).
        0x0a => ("TrackProperty", track_property(value)?),
        // Per QuickTime.pm:2629-2644 — TimeZone (Get16s minutes from UTC).
        0x0b => {
            if value.len() < 2 {
                return None;
            }
            let raw = i16::from_be_bytes([value[0], value[1]]);
            ("TimeZone", TypedValue::Integer(raw as i64))
        }
        // Per QuickTime.pm:2645-2657 — ModifyDate (YYYY/mm/dd HH:MM:SS).
        0x0c => ("ModifyDate", string_value(enc, value)?),
        _ => return None,
    };
    Some(MetadataEntry {
        namespace: "sony_video".into(),
        tag_id: format!("USMT/MTDT/0x{tag:04x}"),
        tag_name: tag_name.into(),
        value: typed,
        provenance: Provenance {
            container: container.into(),
            namespace: "sony_video".into(),
            path: Some("moov/uuid/USMT/MTDT".into()),
            offset_start: Some(offset_start),
            offset_end: None,
            notes: Vec::new(),
        },
        notes: Vec::new(),
    })
}

fn string_value(enc: u16, value: &[u8]) -> Option<TypedValue> {
    let s = match enc {
        0 => {
            // 8-bit (Latin-1/ASCII subset). Strip trailing NUL.
            let trimmed: Vec<u8> = value
                .iter()
                .copied()
                .take_while(|b| *b != 0)
                .filter(|b| (0x20..=0x7e).contains(b))
                .collect();
            String::from_utf8(trimmed).ok()?
        }
        1 => {
            // UTF-16 BE.
            if value.len() % 2 != 0 {
                return None;
            }
            let words: Vec<u16> = value
                .chunks_exact(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .take_while(|w| *w != 0)
                .collect();
            String::from_utf16(&words).ok()?
        }
        _ => return None,
    };
    if s.is_empty() {
        return None;
    }
    Some(TypedValue::String(s))
}

/// Pack TrackProperty's three components into a stable string. ExifTool
/// renders this as a `RawConv` of `unpack("Nnn", $val)` then a 3-element
/// PrintConv list — we surface a compact `flags=N attr=N priority=N` string
/// to keep the bounded shape simple while preserving the raw integers.
fn track_property(value: &[u8]) -> Option<TypedValue> {
    if value.len() < 8 {
        return None;
    }
    let flags = u32::from_be_bytes([value[0], value[1], value[2], value[3]]);
    let attr = u16::from_be_bytes([value[4], value[5]]);
    let priority = u16::from_be_bytes([value[6], value[7]]);
    Some(TypedValue::String(format!(
        "flags={flags} attr={attr} priority={priority}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real bytes from `fixtures/local/C0242.MP4` USMT UUID at offset 25167294
    /// (post-usertype slice — 28 bytes containing one MTDT box).
    fn c0242_usmt_inner_first() -> Vec<u8> {
        // 0000001c 4d544454 0001 0012 0000000a 55c4 0000 0000000100000000
        hex_to_bytes("0000001c4d544454000100120000000a55c400000000000100000000")
    }

    fn hex_to_bytes(hex: &str) -> Vec<u8> {
        hex.as_bytes()
            .chunks(2)
            .map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap())
            .collect()
    }

    #[test]
    fn decodes_real_c0242_track_property() {
        let bytes = c0242_usmt_inner_first();
        let entries = decode_usmt(&bytes, 0, "mp4");
        let track = entries
            .iter()
            .find(|e| e.tag_name == "TrackProperty")
            .unwrap_or_else(|| panic!("no TrackProperty in {entries:?}"));
        match &track.value {
            TypedValue::String(s) => {
                assert!(s.starts_with("flags="), "got {s}");
                assert!(s.contains("attr="));
                assert!(s.contains("priority="));
            }
            other => panic!("expected String, got {other:?}"),
        }
        assert_eq!(track.namespace, "sony_video");
        assert_eq!(track.provenance.container, "mp4");
    }

    #[test]
    fn ignores_unknown_tags_without_panic() {
        // MTDT body: count=1, one record with unknown tag 0xdeadbeef, 8-byte value.
        // 0001 0012 deadbeef 0000 0000 0000000000000000
        let mtdt_body = hex_to_bytes("00010012deadbeef0000000000000000000000000000");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&((mtdt_body.len() + 8) as u32).to_be_bytes());
        bytes.extend_from_slice(b"MTDT");
        bytes.extend_from_slice(&mtdt_body);
        let entries = decode_usmt(&bytes, 0, "mp4");
        assert!(entries.is_empty(), "got {entries:?}");
    }

    #[test]
    fn decodes_synthetic_title_and_software() {
        // Build an MTDT with two records: Title (0x01, ASCII "ABC") and
        // Software (0x04, ASCII "X1"), both encoding=0.
        // Record layout: size(2) tag(4) lang(2) enc(2) value(size-10).
        let title = b"ABC\0";
        let title_size: u16 = 10 + title.len() as u16;
        let software = b"X1\0\0";
        let sw_size: u16 = 10 + software.len() as u16;
        let mut mtdt = Vec::new();
        mtdt.extend_from_slice(&2u16.to_be_bytes()); // count
        mtdt.extend_from_slice(&title_size.to_be_bytes());
        mtdt.extend_from_slice(&0x0000_0001u32.to_be_bytes());
        mtdt.extend_from_slice(&0u16.to_be_bytes()); // lang
        mtdt.extend_from_slice(&0u16.to_be_bytes()); // enc
        mtdt.extend_from_slice(title);
        mtdt.extend_from_slice(&sw_size.to_be_bytes());
        mtdt.extend_from_slice(&0x0000_0004u32.to_be_bytes());
        mtdt.extend_from_slice(&0u16.to_be_bytes());
        mtdt.extend_from_slice(&0u16.to_be_bytes());
        mtdt.extend_from_slice(software);

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&((mtdt.len() + 8) as u32).to_be_bytes());
        bytes.extend_from_slice(b"MTDT");
        bytes.extend_from_slice(&mtdt);

        let entries = decode_usmt(&bytes, 0, "mp4");
        let title = entries
            .iter()
            .find(|e| e.tag_name == "Title")
            .expect("Title");
        assert!(matches!(&title.value, TypedValue::String(s) if s == "ABC"));
        let sw = entries
            .iter()
            .find(|e| e.tag_name == "Software")
            .expect("Software");
        assert!(matches!(&sw.value, TypedValue::String(s) if s == "X1"));
    }

    #[test]
    fn truncated_mtdt_returns_what_was_decoded() {
        // count=2 but body only fits one record.
        let mut mtdt = Vec::new();
        mtdt.extend_from_slice(&2u16.to_be_bytes());
        mtdt.extend_from_slice(&12u16.to_be_bytes()); // size
        mtdt.extend_from_slice(&0x0000_0001u32.to_be_bytes()); // Title
        mtdt.extend_from_slice(&0u16.to_be_bytes());
        mtdt.extend_from_slice(&0u16.to_be_bytes());
        mtdt.extend_from_slice(b"OK");
        // intentionally no second record
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&((mtdt.len() + 8) as u32).to_be_bytes());
        bytes.extend_from_slice(b"MTDT");
        bytes.extend_from_slice(&mtdt);
        let entries = decode_usmt(&bytes, 0, "mp4");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].tag_name, "Title");
    }
}
