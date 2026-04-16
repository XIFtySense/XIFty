use xifty_core::{MetadataEntry, Provenance, TypedValue};

#[derive(Debug, Clone)]
pub struct IccPayload<'a> {
    pub bytes: &'a [u8],
    pub container: &'a str,
    pub path: &'a str,
    pub offset_start: u64,
    pub offset_end: u64,
}

pub fn decode_payload(payload: IccPayload<'_>) -> Vec<MetadataEntry> {
    let icc = payload
        .bytes
        .strip_prefix(b"ICC_PROFILE\0")
        .and_then(|bytes| bytes.get(2..))
        .unwrap_or(payload.bytes);

    let Some(header) = icc.get(..128) else {
        return Vec::new();
    };

    let mut entries = Vec::new();
    push_string(
        &mut entries,
        &payload,
        "ProfileClass",
        "profile_class",
        decode_profile_class(header),
        "decoded from ICC header profile/device class",
    );
    push_string(
        &mut entries,
        &payload,
        "ColorSpace",
        "color_space",
        ascii4(header, 16),
        "decoded from ICC header data color space",
    );
    push_string(
        &mut entries,
        &payload,
        "ConnectionSpace",
        "connection_space",
        ascii4(header, 20),
        "decoded from ICC header PCS field",
    );
    push_string(
        &mut entries,
        &payload,
        "DeviceManufacturer",
        "device_manufacturer",
        ascii4(header, 48),
        "decoded from ICC header device manufacturer",
    );
    push_string(
        &mut entries,
        &payload,
        "DeviceModel",
        "device_model",
        ascii4(header, 52),
        "decoded from ICC header device model",
    );

    if let Some(name) = decode_profile_name(icc) {
        push_string(
            &mut entries,
            &payload,
            "ProfileDescription",
            "profile_description",
            Some(name),
            "decoded from ICC desc tag",
        );
    }

    entries
}

pub fn supported_tags() -> &'static [&'static str] {
    &[
        "ProfileClass",
        "ColorSpace",
        "ConnectionSpace",
        "DeviceManufacturer",
        "DeviceModel",
        "ProfileDescription",
    ]
}

fn ascii4(bytes: &[u8], offset: usize) -> Option<String> {
    let raw = bytes.get(offset..offset + 4)?;
    let value = std::str::from_utf8(raw).ok()?.trim();
    if value.is_empty() {
        return None;
    }
    Some(value.to_string())
}

fn decode_profile_class(header: &[u8]) -> Option<String> {
    match header.get(12..16)? {
        b"mntr" => Some("display".into()),
        b"prtr" => Some("output".into()),
        b"scnr" => Some("input".into()),
        b"link" => Some("device_link".into()),
        b"spac" => Some("color_space".into()),
        b"abst" => Some("abstract".into()),
        b"nmcl" => Some("named_color".into()),
        _ => ascii4(header, 12),
    }
}

fn decode_profile_name(bytes: &[u8]) -> Option<String> {
    let tag_count = u32::from_be_bytes(bytes.get(128..132)?.try_into().ok()?) as usize;
    for index in 0..tag_count {
        let base = 132 + index * 12;
        let signature = bytes.get(base..base + 4)?;
        let tag_offset =
            u32::from_be_bytes(bytes.get(base + 4..base + 8)?.try_into().ok()?) as usize;
        let tag_size =
            u32::from_be_bytes(bytes.get(base + 8..base + 12)?.try_into().ok()?) as usize;
        if signature == b"desc" {
            let tag = bytes.get(tag_offset..tag_offset + tag_size)?;
            return decode_desc_tag(tag);
        }
    }
    None
}

fn decode_desc_tag(tag: &[u8]) -> Option<String> {
    if tag.get(..4)? != b"desc" {
        return None;
    }
    let length = u32::from_be_bytes(tag.get(8..12)?.try_into().ok()?) as usize;
    let text = tag.get(12..12 + length)?;
    let end = text
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(text.len());
    let value = std::str::from_utf8(&text[..end]).ok()?.trim();
    if value.is_empty() {
        return None;
    }
    Some(value.to_string())
}

fn push_string(
    entries: &mut Vec<MetadataEntry>,
    payload: &IccPayload<'_>,
    tag_name: &str,
    tag_id: &str,
    value: Option<String>,
    note: &str,
) {
    let Some(value) = value else {
        return;
    };
    entries.push(MetadataEntry {
        namespace: "icc".into(),
        tag_id: tag_id.into(),
        tag_name: tag_name.into(),
        value: TypedValue::String(value),
        provenance: Provenance {
            container: payload.container.into(),
            namespace: "icc".into(),
            path: Some(payload.path.into()),
            offset_start: Some(payload.offset_start),
            offset_end: Some(payload.offset_end),
            notes: Vec::new(),
        },
        notes: vec![note.into()],
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_bounded_profile_fields() {
        let profile = minimal_icc_profile();
        let entries = decode_payload(IccPayload {
            bytes: &profile,
            container: "jpeg",
            path: "icc_profile",
            offset_start: 0,
            offset_end: profile.len() as u64,
        });
        assert_eq!(entries.len(), 6);
        assert_eq!(entries[0].tag_name, "ProfileClass");
        assert_eq!(entries[5].tag_name, "ProfileDescription");
        assert!(!supported_tags().is_empty());
    }

    fn minimal_icc_profile() -> Vec<u8> {
        let mut bytes = vec![0u8; 128];
        bytes[12..16].copy_from_slice(b"mntr");
        bytes[16..20].copy_from_slice(b"RGB ");
        bytes[20..24].copy_from_slice(b"XYZ ");
        bytes[48..52].copy_from_slice(b"APPL");
        bytes[52..56].copy_from_slice(b"TEST");
        bytes.extend_from_slice(&1u32.to_be_bytes());
        bytes.extend_from_slice(b"desc");
        bytes.extend_from_slice(&144u32.to_be_bytes());

        let mut desc = Vec::new();
        desc.extend_from_slice(b"desc");
        desc.extend_from_slice(&[0, 0, 0, 0]);
        desc.extend_from_slice(&9u32.to_be_bytes());
        desc.extend_from_slice(b"Test ICC\0");
        while desc.len() % 4 != 0 {
            desc.push(0);
        }

        bytes.extend_from_slice(&(desc.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&desc);
        let profile_len = bytes.len() as u32;
        bytes[0..4].copy_from_slice(&profile_len.to_be_bytes());
        bytes
    }
}
