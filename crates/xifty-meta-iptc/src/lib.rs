use xifty_core::{MetadataEntry, Provenance, TypedValue};

#[derive(Debug, Clone)]
pub struct IptcPayload<'a> {
    pub bytes: &'a [u8],
    pub container: &'a str,
    pub path: &'a str,
    pub offset_start: u64,
    pub offset_end: u64,
}

pub fn decode_payload(payload: IptcPayload<'_>) -> Vec<MetadataEntry> {
    let bytes = if payload.bytes.starts_with(b"Photoshop 3.0\0") {
        photoshop_iptc_block(payload.bytes).unwrap_or(&[])
    } else {
        payload.bytes
    };
    decode_iim(bytes, &payload)
}

pub fn supported_datasets() -> &'static [&'static str] {
    &["Headline", "Description", "Keywords", "Author", "Copyright"]
}

fn photoshop_iptc_block(bytes: &[u8]) -> Option<&[u8]> {
    let mut offset = b"Photoshop 3.0\0".len();
    while offset + 12 <= bytes.len() {
        if bytes.get(offset..offset + 4)? != b"8BIM" {
            break;
        }
        let resource_id = u16::from_be_bytes(bytes.get(offset + 4..offset + 6)?.try_into().ok()?);
        offset += 6;
        let name_len = *bytes.get(offset)? as usize;
        offset += 1 + name_len;
        if (1 + name_len) % 2 != 0 {
            offset += 1;
        }
        let size = u32::from_be_bytes(bytes.get(offset..offset + 4)?.try_into().ok()?) as usize;
        offset += 4;
        let data = bytes.get(offset..offset + size)?;
        if resource_id == 0x0404 {
            return Some(data);
        }
        offset += size;
        if size % 2 != 0 {
            offset += 1;
        }
    }
    None
}

fn decode_iim(bytes: &[u8], payload: &IptcPayload<'_>) -> Vec<MetadataEntry> {
    let mut entries = Vec::new();
    let mut offset = 0usize;

    while offset + 5 <= bytes.len() {
        if bytes[offset] != 0x1C {
            offset += 1;
            continue;
        }
        let record = bytes[offset + 1];
        let dataset = bytes[offset + 2];
        let len = u16::from_be_bytes([bytes[offset + 3], bytes[offset + 4]]) as usize;
        let start = offset + 5;
        let end = start + len;
        let Some(value) = bytes.get(start..end) else {
            break;
        };
        let Some((tag_name, tag_id)) = dataset_name(record, dataset) else {
            offset = end;
            continue;
        };
        if let Some(text) = decode_text(value) {
            entries.push(MetadataEntry {
                namespace: "iptc".into(),
                tag_id: tag_id.into(),
                tag_name: tag_name.into(),
                value: TypedValue::String(text),
                provenance: Provenance {
                    container: payload.container.into(),
                    namespace: "iptc".into(),
                    path: Some(payload.path.into()),
                    offset_start: Some(payload.offset_start),
                    offset_end: Some(payload.offset_end),
                    notes: Vec::new(),
                },
                notes: vec![format!("decoded IPTC dataset {record}:{dataset}")],
            });
        }
        offset = end;
    }

    entries
}

fn dataset_name(record: u8, dataset: u8) -> Option<(&'static str, &'static str)> {
    match (record, dataset) {
        (2, 80) => Some(("Author", "2:80")),
        (2, 105) => Some(("Headline", "2:105")),
        (2, 120) => Some(("Description", "2:120")),
        (2, 25) => Some(("Keywords", "2:25")),
        (2, 116) => Some(("Copyright", "2:116")),
        _ => None,
    }
}

fn decode_text(bytes: &[u8]) -> Option<String> {
    let value = std::str::from_utf8(bytes).ok()?.trim();
    if value.is_empty() {
        return None;
    }
    Some(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_bounded_iim_fields() {
        let bytes = [
            0x1c, 0x02, 0x69, 0x00, 0x08, b'H', b'e', b'a', b'd', b'l', b'i', b'n', b'e',
        ];
        let entries = decode_payload(IptcPayload {
            bytes: &bytes,
            container: "jpeg",
            path: "iptc_iim",
            offset_start: 0,
            offset_end: bytes.len() as u64,
        });
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].tag_name, "Headline");
        assert!(!supported_datasets().is_empty());
    }
}
