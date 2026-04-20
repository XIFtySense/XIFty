//! AIFF / AIFC (IFF) container framing.
//!
//! Walks the big-endian IFF chunk structure introduced by Apple's Audio
//! Interchange File Format. Exposes typed views of the `COMM` common
//! chunk (sample rate, channels, bit depth, frame count → duration) and
//! records offsets for `SSND` audio data and the optional `ID3 ` chunk
//! some AIFF writers emit. Metadata interpretation is intentionally out
//! of scope — this crate owns the stream framing only, matching the
//! container/metadata boundary established by the other containers.

use xifty_core::{ContainerNode, Issue, Severity, XiftyError, issue};
use xifty_source::{Cursor, SourceBytes};

pub const FORM_MAGIC: &[u8; 4] = b"FORM";
pub const FORM_TYPE_AIFF: &[u8; 4] = b"AIFF";
pub const FORM_TYPE_AIFC: &[u8; 4] = b"AIFC";

const CHUNK_ID_COMM: &[u8; 4] = b"COMM";
const CHUNK_ID_SSND: &[u8; 4] = b"SSND";
const CHUNK_ID_ID3: &[u8; 4] = b"ID3 ";

/// Decoded view of the AIFF/AIFC `COMM` chunk.
#[derive(Debug, Clone)]
pub struct AiffComm {
    pub num_channels: u16,
    pub num_sample_frames: u32,
    pub sample_size: u16,
    pub sample_rate: f64,
    /// Present for AIFC only; four-byte compression type identifier
    /// (e.g. `NONE`, `sowt`, `fl32`).
    pub compression_type: Option<[u8; 4]>,
    pub offset_start: u64,
    pub offset_end: u64,
}

/// Absolute byte offsets for a recorded IFF chunk.
#[derive(Debug, Clone)]
pub struct AiffChunk {
    pub id: [u8; 4],
    pub offset_start: u64,
    pub offset_end: u64,
    pub data_offset: u64,
    pub data_length: u32,
}

#[derive(Debug, Clone)]
pub struct AiffContainer {
    /// Always one of `b"AIFF"` or `b"AIFC"` when parsing succeeds; any
    /// other value is surfaced via an `aiff_non_standard_form` info
    /// issue but still recorded here verbatim.
    pub form_type: [u8; 4],
    pub nodes: Vec<ContainerNode>,
    pub chunks: Vec<AiffChunk>,
    pub issues: Vec<Issue>,
    pub comm: Option<AiffComm>,
    pub ssnd_offset: Option<u64>,
    pub id3_payload_offset: Option<u64>,
    pub id3_payload_len: Option<u32>,
    pub duration_seconds: Option<f64>,
    pub bit_depth: Option<u16>,
}

impl AiffContainer {
    /// Return the raw ID3v2 payload (with the IFF pad byte stripped if
    /// present) when an `ID3 ` chunk was recorded. Returns `None` when
    /// no such chunk exists or when the offsets fall outside `bytes`.
    pub fn id3v2_payload<'a>(&self, bytes: &'a [u8]) -> Option<&'a [u8]> {
        let offset = usize::try_from(self.id3_payload_offset?).ok()?;
        let len = self.id3_payload_len? as usize;
        bytes.get(offset..offset.checked_add(len)?)
    }
}

pub fn parse(source: &SourceBytes) -> Result<AiffContainer, XiftyError> {
    parse_bytes(source.bytes(), 0)
}

