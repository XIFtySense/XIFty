//! Optional integration test against the user's local fixture pair.
//!
//! Skips silently when `fixtures/local/sony-mediapro-zv-e10.xml` is not
//! present (CI does not check in `local/` files; see `.loswf/config.yaml`
//! exclude list and `fixtures/README.md`).

use std::path::{Path, PathBuf};

use xifty_core::TypedValue;
use xifty_sidecar::SidecarRef;
use xifty_sidecar_mediapro::parse_mediapro;

fn workspace_fixture(name: &str) -> PathBuf {
    // CARGO_MANIFEST_DIR points at this crate's dir; walk up two levels to
    // the workspace root.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(|p| p.join("fixtures").join("local").join(name))
        .unwrap_or_else(|| PathBuf::from(format!("fixtures/local/{name}")))
}

fn tag<'a>(
    payload: &'a xifty_sidecar::SidecarPayload,
    tag_name: &str,
) -> Option<&'a xifty_core::MetadataEntry> {
    payload.entries.iter().find(|e| e.tag_name == tag_name)
}

#[test]
fn parses_zv_e10_consumer_cam_index() {
    let path = workspace_fixture("sony-mediapro-zv-e10.xml");
    if !path.exists() {
        eprintln!("skipping: optional fixture missing at {}", path.display());
        return;
    }
    let bytes = std::fs::read(&path).expect("read fixture");
    let sidecar = SidecarRef {
        path: path.clone(),
        label: "MEDIAPRO|C0242.MP4".into(),
    };
    let payload = parse_mediapro(&sidecar, &bytes);

    // Header asserts.
    assert_eq!(
        tag(&payload, "media_profile.version").unwrap().value,
        TypedValue::String("2.10".into())
    );
    assert_eq!(
        tag(&payload, "media_profile.master_version").unwrap().value,
        TypedValue::String("XAVC-M4@1.10.00".into())
    );
    assert_eq!(
        tag(&payload, "media_profile.system_kind").unwrap().value,
        TypedValue::String("ZV-E10".into())
    );
    assert_eq!(
        tag(&payload, "media_profile.media_id").unwrap().value,
        TypedValue::String("7C771722600806D7CFEFDCFE23FFFE19".into())
    );
    let clip_count = tag(&payload, "media_profile.clip_count")
        .expect("clip_count entry")
        .value
        .clone();
    match clip_count {
        TypedValue::Integer(n) => assert!(n > 100, "expected >100 clips, got {n}"),
        other => panic!("clip_count must be Integer, got {other:?}"),
    }

    // Per-clip C0242 asserts.
    let umid = tag(&payload, "mediapro.umid").expect("matched mediapro.umid");
    if let TypedValue::String(s) = &umid.value {
        assert!(
            s.ends_with("461106C2DCFE23FFFE19CFEF"),
            "UMID tail mismatch: {s}"
        );
    } else {
        panic!("umid must be String");
    }
    assert_eq!(
        tag(&payload, "mediapro.video_type").unwrap().value,
        TypedValue::String("AVC_3840_2160_HP@L51".into())
    );
    assert_eq!(
        tag(&payload, "mediapro.fps").unwrap().value,
        TypedValue::String("23.98p".into())
    );
    assert_eq!(
        tag(&payload, "mediapro.duration_frames").unwrap().value,
        TypedValue::Integer(48)
    );

    // Negative assertions documenting the consumer-cam shape — the ZV-E10
    // fixture has no <RecordingSession> and no <Reference> children.
    assert!(
        !payload
            .entries
            .iter()
            .any(|e| e.tag_name.starts_with("media_profile.recording_session.")),
        "ZV-E10 consumer-cam fixture must not carry recording_session.* entries"
    );
    assert!(
        !payload
            .entries
            .iter()
            .any(|e| e.tag_name.starts_with("media_profile.cross_refs.")),
        "ZV-E10 consumer-cam fixture must not carry cross_refs.* entries"
    );
}
