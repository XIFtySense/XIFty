//! Minimal MP3 container parser.
//!
//! Surfaces ID3v2 tag framing (so a downstream `xifty-meta-id3v2` decoder can
//! interpret frames without re-deriving the tag boundaries) plus the first
//! MPEG audio frame header (version, layer, bitrate, sample rate, channel mode)
//! and a derived duration. Duration math:
//! - VBR with a Xing/Info or VBRI header: `total_frames * samples_per_frame /
//!   sample_rate`.
//! - CBR (no Xing/VBRI): `(audio_bytes * 8) / bitrate_bps`.
//! - VBR without either header: detected by walking a bounded number of
//!   subsequent frames and noticing varying bitrate indices; leaves duration
//!   as `None` and emits an Issue `mp3_vbr_duration_unknown` (per plan; we
//!   do not estimate).
//!
//! `bit_depth` is exposed as a constant 16 — MPEG audio is bitstream-coded
//! and does not carry a sample-width tag; 16-bit PCM is the conventional
//! decoder output (matched to the OGG/Vorbis behavior in `xifty-cli`).

use xifty_core::{ContainerNode, Issue, Severity, XiftyError};
use xifty_source::SourceBytes;

/// Conventional decoded sample width for MPEG audio (no tag-derived value).
pub const MP3_BIT_DEPTH: u32 = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MpegVersion {
    Mpeg1,
    Mpeg2,
    Mpeg25,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MpegLayer {
    Layer1,
    Layer2,
    Layer3,
}

impl MpegLayer {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Layer1 => "Layer I",
            Self::Layer2 => "Layer II",
            Self::Layer3 => "Layer III",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MpegFrameHeader {
    pub version: MpegVersion,
    pub layer: MpegLayer,
    pub bitrate_kbps: u32,
    pub sample_rate_hz: u32,
    pub channels: u32,
    pub samples_per_frame: u32,
    pub frame_size_bytes: u32,
    pub offset_start: u64,
    pub padding: bool,
}

#[derive(Debug, Clone)]
pub struct Id3v2Tag {
    pub version_major: u8,
    pub version_revision: u8,
    pub flags: u8,
    /// Absolute offset of the start of the ID3v2 header (the "ID3" magic).
    pub offset_start: u64,
    /// Absolute offset of the byte just past the tag (= start of audio frames
    /// or padding/footer).
    pub offset_end: u64,
    /// Absolute offset of the first frame byte (just past the 10-byte header,
    /// past the optional extended header).
    pub frames_offset: u64,
    /// Length of the raw frames region in bytes (excludes the 10-byte tag
    /// header and any extended header).
    pub frames_length: u32,
}

#[derive(Debug, Clone)]
pub struct Id3v2Payload<'a> {
    pub bytes: &'a [u8],
    pub version_major: u8,
    pub version_revision: u8,
    pub offset_start: u64,
    pub offset_end: u64,
}

#[derive(Debug, Clone)]
pub struct Id3Container {
    pub nodes: Vec<ContainerNode>,
    pub issues: Vec<Issue>,
    pub id3v2: Option<Id3v2Tag>,
    pub first_frame: Option<MpegFrameHeader>,
    pub duration_seconds: Option<f64>,
    pub bit_depth: Option<u32>,
    pub is_vbr: bool,
    pub total_frames: Option<u32>,
    /// Total length of the audio data region (bytes after the ID3v2 tag,
    /// minus any trailing ID3v1 128-byte block if present).
    pub audio_bytes: u64,
}

impl Id3Container {
    /// Borrow the raw ID3v2 frame region for downstream namespace decoding.
    pub fn id3v2_payload<'a>(&self, bytes: &'a [u8]) -> Option<Id3v2Payload<'a>> {
        let tag = self.id3v2.as_ref()?;
        let start = usize::try_from(tag.frames_offset).ok()?;
        let end = start.checked_add(tag.frames_length as usize)?;
        let slice = bytes.get(start..end)?;
        Some(Id3v2Payload {
            bytes: slice,
            version_major: tag.version_major,
            version_revision: tag.version_revision,
            offset_start: tag.frames_offset,
            offset_end: tag.frames_offset + slice.len() as u64,
        })
    }
}

pub fn parse(source: &SourceBytes) -> Result<Id3Container, XiftyError> {
    parse_bytes(source.bytes(), 0)
}

