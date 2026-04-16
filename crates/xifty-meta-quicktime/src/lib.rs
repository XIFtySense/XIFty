use xifty_core::{MetadataEntry, Provenance, TypedValue};

#[derive(Debug, Clone)]
pub struct QuickTimePayload<'a> {
    pub key: &'a str,
    pub bytes: &'a [u8],
    pub container: &'a str,
    pub offset_start: u64,
    pub offset_end: u64,
}

pub fn decode_payload(payload: QuickTimePayload<'_>) -> Vec<MetadataEntry> {
    let Some(text) = decode_data_box_text(payload.bytes) else {
        return Vec::new();
    };
    let (tag_name, normalized_name) = match payload.key {
        "author" => ("Author", "Author"),
        "software" => ("Software", "Software"),
        "title" => ("Title", "Title"),
        _ => return Vec::new(),
    };

    vec![MetadataEntry {
        namespace: "quicktime".into(),
        tag_id: normalized_name.into(),
        tag_name: tag_name.into(),
        value: TypedValue::String(text),
        provenance: Provenance {
            container: payload.container.into(),
            namespace: "quicktime".into(),
            path: Some("quicktime_data".into()),
            offset_start: Some(payload.offset_start),
            offset_end: Some(payload.offset_end),
            notes: Vec::new(),
        },
        notes: vec![format!(
            "decoded from quicktime metadata payload {}",
            payload.key
        )],
    }]
}

fn decode_data_box_text(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 16 {
        return None;
    }
    let size = u32::from_be_bytes(bytes.get(0..4)?.try_into().ok()?) as usize;
    if size < 16 || size > bytes.len() || bytes.get(4..8)? != b"data" {
        return None;
    }
    let text = std::str::from_utf8(bytes.get(16..size)?)
        .ok()?
        .trim_matches('\0');
    if text.is_empty() {
        return None;
    }
    Some(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_utf8_data_box() {
        let bytes = [
            0, 0, 0, 20, b'd', b'a', b't', b'a', 0, 0, 0, 1, 0, 0, 0, 0, b'K', b'a', b'i', b'\0',
        ];
        let entries = decode_payload(QuickTimePayload {
            key: "author",
            bytes: &bytes,
            container: "mp4",
            offset_start: 0,
            offset_end: 20,
        });
        assert_eq!(entries[0].tag_name, "Author");
    }
}
