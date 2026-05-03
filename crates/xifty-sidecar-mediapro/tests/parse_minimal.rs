//! Handcrafted minimal MEDIAPRO.XML fixtures covering every emitted field
//! group. These do NOT depend on `fixtures/local/` and run in CI.

use std::path::PathBuf;

use xifty_core::{Severity, TypedValue};
use xifty_sidecar::{
    SIDECAR_NO_INDEX_ENTRY, SIDECAR_PARSE_ERROR, SIDECAR_TARGET_MISSING,
    SIDECAR_UNKNOWN_SCHEMA_VERSION, SidecarRef,
};
use xifty_sidecar_mediapro::parse_mediapro;

fn fake_sidecar() -> SidecarRef {
    SidecarRef {
        path: PathBuf::from("/tmp/mediapro-test/MEDIAPRO.XML"),
        label: "MEDIAPRO|C0001.MP4".into(),
    }
}

fn tag<'a>(
    payload: &'a xifty_sidecar::SidecarPayload,
    tag_name: &str,
) -> Option<&'a xifty_core::MetadataEntry> {
    payload.entries.iter().find(|e| e.tag_name == tag_name)
}

fn has_tag(payload: &xifty_sidecar::SidecarPayload, tag_name: &str) -> bool {
    payload.entries.iter().any(|e| e.tag_name == tag_name)
}

const HEADER_AND_MATCHED_MIN: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<MediaProfile xmlns="http://xmlns.sony.net/pro/metadata/mediaprofile" version="2.10" createdAt="2026-01-01T00:00:00+00:00">
  <Properties>
    <System systemId="ABC123" systemKind="ZV-E10" masterVersion="XAVC-M4@1.10.00"/>
    <Attached mediaId="7C771722600806D7CFEFDCFE23FFFE19" mediaKind="memory_stick" mediaName=""/>
  </Properties>
  <Contents>
    <Material uri="./CLIP/C0001.MP4" umid="060A2B340101010501010D43130000000000000000000000000000000000DEAD"
              videoType="AVC_3840_2160_HP@L51" audioType="LPCM16" fps="29.97p"
              dur="900" ch="2" aspectRatio="16:9">
      <RelevantInfo type="JPG" uri="./CLIP/C0001M01.JPG"/>
    </Material>
    <Material uri="./CLIP/C0002.MP4" umid="ZZZ"/>
  </Contents>
</MediaProfile>
"#;

#[test]
fn parses_header_and_matched_material_min() {
    let payload = parse_mediapro(&fake_sidecar(), HEADER_AND_MATCHED_MIN.as_bytes());

    // No warnings/errors expected on the clean fixture.
    assert_eq!(
        payload
            .issues
            .iter()
            .filter(|i| matches!(i.severity, Severity::Warning | Severity::Error))
            .count(),
        0,
        "no warnings/errors on clean fixture, got {:?}",
        payload.issues
    );

    // Header fields — version and master_version are independent.
    assert_eq!(
        tag(&payload, "media_profile.version").unwrap().value,
        TypedValue::String("2.10".into())
    );
    assert_eq!(
        tag(&payload, "media_profile.master_version").unwrap().value,
        TypedValue::String("XAVC-M4@1.10.00".into())
    );
    assert_eq!(
        tag(&payload, "media_profile.system_id").unwrap().value,
        TypedValue::String("ABC123".into())
    );
    assert_eq!(
        tag(&payload, "media_profile.system_kind").unwrap().value,
        TypedValue::String("ZV-E10".into())
    );
    assert_eq!(
        tag(&payload, "media_profile.media_id").unwrap().value,
        TypedValue::String("7C771722600806D7CFEFDCFE23FFFE19".into())
    );
    assert_eq!(
        tag(&payload, "media_profile.media_kind").unwrap().value,
        TypedValue::String("memory_stick".into())
    );
    // Empty mediaName is suppressed.
    assert!(!has_tag(&payload, "media_profile.media_name"));
    // clip_count is i64.
    assert_eq!(
        tag(&payload, "media_profile.clip_count").unwrap().value,
        TypedValue::Integer(2)
    );
    // createdAt surfaces.
    assert_eq!(
        tag(&payload, "media_profile.created_at").unwrap().value,
        TypedValue::String("2026-01-01T00:00:00+00:00".into())
    );

    // Per-clip matched fields.
    assert!(matches!(
        &tag(&payload, "mediapro.umid").unwrap().value,
        TypedValue::String(s) if s.ends_with("DEAD")
    ));
    assert_eq!(
        tag(&payload, "mediapro.video_type").unwrap().value,
        TypedValue::String("AVC_3840_2160_HP@L51".into())
    );
    assert_eq!(
        tag(&payload, "mediapro.audio_type").unwrap().value,
        TypedValue::String("LPCM16".into())
    );
    assert_eq!(
        tag(&payload, "mediapro.fps").unwrap().value,
        TypedValue::String("29.97p".into())
    );
    assert_eq!(
        tag(&payload, "mediapro.duration_frames").unwrap().value,
        TypedValue::Integer(900)
    );
    assert_eq!(
        tag(&payload, "mediapro.channels").unwrap().value,
        TypedValue::Integer(2)
    );
    assert_eq!(
        tag(&payload, "mediapro.aspect_ratio").unwrap().value,
        TypedValue::String("16:9".into())
    );

    // Provenance correctness.
    let entry = tag(&payload, "mediapro.umid").unwrap();
    assert_eq!(entry.namespace, "mediapro");
    assert_eq!(entry.provenance.namespace, "mediapro");
    assert_eq!(entry.provenance.container, "sidecar");

    // Thumbnail path resolves against sidecar dir, but missing on disk
    // should produce a target_missing info issue.
    let thumb = tag(&payload, "mediapro.thumbnail.path").unwrap();
    let TypedValue::String(thumb_path) = &thumb.value else {
        panic!("thumbnail path must be String");
    };
    assert!(
        thumb_path.ends_with("CLIP/C0001M01.JPG") || thumb_path.ends_with("CLIP\\C0001M01.JPG")
    );
    assert!(
        payload
            .issues
            .iter()
            .any(|i| i.code == SIDECAR_TARGET_MISSING && i.severity == Severity::Info),
        "expected sidecar_target_missing info issue, got {:?}",
        payload.issues
    );
}

