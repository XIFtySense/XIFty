//! OGG container framing.
//!
//! Walks the `OggS` page chain defined by RFC 3533, reassembles packets
//! for the first logical bitstream, identifies the codec from the first
//! packet (Vorbis or Opus), decodes the identification header, locates
//! the Vorbis-comment payload inside the second packet, and tracks the
//! granule position of the last page (used by callers to compute
//! duration). Metadata interpretation lives elsewhere — this crate owns
//! the stream framing only, mirroring the FLAC/AIFF container boundary.
//!
//! ## Scope (phase 2)
//!
//! - Single logical bitstream: the parser locks on to the first serial
//!   number it sees. Additional logical streams surface an
//!   `ogg_multiplexed_streams_unsupported` info issue and are ignored.
//! - Two codecs: Vorbis (identified by the `\x01vorbis` prefix on the
//!   first packet) and Opus (identified by the `OpusHead` prefix).
//! - Tolerant to CRC — the phase-2 parser does not validate per-page
//!   CRCs; corrupted frames are surfaced as structural issues instead.
//!
//! ## Opus sample-rate convention
//!
//! Opus always decodes to 48 kHz (RFC 7845 §4). The ident header's
//! `input_sample_rate` is metadata about the source encoder's rate and
//! is exposed to the caller separately from the decoded rate so the
//! normalized `audio.sample_rate` stays semantically consistent.

use xifty_core::{ContainerNode, Issue, Severity, XiftyError};
use xifty_source::{Cursor, SourceBytes};

pub const OGG_MAGIC: &[u8; 4] = b"OggS";

/// Codec identified from the first packet of the primary logical bitstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OggCodec {
    Vorbis,
    Opus,
}

#[derive(Debug, Clone)]
pub struct VorbisIdent {
    pub version: u32,
    pub channels: u8,
    pub sample_rate_hz: u32,
    pub offset_start: u64,
    pub offset_end: u64,
}

#[derive(Debug, Clone)]
pub struct OpusIdent {
    pub version: u8,
    pub channels: u8,
    pub pre_skip: u16,
    pub input_sample_rate: u32,
    pub offset_start: u64,
    pub offset_end: u64,
}

/// View of a Vorbis-comment payload located inside the second packet of
/// an OGG logical stream (`\x03vorbis…` for Vorbis, `OpusTags…` for
/// Opus). Callers feed these bounds to `xifty-meta-vorbis-comment`.
#[derive(Debug, Clone)]
pub struct VorbisCommentBlock {
    pub offset_start: u64,
    pub offset_end: u64,
    pub data_offset: u64,
    pub data_length: u32,
}

#[derive(Debug, Clone)]
pub struct OggContainer {
    pub nodes: Vec<ContainerNode>,
    pub issues: Vec<Issue>,
    pub first_codec: Option<OggCodec>,
    pub vorbis_ident: Option<VorbisIdent>,
    pub opus_ident: Option<OpusIdent>,
    pub vorbis_comment: Option<VorbisCommentBlock>,
    pub granule_last: Option<i64>,
    pub serial: Option<u32>,
}

pub fn parse(source: &SourceBytes) -> Result<OggContainer, XiftyError> {
    parse_bytes(source.bytes(), 0)
}

/// Parsed bytes of a single OGG page header + payload span.
struct ParsedPage {
    header_type: u8,
    granule_position: i64,
    serial: u32,
    segment_lengths: Vec<u8>,
    payload_offset: usize,
    page_end: usize,
}

