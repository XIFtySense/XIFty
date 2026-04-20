use flate2::read::ZlibDecoder;
use std::{io::Read, path::PathBuf};
use xifty_container_isobmff::parse as parse_isobmff;
use xifty_container_jpeg::parse as parse_jpeg;
use xifty_container_png::parse as parse_png;
use xifty_container_riff::parse as parse_riff;
use xifty_container_tiff::parse as parse_tiff;
use xifty_core::{
    AnalysisOutput, Format, InterpretedView, Issue, MetadataEntry, ProbeInput, ProbeOutput,
    Provenance, RawView, SCHEMA_VERSION, Severity, TypedValue, ViewMode, XiftyError,
};
use xifty_detect::detect;
use xifty_meta_apple::decode_from_tiff as decode_apple_from_tiff;
use xifty_meta_exif::{decode_from_tiff, exif_payload_from_jpeg};
use xifty_meta_icc::{IccPayload, decode_payload as decode_icc_payload};
use xifty_meta_iptc::{IptcPayload, decode_payload as decode_iptc_payload};
use xifty_meta_quicktime::{QuickTimePayload, decode_payload as decode_quicktime_payload};
use xifty_meta_rtmd::{RtmdPacket, decode_packet as decode_rtmd_packet};
use xifty_meta_sony::decode_from_tiff as decode_sony_from_tiff;
use xifty_meta_xmp::{XmpPacket, decode_packet, decode_png_text_chunk, decode_webp_xmp_chunk};
use xifty_normalize::normalize_with_policy;
use xifty_source::SourceBytes;
use xifty_validate::build_report;

pub fn probe_path(path: PathBuf) -> Result<ProbeOutput, XiftyError> {
    let source = SourceBytes::from_path(&path)?;
    probe_source(&source)
}

pub fn probe_bytes(bytes: Vec<u8>, file_name: Option<String>) -> Result<ProbeOutput, XiftyError> {
    let source = SourceBytes::new(browser_path(file_name), bytes);
    probe_source(&source)
}

fn probe_source(source: &SourceBytes) -> Result<ProbeOutput, XiftyError> {
    let format = detect(&source)?;
    let (container, nodes, issues) = match format {
        Format::Jpeg => {
            let parsed = parse_jpeg(&source)?;
            ("jpeg".to_string(), parsed.nodes, parsed.issues)
        }
        Format::Tiff => {
            let parsed = parse_tiff(&source)?;
            ("tiff".to_string(), parsed.nodes, parsed.issues)
        }
        Format::Png => {
            let parsed = parse_png(&source)?;
            ("png".to_string(), parsed.nodes, parsed.issues)
        }
        Format::Webp => {
            let parsed = parse_riff(&source)?;
            ("webp".to_string(), parsed.nodes, parsed.issues)
        }
        Format::Heif => {
            let parsed = parse_isobmff(&source)?;
            ("isobmff".to_string(), parsed.nodes, parsed.issues)
        }
        Format::Mp4 => {
            let parsed = parse_isobmff(&source)?;
            ("isobmff".to_string(), parsed.nodes, parsed.issues)
        }
        Format::Mov => {
            let parsed = parse_isobmff(&source)?;
            ("isobmff".to_string(), parsed.nodes, parsed.issues)
        }
    };
    Ok(ProbeOutput {
        schema_version: SCHEMA_VERSION.into(),
        input: ProbeInput {
            path: source.source.path.clone(),
            detected_format: format.as_str().into(),
            container,
        },
        containers: nodes,
        report: build_report(issues, &[]),
    })
}

pub fn extract_path(path: PathBuf, view_mode: ViewMode) -> Result<AnalysisOutput, XiftyError> {
    let source = SourceBytes::from_path(&path)?;
    extract_source(&source, view_mode)
}

pub fn extract_bytes(
    bytes: Vec<u8>,
    file_name: Option<String>,
    view_mode: ViewMode,
) -> Result<AnalysisOutput, XiftyError> {
    let source = SourceBytes::new(browser_path(file_name), bytes);
    extract_source(&source, view_mode)
}

