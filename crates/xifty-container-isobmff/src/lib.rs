//! ISOBMFF (ISO/IEC 14496-12) container parser shared by HEIF/AVIF/MP4/MOV
//! and the Canon CR3 RAW format.
//!
//! Canon CR3 stores EXIF and Canon maker-note data inside a Canon-specific
//! `uuid` box under `moov/uuid`. The payload of that uuid box is laid out as
//! a sequence of FourCC sub-boxes named `CMT1`/`CMT2`/`CMT3`/`CMT4`, each
//! wrapping a TIFF-shaped IFD. The Canon CMT UUID is
//! `85 c0 b6 87 82 0f 11 e0 81 11 f4 ce 46 2b 6a 48` per ExifTool
//! `lib/Image/ExifTool/Canon.pm` (reverse-engineering reference).
//!
//! Dispatch:
//! - `CMT1` is surfaced as `IsobmffPayload { kind: "exif", tag: Some("CMT1") }`
//!   so the existing `exif_payloads()` filter routes it through the EXIF
//!   pipeline (and from there through `xifty-meta-canon`).
//! - `CMT2` / `CMT3` / `CMT4` are surfaced as
//!   `IsobmffPayload { kind: "canon-cmt", tag: Some("CMT*") }` and exposed via
//!   the `canon_cmt_payloads()` helper.

use std::collections::HashMap;

use xifty_core::{ContainerNode, Issue, Severity, XiftyError, issue};
use xifty_source::{Cursor, Endian, SourceBytes};

/// Canon CR3 maker-note UUID. Sub-boxes inside this uuid payload are
/// FourCC-tagged `CMT1`/`CMT2`/`CMT3`/`CMT4` TIFF-shaped IFDs.
/// Source: ExifTool `lib/Image/ExifTool/Canon.pm`.
const CANON_CMT_UUID: [u8; 16] = [
    0x85, 0xc0, 0xb6, 0x87, 0x82, 0x0f, 0x11, 0xe0, 0x81, 0x11, 0xf4, 0xce, 0x46, 0x2b, 0x6a, 0x48,
];

/// Sony "User Data Atom UUID" tail. The full 16-byte usertype is
/// `<atom-name (4 ASCII bytes)> + SONY_USERDATA_UUID_TAIL`. Atom names seen
/// in the wild include `PROF`, `USMT`, `PRPF`, `AOLY`, `MTDT`. Source:
/// ExifTool `lib/Image/ExifTool/QuickTime.pm` (`UUID-PROF` at line ~711-718,
/// `UUID-USMT` at line ~1223-1231 and ~1449-1457).
const SONY_USERDATA_UUID_TAIL: [u8; 12] = [
    0x21, 0xd2, 0x4f, 0xce, 0xbb, 0x88, 0x69, 0x5c, 0xfa, 0xc9, 0xc7, 0x40,
];

/// C2PA "uuid" box usertype, per the C2PA 2.0 specification (also published
/// in ISO 22144). The payload of a matching `uuid` box is a contiguous JUMBF
/// byte stream.
const C2PA_UUID: [u8; 16] = [
    0xd8, 0xfe, 0xc5, 0xd8, 0x76, 0xcc, 0xb9, 0xee, 0x9c, 0xc0, 0x50, 0x4d, 0x8a, 0x16, 0xb1, 0x13,
];

/// Recognised payload `kind` discriminants:
/// `"exif"`, `"xmp"`, `"icc"`, `"iptc"`, `"quicktime"`, `"quicktime-udta"`,
/// `"itunes"`, `"canon-cmt"` (Canon CR3 CMT2/CMT3/CMT4 maker-note IFDs), and
/// `"sony-video-atom"` (Sony PROF/USMT/etc. user-data UUID atoms — `tag`
/// holds the 4-character ASCII atom name from the usertype prefix).
#[derive(Debug, Clone)]
pub struct IsobmffPayload {
    pub kind: &'static str,
    pub tag: Option<String>,
    pub offset_start: u64,
    pub offset_end: u64,
    pub data_offset: u64,
    pub data_length: u64,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct IsobmffDimensions {
    pub width: u32,
    pub height: u32,
    pub offset_start: u64,
    pub offset_end: u64,
    pub path: String,
}

/// HDR colour information surfaced from `colr` (nclx), `cicp`, or `pixi`
/// boxes attached to the primary item via the property-association chain.
///
/// `source` records which box populated the entry so downstream consumers
/// can apply a precedence policy (cicp > pixi > colr/nclx).
#[derive(Debug, Clone)]
pub struct IsobmffColorInfo {
    pub primaries: Option<u16>,
    pub transfer: Option<u16>,
    pub matrix: Option<u16>,
    pub full_range: Option<bool>,
    pub source: &'static str,
    pub offset_start: u64,
    pub offset_end: u64,
    pub path: String,
}

/// Pixel bit-depth per channel surfaced from `pixi`.
#[derive(Debug, Clone)]
pub struct IsobmffPixelInfo {
    pub bits_per_channel: Vec<u8>,
    pub offset_start: u64,
    pub offset_end: u64,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct IsobmffContainer {
    pub major_brand: [u8; 4],
    pub compatible_brands: Vec<[u8; 4]>,
    pub nodes: Vec<ContainerNode>,
    pub payloads: Vec<IsobmffPayload>,
    pub primary_item_dimensions: Option<IsobmffDimensions>,
    pub media_duration_seconds: Option<f64>,
    pub media_created_at: Option<String>,
    pub media_modified_at: Option<String>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub video_frame_rate: Option<f64>,
    pub video_bitrate: Option<u32>,
    pub video_bitrate_note: Option<String>,
    pub audio_channels: Option<u16>,
    pub audio_sample_rate: Option<u32>,
    pub primary_visual_dimensions: Option<IsobmffDimensions>,
    /// HDR colour info attached to the primary item via the property
    /// association chain. Populated from `colr` (nclx), `cicp`, or `pixi`.
    /// When multiple sources are present, the precedence policy
    /// (cicp > pixi > colr/nclx) is left to consumers.
    pub primary_item_color: Option<IsobmffColorInfo>,
    /// Pixel bit-depth per channel for the primary item, sourced from `pixi`.
    pub primary_item_pixel: Option<IsobmffPixelInfo>,
    pub issues: Vec<Issue>,
}

impl IsobmffContainer {
    pub fn exif_payloads(&self) -> impl Iterator<Item = &IsobmffPayload> {
        self.payloads
            .iter()
            .filter(|payload| payload.kind == "exif")
    }

    pub fn xmp_payloads(&self) -> impl Iterator<Item = &IsobmffPayload> {
        self.payloads.iter().filter(|payload| payload.kind == "xmp")
    }

    pub fn icc_payloads(&self) -> impl Iterator<Item = &IsobmffPayload> {
        self.payloads.iter().filter(|payload| payload.kind == "icc")
    }

    pub fn iptc_payloads(&self) -> impl Iterator<Item = &IsobmffPayload> {
        self.payloads
            .iter()
            .filter(|payload| payload.kind == "iptc")
    }

    pub fn quicktime_payloads(&self) -> impl Iterator<Item = &IsobmffPayload> {
        self.payloads
            .iter()
            .filter(|payload| payload.kind == "quicktime")
    }

    pub fn quicktime_udta_payloads(&self) -> impl Iterator<Item = &IsobmffPayload> {
        self.payloads
            .iter()
            .filter(|payload| payload.kind == "quicktime-udta")
    }

    pub fn itunes_payloads(&self) -> impl Iterator<Item = &IsobmffPayload> {
        self.payloads
            .iter()
            .filter(|payload| payload.kind == "itunes")
    }

    /// Canon CR3 maker-note IFDs surfaced from the Canon `uuid` box payload.
    /// CMT1 is routed through the regular `exif_payloads()` channel; this
    /// helper covers CMT2/CMT3/CMT4 which carry Canon-specific TIFF IFDs.
    pub fn canon_cmt_payloads(&self) -> impl Iterator<Item = &IsobmffPayload> {
        self.payloads
            .iter()
            .filter(|payload| payload.kind == "canon-cmt")
    }

    /// C2PA JUMBF manifests surfaced from `uuid` boxes whose 16-byte usertype
    /// matches the canonical C2PA UUID. The payload of such a box is a
    /// contiguous JUMBF byte stream consumable by `xifty-meta-c2pa`.
    pub fn c2pa_payloads(&self) -> impl Iterator<Item = &IsobmffPayload> {
        self.payloads
            .iter()
            .filter(|payload| payload.kind == "c2pa")
    }

    /// Sony video user-data atoms surfaced from `uuid` boxes whose 16-byte
    /// usertype matches `<atom-name (4 ASCII)> + SONY_USERDATA_UUID_TAIL`.
    /// `payload.tag` holds the atom name (`"PROF"`, `"USMT"`, `"PRPF"`, ...).
    /// `payload.data_offset`/`data_length` cover the bytes that follow the
    /// 16-byte usertype — i.e. the slice that decoders consume directly.
    pub fn sony_video_atoms(&self) -> impl Iterator<Item = &IsobmffPayload> {
        self.payloads
            .iter()
            .filter(|payload| payload.kind == "sony-video-atom")
    }

