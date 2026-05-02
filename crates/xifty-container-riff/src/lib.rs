use xifty_core::{ContainerNode, Issue, Severity, XiftyError, issue};
use xifty_source::{Cursor, Endian, SourceBytes};

#[derive(Debug, Clone)]
pub struct RiffChunk {
    pub chunk_id: [u8; 4],
    pub offset_start: u64,
    pub offset_end: u64,
    pub data_offset: u64,
    pub data_length: u32,
}

#[derive(Debug, Clone)]
pub struct RiffContainer {
    pub form_type: [u8; 4],
    pub nodes: Vec<ContainerNode>,
    pub chunks: Vec<RiffChunk>,
    pub issues: Vec<Issue>,
}

impl RiffContainer {
    pub fn exif_payloads(&self) -> impl Iterator<Item = &RiffChunk> {
        self.chunks
            .iter()
            .filter(|chunk| &chunk.chunk_id == b"EXIF")
    }

    pub fn xmp_payloads(&self) -> impl Iterator<Item = &RiffChunk> {
        self.chunks
            .iter()
            .filter(|chunk| &chunk.chunk_id == b"XMP ")
    }

    pub fn icc_payloads(&self) -> impl Iterator<Item = &RiffChunk> {
        self.chunks
            .iter()
            .filter(|chunk| &chunk.chunk_id == b"ICCP")
    }

    pub fn iptc_payloads(&self) -> impl Iterator<Item = &RiffChunk> {
        self.chunks
            .iter()
            .filter(|chunk| &chunk.chunk_id == b"IPTC")
    }

    /// First WAVE `fmt ` chunk (with the canonical trailing space). Returns
    /// `None` for non-WAVE forms or malformed files missing the chunk.
    pub fn fmt_chunk(&self) -> Option<&RiffChunk> {
        self.chunks.iter().find(|chunk| &chunk.chunk_id == b"fmt ")
    }

    /// First WAVE `data` chunk; the byte length lives on `RiffChunk::data_length`.
    pub fn data_chunk(&self) -> Option<&RiffChunk> {
        self.chunks.iter().find(|chunk| &chunk.chunk_id == b"data")
    }

    /// First Broadcast Wave `bext` chunk, when present.
    pub fn bext_chunk(&self) -> Option<&RiffChunk> {
        self.chunks.iter().find(|chunk| &chunk.chunk_id == b"bext")
    }

    /// First iXML chunk, when present.
    pub fn ixml_chunk(&self) -> Option<&RiffChunk> {
        self.chunks.iter().find(|chunk| &chunk.chunk_id == b"iXML")
    }
}

/// Decoded view of a WAVE `fmt ` chunk's leading 16 bytes (PCM-WAVEFORMAT).
///
/// Extensible WAVE (`format_tag == 0xFFFE`) and IEEE-float (`0x0003`) variants
/// store additional fields after these 16 bytes; callers that need codec-
/// specific data should read further into the chunk payload themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WavFormat {
    pub format_tag: u16,
    pub channels: u16,
    pub sample_rate: u32,
    pub byte_rate: u32,
    pub block_align: u16,
    pub bits_per_sample: u16,
}

/// Parse the leading 16 bytes of a WAVE `fmt ` chunk as little-endian
/// PCM-WAVEFORMAT. Returns `None` if the payload is too short.
pub fn parse_wav_format(payload: &[u8]) -> Option<WavFormat> {
    if payload.len() < 16 {
        return None;
    }
    Some(WavFormat {
        format_tag: u16::from_le_bytes(payload[0..2].try_into().ok()?),
        channels: u16::from_le_bytes(payload[2..4].try_into().ok()?),
        sample_rate: u32::from_le_bytes(payload[4..8].try_into().ok()?),
        byte_rate: u32::from_le_bytes(payload[8..12].try_into().ok()?),
        block_align: u16::from_le_bytes(payload[12..14].try_into().ok()?),
        bits_per_sample: u16::from_le_bytes(payload[14..16].try_into().ok()?),
    })
}

