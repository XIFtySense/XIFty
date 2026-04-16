use xifty_core::{MetadataEntry, Provenance, TypedValue};

#[derive(Debug, Clone)]
pub struct RtmdPacket<'a> {
    pub bytes: &'a [u8],
    pub container: &'a str,
    pub offset_start: u64,
    pub offset_end: u64,
}

pub fn decode_packet(packet: RtmdPacket<'_>) -> Vec<MetadataEntry> {
    let text = match std::str::from_utf8(packet.bytes) {
        Ok(text) => text,
        Err(_) => return Vec::new(),
    };

    if !text.contains("<NonRealTimeMeta") {
        return Vec::new();
    }

    let mut entries = Vec::new();
    push_timestamp(
        &mut entries,
        packet.clone(),
        "MetadataDate",
        "MetadataDate",
        tag_attr(text, "NonRealTimeMeta", "lastUpdate"),
        "decoded from NonRealTimeMeta lastUpdate",
    );
    push_timestamp(
        &mut entries,
        packet.clone(),
        "CreateDate",
        "CreateDate",
        tag_attr(text, "CreationDate", "value"),
        "decoded from NonRealTimeMeta CreationDate",
    );
    push_string(
        &mut entries,
        packet.clone(),
        "Make",
        "Make",
        tag_attr(text, "Device", "manufacturer"),
        "decoded from NonRealTimeMeta Device manufacturer",
    );
    push_string(
        &mut entries,
        packet.clone(),
        "Model",
        "Model",
        tag_attr(text, "Device", "modelName"),
        "decoded from NonRealTimeMeta Device modelName",
    );
    push_string(
        &mut entries,
        packet.clone(),
        "CaptureFps",
        "CaptureFps",
        tag_attr(text, "VideoFrame", "captureFps"),
        "decoded from NonRealTimeMeta VideoFrame captureFps",
    );
    push_string(
        &mut entries,
        packet.clone(),
        "FormatFps",
        "FormatFps",
        tag_attr(text, "VideoFrame", "formatFps"),
        "decoded from NonRealTimeMeta VideoFrame formatFps",
    );
    push_string(
        &mut entries,
        packet.clone(),
        "VideoCodecProfile",
        "VideoCodecProfile",
        tag_attr(text, "VideoFrame", "videoCodec"),
        "decoded from NonRealTimeMeta VideoFrame videoCodec",
    );
    push_integer(
        &mut entries,
        packet.clone(),
        "ImageWidth",
        "ImageWidth",
        tag_attr(text, "VideoLayout", "pixel"),
        "decoded from NonRealTimeMeta VideoLayout pixel",
    );
    push_integer(
        &mut entries,
        packet.clone(),
        "ImageHeight",
        "ImageHeight",
        tag_attr(text, "VideoLayout", "numOfVerticalLine"),
        "decoded from NonRealTimeMeta VideoLayout numOfVerticalLine",
    );
    push_string(
        &mut entries,
        packet.clone(),
        "AspectRatio",
        "AspectRatio",
        tag_attr(text, "VideoLayout", "aspectRatio"),
        "decoded from NonRealTimeMeta VideoLayout aspectRatio",
    );
    push_integer(
        &mut entries,
        packet.clone(),
        "AudioChannels",
        "AudioChannels",
        tag_attr(text, "AudioFormat", "numOfChannel"),
        "decoded from NonRealTimeMeta AudioFormat numOfChannel",
    );
    push_string(
        &mut entries,
        packet.clone(),
        "AudioCodecName",
        "AudioCodecName",
        tag_attr(text, "AudioRecPort", "audioCodec"),
        "decoded from NonRealTimeMeta AudioRecPort audioCodec",
    );
    push_string(
        &mut entries,
        packet.clone(),
        "RecordingMode",
        "RecordingMode",
        tag_attr(text, "RecordingMode", "type"),
        "decoded from NonRealTimeMeta RecordingMode type",
    );
    push_string(
        &mut entries,
        packet,
        "CaptureGammaEquation",
        "CaptureGammaEquation",
        acquisition_item_value(text, "CaptureGammaEquation"),
        "decoded from NonRealTimeMeta AcquisitionRecord item",
    );

    entries
}

fn tag_attr(text: &str, tag_name: &str, attr_name: &str) -> Option<String> {
    let start = text.find(&format!("<{tag_name}"))?;
    let rest = &text[start..];
    let end = rest.find('>')?;
    attr_value(&rest[..end], attr_name)
}