fn extract_source(source: &SourceBytes, view_mode: ViewMode) -> Result<AnalysisOutput, XiftyError> {
    let format = detect(&source)?;

    let (container_name, nodes, entries, issues) = match format {
        Format::Jpeg => {
            let jpeg = parse_jpeg(&source)?;
            let mut issues = jpeg.issues.clone();
            let mut entries =
                if let Some((base_offset, exif_payload)) = exif_payload_from_jpeg(&jpeg) {
                    let tiff =
                        xifty_container_tiff::parse_bytes(exif_payload, base_offset, "jpeg_exif")?;
                    issues.extend(tiff.issues.clone());
                    let mut entries = decode_from_tiff(exif_payload, base_offset, "jpeg", &tiff);
                    entries.extend(decode_apple_from_tiff(
                        exif_payload,
                        "jpeg",
                        &tiff,
                        &entries,
                    ));
                    entries.extend(decode_sony_from_tiff(
                        exif_payload,
                        base_offset,
                        "jpeg",
                        &tiff,
                        &entries,
                    ));
                    entries
                } else {
                    Vec::new()
                };
            for (offset_start, payload) in jpeg.icc_payloads() {
                let decoded = decode_icc_payload(IccPayload {
                    bytes: payload,
                    container: "jpeg",
                    path: "app2_icc",
                    offset_start,
                    offset_end: offset_start + payload.len() as u64,
                });
                if decoded.is_empty() {
                    issues.push(namespace_issue(
                        "icc_decode_empty",
                        "recognized ICC payload but could not decode bounded ICC fields",
                        offset_start,
                        "app2_icc",
                    ));
                }
                entries.extend(decoded);
            }
            for (offset_start, payload) in jpeg.iptc_payloads() {
                let decoded = decode_iptc_payload(IptcPayload {
                    bytes: payload,
                    container: "jpeg",
                    path: "app13_iptc",
                    offset_start,
                    offset_end: offset_start + payload.len() as u64,
                });
                if decoded.is_empty() {
                    issues.push(namespace_issue(
                        "iptc_decode_empty",
                        "recognized IPTC payload but could not decode bounded IPTC datasets",
                        offset_start,
                        "app13_iptc",
                    ));
                }
                entries.extend(decoded);
            }
            for (offset_start, payload) in jpeg.xmp_payloads() {
                let decoded = decode_packet(XmpPacket {
                    bytes: payload,
                    container: "jpeg",
                    offset_start,
                    offset_end: offset_start + payload.len() as u64,
                });
                if decoded.is_empty() {
                    issues.push(namespace_issue(
                        "xmp_decode_empty",
                        "recognized XMP payload but could not decode bounded XMP fields",
                        offset_start,
                        "app1_xmp",
                    ));
                }
                entries.extend(decoded);
            }
            ("jpeg".to_string(), jpeg.nodes, entries, issues)
        }
        Format::Tiff => {
            let tiff = parse_tiff(&source)?;
            let mut issues = tiff.issues.clone();
            let mut entries = decode_from_tiff(source.bytes(), 0, "tiff", &tiff);
            entries.extend(decode_apple_from_tiff(
                source.bytes(),
                "tiff",
                &tiff,
                &entries,
            ));
            entries.extend(decode_sony_from_tiff(
                source.bytes(),
                0,
                "tiff",
                &tiff,
                &entries,
            ));
            if let Some((offset_start, payload)) =
                xifty_container_tiff::xmp_payload(source.bytes(), &tiff)
            {
                let decoded = decode_packet(XmpPacket {
                    bytes: payload,
                    container: "tiff",
                    offset_start,
                    offset_end: offset_start + payload.len() as u64,
                });
                if decoded.is_empty() {
                    issues.push(namespace_issue(
                        "xmp_decode_empty",
                        "recognized XMP payload but could not decode bounded XMP fields",
                        offset_start,
                        "ifd0_xmp",
                    ));
                }
                entries.extend(decoded);
            }
            if let Some((offset_start, payload)) =
                xifty_container_tiff::icc_payload(source.bytes(), &tiff)
            {
                let decoded = decode_icc_payload(IccPayload {
                    bytes: payload,
                    container: "tiff",
                    path: "ifd0_icc",
                    offset_start,
                    offset_end: offset_start + payload.len() as u64,
                });
                if decoded.is_empty() {
                    issues.push(namespace_issue(
                        "icc_decode_empty",
                        "recognized ICC payload but could not decode bounded ICC fields",
                        offset_start,
                        "ifd0_icc",
                    ));
                }
                entries.extend(decoded);
            }
            if let Some((offset_start, payload)) =
                xifty_container_tiff::iptc_payload(source.bytes(), &tiff)
            {
                let decoded = decode_iptc_payload(IptcPayload {
                    bytes: payload,
                    container: "tiff",
                    path: "ifd0_iptc",
                    offset_start,
                    offset_end: offset_start + payload.len() as u64,
                });
                if decoded.is_empty() {
                    issues.push(namespace_issue(
                        "iptc_decode_empty",
                        "recognized IPTC payload but could not decode bounded IPTC datasets",
                        offset_start,
                        "ifd0_iptc",
                    ));
                }
                entries.extend(decoded);
            }
            ("tiff".to_string(), tiff.nodes, entries, issues)
        }
        Format::Png => {
            let png = parse_png(&source)?;
            let mut entries = Vec::new();
            let mut issues = png.issues.clone();
            for chunk in png.exif_payloads() {
                if let Some(payload) = payload_slice(
                    source.bytes(),
                    chunk.data_offset,
                    chunk.data_length as usize,
                ) {
                    if let Ok(tiff) =
                        xifty_container_tiff::parse_bytes(payload, chunk.data_offset, "png_exif")
                    {
                        entries.extend(decode_from_tiff(payload, chunk.data_offset, "png", &tiff));
                    }
                }
            }
            for chunk in png.xmp_payloads() {
                if let Some(payload) = payload_slice(
                    source.bytes(),
                    chunk.data_offset,
                    chunk.data_length as usize,
                ) {
                    entries.extend(decode_png_text_chunk(
                        payload,
                        "png",
                        chunk.offset_start,
                        chunk.offset_end,
                    ));
                }
            }
            for chunk in png.iptc_payloads() {
                let Some(payload) = payload_slice(
                    source.bytes(),
                    chunk.data_offset,
                    chunk.data_length as usize,
                ) else {
                    continue;
                };
                let Some(iptc_bytes) = decode_png_iptc_payload(&chunk.chunk_type, payload) else {
                    continue;
                };
                if iptc_bytes.is_empty() {
                    issues.push(namespace_issue(
                        "png_iptc_payload_invalid",
                        "PNG IPTC text chunk could not be decoded",
                        chunk.offset_start,
                        &String::from_utf8_lossy(&chunk.chunk_type),
                    ));
                    continue;
                }
                let decoded = decode_iptc_payload(IptcPayload {
                    bytes: &iptc_bytes,
                    container: "png",
                    path: "png_iptc",
                    offset_start: chunk.offset_start,
                    offset_end: chunk.offset_end,
                });
                if decoded.is_empty() {
                    issues.push(namespace_issue(
                        "iptc_decode_empty",
                        "recognized IPTC payload but could not decode bounded IPTC datasets",
                        chunk.offset_start,
                        "png_iptc",
                    ));
                }
                entries.extend(decoded);
            }
            for chunk in png.icc_payloads() {
                if let Some(payload) = payload_slice(
                    source.bytes(),
                    chunk.data_offset,
                    chunk.data_length as usize,
                ) {
                    if let Some(icc_bytes) = decode_png_iccp_payload(payload) {
                        let decoded = decode_icc_payload(IccPayload {
                            bytes: &icc_bytes,
                            container: "png",
                            path: "iCCP",
                            offset_start: chunk.offset_start,
                            offset_end: chunk.offset_end,
                        });
                        if decoded.is_empty() {
                            issues.push(namespace_issue(
                                "icc_decode_empty",
                                "recognized ICC payload but could not decode bounded ICC fields",
                                chunk.offset_start,
                                "iCCP",
                            ));
                        }
                        entries.extend(decoded);
                    } else {
                        issues.push(namespace_issue(
                            "png_icc_payload_invalid",
                            "PNG iCCP payload could not be decompressed",
                            chunk.offset_start,
                            "iCCP",
                        ));
                    }
                }
            }
            ("png".to_string(), png.nodes, entries, issues)
        }
        Format::Webp => {
            let riff = parse_riff(&source)?;
            let mut entries = Vec::new();
            let mut issues = riff.issues.clone();
            for chunk in riff.xmp_payloads() {
                if let Some(payload) = payload_slice(
                    source.bytes(),
                    chunk.data_offset,
                    chunk.data_length as usize,
                ) {
                    entries.extend(decode_webp_xmp_chunk(
                        payload,
                        "webp",
                        chunk.offset_start,
                        chunk.offset_end,
                    ));
                }
            }
            for chunk in riff.exif_payloads() {
                if let Some(payload) = payload_slice(
                    source.bytes(),
                    chunk.data_offset,
                    chunk.data_length as usize,
                ) {
                    if let Ok(tiff) =
                        xifty_container_tiff::parse_bytes(payload, chunk.data_offset, "webp_exif")
                    {
                        entries.extend(decode_from_tiff(payload, chunk.data_offset, "webp", &tiff));
                    }
                }
            }
            for chunk in riff.iptc_payloads() {
                if let Some(payload) = payload_slice(
                    source.bytes(),
                    chunk.data_offset,
                    chunk.data_length as usize,
                ) {
                    let decoded = decode_iptc_payload(IptcPayload {
                        bytes: payload,
                        container: "webp",
                        path: "webp_iptc",
                        offset_start: chunk.offset_start,
                        offset_end: chunk.offset_end,
                    });
                    if decoded.is_empty() {
                        issues.push(namespace_issue(
                            "iptc_decode_empty",
                            "recognized IPTC payload but could not decode bounded IPTC datasets",
                            chunk.offset_start,
                            "webp_iptc",
                        ));
                    }
                    entries.extend(decoded);
                }
            }
            for chunk in riff.icc_payloads() {
                if let Some(payload) = payload_slice(
                    source.bytes(),
                    chunk.data_offset,
                    chunk.data_length as usize,
                ) {
                    let decoded = decode_icc_payload(IccPayload {
                        bytes: payload,
                        container: "webp",
                        path: "ICCP",
                        offset_start: chunk.offset_start,
                        offset_end: chunk.offset_end,
                    });
                    if decoded.is_empty() {
                        issues.push(namespace_issue(
                            "icc_decode_empty",
                            "recognized ICC payload but could not decode bounded ICC fields",
                            chunk.offset_start,
                            "ICCP",
                        ));
                    }
                    entries.extend(decoded);
                }
            }
            ("webp".to_string(), riff.nodes, entries, issues)
        }
        Format::Heif => {
            let isobmff = parse_isobmff(&source)?;
            let entries = isobmff_entries(&isobmff, source.bytes(), format.as_str());
            (
                "isobmff".to_string(),
                isobmff.nodes,
                entries,
                isobmff.issues,
            )
        }
        Format::Mp4 => {
            let isobmff = parse_isobmff(&source)?;
            let entries = isobmff_entries(&isobmff, source.bytes(), format.as_str());
            (
                "isobmff".to_string(),
                isobmff.nodes,
                entries,
                isobmff.issues,
            )
        }
        Format::Mov => {
            let isobmff = parse_isobmff(&source)?;
            let entries = isobmff_entries(&isobmff, source.bytes(), format.as_str());
            (
                "isobmff".to_string(),
                isobmff.nodes,
                entries,
                isobmff.issues,
            )
        }
    };

    let normalization = normalize_with_policy(&entries);
    let mut report = build_report(issues, &entries);
    report.conflicts.extend(normalization.conflicts);
    Ok(AnalysisOutput {
        schema_version: SCHEMA_VERSION.into(),
        input: ProbeInput {
            path: source.source.path.clone(),
            detected_format: format.as_str().into(),
            container: container_name,
        },
        raw: matches!(view_mode, ViewMode::Full | ViewMode::Raw).then(|| RawView {
            containers: nodes.clone(),
            metadata: entries.clone(),
        }),
        interpreted: matches!(view_mode, ViewMode::Full | ViewMode::Interpreted).then(|| {
            InterpretedView {
                metadata: entries.clone(),
            }
        }),
        normalized: matches!(view_mode, ViewMode::Full | ViewMode::Normalized).then(|| {
            xifty_core::NormalizedView {
                fields: normalization.fields,
            }
        }),
        report,
    })
}

