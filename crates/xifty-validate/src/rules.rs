use std::collections::BTreeMap;

use xifty_core::{Conflict, ConflictSide, MetadataEntry, TypedValue};

fn entry_side(entry: &MetadataEntry) -> ConflictSide {
    ConflictSide {
        namespace: entry.namespace.clone(),
        tag_id: entry.tag_id.clone(),
        tag_name: entry.tag_name.clone(),
        value: entry.value.clone(),
        provenance: entry.provenance.clone(),
    }
}

pub(crate) static SEMANTIC_GROUPS: &[(&str, &[(&str, &str)])] = &[
    (
        "captured_at",
        &[
            ("exif", "DateTimeOriginal"),
            ("xmp", "CreateDate"),
            ("quicktime", "CreationDate"),
        ],
    ),
    (
        "device.make",
        &[("exif", "Make"), ("xmp", "Make"), ("quicktime", "Make")],
    ),
    (
        "device.model",
        &[("exif", "Model"), ("xmp", "Model"), ("quicktime", "Model")],
    ),
    (
        "copyright",
        &[
            ("exif", "Copyright"),
            ("xmp", "Copyright"),
            ("iptc", "Copyright"),
        ],
    ),
];

static NUMERIC_GROUPS: &[(&str, &[(&str, &str)])] = &[
    (
        "exposure.iso",
        &[
            ("exif", "ISO"),
            ("exif", "ISOSpeedRatings"),
            ("xmp", "ISO"),
            ("xmp", "ISOSpeedRatings"),
        ],
    ),
    (
        "exposure.aperture",
        &[("exif", "FNumber"), ("xmp", "FNumber")],
    ),
    (
        "exposure.focal_length_mm",
        &[("exif", "FocalLength"), ("xmp", "FocalLength")],
    ),
    (
        "exposure.shutter_speed",
        &[("exif", "ExposureTime"), ("xmp", "ExposureTime")],
    ),
];

pub(crate) fn detect_conflicts(entries: &[MetadataEntry]) -> Vec<Conflict> {
    let mut out = Vec::new();
    out.extend(detect_cross_namespace_disagreement(
        entries,
        SEMANTIC_GROUPS,
    ));
    out.extend(detect_timestamp_offset_mismatch(entries));
    out.extend(detect_numeric_precision_mismatch(entries, NUMERIC_GROUPS));
    out
}

fn canonicalize_string(field: &str, raw: &str) -> String {
    let trimmed = raw.trim();
    if field == "device.make" || field == "device.model" {
        trimmed.to_lowercase()
    } else {
        trimmed.to_string()
    }
}

fn string_value(value: &TypedValue) -> Option<&str> {
    match value {
        TypedValue::String(s) => Some(s.as_str()),
        TypedValue::Timestamp(s) => Some(s.as_str()),
        _ => None,
    }
}

fn detect_cross_namespace_disagreement(
    entries: &[MetadataEntry],
    groups: &[(&str, &[(&str, &str)])],
) -> Vec<Conflict> {
    let mut conflicts = Vec::new();
    for (field, members) in groups {
        let mut matches: Vec<&MetadataEntry> = Vec::new();
        for entry in entries {
            if members
                .iter()
                .any(|(ns, tag)| *ns == entry.namespace.as_str() && *tag == entry.tag_name.as_str())
                && string_value(&entry.value).is_some()
            {
                matches.push(entry);
            }
        }
        if matches.len() < 2 {
            continue;
        }
        let mut by_canon: BTreeMap<String, &MetadataEntry> = BTreeMap::new();
        for m in &matches {
            let raw = string_value(&m.value).unwrap();
            let canon = canonicalize_string(field, raw);
            by_canon.entry(canon).or_insert(*m);
        }
        if by_canon.len() < 2 {
            continue;
        }
        let mut iter = by_canon.values();
        let a = iter.next().unwrap();
        let b = iter.next().unwrap();
        let a_val = string_value(&a.value).unwrap();
        let b_val = string_value(&b.value).unwrap();
        conflicts.push(Conflict {
            field: (*field).to_string(),
            message: format!(
                "{}:{}={} vs {}:{}={}",
                a.namespace, a.tag_name, a_val, b.namespace, b.tag_name, b_val
            ),
            sources: vec![entry_side(a), entry_side(b)],
        });
    }
    conflicts
}

