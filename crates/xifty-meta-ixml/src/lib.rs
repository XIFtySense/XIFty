//! iXML chunk decoder for production-sound WAV files.
//!
//! iXML is an XML schema embedded in a WAV `iXML` chunk that carries
//! field-recording context (project, scene, take, tape, circled-take flag,
//! free-form notes). This crate intentionally does **not** pull in a full
//! XML parser: the production-sound subset we surface here uses a small,
//! attribute-and-element substring scan in the same shape as
//! `xifty-meta-rtmd`. Anything richer is future work.

use xifty_core::{MetadataEntry, Provenance, TypedValue};

#[derive(Debug, Clone)]
pub struct IxmlPayload<'a> {
    pub bytes: &'a [u8],
    pub container: &'a str,
    pub offset_start: u64,
    pub offset_end: u64,
}

pub fn decode_payload(payload: IxmlPayload<'_>) -> Vec<MetadataEntry> {
    let text = match std::str::from_utf8(payload.bytes) {
        Ok(text) => text,
        Err(_) => return Vec::new(),
    };
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    if !(trimmed.starts_with("<BWFXML") || trimmed.starts_with("<?xml")) {
        return Vec::new();
    }

    let mut entries = Vec::new();

    push_string(
        &mut entries,
        &payload,
        "RawXml",
        trimmed.trim_end().to_string(),
        "raw iXML payload",
    );

    for tag in &["PROJECT", "SCENE", "TAKE", "TAPE", "CIRCLED", "NOTE"] {
        if let Some(value) = element_text(trimmed, tag) {
            push_string(
                &mut entries,
                &payload,
                tag,
                value,
                "decoded from iXML element",
            );
        }
    }

    entries
}

fn element_text(text: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let start = text.find(&open)?;
    let after_open = &text[start + open.len()..];
    let gt = after_open.find('>')?;
    let body_start = gt + 1;
    let close = format!("</{tag}>");
    let close_rel = after_open[body_start..].find(&close)?;
    let body = &after_open[body_start..body_start + close_rel];
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

fn provenance(payload: &IxmlPayload<'_>) -> Provenance {
    Provenance {
        container: payload.container.into(),
        namespace: "ixml".into(),
        path: Some("iXML".into()),
        offset_start: Some(payload.offset_start),
        offset_end: Some(payload.offset_end),
        notes: Vec::new(),
    }
}

fn push_string(
    entries: &mut Vec<MetadataEntry>,
    payload: &IxmlPayload<'_>,
    tag: &str,
    value: String,
    note: &str,
) {
    entries.push(MetadataEntry {
        namespace: "ixml".into(),
        tag_id: tag.into(),
        tag_name: tag.into(),
        value: TypedValue::String(value),
        provenance: provenance(payload),
        notes: vec![note.into()],
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_raw_xml() {
        let xml = b"<BWFXML><PROJECT>xifty</PROJECT></BWFXML>";
        let entries = decode_payload(IxmlPayload {
            bytes: xml,
            container: "wav",
            offset_start: 0,
            offset_end: xml.len() as u64,
        });
        assert!(entries.iter().any(|e| e.tag_name == "RawXml"));
    }

    #[test]
    fn parses_project_and_scene() {
        let xml = b"<BWFXML><PROJECT>xifty</PROJECT><SCENE>test</SCENE></BWFXML>";
        let entries = decode_payload(IxmlPayload {
            bytes: xml,
            container: "wav",
            offset_start: 0,
            offset_end: xml.len() as u64,
        });
        let project = entries
            .iter()
            .find(|e| e.tag_name == "PROJECT")
            .expect("project decoded");
        assert!(matches!(&project.value, TypedValue::String(s) if s == "xifty"));
        let scene = entries
            .iter()
            .find(|e| e.tag_name == "SCENE")
            .expect("scene decoded");
        assert!(matches!(&scene.value, TypedValue::String(s) if s == "test"));
        assert_eq!(project.provenance.container, "wav");
        assert_eq!(project.provenance.namespace, "ixml");
    }

    #[test]
    fn rejects_non_xml_payload() {
        let entries = decode_payload(IxmlPayload {
            bytes: b"definitely not xml",
            container: "wav",
            offset_start: 0,
            offset_end: 18,
        });
        assert!(entries.is_empty());
    }
}
