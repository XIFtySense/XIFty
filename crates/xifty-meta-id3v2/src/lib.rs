//! Bounded ID3v2 frame decoder.
//!
//! Decodes a curated set of text-information frames from ID3v2.3 and 2.4
//! payloads into [`MetadataEntry`] values under the `id3v2` namespace.
//! Container framing (the 10-byte tag header and the audio frame walk) lives
//! in `xifty-container-id3`; this crate only interprets the frame bytes.

use xifty_core::{MetadataEntry, Provenance, TypedValue};

#[derive(Debug, Clone)]
pub struct Id3v2Payload<'a> {
    pub bytes: &'a [u8],
    pub version_major: u8,
    pub container: &'a str,
    pub offset_start: u64,
    pub offset_end: u64,
}

pub fn supported_frames() -> &'static [&'static str] {
    &["TIT2", "TPE1", "TALB", "TCON", "TYER", "TDRC"]
}

pub fn decode_payload(payload: Id3v2Payload<'_>) -> Vec<MetadataEntry> {
    let mut entries = Vec::new();
    let mut offset = 0usize;
    while offset + 10 <= payload.bytes.len() {
        let frame_id_bytes = &payload.bytes[offset..offset + 4];
        if frame_id_bytes == b"\x00\x00\x00\x00" {
            // Padding region; ID3v2 explicitly allows zero-padding after
            // the last real frame.
            break;
        }
        let frame_id = match std::str::from_utf8(frame_id_bytes) {
            Ok(value) => value,
            Err(_) => break,
        };
        let size_bytes = &payload.bytes[offset + 4..offset + 8];
        let frame_size = if payload.version_major >= 4 {
            decode_syncsafe(size_bytes)
        } else {
            decode_be_u32(size_bytes)
        } as usize;
        let _flags = &payload.bytes[offset + 8..offset + 10];
        let body_start = offset + 10;
        let body_end = body_start + frame_size;
        if body_end > payload.bytes.len() {
            break;
        }
        let body = &payload.bytes[body_start..body_end];

        if let Some((tag_name, _id_alias)) = frame_label(frame_id) {
            if frame_id.starts_with('T') && body.len() >= 1 {
                if let Some(text) = decode_text_frame_body(body) {
                    entries.push(MetadataEntry {
                        namespace: "id3v2".into(),
                        tag_id: frame_id.to_string(),
                        tag_name: tag_name.to_string(),
                        value: TypedValue::String(text),
                        provenance: Provenance {
                            container: payload.container.to_string(),
                            namespace: "id3v2".into(),
                            path: Some(format!("id3v2/{frame_id}")),
                            offset_start: Some(payload.offset_start + offset as u64),
                            offset_end: Some(payload.offset_start + body_end as u64),
                            notes: vec![format!(
                                "decoded ID3v2.{} text frame {frame_id}",
                                payload.version_major
                            )],
                        },
                        notes: Vec::new(),
                    });
                }
            }
        }

        offset = body_end;
    }
    entries
}

fn frame_label(frame_id: &str) -> Option<(&'static str, &'static str)> {
    match frame_id {
        "TIT2" => Some(("Title", "TIT2")),
        "TPE1" => Some(("Artist", "TPE1")),
        "TALB" => Some(("Album", "TALB")),
        "TCON" => Some(("Genre", "TCON")),
        "TYER" => Some(("Year", "TYER")),
        "TDRC" => Some(("RecordingTime", "TDRC")),
        _ => None,
    }
}

fn decode_syncsafe(bytes: &[u8]) -> u32 {
    if bytes.len() != 4 {
        return 0;
    }
    ((bytes[0] as u32 & 0x7F) << 21)
        | ((bytes[1] as u32 & 0x7F) << 14)
        | ((bytes[2] as u32 & 0x7F) << 7)
        | (bytes[3] as u32 & 0x7F)
}

fn decode_be_u32(bytes: &[u8]) -> u32 {
    if bytes.len() != 4 {
        return 0;
    }
    u32::from_be_bytes(bytes.try_into().unwrap())
}

fn decode_text_frame_body(body: &[u8]) -> Option<String> {
    let encoding = body[0];
    let payload = &body[1..];
    let text = match encoding {
        0 => decode_iso_8859_1(payload),
        1 => decode_utf16_with_bom(payload)?,
        2 => decode_utf16_be(payload)?,
        3 => String::from_utf8(strip_trailing_nul(payload).to_vec()).ok()?,
        _ => return None,
    };
    let trimmed = text.trim_end_matches('\0').trim().to_string();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed)
}