fn acquisition_item_value(text: &str, item_name: &str) -> Option<String> {
    let item = find_with_attr(text, "Item", "name", item_name)?;
    attr_value(item, "value")
}

fn find_with_attr<'a>(
    text: &'a str,
    tag_name: &str,
    attr_name: &str,
    attr_match: &str,
) -> Option<&'a str> {
    let mut rest = text;
    loop {
        let start = rest.find(&format!("<{tag_name}"))?;
        rest = &rest[start..];
        let end = rest.find('>')?;
        let candidate = &rest[..end];
        if attr_value(candidate, attr_name).as_deref() == Some(attr_match) {
            return Some(candidate);
        }
        rest = &rest[end..];
    }
}

fn attr_value(tag: &str, attr_name: &str) -> Option<String> {
    let needle = format!("{attr_name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn provenance(packet: &RtmdPacket<'_>) -> Provenance {
    Provenance {
        container: packet.container.into(),
        namespace: "rtmd".into(),
        path: Some("non_real_time_meta".into()),
        offset_start: Some(packet.offset_start),
        offset_end: Some(packet.offset_end),
        notes: Vec::new(),
    }
}

fn push_string(
    entries: &mut Vec<MetadataEntry>,
    packet: RtmdPacket<'_>,
    tag_id: &str,
    tag_name: &str,
    value: Option<String>,
    note: &str,
) {
    let Some(value) = value else {
        return;
    };
    entries.push(MetadataEntry {
        namespace: "rtmd".into(),
        tag_id: tag_id.into(),
        tag_name: tag_name.into(),
        value: TypedValue::String(value),
        provenance: provenance(&packet),
        notes: vec![note.into()],
    });
}

fn push_timestamp(
    entries: &mut Vec<MetadataEntry>,
    packet: RtmdPacket<'_>,
    tag_id: &str,
    tag_name: &str,
    value: Option<String>,
    note: &str,
) {
    let Some(value) = value else {
        return;
    };
    entries.push(MetadataEntry {
        namespace: "rtmd".into(),
        tag_id: tag_id.into(),
        tag_name: tag_name.into(),
        value: TypedValue::Timestamp(value),
        provenance: provenance(&packet),
        notes: vec![note.into()],
    });
}

fn push_integer(
    entries: &mut Vec<MetadataEntry>,
    packet: RtmdPacket<'_>,
    tag_id: &str,
    tag_name: &str,
    value: Option<String>,
    note: &str,
) {
    let Some(value) = value else {
        return;
    };
    let Ok(parsed) = value.parse::<i64>() else {
        return;
    };
    entries.push(MetadataEntry {
        namespace: "rtmd".into(),
        tag_id: tag_id.into(),
        tag_name: tag_name.into(),
        value: TypedValue::Integer(parsed),
        provenance: provenance(&packet),
        notes: vec![note.into()],
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_sony_non_real_time_meta() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<NonRealTimeMeta lastUpdate="2026-04-16T06:34:34-08:00">
  <CreationDate value="2026-04-16T06:34:34-08:00"/>
  <VideoFormat>
    <VideoFrame videoCodec="AVC_3840_2160_HP@L51" captureFps="23.98p" formatFps="23.98p"/>
    <VideoLayout pixel="3840" numOfVerticalLine="2160" aspectRatio="16:9"/>
  </VideoFormat>
  <AudioFormat numOfChannel="2">
    <AudioRecPort audioCodec="LPCM16"/>
  </AudioFormat>
  <Device manufacturer="Sony" modelName="ZV-E10"/>
  <RecordingMode type="normal"/>
  <AcquisitionRecord>
    <Group name="CameraUnitMetadataSet">
      <Item name="CaptureGammaEquation" value="rec709-xvycc"/>
    </Group>
  </AcquisitionRecord>
</NonRealTimeMeta>"#;
        let entries = decode_packet(RtmdPacket {
            bytes: xml,
            container: "mp4",
            offset_start: 0,
            offset_end: xml.len() as u64,
        });
        assert!(entries.iter().any(|entry| entry.tag_name == "Make"));
        assert!(entries.iter().any(|entry| entry.tag_name == "Model"));
        assert!(entries.iter().any(|entry| entry.tag_name == "CaptureFps"));
        assert!(
            entries
                .iter()
                .any(|entry| entry.tag_name == "CaptureGammaEquation")
        );
    }
}
