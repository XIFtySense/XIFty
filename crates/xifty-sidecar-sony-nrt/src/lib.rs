//! Sony Non-Real-Time (NRT) XML sidecar adapter.
//!
//! Camera output for Sony XAVC ships a `<basename>M01.XML` sibling beside
//! every `.MP4` clip. The XML carries metadata the MP4 container does not —
//! UMID, capture/format FPS pair, color gamma equation, LTC change table,
//! recording mode, device serial. This adapter parses that XML against the
//! public Sony NRT XSDs at:
//!
//! * `urn:schemas-professionalDisc:nonRealTimeMeta:ver.2.10`
//! * `urn:schemas-professionalDisc:nonRealTimeMeta:ver.2.20`
//!
//! Per-tag citation comments reference the XSD section they were derived
//! from. ExifTool's `Sony.pm` / `XML.pm` were consulted as cross-checks but
//! no third-party code ships in this crate — every byte is native Rust.

use std::path::{Path, PathBuf};

use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use xifty_core::{Issue, MetadataEntry, Provenance, Severity, TypedValue};
use xifty_sidecar::{
    MergeContext, MergePolicy, SIDECAR_PARSE_ERROR, SIDECAR_UNKNOWN_SCHEMA_VERSION, Sidecar,
    SidecarPayload, SidecarRef,
};

const NAMESPACE: &str = "sony_nrt";
const CONTAINER: &str = "sidecar";

const XMLNS_V210: &str = "urn:schemas-professionalDisc:nonRealTimeMeta:ver.2.10";
const XMLNS_V220: &str = "urn:schemas-professionalDisc:nonRealTimeMeta:ver.2.20";

/// Sony NRT XML adapter — registered automatically by
/// [`xifty_sidecar::SidecarRegistry`] when the CLI/FFI sidecar feature is
/// enabled.
#[derive(Debug, Default)]
pub struct SonyNrtSidecar;

impl SonyNrtSidecar {
    pub const fn new() -> Self {
        Self
    }
}

impl Sidecar for SonyNrtSidecar {
    fn name(&self) -> &'static str {
        NAMESPACE
    }

    fn discover(&self, primary_path: Option<&Path>) -> Vec<SidecarRef> {
        let Some(primary) = primary_path else {
            return Vec::new();
        };
        discover_in_dir(primary)
    }

    fn parse(&self, sidecar: &SidecarRef, bytes: &[u8], _ctx: &MergeContext) -> SidecarPayload {
        parse_nrt(sidecar, bytes)
    }

    fn priority(&self) -> u8 {
        100
    }

    fn merge_policy(&self) -> MergePolicy {
        // NRT carries strictly *additional* fields the MP4 lacks (UMID,
        // capture/format FPS pair, gamma equation, etc.). For tags that *do*
        // overlap (CreateDate, codec) the existing conflict-detector flags
        // disagreement — we want both surfaced, not one silently winning.
        MergePolicy::Complement
    }
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// Sony XAVC convention: clip `C0042.MP4` is paired with `C0042M01.XML` (and,
/// rarely, `C0042M02.XML` as a fallback). We walk the parent directory once
/// and accept any file whose stem matches `<primary stem>M\d+\.XML` (case
/// insensitive). Returned in *descending* numeric order so highest-numbered
/// (= most recent take) wins via registry iteration order.
fn discover_in_dir(primary: &Path) -> Vec<SidecarRef> {
    let Some(dir) = primary.parent() else {
        return Vec::new();
    };
    let Some(stem) = primary.file_stem().and_then(|s| s.to_str()) else {
        return Vec::new();
    };
    let read = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    };
    let mut hits: Vec<(u32, PathBuf)> = Vec::new();
    for entry in read.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if let Some(seq) = nrt_sequence(name, stem) {
            hits.push((seq, path));
        }
    }
    hits.sort_by(|a, b| b.0.cmp(&a.0));
    hits.into_iter()
        .map(|(seq, path)| SidecarRef {
            path,
            label: format!("M{seq:02}"),
        })
        .collect()
}