pub fn parse_bytes(bytes: &[u8], base_offset: u64) -> Result<Id3Container, XiftyError> {
    let mut nodes = Vec::new();
    let mut issues = Vec::new();
    nodes.push(ContainerNode {
        kind: "container".into(),
        label: "mp3".into(),
        offset_start: base_offset,
        offset_end: base_offset + bytes.len() as u64,
        parent_label: None,
    });

    let id3v2 = parse_id3v2_header(bytes, base_offset, &mut issues);
    if let Some(tag) = &id3v2 {
        nodes.push(ContainerNode {
            kind: "tag".into(),
            label: "id3v2".into(),
            offset_start: tag.offset_start,
            offset_end: tag.offset_end,
            parent_label: Some("mp3".into()),
        });
    }

    let audio_region_start = id3v2
        .as_ref()
        .map(|t| (t.offset_end - base_offset) as usize)
        .unwrap_or(0);

    // Strip trailing ID3v1 (128 bytes ending in "TAG..." magic) if present so
    // it does not leak into duration math.
    let mut audio_region_end = bytes.len();
    if audio_region_end >= audio_region_start + 128 {
        let v1_start = audio_region_end - 128;
        if &bytes[v1_start..v1_start + 3] == b"TAG" {
            audio_region_end = v1_start;
        }
    }

    let audio_region = bytes
        .get(audio_region_start..audio_region_end)
        .unwrap_or(&[]);
    let audio_bytes = audio_region.len() as u64;

    let mut first_frame = None;
    let mut duration_seconds = None;
    let mut is_vbr = false;
    let mut total_frames = None;

    if let Some((sync_offset_in_region, header)) = find_first_frame(audio_region) {
        let absolute_offset =
            base_offset + audio_region_start as u64 + sync_offset_in_region as u64;
        let header = MpegFrameHeader {
            offset_start: absolute_offset,
            ..header
        };
        nodes.push(ContainerNode {
            kind: "frame".into(),
            label: "mpeg_audio_frame".into(),
            offset_start: header.offset_start,
            offset_end: header.offset_start + header.frame_size_bytes as u64,
            parent_label: Some("mp3".into()),
        });

        // Inspect the side-info region for Xing/Info or VBRI.
        let xing = find_xing(audio_region, sync_offset_in_region, &header);
        let vbri = find_vbri(audio_region, sync_offset_in_region);
        match (xing, vbri) {
            (Some((tag, frames)), _) => {
                // "Xing" magic marks a true VBR stream; "Info" marks CBR-with-toc
                // (same header layout but used by encoders that wrote constant
                // bitrate and still wanted a TOC).
                is_vbr = tag == "Xing";
                total_frames = Some(frames);
                if header.sample_rate_hz > 0 {
                    let samples = frames as u64 * header.samples_per_frame as u64;
                    duration_seconds = Some(samples as f64 / header.sample_rate_hz as f64);
                }
            }
            (None, Some(frames)) => {
                is_vbr = true;
                total_frames = Some(frames);
                if header.sample_rate_hz > 0 {
                    let samples = frames as u64 * header.samples_per_frame as u64;
                    duration_seconds = Some(samples as f64 / header.sample_rate_hz as f64);
                }
            }
            (None, None) => {
                // No Xing/Info or VBRI header. Walk a bounded number of
                // subsequent frames; if their bitrate indices vary, the stream
                // is genuinely VBR and we cannot derive duration from framing.
                if detect_vbr_no_header(audio_region, sync_offset_in_region, &header) {
                    is_vbr = true;
                    issues.push(vbr_duration_unknown_issue(
                        base_offset + audio_region_start as u64 + sync_offset_in_region as u64,
                    ));
                } else if header.bitrate_kbps > 0 {
                    // Treat as CBR using the first-frame bitrate.
                    let bits = audio_bytes * 8;
                    duration_seconds = Some(bits as f64 / (header.bitrate_kbps as f64 * 1000.0));
                }
            }
        }

        first_frame = Some(header);
    } else {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "mp3_no_audio_frame".into(),
            message: "no MPEG audio sync frame located after ID3v2 tag".into(),
            offset: Some(base_offset + audio_region_start as u64),
            context: None,
        });
    }

    let bit_depth = first_frame.as_ref().map(|_| MP3_BIT_DEPTH);

    Ok(Id3Container {
        nodes,
        issues,
        id3v2,
        first_frame,
        duration_seconds,
        bit_depth,
        is_vbr,
        total_frames,
        audio_bytes,
    })
}