pub fn parse_bytes(bytes: &[u8], base_offset: u64) -> Result<OggContainer, XiftyError> {
    let cursor = Cursor::new(bytes, base_offset);
    if cursor.len() < 4 || cursor.slice(0, 4)? != OGG_MAGIC {
        return Err(XiftyError::Parse {
            message: "not an ogg".into(),
        });
    }

    let mut nodes = vec![ContainerNode {
        kind: "container".into(),
        label: "ogg".into(),
        offset_start: base_offset,
        offset_end: base_offset + bytes.len() as u64,
        parent_label: None,
    }];
    let mut issues = Vec::new();
    let mut serial: Option<u32> = None;
    let mut granule_last: Option<i64> = None;
    let mut saw_other_stream = false;

    // Packet assembly state for the primary stream.
    let mut packet_buffers: Vec<Vec<u8>> = Vec::new(); // complete packets
    let mut current_packet: Vec<u8> = Vec::new();
    let mut current_packet_start: Option<u64> = None; // absolute start of the in-flight packet
    let mut packet_starts: Vec<u64> = Vec::new();
    let mut packet_ends: Vec<u64> = Vec::new();

    let mut offset = 0usize;
    while offset < cursor.len() {
        let page = match read_page(&cursor, offset) {
            Ok(page) => page,
            Err(issue) => {
                issues.push(issue);
                break;
            }
        };

        nodes.push(ContainerNode {
            kind: "page".into(),
            label: "page".into(),
            offset_start: cursor.absolute_offset(offset),
            offset_end: cursor.absolute_offset(page.page_end),
            parent_label: Some("ogg".into()),
        });

        let this_serial = page.serial;
        match serial {
            None => serial = Some(this_serial),
            Some(locked) if locked != this_serial => {
                if !saw_other_stream {
                    issues.push(Issue {
                        severity: Severity::Info,
                        code: "ogg_multiplexed_streams_unsupported".into(),
                        message: "additional logical bitstream ignored; phase 2 tracks only the \
                                  first serial observed"
                            .into(),
                        offset: Some(cursor.absolute_offset(offset)),
                        context: Some(format!("serial={this_serial}")),
                    });
                    saw_other_stream = true;
                }
                offset = page.page_end;
                continue;
            }
            _ => {}
        }

        // Reassemble packets from this page's segments.
        let mut seg_cursor = page.payload_offset;
        let mut is_first_segment_of_page = true;
        let continued = (page.header_type & 0x01) != 0;
        if !continued && !current_packet.is_empty() {
            // A new packet starts without the continuation flag — flush
            // any pending packet (this normally should not happen in a
            // well-formed stream, but keep us tolerant).
            flush_packet(
                &mut packet_buffers,
                &mut current_packet,
                &mut current_packet_start,
                &mut packet_starts,
                &mut packet_ends,
                cursor.absolute_offset(page.payload_offset),
            );
        }
        for &seg_len in &page.segment_lengths {
            if current_packet_start.is_none() {
                current_packet_start = Some(cursor.absolute_offset(seg_cursor));
            }
            let seg_start = seg_cursor;
            let seg_end = seg_cursor + seg_len as usize;
            let slice = cursor.slice(seg_start, seg_len as usize)?;
            current_packet.extend_from_slice(slice);
            seg_cursor = seg_end;

            if seg_len < 255 {
                // End of packet.
                let end_abs = cursor.absolute_offset(seg_end);
                flush_packet(
                    &mut packet_buffers,
                    &mut current_packet,
                    &mut current_packet_start,
                    &mut packet_starts,
                    &mut packet_ends,
                    end_abs,
                );
            }
            is_first_segment_of_page = false;
        }
        let _ = is_first_segment_of_page; // suppress unused warning in some configs

        // Last page granule tracking (primary stream only).
        granule_last = Some(page.granule_position);

        offset = page.page_end;
    }

    // Decode ident + comment packets if present.
    let mut first_codec = None;
    let mut vorbis_ident = None;
    let mut opus_ident = None;
    let mut vorbis_comment: Option<VorbisCommentBlock> = None;

    if let Some(packet) = packet_buffers.first() {
        let packet_start = packet_starts[0];
        let packet_end = packet_ends[0];
        if packet.starts_with(b"\x01vorbis") {
            first_codec = Some(OggCodec::Vorbis);
            match decode_vorbis_ident(packet, packet_start, packet_end) {
                Ok(ident) => vorbis_ident = Some(ident),
                Err(msg) => issues.push(Issue {
                    severity: Severity::Warning,
                    code: "ogg_vorbis_ident_invalid".into(),
                    message: msg,
                    offset: Some(packet_start),
                    context: Some("vorbis_ident".into()),
                }),
            }
        } else if packet.starts_with(b"OpusHead") {
            first_codec = Some(OggCodec::Opus);
            match decode_opus_ident(packet, packet_start, packet_end) {
                Ok(ident) => opus_ident = Some(ident),
                Err(msg) => issues.push(Issue {
                    severity: Severity::Warning,
                    code: "ogg_opus_ident_invalid".into(),
                    message: msg,
                    offset: Some(packet_start),
                    context: Some("opus_head".into()),
                }),
            }
        } else {
            issues.push(Issue {
                severity: Severity::Warning,
                code: "ogg_codec_unrecognized".into(),
                message: "first packet did not carry a recognised codec signature (vorbis/opus)"
                    .into(),
                offset: Some(packet_start),
                context: None,
            });
        }
    } else {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "ogg_no_primary_packet".into(),
            message: "no complete packet reassembled from primary logical bitstream".into(),
            offset: Some(base_offset),
            context: None,
        });
    }

    if let (Some(codec), Some(packet)) = (first_codec, packet_buffers.get(1)) {
        let packet_start = packet_starts[1];
        let packet_end = packet_ends[1];
        match codec {
            OggCodec::Vorbis => {
                // Vorbis comment packet: prefix \x03vorbis (7 bytes), then
                // the Vorbis-comment payload (vendor + user list). The
                // Vorbis spec appends a framing bit at the end, which the
                // xifty-meta-vorbis-comment decoder ignores.
                if packet.starts_with(b"\x03vorbis") {
                    let prefix = b"\x03vorbis".len();
                    let data_offset = packet_start + prefix as u64;
                    let data_length = (packet.len() - prefix) as u32;
                    vorbis_comment = Some(VorbisCommentBlock {
                        offset_start: packet_start,
                        offset_end: packet_end,
                        data_offset,
                        data_length,
                    });
                } else {
                    issues.push(Issue {
                        severity: Severity::Warning,
                        code: "ogg_vorbis_comment_prefix_missing".into(),
                        message: "second Vorbis packet did not start with \\x03vorbis".into(),
                        offset: Some(packet_start),
                        context: Some("vorbis_comment".into()),
                    });
                }
            }
            OggCodec::Opus => {
                if packet.starts_with(b"OpusTags") {
                    let prefix = b"OpusTags".len();
                    let data_offset = packet_start + prefix as u64;
                    let data_length = (packet.len() - prefix) as u32;
                    vorbis_comment = Some(VorbisCommentBlock {
                        offset_start: packet_start,
                        offset_end: packet_end,
                        data_offset,
                        data_length,
                    });
                } else {
                    issues.push(Issue {
                        severity: Severity::Warning,
                        code: "ogg_opus_tags_prefix_missing".into(),
                        message: "second Opus packet did not start with OpusTags".into(),
                        offset: Some(packet_start),
                        context: Some("opus_tags".into()),
                    });
                }
            }
        }
    }

    if granule_last.is_none() || matches!(granule_last, Some(value) if value <= 0) {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "ogg_granule_missing".into(),
            message: "no positive granule position observed; duration cannot be derived".into(),
            offset: None,
            context: None,
        });
    }

    Ok(OggContainer {
        nodes,
        issues,
        first_codec,
        vorbis_ident,
        opus_ident,
        vorbis_comment,
        granule_last,
        serial,
    })
}