#[test]
fn primary_not_listed_emits_no_index_entry() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<MediaProfile xmlns="http://xmlns.sony.net/pro/metadata/mediaprofile" version="2.10">
  <Properties>
    <System systemId="X" systemKind="Y" masterVersion="Z"/>
    <Attached mediaId="M" mediaKind="K"/>
  </Properties>
  <Contents>
    <Material uri="./CLIP/COTHER.MP4" umid="ZZZ"/>
  </Contents>
</MediaProfile>
"#;
    let payload = parse_mediapro(&fake_sidecar(), xml.as_bytes());
    let info: Vec<_> = payload
        .issues
        .iter()
        .filter(|i| i.code == SIDECAR_NO_INDEX_ENTRY)
        .collect();
    assert_eq!(info.len(), 1);
    assert_eq!(info[0].severity, Severity::Info);
    // Header still flows through.
    assert!(has_tag(&payload, "media_profile.system_kind"));
    // Per-clip mediapro.* entries do NOT.
    assert!(!has_tag(&payload, "mediapro.umid"));
}

#[test]
fn malformed_label_emits_no_index_entry_and_no_panic() {
    let bad = SidecarRef {
        path: PathBuf::from("/tmp/MEDIAPRO.XML"),
        label: "MEDIAPRO".into(), // no `|`
    };
    let payload = parse_mediapro(&bad, HEADER_AND_MATCHED_MIN.as_bytes());
    assert_eq!(payload.entries.len(), 0);
    assert_eq!(payload.issues.len(), 1);
    assert_eq!(payload.issues[0].code, SIDECAR_NO_INDEX_ENTRY);
}

#[test]
fn unknown_xmlns_emits_info_and_continues_parsing() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<MediaProfile xmlns="http://example.com/not-sony" version="9.99">
  <Properties>
    <System systemId="X" systemKind="Y" masterVersion="Z"/>
  </Properties>
  <Contents>
    <Material uri="./CLIP/C0001.MP4" umid="UMID0001"/>
  </Contents>
</MediaProfile>
"#;
    let payload = parse_mediapro(&fake_sidecar(), xml.as_bytes());
    assert!(
        payload
            .issues
            .iter()
            .any(|i| i.code == SIDECAR_UNKNOWN_SCHEMA_VERSION && i.severity == Severity::Info)
    );
    assert_eq!(
        tag(&payload, "mediapro.umid").unwrap().value,
        TypedValue::String("UMID0001".into())
    );
}

