//! Integration test: round-trip a synthetic Nikon TIFF through the public
//! decoder API and assert that plain entries decode AND encrypted regions
//! report correct outer-file absolute offsets.
//!
//! The synthetic fixture mirrors the layout produced by
//! `tools/generate_fixtures.py::build_nikon_nef`: a TIFF header + IFD0 with
//! Make=NIKON CORPORATION + a Nikon TIFF-in-TIFF v2/v3 MakerNote containing
//! both a plain Quality tag and a synthetic ShotInfo (0x0091) encrypted
//! block.

use xifty_core::{MetadataEntry, Provenance, TypedValue};
use xifty_meta_nikon::{decode_from_tiff, encrypted_regions};

fn build_synthetic_nikon_tiff() -> (Vec<u8>, u64) {
    let p16 = |v: u16| v.to_le_bytes();
    let p32 = |v: u32| v.to_le_bytes();
    // --- Inner Nikon TIFF-in-TIFF MakerNote payload ---
    // Layout:
    //   bytes 0..6   "Nikon\x00\x02"
    //   byte  6      minor version (0x10)
    //   bytes 7..9   "\x00\x00"
    //   bytes 10..18 inner TIFF header (II*\0 + first-IFD offset = 8)
    //   inner offset 8: count(2) + 2 entries(24) + next(4) = ends at inner 38
    //   inner offset 38: blob bytes (Quality "FINE\0", then ShotInfo)
    let quality_blob = b"FINE\x00";
    let shot_info_blob = b"\x02\x04abcdefghijklmnop"; // v0204 + 16 bytes
    let inner_quality_off: u32 = 38;
    let inner_shot_info_off: u32 = inner_quality_off + quality_blob.len() as u32;

    let mut payload = Vec::new();
    payload.extend_from_slice(b"Nikon\x00\x02");
    payload.push(0x10);
    payload.extend_from_slice(&[0x00, 0x00]);
    payload.extend_from_slice(b"II*\0");
    payload.extend_from_slice(&p32(8));
    payload.extend_from_slice(&p16(2));
    // 0x0004 Quality, ASCII, count = 5, out-of-line.
    payload.extend_from_slice(&p16(0x0004));
    payload.extend_from_slice(&p16(2));
    payload.extend_from_slice(&p32(quality_blob.len() as u32));
    payload.extend_from_slice(&p32(inner_quality_off));
    // 0x0091 ShotInfo, UNDEFINED, encrypted blob, out-of-line.
    payload.extend_from_slice(&p16(0x0091));
    payload.extend_from_slice(&p16(7));
    payload.extend_from_slice(&p32(shot_info_blob.len() as u32));
    payload.extend_from_slice(&p32(inner_shot_info_off));
    payload.extend_from_slice(&p32(0));
    payload.extend_from_slice(quality_blob);
    payload.extend_from_slice(shot_info_blob);

    // --- Outer TIFF: header(8) + IFD0 count(2) + 2 entries(24) + next(4) = 38 ---
    let make = b"NIKON CORPORATION\0";
    let make_off: u32 = 38;
    let maker_off: u32 = make_off + make.len() as u32;
    let mut out = Vec::new();
    out.extend_from_slice(b"II*\0");
    out.extend_from_slice(&p32(8));
    out.extend_from_slice(&p16(2));
    out.extend_from_slice(&p16(0x010F));
    out.extend_from_slice(&p16(2));
    out.extend_from_slice(&p32(make.len() as u32));
    out.extend_from_slice(&p32(make_off));
    out.extend_from_slice(&p16(0x927C));
    out.extend_from_slice(&p16(7));
    out.extend_from_slice(&p32(payload.len() as u32));
    out.extend_from_slice(&p32(maker_off));
    out.extend_from_slice(&p32(0));
    out.extend_from_slice(make);
    out.extend_from_slice(&payload);
    // Expected absolute offset of the encrypted ShotInfo blob in the outer
    // file: maker_off + 10 (inner TIFF header) + inner_shot_info_off.
    let expected_abs = maker_off as u64 + 10 + inner_shot_info_off as u64;
    (out, expected_abs)
}

fn exif_make_nikon() -> Vec<MetadataEntry> {
    vec![MetadataEntry {
        namespace: "exif".into(),
        tag_id: "0x010F".into(),
        tag_name: "Make".into(),
        value: TypedValue::String("NIKON CORPORATION".into()),
        provenance: Provenance {
            container: "nef".into(),
            namespace: "exif".into(),
            path: None,
            offset_start: None,
            offset_end: None,
            notes: Vec::new(),
        },
        notes: Vec::new(),
    }]
}

#[test]
fn synthetic_nef_decodes_plain_and_reports_encrypted_with_outer_offsets() {
    let (bytes, expected_abs) = build_synthetic_nikon_tiff();
    let tiff = xifty_container_tiff::parse_bytes(&bytes, 0, "nef").unwrap();
    let exif = exif_make_nikon();

    let entries = decode_from_tiff(&bytes, 0, "nef", &tiff, &exif);
    let quality = entries
        .iter()
        .find(|e| e.tag_name == "Quality")
        .expect("plain Quality decoded");
    assert_eq!(quality.namespace, "nikon");
    assert_eq!(quality.value, TypedValue::String("FINE".into()));
    assert!(
        !entries.iter().any(|e| e.tag_id == "0x0091"),
        "encrypted ShotInfo (0x0091) must NOT appear in decoded entries"
    );

    let regions = encrypted_regions(&bytes, 0, &tiff);
    assert_eq!(
        regions.len(),
        1,
        "expected one encrypted region: {regions:?}"
    );
    let region = &regions[0];
    assert_eq!(region.tag_id, 0x0091);
    assert_eq!(
        region.absolute_offset, expected_abs,
        "EncryptedRegion.absolute_offset must be in outer-file coordinates"
    );

    // Sanity: that absolute offset, when read directly from the outer
    // buffer, must point at the encrypted blob's first byte (the version
    // preamble).
    assert_eq!(bytes[expected_abs as usize], 0x02);
}