/// Returns the `M<NN>` sequence number when `name` matches `<stem>M<NN>.XML`
/// (case insensitive on extension and the `M` separator).
fn nrt_sequence(name: &str, stem: &str) -> Option<u32> {
    // Case-insensitive prefix match on the stem.
    if name.len() <= stem.len() {
        return None;
    }
    if !name
        .get(..stem.len())
        .map(|s| s.eq_ignore_ascii_case(stem))
        .unwrap_or(false)
    {
        return None;
    }
    let rest = &name[stem.len()..];
    let bytes = rest.as_bytes();
    if bytes.first().map(|c| c.eq_ignore_ascii_case(&b'M')) != Some(true) {
        return None;
    }
    // Strip `.XML` (case-insensitive).
    let dot = rest.rfind('.')?;
    let ext = &rest[dot + 1..];
    if !ext.eq_ignore_ascii_case("XML") {
        return None;
    }
    let digits = &rest[1..dot];
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    digits.parse::<u32>().ok()
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SchemaVersion {
    V210,
    V220,
    Unknown,
}

fn parse_nrt(sidecar: &SidecarRef, bytes: &[u8]) -> SidecarPayload {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);

    let mut entries: Vec<MetadataEntry> = Vec::new();
    let mut issues: Vec<Issue> = Vec::new();
    let mut buf = Vec::new();
    let mut in_acquisition_camera_unit = false;
    // Track first vs last LtcChange (per Sony NRT XSD §LtcChangeTable
    // schema, the table sorts by `frameCount`; we capture the first/last
    // we encounter rather than re-sorting).
    let mut first_ltc_change: Option<String> = None;
    let mut last_ltc_change: Option<String> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let local = local_name(e);

                // Root element — capture the schema version from xmlns.
                // per Sony NRT XSD ver.2.20 root <NonRealTimeMeta>.
                if local == "NonRealTimeMeta" {
                    let schema = detect_schema(e);
                    if schema == SchemaVersion::Unknown {
                        let xmlns = attr_value(e, b"xmlns").unwrap_or_default();
                        issues.push(Issue {
                            severity: Severity::Info,
                            code: SIDECAR_UNKNOWN_SCHEMA_VERSION.into(),
                            message: format!(
                                "Sony NRT sidecar uses unrecognized schema xmlns: {}",
                                if xmlns.is_empty() {
                                    "<missing>".into()
                                } else {
                                    xmlns
                                }
                            ),
                            offset: None,
                            context: Some(NAMESPACE.into()),
                        });
                    }
                }

                if local == "AcquisitionRecord" {
                    in_acquisition_camera_unit = false;
                }
                if local == "Group" {
                    in_acquisition_camera_unit = matches!(
                        attr_value(e, b"name").as_deref(),
                        Some("CameraUnitMetadataSet")
                    );
                }

                match local.as_str() {
                    // Device@manufacturer/modelName/serialNo
                    // per Sony NRT XSD ver.2.20 §Device — the MP4 already
                    // carries make/model in the `quicktime` namespace; only
                    // serial is sidecar-only.
                    // (cross-check ExifTool Sony.pm `SerialNumber`).
                    "Device" => {
                        if let Some(serial) = attr_value(e, b"serialNo") {
                            push_string(
                                &mut entries,
                                sidecar,
                                "Device.serialNo",
                                "Device.serialNo",
                                serial,
                            );
                        }
                        if let Some(make) = attr_value(e, b"manufacturer") {
                            push_string(
                                &mut entries,
                                sidecar,
                                "Device.manufacturer",
                                "Device.manufacturer",
                                make,
                            );
                        }
                        if let Some(model) = attr_value(e, b"modelName") {
                            push_string(
                                &mut entries,
                                sidecar,
                                "Device.modelName",
                                "Device.modelName",
                                model,
                            );
                        }
                    }

                    // CreationDate@value — TZ-aware ISO-8601-ish string.
                    // per Sony NRT XSD ver.2.20 §CreationDate.
                    // (cross-check ExifTool XML.pm CreationDate).
                    "CreationDate" => {
                        if let Some(value) = attr_value(e, b"value") {
                            entries.push(timestamp_entry(
                                sidecar,
                                "CreationDate",
                                "CreationDate",
                                value,
                            ));
                        }
                    }

                    // RecordingMode@type / @cacheRec
                    // per Sony NRT XSD ver.2.20 §RecordingMode.
                    "RecordingMode" => {
                        if let Some(value) = attr_value(e, b"type") {
                            push_string(
                                &mut entries,
                                sidecar,
                                "RecordingMode.type",
                                "RecordingMode.type",
                                value,
                            );
                        }
                        if let Some(value) = attr_value(e, b"cacheRec") {
                            push_string(
                                &mut entries,
                                sidecar,
                                "RecordingMode.cacheRec",
                                "RecordingMode.cacheRec",
                                value,
                            );
                        }
                    }

                    // VideoFrame@captureFps + @formatFps + @videoCodec
                    // per Sony NRT XSD ver.2.20 §VideoFormat/VideoFrame.
                    "VideoFrame" => {
                        if let Some(value) = attr_value(e, b"captureFps") {
                            push_string(
                                &mut entries,
                                sidecar,
                                "VideoFrame.captureFps",
                                "VideoFrame.captureFps",
                                value,
                            );
                        }
                        if let Some(value) = attr_value(e, b"formatFps") {
                            push_string(
                                &mut entries,
                                sidecar,
                                "VideoFrame.formatFps",
                                "VideoFrame.formatFps",
                                value,
                            );
                        }
                        if let Some(value) = attr_value(e, b"videoCodec") {
                            push_string(
                                &mut entries,
                                sidecar,
                                "VideoFrame.videoCodec",
                                "VideoFrame.videoCodec",
                                value,
                            );
                        }
                    }

                    // VideoLayout@pixel/numOfVerticalLine/aspectRatio
                    // per Sony NRT XSD ver.2.20 §VideoFormat/VideoLayout —
                    // structural cross-check vs the MP4 `tkhd`/`stsd`.
                    "VideoLayout" => {
                        if let Some(value) = attr_value(e, b"pixel") {
                            push_integer_or_string(
                                &mut entries,
                                sidecar,
                                "VideoLayout.pixel",
                                "VideoLayout.pixel",
                                value,
                            );
                        }
                        if let Some(value) = attr_value(e, b"numOfVerticalLine") {
                            push_integer_or_string(
                                &mut entries,
                                sidecar,
                                "VideoLayout.numOfVerticalLine",
                                "VideoLayout.numOfVerticalLine",
                                value,
                            );
                        }
                        if let Some(value) = attr_value(e, b"aspectRatio") {
                            push_string(
                                &mut entries,
                                sidecar,
                                "VideoLayout.aspectRatio",
                                "VideoLayout.aspectRatio",
                                value,
                            );
                        }
                    }

                    // TargetMaterial@umidRef
                    // per Sony NRT XSD ver.2.20 §TargetMaterial — UMID is a
                    // 32-byte SMPTE 330M unique material identifier (hex).
                    // (cross-check ExifTool Sony.pm umidRef).
                    "TargetMaterial" => {
                        if let Some(value) = attr_value(e, b"umidRef") {
                            push_string(
                                &mut entries,
                                sidecar,
                                "TargetMaterial.umidRef",
                                "TargetMaterial.umidRef",
                                value,
                            );
                        }
                    }

                    // LtcChangeTable@tcFps + @halfStep
                    // per Sony NRT XSD ver.2.20 §LtcChangeTable.
                    "LtcChangeTable" => {
                        if let Some(value) = attr_value(e, b"tcFps") {
                            push_integer_or_string(
                                &mut entries,
                                sidecar,
                                "LtcChangeTable.tcFps",
                                "LtcChangeTable.tcFps",
                                value,
                            );
                        }
                        if let Some(value) = attr_value(e, b"halfStep") {
                            push_string(
                                &mut entries,
                                sidecar,
                                "LtcChangeTable.halfStep",
                                "LtcChangeTable.halfStep",
                                value,
                            );
                        }
                    }

                    // LtcChange@value (carry first + last as separate entries)
                    // per Sony NRT XSD ver.2.20 §LtcChange.
                    "LtcChange" => {
                        if let Some(value) = attr_value(e, b"value") {
                            if first_ltc_change.is_none() {
                                first_ltc_change = Some(value.clone());
                            }
                            last_ltc_change = Some(value);
                        }
                    }

                    // AudioFormat@numOfChannel
                    // per Sony NRT XSD ver.2.20 §AudioFormat.
                    "AudioFormat" => {
                        if let Some(value) = attr_value(e, b"numOfChannel") {
                            push_integer_or_string(
                                &mut entries,
                                sidecar,
                                "AudioFormat.numOfChannel",
                                "AudioFormat.numOfChannel",
                                value,
                            );
                        }
                    }

                    // AudioRecPort@audioCodec
                    // per Sony NRT XSD ver.2.20 §AudioFormat/AudioRecPort.
                    "AudioRecPort" => {
                        if let Some(value) = attr_value(e, b"audioCodec") {
                            push_string(
                                &mut entries,
                                sidecar,
                                "AudioRecPort.audioCodec",
                                "AudioRecPort.audioCodec",
                                value,
                            );
                        }
                    }

                    // Duration@value (frames)
                    // per Sony NRT XSD ver.2.20 §Duration — structural cross
                    // check vs MP4 mvhd duration.
                    "Duration" => {
                        if let Some(value) = attr_value(e, b"value") {
                            push_integer_or_string(
                                &mut entries,
                                sidecar,
                                "Duration.value",
                                "Duration.value",
                                value,
                            );
                        }
                    }

                    // AcquisitionRecord/Group[CameraUnitMetadataSet]/Item[*]
                    // per Sony NRT XSD ver.2.20 §AcquisitionRecord — the
                    // CameraUnitMetadataSet group carries CaptureGammaEquation,
                    // CaptureColorPrimaries, CodingEquations.
                    // (cross-check ExifTool Sony.pm CameraUnitMetadata).
                    "Item" if in_acquisition_camera_unit => {
                        let name = attr_value(e, b"name");
                        let value = attr_value(e, b"value");
                        if let (Some(name), Some(value)) = (name, value) {
                            match name.as_str() {
                                "CaptureGammaEquation"
                                | "CaptureColorPrimaries"
                                | "CodingEquations" => {
                                    push_string(&mut entries, sidecar, &name, &name, value);
                                }
                                _ => {}
                            }
                        }
                    }

                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let local = local_name(e.as_ref());
                if local == "Group" {
                    in_acquisition_camera_unit = false;
                }
            }
            Ok(_) => {}
            Err(error) => {
                issues.push(Issue {
                    severity: Severity::Warning,
                    code: SIDECAR_PARSE_ERROR.into(),
                    message: format!("Sony NRT XML parse error: {error}"),
                    offset: Some(reader.buffer_position()),
                    context: Some(NAMESPACE.into()),
                });
                break;
            }
        }
        buf.clear();
    }

    if let Some(value) = first_ltc_change {
        push_string(
            &mut entries,
            sidecar,
            "LtcChange.first",
            "LtcChange.first",
            value,
        );
    }
    if let Some(value) = last_ltc_change {
        push_string(
            &mut entries,
            sidecar,
            "LtcChange.last",
            "LtcChange.last",
            value,
        );
    }

    SidecarPayload { entries, issues }
}

