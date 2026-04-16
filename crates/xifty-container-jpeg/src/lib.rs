use xifty_core::{ContainerNode, Issue, Severity, XiftyError};
use xifty_source::{Cursor, SourceBytes};

#[derive(Debug, Clone)]
pub struct JpegSegment {
    pub marker: u8,
    pub offset_start: u64,
    pub offset_end: u64,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct JpegContainer {
    pub nodes: Vec<ContainerNode>,
    pub segments: Vec<JpegSegment>,
    pub issues: Vec<Issue>,
}

impl JpegContainer {
    pub fn exif_payload(&self) -> Option<(u64, &[u8])> {
        self.segments.iter().find_map(|segment| {
            if segment.marker == 0xE1 && segment.payload.starts_with(b"Exif\0\0") {
                Some((segment.offset_start + 4 + 6, &segment.payload[6..]))
            } else {
                None
            }
        })
    }
}

pub fn parse(source: &SourceBytes) -> Result<JpegContainer, XiftyError> {
    parse_bytes(source.bytes(), 0)
}

pub fn parse_bytes(bytes: &[u8], base_offset: u64) -> Result<JpegContainer, XiftyError> {
    let cursor = Cursor::new(bytes, base_offset);
    if cursor.len() < 4 || cursor.read_u8(0)? != 0xFF || cursor.read_u8(1)? != 0xD8 {
        return Err(XiftyError::Parse {
            message: "not a jpeg".into(),
        });
    }

    let mut nodes = vec![ContainerNode {
        kind: "container".into(),
        label: "jpeg".into(),
        offset_start: 0,
        offset_end: cursor.len() as u64,
        parent_label: None,
    }];
    let mut segments = Vec::new();
    let mut issues = Vec::new();
    let mut offset = 2usize;

    while offset + 1 < cursor.len() {
        if cursor.read_u8(offset)? != 0xFF {
            break;
        }
        let marker = cursor.read_u8(offset + 1)?;
        if marker == 0xD9 {
            nodes.push(ContainerNode {
                kind: "segment".into(),
                label: "EOI".into(),
                offset_start: offset as u64,
                offset_end: (offset + 2) as u64,
                parent_label: Some("jpeg".into()),
            });
            break;
        }

        if marker == 0xDA {
            nodes.push(ContainerNode {
                kind: "segment".into(),
                label: "SOS".into(),
                offset_start: offset as u64,
                offset_end: cursor.len() as u64,
                parent_label: Some("jpeg".into()),
            });
            break;
        }

        let length =
            u16::from_be_bytes([cursor.read_u8(offset + 2)?, cursor.read_u8(offset + 3)?]) as usize;
        if length < 2 || offset + 2 + length > cursor.len() {
            issues.push(Issue {
                severity: Severity::Warning,
                code: "jpeg_segment_length_invalid".into(),
                message: format!("invalid jpeg segment length at offset {offset}"),
                offset: Some(offset as u64),
                context: Some(format!("marker_{marker:02X}")),
            });
            break;
        }
        let payload = cursor.slice(offset + 4, length - 2)?.to_vec();
        let label = if (0xE0..=0xEF).contains(&marker) {
            format!("APP{:X}", marker - 0xE0)
        } else {
            format!("marker_{marker:02X}")
        };
        nodes.push(ContainerNode {
            kind: "segment".into(),
            label,
            offset_start: offset as u64,
            offset_end: (offset + 2 + length) as u64,
            parent_label: Some("jpeg".into()),
        });
        segments.push(JpegSegment {
            marker,
            offset_start: offset as u64,
            offset_end: (offset + 2 + length) as u64,
            payload,
        });
        offset += 2 + length;
    }

    Ok(JpegContainer {
        nodes,
        segments,
        issues,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn parses_app1_segment() {
        let bytes = [
            0xFF, 0xD8, 0xFF, 0xE1, 0x00, 0x10, b'E', b'x', b'i', b'f', 0, 0, 1, 2, 3, 4, 5, 6,
            0xFF, 0xD9,
        ];
        let mut path = std::env::temp_dir();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("xifty-jpeg-{stamp}.jpg"));
        fs::write(&path, bytes).unwrap();
        let parsed = parse(&SourceBytes::from_path(&path).unwrap()).unwrap();
        assert!(parsed.exif_payload().is_some());
        assert!(!parsed.nodes.is_empty());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn parses_non_app_segments_without_overflow() {
        let bytes = [
            0xFF, 0xD8, 0xFF, 0xDB, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xC0, 0x00, 0x05, 0x08, 0x00,
            0x00, 0xFF, 0xD9,
        ];
        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert!(parsed.nodes.iter().any(|node| node.label == "marker_DB"));
        assert!(parsed.nodes.iter().any(|node| node.label == "marker_C0"));
    }
}
