//! FLAC container framing.
//!
//! Walks the `fLaC` magic + chain of `METADATA_BLOCK_HEADER` records and
//! exposes typed views of the STREAMINFO, VORBIS_COMMENT, and PICTURE
//! blocks. Metadata interpretation lives elsewhere — this crate owns
//! the stream framing only, matching the container/metadata boundary
//! established by the image containers.

use xifty_core::{ContainerNode, Issue, Severity, XiftyError, issue};
use xifty_source::{Cursor, SourceBytes};

pub const FLAC_MAGIC: &[u8; 4] = b"fLaC";

const BLOCK_TYPE_STREAMINFO: u8 = 0;
const BLOCK_TYPE_PADDING: u8 = 1;
const BLOCK_TYPE_APPLICATION: u8 = 2;
const BLOCK_TYPE_SEEKTABLE: u8 = 3;
const BLOCK_TYPE_VORBIS_COMMENT: u8 = 4;
const BLOCK_TYPE_CUESHEET: u8 = 5;
const BLOCK_TYPE_PICTURE: u8 = 6;

#[derive(Debug, Clone)]
pub struct StreamInfo {
    pub sample_rate_hz: u32,
    pub channels: u8,
    pub bits_per_sample: u8,
    pub total_samples: u64,
    pub duration_seconds: Option<f64>,
    pub offset_start: u64,
    pub offset_end: u64,
}

#[derive(Debug, Clone)]
pub struct VorbisCommentBlock {
    pub offset_start: u64,
    pub offset_end: u64,
    pub data_offset: u64,
    pub data_length: u32,
}

#[derive(Debug, Clone)]
pub struct FlacPicture {
    pub picture_type: u32,
    pub mime_type: String,
    pub description: String,
    pub width: u32,
    pub height: u32,
    pub color_depth: u32,
    pub colors_used: u32,
    pub offset_start: u64,
    pub offset_end: u64,
}

#[derive(Debug, Clone)]
pub struct FlacContainer {
    pub nodes: Vec<ContainerNode>,
    pub issues: Vec<Issue>,
    pub stream_info: Option<StreamInfo>,
    pub vorbis_comment: Option<VorbisCommentBlock>,
    pub pictures: Vec<FlacPicture>,
}

pub fn parse(source: &SourceBytes) -> Result<FlacContainer, XiftyError> {
    parse_bytes(source.bytes(), 0)
}

