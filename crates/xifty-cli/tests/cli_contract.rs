use insta::assert_json_snapshot;
use serde_json::Value;
use std::{fs, path::Path, process::Command, sync::OnceLock};
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
    scrub_filesystem_fallback_fields(value);
}

/// Redact filesystem-derived fallback fields so snapshots stay deterministic
/// across hosts. Filesystem `mtime`/`birthtime` values vary per checkout, and
/// the source `path` is an absolute path. We replace the timestamp value with
/// `<filesystem>` and the source path with the file name, but only for
/// entries whose source namespace is `filesystem` — embedded metadata is left
/// untouched.
fn scrub_filesystem_fallback_fields(value: &mut Value) {
    let Some(fields) = value
        .get_mut("normalized")
        .and_then(|normalized| normalized.get_mut("fields"))
        .and_then(|fields| fields.as_array_mut())
    else {
        return;
    };
    for field in fields {
        let from_filesystem = field
            .get("sources")
            .and_then(|s| s.as_array())
            .map(|sources| {
                !sources.is_empty()
                    && sources.iter().all(|source| {
                        source.get("namespace").and_then(|n| n.as_str()) == Some("filesystem")
                    })
            })
            .unwrap_or(false);
        if !from_filesystem {
            continue;
        }
        if let Some(sources) = field
            .get_mut("sources")
            .and_then(|sources| sources.as_array_mut())
        {
            for source in sources {
                if let Some(path_value) = source.get_mut("path") {
                    if let Some(path) = path_value.as_str().map(str::to_string) {
                        let name = Path::new(&path)
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or(path);
                        *path_value = Value::String(name);
                    }
                }
            }
        }
        if let Some(value_obj) = field.get_mut("value") {
            if value_obj.get("kind").and_then(|k| k.as_str()) == Some("timestamp") {
                if let Some(v) = value_obj.get_mut("value") {
                    *v = Value::String("<filesystem>".into());
                }
            }
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
fn probe_snapshot_happy_avif() {
    assert_json_snapshot!("probe_happy_avif", probe_json("happy.avif"));
}

#[test]
fn extract_snapshot_happy_avif_normalized() {
    assert_json_snapshot!(
        "extract_happy_avif_normalized",
        extract_json("happy.avif", ViewMode::Normalized)
    );
}

#[test]
fn extract_snapshot_happy_sequence_avif_normalized() {
    assert_json_snapshot!(
        "extract_happy_sequence_avif_normalized",
        extract_json("happy_sequence.avif", ViewMode::Normalized)
    );
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
fn probe_snapshot_happy_m4a() {
    assert_json_snapshot!("probe_happy_m4a", probe_json("happy.m4a"));
}

#[test]
fn probe_snapshot_dji_mavic3() {
    assert_json_snapshot!("probe_dji_mavic3", probe_json("dji_mavic3.mp4"));
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
fn png_text_creation_time_normalizes_to_captured_at() {
    let output = normalized_map(&extract_json("png_creation_time.png", ViewMode::Normalized));
    assert_eq!(
        output["captured_at"],
        serde_json::json!({
            "kind": "timestamp",
            "value": "2024-05-02T03:04:05Z"
        })
    );
}

#[test]
fn png_time_chunk_normalizes_to_captured_at() {
    let output = normalized_map(&extract_json("png_time_only.png", ViewMode::Normalized));
    assert_eq!(
        output["captured_at"],
        serde_json::json!({
            "kind": "timestamp",
            "value": "2024-05-02T03:04:05Z"
        })
    );
}

#[test]
fn apple_screenshot_png_uses_preserved_modified_time_when_birthtime_is_copy_time() {
    if !cfg!(target_os = "macos") {
        eprintln!("skipping macOS screenshot metadata test on non-macOS host");
        return;
    }

    let fixture_path = fixture("macos_screenshot_small.png");
    let temp_path = std::env::temp_dir().join(format!(
        "xifty-macos-screenshot-copy-{}-{}.png",
        std::process::id(),
        chrono_like_test_suffix()
    ));
    fs::copy(&fixture_path, &temp_path).unwrap();

    let xattr_status = Command::new("xattr")
        .arg("-w")
        .arg("com.apple.metadata:kMDItemIsScreenCapture")
        .arg("bplist00")
        .arg(&temp_path)
        .status()
        .unwrap();
    assert!(
        xattr_status.success(),
        "failed to mark temp file as screenshot"
    );

    let touch_status = Command::new("touch")
        .env("TZ", "UTC")
        .arg("-t")
        .arg("202503242109.11")
        .arg(&temp_path)
        .status()
        .unwrap();
    assert!(
        touch_status.success(),
        "failed to set preserved screenshot mtime"
    );

    let output = serde_json::to_value(
        xifty_cli::extract_path(temp_path.clone(), ViewMode::Normalized).unwrap(),
    )
    .unwrap();
    let output = normalized_map(&output);
    assert_eq!(
        output["captured_at"],
        serde_json::json!({
            "kind": "timestamp",
            "value": "2025-03-24T21:09:11Z"
        })
    );
    assert_eq!(
        output["created_at"],
        serde_json::json!({
            "kind": "timestamp",
            "value": "2025-03-24T21:09:11Z"
        })
    );

    let _ = fs::remove_file(temp_path);
}

#[test]
fn png_without_embedded_datetime_falls_back_to_filesystem_mtime() {
    use std::time::{Duration, SystemTime};

    let fixture_path = fixture("no_exif.png");
    let temp_path = std::env::temp_dir().join(format!(
        "xifty-mtime-fallback-{}-{}.png",
        std::process::id(),
        chrono_like_test_suffix()
    ));
    fs::copy(&fixture_path, &temp_path).unwrap();

    // Whole-second epoch so the formatted ISO-8601 string is stable across
    // filesystems with second-resolution mtimes.
    let target_unix_seconds: u64 = 1_705_320_000; // 2024-01-15T12:00:00Z
    let target = SystemTime::UNIX_EPOCH + Duration::from_secs(target_unix_seconds);
    let expected = "2024-01-15T12:00:00Z";

    let file = fs::File::options().write(true).open(&temp_path).unwrap();
    file.set_modified(target).unwrap();
    drop(file);

    let output = serde_json::to_value(
        xifty_cli::extract_path(temp_path.clone(), ViewMode::Normalized).unwrap(),
    )
    .unwrap();
    let output = normalized_map(&output);

    assert_eq!(
        output["captured_at"],
        serde_json::json!({
            "kind": "timestamp",
            "value": expected,
        }),
        "captured_at should be sourced from filesystem mtime when no embedded \
         capture timestamp is present"
    );
    assert_eq!(
        output["created_at"],
        serde_json::json!({
            "kind": "timestamp",
            "value": expected,
        }),
        "created_at should be sourced from filesystem mtime when no embedded \
         capture timestamp is present"
    );

    let _ = fs::remove_file(temp_path);
}

fn chrono_like_test_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        .to_string()
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
fn extract_snapshot_happy_m4a_normalized() {
    assert_json_snapshot!(
        "extract_happy_m4a_normalized",
        extract_json("happy.m4a", ViewMode::Normalized)
    );
}

#[test]
fn extract_snapshot_happy_m4a_interpreted() {
    assert_json_snapshot!(
        "extract_happy_m4a_interpreted",
        extract_json("happy.m4a", ViewMode::Interpreted)
    );
}

#[test]
fn extract_snapshot_dji_mavic3_normalized() {
    assert_json_snapshot!(
        "extract_dji_mavic3_normalized",
        extract_json("dji_mavic3.mp4", ViewMode::Normalized)
    );
}

#[test]
fn extract_snapshot_dji_mavic3_interpreted() {
    assert_json_snapshot!(
        "extract_dji_mavic3_interpreted",
        extract_json("dji_mavic3.mp4", ViewMode::Interpreted)
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
fn probe_snapshot_happy_dng() {
    assert_json_snapshot!("probe_happy_dng", probe_json("happy.dng"));
}

#[test]
fn extract_snapshot_happy_dng_normalized() {
    assert_json_snapshot!(
        "extract_happy_dng_normalized",
        extract_json("happy.dng", ViewMode::Normalized)
    );
}

#[test]
fn probe_snapshot_happy_cr2() {
    assert_json_snapshot!("probe_happy_cr2", probe_json("happy.cr2"));
}

#[test]
fn extract_snapshot_happy_cr2_raw() {
    assert_json_snapshot!(
        "extract_happy_cr2_raw",
        extract_json("happy.cr2", ViewMode::Raw)
    );
}

#[test]
fn extract_snapshot_happy_cr2_interpreted() {
    assert_json_snapshot!(
        "extract_happy_cr2_interpreted",
        extract_json("happy.cr2", ViewMode::Interpreted)
    );
}

#[test]
fn probe_snapshot_happy_arw() {
    assert_json_snapshot!("probe_happy_arw", probe_json("happy.arw"));
}

#[test]
fn extract_snapshot_happy_arw_normalized() {
    assert_json_snapshot!(
        "extract_happy_arw_normalized",
        extract_json("happy.arw", ViewMode::Normalized)
    );
}

#[test]
fn probe_snapshot_happy_raf() {
    assert_json_snapshot!("probe_happy_raf", probe_json("happy.raf"));
}

#[test]
fn extract_snapshot_happy_raf_normalized() {
    assert_json_snapshot!(
        "extract_happy_raf_normalized",
        extract_json("happy.raf", ViewMode::Normalized)
    );
}

#[test]
fn extract_snapshot_happy_raf_interpreted() {
    assert_json_snapshot!(
        "extract_happy_raf_interpreted",
        extract_json("happy.raf", ViewMode::Interpreted)
    );
}

#[test]
fn probe_snapshot_happy_orf() {
    assert_json_snapshot!("probe_happy_orf", probe_json("happy.orf"));
}

#[test]
fn extract_snapshot_happy_orf_normalized() {
    assert_json_snapshot!(
        "extract_happy_orf_normalized",
        extract_json("happy.orf", ViewMode::Normalized)
    );
}

#[test]
fn probe_snapshot_happy_cr3() {
    assert_json_snapshot!("probe_happy_cr3", probe_json("happy.cr3"));
}

#[test]
fn extract_snapshot_happy_cr3_interpreted() {
    assert_json_snapshot!(
        "extract_happy_cr3_interpreted",
        extract_json("happy.cr3", ViewMode::Interpreted)
    );
}

#[test]
fn probe_snapshot_happy_nef() {
    assert_json_snapshot!("probe_happy_nef", probe_json("happy.nef"));
}

#[test]
fn extract_snapshot_happy_nef_interpreted() {
    assert_json_snapshot!(
        "extract_happy_nef_interpreted",
        extract_json("happy.nef", ViewMode::Interpreted)
    );
}

/// Encrypted-region surfacing regression: the synthetic NEF embeds a
/// 0x0091 ShotInfo block with a v0204 version preamble. The decoder must
/// (1) NOT include 0x0091 in `nikon` entries and (2) the report must carry
/// exactly one `nikon_makernote_encrypted_region` Issue pointing at the
/// outer-file absolute byte offset of the encrypted blob.
#[test]
fn nef_encrypted_makernote_region_surfaces_as_issue() {
    let analysis = xifty_cli::extract_path(fixture("happy.nef"), ViewMode::Interpreted).unwrap();
    let interpreted = analysis.interpreted.expect("interpreted view present");
    assert!(
        !interpreted
            .metadata
            .iter()
            .any(|e| e.namespace == "nikon" && e.tag_id == "0x0091"),
        "encrypted ShotInfo (0x0091) must NOT appear in `nikon` entries: {:?}",
        interpreted.metadata
    );
    let encrypted_issues: Vec<_> = analysis
        .report
        .issues
        .iter()
        .filter(|i| i.code == "nikon_makernote_encrypted_region")
        .collect();
    assert_eq!(
        encrypted_issues.len(),
        1,
        "expected exactly one encrypted-region issue, got {}: {:?}",
        encrypted_issues.len(),
        analysis.report.issues
    );
    // The synthetic fixture lays the encrypted blob at outer-file byte 206.
    // If the fixture layout changes, update this constant alongside it.
    assert_eq!(encrypted_issues[0].offset, Some(206));
}

#[test]
fn probe_snapshot_panasonic_rw2() {
    assert_json_snapshot!("probe_panasonic_rw2", probe_json("happy.rw2"));
}

#[test]
fn extract_snapshot_panasonic_rw2_interpreted() {
    assert_json_snapshot!(
        "extract_panasonic_rw2_interpreted",
        extract_json("happy.rw2", ViewMode::Interpreted)
    );
}

/// Tag-ID collision regression: tag 0x010F means "Make" in standard TIFF /
/// EXIF, but in Panasonic RW2 IFD0 it is a Panasonic-private value. The
/// fixture deliberately includes 0x010F so this test can prove RW2 metadata
/// lands in the `panasonic` namespace and NEVER leaks into `exif`. This is
/// the load-bearing rule documented in `xifty-container-rw2` and
/// `xifty-meta-panasonic`.
#[test]
fn rw2_metadata_uses_panasonic_namespace_never_exif() {
    let analysis = xifty_cli::extract_path(fixture("happy.rw2"), ViewMode::Interpreted).unwrap();
    let interpreted = analysis.interpreted.expect("interpreted view present");
    assert!(
        !interpreted.metadata.is_empty(),
        "expected at least one panasonic entry"
    );
    let panasonic_count = interpreted
        .metadata
        .iter()
        .filter(|e| e.namespace == "panasonic")
        .count();
    let exif_count = interpreted
        .metadata
        .iter()
        .filter(|e| e.namespace == "exif")
        .count();
    assert!(
        panasonic_count >= 1,
        "expected at least one entry in `panasonic` namespace, got {panasonic_count} (entries: {:?})",
        interpreted.metadata,
    );
    assert_eq!(
        exif_count, 0,
        "RW2 must not produce any `exif` namespace entries (collision policy violated): {:?}",
        interpreted.metadata,
    );
    // The 0x010F canary specifically must land in `panasonic`.
    let canary = interpreted
        .metadata
        .iter()
        .find(|e| e.tag_id == "0x010F")
        .expect("0x010F collision canary present");
    assert_eq!(canary.namespace, "panasonic");
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
fn extract_snapshot_icc_heic_normalized() {
    assert_json_snapshot!(
        "extract_icc_heic_normalized",
        extract_json("icc.heic", ViewMode::Normalized)
    );
}

#[test]
fn extract_snapshot_iptc_heic_normalized() {
    assert_json_snapshot!(
        "extract_iptc_heic_normalized",
        extract_json("iptc.heic", ViewMode::Normalized)
    );
}

#[test]
fn icc_heic_interpreted_view_includes_icc_fields() {
    let output = extract_json("icc.heic", ViewMode::Interpreted);
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
fn iptc_heic_normalization_includes_editorial_fields() {
    let output = normalized_map(&extract_json("iptc.heic", ViewMode::Normalized));
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
    // Proves xifty-validate's cross-namespace disagreement rule
    // (detect_cross_namespace_disagreement in xifty-validate/src/rules.rs:94-137)
    // actually runs in the CLI pipeline. The load-bearing asymmetry is the
    // `captured_at` field on fixtures/minimal/validate_conflicts.png:
    //   - exif DateTimeOriginal = "2024:04:16 12:34:56" (EXIF colon-date form)
    //   - xmp  CreateDate       = "2024-04-16T12:34:56" (XMP ISO form)
    // xifty-policy's `normalize_timestamp` (xifty-policy/src/lib.rs:575-592)
    // canonicalises EXIF colon-dates to the ISO form, so policy's
    // `typed_values_equal` reports no material difference and emits NO
    // captured_at conflict. xifty-validate canonicalises with plain
    // `trimmed.to_string()`, treats the two strings as distinct, and emits a
    // conflict with the distinctive "xmp:CreateDate=... vs
    // exif:DateTimeOriginal=..." message.
    //
    // The negative assertion on "selected" is the load-bearing piece: policy
    // messages carry "selected"; validate messages do not. If someone drops
    // `detect_conflicts` from the pipeline, the captured_at entry vanishes
    // entirely (policy is silent on this field) and the `.expect` fires. If
    // someone weakens `normalize_timestamp` so policy also emits this field,
    // dedupe tie-breaks to the policy message and the "selected"-negative
    // assertion fires. Either regression is caught here.
    let output = extract_json("validate_conflicts.png", ViewMode::Report);
    let conflicts = output["report"]["conflicts"].as_array().unwrap();
    let captured_at = conflicts
        .iter()
        .find(|c| c["field"] == "captured_at")
        .expect(
            "missing captured_at conflict in report.conflicts — \
             xifty-validate detect_cross_namespace_disagreement appears not \
             to have fired; xifty-policy does not emit this field because \
             normalize_timestamp collapses EXIF colon-date and XMP ISO-date \
             to the same canonical form",
        );
    let message = captured_at["message"]
        .as_str()
        .expect("captured_at conflict missing message");
    assert!(
        message.contains(" vs "),
        "expected validate-rule ' vs ' infix in captured_at message, got {:?}",
        message
    );
    assert!(
        message.contains("CreateDate"),
        "expected xmp:CreateDate tag name in captured_at message, got {:?}",
        message
    );
    assert!(
        message.contains("DateTimeOriginal"),
        "expected exif:DateTimeOriginal tag name in captured_at message, got {:?}",
        message
    );
    assert!(
        !message.contains("selected"),
        "captured_at message contains 'selected' — this is the policy-layer \
         format, indicating either dedupe collapsed the validate entry into a \
         policy entry or the validate rule did not fire at all; got {:?}",
        message
    );
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
    let values: std::collections::BTreeSet<&str> = sources
        .iter()
        .filter_map(|side| side["value"]["value"].as_str())
        .collect();
    assert!(
        values.contains("2024:04:16 12:34:56"),
        "expected raw EXIF colon-date value in captured_at sources, got {:?}",
        values
    );
    assert!(
        values.contains("2024-04-16T12:34:56"),
        "expected raw XMP ISO-date value in captured_at sources, got {:?}",
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

#[test]
fn probe_snapshot_happy_flac() {
    assert_json_snapshot!("probe_happy_flac", probe_json("happy.flac"));
}

#[test]
fn extract_snapshot_happy_flac_normalized() {
    assert_json_snapshot!(
        "extract_happy_flac_normalized",
        extract_json("happy.flac", ViewMode::Normalized)
    );
}

#[test]
fn flac_normalization_includes_audio_fields() {
    let output = extract_json("happy.flac", ViewMode::Normalized);
    let normalized = normalized_map(&output);
    assert_eq!(
        normalized
            .get("audio.sample_rate")
            .and_then(|v| v["value"].as_i64()),
        Some(44100)
    );
    assert_eq!(
        normalized
            .get("audio.channels")
            .and_then(|v| v["value"].as_i64()),
        Some(2)
    );
    assert_eq!(
        normalized
            .get("audio.bit_depth")
            .and_then(|v| v["value"].as_i64()),
        Some(16)
    );
    assert_eq!(
        normalized.get("duration").and_then(|v| v["value"].as_f64()),
        Some(1.0)
    );
}

#[test]
fn flac_surfaces_vorbis_comment_tags() {
    let output = extract_json("happy.flac", ViewMode::Interpreted);
    assert_eq!(
        interpreted_value(&output, "vorbis_comment", "Title"),
        Some(Value::String("XIFty Track".into()))
    );
    assert_eq!(
        interpreted_value(&output, "vorbis_comment", "Artist"),
        Some(Value::String("XIFty Artist".into()))
    );
    assert_eq!(
        interpreted_value(&output, "vorbis_comment", "Album"),
        Some(Value::String("XIFty Album".into()))
    );
}

#[test]
fn flac_surfaces_picture_block_on_raw_view() {
    let output = extract_json("happy.flac", ViewMode::Raw);
    let mime = output["raw"]["metadata"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["tag_name"] == "PictureMimeType")
        .expect("PictureMimeType entry present");
    assert_eq!(mime["value"]["value"].as_str(), Some("image/png"));
    let width = output["raw"]["metadata"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["tag_name"] == "PictureWidth")
        .expect("PictureWidth entry present");
    assert_eq!(width["value"]["value"].as_i64(), Some(1));
    let height = output["raw"]["metadata"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["tag_name"] == "PictureHeight")
        .expect("PictureHeight entry present");
    assert_eq!(height["value"]["value"].as_i64(), Some(1));
}

#[test]
fn probe_snapshot_happy_aiff() {
    assert_json_snapshot!("probe_happy_aiff", probe_json("happy.aiff"));
}

#[test]
fn probe_snapshot_happy_aifc() {
    assert_json_snapshot!("probe_happy_aifc", probe_json("happy.aifc"));
}

#[test]
fn extract_snapshot_happy_aiff_normalized() {
    assert_json_snapshot!(
        "extract_happy_aiff_normalized",
        extract_json("happy.aiff", ViewMode::Normalized)
    );
}

#[test]
fn aiff_normalization_includes_audio_fields() {
    let output = extract_json("happy.aiff", ViewMode::Normalized);
    let normalized = normalized_map(&output);
    assert_eq!(
        normalized
            .get("audio.sample_rate")
            .and_then(|v| v["value"].as_i64()),
        Some(44100)
    );
    assert_eq!(
        normalized
            .get("audio.channels")
            .and_then(|v| v["value"].as_i64()),
        Some(2)
    );
    assert_eq!(
        normalized
            .get("audio.bit_depth")
            .and_then(|v| v["value"].as_i64()),
        Some(16)
    );
    assert_eq!(
        normalized.get("duration").and_then(|v| v["value"].as_f64()),
        Some(1.0)
    );
}

#[test]
fn probe_snapshot_happy_ogg() {
    assert_json_snapshot!("probe_happy_ogg", probe_json("happy.ogg"));
}

#[test]
fn extract_snapshot_happy_ogg_normalized() {
    assert_json_snapshot!(
        "extract_happy_ogg_normalized",
        extract_json("happy.ogg", ViewMode::Normalized)
    );
}

#[test]
fn ogg_vorbis_normalization_includes_audio_fields() {
    let output = extract_json("happy.ogg", ViewMode::Normalized);
    let normalized = normalized_map(&output);
    assert_eq!(
        normalized
            .get("audio.sample_rate")
            .and_then(|v| v["value"].as_i64()),
        Some(44100)
    );
    assert_eq!(
        normalized
            .get("audio.channels")
            .and_then(|v| v["value"].as_i64()),
        Some(2)
    );
    assert_eq!(
        normalized
            .get("audio.bit_depth")
            .and_then(|v| v["value"].as_i64()),
        Some(16)
    );
    assert_eq!(
        normalized.get("duration").and_then(|v| v["value"].as_f64()),
        Some(1.0)
    );
    assert_eq!(
        normalized
            .get("codec.audio")
            .and_then(|v| v["value"].as_str()),
        Some("vorbis")
    );
}

#[test]
fn ogg_vorbis_surfaces_vorbis_comment_tags() {
    let output = extract_json("happy.ogg", ViewMode::Interpreted);
    assert_eq!(
        interpreted_value(&output, "vorbis_comment", "Title"),
        Some(Value::String("XIFty Track".into()))
    );
    assert_eq!(
        interpreted_value(&output, "vorbis_comment", "Artist"),
        Some(Value::String("XIFty Artist".into()))
    );
    assert_eq!(
        interpreted_value(&output, "vorbis_comment", "Album"),
        Some(Value::String("XIFty Album".into()))
    );
}

#[test]
fn probe_snapshot_happy_opus() {
    assert_json_snapshot!("probe_happy_opus", probe_json("happy.opus"));
}

#[test]
fn extract_snapshot_happy_opus_normalized() {
    assert_json_snapshot!(
        "extract_happy_opus_normalized",
        extract_json("happy.opus", ViewMode::Normalized)
    );
}

#[test]
fn ogg_opus_normalization_includes_audio_fields() {
    let output = extract_json("happy.opus", ViewMode::Normalized);
    let normalized = normalized_map(&output);
    assert_eq!(
        normalized
            .get("audio.sample_rate")
            .and_then(|v| v["value"].as_i64()),
        Some(48_000)
    );
    assert_eq!(
        normalized
            .get("audio.channels")
            .and_then(|v| v["value"].as_i64()),
        Some(2)
    );
    // Opus ident does not encode bits-per-sample — normalized bit_depth must be absent.
    assert!(normalized.get("audio.bit_depth").is_none());
    assert_eq!(
        normalized.get("duration").and_then(|v| v["value"].as_f64()),
        Some(1.0)
    );
    assert_eq!(
        normalized
            .get("codec.audio")
            .and_then(|v| v["value"].as_str()),
        Some("opus")
    );
}

#[test]
fn ogg_opus_surfaces_vorbis_comment_tags() {
    let output = extract_json("happy.opus", ViewMode::Interpreted);
    assert_eq!(
        interpreted_value(&output, "vorbis_comment", "Title"),
        Some(Value::String("XIFty Track".into()))
    );
    assert_eq!(
        interpreted_value(&output, "vorbis_comment", "Artist"),
        Some(Value::String("XIFty Artist".into()))
    );
    assert_eq!(
        interpreted_value(&output, "vorbis_comment", "Album"),
        Some(Value::String("XIFty Album".into()))
    );
}

#[test]
fn probe_snapshot_happy_mp3() {
    assert_json_snapshot!("probe_happy_mp3", probe_json("happy.mp3"));
}

#[test]
fn extract_snapshot_happy_mp3_normalized() {
    assert_json_snapshot!(
        "extract_happy_mp3_normalized",
        extract_json("happy.mp3", ViewMode::Normalized)
    );
}

#[test]
fn mp3_normalization_includes_audio_fields() {
    let output = extract_json("happy.mp3", ViewMode::Normalized);
    let normalized = normalized_map(&output);
    assert_eq!(
        normalized
            .get("audio.sample_rate")
            .and_then(|v| v["value"].as_i64()),
        Some(44100)
    );
    assert_eq!(
        normalized
            .get("audio.channels")
            .and_then(|v| v["value"].as_i64()),
        Some(2)
    );
    assert_eq!(
        normalized
            .get("audio.bit_depth")
            .and_then(|v| v["value"].as_i64()),
        Some(16)
    );
    assert_eq!(
        normalized
            .get("codec.audio")
            .and_then(|v| v["value"].as_str()),
        Some("mp3")
    );
    // 10 frames * 417 bytes audio_bytes => duration ≈ 0.260625 s
    let duration = normalized
        .get("duration")
        .and_then(|v| v["value"].as_f64())
        .expect("duration present");
    assert!((duration - 0.260625).abs() < 1e-6, "got {duration}");
}

#[test]
fn mp3_surfaces_id3v2_text_frames() {
    let output = extract_json("happy.mp3", ViewMode::Interpreted);
    assert_eq!(
        interpreted_value(&output, "id3v2", "Title"),
        Some(Value::String("XIFty MP3 Track".into()))
    );
    assert_eq!(
        interpreted_value(&output, "id3v2", "Artist"),
        Some(Value::String("XIFty Artist".into()))
    );
    assert_eq!(
        interpreted_value(&output, "id3v2", "Album"),
        Some(Value::String("XIFty Album".into()))
    );
}

#[test]
fn mp3_xing_vbr_duration_uses_total_frames() {
    let output = extract_json("vbr_xing.mp3", ViewMode::Normalized);
    let normalized = normalized_map(&output);
    // 20 frames * 1152 samples / 44100 ≈ 0.5224 s
    let duration = normalized
        .get("duration")
        .and_then(|v| v["value"].as_f64())
        .expect("duration present for VBR Xing fixture");
    let expected = 20.0 * 1152.0 / 44100.0;
    assert!(
        (duration - expected).abs() < 1e-6,
        "got {duration}, expected {expected}"
    );
}

#[test]
fn mp3_vbr_no_xing_emits_duration_unknown_issue() {
    // The vbr_no_xing fixture alternates frame bitrates without a Xing/Info
    // or VBRI header. The container parser walks subsequent frames, detects
    // the bitrate variation, and emits `mp3_vbr_duration_unknown`.
    let output = extract_json("vbr_no_xing.mp3", ViewMode::Full);
    assert_eq!(output["input"]["detected_format"].as_str(), Some("mp3"));
    assert_eq!(output["input"]["container"].as_str(), Some("mp3"));

    let issues = output["report"]["issues"]
        .as_array()
        .expect("issues array present");
    let has_vbr_unknown = issues
        .iter()
        .any(|i| i["code"].as_str() == Some("mp3_vbr_duration_unknown"));
    assert!(
        has_vbr_unknown,
        "expected mp3_vbr_duration_unknown in issues, got {issues:?}"
    );
}

#[test]
fn probe_snapshot_happy_wav() {
    assert_json_snapshot!("probe_happy_wav", probe_json("happy.wav"));
}

#[test]
fn extract_snapshot_happy_wav_normalized() {
    assert_json_snapshot!(
        "extract_happy_wav_normalized",
        extract_json("happy.wav", ViewMode::Normalized)
    );
}

#[test]
fn extract_snapshot_bext_wav_interpreted() {
    assert_json_snapshot!(
        "extract_bext_wav_interpreted",
        extract_json("bext.wav", ViewMode::Interpreted)
    );
}

#[test]
fn wav_normalization_includes_audio_fields() {
    let output = extract_json("happy.wav", ViewMode::Normalized);
    let normalized = normalized_map(&output);
    assert_eq!(
        normalized
            .get("audio.sample_rate")
            .and_then(|v| v["value"].as_i64()),
        Some(44100)
    );
    assert_eq!(
        normalized
            .get("audio.channels")
            .and_then(|v| v["value"].as_i64()),
        Some(1)
    );
    assert_eq!(
        normalized
            .get("audio.bit_depth")
            .and_then(|v| v["value"].as_i64()),
        Some(16)
    );
    assert_eq!(
        normalized
            .get("codec.audio")
            .and_then(|v| v["value"].as_str()),
        Some("pcm")
    );
    let duration = normalized
        .get("duration")
        .and_then(|v| v["value"].as_f64())
        .expect("duration present");
    assert!((duration - 1.0).abs() < 1e-6, "got {duration}");
}

#[test]
fn wav_surfaces_bext_and_ixml() {
    let output = extract_json("bext.wav", ViewMode::Interpreted);
    assert_eq!(
        interpreted_value(&output, "bwf", "Description"),
        Some(Value::String("XIFty BWF fixture".into()))
    );
    assert_eq!(
        interpreted_value(&output, "bwf", "Originator"),
        Some(Value::String("XIFty".into()))
    );
    assert_eq!(
        interpreted_value(&output, "ixml", "PROJECT"),
        Some(Value::String("xifty".into()))
    );
    assert_eq!(
        interpreted_value(&output, "ixml", "SCENE"),
        Some(Value::String("test".into()))
    );
}

#[test]
fn wav_does_not_emit_riff_non_webp_form_issue() {
    let output = extract_json("happy.wav", ViewMode::Full);
    assert_eq!(output["input"]["detected_format"].as_str(), Some("wav"));
    assert_eq!(output["input"]["container"].as_str(), Some("wav"));
    let issues = output["report"]["issues"].as_array().unwrap();
    assert!(
        !issues
            .iter()
            .any(|i| i["code"].as_str() == Some("riff_non_webp_form")),
        "WAV should not emit riff_non_webp_form, got {issues:?}"
    );
}

#[test]
fn probe_snapshot_happy_gif() {
    assert_json_snapshot!("probe_happy_gif", probe_json("happy.gif"));
}

#[test]
fn probe_snapshot_animated_gif() {
    assert_json_snapshot!("probe_animated_gif", probe_json("animated.gif"));
}

#[test]
fn extract_snapshot_happy_gif_report() {
    assert_json_snapshot!(
        "extract_happy_gif_report",
        extract_json("happy.gif", ViewMode::Report)
    );
}

#[test]
fn extract_snapshot_animated_gif_normalized() {
    assert_json_snapshot!(
        "extract_animated_gif_normalized",
        extract_json("animated.gif", ViewMode::Normalized)
    );
}

#[test]
fn extract_snapshot_xmp_gif_normalized() {
    assert_json_snapshot!(
        "extract_xmp_gif_normalized",
        extract_json("xmp.gif", ViewMode::Normalized)
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

// ---------------------------------------------------------------------------
// Sidecar (Sony NRT) integration tests — Phase 1 of the parent epic #121.
// ---------------------------------------------------------------------------

fn extract_with_sidecars(path: std::path::PathBuf, view: ViewMode) -> Value {
    let options = xifty_cli::ExtractOptions {
        enable_sidecars: true,
    };
    let mut value =
        serde_json::to_value(xifty_cli::extract_path_with_options(path, view, options).unwrap())
            .unwrap();
    scrub_path(&mut value);
    value
}

#[test]
fn sony_nrt_sidecar_surfaces_all_mapped_fields_on_real_fixture() {
    // Real-fixture gate: needs both `C0242.MP4` and the sibling
    // `C0242M01.XML` in `fixtures/local/`. CI skips when either is absent.
    let mp4 = match optional_fixture("C0242.MP4") {
        Some(path) => path,
        None => {
            skip_missing_local_fixture("C0242.MP4");
            return;
        }
    };
    let xml_sibling = mp4.with_file_name("C0242M01.XML");
    if !xml_sibling.exists() {
        skip_missing_local_fixture("C0242M01.XML");
        return;
    }

    let output = extract_with_sidecars(mp4, ViewMode::Full);

    // Every entry the NRT adapter emits must carry namespace `sony_nrt`,
    // and the same provenance must propagate into the lifted normalized
    // fields (`sources[].namespace == "sony_nrt"`).
    let interpreted = output["interpreted"]["metadata"]
        .as_array()
        .expect("interpreted view present");
    let nrt_entries: Vec<_> = interpreted
        .iter()
        .filter(|e| e["namespace"] == "sony_nrt")
        .collect();
    assert!(
        !nrt_entries.is_empty(),
        "expected sony_nrt entries to surface from sibling NRT XML"
    );

    let normalized_fields = output["normalized"]["fields"]
        .as_array()
        .expect("normalized view present");
    let lifted: std::collections::BTreeMap<String, &Value> = normalized_fields
        .iter()
        .map(|f| (f["field"].as_str().unwrap().to_string(), f))
        .collect();

    for required in [
        "umid",
        "recording.mode",
        "recording.capture_fps",
        "recording.format_fps",
        "device.serial_no",
        "color.gamma_equation",
        "timecode.ltc.start",
    ] {
        let field = lifted
            .get(required)
            .unwrap_or_else(|| panic!("expected normalized field {required} from NRT sidecar"));
        let sources = field["sources"].as_array().unwrap();
        assert!(
            sources.iter().any(|s| s["namespace"] == "sony_nrt"),
            "{required} must list sony_nrt as a source, got {sources:?}"
        );
    }
}

#[test]
fn sony_nrt_sidecar_is_off_by_default() {
    // Without `--sidecars`, even when the XML sibling is on disk, the
    // existing default-off code path must not surface any `sony_nrt`
    // entries. Confirms the flag is opt-in.
    let mp4 = match optional_fixture("C0242.MP4") {
        Some(path) => path,
        None => {
            skip_missing_local_fixture("C0242.MP4");
            return;
        }
    };
    let output = extract_optional_json("C0242.MP4", ViewMode::Interpreted)
        .expect("C0242.MP4 readable in default mode");
    let _ = mp4;
    let interpreted = output["interpreted"]["metadata"]
        .as_array()
        .expect("interpreted view present");
    let nrt_entries: Vec<_> = interpreted
        .iter()
        .filter(|e| e["namespace"] == "sony_nrt")
        .collect();
    assert!(
        nrt_entries.is_empty(),
        "default extract_path must not surface sony_nrt entries (sidecars are opt-in)"
    );
}

#[test]
fn sony_nrt_sidecar_synthetic_minimal_fixture_lifts_fields() {
    // Synthetic fixture pair generated by `tools/generate_fixtures.py`
    // (build_sony_nrt_pair). Both files are checked in under
    // `fixtures/minimal/sony-nrt/`. This test runs unconditionally — if
    // it ever fails, the generator and the parser are out of sync.
    let mp4 =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/minimal/sony-nrt/clip.mp4");
    if !mp4.exists() {
        // The fixture pair lives under fixtures/minimal/ but is generated;
        // fall back gracefully for fresh checkouts that haven't run the
        // generator yet.
        eprintln!("skipping synthetic NRT fixture test; run tools/generate_fixtures.py");
        return;
    }
    let output = extract_with_sidecars(mp4, ViewMode::Normalized);
    let normalized: std::collections::BTreeMap<String, Value> = output["normalized"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| (f["field"].as_str().unwrap().to_string(), f.clone()))
        .collect();
    // All 12 normalized fields lifted by `derive_sony_nrt_fields` for Phase 1.
    // Keep this list in lockstep with the unit-test coverage in
    // `xifty-normalize::tests::lifts_sony_nrt_sidecar_fields_from_namespace`.
    for required in [
        "umid",
        "recording.mode",
        "recording.cache_rec",
        "recording.capture_fps",
        "recording.format_fps",
        "timecode.fps",
        "timecode.half_step",
        "timecode.ltc.start",
        "timecode.ltc.end",
        "color.gamma_equation",
        "color.coding_equations",
        "device.serial_no",
    ] {
        assert!(
            normalized.contains_key(required),
            "synthetic fixture must surface {required}, got {:?}",
            normalized.keys().collect::<Vec<_>>()
        );
    }
}

// ---------------------------------------------------------------------------
// Sidecar (subtitles) integration tests — issue #131.
// ---------------------------------------------------------------------------

#[test]
fn subtitles_sidecar_synthetic_fixture_lifts_per_sidecar_entries() {
    // Synthetic-fixture test: spin up a tempdir with a fake `clip.mp4`
    // (4 magic bytes — the sidecar layer never decodes it, only uses its
    // path as the discovery anchor) plus two real subtitle siblings —
    // `clip.en.srt` and `clip.es.vtt`. Verify both surface as separate
    // entry groups indexed `subtitles.0.*` (each comes from its own
    // discovery hit; the registry walks the adapter once per sidecar).
    let dir = std::env::temp_dir().join("xifty-cli-subtitles-synthetic");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mp4 = dir.join("clip.mp4");
    // Just enough bytes to keep the detector from short-circuiting on a
    // zero-byte read; the sidecar layer doesn't care what's here. We set
    // up a minimal isobmff `ftyp` so detection succeeds.
    let ftyp_bytes: Vec<u8> = [
        0x00, 0x00, 0x00, 0x18, b'f', b't', b'y', b'p', b'm', b'p', b'4', b'2', 0x00, 0x00, 0x00,
        0x00, b'm', b'p', b'4', b'2', b'i', b's', b'o', b'm',
    ]
    .into();
    std::fs::write(&mp4, &ftyp_bytes).unwrap();
    std::fs::write(
        dir.join("clip.en.srt"),
        "1\n00:00:01,000 --> 00:00:04,000\nHello in English\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("clip.es.vtt"),
        "WEBVTT\n\n00:00:01.000 --> 00:00:04.000\nHola en espanol\n",
    )
    .unwrap();

    let output = extract_with_sidecars(mp4.clone(), ViewMode::Full);
    let interpreted = output["interpreted"]["metadata"]
        .as_array()
        .expect("interpreted view present");
    let subtitle_entries: Vec<&Value> = interpreted
        .iter()
        .filter(|e| e["namespace"] == "subtitles")
        .collect();
    assert!(
        !subtitle_entries.is_empty(),
        "expected subtitle entries to surface from sibling sidecars"
    );

    // Each adapter `parse` call emits its own `subtitles.0.*` group,
    // distinguished by `provenance.path`. Group entries by the path
    // suffix so we can assert per-sidecar shape.
    let mut by_path: std::collections::BTreeMap<String, Vec<&Value>> =
        std::collections::BTreeMap::new();
    for entry in &subtitle_entries {
        let path = entry["provenance"]["path"]
            .as_str()
            .unwrap_or("")
            .to_string();
        by_path.entry(path).or_default().push(entry);
    }
    assert_eq!(
        by_path.len(),
        2,
        "expected one entry-group per sidecar, got {by_path:?}"
    );

    // Verify each sidecar surfaces its core fields.
    for entries in by_path.values() {
        let format = entries
            .iter()
            .find(|e| e["tag_name"] == "subtitles.0.format")
            .expect("format entry present");
        let language = entries
            .iter()
            .find(|e| e["tag_name"] == "subtitles.0.language")
            .expect("language entry present");
        let format_str = format["value"]["value"].as_str().unwrap();
        let language_str = language["value"]["value"].as_str().unwrap();
        assert!(
            (format_str == "srt" && language_str == "en")
                || (format_str == "vtt" && language_str == "es"),
            "unexpected (format, language) pair: ({format_str}, {language_str})"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn subtitles_sidecar_is_off_by_default() {
    // Without `enable_sidecars: true`, even when subtitle siblings are on
    // disk, no `subtitles` namespace entries surface.
    let dir = std::env::temp_dir().join("xifty-cli-subtitles-default-off");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mp4 = dir.join("clip.mp4");
    let ftyp_bytes: Vec<u8> = [
        0x00, 0x00, 0x00, 0x18, b'f', b't', b'y', b'p', b'm', b'p', b'4', b'2', 0x00, 0x00, 0x00,
        0x00, b'm', b'p', b'4', b'2', b'i', b's', b'o', b'm',
    ]
    .into();
    std::fs::write(&mp4, &ftyp_bytes).unwrap();
    std::fs::write(
        dir.join("clip.en.srt"),
        "1\n00:00:01,000 --> 00:00:04,000\nHello\n",
    )
    .unwrap();

    let mut value =
        serde_json::to_value(xifty_cli::extract_path(mp4.clone(), ViewMode::Interpreted).unwrap())
            .unwrap();
    scrub_path(&mut value);
    // Interpreted view may be omitted entirely if the primary file has no
    // recognized metadata. Treat absence as "no subtitle entries", which is
    // exactly what we want to assert.
    let subtitle_count = value["interpreted"]["metadata"]
        .as_array()
        .map(|m| m.iter().filter(|e| e["namespace"] == "subtitles").count())
        .unwrap_or(0);
    assert_eq!(
        subtitle_count, 0,
        "default extract_path must not surface subtitles entries (sidecars are opt-in)"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// Sidecar (GoPro `.lrv` / `.thm`) integration tests — issue #134.
// ---------------------------------------------------------------------------

#[test]
fn gopro_sidecar_is_off_by_default() {
    // Without `--sidecars`, the sibling `.LRV` / `.THM` files must not
    // surface any `gopro_sidecar` entries through `extract_path`. Confirms
    // the flag is opt-in for GoPro just like Sony NRT.
    let mp4 =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/minimal/gopro/GH010001.MP4");
    if !mp4.exists() {
        eprintln!("skipping synthetic GoPro fixture test; run tools/generate_fixtures.py");
        return;
    }
    let analysis = xifty_cli::extract_path(mp4, ViewMode::Interpreted).unwrap();
    let value = serde_json::to_value(&analysis).unwrap();
    let interpreted = value["interpreted"]["metadata"]
        .as_array()
        .expect("interpreted view present");
    let gopro_entries: Vec<_> = interpreted
        .iter()
        .filter(|e| e["namespace"] == "gopro_sidecar")
        .collect();
    assert!(
        gopro_entries.is_empty(),
        "default extract_path must not surface gopro_sidecar entries (sidecars are opt-in)"
    );
}

#[test]
fn gopro_sidecar_synthetic_minimal_fixture_lifts_proxy_and_thumbnail() {
    // Synthetic GH/GL/THM triplet checked in under `fixtures/minimal/gopro/`.
    // Asserts the structural proxy + thumbnail entries land in the
    // interpreted view under namespace `gopro_sidecar` when the sidecar
    // option is enabled.
    let mp4 =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/minimal/gopro/GH010001.MP4");
    if !mp4.exists() {
        eprintln!("skipping synthetic GoPro fixture test; run tools/generate_fixtures.py");
        return;
    }
    let output = extract_with_sidecars(mp4, ViewMode::Interpreted);
    let interpreted = output["interpreted"]["metadata"]
        .as_array()
        .expect("interpreted view present");
    let by_tag: std::collections::BTreeMap<String, &Value> = interpreted
        .iter()
        .filter(|e| e["namespace"] == "gopro_sidecar")
        .map(|e| (e["tag_name"].as_str().unwrap().to_string(), e))
        .collect();
    assert!(
        !by_tag.is_empty(),
        "expected gopro_sidecar entries to surface from sibling LRV/THM"
    );
    // Thumbnail dimensions from the 320x180 SOF0 JFIF.
    assert_eq!(
        by_tag
            .get("thumbnail.dimensions.width")
            .and_then(|e| e["value"]["value"].as_i64()),
        Some(320),
        "expected thumbnail.dimensions.width=320, got entries: {:?}",
        by_tag.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        by_tag
            .get("thumbnail.dimensions.height")
            .and_then(|e| e["value"]["value"].as_i64()),
        Some(180)
    );
    // Proxy structural fields from the 640x360 LRV.
    assert_eq!(
        by_tag
            .get("proxy.dimensions.width")
            .and_then(|e| e["value"]["value"].as_i64()),
        Some(640)
    );
    assert!(by_tag.contains_key("proxy.bitrate_bps"));
    assert!(by_tag.contains_key("proxy.path"));
    assert!(by_tag.contains_key("thumbnail.path"));
}

// ---------------------------------------------------------------------------
// C2PA content provenance tests — issue #132. Synthetic fixture is
// committed; real-device fixtures are gated behind `optional_fixture` so
// they skip silently when absent.
// ---------------------------------------------------------------------------

fn c2pa_entries_by_tag<'a>(value: &'a Value) -> std::collections::BTreeMap<String, &'a Value> {
    let interpreted = value["interpreted"]["metadata"]
        .as_array()
        .expect("interpreted view present");
    interpreted
        .iter()
        .filter(|e| {
            let ns = e["namespace"].as_str().unwrap_or("");
            ns == "c2pa" || ns == "ai"
        })
        .map(|e| (e["tag_name"].as_str().unwrap().to_string(), e))
        .collect()
}

#[test]
fn c2pa_synthetic_png_minimal_fixture() {
    let value = extract_json("c2pa_synthetic.png", ViewMode::Interpreted);
    let by_tag = c2pa_entries_by_tag(&value);
    assert_eq!(
        by_tag
            .get("c2pa.claim_generator")
            .and_then(|e| e["value"]["value"].as_str()),
        Some("XIFty Test Suite/0.1.0"),
        "claim_generator missing or wrong; got entries {:?}",
        by_tag.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        by_tag
            .get("c2pa.format")
            .and_then(|e| e["value"]["value"].as_str()),
        Some("image/png")
    );
    assert_eq!(
        by_tag
            .get("c2pa.signature.alg")
            .and_then(|e| e["value"]["value"].as_str()),
        Some("eddsa")
    );
    assert_eq!(
        by_tag
            .get("c2pa.signature.verified")
            .and_then(|e| e["value"]["value"].as_str()),
        Some("unknown")
    );
    assert_eq!(
        by_tag
            .get("c2pa.ai_generated")
            .and_then(|e| e["value"]["value"].as_str()),
        Some("true")
    );
    assert_eq!(
        by_tag
            .get("ai.source_type")
            .and_then(|e| e["value"]["value"].as_str()),
        Some("trainedAlgorithmicMedia")
    );
    assert_eq!(
        by_tag
            .get("c2pa.assertions.0.action")
            .and_then(|e| e["value"]["value"].as_str()),
        Some("c2pa.created")
    );
}

#[test]
fn c2pa_copilot_png_real_fixture() {
    let Some(path) = optional_fixture("Copilot_20260407_185229.png") else {
        skip_missing_local_fixture("Copilot_20260407_185229.png");
        return;
    };
    let mut value =
        serde_json::to_value(xifty_cli::extract_path(path, ViewMode::Interpreted).unwrap())
            .unwrap();
    scrub_path(&mut value);
    let by_tag = c2pa_entries_by_tag(&value);
    let claim_gen = by_tag
        .get("c2pa.claim_generator")
        .and_then(|e| e["value"]["value"].as_str())
        .unwrap_or("");
    assert!(
        claim_gen.to_lowercase().contains("microsoft")
            || claim_gen.to_lowercase().contains("copilot")
            || claim_gen.to_lowercase().contains("designer"),
        "expected Microsoft/Copilot generator; got {claim_gen:?}"
    );
    let issuer = by_tag
        .get("c2pa.signature.issuer")
        .and_then(|e| e["value"]["value"].as_str())
        .unwrap_or("");
    assert!(
        issuer.to_lowercase().contains("microsoft") || issuer == "unknown",
        "expected Microsoft signer issuer or unknown; got {issuer:?}"
    );
    assert_eq!(
        by_tag
            .get("c2pa.ai_generated")
            .and_then(|e| e["value"]["value"].as_str()),
        Some("true")
    );
}

#[test]
fn c2pa_notebooklm_png_real_fixture() {
    let Some(path) = optional_fixture("unnamed-6.png") else {
        skip_missing_local_fixture("unnamed-6.png");
        return;
    };
    let mut value =
        serde_json::to_value(xifty_cli::extract_path(path, ViewMode::Interpreted).unwrap())
            .unwrap();
    scrub_path(&mut value);
    let by_tag = c2pa_entries_by_tag(&value);
    let claim_gen = by_tag
        .get("c2pa.claim_generator")
        .and_then(|e| e["value"]["value"].as_str())
        .unwrap_or("");
    assert!(
        claim_gen.contains("Google C2PA"),
        "expected Google C2PA generator; got {claim_gen:?}"
    );
    let actions: Vec<&str> = by_tag
        .iter()
        .filter(|(k, _)| k.starts_with("c2pa.assertions.") && k.ends_with(".action"))
        .filter_map(|(_, v)| v["value"]["value"].as_str())
        .collect();
    assert!(actions.contains(&"c2pa.created"), "actions={actions:?}");
    assert!(actions.contains(&"c2pa.edited"), "actions={actions:?}");
    assert_eq!(
        by_tag
            .get("c2pa.ai_generated")
            .and_then(|e| e["value"]["value"].as_str()),
        Some("true")
    );
    assert_eq!(
        by_tag
            .get("ai.source_type")
            .and_then(|e| e["value"]["value"].as_str()),
        Some("trainedAlgorithmicMedia")
    );
    assert_eq!(
        by_tag
            .get("ai.generator")
            .and_then(|e| e["value"]["value"].as_str()),
        Some("Google")
    );
    assert_eq!(
        by_tag
            .get("ai.synthid_disclosed")
            .and_then(|e| e["value"]["value"].as_str()),
        Some("true")
    );
}

// ---------------------------------------------------------------------------
// Classic .THM JFIF thumbnail sidecar — issue #135.
// Synthetic .MOV + sibling .THM with APP1 EXIF; mirrors the GoPro
// `gopro_sidecar_synthetic_minimal_fixture_lifts_proxy_and_thumbnail` shape.
// ---------------------------------------------------------------------------

fn build_minimal_qt_mov() -> Vec<u8> {
    // Smallest ftyp box the detect+ISOBMFF parser will accept as a Mov.
    // 4-byte size + "ftyp" + major_brand "qt  " + minor_version 0 + compat "qt  ".
    let mut payload: Vec<u8> = Vec::new();
    payload.extend_from_slice(b"qt  ");
    payload.extend_from_slice(&0u32.to_be_bytes());
    payload.extend_from_slice(b"qt  ");
    let size = (8 + payload.len()) as u32;
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(&size.to_be_bytes());
    out.extend_from_slice(b"ftyp");
    out.extend_from_slice(&payload);
    out
}

fn build_thm_with_make_model_datetime(make: &str, model: &str, datetime: &str) -> Vec<u8> {
    // Minimal little-endian TIFF carrying IFD0 with Make (0x010F),
    // Model (0x0110), and DateTimeOriginal (0x9003).
    //
    // Layout:
    //   0..2   "II"
    //   2..4   42 (LE)
    //   4..8   IFD0 offset = 8
    //   8..10  entry count = 3
    //   10..22 entry 0: Make
    //   22..34 entry 1: Model
    //   34..46 entry 2: DateTimeOriginal
    //   46..50 next IFD = 0
    //   50..   value data (NUL-terminated ASCII strings, 4-byte aligned not required)
    let make_bytes = {
        let mut v = make.as_bytes().to_vec();
        v.push(0);
        v
    };
    let model_bytes = {
        let mut v = model.as_bytes().to_vec();
        v.push(0);
        v
    };
    let dt_bytes = {
        let mut v = datetime.as_bytes().to_vec();
        v.push(0);
        v
    };

    let header_and_ifd_len = 8 + 2 + 12 * 3 + 4; // = 50
    let make_off = header_and_ifd_len as u32;
    let model_off = make_off + make_bytes.len() as u32;
    let dt_off = model_off + model_bytes.len() as u32;

    let mut tiff: Vec<u8> = Vec::new();
    tiff.extend_from_slice(b"II");
    tiff.extend_from_slice(&42u16.to_le_bytes());
    tiff.extend_from_slice(&8u32.to_le_bytes());
    tiff.extend_from_slice(&3u16.to_le_bytes()); // entry count

    fn ascii_entry(tag: u16, count: usize, value_or_offset: u32) -> [u8; 12] {
        let mut e = [0u8; 12];
        e[0..2].copy_from_slice(&tag.to_le_bytes());
        e[2..4].copy_from_slice(&2u16.to_le_bytes()); // type ASCII
        e[4..8].copy_from_slice(&(count as u32).to_le_bytes());
        e[8..12].copy_from_slice(&value_or_offset.to_le_bytes());
        e
    }

    tiff.extend_from_slice(&ascii_entry(0x010F, make_bytes.len(), make_off));
    tiff.extend_from_slice(&ascii_entry(0x0110, model_bytes.len(), model_off));
    tiff.extend_from_slice(&ascii_entry(0x9003, dt_bytes.len(), dt_off));
    tiff.extend_from_slice(&0u32.to_le_bytes()); // next IFD = 0
    tiff.extend_from_slice(&make_bytes);
    tiff.extend_from_slice(&model_bytes);
    tiff.extend_from_slice(&dt_bytes);

    // APP1 segment payload = "Exif\0\0" + tiff.
    let mut app1_payload: Vec<u8> = Vec::new();
    app1_payload.extend_from_slice(b"Exif\0\0");
    app1_payload.extend_from_slice(&tiff);
    let seg_len = (app1_payload.len() + 2) as u16;

    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(&[0xFF, 0xD8]); // SOI
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&seg_len.to_be_bytes());
    out.extend_from_slice(&app1_payload);
    // SOF0 320x180
    out.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x0B]);
    out.extend_from_slice(&[0x08, 0x00, 0xB4, 0x01, 0x40, 0x01, 0x01, 0x11, 0x00]);
    out.extend_from_slice(&[0xFF, 0xD9]); // EOI
    out
}

#[test]
fn classic_thm_sidecar_projects_app1_exif_into_flat_fields() {
    use std::time::{SystemTime, UNIX_EPOCH};
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("xifty-classic-thm-it-{stamp}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let mov = dir.join("CLIP0001.MOV");
    let thm = dir.join("CLIP0001.THM");
    fs::write(&mov, build_minimal_qt_mov()).unwrap();
    fs::write(
        &thm,
        build_thm_with_make_model_datetime("Canon", "Canon EOS-1D Mark IV", "2009:11:01 12:34:56"),
    )
    .unwrap();

    let output = extract_with_sidecars(mov.clone(), ViewMode::Interpreted);
    let interpreted = output["interpreted"]["metadata"]
        .as_array()
        .expect("interpreted view present");
    let by_tag: std::collections::BTreeMap<String, &Value> = interpreted
        .iter()
        .filter(|e| e["namespace"] == "classic_thm")
        .map(|e| (e["tag_name"].as_str().unwrap().to_string(), e))
        .collect();

    assert!(
        by_tag.contains_key("thumbnail.path"),
        "expected thumbnail.path; got {:?}",
        by_tag.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        by_tag
            .get("thumbnail.format")
            .and_then(|e| e["value"]["value"].as_str()),
        Some("jfif")
    );
    assert_eq!(
        by_tag
            .get("thumbnail.dimensions.width")
            .and_then(|e| e["value"]["value"].as_i64()),
        Some(320)
    );
    assert_eq!(
        by_tag
            .get("thumbnail.dimensions.height")
            .and_then(|e| e["value"]["value"].as_i64()),
        Some(180)
    );
    assert_eq!(
        by_tag
            .get("thumbnail.exif.make")
            .and_then(|e| e["value"]["value"].as_str()),
        Some("Canon")
    );
    assert_eq!(
        by_tag
            .get("thumbnail.exif.model")
            .and_then(|e| e["value"]["value"].as_str()),
        Some("Canon EOS-1D Mark IV")
    );
    assert_eq!(
        by_tag
            .get("thumbnail.exif.captured_at")
            .and_then(|e| e["value"]["value"].as_str()),
        Some("2009:11:01 12:34:56")
    );
    // Raw payload entry must NEVER reach the final output.
    assert!(
        !by_tag.contains_key("thumbnail.exif_payload"),
        "raw thumbnail.exif_payload bytes leaked into interpreted view"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn classic_thm_sidecar_skips_gopro_gh_stems() {
    use std::time::{SystemTime, UNIX_EPOCH};
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("xifty-classic-thm-skip-{stamp}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    // GH-stem: classic_thm must NOT claim it (GoPro adapter handles GoPro
    // THMs). The MOV is detected as Mov but the classic_thm namespace must
    // have zero `thumbnail.exif.*` projection here.
    let mov = dir.join("GH010024.MOV");
    let thm = dir.join("GH010024.THM");
    fs::write(&mov, build_minimal_qt_mov()).unwrap();
    fs::write(
        &thm,
        build_thm_with_make_model_datetime("GoPro", "HERO12", "2024:01:01 00:00:00"),
    )
    .unwrap();

    let output = extract_with_sidecars(mov.clone(), ViewMode::Interpreted);
    let interpreted = output["interpreted"]["metadata"]
        .as_array()
        .expect("interpreted view present");
    let classic: Vec<&Value> = interpreted
        .iter()
        .filter(|e| e["namespace"] == "classic_thm")
        .collect();
    assert!(
        classic.is_empty(),
        "classic_thm must not claim GH-prefix stems; got {classic:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}
