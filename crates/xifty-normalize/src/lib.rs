use xifty_core::{MetadataEntry, NormalizedField, Provenance, TypedValue};

pub fn normalize(entries: &[MetadataEntry]) -> Vec<NormalizedField> {
    let mut fields = Vec::new();

    maybe_push_string(
        entries,
        "DateTimeOriginal",
        "captured_at",
        &mut fields,
        true,
    );
    maybe_push_string(entries, "Make", "device.make", &mut fields, false);
    maybe_push_string(entries, "Model", "device.model", &mut fields, false);
    maybe_push_string(entries, "Software", "software", &mut fields, false);
    maybe_push_integer(entries, "Orientation", "orientation", &mut fields);

    let width = entry_integer(entries, "ImageWidth");
    let height = entry_integer(entries, "ImageHeight");
    if let (Some((width_entry, width_value)), Some((height_entry, height_value))) = (width, height)
    {
        fields.push(NormalizedField {
            field: "dimensions.width".into(),
            value: TypedValue::Integer(width_value),
            confidence: 0.95,
            sources: vec![width_entry.provenance.clone()],
            notes: Vec::new(),
        });
        fields.push(NormalizedField {
            field: "dimensions.height".into(),
            value: TypedValue::Integer(height_value),
            confidence: 0.95,
            sources: vec![height_entry.provenance.clone()],
            notes: Vec::new(),
        });
    }

    let lat = gps_coordinate(entries, "GPSLatitude", "GPSLatitudeRef");
    let lon = gps_coordinate(entries, "GPSLongitude", "GPSLongitudeRef");
    if let (Some((lat_src, latitude)), Some((lon_src, longitude))) = (lat, lon) {
        fields.push(NormalizedField {
            field: "location".into(),
            value: TypedValue::Coordinates {
                latitude,
                longitude,
            },
            confidence: 0.9,
            sources: vec![lat_src, lon_src],
            notes: Vec::new(),
        });
    }

    fields
}

fn maybe_push_string(
    entries: &[MetadataEntry],
    tag_name: &str,
    field_name: &str,
    fields: &mut Vec<NormalizedField>,
    timestamp: bool,
) {
    if let Some(entry) = entries.iter().find(|entry| entry.tag_name == tag_name) {
        if let TypedValue::String(text) | TypedValue::Timestamp(text) = &entry.value {
            let value = if timestamp {
                TypedValue::Timestamp(normalize_exif_datetime(text))
            } else {
                TypedValue::String(text.clone())
            };
            fields.push(NormalizedField {
                field: field_name.into(),
                value,
                confidence: 0.95,
                sources: vec![entry.provenance.clone()],
                notes: Vec::new(),
            });
        }
    }
}

fn maybe_push_integer(
    entries: &[MetadataEntry],
    tag_name: &str,
    field_name: &str,
    fields: &mut Vec<NormalizedField>,
) {
    if let Some((entry, value)) = entry_integer(entries, tag_name) {
        fields.push(NormalizedField {
            field: field_name.into(),
            value: TypedValue::Integer(value),
            confidence: 0.95,
            sources: vec![entry.provenance.clone()],
            notes: Vec::new(),
        });
    }
}

fn entry_integer<'a>(
    entries: &'a [MetadataEntry],
    tag_name: &str,
) -> Option<(&'a MetadataEntry, i64)> {
    let entry = entries.iter().find(|entry| entry.tag_name == tag_name)?;
    match entry.value {
        TypedValue::Integer(value) => Some((entry, value)),
        _ => None,
    }
}

fn gps_coordinate(
    entries: &[MetadataEntry],
    coord_tag: &str,
    ref_tag: &str,
) -> Option<(Provenance, f64)> {
    let coord = entries.iter().find(|entry| entry.tag_name == coord_tag)?;
    let dir = entries.iter().find(|entry| entry.tag_name == ref_tag)?;
    let direction = match &dir.value {
        TypedValue::String(value) => value.as_str(),
        _ => return None,
    };
    let values = match &coord.value {
        TypedValue::RationalList(values) if values.len() == 3 => values,
        _ => return None,
    };
    let decimal = rational_triplet_to_decimal(values)?;
    let signed = if matches!(direction, "S" | "W") {
        -decimal
    } else {
        decimal
    };
    Some((coord.provenance.clone(), signed))
}

fn rational_triplet_to_decimal(values: &[xifty_core::RationalValue]) -> Option<f64> {
    if values.len() != 3 {
        return None;
    }
    let degrees = rational_to_f64(&values[0])?;
    let minutes = rational_to_f64(&values[1])?;
    let seconds = rational_to_f64(&values[2])?;
    Some(degrees + (minutes / 60.0) + (seconds / 3600.0))
}

fn rational_to_f64(value: &xifty_core::RationalValue) -> Option<f64> {
    if value.denominator == 0 {
        return None;
    }
    Some(value.numerator as f64 / value.denominator as f64)
}

pub fn normalize_exif_datetime(input: &str) -> String {
    if input.len() >= 19 {
        let bytes = input.as_bytes();
        if bytes[4] == b':' && bytes[7] == b':' && bytes[10] == b' ' {
            return format!(
                "{}-{}-{}T{}",
                &input[0..4],
                &input[5..7],
                &input[8..10],
                &input[11..19]
            );
        }
    }
    input.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use xifty_core::{MetadataEntry, Provenance, TypedValue};

    fn prov() -> Provenance {
        Provenance {
            container: "jpeg".into(),
            namespace: "exif".into(),
            path: None,
            offset_start: None,
            offset_end: None,
            notes: Vec::new(),
        }
    }

    #[test]
    fn normalizes_capture_time() {
        let entries = vec![MetadataEntry {
            namespace: "exif".into(),
            tag_id: "0x9003".into(),
            tag_name: "DateTimeOriginal".into(),
            value: TypedValue::Timestamp("2024:04:16 12:34:56".into()),
            provenance: prov(),
            notes: Vec::new(),
        }];
        let fields = normalize(&entries);
        assert_eq!(fields[0].field, "captured_at");
    }
}
