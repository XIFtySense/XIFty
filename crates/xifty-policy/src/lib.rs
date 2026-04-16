use xifty_core::{Conflict, MetadataEntry, NormalizedField, TypedValue};

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
    maybe_choose_float(
        entries,
        &mut result,
        "duration",
        &["DurationSeconds"],
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

    result
}

#[derive(Debug, Clone, Copy)]
enum NamespacePreference {
    ExifFirst,
    XmpFirst,
    XmpThenQuickTime,
    QuickTimeFirst,
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
        notes: conflict_note(&matches, winner),
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
        _ => false,
    }
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
}
