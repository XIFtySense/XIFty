//! Vorbis comment payload decoder.
//!
//! Handles the Vorbis-comment framing defined by the Vorbis I specification:
//!   `vendor_length | vendor_string | user_comment_list_length | (length | "KEY=VALUE")*`
//! All lengths are little-endian u32 — distinct from the big-endian FLAC
//! block framing that wraps the payload. This crate is container-agnostic:
//! FLAC passes `"flac"` as the container, and OGG passes `"ogg"` as the
//! container — both consume the same decoder without modification.

use xifty_core::{MetadataEntry, Provenance, TypedValue};

/// Bounded payload view with the exact offsets of the wrapping block so
/// every emitted [`MetadataEntry`] carries accurate provenance. Callers
/// supply the container name; this crate does not hard-code a container.
#[derive(Debug, Clone)]
pub struct VorbisCommentPayload<'a> {
    pub bytes: &'a [u8],
    pub container: &'a str,
    pub offset_start: u64,
    pub offset_end: u64,
}

/// List of Vorbis-comment keys this crate upgrades to canonical tag names.
/// Unknown keys are still emitted, tagged with the raw (upper-cased) key.
pub fn supported_tags() -> &'static [&'static str] {
    &[
        "TITLE",
        "ARTIST",
        "ALBUM",
        "DATE",
        "GENRE",
        "TRACKNUMBER",
        "ALBUMARTIST",
        "COMPOSER",
        "COMMENT",
    ]
}

/// Parse the payload into metadata entries. Malformed tail bytes are
/// ignored — the caller owns the decision about whether to emit an
/// outer container-level `Issue`.
pub fn decode_payload(payload: VorbisCommentPayload<'_>) -> Vec<MetadataEntry> {
    let mut entries = Vec::new();
    let mut cursor = 0usize;

    let Some(vendor_len) = read_u32_le(payload.bytes, &mut cursor) else {
        return entries;
    };
    let Some(vendor_bytes) = read_slice(payload.bytes, &mut cursor, vendor_len as usize) else {
        return entries;
    };
    if let Ok(vendor) = std::str::from_utf8(vendor_bytes) {
        if !vendor.is_empty() {
            entries.push(make_entry("Vendor", "Vendor", vendor, &payload));
        }
    }

    let Some(comment_count) = read_u32_le(payload.bytes, &mut cursor) else {
        return entries;
    };

    for _ in 0..comment_count {
        let Some(comment_len) = read_u32_le(payload.bytes, &mut cursor) else {
            break;
        };
        let Some(comment_bytes) = read_slice(payload.bytes, &mut cursor, comment_len as usize)
        else {
            break;
        };
        let Ok(comment_text) = std::str::from_utf8(comment_bytes) else {
            continue;
        };
        let Some(eq_index) = comment_text.find('=') else {
            continue;
        };
        let (key, value) = comment_text.split_at(eq_index);
        let value = &value[1..];
        if key.is_empty() {
            continue;
        }
        let upper_key = key.to_ascii_uppercase();
        let tag_name = canonical_tag_name(&upper_key).unwrap_or(upper_key.as_str());
        entries.push(make_entry(&upper_key, tag_name, value, &payload));
    }

    entries
}

fn canonical_tag_name(upper_key: &str) -> Option<&'static str> {
    match upper_key {
        "TITLE" => Some("Title"),
        "ARTIST" => Some("Artist"),
        "ALBUM" => Some("Album"),
        "DATE" => Some("Date"),
        "GENRE" => Some("Genre"),
        "TRACKNUMBER" => Some("TrackNumber"),
        "ALBUMARTIST" => Some("AlbumArtist"),
        "COMPOSER" => Some("Composer"),
        "COMMENT" => Some("Comment"),
        _ => None,
    }
}

