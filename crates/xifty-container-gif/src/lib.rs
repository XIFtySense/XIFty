use xifty_core::{ContainerNode, Issue, Severity, XiftyError, issue};
use xifty_source::{Cursor, Endian, SourceBytes};

/// Recognised GIF block kinds. Image and Graphic Control are represented
/// explicitly because they drive frame counting and animation duration; the
/// generic `Extension` arm carries application/comment/plain-text payloads.
#[derive(Debug, Clone)]
pub enum GifBlockKind {
    ImageDescriptor,
    GraphicControlExtension,
    ApplicationExtension { identifier: [u8; 11] },
    CommentExtension,
    PlainTextExtension,
    Trailer,
}

#[derive(Debug, Clone)]
pub struct GifBlock {
    pub kind: GifBlockKind,
    pub offset_start: u64,
    pub offset_end: u64,
}

/// Application Extension payload assembled from sub-blocks (length bytes
/// stripped). Used to surface XMP packets to the meta-xmp decoder.
#[derive(Debug, Clone)]
pub struct GifAppExtension {
    pub identifier: [u8; 11],
    /// Concatenated sub-block bytes (length prefixes removed).
    pub payload: Vec<u8>,
    pub offset_start: u64,
    pub offset_end: u64,
}

#[derive(Debug, Clone)]
pub struct GifContainer {
    pub nodes: Vec<ContainerNode>,
    pub blocks: Vec<GifBlock>,
    pub width: u16,
    pub height: u16,
    pub background_color_index: u8,
    /// Number of bytes consumed by the global colour table (0 when absent).
    pub global_color_table_size: u32,
    pub frame_count: u32,
    /// Sum of graphic-control delays across the file, in centiseconds.
    /// Multiply by 0.01 to get seconds.
    pub animation_duration_centiseconds: u64,
    /// Netscape `NETSCAPE2.0` loop count if present (0 = infinite).
    pub loop_count: Option<u16>,
    pub app_extensions: Vec<GifAppExtension>,
    pub issues: Vec<Issue>,
}

impl GifContainer {
    /// Iterator over Application Extension payloads matching the Adobe XMP
    /// identifier (`XMP Data` + version byte 0x01). Each item is a (offset_start,
    /// offset_end, payload bytes) tuple where the magic trailer has already been
    /// stripped — caller can hand the bytes straight to `decode_packet`.
    pub fn xmp_payloads(&self) -> impl Iterator<Item = (u64, u64, &[u8])> {
        self.app_extensions
            .iter()
            .filter(|ext| &ext.identifier == b"XMP DataXMP")
            .map(|ext| {
                let payload = strip_xmp_magic_trailer(&ext.payload);
                (ext.offset_start, ext.offset_end, payload)
            })
    }
}

/// Strip the Adobe-mandated 258-byte XMP magic trailer from a GIF Application
/// Extension payload. The trailer starts with `0x01 0xFF 0xFE 0xFD ... 0x00`.
/// If the payload is shorter than 258 bytes or does not end with the expected
/// trailer, the original payload is returned unchanged so a partially-valid
/// XMP packet is still handed downstream rather than dropped silently.
fn strip_xmp_magic_trailer(payload: &[u8]) -> &[u8] {
    if payload.len() < 258 {
        return payload;
    }
    let trailer_start = payload.len() - 258;
    let trailer = &payload[trailer_start..];
    // Trailer pattern: 0x01 0xFF 0xFE ... 0x01 0x00. Verify a few anchor
    // bytes rather than scanning the full sequence: cheaper, and any
    // non-malicious encoder produces the canonical sequence verbatim.
    if trailer[0] == 0x01 && trailer[1] == 0xFF && trailer[257] == 0x00 {
        return &payload[..trailer_start];
    }
    payload
}

pub fn parse(source: &SourceBytes) -> Result<GifContainer, XiftyError> {
    parse_bytes(source.bytes(), 0)
}

