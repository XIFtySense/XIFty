use xifty_core::{MetadataEntry, Provenance, TypedValue};

#[derive(Debug, Clone)]
pub struct QuickTimePayload<'a> {
    pub key: &'a str,
    pub bytes: &'a [u8],
    pub container: &'a str,
    pub offset_start: u64,
    pub offset_end: u64,
}

/// Direct-`udta` ©-prefixed atom payload in classic QuickTime user-data
/// text format: `{u16 BE length}{u16 BE language}{ascii bytes of length}`.
///
/// Distinct from `QuickTimePayload`, which expects an iTunes `data` sub-box.
/// DJI drone firmware (FC* models / Mavic 3 family confirmed) uses this
/// classic shape for ©fpt/©fyw/©frl (flight pitch/yaw/roll), ©gpt/©gyw/©grl
/// (gimbal), ©xsp/©ysp/©zsp (speed XYZ), ©xyz (ISO 6709 location), ©mdl
/// (camera model), ©csn (camera serial number).
#[derive(Debug, Clone)]
pub struct QuickTimeUdtaPayload<'a> {
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

/// Decode a classic QuickTime user-data ©-text atom. The payload is
/// `{u16 BE length}{u16 BE language}{bytes-of-length}`. Language is
/// frequently 0xFF7F ("no language") in DJI captures; we record but do not
/// validate it.
///
/// Mapping for known DJI keys lands in the `dji` namespace; any other key
/// passes through under namespace `quicktime` so generic classic-QuickTime
/// callers (Final Cut udta-text, Compressor exports, …) still get something.
/// `©xyz` (ISO 6709 location) is a standard QuickTime tag and emits its
/// split lat/lon/alt entries under namespace `quicktime`.
pub fn decode_udta_payload(payload: QuickTimeUdtaPayload<'_>) -> Vec<MetadataEntry> {
    let Some(text) = decode_udta_text(payload.bytes) else {
        return Vec::new();
    };
    let provenance = || Provenance {
        container: payload.container.into(),
        namespace: "quicktime-udta".into(),
        path: Some(format!("udta/{}", payload.key)),
        offset_start: Some(payload.offset_start),
        offset_end: Some(payload.offset_end),
        notes: Vec::new(),
    };
    let note = format!("decoded from classic udta text atom {}", payload.key);

    if payload.key == "\u{a9}xyz" {
        return decode_iso6709(
            &text,
            payload.container,
            payload.offset_start,
            payload.offset_end,
        );
    }

    if let Some((tag_name, kind)) = dji_key_mapping(payload.key) {
        return match kind {
            UdtaValueKind::Float => parse_signed_float(&text)
                .map(|v| {
                    vec![MetadataEntry {
                        namespace: "dji".into(),
                        tag_id: tag_name.into(),
                        tag_name: tag_name.into(),
                        value: TypedValue::Float(v),
                        provenance: provenance(),
                        notes: vec![note.clone()],
                    }]
                })
                .unwrap_or_default(),
            UdtaValueKind::String => vec![MetadataEntry {
                namespace: "dji".into(),
                tag_id: tag_name.into(),
                tag_name: tag_name.into(),
                value: TypedValue::String(text.clone()),
                provenance: provenance(),
                notes: vec![note],
            }],
        };
    }

    // Unknown key — surface losslessly under the generic quicktime namespace.
    vec![MetadataEntry {
        namespace: "quicktime".into(),
        tag_id: payload.key.into(),
        tag_name: payload.key.into(),
        value: TypedValue::String(text),
        provenance: provenance(),
        notes: vec![note],
    }]
}

#[derive(Copy, Clone)]
enum UdtaValueKind {
    Float,
    String,
}

fn dji_key_mapping(key: &str) -> Option<(&'static str, UdtaValueKind)> {
    Some(match key {
        "\u{a9}fpt" => ("FlightPitchDegree", UdtaValueKind::Float),
        "\u{a9}fyw" => ("FlightYawDegree", UdtaValueKind::Float),
        "\u{a9}frl" => ("FlightRollDegree", UdtaValueKind::Float),
        "\u{a9}gpt" => ("GimbalPitchDegree", UdtaValueKind::Float),
        "\u{a9}gyw" => ("GimbalYawDegree", UdtaValueKind::Float),
        "\u{a9}grl" => ("GimbalRollDegree", UdtaValueKind::Float),
        "\u{a9}xsp" => ("SpeedX", UdtaValueKind::Float),
        "\u{a9}ysp" => ("SpeedY", UdtaValueKind::Float),
        "\u{a9}zsp" => ("SpeedZ", UdtaValueKind::Float),
        "\u{a9}mdl" => ("Model", UdtaValueKind::String),
        "\u{a9}csn" => ("SerialNumber", UdtaValueKind::String),
        _ => return None,
    })
}

