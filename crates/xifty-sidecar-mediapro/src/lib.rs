//! Sony MEDIAPRO.XML cross-clip index sidecar adapter.
//!
//! Sony XAVC SD cards ship a card-level index file at
//! `PRIVATE/M4ROOT/MEDIAPRO.XML` cataloguing every clip on the card with
//! cross-clip metadata: system kind, master version, media id, per-clip
//! UMID, video/audio type, FPS, duration, aspect, thumbnail URI, and — on
//! pro-cam variants — recording-session windows and cross-clip references
//! such as "previous take" / "thumbnail of".
//!
//! Unlike the per-clip NRT XML (`xifty-sidecar-sony-nrt`, issue #122) this
//! sidecar is a single index file that lives several directories above the
//! primary clip. The current `xifty-sidecar` framework expects co-located
//! siblings, so we implement a bounded walk-up (≤ 4 hops) and encode the
//! present-clip's basename in [`SidecarRef::label`] so `parse` can do an
//! entry-level lookup.
//!
//! Real cards routinely contain orphan entries pointing at deleted clips,
//! and cards may have been written by cameras whose clip is not listed at
//! all — both surface as `info` issues, never panic. See the project plan
//! `specs/drafts/123-sidecar-mediapro.md` for the full field surface.

use std::path::{Path, PathBuf};

use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use xifty_core::{Issue, MetadataEntry, Provenance, Severity, TypedValue};
use xifty_sidecar::{
    MergeContext, MergePolicy, SIDECAR_NO_INDEX_ENTRY, SIDECAR_PARSE_ERROR, SIDECAR_TARGET_MISSING,
    SIDECAR_UNKNOWN_SCHEMA_VERSION, Sidecar, SidecarPayload, SidecarRef,
};

const NAMESPACE: &str = "mediapro";
const CONTAINER: &str = "sidecar";

/// Schema URN prefix shared across MEDIAPRO.XML versions. The actual
/// `MediaProfile@xmlns` carries the full URN; we treat any value beginning
/// with this prefix as recognised.
const XMLNS_PREFIX: &str = "http://xmlns.sony.net/pro/metadata/mediaprofile";

/// Bounded walk-up depth — counted from `primary.parent()`. Sony layout is
/// `M4ROOT/CLIP/C*.MP4` → `M4ROOT/MEDIAPRO.XML` (2 hops). Allow 4 so users
/// can mount the card under a wrapper directory without recursing toward `/`.
const MAX_WALK_UP: usize = 4;

const INDEX_FILENAME: &str = "MEDIAPRO.XML";

/// Sony MEDIAPRO.XML card-index sidecar adapter.
#[derive(Debug, Default)]
pub struct MediaproSidecar;

impl MediaproSidecar {
    pub const fn new() -> Self {
        Self
    }
}

impl Sidecar for MediaproSidecar {
    fn name(&self) -> &'static str {
        NAMESPACE
    }

    fn discover(&self, primary_path: Option<&Path>) -> Vec<SidecarRef> {
        let Some(primary) = primary_path else {
            return Vec::new();
        };
        discover_walk_up(primary)
    }

    fn parse(&self, sidecar: &SidecarRef, bytes: &[u8], _ctx: &MergeContext) -> SidecarPayload {
        parse_mediapro(sidecar, bytes)
    }

    fn priority(&self) -> u8 {
        100
    }

    fn merge_policy(&self) -> MergePolicy {
        // The card index carries strictly *additional* fields the per-clip
        // MP4 + NRT pair don't have (system kind, master version, per-clip
        // UMID cross-check, cross-take references). Overlap on UMID with
        // `sony_nrt::TargetMaterial.umidRef` is intentional and surfaces
        // through the standard conflict detector if values disagree.
        MergePolicy::Complement
    }
}

// ---------------------------------------------------------------------------
// Discovery — bounded walk-up
// ---------------------------------------------------------------------------