pub fn parse_bytes(bytes: &[u8], base_offset: u64) -> Result<GifContainer, XiftyError> {
    let cursor = Cursor::new(bytes, base_offset);
    if cursor.len() < 13 {
        return Err(XiftyError::Parse {
            message: "gif shorter than header + lsd".into(),
        });
    }
    let signature = cursor.slice(0, 6)?;
    if signature != b"GIF87a" && signature != b"GIF89a" {
        return Err(XiftyError::Parse {
            message: "not a gif".into(),
        });
    }

    let mut nodes = vec![ContainerNode {
        kind: "container".into(),
        label: "gif".into(),
        offset_start: base_offset,
        offset_end: base_offset + bytes.len() as u64,
        parent_label: None,
    }];
    let mut blocks = Vec::new();
    let mut app_extensions = Vec::new();
    let mut issues = Vec::new();

    // Logical Screen Descriptor: 7 bytes immediately after the 6-byte signature.
    let width = cursor.read_u16(6, Endian::Little)?;
    let height = cursor.read_u16(8, Endian::Little)?;
    let packed = cursor.read_u8(10)?;
    let background_color_index = cursor.read_u8(11)?;
    nodes.push(ContainerNode {
        kind: "block".into(),
        label: "lsd".into(),
        offset_start: cursor.absolute_offset(6),
        offset_end: cursor.absolute_offset(13),
        parent_label: Some("gif".into()),
    });

    let gct_flag = (packed & 0x80) != 0;
    let gct_size_field = packed & 0x07;
    let global_color_table_size = if gct_flag {
        3u32 * (1u32 << (gct_size_field as u32 + 1))
    } else {
        0
    };

    let mut offset = 13usize;
    if gct_flag {
        let gct_end = offset + global_color_table_size as usize;
        if gct_end > cursor.len() {
            issues.push(Issue {
                severity: Severity::Warning,
                code: "gif_gct_out_of_bounds".into(),
                message: "global color table extends past end of file".into(),
                offset: Some(cursor.absolute_offset(offset)),
                context: None,
            });
            return Ok(GifContainer {
                nodes,
                blocks,
                width,
                height,
                background_color_index,
                global_color_table_size,
                frame_count: 0,
                animation_duration_centiseconds: 0,
                loop_count: None,
                app_extensions,
                issues,
            });
        }
        nodes.push(ContainerNode {
            kind: "block".into(),
            label: "gct".into(),
            offset_start: cursor.absolute_offset(offset),
            offset_end: cursor.absolute_offset(gct_end),
            parent_label: Some("gif".into()),
        });
        offset = gct_end;
    }

    let mut frame_count: u32 = 0;
    let mut animation_duration_centiseconds: u64 = 0;
    let mut loop_count: Option<u16> = None;
    let mut saw_trailer = false;

    while offset < cursor.len() {
        let block_start = offset;
        let intro = cursor.read_u8(offset)?;
        offset += 1;
        match intro {
            0x3B => {
                blocks.push(GifBlock {
                    kind: GifBlockKind::Trailer,
                    offset_start: cursor.absolute_offset(block_start),
                    offset_end: cursor.absolute_offset(offset),
                });
                nodes.push(ContainerNode {
                    kind: "block".into(),
                    label: "trailer".into(),
                    offset_start: cursor.absolute_offset(block_start),
                    offset_end: cursor.absolute_offset(offset),
                    parent_label: Some("gif".into()),
                });
                saw_trailer = true;
                break;
            }
            0x2C => {
                // Image Descriptor: 9 more bytes (we already consumed the 0x2C).
                if offset + 9 > cursor.len() {
                    issues.push(Issue {
                        severity: Severity::Warning,
                        code: "gif_image_descriptor_truncated".into(),
                        message: "image descriptor truncated".into(),
                        offset: Some(cursor.absolute_offset(block_start)),
                        context: None,
                    });
                    break;
                }
                let img_packed = cursor.read_u8(offset + 8)?;
                offset += 9;
                let lct_flag = (img_packed & 0x80) != 0;
                if lct_flag {
                    let lct_size = 3u32 * (1u32 << ((img_packed & 0x07) as u32 + 1));
                    let lct_end = offset + lct_size as usize;
                    if lct_end > cursor.len() {
                        issues.push(Issue {
                            severity: Severity::Warning,
                            code: "gif_lct_out_of_bounds".into(),
                            message: "local color table extends past end of file".into(),
                            offset: Some(cursor.absolute_offset(offset)),
                            context: None,
                        });
                        break;
                    }
                    offset = lct_end;
                }
                // LZW minimum code size byte.
                if offset >= cursor.len() {
                    issues.push(Issue {
                        severity: Severity::Warning,
                        code: "gif_image_data_missing".into(),
                        message: "image descriptor missing image data".into(),
                        offset: Some(cursor.absolute_offset(offset)),
                        context: None,
                    });
                    break;
                }
                offset += 1;
                offset = match skip_sub_blocks(&cursor, offset) {
                    Ok(end) => end,
                    Err(end) => {
                        issues.push(Issue {
                            severity: Severity::Warning,
                            code: "gif_image_sub_blocks_unterminated".into(),
                            message: "image data sub-blocks ended without terminator".into(),
                            offset: Some(cursor.absolute_offset(block_start)),
                            context: None,
                        });
                        end
                    }
                };
                blocks.push(GifBlock {
                    kind: GifBlockKind::ImageDescriptor,
                    offset_start: cursor.absolute_offset(block_start),
                    offset_end: cursor.absolute_offset(offset),
                });
                nodes.push(ContainerNode {
                    kind: "block".into(),
                    label: "image_descriptor".into(),
                    offset_start: cursor.absolute_offset(block_start),
                    offset_end: cursor.absolute_offset(offset),
                    parent_label: Some("gif".into()),
                });
                frame_count += 1;
            }
            0x21 => {
                // Extension introducer.
                if offset >= cursor.len() {
                    issues.push(Issue {
                        severity: Severity::Warning,
                        code: "gif_extension_label_missing".into(),
                        message: "extension introducer with no label".into(),
                        offset: Some(cursor.absolute_offset(block_start)),
                        context: None,
                    });
                    break;
                }
                let label = cursor.read_u8(offset)?;
                offset += 1;
                match label {
                    0xF9 => {
                        // Graphic Control Extension. Block size is always 4,
                        // but we read it and accept whatever the file claims so
                        // a malformed file doesn't crash decoding.
                        let bs_offset = offset;
                        let block_size = cursor.read_u8(bs_offset)? as usize;
                        offset += 1;
                        if offset + block_size > cursor.len() {
                            issues.push(Issue {
                                severity: Severity::Warning,
                                code: "gif_gce_truncated".into(),
                                message: "graphic control extension truncated".into(),
                                offset: Some(cursor.absolute_offset(block_start)),
                                context: None,
                            });
                            break;
                        }
                        if block_size >= 4 {
                            let delay = cursor.read_u16(offset + 1, Endian::Little)?;
                            animation_duration_centiseconds += delay as u64;
                        }
                        offset += block_size;
                        offset = match skip_sub_blocks(&cursor, offset) {
                            Ok(end) => end,
                            Err(end) => {
                                issues.push(Issue {
                                    severity: Severity::Warning,
                                    code: "gif_gce_sub_blocks_unterminated".into(),
                                    message: "graphic control sub-blocks unterminated".into(),
                                    offset: Some(cursor.absolute_offset(block_start)),
                                    context: None,
                                });
                                end
                            }
                        };
                        blocks.push(GifBlock {
                            kind: GifBlockKind::GraphicControlExtension,
                            offset_start: cursor.absolute_offset(block_start),
                            offset_end: cursor.absolute_offset(offset),
                        });
                        nodes.push(ContainerNode {
                            kind: "block".into(),
                            label: "graphic_control_ext".into(),
                            offset_start: cursor.absolute_offset(block_start),
                            offset_end: cursor.absolute_offset(offset),
                            parent_label: Some("gif".into()),
                        });
                    }
                    0xFF => {
                        // Application Extension. Block size byte (always 11),
                        // then the 11-byte identifier+auth, then sub-blocks.
                        let bs_offset = offset;
                        let block_size = cursor.read_u8(bs_offset)? as usize;
                        offset += 1;
                        if block_size != 11 || offset + block_size > cursor.len() {
                            issues.push(Issue {
                                severity: Severity::Warning,
                                code: "gif_app_ext_invalid_block_size".into(),
                                message: format!(
                                    "application extension block size = {block_size}, expected 11"
                                ),
                                offset: Some(cursor.absolute_offset(block_start)),
                                context: None,
                            });
                            break;
                        }
                        let id_bytes = cursor.slice(offset, 11)?;
                        let mut identifier = [0u8; 11];
                        identifier.copy_from_slice(id_bytes);
                        offset += 11;
                        let payload_start = offset;
                        let mut payload = Vec::new();
                        let payload_end = match collect_sub_blocks(&cursor, offset, &mut payload) {
                            Ok(end) => end,
                            Err(end) => {
                                issues.push(Issue {
                                    severity: Severity::Warning,
                                    code: "gif_app_ext_sub_blocks_unterminated".into(),
                                    message: "application extension sub-blocks unterminated".into(),
                                    offset: Some(cursor.absolute_offset(block_start)),
                                    context: None,
                                });
                                end
                            }
                        };
                        offset = payload_end;

                        // Netscape looping extension carries loop count in
                        // the first 3 bytes of its sub-block payload:
                        //   [0x01, loop_count_le_lo, loop_count_le_hi]
                        if &identifier == b"NETSCAPE2.0" && payload.len() >= 3 && payload[0] == 0x01
                        {
                            loop_count = Some(u16::from_le_bytes([payload[1], payload[2]]));
                        }

                        app_extensions.push(GifAppExtension {
                            identifier,
                            payload,
                            offset_start: cursor.absolute_offset(payload_start),
                            offset_end: cursor.absolute_offset(payload_end),
                        });

                        blocks.push(GifBlock {
                            kind: GifBlockKind::ApplicationExtension { identifier },
                            offset_start: cursor.absolute_offset(block_start),
                            offset_end: cursor.absolute_offset(offset),
                        });
                        let label_str = String::from_utf8_lossy(&identifier).trim().to_string();
                        nodes.push(ContainerNode {
                            kind: "block".into(),
                            label: format!("app_ext:{label_str}"),
                            offset_start: cursor.absolute_offset(block_start),
                            offset_end: cursor.absolute_offset(offset),
                            parent_label: Some("gif".into()),
                        });
                    }
                    0xFE | 0x01 => {
                        // Comment / Plain-text extensions: skip sub-blocks.
                        let kind = if label == 0xFE {
                            GifBlockKind::CommentExtension
                        } else {
                            GifBlockKind::PlainTextExtension
                        };
                        let label_name = if label == 0xFE {
                            "comment_ext"
                        } else {
                            "plain_text_ext"
                        };
                        // Plain-text has a fixed 12-byte header before sub-blocks.
                        if label == 0x01 {
                            let bs = cursor.read_u8(offset)? as usize;
                            offset += 1;
                            if offset + bs > cursor.len() {
                                issues.push(Issue {
                                    severity: Severity::Warning,
                                    code: "gif_plain_text_truncated".into(),
                                    message: "plain text extension truncated".into(),
                                    offset: Some(cursor.absolute_offset(block_start)),
                                    context: None,
                                });
                                break;
                            }
                            offset += bs;
                        }
                        offset = match skip_sub_blocks(&cursor, offset) {
                            Ok(end) => end,
                            Err(end) => {
                                issues.push(Issue {
                                    severity: Severity::Warning,
                                    code: "gif_extension_sub_blocks_unterminated".into(),
                                    message: format!("{label_name} sub-blocks unterminated"),
                                    offset: Some(cursor.absolute_offset(block_start)),
                                    context: None,
                                });
                                end
                            }
                        };
                        blocks.push(GifBlock {
                            kind,
                            offset_start: cursor.absolute_offset(block_start),
                            offset_end: cursor.absolute_offset(offset),
                        });
                        nodes.push(ContainerNode {
                            kind: "block".into(),
                            label: label_name.into(),
                            offset_start: cursor.absolute_offset(block_start),
                            offset_end: cursor.absolute_offset(offset),
                            parent_label: Some("gif".into()),
                        });
                    }
                    _ => {
                        // Unknown extension label: still walk its sub-blocks
                        // so we can keep reading the file.
                        offset = match skip_sub_blocks(&cursor, offset) {
                            Ok(end) => end,
                            Err(end) => end,
                        };
                        issues.push(Issue {
                            severity: Severity::Info,
                            code: "gif_unknown_extension".into(),
                            message: format!("unknown extension label 0x{label:02X}"),
                            offset: Some(cursor.absolute_offset(block_start)),
                            context: None,
                        });
                    }
                }
            }
            other => {
                issues.push(Issue {
                    severity: Severity::Warning,
                    code: "gif_unknown_block".into(),
                    message: format!("unknown block introducer 0x{other:02X}"),
                    offset: Some(cursor.absolute_offset(block_start)),
                    context: None,
                });
                break;
            }
        }
    }

    if !saw_trailer {
        issues.push(issue(
            Severity::Warning,
            "gif_missing_trailer",
            "gif stream ended without 0x3B trailer",
        ));
    }

    Ok(GifContainer {
        nodes,
        blocks,
        width,
        height,
        background_color_index,
        global_color_table_size,
        frame_count,
        animation_duration_centiseconds,
        loop_count,
        app_extensions,
        issues,
    })
}

