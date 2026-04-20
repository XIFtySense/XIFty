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
    push_string(
        &mut entries,
        packet.clone(),
        "Headline",
        "Headline",
        find_text(text, &["photoshop:Headline"]),
        false,
    );
    push_string(
        &mut entries,
        packet.clone(),
        "Description",
        "Description",
        find_text(text, &["dc:description", "photoshop:Caption-Abstract"]),
        false,
    );
    push_string_multi(
        &mut entries,
        packet.clone(),
        "Keywords",
        "Keywords",
        find_element_all(text, "dc:subject"),
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

/// Like `push_string`, but emits one `MetadataEntry` per decoded value. Used
/// for XMP bag/seq fields such as `dc:subject` where each `rdf:li` becomes a
/// distinct `Keywords` entry (mirroring the IPTC Keywords path).
///
/// Unlike `push_string`, the value type is always `TypedValue::String`
/// (keywords are never timestamps), so there is no `timestamp` parameter.
fn push_string_multi(
    entries: &mut Vec<MetadataEntry>,
    packet: XmpPacket<'_>,
    tag_id: &str,
    tag_name: &str,
    values: Vec<DecodedText>,
) {
    for decoded in values {
        entries.push(MetadataEntry {
            namespace: "xmp".into(),
            tag_id: tag_id.into(),
            tag_name: tag_name.into(),
            value: TypedValue::String(decoded.value),
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
        let close = format!("</{name}>");
        let element_end = body.find(&close);
        // If the element contains an <rdf:Alt> block, prefer the xml:lang="x-default"
        // alternative before falling back to the first <rdf:li>. Otherwise fall back
        // to the first <rdf:li …> (attributed or bare) inside e.g. <rdf:Bag>/<rdf:Seq>.
        let scan_slice = match element_end {
            Some(end) => &body[..end],
            None => body,
        };
        if let Some(alt_start) = scan_slice.find("<rdf:Alt") {
            // Walk all rdf:li children in the Alt block; prefer x-default.
            let alt_body = &scan_slice[alt_start..];
            let mut first_li: Option<String> = None;
            let mut x_default: Option<String> = None;
            let mut cursor = alt_body;
            while let Some((open_tag_end, body_start, li_value)) = next_rdf_li(cursor) {
                // `open_tag_end` is the index of `>` for the opening tag (within `cursor`);
                // `body_start` is the index just past it; both are relative to `cursor`.
                let opening_tag = &cursor[..=open_tag_end];
                let is_default = opening_tag.contains("xml:lang=\"x-default\"");
                if first_li.is_none() {
                    first_li = Some(li_value.clone());
                }
                if is_default {
                    x_default = Some(li_value.clone());
                    break;
                }
                // Advance past this <rdf:li>…</rdf:li>.
                let consumed = match cursor[body_start..].find("</rdf:li>") {
                    Some(end) => body_start + end + "</rdf:li>".len(),
                    None => break,
                };
                cursor = &cursor[consumed..];
            }
            let preferred = x_default
                .map(|v| (v, true))
                .or_else(|| first_li.map(|v| (v, false)));
            if let Some((value, is_default)) = preferred {
                let note = if is_default {
                    format!("decoded from xmp rdf:Alt x-default inside {name}")
                } else {
                    format!("decoded from xmp rdf:Alt rdf:li inside {name}")
                };
                return Some(DecodedText {
                    value: xml_unescape(&value),
                    note,
                });
            }
        }
        if let Some((_, body_start, li_value)) = next_rdf_li(scan_slice) {
            let _ = body_start;
            return Some(DecodedText {
                value: xml_unescape(&li_value),
                note: format!("decoded from xmp rdf:li inside {name}"),
            });
        }
        let end = element_end?;
        return Some(DecodedText {
            value: xml_unescape(&body[..end]),
            note: format!("decoded from xmp element {name}"),
        });
    }

    None
}

/// Return every `<rdf:li …>…</rdf:li>` payload inside the first `<{name}>…</{name}>`
/// element. Used for XMP `rdf:Bag`/`rdf:Seq` fields such as `dc:subject` where
/// each `rdf:li` is a distinct value (e.g. a keyword). Accepts both bare
/// `<rdf:li>` and attributed `<rdf:li xml:lang="…">` openings via the shared
/// `next_rdf_li` matcher. Returns an empty Vec if the element is absent or has
/// no `rdf:li` children.
fn find_element_all(xml: &str, name: &str) -> Vec<DecodedText> {
    let mut values = Vec::new();
    let open = format!("<{name}>");
    let Some(start) = xml.find(&open) else {
        return values;
    };
    let body = &xml[start + open.len()..];
    let close = format!("</{name}>");
    let scan_slice = match body.find(&close) {
        Some(end) => &body[..end],
        None => body,
    };
    let mut cursor = scan_slice;
    while let Some((_, body_start, li_value)) = next_rdf_li(cursor) {
        values.push(DecodedText {
            value: xml_unescape(&li_value),
            note: format!("decoded from xmp rdf:li inside {name}"),
        });
        let consumed = match cursor[body_start..].find("</rdf:li>") {
            Some(end) => body_start + end + "</rdf:li>".len(),
            None => break,
        };
        cursor = &cursor[consumed..];
    }
    values
}

/// Locate the next `<rdf:li …>` element inside `body`, accepting both the bare
/// `<rdf:li>` form and the attributed `<rdf:li xml:lang="x-default">` form.
///
/// Returns `(open_tag_gt_idx, body_start_idx, inner_value)` where the indices
/// are offsets within `body`. The scan stops at the first `>` after `<rdf:li`;
/// pathological attribute values containing an unescaped `>` would mis-terminate,
/// but XMP producers entity-encode such content. A quick-xml migration is the
/// right long-term answer — this keeps the file dependency-free per the plan.
fn next_rdf_li(body: &str) -> Option<(usize, usize, String)> {
    let li_start = body.find("<rdf:li")?;
    let after_tag = &body[li_start..];
    let gt_rel = after_tag.find('>')?;
    let open_tag_gt_idx = li_start + gt_rel;
    let body_start = open_tag_gt_idx + 1;
    let inner = &body[body_start..];
    let end = inner.find("</rdf:li>")?;
    Some((open_tag_gt_idx, body_start, inner[..end].to_string()))
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

    #[test]
    fn xmp_decoder_handles_rdf_alt_x_default_copyright() {
        let entries = decode_packet(XmpPacket {
            bytes: r#"<x:xmpmeta><rdf:Description><dc:rights><rdf:Alt><rdf:li xml:lang="x-default">© 2025 K</rdf:li></rdf:Alt></dc:rights></rdf:Description></x:xmpmeta>"#.as_bytes(),
            container: "png",
            offset_start: 0,
            offset_end: 180,
        });
        let copyright = entries
            .iter()
            .find(|entry| entry.tag_name == "Copyright")
            .expect("copyright entry present");
        match &copyright.value {
            TypedValue::String(value) => assert_eq!(value, "© 2025 K"),
            other => panic!("expected string, got {other:?}"),
        }
        assert!(
            copyright
                .notes
                .iter()
                .any(|note| note.contains("rdf:Alt x-default")),
            "expected x-default note, got {:?}",
            copyright.notes
        );
    }

    #[test]
    fn xmp_decoder_handles_rdf_alt_x_default_description() {
        let entries = decode_packet(XmpPacket {
            bytes: br#"<x:xmpmeta><rdf:Description><dc:description><rdf:Alt><rdf:li xml:lang="x-default">A photo of a cat.</rdf:li></rdf:Alt></dc:description></rdf:Description></x:xmpmeta>"#,
            container: "png",
            offset_start: 0,
            offset_end: 200,
        });
        let description = entries
            .iter()
            .find(|entry| entry.tag_name == "Description")
            .expect("description entry present");
        match &description.value {
            TypedValue::String(value) => assert_eq!(value, "A photo of a cat."),
            other => panic!("expected string, got {other:?}"),
        }
        assert!(
            description
                .notes
                .iter()
                .any(|note| note.contains("rdf:Alt x-default")),
            "expected x-default note, got {:?}",
            description.notes
        );
    }

    #[test]
    fn xmp_decoder_extracts_keywords_from_dc_subject() {
        let entries = decode_packet(XmpPacket {
            bytes: br#"<x:xmpmeta><rdf:Description><dc:subject><rdf:Bag><rdf:li>alpha</rdf:li><rdf:li>beta</rdf:li></rdf:Bag></dc:subject></rdf:Description></x:xmpmeta>"#,
            container: "png",
            offset_start: 0,
            offset_end: 160,
        });
        let keywords: Vec<&MetadataEntry> = entries
            .iter()
            .filter(|entry| entry.tag_name == "Keywords")
            .collect();
        assert_eq!(keywords.len(), 2, "expected two Keywords entries");
        let values: Vec<&str> = keywords
            .iter()
            .filter_map(|entry| match &entry.value {
                TypedValue::String(value) => Some(value.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(values, vec!["alpha", "beta"]);
        for entry in &keywords {
            assert!(
                entry
                    .notes
                    .iter()
                    .any(|note| note == "decoded from xmp rdf:li inside dc:subject"),
                "expected rdf:li inside dc:subject note, got {:?}",
                entry.notes
            );
        }
    }

    #[test]
    fn xmp_decoder_prefers_x_default_over_other_languages() {
        let entries = decode_packet(XmpPacket {
            bytes: r#"<x:xmpmeta><rdf:Description><dc:rights><rdf:Alt><rdf:li xml:lang="fr">© 2025 français</rdf:li><rdf:li xml:lang="x-default">© 2025 default</rdf:li></rdf:Alt></dc:rights></rdf:Description></x:xmpmeta>"#.as_bytes(),
            container: "png",
            offset_start: 0,
            offset_end: 240,
        });
        let copyright = entries
            .iter()
            .find(|entry| entry.tag_name == "Copyright")
            .expect("copyright entry present");
        match &copyright.value {
            TypedValue::String(value) => assert_eq!(value, "© 2025 default"),
            other => panic!("expected string, got {other:?}"),
        }
    }
}