/// Walk parent directories upward (up to [`MAX_WALK_UP`] hops, counted from
/// `primary.parent()`) and return the first directory containing a
/// case-insensitive `MEDIAPRO.XML` file. The sidecar's primary basename is
/// encoded into [`SidecarRef::label`] as `MEDIAPRO|<basename>` so `parse`
/// can do per-clip entry lookup without changing `MergeContext`.
pub fn discover_walk_up(primary: &Path) -> Vec<SidecarRef> {
    let Some(basename) = primary.file_name().and_then(|n| n.to_str()) else {
        return Vec::new();
    };
    let label = format!("MEDIAPRO|{basename}");

    let mut current = match primary.parent() {
        Some(p) => p.to_path_buf(),
        None => return Vec::new(),
    };

    for _ in 0..=MAX_WALK_UP {
        if let Some(hit) = find_index_file(&current) {
            return vec![SidecarRef { path: hit, label }];
        }
        let next = match current.parent() {
            Some(p) => p.to_path_buf(),
            None => return Vec::new(),
        };
        if next == current {
            return Vec::new();
        }
        current = next;
    }
    Vec::new()
}

/// Case-insensitive scan of `dir` for `MEDIAPRO.XML`. We do a directory scan
/// rather than `Path::join("MEDIAPRO.XML").exists()` because SD cards on
/// Linux mount as case-sensitive `vfat`, while macOS mounts the same media
/// as case-insensitive `msdos` — a directory scan behaves identically
/// across hosts.
fn find_index_file(dir: &Path) -> Option<PathBuf> {
    let read = std::fs::read_dir(dir).ok()?;
    for entry in read.flatten() {
        if let Some(name) = entry.file_name().to_str()
            && name.eq_ignore_ascii_case(INDEX_FILENAME)
        {
            return Some(entry.path());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Parse a MEDIAPRO.XML byte slice and return the present-clip's matched
/// entry plus card-level header fields. Public so integration tests can
/// drive it without going through the discovery path.
pub fn parse_mediapro(sidecar: &SidecarRef, bytes: &[u8]) -> SidecarPayload {
    let mut entries: Vec<MetadataEntry> = Vec::new();
    let mut issues: Vec<Issue> = Vec::new();

    // Decode the present-clip basename from the SidecarRef label as the
    // very first operation. Defensive — never panic on a malformed label.
    let primary_basename = match sidecar.label.split_once('|') {
        Some((tag, basename)) if tag.eq_ignore_ascii_case("MEDIAPRO") && !basename.is_empty() => {
            basename.to_string()
        }
        _ => {
            issues.push(Issue {
                severity: Severity::Info,
                code: SIDECAR_NO_INDEX_ENTRY.into(),
                message: format!(
                    "MEDIAPRO sidecar label missing primary basename (got {:?})",
                    sidecar.label
                ),
                offset: None,
                context: Some(NAMESPACE.into()),
            });
            return SidecarPayload { entries, issues };
        }
    };

    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut clip_count: i64 = 0;
    let mut matched_material: Option<MatchedMaterial> = None;
    let mut in_matched_material = false;
    let mut matched_thumb_uri: Option<String> = None;
    let mut cross_refs: Vec<(Option<String>, Option<String>)> = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(ref event @ Event::Start(_)) | Ok(ref event @ Event::Empty(_)) => {
                let (e, is_start): (&BytesStart<'_>, bool) = match event {
                    Event::Start(e) => (e, true),
                    Event::Empty(e) => (e, false),
                    _ => unreachable!(),
                };
                let local = local_name(e);

                match local.as_str() {
                    // Root element — schema version + createdAt + xmlns check.
                    "MediaProfile" => {
                        if let Some(xmlns) = attr_value(e, b"xmlns")
                            && !xmlns.starts_with(XMLNS_PREFIX)
                        {
                            issues.push(Issue {
                                severity: Severity::Info,
                                code: SIDECAR_UNKNOWN_SCHEMA_VERSION.into(),
                                message: format!(
                                    "MEDIAPRO sidecar uses unrecognized schema xmlns: {xmlns}"
                                ),
                                offset: None,
                                context: Some(NAMESPACE.into()),
                            });
                        }
                        if let Some(v) = attr_value(e, b"version") {
                            push_string(
                                &mut entries,
                                sidecar,
                                "media_profile.version",
                                "media_profile.version",
                                v,
                            );
                        }
                        if let Some(v) = attr_value(e, b"createdAt") {
                            push_string(
                                &mut entries,
                                sidecar,
                                "media_profile.created_at",
                                "media_profile.created_at",
                                v,
                            );
                        }
                    }
                    "System" => {
                        if let Some(v) = attr_value(e, b"systemId") {
                            push_string(
                                &mut entries,
                                sidecar,
                                "media_profile.system_id",
                                "media_profile.system_id",
                                v,
                            );
                        }
                        if let Some(v) = attr_value(e, b"systemKind") {
                            push_string(
                                &mut entries,
                                sidecar,
                                "media_profile.system_kind",
                                "media_profile.system_kind",
                                v,
                            );
                        }
                        if let Some(v) = attr_value(e, b"masterVersion") {
                            // INDEPENDENT of `media_profile.version` — the
                            // `MediaProfile@version` is the schema/file
                            // version (e.g. "2.10"); `<System masterVersion>`
                            // is the recording-format version
                            // (e.g. "XAVC-M4@1.10.00"). Do not collapse.
                            push_string(
                                &mut entries,
                                sidecar,
                                "media_profile.master_version",
                                "media_profile.master_version",
                                v,
                            );
                        }
                    }
                    "Attached" => {
                        if let Some(v) = attr_value(e, b"mediaId") {
                            push_string(
                                &mut entries,
                                sidecar,
                                "media_profile.media_id",
                                "media_profile.media_id",
                                v,
                            );
                        }
                        if let Some(v) = attr_value(e, b"mediaKind") {
                            push_string(
                                &mut entries,
                                sidecar,
                                "media_profile.media_kind",
                                "media_profile.media_kind",
                                v,
                            );
                        }
                        if let Some(v) = attr_value(e, b"mediaName")
                            && !v.is_empty()
                        {
                            push_string(
                                &mut entries,
                                sidecar,
                                "media_profile.media_name",
                                "media_profile.media_name",
                                v,
                            );
                        }
                    }
                    // pro-cam only — element name and attribute spellings
                    // inferred. Accept either `startTime`/`endTime` or
                    // `start`/`end`.
                    "RecordingSession" => {
                        let start = attr_value(e, b"startTime").or_else(|| attr_value(e, b"start"));
                        let end = attr_value(e, b"endTime").or_else(|| attr_value(e, b"end"));
                        if let Some(v) = start {
                            push_string(
                                &mut entries,
                                sidecar,
                                "media_profile.recording_session.start",
                                "media_profile.recording_session.start",
                                v,
                            );
                        }
                        if let Some(v) = end {
                            push_string(
                                &mut entries,
                                sidecar,
                                "media_profile.recording_session.end",
                                "media_profile.recording_session.end",
                                v,
                            );
                        }
                    }
                    "Material" => {
                        clip_count += 1;
                        // Match the present clip by basename suffix on
                        // `Material@uri` (e.g. `./CLIP/C0242.MP4`).
                        if matched_material.is_none()
                            && let Some(uri) = attr_value(e, b"uri")
                            && uri_matches_basename(&uri, &primary_basename)
                        {
                            // Only enter "inside material" mode for a real
                            // <Material>...</Material> pair. Self-closing
                            // (`Empty`) materials carry no children to attribute.
                            in_matched_material = is_start;
                            matched_material = Some(MatchedMaterial::from(e));
                        }
                    }
                    "RelevantInfo" if in_matched_material => {
                        if attr_value(e, b"type").as_deref() == Some("JPG")
                            && let Some(uri) = attr_value(e, b"uri")
                        {
                            matched_thumb_uri = Some(uri);
                        }
                    }
                    // pro-cam cross-references; appear as direct child of
                    // `<Material>` or nested under `<Component>`. We use
                    // local-name match plus dual attribute spellings.
                    "Reference" if in_matched_material => {
                        let kind = attr_value(e, b"type").or_else(|| attr_value(e, b"kind"));
                        let umid = attr_value(e, b"umidRef").or_else(|| attr_value(e, b"umid"));
                        if kind.is_some() || umid.is_some() {
                            cross_refs.push((kind, umid));
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let local = bytes_to_local_name(e.name().as_ref());
                if local == "Material" && in_matched_material {
                    in_matched_material = false;
                }
            }
            Ok(_) => {}
            Err(error) => {
                issues.push(Issue {
                    severity: Severity::Warning,
                    code: SIDECAR_PARSE_ERROR.into(),
                    message: format!("MEDIAPRO XML parse error: {error}"),
                    offset: Some(reader.buffer_position()),
                    context: Some(NAMESPACE.into()),
                });
                break;
            }
        }
        buf.clear();
    }

    // Card-level summary always emitted (clip_count) once we've walked
    // the whole document; missing-version cases still yield a count.
    push_integer(
        &mut entries,
        sidecar,
        "media_profile.clip_count",
        "media_profile.clip_count",
        clip_count,
    );

    if let Some(m) = matched_material {
        // Per-clip structural fields under the `mediapro.*` keyspace.
        if let Some(v) = m.umid {
            push_string(&mut entries, sidecar, "mediapro.umid", "mediapro.umid", v);
        }
        if let Some(v) = m.video_type {
            push_string(
                &mut entries,
                sidecar,
                "mediapro.video_type",
                "mediapro.video_type",
                v,
            );
        }
        if let Some(v) = m.audio_type {
            push_string(
                &mut entries,
                sidecar,
                "mediapro.audio_type",
                "mediapro.audio_type",
                v,
            );
        }
        if let Some(v) = m.fps {
            push_string(&mut entries, sidecar, "mediapro.fps", "mediapro.fps", v);
        }
        if let Some(v) = m.dur {
            push_integer_or_string(
                &mut entries,
                sidecar,
                "mediapro.duration_frames",
                "mediapro.duration_frames",
                v,
            );
        }
        if let Some(v) = m.ch {
            push_integer_or_string(
                &mut entries,
                sidecar,
                "mediapro.channels",
                "mediapro.channels",
                v,
            );
        }
        if let Some(v) = m.aspect_ratio {
            push_string(
                &mut entries,
                sidecar,
                "mediapro.aspect_ratio",
                "mediapro.aspect_ratio",
                v,
            );
        }

        // Thumbnail resolution + orphan reporting.
        if let Some(uri) = matched_thumb_uri {
            let resolved = resolve_uri(&sidecar.path, &uri);
            push_string(
                &mut entries,
                sidecar,
                "mediapro.thumbnail.path",
                "mediapro.thumbnail.path",
                resolved.to_string_lossy().into_owned(),
            );
            if std::fs::metadata(&resolved).is_err() {
                issues.push(Issue {
                    severity: Severity::Info,
                    code: SIDECAR_TARGET_MISSING.into(),
                    message: format!("MEDIAPRO thumbnail target missing: {}", resolved.display()),
                    offset: None,
                    context: Some(NAMESPACE.into()),
                });
            }
        }

        // Flat cross_refs[i].{kind,umid} array — pro-cam only.
        for (i, (kind, umid)) in cross_refs.iter().enumerate() {
            if let Some(k) = kind {
                let key = format!("media_profile.cross_refs.{i}.kind");
                push_string(&mut entries, sidecar, &key, &key, k.clone());
            }
            if let Some(u) = umid {
                let key = format!("media_profile.cross_refs.{i}.umid");
                push_string(&mut entries, sidecar, &key, &key, u.clone());
            }
        }
    } else {
        issues.push(Issue {
            severity: Severity::Info,
            code: SIDECAR_NO_INDEX_ENTRY.into(),
            message: format!("primary clip {primary_basename} not listed in MEDIAPRO.XML"),
            offset: None,
            context: Some(NAMESPACE.into()),
        });
    }

    SidecarPayload { entries, issues }
}

#[derive(Debug, Default)]
struct MatchedMaterial {
    umid: Option<String>,
    video_type: Option<String>,
    audio_type: Option<String>,
    fps: Option<String>,
    dur: Option<String>,
    ch: Option<String>,
    aspect_ratio: Option<String>,
}

impl MatchedMaterial {
    fn from(e: &BytesStart<'_>) -> Self {
        Self {
            umid: attr_value(e, b"umid"),
            video_type: attr_value(e, b"videoType"),
            audio_type: attr_value(e, b"audioType"),
            fps: attr_value(e, b"fps"),
            dur: attr_value(e, b"dur"),
            ch: attr_value(e, b"ch"),
            aspect_ratio: attr_value(e, b"aspectRatio"),
        }
    }
}

/// Match `Material@uri` (e.g. `./CLIP/C0242.MP4`) to a primary basename
/// (`C0242.MP4`) case-insensitively. Accepts either `/` or `\` separators.
fn uri_matches_basename(uri: &str, basename: &str) -> bool {
    let tail = uri.rsplit(|c| c == '/' || c == '\\').next().unwrap_or(uri);
    tail.eq_ignore_ascii_case(basename)
}

/// Resolve a relative URI from inside MEDIAPRO.XML against the directory
/// containing the sidecar. Absolute paths are returned unchanged. URIs
/// using `/` separators are converted to native `Path` joins.
fn resolve_uri(sidecar_path: &Path, uri: &str) -> PathBuf {
    let cleaned = uri.trim_start_matches("./");
    let candidate = Path::new(cleaned);
    if candidate.is_absolute() {
        return candidate.to_path_buf();
    }
    let base = sidecar_path.parent().unwrap_or(Path::new(""));
    let mut resolved = base.to_path_buf();
    for segment in cleaned.split(|c| c == '/' || c == '\\') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            resolved.pop();
            continue;
        }
        resolved.push(segment);
    }
    resolved
}

// ---------------------------------------------------------------------------
// Attribute helpers (mirrors xifty-sidecar-sony-nrt)
// ---------------------------------------------------------------------------

fn local_name(e: &BytesStart<'_>) -> String {
    bytes_to_local_name(e.name().as_ref())
}

fn bytes_to_local_name(name: &[u8]) -> String {
    let s = std::str::from_utf8(name).unwrap_or("");
    if let Some(idx) = s.rfind(':') {
        s[idx + 1..].to_string()
    } else {
        s.to_string()
    }
}

/// Read a MEDIAPRO attribute by local name. Mirrors the NRT helper — see
/// `xifty-sidecar-sony-nrt::attr_value` for the rationale on not running
/// XML entity decoding (every attribute payload defined in the Sony pro
/// metadata schema is constrained to ASCII-clean tokens).
fn attr_value(e: &BytesStart<'_>, key: &[u8]) -> Option<String> {
    for attr in e.attributes().flatten() {
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
        notes: vec![format!("sony mediapro sidecar ({})", sidecar.label)],
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

fn push_integer(
    entries: &mut Vec<MetadataEntry>,
    sidecar: &SidecarRef,
    tag_id: &str,
    tag_name: &str,
    value: i64,
) {
    entries.push(MetadataEntry {
        namespace: NAMESPACE.into(),
        tag_id: tag_id.into(),
        tag_name: tag_name.into(),
        value: TypedValue::Integer(value),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_buffer_only_returns_empty() {
        let s = MediaproSidecar::new();
        assert!(s.discover(None).is_empty());
    }

    #[test]
    fn name_is_mediapro() {
        assert_eq!(MediaproSidecar::new().name(), "mediapro");
    }

    #[test]
    fn uri_basename_match_is_case_insensitive() {
        assert!(uri_matches_basename("./CLIP/C0242.MP4", "C0242.MP4"));
        assert!(uri_matches_basename("CLIP/c0242.mp4", "C0242.MP4"));
        assert!(uri_matches_basename(r"CLIP\C0242.MP4", "C0242.MP4"));
        assert!(!uri_matches_basename("CLIP/C0001.MP4", "C0242.MP4"));
    }

    #[test]
    fn resolve_uri_joins_against_sidecar_dir() {
        let sidecar = Path::new("/tmp/M4ROOT/MEDIAPRO.XML");
        let r = resolve_uri(sidecar, "./CLIP/C0001M01.JPG");
        assert_eq!(r, PathBuf::from("/tmp/M4ROOT/CLIP/C0001M01.JPG"));
    }
}
