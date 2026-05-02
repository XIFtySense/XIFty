//! PROF (Picture Profile) UUID atom decoder.
//!
//! ## Wire format (per ExifTool `lib/Image/ExifTool/QuickTime.pm`)
//!
//! - The Sony `PROF` UUID box wraps a 16-byte usertype
//!   `50 52 4f 46 21 d2 4f ce bb 88 69 5c fa c9 c7 40` (PROF + Sony tail).
//!   See `QuickTime.pm` line ~711-718 (`UUID-PROF` dispatch:
//!   `Start => 24, # uid(16) + version(1) + flags(3) + count(4)`).
//! - After the 16-byte usertype the payload is a "profile container":
//!   `version(1) + flags(3) + count(4) + N × (size(4) + FourCC(4) + body)`.
//!   The container is dispatched via `%Image::ExifTool::QuickTime::Profile`
//!   (`QuickTime.pm` line ~2668-2692). Recognised sub-box FourCCs are
//!   `FPRF` (FileGlobalProfile), `APRF` (AudioProfile), `VPRF` (VideoProfile).
//! - Each sub-box body is a `ProcessBinaryData` blob with `FORMAT => 'int32u'`
//!   (big-endian); fields are addressed by 32-bit-word index. Tables:
//!   - `FileProf`  — `QuickTime.pm` line ~2694-2709.
//!   - `AudioProf` — `QuickTime.pm` line ~2711-2747.
//!   - `VideoProf` — `QuickTime.pm` line ~2749-2804.
//!
//! Reference material only — read to understand, port to Rust, ship Rust.
//! Nothing from ExifTool ships in the artifact.

use xifty_core::{MetadataEntry, Provenance, TypedValue};
use xifty_source::{Cursor, Endian};

/// Decode the inner bytes of a Sony `PROF` UUID atom (post-usertype slice).
///
/// `bytes` must contain the bytes that follow the 16-byte usertype — i.e.
/// `version(1) + flags(3) + count(4) + sub-boxes...`. `base_offset` is the
/// absolute offset within the source where `bytes` begins; provenance is
/// recorded against this base.
pub fn decode_prof(bytes: &[u8], base_offset: u64, container_name: &str) -> Vec<MetadataEntry> {
    let mut out = Vec::new();
    if bytes.len() < 8 {
        return out;
    }
    let cursor = Cursor::new(bytes, base_offset);
    let Ok(count) = cursor.read_u32(4, Endian::Big) else {
        return out;
    };
    let mut pos = 8usize;
    let mut emitted = 0u32;
    while emitted < count && pos + 8 <= bytes.len() {
        let Ok(box_size) = cursor.read_u32(pos, Endian::Big) else {
            return out;
        };
        let box_size = box_size as usize;
        if box_size < 8 || pos + box_size > bytes.len() {
            return out;
        }
        let fourcc: [u8; 4] = match bytes.get(pos + 4..pos + 8) {
            Some(slice) => [slice[0], slice[1], slice[2], slice[3]],
            None => return out,
        };
        let body_start = pos + 8;
        let body_end = pos + box_size;
        let body = &bytes[body_start..body_end];
        let body_offset = base_offset + body_start as u64;
        match &fourcc {
            b"FPRF" => decode_fprf(body, body_offset, container_name, &mut out),
            b"APRF" => decode_aprf(body, body_offset, container_name, &mut out),
            b"VPRF" => decode_vprf(body, body_offset, container_name, &mut out),
            _ => {
                // Unknown sub-box (e.g. PREX, OLYM): skip silently. ExifTool
                // routes unknowns through generic ProcessBinaryData; we drop
                // them here per the bounded-ship policy.
            }
        }
        pos += box_size;
        emitted += 1;
    }
    out
}

/// FPRF — FileGlobalProfile. Per `QuickTime.pm:2694-2709`.
/// Word 0: FileProfileVersion (Unknown, skipped).
/// Word 1: FileFunctionFlags (bitmask).
fn decode_fprf(body: &[u8], base_offset: u64, container: &str, out: &mut Vec<MetadataEntry>) {
    let cursor = Cursor::new(body, base_offset);
    if let Ok(flags) = cursor.read_u32(4, Endian::Big) {
        out.push(entry(
            container,
            "PROF/FPRF",
            "FileFunctionFlags",
            TypedValue::Integer(flags as i64),
            "moov/uuid/PROF/FPRF",
            base_offset,
        ));
    }
}