pub fn parse_bytes(bytes: &[u8], base_offset: u64) -> Result<AiffContainer, XiftyError> {
    let cursor = Cursor::new(bytes, base_offset);
    if cursor.len() < 12 || cursor.slice(0, 4)? != FORM_MAGIC {
        return Err(XiftyError::Parse {
            message: "not an aiff/aifc".into(),
        });
    }

    let form_size_raw = cursor.slice(4, 4)?;
    let form_size = u32::from_be_bytes([
        form_size_raw[0],
        form_size_raw[1],
        form_size_raw[2],
        form_size_raw[3],
    ]) as usize;

    let form_type_slice = cursor.slice(8, 4)?;
    let form_type: [u8; 4] = [
        form_type_slice[0],
        form_type_slice[1],
        form_type_slice[2],
        form_type_slice[3],
    ];

    let mut issues: Vec<Issue> = Vec::new();

    if &form_type != FORM_TYPE_AIFF && &form_type != FORM_TYPE_AIFC {
        issues.push(Issue {
            severity: Severity::Info,
            code: "aiff_non_standard_form".into(),
            message: format!(
                "unexpected IFF form type {:?}; continuing to walk chunks",
                String::from_utf8_lossy(&form_type)
            ),
            offset: Some(cursor.absolute_offset(8)),
            context: Some("form_type".into()),
        });
    }

    // FORM size counts the bytes after itself; honor it but clamp to
    // available data so a bogus size cannot read past the buffer.
    let declared_end = 8usize.saturating_add(form_size);
    let effective_end = declared_end.min(cursor.len());
    if declared_end > cursor.len() {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "aiff_form_size_truncated".into(),
            message: format!(
                "FORM size claims {form_size} bytes but only {} bytes follow",
                cursor.len().saturating_sub(8)
            ),
            offset: Some(cursor.absolute_offset(4)),
            context: Some("form_size".into()),
        });
    }

    let container_label = match &form_type {
        FORM_TYPE_AIFC => "aifc",
        _ => "aiff",
    };

    let mut nodes = vec![ContainerNode {
        kind: "container".into(),
        label: container_label.into(),
        offset_start: cursor.absolute_offset(0),
        offset_end: cursor.absolute_offset(effective_end),
        parent_label: None,
    }];
    let mut chunks: Vec<AiffChunk> = Vec::new();
    let mut comm: Option<AiffComm> = None;
    let mut ssnd_offset: Option<u64> = None;
    let mut id3_payload_offset: Option<u64> = None;
    let mut id3_payload_len: Option<u32> = None;

    let mut offset = 12usize;
    while offset < effective_end {
        if offset + 8 > effective_end {
            issues.push(Issue {
                severity: Severity::Warning,
                code: "aiff_chunk_header_truncated".into(),
                message: "truncated IFF chunk header".into(),
                offset: Some(cursor.absolute_offset(offset)),
                context: None,
            });
            break;
        }
        let header = cursor.slice(offset, 8)?;
        let id: [u8; 4] = [header[0], header[1], header[2], header[3]];
        let length = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);
        let data_offset = offset + 8;
        let Some(data_end) = data_offset.checked_add(length as usize) else {
            issues.push(Issue {
                severity: Severity::Warning,
                code: "aiff_chunk_length_invalid".into(),
                message: format!(
                    "IFF chunk {:?} length overflows usize",
                    String::from_utf8_lossy(&id)
                ),
                offset: Some(cursor.absolute_offset(offset)),
                context: Some("chunk_length".into()),
            });
            break;
        };
        if data_end > effective_end {
            issues.push(Issue {
                severity: Severity::Warning,
                code: "aiff_chunk_length_invalid".into(),
                message: format!(
                    "IFF chunk {:?} length {length} exceeds FORM bounds",
                    String::from_utf8_lossy(&id)
                ),
                offset: Some(cursor.absolute_offset(offset)),
                context: Some("chunk_length".into()),
            });
            break;
        }

        let abs_offset_start = cursor.absolute_offset(offset);
        let abs_data_offset = cursor.absolute_offset(data_offset);
        let abs_offset_end = cursor.absolute_offset(data_end);

        nodes.push(ContainerNode {
            kind: "chunk".into(),
            label: chunk_label(&id).into(),
            offset_start: abs_offset_start,
            offset_end: abs_offset_end,
            parent_label: Some(container_label.into()),
        });

        chunks.push(AiffChunk {
            id,
            offset_start: abs_offset_start,
            offset_end: abs_offset_end,
            data_offset: abs_data_offset,
            data_length: length,
        });

        match &id {
            CHUNK_ID_COMM => {
                let payload = cursor.slice(data_offset, length as usize)?;
                match decode_comm(payload, abs_data_offset, abs_offset_end, &form_type) {
                    Ok(decoded) => {
                        if !decoded.sample_rate.is_finite() || decoded.sample_rate <= 0.0 {
                            issues.push(Issue {
                                severity: Severity::Warning,
                                code: "aiff_comm_invalid_rate".into(),
                                message: format!(
                                    "AIFF COMM decoded non-positive sample rate {}",
                                    decoded.sample_rate
                                ),
                                offset: Some(abs_data_offset),
                                context: Some("comm".into()),
                            });
                        }
                        comm = Some(decoded);
                    }
                    Err(message) => issues.push(Issue {
                        severity: Severity::Warning,
                        code: "aiff_comm_invalid".into(),
                        message,
                        offset: Some(abs_data_offset),
                        context: Some("comm".into()),
                    }),
                }
            }
            CHUNK_ID_SSND => {
                ssnd_offset = Some(abs_data_offset);
            }
            CHUNK_ID_ID3 => {
                // IFF chunks are padded to even length; strip a trailing
                // pad byte when the declared length is odd so the
                // downstream ID3v2 decoder does not ingest it.
                let adjusted_len = if length % 2 == 1 {
                    length.saturating_sub(1)
                } else {
                    length
                };
                id3_payload_offset = Some(abs_data_offset);
                id3_payload_len = Some(adjusted_len);
            }
            _ => {
                // Unrecognized chunk — framed and recorded, not interpreted.
            }
        }

        // Advance past the data region, honoring the IFF even-length
        // padding rule (chunks are padded to a 16-bit boundary).
        let mut next = data_end;
        if length % 2 == 1 && next < effective_end {
            next += 1;
        }
        offset = next;
    }

    if comm.is_none() {
        issues.push(issue(
            Severity::Warning,
            "aiff_comm_missing",
            "AIFF/AIFC file did not include a COMM common chunk",
        ));
    }

    let (duration_seconds, bit_depth) = match &comm {
        Some(c) if c.sample_rate.is_finite() && c.sample_rate > 0.0 && c.num_sample_frames > 0 => (
            Some(c.num_sample_frames as f64 / c.sample_rate),
            Some(c.sample_size),
        ),
        Some(c) => (None, Some(c.sample_size)),
        None => (None, None),
    };

    Ok(AiffContainer {
        form_type,
        nodes,
        chunks,
        issues,
        comm,
        ssnd_offset,
        id3_payload_offset,
        id3_payload_len,
        duration_seconds,
        bit_depth,
    })
}

