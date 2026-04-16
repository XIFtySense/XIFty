use insta::assert_json_snapshot;
use serde_json::Value;
use std::{path::Path, process::Command};
use xifty_core::ViewMode;

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/minimal")
        .join(name)
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

fn probe_json(name: &str) -> Value {
    let mut value = serde_json::to_value(xifty_cli::probe_path(fixture(name)).unwrap()).unwrap();
    scrub_path(&mut value);
    value
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
fn extract_snapshot_real_camera_mp4_normalized() {
    assert_json_snapshot!(
        "extract_real_camera_mp4_normalized",
        extract_json("C0242.MP4", ViewMode::Normalized)
    );
}

#[test]
fn extract_snapshot_real_camera_mp4_interpreted() {
    assert_json_snapshot!(
        "extract_real_camera_mp4_interpreted",
        extract_json("C0242.MP4", ViewMode::Interpreted)
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
fn malformed_png_report_snapshot() {
    assert_json_snapshot!(
        "malformed_png_report",
        extract_json("malformed_chunk.png", ViewMode::Report)
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
fn exiftool_differential_real_camera_mp4_supported_fields() {
    differential_assert_camera_mp4("C0242.MP4");
}

#[test]
fn mp4_normalization_includes_media_fields() {
    let output = normalized_map(&extract_json("happy.mp4", ViewMode::Normalized));
    assert_eq!(output["duration"]["value"], Value::from(12.0));
    assert_eq!(output["codec.video"]["value"], Value::String("avc1".into()));
    assert_eq!(output["codec.audio"]["value"], Value::String("mp4a".into()));
    assert_eq!(output["author"]["value"], Value::String("Kai".into()));
    assert_eq!(
        output["software"]["value"],
        Value::String("XIFtyMediaGen".into())
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
    let output = normalized_map(&extract_json("DSC04504.JPG", ViewMode::Normalized));

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
    let output = extract_json("DSC04504.JPG", ViewMode::Interpreted);
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
    let output = extract_json("DSC04504.JPG", ViewMode::Interpreted);

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
    let output = normalized_map(&extract_json(
        "IMG_5B74BABE-DF0A-48EB-A6A4-6AAA54D5198E.JPEG",
        ViewMode::Normalized,
    ));

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
    let output = extract_json(
        "IMG_5B74BABE-DF0A-48EB-A6A4-6AAA54D5198E.JPEG",
        ViewMode::Interpreted,
    );
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
    let output = extract_json(
        "IMG_5B74BABE-DF0A-48EB-A6A4-6AAA54D5198E.JPEG",
        ViewMode::Interpreted,
    );

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
}

fn differential_assert_camera_mp4(name: &str) {
    let ours = extract_json(name, ViewMode::Normalized);
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

    assert_eq!(ours["device.make"]["value"], first["DeviceManufacturer"]);
    assert_eq!(ours["device.model"]["value"], first["DeviceModelName"]);
    assert_eq!(ours["dimensions.width"]["value"], first["ImageWidth"]);
    assert_eq!(ours["dimensions.height"]["value"], first["ImageHeight"]);
    assert_eq!(ours["codec.video"]["value"], first["CompressorID"]);
    assert_eq!(ours["codec.audio"]["value"], first["AudioFormat"]);
}

#[test]
fn exiftool_differential_real_camera_jpeg_supported_fields() {
    let ours = extract_json("DSC04504.JPG", ViewMode::Normalized);
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
        .arg(fixture("DSC04504.JPG"))
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
    let ours = extract_json("DSC04504.JPG", ViewMode::Interpreted);

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
        .arg(fixture("DSC04504.JPG"))
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
    let ours = extract_json(
        "IMG_5B74BABE-DF0A-48EB-A6A4-6AAA54D5198E.JPEG",
        ViewMode::Normalized,
    );
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
        .arg(fixture("IMG_5B74BABE-DF0A-48EB-A6A4-6AAA54D5198E.JPEG"))
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
    let ours = extract_json(
        "IMG_5B74BABE-DF0A-48EB-A6A4-6AAA54D5198E.JPEG",
        ViewMode::Interpreted,
    );

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
        .arg(fixture("IMG_5B74BABE-DF0A-48EB-A6A4-6AAA54D5198E.JPEG"))
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
