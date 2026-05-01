use xifty_core::{Conflict, ConflictSide, MetadataEntry, NormalizedField, TypedValue};

#[derive(Debug, Clone, Default)]
pub struct PolicyResult {
    pub fields: Vec<NormalizedField>,
    pub conflicts: Vec<Conflict>,
}

pub fn reconcile(entries: &[MetadataEntry]) -> PolicyResult {
    let mut result = PolicyResult::default();

    maybe_choose_string(
        entries,
        &mut result,
        "captured_at",
        &["DateTimeOriginal", "CreateDate"],
        NamespacePreference::ExifFirst,
        true,
    );
    maybe_choose_string(
        entries,
        &mut result,
        "created_at",
        &["CreateDate"],
        NamespacePreference::ExifFirst,
        true,
    );
    maybe_choose_string(
        entries,
        &mut result,
        "modified_at",
        &["ModifyDate"],
        NamespacePreference::ExifFirst,
        true,
    );
    maybe_choose_string(
        entries,
        &mut result,
        "device.make",
        &["Make"],
        NamespacePreference::ExifFirst,
        false,
    );
    maybe_choose_string(
        entries,
        &mut result,
        "device.model",
        &["Model"],
        NamespacePreference::ExifFirst,
        false,
    );
    maybe_choose_string(
        entries,
        &mut result,
        "software",
        &["Software"],
        NamespacePreference::ExifFirst,
        false,
    );
    maybe_choose_string(
        entries,
        &mut result,
        "author",
        &["Author"],
        NamespacePreference::XmpThenQuickTime,
        false,
    );
    maybe_choose_string(
        entries,
        &mut result,
        "copyright",
        &["Copyright"],
        NamespacePreference::XmpFirst,
        false,
    );
    maybe_choose_string(
        entries,
        &mut result,
        "headline",
        &["Headline"],
        NamespacePreference::XmpFirst,
        false,
    );
    maybe_choose_string(
        entries,
        &mut result,
        "description",
        &["Description"],
        NamespacePreference::XmpFirst,
        false,
    );
    maybe_choose_string(
        entries,
        &mut result,
        "color.profile.name",
        &["ProfileDescription"],
        NamespacePreference::IccFirst,
        false,
    );
    maybe_choose_string(
        entries,
        &mut result,
        "color.profile.class",
        &["ProfileClass"],
        NamespacePreference::IccFirst,
        false,
    );
    maybe_choose_string(
        entries,
        &mut result,
        "color.space",
        &["ColorSpace"],
        NamespacePreference::IccFirst,
        false,
    );
    maybe_choose_integer(
        entries,
        &mut result,
        "orientation",
        &["Orientation"],
        NamespacePreference::ExifFirst,
    );
    maybe_choose_integer(
        entries,
        &mut result,
        "dimensions.width",
        &["ImageWidth"],
        NamespacePreference::ExifFirst,
    );
    maybe_choose_integer(
        entries,
        &mut result,
        "dimensions.height",
        &["ImageHeight"],
        NamespacePreference::ExifFirst,
    );
    maybe_choose_integer(
        entries,
        &mut result,
        "exposure.iso",
        &["ISO", "ISOSpeedRatings"],
        NamespacePreference::ExifFirst,
    );
    maybe_choose_rational(
        entries,
        &mut result,
        "exposure.aperture",
        &["FNumber"],
        NamespacePreference::ExifFirst,
    );
    maybe_choose_rational(
        entries,
        &mut result,
        "exposure.shutter_speed",
        &["ExposureTime"],
        NamespacePreference::ExifFirst,
    );
    maybe_choose_float_like(
        entries,
        &mut result,
        "exposure.focal_length_mm",
        &["FocalLength"],
        NamespacePreference::ExifFirst,
    );
    maybe_choose_string(
        entries,
        &mut result,
        "lens.model",
        &["LensModel"],
        NamespacePreference::ExifFirst,
        false,
    );
    maybe_choose_string(
        entries,
        &mut result,
        "lens.make",
        &["LensMake"],
        NamespacePreference::ExifFirst,
        false,
    );
    maybe_choose_float(
        entries,
        &mut result,
        "duration",
        &["DurationSeconds"],
        NamespacePreference::QuickTimeFirst,
    );
    maybe_choose_float(
        entries,
        &mut result,
        "video.framerate",
        &["VideoFrameRate"],
        NamespacePreference::QuickTimeFirst,
    );
    maybe_choose_integer(
        entries,
        &mut result,
        "video.bitrate",
        &["VideoBitrate"],
        NamespacePreference::QuickTimeFirst,
    );
    maybe_choose_string(
        entries,
        &mut result,
        "codec.video",
        &["VideoCodec"],
        NamespacePreference::QuickTimeFirst,
        false,
    );
    maybe_choose_string(
        entries,
        &mut result,
        "codec.audio",
        &["AudioCodec"],
        NamespacePreference::QuickTimeFirst,
        false,
    );
    maybe_choose_integer(
        entries,
        &mut result,
        "audio.channels",
        &["AudioChannels"],
        NamespacePreference::QuickTimeFirst,
    );
    maybe_choose_integer(
        entries,
        &mut result,
        "audio.sample_rate",
        &["AudioSampleRate"],
        NamespacePreference::QuickTimeFirst,
    );
    maybe_choose_integer(
        entries,
        &mut result,
        "audio.bit_depth",
        &["AudioBitDepth"],
        NamespacePreference::QuickTimeFirst,
    );

    // Camera serial number — currently only emitted by the DJI udta path
    // (©csn). EXIF stores it under `BodySerialNumber`, not `SerialNumber`,
    // so no overlap with EXIF today; if a future source emits it, EXIF
    // takes precedence.
    maybe_choose_string(
        entries,
        &mut result,
        "device.serial_number",
        &["SerialNumber"],
        NamespacePreference::ExifFirst,
        false,
    );

    // Drone telemetry — emitted only by the `dji` namespace (direct-udta
    // ©fpt/©fyw/©frl/©gpt/©gyw/©grl/©xsp/©ysp/©zsp). DjiFirst preference is
    // a no-op today (no other source emits these tags) but documents intent.
    maybe_choose_float(
        entries,
        &mut result,
        "drone.flight.pitch_deg",
        &["FlightPitchDegree"],
        NamespacePreference::DjiFirst,
    );
    maybe_choose_float(
        entries,
        &mut result,
        "drone.flight.yaw_deg",
        &["FlightYawDegree"],
        NamespacePreference::DjiFirst,
    );
    maybe_choose_float(
        entries,
        &mut result,
        "drone.flight.roll_deg",
        &["FlightRollDegree"],
        NamespacePreference::DjiFirst,
    );
    maybe_choose_float(
        entries,
        &mut result,
        "drone.gimbal.pitch_deg",
        &["GimbalPitchDegree"],
        NamespacePreference::DjiFirst,
    );
    maybe_choose_float(
        entries,
        &mut result,
        "drone.gimbal.yaw_deg",
        &["GimbalYawDegree"],
        NamespacePreference::DjiFirst,
    );
    maybe_choose_float(
        entries,
        &mut result,
        "drone.gimbal.roll_deg",
        &["GimbalRollDegree"],
        NamespacePreference::DjiFirst,
    );
    maybe_choose_float(
        entries,
        &mut result,
        "drone.speed.x_mps",
        &["SpeedX"],
        NamespacePreference::DjiFirst,
    );
    maybe_choose_float(
        entries,
        &mut result,
        "drone.speed.y_mps",
        &["SpeedY"],
        NamespacePreference::DjiFirst,
    );
    maybe_choose_float(
        entries,
        &mut result,
        "drone.speed.z_mps",
        &["SpeedZ"],
        NamespacePreference::DjiFirst,
    );

    result
}