fn decode_udta_text(bytes: &[u8]) -> Option<String> {
    // First try the canonical classic-QuickTime shape: `{u16 BE length}
    // {u16 BE language}{ascii bytes of length}`. This is what DJI uses for
    // ©fpt/©fyw/©frl/©gpt/©gyw/©grl/©xsp/©ysp/©zsp/©xyz on real captures.
    if bytes.len() >= 4 {
        let len = u16::from_be_bytes(bytes[0..2].try_into().ok()?) as usize;
        if 4 + len <= bytes.len() {
            let body = &bytes[4..4 + len];
            if let Ok(s) = std::str::from_utf8(body) {
                let trimmed = s.trim_end_matches('\0');
                if !trimmed.is_empty() && is_ascii_printable(trimmed) {
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    // Fallback: some DJI firmware (FC3682 / Mavic 3 confirmed) writes ©mdl
    // and ©csn as raw null-terminated ASCII in a fixed-size atom (e.g.
    // 32-byte ©mdl, 48-byte ©csn) with NO {len,lang} header. Treat the
    // entire payload as null-padded ASCII when the length-prefixed parse
    // can't produce a clean printable string.
    if let Ok(raw) = std::str::from_utf8(bytes) {
        let trimmed = raw.trim_end_matches('\0');
        if !trimmed.is_empty() && is_ascii_printable(trimmed) {
            return Some(trimmed.to_string());
        }
    }
    None
}

fn is_ascii_printable(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| (0x20..=0x7E).contains(&b))
}

fn parse_signed_float(text: &str) -> Option<f64> {
    // DJI emits values like "+0.90", "-3.90", "+175.50". `f64::parse`
    // already accepts a leading `+`, but trim defensively.
    text.trim().parse::<f64>().ok()
}

/// Split an ISO 6709 short-form coordinate string (e.g. `+40.7922-73.9584`,
/// `+40.7922-73.9584+050.000/`) into latitude / longitude / optional altitude
/// MetadataEntries under namespace `quicktime` (since `©xyz` is a standard
/// QuickTime tag, not DJI-specific).
fn decode_iso6709(
    text: &str,
    container: &str,
    offset_start: u64,
    offset_end: u64,
) -> Vec<MetadataEntry> {
    let cleaned = text.trim_end_matches('/');
    let mut parts = Vec::new();
    let bytes = cleaned.as_bytes();
    let mut start = 0usize;
    for (idx, &b) in bytes.iter().enumerate() {
        if idx > 0 && (b == b'+' || b == b'-') {
            parts.push(&cleaned[start..idx]);
            start = idx;
        }
    }
    parts.push(&cleaned[start..]);
    if parts.len() < 2 {
        return Vec::new();
    }
    let lat: f64 = match parts[0].parse() {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let lon: f64 = match parts[1].parse() {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let alt: Option<f64> = parts.get(2).and_then(|p| p.parse().ok());

    let prov = || Provenance {
        container: container.into(),
        namespace: "quicktime-udta".into(),
        path: Some("udta/\u{a9}xyz".into()),
        offset_start: Some(offset_start),
        offset_end: Some(offset_end),
        notes: Vec::new(),
    };
    let note = "decoded from quicktime ©xyz ISO 6709 location";

    let mut out = vec![
        MetadataEntry {
            namespace: "quicktime".into(),
            tag_id: "GPSLatitude".into(),
            tag_name: "GPSLatitude".into(),
            value: TypedValue::Float(lat),
            provenance: prov(),
            notes: vec![note.into()],
        },
        MetadataEntry {
            namespace: "quicktime".into(),
            tag_id: "GPSLongitude".into(),
            tag_name: "GPSLongitude".into(),
            value: TypedValue::Float(lon),
            provenance: prov(),
            notes: vec![note.into()],
        },
    ];
    if let Some(altitude) = alt {
        out.push(MetadataEntry {
            namespace: "quicktime".into(),
            tag_id: "GPSAltitude".into(),
            tag_name: "GPSAltitude".into(),
            value: TypedValue::Float(altitude),
            provenance: prov(),
            notes: vec![note.into()],
        });
    }
    out
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

    fn udta_text_atom(text: &str) -> Vec<u8> {
        let body = {
            let mut v = text.as_bytes().to_vec();
            v.push(0);
            v
        };
        let mut out = Vec::new();
        out.extend_from_slice(&(body.len() as u16).to_be_bytes());
        out.extend_from_slice(&0xFF7Fu16.to_be_bytes());
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn decodes_dji_signed_float_udta_atom() {
        let bytes = udta_text_atom("-3.90");
        let entries = decode_udta_payload(QuickTimeUdtaPayload {
            key: "\u{a9}frl",
            bytes: &bytes,
            container: "mp4",
            offset_start: 0,
            offset_end: bytes.len() as u64,
        });
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].namespace, "dji");
        assert_eq!(entries[0].tag_name, "FlightRollDegree");
        match &entries[0].value {
            TypedValue::Float(v) => assert!((v - -3.9).abs() < 1e-9),
            v => panic!("expected float, got {v:?}"),
        }
    }

    #[test]
    fn decodes_dji_string_udta_atom() {
        let bytes = udta_text_atom("FC3682");
        let entries = decode_udta_payload(QuickTimeUdtaPayload {
            key: "\u{a9}mdl",
            bytes: &bytes,
            container: "mp4",
            offset_start: 0,
            offset_end: bytes.len() as u64,
        });
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].namespace, "dji");
        assert_eq!(entries[0].tag_name, "Model");
        assert!(matches!(&entries[0].value, TypedValue::String(s) if s == "FC3682"));
    }

    #[test]
    fn splits_iso6709_lat_lon_into_three_entries_with_altitude() {
        let bytes = udta_text_atom("+40.7922-73.9584+050.000/");
        let entries = decode_udta_payload(QuickTimeUdtaPayload {
            key: "\u{a9}xyz",
            bytes: &bytes,
            container: "mp4",
            offset_start: 0,
            offset_end: bytes.len() as u64,
        });
        let names: Vec<&str> = entries.iter().map(|e| e.tag_name.as_str()).collect();
        assert_eq!(names, vec!["GPSLatitude", "GPSLongitude", "GPSAltitude"]);
        assert!(entries.iter().all(|e| e.namespace == "quicktime"));
        match &entries[0].value {
            TypedValue::Float(v) => assert!((v - 40.7922).abs() < 1e-6),
            v => panic!("expected float, got {v:?}"),
        }
        match &entries[1].value {
            TypedValue::Float(v) => assert!((v - -73.9584).abs() < 1e-6),
            v => panic!("expected float, got {v:?}"),
        }
        match &entries[2].value {
            TypedValue::Float(v) => assert!((v - 50.0).abs() < 1e-6),
            v => panic!("expected float, got {v:?}"),
        }
    }

    #[test]
    fn splits_iso6709_lat_lon_without_altitude() {
        let bytes = udta_text_atom("+40.7922-73.9584");
        let entries = decode_udta_payload(QuickTimeUdtaPayload {
            key: "\u{a9}xyz",
            bytes: &bytes,
            container: "mp4",
            offset_start: 0,
            offset_end: bytes.len() as u64,
        });
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn unknown_udta_key_passes_through_under_quicktime_namespace() {
        let bytes = udta_text_atom("hello");
        let entries = decode_udta_payload(QuickTimeUdtaPayload {
            key: "\u{a9}foo",
            bytes: &bytes,
            container: "mp4",
            offset_start: 0,
            offset_end: bytes.len() as u64,
        });
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].namespace, "quicktime");
        assert_eq!(entries[0].tag_name, "\u{a9}foo");
    }

    #[test]
    fn decodes_dji_raw_null_padded_ascii_for_mdl() {
        // FC3682 firmware stores ©mdl as 24 bytes of "FC3682" followed by
        // NUL padding — no {len,lang} header. The first two bytes ("FC" =
        // 0x4643 = 17987) interpreted as a length would overshoot, so the
        // length-prefixed parse must fall through to the raw-ASCII path.
        let mut bytes = Vec::from(b"FC3682".as_slice());
        bytes.resize(24, 0);
        let entries = decode_udta_payload(QuickTimeUdtaPayload {
            key: "\u{a9}mdl",
            bytes: &bytes,
            container: "mp4",
            offset_start: 0,
            offset_end: bytes.len() as u64,
        });
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].tag_name, "Model");
        assert!(matches!(&entries[0].value, TypedValue::String(s) if s == "FC3682"));
    }

    #[test]
    fn rejects_truncated_udta_atom() {
        // length=0x10 but body only has 3 bytes after the header.
        let bytes = vec![0x00, 0x10, 0xFF, 0x7F, b'a', b'b', b'c'];
        let entries = decode_udta_payload(QuickTimeUdtaPayload {
            key: "\u{a9}fpt",
            bytes: &bytes,
            container: "mp4",
            offset_start: 0,
            offset_end: bytes.len() as u64,
        });
        assert!(entries.is_empty());
    }

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