fn detect_schema(e: &BytesStart<'_>) -> SchemaVersion {
    match attr_value(e, b"xmlns").as_deref() {
        Some(XMLNS_V210) => SchemaVersion::V210,
        Some(XMLNS_V220) => SchemaVersion::V220,
        _ => SchemaVersion::Unknown,
    }
}

fn local_name<'a>(e: impl Into<LocalNameSrc<'a>>) -> String {
    match e.into() {
        LocalNameSrc::Start(b) => bytes_to_local_name(b.name().as_ref()),
        LocalNameSrc::Bytes(bytes) => bytes_to_local_name(bytes),
    }
}

enum LocalNameSrc<'a> {
    Start(&'a BytesStart<'a>),
    Bytes(&'a [u8]),
}

impl<'a> From<&'a BytesStart<'a>> for LocalNameSrc<'a> {
    fn from(value: &'a BytesStart<'a>) -> Self {
        LocalNameSrc::Start(value)
    }
}

impl<'a> From<&'a [u8]> for LocalNameSrc<'a> {
    fn from(value: &'a [u8]) -> Self {
        LocalNameSrc::Bytes(value)
    }
}

fn bytes_to_local_name(name: &[u8]) -> String {
    let s = std::str::from_utf8(name).unwrap_or("");
    if let Some(idx) = s.rfind(':') {
        s[idx + 1..].to_string()
    } else {
        s.to_string()
    }
}

