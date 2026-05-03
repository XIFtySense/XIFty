//! Decoder for PNG `Raw profile type *` text-chunk payloads.
//!
//! ImageMagick / libvips / GraphicsMagick (and Google's NotebookLM pipeline)
//! transcode JPEG segments into PNG `tEXt`/`zTXt`/`iTXt` chunks keyed
//! `Raw profile type <name>`. The body is a small ASCII frame followed by
//! hex-encoded bytes that wrap the raw JPEG segment.
//!
//! This module returns **raw bytes only**. It does **not** depend on or call
//! any `xifty-meta-*` crate or `xifty-container-tiff`. All meta dispatch
//! lives in `xifty-cli`. The `RawProfilePayload` enum is the typed handoff
//! that drives that dispatch.
//!
//! For APP1 carriers we mirror the JPEG container's prefix-stripping rules
//! (see `crates/xifty-container-jpeg/src/lib.rs:22-23` and `:52-57`) so the
//! returned bytes are directly consumable by the matching meta decoder
//! without further unwrapping.

use std::io::Read;

use flate2::read::ZlibDecoder;

/// Classification of a `Raw profile type *` keyword. The keyword is what
/// follows the `Raw profile type ` prefix inside the chunk's keyword field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawProfileKind {
    /// JPEG `APP1` segment — usually EXIF (`Exif\0\0` prefix) or XMP
    /// (`http://ns.adobe.com/xap/1.0/\0` prefix), occasionally something
    /// exotic like Flashpix or Stim.
    App1,
    /// Bare TIFF/EXIF stream (no `Exif\0\0` prefix).
    Exif,
    /// Bare XMP packet (no Adobe namespace prefix).
    Xmp,
    /// ICC profile bytes.
    Icc,
    /// Alternate ImageMagick spelling for ICC.
    Icm,
    /// Bare IPTC IIM stream.
    Iptc,
    /// JPEG `APP13` segment — Photoshop 3.0 / 8BIM Image Resource Block.
    App13,
    /// Direct 8BIM Image Resource Block.
    EightBim,
}

/// Typed raw-byte payload for a recognised `Raw profile type *` chunk.
///
/// Variants carry bytes ready for the matching meta-crate decoder: APP1
/// carriers have their JPEG-style signature prefix stripped; everything else
/// is passed through as-is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawProfilePayload {
    /// TIFF stream — `Exif\0\0` prefix already stripped (when present).
    Exif(Vec<u8>),
    /// XMP packet — `http://ns.adobe.com/xap/1.0/\0` prefix already stripped
    /// (when present).
    Xmp(Vec<u8>),
    /// IPTC payload — bare IIM or `Photoshop 3.0`/8BIM IRB. Passed through;
    /// `xifty-meta-iptc` handles both shapes transparently.
    Iptc(Vec<u8>),
    /// ICC profile bytes — passed through.
    Icc(Vec<u8>),
    /// APP1 segment with an unrecognised signature (not EXIF, not XMP).
    App1Unknown(Vec<u8>),
}

/// Recognise a PNG text-chunk keyword as a member of the `Raw profile type *`
/// family. The keyword passed in is the full chunk keyword (e.g.
/// `"Raw profile type APP1"` or `"raw profile type icc"` — matching is
/// case-insensitive on the suffix).
pub fn classify_raw_profile_keyword(keyword: &str) -> Option<RawProfileKind> {
    let suffix = strip_prefix_ci(keyword, "Raw profile type ")?;
    let suffix = suffix.trim();
    match suffix.to_ascii_lowercase().as_str() {
        "app1" => Some(RawProfileKind::App1),
        "exif" => Some(RawProfileKind::Exif),
        "xmp" => Some(RawProfileKind::Xmp),
        "icc" => Some(RawProfileKind::Icc),
        "icm" => Some(RawProfileKind::Icm),
        "iptc" => Some(RawProfileKind::Iptc),
        "app13" => Some(RawProfileKind::App13),
        "8bim" => Some(RawProfileKind::EightBim),
        _ => None,
    }
}

fn strip_prefix_ci<'a>(input: &'a str, prefix: &str) -> Option<&'a str> {
    if input.len() < prefix.len() {
        return None;
    }
    let (head, tail) = input.split_at(prefix.len());
    if head.eq_ignore_ascii_case(prefix) {
        Some(tail)
    } else {
        None
    }
}