fn parse_id3v2_header(bytes: &[u8], base_offset: u64, issues: &mut Vec<Issue>) -> Option<Id3v2Tag> {
    if bytes.len() < 10 || &bytes[0..3] != b"ID3" {
        return None;
    }
    let version_major = bytes[3];
    let version_revision = bytes[4];
    let flags = bytes[5];
    let size = match decode_syncsafe(&bytes[6..10]) {
        Some(value) => value,
        None => {
            issues.push(Issue {
                severity: Severity::Warning,
                code: "id3v2_size_invalid".into(),
                message: "ID3v2 size field contains a non-syncsafe byte".into(),
                offset: Some(base_offset + 6),
                context: None,
            });
            return None;
        }
    };
    let extended_header_present = (flags & 0b0100_0000) != 0;
    let footer_present = (flags & 0b0001_0000) != 0;

    let header_size: u32 = 10;
    let footer_size: u32 = if footer_present { 10 } else { 0 };
    let mut frames_offset = base_offset + header_size as u64;
    let mut frames_length = size;
    if extended_header_present && (frames_length as usize) >= 4 {
        // ID3v2.3: extended header size is a regular big-endian u32; ID3v2.4:
        // syncsafe. We only need to skip past it for downstream framing; treat
        // both shapes defensively (try syncsafe first, then big-endian).
        let ext_offset = (frames_offset - base_offset) as usize;
        if let Some(ext_size_bytes) = bytes.get(ext_offset..ext_offset + 4) {
            let syncsafe = decode_syncsafe(ext_size_bytes);
            let plain = u32::from_be_bytes(ext_size_bytes.try_into().unwrap());
            let chosen = if version_major >= 4 {
                syncsafe.unwrap_or(plain)
            } else {
                // 2.3 extended header size excludes itself per spec.
                plain.saturating_add(4)
            };
            if chosen <= frames_length {
                frames_offset += chosen as u64;
                frames_length = frames_length.saturating_sub(chosen);
            }
        }
    }

    let offset_end = base_offset + header_size as u64 + size as u64 + footer_size as u64;

    Some(Id3v2Tag {
        version_major,
        version_revision,
        flags,
        offset_start: base_offset,
        offset_end,
        frames_offset,
        frames_length,
    })
}

fn decode_syncsafe(bytes: &[u8]) -> Option<u32> {
    if bytes.len() != 4 {
        return None;
    }
    for byte in bytes {
        if byte & 0x80 != 0 {
            return None;
        }
    }
    Some(
        ((bytes[0] as u32) << 21)
            | ((bytes[1] as u32) << 14)
            | ((bytes[2] as u32) << 7)
            | (bytes[3] as u32),
    )
}

fn find_first_frame(region: &[u8]) -> Option<(usize, MpegFrameHeader)> {
    let mut offset = 0usize;
    while offset + 4 <= region.len() {
        if region[offset] == 0xFF && (region[offset + 1] & 0xE0) == 0xE0 {
            if let Some(header) = decode_frame_header(&region[offset..offset + 4], 0) {
                return Some((offset, header));
            }
        }
        offset += 1;
    }
    None
}

fn decode_frame_header(bytes: &[u8], absolute_offset: u64) -> Option<MpegFrameHeader> {
    if bytes.len() < 4 {
        return None;
    }
    let b1 = bytes[1];
    let b2 = bytes[2];
    let b3 = bytes[3];
    let version = match (b1 >> 3) & 0b11 {
        0b11 => MpegVersion::Mpeg1,
        0b10 => MpegVersion::Mpeg2,
        0b00 => MpegVersion::Mpeg25,
        _ => return None,
    };
    let layer = match (b1 >> 1) & 0b11 {
        0b11 => MpegLayer::Layer1,
        0b10 => MpegLayer::Layer2,
        0b01 => MpegLayer::Layer3,
        _ => return None,
    };
    let bitrate_index = ((b2 >> 4) & 0x0F) as usize;
    let sample_rate_index = ((b2 >> 2) & 0b11) as usize;
    let padding = ((b2 >> 1) & 1) == 1;
    let channel_mode = (b3 >> 6) & 0b11;

    let bitrate_kbps = bitrate_lookup(&version, &layer, bitrate_index)?;
    let sample_rate_hz = sample_rate_lookup(&version, sample_rate_index)?;
    let samples_per_frame = samples_per_frame(&version, &layer);
    let channels = if channel_mode == 0b11 { 1 } else { 2 };

    let frame_size_bytes = compute_frame_size(
        &layer,
        bitrate_kbps,
        sample_rate_hz,
        samples_per_frame,
        padding,
    )?;

    Some(MpegFrameHeader {
        version,
        layer,
        bitrate_kbps,
        sample_rate_hz,
        channels,
        samples_per_frame,
        frame_size_bytes,
        offset_start: absolute_offset,
        padding,
    })
}

