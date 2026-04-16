use xifty_container_isobmff::parse as parse_isobmff;
use xifty_container_jpeg::parse as parse_jpeg;
use xifty_container_png::parse as parse_png;
use xifty_container_riff::parse as parse_riff;
use xifty_container_tiff::parse as parse_tiff;
use xifty_core::{
    AnalysisOutput, Format, InterpretedView, MetadataEntry, ProbeInput, ProbeOutput, Provenance,
    RawView, SCHEMA_VERSION, TypedValue, ViewMode, XiftyError,
};
use xifty_detect::detect;
use xifty_meta_exif::{decode_from_tiff, exif_payload_from_jpeg};
use xifty_meta_xmp::{XmpPacket, decode_packet, decode_png_text_chunk, decode_webp_xmp_chunk};
use xifty_normalize::normalize_with_policy;
use xifty_source::SourceBytes;
use xifty_validate::build_report;

pub fn probe_path(path: std::path::PathBuf) -> Result<ProbeOutput, XiftyError> {
    let source = SourceBytes::from_path(&path)?;
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
    };
    Ok(ProbeOutput {
        schema_version: SCHEMA_VERSION.into(),
        input: ProbeInput {
            path,
            detected_format: format.as_str().into(),
            container,
        },
        containers: nodes,
        report: build_report(issues, &[]),
    })
}

pub fn extract_path(
    path: std::path::PathBuf,
    view_mode: ViewMode,
) -> Result<AnalysisOutput, XiftyError> {
    let source = SourceBytes::from_path(&path)?;
    let format = detect(&source)?;

    let (container_name, nodes, entries, issues) = match format {
        Format::Jpeg => {
            let jpeg = parse_jpeg(&source)?;
            let mut issues = jpeg.issues.clone();
            let entries = if let Some((base_offset, exif_payload)) = exif_payload_from_jpeg(&jpeg) {
                let tiff =
                    xifty_container_tiff::parse_bytes(exif_payload, base_offset, "jpeg_exif")?;
                issues.extend(tiff.issues.clone());
                decode_from_tiff(exif_payload, base_offset, "jpeg", &tiff)
            } else {
                Vec::new()
            };
            ("jpeg".to_string(), jpeg.nodes, entries, issues)
        }
        Format::Tiff => {
            let tiff = parse_tiff(&source)?;
            let entries = decode_from_tiff(source.bytes(), 0, "tiff", &tiff);
            ("tiff".to_string(), tiff.nodes, entries, tiff.issues)
        }
        Format::Png => {
            let png = parse_png(&source)?;
            let mut entries = Vec::new();
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
            ("png".to_string(), png.nodes, entries, png.issues)
        }
        Format::Webp => {
            let riff = parse_riff(&source)?;
            let mut entries = Vec::new();
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
            ("webp".to_string(), riff.nodes, entries, riff.issues)
        }
        Format::Heif => {
            let isobmff = parse_isobmff(&source)?;
            let mut entries = Vec::new();
            for payload in isobmff.exif_payloads() {
                if let Some(bytes) = payload_slice(
                    source.bytes(),
                    payload.data_offset,
                    payload.data_length as usize,
                ) {
                    if let Some((tiff_offset, tiff_bytes)) =
                        heif_exif_tiff(bytes, payload.data_offset)
                    {
                        if let Ok(tiff) =
                            xifty_container_tiff::parse_bytes(tiff_bytes, tiff_offset, "heif_exif")
                        {
                            entries.extend(decode_from_tiff(
                                tiff_bytes,
                                tiff_offset,
                                "heif",
                                &tiff,
                            ));
                        }
                    }
                }
            }
            for payload in isobmff.xmp_payloads() {
                if let Some(bytes) = payload_slice(
                    source.bytes(),
                    payload.data_offset,
                    payload.data_length as usize,
                ) {
                    entries.extend(decode_packet(XmpPacket {
                        bytes,
                        container: "heif",
                        offset_start: payload.offset_start,
                        offset_end: payload.offset_end,
                    }));
                }
            }
            if let Some(dimensions) = &isobmff.primary_item_dimensions {
                entries.extend(heif_dimension_entries(dimensions));
            }
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
    report.conflicts = normalization.conflicts;
    Ok(AnalysisOutput {
        schema_version: SCHEMA_VERSION.into(),
        input: ProbeInput {
            path,
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

fn payload_slice(bytes: &[u8], absolute_offset: u64, len: usize) -> Option<&[u8]> {
    let start = usize::try_from(absolute_offset).ok()?;
    bytes.get(start..start + len)
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
