//! ID3v2 (versions 2.3 and 2.4) frame decoder.
//!
//! Metadata-only: decodes raw ID3v2 frame bytes into [`MetadataEntry`] values
//! under the `id3v2` namespace. The container parser (xifty-container-id3)
//! owns tag framing / size decoding; this crate receives the already-stripped
//! payload bytes through [`Id3v2Payload`].

use xifty_core::{MetadataEntry, Provenance, TypedValue};

/// Raw ID3v2 frame payload, as produced by `xifty-container-id3`.
#[derive(Debug, Clone)]
pub struct Id3v2Payload<'a> {
    /// ID3v2 major version (3 or 4 are supported).
    pub version_major: u8,
    /// Absolute offset where the frame bytes begin (after the 10-byte header).
    pub offset_start: u64,
    /// Absolute offset where the frame bytes end.
    pub offset_end: u64,
    /// Frame bytes (i.e. the `body` of the ID3v2 tag).
    pub bytes: &'a [u8],
    /// Container emitting the payload (typically "mp3").
    pub container: &'a str,
}

/// Decode an ID3v2 payload into metadata entries.
pub fn decode_payload(payload: Id3v2Payload<'_>) -> Vec<MetadataEntry> {
    let mut entries = Vec::new();
    let bytes = payload.bytes;
    let mut offset = 0usize;

    while offset + 10 <= bytes.len() {
        let frame_id = &bytes[offset..offset + 4];
        // End-of-tag padding starts with a zero byte.
        if frame_id[0] == 0 {
            break;
        }
        if !is_valid_frame_id(frame_id) {
            break;
        }

        let size_bytes = &bytes[offset + 4..offset + 8];
        let size = if payload.version_major >= 4 {
            syncsafe_u32(size_bytes)
        } else {
            u32::from_be_bytes([size_bytes[0], size_bytes[1], size_bytes[2], size_bytes[3]])
        };
        let body_start = offset + 10;
        let body_end = body_start + size as usize;
        if body_end > bytes.len() {
            break;
        }
        let body = &bytes[body_start..body_end];
        let frame_id_str = std::str::from_utf8(frame_id)
            .unwrap_or("????")
            .to_string();

        if let Some(tag_name) = supported_text_frame(&frame_id_str) {
            if let Some(text) = decode_text_frame(body) {
                entries.push(MetadataEntry {
                    namespace: "id3v2".into(),
                    tag_id: frame_id_str.clone(),
                    tag_name: tag_name.into(),
                    value: TypedValue::String(text),
                    provenance: Provenance {
                        container: payload.container.into(),
                        namespace: "id3v2".into(),
                        path: Some(format!("id3v2/{frame_id_str}")),
                        offset_start: Some(payload.offset_start + offset as u64),
                        offset_end: Some(payload.offset_start + body_end as u64),
                        notes: Vec::new(),
                    },
                    notes: Vec::new(),
                });
            }
        }

        offset = body_end;
    }

    entries
}

/// Supported ID3v2 text-frame ids for phase 1.
pub fn supported_frames() -> &'static [&'static str] {
    &["TIT2", "TPE1", "TALB", "TCON", "TYER", "TDRC"]
}

fn supported_text_frame(frame_id: &str) -> Option<&'static str> {
    match frame_id {
        "TIT2" => Some("Title"),
        "TPE1" => Some("Artist"),
        "TALB" => Some("Album"),
        "TCON" => Some("Genre"),
        "TYER" | "TDRC" => Some("Year"),
        _ => None,
    }
}

fn is_valid_frame_id(id: &[u8]) -> bool {
    id.len() == 4
        && id
            .iter()
            .all(|b| (b'A'..=b'Z').contains(b) || (b'0'..=b'9').contains(b))
}

fn syncsafe_u32(bytes: &[u8]) -> u32 {
    let b0 = bytes[0] as u32;
    let b1 = bytes[1] as u32;
    let b2 = bytes[2] as u32;
    let b3 = bytes[3] as u32;
    ((b0 & 0x7F) << 21) | ((b1 & 0x7F) << 14) | ((b2 & 0x7F) << 7) | (b3 & 0x7F)
}

fn decode_text_frame(body: &[u8]) -> Option<String> {
    if body.is_empty() {
        return None;
    }
    let encoding = body[0];
    let payload = &body[1..];
    match encoding {
        0x00 => Some(decode_iso_8859_1(payload)),
        0x03 => Some(decode_utf8(payload)),
        0x01 => decode_utf16_with_bom(payload),
        0x02 => decode_utf16_be(payload),
        _ => None,
    }
}