fn chunk_label(id: &[u8; 4]) -> &'static str {
    match id {
        CHUNK_ID_COMM => "comm",
        CHUNK_ID_SSND => "ssnd",
        CHUNK_ID_ID3 => "id3",
        b"MARK" => "mark",
        b"INST" => "inst",
        b"COMT" => "comt",
        b"NAME" => "name",
        b"AUTH" => "auth",
        b"(c) " => "copyright",
        b"ANNO" => "anno",
        b"FVER" => "fver",
        _ => "chunk",
    }
}

fn decode_comm(
    payload: &[u8],
    offset_start: u64,
    offset_end: u64,
    form_type: &[u8; 4],
) -> Result<AiffComm, String> {
    // AIFF COMM layout (18 bytes minimum):
    //   u16 num_channels
    //   u32 num_sample_frames
    //   u16 sample_size
    //   80-bit IEEE-754 extended precision sample_rate
    // AIFC extends this with a u32 compression_type plus a Pascal string
    // compression_name; this decoder reads only as much as the chunk
    // size permits so short COMM chunks do not overrun.
    if payload.len() < 18 {
        return Err(format!(
            "COMM chunk length {} below required 18 bytes",
            payload.len()
        ));
    }

    let num_channels = u16::from_be_bytes([payload[0], payload[1]]);
    let num_sample_frames = u32::from_be_bytes([payload[2], payload[3], payload[4], payload[5]]);
    let sample_size = u16::from_be_bytes([payload[6], payload[7]]);
    let sample_rate = decode_extended_f64(&payload[8..18]);

    let compression_type = if form_type == FORM_TYPE_AIFC && payload.len() >= 22 {
        Some([payload[18], payload[19], payload[20], payload[21]])
    } else {
        None
    };

    Ok(AiffComm {
        num_channels,
        num_sample_frames,
        sample_size,
        sample_rate,
        compression_type,
        offset_start,
        offset_end,
    })
}

/// Decode a 10-byte IEEE-754 80-bit extended-precision float (big
/// endian, as used by the AIFF COMM sample-rate field) into `f64`.
///
/// Returns `0.0` for zero-exponent values that are otherwise zero,
/// `f64::INFINITY` / `f64::NAN` for the corresponding special cases,
/// and a best-effort converted value for normal / denormal numbers.
fn decode_extended_f64(bytes: &[u8]) -> f64 {
    debug_assert_eq!(bytes.len(), 10);
    let sign = (bytes[0] & 0x80) != 0;
    let exponent = ((bytes[0] as u16 & 0x7F) << 8) | bytes[1] as u16;
    let mantissa = u64::from_be_bytes([
        bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7], bytes[8], bytes[9],
    ]);

    let magnitude = if exponent == 0 && mantissa == 0 {
        0.0f64
    } else if exponent == 0x7FFF {
        if mantissa == 0 {
            f64::INFINITY
        } else {
            f64::NAN
        }
    } else {
        // The 80-bit layout has an explicit integer bit at the top of
        // the mantissa. Convert: value = mantissa * 2^(exponent - 16383 - 63).
        let exp_unbiased = exponent as i32 - 16383 - 63;
        (mantissa as f64) * (2.0f64).powi(exp_unbiased)
    };

    if sign { -magnitude } else { magnitude }
}