    pub fn is_heif_still_image(&self) -> bool {
        heif_brand(self.major_brand) || self.compatible_brands.iter().copied().any(heif_brand)
    }
}

#[derive(Debug, Clone)]
struct ParsedBox {
    start: usize,
    box_type: [u8; 4],
    data_offset: usize,
    end: usize,
    path: String,
}

#[derive(Debug, Clone)]
struct ItemInfo {
    item_id: u32,
    item_type: [u8; 4],
    content_type: Option<String>,
}

#[derive(Debug, Clone)]
struct ItemExtent {
    offset: u64,
    length: u64,
}

#[derive(Debug, Clone)]
struct ItemLocation {
    item_id: u32,
    construction_method: u16,
    base_offset: u64,
    extents: Vec<ItemExtent>,
}

#[derive(Debug, Clone)]
struct PropertyAssociation {
    item_id: u32,
    property_indexes: Vec<u16>,
}

#[derive(Debug, Clone, Default)]
struct MovieHeader {
    created_at: Option<String>,
    modified_at: Option<String>,
    duration_seconds: Option<f64>,
}

#[derive(Debug, Clone, Default)]
struct TrackFacts {
    handler_type: Option<[u8; 4]>,
    timescale: Option<u32>,
    duration_seconds: Option<f64>,
    codec: Option<String>,
    frame_rate: Option<f64>,
    bitrate: Option<u32>,
    total_sample_bytes: u64,
    audio_channels: Option<u16>,
    audio_sample_rate: Option<u32>,
    dimensions: Option<(u32, u32)>,
}

#[derive(Debug, Clone)]
struct ParseState {
    item_infos: Vec<ItemInfo>,
    item_locations: Vec<ItemLocation>,
    property_associations: Vec<PropertyAssociation>,
    property_dimensions: HashMap<u16, IsobmffDimensions>,
    property_colors: HashMap<u16, IsobmffColorInfo>,
    property_pixels: HashMap<u16, IsobmffPixelInfo>,
    /// Monotonic property counter used to assign 1-based ipco property
    /// indexes regardless of which child box (`ispe`, `colr`, `cicp`,
    /// `pixi`, …) produced the entry. The `ipco` walk visits children in
    /// document order, which matches how `ipma` references them.
    property_count: u16,
    primary_item_id: Option<u32>,
    idat_payloads: Vec<(u64, u64, String)>,
    movie_header: MovieHeader,
    tracks: HashMap<String, TrackFacts>,
}

pub fn parse(source: &SourceBytes) -> Result<IsobmffContainer, XiftyError> {
    parse_bytes(source.bytes(), 0)
}

pub fn parse_bytes(bytes: &[u8], base_offset: u64) -> Result<IsobmffContainer, XiftyError> {
    let cursor = Cursor::new(bytes, base_offset);
    if cursor.len() < 16 || cursor.slice(4, 4)? != b"ftyp" {
        return Err(XiftyError::Parse {
            message: "not an isobmff container".into(),
        });
    }

    let mut state = ParseState {
        item_infos: Vec::new(),
        item_locations: Vec::new(),
        property_associations: Vec::new(),
        property_dimensions: HashMap::new(),
        property_colors: HashMap::new(),
        property_pixels: HashMap::new(),
        property_count: 0,
        primary_item_id: None,
        idat_payloads: Vec::new(),
        movie_header: MovieHeader::default(),
        tracks: HashMap::new(),
    };
    let mut nodes = Vec::new();
    let mut payloads = Vec::new();
    let mut issues = Vec::new();
    let mut major_brand = *b"    ";
    let mut compatible_brands = Vec::new();

    parse_children(
        &cursor,
        0,
        cursor.len(),
        None,
        None,
        &mut state,
        &mut nodes,
        &mut payloads,
        &mut issues,
        &mut major_brand,
        &mut compatible_brands,
    )?;

    payloads.extend(payloads_from_items(&cursor, &state, &mut issues));
    let primary_item_dimensions = primary_item_dimensions(&state);
    let primary_item_color = primary_item_color(&state);
    let primary_item_pixel = primary_item_pixel(&state);
    let primary_visual_dimensions = primary_visual_dimensions(&state);
    let video_codec = state
        .tracks
        .values()
        .find(|track| track.handler_type == Some(*b"vide"))
        .and_then(|track| track.codec.clone());
    let video_frame_rate = state
        .tracks
        .values()
        .find(|track| track.handler_type == Some(*b"vide"))
        .and_then(|track| track.frame_rate);
    let video_bitrate = state
        .tracks
        .values()
        .find(|track| track.handler_type == Some(*b"vide"))
        .and_then(|track| track.bitrate.or_else(|| derive_track_bitrate(track)));
    let video_bitrate_note = state
        .tracks
        .values()
        .find(|track| track.handler_type == Some(*b"vide"))
        .and_then(|track| {
            if track.bitrate.is_some() {
                Some("derived from video sample description bitrate box".to_string())
            } else if track.total_sample_bytes > 0 {
                Some("derived from video track sample sizes and duration".to_string())
            } else {
                None
            }
        });
    let audio_codec = state
        .tracks
        .values()
        .find(|track| track.handler_type == Some(*b"soun"))
        .and_then(|track| track.codec.clone());
    let audio_channels = state
        .tracks
        .values()
        .find(|track| track.handler_type == Some(*b"soun"))
        .and_then(|track| track.audio_channels);
    let audio_sample_rate = state
        .tracks
        .values()
        .find(|track| track.handler_type == Some(*b"soun"))
        .and_then(|track| track.audio_sample_rate);

    if !supported_brand(major_brand) && !compatible_brands.iter().copied().any(supported_brand) {
        issues.push(issue(
            Severity::Info,
            "isobmff_unrecognized_brand",
            format!(
                "isobmff major brand {} is not a recognized supported brand",
                fourcc(major_brand)
            ),
        ));
    }

    Ok(IsobmffContainer {
        major_brand,
        compatible_brands,
        nodes,
        payloads,
        primary_item_dimensions,
        media_duration_seconds: state.movie_header.duration_seconds.or_else(|| {
            state
                .tracks
                .values()
                .filter_map(|track| track.duration_seconds)
                .max_by(f64::total_cmp)
        }),
        media_created_at: state.movie_header.created_at,
        media_modified_at: state.movie_header.modified_at,
        video_codec,
        audio_codec,
        video_frame_rate,
        video_bitrate,
        video_bitrate_note,
        audio_channels,
        audio_sample_rate,
        primary_visual_dimensions,
        primary_item_color,
        primary_item_pixel,
        issues,
    })
}

fn parse_children(
    cursor: &Cursor<'_>,
    start: usize,
    end: usize,
    parent: Option<&ParsedBox>,
    current_track: Option<String>,
    state: &mut ParseState,
    nodes: &mut Vec<ContainerNode>,
    payloads: &mut Vec<IsobmffPayload>,
    issues: &mut Vec<Issue>,
    major_brand: &mut [u8; 4],
    compatible_brands: &mut Vec<[u8; 4]>,
) -> Result<(), XiftyError> {
    let mut offset = start;
    while offset < end {
        let Some(parsed) = parse_box_header(cursor, offset, end, parent, issues)? else {
            break;
        };

        nodes.push(ContainerNode {
            kind: "box".into(),
            label: parsed.path.clone(),
            offset_start: cursor.absolute_offset(offset),
            offset_end: cursor.absolute_offset(parsed.end),
            parent_label: parent.map(|item| item.path.clone()),
        });

        let child_track = if &parsed.box_type == b"trak" {
            Some(format!("{}@{}", parsed.path, parsed.start))
        } else {
            current_track.clone()
        };

        match &parsed.box_type {
            b"ftyp" => parse_ftyp(cursor, &parsed, major_brand, compatible_brands)?,
            b"meta" => {
                if parsed.data_offset + 4 > parsed.end {
                    issues.push(Issue {
                        severity: Severity::Warning,
                        code: "isobmff_meta_header_truncated".into(),
                        message: "meta box is missing full-box header bytes".into(),
                        offset: Some(cursor.absolute_offset(parsed.data_offset)),
                        context: Some(parsed.path.clone()),
                    });
                } else {
                    parse_children(
                        cursor,
                        parsed.data_offset + 4,
                        parsed.end,
                        Some(&parsed),
                        child_track.clone(),
                        state,
                        nodes,
                        payloads,
                        issues,
                        major_brand,
                        compatible_brands,
                    )?;
                }
            }
            b"dinf" | b"dref" | b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" | b"edts"
            | b"udta" | b"iprp" | b"ipco" | b"ilst" => {
                parse_children(
                    cursor,
                    parsed.data_offset,
                    parsed.end,
                    Some(&parsed),
                    child_track.clone(),
                    state,
                    nodes,
                    payloads,
                    issues,
                    major_brand,
                    compatible_brands,
                )?;
            }
            b"iinf" => {
                let child_start = parse_iinf(cursor, &parsed, state, issues)?;
                if child_start <= parsed.end {
                    parse_children(
                        cursor,
                        child_start,
                        parsed.end,
                        Some(&parsed),
                        child_track.clone(),
                        state,
                        nodes,
                        payloads,
                        issues,
                        major_brand,
                        compatible_brands,
                    )?;
                }
            }
            b"mvhd" => parse_mvhd(cursor, &parsed, state, issues)?,
            b"tkhd" => parse_tkhd(cursor, &parsed, current_track.as_deref(), state, issues)?,
            b"mdhd" => parse_mdhd(cursor, &parsed, current_track.as_deref(), state, issues)?,
            b"hdlr" => parse_hdlr(cursor, &parsed, current_track.as_deref(), state, issues)?,
            b"stsd" => parse_stsd(cursor, &parsed, current_track.as_deref(), state, issues)?,
            b"stts" => parse_stts(cursor, &parsed, current_track.as_deref(), state, issues)?,
            b"stsz" => parse_stsz(cursor, &parsed, current_track.as_deref(), state, issues)?,
            b"infe" => parse_infe(cursor, &parsed, state, issues)?,
            b"pitm" => parse_pitm(cursor, &parsed, state, issues)?,
            b"iloc" => parse_iloc(cursor, &parsed, state, issues)?,
            b"ipma" => parse_ipma(cursor, &parsed, state, issues)?,
            b"ispe" => parse_ispe(cursor, &parsed, state, issues)?,
            b"\xa9ART" | b"\xa9too" | b"\xa9nam" => {
                if let Some(payload) = parse_quicktime_item(cursor, &parsed) {
                    payloads.push(payload);
                }
                if let Some(payload) = parse_itunes_item(cursor, &parsed) {
                    payloads.push(payload);
                }
            }
            b"\xa9alb" | b"\xa9day" | b"\xa9gen" | b"\xa9cmt" | b"\xa9wrt" | b"\xa9lyr"
            | b"aART" | b"trkn" | b"disk" | b"cpil" | b"tmpo" | b"covr" => {
                if let Some(payload) = parse_itunes_item(cursor, &parsed) {
                    payloads.push(payload);
                }
            }
            // DJI drone telemetry + classic QuickTime location, stored as
            // direct-`udta` 4cc atoms in classic user-data text format
            // ({u16 length}{u16 lang}{ascii}). Only fires outside `ilst` so
            // iTunes routing is untouched.
            b"\xa9xyz" | b"\xa9xsp" | b"\xa9ysp" | b"\xa9zsp" | b"\xa9fpt" | b"\xa9fyw"
            | b"\xa9frl" | b"\xa9gpt" | b"\xa9gyw" | b"\xa9grl" | b"\xa9mdl" | b"\xa9csn"
                if !parsed.path.contains("ilst") =>
            {
                if let Some(payload) = parse_quicktime_udta_text(cursor, &parsed) {
                    payloads.push(payload);
                }
            }
            b"idat" => {
                state.idat_payloads.push((
                    cursor.absolute_offset(parsed.data_offset),
                    (parsed.end - parsed.data_offset) as u64,
                    parsed.path.clone(),
                ));
                issues.push(Issue {
                    severity: Severity::Info,
                    code: "isobmff_structure_recognized_uninterpreted".into(),
                    message: "recognized idat structure and retained it for item routing".into(),
                    offset: Some(cursor.absolute_offset(offset)),
                    context: Some(parsed.path.clone()),
                });
            }
            b"Exif" => {
                payloads.push(IsobmffPayload {
                    kind: "exif",
                    tag: None,
                    offset_start: cursor.absolute_offset(offset),
                    offset_end: cursor.absolute_offset(parsed.end),
                    data_offset: cursor.absolute_offset(parsed.data_offset),
                    data_length: (parsed.end - parsed.data_offset) as u64,
                    path: parsed.path.clone(),
                });
            }
            b"mime" => {
                if let Some(payload) = parse_mime_payload(cursor, &parsed) {
                    payloads.push(payload);
                } else {
                    issues.push(Issue {
                        severity: Severity::Info,
                        code: "isobmff_mime_payload_unsupported".into(),
                        message:
                            "mime box was recognized but did not contain a supported XMP payload"
                                .into(),
                        offset: Some(cursor.absolute_offset(offset)),
                        context: Some(parsed.path.clone()),
                    });
                }
            }
            b"xml " => {
                payloads.push(IsobmffPayload {
                    kind: "xmp",
                    tag: None,
                    offset_start: cursor.absolute_offset(offset),
                    offset_end: cursor.absolute_offset(parsed.end),
                    data_offset: cursor.absolute_offset(parsed.data_offset),
                    data_length: (parsed.end - parsed.data_offset) as u64,
                    path: parsed.path.clone(),
                });
            }
            b"colr" => {
                parse_colr(cursor, &parsed, state, payloads, issues);
            }
            // `cicp` and `pixi` live as siblings of `colr` inside `ipco`'s
            // children walk. Dispatching here (NOT under the outer `b"ipco"`
            // arm) is required: the recursive walk processes ipco's
            // children at this level, and ipma references those children
            // by 1-based document-order index.
            b"cicp" => {
                parse_cicp(cursor, &parsed, state, issues);
            }
            b"pixi" => {
                parse_pixi(cursor, &parsed, state, issues);
            }
            b"iref" => {
                issues.push(Issue {
                    severity: Severity::Info,
                    code: "isobmff_structure_recognized_uninterpreted".into(),
                    message: format!(
                        "recognized {} structure but do not yet interpret its full semantics",
                        fourcc(parsed.box_type)
                    ),
                    offset: Some(cursor.absolute_offset(offset)),
                    context: Some(parsed.path.clone()),
                });
            }
            b"uuid" => {
                parse_uuid(cursor, &parsed, payloads, issues);
            }
            _ => {}
        }

        offset = parsed.end;
    }

    Ok(())
}

fn parse_box_header(
    cursor: &Cursor<'_>,
    offset: usize,
    parent_end: usize,
    parent: Option<&ParsedBox>,
    issues: &mut Vec<Issue>,
) -> Result<Option<ParsedBox>, XiftyError> {
    if offset + 8 > parent_end {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "isobmff_box_header_out_of_bounds".into(),
            message: "truncated isobmff box header".into(),
            offset: Some(cursor.absolute_offset(offset)),
            context: None,
        });
        return Ok(None);
    }

    let size32 = cursor.read_u32(offset, Endian::Big)? as u64;
    let type_slice = cursor.slice(offset + 4, 4)?;
    let box_type = [type_slice[0], type_slice[1], type_slice[2], type_slice[3]];
    let mut header_size = 8usize;
    let box_size = if size32 == 1 {
        if offset + 16 > parent_end {
            issues.push(Issue {
                severity: Severity::Warning,
                code: "isobmff_large_size_header_truncated".into(),
                message: "large-size box header truncated".into(),
                offset: Some(cursor.absolute_offset(offset)),
                context: Some(fourcc(box_type)),
            });
            return Ok(None);
        }
        header_size = 16;
        let high = cursor.read_u32(offset + 8, Endian::Big)? as u64;
        let low = cursor.read_u32(offset + 12, Endian::Big)? as u64;
        (high << 32) | low
    } else if size32 == 0 {
        (parent_end - offset) as u64
    } else {
        size32
    };

    if box_size < header_size as u64 {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "isobmff_box_size_invalid".into(),
            message: format!(
                "box {} declared invalid size {}",
                fourcc(box_type),
                box_size
            ),
            offset: Some(cursor.absolute_offset(offset)),
            context: Some(fourcc(box_type)),
        });
        return Ok(None);
    }

    let end = offset.saturating_add(box_size as usize);
    if end > parent_end {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "isobmff_box_size_out_of_bounds".into(),
            message: format!("box {} exceeds containing bounds", fourcc(box_type)),
            offset: Some(cursor.absolute_offset(offset)),
            context: Some(fourcc(box_type)),
        });
        return Ok(None);
    }

    let path = if let Some(parent) = parent {
        format!("{}/{}", parent.path, fourcc(box_type))
    } else {
        fourcc(box_type)
    };

    Ok(Some(ParsedBox {
        start: offset,
        box_type,
        data_offset: offset + header_size,
        end,
        path,
    }))
}

fn parse_ftyp(
    cursor: &Cursor<'_>,
    parsed: &ParsedBox,
    major_brand: &mut [u8; 4],
    compatible_brands: &mut Vec<[u8; 4]>,
) -> Result<(), XiftyError> {
    if parsed.data_offset + 8 > parsed.end {
        return Err(XiftyError::Parse {
            message: "ftyp box too small".into(),
        });
    }
    let major = cursor.slice(parsed.data_offset, 4)?;
    *major_brand = [major[0], major[1], major[2], major[3]];
    compatible_brands.clear();
    let mut offset = parsed.data_offset + 8;
    while offset + 4 <= parsed.end {
        let brand = cursor.slice(offset, 4)?;
        compatible_brands.push([brand[0], brand[1], brand[2], brand[3]]);
        offset += 4;
    }
    Ok(())
}

