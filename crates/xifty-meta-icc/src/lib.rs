use xifty_core::MetadataEntry;

#[derive(Debug, Clone)]
pub struct IccPayload<'a> {
    pub bytes: &'a [u8],
    pub container: &'a str,
    pub path: &'a str,
    pub offset_start: u64,
    pub offset_end: u64,
}

pub fn decode_payload(_payload: IccPayload<'_>) -> Vec<MetadataEntry> {
    Vec::new()
}

pub fn supported_tags() -> &'static [&'static str] {
    &[]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_no_entries_for_stub_decoder() {
        let entries = decode_payload(IccPayload {
            bytes: &[],
            container: "jpeg",
            path: "icc_profile",
            offset_start: 0,
            offset_end: 0,
        });
        assert!(entries.is_empty());
        assert!(supported_tags().is_empty());
    }
}