fn parse_timestamp_offset(input: &str) -> Option<(String, Option<i32>)> {
    let trimmed = input.trim();
    if trimmed.len() < 19 {
        return None;
    }
    let wall = &trimmed[..19];
    let wb = wall.as_bytes();
    if !(wb[4] == b'-'
        && wb[7] == b'-'
        && (wb[10] == b'T' || wb[10] == b' ')
        && wb[13] == b':'
        && wb[16] == b':')
    {
        return None;
    }
    let rest = &trimmed[19..];
    if rest.is_empty() {
        return Some((wall.replace(' ', "T"), None));
    }
    if rest == "Z" || rest == "z" {
        return Some((wall.replace(' ', "T"), Some(0)));
    }
    let bytes = rest.as_bytes();
    let sign = match bytes[0] {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let (hh, mm) = if rest.len() >= 6 && bytes[3] == b':' {
        (&rest[1..3], &rest[4..6])
    } else if rest.len() >= 5 {
        (&rest[1..3], &rest[3..5])
    } else {
        return None;
    };
    let hours: i32 = hh.parse().ok()?;
    let mins: i32 = mm.parse().ok()?;
    Some((wall.replace(' ', "T"), Some(sign * (hours * 60 + mins))))
}

fn detect_timestamp_offset_mismatch(entries: &[MetadataEntry]) -> Vec<Conflict> {
    let timestamp_members: &[(&str, &str)] = &[
        ("exif", "DateTimeOriginal"),
        ("xmp", "CreateDate"),
        ("quicktime", "CreationDate"),
    ];
    let mut parsed: Vec<(&MetadataEntry, String, Option<i32>)> = Vec::new();
    for entry in entries {
        if !timestamp_members
            .iter()
            .any(|(ns, tag)| *ns == entry.namespace.as_str() && *tag == entry.tag_name.as_str())
        {
            continue;
        }
        let Some(raw) = string_value(&entry.value) else {
            continue;
        };
        if let Some((wall, offset)) = parse_timestamp_offset(raw) {
            parsed.push((entry, wall, offset));
        }
    }
    let mut conflicts = Vec::new();
    for i in 0..parsed.len() {
        for j in (i + 1)..parsed.len() {
            let (a, a_wall, a_off) = (&parsed[i].0, &parsed[i].1, parsed[i].2);
            let (b, b_wall, b_off) = (&parsed[j].0, &parsed[j].1, parsed[j].2);
            if a_wall != b_wall {
                continue;
            }
            let disagree = match (a_off, b_off) {
                (Some(x), Some(y)) => x != y,
                (Some(_), None) | (None, Some(_)) => true,
                (None, None) => false,
            };
            if disagree {
                let a_raw = string_value(&a.value).unwrap_or("");
                let b_raw = string_value(&b.value).unwrap_or("");
                conflicts.push(Conflict {
                    field: "captured_at".into(),
                    message: format!(
                        "timezone offset disagreement: {}:{}={} vs {}:{}={}",
                        a.namespace, a.tag_name, a_raw, b.namespace, b.tag_name, b_raw
                    ),
                    sources: vec![entry_side(a), entry_side(b)],
                });
                return conflicts;
            }
        }
    }
    conflicts
}

fn numeric_value(value: &TypedValue) -> Option<f64> {
    match value {
        TypedValue::Float(v) => Some(*v),
        TypedValue::Integer(v) => Some(*v as f64),
        TypedValue::Rational {
            numerator,
            denominator,
        } => {
            if *denominator == 0 {
                None
            } else {
                Some(*numerator as f64 / *denominator as f64)
            }
        }
        _ => None,
    }
}

fn relative_mismatch(a: f64, b: f64) -> bool {
    let mag = a.abs().max(b.abs());
    if mag == 0.0 {
        return false;
    }
    ((a - b).abs() / mag) > 0.005
}

fn detect_numeric_precision_mismatch(
    entries: &[MetadataEntry],
    groups: &[(&str, &[(&str, &str)])],
) -> Vec<Conflict> {
    let mut conflicts = Vec::new();
    for (field, members) in groups {
        let mut matches: Vec<(&MetadataEntry, f64)> = Vec::new();
        for entry in entries {
            if members
                .iter()
                .any(|(ns, tag)| *ns == entry.namespace.as_str() && *tag == entry.tag_name.as_str())
                && let Some(v) = numeric_value(&entry.value)
            {
                matches.push((entry, v));
            }
        }
        if matches.len() < 2 {
            continue;
        }
        'outer: for i in 0..matches.len() {
            for j in (i + 1)..matches.len() {
                let (a, av) = matches[i];
                let (b, bv) = matches[j];
                if relative_mismatch(av, bv) {
                    conflicts.push(Conflict {
                        field: (*field).to_string(),
                        message: format!(
                            "{}:{}={} vs {}:{}={}",
                            a.namespace, a.tag_name, av, b.namespace, b.tag_name, bv
                        ),
                        sources: vec![entry_side(a), entry_side(b)],
                    });
                    break 'outer;
                }
            }
        }
    }
    conflicts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalize_lowercases_device_fields_only() {
        assert_eq!(canonicalize_string("device.make", "  Canon "), "canon");
        assert_eq!(canonicalize_string("device.model", " EOS R5 "), "eos r5");
        assert_eq!(canonicalize_string("copyright", "  Kai "), "Kai");
    }

    #[test]
    fn parse_offset_handles_z_and_signed() {
        let (wall, off) = parse_timestamp_offset("2024-01-01T10:00:00Z").unwrap();
        assert_eq!(wall, "2024-01-01T10:00:00");
        assert_eq!(off, Some(0));

        let (_, off) = parse_timestamp_offset("2024-01-01T10:00:00+00:00").unwrap();
        assert_eq!(off, Some(0));

        let (_, off) = parse_timestamp_offset("2024-01-01T10:00:00-05:00").unwrap();
        assert_eq!(off, Some(-300));

        let (_, off) = parse_timestamp_offset("2024-01-01T10:00:00").unwrap();
        assert_eq!(off, None);
    }

    #[test]
    fn relative_mismatch_tolerates_small_deltas() {
        assert!(!relative_mismatch(200.0, 200.0));
        assert!(!relative_mismatch(200.0, 200.5));
        assert!(relative_mismatch(200.0, 400.0));
        assert!(relative_mismatch(1.0 / 200.0, 1.0 / 400.0));
    }
}