fn bitrate_lookup(version: &MpegVersion, layer: &MpegLayer, index: usize) -> Option<u32> {
    if index == 0 || index == 15 {
        return None;
    }
    // kbps tables; columns indexed 1..=14
    const MPEG1_L1: [u32; 15] = [
        0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448,
    ];
    const MPEG1_L2: [u32; 15] = [
        0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384,
    ];
    const MPEG1_L3: [u32; 15] = [
        0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
    ];
    const MPEG2_L1: [u32; 15] = [
        0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256,
    ];
    const MPEG2_L23: [u32; 15] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];
    let table: &[u32; 15] = match (version, layer) {
        (MpegVersion::Mpeg1, MpegLayer::Layer1) => &MPEG1_L1,
        (MpegVersion::Mpeg1, MpegLayer::Layer2) => &MPEG1_L2,
        (MpegVersion::Mpeg1, MpegLayer::Layer3) => &MPEG1_L3,
        (MpegVersion::Mpeg2 | MpegVersion::Mpeg25, MpegLayer::Layer1) => &MPEG2_L1,
        (MpegVersion::Mpeg2 | MpegVersion::Mpeg25, _) => &MPEG2_L23,
    };
    Some(table[index])
}

fn sample_rate_lookup(version: &MpegVersion, index: usize) -> Option<u32> {
    if index >= 3 {
        return None;
    }
    let base = match version {
        MpegVersion::Mpeg1 => [44100, 48000, 32000],
        MpegVersion::Mpeg2 => [22050, 24000, 16000],
        MpegVersion::Mpeg25 => [11025, 12000, 8000],
    };
    Some(base[index])
}

fn samples_per_frame(version: &MpegVersion, layer: &MpegLayer) -> u32 {
    match (version, layer) {
        (_, MpegLayer::Layer1) => 384,
        (MpegVersion::Mpeg1, MpegLayer::Layer2) => 1152,
        (MpegVersion::Mpeg1, MpegLayer::Layer3) => 1152,
        (_, MpegLayer::Layer2) => 1152,
        (_, MpegLayer::Layer3) => 576,
    }
}

fn compute_frame_size(
    layer: &MpegLayer,
    bitrate_kbps: u32,
    sample_rate_hz: u32,
    samples_per_frame: u32,
    padding: bool,
) -> Option<u32> {
    if sample_rate_hz == 0 || bitrate_kbps == 0 {
        return None;
    }
    let pad = if padding { 1 } else { 0 };
    let bps = bitrate_kbps * 1000;
    Some(match layer {
        MpegLayer::Layer1 => ((12 * bps / sample_rate_hz) + pad) * 4,
        _ => (samples_per_frame / 8 * bps / sample_rate_hz) + pad,
    })
}