/// Read a Sony NRT attribute value as a UTF-8 string.
///
/// We deliberately do NOT call `attr.decode_and_unescape_value(...)` here.
/// Every Sony NRT attribute payload defined in XSD ver.2.10 / ver.2.20 is
/// constrained to ASCII-clean tokens (UMID hex, FPS strings, codec names,
/// SMPTE timecodes, serial numbers, enum-style values like `s-log3` /
/// `rec2020` / `normal`). None carry XML entity references in real-world
/// camera output, and the `xmlns` attribute we read at the root is a fixed
/// schema URN.
///
/// If a future Sony schema version (or a hand-edited sidecar) introduces
/// entity-encoded values such as `manufacturer="Foo &amp; Bar"`, this reader
/// would surface them with `&amp;` intact. Switching to
/// `decode_and_unescape_value(reader.decoder())` would fix that, but requires
/// threading the decoder through every call site. Defer until we see a real
/// fixture that needs it.
fn attr_value(e: &BytesStart<'_>, key: &[u8]) -> Option<String> {
    for attr in e.attributes().flatten() {
        // Match by local part — Sony NRT does not prefix its own attributes
        // but `xmlns` may carry no prefix either way.
        let attr_local = match attr.key.as_ref().rsplit(|b| *b == b':').next() {
            Some(local) => local,
            None => continue,
        };
        if attr_local == key || attr.key.as_ref() == key {
            return std::str::from_utf8(&attr.value).ok().map(|s| s.to_string());
        }
    }
    None
}