fn browser_path(file_name: Option<String>) -> PathBuf {
    match file_name {
        Some(name) if !name.trim().is_empty() => PathBuf::from(name),
        _ => PathBuf::from("<memory>"),
    }
}

fn payload_slice(bytes: &[u8], absolute_offset: u64, len: usize) -> Option<&[u8]> {
    let start = usize::try_from(absolute_offset).ok()?;
    bytes.get(start..start + len)
}

fn decode_png_iccp_payload(payload: &[u8]) -> Option<Vec<u8>> {
    let separator = payload.iter().position(|byte| *byte == 0)?;
    let compression_method = *payload.get(separator + 1)?;
    if compression_method != 0 {
        return None;
    }
    let compressed = payload.get(separator + 2..)?;
    let mut decoder = ZlibDecoder::new(compressed);
    let mut decoded = Vec::new();
    decoder.read_to_end(&mut decoded).ok()?;
    Some(decoded)
}

/// Decode a PNG text chunk (tEXt/zTXt/iTXt) into raw IPTC IIM bytes.
///
/// Returns `None` when the chunk does not carry IPTC metadata. Returns
/// `Some(empty)` when the keyword matches but the payload is malformed
/// (so the caller can emit a targeted issue).
fn decode_png_iptc_payload(chunk_type: &[u8; 4], payload: &[u8]) -> Option<Vec<u8>> {
    let nul = payload.iter().position(|byte| *byte == 0)?;
    let keyword = &payload[..nul];
    let is_raw_profile_iptc = keyword.eq_ignore_ascii_case(b"Raw profile type iptc")
        || keyword.eq_ignore_ascii_case(b"Raw profile type 8bim");
    let is_direct_iptc = keyword == b"IPTC-NAA" || keyword.eq_ignore_ascii_case(b"iptc");
    if !is_raw_profile_iptc && !is_direct_iptc {
        return None;
    }

    // Extract the content bytes after the keyword framing, honoring chunk type.
    let content: Vec<u8> = match chunk_type {
        b"tEXt" => payload.get(nul + 1..)?.to_vec(),
        b"zTXt" => {
            // after keyword nul: 1-byte compression method, then zlib stream
            let method = *payload.get(nul + 1)?;
            if method != 0 {
                return Some(Vec::new());
            }
            let compressed = payload.get(nul + 2..)?;
            let mut decoder = ZlibDecoder::new(compressed);
            let mut decoded = Vec::new();
            if decoder.read_to_end(&mut decoded).is_err() {
                return Some(Vec::new());
            }
            decoded
        }
        b"iTXt" => {
            // after keyword nul: compression flag (1), compression method (1),
            // language tag (nul-terminated), translated keyword (nul-terminated), text
            let mut cursor = nul + 1;
            let compression_flag = *payload.get(cursor)?;
            cursor += 1;
            let compression_method = *payload.get(cursor)?;
            cursor += 1;
            let lang_end = payload.get(cursor..)?.iter().position(|b| *b == 0)?;
            cursor += lang_end + 1;
            let tr_end = payload.get(cursor..)?.iter().position(|b| *b == 0)?;
            cursor += tr_end + 1;
            let text = payload.get(cursor..)?;
            if compression_flag == 0 {
                text.to_vec()
            } else {
                if compression_method != 0 {
                    return Some(Vec::new());
                }
                let mut decoder = ZlibDecoder::new(text);
                let mut decoded = Vec::new();
                if decoder.read_to_end(&mut decoded).is_err() {
                    return Some(Vec::new());
                }
                decoded
            }
        }
        _ => return None,
    };

    if is_direct_iptc {
        return Some(content);
    }

    // ImageMagick "Raw profile type iptc" framing:
    //   "\n<profile-name>\n<spaces><decimal length>\n<hex bytes>\n"
    Some(decode_imagemagick_raw_profile(&content).unwrap_or_default())
}