/// Walk a GIF sub-block sequence, discarding contents. Returns the offset
/// immediately after the 0-length terminator on success, or `Err(offset)` with
/// the offset where the stream ran out of bytes when the terminator is missing.
fn skip_sub_blocks(cursor: &Cursor<'_>, mut offset: usize) -> Result<usize, usize> {
    while offset < cursor.len() {
        let size = match cursor.read_u8(offset) {
            Ok(byte) => byte as usize,
            Err(_) => return Err(offset),
        };
        offset += 1;
        if size == 0 {
            return Ok(offset);
        }
        if offset + size > cursor.len() {
            return Err(cursor.len());
        }
        offset += size;
    }
    Err(offset)
}

/// Same as `skip_sub_blocks`, but appends each sub-block's payload bytes
/// (without the length prefix) to `out`. Used for assembling Application
/// Extension payloads such as Netscape loop and Adobe XMP.
fn collect_sub_blocks(
    cursor: &Cursor<'_>,
    mut offset: usize,
    out: &mut Vec<u8>,
) -> Result<usize, usize> {
    while offset < cursor.len() {
        let size = match cursor.read_u8(offset) {
            Ok(byte) => byte as usize,
            Err(_) => return Err(offset),
        };
        offset += 1;
        if size == 0 {
            return Ok(offset);
        }
        if offset + size > cursor.len() {
            return Err(cursor.len());
        }
        match cursor.slice(offset, size) {
            Ok(chunk) => out.extend_from_slice(chunk),
            Err(_) => return Err(cursor.len()),
        }
        offset += size;
    }
    Err(offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_static_gif() -> Vec<u8> {
        // GIF89a, 1x1, GCT (2 entries -> 6 bytes), single image descriptor,
        // trailer.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"GIF89a");
        bytes.extend_from_slice(&1u16.to_le_bytes()); // width
        bytes.extend_from_slice(&1u16.to_le_bytes()); // height
        bytes.push(0b1000_0000); // packed: GCT flag, gct size 0 -> 2 entries
        bytes.push(0); // background color index
        bytes.push(0); // pixel aspect ratio
        // GCT: 2 entries * 3 bytes = 6 bytes
        bytes.extend_from_slice(&[0, 0, 0, 255, 255, 255]);
        // Image descriptor
        bytes.push(0x2C);
        bytes.extend_from_slice(&0u16.to_le_bytes()); // left
        bytes.extend_from_slice(&0u16.to_le_bytes()); // top
        bytes.extend_from_slice(&1u16.to_le_bytes()); // width
        bytes.extend_from_slice(&1u16.to_le_bytes()); // height
        bytes.push(0); // packed: no LCT, no interlace
        // image data
        bytes.push(0x02); // LZW min code size
        bytes.push(0x02); // sub-block length
        bytes.push(0x44); // data
        bytes.push(0x01);
        bytes.push(0x00); // sub-block terminator
        // Trailer
        bytes.push(0x3B);
        bytes
    }

    #[test]
    fn parses_minimal_static_gif() {
        let bytes = minimal_static_gif();
        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert_eq!(parsed.width, 1);
        assert_eq!(parsed.height, 1);
        assert_eq!(parsed.frame_count, 1);
        assert_eq!(parsed.animation_duration_centiseconds, 0);
        assert_eq!(parsed.global_color_table_size, 6);
        assert!(parsed.loop_count.is_none());
        assert!(parsed.issues.is_empty());
        assert!(
            parsed
                .blocks
                .iter()
                .any(|b| matches!(b.kind, GifBlockKind::Trailer))
        );
    }

    fn animated_gif_with_loop() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"GIF89a");
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.push(0b1000_0000);
        bytes.push(0);
        bytes.push(0);
        bytes.extend_from_slice(&[0, 0, 0, 255, 255, 255]);
        // Netscape looping extension
        bytes.push(0x21);
        bytes.push(0xFF);
        bytes.push(0x0B);
        bytes.extend_from_slice(b"NETSCAPE2.0");
        bytes.push(0x03); // sub-block length
        bytes.push(0x01);
        bytes.extend_from_slice(&7u16.to_le_bytes()); // loop 7 times
        bytes.push(0x00); // terminator
        // Frame 1
        bytes.push(0x21);
        bytes.push(0xF9);
        bytes.push(0x04);
        bytes.push(0x00);
        bytes.extend_from_slice(&50u16.to_le_bytes()); // 50 cs delay
        bytes.push(0x00); // transparent color
        bytes.push(0x00); // sub-block terminator
        bytes.push(0x2C);
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.push(0);
        bytes.push(0x02);
        bytes.push(0x02);
        bytes.push(0x44);
        bytes.push(0x01);
        bytes.push(0x00);
        // Frame 2
        bytes.push(0x21);
        bytes.push(0xF9);
        bytes.push(0x04);
        bytes.push(0x00);
        bytes.extend_from_slice(&50u16.to_le_bytes());
        bytes.push(0x00);
        bytes.push(0x00);
        bytes.push(0x2C);
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.push(0);
        bytes.push(0x02);
        bytes.push(0x02);
        bytes.push(0x44);
        bytes.push(0x01);
        bytes.push(0x00);
        bytes.push(0x3B);
        bytes
    }

    #[test]
    fn parses_animated_gif_with_loop_and_delays() {
        let bytes = animated_gif_with_loop();
        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert_eq!(parsed.frame_count, 2);
        assert_eq!(parsed.animation_duration_centiseconds, 100);
        assert_eq!(parsed.loop_count, Some(7));
        assert!(parsed.issues.is_empty());
    }

    fn xmp_magic_trailer() -> Vec<u8> {
        // 0x01 0xFF 0xFE 0xFD ... 0x01 0x00, total 258 bytes.
        let mut trailer = vec![0x01];
        for v in (0u8..=255u8).rev() {
            trailer.push(v);
        }
        trailer.push(0x00);
        assert_eq!(trailer.len(), 258);
        trailer
    }

    fn build_xmp_gif(packet: &[u8]) -> Vec<u8> {
        // GIF89a + LSD, no GCT, then an Application Extension carrying the
        // XMP packet bytes followed by the magic trailer, then trailer.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"GIF89a");
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.push(0); // no GCT
        bytes.push(0);
        bytes.push(0);
        // Application Extension
        bytes.push(0x21);
        bytes.push(0xFF);
        bytes.push(0x0B);
        bytes.extend_from_slice(b"XMP DataXMP");
        // Total payload = packet bytes + 258-byte trailer, emitted as a
        // single contiguous stream where each leading length byte happens to
        // describe the upcoming chunk. We produce the canonical form: start
        // with one max-size sub-block containing the XMP packet bytes plus
        // the 0x01 byte of the trailer (when packet is short), followed by
        // the trailer bytes which themselves act as length+payload.
        //
        // Simplest valid encoding: emit the entire (packet || trailer) byte
        // stream in 255-byte sub-blocks. Each length byte is added BEFORE
        // its 255 chunk; the parser strips length bytes when reassembling.
        let mut stream: Vec<u8> = Vec::new();
        stream.extend_from_slice(packet);
        stream.extend_from_slice(&xmp_magic_trailer());
        let mut idx = 0;
        while idx < stream.len() {
            let chunk = (stream.len() - idx).min(255);
            bytes.push(chunk as u8);
            bytes.extend_from_slice(&stream[idx..idx + chunk]);
            idx += chunk;
        }
        bytes.push(0x00); // sub-block terminator
        bytes.push(0x3B);
        bytes
    }

    #[test]
    fn extracts_xmp_with_magic_trailer_stripped() {
        let packet = b"<x:xmpmeta xmlns:x='adobe:ns:meta/'></x:xmpmeta>";
        let bytes = build_xmp_gif(packet);
        let parsed = parse_bytes(&bytes, 0).unwrap();
        let payloads: Vec<_> = parsed.xmp_payloads().collect();
        assert_eq!(payloads.len(), 1);
        let (_, _, body) = payloads[0];
        assert_eq!(body, packet.as_slice());
    }

    #[test]
    fn flags_truncated_image_descriptor() {
        // GIF89a header + LSD (no GCT) + image descriptor introducer with
        // missing body bytes.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"GIF89a");
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.push(0);
        bytes.push(0);
        bytes.push(0);
        bytes.push(0x2C); // Image Descriptor introducer
        // No body bytes follow.
        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert!(
            parsed
                .issues
                .iter()
                .any(|issue| issue.code == "gif_image_descriptor_truncated")
        );
    }

    #[test]
    fn rejects_non_gif() {
        let result = parse_bytes(b"\x89PNG\r\n\x1a\n\0\0\0\0\0", 0);
        assert!(result.is_err());
    }
}