#[test]
fn malformed_xml_emits_parse_error_warning_no_panic() {
    let bad = b"<MediaProfile><Contents><Material uri=\"./CLIP/C0001.MP4\" umid=\"X\"";
    let payload = parse_mediapro(&fake_sidecar(), bad);
    assert!(
        payload
            .issues
            .iter()
            .any(|i| i.code == SIDECAR_PARSE_ERROR && i.severity == Severity::Warning),
        "expected sidecar_parse_error warning, got {:?}",
        payload.issues
    );
}

#[test]
fn recording_session_surfaces_with_starttime_endtime_attrs() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<MediaProfile xmlns="http://xmlns.sony.net/pro/metadata/mediaprofile" version="2.20">
  <Properties>
    <System systemId="X" systemKind="FX9" masterVersion="V"/>
    <Attached mediaId="M" mediaKind="K"/>
    <RecordingSession startTime="2026-04-15T10:00:00-08:00" endTime="2026-04-15T18:30:00-08:00"/>
  </Properties>
  <Contents>
    <Material uri="./CLIP/C0001.MP4" umid="U"/>
  </Contents>
</MediaProfile>
"#;
    let payload = parse_mediapro(&fake_sidecar(), xml.as_bytes());
    assert_eq!(
        tag(&payload, "media_profile.recording_session.start")
            .unwrap()
            .value,
        TypedValue::String("2026-04-15T10:00:00-08:00".into())
    );
    assert_eq!(
        tag(&payload, "media_profile.recording_session.end")
            .unwrap()
            .value,
        TypedValue::String("2026-04-15T18:30:00-08:00".into())
    );
}

#[test]
fn recording_session_accepts_start_end_attribute_fallback() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<MediaProfile xmlns="http://xmlns.sony.net/pro/metadata/mediaprofile" version="2.20">
  <Properties>
    <RecordingSession start="2026-05-01T00:00:00Z" end="2026-05-01T01:00:00Z"/>
  </Properties>
  <Contents>
    <Material uri="./CLIP/C0001.MP4"/>
  </Contents>
</MediaProfile>
"#;
    let payload = parse_mediapro(&fake_sidecar(), xml.as_bytes());
    assert_eq!(
        tag(&payload, "media_profile.recording_session.start")
            .unwrap()
            .value,
        TypedValue::String("2026-05-01T00:00:00Z".into())
    );
    assert_eq!(
        tag(&payload, "media_profile.recording_session.end")
            .unwrap()
            .value,
        TypedValue::String("2026-05-01T01:00:00Z".into())
    );
}

#[test]
fn cross_refs_flat_array_two_references() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<MediaProfile xmlns="http://xmlns.sony.net/pro/metadata/mediaprofile" version="2.20">
  <Contents>
    <Material uri="./CLIP/C0001.MP4" umid="U">
      <Reference type="thumbnail" umidRef="UMID-THUMB-AAAA"/>
      <Reference type="previousTake" umidRef="UMID-PREV-BBBB"/>
    </Material>
  </Contents>
</MediaProfile>
"#;
    let payload = parse_mediapro(&fake_sidecar(), xml.as_bytes());
    assert_eq!(
        tag(&payload, "media_profile.cross_refs.0.kind")
            .unwrap()
            .value,
        TypedValue::String("thumbnail".into())
    );
    assert_eq!(
        tag(&payload, "media_profile.cross_refs.0.umid")
            .unwrap()
            .value,
        TypedValue::String("UMID-THUMB-AAAA".into())
    );
    assert_eq!(
        tag(&payload, "media_profile.cross_refs.1.kind")
            .unwrap()
            .value,
        TypedValue::String("previousTake".into())
    );
    assert_eq!(
        tag(&payload, "media_profile.cross_refs.1.umid")
            .unwrap()
            .value,
        TypedValue::String("UMID-PREV-BBBB".into())
    );
}

#[test]
fn cross_refs_accepts_kind_umid_attribute_fallback() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<MediaProfile xmlns="http://xmlns.sony.net/pro/metadata/mediaprofile" version="2.20">
  <Contents>
    <Material uri="./CLIP/C0001.MP4">
      <Reference kind="continuation" umid="UMID-CONT-1"/>
    </Material>
  </Contents>
</MediaProfile>
"#;
    let payload = parse_mediapro(&fake_sidecar(), xml.as_bytes());
    assert_eq!(
        tag(&payload, "media_profile.cross_refs.0.kind")
            .unwrap()
            .value,
        TypedValue::String("continuation".into())
    );
    assert_eq!(
        tag(&payload, "media_profile.cross_refs.0.umid")
            .unwrap()
            .value,
        TypedValue::String("UMID-CONT-1".into())
    );
}
