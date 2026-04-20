use insta::assert_json_snapshot;
use serde_json::Value;
use std::{path::Path, process::Command, sync::OnceLock};
use xifty_core::ViewMode;

static EXIFTOOL_AVAILABLE: OnceLock<bool> = OnceLock::new();

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/minimal")
        .join(name)
}

fn local_fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/local")
        .join(name)
}

fn optional_fixture(name: &str) -> Option<std::path::PathBuf> {
    let local = local_fixture(name);
    local.exists().then_some(local)
}

fn scrub_path(value: &mut Value) {
    if let Some(path_value) = value
        .get_mut("input")
        .and_then(|input| input.get_mut("path"))
    {
        if let Some(path) = path_value.as_str().map(str::to_string) {
            let name = Path::new(&path)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string();
            value["input"]["path"] = Value::String(name);
        }
    }
}

fn extract_json(name: &str, view: ViewMode) -> Value {
    let mut value =
        serde_json::to_value(xifty_cli::extract_path(fixture(name), view).unwrap()).unwrap();
    scrub_path(&mut value);
    value
}

fn extract_optional_json(name: &str, view: ViewMode) -> Option<Value> {
    let path = optional_fixture(name)?;
    let mut value = serde_json::to_value(xifty_cli::extract_path(path, view).unwrap()).unwrap();
    scrub_path(&mut value);
    Some(value)
}

fn probe_json(name: &str) -> Value {
    let mut value = serde_json::to_value(xifty_cli::probe_path(fixture(name)).unwrap()).unwrap();
    scrub_path(&mut value);
    value
}

fn skip_missing_local_fixture(name: &str) {
    eprintln!("skipping optional local fixture test for {name}");
}

fn ensure_exiftool_available() -> bool {
    let available = *EXIFTOOL_AVAILABLE.get_or_init(|| {
        Command::new("exiftool")
            .arg("-ver")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    });

    if !available {
        if std::env::var("XIFTY_REQUIRE_EXIFTOOL").as_deref() == Ok("1") {
            panic!("ExifTool is required for oracle-backed differential tests");
        }
        eprintln!("skipping ExifTool-backed differential test because exiftool is unavailable");
    }

    available
}

