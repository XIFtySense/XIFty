use flate2::read::ZlibDecoder;
#[cfg(target_os = "macos")]
use std::process::Command;
use std::{fs, io::Read, path::PathBuf, time::SystemTime};
use xifty_container_aiff::{AiffContainer, parse as parse_aiff};
use xifty_container_flac::{FlacContainer, parse as parse_flac};
use xifty_container_gif::{GifContainer, parse as parse_gif};
use xifty_container_id3::{Id3Container, parse as parse_mp3};
use xifty_container_isobmff::parse as parse_isobmff;
use xifty_container_jpeg::parse as parse_jpeg;
use xifty_container_ogg::{OggCodec, OggContainer, parse as parse_ogg};
use xifty_container_png::parse as parse_png;
use xifty_container_raf::{embedded_tiff_slice as raf_embedded_tiff_slice, parse as parse_raf};
use xifty_container_riff::parse as parse_riff;
use xifty_container_rw2::parse as parse_rw2;
use xifty_container_tiff::parse as parse_tiff;
use xifty_core::{
    AnalysisOutput, Format, InterpretedView, Issue, MetadataEntry, ProbeInput, ProbeOutput,
    Provenance, RawView, SCHEMA_VERSION, Severity, TypedValue, ViewMode, XiftyError,
};
use xifty_detect::detect;
use xifty_meta_apple::decode_from_tiff as decode_apple_from_tiff;
use xifty_meta_bwf::{BwfPayload, decode_payload as decode_bwf_payload};
use xifty_meta_canon::decode_from_tiff as decode_canon_from_tiff;
use xifty_meta_exif::{decode_from_tiff, exif_payload_from_jpeg};
use xifty_meta_fuji::decode_from_tiff as decode_fuji_from_tiff;
use xifty_meta_icc::{IccPayload, decode_payload as decode_icc_payload};
use xifty_meta_id3v2::{
    Id3v2Payload as Id3v2DecodePayload, decode_payload as decode_id3v2_payload,
};
use xifty_meta_iptc::{IptcPayload, decode_payload as decode_iptc_payload};
use xifty_meta_itunes::{ItunesPayload, decode_payload as decode_itunes_payload};
use xifty_meta_ixml::{IxmlPayload, decode_payload as decode_ixml_payload};
use xifty_meta_nikon::{
    decode_from_tiff as decode_nikon_from_tiff, encrypted_regions as nikon_encrypted_regions,
};
use xifty_meta_olympus::decode_from_tiff as decode_olympus_from_tiff;
use xifty_meta_panasonic::decode_from_rw2;
use xifty_meta_quicktime::{
    QuickTimePayload, QuickTimeUdtaPayload, decode_payload as decode_quicktime_payload,
    decode_udta_payload,
};
use xifty_meta_rtmd::{RtmdPacket, decode_packet as decode_rtmd_packet};
use xifty_meta_sony::decode_from_tiff as decode_sony_from_tiff;
use xifty_meta_sony_video::{decode_prof, decode_usmt};
use xifty_meta_vorbis_comment::{
    VorbisCommentPayload, decode_payload as decode_vorbis_comment_payload,
};
use xifty_meta_xmp::{XmpPacket, decode_packet, decode_png_text_chunk, decode_webp_xmp_chunk};
use xifty_normalize::normalize_with_policy;
use xifty_source::SourceBytes;
use xifty_validate::build_report;