#[derive(Debug, Clone, Copy)]
enum NamespacePreference {
    ExifFirst,
    XmpFirst,
    XmpThenQuickTime,
    QuickTimeFirst,
    IccFirst,
    DjiFirst,
}

fn maybe_choose_string(
    entries: &[MetadataEntry],
    result: &mut PolicyResult,
    field_name: &str,
    tag_names: &[&str],
    preference: NamespacePreference,
    timestamp: bool,
) {
    let matches = find_matches(entries, tag_names);
    if matches.is_empty() {
        return;
    }

    let winner = choose_match(&matches, preference);
    let winner_text = match &winner.value {
        TypedValue::String(value) | TypedValue::Timestamp(value) => value.clone(),
        _ => return,
    };

    if has_material_difference(&matches, &winner.value) {
        result.conflicts.push(Conflict {
            field: field_name.into(),
            message: format!(
                "multiple candidates disagreed; selected {} from {}",
                winner.tag_name, winner.namespace
            ),
            sources: build_conflict_sides(&matches, winner),
        });
    }

    result.fields.push(NormalizedField {
        field: field_name.into(),
        value: if timestamp {
            TypedValue::Timestamp(normalize_timestamp(&winner_text))
        } else {
            TypedValue::String(winner_text)
        },
        confidence: 0.95,
        sources: vec![winner.provenance.clone()],
        notes: string_field_notes(field_name, &matches, winner),
    });
}