fn provenance(sidecar: &SidecarRef) -> Provenance {
    Provenance {
        container: CONTAINER.into(),
        namespace: NAMESPACE.into(),
        path: Some(sidecar.path.to_string_lossy().into_owned()),
        offset_start: None,
        offset_end: None,
        notes: vec![format!("sony nrt sidecar ({})", sidecar.label)],
    }
}

fn push_string(
    entries: &mut Vec<MetadataEntry>,
    sidecar: &SidecarRef,
    tag_id: &str,
    tag_name: &str,
    value: String,
) {
    entries.push(MetadataEntry {
        namespace: NAMESPACE.into(),
        tag_id: tag_id.into(),
        tag_name: tag_name.into(),
        value: TypedValue::String(value),
        provenance: provenance(sidecar),
        notes: Vec::new(),
    });
}

fn push_integer_or_string(
    entries: &mut Vec<MetadataEntry>,
    sidecar: &SidecarRef,
    tag_id: &str,
    tag_name: &str,
    value: String,
) {
    let typed = match value.parse::<i64>() {
        Ok(parsed) => TypedValue::Integer(parsed),
        Err(_) => TypedValue::String(value),
    };
    entries.push(MetadataEntry {
        namespace: NAMESPACE.into(),
        tag_id: tag_id.into(),
        tag_name: tag_name.into(),
        value: typed,
        provenance: provenance(sidecar),
        notes: Vec::new(),
    });
}

