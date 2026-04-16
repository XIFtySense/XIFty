use xifty_core::{MetadataEntry, Provenance, TypedValue};

#[derive(Debug, Clone)]
pub struct XmpPacket<'a> {
    pub bytes: &'a [u8],
    pub container: &'a str,
    pub offset_start: u64,
    pub offset_end: u64,
}

pub fn decode_packet(packet: XmpPacket<'_>) -> Vec<MetadataEntry> {
    let text = match std::str::from_utf8(packet.bytes) {
        Ok(text) => text,
        Err(_) => return Vec::new(),
    };

    let mut entries = Vec::new();
    push_string(
        &mut entries,
        packet.clone(),
        "CreateDate",
        "CreateDate",
        find_text(text, &["xmp:CreateDate", "photoshop:DateCreated"]),
        false,
    );
    push_string(
        &mut entries,
        packet.clone(),
        "ModifyDate",
        "ModifyDate",
        find_text(text, &["xmp:ModifyDate"]),
        false,
    );
    push_string(
        &mut entries,
        packet.clone(),
        "MetadataDate",
        "MetadataDate",
        find_text(text, &["xmp:MetadataDate"]),
        false,
    );
    push_string(
        &mut entries,
        packet.clone(),
        "DateTimeOriginal",
        "DateTimeOriginal",
        find_text(text, &["exif:DateTimeOriginal"]),
        true,
    );
    push_string(
        &mut entries,
        packet.clone(),
        "Make",
        "Make",
        find_text(text, &["tiff:Make"]),
        false,
    );
    push_string(
        &mut entries,
        packet.clone(),
        "Model",
        "Model",
        find_text(text, &["tiff:Model"]),
        false,
    );
    push_integer(
        &mut entries,
        packet.clone(),
        "Orientation",
        "Orientation",
        find_text(text, &["tiff:Orientation"]),
    );
    push_integer(
        &mut entries,
        packet.clone(),
        "ImageWidth",
        "ImageWidth",
        find_text(text, &["tiff:ImageWidth", "exif:PixelXDimension"]),
    );
    push_integer(
        &mut entries,
        packet.clone(),
        "ImageHeight",
        "ImageHeight",
        find_text(text, &["tiff:ImageLength", "exif:PixelYDimension"]),
    );
    push_string(
        &mut entries,
        packet.clone(),
        "GPSLatitude",
        "GPSLatitude",
        find_text(text, &["exif:GPSLatitude"]),
        false,
    );
    push_string(
        &mut entries,
        packet.clone(),
        "GPSLongitude",
        "GPSLongitude",
        find_text(text, &["exif:GPSLongitude"]),
        false,
    );
    push_string(
        &mut entries,
        packet.clone(),
        "Software",
        "Software",
        find_text(text, &["xmp:CreatorTool"]),
        false,
    );
    push_string(
        &mut entries,
        packet.clone(),
        "Author",
        "Author",
        find_text(text, &["dc:creator"]),
        false,
    );
    push_string(
        &mut entries,
        packet.clone(),
        "Copyright",
        "Copyright",
        find_text(text, &["dc:rights"]),
        false,
    );

    entries
}

pub fn decode_png_text_chunk(
    payload: &[u8],
    container: &str,
    offset_start: u64,
    offset_end: u64,
) -> Vec<MetadataEntry> {
    let Some(packet) = extract_png_xmp_packet(payload) else {
        return Vec::new();
    };
    decode_packet(XmpPacket {
        bytes: packet,
        container,
        offset_start,
        offset_end,
    })
}

pub fn decode_webp_xmp_chunk(
    payload: &[u8],
    container: &str,
    offset_start: u64,
    offset_end: u64,
) -> Vec<MetadataEntry> {
    decode_packet(XmpPacket {
        bytes: payload,
        container,
        offset_start,
        offset_end,
    })
}

fn extract_png_xmp_packet(payload: &[u8]) -> Option<&[u8]> {
    let nul = payload.iter().position(|byte| *byte == 0)?;
    let keyword = &payload[..nul];
    if keyword == b"XML:com.adobe.xmp" {
        if payload.len() < nul + 5 {
            return None;
        }
        let text_start = nul + 5;
        return payload.get(text_start..);
    }

    if keyword == b"XML:com.adobe.xmp\x00" {
        return payload.get(nul + 1..);
    }

    None
}