fn maybe_choose_integer(
    entries: &[MetadataEntry],
    result: &mut PolicyResult,
    field_name: &str,
    tag_names: &[&str],
    preference: NamespacePreference,
) {
    let matches = find_matches(entries, tag_names);
    if matches.is_empty() {
        return;
    }

    let winner = choose_match(&matches, preference);
    let TypedValue::Integer(value) = &winner.value else {
        return;
    };

    if has_material_difference(&matches, &TypedValue::Integer(*value)) {
        result.conflicts.push(Conflict {
            field: field_name.into(),
            message: format!(
                "multiple candidates disagreed; selected {} from {}",
                winner.tag_name, winner.namespace
            ),
            sources: build_conflict_sides(&matches, winner),
        });
    }

    result.fields.push(NormalizedField {
        field: field_name.into(),
        value: TypedValue::Integer(*value),
        confidence: 0.95,
        sources: vec![winner.provenance.clone()],
        notes: conflict_note(&matches, winner),
    });
}

fn maybe_choose_float(
    entries: &[MetadataEntry],
    result: &mut PolicyResult,
    field_name: &str,
    tag_names: &[&str],
    preference: NamespacePreference,
) {
    let matches = find_matches(entries, tag_names);
    if matches.is_empty() {
        return;
    }

    let winner = choose_match(&matches, preference);
    let TypedValue::Float(value) = &winner.value else {
        return;
    };

    if has_material_difference(&matches, &TypedValue::Float(*value)) {
        result.conflicts.push(Conflict {
            field: field_name.into(),
            message: format!(
                "multiple candidates disagreed; selected {} from {}",
                winner.tag_name, winner.namespace
            ),
            sources: build_conflict_sides(&matches, winner),
        });
    }

    result.fields.push(NormalizedField {
        field: field_name.into(),
        value: TypedValue::Float(*value),
        confidence: 0.95,
        sources: vec![winner.provenance.clone()],
        notes: conflict_note(&matches, winner),
    });
}

fn maybe_choose_rational(
    entries: &[MetadataEntry],
    result: &mut PolicyResult,
    field_name: &str,
    tag_names: &[&str],
    preference: NamespacePreference,
) {
    let matches = find_matches(entries, tag_names);
    if matches.is_empty() {
        return;
    }

    let winner = choose_match(&matches, preference);
    let Some(value) = rational_value(&winner.value) else {
        return;
    };

    if has_material_difference(&matches, &value) {
        result.conflicts.push(Conflict {
            field: field_name.into(),
            message: format!(
                "multiple candidates disagreed; selected {} from {}",
                winner.tag_name, winner.namespace
            ),
            sources: build_conflict_sides(&matches, winner),
        });
    }

    result.fields.push(NormalizedField {
        field: field_name.into(),
        value,
        confidence: 0.95,
        sources: vec![winner.provenance.clone()],
        notes: conflict_note(&matches, winner),
    });
}

fn maybe_choose_float_like(
    entries: &[MetadataEntry],
    result: &mut PolicyResult,
    field_name: &str,
    tag_names: &[&str],
    preference: NamespacePreference,
) {
    let matches = find_matches(entries, tag_names);
    if matches.is_empty() {
        return;
    }

    let winner = choose_match(&matches, preference);
    let Some(value) = numeric_value(&winner.value) else {
        return;
    };

    if has_material_difference(&matches, &winner.value) {
        result.conflicts.push(Conflict {
            field: field_name.into(),
            message: format!(
                "multiple candidates disagreed; selected {} from {}",
                winner.tag_name, winner.namespace
            ),
            sources: build_conflict_sides(&matches, winner),
        });
    }

    result.fields.push(NormalizedField {
        field: field_name.into(),
        value: TypedValue::Float(value),
        confidence: 0.95,
        sources: vec![winner.provenance.clone()],
        notes: conflict_note(&matches, winner),
    });
}