pub fn parse(source: &SourceBytes) -> Result<RiffContainer, XiftyError> {
    parse_bytes(source.bytes(), 0)
}

pub fn parse_bytes(bytes: &[u8], base_offset: u64) -> Result<RiffContainer, XiftyError> {
    let cursor = Cursor::new(bytes, base_offset);
    if cursor.len() < 12 || cursor.slice(0, 4)? != b"RIFF" {
        return Err(XiftyError::Parse {
            message: "not a riff container".into(),
        });
    }

    let riff_size = cursor.read_u32(4, Endian::Little)? as usize;
    let form_type_bytes = cursor.slice(8, 4)?;
    let form_type = [
        form_type_bytes[0],
        form_type_bytes[1],
        form_type_bytes[2],
        form_type_bytes[3],
    ];
    let mut issues = Vec::new();
    if cursor.len() >= 8 && riff_size + 8 != cursor.len() {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "riff_size_mismatch".into(),
            message: format!(
                "riff declared size {} does not match actual size {}",
                riff_size + 8,
                cursor.len()
            ),
            offset: Some(base_offset + 4),
            context: Some(String::from_utf8_lossy(&form_type).into_owned()),
        });
    }

    let root_label = if &form_type == b"WEBP" {
        "webp"
    } else if &form_type == b"WAVE" {
        "wav"
    } else {
        "riff"
    };
    let mut nodes = vec![ContainerNode {
        kind: "container".into(),
        label: root_label.into(),
        offset_start: base_offset,
        offset_end: base_offset + bytes.len() as u64,
        parent_label: None,
    }];
    let mut chunks = Vec::new();
    let mut offset = 12usize;

    while offset < cursor.len() {
        if offset + 8 > cursor.len() {
            issues.push(Issue {
                severity: Severity::Warning,
                code: "riff_chunk_header_out_of_bounds".into(),
                message: "truncated riff chunk header".into(),
                offset: Some(cursor.absolute_offset(offset)),
                context: None,
            });
            break;
        }

        let chunk_id_bytes = cursor.slice(offset, 4)?;
        let chunk_id = [
            chunk_id_bytes[0],
            chunk_id_bytes[1],
            chunk_id_bytes[2],
            chunk_id_bytes[3],
        ];
        let data_length = cursor.read_u32(offset + 4, Endian::Little)? as usize;
        let padded_length = data_length + (data_length % 2);
        let chunk_end = offset + 8 + padded_length;
        if chunk_end > cursor.len() {
            issues.push(Issue {
                severity: Severity::Warning,
                code: "riff_chunk_length_invalid".into(),
                message: format!(
                    "riff chunk {} exceeds available bytes",
                    String::from_utf8_lossy(&chunk_id)
                ),
                offset: Some(cursor.absolute_offset(offset)),
                context: Some(String::from_utf8_lossy(&chunk_id).into_owned()),
            });
            break;
        }

        let label = String::from_utf8_lossy(&chunk_id).into_owned();
        nodes.push(ContainerNode {
            kind: "chunk".into(),
            label: label.clone(),
            offset_start: cursor.absolute_offset(offset),
            offset_end: cursor.absolute_offset(chunk_end),
            parent_label: Some(root_label.into()),
        });
        chunks.push(RiffChunk {
            chunk_id,
            offset_start: cursor.absolute_offset(offset),
            offset_end: cursor.absolute_offset(chunk_end),
            data_offset: cursor.absolute_offset(offset + 8),
            data_length: data_length as u32,
        });

        offset = chunk_end;
    }

    // The `riff_non_webp_form` info issue is a "this RIFF flavour is not yet
    // supported" hint. WAVE is now first-class (Issue #54), so suppress the
    // hint for both WEBP and WAVE; emit it only for genuinely unrecognised
    // RIFF form types (AVI, RMID, etc.).
    if &form_type != b"WEBP" && &form_type != b"WAVE" {
        issues.push(issue(
            Severity::Info,
            "riff_non_webp_form",
            format!(
                "riff container form type {} is not WEBP or WAVE",
                String::from_utf8_lossy(&form_type)
            ),
        ));
    }

    Ok(RiffContainer {
        form_type,
        nodes,
        chunks,
        issues,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_webp_riff() {
        let bytes = [b'R', b'I', b'F', b'F', 4, 0, 0, 0, b'W', b'E', b'B', b'P'];
        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert_eq!(&parsed.form_type, b"WEBP");
    }

    #[test]
    fn routes_iccp_chunks() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&12u32.to_le_bytes());
        bytes.extend_from_slice(b"WEBP");
        bytes.extend_from_slice(b"ICCP");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert!(parsed.icc_payloads().next().is_some());
    }

    fn build_wave(extra_chunks: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.extend_from_slice(b"WAVE");
        for (id, data) in extra_chunks {
            payload.extend_from_slice(*id);
            payload.extend_from_slice(&(data.len() as u32).to_le_bytes());
            payload.extend_from_slice(data);
            if data.len() % 2 == 1 {
                payload.push(0);
            }
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&payload);
        out
    }

    fn pcm_fmt() -> Vec<u8> {
        // PCM, 1 channel, 44100 Hz, 16-bit
        let format_tag: u16 = 1;
        let channels: u16 = 1;
        let sample_rate: u32 = 44100;
        let bits_per_sample: u16 = 16;
        let block_align: u16 = channels * bits_per_sample / 8;
        let byte_rate: u32 = sample_rate * block_align as u32;
        let mut data = Vec::new();
        data.extend_from_slice(&format_tag.to_le_bytes());
        data.extend_from_slice(&channels.to_le_bytes());
        data.extend_from_slice(&sample_rate.to_le_bytes());
        data.extend_from_slice(&byte_rate.to_le_bytes());
        data.extend_from_slice(&block_align.to_le_bytes());
        data.extend_from_slice(&bits_per_sample.to_le_bytes());
        data
    }

    #[test]
    fn parses_minimal_wave_riff() {
        let bytes = build_wave(&[(b"fmt ", pcm_fmt()), (b"data", vec![])]);
        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert_eq!(&parsed.form_type, b"WAVE");
        assert!(parsed.fmt_chunk().is_some());
        assert!(parsed.data_chunk().is_some());
    }

    #[test]
    fn suppresses_non_webp_info_for_wave() {
        let bytes = build_wave(&[(b"fmt ", pcm_fmt()), (b"data", vec![])]);
        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert!(
            !parsed
                .issues
                .iter()
                .any(|issue| issue.code == "riff_non_webp_form"),
            "WAVE form should not emit riff_non_webp_form, got: {:?}",
            parsed.issues
        );
    }

    #[test]
    fn emits_non_webp_info_for_avi() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(b"AVI ");
        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert!(
            parsed
                .issues
                .iter()
                .any(|issue| issue.code == "riff_non_webp_form"),
            "AVI form should still emit riff_non_webp_form"
        );
    }

    #[test]
    fn parses_wav_format() {
        let fmt = pcm_fmt();
        let format = parse_wav_format(&fmt).expect("decoded");
        assert_eq!(format.format_tag, 1);
        assert_eq!(format.channels, 1);
        assert_eq!(format.sample_rate, 44100);
        assert_eq!(format.bits_per_sample, 16);
        assert_eq!(format.block_align, 2);
        assert_eq!(format.byte_rate, 88200);
    }

    #[test]
    fn routes_iptc_chunks() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&12u32.to_le_bytes());
        bytes.extend_from_slice(b"WEBP");
        bytes.extend_from_slice(b"IPTC");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert!(parsed.iptc_payloads().next().is_some());
    }
}