fn make_entry(
    tag_id: &str,
    tag_name: &str,
    value: &str,
    payload: &VorbisCommentPayload<'_>,
) -> MetadataEntry {
    MetadataEntry {
        namespace: "vorbis_comment".into(),
        tag_id: tag_id.into(),
        tag_name: tag_name.into(),
        value: TypedValue::String(value.to_string()),
        provenance: Provenance {
            container: payload.container.into(),
            namespace: "vorbis_comment".into(),
            path: Some("vorbis_comment".into()),
            offset_start: Some(payload.offset_start),
            offset_end: Some(payload.offset_end),
            notes: Vec::new(),
        },
        notes: Vec::new(),
    }
}

fn read_u32_le(bytes: &[u8], cursor: &mut usize) -> Option<u32> {
    let end = cursor.checked_add(4)?;
    let slice = bytes.get(*cursor..end)?;
    *cursor = end;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_slice<'a>(bytes: &'a [u8], cursor: &mut usize, len: usize) -> Option<&'a [u8]> {
    let end = cursor.checked_add(len)?;
    let slice = bytes.get(*cursor..end)?;
    *cursor = end;
    Some(slice)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_payload(vendor: &str, comments: &[&str]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        out.extend_from_slice(vendor.as_bytes());
        out.extend_from_slice(&(comments.len() as u32).to_le_bytes());
        for comment in comments {
            out.extend_from_slice(&(comment.len() as u32).to_le_bytes());
            out.extend_from_slice(comment.as_bytes());
        }
        out
    }

    fn decode(bytes: &[u8]) -> Vec<MetadataEntry> {
        decode_payload(VorbisCommentPayload {
            bytes,
            container: "flac",
            offset_start: 10,
            offset_end: 10 + bytes.len() as u64,
        })
    }

    #[test]
    fn round_trips_title_artist_album() {
        let bytes = build_payload(
            "reference-encoder",
            &["TITLE=Song", "ARTIST=Kai", "ALBUM=Demo"],
        );
        let entries = decode(&bytes);
        assert!(
            entries
                .iter()
                .any(|e| e.tag_name == "Title" && e.value == TypedValue::String("Song".into()))
        );
        assert!(entries.iter().any(|e| e.tag_name == "Artist"));
        assert!(entries.iter().any(|e| e.tag_name == "Album"));
        // Vendor is also surfaced.
        assert!(entries.iter().any(|e| e.tag_name == "Vendor"));
        assert!(!supported_tags().is_empty());
    }

    #[test]
    fn preserves_unknown_keys_as_upper_case_tag_id() {
        let bytes = build_payload("v", &["CUSTOMKEY=hello"]);
        let entries = decode(&bytes);
        let custom = entries
            .iter()
            .find(|e| e.tag_id == "CUSTOMKEY")
            .expect("custom key preserved");
        assert_eq!(custom.tag_name, "CUSTOMKEY");
        assert_eq!(custom.value, TypedValue::String("hello".into()));
    }

    #[test]
    fn preserves_multi_value_repeated_keys() {
        let bytes = build_payload("v", &["GENRE=Rock", "GENRE=Indie"]);
        let entries = decode(&bytes);
        let genres: Vec<_> = entries.iter().filter(|e| e.tag_name == "Genre").collect();
        assert_eq!(genres.len(), 2);
    }

    #[test]
    fn empty_comment_list_returns_only_vendor() {
        let bytes = build_payload("v", &[]);
        let entries = decode(&bytes);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].tag_name, "Vendor");
    }

    #[test]
    fn malformed_length_returns_entries_decoded_so_far() {
        // valid vendor + declared comment_count=2, but only 1 valid comment + truncated length.
        let vendor = "v";
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        bytes.extend_from_slice(vendor.as_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());
        let comment = "TITLE=Song";
        bytes.extend_from_slice(&(comment.len() as u32).to_le_bytes());
        bytes.extend_from_slice(comment.as_bytes());
        // Tail: only 2 bytes of a length header — not enough for u32.
        bytes.extend_from_slice(&[0u8, 0u8]);
        let entries = decode(&bytes);
        assert!(entries.iter().any(|e| e.tag_name == "Title"));
    }

    #[test]
    fn skips_entries_without_equals_sign() {
        let bytes = build_payload("v", &["NOEQUALS"]);
        let entries = decode(&bytes);
        // Only vendor remains.
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].tag_name, "Vendor");
    }
}