/// APRF — AudioProfile. Per `QuickTime.pm:2711-2747`.
fn decode_aprf(body: &[u8], base_offset: u64, container: &str, out: &mut Vec<MetadataEntry>) {
    let cursor = Cursor::new(body, base_offset);
    // Word 1: AudioTrackID
    if let Ok(value) = cursor.read_u32(4, Endian::Big) {
        out.push(entry(
            container,
            "PROF/APRF/1",
            "AudioTrackID",
            TypedValue::Integer(value as i64),
            "moov/uuid/PROF/APRF",
            base_offset,
        ));
    }
    // Word 2: AudioCodec (4-byte FourCC, ASCII)
    if let Some(slice) = body.get(8..12)
        && let Some(codec) = ascii_fourcc(slice)
    {
        out.push(entry(
            container,
            "PROF/APRF/2",
            "AudioCodec",
            TypedValue::String(codec),
            "moov/uuid/PROF/APRF",
            base_offset,
        ));
    }
    // Word 5: AudioAvgBitrate (kbps, * 1000 in ExifTool ValueConv).
    if let Ok(value) = cursor.read_u32(20, Endian::Big) {
        out.push(entry(
            container,
            "PROF/APRF/5",
            "AudioAvgBitrate",
            TypedValue::Integer((value as i64) * 1000),
            "moov/uuid/PROF/APRF",
            base_offset,
        ));
    }
    // Word 6: AudioMaxBitrate (kbps).
    if let Ok(value) = cursor.read_u32(24, Endian::Big) {
        out.push(entry(
            container,
            "PROF/APRF/6",
            "AudioMaxBitrate",
            TypedValue::Integer((value as i64) * 1000),
            "moov/uuid/PROF/APRF",
            base_offset,
        ));
    }
    // Word 7: AudioSampleRate
    if let Ok(value) = cursor.read_u32(28, Endian::Big) {
        out.push(entry(
            container,
            "PROF/APRF/7",
            "AudioSampleRate",
            TypedValue::Integer(value as i64),
            "moov/uuid/PROF/APRF",
            base_offset,
        ));
    }
    // Word 8: AudioChannels (high 16 bits of a 32-bit slot in some captures —
    // ExifTool reads as int32u; we mirror that.)
    if let Ok(value) = cursor.read_u32(32, Endian::Big) {
        out.push(entry(
            container,
            "PROF/APRF/8",
            "AudioChannels",
            TypedValue::Integer(value as i64),
            "moov/uuid/PROF/APRF",
            base_offset,
        ));
    }
}

/// VPRF — VideoProfile. Per `QuickTime.pm:2749-2804`.
fn decode_vprf(body: &[u8], base_offset: u64, container: &str, out: &mut Vec<MetadataEntry>) {
    let cursor = Cursor::new(body, base_offset);
    // Word 1: VideoTrackID
    if let Ok(value) = cursor.read_u32(4, Endian::Big) {
        out.push(entry(
            container,
            "PROF/VPRF/1",
            "VideoTrackID",
            TypedValue::Integer(value as i64),
            "moov/uuid/PROF/VPRF",
            base_offset,
        ));
    }
    // Word 2: VideoCodec (4-byte FourCC).
    if let Some(slice) = body.get(8..12)
        && let Some(codec) = ascii_fourcc(slice)
    {
        out.push(entry(
            container,
            "PROF/VPRF/2",
            "VideoCodec",
            TypedValue::String(codec),
            "moov/uuid/PROF/VPRF",
            base_offset,
        ));
    }
    // Word 5: VideoAvgBitrate (kbps).
    if let Ok(value) = cursor.read_u32(20, Endian::Big) {
        out.push(entry(
            container,
            "PROF/VPRF/5",
            "VideoAvgBitrate",
            TypedValue::Integer((value as i64) * 1000),
            "moov/uuid/PROF/VPRF",
            base_offset,
        ));
    }
    // Word 6: VideoMaxBitrate (kbps).
    if let Ok(value) = cursor.read_u32(24, Endian::Big) {
        out.push(entry(
            container,
            "PROF/VPRF/6",
            "VideoMaxBitrate",
            TypedValue::Integer((value as i64) * 1000),
            "moov/uuid/PROF/VPRF",
            base_offset,
        ));
    }
    // Word 7: VideoAvgFrameRate (16.16 fixed-point).
    if let Ok(value) = cursor.read_u32(28, Endian::Big) {
        let fps = (value as f64) / 65536.0;
        out.push(entry(
            container,
            "PROF/VPRF/7",
            "VideoAvgFrameRate",
            TypedValue::Float(fps),
            "moov/uuid/PROF/VPRF",
            base_offset,
        ));
    }
    // Word 8: VideoMaxFrameRate (16.16 fixed-point).
    if let Ok(value) = cursor.read_u32(32, Endian::Big) {
        let fps = (value as f64) / 65536.0;
        out.push(entry(
            container,
            "PROF/VPRF/8",
            "VideoMaxFrameRate",
            TypedValue::Float(fps),
            "moov/uuid/PROF/VPRF",
            base_offset,
        ));
    }
    // Word 9: VideoSize — int16u[2] (width, height).
    if let (Ok(width), Ok(height)) = (
        cursor.read_u16(36, Endian::Big),
        cursor.read_u16(38, Endian::Big),
    ) {
        out.push(entry(
            container,
            "PROF/VPRF/9",
            "VideoSize",
            TypedValue::Dimensions {
                width: width as u32,
                height: height as u32,
            },
            "moov/uuid/PROF/VPRF",
            base_offset,
        ));
    }
    // Word 10: PixelAspectRatio — int16u[2].
    if let (Ok(num), Ok(den)) = (
        cursor.read_u16(40, Endian::Big),
        cursor.read_u16(42, Endian::Big),
    ) {
        out.push(entry(
            container,
            "PROF/VPRF/10",
            "PixelAspectRatio",
            TypedValue::Rational {
                numerator: num as i64,
                denominator: den as i64,
            },
            "moov/uuid/PROF/VPRF",
            base_offset,
        ));
    }
}

