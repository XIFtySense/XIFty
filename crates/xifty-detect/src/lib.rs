use xifty_core::{Format, XiftyError};
use xifty_source::SourceBytes;

pub fn detect(source: &SourceBytes) -> Result<Format, XiftyError> {
    let bytes = source.bytes();
    if bytes.len() >= 4 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
        return Ok(Format::Jpeg);
    }

    if bytes.len() >= 4 && (&bytes[0..4] == b"II*\0" || &bytes[0..4] == b"MM\0*") {
        // CR2 must be checked BEFORE DNG: both are TIFF-shaped but mutually
        // exclusive (Canon CR2 carries no DNGVersion tag). The Canon
        // signature lives at bytes 8..12 with IFD0 placed at offset 0x10.
        if is_cr2_tiff(bytes) {
            return Ok(Format::Cr2);
        }
        if is_dng_tiff(bytes) {
            return Ok(Format::Dng);
        }
        return Ok(Format::Tiff);
    }

    if bytes.len() >= 8 && &bytes[0..8] == b"\x89PNG\r\n\x1a\n" {
        return Ok(Format::Png);
    }

    if bytes.len() >= 6 && (&bytes[0..6] == b"GIF87a" || &bytes[0..6] == b"GIF89a") {
        return Ok(Format::Gif);
    }

    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Ok(Format::Webp);
    }

    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        return Ok(Format::Wav);
    }

    if bytes.len() >= 12
        && &bytes[0..4] == b"FORM"
        && (&bytes[8..12] == b"AIFF" || &bytes[8..12] == b"AIFC")
    {
        return Ok(Format::Aiff);
    }

    if bytes.len() >= 4 && &bytes[0..4] == b"fLaC" {
        return Ok(Format::Flac);
    }

    if bytes.len() >= 4 && &bytes[0..4] == b"OggS" {
        return Ok(Format::Ogg);
    }

    // ID3v2-prefixed MP3, or a bare MPEG audio frame sync (0xFFEx).
    // The prefix path matches "ID3" + a plausible major version byte
    // (ID3v2.2/2.3/2.4 in the wild).
    if bytes.len() >= 4 && &bytes[0..3] == b"ID3" && matches!(bytes[3], 2 | 3 | 4) {
        return Ok(Format::Mp3);
    }
    if bytes.len() >= 2 && bytes[0] == 0xFF && (bytes[1] & 0xE0) == 0xE0 {
        return Ok(Format::Mp3);
    }

    if bytes.len() >= 16 && &bytes[4..8] == b"ftyp" {
        // AVIF must be checked BEFORE HEIF: an `avif`/`avis` brand in either
        // the major position or the compatible-brand list routes to
        // `Format::Avif` even when paired with a HEIF brand. Real-world
        // dual-decode files (e.g. `major=heic compat=[avif, mif1]`) thus
        // surface as AVIF, matching AVIF spec expectations that AVIF
        // decoders accept such files while HEIF decoders may not.
        if is_avif_brand(bytes) {
            return Ok(Format::Avif);
        }
        if is_heif_brand(bytes) {
            return Ok(Format::Heif);
        }
        if is_mov_brand(bytes) {
            return Ok(Format::Mov);
        }
        if is_m4a_brand(bytes) {
            return Ok(Format::M4a);
        }
        if is_mp4_brand(bytes) {
            return Ok(Format::Mp4);
        }
    }

    Err(XiftyError::UnsupportedFormat)
}

/// Detect Canon CR2 by its dual-anchor signature: IFD0 offset must equal
/// `0x10` AND the four bytes at offsets 8..12 must form Canon's marker
/// (`CR\x02\x00` little-endian / `CR\x00\x02` big-endian). Real CR2 writers
/// always reserve bytes 8..12 for this marker and place IFD0 at byte 16; the
/// dual-anchor check rejects plain TIFFs that happen to carry the same magic
/// at byte 8 by coincidence.
fn is_cr2_tiff(bytes: &[u8]) -> bool {
    if bytes.len() < 16 {
        return false;
    }
    let little_endian = &bytes[0..2] == b"II";
    let ifd0_offset = if little_endian {
        u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]])
    } else {
        u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]])
    };
    if ifd0_offset != 0x10 {
        return false;
    }
    // The CR2 marker is two big-endian-friendly bytes ("CR") followed by a
    // version u16 (`0x0002`) whose byte order matches the file endianness.
    if little_endian {
        &bytes[8..12] == b"CR\x02\x00"
    } else {
        &bytes[8..12] == b"CR\x00\x02"
    }
}

