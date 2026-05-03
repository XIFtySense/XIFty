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

    pub fn icc_payloads(&self) -> impl Iterator<Item = (u64, &[u8])> {
        self.segments.iter().filter_map(|segment| {
            if segment.marker == 0xE2 && segment.payload.starts_with(b"ICC_PROFILE\0") {
                Some((segment.offset_start + 4, segment.payload.as_slice()))
            } else {
                None
            }
        })
    }

    pub fn iptc_payloads(&self) -> impl Iterator<Item = (u64, &[u8])> {
        self.segments.iter().filter_map(|segment| {
            if segment.marker == 0xED && segment.payload.starts_with(b"Photoshop 3.0\0") {
                Some((segment.offset_start + 4, segment.payload.as_slice()))
            } else {
                None
            }
        })
    }

    pub fn xmp_payloads(&self) -> impl Iterator<Item = (u64, &[u8])> {
        self.segments.iter().filter_map(|segment| {
            let prefix = b"http://ns.adobe.com/xap/1.0/\0";
            if segment.marker == 0xE1 && segment.payload.starts_with(prefix) {
                Some((
                    segment.offset_start + 4 + prefix.len() as u64,
                    &segment.payload[prefix.len()..],
                ))
            } else {
                None
            }
        })
    }

    /// Reassemble JUMBF (C2PA) payloads carried in APP11 segments per
    /// ISO 19566-5 §A.4. Each APP11 JUMBF segment payload starts with:
    ///
    /// ```text
    /// <2-byte CI = 'JP'> <2-byte En (Entity Identifier, BE)>
    /// <4-byte Z (sequence number, BE)> <box bytes>
    /// ```
    ///
    /// `En` identifies a logical manifest; manifests > ~64 KB span multiple
    /// segments with a strictly increasing `Z`. Segments are grouped by `En`
    /// and concatenated in `Z`-ascending order. Segments whose CI prefix
    /// is not `'JP'` are silently skipped (they are not C2PA carriers).
    /// True structural defects are surfaced as `Issue`s.
    pub fn jumbf_app11_payloads(&self) -> (Vec<Vec<u8>>, Vec<Issue>) {
        use std::collections::BTreeMap;
        let mut groups: BTreeMap<u16, BTreeMap<u32, Vec<u8>>> = BTreeMap::new();
        let mut issues: Vec<Issue> = Vec::new();
        for segment in &self.segments {
            if segment.marker != 0xEB {
                continue;
            }
            if segment.payload.len() < 8 {
                // Don't issue — this is just a non-C2PA APP11 (or a tiny
                // segment); silently skip.
                continue;
            }
            if &segment.payload[0..2] != b"JP" {
                continue;
            }
            let en = u16::from_be_bytes([segment.payload[2], segment.payload[3]]);
            let z = u32::from_be_bytes([
                segment.payload[4],
                segment.payload[5],
                segment.payload[6],
                segment.payload[7],
            ]);
            let entry = groups.entry(en).or_default().entry(z).or_default();
            if !entry.is_empty() {
                issues.push(Issue {
                    severity: Severity::Warning,
                    code: "jpeg_app11_duplicate_sequence".into(),
                    message: format!(
                        "duplicate JUMBF APP11 sequence En={en} Z={z}; later segment ignored"
                    ),
                    offset: Some(segment.offset_start),
                    context: Some(format!("APP11 En={en} Z={z}")),
                });
                continue;
            }
            entry.extend_from_slice(&segment.payload[8..]);
        }
        let mut payloads = Vec::with_capacity(groups.len());
        for (en, by_z) in groups {
            let mut combined = Vec::new();
            let mut expected: Option<u32> = None;
            for (z, chunk) in &by_z {
                if let Some(prev) = expected {
                    if *z != prev {
                        issues.push(Issue {
                            severity: Severity::Warning,
                            code: "jpeg_app11_sequence_gap".into(),
                            message: format!(
                                "JUMBF APP11 En={en} has gap in Z (expected {prev}, found {z})"
                            ),
                            offset: None,
                            context: Some(format!("APP11 En={en}")),
                        });
                    }
                }
                expected = Some(z + 1);
                combined.extend_from_slice(chunk);
            }
            payloads.push(combined);
        }
        (payloads, issues)
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

    fn build_app11_segment(en: u16, z: u32, box_bytes: &[u8]) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.extend_from_slice(b"JP");
        payload.extend_from_slice(&en.to_be_bytes());
        payload.extend_from_slice(&z.to_be_bytes());
        payload.extend_from_slice(box_bytes);
        let length = (payload.len() + 2) as u16;
        let mut out = Vec::new();
        out.extend_from_slice(&[0xFF, 0xEB]);
        out.extend_from_slice(&length.to_be_bytes());
        out.extend_from_slice(&payload);
        out
    }

    #[test]
    fn app11_single_segment_reassembly() {
        let mut box_bytes = Vec::new();
        box_bytes.extend_from_slice(&32u32.to_be_bytes());
        box_bytes.extend_from_slice(b"jumb");
        box_bytes.extend_from_slice(&[0u8; 24]);

        let mut bytes = vec![0xFF, 0xD8];
        bytes.extend_from_slice(&build_app11_segment(1, 1, &box_bytes));
        bytes.extend_from_slice(&[0xFF, 0xD9]);

        let parsed = parse_bytes(&bytes, 0).unwrap();
        let (payloads, issues) = parsed.jumbf_app11_payloads();
        assert!(issues.is_empty(), "no issues, got {:?}", issues);
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0], box_bytes);
    }

    #[test]
    fn app11_multi_segment_reassembly_orders_by_z() {
        let part_a = vec![0xAA; 10];
        let part_b = vec![0xBB; 10];

        let mut bytes = vec![0xFF, 0xD8];
        // Insert in REVERSE Z order; reassembler must sort.
        bytes.extend_from_slice(&build_app11_segment(7, 2, &part_b));
        bytes.extend_from_slice(&build_app11_segment(7, 1, &part_a));
        bytes.extend_from_slice(&[0xFF, 0xD9]);

        let parsed = parse_bytes(&bytes, 0).unwrap();
        let (payloads, issues) = parsed.jumbf_app11_payloads();
        assert!(issues.is_empty(), "no issues, got {:?}", issues);
        assert_eq!(payloads.len(), 1);
        let mut expected = Vec::new();
        expected.extend_from_slice(&part_a);
        expected.extend_from_slice(&part_b);
        assert_eq!(payloads[0], expected);
    }

    #[test]
    fn app11_signature_mismatch_skipped() {
        // APP11 with non-`JP` CI: skipped silently, no issue.
        let mut bad = Vec::new();
        bad.extend_from_slice(b"XX"); // wrong CI
        bad.extend_from_slice(&1u16.to_be_bytes());
        bad.extend_from_slice(&1u32.to_be_bytes());
        bad.extend_from_slice(&[0u8; 4]);
        let length = (bad.len() + 2) as u16;
        let mut bytes = vec![0xFF, 0xD8, 0xFF, 0xEB];
        bytes.extend_from_slice(&length.to_be_bytes());
        bytes.extend_from_slice(&bad);
        bytes.extend_from_slice(&[0xFF, 0xD9]);

        let parsed = parse_bytes(&bytes, 0).unwrap();
        let (payloads, issues) = parsed.jumbf_app11_payloads();
        assert!(payloads.is_empty());
        assert!(issues.is_empty());
    }

    #[test]
    fn routes_xmp_iptc_and_icc_segments() {
        let xmp = b"http://ns.adobe.com/xap/1.0/\0<x:xmpmeta/>";
        let icc = b"ICC_PROFILE\0\x01\x01abcd";
        let iptc = b"Photoshop 3.0\0abcd";
        let mut bytes = vec![0xFF, 0xD8];
        bytes.extend_from_slice(&[0xFF, 0xE1]);
        bytes.extend_from_slice(&((xmp.len() + 2) as u16).to_be_bytes());
        bytes.extend_from_slice(xmp);
        bytes.extend_from_slice(&[0xFF, 0xE2]);
        bytes.extend_from_slice(&((icc.len() + 2) as u16).to_be_bytes());
        bytes.extend_from_slice(icc);
        bytes.extend_from_slice(&[0xFF, 0xED]);
        bytes.extend_from_slice(&((iptc.len() + 2) as u16).to_be_bytes());
        bytes.extend_from_slice(iptc);
        bytes.extend_from_slice(&[0xFF, 0xD9]);

        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert!(parsed.xmp_payloads().next().is_some());
        assert!(parsed.icc_payloads().next().is_some());
        assert!(parsed.iptc_payloads().next().is_some());
    }
}