pub fn parse_bytes(bytes: &[u8], base_offset: u64) -> Result<FlacContainer, XiftyError> {
    let cursor = Cursor::new(bytes, base_offset);
    if cursor.len() < 4 || cursor.slice(0, 4)? != FLAC_MAGIC {
        return Err(XiftyError::Parse {
            message: "not a flac".into(),
        });
    }

    let mut nodes = vec![ContainerNode {
        kind: "container".into(),
        label: "flac".into(),
        offset_start: base_offset,
        offset_end: base_offset + bytes.len() as u64,
        parent_label: None,
    }];
    let mut issues = Vec::new();
    let mut stream_info: Option<StreamInfo> = None;
    let mut vorbis_comment: Option<VorbisCommentBlock> = None;
    let mut pictures: Vec<FlacPicture> = Vec::new();

    let mut offset = 4usize;
    let mut saw_last = false;

    while offset < cursor.len() {
        if offset + 4 > cursor.len() {
            issues.push(Issue {
                severity: Severity::Warning,
                code: "flac_block_header_out_of_bounds".into(),
                message: "truncated flac metadata block header".into(),
                offset: Some(cursor.absolute_offset(offset)),
                context: None,
            });
            break;
        }
        let header = cursor.slice(offset, 4)?;
        let flag_and_type = header[0];
        let last = (flag_and_type & 0x80) != 0;
        let block_type = flag_and_type & 0x7F;
        let length =
            ((header[1] as usize) << 16) | ((header[2] as usize) << 8) | (header[3] as usize);
        let data_offset = offset + 4;
        let block_end = data_offset + length;
        if block_end > cursor.len() {
            issues.push(Issue {
                severity: Severity::Warning,
                code: "flac_malformed_block".into(),
                message: format!("flac metadata block {block_type} exceeds available bytes"),
                offset: Some(cursor.absolute_offset(offset)),
                context: Some(format!("type={block_type} length={length}")),
            });
            break;
        }

        let label = block_label(block_type);
        nodes.push(ContainerNode {
            kind: "block".into(),
            label: label.into(),
            offset_start: cursor.absolute_offset(offset),
            offset_end: cursor.absolute_offset(block_end),
            parent_label: Some("flac".into()),
        });

        let abs_data_offset = cursor.absolute_offset(data_offset);
        let abs_block_end = cursor.absolute_offset(block_end);

        match block_type {
            BLOCK_TYPE_STREAMINFO => {
                let payload = cursor.slice(data_offset, length)?;
                match decode_stream_info(payload, abs_data_offset, abs_block_end) {
                    Ok(info) => {
                        if info.sample_rate_hz == 0 {
                            issues.push(Issue {
                                severity: Severity::Warning,
                                code: "flac_streaminfo_sample_rate_zero".into(),
                                message: "flac STREAMINFO decoded sample rate of 0".into(),
                                offset: Some(abs_data_offset),
                                context: Some("streaminfo".into()),
                            });
                        }
                        stream_info = Some(info);
                    }
                    Err(message) => issues.push(Issue {
                        severity: Severity::Warning,
                        code: "flac_streaminfo_invalid".into(),
                        message,
                        offset: Some(abs_data_offset),
                        context: Some("streaminfo".into()),
                    }),
                }
            }
            BLOCK_TYPE_VORBIS_COMMENT => {
                vorbis_comment = Some(VorbisCommentBlock {
                    offset_start: cursor.absolute_offset(offset),
                    offset_end: abs_block_end,
                    data_offset: abs_data_offset,
                    data_length: length as u32,
                });
            }
            BLOCK_TYPE_PICTURE => {
                let payload = cursor.slice(data_offset, length)?;
                match decode_picture(payload, abs_data_offset, abs_block_end) {
                    Ok(picture) => pictures.push(picture),
                    Err(message) => issues.push(Issue {
                        severity: Severity::Warning,
                        code: "flac_picture_invalid".into(),
                        message,
                        offset: Some(abs_data_offset),
                        context: Some("picture".into()),
                    }),
                }
            }
            BLOCK_TYPE_PADDING
            | BLOCK_TYPE_APPLICATION
            | BLOCK_TYPE_SEEKTABLE
            | BLOCK_TYPE_CUESHEET => {
                // Framed only; body is not interpreted in phase 1.
            }
            _ => {
                // Reserved block type — framed and recorded, but not interpreted.
            }
        }

        offset = block_end;
        if last {
            saw_last = true;
            break;
        }
    }

    if !saw_last {
        issues.push(issue(
            Severity::Warning,
            "flac_last_block_flag_missing",
            "flac metadata block chain ended without a last-block flag",
        ));
    }

    Ok(FlacContainer {
        nodes,
        issues,
        stream_info,
        vorbis_comment,
        pictures,
    })
}

fn block_label(block_type: u8) -> &'static str {
    match block_type {
        BLOCK_TYPE_STREAMINFO => "streaminfo",
        BLOCK_TYPE_PADDING => "padding",
        BLOCK_TYPE_APPLICATION => "application",
        BLOCK_TYPE_SEEKTABLE => "seektable",
        BLOCK_TYPE_VORBIS_COMMENT => "vorbis_comment",
        BLOCK_TYPE_CUESHEET => "cuesheet",
        BLOCK_TYPE_PICTURE => "picture",
        _ => "reserved",
    }
}