/// Probe IFD0 of a TIFF-shaped byte stream for the DNGVersion tag (0xC612).
///
/// Kept defensive: any read that falls outside the byte slice or an IFD0 offset
/// that does not fit into the buffer returns `false` so detection degrades to
/// plain TIFF rather than surfacing a parse error (detection has never
/// returned `XiftyError::Parse`).
fn is_dng_tiff(bytes: &[u8]) -> bool {
    if bytes.len() < 8 {
        return false;
    }
    let little_endian = &bytes[0..2] == b"II";
    let read_u16 = |slice: &[u8]| -> Option<u16> {
        let arr: [u8; 2] = slice.try_into().ok()?;
        Some(if little_endian {
            u16::from_le_bytes(arr)
        } else {
            u16::from_be_bytes(arr)
        })
    };
    let read_u32 = |slice: &[u8]| -> Option<u32> {
        let arr: [u8; 4] = slice.try_into().ok()?;
        Some(if little_endian {
            u32::from_le_bytes(arr)
        } else {
            u32::from_be_bytes(arr)
        })
    };

    let ifd0_offset = match read_u32(&bytes[4..8]) {
        Some(offset) => offset as usize,
        None => return false,
    };
    let count_slice = match bytes.get(ifd0_offset..ifd0_offset + 2) {
        Some(slice) => slice,
        None => return false,
    };
    let count = match read_u16(count_slice) {
        Some(count) => count as usize,
        None => return false,
    };
    let entries_start = ifd0_offset + 2;
    let entries_end = entries_start + count * 12;
    let entries = match bytes.get(entries_start..entries_end) {
        Some(slice) => slice,
        None => return false,
    };
    for entry in entries.chunks_exact(12) {
        if let Some(tag) = read_u16(&entry[0..2]) {
            if tag == 0xC612 {
                return true;
            }
        }
    }
    false
}

fn is_heif_brand(bytes: &[u8]) -> bool {
    let Some(brand_bytes) = bytes.get(8..16) else {
        return false;
    };
    let brand = [
        brand_bytes[0],
        brand_bytes[1],
        brand_bytes[2],
        brand_bytes[3],
    ];
    let compat = bytes[16..]
        .chunks_exact(4)
        .map(|chunk| [chunk[0], chunk[1], chunk[2], chunk[3]]);
    heif_brand(brand) || compat.into_iter().any(heif_brand)
}

fn heif_brand(brand: [u8; 4]) -> bool {
    // Note: `avif`/`avis` are intentionally absent here. They route through
    // `is_avif_brand` first (see the `ftyp` block above) so AVIF wins on any
    // AVIF brand even when a HEIF brand also appears.
    matches!(
        &brand,
        b"mif1" | b"msf1" | b"heic" | b"heix" | b"hevc" | b"heim" | b"heis"
    )
}

fn is_avif_brand(bytes: &[u8]) -> bool {
    let Some(brand_bytes) = bytes.get(8..16) else {
        return false;
    };
    let major = [
        brand_bytes[0],
        brand_bytes[1],
        brand_bytes[2],
        brand_bytes[3],
    ];
    let compat = bytes[16..]
        .chunks_exact(4)
        .map(|chunk| [chunk[0], chunk[1], chunk[2], chunk[3]]);
    avif_brand(major) || compat.into_iter().any(avif_brand)
}

fn avif_brand(brand: [u8; 4]) -> bool {
    matches!(&brand, b"avif" | b"avis")
}

fn is_mov_brand(bytes: &[u8]) -> bool {
    let Some(brand_bytes) = bytes.get(8..12) else {
        return false;
    };
    brand_bytes == b"qt  "
}