/// Encode an `f64` sample rate into a 10-byte big-endian IEEE-754
/// extended-precision value. Used by tests and fixture generators; the
/// production parser only needs decode.
pub fn encode_extended_f64(value: f64) -> [u8; 10] {
    let mut out = [0u8; 10];
    if value == 0.0 {
        return out;
    }
    let sign_bit = if value.is_sign_negative() {
        0x8000u16
    } else {
        0
    };
    let magnitude = value.abs();
    if magnitude.is_infinite() {
        let exp = 0x7FFFu16 | sign_bit;
        out[0] = (exp >> 8) as u8;
        out[1] = exp as u8;
        return out;
    }
    if magnitude.is_nan() {
        let exp = 0x7FFFu16 | sign_bit;
        out[0] = (exp >> 8) as u8;
        out[1] = exp as u8;
        // Set a non-zero mantissa bit to signal NaN.
        out[2] = 0x40;
        return out;
    }
    // Decompose magnitude into mantissa (53-bit significand) and
    // exponent via frexp-equivalent math, then renormalize to the
    // 80-bit layout with an explicit integer bit.
    let (frac, exp2) = frexp(magnitude);
    // frac is in [0.5, 1.0); shift left 63 bits to get a u64 with the
    // integer bit set.
    let scaled = frac * (1u128 << 63) as f64 * 2.0;
    let mantissa = scaled as u64;
    let exponent = (exp2 + 16383 - 1) as u16;
    let biased = (exponent & 0x7FFF) | sign_bit;
    out[0] = (biased >> 8) as u8;
    out[1] = biased as u8;
    out[2..10].copy_from_slice(&mantissa.to_be_bytes());
    out
}