/// Locate the Xing/Info header inside the side-info region of the first frame.
/// Returns `(tag, total_frames)`; `tag` is "Xing" for VBR, "Info" for CBR-with-toc.
fn find_xing(
    region: &[u8],
    frame_offset: usize,
    header: &MpegFrameHeader,
) -> Option<(&'static str, u32)> {
    // Side-info offset depends on version and channel mode.
    let side_info = side_info_size(&header.version, header.channels);
    let scan_start = frame_offset + 4 + side_info;
    let scan_end = (frame_offset + header.frame_size_bytes as usize).min(region.len());
    let area = region.get(scan_start..scan_end)?;
    let pos = area.windows(4).position(|w| w == b"Xing" || w == b"Info")?;
    let tag = if &area[pos..pos + 4] == b"Xing" {
        "Xing"
    } else {
        "Info"
    };
    let after_tag = area.get(pos + 4..pos + 8)?;
    let flags = u32::from_be_bytes(after_tag.try_into().ok()?);
    let mut cursor = pos + 8;
    let frames = if flags & 0x0001 != 0 {
        let bytes = area.get(cursor..cursor + 4)?;
        cursor += 4;
        let _ = cursor; // silence unused if no other flags consumed
        u32::from_be_bytes(bytes.try_into().ok()?)
    } else {
        return None;
    };
    Some((tag, frames))
}

fn find_vbri(region: &[u8], frame_offset: usize) -> Option<u32> {
    // VBRI lives at fixed offset 32 bytes after the frame header.
    let start = frame_offset + 4 + 32;
    let area = region.get(start..start + 26)?;
    if &area[0..4] != b"VBRI" {
        return None;
    }
    // Layout: id(4) version(2) delay(2) quality(2) bytes(4) frames(4) ...
    let frames_bytes = area.get(14..18)?;
    Some(u32::from_be_bytes(frames_bytes.try_into().ok()?))
}

fn side_info_size(version: &MpegVersion, channels: u32) -> usize {
    match (version, channels) {
        (MpegVersion::Mpeg1, 1) => 17,
        (MpegVersion::Mpeg1, _) => 32,
        (_, 1) => 9,
        (_, _) => 17,
    }
}

/// Walk up to `MAX_VBR_PROBE_FRAMES` frames after the first sync frame and
/// return `true` if any subsequent frame advertises a different bitrate index
/// than the first. A varying bitrate combined with the absence of a Xing/Info
/// or VBRI header means we have a genuinely VBR stream whose duration is not
/// derivable from container framing.
fn detect_vbr_no_header(region: &[u8], first_frame_offset: usize, first: &MpegFrameHeader) -> bool {
    const MAX_VBR_PROBE_FRAMES: usize = 16;
    let mut cursor = first_frame_offset + first.frame_size_bytes as usize;
    let mut probed = 0usize;
    while probed < MAX_VBR_PROBE_FRAMES && cursor + 4 <= region.len() {
        // Resync if the next 4 bytes are not a valid frame header — bail
        // rather than scan the whole file (this keeps the probe bounded).
        if region[cursor] != 0xFF || (region[cursor + 1] & 0xE0) != 0xE0 {
            return false;
        }
        let next = match decode_frame_header(&region[cursor..cursor + 4], 0) {
            Some(h) => h,
            None => return false,
        };
        if next.bitrate_kbps != first.bitrate_kbps {
            return true;
        }
        cursor += next.frame_size_bytes as usize;
        probed += 1;
    }
    false
}

