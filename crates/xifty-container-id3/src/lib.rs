//! MP3 (ID3v2 + MPEG audio frame) container parser.
//!
//! Container-only: walks ID3v2 tag framing and the first MPEG audio frame
//! header to derive duration / sample rate / channel count. ID3v2 frame
//! interpretation lives in `xifty-meta-id3v2`. ID3v1 trailers are out of
//! scope for phase 1.

use xifty_core::{ContainerNode, Issue, Severity, XiftyError};
use xifty_source::SourceBytes;

/// MPEG audio version (from the frame header version bits).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MpegVersion {
    /// MPEG-1.
    V1,
    /// MPEG-2 (low sample rates).
    V2,
    /// MPEG-2.5 (very low sample rates; non-standard extension).
    V25,
}

/// MPEG audio layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MpegLayer {
    /// Layer I.
    LayerI,
    /// Layer II.
    LayerII,
    /// Layer III (the "MP3" codec).
    LayerIII,
}

impl MpegLayer {
    /// Canonical display form used for metadata entries.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::LayerI => "I",
            Self::LayerII => "II",
            Self::LayerIII => "III",
        }
    }
}

/// Decoded MPEG audio frame header (the first sync-word frame in the file).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MpegFrameHeader {
    /// Audio version.
    pub version: MpegVersion,
    /// Audio layer.
    pub layer: MpegLayer,
    /// Declared bitrate in bits/sec (0 means "free-format").
    pub bitrate_bps: u32,
    /// Sample rate in Hz.
    pub sample_rate_hz: u32,
    /// Channel count (1 for mono, 2 otherwise).
    pub channels: u8,
    /// Absolute offset of the first sync byte.
    pub offset: u64,
}

impl MpegFrameHeader {
    /// Samples per frame for the decoded (version, layer) pair.
    pub fn samples_per_frame(&self) -> u32 {
        match (self.version, self.layer) {
            (_, MpegLayer::LayerI) => 384,
            (MpegVersion::V1, MpegLayer::LayerII) | (MpegVersion::V1, MpegLayer::LayerIII) => 1152,
            (_, MpegLayer::LayerII) => 1152,
            (_, MpegLayer::LayerIII) => 576,
        }
    }
}

/// Access to the ID3v2 tag payload (raw frame bytes, version, absolute offset).
#[derive(Debug, Clone)]
pub struct Id3v2Payload<'a> {
    /// Major version (3 or 4 in practice).
    pub version_major: u8,
    /// Revision byte.
    pub version_minor: u8,
    /// Header flags byte.
    pub flags: u8,
    /// Absolute offset of the payload (first byte after the 10-byte header).
    pub offset_start: u64,
    /// Raw frame bytes (immediately after the 10-byte header).
    pub bytes: &'a [u8],
}

/// Parsed MP3 container view.
#[derive(Debug, Clone)]
pub struct Id3Container {
    /// Container nodes (hierarchical layout).
    pub nodes: Vec<ContainerNode>,
    /// Parser-level issues.
    pub issues: Vec<Issue>,
    /// Absolute offset of the first MPEG audio frame header, if any.
    pub mpeg_frame_offset: Option<u64>,
    /// Decoded first-frame header, if any.
    pub mpeg_frame: Option<MpegFrameHeader>,
    /// Total duration in seconds. `None` when VBR without Xing/VBRI.
    pub duration_seconds: Option<f64>,
    /// Fixed MPEG-decoder-output bit depth (16 per audio convention).
    pub bit_depth: Option<u32>,
    /// ID3v2 tag version_major (3 or 4) if the file has an ID3v2 tag.
    id3v2_version_major: u8,
    id3v2_version_minor: u8,
    id3v2_flags: u8,
    id3v2_payload_offset: u64,
    id3v2_payload_len: u32,
    has_id3v2: bool,
    /// Cached bytes of the full source (only used for payload accessor).
    raw: Vec<u8>,
}