fn find_matches<'a>(entries: &'a [MetadataEntry], tag_names: &[&str]) -> Vec<&'a MetadataEntry> {
    entries
        .iter()
        .filter(|entry| tag_names.iter().any(|tag_name| entry.tag_name == *tag_name))
        .collect()
}

fn choose_match<'a>(
    matches: &[&'a MetadataEntry],
    preference: NamespacePreference,
) -> &'a MetadataEntry {
    let preferred_namespaces: &[&str] = match preference {
        NamespacePreference::ExifFirst => &["exif"],
        NamespacePreference::XmpFirst => &["xmp"],
        NamespacePreference::XmpThenQuickTime => &["xmp", "quicktime"],
        NamespacePreference::QuickTimeFirst => &["quicktime"],
        NamespacePreference::IccFirst => &["icc"],
        NamespacePreference::DjiFirst => &["dji"],
    };

    for namespace in preferred_namespaces {
        if let Some(entry) = matches
            .iter()
            .copied()
            .find(|entry| entry.namespace == *namespace)
        {
            return entry;
        }
    }

    matches[0]
}

fn has_material_difference(matches: &[&MetadataEntry], winner: &TypedValue) -> bool {
    matches
        .iter()
        .any(|entry| !typed_values_equal(&entry.value, winner))
}

fn typed_values_equal(left: &TypedValue, right: &TypedValue) -> bool {
    match (left, right) {
        (TypedValue::String(a), TypedValue::String(b))
        | (TypedValue::String(a), TypedValue::Timestamp(b))
        | (TypedValue::Timestamp(a), TypedValue::String(b))
        | (TypedValue::Timestamp(a), TypedValue::Timestamp(b)) => {
            normalize_timestamp(a) == normalize_timestamp(b)
        }
        (TypedValue::Integer(a), TypedValue::Integer(b)) => a == b,
        (TypedValue::Float(a), TypedValue::Float(b)) => (*a - *b).abs() < f64::EPSILON,
        (
            TypedValue::Rational {
                numerator: an,
                denominator: ad,
            },
            TypedValue::Rational {
                numerator: bn,
                denominator: bd,
            },
        ) => {
            (an == bn && ad == bd)
                || ((*an as f64 / *ad as f64) - (*bn as f64 / *bd as f64)).abs() < 0.000_001
        }
        _ if numeric_value(left).is_some() && numeric_value(right).is_some() => {
            let left = numeric_value(left).unwrap();
            let right = numeric_value(right).unwrap();
            (left - right).abs() < 0.000_001
        }
        _ => false,
    }
}

fn rational_value(value: &TypedValue) -> Option<TypedValue> {
    if let TypedValue::Rational {
        numerator,
        denominator,
    } = value
    {
        if *denominator == 0 {
            return None;
        }
        return Some(TypedValue::Rational {
            numerator: *numerator,
            denominator: *denominator,
        });
    }

    let (numerator, denominator) = match value {
        TypedValue::Integer(value) => (*value, 1),
        TypedValue::Float(value) => decimal_to_rational(*value)?,
        TypedValue::String(value) | TypedValue::Timestamp(value) => parse_rational_text(value)?,
        _ => return None,
    };
    let (numerator, denominator) = reduce_rational(numerator, denominator)?;
    Some(TypedValue::Rational {
        numerator,
        denominator,
    })
}

fn parse_rational_text(input: &str) -> Option<(i64, i64)> {
    let trimmed = input.trim().trim_end_matches('s').trim();
    if let Some((left, right)) = trimmed.split_once('/') {
        let numerator = left.trim().parse::<i64>().ok()?;
        let denominator = right.trim().parse::<i64>().ok()?;
        return Some((numerator, denominator));
    }
    decimal_to_rational(trimmed.parse::<f64>().ok()?)
}

fn rational_value_from_text(input: &str) -> Option<TypedValue> {
    let (numerator, denominator) = parse_rational_text(input)?;
    let (numerator, denominator) = reduce_rational(numerator, denominator)?;
    Some(TypedValue::Rational {
        numerator,
        denominator,
    })
}

