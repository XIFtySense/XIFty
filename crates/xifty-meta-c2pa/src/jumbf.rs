//! ISO 19566-5 (JUMBF) box walker.
//!
//! Box header: `<4-byte LBox big-endian><4-byte TBox>`. `LBox == 1` means a
//! 64-bit XLBox follows immediately; `LBox == 0` means the box runs to the
//! end of the enclosing range. `jumb` boxes are superboxes whose payload is
//! a sequence of child boxes; the first child of every `jumb` is a `jumd`
//! description box carrying the 16-byte UUID + label.
//!
//! Box identity in C2PA is **UUID-driven**. Labels (`c2pa`, `c2cl`, `c2as`,
//! `c2cs`, …) are advisory; the 16-byte UUID inside the `jumd` description
//! box is authoritative. Per the C2PA 2.0 specification "Box Type Identifiers"
//! section, the first 4 bytes of each superbox UUID encode the ASCII label,
//! followed by the JUMBF base UUID tail
//! `00 11 00 10 80 00 00 AA 00 38 9B 71`.

use xifty_core::{Issue, Severity};

const MAX_DEPTH: usize = 16;
#[cfg_attr(not(test), allow(dead_code))]
const JUMBF_BASE_TAIL: [u8; 12] = [
    0x00, 0x11, 0x00, 0x10, 0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71,
];

/// Build a C2PA superbox UUID from its 4-byte ASCII label. Per C2PA 2.0
/// "Box Type Identifiers".
pub const fn c2pa_uuid(label: [u8; 4]) -> [u8; 16] {
    [
        label[0], label[1], label[2], label[3], 0x00, 0x11, 0x00, 0x10, 0x80, 0x00, 0x00, 0xAA,
        0x00, 0x38, 0x9B, 0x71,
    ]
}

pub const UUID_C2PA: [u8; 16] = c2pa_uuid(*b"c2pa");
pub const UUID_C2MA: [u8; 16] = c2pa_uuid(*b"c2ma");
pub const UUID_C2CL: [u8; 16] = c2pa_uuid(*b"c2cl");
pub const UUID_C2AS: [u8; 16] = c2pa_uuid(*b"c2as");
pub const UUID_C2CS: [u8; 16] = c2pa_uuid(*b"c2cs");
pub const UUID_C2IG: [u8; 16] = c2pa_uuid(*b"c2ig");
pub const UUID_CBOR: [u8; 16] = c2pa_uuid(*b"cbor");
pub const UUID_JSON: [u8; 16] = c2pa_uuid(*b"json");

/// Decoded JUMBF box payload. `Children` holds parsed child boxes for `jumb`
/// superboxes (with their leading `jumd` description box already lifted into
/// the parent's `uuid`/`label`). `Bytes` keeps unrecognized raw payloads.
#[derive(Debug, Clone)]
pub enum BoxPayload<'a> {
    Cbor(&'a [u8]),
    Json(&'a [u8]),
    Children(Vec<JumbfBox<'a>>),
    Bytes(&'a [u8]),
}

#[derive(Debug, Clone)]
pub struct JumbfBox<'a> {
    /// 4-byte TBox FourCC. For `jumb` superboxes whose `jumd` description was
    /// lifted, this is `*b"jumb"` and `uuid`/`label` are populated from the
    /// description box.
    pub ty: [u8; 4],
    pub uuid: Option<[u8; 16]>,
    pub label: Option<String>,
    pub payload: BoxPayload<'a>,
}

/// Walk a contiguous JUMBF byte stream. Returns the top-level boxes (typically
/// a single `jumb` C2PA superbox) plus any non-fatal parse issues.
pub fn walk(bytes: &[u8]) -> (Vec<JumbfBox<'_>>, Vec<Issue>) {
    let mut issues = Vec::new();
    let boxes = walk_range(bytes, 0, bytes.len(), 0, &mut issues);
    (boxes, issues)
}

fn walk_range<'a>(
    bytes: &'a [u8],
    start: usize,
    end: usize,
    depth: usize,
    issues: &mut Vec<Issue>,
) -> Vec<JumbfBox<'a>> {
    if depth > MAX_DEPTH {
        issues.push(issue(
            "c2pa_jumbf_depth_exceeded",
            "JUMBF box nesting exceeds depth limit",
        ));
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut offset = start;
    while offset + 8 <= end {
        let lbox = u32::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]);
        let tbox = [
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ];
        let header_size;
        let total_size;
        if lbox == 1 {
            // 64-bit XLBox follows.
            if offset + 16 > end {
                issues.push(issue(
                    "c2pa_jumbf_truncated",
                    "JUMBF box has LBox=1 but payload truncated before XLBox",
                ));
                break;
            }
            let xlbox = u64::from_be_bytes([
                bytes[offset + 8],
                bytes[offset + 9],
                bytes[offset + 10],
                bytes[offset + 11],
                bytes[offset + 12],
                bytes[offset + 13],
                bytes[offset + 14],
                bytes[offset + 15],
            ]);
            if xlbox < 16 {
                issues.push(issue(
                    "c2pa_jumbf_malformed",
                    "JUMBF XLBox declares size < 16",
                ));
                break;
            }
            header_size = 16;
            total_size = xlbox as usize;
        } else if lbox == 0 {
            header_size = 8;
            total_size = end - offset;
        } else if lbox < 8 {
            issues.push(issue(
                "c2pa_jumbf_malformed",
                "JUMBF LBox declares size < 8",
            ));
            break;
        } else {
            header_size = 8;
            total_size = lbox as usize;
        }
        let box_end = offset.saturating_add(total_size);
        if box_end > end {
            issues.push(issue(
                "c2pa_jumbf_truncated",
                "JUMBF box length extends past enclosing range",
            ));
            break;
        }
        let payload_start = offset + header_size;
        let payload_slice = &bytes[payload_start..box_end];
        let parsed = parse_box(&tbox, payload_slice, depth, issues);
        out.push(parsed);
        if box_end == offset {
            break;
        }
        offset = box_end;
    }
    out
}