fn decode_imagemagick_raw_profile(content: &[u8]) -> Option<Vec<u8>> {
    // Skip leading newline, then profile-name line, then length line, then hex.
    let text = std::str::from_utf8(content).ok()?;
    let trimmed = text.trim_start_matches('\n');
    let mut lines = trimmed.splitn(3, '\n');
    let _name = lines.next()?;
    let _length = lines.next()?.trim();
    let rest = lines.next()?;
    // rest contains hex possibly with whitespace; stop at terminating newline/content.
    let hex: String = rest.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.is_empty() || hex.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    let bytes = hex.as_bytes();
    for chunk in bytes.chunks(2) {
        let high = hex_digit(chunk[0])?;
        let low = hex_digit(chunk[1])?;
        out.push((high << 4) | low);
    }
    Some(out)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn namespace_issue(code: &str, message: &str, offset: u64, context: &str) -> Issue {
    Issue {
        severity: Severity::Warning,
        code: code.into(),
        message: message.into(),
        offset: Some(offset),
        context: Some(context.into()),
    }
}

fn heif_exif_tiff(payload: &[u8], absolute_offset: u64) -> Option<(u64, &[u8])> {
    if payload.len() >= 10 {
        let offset = u32::from_be_bytes(payload[0..4].try_into().ok()?) as usize;
        let start = 4usize.checked_add(offset)?;
        let tiff = payload.get(start..)?;
        if tiff.starts_with(b"II") || tiff.starts_with(b"MM") {
            return Some((absolute_offset + start as u64, tiff));
        }
    }

    if payload.starts_with(b"II") || payload.starts_with(b"MM") {
        return Some((absolute_offset, payload));
    }

    None
}

fn heif_dimension_entries(
    dimensions: &xifty_container_isobmff::IsobmffDimensions,
) -> Vec<MetadataEntry> {
    let provenance = Provenance {
        container: "heif".into(),
        namespace: "heif".into(),
        path: Some(dimensions.path.clone()),
        offset_start: Some(dimensions.offset_start),
        offset_end: Some(dimensions.offset_end),
        notes: vec!["derived from ispe property for primary item".into()],
    };

    vec![
        MetadataEntry {
            namespace: "heif".into(),
            tag_id: "ImageWidth".into(),
            tag_name: "ImageWidth".into(),
            value: TypedValue::Integer(dimensions.width as i64),
            provenance: provenance.clone(),
            notes: Vec::new(),
        },
        MetadataEntry {
            namespace: "heif".into(),
            tag_id: "ImageHeight".into(),
            tag_name: "ImageHeight".into(),
            value: TypedValue::Integer(dimensions.height as i64),
            provenance,
            notes: Vec::new(),
        },
    ]
}

fn isobmff_entries(
    container: &xifty_container_isobmff::IsobmffContainer,
    bytes: &[u8],
    format_name: &str,
) -> Vec<MetadataEntry> {
    let mut entries = Vec::new();

    for payload in container.exif_payloads() {
        if let Some(payload_bytes) =
            payload_slice(bytes, payload.data_offset, payload.data_length as usize)
        {
            let tiff_view = if format_name == "heif" {
                heif_exif_tiff(payload_bytes, payload.data_offset)
            } else if payload_bytes.starts_with(b"II") || payload_bytes.starts_with(b"MM") {
                Some((payload.data_offset, payload_bytes))
            } else {
                None
            };

            if let Some((tiff_offset, tiff_bytes)) = tiff_view {
                if let Ok(tiff) = xifty_container_tiff::parse_bytes(
                    tiff_bytes,
                    tiff_offset,
                    &format!("{format_name}_exif"),
                ) {
                    entries.extend(decode_from_tiff(
                        tiff_bytes,
                        tiff_offset,
                        format_name,
                        &tiff,
                    ));
                }
            }
        }
    }

    for payload in container.xmp_payloads() {
        if let Some(payload_bytes) =
            payload_slice(bytes, payload.data_offset, payload.data_length as usize)
        {
            let rtmd_entries = decode_rtmd_packet(RtmdPacket {
                bytes: payload_bytes,
                container: format_name,
                offset_start: payload.offset_start,
                offset_end: payload.offset_end,
            });
            if rtmd_entries.is_empty() {
                entries.extend(decode_packet(XmpPacket {
                    bytes: payload_bytes,
                    container: format_name,
                    offset_start: payload.offset_start,
                    offset_end: payload.offset_end,
                }));
            } else {
                entries.extend(rtmd_entries);
            }
        }
    }

    for payload in container.quicktime_payloads() {
        if let (Some(tag), Some(payload_bytes)) = (
            payload.tag.as_deref(),
            payload_slice(bytes, payload.data_offset, payload.data_length as usize),
        ) {
            entries.extend(decode_quicktime_payload(QuickTimePayload {
                key: tag,
                bytes: payload_bytes,
                container: format_name,
                offset_start: payload.offset_start,
                offset_end: payload.offset_end,
            }));
        }
    }

    if let Some(dimensions) = &container.primary_item_dimensions {
        entries.extend(heif_dimension_entries(dimensions));
    }
    if let Some(dimensions) = &container.primary_visual_dimensions {
        entries.extend(media_dimension_entries(dimensions, format_name));
    }
    if let Some(duration) = container.media_duration_seconds {
        entries.push(media_scalar_entry(
            format_name,
            "DurationSeconds",
            TypedValue::Float(duration),
            "derived from mvhd or media track timing",
        ));
    }
    if let Some(codec) = &container.video_codec {
        entries.push(media_scalar_entry(
            format_name,
            "VideoCodec",
            TypedValue::String(codec.clone()),
            "derived from video track sample description",
        ));
    }
    if let Some(codec) = &container.audio_codec {
        entries.push(media_scalar_entry(
            format_name,
            "AudioCodec",
            TypedValue::String(codec.clone()),
            "derived from audio track sample description",
        ));
    }
    if let Some(frame_rate) = container.video_frame_rate {
        entries.push(media_scalar_entry(
            format_name,
            "VideoFrameRate",
            TypedValue::Float(frame_rate),
            "derived from video track timing",
        ));
    }
    if let Some(bitrate) = container.video_bitrate {
        entries.push(media_scalar_entry(
            format_name,
            "VideoBitrate",
            TypedValue::Integer(bitrate as i64),
            container
                .video_bitrate_note
                .as_deref()
                .unwrap_or("derived from video track metadata"),
        ));
    }
    if let Some(channels) = container.audio_channels {
        entries.push(media_scalar_entry(
            format_name,
            "AudioChannels",
            TypedValue::Integer(channels as i64),
            "derived from audio track sample entry",
        ));
    }
    if let Some(sample_rate) = container.audio_sample_rate {
        entries.push(media_scalar_entry(
            format_name,
            "AudioSampleRate",
            TypedValue::Integer(sample_rate as i64),
            "derived from audio track sample entry",
        ));
    }
    if let Some(created_at) = &container.media_created_at {
        entries.push(media_scalar_entry(
            format_name,
            "CreateDate",
            TypedValue::Timestamp(created_at.clone()),
            "derived from movie header creation time",
        ));
    }
    if let Some(modified_at) = &container.media_modified_at {
        entries.push(media_scalar_entry(
            format_name,
            "ModifyDate",
            TypedValue::Timestamp(modified_at.clone()),
            "derived from movie header modification time",
        ));
    }

    entries
}

fn media_dimension_entries(
    dimensions: &xifty_container_isobmff::IsobmffDimensions,
    container_name: &str,
) -> Vec<MetadataEntry> {
    let provenance = Provenance {
        container: container_name.into(),
        namespace: "quicktime".into(),
        path: Some(dimensions.path.clone()),
        offset_start: Some(dimensions.offset_start),
        offset_end: Some(dimensions.offset_end),
        notes: vec!["derived from visual track header".into()],
    };

    vec![
        MetadataEntry {
            namespace: "quicktime".into(),
            tag_id: "ImageWidth".into(),
            tag_name: "ImageWidth".into(),
            value: TypedValue::Integer(dimensions.width as i64),
            provenance: provenance.clone(),
            notes: Vec::new(),
        },
        MetadataEntry {
            namespace: "quicktime".into(),
            tag_id: "ImageHeight".into(),
            tag_name: "ImageHeight".into(),
            value: TypedValue::Integer(dimensions.height as i64),
            provenance,
            notes: Vec::new(),
        },
    ]
}

fn media_scalar_entry(
    container_name: &str,
    tag_name: &str,
    value: TypedValue,
    note: &str,
) -> MetadataEntry {
    MetadataEntry {
        namespace: "quicktime".into(),
        tag_id: tag_name.into(),
        tag_name: tag_name.into(),
        value,
        provenance: Provenance {
            container: container_name.into(),
            namespace: "quicktime".into(),
            path: None,
            offset_start: None,
            offset_end: None,
            notes: vec![note.into()],
        },
        notes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::ZlibEncoder};
    use std::io::Write;

    fn imagemagick_framing(iim: &[u8]) -> Vec<u8> {
        let mut hex = String::new();
        for (i, byte) in iim.iter().enumerate() {
            if i % 36 == 0 {
                hex.push('\n');
            }
            hex.push_str(&format!("{:02x}", byte));
        }
        hex.push('\n');
        let mut out = Vec::new();
        out.extend_from_slice(b"\niptc\n");
        out.extend_from_slice(format!("      {}", iim.len()).as_bytes());
        out.extend_from_slice(hex.as_bytes());
        out
    }

    fn iim_sample() -> Vec<u8> {
        // Record 2, dataset 80 (By-line), value "Kai"
        let mut out = Vec::new();
        let text = b"Kai";
        out.extend_from_slice(&[0x1C, 2, 80]);
        out.extend_from_slice(&(text.len() as u16).to_be_bytes());
        out.extend_from_slice(text);
        out
    }

    #[test]
    fn decodes_raw_profile_iptc_ztxt() {
        let iim = iim_sample();
        let framing = imagemagick_framing(&iim);
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&framing).unwrap();
        let compressed = encoder.finish().unwrap();
        let mut payload = Vec::new();
        payload.extend_from_slice(b"Raw profile type iptc\x00");
        payload.push(0u8); // compression method
        payload.extend_from_slice(&compressed);
        let decoded = decode_png_iptc_payload(b"zTXt", &payload).expect("keyword matches");
        assert_eq!(decoded, iim);
    }

    #[test]
    fn decodes_raw_profile_iptc_text() {
        let iim = iim_sample();
        let framing = imagemagick_framing(&iim);
        let mut payload = Vec::new();
        payload.extend_from_slice(b"Raw profile type iptc\x00");
        payload.extend_from_slice(&framing);
        let decoded = decode_png_iptc_payload(b"tEXt", &payload).expect("keyword matches");
        assert_eq!(decoded, iim);
    }

    #[test]
    fn decodes_direct_iptc_naa_text() {
        let iim = iim_sample();
        let mut payload = Vec::new();
        payload.extend_from_slice(b"IPTC-NAA\x00");
        payload.extend_from_slice(&iim);
        let decoded = decode_png_iptc_payload(b"tEXt", &payload).expect("keyword matches");
        assert_eq!(decoded, iim);
    }

    #[test]
    fn decodes_direct_iptc_naa_ztxt() {
        let iim = iim_sample();
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&iim).unwrap();
        let compressed = encoder.finish().unwrap();
        let mut payload = Vec::new();
        payload.extend_from_slice(b"IPTC-NAA\x00");
        payload.push(0u8);
        payload.extend_from_slice(&compressed);
        let decoded = decode_png_iptc_payload(b"zTXt", &payload).expect("keyword matches");
        assert_eq!(decoded, iim);
    }

    #[test]
    fn ignores_non_iptc_keywords() {
        let mut payload = Vec::new();
        payload.extend_from_slice(b"XML:com.adobe.xmp\x00");
        payload.extend_from_slice(b"irrelevant");
        assert!(decode_png_iptc_payload(b"tEXt", &payload).is_none());
    }
}
