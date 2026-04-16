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

    Err(XiftyError::UnsupportedFormat)
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
        assert_eq!(
            detect(&SourceBytes::from_path(&jpeg).unwrap()).unwrap(),
            Format::Jpeg
        );
        assert_eq!(
            detect(&SourceBytes::from_path(&tiff).unwrap()).unwrap(),
            Format::Tiff
        );
        let _ = fs::remove_file(jpeg);
        let _ = fs::remove_file(tiff);
    }
}