fn parse_box<'a>(
    tbox: &[u8; 4],
    payload: &'a [u8],
    depth: usize,
    issues: &mut Vec<Issue>,
) -> JumbfBox<'a> {
    if tbox == b"jumb" {
        // Superbox: first child should be a `jumd` description box.
        let children_raw = walk_range(payload, 0, payload.len(), depth + 1, issues);
        let mut uuid = None;
        let mut label = None;
        let mut remaining: Vec<JumbfBox<'_>> = Vec::with_capacity(children_raw.len());
        for (i, child) in children_raw.into_iter().enumerate() {
            if i == 0 && &child.ty == b"jumd" {
                if let Some(u) = child.uuid {
                    uuid = Some(u);
                }
                if let Some(l) = child.label.clone() {
                    label = Some(l);
                }
                continue;
            }
            remaining.push(child);
        }
        JumbfBox {
            ty: *tbox,
            uuid,
            label,
            payload: BoxPayload::Children(remaining),
        }
    } else if tbox == b"jumd" {
        parse_jumd(payload, issues)
    } else if tbox == b"cbor" {
        JumbfBox {
            ty: *tbox,
            uuid: None,
            label: None,
            payload: BoxPayload::Cbor(payload),
        }
    } else if tbox == b"json" {
        JumbfBox {
            ty: *tbox,
            uuid: None,
            label: None,
            payload: BoxPayload::Json(payload),
        }
    } else {
        JumbfBox {
            ty: *tbox,
            uuid: None,
            label: None,
            payload: BoxPayload::Bytes(payload),
        }
    }
}

/// Parse a JUMBF description box payload:
///   16-byte UUID
///   1-byte toggles
///   if toggles & 0x02 (label-present): null-terminated UTF-8 label
///   (request-id / signature optional fields are ignored at level A)
fn parse_jumd<'a>(payload: &'a [u8], issues: &mut Vec<Issue>) -> JumbfBox<'a> {
    if payload.len() < 17 {
        issues.push(issue(
            "c2pa_jumbf_malformed",
            "JUMBF description box truncated",
        ));
        return JumbfBox {
            ty: *b"jumd",
            uuid: None,
            label: None,
            payload: BoxPayload::Bytes(payload),
        };
    }
    let mut uuid = [0u8; 16];
    uuid.copy_from_slice(&payload[..16]);
    let toggles = payload[16];
    let mut cursor = 17usize;
    let mut label = None;
    if toggles & 0x02 != 0 {
        // Find null terminator.
        if let Some(rel) = payload[cursor..].iter().position(|&b| b == 0) {
            let raw = &payload[cursor..cursor + rel];
            label = Some(String::from_utf8_lossy(raw).into_owned());
            cursor += rel + 1;
        }
    }
    let _ = cursor;
    JumbfBox {
        ty: *b"jumd",
        uuid: Some(uuid),
        label,
        payload: BoxPayload::Bytes(payload),
    }
}