fn timestamp_entry(
    sidecar: &SidecarRef,
    tag_id: &str,
    tag_name: &str,
    value: String,
) -> MetadataEntry {
    MetadataEntry {
        namespace: NAMESPACE.into(),
        tag_id: tag_id.into(),
        tag_name: tag_name.into(),
        value: TypedValue::Timestamp(value),
        provenance: provenance(sidecar),
        notes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_sidecar() -> SidecarRef {
        SidecarRef {
            path: PathBuf::from("/tmp/fake.M01.XML"),
            label: "M01".into(),
        }
    }

    const MIN_V220: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<NonRealTimeMeta xmlns="urn:schemas-professionalDisc:nonRealTimeMeta:ver.2.20">
  <Duration value="3000"/>
  <LtcChangeTable tcFps="29" halfStep="true">
    <LtcChange frameCount="0" value="01000000" status="increment"/>
    <LtcChange frameCount="2999" value="01005959" status="end"/>
  </LtcChangeTable>
  <CreationDate value="2024-04-16T12:00:00+09:00"/>
  <VideoFormat>
    <VideoFrame videoCodec="AVC_3840_2160_HP@L51" captureFps="29.97p" formatFps="29.97p"/>
    <VideoLayout pixel="3840" numOfVerticalLine="2160" aspectRatio="16:9"/>
  </VideoFormat>
  <AudioFormat numOfChannel="2">
    <AudioRecPort port="DIRECT1" audioCodec="LPCM16" trackDst="CH1"/>
  </AudioFormat>
  <Device manufacturer="Sony" modelName="ILME-FX3" serialNo="0123456"/>
  <RecordingMode type="normal" cacheRec="false"/>
  <TargetMaterial umidRef="060A2B340101010501010D0013000000080046A0000000000000000000000000"/>
  <AcquisitionRecord>
    <Group name="CameraUnitMetadataSet">
      <Item name="CaptureGammaEquation" value="s-log3"/>
      <Item name="CaptureColorPrimaries" value="rec2020"/>
      <Item name="CodingEquations" value="rec709"/>
    </Group>
  </AcquisitionRecord>
</NonRealTimeMeta>
"#;

    fn tag<'a>(payload: &'a SidecarPayload, tag_name: &str) -> Option<&'a MetadataEntry> {
        payload.entries.iter().find(|e| e.tag_name == tag_name)
    }

    #[test]
    fn parses_v220_minimal_payload_surfaces_all_mapped_fields() {
        let payload = parse_nrt(&fake_sidecar(), MIN_V220.as_bytes());
        assert_eq!(
            payload
                .issues
                .iter()
                .filter(|i| i.severity == Severity::Warning || i.severity == Severity::Error)
                .count(),
            0,
            "no warnings/errors expected on a clean v2.20 fixture, got {:?}",
            payload.issues
        );

        let expected = [
            "Device.serialNo",
            "Device.manufacturer",
            "Device.modelName",
            "CreationDate",
            "RecordingMode.type",
            "RecordingMode.cacheRec",
            "VideoFrame.captureFps",
            "VideoFrame.formatFps",
            "VideoFrame.videoCodec",
            "VideoLayout.pixel",
            "VideoLayout.numOfVerticalLine",
            "VideoLayout.aspectRatio",
            "TargetMaterial.umidRef",
            "LtcChangeTable.tcFps",
            "LtcChangeTable.halfStep",
            "LtcChange.first",
            "LtcChange.last",
            "AudioFormat.numOfChannel",
            "AudioRecPort.audioCodec",
            "Duration.value",
            "CaptureGammaEquation",
            "CaptureColorPrimaries",
            "CodingEquations",
        ];
        for tag_name in expected {
            assert!(
                tag(&payload, tag_name).is_some(),
                "expected NRT entry {tag_name} in {:?}",
                payload
                    .entries
                    .iter()
                    .map(|e| e.tag_name.clone())
                    .collect::<Vec<_>>()
            );
        }

        assert_eq!(
            tag(&payload, "Device.serialNo").unwrap().value,
            TypedValue::String("0123456".into())
        );
        assert_eq!(
            tag(&payload, "VideoLayout.pixel").unwrap().value,
            TypedValue::Integer(3840)
        );
        assert_eq!(
            tag(&payload, "LtcChange.first").unwrap().value,
            TypedValue::String("01000000".into())
        );
        assert_eq!(
            tag(&payload, "LtcChange.last").unwrap().value,
            TypedValue::String("01005959".into())
        );
        assert_eq!(
            tag(&payload, "CaptureGammaEquation").unwrap().value,
            TypedValue::String("s-log3".into())
        );
        // Provenance namespace + container stamped correctly.
        let entry = tag(&payload, "TargetMaterial.umidRef").unwrap();
        assert_eq!(entry.namespace, "sony_nrt");
        assert_eq!(entry.provenance.namespace, "sony_nrt");
        assert_eq!(entry.provenance.container, "sidecar");
    }

    const MIN_V210: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<NonRealTimeMeta xmlns="urn:schemas-professionalDisc:nonRealTimeMeta:ver.2.10">
  <CreationDate value="2018-06-01T08:00:00+00:00"/>
  <Device manufacturer="Sony" modelName="PXW-FS5" serialNo="9999"/>
