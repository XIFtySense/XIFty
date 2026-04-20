use xifty_core::{MetadataEntry, Provenance, TypedValue};
use xifty_validate::build_report;

fn prov(namespace: &str) -> Provenance {
    Provenance {
        container: "jpeg".into(),
        namespace: namespace.into(),
        path: None,
        offset_start: None,
        offset_end: None,
        notes: Vec::new(),
    }
}

fn string_entry(namespace: &str, tag: &str, value: &str) -> MetadataEntry {
    MetadataEntry {
        namespace: namespace.into(),
        tag_id: tag.into(),
        tag_name: tag.into(),
        value: TypedValue::String(value.into()),
        provenance: prov(namespace),
        notes: Vec::new(),
    }
}

fn timestamp_entry(namespace: &str, tag: &str, value: &str) -> MetadataEntry {
    MetadataEntry {
        namespace: namespace.into(),
        tag_id: tag.into(),
        tag_name: tag.into(),
        value: TypedValue::Timestamp(value.into()),
        provenance: prov(namespace),
        notes: Vec::new(),
    }
}

fn int_entry(namespace: &str, tag: &str, value: i64) -> MetadataEntry {
    MetadataEntry {
        namespace: namespace.into(),
        tag_id: tag.into(),
        tag_name: tag.into(),
        value: TypedValue::Integer(value),
        provenance: prov(namespace),
        notes: Vec::new(),
    }
}

#[test]
fn cross_namespace_string_disagreement_is_reported() {
    // Canon vs Nikon — the fixture pair called out explicitly in the plan.
    let entries = vec![
        string_entry("exif", "Make", "Canon"),
        string_entry("xmp", "Make", "Nikon"),
    ];
    let report = build_report(Vec::new(), &entries);
    let hits: Vec<_> = report
        .conflicts
        .iter()
        .filter(|c| c.field == "device.make")
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly one device.make conflict, got: {:?}",
        report.conflicts
    );
    assert!(hits[0].message.contains("Canon"));
    assert!(hits[0].message.contains("Nikon"));
}

#[test]
fn timestamp_offset_mismatch_is_reported() {
    let entries = vec![
        timestamp_entry("exif", "DateTimeOriginal", "2024-01-01T10:00:00+00:00"),
        timestamp_entry("xmp", "CreateDate", "2024-01-01T10:00:00-05:00"),
    ];
    let report = build_report(Vec::new(), &entries);
    let hits: Vec<_> = report
        .conflicts
        .iter()
        .filter(|c| c.field == "captured_at" && c.message.contains("timezone offset"))
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly one timezone offset conflict, got: {:?}",
        report.conflicts
    );
}

#[test]
fn numeric_precision_mismatch_is_reported() {
    let entries = vec![int_entry("exif", "ISO", 200), int_entry("xmp", "ISO", 400)];
    let report = build_report(Vec::new(), &entries);
    let hits: Vec<_> = report
        .conflicts
        .iter()
        .filter(|c| c.field == "exposure.iso")
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "expected one exposure.iso conflict, got: {:?}",
        report.conflicts
    );
}

#[test]
fn numeric_agreement_across_namespaces_produces_no_conflict() {
    let entries = vec![
        int_entry("exif", "ISO", 200),
        int_entry("exif", "ISOSpeedRatings", 200),
    ];
    let report = build_report(Vec::new(), &entries);
    assert!(
        !report.conflicts.iter().any(|c| c.field == "exposure.iso"),
        "unexpected exposure.iso conflict: {:?}",
        report.conflicts
    );
}

#[test]
fn agreement_across_namespaces_produces_no_conflict() {
    let entries = vec![
        string_entry("exif", "Make", "Canon"),
        string_entry("xmp", "Make", "canon"),
        string_entry("exif", "Model", "EOS R5"),
        string_entry("xmp", "Model", "EOS R5"),
        timestamp_entry("exif", "DateTimeOriginal", "2024-01-01T10:00:00+00:00"),
        timestamp_entry("xmp", "CreateDate", "2024-01-01T10:00:00+00:00"),
        int_entry("exif", "ISO", 200),
        int_entry("xmp", "ISO", 200),
    ];
    let report = build_report(Vec::new(), &entries);
    assert!(
        report.conflicts.is_empty(),
        "expected no conflicts on agreement, got: {:?}",
        report.conflicts
    );
}
