//! End-to-end test: decode the committed synthetic PNG fixture and verify
//! every bounded-template `c2pa.*` / `ai.*` field surfaces.

use std::path::Path;

use xifty_meta_c2pa::{C2paPayload, decode_jumbf};

fn read_cabx(png_bytes: &[u8]) -> Option<Vec<u8>> {
    if !png_bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return None;
    }
    let mut offset = 8usize;
    while offset + 12 <= png_bytes.len() {
        let length = u32::from_be_bytes([
            png_bytes[offset],
            png_bytes[offset + 1],
            png_bytes[offset + 2],
            png_bytes[offset + 3],
        ]) as usize;
        let chunk_type = &png_bytes[offset + 4..offset + 8];
        let data_start = offset + 8;
        let data_end = data_start + length;
        if data_end > png_bytes.len() {
            return None;
        }
        if chunk_type == b"caBX" {
            return Some(png_bytes[data_start..data_end].to_vec());
        }
        offset = data_end + 4; // skip CRC
    }
    None
}

#[test]
fn synthetic_fixture_round_trips() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/minimal/c2pa_synthetic.png");
    let bytes = std::fs::read(&path).expect("synthetic fixture present");
    let cabx = read_cabx(&bytes).expect("synthetic PNG contains caBX chunk");
    let (entries, issues) = decode_jumbf(C2paPayload {
        bytes: &cabx,
        container: "png",
        path: "caBX",
        offset_start: 0,
        offset_end: cabx.len() as u64,
    });
    let no_warning = issues
        .iter()
        .filter(|i| !matches!(i.severity, xifty_core::Severity::Info))
        .count();
    assert_eq!(no_warning, 0, "no warnings expected, got {:?}", issues);

    let names: Vec<&str> = entries.iter().map(|e| e.tag_name.as_str()).collect();
    for required in [
        "c2pa.claim_generator",
        "c2pa.format",
        "c2pa.instance_id",
        "c2pa.signature.alg",
        "c2pa.signature.issuer",
        "c2pa.signature.verified",
        "c2pa.assertions.0.action",
        "c2pa.assertions.0.digitalSourceType",
        "c2pa.assertions.0.softwareAgent",
        "c2pa.assertions.0.when",
        "c2pa.assertions.0.description",
        "c2pa.assertions.0.label",
        "c2pa.ai_generated",
        "ai.source_type",
    ] {
        assert!(
            names.contains(&required),
            "missing required field {required}; got {:?}",
            names
        );
    }

    // Spot-check key values.
    let value_of = |tag: &str| {
        entries
            .iter()
            .find(|e| e.tag_name == tag)
            .map(|e| match &e.value {
                xifty_core::TypedValue::String(s) => s.clone(),
                other => format!("{:?}", other),
            })
            .unwrap_or_default()
    };
    assert_eq!(value_of("c2pa.claim_generator"), "XIFty Test Suite/0.1.0");
    assert_eq!(value_of("c2pa.format"), "image/png");
    assert_eq!(value_of("c2pa.signature.alg"), "eddsa");
    assert_eq!(value_of("c2pa.signature.verified"), "unknown");
    assert_eq!(value_of("c2pa.ai_generated"), "true");
    assert_eq!(value_of("ai.source_type"), "trainedAlgorithmicMedia");
    assert_eq!(value_of("c2pa.assertions.0.action"), "c2pa.created");
}