</NonRealTimeMeta>
"#;

    #[test]
    fn parses_v210_minimal_payload_with_known_shared_tags() {
        let payload = parse_nrt(&fake_sidecar(), MIN_V210.as_bytes());
        assert!(payload.issues.is_empty(), "{:?}", payload.issues);
        assert_eq!(
            tag(&payload, "Device.serialNo").unwrap().value,
            TypedValue::String("9999".into())
        );
        assert_eq!(
            tag(&payload, "CreationDate").unwrap().value,
            TypedValue::Timestamp("2018-06-01T08:00:00+00:00".into())
        );
    }

    const UNKNOWN_VERSION: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<NonRealTimeMeta xmlns="urn:schemas-professionalDisc:nonRealTimeMeta:ver.9.99">
  <Device serialNo="UNK"/>
</NonRealTimeMeta>
"#;

    #[test]
    fn unknown_xmlns_emits_info_issue_but_still_parses_known_tags() {
        let payload = parse_nrt(&fake_sidecar(), UNKNOWN_VERSION.as_bytes());
        assert_eq!(payload.issues.len(), 1);
        assert_eq!(payload.issues[0].code, SIDECAR_UNKNOWN_SCHEMA_VERSION);
        assert_eq!(payload.issues[0].severity, Severity::Info);
        assert_eq!(
            tag(&payload, "Device.serialNo").unwrap().value,
            TypedValue::String("UNK".into())
        );
    }

    #[test]
    fn discover_buffer_only_returns_empty() {
        let s = SonyNrtSidecar::new();
        assert!(s.discover(None).is_empty());
    }

    #[test]
    fn discover_finds_sibling_xml_with_descending_sequence_priority() {
        let dir = std::env::temp_dir().join("xifty-sidecar-sony-nrt-discover-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let primary = dir.join("C0042.MP4");
        std::fs::write(&primary, b"x").unwrap();
        std::fs::write(dir.join("C0042M01.XML"), b"<x/>").unwrap();
        std::fs::write(dir.join("C0042M02.XML"), b"<x/>").unwrap();
        // Unrelated decoy.
        std::fs::write(dir.join("OTHER.XML"), b"<x/>").unwrap();

        let s = SonyNrtSidecar::new();
        let hits = s.discover(Some(&primary));
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].label, "M02");
        assert_eq!(hits[1].label, "M01");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn discover_case_insensitive_extension() {
        let dir = std::env::temp_dir().join("xifty-sidecar-sony-nrt-discover-case");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let primary = dir.join("C0001.mp4");
        std::fs::write(&primary, b"x").unwrap();
        std::fs::write(dir.join("C0001M01.xml"), b"<x/>").unwrap();
        let s = SonyNrtSidecar::new();
        let hits = s.discover(Some(&primary));
        assert_eq!(hits.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