fn frexp(value: f64) -> (f64, i32) {
    if value == 0.0 || !value.is_finite() {
        return (value, 0);
    }
    let bits = value.to_bits();
    let exp = ((bits >> 52) & 0x7FF) as i32;
    if exp == 0 {
        // Subnormal: normalize by scaling.
        let (frac, shift) = frexp(value * (1u64 << 54) as f64);
        return (frac, shift - 54);
    }
    let mantissa_bits = (bits & ((1u64 << 52) - 1)) | (1u64 << 52);
    let frac = (mantissa_bits as f64) / (1u64 << 53) as f64;
    let sign = if (bits >> 63) != 0 { -1.0 } else { 1.0 };
    (sign * frac, exp - 1022)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u16_be(v: u16) -> [u8; 2] {
        v.to_be_bytes()
    }
    fn u32_be(v: u32) -> [u8; 4] {
        v.to_be_bytes()
    }

    fn comm_body_aiff(
        num_channels: u16,
        num_sample_frames: u32,
        sample_size: u16,
        sample_rate: f64,
    ) -> Vec<u8> {
        let mut body = Vec::with_capacity(18);
        body.extend_from_slice(&u16_be(num_channels));
        body.extend_from_slice(&u32_be(num_sample_frames));
        body.extend_from_slice(&u16_be(sample_size));
        body.extend_from_slice(&encode_extended_f64(sample_rate));
        body
    }

    fn comm_body_aifc(
        num_channels: u16,
        num_sample_frames: u32,
        sample_size: u16,
        sample_rate: f64,
        compression: &[u8; 4],
    ) -> Vec<u8> {
        let mut body = comm_body_aiff(num_channels, num_sample_frames, sample_size, sample_rate);
        body.extend_from_slice(compression);
        // Pascal string compression name: length byte + text; pad to
        // even total length.
        let name = b"not compressed";
        body.push(name.len() as u8);
        body.extend_from_slice(name);
        if body.len() % 2 == 1 {
            body.push(0);
        }
        body
    }

    fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + body.len() + 1);
        out.extend_from_slice(id);
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        out.extend_from_slice(body);
        if body.len() % 2 == 1 {
            out.push(0);
        }
        out
    }

    fn assemble(form_type: &[u8; 4], chunks: &[Vec<u8>]) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(form_type);
        for c in chunks {
            body.extend_from_slice(c);
        }
        let mut out = Vec::with_capacity(8 + body.len());
        out.extend_from_slice(FORM_MAGIC);
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn parses_minimal_aiff() {
        let comm = chunk(CHUNK_ID_COMM, &comm_body_aiff(2, 44100, 16, 44100.0));
        let ssnd = chunk(CHUNK_ID_SSND, &[0u8; 8]); // 8-byte SSND header-only stub
        let bytes = assemble(FORM_TYPE_AIFF, &[comm, ssnd]);
        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert_eq!(&parsed.form_type, FORM_TYPE_AIFF);
        let info = parsed.comm.expect("comm decoded");
        assert_eq!(info.num_channels, 2);
        assert_eq!(info.sample_size, 16);
        assert_eq!(info.num_sample_frames, 44100);
        assert!((info.sample_rate - 44100.0).abs() < 0.001);
        assert_eq!(parsed.duration_seconds, Some(1.0));
        assert_eq!(parsed.bit_depth, Some(16));
        assert!(parsed.ssnd_offset.is_some());
        assert!(
            parsed
                .nodes
                .iter()
                .any(|n| n.label == "comm" && n.kind == "chunk")
        );
    }

    #[test]
    fn parses_aifc_form_type() {
        let comm = chunk(
            CHUNK_ID_COMM,
            &comm_body_aifc(1, 22050, 8, 22050.0, b"NONE"),
        );
        let bytes = assemble(FORM_TYPE_AIFC, &[comm]);
        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert_eq!(&parsed.form_type, FORM_TYPE_AIFC);
        let info = parsed.comm.expect("comm decoded");
        assert_eq!(info.compression_type, Some(*b"NONE"));
        assert!(
            parsed
                .nodes
                .first()
                .map(|n| n.label == "aifc")
                .unwrap_or(false)
        );
    }

    #[test]
    fn emits_warning_when_comm_missing() {
        let ssnd = chunk(CHUNK_ID_SSND, &[0u8; 4]);
        let bytes = assemble(FORM_TYPE_AIFF, &[ssnd]);
        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert!(parsed.comm.is_none());
        assert!(
            parsed
                .issues
                .iter()
                .any(|iss| iss.code == "aiff_comm_missing")
        );
    }

    #[test]
    fn emits_warning_when_chunk_length_exceeds_bounds() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(FORM_MAGIC);
        bytes.extend_from_slice(&(4u32 + 8 + 4).to_be_bytes()); // FORM size
        bytes.extend_from_slice(FORM_TYPE_AIFF);
        bytes.extend_from_slice(CHUNK_ID_COMM);
        bytes.extend_from_slice(&100u32.to_be_bytes()); // bogus length
        bytes.extend_from_slice(&[0u8; 4]);
        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert!(
            parsed
                .issues
                .iter()
                .any(|iss| iss.code == "aiff_chunk_length_invalid")
        );
    }

    #[test]
    fn records_id3_chunk_with_pad_stripped() {
        let id3_payload = b"ID3\x03\x00\x00\x00\x00\x00\x03TIT2\x00\x00\x00\x04\x00\x00\x03abc";
        let comm = chunk(CHUNK_ID_COMM, &comm_body_aiff(2, 100, 16, 44100.0));
        let id3 = chunk(CHUNK_ID_ID3, id3_payload);
        let bytes = assemble(FORM_TYPE_AIFF, &[comm, id3]);
        let parsed = parse_bytes(&bytes, 0).unwrap();
        let payload = parsed.id3v2_payload(&bytes).expect("id3 payload present");
        assert_eq!(payload.len(), id3_payload.len());
        assert_eq!(&payload[..3], b"ID3");
    }

    #[test]
    fn flags_non_standard_form_type() {
        let comm = chunk(CHUNK_ID_COMM, &comm_body_aiff(2, 100, 16, 44100.0));
        let bytes = assemble(b"XXXX", &[comm]);
        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert!(
            parsed
                .issues
                .iter()
                .any(|iss| iss.code == "aiff_non_standard_form"
                    && matches!(iss.severity, Severity::Info))
        );
    }

    #[test]
    fn rejects_non_aiff_magic() {
        let bytes = [0u8; 16];
        assert!(parse_bytes(&bytes, 0).is_err());
    }

    #[test]
    fn decode_extended_f64_round_trip_common_rates() {
        for rate in [22050.0, 44100.0, 48000.0, 96000.0, 192000.0] {
            let encoded = encode_extended_f64(rate);
            let decoded = decode_extended_f64(&encoded);
            assert!(
                (decoded - rate).abs() < 0.001,
                "rate {rate} decoded as {decoded}"
            );
        }
    }

    #[test]
    fn decode_extended_f64_zero_and_specials() {
        assert_eq!(decode_extended_f64(&[0u8; 10]), 0.0);
        let inf_bytes = [0x7F, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0];
        assert!(decode_extended_f64(&inf_bytes).is_infinite());
        let nan_bytes = [0x7F, 0xFF, 0x40, 0, 0, 0, 0, 0, 0, 0];
        assert!(decode_extended_f64(&nan_bytes).is_nan());
    }
}