impl Id3Container {
    /// Return the ID3v2 payload reference, if the file carries an ID3v2 tag.
    pub fn id3v2_payload(&self) -> Option<Id3v2Payload<'_>> {
        if !self.has_id3v2 {
            return None;
        }
        let start = self.id3v2_payload_offset as usize;
        let end = start + self.id3v2_payload_len as usize;
        let bytes = self.raw.get(start..end)?;
        Some(Id3v2Payload {
            version_major: self.id3v2_version_major,
            version_minor: self.id3v2_version_minor,
            flags: self.id3v2_flags,
            offset_start: self.id3v2_payload_offset,
            bytes,
        })
    }
}

/// Parse an MP3 source.
pub fn parse(source: &SourceBytes) -> Result<Id3Container, XiftyError> {
    parse_bytes(source.bytes(), 0)
}

/// Parse MP3 bytes at a known absolute base offset.
pub fn parse_bytes(bytes: &[u8], base_offset: u64) -> Result<Id3Container, XiftyError> {
    let mut issues: Vec<Issue> = Vec::new();
    let mut nodes = vec![ContainerNode {
        kind: "container".into(),
        label: "mp3".into(),
        offset_start: base_offset,
        offset_end: base_offset + bytes.len() as u64,
        parent_label: None,
    }];

    // --- ID3v2 tag detection ---
    let (has_id3v2, id3v2_version_major, id3v2_version_minor, id3v2_flags, id3v2_payload_len) =
        if bytes.len() >= 10 && &bytes[0..3] == b"ID3" {
            let version_major = bytes[3];
            let version_minor = bytes[4];
            let flags = bytes[5];
            // Syncsafe size: four 7-bit groups, big-endian.
            let s0 = bytes[6] as u32;
            let s1 = bytes[7] as u32;
            let s2 = bytes[8] as u32;
            let s3 = bytes[9] as u32;
            if (s0 | s1 | s2 | s3) & 0x80 != 0 {
                issues.push(Issue {
                    severity: Severity::Warning,
                    code: "id3v2_syncsafe_size_invalid".into(),
                    message: "ID3v2 size field has the MSB of a byte set (non-syncsafe)".into(),
                    offset: Some(base_offset + 6),
                    context: None,
                });
            }
            let size = ((s0 & 0x7F) << 21) | ((s1 & 0x7F) << 14) | ((s2 & 0x7F) << 7) | (s3 & 0x7F);
            nodes.push(ContainerNode {
                kind: "tag".into(),
                label: "id3v2".into(),
                offset_start: base_offset,
                offset_end: base_offset + 10 + size as u64,
                parent_label: Some("mp3".into()),
            });
            (true, version_major, version_minor, flags, size)
        } else {
            (false, 0, 0, 0, 0u32)
        };

    // --- Find first MPEG audio frame sync ---
    let frame_search_start = if has_id3v2 {
        (10 + id3v2_payload_len) as usize
    } else {
        0
    };

    let mut mpeg_frame: Option<MpegFrameHeader> = None;
    let mut mpeg_frame_offset: Option<u64> = None;
    let mut mpeg_frame_local: Option<usize> = None;

    if frame_search_start < bytes.len() {
        if let Some((local_offset, header)) =
            find_mpeg_frame(bytes, frame_search_start, base_offset)
        {
            nodes.push(ContainerNode {
                kind: "frame".into(),
                label: "mpeg_audio_frame".into(),
                offset_start: base_offset + local_offset as u64,
                offset_end: base_offset + bytes.len() as u64,
                parent_label: Some("mp3".into()),
            });
            mpeg_frame_offset = Some(header.offset);
            mpeg_frame_local = Some(local_offset);
            mpeg_frame = Some(header);
        } else {
            issues.push(Issue {
                severity: Severity::Warning,
                code: "mp3_no_mpeg_frame".into(),
                message: "no MPEG audio frame sync word found after ID3v2 tag".into(),
                offset: Some(base_offset + frame_search_start as u64),
                context: None,
            });
        }
    }

    // --- Compute duration ---
    let mut duration_seconds: Option<f64> = None;
    let mut bit_depth: Option<u32> = None;
    if let (Some(frame), Some(local_offset)) = (mpeg_frame.as_ref(), mpeg_frame_local) {
        bit_depth = Some(16);
        // Look for Xing / Info / VBRI header inside the first frame.
        let xing_offset = xing_data_offset(frame);
        let xing_candidate_start = local_offset + xing_offset;
        let mut vbr_frames: Option<u32> = None;

        if xing_candidate_start + 8 <= bytes.len() {
            let tag = &bytes[xing_candidate_start..xing_candidate_start + 4];
            if tag == b"Xing" || tag == b"Info" {
                let flags_bytes =
                    &bytes[xing_candidate_start + 4..xing_candidate_start + 8];
                let flags = u32::from_be_bytes([
                    flags_bytes[0],
                    flags_bytes[1],
                    flags_bytes[2],
                    flags_bytes[3],
                ]);
                if flags & 0x0001 != 0 && xing_candidate_start + 12 <= bytes.len() {
                    let fc = &bytes[xing_candidate_start + 8..xing_candidate_start + 12];
                    vbr_frames =
                        Some(u32::from_be_bytes([fc[0], fc[1], fc[2], fc[3]]));
                }
            }
        }

        // VBRI header sits at a fixed offset (36 bytes after frame start).
        if vbr_frames.is_none() {
            let vbri_start = local_offset + 36;
            if vbri_start + 32 <= bytes.len() && &bytes[vbri_start..vbri_start + 4] == b"VBRI" {
                // VBRI frame-count is at offset +14 (u32 big-endian) from "VBRI".
                let fc = &bytes[vbri_start + 14..vbri_start + 18];
                vbr_frames = Some(u32::from_be_bytes([fc[0], fc[1], fc[2], fc[3]]));
            }
        }

        if let Some(frames) = vbr_frames {
            let samples = frames as u64 * frame.samples_per_frame() as u64;
            if frame.sample_rate_hz > 0 {
                duration_seconds = Some(samples as f64 / frame.sample_rate_hz as f64);
            }
        } else {
            // No Xing/VBRI — attempt CBR duration from bitrate + audio bytes.
            if frame.bitrate_bps > 0 {
                let audio_bytes = (bytes.len() - local_offset) as u64;
                duration_seconds =
                    Some(audio_bytes as f64 * 8.0 / frame.bitrate_bps as f64);
            } else {
                // Free-format or unknown bitrate: cannot determine duration.
                issues.push(Issue {
                    severity: Severity::Info,
                    code: "mp3_vbr_duration_unknown".into(),
                    message: "MP3 duration cannot be determined (VBR without Xing/VBRI or free-format bitrate)".into(),
                    offset: Some(frame.offset),
                    context: None,
                });
            }
        }
    }

    Ok(Id3Container {
        nodes,
        issues,
        mpeg_frame_offset,
        mpeg_frame,
        duration_seconds,
        bit_depth,
        id3v2_version_major,
        id3v2_version_minor,
        id3v2_flags,
        id3v2_payload_offset: if has_id3v2 { base_offset + 10 } else { 0 },
        id3v2_payload_len,
        has_id3v2,
        raw: bytes.to_vec(),
    })
}