fn parse_iinf(
    cursor: &Cursor<'_>,
    parsed: &ParsedBox,
    _state: &mut ParseState,
    issues: &mut Vec<Issue>,
) -> Result<usize, XiftyError> {
    if parsed.data_offset + 6 > parsed.end {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "isobmff_iinf_truncated".into(),
            message: "iinf box is truncated".into(),
            offset: Some(cursor.absolute_offset(parsed.data_offset)),
            context: Some(parsed.path.clone()),
        });
        return Ok(parsed.end);
    }
    let version = cursor.read_u8(parsed.data_offset)?;
    Ok(parsed.data_offset + if version == 0 { 6 } else { 8 })
}

fn parse_infe(
    cursor: &Cursor<'_>,
    parsed: &ParsedBox,
    state: &mut ParseState,
    issues: &mut Vec<Issue>,
) -> Result<(), XiftyError> {
    if parsed.data_offset + 4 > parsed.end {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "isobmff_infe_truncated".into(),
            message: "infe box is truncated".into(),
            offset: Some(cursor.absolute_offset(parsed.data_offset)),
            context: Some(parsed.path.clone()),
        });
        return Ok(());
    }
    let version = cursor.read_u8(parsed.data_offset)?;
    if version < 2 {
        return Ok(());
    }

    let mut offset = parsed.data_offset + 4;
    let item_id = cursor.read_u16(offset, Endian::Big)? as u32;
    offset += 2;
    offset += 2; // item_protection_index
    let item_type_bytes = cursor.slice(offset, 4)?;
    let item_type = [
        item_type_bytes[0],
        item_type_bytes[1],
        item_type_bytes[2],
        item_type_bytes[3],
    ];
    offset += 4;

    let payload = cursor.bytes().get(offset..parsed.end).unwrap_or_default();
    let mut parts = payload.split(|byte| *byte == 0);
    let _name = parts.next().unwrap_or_default();
    let content_type = if &item_type == b"mime" {
        parts
            .next()
            .and_then(|value| std::str::from_utf8(value).ok())
            .map(str::to_string)
    } else {
        None
    };

    state.item_infos.push(ItemInfo {
        item_id,
        item_type,
        content_type,
    });
    Ok(())
}

fn parse_pitm(
    cursor: &Cursor<'_>,
    parsed: &ParsedBox,
    state: &mut ParseState,
    issues: &mut Vec<Issue>,
) -> Result<(), XiftyError> {
    if parsed.data_offset + 6 > parsed.end {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "isobmff_pitm_truncated".into(),
            message: "pitm box is truncated".into(),
            offset: Some(cursor.absolute_offset(parsed.data_offset)),
            context: Some(parsed.path.clone()),
        });
        return Ok(());
    }
    let version = cursor.read_u8(parsed.data_offset)?;
    let item_id = if version == 0 {
        cursor.read_u16(parsed.data_offset + 4, Endian::Big)? as u32
    } else {
        cursor.read_u32(parsed.data_offset + 4, Endian::Big)?
    };
    state.primary_item_id = Some(item_id);
    Ok(())
}

fn parse_iloc(
    cursor: &Cursor<'_>,
    parsed: &ParsedBox,
    state: &mut ParseState,
    issues: &mut Vec<Issue>,
) -> Result<(), XiftyError> {
    if parsed.data_offset + 8 > parsed.end {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "isobmff_iloc_truncated".into(),
            message: "iloc box is truncated".into(),
            offset: Some(cursor.absolute_offset(parsed.data_offset)),
            context: Some(parsed.path.clone()),
        });
        return Ok(());
    }
    let version = cursor.read_u8(parsed.data_offset)?;
    let sizes = cursor.read_u8(parsed.data_offset + 4)?;
    let offset_size = (sizes >> 4) as usize;
    let length_size = (sizes & 0x0F) as usize;
    let base_and_index = cursor.read_u8(parsed.data_offset + 5)?;
    let base_offset_size = (base_and_index >> 4) as usize;
    let index_size = if version >= 1 {
        (base_and_index & 0x0F) as usize
    } else {
        0
    };
    let mut offset = parsed.data_offset + 6;
    let item_count = if version < 2 {
        cursor.read_u16(offset, Endian::Big)? as u32
    } else {
        cursor.read_u32(offset, Endian::Big)?
    };
    offset += if version < 2 { 2 } else { 4 };

    for _ in 0..item_count {
        let item_id = if version < 2 {
            let value = cursor.read_u16(offset, Endian::Big)? as u32;
            offset += 2;
            value
        } else {
            let value = cursor.read_u32(offset, Endian::Big)?;
            offset += 4;
            value
        };
        let construction_method = if version == 1 || version == 2 {
            let value = cursor.read_u16(offset, Endian::Big)? & 0x000F;
            offset += 2;
            value
        } else {
            0
        };
        offset += 2; // data_reference_index
        let base_offset = read_sized_uint(cursor, &mut offset, base_offset_size)?;
        let extent_count = cursor.read_u16(offset, Endian::Big)? as usize;
        offset += 2;

        let mut extents = Vec::new();
        for _ in 0..extent_count {
            if (version == 1 || version == 2) && index_size > 0 {
                let _ = read_sized_uint(cursor, &mut offset, index_size)?;
            }
            let extent_offset = read_sized_uint(cursor, &mut offset, offset_size)?;
            let extent_length = read_sized_uint(cursor, &mut offset, length_size)?;
            extents.push(ItemExtent {
                offset: extent_offset,
                length: extent_length,
            });
        }

        state.item_locations.push(ItemLocation {
            item_id,
            construction_method,
            base_offset,
            extents,
        });
    }

    Ok(())
}

fn parse_ipma(
    cursor: &Cursor<'_>,
    parsed: &ParsedBox,
    state: &mut ParseState,
    issues: &mut Vec<Issue>,
) -> Result<(), XiftyError> {
    if parsed.data_offset + 8 > parsed.end {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "isobmff_ipma_truncated".into(),
            message: "ipma box is truncated".into(),
            offset: Some(cursor.absolute_offset(parsed.data_offset)),
            context: Some(parsed.path.clone()),
        });
        return Ok(());
    }
    let version = cursor.read_u8(parsed.data_offset)?;
    let flags = ((cursor.read_u8(parsed.data_offset + 1)? as u32) << 16)
        | ((cursor.read_u8(parsed.data_offset + 2)? as u32) << 8)
        | (cursor.read_u8(parsed.data_offset + 3)? as u32);
    let association_16 = (flags & 1) != 0;
    let mut offset = parsed.data_offset + 4;
    let entry_count = cursor.read_u32(offset, Endian::Big)? as usize;
    offset += 4;

    for _ in 0..entry_count {
        let item_id = if version < 1 {
            let value = cursor.read_u16(offset, Endian::Big)? as u32;
            offset += 2;
            value
        } else {
            let value = cursor.read_u32(offset, Endian::Big)?;
            offset += 4;
            value
        };
        let association_count = cursor.read_u8(offset)? as usize;
        offset += 1;
        let mut property_indexes = Vec::new();
        for _ in 0..association_count {
            let value = if association_16 {
                let raw = cursor.read_u16(offset, Endian::Big)?;
                offset += 2;
                raw & 0x7FFF
            } else {
                let raw = cursor.read_u8(offset)? as u16;
                offset += 1;
                raw & 0x7F
            };
            if value != 0 {
                property_indexes.push(value);
            }
        }
        state.property_associations.push(PropertyAssociation {
            item_id,
            property_indexes,
        });
    }

    Ok(())
}

fn parse_ispe(
    cursor: &Cursor<'_>,
    parsed: &ParsedBox,
    state: &mut ParseState,
    issues: &mut Vec<Issue>,
) -> Result<(), XiftyError> {
    if parsed.data_offset + 12 > parsed.end {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "isobmff_ispe_truncated".into(),
            message: "ispe box is truncated".into(),
            offset: Some(cursor.absolute_offset(parsed.data_offset)),
            context: Some(parsed.path.clone()),
        });
        return Ok(());
    }
    let width = cursor.read_u32(parsed.data_offset + 4, Endian::Big)?;
    let height = cursor.read_u32(parsed.data_offset + 8, Endian::Big)?;
    state.property_count += 1;
    let property_index = state.property_count;
    state.property_dimensions.insert(
        property_index,
        IsobmffDimensions {
            width,
            height,
            offset_start: cursor.absolute_offset(parsed.start),
            offset_end: cursor.absolute_offset(parsed.end),
            path: parsed.path.clone(),
        },
    );
    Ok(())
}

fn parse_mvhd(
    cursor: &Cursor<'_>,
    parsed: &ParsedBox,
    state: &mut ParseState,
    issues: &mut Vec<Issue>,
) -> Result<(), XiftyError> {
    let Some(header) = read_time_header(cursor, parsed, issues, "mvhd")? else {
        return Ok(());
    };
    state.movie_header.created_at = qt_seconds_to_iso_string(header.created_at);
    state.movie_header.modified_at = qt_seconds_to_iso_string(header.modified_at);
    state.movie_header.duration_seconds = duration_seconds(header.timescale, header.duration);
    Ok(())
}

fn parse_tkhd(
    cursor: &Cursor<'_>,
    parsed: &ParsedBox,
    track_key: Option<&str>,
    state: &mut ParseState,
    _issues: &mut Vec<Issue>,
) -> Result<(), XiftyError> {
    if parsed.end < parsed.data_offset + 8 {
        return Ok(());
    }
    let width_raw = cursor.read_u32(parsed.end - 8, Endian::Big)?;
    let height_raw = cursor.read_u32(parsed.end - 4, Endian::Big)?;
    let width = width_raw >> 16;
    let height = height_raw >> 16;
    if width == 0 || height == 0 {
        return Ok(());
    }
    if let Some(key) = track_key {
        state.tracks.entry(key.to_string()).or_default().dimensions = Some((width, height));
    }
    Ok(())
}

fn parse_mdhd(
    cursor: &Cursor<'_>,
    parsed: &ParsedBox,
    track_key: Option<&str>,
    state: &mut ParseState,
    issues: &mut Vec<Issue>,
) -> Result<(), XiftyError> {
    let Some(header) = read_time_header(cursor, parsed, issues, "mdhd")? else {
        return Ok(());
    };
    if let Some(key) = track_key {
        let track = state.tracks.entry(key.to_string()).or_default();
        track.timescale = Some(header.timescale);
        track.duration_seconds = duration_seconds(header.timescale, header.duration);
    }
    Ok(())
}

fn parse_hdlr(
    cursor: &Cursor<'_>,
    parsed: &ParsedBox,
    track_key: Option<&str>,
    state: &mut ParseState,
    _issues: &mut Vec<Issue>,
) -> Result<(), XiftyError> {
    if parsed.data_offset + 12 > parsed.end {
        return Ok(());
    }
    let handler = cursor.slice(parsed.data_offset + 8, 4)?;
    let handler_type = [handler[0], handler[1], handler[2], handler[3]];
    if let Some(key) = track_key {
        state
            .tracks
            .entry(key.to_string())
            .or_default()
            .handler_type = Some(handler_type);
    }
    Ok(())
}

fn parse_stsd(
    cursor: &Cursor<'_>,
    parsed: &ParsedBox,
    track_key: Option<&str>,
    state: &mut ParseState,
    _issues: &mut Vec<Issue>,
) -> Result<(), XiftyError> {
    if parsed.data_offset + 16 > parsed.end {
        return Ok(());
    }
    let entry_count = cursor.read_u32(parsed.data_offset + 4, Endian::Big)? as usize;
    if entry_count == 0 || parsed.data_offset + 24 > parsed.end {
        return Ok(());
    }
    let sample_entry_offset = parsed.data_offset + 8;
    let sample_entry_size = cursor.read_u32(sample_entry_offset, Endian::Big)? as usize;
    if sample_entry_size < 16 || sample_entry_offset + sample_entry_size > parsed.end {
        return Ok(());
    }
    let sample_type = cursor.slice(sample_entry_offset + 4, 4)?;
    let codec = String::from_utf8_lossy(sample_type).into_owned();
    if let Some(key) = track_key {
        let track = state.tracks.entry(key.to_string()).or_default();
        track.codec = Some(codec);
        let sample_data_offset = sample_entry_offset + 8;
        let sample_data_end = sample_entry_offset + sample_entry_size;
        parse_sample_entry_details(
            cursor,
            sample_type,
            sample_data_offset,
            sample_data_end,
            track,
        )?;
    }
    Ok(())
}

