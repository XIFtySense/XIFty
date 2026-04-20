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

    result
}

#[derive(Debug, Clone, Copy)]
enum NamespacePreference {
    ExifFirst,
    XmpFirst,
    XmpThenQuickTime,
    QuickTimeFirst,
    IccFirst,
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
    let TypedValue::Rational {
        numerator,
        denominator,
    } = &winner.value
    else {
        return;
    };

    let value = TypedValue::Rational {
        numerator: *numerator,
        denominator: *denominator,
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
        ) => an == bn && ad == bd,
        _ => false,
    }
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
}