// Offset (relative to frame sync start) where a Xing/Info header would begin,
// which depends on MPEG version and channel mode.
fn xing_data_offset(frame: &MpegFrameHeader) -> usize {
    // After the 4-byte frame header, there's a side-information region:
    //   MPEG-1 stereo: 32 bytes; MPEG-1 mono: 17 bytes
    //   MPEG-2/2.5 stereo: 17 bytes; MPEG-2/2.5 mono: 9 bytes
    let side_info = match (frame.version, frame.channels) {
        (MpegVersion::V1, 1) => 17,
        (MpegVersion::V1, _) => 32,
        (_, 1) => 9,
        (_, _) => 17,
    };
    4 + side_info
}

fn find_mpeg_frame(
    bytes: &[u8],
    start: usize,
    base_offset: u64,
) -> Option<(usize, MpegFrameHeader)> {
    // Scan up to 64 KiB past `start` (enough for realistic padding/garbage).
    let scan_end = (start + 65536).min(bytes.len().saturating_sub(3));
    let mut cursor = start;
    while cursor < scan_end {
        if bytes[cursor] == 0xFF && (bytes[cursor + 1] & 0xE0) == 0xE0 {
            if let Some(header) =
                decode_mpeg_header(&bytes[cursor..cursor + 4], base_offset + cursor as u64)
            {
                return Some((cursor, header));
            }
        }
        cursor += 1;
    }
    None
}