/// Decode a `Raw profile type *` chunk body into a typed payload.
///
/// `keyword` is the chunk's keyword (e.g. `"Raw profile type APP1"`).
/// `body` is the chunk content **after** the keyword and its framing
/// (compression flag/method, language tag, translated keyword, etc.) have
/// been stripped — i.e. the decompressed body of `tEXt`/`zTXt`/`iTXt`.
///
/// Returns `None` when the keyword is not a `Raw profile type *` family
/// member, when the framing is malformed, or when the hex stream is invalid
/// (odd length, non-hex characters).
pub fn decode_raw_profile(keyword: &str, body: &[u8]) -> Option<RawProfilePayload> {
    let kind = classify_raw_profile_keyword(keyword)?;
    let bytes = decode_imagemagick_raw_profile(body)?;
    Some(payload_for_kind(kind, bytes))
}

fn payload_for_kind(kind: RawProfileKind, bytes: Vec<u8>) -> RawProfilePayload {
    match kind {
        RawProfileKind::App1 => {
            const EXIF_PREFIX: &[u8] = b"Exif\0\0";
            const XMP_PREFIX: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
            if bytes.starts_with(EXIF_PREFIX) {
                RawProfilePayload::Exif(bytes[EXIF_PREFIX.len()..].to_vec())
            } else if bytes.starts_with(XMP_PREFIX) {
                RawProfilePayload::Xmp(bytes[XMP_PREFIX.len()..].to_vec())
            } else {
                RawProfilePayload::App1Unknown(bytes)
            }
        }
        RawProfileKind::Exif => RawProfilePayload::Exif(bytes),
        RawProfileKind::Xmp => RawProfilePayload::Xmp(bytes),
        RawProfileKind::Icc | RawProfileKind::Icm => RawProfilePayload::Icc(bytes),
        RawProfileKind::Iptc | RawProfileKind::App13 | RawProfileKind::EightBim => {
            RawProfilePayload::Iptc(bytes)
        }
    }
}