fn parse_stts(
    cursor: &Cursor<'_>,
    parsed: &ParsedBox,
    track_key: Option<&str>,
    state: &mut ParseState,
    _issues: &mut Vec<Issue>,
) -> Result<(), XiftyError> {
    if parsed.data_offset + 16 > parsed.end {
        return Ok(());
    }
    let entry_count = cursor.read_u32(parsed.data_offset + 4, Endian::Big)? as usize;
    if entry_count == 0 {
        return Ok(());
    }
    let sample_count = cursor.read_u32(parsed.data_offset + 8, Endian::Big)?;
    let sample_delta = cursor.read_u32(parsed.data_offset + 12, Endian::Big)?;
    if sample_count == 0 || sample_delta == 0 {
        return Ok(());
    }
    if let Some(key) = track_key {
        if let Some(track) = state.tracks.get_mut(key) {
            if track.handler_type == Some(*b"vide") {
                if let Some(timescale) = track.timescale {
                    track.frame_rate = Some(timescale as f64 / sample_delta as f64);
                }
            }
        }
    }
    Ok(())
}

fn parse_sample_entry_details(
    cursor: &Cursor<'_>,
    sample_type: &[u8],
    data_offset: usize,
    data_end: usize,
    track: &mut TrackFacts,
) -> Result<(), XiftyError> {
    match sample_type {
        b"avc1" | b"hvc1" | b"hev1" | b"av01" => {
            parse_visual_sample_entry(cursor, data_offset, data_end, track)
        }
        b"mp4a" | b"ac-3" | b"ec-3" | b"alac" | b"twos" | b"sowt" | b"lpcm" | b"in24" | b"in32"
        | b"fl32" | b"fl64" | b"ulaw" | b"alaw" => {
            parse_audio_sample_entry(cursor, data_offset, data_end, track)
        }
        _ => Ok(()),
    }
}

fn parse_visual_sample_entry(
    cursor: &Cursor<'_>,
    data_offset: usize,
    data_end: usize,
    track: &mut TrackFacts,
) -> Result<(), XiftyError> {
    if data_offset + 78 > data_end {
        return Ok(());
    }
    let mut child_offset = data_offset + 78;
    while child_offset + 8 <= data_end {
        let child_size = cursor.read_u32(child_offset, Endian::Big)? as usize;
        if child_size < 8 || child_offset + child_size > data_end {
            break;
        }
        let child_type = cursor.slice(child_offset + 4, 4)?;
        if child_type == b"btrt" && child_size >= 20 {
            let avg_bitrate = cursor.read_u32(child_offset + 16, Endian::Big)?;
            if avg_bitrate != 0 {
                track.bitrate = Some(avg_bitrate);
            }
        }
        child_offset += child_size;
    }
    Ok(())
}

fn parse_audio_sample_entry(
    cursor: &Cursor<'_>,
    data_offset: usize,
    data_end: usize,
    track: &mut TrackFacts,
) -> Result<(), XiftyError> {
    if data_offset + 28 > data_end {
        return Ok(());
    }
    let channels = cursor.read_u16(data_offset + 16, Endian::Big)?;
    let sample_rate_fixed = cursor.read_u32(data_offset + 24, Endian::Big)?;
    if channels != 0 {
        track.audio_channels = Some(channels);
    }
    let sample_rate = sample_rate_fixed >> 16;
    if sample_rate != 0 {
        track.audio_sample_rate = Some(sample_rate);
    }
    Ok(())
}

fn parse_stsz(
    cursor: &Cursor<'_>,
    parsed: &ParsedBox,
    track_key: Option<&str>,
    state: &mut ParseState,
    _issues: &mut Vec<Issue>,
) -> Result<(), XiftyError> {
    if parsed.data_offset + 12 > parsed.end {
        return Ok(());
    }
    let sample_size = cursor.read_u32(parsed.data_offset + 4, Endian::Big)?;
    let sample_count = cursor.read_u32(parsed.data_offset + 8, Endian::Big)? as usize;
    let Some(key) = track_key else {
        return Ok(());
    };
    let track = state.tracks.entry(key.to_string()).or_default();

    if sample_size != 0 {
        track.total_sample_bytes = sample_size as u64 * sample_count as u64;
        return Ok(());
    }

    let table_start = parsed.data_offset + 12;
    let table_bytes = sample_count.saturating_mul(4);
    if table_start + table_bytes > parsed.end {
        return Ok(());
    }

    let mut total = 0u64;
    for index in 0..sample_count {
        let sample = cursor.read_u32(table_start + (index * 4), Endian::Big)? as u64;
        total = total.saturating_add(sample);
    }
    track.total_sample_bytes = total;
    Ok(())
}

fn derive_track_bitrate(track: &TrackFacts) -> Option<u32> {
    let duration = track.duration_seconds?;
    if duration <= 0.0 || track.total_sample_bytes == 0 {
        return None;
    }
    let bits_per_second = ((track.total_sample_bytes as f64 * 8.0) / duration).round();
    if !(1.0..=u32::MAX as f64).contains(&bits_per_second) {
        return None;
    }
    Some(bits_per_second as u32)
}

fn parse_itunes_item(cursor: &Cursor<'_>, parsed: &ParsedBox) -> Option<IsobmffPayload> {
    let payload = cursor.bytes().get(parsed.data_offset..parsed.end)?;
    if payload.len() < 16 || payload.get(4..8)? != b"data" {
        return None;
    }
    let tag = itunes_tag_label(parsed.box_type);
    Some(IsobmffPayload {
        kind: "itunes",
        tag: Some(tag),
        offset_start: cursor.absolute_offset(parsed.start),
        offset_end: cursor.absolute_offset(parsed.end),
        data_offset: cursor.absolute_offset(parsed.data_offset),
        data_length: (parsed.end - parsed.data_offset) as u64,
        path: parsed.path.clone(),
    })
}

/// Produce a canonical iTunes atom label, mapping the leading 0xA9 "©"
/// prefix byte (Latin-1 copyright sign) to the UTF-8 copyright code point
/// so downstream consumers can match on stable strings.
fn itunes_tag_label(box_type: [u8; 4]) -> String {
    if box_type[0] == 0xA9 {
        let mut out = String::with_capacity(6);
        out.push('\u{a9}');
        out.push_str(&String::from_utf8_lossy(&box_type[1..]));
        out
    } else {
        fourcc(box_type)
    }
}

/// Surface a direct-`udta` ©-prefixed atom as a `quicktime-udta` payload.
///
/// We do **not** validate the body shape here — DJI firmware ships at least
/// two flavours: classic `{u16 length}{u16 language}{ascii}` (©fpt/©xyz/…)
/// and raw null-padded ASCII (©mdl/©csn on FC3682). Decoding lives in
/// `xifty-meta-quicktime` which tries both shapes. We only reject the
/// iTunes `data`-sub-box shape so `udta/meta/ilst/©foo` keeps routing to
/// `parse_itunes_item` without double-emit.
fn parse_quicktime_udta_text(cursor: &Cursor<'_>, parsed: &ParsedBox) -> Option<IsobmffPayload> {
    let payload = cursor.bytes().get(parsed.data_offset..parsed.end)?;
    if payload.is_empty() {
        return None;
    }
    if payload.len() >= 8 && payload.get(4..8) == Some(b"data") {
        return None;
    }
    Some(IsobmffPayload {
        kind: "quicktime-udta",
        tag: Some(itunes_tag_label(parsed.box_type)),
        offset_start: cursor.absolute_offset(parsed.start),
        offset_end: cursor.absolute_offset(parsed.end),
        data_offset: cursor.absolute_offset(parsed.data_offset),
        data_length: (parsed.end - parsed.data_offset) as u64,
        path: parsed.path.clone(),
    })
}

fn parse_quicktime_item(cursor: &Cursor<'_>, parsed: &ParsedBox) -> Option<IsobmffPayload> {
    let payload = cursor.bytes().get(parsed.data_offset..parsed.end)?;
    if payload.len() < 16 || payload.get(4..8)? != b"data" {
        return None;
    }
    let tag = quicktime_tag_name(parsed.box_type)?;
    Some(IsobmffPayload {
        kind: "quicktime",
        tag: Some(tag.to_string()),
        offset_start: cursor.absolute_offset(parsed.start),
        offset_end: cursor.absolute_offset(parsed.end),
        data_offset: cursor.absolute_offset(parsed.data_offset),
        data_length: (parsed.end - parsed.data_offset) as u64,
        path: parsed.path.clone(),
    })
}

#[derive(Debug, Clone, Copy)]
struct TimeHeader {
    created_at: u64,
    modified_at: u64,
    timescale: u32,
    duration: u64,
}

fn read_time_header(
    cursor: &Cursor<'_>,
    parsed: &ParsedBox,
    issues: &mut Vec<Issue>,
    box_name: &str,
) -> Result<Option<TimeHeader>, XiftyError> {
    if parsed.data_offset + 20 > parsed.end {
        issues.push(Issue {
            severity: Severity::Warning,
            code: format!("isobmff_{box_name}_truncated"),
            message: format!("{box_name} box is truncated"),
            offset: Some(cursor.absolute_offset(parsed.data_offset)),
            context: Some(parsed.path.clone()),
        });
        return Ok(None);
    }
    let version = cursor.read_u8(parsed.data_offset)?;
    let header = if version == 1 {
        if parsed.data_offset + 32 > parsed.end {
            return Ok(None);
        }
        TimeHeader {
            created_at: read_u64_be(cursor, parsed.data_offset + 4)?,
            modified_at: read_u64_be(cursor, parsed.data_offset + 12)?,
            timescale: cursor.read_u32(parsed.data_offset + 20, Endian::Big)?,
            duration: read_u64_be(cursor, parsed.data_offset + 24)?,
        }
    } else {
        TimeHeader {
            created_at: cursor.read_u32(parsed.data_offset + 4, Endian::Big)? as u64,
            modified_at: cursor.read_u32(parsed.data_offset + 8, Endian::Big)? as u64,
            timescale: cursor.read_u32(parsed.data_offset + 12, Endian::Big)?,
            duration: cursor.read_u32(parsed.data_offset + 16, Endian::Big)? as u64,
        }
    };
    Ok(Some(header))
}

fn read_u64_be(cursor: &Cursor<'_>, offset: usize) -> Result<u64, XiftyError> {
    let high = cursor.read_u32(offset, Endian::Big)? as u64;
    let low = cursor.read_u32(offset + 4, Endian::Big)? as u64;
    Ok((high << 32) | low)
}

fn duration_seconds(timescale: u32, duration: u64) -> Option<f64> {
    if timescale == 0 {
        return None;
    }
    Some(duration as f64 / timescale as f64)
}

fn primary_visual_dimensions(state: &ParseState) -> Option<IsobmffDimensions> {
    let (path, track) = state
        .tracks
        .iter()
        .find(|(_, track)| track.handler_type == Some(*b"vide") && track.dimensions.is_some())?;
    let (width, height) = track.dimensions?;
    Some(IsobmffDimensions {
        width,
        height,
        offset_start: 0,
        offset_end: 0,
        path: format!("{path}/tkhd"),
    })
}

fn quicktime_tag_name(box_type: [u8; 4]) -> Option<&'static str> {
    match &box_type {
        b"\xa9ART" => Some("author"),
        b"\xa9too" => Some("software"),
        b"\xa9nam" => Some("title"),
        _ => None,
    }
}

fn qt_seconds_to_iso_string(seconds: u64) -> Option<String> {
    const QT_TO_UNIX: i128 = 2_082_844_800;
    let unix = i128::from(seconds) - QT_TO_UNIX;
    if unix < 0 {
        return None;
    }

    let days = unix / 86_400;
    let seconds_of_day = (unix % 86_400) as i64;
    let (year, month, day) = civil_from_days(days as i64)?;
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;

    Some(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}"
    ))
}

fn civil_from_days(days_since_unix_epoch: i64) -> Option<(i32, u32, u32)> {
    let z = days_since_unix_epoch.checked_add(719_468)?;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if month <= 2 { 1 } else { 0 };

    Some((
        i32::try_from(year).ok()?,
        u32::try_from(month).ok()?,
        u32::try_from(day).ok()?,
    ))
}

fn payloads_from_items(
    cursor: &Cursor<'_>,
    state: &ParseState,
    issues: &mut Vec<Issue>,
) -> Vec<IsobmffPayload> {
    let mut payloads = Vec::new();

    for info in &state.item_infos {
        let Some(kind) = metadata_item_kind(info) else {
            continue;
        };
        let Some(location) = state
            .item_locations
            .iter()
            .find(|location| location.item_id == info.item_id)
        else {
            continue;
        };
        if location.extents.len() != 1 {
            issues.push(Issue {
                severity: Severity::Info,
                code: "isobmff_item_multi_extent_unsupported".into(),
                message: format!(
                    "metadata item {} uses {} extents; only single-extent routing is supported",
                    info.item_id,
                    location.extents.len()
                ),
                offset: None,
                context: Some(format!("item {}", info.item_id)),
            });
            continue;
        }

        let extent = &location.extents[0];
        let absolute_data_offset = match location.construction_method {
            0 => location.base_offset + extent.offset,
            1 => {
                let Some((idat_offset, _, path)) = state.idat_payloads.first() else {
                    issues.push(Issue {
                        severity: Severity::Info,
                        code: "isobmff_item_idat_missing".into(),
                        message: format!(
                            "metadata item {} requires idat construction, but no idat payload was found",
                            info.item_id
                        ),
                        offset: None,
                        context: Some(format!("item {}", info.item_id)),
                    });
                    continue;
                };
                let _ = path;
                idat_offset + location.base_offset + extent.offset
            }
            method => {
                issues.push(Issue {
                    severity: Severity::Info,
                    code: "isobmff_item_construction_unsupported".into(),
                    message: format!(
                        "metadata item {} uses unsupported construction method {}",
                        info.item_id, method
                    ),
                    offset: None,
                    context: Some(format!("item {}", info.item_id)),
                });
                continue;
            }
        };

        let Ok(start) = usize::try_from(absolute_data_offset) else {
            continue;
        };
        let Ok(length) = usize::try_from(extent.length) else {
            continue;
        };
        if cursor.bytes().get(start..start + length).is_none() {
            issues.push(Issue {
                severity: Severity::Warning,
                code: "isobmff_item_payload_out_of_bounds".into(),
                message: format!("metadata item {} points outside file bounds", info.item_id),
                offset: Some(absolute_data_offset),
                context: Some(format!("item {}", info.item_id)),
            });
            continue;
        }

        payloads.push(IsobmffPayload {
            kind,
            tag: None,
            offset_start: absolute_data_offset,
            offset_end: absolute_data_offset + extent.length,
            data_offset: absolute_data_offset,
            data_length: extent.length,
            path: format!("meta/item[{}/{}]", info.item_id, fourcc(info.item_type)),
        });
    }

    payloads
}