fn decode_mpeg_header(hdr: &[u8], absolute_offset: u64) -> Option<MpegFrameHeader> {
    if hdr.len() < 4 {
        return None;
    }
    let b1 = hdr[1];
    let b2 = hdr[2];
    let b3 = hdr[3];

    let version = match (b1 >> 3) & 0b11 {
        0b00 => MpegVersion::V25,
        0b10 => MpegVersion::V2,
        0b11 => MpegVersion::V1,
        _ => return None, // reserved
    };
    let layer = match (b1 >> 1) & 0b11 {
        0b01 => MpegLayer::LayerIII,
        0b10 => MpegLayer::LayerII,
        0b11 => MpegLayer::LayerI,
        _ => return None,
    };

    let bitrate_index = (b2 >> 4) & 0b1111;
    let sample_rate_index = (b2 >> 2) & 0b11;
    let channel_mode = (b3 >> 6) & 0b11;

    let bitrate_bps = bitrate_table(version, layer, bitrate_index)?;
    let sample_rate_hz = sample_rate_table(version, sample_rate_index)?;
    let channels = if channel_mode == 0b11 { 1 } else { 2 };

    Some(MpegFrameHeader {
        version,
        layer,
        bitrate_bps,
        sample_rate_hz,
        channels,
        offset: absolute_offset,
    })
}

fn bitrate_table(version: MpegVersion, layer: MpegLayer, index: u8) -> Option<u32> {
    if index == 0 {
        // Free-format: declared bitrate is unknown.
        return Some(0);
    }
    if index == 0b1111 {
        return None;
    }
    let i = index as usize;
    // Values in kbps.
    let kbps = match (version, layer) {
        (MpegVersion::V1, MpegLayer::LayerI) => [
            0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448,
        ][i],
        (MpegVersion::V1, MpegLayer::LayerII) => [
            0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384,
        ][i],
        (MpegVersion::V1, MpegLayer::LayerIII) => {
            [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320][i]
        }
        (_, MpegLayer::LayerI) => {
            [0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256][i]
        }
        (_, _) => [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160][i],
    };
    Some((kbps as u32) * 1000)
}