fn is_mp4_brand(bytes: &[u8]) -> bool {
    let Some(brand_bytes) = bytes.get(8..16) else {
        return false;
    };
    let major = [
        brand_bytes[0],
        brand_bytes[1],
        brand_bytes[2],
        brand_bytes[3],
    ];
    let compat = bytes[16..]
        .chunks_exact(4)
        .map(|chunk| [chunk[0], chunk[1], chunk[2], chunk[3]]);
    mp4_brand(major) || compat.into_iter().any(mp4_brand)
}

fn mp4_brand(brand: [u8; 4]) -> bool {
    matches!(
        &brand,
        b"isom" | b"iso2" | b"mp41" | b"mp42" | b"avc1" | b"M4V " | b"3gp4" | b"3gp5" | b"3g2a"
    )
}

fn is_m4a_brand(bytes: &[u8]) -> bool {
    let Some(brand_bytes) = bytes.get(8..12) else {
        return false;
    };
    matches!(brand_bytes, b"M4A " | b"M4B " | b"M4P ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_file(name: &str, bytes: &[u8]) -> PathBuf {
        let mut path = std::env::temp_dir();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("xifty-{stamp}-{name}"));
        fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn detects_formats() {
        let jpeg = temp_file("a.jpg", &[0xFF, 0xD8, 0xFF, 0xE1]);
        let tiff = temp_file("a.tif", b"II*\0\x08\0\0\0");
        let png = temp_file("a.png", b"\x89PNG\r\n\x1a\n");
        let webp = temp_file("a.webp", b"RIFF\x00\x00\x00\x00WEBP");
        let wav = temp_file("a.wav", b"RIFF\x00\x00\x00\x00WAVE");
        let heif = temp_file("a.heic", b"\x00\x00\x00\x18ftypheic\0\0\0\0mif1");
        let mp4 = temp_file("a.mp4", b"\x00\x00\x00\x18ftypisom\0\0\0\0mp42");
        let mov = temp_file("a.mov", b"\x00\x00\x00\x14ftypqt  \0\0\0\0");
        let m4a = temp_file("a.m4a", b"\x00\x00\x00\x18ftypM4A \0\0\0\0mp42");
        let m4b = temp_file("a.m4b", b"\x00\x00\x00\x18ftypM4B \0\0\0\0mp42");
        let m4p = temp_file("a.m4p", b"\x00\x00\x00\x18ftypM4P \0\0\0\0mp42");
        let flac = temp_file("a.flac", b"fLaC\x00\x00\x00\x22");
        let aiff = temp_file("a.aiff", b"FORM\x00\x00\x00\x04AIFF");
        let aifc = temp_file("a.aifc", b"FORM\x00\x00\x00\x04AIFC");
        let ogg = temp_file("a.ogg", b"OggS\x00\x02\x00\x00");
        let mp3_id3 = temp_file("a.mp3", b"ID3\x03\x00\x00\x00\x00\x00\x00");
        let mp3_sync = temp_file("b.mp3", &[0xFF, 0xFB, 0x90, 0x04]);
        let gif87 = temp_file("a.gif", b"GIF87a\x01\x00\x01\x00\x00\x00\x00");
        let gif89 = temp_file("b.gif", b"GIF89a\x01\x00\x01\x00\x00\x00\x00");
        assert_eq!(
            detect(&SourceBytes::from_path(&jpeg).unwrap()).unwrap(),
            Format::Jpeg
        );
        assert_eq!(
            detect(&SourceBytes::from_path(&tiff).unwrap()).unwrap(),
            Format::Tiff
        );
        assert_eq!(
            detect(&SourceBytes::from_path(&png).unwrap()).unwrap(),
            Format::Png
        );
        assert_eq!(
            detect(&SourceBytes::from_path(&webp).unwrap()).unwrap(),
            Format::Webp
        );
        assert_eq!(
            detect(&SourceBytes::from_path(&wav).unwrap()).unwrap(),
            Format::Wav
        );
        assert_eq!(
            detect(&SourceBytes::from_path(&heif).unwrap()).unwrap(),
            Format::Heif
        );
        assert_eq!(
            detect(&SourceBytes::from_path(&mp4).unwrap()).unwrap(),
            Format::Mp4
        );
        assert_eq!(
            detect(&SourceBytes::from_path(&mov).unwrap()).unwrap(),
            Format::Mov
        );
        assert_eq!(
            detect(&SourceBytes::from_path(&m4a).unwrap()).unwrap(),
            Format::M4a
        );
        assert_eq!(
            detect(&SourceBytes::from_path(&m4b).unwrap()).unwrap(),
            Format::M4a
        );
        assert_eq!(
            detect(&SourceBytes::from_path(&m4p).unwrap()).unwrap(),
            Format::M4a
        );
        assert_eq!(
            detect(&SourceBytes::from_path(&flac).unwrap()).unwrap(),
            Format::Flac
        );
        assert_eq!(
            detect(&SourceBytes::from_path(&aiff).unwrap()).unwrap(),
            Format::Aiff
        );
        assert_eq!(
            detect(&SourceBytes::from_path(&aifc).unwrap()).unwrap(),
            Format::Aiff
        );
        assert_eq!(
            detect(&SourceBytes::from_path(&ogg).unwrap()).unwrap(),
            Format::Ogg
        );
        assert_eq!(
            detect(&SourceBytes::from_path(&mp3_id3).unwrap()).unwrap(),
            Format::Mp3
        );
        assert_eq!(
            detect(&SourceBytes::from_path(&mp3_sync).unwrap()).unwrap(),
            Format::Mp3
        );
        assert_eq!(
            detect(&SourceBytes::from_path(&gif87).unwrap()).unwrap(),
            Format::Gif
        );
        assert_eq!(
            detect(&SourceBytes::from_path(&gif89).unwrap()).unwrap(),
            Format::Gif
        );
        let _ = fs::remove_file(mp3_id3);
        let _ = fs::remove_file(mp3_sync);
        let _ = fs::remove_file(gif87);
        let _ = fs::remove_file(gif89);
        let _ = fs::remove_file(jpeg);
        let _ = fs::remove_file(tiff);
        let _ = fs::remove_file(png);
        let _ = fs::remove_file(webp);
        let _ = fs::remove_file(wav);
        let _ = fs::remove_file(heif);
        let _ = fs::remove_file(mp4);
        let _ = fs::remove_file(mov);
        let _ = fs::remove_file(m4a);
        let _ = fs::remove_file(m4b);
        let _ = fs::remove_file(m4p);
        let _ = fs::remove_file(flac);
        let _ = fs::remove_file(aiff);
        let _ = fs::remove_file(aifc);
        let _ = fs::remove_file(ogg);
    }

    /// Build a minimal little-endian TIFF with a single IFD0 entry carrying `tag`.
    fn tiff_with_tag(tag: u16) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"II*\0");
        out.extend_from_slice(&8u32.to_le_bytes()); // IFD0 at offset 8
        out.extend_from_slice(&1u16.to_le_bytes()); // one entry
        out.extend_from_slice(&tag.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // type = BYTE
        out.extend_from_slice(&4u32.to_le_bytes()); // count = 4
        out.extend_from_slice(&[0x01, 0x04, 0x00, 0x00]); // inline value
        out.extend_from_slice(&0u32.to_le_bytes()); // next IFD = 0
        out
    }

    /// Build a minimal little-endian CR2-shaped TIFF: II*\0 header, IFD0 offset
    /// 0x10, Canon marker `CR\x02\x00` at bytes 8..12, then a single IFD0 entry.
    fn cr2_with_tag(tag: u16) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"II*\0");
        out.extend_from_slice(&0x10u32.to_le_bytes()); // IFD0 at offset 16
        out.extend_from_slice(b"CR\x02\x00"); // Canon CR2 marker
        out.extend_from_slice(&1u16.to_le_bytes()); // one entry
        out.extend_from_slice(&tag.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // type = BYTE
        out.extend_from_slice(&4u32.to_le_bytes()); // count = 4
        out.extend_from_slice(&[0x01, 0x04, 0x00, 0x00]); // inline value
        out.extend_from_slice(&0u32.to_le_bytes()); // next IFD = 0
        out
    }

    #[test]
    fn detects_cr2_when_canon_marker_present() {
        let cr2_bytes = cr2_with_tag(0x010F);
        let path = temp_file("a.cr2", &cr2_bytes);
        assert_eq!(
            detect(&SourceBytes::from_path(&path).unwrap()).unwrap(),
            Format::Cr2
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn does_not_classify_dng_as_cr2() {
        // DNG files have IFD0 at offset 8 and no Canon marker — must stay DNG.
        let dng_bytes = tiff_with_tag(0xC612);
        let path = temp_file("a.dng", &dng_bytes);
        assert_eq!(
            detect(&SourceBytes::from_path(&path).unwrap()).unwrap(),
            Format::Dng
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn does_not_classify_plain_tiff_as_cr2_when_marker_missing() {
        // IFD0 offset 0x10 alone is not enough — bytes 8..12 must match.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"II*\0");
        bytes.extend_from_slice(&0x10u32.to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 0, 0]); // not the Canon marker
        bytes.extend_from_slice(&0u16.to_le_bytes()); // 0 entries
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let path = temp_file("a.tif", &bytes);
        assert_eq!(
            detect(&SourceBytes::from_path(&path).unwrap()).unwrap(),
            Format::Tiff
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn detects_dng_when_dng_version_present() {
        let dng_bytes = tiff_with_tag(0xC612);
        let plain_tiff_bytes = tiff_with_tag(0x010F);
        let dng_path = temp_file("a.dng", &dng_bytes);
        let tiff_path = temp_file("a.tif", &plain_tiff_bytes);
        assert_eq!(
            detect(&SourceBytes::from_path(&dng_path).unwrap()).unwrap(),
            Format::Dng
        );
        assert_eq!(
            detect(&SourceBytes::from_path(&tiff_path).unwrap()).unwrap(),
            Format::Tiff
        );
        let _ = fs::remove_file(dng_path);
        let _ = fs::remove_file(tiff_path);
    }

    #[test]
    fn detects_avif_from_major_brand() {
        let path = temp_file("a.avif", b"\x00\x00\x00\x18ftypavif\0\0\0\0mif1");
        assert_eq!(
            detect(&SourceBytes::from_path(&path).unwrap()).unwrap(),
            Format::Avif
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn detects_avis_from_major_brand() {
        let path = temp_file("a.avif", b"\x00\x00\x00\x18ftypavis\0\0\0\0msf1");
        assert_eq!(
            detect(&SourceBytes::from_path(&path).unwrap()).unwrap(),
            Format::Avif
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn detects_avif_from_compat_brand() {
        let path = temp_file("a.avif", b"\x00\x00\x00\x18ftypmif1\0\0\0\0avif");
        assert_eq!(
            detect(&SourceBytes::from_path(&path).unwrap()).unwrap(),
            Format::Avif
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn detects_avif_when_heic_major_but_avif_compat() {
        // Ordering invariant: AVIF wins on any AVIF brand, even when the
        // major brand is `heic` and would otherwise classify as HEIF.
        let path = temp_file("a.avif", b"\x00\x00\x00\x1cftypheic\0\0\0\0avifmif1");
        assert_eq!(
            detect(&SourceBytes::from_path(&path).unwrap()).unwrap(),
            Format::Avif
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn detects_heif_from_pure_heic() {
        // No AVIF brand anywhere: stays HEIF.
        let path = temp_file("a.heic", b"\x00\x00\x00\x1cftypheic\0\0\0\0mif1heic");
        assert_eq!(
            detect(&SourceBytes::from_path(&path).unwrap()).unwrap(),
            Format::Heif
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn malformed_tiff_ifd_offset_falls_back_to_tiff() {
        // IFD0 offset points past the buffer — detect must not return Dng and must not panic.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"II*\0");
        bytes.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        let path = temp_file("a.tif", &bytes);
        assert_eq!(
            detect(&SourceBytes::from_path(&path).unwrap()).unwrap(),
            Format::Tiff
        );
        let _ = fs::remove_file(path);
    }
}
