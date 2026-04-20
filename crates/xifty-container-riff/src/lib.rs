use xifty_core::{ContainerNode, Issue, Severity, XiftyError, issue};
use xifty_source::{Cursor, Endian, SourceBytes};

#[derive(Debug, Clone)]
pub struct RiffChunk {
    pub chunk_id: [u8; 4],
    pub offset_start: u64,
    pub offset_end: u64,
    pub data_offset: u64,
    pub data_length: u32,
}

#[derive(Debug, Clone)]
pub struct RiffContainer {
    pub form_type: [u8; 4],
    pub nodes: Vec<ContainerNode>,
    pub chunks: Vec<RiffChunk>,
    pub issues: Vec<Issue>,
}

impl RiffContainer {
    pub fn exif_payloads(&self) -> impl Iterator<Item = &RiffChunk> {
        self.chunks
            .iter()
            .filter(|chunk| &chunk.chunk_id == b"EXIF")
    }

    pub fn xmp_payloads(&self) -> impl Iterator<Item = &RiffChunk> {
        self.chunks
            .iter()
            .filter(|chunk| &chunk.chunk_id == b"XMP ")
    }

    pub fn icc_payloads(&self) -> impl Iterator<Item = &RiffChunk> {
        self.chunks
            .iter()
            .filter(|chunk| &chunk.chunk_id == b"ICCP")
    }

    pub fn iptc_payloads(&self) -> impl Iterator<Item = &RiffChunk> {
        self.chunks
            .iter()
            .filter(|chunk| &chunk.chunk_id == b"IPTC")
    }
}

pub fn parse(source: &SourceBytes) -> Result<RiffContainer, XiftyError> {
    parse_bytes(source.bytes(), 0)
}

pub fn parse_bytes(bytes: &[u8], base_offset: u64) -> Result<RiffContainer, XiftyError> {
    let cursor = Cursor::new(bytes, base_offset);
    if cursor.len() < 12 || cursor.slice(0, 4)? != b"RIFF" {
        return Err(XiftyError::Parse {
            message: "not a riff container".into(),
        });
    }

    let riff_size = cursor.read_u32(4, Endian::Little)? as usize;
    let form_type_bytes = cursor.slice(8, 4)?;
    let form_type = [
        form_type_bytes[0],
        form_type_bytes[1],
        form_type_bytes[2],
        form_type_bytes[3],
    ];
    let mut issues = Vec::new();
    if cursor.len() >= 8 && riff_size + 8 != cursor.len() {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "riff_size_mismatch".into(),
            message: format!(
                "riff declared size {} does not match actual size {}",
                riff_size + 8,
                cursor.len()
            ),
            offset: Some(base_offset + 4),
            context: Some(String::from_utf8_lossy(&form_type).into_owned()),
        });
    }

    let root_label = if &form_type == b"WEBP" {
        "webp"
    } else {
        "riff"
    };
    let mut nodes = vec![ContainerNode {
        kind: "container".into(),
        label: root_label.into(),
        offset_start: base_offset,
        offset_end: base_offset + bytes.len() as u64,
        parent_label: None,
    }];
    let mut chunks = Vec::new();
    let mut offset = 12usize;

    while offset < cursor.len() {
        if offset + 8 > cursor.len() {
            issues.push(Issue {
                severity: Severity::Warning,
                code: "riff_chunk_header_out_of_bounds".into(),
                message: "truncated riff chunk header".into(),
                offset: Some(cursor.absolute_offset(offset)),
                context: None,
            });
            break;
        }

        let chunk_id_bytes = cursor.slice(offset, 4)?;
        let chunk_id = [
            chunk_id_bytes[0],
            chunk_id_bytes[1],
            chunk_id_bytes[2],
            chunk_id_bytes[3],
        ];
        let data_length = cursor.read_u32(offset + 4, Endian::Little)? as usize;
        let padded_length = data_length + (data_length % 2);
        let chunk_end = offset + 8 + padded_length;
        if chunk_end > cursor.len() {
            issues.push(Issue {
                severity: Severity::Warning,
                code: "riff_chunk_length_invalid".into(),
                message: format!(
                    "riff chunk {} exceeds available bytes",
                    String::from_utf8_lossy(&chunk_id)
                ),
                offset: Some(cursor.absolute_offset(offset)),
                context: Some(String::from_utf8_lossy(&chunk_id).into_owned()),
            });
            break;
        }

        let label = String::from_utf8_lossy(&chunk_id).into_owned();
        nodes.push(ContainerNode {
            kind: "chunk".into(),
            label: label.clone(),
            offset_start: cursor.absolute_offset(offset),
            offset_end: cursor.absolute_offset(chunk_end),
            parent_label: Some(root_label.into()),
        });
        chunks.push(RiffChunk {
            chunk_id,
            offset_start: cursor.absolute_offset(offset),
            offset_end: cursor.absolute_offset(chunk_end),
            data_offset: cursor.absolute_offset(offset + 8),
            data_length: data_length as u32,
        });

        offset = chunk_end;
    }

    if &form_type != b"WEBP" {
        issues.push(issue(
            Severity::Info,
            "riff_non_webp_form",
            format!(
                "riff container form type {} is not WEBP",
                String::from_utf8_lossy(&form_type)
            ),
        ));
    }

    Ok(RiffContainer {
        form_type,
        nodes,
        chunks,
        issues,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_webp_riff() {
        let bytes = [b'R', b'I', b'F', b'F', 4, 0, 0, 0, b'W', b'E', b'B', b'P'];
        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert_eq!(&parsed.form_type, b"WEBP");
    }

    #[test]
    fn routes_iccp_chunks() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&12u32.to_le_bytes());
        bytes.extend_from_slice(b"WEBP");
        bytes.extend_from_slice(b"ICCP");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert!(parsed.icc_payloads().next().is_some());
    }

    #[test]
    fn routes_iptc_chunks() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&12u32.to_le_bytes());
        bytes.extend_from_slice(b"WEBP");
        bytes.extend_from_slice(b"IPTC");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let parsed = parse_bytes(&bytes, 0).unwrap();
        assert!(parsed.iptc_payloads().next().is_some());
    }
}