/// Construct an explicit VBR-without-Xing issue. Emitted automatically by the
/// extract path when frame-walk detection confirms VBR, and exposed publicly
/// for callers that already know VBR is in play.
pub fn vbr_duration_unknown_issue(offset: u64) -> Issue {
    Issue {
        severity: Severity::Warning,
        code: "mp3_vbr_duration_unknown".into(),
        message: "VBR MP3 without Xing/Info or VBRI header — duration not derivable from container framing".into(),
        offset: Some(offset),
        context: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_id3v2_3(frames: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"ID3");
        out.push(3); // major
        out.push(0); // revision
        out.push(0); // flags
        // syncsafe size
        let size = frames.len() as u32;
        out.push(((size >> 21) & 0x7F) as u8);
        out.push(((size >> 14) & 0x7F) as u8);
        out.push(((size >> 7) & 0x7F) as u8);
        out.push((size & 0x7F) as u8);
        out.extend_from_slice(frames);
        out
    }

    fn cbr_frame_header(padding: bool) -> [u8; 4] {
        // MPEG-1 Layer III, 128 kbps, 44.1 kHz, stereo
        // FF FB 90 (or 92 with padding) 04
        // byte1: 1111 1111 (sync 11) 1011 (version=01 mpeg1, layer=01 L3, no crc=1) -> 0xFB
        // byte2: bitrate=1001(128) sr=00(44.1) pad=0 priv=0 -> 1001 0000 = 0x90
        // byte3: channel mode=00(stereo) ... -> 0x00 (we pick 0x04 to mirror real files; harmless)
        let pad_bit: u8 = if padding { 0x02 } else { 0x00 };
        [0xFF, 0xFB, 0x90 | pad_bit, 0x04]
    }

    fn synth_cbr_audio(frame_count: usize) -> Vec<u8> {
        // Real frame size at 128 kbps / 44.1 kHz (no padding) = 417 bytes.
        // We pad each frame with zeros so the CBR duration math has bytes to chew on.
        let frame_size = 417usize;
        let header = cbr_frame_header(false);
        let mut out = Vec::new();
        for _ in 0..frame_count {
            out.extend_from_slice(&header);
            out.extend(std::iter::repeat(0u8).take(frame_size - 4));
        }
        out
    }

    #[test]
    fn parses_id3v2_then_first_frame_cbr() {
        let mut bytes = build_id3v2_3(b"DUMMYFRAME"); // 10 bytes of fake frame data
        bytes.extend(synth_cbr_audio(10));
        let parsed = parse_bytes(&bytes, 0).unwrap();
        let tag = parsed.id3v2.as_ref().expect("id3v2 tag");
        assert_eq!(tag.version_major, 3);
        assert_eq!(tag.frames_length, 10);
        let frame = parsed.first_frame.as_ref().expect("first frame");
        assert_eq!(frame.sample_rate_hz, 44100);
        assert_eq!(frame.channels, 2);
        assert_eq!(frame.bitrate_kbps, 128);
        // 10 frames * 417 bytes = 4170 bytes audio. Duration = 4170 * 8 / 128000 ≈ 0.260625
        let duration = parsed.duration_seconds.unwrap();
        assert!((duration - 0.260625).abs() < 1e-6, "got {duration}");
        assert_eq!(parsed.bit_depth, Some(16));
    }

    #[test]
    fn parses_xing_vbr_duration() {
        // First frame contains a Xing header advertising 100 frames.
        let header = cbr_frame_header(false);
        let mut frame = Vec::new();
        frame.extend_from_slice(&header);
        // pad up to side_info offset (MPEG-1 stereo = 32 bytes side info)
        frame.extend(std::iter::repeat(0u8).take(32));
        frame.extend_from_slice(b"Xing");
        frame.extend_from_slice(&0x0001u32.to_be_bytes()); // flags: frames present
        frame.extend_from_slice(&100u32.to_be_bytes()); // 100 frames
        // pad rest of frame
        while frame.len() < 417 {
            frame.push(0);
        }
        let parsed = parse_bytes(&frame, 0).unwrap();
        assert!(parsed.is_vbr);
        assert_eq!(parsed.total_frames, Some(100));
        // 100 frames * 1152 samples / 44100 ≈ 2.6122 s
        let duration = parsed.duration_seconds.unwrap();
        assert!((duration - (100.0 * 1152.0 / 44100.0)).abs() < 1e-6);
    }

    #[test]
    fn vbr_no_xing_helper_issue() {
        let issue = vbr_duration_unknown_issue(0);
        assert_eq!(issue.code, "mp3_vbr_duration_unknown");
        assert!(matches!(issue.severity, Severity::Warning));
    }

    #[test]
    fn id3v2_payload_borrow_returns_frames_slice() {
        let bytes = build_id3v2_3(b"ABCDEFGH");
        let parsed = parse_bytes(&bytes, 0).unwrap();
        let payload = parsed.id3v2_payload(&bytes).expect("payload");
        assert_eq!(payload.bytes, b"ABCDEFGH");
        assert_eq!(payload.version_major, 3);
        assert_eq!(payload.offset_start, 10);
    }

    #[test]
    fn strips_trailing_id3v1_from_audio_region() {
        let mut bytes = build_id3v2_3(b"X"); // 1-byte fake tag
        bytes.extend(synth_cbr_audio(2)); // 834 bytes
        // Append a synthetic ID3v1 footer
        let mut v1 = vec![b'T', b'A', b'G'];
        v1.extend(std::iter::repeat(0u8).take(125));
        bytes.extend(v1);
        let parsed = parse_bytes(&bytes, 0).unwrap();
        // 2 frames * 417 = 834 audio bytes
        assert_eq!(parsed.audio_bytes, 834);
    }
}