fn flush_packet(
    packets: &mut Vec<Vec<u8>>,
    current: &mut Vec<u8>,
    start: &mut Option<u64>,
    starts: &mut Vec<u64>,
    ends: &mut Vec<u64>,
    end_abs: u64,
) {
    if current.is_empty() && start.is_none() {
        return;
    }
    let s = start.take().unwrap_or(end_abs);
    let buf = std::mem::take(current);
    starts.push(s);
    ends.push(end_abs);
    packets.push(buf);
}

fn read_page(cursor: &Cursor<'_>, offset: usize) -> Result<ParsedPage, Issue> {
    const HEADER_FIXED: usize = 27;
    if offset + HEADER_FIXED > cursor.len() {
        return Err(Issue {
            severity: Severity::Warning,
            code: "ogg_page_truncated".into(),
            message: "truncated ogg page header".into(),
            offset: Some(cursor.absolute_offset(offset)),
            context: None,
        });
    }
    let header = cursor.slice(offset, HEADER_FIXED).map_err(|e| Issue {
        severity: Severity::Warning,
        code: "ogg_page_read_failed".into(),
        message: format!("{e}"),
        offset: Some(cursor.absolute_offset(offset)),
        context: None,
    })?;
    if &header[0..4] != OGG_MAGIC {
        return Err(Issue {
            severity: Severity::Warning,
            code: "ogg_page_magic_mismatch".into(),
            message: "expected OggS magic at page start".into(),
            offset: Some(cursor.absolute_offset(offset)),
            context: None,
        });
    }
    let version = header[4];
    if version != 0 {
        return Err(Issue {
            severity: Severity::Warning,
            code: "ogg_page_version_unsupported".into(),
            message: format!("unsupported ogg page version {version}"),
            offset: Some(cursor.absolute_offset(offset)),
            context: None,
        });
    }
    let header_type = header[5];
    let granule_position = i64::from_le_bytes([
        header[6], header[7], header[8], header[9], header[10], header[11], header[12], header[13],
    ]);
    let serial = u32::from_le_bytes([header[14], header[15], header[16], header[17]]);
    // header[18..22] = page sequence number; header[22..26] = crc; header[26] = segment count.
    let page_segments = header[26] as usize;
    let table_offset = offset + HEADER_FIXED;
    let table_end = table_offset + page_segments;
    if table_end > cursor.len() {
        return Err(Issue {
            severity: Severity::Warning,
            code: "ogg_page_truncated".into(),
            message: "truncated ogg segment table".into(),
            offset: Some(cursor.absolute_offset(offset)),
            context: None,
        });
    }
    let table = cursor
        .slice(table_offset, page_segments)
        .map_err(|e| Issue {
            severity: Severity::Warning,
            code: "ogg_page_read_failed".into(),
            message: format!("{e}"),
            offset: Some(cursor.absolute_offset(offset)),
            context: None,
        })?;
    let segment_lengths = table.to_vec();
    let payload_offset = table_end;
    let payload_length: usize = segment_lengths.iter().map(|b| *b as usize).sum();
    let page_end = payload_offset + payload_length;
    if page_end > cursor.len() {
        return Err(Issue {
            severity: Severity::Warning,
            code: "ogg_page_truncated".into(),
            message: "ogg page payload extends past available bytes".into(),
            offset: Some(cursor.absolute_offset(offset)),
            context: Some(format!("payload_length={payload_length}")),
        });
    }
    Ok(ParsedPage {
        header_type,
        granule_position,
        serial,
        segment_lengths,
        payload_offset,
        page_end,
    })
}