fn issue(code: &str, message: &str) -> Issue {
    Issue {
        severity: Severity::Warning,
        code: code.into(),
        message: message.into(),
        offset: None,
        context: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_box(tbox: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let total = (8 + payload.len()) as u32;
        let mut out = Vec::new();
        out.extend_from_slice(&total.to_be_bytes());
        out.extend_from_slice(tbox);
        out.extend_from_slice(payload);
        out
    }

    fn build_jumd(uuid: [u8; 16], label: &str) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&uuid);
        payload.push(0x03); // toggles: requestable + label present
        payload.extend_from_slice(label.as_bytes());
        payload.push(0);
        build_box(b"jumd", &payload)
    }

    #[test]
    fn uuid_constants_match_spec() {
        // c2pa: bytes 0..4 = ASCII label, then JUMBF base tail.
        assert_eq!(&UUID_C2PA[..4], b"c2pa");
        assert_eq!(&UUID_C2PA[4..], &JUMBF_BASE_TAIL);
        assert_eq!(&UUID_C2MA[..4], b"c2ma");
        assert_eq!(&UUID_C2CL[..4], b"c2cl");
        assert_eq!(&UUID_C2AS[..4], b"c2as");
        assert_eq!(&UUID_C2CS[..4], b"c2cs");
        assert_eq!(&UUID_CBOR[..4], b"cbor");
        assert_eq!(&UUID_JSON[..4], b"json");
    }

    #[test]
    fn walks_jumb_with_jumd_and_cbor_child() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&build_jumd(UUID_C2CL, "c2pa.claim"));
        payload.extend_from_slice(&build_box(b"cbor", &[0xA0]));
        let outer = build_box(b"jumb", &payload);

        let (boxes, issues) = walk(&outer);
        assert!(issues.is_empty(), "no issues, got {:?}", issues);
        assert_eq!(boxes.len(), 1);
        assert_eq!(&boxes[0].ty, b"jumb");
        assert_eq!(boxes[0].uuid.unwrap(), UUID_C2CL);
        assert_eq!(boxes[0].label.as_deref(), Some("c2pa.claim"));
        match &boxes[0].payload {
            BoxPayload::Children(children) => {
                assert_eq!(children.len(), 1);
                assert_eq!(&children[0].ty, b"cbor");
                match &children[0].payload {
                    BoxPayload::Cbor(b) => assert_eq!(*b, &[0xA0][..]),
                    other => panic!("expected cbor payload, got {:?}", other),
                }
            }
            other => panic!("expected children, got {:?}", other),
        }
    }

    #[test]
    fn walks_xlbox_64bit() {
        let inner = build_jumd(UUID_C2PA, "c2pa");
        let mut outer = Vec::new();
        // LBox=1 marker
        outer.extend_from_slice(&1u32.to_be_bytes());
        outer.extend_from_slice(b"jumb");
        let xl = (16 + inner.len()) as u64;
        outer.extend_from_slice(&xl.to_be_bytes());
        outer.extend_from_slice(&inner);

        let (boxes, issues) = walk(&outer);
        assert!(issues.is_empty());
        assert_eq!(boxes.len(), 1);
        assert_eq!(boxes[0].uuid.unwrap(), UUID_C2PA);
    }

    #[test]
    fn truncated_header_emits_issue() {
        let bytes = vec![0x00, 0x00, 0x00, 0x10, b'j', b'u', b'm', b'b']; // claims 16 bytes, only 8 present
        let (boxes, issues) = walk(&bytes);
        assert!(boxes.is_empty());
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|i| i.code == "c2pa_jumbf_truncated"));
    }

    #[test]
    fn unknown_tbox_preserved_as_bytes() {
        let outer = build_box(b"xxxx", &[1, 2, 3]);
        let (boxes, _) = walk(&outer);
        assert_eq!(boxes.len(), 1);
        match &boxes[0].payload {
            BoxPayload::Bytes(b) => assert_eq!(*b, &[1, 2, 3][..]),
            other => panic!("expected bytes, got {:?}", other),
        }
    }

    #[test]
    fn lbox_below_minimum_is_malformed() {
        let bytes = vec![0x00, 0x00, 0x00, 0x04, b'j', b'u', b'm', b'b'];
        let (_boxes, issues) = walk(&bytes);
        assert!(issues.iter().any(|i| i.code == "c2pa_jumbf_malformed"));
    }

    #[test]
    fn deeply_nested_emits_depth_issue() {
        // Build MAX_DEPTH+2 nested jumbs.
        let mut payload: Vec<u8> = Vec::new();
        // innermost: just a jumd
        payload.extend_from_slice(&build_jumd(UUID_C2PA, "x"));
        for _ in 0..(MAX_DEPTH + 2) {
            let mut wrap = Vec::new();
            wrap.extend_from_slice(&build_jumd(UUID_C2PA, "x"));
            wrap.extend_from_slice(&payload);
            payload = build_box(b"jumb", &wrap);
        }
        let (_boxes, issues) = walk(&payload);
        assert!(
            issues.iter().any(|i| i.code == "c2pa_jumbf_depth_exceeded"),
            "expected depth_exceeded, got {:?}",
            issues
        );
    }
}