fn decimal_to_rational(value: f64) -> Option<(i64, i64)> {
    if !value.is_finite() {
        return None;
    }
    if value == 0.0 {
        return Some((0, 1));
    }
    let sign = if value.is_sign_negative() { -1 } else { 1 };
    let absolute = value.abs();
    if absolute < 1.0 {
        let reciprocal = 1.0 / absolute;
        let rounded = reciprocal.round();
        if (reciprocal - rounded).abs() < 0.000_001 && rounded <= i64::MAX as f64 {
            return Some((sign, rounded as i64));
        }
    }
    let denominator = 1_000_000i64;
    let numerator = (absolute * denominator as f64).round();
    if numerator > i64::MAX as f64 {
        return None;
    }
    Some((sign * numerator as i64, denominator))
}

fn reduce_rational(numerator: i64, denominator: i64) -> Option<(i64, i64)> {
    if denominator == 0 {
        return None;
    }
    let sign = if denominator < 0 { -1 } else { 1 };
    let mut numerator = numerator * sign;
    let mut denominator = denominator.abs();
    let divisor = gcd(numerator.abs(), denominator);
    numerator /= divisor;
    denominator /= divisor;
    Some((numerator, denominator))
}

fn gcd(mut left: i64, mut right: i64) -> i64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left.max(1)
}

fn numeric_value(value: &TypedValue) -> Option<f64> {
    match value {
        TypedValue::Float(value) => Some(*value),
        TypedValue::Integer(value) => Some(*value as f64),
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
        TypedValue::String(value) | TypedValue::Timestamp(value) => {
            let rational = rational_value_from_text(value)?;
            numeric_value(&rational)
        }
        _ => None,
    }
}

fn build_conflict_sides(matches: &[&MetadataEntry], winner: &MetadataEntry) -> Vec<ConflictSide> {
    let mut sides = Vec::with_capacity(matches.len());
    sides.push(ConflictSide {
        namespace: winner.namespace.clone(),
        tag_id: winner.tag_id.clone(),
        tag_name: winner.tag_name.clone(),
        value: winner.value.clone(),
        provenance: winner.provenance.clone(),
    });
    for m in matches {
        if std::ptr::eq(*m, winner) {
            continue;
        }
        if !typed_values_equal(&m.value, &winner.value) {
            sides.push(ConflictSide {
                namespace: m.namespace.clone(),
                tag_id: m.tag_id.clone(),
                tag_name: m.tag_name.clone(),
                value: m.value.clone(),
                provenance: m.provenance.clone(),
            });
        }
    }
    sides
}

fn conflict_note(matches: &[&MetadataEntry], winner: &MetadataEntry) -> Vec<String> {
    if !has_material_difference(matches, &winner.value) {
        return Vec::new();
    }
    vec![format!(
        "selected {} from {} over {} competing candidate(s)",
        winner.tag_name,
        winner.namespace,
        matches.len().saturating_sub(1)
    )]
}

fn string_field_notes(
    field_name: &str,
    matches: &[&MetadataEntry],
    winner: &MetadataEntry,
) -> Vec<String> {
    let mut notes = conflict_note(matches, winner);
    if field_name.starts_with("color.") && winner.namespace == "icc" {
        notes.push("selected bounded ICC metadata as authoritative color-profile source".into());
    }
    notes
}

