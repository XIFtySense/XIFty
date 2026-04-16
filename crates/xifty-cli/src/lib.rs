use xifty_container_jpeg::parse as parse_jpeg;
use xifty_container_png::parse as parse_png;
use xifty_container_riff::parse as parse_riff;
use xifty_container_tiff::parse as parse_tiff;
use xifty_core::{
    AnalysisOutput, Format, InterpretedView, ProbeInput, ProbeOutput, RawView, SCHEMA_VERSION,
    ViewMode, XiftyError,
};
use xifty_detect::detect;
use xifty_meta_exif::{decode_from_tiff, exif_payload_from_jpeg};
use xifty_meta_xmp::{decode_png_text_chunk, decode_webp_xmp_chunk};
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
