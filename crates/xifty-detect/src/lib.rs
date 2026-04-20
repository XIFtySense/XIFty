use xifty_core::{Format, XiftyError};
use xifty_source::SourceBytes;

pub fn detect(source: &SourceBytes) -> Result<Format, XiftyError> {
    let bytes = source.bytes();
    if bytes.len() >= 4 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
        return Ok(Format::Jpeg);
    }

    if bytes.len() >= 4 && (&bytes[0..4] == b"II*\0" || &bytes[0..4] == b"MM\0*") {
        return Ok(Format::Tiff);
    }

    if bytes.len() >= 8 && &bytes[0..8] == b"\x89PNG\r\n\x1a\n" {
        return Ok(Format::Png);
    }

    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Ok(Format::Webp);
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

    if bytes.len() >= 16 && &bytes[4..8] == b"ftyp" {
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
    matches!(
        &brand,
        b"mif1" | b"msf1" | b"heic" | b"heix" | b"hevc" | b"heim" | b"heis" | b"avif" | b"avis"
    )
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
        let heif = temp_file("a.heic", b"\x00\x00\x00\x18ftypheic\0\0\0\0mif1");
        let mp4 = temp_file("a.mp4", b"\x00\x00\x00\x18ftypisom\0\0\0\0mp42");
        let mov = temp_file("a.mov", b"\x00\x00\x00\x14ftypqt  \0\0\0\0");
        let m4a = temp_file("a.m4a", b"\x00\x00\x00\x18ftypM4A \0\0\0\0mp42");
        let m4b = temp_file("a.m4b", b"\x00\x00\x00\x18ftypM4B \0\0\0\0mp42");
        let m4p = temp_file("a.m4p", b"\x00\x00\x00\x18ftypM4P \0\0\0\0mp42");
        let flac = temp_file("a.flac", b"fLaC\x00\x00\x00\x22");
        let aiff = temp_file("a.aiff", b"FORM\x00\x00\x00\x04AIFF");
        let aifc = temp_file("a.aifc", b"FORM\x00\x00\x00\x04AIFC");
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
        let _ = fs::remove_file(jpeg);
        let _ = fs::remove_file(tiff);
        let _ = fs::remove_file(png);
        let _ = fs::remove_file(webp);
        let _ = fs::remove_file(heif);
        let _ = fs::remove_file(mp4);
        let _ = fs::remove_file(mov);
        let _ = fs::remove_file(m4a);
        let _ = fs::remove_file(m4b);
        let _ = fs::remove_file(m4p);
        let _ = fs::remove_file(flac);
        let _ = fs::remove_file(aiff);
        let _ = fs::remove_file(aifc);
    }
}
