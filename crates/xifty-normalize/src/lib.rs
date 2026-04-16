use xifty_core::{MetadataEntry, NormalizedField, Provenance, TypedValue};
use xifty_policy::PolicyResult;

pub fn normalize(entries: &[MetadataEntry]) -> Vec<NormalizedField> {
    normalize_with_policy(entries).fields
}

pub fn normalize_with_policy(entries: &[MetadataEntry]) -> PolicyResult {
    let mut result = xifty_policy::reconcile(entries);

    enrich_exif_timestamps(entries, &mut result.fields);

    let width = entry_integer(entries, &["ImageWidth", "ExifImageWidth"]);
    let height = entry_integer(entries, &["ImageHeight", "ExifImageHeight"]);
    if let (Some((width_entry, width_value)), Some((height_entry, height_value))) = (width, height)
    {
        ensure_field(
            &mut result.fields,
            "dimensions.width",
            width_value,
            &width_entry.provenance,
        );
        ensure_field(
            &mut result.fields,
            "dimensions.height",
            height_value,
            &height_entry.provenance,
        );
    }

    let coordinates = gps_coordinate(entries, "GPSLatitude", "GPSLatitudeRef")
        .zip(gps_coordinate(entries, "GPSLongitude", "GPSLongitudeRef"))
        .or_else(|| {
            string_coordinate(entries, "GPSLatitude")
                .zip(string_coordinate(entries, "GPSLongitude"))
        });
    if let Some(((lat_src, latitude), (lon_src, longitude))) = coordinates {
        result.fields.push(NormalizedField {
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

    result
}

fn enrich_exif_timestamps(entries: &[MetadataEntry], fields: &mut [NormalizedField]) {
    enrich_timestamp_field(
        entries,
        fields,
        "captured_at",
        "DateTimeOriginal",
        "SubSecTimeOriginal",
        "OffsetTimeOriginal",
    );
    enrich_timestamp_field(
        entries,
        fields,
        "created_at",
        "CreateDate",
        "SubSecTimeDigitized",
        "OffsetTimeDigitized",
    );
    enrich_timestamp_field(
        entries,
        fields,
        "modified_at",
        "ModifyDate",
        "SubSecTime",
        "OffsetTime",
    );
}

fn enrich_timestamp_field(
    entries: &[MetadataEntry],
    fields: &mut [NormalizedField],
    field_name: &str,
    base_tag: &str,
    subsec_tag: &str,
    offset_tag: &str,
) {
    let Some(field) = fields.iter_mut().find(|field| field.field == field_name) else {
        return;
    };
    let Some(base_value) = entry_string(entries, base_tag) else {
        return;
    };
    let subsec = entry_string(entries, subsec_tag);
    let offset = entry_string(entries, offset_tag);
    let Some(enriched) = compose_exif_timestamp(base_value, subsec, offset) else {
        return;
    };

    field.value = TypedValue::Timestamp(enriched);
    if subsec.is_some() || offset.is_some() {
        field.notes.push(format!(
            "enriched from EXIF {}{}{}",
            base_tag,
            if subsec.is_some() {
                format!(", {subsec_tag}")
            } else {
                String::new()
            },
            if offset.is_some() {
                format!(", {offset_tag}")
            } else {
                String::new()
            }
        ));
    }
}

fn string_coordinate(entries: &[MetadataEntry], tag_name: &str) -> Option<(Provenance, f64)> {
    let entry = entries.iter().find(|entry| entry.tag_name == tag_name)?;
    let TypedValue::String(value) = &entry.value else {
        return None;
    };
    let parsed = parse_coordinate_string(value)?;
    Some((entry.provenance.clone(), parsed))
}

fn parse_coordinate_string(input: &str) -> Option<f64> {
    input.trim().parse::<f64>().ok()
}

fn entry_string<'a>(entries: &'a [MetadataEntry], tag_name: &str) -> Option<&'a str> {
    let entry = entries.iter().find(|entry| entry.tag_name == tag_name)?;
    match &entry.value {
        TypedValue::String(value) | TypedValue::Timestamp(value) => Some(value.as_str()),
        _ => None,
    }
}

fn ensure_field(
    fields: &mut Vec<NormalizedField>,
    field_name: &str,
    value: i64,
    provenance: &Provenance,
) {
    if fields.iter().any(|field| field.field == field_name) {
        return;
    }
    fields.push(NormalizedField {
        field: field_name.into(),
        value: TypedValue::Integer(value),
        confidence: 0.95,
        sources: vec![provenance.clone()],
        notes: Vec::new(),
    });
}

fn entry_integer<'a>(
    entries: &'a [MetadataEntry],
    tag_names: &[&str],
) -> Option<(&'a MetadataEntry, i64)> {
    let entry = entries
        .iter()
        .find(|entry| tag_names.iter().any(|tag_name| entry.tag_name == *tag_name))?;
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

fn compose_exif_timestamp(
    base: &str,
    subsec: Option<&str>,
    offset: Option<&str>,
) -> Option<String> {
    let normalized = normalize_exif_datetime(base);
    let mut timestamp = normalized;

    if let Some(subsec) = subsec.map(str::trim).filter(|value| !value.is_empty()) {
        let fractional = subsec.trim_start_matches('.');
        if !fractional.is_empty() {
            timestamp.push('.');
            timestamp.push_str(fractional);
        }
    }

    if let Some(offset) = offset.map(str::trim).filter(|value| !value.is_empty()) {
        if offset.starts_with('+') || offset.starts_with('-') {
            timestamp.push_str(offset);
        } else {
            return None;
        }
    }

    Some(timestamp)
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

    #[test]
    fn normalizes_xmp_string_coordinates() {
        let entries = vec![
            MetadataEntry {
                namespace: "xmp".into(),
                tag_id: "GPSLatitude".into(),
                tag_name: "GPSLatitude".into(),
                value: TypedValue::String("40.4462".into()),
                provenance: prov(),
                notes: vec!["decoded from xmp attribute exif:GPSLatitude".into()],
            },
            MetadataEntry {
                namespace: "xmp".into(),
                tag_id: "GPSLongitude".into(),
                tag_name: "GPSLongitude".into(),
                value: TypedValue::String("-79.98".into()),
                provenance: prov(),
                notes: vec!["decoded from xmp attribute exif:GPSLongitude".into()],
            },
        ];
        let fields = normalize(&entries);
        assert!(fields.iter().any(|field| field.field == "location"));
    }

    #[test]
    fn enriches_exif_timestamp_with_subseconds_and_offset() {
        let entries = vec![
            MetadataEntry {
                namespace: "exif".into(),
                tag_id: "0x9003".into(),
                tag_name: "DateTimeOriginal".into(),
                value: TypedValue::Timestamp("2025:08:07 10:44:16".into()),
                provenance: prov(),
                notes: Vec::new(),
            },
            MetadataEntry {
                namespace: "exif".into(),
                tag_id: "0x9291".into(),
                tag_name: "SubSecTimeOriginal".into(),
                value: TypedValue::String("046".into()),
                provenance: prov(),
                notes: Vec::new(),
            },
            MetadataEntry {
                namespace: "exif".into(),
                tag_id: "0x9011".into(),
                tag_name: "OffsetTimeOriginal".into(),
                value: TypedValue::String("-08:00".into()),
                provenance: prov(),
                notes: Vec::new(),
            },
        ];

        let fields = normalize(&entries);
        let captured = fields
            .iter()
            .find(|field| field.field == "captured_at")
            .unwrap();
        assert_eq!(
            captured.value,
            TypedValue::Timestamp("2025-08-07T10:44:16.046-08:00".into())
        );
    }

    #[test]
    fn uses_exif_dimensions_as_fallback() {
        let entries = vec![
            MetadataEntry {
                namespace: "exif".into(),
                tag_id: "0xA002".into(),
                tag_name: "ExifImageWidth".into(),
                value: TypedValue::Integer(6000),
                provenance: prov(),
                notes: Vec::new(),
            },
            MetadataEntry {
                namespace: "exif".into(),
                tag_id: "0xA003".into(),
                tag_name: "ExifImageHeight".into(),
                value: TypedValue::Integer(4000),
                provenance: prov(),
                notes: Vec::new(),
            },
        ];

        let fields = normalize(&entries);
        assert!(fields.iter().any(|field| field.field == "dimensions.width"));
        assert!(
            fields
                .iter()
                .any(|field| field.field == "dimensions.height")
        );
    }
}