fn ascii_fourcc(slice: &[u8]) -> Option<String> {
    let trimmed: Vec<u8> = slice.iter().take_while(|b| **b != 0).copied().collect();
    if trimmed.is_empty() {
        return None;
    }
    if !trimmed.iter().all(|b| (0x20..=0x7e).contains(b) || *b == 0) {
        return None;
    }
    String::from_utf8(trimmed).ok()
}

fn entry(
    container: &str,
    tag_id: &str,
    tag_name: &str,
    value: TypedValue,
    path: &str,
    offset_start: u64,
) -> MetadataEntry {
    MetadataEntry {
        namespace: "sony_video".into(),
        tag_id: tag_id.into(),
        tag_name: tag_name.into(),
        value,
        provenance: Provenance {
            container: container.into(),
            namespace: "sony_video".into(),
            path: Some(path.into()),
            offset_start: Some(offset_start),
            offset_end: None,
            notes: Vec::new(),
        },
        notes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real bytes captured from `fixtures/local/C0242.MP4` (Sony FX-series),
    /// PROF UUID at file offset 28. After stripping the 16-byte usertype this
    /// is the exact payload (124 bytes). Used as the canonical test vector
    /// since synthetic byte buffers cannot reliably exercise the wire format.
    fn c0242_prof_inner() -> Vec<u8> {
        let hex = "000000000000000300000014465052460000000020000000000000000000002c\
                   41505246000000000000000274776f730000000000000000000006000000060\
                   00000bb800000000200000034565052460000000000000001617663310164003\
                   30003000200018\
                   6a0000186a00017f9dc0017f9dc0f00087000010001";
        // The hex above was line-folded for readability; rebuild without nl.
        hex.chars()
            .filter(|c| c.is_ascii_hexdigit())
            .collect::<String>()
            .as_bytes()
            .chunks(2)
            .map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap())
            .collect()
    }

    #[test]
    fn decodes_real_c0242_prof_payload() {
        let bytes = c0242_prof_inner();
        let entries = decode_prof(&bytes, 0, "mp4");
        // VPRF + APRF + FPRF should each contribute fields.
        let names: Vec<_> = entries.iter().map(|e| e.tag_name.as_str()).collect();
        assert!(names.contains(&"FileFunctionFlags"), "names={names:?}");
        assert!(names.contains(&"AudioCodec"), "names={names:?}");
        assert!(names.contains(&"VideoCodec"), "names={names:?}");
        assert!(names.contains(&"VideoSize"), "names={names:?}");

        let codec = entries
            .iter()
            .find(|e| e.tag_name == "VideoCodec")
            .expect("VideoCodec entry");
        assert!(matches!(&codec.value, TypedValue::String(s) if s == "avc1"));

        let audio_codec = entries
            .iter()
            .find(|e| e.tag_name == "AudioCodec")
            .expect("AudioCodec entry");
        assert!(matches!(&audio_codec.value, TypedValue::String(s) if s == "twos"));

        let size = entries
            .iter()
            .find(|e| e.tag_name == "VideoSize")
            .expect("VideoSize entry");
        match &size.value {
            TypedValue::Dimensions { width, height } => {
                assert_eq!(*width, 3840);
                assert_eq!(*height, 2160);
            }
            other => panic!("expected Dimensions, got {other:?}"),
        }
    }

    #[test]
    fn empty_input_returns_no_entries() {
        assert!(decode_prof(&[], 0, "mp4").is_empty());
        assert!(decode_prof(&[0u8; 4], 0, "mp4").is_empty());
    }

    #[test]
    fn provenance_uses_supplied_container_and_namespace() {
        let bytes = c0242_prof_inner();
        let entries = decode_prof(&bytes, 100, "mov");
        let any = entries.first().expect("at least one entry");
        assert_eq!(any.namespace, "sony_video");
        assert_eq!(any.provenance.container, "mov");
        assert_eq!(any.provenance.namespace, "sony_video");
        assert!(
            any.provenance
                .path
                .as_ref()
                .unwrap()
                .starts_with("moov/uuid/PROF/")
        );
        assert!(any.provenance.offset_start.unwrap() >= 100);
    }
}