fn normalize_timestamp(input: &str) -> String {
    if input.contains('T') {
        return input.to_string();
    }
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

    #[test]
    fn default_policy_result_is_empty() {
        let result = reconcile(&[]);
        assert!(result.fields.is_empty());
        assert!(result.conflicts.is_empty());
    }

    #[test]
    fn prefers_exif_and_emits_conflict_for_disagreement() {
        let prov = xifty_core::Provenance {
            container: "png".into(),
            namespace: "xmp".into(),
            path: None,
            offset_start: None,
            offset_end: None,
            notes: Vec::new(),
        };
        let exif = MetadataEntry {
            namespace: "exif".into(),
            tag_id: "0x0110".into(),
            tag_name: "Model".into(),
            value: TypedValue::String("ExifCam".into()),
            provenance: prov.clone(),
            notes: Vec::new(),
        };
        let xmp = MetadataEntry {
            namespace: "xmp".into(),
            tag_id: "Model".into(),
            tag_name: "Model".into(),
            value: TypedValue::String("XmpCam".into()),
            provenance: prov,
            notes: Vec::new(),
        };
        let result = reconcile(&[xmp, exif]);
        assert!(
            result
                .fields
                .iter()
                .any(|field| field.field == "device.model")
        );
        assert_eq!(result.conflicts.len(), 1);
    }

    #[test]
    fn selects_quicktime_media_fields() {
        let prov = xifty_core::Provenance {
            container: "mp4".into(),
            namespace: "quicktime".into(),
            path: None,
            offset_start: None,
            offset_end: None,
            notes: Vec::new(),
        };
        let entries = vec![
            MetadataEntry {
                namespace: "quicktime".into(),
                tag_id: "DurationSeconds".into(),
                tag_name: "DurationSeconds".into(),
                value: TypedValue::Float(12.0),
                provenance: prov.clone(),
                notes: Vec::new(),
            },
            MetadataEntry {
                namespace: "quicktime".into(),
                tag_id: "VideoCodec".into(),
                tag_name: "VideoCodec".into(),
                value: TypedValue::String("avc1".into()),
                provenance: prov.clone(),
                notes: Vec::new(),
            },
            MetadataEntry {
                namespace: "quicktime".into(),
                tag_id: "AudioCodec".into(),
                tag_name: "AudioCodec".into(),
                value: TypedValue::String("mp4a".into()),
                provenance: prov,
                notes: Vec::new(),
            },
        ];

        let result = reconcile(&entries);
        assert!(result.fields.iter().any(|field| field.field == "duration"));
        assert!(
            result
                .fields
                .iter()
                .any(|field| field.field == "codec.video")
        );
        assert!(
            result
                .fields
                .iter()
                .any(|field| field.field == "codec.audio")
        );
    }

    #[test]
    fn prefers_xmp_over_iptc_for_editorial_conflicts() {
        let xmp_prov = xifty_core::Provenance {
            container: "jpeg".into(),
            namespace: "xmp".into(),
            path: Some("xmp_packet".into()),
            offset_start: Some(10),
            offset_end: Some(20),
            notes: Vec::new(),
        };
        let iptc_prov = xifty_core::Provenance {
            container: "jpeg".into(),
            namespace: "iptc".into(),
            path: Some("app13_iptc".into()),
            offset_start: Some(30),
            offset_end: Some(40),
            notes: Vec::new(),
        };
        let xmp = MetadataEntry {
            namespace: "xmp".into(),
            tag_id: "Author".into(),
            tag_name: "Author".into(),
            value: TypedValue::String("XMP Kai".into()),
            provenance: xmp_prov,
            notes: Vec::new(),
        };
        let iptc = MetadataEntry {
            namespace: "iptc".into(),
            tag_id: "2:80".into(),
            tag_name: "Author".into(),
            value: TypedValue::String("IPTC Kai".into()),
            provenance: iptc_prov,
            notes: Vec::new(),
        };

        let result = reconcile(&[iptc, xmp]);
        let author = result
            .fields
            .iter()
            .find(|field| field.field == "author")
            .expect("missing author field");
        assert_eq!(author.value, TypedValue::String("XMP Kai".into()));
        assert_eq!(result.conflicts.len(), 1);
        assert!(author.notes.iter().any(|note| note.contains("selected")));
    }

    #[test]
    fn canonicalizes_exposure_time_to_rational_from_float_and_string() {
        let prov = xifty_core::Provenance {
            container: "mp4".into(),
            namespace: "quicktime".into(),
            path: None,
            offset_start: None,
            offset_end: None,
            notes: Vec::new(),
        };
        for value in [
            TypedValue::Float(0.004),
            TypedValue::String("1/250".into()),
            TypedValue::String("0.004s".into()),
        ] {
            let entry = MetadataEntry {
                namespace: "quicktime".into(),
                tag_id: "ExposureTime".into(),
                tag_name: "ExposureTime".into(),
                value,
                provenance: prov.clone(),
                notes: Vec::new(),
            };
            let result = reconcile(&[entry]);
            let shutter = result
                .fields
                .iter()
                .find(|field| field.field == "exposure.shutter_speed")
                .expect("missing shutter speed");
            assert_eq!(
                shutter.value,
                TypedValue::Rational {
                    numerator: 1,
                    denominator: 250
                }
            );
        }
    }
}
