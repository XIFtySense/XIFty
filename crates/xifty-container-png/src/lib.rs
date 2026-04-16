use xifty_core::{ContainerNode, Issue, Severity, XiftyError, issue};
use xifty_source::{Cursor, Endian, SourceBytes};

#[derive(Debug, Clone)]
pub struct PngChunk {
    pub chunk_type: [u8; 4],
    pub offset_start: u64,
    pub offset_end: u64,
    pub data_offset: u64,
    pub data_length: u32,
}

#[derive(Debug, Clone)]
pub struct PngContainer {
    pub nodes: Vec<ContainerNode>,
    pub chunks: Vec<PngChunk>,
    pub issues: Vec<Issue>,
}

impl PngContainer {
    pub fn exif_payloads(&self) -> impl Iterator<Item = &PngChunk> {
        self.chunks
            .iter()
            .filter(|chunk| &chunk.chunk_type == b"eXIf")
    }

    pub fn xmp_payloads(&self) -> impl Iterator<Item = &PngChunk> {
        self.chunks
            .iter()
            .filter(|chunk| &chunk.chunk_type == b"iTXt" || &chunk.chunk_type == b"tEXt")
    }
}

pub fn parse(source: &SourceBytes) -> Result<PngContainer, XiftyError> {
    parse_bytes(source.bytes(), 0)
}

pub fn parse_bytes(bytes: &[u8], base_offset: u64) -> Result<PngContainer, XiftyError> {
    let cursor = Cursor::new(bytes, base_offset);
    if cursor.len() < 8 || cursor.slice(0, 8)? != b"\x89PNG\r\n\x1a\n" {
        return Err(XiftyError::Parse {
            message: "not a png".into(),
        });
    }

    let mut nodes = vec![ContainerNode {
        kind: "container".into(),
        label: "png".into(),
        offset_start: base_offset,
        offset_end: base_offset + bytes.len() as u64,
        parent_label: None,
    }];
    let mut chunks = Vec::new();
    let mut issues = Vec::new();
    let mut offset = 8usize;

    while offset < cursor.len() {
        if offset + 12 > cursor.len() {
            issues.push(Issue {
                severity: Severity::Warning,
                code: "png_chunk_header_out_of_bounds".into(),
                message: "truncated png chunk header".into(),
                offset: Some(cursor.absolute_offset(offset)),
                context: None,
            });
            break;
        }

        let length = cursor.read_u32(offset, Endian::Big)? as usize;
        let chunk_type_bytes = cursor.slice(offset + 4, 4)?;
        let chunk_type = [
            chunk_type_bytes[0],
            chunk_type_bytes[1],
            chunk_type_bytes[2],
            chunk_type_bytes[3],
        ];
        let chunk_end = offset + 12 + length;
        if chunk_end > cursor.len() {
            issues.push(Issue {
                severity: Severity::Warning,
                code: "png_chunk_length_invalid".into(),
                message: format!(
                    "png chunk {} exceeds available bytes",
                    String::from_utf8_lossy(&chunk_type)
                ),
                offset: Some(cursor.absolute_offset(offset)),
                context: Some(String::from_utf8_lossy(&chunk_type).into_owned()),
            });
            break;
        }

        let label = String::from_utf8_lossy(&chunk_type).into_owned();
        nodes.push(ContainerNode {
            kind: "chunk".into(),
            label: label.clone(),
            offset_start: cursor.absolute_offset(offset),
            offset_end: cursor.absolute_offset(chunk_end),
            parent_label: Some("png".into()),
        });
        chunks.push(PngChunk {
            chunk_type,
            offset_start: cursor.absolute_offset(offset),
            offset_end: cursor.absolute_offset(chunk_end),
            data_offset: cursor.absolute_offset(offset + 8),
            data_length: length as u32,
        });

        offset = chunk_end;
        if &chunk_type == b"IEND" {
            break;
        }
    }

    if !chunks.iter().any(|chunk| &chunk.chunk_type == b"IEND") {
        issues.push(issue(
            Severity::Warning,
            "png_missing_iend",
            "png stream ended without IEND chunk",
        ));
    }

    Ok(PngContainer {
        nodes,
        chunks,
        issues,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_png() {
        let bytes = [
            0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0, b'I', b'E', b'N', b'D', 0,
            0, 0, 0,
        ];
        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert_eq!(parsed.chunks.len(), 1);
        assert_eq!(&parsed.chunks[0].chunk_type, b"IEND");
    }
}