fn normalized_map(output: &Value) -> std::collections::BTreeMap<String, Value> {
    output["normalized"]["fields"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .map(|field| {
            (
                field["field"].as_str().unwrap().to_string(),
                field["value"].clone(),
            )
        })
        .collect()
}

fn interpreted_value(output: &Value, namespace: &str, tag_name: &str) -> Option<Value> {
    output["interpreted"]["metadata"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .find(|entry| {
            entry["namespace"].as_str() == Some(namespace)
                && entry["tag_name"].as_str() == Some(tag_name)
        })
        .map(|entry| entry["value"]["value"].clone())
}

fn json_stringified(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn normalized_decimal_string(value: &Value) -> Option<String> {
    let text = json_stringified(value)?;
    let number = text.parse::<f64>().ok()?;
    Some(format!("{number}"))
}

fn rational_json_to_f64(value: &Value) -> Option<f64> {
    let numerator = value["value"]["numerator"].as_i64()?;
    let denominator = value["value"]["denominator"].as_i64()?;
    if denominator == 0 {
        return None;
    }
    Some(numerator as f64 / denominator as f64)
}

#[test]
fn probe_snapshot_happy_jpeg() {
    assert_json_snapshot!("probe_happy_jpeg", probe_json("happy.jpg"));
}

#[test]
fn probe_snapshot_happy_png() {
    assert_json_snapshot!("probe_happy_png", probe_json("happy.png"));
}

#[test]
fn probe_snapshot_happy_webp() {
    assert_json_snapshot!("probe_happy_webp", probe_json("happy.webp"));
}

#[test]
fn probe_snapshot_happy_heic() {
    assert_json_snapshot!("probe_happy_heic", probe_json("happy.heic"));
}

#[test]
fn probe_snapshot_happy_mp4() {
    assert_json_snapshot!("probe_happy_mp4", probe_json("happy.mp4"));
}

#[test]
fn probe_snapshot_happy_mov() {
    assert_json_snapshot!("probe_happy_mov", probe_json("happy.mov"));
}

#[test]
fn extract_snapshot_happy_jpeg() {
    assert_json_snapshot!(
        "extract_happy_jpeg",
        extract_json("happy.jpg", ViewMode::Full)
    );
}

#[test]
fn extract_snapshot_gps_jpeg_normalized() {
    assert_json_snapshot!(
        "extract_gps_jpeg_normalized",
        extract_json("gps.jpg", ViewMode::Normalized)
    );
}

#[test]
fn extract_snapshot_big_endian_tiff_normalized() {
    assert_json_snapshot!(
        "extract_big_endian_tiff_normalized",
        extract_json("big_endian.tiff", ViewMode::Normalized)
    );
}

#[test]
fn malformed_tiff_report_snapshot() {
    assert_json_snapshot!(
        "malformed_tiff_report",
        extract_json("malformed_offsets.tiff", ViewMode::Report)
    );
}

#[test]
fn malformed_jpeg_report_snapshot() {
    assert_json_snapshot!(
        "malformed_jpeg_report",
        extract_json("malformed_app1.jpg", ViewMode::Report)
    );
}

#[test]
fn extract_snapshot_happy_png_report() {
    assert_json_snapshot!(
        "extract_happy_png_report",
        extract_json("happy.png", ViewMode::Report)
    );
}

#[test]
fn extract_snapshot_happy_webp_report() {
    assert_json_snapshot!(
        "extract_happy_webp_report",
        extract_json("happy.webp", ViewMode::Report)
    );
}

#[test]
fn extract_snapshot_xmp_only_png_normalized() {
    assert_json_snapshot!(
        "extract_xmp_only_png_normalized",
        extract_json("xmp_only.png", ViewMode::Normalized)
    );
}

#[test]
fn extract_snapshot_xmp_only_webp_normalized() {
    assert_json_snapshot!(
        "extract_xmp_only_webp_normalized",
        extract_json("xmp_only.webp", ViewMode::Normalized)
    );
}

#[test]
fn extract_snapshot_mixed_png_normalized() {
    assert_json_snapshot!(
        "extract_mixed_png_normalized",
        extract_json("mixed.png", ViewMode::Normalized)
    );
}

#[test]
fn extract_snapshot_mixed_webp_normalized() {
    assert_json_snapshot!(
        "extract_mixed_webp_normalized",
        extract_json("mixed.webp", ViewMode::Normalized)
    );
}

#[test]
fn extract_snapshot_mixed_heic_normalized() {
    assert_json_snapshot!(
        "extract_mixed_heic_normalized",
        extract_json("mixed.heic", ViewMode::Normalized)
    );
}

#[test]
fn extract_snapshot_real_exif_heic_normalized() {
    assert_json_snapshot!(
        "extract_real_exif_heic_normalized",
        extract_json("real_exif.heic", ViewMode::Normalized)
    );
}

#[test]
fn extract_snapshot_happy_mp4_normalized() {
    assert_json_snapshot!(
        "extract_happy_mp4_normalized",
        extract_json("happy.mp4", ViewMode::Normalized)
    );
}

#[test]
fn extract_snapshot_happy_mov_normalized() {
    assert_json_snapshot!(
        "extract_happy_mov_normalized",
        extract_json("happy.mov", ViewMode::Normalized)
    );
}

#[test]
fn extract_snapshot_video_only_mp4_normalized() {
    assert_json_snapshot!(
        "extract_video_only_mp4_normalized",
        extract_json("video_only.mp4", ViewMode::Normalized)
    );
}

#[test]
fn extract_snapshot_icc_jpeg_normalized() {
    assert_json_snapshot!(
        "extract_icc_jpeg_normalized",
        extract_json("icc.jpg", ViewMode::Normalized)
    );
}

#[test]
fn extract_snapshot_xmp_tiff_normalized() {
    assert_json_snapshot!(
        "extract_xmp_tiff_normalized",
        extract_json("xmp.tiff", ViewMode::Normalized)
    );
}

#[test]
fn extract_snapshot_icc_tiff_normalized() {
    assert_json_snapshot!(
        "extract_icc_tiff_normalized",
        extract_json("icc.tiff", ViewMode::Normalized)
    );
}

#[test]
fn extract_snapshot_iptc_tiff_normalized() {
    assert_json_snapshot!(
        "extract_iptc_tiff_normalized",
        extract_json("iptc.tiff", ViewMode::Normalized)
    );
}

#[test]
fn extract_snapshot_iptc_jpeg_normalized() {
    assert_json_snapshot!(
        "extract_iptc_jpeg_normalized",
        extract_json("iptc.jpg", ViewMode::Normalized)
    );
}

#[test]
fn extract_snapshot_iptc_png_normalized() {
    assert_json_snapshot!(
        "extract_iptc_png_normalized",
        extract_json("iptc.png", ViewMode::Normalized)
    );
}

#[test]
fn extract_snapshot_iptc_webp_normalized() {
    assert_json_snapshot!(
        "extract_iptc_webp_normalized",
        extract_json("iptc.webp", ViewMode::Normalized)
    );
}

#[test]
fn icc_jpeg_normalization_includes_color_fields() {
    let output = normalized_map(&extract_json("icc.jpg", ViewMode::Normalized));
    assert_eq!(
        output["color.profile.name"]["value"],
        Value::String("XIFty Display Profile".into())
    );
    assert_eq!(
        output["color.profile.class"]["value"],
        Value::String("display".into())
    );
    assert_eq!(output["color.space"]["value"], Value::String("RGB".into()));
}

#[test]
fn iptc_jpeg_normalization_includes_editorial_fields() {
    let output = normalized_map(&extract_json("iptc.jpg", ViewMode::Normalized));
    assert_eq!(output["author"]["value"], Value::String("Kai".into()));
    assert_eq!(
        output["headline"]["value"],
        Value::String("XIFty Headline".into())
    );
    assert_eq!(
        output["description"]["value"],
        Value::String("XIFty Caption".into())
    );
    assert_eq!(output["copyright"]["value"], Value::String("XIFty".into()));
    assert_eq!(
        output["keywords"]["value"],
        Value::String("xifty, metadata".into())
    );
}

#[test]
fn no_iptc_jpeg_omits_iptc_namespace_entries() {
    let output = extract_json("no_iptc.jpg", ViewMode::Interpreted);
    let metadata = output
        .get("interpreted")
        .and_then(|view| view.get("metadata"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert!(
        !metadata
            .iter()
            .any(|entry| entry["namespace"].as_str() == Some("iptc"))
    );
}

#[test]
fn no_icc_png_omits_icc_namespace_entries() {
    let output = extract_json("no_icc.png", ViewMode::Interpreted);
    let metadata = output
        .get("interpreted")
        .and_then(|view| view.get("metadata"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert!(
        !metadata
            .iter()
            .any(|entry| entry["namespace"].as_str() == Some("icc"))
    );
}

#[test]
fn overlap_editorial_jpeg_prefers_xmp_for_editorial_fields() {
    let output = extract_json("overlap_editorial.jpg", ViewMode::Full);
    let normalized = normalized_map(&output);

    assert_eq!(
        normalized["author"]["value"],
        Value::String("XMP Kai".into())
    );
    assert_eq!(
        normalized["copyright"]["value"],
        Value::String("XMP Rights".into())
    );
    assert_eq!(
        normalized["headline"]["value"],
        Value::String("XIFty XMP Headline".into())
    );
    assert_eq!(
        normalized["description"]["value"],
        Value::String("XIFty XMP Description".into())
    );

    let conflicts = output["report"]["conflicts"].as_array().unwrap();
    assert!(
        conflicts
            .iter()
            .any(|conflict| conflict["field"] == "author")
    );
    assert!(
        conflicts
            .iter()
            .any(|conflict| conflict["field"] == "copyright")
    );
    assert!(
        conflicts
            .iter()
            .any(|conflict| conflict["field"] == "headline")
    );
    assert!(
        conflicts
            .iter()
            .any(|conflict| conflict["field"] == "description")
    );

    let author_conflict = conflicts
        .iter()
        .find(|conflict| conflict["field"] == "author")
        .expect("missing author conflict");
    let sources = author_conflict["sources"]
        .as_array()
        .expect("author conflict missing sources");
    assert!(
        sources.len() >= 2,
        "expected at least two conflicting sources for author, got {}",
        sources.len()
    );
    let namespaces: std::collections::BTreeSet<&str> = sources
        .iter()
        .filter_map(|side| side["provenance"]["namespace"].as_str())
        .collect();
    assert!(
        namespaces.contains("xmp"),
        "expected xmp namespace in author sources, got {:?}",
        namespaces
    );
    assert!(
        namespaces.contains("exif") || namespaces.contains("iptc"),
        "expected exif or iptc namespace in author sources, got {:?}",
        namespaces
    );
    assert_eq!(
        sources[0]["provenance"]["namespace"].as_str(),
        Some("xmp"),
        "winner should appear first in sources"
    );
}

#[test]
fn conflicting_png_report_exposes_source_namespaces() {
    let output = extract_json("conflicting.png", ViewMode::Report);
    let conflicts = output["report"]["conflicts"].as_array().unwrap();

    let captured_at = conflicts
        .iter()
        .find(|conflict| {
            conflict["field"] == "captured_at"
                && conflict["message"]
                    .as_str()
                    .is_some_and(|msg| msg.contains("selected"))
        })
        .expect("missing captured_at conflict with winner");
    let sources = captured_at["sources"]
        .as_array()
        .expect("captured_at conflict missing sources");
    assert!(
        sources.len() >= 2,
        "expected at least two sides for captured_at conflict, got {}",
        sources.len()
    );
    let namespaces: std::collections::BTreeSet<&str> = sources
        .iter()
        .filter_map(|side| side["provenance"]["namespace"].as_str())
        .collect();
    assert!(
        namespaces.contains("exif") && namespaces.contains("xmp"),
        "expected both exif and xmp namespaces in captured_at sources, got {:?}",
        namespaces
    );
    assert_eq!(
        sources[0]["provenance"]["namespace"].as_str(),
        Some("exif"),
        "winner should appear first in sources"
    );
    for side in sources {
        assert!(side.get("tag_id").and_then(Value::as_str).is_some());
        assert!(side.get("tag_name").and_then(Value::as_str).is_some());
        assert!(side.get("value").is_some());
    }
}

#[test]
fn validate_rules_fire_end_to_end_on_cross_namespace_fixture() {
    // Exercises the conflict-detection pipeline end-to-end on a fixture
    // whose only disagreement is a cross-namespace `device.make` string
    // (exif="Canon" vs xmp:tiff:Make="Nikon"). The xifty-validate
    // cross-namespace string-disagreement rule fires on this pairing
    // (rules.rs:94-137); its message is collapsed against the policy-layer
    // conflict by the CLI-side dedupe (see crates/xifty-cli/src/conflict_dedupe.rs),
    // so we assert on observable evidence rather than message format:
    // the device.make conflict must reach `report.conflicts` with both
    // raw values and both source namespaces preserved.
    // Timestamp and numeric rules deferred — see issue #43 plan notes.
    let output = extract_json("validate_conflicts.png", ViewMode::Report);
    let conflicts = output["report"]["conflicts"].as_array().unwrap();
    let make_conflict = conflicts
        .iter()
        .find(|c| c["field"] == "device.make")
        .expect("missing device.make conflict in report.conflicts");
    let sources = make_conflict["sources"].as_array().expect("sources array");
    assert!(
        sources.len() >= 2,
        "expected at least two sides for device.make conflict, got {}",
        sources.len()
    );
    let namespaces: std::collections::BTreeSet<&str> = sources
        .iter()
        .filter_map(|side| side["provenance"]["namespace"].as_str())
        .collect();
    assert!(
        namespaces.contains("exif") && namespaces.contains("xmp"),
        "expected both exif and xmp namespaces in device.make sources, got {:?}",
        namespaces
    );
    let values: std::collections::BTreeSet<&str> = sources
        .iter()
        .filter_map(|side| side["value"]["value"].as_str())
        .collect();
    assert!(
        values.contains("Canon") && values.contains("Nikon"),
        "expected Canon and Nikon raw source values, got {:?}",
        values
    );
}

#[test]
fn icc_png_interpreted_view_includes_icc_fields() {
    let output = extract_json("icc.png", ViewMode::Interpreted);
    assert_eq!(
        interpreted_value(&output, "icc", "ProfileClass"),
        Some(Value::String("display".into()))
    );
    assert_eq!(
        interpreted_value(&output, "icc", "ColorSpace"),
        Some(Value::String("RGB".into()))
    );
    assert_eq!(
        interpreted_value(&output, "icc", "ProfileDescription"),
        Some(Value::String("XIFty Display Profile".into()))
    );
}

#[test]
fn icc_webp_interpreted_view_includes_icc_fields() {
    let output = extract_json("icc.webp", ViewMode::Interpreted);
    assert_eq!(
        interpreted_value(&output, "icc", "ProfileClass"),
        Some(Value::String("display".into()))
    );
    assert_eq!(
        interpreted_value(&output, "icc", "ColorSpace"),
        Some(Value::String("RGB".into()))
    );
    assert_eq!(
        interpreted_value(&output, "icc", "ConnectionSpace"),
        Some(Value::String("XYZ".into()))
    );
    assert_eq!(
        interpreted_value(&output, "icc", "DeviceManufacturer"),
        Some(Value::String("XFTY".into()))
    );
    assert_eq!(
        interpreted_value(&output, "icc", "DeviceModel"),
        Some(Value::String("TEST".into()))
    );
    assert_eq!(
        interpreted_value(&output, "icc", "ProfileDescription"),
        Some(Value::String("XIFty Display Profile".into()))
    );
}

#[test]
fn extract_snapshot_real_camera_mp4_normalized() {
    let Some(output) = extract_optional_json("C0242.MP4", ViewMode::Normalized) else {
        skip_missing_local_fixture("C0242.MP4");
        return;
    };
    assert_json_snapshot!("extract_real_camera_mp4_normalized", output);
}

#[test]
fn extract_snapshot_real_camera_mp4_interpreted() {
    let Some(output) = extract_optional_json("C0242.MP4", ViewMode::Interpreted) else {
        skip_missing_local_fixture("C0242.MP4");
        return;
    };
    assert_json_snapshot!("extract_real_camera_mp4_interpreted", output);
}

#[test]
fn real_camera_mp4_normalization_includes_technical_media_fields() {
    let Some(output) = extract_optional_json("C0242.MP4", ViewMode::Normalized) else {
        skip_missing_local_fixture("C0242.MP4");
        return;
    };
    let output = normalized_map(&output);

    assert!(
        output
            .get("video.bitrate")
            .and_then(|value| value["value"].as_i64())
            .is_some_and(|value| value > 0),
        "expected non-zero video.bitrate"
    );
    assert!(
        output
            .get("audio.sample_rate")
            .and_then(|value| value["value"].as_i64())
            .is_some_and(|value| value > 0),
        "expected non-zero audio.sample_rate"
    );
}

#[test]
fn conflicting_png_report_snapshot() {
    assert_json_snapshot!(
        "conflicting_png_report",
        extract_json("conflicting.png", ViewMode::Report)
    );
}

#[test]
fn validate_conflicts_png_report_snapshot() {
    assert_json_snapshot!(
        "validate_conflicts_png_report",
        extract_json("validate_conflicts.png", ViewMode::Report)
    );
}

#[test]
fn malformed_png_report_snapshot() {
    assert_json_snapshot!(
        "malformed_png_report",
        extract_json("malformed_chunk.png", ViewMode::Report)
    );
}

#[test]
fn malformed_icc_png_report_snapshot() {
    assert_json_snapshot!(
        "malformed_icc_png_report",
        extract_json("malformed_icc.png", ViewMode::Report)
    );
}

#[test]
fn malformed_iptc_jpeg_report_snapshot() {
    assert_json_snapshot!(
        "malformed_iptc_jpeg_report",
        extract_json("malformed_iptc.jpg", ViewMode::Report)
    );
}

#[test]
fn malformed_webp_report_snapshot() {
    assert_json_snapshot!(
        "malformed_webp_report",
        extract_json("malformed_chunk.webp", ViewMode::Report)
    );
}

#[test]
fn unsupported_heic_report_snapshot() {
    assert_json_snapshot!(
        "unsupported_heic_report",
        extract_json("unsupported.heic", ViewMode::Report)
    );
}

#[test]
fn malformed_heic_report_snapshot() {
    assert_json_snapshot!(
        "malformed_heic_report",
        extract_json("malformed_box.heic", ViewMode::Report)
    );
}

#[test]
fn malformed_mp4_report_snapshot() {
    assert_json_snapshot!(
        "malformed_mp4_report",
        extract_json("malformed.mp4", ViewMode::Report)
    );
}

#[test]
fn malformed_mov_report_snapshot() {
    assert_json_snapshot!(
        "malformed_mov_report",
        extract_json("malformed.mov", ViewMode::Report)
    );
}

#[test]
fn unsupported_mp4_report_snapshot() {
    assert_json_snapshot!(
        "unsupported_mp4_report",
        extract_json("unsupported.mp4", ViewMode::Report)
    );
}

#[test]
fn no_exif_file_surfaces_empty_metadata_issue() {
    let output = extract_json("no_exif.jpg", ViewMode::Report);
    let issues = output["report"]["issues"].as_array().unwrap();
    assert!(
        issues
            .iter()
            .any(|issue| issue["code"] == "no_metadata_entries")
    );
}

#[test]
fn no_exif_heic_surfaces_empty_metadata_issue() {
    let output = extract_json("no_exif.heic", ViewMode::Report);
    let issues = output["report"]["issues"].as_array().unwrap();
    assert!(
        issues
            .iter()
            .any(|issue| issue["code"] == "no_metadata_entries")
    );
}

#[test]
fn no_metadata_mp4_surfaces_empty_metadata_issue() {
    let output = extract_json("no_metadata.mp4", ViewMode::Report);
    let issues = output["report"]["issues"].as_array().unwrap();
    assert!(
        issues
            .iter()
            .any(|issue| issue["code"] == "no_metadata_entries")
    );
}

#[test]
fn capability_artifact_declares_iteration_five_support() {
    let capabilities = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../CAPABILITIES.json");
    let content = std::fs::read_to_string(capabilities).expect("missing CAPABILITIES.json");
    let parsed: Value = serde_json::from_str(&content).expect("invalid CAPABILITIES.json");

    assert_eq!(parsed["schema_version"], Value::String("0.1.0".into()));
    assert_eq!(
        parsed["namespaces"]["icc"]["status"],
        Value::String("bounded".into())
    );
    assert_eq!(
        parsed["namespaces"]["iptc"]["status"],
        Value::String("bounded".into())
    );
    assert_eq!(
        parsed["containers"]["jpeg"]["namespaces"]["icc"],
        Value::String("supported".into())
    );
    assert_eq!(
        parsed["containers"]["png"]["namespaces"]["icc"],
        Value::String("supported".into())
    );
    assert_eq!(
        parsed["containers"]["webp"]["namespaces"]["icc"],
        Value::String("supported".into())
    );
    assert_eq!(
        parsed["containers"]["jpeg"]["namespaces"]["iptc"],
        Value::String("supported".into())
    );
}

#[test]
fn checked_in_schema_artifacts_match_current_schema_version() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../");
    let analysis = root.join("schemas/xifty-analysis-0.1.0.schema.json");
    let probe = root.join("schemas/xifty-probe-0.1.0.schema.json");

    for path in [analysis, probe] {
        let content = std::fs::read_to_string(&path).expect("missing schema artifact");
        let parsed: Value = serde_json::from_str(&content).expect("invalid schema artifact");
        assert_eq!(
            parsed["$schema"],
            Value::String("https://json-schema.org/draft/2020-12/schema".into())
        );
        assert_eq!(
            parsed["properties"]["schema_version"]["const"],
            Value::String(xifty_core::SCHEMA_VERSION.into())
        );
    }
}

#[test]
fn exiftool_differential_happy_jpeg_supported_fields() {
    differential_assert("happy.jpg", false);
}

#[test]
fn exiftool_differential_gps_jpeg_supported_fields() {
    differential_assert("gps.jpg", true);
}

#[test]
fn exiftool_differential_happy_tiff_supported_fields() {
    differential_assert("happy.tiff", false);
}

#[test]
fn exiftool_differential_xmp_only_png_supported_fields() {
    differential_assert_xmp("xmp_only.png", false);
}

#[test]
fn exiftool_differential_xmp_only_webp_supported_fields() {
    differential_assert_xmp("xmp_only.webp", true);
}

#[test]
fn exiftool_differential_real_exif_heic_supported_fields() {
    differential_assert_heif("real_exif.heic");
}

#[test]
fn exiftool_differential_happy_mp4_supported_fields() {
    differential_assert_media("happy.mp4");
}

#[test]
fn exiftool_differential_happy_mov_supported_fields() {
    differential_assert_media("happy.mov");
}

#[test]
fn exiftool_differential_icc_jpeg_supported_fields() {
    differential_assert_icc("icc.jpg");
}

#[test]
fn exiftool_differential_icc_png_supported_fields() {
    differential_assert_icc("icc.png");
}

#[test]
fn exiftool_differential_icc_webp_supported_fields() {
    differential_assert_icc("icc.webp");
}

#[test]
fn exiftool_differential_iptc_jpeg_supported_fields() {
    differential_assert_iptc("iptc.jpg");
}

#[test]
fn exiftool_differential_overlap_editorial_jpeg_supported_fields() {
    assert_exiftool_sees_overlap_editorial_sources("overlap_editorial.jpg");
}

#[test]
fn exiftool_differential_real_camera_mp4_supported_fields() {
    if optional_fixture("C0242.MP4").is_none() {
        skip_missing_local_fixture("C0242.MP4");
        return;
    }
    differential_assert_camera_mp4("C0242.MP4");
}

#[test]
fn mp4_normalization_includes_media_fields() {
    let output = normalized_map(&extract_json("happy.mp4", ViewMode::Normalized));
    assert_eq!(output["duration"]["value"], Value::from(12.0));
    let fps = output["video.framerate"]["value"].as_f64().unwrap();
    assert!((fps - 23.976).abs() < 0.01, "unexpected fps {fps}");
    assert_eq!(output["video.bitrate"]["value"], Value::from(24_000_000));
    assert_eq!(output["codec.video"]["value"], Value::String("avc1".into()));
    assert_eq!(output["codec.audio"]["value"], Value::String("mp4a".into()));
    assert_eq!(output["audio.channels"]["value"], Value::from(2));
    assert_eq!(output["audio.sample_rate"]["value"], Value::from(48_000));
    assert_eq!(output["author"]["value"], Value::String("Kai".into()));
    assert_eq!(
        output["software"]["value"],
        Value::String("XIFtyMediaGen".into())
    );
}

#[test]
fn happy_jpeg_normalization_includes_photographic_fields() {
    let output = normalized_map(&extract_json("happy.jpg", ViewMode::Normalized));
    assert_eq!(output["exposure.iso"]["value"], Value::from(200));
    assert_eq!(
        output["exposure.aperture"],
        serde_json::json!({"kind": "rational", "value": {"numerator": 56, "denominator": 10}})
    );
    assert_eq!(
        output["exposure.shutter_speed"],
        serde_json::json!({"kind": "rational", "value": {"numerator": 1, "denominator": 250}})
    );
    let focal = output["exposure.focal_length_mm"]["value"]
        .as_f64()
        .unwrap();
    assert!((focal - 50.0).abs() < f64::EPSILON);
    assert_eq!(
        output["lens.model"]["value"],
        Value::String("XIFty 50mm F2".into())
    );
    assert_eq!(
        output["lens.make"]["value"],
        Value::String("XIFty Optics".into())
    );
}

#[test]
fn video_only_mp4_omits_audio_codec() {
    let output = normalized_map(&extract_json("video_only.mp4", ViewMode::Normalized));
    assert!(!output.contains_key("codec.audio"));
    assert_eq!(output["codec.video"]["value"], Value::String("avc1".into()));
}

#[test]
fn sony_jpeg_normalization_includes_richer_standard_exif_fields() {
    let Some(output) = extract_optional_json("DSC04504.JPG", ViewMode::Normalized) else {
        skip_missing_local_fixture("DSC04504.JPG");
        return;
    };
    let output = normalized_map(&output);

    assert_eq!(
        output["captured_at"]["value"],
        Value::String("2025-08-07T10:44:16.046-08:00".into())
    );
    assert_eq!(
        output["created_at"]["value"],
        Value::String("2025-08-07T10:44:16.046-08:00".into())
    );
    assert_eq!(
        output["modified_at"]["value"],
        Value::String("2025-08-07T10:44:16.046-08:00".into())
    );
    assert_eq!(output["device.make"]["value"], Value::String("SONY".into()));
    assert_eq!(
        output["device.model"]["value"],
        Value::String("ZV-E10".into())
    );
    assert_eq!(output["dimensions.width"]["value"], Value::from(6000));
    assert_eq!(output["dimensions.height"]["value"], Value::from(4000));
}

#[test]
fn sony_jpeg_interpreted_view_names_common_exif_camera_tags() {
    let Some(output) = extract_optional_json("DSC04504.JPG", ViewMode::Interpreted) else {
        skip_missing_local_fixture("DSC04504.JPG");
        return;
    };
    let metadata = output["interpreted"]["metadata"].as_array().unwrap();

    let tag_names: std::collections::BTreeSet<_> = metadata
        .iter()
        .filter_map(|entry| entry["tag_name"].as_str())
        .collect();

    for expected in [
        "ExposureTime",
        "FNumber",
        "ISO",
        "OffsetTimeOriginal",
        "FocalLength",
        "ExifImageWidth",
        "ExifImageHeight",
        "LensInfo",
        "LensModel",
        "MakerNote",
    ] {
        assert!(
            tag_names.contains(expected),
            "missing interpreted EXIF tag {expected}"
        );
    }
}

#[test]
fn sony_jpeg_interpreted_view_includes_sony_makernote_fields() {
    let Some(output) = extract_optional_json("DSC04504.JPG", ViewMode::Interpreted) else {
        skip_missing_local_fixture("DSC04504.JPG");
        return;
    };

    assert_eq!(
        interpreted_value(&output, "sony", "SonyDateTime"),
        Some(Value::String("2025:08:07 10:44:16".into()))
    );
    assert_eq!(
        interpreted_value(&output, "sony", "AmbientTemperature"),
        Some(Value::String("30 C".into()))
    );
    assert_eq!(
        interpreted_value(&output, "sony", "BatteryTemperature"),
        Some(Value::String("36.7 C".into()))
    );
    assert_eq!(
        interpreted_value(&output, "sony", "BatteryLevel"),
        Some(Value::String("63%".into()))
    );
    assert_eq!(
        interpreted_value(&output, "sony", "LensType3"),
        Some(Value::String("Sony E PZ 16-50mm F3.5-5.6 OSS".into()))
    );
    assert_eq!(
        interpreted_value(&output, "sony", "LensSpec"),
        Some(Value::String("E PZ 16-50mm F3.5-5.6 OSS".into()))
    );
    assert_eq!(
        interpreted_value(&output, "sony", "LensSpecFeatures"),
        Some(Value::String("E PZ OSS".into()))
    );
    assert_eq!(
        interpreted_value(&output, "sony", "LensFirmwareVersion"),
        Some(Value::String("Ver.02.000".into()))
    );
    assert_eq!(
        interpreted_value(&output, "sony", "ShutterCount"),
        Some(Value::from(24411))
    );
    assert_eq!(
        interpreted_value(&output, "sony", "InternalSerialNumber"),
        Some(Value::String("eaff0000690c".into()))
    );
    assert_eq!(
        interpreted_value(&output, "sony", "WB_RGBLevels"),
        Some(Value::String("700 256 458".into()))
    );
    assert_eq!(
        interpreted_value(&output, "sony", "AspectRatio"),
        Some(Value::String("3:2".into()))
    );
}

#[test]
fn apple_jpeg_normalization_includes_standard_fields() {
    let Some(output) = extract_optional_json(
        "IMG_5B74BABE-DF0A-48EB-A6A4-6AAA54D5198E.JPEG",
        ViewMode::Normalized,
    ) else {
        skip_missing_local_fixture("IMG_5B74BABE-DF0A-48EB-A6A4-6AAA54D5198E.JPEG");
        return;
    };
    let output = normalized_map(&output);

    assert_eq!(
        output["captured_at"]["value"],
        Value::String("2026-04-15T09:11:16.037-04:00".into())
    );
    assert_eq!(
        output["created_at"]["value"],
        Value::String("2026-04-15T09:11:16.037-04:00".into())
    );
    assert_eq!(
        output["modified_at"]["value"],
        Value::String("2026-04-15T09:11:16-04:00".into())
    );
    assert_eq!(
        output["device.make"]["value"],
        Value::String("Apple".into())
    );
    assert_eq!(
        output["device.model"]["value"],
        Value::String("iPhone 15 Pro".into())
    );
    assert_eq!(output["software"]["value"], Value::String("26.3.1".into()));
    assert_eq!(output["dimensions.width"]["value"], Value::from(4032));
    assert_eq!(output["dimensions.height"]["value"], Value::from(3024));
    assert_eq!(output["orientation"]["value"], Value::from(6));
}

#[test]
fn apple_jpeg_interpreted_view_names_common_exif_camera_tags() {
    let Some(output) = extract_optional_json(
        "IMG_5B74BABE-DF0A-48EB-A6A4-6AAA54D5198E.JPEG",
        ViewMode::Interpreted,
    ) else {
        skip_missing_local_fixture("IMG_5B74BABE-DF0A-48EB-A6A4-6AAA54D5198E.JPEG");
        return;
    };
    let metadata = output["interpreted"]["metadata"].as_array().unwrap();

    let tag_names: std::collections::BTreeSet<_> = metadata
        .iter()
        .filter_map(|entry| entry["tag_name"].as_str())
        .collect();

    for expected in [
        "HostComputer",
        "ShutterSpeedValue",
        "ApertureValue",
        "SubjectArea",
        "SensingMethod",
        "LensMake",
        "LensModel",
        "MakerNote",
    ] {
        assert!(
            tag_names.contains(expected),
            "missing interpreted EXIF tag {expected}"
        );
    }
}

#[test]
fn apple_jpeg_interpreted_view_includes_apple_makernote_fields() {
    let Some(output) = extract_optional_json(
        "IMG_5B74BABE-DF0A-48EB-A6A4-6AAA54D5198E.JPEG",
        ViewMode::Interpreted,
    ) else {
        skip_missing_local_fixture("IMG_5B74BABE-DF0A-48EB-A6A4-6AAA54D5198E.JPEG");
        return;
    };

    assert_eq!(
        interpreted_value(&output, "apple", "MakerNoteVersion"),
        Some(Value::from(16))
    );
    assert_eq!(
        interpreted_value(&output, "apple", "RunTimeFlags"),
        Some(Value::String("Valid".into()))
    );
    assert_eq!(
        interpreted_value(&output, "apple", "RunTimeValue"),
        Some(Value::from(755781356991791_i64))
    );
    assert_eq!(
        interpreted_value(&output, "apple", "RunTimeEpoch"),
        Some(Value::from(0))
    );
    assert_eq!(
        interpreted_value(&output, "apple", "RunTimeScale"),
        Some(Value::from(1_000_000_000_i64))
    );
    assert_eq!(
        interpreted_value(&output, "apple", "AEStable"),
        Some(Value::String("Yes".into()))
    );
    assert_eq!(
        interpreted_value(&output, "apple", "AETarget"),
        Some(Value::from(188))
    );
    assert_eq!(
        interpreted_value(&output, "apple", "AEAverage"),
        Some(Value::from(184))
    );
    assert_eq!(
        interpreted_value(&output, "apple", "AFStable"),
        Some(Value::String("Yes".into()))
    );
    assert_eq!(
        interpreted_value(&output, "apple", "ImageCaptureType"),
        Some(Value::String("Photo".into()))
    );
    assert_eq!(
        interpreted_value(&output, "apple", "LivePhotoVideoIndex"),
        Some(Value::from(5_283_876_i64))
    );
    assert_eq!(
        interpreted_value(&output, "apple", "PhotosAppFeatureFlags"),
        Some(Value::from(0))
    );
    assert_eq!(
        interpreted_value(&output, "apple", "PhotoIdentifier"),
        Some(Value::String("E894E84C-6852-44DC-8852-9EDC76AF1AB4".into()))
    );
    assert_eq!(
        interpreted_value(&output, "apple", "ColorTemperature"),
        Some(Value::from(5401))
    );
    assert_eq!(
        interpreted_value(&output, "apple", "CameraType"),
        Some(Value::String("Back Normal".into()))
    );
    assert_eq!(
        interpreted_value(&output, "apple", "FocusPosition"),
        Some(Value::from(72))
    );
    assert_eq!(
        interpreted_value(&output, "apple", "AFMeasuredDepth"),
        Some(Value::from(9))
    );
    assert_eq!(
        interpreted_value(&output, "apple", "AFConfidence"),
        Some(Value::from(96))
    );
}

fn differential_assert(name: &str, expect_gps: bool) {
    if !ensure_exiftool_available() {
        return;
    }
    let ours = extract_json(name, ViewMode::Normalized);
    let ours = normalized_map(&ours);

    let output = Command::new("exiftool")
        .args([
            "-json",
            "-n",
            "-DateTimeOriginal",
            "-Make",
            "-Model",
            "-Software",
            "-ImageWidth",
            "-ImageHeight",
            "-Orientation",
            "-ISO",
            "-FNumber",
            "-ExposureTime",
            "-FocalLength",
            "-LensModel",
            "-LensMake",
            "-GPSLatitude",
            "-GPSLongitude",
        ])
        .arg(fixture(name))
        .output()
        .expect("failed to run exiftool");
    assert!(
        output.status.success(),
        "exiftool failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).unwrap();
    let first = &parsed.as_array().unwrap()[0];

    assert_eq!(
        ours["captured_at"]["value"],
        Value::String("2024-04-16T12:34:56".into())
    );
    assert_eq!(ours["device.make"]["value"], first["Make"]);
    assert_eq!(ours["device.model"]["value"], first["Model"]);
    assert_eq!(ours["software"]["value"], first["Software"]);
    assert_eq!(ours["dimensions.width"]["value"], first["ImageWidth"]);
    assert_eq!(ours["dimensions.height"]["value"], first["ImageHeight"]);
    assert_eq!(ours["orientation"]["value"], first["Orientation"]);
    assert_eq!(ours["exposure.iso"]["value"], first["ISO"]);
    let ours_aperture = rational_json_to_f64(&ours["exposure.aperture"]).unwrap();
    let exif_aperture = first["FNumber"].as_f64().unwrap();
    assert!((ours_aperture - exif_aperture).abs() < 0.0001);
    let ours_shutter = rational_json_to_f64(&ours["exposure.shutter_speed"]).unwrap();
    let exif_shutter = first["ExposureTime"].as_f64().unwrap();
    assert!((ours_shutter - exif_shutter).abs() < 0.0001);
    let ours_focal = ours["exposure.focal_length_mm"]["value"].as_f64().unwrap();
    let exif_focal = first["FocalLength"].as_f64().unwrap();
    assert!((ours_focal - exif_focal).abs() < 0.0001);
    assert_eq!(ours["lens.model"]["value"], first["LensModel"]);
    assert_eq!(ours["lens.make"]["value"], first["LensMake"]);

    if expect_gps {
        let lat = ours["location"]["value"]["latitude"].as_f64().unwrap();
        let lon = ours["location"]["value"]["longitude"].as_f64().unwrap();
        let exif_lat = first["GPSLatitude"].as_f64().unwrap();
        let exif_lon = first["GPSLongitude"].as_f64().unwrap();
        assert!(
            (lat - exif_lat).abs() < 0.0001,
            "lat mismatch {lat} vs {exif_lat}"
        );
        assert!(
            (lon - exif_lon).abs() < 0.0001,
            "lon mismatch {lon} vs {exif_lon}"
        );
    } else {
        assert!(!ours.contains_key("location"));
    }
}

fn differential_assert_xmp(name: &str, compare_dimensions: bool) {
    if !ensure_exiftool_available() {
        return;
    }
    let ours = extract_json(name, ViewMode::Normalized);
    let ours = normalized_map(&ours);

    let output = Command::new("exiftool")
        .args([
            "-json",
            "-n",
            "-Make",
            "-Model",
            "-Orientation",
            "-Creator",
            "-Rights",
            "-ImageWidth",
            "-ImageHeight",
        ])
        .arg(fixture(name))
        .output()
        .expect("failed to run exiftool");
    assert!(
        output.status.success(),
        "exiftool failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).unwrap();
    let first = &parsed.as_array().unwrap()[0];

    assert_eq!(ours["device.make"]["value"], first["Make"]);
    assert_eq!(ours["device.model"]["value"], first["Model"]);
    assert_eq!(ours["orientation"]["value"], first["Orientation"]);
    assert_eq!(ours["author"]["value"], first["Creator"]);
    assert_eq!(ours["copyright"]["value"], first["Rights"]);

    if compare_dimensions {
        assert_eq!(ours["dimensions.width"]["value"], first["ImageWidth"]);
        assert_eq!(ours["dimensions.height"]["value"], first["ImageHeight"]);
    }
}

fn differential_assert_heif(name: &str) {
    if !ensure_exiftool_available() {
        return;
    }
    let ours = extract_json(name, ViewMode::Normalized);
    let ours = normalized_map(&ours);

    let output = Command::new("exiftool")
        .args(["-json", "-n", "-ImageWidth", "-ImageHeight", "-Orientation"])
        .arg(fixture(name))
        .output()
        .expect("failed to run exiftool");
    assert!(
        output.status.success(),
        "exiftool failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).unwrap();
    let first = &parsed.as_array().unwrap()[0];

    assert_eq!(ours["dimensions.width"]["value"], first["ImageWidth"]);
    assert_eq!(ours["dimensions.height"]["value"], first["ImageHeight"]);
    assert_eq!(ours["orientation"]["value"], first["Orientation"]);
}

fn differential_assert_media(name: &str) {
    if !ensure_exiftool_available() {
        return;
    }
    let ours = extract_json(name, ViewMode::Normalized);
    let ours = normalized_map(&ours);

    let output = Command::new("exiftool")
        .args([
            "-json",
            "-n",
            "-CreateDate",
            "-ModifyDate",
            "-Artist",
            "-Encoder",
            "-Duration",
            "-ImageWidth",
            "-ImageHeight",
            "-CompressorID",
            "-AudioFormat",
            "-VideoFrameRate",
            "-AvgBitrate",
            "-AudioChannels",
            "-AudioSampleRate",
        ])
        .arg(fixture(name))
        .output()
        .expect("failed to run exiftool");
    assert!(
        output.status.success(),
        "exiftool failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).unwrap();
    let first = &parsed.as_array().unwrap()[0];

    assert_eq!(
        ours["created_at"]["value"],
        Value::String("2024-04-16T12:34:56".into())
    );
    assert_eq!(
        ours["modified_at"]["value"],
        Value::String("2024-04-16T13:00:00".into())
    );
    assert_eq!(ours["author"]["value"], first["Artist"]);
    assert_eq!(ours["software"]["value"], first["Encoder"]);
    let ours_duration = ours["duration"]["value"].as_f64().unwrap();
    let exiftool_duration = first["Duration"].as_f64().unwrap();
    assert!(
        (ours_duration - exiftool_duration).abs() < 0.0001,
        "duration mismatch {ours_duration} vs {exiftool_duration}"
    );
    assert_eq!(ours["dimensions.width"]["value"], first["ImageWidth"]);
    assert_eq!(ours["dimensions.height"]["value"], first["ImageHeight"]);
    assert_eq!(ours["codec.video"]["value"], first["CompressorID"]);
    assert_eq!(ours["codec.audio"]["value"], first["AudioFormat"]);
    let ours_fps = ours["video.framerate"]["value"].as_f64().unwrap();
    let exif_fps = first["VideoFrameRate"].as_f64().unwrap();
    assert!((ours_fps - exif_fps).abs() < 0.01);
    if !first["AvgBitrate"].is_null() {
        assert_eq!(ours["video.bitrate"]["value"], first["AvgBitrate"]);
    }
    if !first["AudioChannels"].is_null() {
        assert_eq!(ours["audio.channels"]["value"], first["AudioChannels"]);
    }
    if !first["AudioSampleRate"].is_null() {
        assert_eq!(ours["audio.sample_rate"]["value"], first["AudioSampleRate"]);
    }
}

fn differential_assert_icc(name: &str) {
    if !ensure_exiftool_available() {
        return;
    }
    let ours_interpreted = extract_json(name, ViewMode::Interpreted);
    let ours_normalized = extract_json(name, ViewMode::Normalized);
    let ours_normalized = normalized_map(&ours_normalized);

    let output = Command::new("exiftool")
        .args([
            "-json",
            "-n",
            "-ProfileClass",
            "-ColorSpaceData",
            "-ProfileDescription",
            "-DeviceManufacturer",
            "-DeviceModel",
        ])
        .arg(fixture(name))
        .output()
        .expect("failed to run exiftool");
    assert!(
        output.status.success(),
        "exiftool failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).unwrap();
    let first = &parsed.as_array().unwrap()[0];

    assert_eq!(
        ours_normalized["color.profile.class"]["value"],
        Value::String("display".into())
    );
    assert_eq!(
        ours_normalized["color.space"]["value"],
        Value::String("RGB".into())
    );
    assert_eq!(
        ours_normalized["color.profile.name"]["value"],
        first["ProfileDescription"]
    );

    assert_eq!(
        interpreted_value(&ours_interpreted, "icc", "ProfileDescription"),
        Some(first["ProfileDescription"].clone())
    );
    assert_eq!(
        interpreted_value(&ours_interpreted, "icc", "DeviceModel"),
        Some(first["DeviceModel"].clone())
    );

    let manufacturer = json_stringified(&first["DeviceManufacturer"]).unwrap_or_default();
    assert!(
        manufacturer.contains("XFTY"),
        "expected DeviceManufacturer to mention XFTY, got {manufacturer}"
    );
}

fn differential_assert_iptc(name: &str) {
    if !ensure_exiftool_available() {
        return;
    }
    let ours = extract_json(name, ViewMode::Normalized);
    let ours = normalized_map(&ours);

    let output = Command::new("exiftool")
        .args([
            "-json",
            "-n",
            "-Headline",
            "-Caption-Abstract",
            "-By-line",
            "-CopyrightNotice",
            "-Keywords",
        ])
        .arg(fixture(name))
        .output()
        .expect("failed to run exiftool");
    assert!(
        output.status.success(),
        "exiftool failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).unwrap();
    let first = &parsed.as_array().unwrap()[0];

    assert_eq!(ours["headline"]["value"], first["Headline"]);
    assert_eq!(ours["description"]["value"], first["Caption-Abstract"]);
    assert_eq!(ours["author"]["value"], first["By-line"]);
    assert_eq!(ours["copyright"]["value"], first["CopyrightNotice"]);

    let keywords = first["Keywords"]
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(json_stringified)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    assert_eq!(ours["keywords"]["value"], Value::String(keywords));
}

fn assert_exiftool_sees_overlap_editorial_sources(name: &str) {
    if !ensure_exiftool_available() {
        return;
    }
    let output = Command::new("exiftool")
        .args([
            "-json",
            "-G1",
            "-n",
            "-XMP-dc:Creator",
            "-XMP-dc:Rights",
            "-XMP-photoshop:Headline",
            "-IPTC:Headline",
            "-XMP-dc:Description",
        ])
        .arg(fixture(name))
        .output()
        .expect("failed to run exiftool");
    assert!(
        output.status.success(),
        "exiftool failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).unwrap();
    let first = &parsed.as_array().unwrap()[0];

    assert_eq!(first["XMP-dc:Creator"], Value::String("XMP Kai".into()));
    assert_eq!(first["XMP-dc:Rights"], Value::String("XMP Rights".into()));
    assert_eq!(
        first["XMP-photoshop:Headline"],
        Value::String("XIFty XMP Headline".into())
    );
    assert_eq!(
        first["IPTC:Headline"],
        Value::String("XIFty IPTC Headline".into())
    );
    assert_eq!(
        first["XMP-dc:Description"],
        Value::String("XIFty XMP Description".into())
    );
}

fn differential_assert_camera_mp4(name: &str) {
    if !ensure_exiftool_available() {
        return;
    }
    let ours = extract_optional_json(name, ViewMode::Normalized)
        .unwrap_or_else(|| panic!("missing optional local fixture {name}"));
    let ours = normalized_map(&ours);

    let output = Command::new("exiftool")
        .args([
            "-json",
            "-n",
            "-CreateDate",
            "-ModifyDate",
            "-ImageWidth",
            "-ImageHeight",
            "-CompressorID",
            "-AudioFormat",
            "-DeviceManufacturer",
            "-DeviceModelName",
        ])
        .arg(
            optional_fixture(name)
                .unwrap_or_else(|| panic!("missing optional local fixture {name}")),
        )
        .output()
        .expect("failed to run exiftool");
    assert!(
        output.status.success(),
        "exiftool failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).unwrap();
    let first = &parsed.as_array().unwrap()[0];

    assert_eq!(ours["device.make"]["value"], first["DeviceManufacturer"]);
    assert_eq!(ours["device.model"]["value"], first["DeviceModelName"]);
    assert_eq!(ours["dimensions.width"]["value"], first["ImageWidth"]);
    assert_eq!(ours["dimensions.height"]["value"], first["ImageHeight"]);
    assert_eq!(ours["codec.video"]["value"], first["CompressorID"]);
    assert_eq!(ours["codec.audio"]["value"], first["AudioFormat"]);
}

#[test]
fn exiftool_differential_real_camera_jpeg_supported_fields() {
    if !ensure_exiftool_available() {
        return;
    }
    let Some(ours) = extract_optional_json("DSC04504.JPG", ViewMode::Normalized) else {
        skip_missing_local_fixture("DSC04504.JPG");
        return;
    };
    let ours = normalized_map(&ours);

    let output = Command::new("exiftool")
        .args([
            "-json",
            "-n",
            "-DateTimeOriginal",
            "-CreateDate",
            "-ModifyDate",
            "-Make",
            "-Model",
            "-Software",
            "-ImageWidth",
            "-ImageHeight",
            "-Orientation",
            "-LensModel",
        ])
        .arg(optional_fixture("DSC04504.JPG").unwrap())
        .output()
        .expect("failed to run exiftool");
    assert!(
        output.status.success(),
        "exiftool failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).unwrap();
    let first = &parsed.as_array().unwrap()[0];

    assert_eq!(
        ours["captured_at"]["value"],
        Value::String("2025-08-07T10:44:16.046-08:00".into())
    );
    assert_eq!(
        ours["created_at"]["value"],
        Value::String("2025-08-07T10:44:16.046-08:00".into())
    );
    assert_eq!(
        ours["modified_at"]["value"],
        Value::String("2025-08-07T10:44:16.046-08:00".into())
    );
    assert_eq!(ours["device.make"]["value"], first["Make"]);
    assert_eq!(ours["device.model"]["value"], first["Model"]);
    assert_eq!(ours["software"]["value"], first["Software"]);
    assert_eq!(ours["dimensions.width"]["value"], first["ImageWidth"]);
    assert_eq!(ours["dimensions.height"]["value"], first["ImageHeight"]);
    assert_eq!(ours["orientation"]["value"], first["Orientation"]);
}

#[test]
fn exiftool_differential_real_camera_jpeg_sony_makernote_fields() {
    if !ensure_exiftool_available() {
        return;
    }
    let Some(ours) = extract_optional_json("DSC04504.JPG", ViewMode::Interpreted) else {
        skip_missing_local_fixture("DSC04504.JPG");
        return;
    };

    let output = Command::new("exiftool")
        .args([
            "-json",
            "-CreativeStyle",
            "-AmbientTemperature",
            "-BatteryTemperature",
            "-BatteryLevel",
            "-FocusMode",
            "-AFAreaMode",
            "-LensFirmwareVersion",
            "-CameraE-mountVersion",
            "-LensE-mountVersion",
            "-ShutterCount",
            "-InternalSerialNumber",
            "-LensSpec",
            "-LensSpecFeatures",
            "-WB_RGBLevels",
            "-AspectRatio",
            "-FlashMode",
            "-Quality2",
        ])
        .arg(optional_fixture("DSC04504.JPG").unwrap())
        .output()
        .expect("failed to run exiftool");
    assert!(
        output.status.success(),
        "exiftool failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).unwrap();
    let first = &parsed.as_array().unwrap()[0];

    assert_eq!(
        interpreted_value(&ours, "sony", "CreativeStyle"),
        Some(first["CreativeStyle"].clone())
    );
    assert_eq!(
        interpreted_value(&ours, "sony", "AmbientTemperature"),
        Some(first["AmbientTemperature"].clone())
    );
    assert_eq!(
        interpreted_value(&ours, "sony", "BatteryTemperature"),
        Some(first["BatteryTemperature"].clone())
    );
    assert_eq!(
        interpreted_value(&ours, "sony", "BatteryLevel"),
        Some(first["BatteryLevel"].clone())
    );
    assert_eq!(
        interpreted_value(&ours, "sony", "FocusMode"),
        Some(first["FocusMode"].clone())
    );
    assert_eq!(
        interpreted_value(&ours, "sony", "AFAreaMode"),
        Some(first["AFAreaMode"].clone())
    );
    assert_eq!(
        interpreted_value(&ours, "sony", "LensFirmwareVersion"),
        Some(first["LensFirmwareVersion"].clone())
    );
    assert_eq!(
        interpreted_value(&ours, "sony", "CameraE-mountVersion")
            .and_then(|value| normalized_decimal_string(&value)),
        normalized_decimal_string(&first["CameraE-mountVersion"])
    );
    assert_eq!(
        interpreted_value(&ours, "sony", "LensE-mountVersion")
            .and_then(|value| normalized_decimal_string(&value)),
        normalized_decimal_string(&first["LensE-mountVersion"])
    );
    assert_eq!(
        interpreted_value(&ours, "sony", "ShutterCount"),
        Some(first["ShutterCount"].clone())
    );
    assert_eq!(
        interpreted_value(&ours, "sony", "InternalSerialNumber"),
        Some(first["InternalSerialNumber"].clone())
    );
    assert_eq!(
        interpreted_value(&ours, "sony", "LensSpec"),
        Some(first["LensSpec"].clone())
    );
    assert_eq!(
        interpreted_value(&ours, "sony", "LensSpecFeatures"),
        Some(first["LensSpecFeatures"].clone())
    );
    assert_eq!(
        interpreted_value(&ours, "sony", "WB_RGBLevels"),
        Some(first["WB_RGBLevels"].clone())
    );
    assert_eq!(
        interpreted_value(&ours, "sony", "AspectRatio"),
        Some(first["AspectRatio"].clone())
    );
    assert_eq!(
        interpreted_value(&ours, "sony", "FlashMode"),
        Some(first["FlashMode"].clone())
    );
    assert_eq!(
        interpreted_value(&ours, "sony", "Quality2"),
        Some(first["Quality2"].clone())
    );
}

#[test]
fn exiftool_differential_apple_jpeg_supported_fields() {
    if !ensure_exiftool_available() {
        return;
    }
    let Some(ours) = extract_optional_json(
        "IMG_5B74BABE-DF0A-48EB-A6A4-6AAA54D5198E.JPEG",
        ViewMode::Normalized,
    ) else {
        skip_missing_local_fixture("IMG_5B74BABE-DF0A-48EB-A6A4-6AAA54D5198E.JPEG");
        return;
    };
    let ours = normalized_map(&ours);

    let output = Command::new("exiftool")
        .args([
            "-json",
            "-n",
            "-DateTimeOriginal",
            "-CreateDate",
            "-ModifyDate",
            "-Make",
            "-Model",
            "-Software",
            "-ImageWidth",
            "-ImageHeight",
            "-Orientation",
        ])
        .arg(optional_fixture("IMG_5B74BABE-DF0A-48EB-A6A4-6AAA54D5198E.JPEG").unwrap())
        .output()
        .expect("failed to run exiftool");
    assert!(
        output.status.success(),
        "exiftool failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).unwrap();
    let first = &parsed.as_array().unwrap()[0];

    assert_eq!(
        ours["captured_at"]["value"],
        Value::String("2026-04-15T09:11:16.037-04:00".into())
    );
    assert_eq!(
        ours["created_at"]["value"],
        Value::String("2026-04-15T09:11:16.037-04:00".into())
    );
    assert_eq!(
        ours["modified_at"]["value"],
        Value::String("2026-04-15T09:11:16-04:00".into())
    );
    assert_eq!(ours["device.make"]["value"], first["Make"]);
    assert_eq!(ours["device.model"]["value"], first["Model"]);
    assert_eq!(ours["software"]["value"], first["Software"]);
    assert_eq!(ours["dimensions.width"]["value"], first["ImageWidth"]);
    assert_eq!(ours["dimensions.height"]["value"], first["ImageHeight"]);
    assert_eq!(ours["orientation"]["value"], first["Orientation"]);
}

#[test]
fn exiftool_differential_apple_jpeg_makernote_fields() {
    if !ensure_exiftool_available() {
        return;
    }
    let Some(ours) = extract_optional_json(
        "IMG_5B74BABE-DF0A-48EB-A6A4-6AAA54D5198E.JPEG",
        ViewMode::Interpreted,
    ) else {
        skip_missing_local_fixture("IMG_5B74BABE-DF0A-48EB-A6A4-6AAA54D5198E.JPEG");
        return;
    };

    let output = Command::new("exiftool")
        .args([
            "-json",
            "-n",
            "-MakerNoteVersion",
            "-RunTimeFlags",
            "-RunTimeValue",
            "-RunTimeEpoch",
            "-RunTimeScale",
            "-AEStable",
            "-AETarget",
            "-AEAverage",
            "-AFStable",
            "-ImageCaptureType",
            "-LivePhotoVideoIndex",
            "-PhotosAppFeatureFlags",
            "-HDRHeadroom",
            "-SignalToNoiseRatio",
            "-PhotoIdentifier",
            "-ColorTemperature",
            "-CameraType",
            "-FocusPosition",
            "-HDRGain",
            "-AFMeasuredDepth",
            "-AFConfidence",
        ])
        .arg(optional_fixture("IMG_5B74BABE-DF0A-48EB-A6A4-6AAA54D5198E.JPEG").unwrap())
        .output()
        .expect("failed to run exiftool");
    assert!(
        output.status.success(),
        "exiftool failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).unwrap();
    let first = &parsed.as_array().unwrap()[0];

    assert_eq!(
        interpreted_value(&ours, "apple", "MakerNoteVersion"),
        Some(first["MakerNoteVersion"].clone())
    );
    assert_eq!(
        interpreted_value(&ours, "apple", "RunTimeFlags")
            .and_then(|value| value.as_str().map(str::to_owned)),
        Some("Valid".into())
    );
    assert_eq!(
        interpreted_value(&ours, "apple", "RunTimeValue"),
        Some(first["RunTimeValue"].clone())
    );
    assert_eq!(
        interpreted_value(&ours, "apple", "RunTimeEpoch"),
        Some(first["RunTimeEpoch"].clone())
    );
    assert_eq!(
        interpreted_value(&ours, "apple", "RunTimeScale"),
        Some(first["RunTimeScale"].clone())
    );
    assert_eq!(
        interpreted_value(&ours, "apple", "AEStable"),
        Some(Value::String("Yes".into()))
    );
    assert_eq!(
        interpreted_value(&ours, "apple", "AETarget"),
        Some(first["AETarget"].clone())
    );
    assert_eq!(
        interpreted_value(&ours, "apple", "AEAverage"),
        Some(first["AEAverage"].clone())
    );
    assert_eq!(
        interpreted_value(&ours, "apple", "AFStable"),
        Some(Value::String("Yes".into()))
    );
    assert_eq!(
        interpreted_value(&ours, "apple", "ImageCaptureType"),
        Some(Value::String("Photo".into()))
    );
    assert_eq!(
        interpreted_value(&ours, "apple", "LivePhotoVideoIndex"),
        Some(first["LivePhotoVideoIndex"].clone())
    );
    assert_eq!(
        interpreted_value(&ours, "apple", "PhotosAppFeatureFlags"),
        Some(first["PhotosAppFeatureFlags"].clone())
    );
    assert_float_close(
        interpreted_value(&ours, "apple", "HDRHeadroom").and_then(|value| value.as_f64()),
        first["HDRHeadroom"].as_f64(),
        "HDRHeadroom",
    );
    assert_float_close(
        interpreted_value(&ours, "apple", "SignalToNoiseRatio").and_then(|value| value.as_f64()),
        first["SignalToNoiseRatio"].as_f64(),
        "SignalToNoiseRatio",
    );
    assert_eq!(
        interpreted_value(&ours, "apple", "PhotoIdentifier"),
        Some(first["PhotoIdentifier"].clone())
    );
    assert_eq!(
        interpreted_value(&ours, "apple", "ColorTemperature"),
        Some(first["ColorTemperature"].clone())
    );
    assert_eq!(
        interpreted_value(&ours, "apple", "CameraType"),
        Some(Value::String("Back Normal".into()))
    );
    assert_eq!(
        interpreted_value(&ours, "apple", "FocusPosition"),
        Some(first["FocusPosition"].clone())
    );
    assert_float_close(
        interpreted_value(&ours, "apple", "HDRGain").and_then(|value| value.as_f64()),
        first["HDRGain"].as_f64(),
        "HDRGain",
    );
    assert_eq!(
        interpreted_value(&ours, "apple", "AFMeasuredDepth"),
        Some(first["AFMeasuredDepth"].clone())
    );
    assert_eq!(
        interpreted_value(&ours, "apple", "AFConfidence"),
        Some(first["AFConfidence"].clone())
    );
}

fn assert_float_close(left: Option<f64>, right: Option<f64>, label: &str) {
    let left = left.expect("missing left float value");
    let right = right.expect("missing right float value");
    assert!(
        (left - right).abs() < 0.000_001,
        "{label} mismatch {left} vs {right}"
    );
}