fn decode_iso_8859_1(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take_while(|b| **b != 0)
        .map(|b| *b as char)
        .collect()
}

fn decode_utf8(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn decode_utf16_with_bom(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 2 {
        return None;
    }
    let (is_be, rest) = match (bytes[0], bytes[1]) {
        (0xFE, 0xFF) => (true, &bytes[2..]),
        (0xFF, 0xFE) => (false, &bytes[2..]),
        _ => (false, bytes),
    };
    let units: Vec<u16> = rest
        .chunks_exact(2)
        .map(|chunk| {
            if is_be {
                u16::from_be_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_le_bytes([chunk[0], chunk[1]])
            }
        })
        .take_while(|u| *u != 0)
        .collect();
    Some(String::from_utf16_lossy(&units))
}

fn decode_utf16_be(bytes: &[u8]) -> Option<String> {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
        .take_while(|u| *u != 0)
        .collect();
    Some(String::from_utf16_lossy(&units))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_frame_v23(id: &[u8; 4], text: &str) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(id);
        // Big-endian non-syncsafe size (ID3v2.3).
        let size = (text.len() + 1) as u32;
        out.extend_from_slice(&size.to_be_bytes());
        out.extend_from_slice(&[0u8, 0u8]); // flags
        out.push(0x00); // ISO-8859-1
        out.extend_from_slice(text.as_bytes());
        out
    }

    fn text_frame_v24_syncsafe(id: &[u8; 4], text: &str) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(id);
        let size = (text.len() + 1) as u32;
        let s0 = ((size >> 21) & 0x7F) as u8;
        let s1 = ((size >> 14) & 0x7F) as u8;
        let s2 = ((size >> 7) & 0x7F) as u8;
        let s3 = (size & 0x7F) as u8;
        out.extend_from_slice(&[s0, s1, s2, s3]);
        out.extend_from_slice(&[0u8, 0u8]); // flags
        out.push(0x03); // UTF-8
        out.extend_from_slice(text.as_bytes());
        out
    }

    #[test]
    fn decodes_id3v2_3_title_artist_album() {
        let mut bytes = Vec::new();
        bytes.extend(text_frame_v23(b"TIT2", "Hello World"));
        bytes.extend(text_frame_v23(b"TPE1", "XIFty Artist"));
        bytes.extend(text_frame_v23(b"TALB", "XIFty Album"));
        let entries = decode_payload(Id3v2Payload {
            version_major: 3,
            offset_start: 10,
            offset_end: 10 + bytes.len() as u64,
            bytes: &bytes,
            container: "mp3",
        });
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].tag_name, "Title");
        assert_eq!(
            entries[0].value,
            TypedValue::String("Hello World".into())
        );
        assert_eq!(entries[1].tag_name, "Artist");
        assert_eq!(entries[2].tag_name, "Album");
    }

    #[test]
    fn decodes_id3v2_4_syncsafe_sizes() {
        let mut bytes = Vec::new();
        bytes.extend(text_frame_v24_syncsafe(b"TIT2", "V2.4 Title"));
        bytes.extend(text_frame_v24_syncsafe(b"TPE1", "V2.4 Artist"));
        bytes.extend(text_frame_v24_syncsafe(b"TALB", "V2.4 Album"));
        let entries = decode_payload(Id3v2Payload {
            version_major: 4,
            offset_start: 10,
            offset_end: 10 + bytes.len() as u64,
            bytes: &bytes,
            container: "mp3",
        });
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].value, TypedValue::String("V2.4 Title".into()));
        assert_eq!(entries[2].value, TypedValue::String("V2.4 Album".into()));
    }

    #[test]
    fn skips_unknown_frames_without_panicking() {
        let mut bytes = Vec::new();
        bytes.extend(text_frame_v23(b"TXXX", "unused")); // not in supported list
        bytes.extend(text_frame_v23(b"TIT2", "Real Title"));
        let entries = decode_payload(Id3v2Payload {
            version_major: 3,
            offset_start: 0,
            offset_end: bytes.len() as u64,
            bytes: &bytes,
            container: "mp3",
        });
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].tag_name, "Title");
    }

    #[test]
    fn stops_at_padding_zero_bytes() {
        let mut bytes = text_frame_v23(b"TIT2", "Only Title");
        bytes.extend(std::iter::repeat_n(0u8, 50));
        let entries = decode_payload(Id3v2Payload {
            version_major: 3,
            offset_start: 0,
            offset_end: bytes.len() as u64,
            bytes: &bytes,
            container: "mp3",
        });
        assert_eq!(entries.len(), 1);
    }
}