fn strip_trailing_nul(bytes: &[u8]) -> &[u8] {
    let mut end = bytes.len();
    while end > 0 && bytes[end - 1] == 0 {
        end -= 1;
    }
    &bytes[..end]
}

fn decode_iso_8859_1(bytes: &[u8]) -> String {
    strip_trailing_nul(bytes)
        .iter()
        .map(|b| *b as char)
        .collect()
}

fn decode_utf16_with_bom(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 2 {
        return None;
    }
    let (le, payload) = match (bytes[0], bytes[1]) {
        (0xFF, 0xFE) => (true, &bytes[2..]),
        (0xFE, 0xFF) => (false, &bytes[2..]),
        _ => (true, bytes),
    };
    decode_utf16(payload, le)
}

fn decode_utf16_be(bytes: &[u8]) -> Option<String> {
    decode_utf16(bytes, false)
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> Option<String> {
    if bytes.len() % 2 != 0 {
        return None;
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|chunk| {
            if little_endian {
                u16::from_le_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_be_bytes([chunk[0], chunk[1]])
            }
        })
        .collect();
    String::from_utf16(&units).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_text_frame_v3(frame_id: &[u8; 4], text: &str) -> Vec<u8> {
        let mut body = vec![0u8]; // ISO-8859-1
        body.extend_from_slice(text.as_bytes());
        let mut frame = Vec::new();
        frame.extend_from_slice(frame_id);
        frame.extend_from_slice(&(body.len() as u32).to_be_bytes());
        frame.extend_from_slice(&[0u8, 0u8]); // flags
        frame.extend_from_slice(&body);
        frame
    }

    fn build_text_frame_v4(frame_id: &[u8; 4], text: &str) -> Vec<u8> {
        let mut body = vec![3u8]; // UTF-8
        body.extend_from_slice(text.as_bytes());
        let mut frame = Vec::new();
        frame.extend_from_slice(frame_id);
        // syncsafe size
        let size = body.len() as u32;
        frame.push(((size >> 21) & 0x7F) as u8);
        frame.push(((size >> 14) & 0x7F) as u8);
        frame.push(((size >> 7) & 0x7F) as u8);
        frame.push((size & 0x7F) as u8);
        frame.extend_from_slice(&[0u8, 0u8]);
        frame.extend_from_slice(&body);
        frame
    }

    #[test]
    fn decodes_id3v2_3_text_frames() {
        let mut payload = Vec::new();
        payload.extend(build_text_frame_v3(b"TIT2", "Title"));
        payload.extend(build_text_frame_v3(b"TPE1", "Artist"));
        payload.extend(build_text_frame_v3(b"TALB", "Album"));
        let entries = decode_payload(Id3v2Payload {
            bytes: &payload,
            version_major: 3,
            container: "mp3",
            offset_start: 0,
            offset_end: payload.len() as u64,
        });
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].tag_id, "TIT2");
        assert_eq!(entries[0].tag_name, "Title");
        assert_eq!(entries[0].value, TypedValue::String("Title".into()));
        assert_eq!(entries[1].tag_name, "Artist");
        assert_eq!(entries[2].tag_name, "Album");
    }

    #[test]
    fn decodes_id3v2_4_syncsafe_text_frames() {
        let mut payload = Vec::new();
        payload.extend(build_text_frame_v4(b"TIT2", "Title4"));
        payload.extend(build_text_frame_v4(b"TPE1", "Artist4"));
        let entries = decode_payload(Id3v2Payload {
            bytes: &payload,
            version_major: 4,
            container: "mp3",
            offset_start: 0,
            offset_end: payload.len() as u64,
        });
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].value, TypedValue::String("Title4".into()));
        assert_eq!(entries[1].value, TypedValue::String("Artist4".into()));
    }

    #[test]
    fn ignores_unsupported_frames() {
        let payload = build_text_frame_v3(b"TPUB", "Publisher");
        let entries = decode_payload(Id3v2Payload {
            bytes: &payload,
            version_major: 3,
            container: "mp3",
            offset_start: 0,
            offset_end: payload.len() as u64,
        });
        assert!(entries.is_empty());
        assert!(supported_frames().contains(&"TIT2"));
    }

    #[test]
    fn stops_at_padding() {
        let mut payload = Vec::new();
        payload.extend(build_text_frame_v3(b"TIT2", "Title"));
        payload.extend(std::iter::repeat(0u8).take(64));
        let entries = decode_payload(Id3v2Payload {
            bytes: &payload,
            version_major: 3,
            container: "mp3",
            offset_start: 0,
            offset_end: payload.len() as u64,
        });
        assert_eq!(entries.len(), 1);
    }
}