fn decode_stream_info(
    payload: &[u8],
    offset_start: u64,
    offset_end: u64,
) -> Result<StreamInfo, String> {
    // STREAMINFO is exactly 34 bytes. Bit layout:
    //   16 min blocksize | 16 max blocksize | 24 min framesize | 24 max framesize
    //   20 sample rate | 3 channels-1 | 5 bits_per_sample-1 | 36 total_samples | 128 md5
    if payload.len() < 34 {
        return Err(format!(
            "streaminfo block length {} below required 34 bytes",
            payload.len()
        ));
    }
    // Skip 2+2+3+3 = 10 bytes (min/max block & frame sizes).
    let packed = &payload[10..18];
    // First 20 bits of packed = sample rate.
    let sample_rate_hz =
        ((packed[0] as u32) << 12) | ((packed[1] as u32) << 4) | ((packed[2] as u32) >> 4);
    // Next 3 bits = channels-1.
    let channels = ((packed[2] >> 1) & 0x07) + 1;
    // Next 5 bits = bits_per_sample-1 (split across packed[2] low bit and packed[3] high bits).
    let bits_per_sample = (((packed[2] & 0x01) << 4) | (packed[3] >> 4)) + 1;
    // Next 36 bits = total samples: low nibble of packed[3] is the high 4 bits,
    // then packed[4..8] are the low 32 bits.
    let total_samples = ((packed[3] as u64 & 0x0F) << 32)
        | ((packed[4] as u64) << 24)
        | ((packed[5] as u64) << 16)
        | ((packed[6] as u64) << 8)
        | (packed[7] as u64);
    let duration_seconds = if sample_rate_hz > 0 && total_samples > 0 {
        Some(total_samples as f64 / sample_rate_hz as f64)
    } else {
        None
    };
    Ok(StreamInfo {
        sample_rate_hz,
        channels,
        bits_per_sample,
        total_samples,
        duration_seconds,
        offset_start,
        offset_end,
    })
}

fn decode_picture(
    payload: &[u8],
    offset_start: u64,
    offset_end: u64,
) -> Result<FlacPicture, String> {
    let mut cursor = 0usize;
    let picture_type = read_u32_be(payload, &mut cursor)?;
    let mime_len = read_u32_be(payload, &mut cursor)? as usize;
    let mime_bytes = read_slice(payload, &mut cursor, mime_len)?;
    let mime_type = std::str::from_utf8(mime_bytes)
        .map_err(|_| "picture mime type is not valid utf-8".to_string())?
        .to_string();
    let desc_len = read_u32_be(payload, &mut cursor)? as usize;
    let desc_bytes = read_slice(payload, &mut cursor, desc_len)?;
    let description = std::str::from_utf8(desc_bytes)
        .map_err(|_| "picture description is not valid utf-8".to_string())?
        .to_string();
    let width = read_u32_be(payload, &mut cursor)?;
    let height = read_u32_be(payload, &mut cursor)?;
    let color_depth = read_u32_be(payload, &mut cursor)?;
    let colors_used = read_u32_be(payload, &mut cursor)?;
    let data_len = read_u32_be(payload, &mut cursor)? as usize;
    // Validate that the data region fits, but do not copy the bytes.
    if cursor
        .checked_add(data_len)
        .map(|end| end > payload.len())
        .unwrap_or(true)
    {
        return Err("picture data length exceeds block".into());
    }
    Ok(FlacPicture {
        picture_type,
        mime_type,
        description,
        width,
        height,
        color_depth,
        colors_used,
        offset_start,
        offset_end,
    })
}

