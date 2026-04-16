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

#[test]
fn probe_snapshot_happy_jpeg() {
    assert_json_snapshot!("probe_happy_jpeg", probe_json("happy.jpg"));
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