fn sample_rate_table(version: MpegVersion, index: u8) -> Option<u32> {
    if index == 0b11 {
        return None;
    }
    let i = index as usize;
    Some(match version {
        MpegVersion::V1 => [44100, 48000, 32000][i],
        MpegVersion::V2 => [22050, 24000, 16000][i],
        MpegVersion::V25 => [11025, 12000, 8000][i],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Build a single MPEG-1 Layer III, 128 kbps, 44.1 kHz, stereo frame header + filler.
    fn mp3_frame_header_stereo_cbr_128() -> [u8; 4] {
        // 11111111 11111011 10010000 00000000
        // sync=0xFFE, version=MPEG-1 (11), layer=III (01), protection=1
        // bitrate_idx=1001 (128 kbps), samplerate_idx=00 (44100), padding=0, private=0
        // channel_mode=00 (stereo), mode_ext=00, copy=0, orig=0, emph=00
        [0xFF, 0xFB, 0x90, 0x00]
    }

    fn mp3_frame_header_mono_cbr_128() -> [u8; 4] {
        // channel_mode=11 (mono)
        [0xFF, 0xFB, 0x90, 0xC0]
    }

    fn id3v2_tag(body_size: u32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"ID3");
        out.push(3); // version 2.3
        out.push(0);
        out.push(0);
        // Syncsafe encode
        let s0 = ((body_size >> 21) & 0x7F) as u8;
        let s1 = ((body_size >> 14) & 0x7F) as u8;
        let s2 = ((body_size >> 7) & 0x7F) as u8;
        let s3 = (body_size & 0x7F) as u8;
        out.extend_from_slice(&[s0, s1, s2, s3]);
        out.extend(std::iter::repeat_n(0, body_size as usize));
        out
    }

    #[test]
    fn parses_minimal_cbr_mp3_without_id3() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&mp3_frame_header_stereo_cbr_128());
        // Add 4000 bytes of audio payload.
        bytes.extend(std::iter::repeat_n(0u8, 4000));
        let parsed = parse_bytes(&bytes, 0).unwrap();
        let frame = parsed.mpeg_frame.expect("frame");
        assert_eq!(frame.sample_rate_hz, 44100);
        assert_eq!(frame.channels, 2);
        assert_eq!(frame.bitrate_bps, 128_000);
        assert_eq!(frame.layer, MpegLayer::LayerIII);
        assert_eq!(parsed.bit_depth, Some(16));
        // CBR duration = (4004 * 8) / 128000 ≈ 0.25 s
        let duration = parsed.duration_seconds.unwrap();
        assert!((duration - 0.25).abs() < 0.05, "duration {duration}");
    }

    #[test]
    fn parses_mp3_with_id3v2_tag_and_finds_frame() {
        let mut bytes = id3v2_tag(32);
        bytes.extend_from_slice(&mp3_frame_header_stereo_cbr_128());
        bytes.extend(std::iter::repeat_n(0u8, 1000));
        let parsed = parse_bytes(&bytes, 0).unwrap();
        let payload = parsed.id3v2_payload().expect("id3v2 payload");
        assert_eq!(payload.version_major, 3);
        assert_eq!(payload.bytes.len(), 32);
        assert_eq!(payload.offset_start, 10);
        let frame = parsed.mpeg_frame.expect("frame");
        assert_eq!(frame.offset, 10 + 32);
    }

    #[test]
    fn computes_xing_vbr_duration() {
        // MPEG-1 stereo => Xing data at offset 4 + 32 = 36 from frame start.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&mp3_frame_header_stereo_cbr_128());
        bytes.extend(std::iter::repeat_n(0u8, 32)); // side info
        bytes.extend_from_slice(b"Xing");
        bytes.extend_from_slice(&0x0001u32.to_be_bytes()); // flags: frames present
        bytes.extend_from_slice(&100u32.to_be_bytes()); // 100 frames
        // Samples per frame for MPEG-1 Layer III = 1152
        // Expected duration = 100 * 1152 / 44100 ≈ 2.612 s
        let parsed = parse_bytes(&bytes, 0).unwrap();
        let duration = parsed.duration_seconds.unwrap();
        let expected = 100.0 * 1152.0 / 44100.0;
        assert!(
            (duration - expected).abs() < 0.001,
            "duration {duration} vs {expected}"
        );
    }

    #[test]
    fn emits_info_issue_for_free_format_without_xing() {
        // Free-format bitrate (index=0) and no Xing/VBRI -> info issue.
        let free_format_header = [0xFF, 0xFB, 0x00, 0x00];
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&free_format_header);
        bytes.extend(std::iter::repeat_n(0u8, 200));
        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert!(parsed.duration_seconds.is_none());
        assert!(
            parsed
                .issues
                .iter()
                .any(|i| i.code == "mp3_vbr_duration_unknown")
        );
    }

    #[test]
    fn mono_frame_decodes_channel_count() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&mp3_frame_header_mono_cbr_128());
        bytes.extend(std::iter::repeat_n(0u8, 500));
        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert_eq!(parsed.mpeg_frame.unwrap().channels, 1);
    }
}