fn metadata_item_kind(info: &ItemInfo) -> Option<&'static str> {
    if &info.item_type == b"Exif" {
        return Some("exif");
    }
    if &info.item_type == b"iptc" {
        return Some("iptc");
    }
    if &info.item_type == b"mime" {
        return match info.content_type.as_deref() {
            Some("application/rdf+xml" | "application/xml" | "text/xml") => Some("xmp"),
            Some("application/x-iptc") => Some("iptc"),
            _ => None,
        };
    }
    None
}

fn parse_colr(
    cursor: &Cursor<'_>,
    parsed: &ParsedBox,
    state: &mut ParseState,
    payloads: &mut Vec<IsobmffPayload>,
    issues: &mut Vec<Issue>,
) {
    if parsed.data_offset + 4 > parsed.end {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "isobmff_colr_truncated".into(),
            message: "colr box is missing colour_type header".into(),
            offset: Some(cursor.absolute_offset(parsed.data_offset)),
            context: Some(parsed.path.clone()),
        });
        return;
    }
    // `colr` always consumes a property index in `ipco` regardless of payload
    // type, so `ipma` references stay aligned with document order.
    state.property_count += 1;
    let property_index = state.property_count;
    let Ok(colour_type_bytes) = cursor.slice(parsed.data_offset, 4) else {
        return;
    };
    let colour_type = [
        colour_type_bytes[0],
        colour_type_bytes[1],
        colour_type_bytes[2],
        colour_type_bytes[3],
    ];
    if &colour_type == b"prof" || &colour_type == b"rICC" {
        let icc_offset = parsed.data_offset + 4;
        if icc_offset >= parsed.end {
            return;
        }
        payloads.push(IsobmffPayload {
            kind: "icc",
            tag: None,
            offset_start: cursor.absolute_offset(parsed.start),
            offset_end: cursor.absolute_offset(parsed.end),
            data_offset: cursor.absolute_offset(icc_offset),
            data_length: (parsed.end - icc_offset) as u64,
            path: parsed.path.clone(),
        });
        return;
    }
    if &colour_type == b"nclx" || &colour_type == b"nclc" {
        // nclx layout (after 4-byte colour_type):
        //   colour_primaries (u16) +
        //   transfer_characteristics (u16) +
        //   matrix_coefficients (u16) +
        //   full_range_flag (1 bit, top of next byte) — nclc omits the flag.
        if parsed.data_offset + 10 > parsed.end {
            issues.push(Issue {
                severity: Severity::Warning,
                code: "isobmff_colr_nclx_truncated".into(),
                message: "colr nclx/nclc payload is truncated".into(),
                offset: Some(cursor.absolute_offset(parsed.data_offset)),
                context: Some(parsed.path.clone()),
            });
            return;
        }
        let Ok(primaries) = cursor.read_u16(parsed.data_offset + 4, Endian::Big) else {
            return;
        };
        let Ok(transfer) = cursor.read_u16(parsed.data_offset + 6, Endian::Big) else {
            return;
        };
        let Ok(matrix) = cursor.read_u16(parsed.data_offset + 8, Endian::Big) else {
            return;
        };
        let full_range = if &colour_type == b"nclx" {
            cursor
                .read_u8(parsed.data_offset + 10)
                .ok()
                .map(|byte| (byte & 0x80) != 0)
        } else {
            None
        };
        state.property_colors.insert(
            property_index,
            IsobmffColorInfo {
                primaries: Some(primaries),
                transfer: Some(transfer),
                matrix: Some(matrix),
                full_range,
                source: "colr_nclx",
                offset_start: cursor.absolute_offset(parsed.start),
                offset_end: cursor.absolute_offset(parsed.end),
                path: parsed.path.clone(),
            },
        );
    }
}

fn parse_cicp(
    cursor: &Cursor<'_>,
    parsed: &ParsedBox,
    state: &mut ParseState,
    issues: &mut Vec<Issue>,
) {
    // cicp full-box layout (after 4-byte version+flags):
    //   colour_primaries (u8) + transfer_characteristics (u8) +
    //   matrix_coefficients (u8) + full_range_flag (top bit of next byte)
    state.property_count += 1;
    let property_index = state.property_count;
    if parsed.data_offset + 8 > parsed.end {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "isobmff_cicp_truncated".into(),
            message: "cicp box is truncated".into(),
            offset: Some(cursor.absolute_offset(parsed.data_offset)),
            context: Some(parsed.path.clone()),
        });
        return;
    }
    let Ok(primaries) = cursor.read_u8(parsed.data_offset + 4) else {
        return;
    };
    let Ok(transfer) = cursor.read_u8(parsed.data_offset + 5) else {
        return;
    };
    let Ok(matrix) = cursor.read_u8(parsed.data_offset + 6) else {
        return;
    };
    let Ok(range_byte) = cursor.read_u8(parsed.data_offset + 7) else {
        return;
    };
    state.property_colors.insert(
        property_index,
        IsobmffColorInfo {
            primaries: Some(primaries as u16),
            transfer: Some(transfer as u16),
            matrix: Some(matrix as u16),
            full_range: Some((range_byte & 0x80) != 0),
            source: "cicp",
            offset_start: cursor.absolute_offset(parsed.start),
            offset_end: cursor.absolute_offset(parsed.end),
            path: parsed.path.clone(),
        },
    );
}

fn parse_pixi(
    cursor: &Cursor<'_>,
    parsed: &ParsedBox,
    state: &mut ParseState,
    issues: &mut Vec<Issue>,
) {
    // pixi full-box layout (after 4-byte version+flags):
    //   num_channels (u8) + bits_per_channel[num_channels] (u8 each)
    state.property_count += 1;
    let property_index = state.property_count;
    if parsed.data_offset + 5 > parsed.end {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "isobmff_pixi_truncated".into(),
            message: "pixi box is truncated".into(),
            offset: Some(cursor.absolute_offset(parsed.data_offset)),
            context: Some(parsed.path.clone()),
        });
        return;
    }
    let Ok(num_channels) = cursor.read_u8(parsed.data_offset + 4) else {
        return;
    };
    let count = num_channels as usize;
    let bits_offset = parsed.data_offset + 5;
    if bits_offset + count > parsed.end {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "isobmff_pixi_truncated".into(),
            message: "pixi bits_per_channel array is truncated".into(),
            offset: Some(cursor.absolute_offset(parsed.data_offset)),
            context: Some(parsed.path.clone()),
        });
        return;
    }
    let Ok(bits_slice) = cursor.slice(bits_offset, count) else {
        return;
    };
    state.property_pixels.insert(
        property_index,
        IsobmffPixelInfo {
            bits_per_channel: bits_slice.to_vec(),
            offset_start: cursor.absolute_offset(parsed.start),
            offset_end: cursor.absolute_offset(parsed.end),
            path: parsed.path.clone(),
        },
    );
}

fn primary_item_dimensions(state: &ParseState) -> Option<IsobmffDimensions> {
    let item_id = state.primary_item_id?;
    let associations = state
        .property_associations
        .iter()
        .find(|association| association.item_id == item_id)?;
    for property_index in &associations.property_indexes {
        if let Some(dimensions) = state.property_dimensions.get(property_index) {
            return Some(dimensions.clone());
        }
    }
    None
}

/// Walk the primary item's property associations and return the most
/// authoritative colour info entry, applying the cicp > colr/nclx
/// precedence policy. (Pixi is reported separately via
/// `primary_item_pixel` because it carries bit depths, not a colour
/// description.)
fn primary_item_color(state: &ParseState) -> Option<IsobmffColorInfo> {
    let item_id = state.primary_item_id?;
    let associations = state
        .property_associations
        .iter()
        .find(|association| association.item_id == item_id)?;
    let mut cicp = None;
    let mut nclx = None;
    for property_index in &associations.property_indexes {
        if let Some(info) = state.property_colors.get(property_index) {
            match info.source {
                "cicp" if cicp.is_none() => cicp = Some(info.clone()),
                "colr_nclx" if nclx.is_none() => nclx = Some(info.clone()),
                _ => {}
            }
        }
    }
    cicp.or(nclx)
}

fn primary_item_pixel(state: &ParseState) -> Option<IsobmffPixelInfo> {
    let item_id = state.primary_item_id?;
    let associations = state
        .property_associations
        .iter()
        .find(|association| association.item_id == item_id)?;
    for property_index in &associations.property_indexes {
        if let Some(pixel) = state.property_pixels.get(property_index) {
            return Some(pixel.clone());
        }
    }
    None
}

fn read_sized_uint(
    cursor: &Cursor<'_>,
    offset: &mut usize,
    size: usize,
) -> Result<u64, XiftyError> {
    if size == 0 {
        return Ok(0);
    }
    if size > 8 {
        return Err(XiftyError::Parse {
            message: format!("unsupported integer size {size} in isobmff parser"),
        });
    }
    let bytes = cursor.slice(*offset, size)?;
    *offset += size;
    let mut value = 0u64;
    for byte in bytes {
        value = (value << 8) | u64::from(*byte);
    }
    Ok(value)
}

/// Walk a `uuid` box. When the 16-byte usertype matches the Canon CMT UUID,
/// emit one `IsobmffPayload` per CMT* sub-box (CMT1 → `kind="exif"`,
/// CMT2/3/4 → `kind="canon-cmt"`). All other UUIDs are recorded as a
/// recognised-but-uninterpreted structural info issue and skipped without
/// raising a parse error so unrelated `uuid` payloads (XMP-via-uuid, GoPro
/// telemetry, vendor blobs) keep flowing through the regular pipeline
/// untouched.
fn parse_uuid(
    cursor: &Cursor<'_>,
    parsed: &ParsedBox,
    payloads: &mut Vec<IsobmffPayload>,
    issues: &mut Vec<Issue>,
) {
    if parsed.data_offset + 16 > parsed.end {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "isobmff_uuid_usertype_truncated".into(),
            message: "uuid box is missing its 16-byte usertype".into(),
            offset: Some(cursor.absolute_offset(parsed.data_offset)),
            context: Some(parsed.path.clone()),
        });
        return;
    }
    let Ok(usertype_slice) = cursor.slice(parsed.data_offset, 16) else {
        return;
    };
    let mut usertype = [0u8; 16];
    usertype.copy_from_slice(usertype_slice);
    let inner_start = parsed.data_offset + 16;
    if usertype != CANON_CMT_UUID {
        // C2PA JUMBF manifests carried inside a `uuid` box (HEIF/AVIF/MP4/MOV
        // path). The payload after the 16-byte usertype is a contiguous
        // JUMBF byte stream that `xifty-meta-c2pa::decode_jumbf` consumes
        // directly.
        if usertype == C2PA_UUID {
            payloads.push(IsobmffPayload {
                kind: "c2pa",
                tag: Some("uuid_c2pa".into()),
                offset_start: cursor.absolute_offset(parsed.start),
                offset_end: cursor.absolute_offset(parsed.end),
                data_offset: cursor.absolute_offset(inner_start),
                data_length: (parsed.end - inner_start) as u64,
                path: format!("{}/c2pa", parsed.path),
            });
            return;
        }
        // Sony "User Data Atom UUID family": usertype tail bytes 4..16 match
        // SONY_USERDATA_UUID_TAIL and bytes 0..4 carry the atom name in
        // ASCII (PROF/USMT/PRPF/AOLY/MTDT/...). Surface as a single payload
        // per atom; downstream `xifty-meta-sony-video` dispatches by tag.
        if usertype[4..16] == SONY_USERDATA_UUID_TAIL
            && usertype[0..4]
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || *b == b' ')
        {
            let name_bytes = [usertype[0], usertype[1], usertype[2], usertype[3]];
            let atom_name = fourcc(name_bytes);
            payloads.push(IsobmffPayload {
                kind: "sony-video-atom",
                tag: Some(atom_name.clone()),
                offset_start: cursor.absolute_offset(parsed.start),
                offset_end: cursor.absolute_offset(parsed.end),
                data_offset: cursor.absolute_offset(inner_start),
                data_length: (parsed.end - inner_start) as u64,
                path: format!("{}/{}", parsed.path, atom_name),
            });
            return;
        }
        let usertype_hex = usertype
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        issues.push(Issue {
            severity: Severity::Info,
            code: "isobmff_structure_recognized_uninterpreted".into(),
            message: format!("recognized uuid box but the usertype {usertype_hex} is not handled"),
            offset: Some(cursor.absolute_offset(parsed.start)),
            context: Some(parsed.path.clone()),
        });
        return;
    }

    let mut offset = inner_start;
    while offset + 8 <= parsed.end {
        let Ok(size32) = cursor.read_u32(offset, Endian::Big) else {
            break;
        };
        let Ok(type_slice) = cursor.slice(offset + 4, 4) else {
            break;
        };
        let box_type = [type_slice[0], type_slice[1], type_slice[2], type_slice[3]];
        let box_size = size32 as usize;
        if box_size < 8 || offset + box_size > parsed.end {
            issues.push(Issue {
                severity: Severity::Warning,
                code: "isobmff_canon_cmt_truncated".into(),
                message: format!(
                    "canon uuid sub-box {} declared invalid size {}",
                    fourcc(box_type),
                    box_size
                ),
                offset: Some(cursor.absolute_offset(offset)),
                context: Some(parsed.path.clone()),
            });
            return;
        }
        let data_offset = offset + 8;
        let end = offset + box_size;
        let (kind, tag) = match &box_type {
            b"CMT1" => ("exif", "CMT1"),
            b"CMT2" => ("canon-cmt", "CMT2"),
            b"CMT3" => ("canon-cmt", "CMT3"),
            b"CMT4" => ("canon-cmt", "CMT4"),
            _ => {
                offset = end;
                continue;
            }
        };
        payloads.push(IsobmffPayload {
            kind,
            tag: Some(tag.into()),
            offset_start: cursor.absolute_offset(offset),
            offset_end: cursor.absolute_offset(end),
            data_offset: cursor.absolute_offset(data_offset),
            data_length: (end - data_offset) as u64,
            path: format!("{}/{}", parsed.path, fourcc(box_type)),
        });
        offset = end;
    }
}