fn push_string(
    entries: &mut Vec<MetadataEntry>,
    packet: XmpPacket<'_>,
    tag_id: &str,
    tag_name: &str,
    value: Option<DecodedText>,
    timestamp: bool,
) {
    let Some(decoded) = value else {
        return;
    };
    entries.push(MetadataEntry {
        namespace: "xmp".into(),
        tag_id: tag_id.into(),
        tag_name: tag_name.into(),
        value: if timestamp {
            TypedValue::Timestamp(decoded.value)
        } else {
            TypedValue::String(decoded.value)
        },
        provenance: Provenance {
            container: packet.container.into(),
            namespace: "xmp".into(),
            path: Some("xmp_packet".into()),
            offset_start: Some(packet.offset_start),
            offset_end: Some(packet.offset_end),
            notes: Vec::new(),
        },
        notes: vec![decoded.note],
    });
}

fn push_integer(
    entries: &mut Vec<MetadataEntry>,
    packet: XmpPacket<'_>,
    tag_id: &str,
    tag_name: &str,
    value: Option<DecodedText>,
) {
    let Some(decoded) = value else {
        return;
    };
    let Ok(value) = decoded.value.trim().parse::<i64>() else {
        return;
    };
    entries.push(MetadataEntry {
        namespace: "xmp".into(),
        tag_id: tag_id.into(),
        tag_name: tag_name.into(),
        value: TypedValue::Integer(value),
        provenance: Provenance {
            container: packet.container.into(),
            namespace: "xmp".into(),
            path: Some("xmp_packet".into()),
            offset_start: Some(packet.offset_start),
            offset_end: Some(packet.offset_end),
            notes: Vec::new(),
        },
        notes: vec![decoded.note],
    });
}

#[derive(Debug, Clone)]
struct DecodedText {
    value: String,
    note: String,
}

fn find_text(xml: &str, names: &[&str]) -> Option<DecodedText> {
    names
        .iter()
        .find_map(|name| find_attr(xml, name).or_else(|| find_element(xml, name)))
}

fn find_attr(xml: &str, name: &str) -> Option<DecodedText> {
    let needle = format!("{name}=\"");
    let start = xml.find(&needle)? + needle.len();
    let tail = &xml[start..];
    let end = tail.find('"')?;
    Some(DecodedText {
        value: xml_unescape(&tail[..end]),
        note: format!("decoded from xmp attribute {name}"),
    })
}

fn find_element(xml: &str, name: &str) -> Option<DecodedText> {
    let open = format!("<{name}>");
    if let Some(start) = xml.find(&open) {
        let body = &xml[start + open.len()..];
        if let Some(li_start) = body.find("<rdf:li>") {
            let li_body = &body[li_start + 8..];
            let li_end = li_body.find("</rdf:li>")?;
            return Some(DecodedText {
                value: xml_unescape(&li_body[..li_end]),
                note: format!("decoded from xmp rdf:li inside {name}"),
            });
        }
        let close = format!("</{name}>");
        let end = body.find(&close)?;
        return Some(DecodedText {
            value: xml_unescape(&body[..end]),
            note: format!("decoded from xmp element {name}"),
        });
    }

    None
}

fn xml_unescape(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xmp_decoder_extracts_supported_fields() {
        let entries = decode_packet(XmpPacket {
            bytes: br#"<x:xmpmeta><rdf:Description tiff:Make="XIFtyCam" xmp:CreatorTool="XIFtyTool" tiff:ImageWidth="800" exif:GPSLatitude="40.4462" exif:GPSLongitude="-79.98" /><dc:creator><rdf:Seq><rdf:li>K</rdf:li></rdf:Seq></dc:creator></x:xmpmeta>"#,
            container: "png",
            offset_start: 0,
            offset_end: 240,
        });
        assert!(entries.iter().any(|entry| entry.tag_name == "Make"));
        assert!(entries.iter().any(|entry| entry.tag_name == "Software"));
        assert!(entries.iter().any(|entry| entry.tag_name == "Author"));
        assert!(entries.iter().any(|entry| entry.tag_name == "GPSLatitude"));
        assert!(entries.iter().all(|entry| !entry.notes.is_empty()));
    }
}