fn decode_vorbis_ident(
    packet: &[u8],
    offset_start: u64,
    offset_end: u64,
) -> Result<VorbisIdent, String> {
    // Layout (Vorbis I §4.2.2):
    //   "\x01vorbis" (7) | vorbis_version u32 LE | channels u8 |
    //   sample_rate u32 LE | bitrate_max i32 | bitrate_nom i32 |
    //   bitrate_min i32 | blocksize u8 | framing u8
    if packet.len() < 7 + 4 + 1 + 4 {
        return Err(format!(
            "vorbis ident header too short ({} bytes)",
            packet.len()
        ));
    }
    let cursor = &packet[7..];
    let version = u32::from_le_bytes([cursor[0], cursor[1], cursor[2], cursor[3]]);
    let channels = cursor[4];
    let sample_rate_hz = u32::from_le_bytes([cursor[5], cursor[6], cursor[7], cursor[8]]);
    Ok(VorbisIdent {
        version,
        channels,
        sample_rate_hz,
        offset_start,
        offset_end,
    })
}

fn decode_opus_ident(
    packet: &[u8],
    offset_start: u64,
    offset_end: u64,
) -> Result<OpusIdent, String> {
    // Layout (RFC 7845 §5.1):
    //   "OpusHead" (8) | version u8 | channel_count u8 | pre_skip u16 LE |
    //   input_sample_rate u32 LE | output_gain i16 | mapping_family u8
    if packet.len() < 8 + 1 + 1 + 2 + 4 {
        return Err(format!(
            "opus ident header too short ({} bytes)",
            packet.len()
        ));
    }
    let cursor = &packet[8..];
    let version = cursor[0];
    let channels = cursor[1];
    let pre_skip = u16::from_le_bytes([cursor[2], cursor[3]]);
    let input_sample_rate = u32::from_le_bytes([cursor[4], cursor[5], cursor[6], cursor[7]]);
    Ok(OpusIdent {
        version,
        channels,
        pre_skip,
        input_sample_rate,
        offset_start,
        offset_end,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERIAL: u32 = 0x12345678;

    fn build_page(
        header_type: u8,
        granule_position: i64,
        serial: u32,
        page_sequence: u32,
        packets: &[&[u8]],
    ) -> Vec<u8> {
        // Turn each packet into a list of 255-capped segments.
        let mut segment_table: Vec<u8> = Vec::new();
        let mut payload: Vec<u8> = Vec::new();
        for packet in packets {
            let mut remaining = packet.len();
            let mut offset = 0usize;
            loop {
                let take = remaining.min(255);
                segment_table.push(take as u8);
                payload.extend_from_slice(&packet[offset..offset + take]);
                offset += take;
                if remaining < 255 {
                    break;
                }
                remaining -= 255;
                if remaining == 0 {
                    // Exact multiple of 255 — append a zero-length segment to terminate.
                    segment_table.push(0u8);
                    break;
                }
            }
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"OggS");
        out.push(0); // version
        out.push(header_type);
        out.extend_from_slice(&granule_position.to_le_bytes());
        out.extend_from_slice(&serial.to_le_bytes());
        out.extend_from_slice(&page_sequence.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // crc (tolerated)
        out.push(segment_table.len() as u8);
        out.extend_from_slice(&segment_table);
        out.extend_from_slice(&payload);
        out
    }

    fn vorbis_ident_packet(channels: u8, sample_rate: u32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"\x01vorbis");
        out.extend_from_slice(&0u32.to_le_bytes()); // version
        out.push(channels);
        out.extend_from_slice(&sample_rate.to_le_bytes());
        out.extend_from_slice(&0i32.to_le_bytes()); // bitrate max
        out.extend_from_slice(&0i32.to_le_bytes()); // bitrate nom
        out.extend_from_slice(&0i32.to_le_bytes()); // bitrate min
        out.push(0xB8); // blocksize packing (nominal)
        out.push(0x01); // framing bit
        out
    }

    fn vorbis_comment_packet(vendor: &str, comments: &[&str]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"\x03vorbis");
        out.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        out.extend_from_slice(vendor.as_bytes());
        out.extend_from_slice(&(comments.len() as u32).to_le_bytes());
        for c in comments {
            out.extend_from_slice(&(c.len() as u32).to_le_bytes());
            out.extend_from_slice(c.as_bytes());
        }
        out.push(0x01); // framing bit
        out
    }

    fn opus_head_packet(channels: u8, pre_skip: u16, input_sample_rate: u32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"OpusHead");
        out.push(1); // version
        out.push(channels);
        out.extend_from_slice(&pre_skip.to_le_bytes());
        out.extend_from_slice(&input_sample_rate.to_le_bytes());
        out.extend_from_slice(&0i16.to_le_bytes()); // output gain
        out.push(0); // mapping family
        out
    }

    fn opus_tags_packet(vendor: &str, comments: &[&str]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"OpusTags");
        out.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        out.extend_from_slice(vendor.as_bytes());
        out.extend_from_slice(&(comments.len() as u32).to_le_bytes());
        for c in comments {
            out.extend_from_slice(&(c.len() as u32).to_le_bytes());
            out.extend_from_slice(c.as_bytes());
        }
        out
    }

    #[test]
    fn parses_vorbis_ident_and_comment_pages() {
        let ident = vorbis_ident_packet(2, 44100);
        let comment = vorbis_comment_packet("xifty-test", &["TITLE=Song"]);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&build_page(0x02, 0, SERIAL, 0, &[&ident]));
        bytes.extend_from_slice(&build_page(0x00, 0, SERIAL, 1, &[&comment]));
        bytes.extend_from_slice(&build_page(0x04, 44100, SERIAL, 2, &[&[0u8; 4]]));
        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert_eq!(parsed.first_codec, Some(OggCodec::Vorbis));
        let ident = parsed.vorbis_ident.expect("vorbis ident");
        assert_eq!(ident.channels, 2);
        assert_eq!(ident.sample_rate_hz, 44100);
        let vc = parsed.vorbis_comment.expect("vorbis comment block");
        assert!(vc.data_length > 0);
        assert_eq!(parsed.granule_last, Some(44100));
        assert_eq!(parsed.serial, Some(SERIAL));
    }

    #[test]
    fn parses_opus_head_and_tags_pages() {
        let ident = opus_head_packet(2, 312, 48000);
        let tags = opus_tags_packet("xifty-test", &["ARTIST=Kai"]);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&build_page(0x02, 0, SERIAL, 0, &[&ident]));
        bytes.extend_from_slice(&build_page(0x00, 0, SERIAL, 1, &[&tags]));
        bytes.extend_from_slice(&build_page(0x04, 48_000, SERIAL, 2, &[&[0u8; 4]]));
        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert_eq!(parsed.first_codec, Some(OggCodec::Opus));
        let ident = parsed.opus_ident.expect("opus ident");
        assert_eq!(ident.channels, 2);
        assert_eq!(ident.input_sample_rate, 48_000);
        assert_eq!(ident.pre_skip, 312);
        let vc = parsed.vorbis_comment.expect("opus tags block");
        assert!(vc.data_length > 0);
        assert_eq!(parsed.granule_last, Some(48_000));
    }

    #[test]
    fn reassembles_packet_across_255_segment_runs() {
        // Build a packet larger than 255 bytes so the segment table must
        // encode two segments (255 + remainder) — exercise reassembly.
        let mut large_packet = Vec::with_capacity(600);
        large_packet.extend_from_slice(b"\x01vorbis");
        large_packet.extend_from_slice(&0u32.to_le_bytes()); // version
        large_packet.push(1); // channels
        large_packet.extend_from_slice(&22050u32.to_le_bytes()); // sample rate
        large_packet.extend_from_slice(&0i32.to_le_bytes());
        large_packet.extend_from_slice(&0i32.to_le_bytes());
        large_packet.extend_from_slice(&0i32.to_le_bytes());
        large_packet.push(0xB8);
        large_packet.push(0x01);
        // Pad with arbitrary bytes so total packet size > 255.
        large_packet.resize(300, 0xAA);
        let bytes = build_page(0x02, 0, SERIAL, 0, &[&large_packet]);
        let parsed = parse_bytes(&bytes, 0).unwrap();
        let ident = parsed.vorbis_ident.expect("vorbis ident reassembled");
        assert_eq!(ident.channels, 1);
        assert_eq!(ident.sample_rate_hz, 22050);
    }

    #[test]
    fn surfaces_truncated_segment_table() {
        // Build a valid page header claiming 5 segments but provide zero
        // segment-table bytes.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"OggS");
        bytes.push(0);
        bytes.push(0x02);
        bytes.extend_from_slice(&0i64.to_le_bytes());
        bytes.extend_from_slice(&SERIAL.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.push(5);
        // Missing 5 segment-length bytes.
        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert!(
            parsed
                .issues
                .iter()
                .any(|iss| iss.code == "ogg_page_truncated")
        );
    }

    #[test]
    fn rejects_non_ogg_magic() {
        let bytes = [0u8; 8];
        assert!(parse_bytes(&bytes, 0).is_err());
    }

    #[test]
    fn ignores_additional_logical_streams() {
        let ident = vorbis_ident_packet(2, 44100);
        let comment = vorbis_comment_packet("v", &[]);
        let other = vorbis_ident_packet(1, 22050);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&build_page(0x02, 0, SERIAL, 0, &[&ident]));
        bytes.extend_from_slice(&build_page(0x00, 0, SERIAL, 1, &[&comment]));
        bytes.extend_from_slice(&build_page(0x02, 0, 0xDEADBEEF, 0, &[&other]));
        bytes.extend_from_slice(&build_page(0x04, 44100, SERIAL, 2, &[&[0u8; 2]]));
        let parsed = parse_bytes(&bytes, 0).unwrap();
        let ident = parsed.vorbis_ident.expect("primary ident");
        assert_eq!(ident.channels, 2);
        assert!(
            parsed
                .issues
                .iter()
                .any(|iss| iss.code == "ogg_multiplexed_streams_unsupported")
        );
    }
}