fn parse_mime_payload(cursor: &Cursor<'_>, parsed: &ParsedBox) -> Option<IsobmffPayload> {
    let payload = cursor.bytes().get(parsed.data_offset..parsed.end)?;
    let nul = payload.iter().position(|byte| *byte == 0)?;
    let mime = std::str::from_utf8(&payload[..nul]).ok()?;
    if mime != "application/rdf+xml" && mime != "application/xml" && mime != "text/xml" {
        return None;
    }
    let data_offset = parsed.data_offset + nul + 1;
    if data_offset >= parsed.end {
        return None;
    }
    Some(IsobmffPayload {
        kind: "xmp",
        tag: None,
        offset_start: cursor.absolute_offset(parsed.start),
        offset_end: cursor.absolute_offset(parsed.end),
        data_offset: cursor.absolute_offset(data_offset),
        data_length: (parsed.end - data_offset) as u64,
        path: parsed.path.clone(),
    })
}

fn fourcc(value: [u8; 4]) -> String {
    String::from_utf8_lossy(&value).into_owned()
}

fn heif_brand(brand: [u8; 4]) -> bool {
    matches!(
        &brand,
        b"mif1" | b"msf1" | b"heic" | b"heix" | b"hevc" | b"heim" | b"heis" | b"avif" | b"avis"
    )
}