/// Decode the ImageMagick / libvips raw-profile framing.
///
/// Frame shape:
///   `\n<profile-name>\n<spaces?><decimal length>\n<hex bytes>\n`
///
/// libvips uses `\n  <len>\n` (leading spaces); ImageMagick uses `\n<len>\n`.
/// Both are accepted. Whitespace inside the hex stream (newlines, spaces) is
/// ignored. Returns `None` for odd-length hex, non-hex characters, or
/// missing framing lines.
fn decode_imagemagick_raw_profile(content: &[u8]) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(content).ok()?;
    let trimmed = text.trim_start_matches('\n');
    let mut lines = trimmed.splitn(3, '\n');
    let _name = lines.next()?;
    let _length = lines.next()?.trim();
    let rest = lines.next()?;
    let hex: String = rest.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.is_empty() || !hex.len().is_multiple_of(2) {
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

/// Decode an `iCCP` chunk payload into raw ICC profile bytes.
///
/// PNG `iCCP` shape: `<profile-name>\0<compression-method><zlib-stream>`.
/// Only compression method `0` (deflate) is defined by the spec. Returns
/// `None` for malformed chunks or unknown compression methods.
pub fn decode_iccp_payload(payload: &[u8]) -> Option<Vec<u8>> {
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

/// Decode the keyword + framing from a raw `tEXt`/`zTXt`/`iTXt` chunk
/// payload. Returns the keyword and the (decompressed) body bytes.
///
/// This is exposed for callers (notably `xifty-cli`) that already hold the
/// raw chunk payload from `PngContainer::text_payloads()`. It does **not**
/// classify the keyword — pair it with `decode_raw_profile` (or
/// `classify_raw_profile_keyword`) for that.
pub fn decode_text_chunk(chunk_type: &[u8; 4], payload: &[u8]) -> Option<(String, Vec<u8>)> {
    let nul = payload.iter().position(|byte| *byte == 0)?;
    let keyword = String::from_utf8_lossy(&payload[..nul]).into_owned();
    let body: Vec<u8> = match chunk_type {
        b"tEXt" => payload.get(nul + 1..)?.to_vec(),
        b"zTXt" => {
            let method = *payload.get(nul + 1)?;
            if method != 0 {
                return None;
            }
            let compressed = payload.get(nul + 2..)?;
            let mut decoder = ZlibDecoder::new(compressed);
            let mut decoded = Vec::new();
            decoder.read_to_end(&mut decoded).ok()?;
            decoded
        }
        b"iTXt" => {
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
                    return None;
                }
                let mut decoder = ZlibDecoder::new(text);
                let mut decoded = Vec::new();
                decoder.read_to_end(&mut decoded).ok()?;
                decoded
            }
        }
        _ => return None,
    };
    Some((keyword, body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::ZlibEncoder};
    use std::io::Write;

    fn imagemagick_frame(name: &str, raw: &[u8]) -> Vec<u8> {
        // ImageMagick: "\n<name>\n<len>\n<hex>\n"
        let mut hex = String::new();
        for (i, byte) in raw.iter().enumerate() {
            if i % 36 == 0 {
                hex.push('\n');
            }
            hex.push_str(&format!("{:02x}", byte));
        }
        hex.push('\n');
        let mut out = Vec::new();
        out.push(b'\n');
        out.extend_from_slice(name.as_bytes());
        out.push(b'\n');
        out.extend_from_slice(format!("{}", raw.len()).as_bytes());
        out.extend_from_slice(hex.as_bytes());
        out
    }

    fn libvips_frame(name: &str, raw: &[u8]) -> Vec<u8> {
        // libvips: "\n<name>\n      <len>\n<hex>\n" (leading spaces)
        let mut hex = String::new();
        for (i, byte) in raw.iter().enumerate() {
            if i % 36 == 0 {
                hex.push('\n');
            }
            hex.push_str(&format!("{:02x}", byte));
        }
        hex.push('\n');
        let mut out = Vec::new();
        out.push(b'\n');
        out.extend_from_slice(name.as_bytes());
        out.push(b'\n');
        out.extend_from_slice(format!("      {}", raw.len()).as_bytes());
        out.extend_from_slice(hex.as_bytes());
        out
    }

    #[test]
    fn classifies_each_keyword() {
        assert_eq!(
            classify_raw_profile_keyword("Raw profile type APP1"),
            Some(RawProfileKind::App1)
        );
        assert_eq!(
            classify_raw_profile_keyword("raw profile type exif"),
            Some(RawProfileKind::Exif)
        );
        assert_eq!(
            classify_raw_profile_keyword("Raw profile type xmp"),
            Some(RawProfileKind::Xmp)
        );
        assert_eq!(
            classify_raw_profile_keyword("Raw profile type icc"),
            Some(RawProfileKind::Icc)
        );
        assert_eq!(
            classify_raw_profile_keyword("Raw profile type icm"),
            Some(RawProfileKind::Icm)
        );
        assert_eq!(
            classify_raw_profile_keyword("Raw profile type iptc"),
            Some(RawProfileKind::Iptc)
        );
        assert_eq!(
            classify_raw_profile_keyword("Raw profile type APP13"),
            Some(RawProfileKind::App13)
        );
        assert_eq!(
            classify_raw_profile_keyword("Raw profile type 8bim"),
            Some(RawProfileKind::EightBim)
        );
        assert_eq!(classify_raw_profile_keyword("XML:com.adobe.xmp"), None);
        assert_eq!(classify_raw_profile_keyword("Raw profile type bogus"), None);
    }

    #[test]
    fn app1_exif_strips_six_byte_prefix() {
        let mut raw = Vec::new();
        raw.extend_from_slice(b"Exif\0\0");
        raw.extend_from_slice(&[0xCA, 0xFE, 0xBA, 0xBE]);
        let body = imagemagick_frame("APP1", &raw);
        match decode_raw_profile("Raw profile type APP1", &body).unwrap() {
            RawProfilePayload::Exif(bytes) => {
                assert_eq!(bytes, vec![0xCA, 0xFE, 0xBA, 0xBE]);
            }
            other => panic!("expected Exif, got {other:?}"),
        }
    }

    #[test]
    fn app1_xmp_strips_adobe_namespace_prefix() {
        let mut raw = Vec::new();
        raw.extend_from_slice(b"http://ns.adobe.com/xap/1.0/\0");
        raw.extend_from_slice(b"<x:xmpmeta/>");
        let body = imagemagick_frame("APP1", &raw);
        match decode_raw_profile("Raw profile type APP1", &body).unwrap() {
            RawProfilePayload::Xmp(bytes) => {
                assert_eq!(bytes, b"<x:xmpmeta/>".to_vec());
            }
            other => panic!("expected Xmp, got {other:?}"),
        }
    }

    #[test]
    fn app1_unknown_signature_passed_through() {
        let raw = b"Flashpix\0not-recognised".to_vec();
        let body = imagemagick_frame("APP1", &raw);
        match decode_raw_profile("Raw profile type APP1", &body).unwrap() {
            RawProfilePayload::App1Unknown(bytes) => assert_eq!(bytes, raw),
            other => panic!("expected App1Unknown, got {other:?}"),
        }
    }

    #[test]
    fn exif_keyword_returns_bytes_unchanged() {
        let raw = b"II*\0".to_vec();
        let body = imagemagick_frame("exif", &raw);
        match decode_raw_profile("Raw profile type exif", &body).unwrap() {
            RawProfilePayload::Exif(bytes) => assert_eq!(bytes, raw),
            other => panic!("expected Exif, got {other:?}"),
        }
    }

    #[test]
    fn xmp_keyword_returns_bytes_unchanged() {
        let raw = b"<?xpacket begin=\"\xEF\xBB\xBF\"?>".to_vec();
        let body = imagemagick_frame("xmp", &raw);
        match decode_raw_profile("Raw profile type xmp", &body).unwrap() {
            RawProfilePayload::Xmp(bytes) => assert_eq!(bytes, raw),
            other => panic!("expected Xmp, got {other:?}"),
        }
    }

    #[test]
    fn iptc_keyword_returns_bytes_unchanged() {
        let raw = vec![0x1C, 2, 80, 0, 3, b'K', b'a', b'i'];
        let body = imagemagick_frame("iptc", &raw);
        match decode_raw_profile("Raw profile type iptc", &body).unwrap() {
            RawProfilePayload::Iptc(bytes) => assert_eq!(bytes, raw),
            other => panic!("expected Iptc, got {other:?}"),
        }
    }

    #[test]
    fn eight_bim_keyword_returns_bytes_unchanged() {
        let raw = b"8BIM\x04\x04".to_vec();
        let body = imagemagick_frame("8bim", &raw);
        match decode_raw_profile("Raw profile type 8bim", &body).unwrap() {
            RawProfilePayload::Iptc(bytes) => assert_eq!(bytes, raw),
            other => panic!("expected Iptc (from 8bim), got {other:?}"),
        }
    }

    #[test]
    fn app13_keyword_returns_bytes_unchanged() {
        let raw = b"Photoshop 3.0\0".to_vec();
        let body = imagemagick_frame("APP13", &raw);
        match decode_raw_profile("Raw profile type APP13", &body).unwrap() {
            RawProfilePayload::Iptc(bytes) => assert_eq!(bytes, raw),
            other => panic!("expected Iptc (from APP13), got {other:?}"),
        }
    }

    #[test]
    fn icc_keyword_returns_bytes_unchanged() {
        let raw = vec![0u8; 12];
        let body = imagemagick_frame("icc", &raw);
        match decode_raw_profile("Raw profile type icc", &body).unwrap() {
            RawProfilePayload::Icc(bytes) => assert_eq!(bytes, raw),
            other => panic!("expected Icc, got {other:?}"),
        }
    }

    #[test]
    fn icm_keyword_returns_bytes_unchanged() {
        let raw = vec![0u8; 12];
        let body = imagemagick_frame("icm", &raw);
        match decode_raw_profile("Raw profile type icm", &body).unwrap() {
            RawProfilePayload::Icc(bytes) => assert_eq!(bytes, raw),
            other => panic!("expected Icc (from icm), got {other:?}"),
        }
    }

    #[test]
    fn libvips_framing_with_leading_spaces() {
        let raw = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let body = libvips_frame("iptc", &raw);
        match decode_raw_profile("Raw profile type iptc", &body).unwrap() {
            RawProfilePayload::Iptc(bytes) => assert_eq!(bytes, raw),
            other => panic!("expected Iptc, got {other:?}"),
        }
    }

    #[test]
    fn odd_length_hex_rejected() {
        // Hex section has 3 nibbles (`0` + `12`) — odd, so `decode_raw_profile`
        // must reject the frame rather than silently truncate.
        let body_odd = b"\nicc\n2\n0\n12\n".to_vec();
        assert!(decode_raw_profile("Raw profile type icc", &body_odd).is_none());
    }

    #[test]
    fn unknown_keyword_returns_none() {
        let body = imagemagick_frame("APP1", &[0u8; 4]);
        assert!(decode_raw_profile("XML:com.adobe.xmp", &body).is_none());
        assert!(decode_raw_profile("Creation Time", &body).is_none());
    }

    #[test]
    fn decodes_text_chunk_ztxt() {
        let mut payload = Vec::new();
        payload.extend_from_slice(b"Raw profile type iptc\0");
        payload.push(0u8); // compression method
        let body = imagemagick_frame("iptc", &[0xAA, 0xBB]);
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&body).unwrap();
        payload.extend_from_slice(&encoder.finish().unwrap());

        let (keyword, decompressed) = decode_text_chunk(b"zTXt", &payload).unwrap();
        assert_eq!(keyword, "Raw profile type iptc");
        assert_eq!(decompressed, body);
    }

    #[test]
    fn decodes_text_chunk_text() {
        let body = imagemagick_frame("iptc", &[0xAA, 0xBB]);
        let mut payload = Vec::new();
        payload.extend_from_slice(b"Raw profile type iptc\0");
        payload.extend_from_slice(&body);

        let (keyword, decompressed) = decode_text_chunk(b"tEXt", &payload).unwrap();
        assert_eq!(keyword, "Raw profile type iptc");
        assert_eq!(decompressed, body);
    }
}