fn read_u32_be(payload: &[u8], cursor: &mut usize) -> Result<u32, String> {
    let slice = read_slice(payload, cursor, 4)?;
    Ok(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_slice<'a>(payload: &'a [u8], cursor: &mut usize, len: usize) -> Result<&'a [u8], String> {
    let end = cursor
        .checked_add(len)
        .ok_or_else(|| "picture field offset overflow".to_string())?;
    let slice = payload
        .get(*cursor..end)
        .ok_or_else(|| format!("picture field out of bounds: need {len} bytes at {cursor}"))?;
    *cursor = end;
    Ok(slice)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block_header(last: bool, block_type: u8, length: usize) -> [u8; 4] {
        let last_flag = if last { 0x80 } else { 0 };
        [
            last_flag | (block_type & 0x7F),
            ((length >> 16) & 0xFF) as u8,
            ((length >> 8) & 0xFF) as u8,
            (length & 0xFF) as u8,
        ]
    }

    fn streaminfo_body(sample_rate: u32, channels: u8, bps: u8, total_samples: u64) -> Vec<u8> {
        let mut body = vec![0u8; 34];
        // min/max block/frame sizes left as zero.
        let channels_minus_one = (channels - 1) as u32;
        let bps_minus_one = (bps - 1) as u32;
        // Pack the 20+3+5+36 bit group into bytes [10..18].
        body[10] = ((sample_rate >> 12) & 0xFF) as u8;
        body[11] = ((sample_rate >> 4) & 0xFF) as u8;
        body[12] = (((sample_rate & 0x0F) << 4) as u8)
            | (((channels_minus_one & 0x07) << 1) as u8)
            | (((bps_minus_one >> 4) & 0x01) as u8);
        body[13] = (((bps_minus_one & 0x0F) << 4) as u8) | (((total_samples >> 32) & 0x0F) as u8);
        body[14] = ((total_samples >> 24) & 0xFF) as u8;
        body[15] = ((total_samples >> 16) & 0xFF) as u8;
        body[16] = ((total_samples >> 8) & 0xFF) as u8;
        body[17] = (total_samples & 0xFF) as u8;
        body
    }

    fn vorbis_comment_body() -> Vec<u8> {
        let mut body = Vec::new();
        let vendor = b"xifty";
        body.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        body.extend_from_slice(vendor);
        body.extend_from_slice(&0u32.to_le_bytes()); // comment count
        body
    }

    fn picture_body() -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&3u32.to_be_bytes()); // picture type (front cover)
        let mime = b"image/png";
        body.extend_from_slice(&(mime.len() as u32).to_be_bytes());
        body.extend_from_slice(mime);
        let desc = b"cover";
        body.extend_from_slice(&(desc.len() as u32).to_be_bytes());
        body.extend_from_slice(desc);
        body.extend_from_slice(&640u32.to_be_bytes()); // width
        body.extend_from_slice(&480u32.to_be_bytes()); // height
        body.extend_from_slice(&24u32.to_be_bytes()); // color depth
        body.extend_from_slice(&0u32.to_be_bytes()); // colors used
        let data = [0u8, 1, 2, 3];
        body.extend_from_slice(&(data.len() as u32).to_be_bytes());
        body.extend_from_slice(&data);
        body
    }

    fn assemble(blocks: Vec<(bool, u8, Vec<u8>)>) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(FLAC_MAGIC);
        for (last, block_type, body) in blocks {
            out.extend_from_slice(&block_header(last, block_type, body.len()));
            out.extend_from_slice(&body);
        }
        out
    }

    #[test]
    fn parses_streaminfo_sample_rate_and_duration() {
        let body = streaminfo_body(44100, 2, 16, 44100);
        let bytes = assemble(vec![(true, BLOCK_TYPE_STREAMINFO, body)]);
        let parsed = parse_bytes(&bytes, 0).unwrap();
        let info = parsed.stream_info.expect("streaminfo decoded");
        assert_eq!(info.sample_rate_hz, 44100);
        assert_eq!(info.channels, 2);
        assert_eq!(info.bits_per_sample, 16);
        assert_eq!(info.total_samples, 44100);
        assert_eq!(info.duration_seconds, Some(1.0));
        assert!(
            parsed
                .issues
                .iter()
                .all(|iss| iss.code != "flac_last_block_flag_missing")
        );
    }

    #[test]
    fn surfaces_vorbis_comment_and_picture_blocks() {
        let bytes = assemble(vec![
            (
                false,
                BLOCK_TYPE_STREAMINFO,
                streaminfo_body(48000, 1, 24, 48000),
            ),
            (false, BLOCK_TYPE_VORBIS_COMMENT, vorbis_comment_body()),
            (true, BLOCK_TYPE_PICTURE, picture_body()),
        ]);
        let parsed = parse_bytes(&bytes, 0).unwrap();
        let vc = parsed.vorbis_comment.expect("vorbis_comment block found");
        assert!(vc.data_length > 0);
        assert_eq!(parsed.pictures.len(), 1);
        let pic = &parsed.pictures[0];
        assert_eq!(pic.mime_type, "image/png");
        assert_eq!(pic.width, 640);
        assert_eq!(pic.height, 480);
        assert_eq!(pic.color_depth, 24);
        assert!(
            parsed
                .nodes
                .iter()
                .any(|n| n.label == "vorbis_comment" && n.kind == "block")
        );
    }

    #[test]
    fn flags_truncated_block_header() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(FLAC_MAGIC);
        // Declare a streaminfo block with length 100 but only give 4 bytes of body.
        bytes.extend_from_slice(&block_header(true, BLOCK_TYPE_STREAMINFO, 100));
        bytes.extend_from_slice(&[0u8; 4]);
        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert!(
            parsed
                .issues
                .iter()
                .any(|iss| iss.code == "flac_malformed_block")
        );
    }

    #[test]
    fn rejects_non_flac_magic() {
        let bytes = [0u8; 8];
        assert!(parse_bytes(&bytes, 0).is_err());
    }
}