fn supported_brand(brand: [u8; 4]) -> bool {
    heif_brand(brand)
        || matches!(
            &brand,
            b"qt  " | b"isom" | b"iso2" | b"mp41" | b"mp42" | b"M4A " | b"M4B " | b"M4P "
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boxed(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let size = (8 + data.len()) as u32;
        let mut out = Vec::new();
        out.extend_from_slice(&size.to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        out
    }

    fn full_box(kind: &[u8; 4], version_flags: [u8; 4], children: &[u8]) -> Vec<u8> {
        let mut data = version_flags.to_vec();
        data.extend_from_slice(children);
        boxed(kind, &data)
    }

    fn quicktime_data_box(text: &str) -> Vec<u8> {
        let mut payload = vec![0, 0, 0, 1, 0, 0, 0, 0];
        payload.extend_from_slice(text.as_bytes());
        payload.push(0);
        boxed(b"data", &payload)
    }

    fn media_track(handler: &[u8; 4], codec: &[u8; 4], width: u32, height: u32) -> Vec<u8> {
        media_track_with_tables(handler, codec, width, height, None, None, None, None, None)
    }

    fn media_track_with_tables(
        handler: &[u8; 4],
        codec: &[u8; 4],
        width: u32,
        height: u32,
        video_bitrate: Option<u32>,
        audio_channels: Option<u16>,
        audio_sample_rate: Option<u32>,
        sample_size: Option<u32>,
        sample_count: Option<u32>,
    ) -> Vec<u8> {
        let mut tkhd_payload = vec![0; 72];
        tkhd_payload.extend_from_slice(&(width << 16).to_be_bytes());
        tkhd_payload.extend_from_slice(&(height << 16).to_be_bytes());
        let tkhd = full_box(b"tkhd", [0, 0, 0, 0], &tkhd_payload);

        let mut mdhd_payload = Vec::new();
        mdhd_payload.extend_from_slice(&3_800_000_000u32.to_be_bytes());
        mdhd_payload.extend_from_slice(&3_800_000_100u32.to_be_bytes());
        mdhd_payload.extend_from_slice(&1_000u32.to_be_bytes());
        mdhd_payload.extend_from_slice(&12_000u32.to_be_bytes());
        mdhd_payload.extend_from_slice(&0u32.to_be_bytes());
        let mdhd = full_box(b"mdhd", [0, 0, 0, 0], &mdhd_payload);

        let mut hdlr_payload = vec![0; 4];
        hdlr_payload.extend_from_slice(handler);
        hdlr_payload.extend_from_slice(&[0; 12]);
        let hdlr = full_box(b"hdlr", [0, 0, 0, 0], &hdlr_payload);

        let sample_entry = if handler == b"vide" {
            let mut payload = vec![0; 78];
            if let Some(bitrate) = video_bitrate {
                let mut btrt = Vec::new();
                btrt.extend_from_slice(&(bitrate * 2).to_be_bytes());
                btrt.extend_from_slice(&(bitrate * 2).to_be_bytes());
                btrt.extend_from_slice(&bitrate.to_be_bytes());
                payload.extend_from_slice(&boxed(b"btrt", &btrt));
            }
            boxed(codec, &payload)
        } else {
            let mut payload = vec![0; 28];
            if let Some(channels) = audio_channels {
                payload[16..18].copy_from_slice(&channels.to_be_bytes());
            }
            if let Some(sample_rate) = audio_sample_rate {
                payload[24..28].copy_from_slice(&(sample_rate << 16).to_be_bytes());
            }
            boxed(codec, &payload)
        };
        let mut stsd_payload = Vec::new();
        stsd_payload.extend_from_slice(&1u32.to_be_bytes());
        stsd_payload.extend_from_slice(&sample_entry);
        let stsd = full_box(b"stsd", [0, 0, 0, 0], &stsd_payload);
        let stts = full_box(
            b"stts",
            [0, 0, 0, 0],
            &[
                1u32.to_be_bytes(),
                24u32.to_be_bytes(),
                1001u32.to_be_bytes(),
            ]
            .concat(),
        );
        let stsz = full_box(
            b"stsz",
            [0, 0, 0, 0],
            &[
                sample_size.unwrap_or(0).to_be_bytes(),
                sample_count.unwrap_or(0).to_be_bytes(),
            ]
            .concat(),
        );

        boxed(
            b"trak",
            &[
                tkhd,
                boxed(
                    b"mdia",
                    &[
                        mdhd,
                        hdlr,
                        boxed(b"minf", &boxed(b"stbl", &[stsd, stts, stsz].concat())),
                    ]
                    .concat(),
                ),
            ]
            .concat(),
        )
    }

    fn minimal_media_file(include_audio: bool, include_unsupported: bool) -> Vec<u8> {
        let mut mvhd_payload = Vec::new();
        mvhd_payload.extend_from_slice(&3_800_000_000u32.to_be_bytes());
        mvhd_payload.extend_from_slice(&3_800_000_100u32.to_be_bytes());
        mvhd_payload.extend_from_slice(&1_000u32.to_be_bytes());
        mvhd_payload.extend_from_slice(&12_000u32.to_be_bytes());
        mvhd_payload.extend_from_slice(&[0; 8]);
        let mvhd = full_box(b"mvhd", [0, 0, 0, 0], &mvhd_payload);

        let mut moov_children = vec![mvhd, media_track(b"vide", b"avc1", 1920, 1080)];
        if include_audio {
            moov_children.push(media_track(b"soun", b"mp4a", 0, 0));
        }
        moov_children.push(boxed(
            b"udta",
            &full_box(
                b"meta",
                [0, 0, 0, 0],
                &boxed(
                    b"ilst",
                    &[
                        boxed(b"\xa9ART", &quicktime_data_box("Kai")),
                        boxed(b"\xa9too", &quicktime_data_box("XIFtyMediaGen")),
                    ]
                    .concat(),
                ),
            ),
        ));
        if include_unsupported {
            moov_children.push(full_box(b"iref", [0, 0, 0, 0], &[0, 0, 0, 0]));
        }

        [
            boxed(b"ftyp", b"isom\0\0\0\0mp42"),
            boxed(b"moov", &moov_children.concat()),
        ]
        .concat()
    }

    #[test]
    fn parses_minimal_heif() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&boxed(b"ftyp", b"heic\0\0\0\0mif1"));
        bytes.extend_from_slice(&full_box(
            b"meta",
            [0, 0, 0, 0],
            &boxed(b"Exif", b"II*\0\x08\0\0\0\x00\x00\0\0\0\0"),
        ));

        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert_eq!(&parsed.major_brand, b"heic");
        assert!(parsed.is_heif_still_image());
        assert_eq!(parsed.exif_payloads().count(), 1);
    }

    /// Build a synthetic Canon CR3: ftyp(crx ) + moov{ uuid(Canon CMT UUID
    /// + inner sub-boxes) }. Each `cmt_payload` is a (FourCC, bytes) pair.
    fn cr3_with_cmt(cmt_payloads: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let ftyp = boxed(b"ftyp", b"crx \0\0\0\0crx isom");
        let mut uuid_payload = CANON_CMT_UUID.to_vec();
        for (kind, payload) in cmt_payloads {
            uuid_payload.extend_from_slice(&boxed(kind, payload));
        }
        let uuid = boxed(b"uuid", &uuid_payload);
        let moov = boxed(b"moov", &uuid);
        [ftyp, moov].concat()
    }

    /// Smallest legal TIFF carrying a single ASCII Make entry.
    fn tiff_with_make_ascii(make: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"II*\0");
        bytes.extend_from_slice(&8u32.to_le_bytes()); // IFD0 at offset 8
        bytes.extend_from_slice(&1u16.to_le_bytes()); // one entry
        bytes.extend_from_slice(&0x010Fu16.to_le_bytes()); // Make
        bytes.extend_from_slice(&2u16.to_le_bytes()); // ASCII
        let mut value = make.as_bytes().to_vec();
        value.push(0);
        let count = value.len() as u32;
        bytes.extend_from_slice(&count.to_le_bytes());
        if value.len() <= 4 {
            let mut padded = [0u8; 4];
            padded[..value.len()].copy_from_slice(&value);
            bytes.extend_from_slice(&padded);
            bytes.extend_from_slice(&0u32.to_le_bytes()); // next IFD = 0
        } else {
            let value_offset = 26u32;
            bytes.extend_from_slice(&value_offset.to_le_bytes());
            bytes.extend_from_slice(&0u32.to_le_bytes());
            bytes.extend_from_slice(&value);
        }
        bytes
    }

    #[test]
    fn parses_minimal_cr3_with_cmt1_payload() {
        let bytes = cr3_with_cmt(&[(b"CMT1", tiff_with_make_ascii("Canon"))]);
        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert_eq!(&parsed.major_brand, b"crx ");
        let cmt1: Vec<_> = parsed
            .exif_payloads()
            .filter(|p| p.tag.as_deref() == Some("CMT1"))
            .collect();
        assert_eq!(cmt1.len(), 1, "expected one CMT1 exif payload");
        let payload = cmt1[0];
        // Slice the surfaced byte range and assert it round-trips to the
        // TIFF header — proves data_offset/data_length point at real bytes.
        let slice = &bytes
            [payload.data_offset as usize..(payload.data_offset + payload.data_length) as usize];
        assert!(slice.starts_with(b"II*\0"));
    }

    #[test]
    fn parses_minimal_cr3_with_cmt2_payload() {
        let bytes = cr3_with_cmt(&[
            (b"CMT1", tiff_with_make_ascii("Canon")),
            (b"CMT2", tiff_with_make_ascii("Canon")),
        ]);
        let parsed = parse_bytes(&bytes, 0).unwrap();
        let cmt2: Vec<_> = parsed
            .canon_cmt_payloads()
            .filter(|p| p.tag.as_deref() == Some("CMT2"))
            .collect();
        assert_eq!(cmt2.len(), 1, "expected one CMT2 canon-cmt payload");
        // CMT1 still arrives via the exif channel, not the canon-cmt channel.
        assert_eq!(parsed.canon_cmt_payloads().count(), 1);
        assert_eq!(
            parsed
                .exif_payloads()
                .filter(|p| p.tag.as_deref() == Some("CMT1"))
                .count(),
            1
        );
    }

    #[test]
    fn ignores_unknown_uuid_box() {
        // Non-Canon UUID payload must be skipped without raising a parse
        // error and must surface only an `isobmff_structure_recognized_uninterpreted`
        // info issue.
        let mut uuid_payload = vec![0u8; 16]; // all-zero UUID, definitely not Canon CMT
        uuid_payload.extend_from_slice(b"opaque-bytes");
        let uuid = boxed(b"uuid", &uuid_payload);
        let bytes = [boxed(b"ftyp", b"isom\0\0\0\0mp42"), boxed(b"moov", &uuid)].concat();

        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert_eq!(parsed.canon_cmt_payloads().count(), 0);
        assert_eq!(parsed.exif_payloads().count(), 0);
        assert_eq!(parsed.sony_video_atoms().count(), 0);
        let issue = parsed
            .issues
            .iter()
            .find(|issue| {
                issue.code == "isobmff_structure_recognized_uninterpreted"
                    && issue.context.as_deref() == Some("moov/uuid")
            })
            .expect("uninterpreted-uuid issue");
        // Issue body now carries the lowercase-hex usertype so future unknown
        // families are actionable from the issue stream alone.
        assert!(
            issue.message.contains("00000000000000000000000000000000"),
            "expected hex in {}",
            issue.message,
        );
    }

    #[test]
    fn recognizes_sony_userdata_uuid_family() {
        // Synthetic Sony-PROF UUID box. Usertype = "PROF" + Sony tail; payload
        // body is opaque (the decoder lives in xifty-meta-sony-video).
        let mut uuid_payload = Vec::new();
        uuid_payload.extend_from_slice(b"PROF");
        uuid_payload.extend_from_slice(&[
            0x21, 0xd2, 0x4f, 0xce, 0xbb, 0x88, 0x69, 0x5c, 0xfa, 0xc9, 0xc7, 0x40,
        ]);
        uuid_payload.extend_from_slice(b"opaque-prof-bytes");
        let uuid = boxed(b"uuid", &uuid_payload);
        let bytes = [boxed(b"ftyp", b"isom\0\0\0\0mp42"), boxed(b"moov", &uuid)].concat();

        let parsed = parse_bytes(&bytes, 0).unwrap();
        let atoms: Vec<_> = parsed.sony_video_atoms().collect();
        assert_eq!(atoms.len(), 1, "expected one sony-video-atom payload");
        let atom = atoms[0];
        assert_eq!(atom.tag.as_deref(), Some("PROF"));
        assert_eq!(atom.kind, "sony-video-atom");
        // data_offset/data_length must skip the 16-byte usertype.
        assert_eq!(atom.data_length, b"opaque-prof-bytes".len() as u64);
        // No uninterpreted info-issue for a known Sony family.
        assert!(
            !parsed.issues.iter().any(|issue| issue.code
                == "isobmff_structure_recognized_uninterpreted"
                && issue
                    .context
                    .as_deref()
                    .is_some_and(|ctx| ctx.contains("uuid"))),
            "Sony-PROF UUID should not produce an uninterpreted info issue: {:?}",
            parsed.issues,
        );
    }

    #[test]
    fn recognizes_c2pa_uuid_box() {
        // ftyp + uuid(C2PA UUID + minimal JUMBF-shaped payload).
        let mut uuid_payload = Vec::new();
        uuid_payload.extend_from_slice(&[
            0xd8, 0xfe, 0xc5, 0xd8, 0x76, 0xcc, 0xb9, 0xee, 0x9c, 0xc0, 0x50, 0x4d, 0x8a, 0x16,
            0xb1, 0x13,
        ]);
        let jumbf_bytes = b"\x00\x00\x00\x10jumbXXXXXXXX";
        uuid_payload.extend_from_slice(jumbf_bytes);
        let uuid = boxed(b"uuid", &uuid_payload);
        let bytes = [boxed(b"ftyp", b"isom\0\0\0\0mp42"), uuid].concat();

        let parsed = parse_bytes(&bytes, 0).unwrap();
        let c2pa: Vec<_> = parsed.c2pa_payloads().collect();
        assert_eq!(c2pa.len(), 1, "expected one c2pa payload");
        let entry = c2pa[0];
        assert_eq!(entry.kind, "c2pa");
        assert_eq!(entry.tag.as_deref(), Some("uuid_c2pa"));
        assert_eq!(entry.data_length, jumbf_bytes.len() as u64);
        let slice =
            &bytes[entry.data_offset as usize..(entry.data_offset + entry.data_length) as usize];
        assert_eq!(slice, jumbf_bytes);
        // C2PA UUID is recognized — no uninterpreted-info-issue.
        assert!(
            !parsed
                .issues
                .iter()
                .any(|issue| issue.code == "isobmff_structure_recognized_uninterpreted"),
            "C2PA UUID should not raise uninterpreted info issue: {:?}",
            parsed.issues,
        );
    }

    #[test]
    fn parses_item_based_metadata_and_primary_dimensions() {
        let bytes = include_bytes!("../../../fixtures/minimal/real_exif.heic");
        let parsed = parse_bytes(bytes, 0).unwrap();
        assert_eq!(parsed.exif_payloads().count(), 1);
        let dimensions = parsed.primary_item_dimensions.unwrap();
        assert_eq!((dimensions.width, dimensions.height), (700, 476));
    }

    #[test]
    fn parses_nested_meta_and_xmp_payload() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&boxed(b"ftyp", b"mif1\0\0\0\0heic"));
        bytes.extend_from_slice(&full_box(
            b"meta",
            [0, 0, 0, 0],
            &boxed(
                b"mime",
                b"application/rdf+xml\0<x:xmpmeta><rdf:Description tiff:Make=\"XIFtyCam\" /></x:xmpmeta>",
            ),
        ));

        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert_eq!(parsed.xmp_payloads().count(), 1);
        assert!(parsed.nodes.iter().any(|node| node.label == "meta/mime"));
    }

    #[test]
    fn parses_colr_prof_as_icc_payload() {
        let mut colr_data = b"prof".to_vec();
        colr_data.extend_from_slice(b"ICCBYTES");
        let ipco = boxed(b"ipco", &boxed(b"colr", &colr_data));
        let iprp = boxed(b"iprp", &ipco);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&boxed(b"ftyp", b"heic\0\0\0\0mif1"));
        bytes.extend_from_slice(&full_box(b"meta", [0, 0, 0, 0], &iprp));

        let parsed = parse_bytes(&bytes, 0).unwrap();
        let payload = parsed.icc_payloads().next().expect("expected icc payload");
        assert_eq!(payload.data_length, 8);
        let icc_bytes = &bytes
            [payload.data_offset as usize..(payload.data_offset + payload.data_length) as usize];
        assert_eq!(icc_bytes, b"ICCBYTES");
    }

    /// Build a synthetic AVIF-shaped fixture pairing the primary item with
    /// `ispe`, `pixi`, `colr nclx`, and `cicp` properties via `ipma`.
    /// Used to verify that `cicp`/`pixi` dispatch lives at the correct
    /// nesting level (siblings of `colr` inside `ipco`'s children walk).
    fn avif_with_color_properties() -> Vec<u8> {
        // ispe: reserved(4) + width(4) + height(4)
        let ispe = full_box(b"ispe", [0, 0, 0, 0], &{
            let mut payload = Vec::new();
            payload.extend_from_slice(&100u32.to_be_bytes());
            payload.extend_from_slice(&80u32.to_be_bytes());
            payload
        });
        // pixi: num_channels=3 + bits[3] = [10, 10, 10]
        let pixi = full_box(b"pixi", [0, 0, 0, 0], &[3u8, 10, 10, 10]);
        // colr nclx: type='nclx' + primaries=9 + transfer=16 + matrix=9 + range=0x80
        let mut colr_payload = b"nclx".to_vec();
        colr_payload.extend_from_slice(&9u16.to_be_bytes());
        colr_payload.extend_from_slice(&16u16.to_be_bytes());
        colr_payload.extend_from_slice(&9u16.to_be_bytes());
        colr_payload.push(0x80);
        let colr = boxed(b"colr", &colr_payload);
        // cicp: full-box + primaries(1) + transfer(1) + matrix(1) + range(0x80)
        let cicp = full_box(b"cicp", [0, 0, 0, 0], &[9u8, 16, 9, 0x80]);

        let ipco = boxed(
            b"ipco",
            &[ispe.clone(), pixi.clone(), colr.clone(), cicp.clone()].concat(),
        );

        // ipma: entry_count(4) + entry [item_id(2) + assoc_count(1) +
        //                               4 x property_index(1) + essential bit cleared]
        let mut ipma_payload = Vec::new();
        ipma_payload.extend_from_slice(&1u32.to_be_bytes()); // entry_count
        ipma_payload.extend_from_slice(&1u16.to_be_bytes()); // item_id
        ipma_payload.push(4); // association_count
        ipma_payload.extend_from_slice(&[1, 2, 3, 4]); // 1=ispe 2=pixi 3=colr 4=cicp
        let ipma = full_box(b"ipma", [0, 0, 0, 0], &ipma_payload);

        let iprp = boxed(b"iprp", &[ipco, ipma].concat());

        // pitm v0: reserved(2) + item_id(2)=1
        let pitm = full_box(b"pitm", [0, 0, 0, 0], &1u16.to_be_bytes());

        let meta = full_box(b"meta", [0, 0, 0, 0], &[pitm, iprp].concat());

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&boxed(b"ftyp", b"avif\0\0\0\0mif1"));
        bytes.extend_from_slice(&meta);
        bytes
    }

    #[test]
    fn surfaces_color_and_pixel_for_primary_avif_item() {
        let bytes = avif_with_color_properties();
        let parsed = parse_bytes(&bytes, 0).unwrap();
        let color = parsed
            .primary_item_color
            .as_ref()
            .expect("expected color info");
        // cicp must win over colr_nclx per the precedence policy.
        assert_eq!(color.source, "cicp");
        assert_eq!(color.primaries, Some(9));
        assert_eq!(color.transfer, Some(16));
        assert_eq!(color.matrix, Some(9));
        assert_eq!(color.full_range, Some(true));
        let pixel = parsed
            .primary_item_pixel
            .as_ref()
            .expect("expected pixel info");
        assert_eq!(pixel.bits_per_channel, vec![10, 10, 10]);
        let dimensions = parsed
            .primary_item_dimensions
            .as_ref()
            .expect("expected primary dimensions");
        assert_eq!((dimensions.width, dimensions.height), (100, 80));
    }

    #[test]
    fn parses_colr_nclx_without_cicp() {
        // No cicp: colr_nclx must be the surfaced color source.
        let mut colr_payload = b"nclx".to_vec();
        colr_payload.extend_from_slice(&1u16.to_be_bytes()); // primaries (BT.709)
        colr_payload.extend_from_slice(&13u16.to_be_bytes()); // transfer (sRGB)
        colr_payload.extend_from_slice(&1u16.to_be_bytes()); // matrix (BT.709)
        colr_payload.push(0x00); // narrow-range
        let colr = boxed(b"colr", &colr_payload);
        let ispe = full_box(b"ispe", [0, 0, 0, 0], &{
            let mut payload = Vec::new();
            payload.extend_from_slice(&64u32.to_be_bytes());
            payload.extend_from_slice(&64u32.to_be_bytes());
            payload
        });
        let ipco = boxed(b"ipco", &[ispe, colr].concat());

        let mut ipma_payload = Vec::new();
        ipma_payload.extend_from_slice(&1u32.to_be_bytes());
        ipma_payload.extend_from_slice(&1u16.to_be_bytes());
        ipma_payload.push(2);
        ipma_payload.extend_from_slice(&[1, 2]);
        let ipma = full_box(b"ipma", [0, 0, 0, 0], &ipma_payload);

        let iprp = boxed(b"iprp", &[ipco, ipma].concat());
        let pitm = full_box(b"pitm", [0, 0, 0, 0], &1u16.to_be_bytes());
        let meta = full_box(b"meta", [0, 0, 0, 0], &[pitm, iprp].concat());

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&boxed(b"ftyp", b"avif\0\0\0\0mif1"));
        bytes.extend_from_slice(&meta);
        let parsed = parse_bytes(&bytes, 0).unwrap();
        let color = parsed.primary_item_color.expect("color expected");
        assert_eq!(color.source, "colr_nclx");
        assert_eq!(color.primaries, Some(1));
        assert_eq!(color.transfer, Some(13));
        assert_eq!(color.matrix, Some(1));
        assert_eq!(color.full_range, Some(false));
    }

    #[test]
    fn ignores_colr_nclx() {
        let mut colr_data = b"nclx".to_vec();
        colr_data.extend_from_slice(&[0u8; 7]);
        let ipco = boxed(b"ipco", &boxed(b"colr", &colr_data));
        let iprp = boxed(b"iprp", &ipco);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&boxed(b"ftyp", b"heic\0\0\0\0mif1"));
        bytes.extend_from_slice(&full_box(b"meta", [0, 0, 0, 0], &iprp));

        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert_eq!(parsed.icc_payloads().count(), 0);
    }

    #[test]
    fn routes_iptc_item_payload() {
        // Build: ftyp + meta { iinf(infe item_id=1 type=iptc) iloc(item_id=1 extent)
        //                     mdat-like bytes in a later box for extent offset }.
        // Use construction_method=0 with base_offset pointing at raw IIM bytes
        // we place inline after the meta (mimicking mdat).
        let iim = b"\x1c\x02\x69\x00\x05Hello";

        // infe v2: version(1)+flags(3) + item_id(2) + item_protection_index(2) + item_type(4) + item_name(nul)
        let mut infe_payload = Vec::new();
        infe_payload.extend_from_slice(&1u16.to_be_bytes()); // item_id
        infe_payload.extend_from_slice(&0u16.to_be_bytes()); // item_protection_index
        infe_payload.extend_from_slice(b"iptc"); // item_type
        infe_payload.push(0); // item_name terminator
        let infe = full_box(b"infe", [2, 0, 0, 0], &infe_payload);

        // iinf v0: entry_count(2) + infe boxes
        let mut iinf_payload = Vec::new();
        iinf_payload.extend_from_slice(&1u16.to_be_bytes());
        iinf_payload.extend_from_slice(&infe);
        let iinf = full_box(b"iinf", [0, 0, 0, 0], &iinf_payload);

        // We need to know the absolute offset of the IIM bytes. Lay out file
        // deterministically: ftyp(16) + meta(...) + iim_bytes.
        // Compute meta size with a placeholder iloc whose offset we fill after.
        // Easier path: place IIM bytes before meta via a wrapper box. Use an
        // "mdat" box placed BEFORE meta, then base_offset targets it.
        let mdat = boxed(b"mdat", iim);
        let ftyp = boxed(b"ftyp", b"heic\0\0\0\0mif1");
        let iim_absolute_offset = (ftyp.len() + 8) as u64; // inside mdat, after 8-byte header

        // iloc v1: version(1) flags(3) offset_size=4<<4|length_size=4 = 0x44
        //         base_offset_size=0<<4|index_size=0 = 0x00
        //         item_count(2) = 1
        //         per-item: item_id(2) construction_method(2)=0 data_ref(2)=0
        //                   base_offset(0 bytes) extent_count(2)=1
        //                   extent_offset(4) extent_length(4)
        let mut iloc_payload = Vec::new();
        iloc_payload.push(0x44); // offset_size=4, length_size=4
        iloc_payload.push(0x00); // base_offset_size=0, index_size=0
        iloc_payload.extend_from_slice(&1u16.to_be_bytes()); // item_count
        iloc_payload.extend_from_slice(&1u16.to_be_bytes()); // item_id
        iloc_payload.extend_from_slice(&0u16.to_be_bytes()); // construction_method
        iloc_payload.extend_from_slice(&0u16.to_be_bytes()); // data_reference_index
        iloc_payload.extend_from_slice(&(iim_absolute_offset as u32).to_be_bytes()); // extent_offset (absolute)
        iloc_payload.extend_from_slice(&1u16.to_be_bytes()); // extent_count
        iloc_payload.extend_from_slice(&(iim_absolute_offset as u32).to_be_bytes()); // extent_offset
        iloc_payload.extend_from_slice(&(iim.len() as u32).to_be_bytes()); // extent_length

        // iloc v1 layout actually: after version+flags: sizes byte, base_index byte,
        // item_count, then per-item: item_id, construction_method (only in v1/v2),
        // data_reference_index, base_offset, extent_count, extents.
        // We wrote item_id+construction_method+data_ref inline above but that
        // duplicated the absolute_offset. Rebuild cleanly:
        let mut iloc_payload = Vec::new();
        iloc_payload.push(0x44); // offset_size=4, length_size=4
        iloc_payload.push(0x00); // base_offset_size=0, index_size=0
        iloc_payload.extend_from_slice(&1u16.to_be_bytes()); // item_count (v<2 -> u16)
        iloc_payload.extend_from_slice(&1u16.to_be_bytes()); // item_id
        iloc_payload.extend_from_slice(&0u16.to_be_bytes()); // construction_method (v>=1)
        iloc_payload.extend_from_slice(&0u16.to_be_bytes()); // data_reference_index
        // base_offset: 0 bytes
        iloc_payload.extend_from_slice(&1u16.to_be_bytes()); // extent_count
        iloc_payload.extend_from_slice(&(iim_absolute_offset as u32).to_be_bytes());
        iloc_payload.extend_from_slice(&(iim.len() as u32).to_be_bytes());
        let iloc = full_box(b"iloc", [1, 0, 0, 0], &iloc_payload);

        let meta = full_box(
            b"meta",
            [0, 0, 0, 0],
            &[iinf.clone(), iloc.clone()].concat(),
        );

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&ftyp);
        bytes.extend_from_slice(&mdat);
        bytes.extend_from_slice(&meta);

        let parsed = parse_bytes(&bytes, 0).unwrap();
        let payload = parsed
            .iptc_payloads()
            .next()
            .expect("expected iptc payload");
        let slice = &bytes
            [payload.data_offset as usize..(payload.data_offset + payload.data_length) as usize];
        assert_eq!(slice, iim);
    }

    #[test]
    fn reports_box_size_out_of_bounds() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&boxed(b"ftyp", b"heic\0\0\0\0mif1"));
        bytes.extend_from_slice(&[
            0x00, 0x00, 0x00, 0x40, b'm', b'e', b't', b'a', 0x00, 0x00, 0x00, 0x00,
        ]);

        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert!(
            parsed
                .issues
                .iter()
                .any(|issue| issue.code == "isobmff_box_size_out_of_bounds")
        );
    }

    #[test]
    fn derives_media_duration_codecs_and_dimensions() {
        let parsed = parse_bytes(&minimal_media_file(true, false), 0).unwrap();
        assert_eq!(parsed.media_duration_seconds, Some(12.0));
        assert_eq!(parsed.video_codec.as_deref(), Some("avc1"));
        assert_eq!(parsed.audio_codec.as_deref(), Some("mp4a"));
        let dimensions = parsed.primary_visual_dimensions.unwrap();
        assert_eq!((dimensions.width, dimensions.height), (1920, 1080));
    }

    #[test]
    fn surfaces_dji_direct_udta_text_atoms_as_quicktime_udta_payloads() {
        let bytes = include_bytes!("../../../fixtures/minimal/dji_mavic3.mp4");
        let parsed = parse_bytes(bytes, 0).unwrap();
        // 12 documented DJI keys: ©xyz, ©xsp/©ysp/©zsp, ©fpt/©fyw/©frl,
        // ©gpt/©gyw/©grl, ©mdl, ©csn — all direct children of `udta`,
        // none under `ilst`.
        assert_eq!(parsed.quicktime_udta_payloads().count(), 12);
        let tags: Vec<&str> = parsed
            .quicktime_udta_payloads()
            .filter_map(|p| p.tag.as_deref())
            .collect();
        assert!(tags.contains(&"\u{a9}fpt"));
        assert!(tags.contains(&"\u{a9}xyz"));
        assert!(tags.contains(&"\u{a9}mdl"));
        assert!(tags.contains(&"\u{a9}csn"));
        // iTunes routing untouched (no `ilst` in this fixture).
        assert_eq!(parsed.itunes_payloads().count(), 0);
        // The classic-quicktime `data`-box path also untouched.
        assert_eq!(parsed.quicktime_payloads().count(), 0);
    }

    #[test]
    fn routes_quicktime_payloads_and_reports_unsupported_structure() {
        let parsed = parse_bytes(&minimal_media_file(true, true), 0).unwrap();
        assert_eq!(parsed.quicktime_payloads().count(), 2);
        assert!(
            parsed
                .issues
                .iter()
                .any(|issue| issue.code == "isobmff_structure_recognized_uninterpreted")
        );
    }

    #[test]
    fn parses_itunes_ilst_atoms() {
        // Integer-pair atom body for trkn: 8-byte pair slot
        let trkn_pair = vec![0u8, 0, 0, 0, 0, 3, 0, 10]; // track 3 of 10
        let mut trkn_payload = vec![0, 0, 0, 0, 0, 0, 0, 0];
        trkn_payload.extend_from_slice(&trkn_pair);
        let trkn_data = boxed(b"data", &trkn_payload);

        // bool cpil body (1 byte = 1)
        let cpil_body = vec![0, 0, 0, 0x15, 0, 0, 0, 0, 1u8];
        let cpil_data = boxed(b"data", &cpil_body);

        // covr binary body (tiny PNG-ish bytes, flags=0x0D = PNG)
        let covr_body = vec![0, 0, 0, 0x0D, 0, 0, 0, 0, 0x89, b'P', b'N', b'G'];
        let covr_data = boxed(b"data", &covr_body);

        let ilst = boxed(
            b"ilst",
            &[
                boxed(b"\xa9nam", &quicktime_data_box("Title")),
                boxed(b"\xa9ART", &quicktime_data_box("Artist")),
                boxed(b"\xa9alb", &quicktime_data_box("Album")),
                boxed(b"\xa9day", &quicktime_data_box("2024")),
                boxed(b"\xa9gen", &quicktime_data_box("Ambient")),
                boxed(b"aART", &quicktime_data_box("Album Artist")),
                boxed(b"trkn", &trkn_data),
                boxed(b"cpil", &cpil_data),
                boxed(b"covr", &covr_data),
            ]
            .concat(),
        );
        let udta = boxed(b"udta", &full_box(b"meta", [0, 0, 0, 0], &ilst));
        let bytes = [boxed(b"ftyp", b"M4A \0\0\0\0mp42"), boxed(b"moov", &udta)].concat();

        let parsed = parse_bytes(&bytes, 0).unwrap();
        let itunes: Vec<_> = parsed.itunes_payloads().collect();
        assert!(itunes.iter().any(|p| p.tag.as_deref() == Some("\u{a9}nam")));
        assert!(itunes.iter().any(|p| p.tag.as_deref() == Some("\u{a9}alb")));
        assert!(itunes.iter().any(|p| p.tag.as_deref() == Some("trkn")));
        assert!(itunes.iter().any(|p| p.tag.as_deref() == Some("cpil")));
        assert!(itunes.iter().any(|p| p.tag.as_deref() == Some("covr")));
        assert!(itunes.iter().any(|p| p.tag.as_deref() == Some("aART")));
        // back-compat: quicktime payloads still emitted for the legacy three
        assert_eq!(parsed.quicktime_payloads().count(), 2);
        // M4A brand is now a supported brand (no unrecognized-brand info issue)
        assert!(
            !parsed
                .issues
                .iter()
                .any(|issue| issue.code == "isobmff_unrecognized_brand")
        );
    }

    #[test]
    fn omits_audio_codec_when_audio_track_is_missing() {
        let parsed = parse_bytes(&minimal_media_file(false, false), 0).unwrap();
        assert_eq!(parsed.video_codec.as_deref(), Some("avc1"));
        assert_eq!(parsed.audio_codec, None);
    }

    #[test]
    fn derives_video_bitrate_from_sample_sizes_when_btrt_is_missing() {
        let mut mvhd_payload = Vec::new();
        mvhd_payload.extend_from_slice(&3_800_000_000u32.to_be_bytes());
        mvhd_payload.extend_from_slice(&3_800_000_100u32.to_be_bytes());
        mvhd_payload.extend_from_slice(&1_000u32.to_be_bytes());
        mvhd_payload.extend_from_slice(&12_000u32.to_be_bytes());
        mvhd_payload.extend_from_slice(&[0; 8]);
        let mvhd = full_box(b"mvhd", [0, 0, 0, 0], &mvhd_payload);
        let bytes = [
            boxed(b"ftyp", b"isom\0\0\0\0mp42"),
            boxed(
                b"moov",
                &[
                    mvhd,
                    media_track_with_tables(
                        b"vide",
                        b"avc1",
                        1920,
                        1080,
                        None,
                        None,
                        None,
                        Some(150_000),
                        Some(240),
                    ),
                ]
                .concat(),
            ),
        ]
        .concat();

        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert_eq!(parsed.video_bitrate, Some(24_000_000));
        assert_eq!(
            parsed.video_bitrate_note.as_deref(),
            Some("derived from video track sample sizes and duration")
        );
    }

    #[test]
    fn parses_pcm_audio_sample_rate_for_twos_entries() {
        let mut mvhd_payload = Vec::new();
        mvhd_payload.extend_from_slice(&3_800_000_000u32.to_be_bytes());
        mvhd_payload.extend_from_slice(&3_800_000_100u32.to_be_bytes());
        mvhd_payload.extend_from_slice(&1_000u32.to_be_bytes());
        mvhd_payload.extend_from_slice(&12_000u32.to_be_bytes());
        mvhd_payload.extend_from_slice(&[0; 8]);
        let mvhd = full_box(b"mvhd", [0, 0, 0, 0], &mvhd_payload);
        let bytes = [
            boxed(b"ftyp", b"isom\0\0\0\0mp42"),
            boxed(
                b"moov",
                &[
                    mvhd,
                    media_track_with_tables(
                        b"soun",
                        b"twos",
                        0,
                        0,
                        None,
                        Some(2),
                        Some(48_000),
                        None,
                        None,
                    ),
                ]
                .concat(),
            ),
        ]
        .concat();

        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert_eq!(parsed.audio_codec.as_deref(), Some("twos"));
        assert_eq!(parsed.audio_channels, Some(2));
        assert_eq!(parsed.audio_sample_rate, Some(48_000));
    }
}