mod conflict_dedupe;
use conflict_dedupe::dedupe_conflicts;

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
        Format::Dng => {
            let parsed = parse_tiff(&source)?;
            ("dng".to_string(), parsed.nodes, parsed.issues)
        }
        Format::Cr2 => {
            let parsed = parse_tiff(&source)?;
            ("cr2".to_string(), parsed.nodes, parsed.issues)
        }
        Format::Cr3 => {
            let parsed = parse_isobmff(&source)?;
            ("cr3".to_string(), parsed.nodes, parsed.issues)
        }
        Format::Arw => {
            let parsed = parse_tiff(&source)?;
            ("arw".to_string(), parsed.nodes, parsed.issues)
        }
        Format::Raf => {
            let parsed = parse_raf(&source)?;
            ("raf".to_string(), parsed.nodes, parsed.issues)
        }
        Format::Orf => {
            let parsed = xifty_container_tiff::parse_bytes_accepting(
                source.bytes(),
                0,
                "orf",
                ORF_TIFF_MAGICS,
            )?;
            ("orf".to_string(), parsed.nodes, parsed.issues)
        }
        Format::Rw2 => {
            let parsed = parse_rw2(&source)?;
            ("rw2".to_string(), parsed.nodes, parsed.issues)
        }
        Format::Nef => {
            let parsed = parse_tiff(&source)?;
            ("nef".to_string(), parsed.nodes, parsed.issues)
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
        Format::Avif => {
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
        Format::M4a => {
            let parsed = parse_isobmff(&source)?;
            ("isobmff".to_string(), parsed.nodes, parsed.issues)
        }
        Format::Flac => {
            let parsed = parse_flac(&source)?;
            ("flac".to_string(), parsed.nodes, parsed.issues)
        }
        Format::Aiff => {
            let parsed = parse_aiff(&source)?;
            ("aiff".to_string(), parsed.nodes, parsed.issues)
        }
        Format::Ogg => {
            let parsed = parse_ogg(&source)?;
            ("ogg".to_string(), parsed.nodes, parsed.issues)
        }
        Format::Mp3 => {
            let parsed = parse_mp3(&source)?;
            ("mp3".to_string(), parsed.nodes, parsed.issues)
        }
        Format::Wav => {
            let parsed = parse_riff(&source)?;
            ("wav".to_string(), parsed.nodes, parsed.issues)
        }
        Format::Gif => {
            let parsed = parse_gif(&source)?;
            ("gif".to_string(), parsed.nodes, parsed.issues)
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
    let filesystem_metadata = fs::metadata(&path).ok();
    extract_source(&source, view_mode, filesystem_metadata.as_ref())
}

pub fn extract_bytes(
    bytes: Vec<u8>,
    file_name: Option<String>,
    view_mode: ViewMode,
) -> Result<AnalysisOutput, XiftyError> {
    let source = SourceBytes::new(browser_path(file_name), bytes);
    extract_source(&source, view_mode, None)
}

fn extract_source(
    source: &SourceBytes,
    view_mode: ViewMode,
    filesystem_metadata: Option<&fs::Metadata>,
) -> Result<AnalysisOutput, XiftyError> {
    let format = detect(&source)?;

    let (container_name, nodes, mut entries, issues) = match format {
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
        Format::Tiff => tiff_extract(&source, "tiff")?,
        Format::Dng => tiff_extract(&source, "dng")?,
        Format::Cr2 => tiff_extract(&source, "cr2")?,
        Format::Cr3 => cr3_extract(&source)?,
        Format::Arw => tiff_extract(&source, "arw")?,
        Format::Raf => raf_extract(&source)?,
        Format::Orf => orf_extract(&source)?,
        Format::Rw2 => rw2_extract(&source)?,
        Format::Nef => tiff_extract(&source, "nef")?,
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
            for chunk in png.text_payloads() {
                let Some(payload) = payload_slice(
                    source.bytes(),
                    chunk.data_offset,
                    chunk.data_length as usize,
                ) else {
                    continue;
                };
                match decode_png_creation_time_payload(&chunk.chunk_type, payload) {
                    Ok(Some(value)) => entries.push(png_timestamp_entry(
                        "CreationTime",
                        "CreateDate",
                        value,
                        chunk.offset_start,
                        chunk.offset_end,
                        &String::from_utf8_lossy(&chunk.chunk_type),
                    )),
                    Ok(None) => {}
                    Err(()) => issues.push(namespace_issue(
                        "png_creation_time_payload_invalid",
                        "PNG Creation Time text chunk could not be decoded",
                        chunk.offset_start,
                        &String::from_utf8_lossy(&chunk.chunk_type),
                    )),
                }
            }
            for chunk in png.time_payloads() {
                let Some(payload) = payload_slice(
                    source.bytes(),
                    chunk.data_offset,
                    chunk.data_length as usize,
                ) else {
                    continue;
                };
                if let Some(value) = decode_png_time_payload(payload) {
                    entries.push(png_timestamp_entry(
                        "tIME",
                        "CreateDate",
                        value,
                        chunk.offset_start,
                        chunk.offset_end,
                        "tIME",
                    ));
                } else {
                    issues.push(namespace_issue(
                        "png_time_payload_invalid",
                        "PNG tIME chunk could not be decoded",
                        chunk.offset_start,
                        "tIME",
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
            let mut issues = isobmff.issues.clone();
            let entries = isobmff_entries(&isobmff, source.bytes(), format.as_str(), &mut issues);
            ("isobmff".to_string(), isobmff.nodes, entries, issues)
        }
        Format::Avif => {
            let isobmff = parse_isobmff(&source)?;
            let mut issues = isobmff.issues.clone();
            let entries = isobmff_entries(&isobmff, source.bytes(), format.as_str(), &mut issues);
            ("isobmff".to_string(), isobmff.nodes, entries, issues)
        }
        Format::Mp4 => {
            let isobmff = parse_isobmff(&source)?;
            let mut issues = isobmff.issues.clone();
            let entries = isobmff_entries(&isobmff, source.bytes(), format.as_str(), &mut issues);
            ("isobmff".to_string(), isobmff.nodes, entries, issues)
        }
        Format::Mov => {
            let isobmff = parse_isobmff(&source)?;
            let mut issues = isobmff.issues.clone();
            let entries = isobmff_entries(&isobmff, source.bytes(), format.as_str(), &mut issues);
            ("isobmff".to_string(), isobmff.nodes, entries, issues)
        }
        Format::M4a => {
            let isobmff = parse_isobmff(&source)?;
            let mut issues = isobmff.issues.clone();
            let mut entries =
                isobmff_entries(&isobmff, source.bytes(), format.as_str(), &mut issues);
            entries.extend(itunes_entries(&isobmff, source.bytes(), format.as_str()));
            ("isobmff".to_string(), isobmff.nodes, entries, issues)
        }
        Format::Flac => {
            let flac = parse_flac(&source)?;
            let mut issues = flac.issues.clone();
            let entries = flac_entries(&flac, source.bytes(), &mut issues);
            ("flac".to_string(), flac.nodes, entries, issues)
        }
        Format::Aiff => {
            let aiff = parse_aiff(&source)?;
            let issues = aiff.issues.clone();
            let entries = aiff_entries(&aiff);
            ("aiff".to_string(), aiff.nodes, entries, issues)
        }
        Format::Ogg => {
            let ogg = parse_ogg(&source)?;
            let mut issues = ogg.issues.clone();
            let entries = ogg_entries(&ogg, source.bytes(), &mut issues);
            ("ogg".to_string(), ogg.nodes, entries, issues)
        }
        Format::Mp3 => {
            let mp3 = parse_mp3(&source)?;
            let issues = mp3.issues.clone();
            let entries = mp3_entries(&mp3, source.bytes());
            ("mp3".to_string(), mp3.nodes, entries, issues)
        }
        Format::Wav => {
            let riff = parse_riff(&source)?;
            let mut issues = riff.issues.clone();
            let entries = wav_entries(&riff, source.bytes(), &mut issues);
            ("wav".to_string(), riff.nodes, entries, issues)
        }
        Format::Gif => {
            let gif = parse_gif(&source)?;
            let mut issues = gif.issues.clone();
            let entries = gif_entries(&gif, &mut issues);
            ("gif".to_string(), gif.nodes, entries, issues)
        }
    };

    add_filesystem_timestamp_fallbacks(
        &mut entries,
        &source.source.path,
        &format,
        filesystem_metadata,
    );

    let normalization = normalize_with_policy(&entries);
    let mut report = build_report(issues, &entries);
    let mut merged = std::mem::take(&mut report.conflicts);
    merged.extend(normalization.conflicts);
    report.conflicts = dedupe_conflicts(merged);
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

/// Shared extraction path for TIFF-shaped containers (TIFF, DNG, CR2, ARW).
///
/// Keyed on `container_label` so snapshot output identifies the source
/// container faithfully while reusing the same parse + namespace decoders.
fn tiff_extract(
    source: &SourceBytes,
    container_label: &'static str,
) -> Result<
    (
        String,
        Vec<xifty_core::ContainerNode>,
        Vec<MetadataEntry>,
        Vec<Issue>,
    ),
    XiftyError,
> {
    let tiff = parse_tiff(source)?;
    let mut issues = tiff.issues.clone();
    let mut entries = decode_from_tiff(source.bytes(), 0, container_label, &tiff);
    entries.extend(decode_apple_from_tiff(
        source.bytes(),
        container_label,
        &tiff,
        &entries,
    ));
    entries.extend(decode_sony_from_tiff(
        source.bytes(),
        0,
        container_label,
        &tiff,
        &entries,
    ));
    entries.extend(decode_canon_from_tiff(
        source.bytes(),
        0,
        container_label,
        &tiff,
        &entries,
    ));
    entries.extend(decode_nikon_from_tiff(
        source.bytes(),
        0,
        container_label,
        &tiff,
        &entries,
    ));
    // Surface Nikon encrypted MakerNote regions as non-fatal Issues. The
    // decoder above intentionally skips them (XIFty does not attempt
    // decryption at v1); this loop turns each skipped region into a
    // discoverable Warning so downstream consumers know the data is
    // present-but-opaque rather than missing.
    for region in nikon_encrypted_regions(source.bytes(), 0, &tiff) {
        issues.push(namespace_issue(
            "nikon_makernote_encrypted_region",
            &format!(
                "Nikon MakerNote tag 0x{:04X} is encrypted; skipping per v1 policy",
                region.tag_id
            ),
            region.absolute_offset,
            "ifd0_makernote",
        ));
    }
    if let Some((offset_start, payload)) = xifty_container_tiff::xmp_payload(source.bytes(), &tiff)
    {
        let decoded = decode_packet(XmpPacket {
            bytes: payload,
            container: container_label,
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
    if let Some((offset_start, payload)) = xifty_container_tiff::icc_payload(source.bytes(), &tiff)
    {
        let decoded = decode_icc_payload(IccPayload {
            bytes: payload,
            container: container_label,
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
    if let Some((offset_start, payload)) = xifty_container_tiff::iptc_payload(source.bytes(), &tiff)
    {
        let decoded = decode_iptc_payload(IptcPayload {
            bytes: payload,
            container: container_label,
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
    Ok((container_label.to_string(), tiff.nodes, entries, issues))
}

/// Extraction path for Canon CR3 (ISOBMFF-based RAW).
///
/// CR3 stores its EXIF + Canon maker-note IFDs in CMT* sub-boxes inside a
/// Canon-specific `uuid` box under `moov/uuid`. The container parser
/// surfaces:
///
/// - CMT1 → `IsobmffPayload { kind: "exif", tag: Some("CMT1") }`
///   (routed through the regular EXIF + Canon pipeline)
/// - CMT2 / CMT3 / CMT4 → `IsobmffPayload { kind: "canon-cmt", tag: ... }`
///   (each parsed as a single TIFF and decoded via `xifty-meta-canon` with a
///   distinct `container_name` so downstream provenance distinguishes the
///   IFDs).
///
/// Per plan-reviewer ruling, no multi-IFD overload is added to
/// `xifty-meta-canon`; each CMT payload calls the existing
/// `decode_from_tiff` once with its own container label.
fn cr3_extract(
    source: &SourceBytes,
) -> Result<
    (
        String,
        Vec<xifty_core::ContainerNode>,
        Vec<MetadataEntry>,
        Vec<Issue>,
    ),
    XiftyError,
> {
    let isobmff = parse_isobmff(source)?;
    let issues = isobmff.issues.clone();
    let mut entries = Vec::new();

    // CMT1 carries EXIF (Make/Model/DateTimeOriginal/etc.) — flow through
    // EXIF then Canon, mirroring the JPEG/TIFF pipelines so Make=Canon
    // gates the Canon decoder consistently.
    for payload in isobmff.exif_payloads() {
        let Some(payload_bytes) = payload_slice(
            source.bytes(),
            payload.data_offset,
            payload.data_length as usize,
        ) else {
            continue;
        };
        if !(payload_bytes.starts_with(b"II") || payload_bytes.starts_with(b"MM")) {
            continue;
        }
        let Ok(tiff) =
            xifty_container_tiff::parse_bytes(payload_bytes, payload.data_offset, "cr3_exif")
        else {
            continue;
        };
        let mut exif_entries = decode_from_tiff(payload_bytes, payload.data_offset, "cr3", &tiff);
        exif_entries.extend(decode_canon_from_tiff(
            payload_bytes,
            payload.data_offset,
            "cr3",
            &tiff,
            &exif_entries,
        ));
        entries.extend(exif_entries);
    }

    // Reuse the EXIF entries we already accumulated above so the Canon
    // decoder's `Make=Canon` gate is satisfied for the CMT2/3/4 IFDs.
    let exif_entries_for_gate: Vec<MetadataEntry> = entries
        .iter()
        .filter(|entry| entry.namespace == "exif")
        .cloned()
        .collect();

    for payload in isobmff.canon_cmt_payloads() {
        let Some(payload_bytes) = payload_slice(
            source.bytes(),
            payload.data_offset,
            payload.data_length as usize,
        ) else {
            continue;
        };
        if !(payload_bytes.starts_with(b"II") || payload_bytes.starts_with(b"MM")) {
            continue;
        }
        let container_name: &'static str = match payload.tag.as_deref() {
            Some("CMT2") => "cr3-cmt2",
            Some("CMT3") => "cr3-cmt3",
            Some("CMT4") => "cr3-cmt4",
            _ => "cr3-cmt",
        };
        let Ok(tiff) =
            xifty_container_tiff::parse_bytes(payload_bytes, payload.data_offset, "cr3_canon_cmt")
        else {
            continue;
        };
        entries.extend(decode_canon_from_tiff(
            payload_bytes,
            payload.data_offset,
            container_name,
            &tiff,
            &exif_entries_for_gate,
        ));
    }

    Ok(("cr3".to_string(), isobmff.nodes, entries, issues))
}

/// Extraction path for Fuji RAF.
///
/// RAF is a custom container that wraps an embedded EXIF TIFF inside its JPEG
/// preview block. The container parser surfaces the byte layout; metadata
/// decoding delegates to the existing TIFF parser + EXIF decoder + the Fuji
/// MakerNote decoder, preserving the SRS §3.1 separation between container
/// parsing and metadata interpretation. Apple's MakerNote decoder is
/// intentionally not invoked here — it targets Apple JPEG MakerNote, not
/// Fuji's, and would never match against a Fuji `Make`.
fn raf_extract(
    source: &SourceBytes,
) -> Result<
    (
        String,
        Vec<xifty_core::ContainerNode>,
        Vec<MetadataEntry>,
        Vec<Issue>,
    ),
    XiftyError,
> {
    let raf = parse_raf(source)?;
    let mut nodes = raf.nodes.clone();
    let mut issues = raf.issues.clone();
    let mut entries: Vec<MetadataEntry> = Vec::new();

    if let Some((tiff_offset, tiff_payload)) = raf_embedded_tiff_slice(source.bytes(), &raf) {
        match xifty_container_tiff::parse_bytes(tiff_payload, tiff_offset, "raf_exif") {
            Ok(tiff) => {
                nodes.extend(tiff.nodes.clone());
                issues.extend(tiff.issues.clone());
                let exif_entries = decode_from_tiff(tiff_payload, tiff_offset, "raf", &tiff);
                entries.extend(exif_entries.clone());
                entries.extend(decode_fuji_from_tiff(
                    tiff_payload,
                    tiff_offset,
                    "raf",
                    &tiff,
                    &exif_entries,
                ));
            }
            Err(_) => {
                issues.push(namespace_issue(
                    "raf_embedded_tiff_parse_failed",
                    "embedded EXIF TIFF inside RAF preview could not be parsed",
                    tiff_offset,
                    "raf_embedded_tiff",
                ));
            }
        }
    }

    Ok(("raf".to_string(), nodes, entries, issues))
}

/// Olympus ORF accepted TIFF magic words (post-endianness u16):
/// `RO` (`IIRO` LE / `MMOR` BE = 0x4F52) and `RS` (`IIRS` LE = 0x5352).
const ORF_TIFF_MAGICS: &[u16] = &[0x4F52, 0x5352];

/// Extraction path for Olympus ORF.
///
/// ORF is byte-for-byte TIFF after a vendor magic word, so we reuse the
/// existing TIFF entry parsers via [`xifty_container_tiff::parse_bytes_accepting`]
/// and chain the standard EXIF / XMP / ICC / IPTC decoders alongside the
/// Olympus MakerNote decoder. The Apple, Sony, Canon, and Fuji MakerNote
/// decoders are intentionally not invoked here — they are gated on Apple,
/// Sony, Canon, and Fuji `Make` strings respectively and would never match
/// against an Olympus body.
fn orf_extract(
    source: &SourceBytes,
) -> Result<
    (
        String,
        Vec<xifty_core::ContainerNode>,
        Vec<MetadataEntry>,
        Vec<Issue>,
    ),
    XiftyError,
> {
    let tiff =
        xifty_container_tiff::parse_bytes_accepting(source.bytes(), 0, "orf", ORF_TIFF_MAGICS)?;
    let mut issues = tiff.issues.clone();
    let mut entries = decode_from_tiff(source.bytes(), 0, "orf", &tiff);
    entries.extend(decode_olympus_from_tiff(
        source.bytes(),
        0,
        "orf",
        &tiff,
        &entries,
    ));
    if let Some((offset_start, payload)) = xifty_container_tiff::xmp_payload(source.bytes(), &tiff)
    {
        let decoded = decode_packet(XmpPacket {
            bytes: payload,
            container: "orf",
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
    if let Some((offset_start, payload)) = xifty_container_tiff::icc_payload(source.bytes(), &tiff)
    {
        let decoded = decode_icc_payload(IccPayload {
            bytes: payload,
            container: "orf",
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
    if let Some((offset_start, payload)) = xifty_container_tiff::iptc_payload(source.bytes(), &tiff)
    {
        let decoded = decode_iptc_payload(IptcPayload {
            bytes: payload,
            container: "orf",
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
    Ok(("orf".to_string(), tiff.nodes, entries, issues))
}

/// Extraction path for Panasonic RW2.
///
/// **Load-bearing isolation rule:** RW2 IFD0 tag IDs collide numerically with
/// standard TIFF / EXIF tag IDs but carry Panasonic-private semantics. This
/// helper therefore MUST NEVER call `decode_from_tiff` (the EXIF decoder),
/// `decode_apple_from_tiff`, `decode_sony_from_tiff`, `decode_canon_from_tiff`,
/// `decode_fuji_from_tiff`, or `decode_olympus_from_tiff`. All RW2 metadata
/// flows through `xifty-meta-panasonic` only, which emits entries in the
/// dedicated `panasonic` namespace. See `xifty-container-rw2` and
/// `xifty-meta-panasonic` module docs for the full collision policy.
fn rw2_extract(
    source: &SourceBytes,
) -> Result<
    (
        String,
        Vec<xifty_core::ContainerNode>,
        Vec<MetadataEntry>,
        Vec<Issue>,
    ),
    XiftyError,
> {
    let rw2 = parse_rw2(source)?;
    let issues = rw2.issues.clone();
    let entries = decode_from_rw2(source.bytes(), 0, "rw2", &rw2);
    Ok(("rw2".to_string(), rw2.nodes, entries, issues))
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

/// Emit a filesystem-derived capture timestamp when a PNG carries no embedded
/// `DateTimeOriginal` / `CreateDate`.
///
/// Two-tier precedence:
/// 1. **Universal (all platforms):** filesystem `mtime` then `birthtime`/`ctime`.
///    Runs whenever a PNG decode produced no capture-date candidate, so Linux,
///    Windows, and macOS hosts all surface the host filesystem fallback.
/// 2. **macOS Apple-screencap enrichment:** if the file is tagged as an Apple
///    screen capture (via `xattr com.apple.metadata:kMDItemIsScreenCapture`)
///    we prefer Spotlight's `kMDItemContentCreationDate` over the filesystem
///    times. This branch is gated to `target_os = "macos"` because it shells
///    out to macOS-only tools (`xattr`, `mdls`).
fn add_filesystem_timestamp_fallbacks(
    entries: &mut Vec<MetadataEntry>,
    path: &std::path::Path,
    format: &Format,
    metadata: Option<&fs::Metadata>,
) {
    let Some(metadata) = metadata else {
        return;
    };
    if !matches!(format, Format::Png) {
        return;
    }
    let has_captured_candidate = entries
        .iter()
        .any(|entry| matches!(entry.tag_name.as_str(), "DateTimeOriginal" | "CreateDate"));
    if has_captured_candidate {
        return;
    }

    #[cfg(target_os = "macos")]
    let apple_candidate = if is_apple_screen_capture(path) {
        apple_content_creation_timestamp(path).map(|value| {
            (
                "AppleContentCreationDate",
                value,
                "Apple content creation date used because no embedded capture timestamp was decoded",
            )
        })
    } else {
        None
    };
    #[cfg(not(target_os = "macos"))]
    let apple_candidate: Option<(&'static str, String, &'static str)> = None;

    if let Some((tag_id, value, note)) = apple_candidate
        .or_else(|| {
            metadata
                .modified()
                .ok()
                .and_then(system_time_to_utc_timestamp)
                .map(|value| {
                    (
                        "FileModifyDate",
                        value,
                        "filesystem modified time used because no embedded capture timestamp was decoded",
                    )
                })
        })
        .or_else(|| {
            metadata
                .created()
                .ok()
                .and_then(system_time_to_utc_timestamp)
                .map(|value| {
                    (
                        "FileCreateDate",
                        value,
                        "filesystem creation time used because no embedded capture timestamp was decoded",
                    )
                })
        })
    {
        entries.push(filesystem_timestamp_entry(
            path,
            tag_id,
            "CreateDate",
            value,
            note,
        ));
    }
}

#[cfg(target_os = "macos")]
fn is_apple_screen_capture(path: &std::path::Path) -> bool {
    Command::new("xattr")
        .arg("-p")
        .arg("com.apple.metadata:kMDItemIsScreenCapture")
        .arg(path)
        .output()
        .map(|output| output.status.success() && output.stdout.starts_with(b"bplist00"))
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn apple_content_creation_timestamp(path: &std::path::Path) -> Option<String> {
    let output = Command::new("mdls")
        .arg("-raw")
        .arg("-name")
        .arg("kMDItemContentCreationDate")
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_mdls_utc_timestamp(std::str::from_utf8(&output.stdout).ok()?.trim())
}

#[cfg(target_os = "macos")]
fn parse_mdls_utc_timestamp(value: &str) -> Option<String> {
    if value == "(null)" {
        return None;
    }
    let (date, rest) = value.split_once(' ')?;
    let (time, offset) = rest.split_once(' ')?;
    if offset != "+0000" {
        return None;
    }
    Some(format!("{date}T{time}Z"))
}

fn filesystem_timestamp_entry(
    path: &std::path::Path,
    tag_id: &str,
    tag_name: &str,
    value: String,
    note: &str,
) -> MetadataEntry {
    MetadataEntry {
        namespace: "filesystem".into(),
        tag_id: tag_id.into(),
        tag_name: tag_name.into(),
        value: TypedValue::Timestamp(value),
        provenance: Provenance {
            container: "source".into(),
            namespace: "filesystem".into(),
            path: Some(path.to_string_lossy().into_owned()),
            offset_start: None,
            offset_end: None,
            notes: Vec::new(),
        },
        notes: vec![note.into()],
    }
}

fn system_time_to_utc_timestamp(time: SystemTime) -> Option<String> {
    let duration = time.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    let seconds = i64::try_from(duration.as_secs()).ok()?;
    let (year, month, day, hour, minute, second) = unix_seconds_to_utc(seconds)?;
    Some(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"
    ))
}

fn unix_seconds_to_utc(seconds: i64) -> Option<(i64, u32, u32, u32, u32, u32)> {
    if seconds < 0 {
        return None;
    }
    let days = seconds / 86_400;
    let seconds_of_day = seconds % 86_400;
    let (year, month, day) = civil_from_days(days)?;
    let hour = u32::try_from(seconds_of_day / 3_600).ok()?;
    let minute = u32::try_from((seconds_of_day % 3_600) / 60).ok()?;
    let second = u32::try_from(seconds_of_day % 60).ok()?;
    Some((year, month, day, hour, minute, second))
}

fn civil_from_days(days_since_unix_epoch: i64) -> Option<(i64, u32, u32)> {
    let z = days_since_unix_epoch + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };
    Some((year, u32::try_from(month).ok()?, u32::try_from(day).ok()?))
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

fn decode_png_creation_time_payload(
    chunk_type: &[u8; 4],
    payload: &[u8],
) -> Result<Option<String>, ()> {
    let Some((keyword, text)) = decode_png_text_payload(chunk_type, payload)? else {
        return Ok(None);
    };
    if keyword.eq_ignore_ascii_case("Creation Time") {
        return Ok(Some(text.trim().to_string()));
    }
    Ok(None)
}

fn decode_png_text_payload(
    chunk_type: &[u8; 4],
    payload: &[u8],
) -> Result<Option<(String, String)>, ()> {
    let nul = payload.iter().position(|byte| *byte == 0).ok_or(())?;
    let keyword = String::from_utf8_lossy(&payload[..nul]).into_owned();
    let content = match chunk_type {
        b"tEXt" => payload.get(nul + 1..).ok_or(())?.to_vec(),
        b"zTXt" => {
            let method = *payload.get(nul + 1).ok_or(())?;
            if method != 0 {
                return Err(());
            }
            let compressed = payload.get(nul + 2..).ok_or(())?;
            let mut decoder = ZlibDecoder::new(compressed);
            let mut decoded = Vec::new();
            decoder.read_to_end(&mut decoded).map_err(|_| ())?;
            decoded
        }
        b"iTXt" => {
            let mut cursor = nul + 1;
            let compression_flag = *payload.get(cursor).ok_or(())?;
            cursor += 1;
            let compression_method = *payload.get(cursor).ok_or(())?;
            cursor += 1;
            let lang_end = payload
                .get(cursor..)
                .ok_or(())?
                .iter()
                .position(|byte| *byte == 0)
                .ok_or(())?;
            cursor += lang_end + 1;
            let translated_end = payload
                .get(cursor..)
                .ok_or(())?
                .iter()
                .position(|byte| *byte == 0)
                .ok_or(())?;
            cursor += translated_end + 1;
            let text = payload.get(cursor..).ok_or(())?;
            if compression_flag == 0 {
                text.to_vec()
            } else {
                if compression_method != 0 {
                    return Err(());
                }
                let mut decoder = ZlibDecoder::new(text);
                let mut decoded = Vec::new();
                decoder.read_to_end(&mut decoded).map_err(|_| ())?;
                decoded
            }
        }
        _ => return Ok(None),
    };
    let text = String::from_utf8(content).map_err(|_| ())?;
    Ok(Some((keyword, text)))
}

fn decode_png_time_payload(payload: &[u8]) -> Option<String> {
    if payload.len() != 7 {
        return None;
    }
    let year = u16::from_be_bytes(payload[0..2].try_into().ok()?);
    let month = payload[2];
    let day = payload[3];
    let hour = payload[4];
    let minute = payload[5];
    let second = payload[6];
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    Some(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"
    ))
}

fn png_timestamp_entry(
    tag_id: &str,
    tag_name: &str,
    value: String,
    offset_start: u64,
    offset_end: u64,
    path: &str,
) -> MetadataEntry {
    MetadataEntry {
        namespace: "png".into(),
        tag_id: tag_id.into(),
        tag_name: tag_name.into(),
        value: TypedValue::Timestamp(value),
        provenance: Provenance {
            container: "png".into(),
            namespace: "png".into(),
            path: Some(path.into()),
            offset_start: Some(offset_start),
            offset_end: Some(offset_end),
            notes: Vec::new(),
        },
        notes: vec!["decoded from PNG timestamp chunk".into()],
    }
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

fn avif_color_entries(color: &xifty_container_isobmff::IsobmffColorInfo) -> Vec<MetadataEntry> {
    let provenance = Provenance {
        container: "avif".into(),
        namespace: "avif".into(),
        path: Some(color.path.clone()),
        offset_start: Some(color.offset_start),
        offset_end: Some(color.offset_end),
        notes: vec![format!(
            "derived from {} property for primary item",
            color.source
        )],
    };
    let mut entries = Vec::new();
    if let Some(primaries) = color.primaries {
        entries.push(MetadataEntry {
            namespace: "avif".into(),
            tag_id: "ColorPrimaries".into(),
            tag_name: "ColorPrimaries".into(),
            value: TypedValue::Integer(primaries as i64),
            provenance: provenance.clone(),
            notes: Vec::new(),
        });
    }
    if let Some(transfer) = color.transfer {
        entries.push(MetadataEntry {
            namespace: "avif".into(),
            tag_id: "TransferCharacteristics".into(),
            tag_name: "TransferCharacteristics".into(),
            value: TypedValue::Integer(transfer as i64),
            provenance: provenance.clone(),
            notes: Vec::new(),
        });
    }
    if let Some(matrix) = color.matrix {
        entries.push(MetadataEntry {
            namespace: "avif".into(),
            tag_id: "MatrixCoefficients".into(),
            tag_name: "MatrixCoefficients".into(),
            value: TypedValue::Integer(matrix as i64),
            provenance: provenance.clone(),
            notes: Vec::new(),
        });
    }
    if let Some(range) = color.full_range {
        entries.push(MetadataEntry {
            namespace: "avif".into(),
            tag_id: "FullRangeFlag".into(),
            tag_name: "FullRangeFlag".into(),
            value: TypedValue::Integer(if range { 1 } else { 0 }),
            provenance,
            notes: Vec::new(),
        });
    }
    entries
}

fn avif_pixi_entries(pixel: &xifty_container_isobmff::IsobmffPixelInfo) -> Vec<MetadataEntry> {
    // Use the first channel's bit depth as the canonical "BitDepth"; AVIF
    // images are typically uniform across channels. Per-channel data is
    // preserved via the synthesized PixelBitDepths string entry.
    let provenance = Provenance {
        container: "avif".into(),
        namespace: "avif".into(),
        path: Some(pixel.path.clone()),
        offset_start: Some(pixel.offset_start),
        offset_end: Some(pixel.offset_end),
        notes: vec!["derived from pixi property for primary item".into()],
    };
    let mut entries = Vec::new();
    if let Some(first) = pixel.bits_per_channel.first().copied() {
        entries.push(MetadataEntry {
            namespace: "avif".into(),
            tag_id: "BitDepth".into(),
            tag_name: "BitDepth".into(),
            value: TypedValue::Integer(first as i64),
            provenance: provenance.clone(),
            notes: Vec::new(),
        });
    }
    if !pixel.bits_per_channel.is_empty() {
        let joined = pixel
            .bits_per_channel
            .iter()
            .map(|b| b.to_string())
            .collect::<Vec<_>>()
            .join(",");
        entries.push(MetadataEntry {
            namespace: "avif".into(),
            tag_id: "PixelBitDepths".into(),
            tag_name: "PixelBitDepths".into(),
            value: TypedValue::String(joined),
            provenance,
            notes: Vec::new(),
        });
    }
    entries
}

fn item_dimension_entries(
    dimensions: &xifty_container_isobmff::IsobmffDimensions,
    namespace: &str,
) -> Vec<MetadataEntry> {
    let provenance = Provenance {
        container: namespace.into(),
        namespace: namespace.into(),
        path: Some(dimensions.path.clone()),
        offset_start: Some(dimensions.offset_start),
        offset_end: Some(dimensions.offset_end),
        notes: vec!["derived from ispe property for primary item".into()],
    };

    vec![
        MetadataEntry {
            namespace: namespace.into(),
            tag_id: "ImageWidth".into(),
            tag_name: "ImageWidth".into(),
            value: TypedValue::Integer(dimensions.width as i64),
            provenance: provenance.clone(),
            notes: Vec::new(),
        },
        MetadataEntry {
            namespace: namespace.into(),
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
    issues: &mut Vec<Issue>,
) -> Vec<MetadataEntry> {
    let mut entries = Vec::new();

    for payload in container.exif_payloads() {
        if let Some(payload_bytes) =
            payload_slice(bytes, payload.data_offset, payload.data_length as usize)
        {
            let tiff_view = if format_name == "heif" || format_name == "avif" {
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

    for payload in container.icc_payloads() {
        if let Some(payload_bytes) =
            payload_slice(bytes, payload.data_offset, payload.data_length as usize)
        {
            let decoded = decode_icc_payload(IccPayload {
                bytes: payload_bytes,
                container: format_name,
                path: "heif_icc",
                offset_start: payload.offset_start,
                offset_end: payload.offset_end,
            });
            if decoded.is_empty() {
                issues.push(namespace_issue(
                    "icc_decode_empty",
                    "recognized ICC payload but could not decode bounded ICC fields",
                    payload.offset_start,
                    "heif_icc",
                ));
            }
            entries.extend(decoded);
        }
    }

    for payload in container.iptc_payloads() {
        if let Some(payload_bytes) =
            payload_slice(bytes, payload.data_offset, payload.data_length as usize)
        {
            let decoded = decode_iptc_payload(IptcPayload {
                bytes: payload_bytes,
                container: format_name,
                path: "heif_iptc",
                offset_start: payload.offset_start,
                offset_end: payload.offset_end,
            });
            if decoded.is_empty() {
                issues.push(namespace_issue(
                    "iptc_decode_empty",
                    "recognized IPTC payload but could not decode bounded IPTC datasets",
                    payload.offset_start,
                    "heif_iptc",
                ));
            }
            entries.extend(decoded);
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

    for payload in container.quicktime_udta_payloads() {
        if let (Some(tag), Some(payload_bytes)) = (
            payload.tag.as_deref(),
            payload_slice(bytes, payload.data_offset, payload.data_length as usize),
        ) {
            entries.extend(decode_udta_payload(QuickTimeUdtaPayload {
                key: tag,
                bytes: payload_bytes,
                container: format_name,
                offset_start: payload.offset_start,
                offset_end: payload.offset_end,
            }));
        }
    }

    // Sony video user-data UUID atoms (PROF/USMT/...). The container parser
    // recognises the full Sony-userdata family by usertype tail; here we
    // dispatch the bounded ship-list (PROF + USMT). Unknown atom names are a
    // silent no-op — the recognition layer already swallowed the
    // uninterpreted info-issue, so adding a future decoder is a one-line
    // change with no container-layer churn.
    for payload in container.sony_video_atoms() {
        let Some(payload_bytes) =
            payload_slice(bytes, payload.data_offset, payload.data_length as usize)
        else {
            continue;
        };
        match payload.tag.as_deref() {
            Some("PROF") => {
                entries.extend(decode_prof(payload_bytes, payload.data_offset, format_name))
            }
            Some("USMT") => {
                entries.extend(decode_usmt(payload_bytes, payload.data_offset, format_name))
            }
            _ => {}
        }
    }

    if let Some(dimensions) = &container.primary_item_dimensions {
        let namespace = if format_name == "avif" {
            "avif"
        } else {
            "heif"
        };
        entries.extend(item_dimension_entries(dimensions, namespace));
    }
    if format_name == "avif" {
        if let Some(color) = &container.primary_item_color {
            entries.extend(avif_color_entries(color));
        }
        if let Some(pixel) = &container.primary_item_pixel {
            entries.extend(avif_pixi_entries(pixel));
        }
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

fn itunes_entries(
    container: &xifty_container_isobmff::IsobmffContainer,
    bytes: &[u8],
    format_name: &str,
) -> Vec<MetadataEntry> {
    let mut entries = Vec::new();
    for payload in container.itunes_payloads() {
        if let (Some(tag), Some(payload_bytes)) = (
            payload.tag.as_deref(),
            payload_slice(bytes, payload.data_offset, payload.data_length as usize),
        ) {
            entries.extend(decode_itunes_payload(ItunesPayload {
                key: tag,
                bytes: payload_bytes,
                container: format_name,
                offset_start: payload.offset_start,
                offset_end: payload.offset_end,
            }));
        }
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

fn flac_entries(flac: &FlacContainer, bytes: &[u8], issues: &mut Vec<Issue>) -> Vec<MetadataEntry> {
    let mut entries = Vec::new();

    if let Some(info) = &flac.stream_info {
        entries.push(flac_scalar_entry(
            "AudioSampleRate",
            TypedValue::Integer(info.sample_rate_hz as i64),
            "derived from STREAMINFO block",
            Some(info.offset_start),
            Some(info.offset_end),
            Some("streaminfo"),
        ));
        entries.push(flac_scalar_entry(
            "AudioChannels",
            TypedValue::Integer(info.channels as i64),
            "derived from STREAMINFO block",
            Some(info.offset_start),
            Some(info.offset_end),
            Some("streaminfo"),
        ));
        entries.push(flac_scalar_entry(
            "AudioBitDepth",
            TypedValue::Integer(info.bits_per_sample as i64),
            "derived from STREAMINFO block",
            Some(info.offset_start),
            Some(info.offset_end),
            Some("streaminfo"),
        ));
        if let Some(duration) = info.duration_seconds {
            entries.push(flac_scalar_entry(
                "DurationSeconds",
                TypedValue::Float(duration),
                "derived from STREAMINFO total_samples / sample_rate",
                Some(info.offset_start),
                Some(info.offset_end),
                Some("streaminfo"),
            ));
        }
    }

    if let Some(block) = &flac.vorbis_comment {
        let start = match usize::try_from(block.data_offset) {
            Ok(value) => value,
            Err(_) => {
                issues.push(namespace_issue(
                    "vorbis_comment_decode_empty",
                    "vorbis_comment block offset did not fit in usize",
                    block.offset_start,
                    "vorbis_comment",
                ));
                return entries;
            }
        };
        let end = start + block.data_length as usize;
        let payload = bytes.get(start..end);
        if let Some(payload) = payload {
            let decoded = decode_vorbis_comment_payload(VorbisCommentPayload {
                bytes: payload,
                container: "flac",
                offset_start: block.offset_start,
                offset_end: block.offset_end,
            });
            if decoded.is_empty() {
                issues.push(namespace_issue(
                    "vorbis_comment_decode_empty",
                    "recognized VORBIS_COMMENT block but decoded no entries",
                    block.offset_start,
                    "vorbis_comment",
                ));
            }
            entries.extend(decoded);
        }
    }

    for picture in &flac.pictures {
        entries.push(flac_scalar_entry(
            "PictureMimeType",
            TypedValue::String(picture.mime_type.clone()),
            "derived from PICTURE block",
            Some(picture.offset_start),
            Some(picture.offset_end),
            Some("picture"),
        ));
        entries.push(flac_scalar_entry(
            "PictureWidth",
            TypedValue::Integer(picture.width as i64),
            "derived from PICTURE block",
            Some(picture.offset_start),
            Some(picture.offset_end),
            Some("picture"),
        ));
        entries.push(flac_scalar_entry(
            "PictureHeight",
            TypedValue::Integer(picture.height as i64),
            "derived from PICTURE block",
            Some(picture.offset_start),
            Some(picture.offset_end),
            Some("picture"),
        ));
    }

    entries
}

fn aiff_entries(aiff: &AiffContainer) -> Vec<MetadataEntry> {
    let mut entries = Vec::new();
    let Some(comm) = &aiff.comm else {
        return entries;
    };

    let offset_start = Some(comm.offset_start);
    let offset_end = Some(comm.offset_end);
    let path = Some("comm");

    if comm.sample_rate.is_finite() && comm.sample_rate > 0.0 {
        entries.push(aiff_scalar_entry(
            "AudioSampleRate",
            TypedValue::Integer(comm.sample_rate.round() as i64),
            "derived from AIFF COMM chunk",
            offset_start,
            offset_end,
            path,
        ));
    }
    entries.push(aiff_scalar_entry(
        "AudioChannels",
        TypedValue::Integer(comm.num_channels as i64),
        "derived from AIFF COMM chunk",
        offset_start,
        offset_end,
        path,
    ));
    entries.push(aiff_scalar_entry(
        "AudioBitDepth",
        TypedValue::Integer(comm.sample_size as i64),
        "derived from AIFF COMM chunk",
        offset_start,
        offset_end,
        path,
    ));
    if let Some(duration) = aiff.duration_seconds {
        entries.push(aiff_scalar_entry(
            "DurationSeconds",
            TypedValue::Float(duration),
            "derived from AIFF COMM num_sample_frames / sample_rate",
            offset_start,
            offset_end,
            path,
        ));
    }
    entries
}

fn aiff_scalar_entry(
    tag_name: &str,
    value: TypedValue,
    note: &str,
    offset_start: Option<u64>,
    offset_end: Option<u64>,
    path: Option<&str>,
) -> MetadataEntry {
    MetadataEntry {
        namespace: "aiff".into(),
        tag_id: tag_name.into(),
        tag_name: tag_name.into(),
        value,
        provenance: Provenance {
            container: "aiff".into(),
            namespace: "aiff".into(),
            path: path.map(|p| p.to_string()),
            offset_start,
            offset_end,
            notes: vec![note.into()],
        },
        notes: Vec::new(),
    }
}

fn flac_scalar_entry(
    tag_name: &str,
    value: TypedValue,
    note: &str,
    offset_start: Option<u64>,
    offset_end: Option<u64>,
    path: Option<&str>,
) -> MetadataEntry {
    MetadataEntry {
        namespace: "flac".into(),
        tag_id: tag_name.into(),
        tag_name: tag_name.into(),
        value,
        provenance: Provenance {
            container: "flac".into(),
            namespace: "flac".into(),
            path: path.map(|p| p.to_string()),
            offset_start,
            offset_end,
            notes: vec![note.into()],
        },
        notes: Vec::new(),
    }
}

fn ogg_entries(ogg: &OggContainer, bytes: &[u8], issues: &mut Vec<Issue>) -> Vec<MetadataEntry> {
    let mut entries = Vec::new();

    match (ogg.first_codec, &ogg.vorbis_ident, &ogg.opus_ident) {
        (Some(OggCodec::Vorbis), Some(ident), _) => {
            entries.push(ogg_scalar_entry(
                "AudioSampleRate",
                TypedValue::Integer(ident.sample_rate_hz as i64),
                "derived from Vorbis identification header",
                Some(ident.offset_start),
                Some(ident.offset_end),
                Some("vorbis_ident"),
            ));
            entries.push(ogg_scalar_entry(
                "AudioChannels",
                TypedValue::Integer(ident.channels as i64),
                "derived from Vorbis identification header",
                Some(ident.offset_start),
                Some(ident.offset_end),
                Some("vorbis_ident"),
            ));
            // Vorbis does not encode bits-per-sample; emit a default
            // assumption of 16 with an explicit provenance note so the
            // normalized `audio.bit_depth` stays populated.
            entries.push(ogg_scalar_entry_with_notes(
                "AudioBitDepth",
                TypedValue::Integer(16),
                "default assumption; vorbis ident header does not encode bits_per_sample",
                Some(ident.offset_start),
                Some(ident.offset_end),
                Some("vorbis_ident"),
                Vec::new(),
            ));
            entries.push(ogg_scalar_entry(
                "AudioCodec",
                TypedValue::String("vorbis".into()),
                "derived from first packet signature",
                Some(ident.offset_start),
                Some(ident.offset_end),
                Some("vorbis_ident"),
            ));
            if let Some(granule) = ogg.granule_last {
                if granule > 0 && ident.sample_rate_hz > 0 {
                    let duration = granule as f64 / ident.sample_rate_hz as f64;
                    entries.push(ogg_scalar_entry(
                        "DurationSeconds",
                        TypedValue::Float(duration),
                        "derived from last-page granule / sample_rate",
                        Some(ident.offset_start),
                        Some(ident.offset_end),
                        Some("vorbis_ident"),
                    ));
                }
            }
        }
        (Some(OggCodec::Opus), _, Some(ident)) => {
            // Opus always decodes to 48 kHz.
            entries.push(ogg_scalar_entry(
                "AudioSampleRate",
                TypedValue::Integer(48_000),
                "opus decoded rate per RFC 7845",
                Some(ident.offset_start),
                Some(ident.offset_end),
                Some("opus_head"),
            ));
            entries.push(ogg_scalar_entry(
                "AudioChannels",
                TypedValue::Integer(ident.channels as i64),
                "derived from Opus identification header",
                Some(ident.offset_start),
                Some(ident.offset_end),
                Some("opus_head"),
            ));
            entries.push(ogg_scalar_entry(
                "AudioCodec",
                TypedValue::String("opus".into()),
                "derived from first packet signature",
                Some(ident.offset_start),
                Some(ident.offset_end),
                Some("opus_head"),
            ));
            entries.push(ogg_scalar_entry(
                "OpusInputSampleRate",
                TypedValue::Integer(ident.input_sample_rate as i64),
                "opus ident header records the encoder's source sample rate (decoding is fixed at 48 kHz)",
                Some(ident.offset_start),
                Some(ident.offset_end),
                Some("opus_head"),
            ));
            if let Some(granule) = ogg.granule_last {
                if granule > 0 {
                    let adjusted = (granule - ident.pre_skip as i64).max(0);
                    let duration = adjusted as f64 / 48_000.0;
                    entries.push(ogg_scalar_entry(
                        "DurationSeconds",
                        TypedValue::Float(duration),
                        "derived from (last-page granule - pre_skip) / 48000",
                        Some(ident.offset_start),
                        Some(ident.offset_end),
                        Some("opus_head"),
                    ));
                }
            }
        }
        _ => {}
    }

    if let Some(block) = &ogg.vorbis_comment {
        let start = match usize::try_from(block.data_offset) {
            Ok(value) => value,
            Err(_) => {
                issues.push(namespace_issue(
                    "ogg_vorbis_comment_decode_empty",
                    "vorbis-comment block offset did not fit in usize",
                    block.offset_start,
                    "vorbis_comment",
                ));
                return entries;
            }
        };
        let end = start + block.data_length as usize;
        if let Some(payload) = bytes.get(start..end) {
            let decoded = decode_vorbis_comment_payload(VorbisCommentPayload {
                bytes: payload,
                container: "ogg",
                offset_start: block.offset_start,
                offset_end: block.offset_end,
            });
            if decoded.is_empty() {
                issues.push(namespace_issue(
                    "ogg_vorbis_comment_decode_empty",
                    "recognized vorbis-comment payload but decoded no entries",
                    block.offset_start,
                    "vorbis_comment",
                ));
            }
            entries.extend(decoded);
        }
    }

    entries
}

fn ogg_scalar_entry(
    tag_name: &str,
    value: TypedValue,
    note: &str,
    offset_start: Option<u64>,
    offset_end: Option<u64>,
    path: Option<&str>,
) -> MetadataEntry {
    ogg_scalar_entry_with_notes(
        tag_name,
        value,
        note,
        offset_start,
        offset_end,
        path,
        Vec::new(),
    )
}

fn ogg_scalar_entry_with_notes(
    tag_name: &str,
    value: TypedValue,
    note: &str,
    offset_start: Option<u64>,
    offset_end: Option<u64>,
    path: Option<&str>,
    extra_notes: Vec<String>,
) -> MetadataEntry {
    MetadataEntry {
        namespace: "ogg".into(),
        tag_id: tag_name.into(),
        tag_name: tag_name.into(),
        value,
        provenance: Provenance {
            container: "ogg".into(),
            namespace: "ogg".into(),
            path: path.map(|p| p.to_string()),
            offset_start,
            offset_end,
            notes: vec![note.into()],
        },
        notes: extra_notes,
    }
}

fn mp3_entries(mp3: &Id3Container, bytes: &[u8]) -> Vec<MetadataEntry> {
    let mut entries = Vec::new();

    // Synthesize audio scalar entries from the first MPEG frame header.
    if let Some(frame) = &mp3.first_frame {
        let frame_offset = Some(frame.offset_start);
        let frame_end = Some(frame.offset_start + frame.frame_size_bytes as u64);
        entries.push(mp3_scalar_entry(
            "AudioSampleRate",
            TypedValue::Integer(frame.sample_rate_hz as i64),
            "derived from first MPEG audio frame header",
            frame_offset,
            frame_end,
            Some("mpeg_audio_frame"),
        ));
        entries.push(mp3_scalar_entry(
            "AudioChannels",
            TypedValue::Integer(frame.channels as i64),
            "derived from first MPEG audio frame header channel mode",
            frame_offset,
            frame_end,
            Some("mpeg_audio_frame"),
        ));
        if let Some(bit_depth) = mp3.bit_depth {
            entries.push(mp3_scalar_entry(
                "AudioBitDepth",
                TypedValue::Integer(bit_depth as i64),
                "MPEG audio decoder convention; not a tag-derived value",
                frame_offset,
                frame_end,
                Some("mpeg_audio_frame"),
            ));
        }
        entries.push(mp3_scalar_entry(
            "MpegLayer",
            TypedValue::String(frame.layer.as_str().into()),
            "derived from first MPEG audio frame header layer field",
            frame_offset,
            frame_end,
            Some("mpeg_audio_frame"),
        ));
        if let Some(duration) = mp3.duration_seconds {
            let note = if mp3.is_vbr {
                "derived from VBR header (Xing/VBRI) total_frames * samples_per_frame / sample_rate"
            } else {
                "derived from CBR audio_bytes * 8 / bitrate"
            };
            entries.push(mp3_scalar_entry(
                "DurationSeconds",
                TypedValue::Float(duration),
                note,
                frame_offset,
                frame_end,
                Some("mpeg_audio_frame"),
            ));
        }
        entries.push(mp3_scalar_entry(
            "AudioCodec",
            TypedValue::String("mp3".into()),
            "MPEG audio codec identified from frame header version+layer",
            frame_offset,
            frame_end,
            Some("mpeg_audio_frame"),
        ));
    }

    if let Some(payload) = mp3.id3v2_payload(bytes) {
        let decoded = decode_id3v2_payload(Id3v2DecodePayload {
            bytes: payload.bytes,
            version_major: payload.version_major,
            container: "mp3",
            offset_start: payload.offset_start,
            offset_end: payload.offset_end,
        });
        entries.extend(decoded);
    }

    entries
}

fn wav_entries(
    riff: &xifty_container_riff::RiffContainer,
    bytes: &[u8],
    issues: &mut Vec<Issue>,
) -> Vec<MetadataEntry> {
    let mut entries = Vec::new();

    // Synthesize audio scalar entries from the WAVE `fmt ` chunk.
    let mut fmt_byte_rate: Option<u32> = None;
    if let Some(chunk) = riff.fmt_chunk() {
        if let Some(payload) = payload_slice(bytes, chunk.data_offset, chunk.data_length as usize) {
            if let Some(format) = xifty_container_riff::parse_wav_format(payload) {
                fmt_byte_rate = Some(format.byte_rate);
                let fmt_offset = Some(chunk.offset_start);
                let fmt_end = Some(chunk.offset_end);
                let codec = wav_codec_name(format.format_tag);
                if codec.is_none() {
                    issues.push(namespace_issue(
                        "wav_non_pcm_format",
                        &format!(
                            "WAVE fmt chunk advertises non-PCM format tag 0x{:04x}; bit depth and duration may be inaccurate",
                            format.format_tag
                        ),
                        chunk.offset_start,
                        "wav_fmt",
                    ));
                }
                entries.push(wav_scalar_entry(
                    "AudioSampleRate",
                    TypedValue::Integer(format.sample_rate as i64),
                    "derived from WAVE fmt chunk",
                    fmt_offset,
                    fmt_end,
                    Some("wav_fmt"),
                ));
                entries.push(wav_scalar_entry(
                    "AudioChannels",
                    TypedValue::Integer(format.channels as i64),
                    "derived from WAVE fmt chunk",
                    fmt_offset,
                    fmt_end,
                    Some("wav_fmt"),
                ));
                entries.push(wav_scalar_entry(
                    "AudioBitDepth",
                    TypedValue::Integer(format.bits_per_sample as i64),
                    "derived from WAVE fmt chunk",
                    fmt_offset,
                    fmt_end,
                    Some("wav_fmt"),
                ));
                entries.push(wav_scalar_entry(
                    "AudioFormat",
                    TypedValue::Integer(format.format_tag as i64),
                    "WAVE format tag (1=PCM, 3=IEEE float, 0xFFFE=extensible)",
                    fmt_offset,
                    fmt_end,
                    Some("wav_fmt"),
                ));
                if let Some(name) = codec {
                    entries.push(wav_scalar_entry(
                        "AudioCodec",
                        TypedValue::String(name.into()),
                        "derived from WAVE fmt chunk format tag",
                        fmt_offset,
                        fmt_end,
                        Some("wav_fmt"),
                    ));
                }
            } else {
                issues.push(namespace_issue(
                    "wav_fmt_unparseable",
                    "WAVE fmt chunk present but smaller than 16 bytes; cannot decode header",
                    chunk.offset_start,
                    "wav_fmt",
                ));
            }
        }
    }

    if let Some(chunk) = riff.data_chunk() {
        if let Some(byte_rate) = fmt_byte_rate {
            if byte_rate > 0 {
                let duration = chunk.data_length as f64 / byte_rate as f64;
                entries.push(wav_scalar_entry(
                    "DurationSeconds",
                    TypedValue::Float(duration),
                    "derived from WAVE data chunk byte length / byte_rate",
                    Some(chunk.offset_start),
                    Some(chunk.offset_end),
                    Some("wav_data"),
                ));
            }
        }
    }

    if let Some(chunk) = riff.bext_chunk() {
        if let Some(payload) = payload_slice(bytes, chunk.data_offset, chunk.data_length as usize) {
            entries.extend(decode_bwf_payload(BwfPayload {
                bytes: payload,
                container: "wav",
                offset_start: chunk.offset_start,
                offset_end: chunk.offset_end,
            }));
        }
    }

    if let Some(chunk) = riff.ixml_chunk() {
        if let Some(payload) = payload_slice(bytes, chunk.data_offset, chunk.data_length as usize) {
            let decoded = decode_ixml_payload(IxmlPayload {
                bytes: payload,
                container: "wav",
                offset_start: chunk.offset_start,
                offset_end: chunk.offset_end,
            });
            if decoded.is_empty() {
                issues.push(namespace_issue(
                    "wav_ixml_not_xml",
                    "WAVE iXML chunk present but did not start with <BWFXML or <?xml",
                    chunk.offset_start,
                    "iXML",
                ));
            }
            entries.extend(decoded);
        }
    }

    entries
}

fn wav_codec_name(format_tag: u16) -> Option<&'static str> {
    match format_tag {
        0x0001 => Some("pcm"),
        0x0003 => Some("ieee_float"),
        0x0006 => Some("alaw"),
        0x0007 => Some("mulaw"),
        0xFFFE => Some("extensible"),
        _ => None,
    }
}

fn wav_scalar_entry(
    tag_name: &str,
    value: TypedValue,
    note: &str,
    offset_start: Option<u64>,
    offset_end: Option<u64>,
    path: Option<&str>,
) -> MetadataEntry {
    MetadataEntry {
        namespace: "wav".into(),
        tag_id: tag_name.into(),
        tag_name: tag_name.into(),
        value,
        provenance: Provenance {
            container: "wav".into(),
            namespace: "wav".into(),
            path: path.map(|p| p.to_string()),
            offset_start,
            offset_end,
            notes: vec![note.into()],
        },
        notes: Vec::new(),
    }
}

fn gif_entries(gif: &GifContainer, issues: &mut Vec<Issue>) -> Vec<MetadataEntry> {
    let mut entries = Vec::new();

    let lsd_offset = Some(6u64);
    let lsd_end = Some(13u64);

    entries.push(gif_scalar_entry(
        "ImageWidth",
        TypedValue::Integer(gif.width as i64),
        "derived from GIF Logical Screen Descriptor",
        lsd_offset,
        lsd_end,
        Some("lsd"),
    ));
    entries.push(gif_scalar_entry(
        "ImageHeight",
        TypedValue::Integer(gif.height as i64),
        "derived from GIF Logical Screen Descriptor",
        lsd_offset,
        lsd_end,
        Some("lsd"),
    ));
    entries.push(gif_scalar_entry(
        "FrameCount",
        TypedValue::Integer(gif.frame_count as i64),
        "count of GIF Image Descriptor blocks",
        None,
        None,
        None,
    ));
    if gif.global_color_table_size > 0 {
        entries.push(gif_scalar_entry(
            "GlobalPaletteSize",
            TypedValue::Integer(gif.global_color_table_size as i64),
            "byte length of GIF global color table",
            None,
            None,
            Some("gct"),
        ));
    }
    if gif.frame_count > 1 {
        let duration = gif.animation_duration_centiseconds as f64 / 100.0;
        entries.push(gif_scalar_entry(
            "AnimationDurationSeconds",
            TypedValue::Float(duration),
            "sum of GIF Graphic Control Extension delays (centiseconds / 100)",
            None,
            None,
            None,
        ));
        if let Some(loop_count) = gif.loop_count {
            entries.push(gif_scalar_entry(
                "LoopCount",
                TypedValue::Integer(loop_count as i64),
                "decoded from NETSCAPE2.0 application extension (0 = infinite)",
                None,
                None,
                Some("app_ext:NETSCAPE2.0"),
            ));
        }
    }

    for (offset_start, offset_end, payload) in gif.xmp_payloads() {
        let decoded = decode_packet(XmpPacket {
            bytes: payload,
            container: "gif",
            offset_start,
            offset_end,
        });
        if decoded.is_empty() {
            issues.push(namespace_issue(
                "xmp_decode_empty",
                "recognized XMP payload but could not decode bounded XMP fields",
                offset_start,
                "app_ext:XMP Data",
            ));
        }
        entries.extend(decoded);
    }

    entries
}

fn gif_scalar_entry(
    tag_name: &str,
    value: TypedValue,
    note: &str,
    offset_start: Option<u64>,
    offset_end: Option<u64>,
    path: Option<&str>,
) -> MetadataEntry {
    MetadataEntry {
        namespace: "gif".into(),
        tag_id: tag_name.into(),
        tag_name: tag_name.into(),
        value,
        provenance: Provenance {
            container: "gif".into(),
            namespace: "gif".into(),
            path: path.map(|p| p.to_string()),
            offset_start,
            offset_end,
            notes: vec![note.into()],
        },
        notes: Vec::new(),
    }
}

fn mp3_scalar_entry(
    tag_name: &str,
    value: TypedValue,
    note: &str,
    offset_start: Option<u64>,
    offset_end: Option<u64>,
    path: Option<&str>,
) -> MetadataEntry {
    MetadataEntry {
        namespace: "mp3".into(),
        tag_id: tag_name.into(),
        tag_name: tag_name.into(),
        value,
        provenance: Provenance {
            container: "mp3".into(),
            namespace: "mp3".into(),
            path: path.map(|p| p.to_string()),
            offset_start,
            offset_end,
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
