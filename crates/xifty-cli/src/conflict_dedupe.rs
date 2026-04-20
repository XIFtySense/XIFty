//! Dedupe `Conflict` entries that both `xifty-validate` and `xifty-policy`
//! emit for the same underlying disagreement.
//!
//! Both crates have legitimate independent reasons to surface a finding
//! (they are consumed standalone via `xifty-ffi` and `xifty-json`), so we
//! dedupe at the CLI composition site rather than inside either emitter.
//!
//! Fingerprint: `(field, sorted Vec<(namespace, tag_id)>)` drawn from
//! `Conflict.sources`. When two conflicts collide on that key, we keep the
//! entry with more `sources` (more evidence); on a source-count tie we
//! prefer the entry whose message names the selected winner (the
//! `xifty-policy` format, which contains the literal "selected"), falling
//! back to first-seen order. This keeps the more informative message
//! visible to CLI consumers while still collapsing the duplicate.
//!
//! Intentional non-goal: conflicts that share a `field` but have different
//! source sets (e.g. validate pairs exif+xmp, policy pulls in a third
//! namespace) are NOT merged — they surface distinct evidence. Do not
//! tighten the key to `field`-only without revisiting this.

use xifty_core::Conflict;

type Fingerprint = (String, Vec<(String, String)>);

fn fingerprint(conflict: &Conflict) -> Fingerprint {
    let mut sources: Vec<(String, String)> = conflict
        .sources
        .iter()
        .map(|s| (s.namespace.clone(), s.tag_id.clone()))
        .collect();
    sources.sort();
    (conflict.field.clone(), sources)
}

pub(crate) fn dedupe_conflicts(conflicts: Vec<Conflict>) -> Vec<Conflict> {
    // Track fingerprint -> index into output vector so we can upgrade an
    // earlier entry if a later one carries more evidence, while preserving
    // the stable first-seen ordering.
    let mut out: Vec<Conflict> = Vec::with_capacity(conflicts.len());
    let mut index: Vec<(Fingerprint, usize)> = Vec::with_capacity(conflicts.len());

    for conflict in conflicts {
        let fp = fingerprint(&conflict);
        if let Some((_, idx)) = index.iter().find(|(existing, _)| existing == &fp) {
            let idx = *idx;
            let existing = &out[idx];
            let replace = if conflict.sources.len() != existing.sources.len() {
                conflict.sources.len() > existing.sources.len()
            } else {
                // Source-count tie: prefer the winner-naming (policy) message
                // over the "A vs B" (validate) message so the report carries
                // the most informative wording. Otherwise keep first-seen.
                !existing.message.contains("selected") && conflict.message.contains("selected")
            };
            if replace {
                out[idx] = conflict;
            }
        } else {
            index.push((fp, out.len()));
            out.push(conflict);
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use xifty_core::{ConflictSide, Provenance, TypedValue};

    fn prov(ns: &str) -> Provenance {
        Provenance {
            container: "png".into(),
            namespace: ns.into(),
            path: Some("x".into()),
            offset_start: Some(0),
            offset_end: Some(1),
            notes: Vec::new(),
        }
    }

    fn side(ns: &str, tag_id: &str) -> ConflictSide {
        ConflictSide {
            namespace: ns.into(),
            tag_id: tag_id.into(),
            tag_name: tag_id.into(),
            value: TypedValue::String("v".into()),
            provenance: prov(ns),
        }
    }

    fn conflict(field: &str, message: &str, sources: Vec<ConflictSide>) -> Conflict {
        Conflict {
            field: field.into(),
            message: message.into(),
            sources,
        }
    }

    #[test]
    fn identical_fingerprints_collapse_to_one() {
        let a = conflict(
            "captured_at",
            "xmp:CreateDate=... vs exif:...",
            vec![side("xmp", "CreateDate"), side("exif", "0x9003")],
        );
        let b = conflict(
            "captured_at",
            "xmp:Later vs exif:Other",
            vec![side("exif", "0x9003"), side("xmp", "CreateDate")],
        );
        let out = dedupe_conflicts(vec![a.clone(), b]);
        assert_eq!(out.len(), 1);
        // Neither message names a winner; first-seen wins.
        assert_eq!(out[0].message, a.message);
    }

    #[test]
    fn tie_break_prefers_winner_selected_message() {
        let validate = conflict(
            "captured_at",
            "xmp:CreateDate=x vs exif:DateTimeOriginal=y",
            vec![side("xmp", "CreateDate"), side("exif", "0x9003")],
        );
        let policy = conflict(
            "captured_at",
            "multiple candidates disagreed; selected DateTimeOriginal from exif",
            vec![side("exif", "0x9003"), side("xmp", "CreateDate")],
        );
        let out = dedupe_conflicts(vec![validate, policy.clone()]);
        assert_eq!(out.len(), 1);
        assert!(out[0].message.contains("selected"));
        // Winner (exif) should lead in sources because the policy entry won.
        assert_eq!(out[0].sources[0].namespace, "exif");
    }

    #[test]
    fn same_field_different_source_sets_are_kept_separate() {
        let a = conflict(
            "captured_at",
            "exif vs xmp",
            vec![side("exif", "0x9003"), side("xmp", "CreateDate")],
        );
        let b = conflict(
            "captured_at",
            "exif vs quicktime",
            vec![side("exif", "0x9003"), side("quicktime", "©day")],
        );
        let out = dedupe_conflicts(vec![a, b]);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn later_entry_with_more_sources_replaces_earlier() {
        let empty = conflict("device.model", "thin", vec![]);
        let rich = conflict(
            "device.model",
            "rich",
            vec![side("exif", "0x0110"), side("xmp", "Model")],
        );
        // Different fingerprints because sources differ — this guards the
        // "do not merge unequal source sets" contract.
        let out = dedupe_conflicts(vec![empty.clone(), rich.clone()]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].message, "thin");
        assert_eq!(out[1].message, "rich");
    }

    #[test]
    fn first_seen_with_populated_sources_beats_later_empty_duplicate() {
        // Both conflicts dedupe only when fingerprints match; two empty-source
        // conflicts on the same field DO share a fingerprint (field, []).
        let first = conflict("software", "first", vec![]);
        let second = conflict("software", "second", vec![]);
        let out = dedupe_conflicts(vec![first.clone(), second]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].message, "first");
    }

    #[test]
    fn ordering_preserved_for_distinct_entries() {
        let a = conflict("a", "", vec![side("exif", "1")]);
        let b = conflict("b", "", vec![side("exif", "2")]);
        let c = conflict("c", "", vec![side("exif", "3")]);
        let out = dedupe_conflicts(vec![a, b, c]);
        let fields: Vec<_> = out.iter().map(|c| c.field.as_str()).collect();
        assert_eq!(fields, vec!["a", "b", "c"]);
    }

    #[test]
    fn richer_later_entry_upgrades_earlier_on_same_fingerprint() {
        // Same field, same sorted source set; the later one carries extra
        // message detail (but sources are equal so first-seen wins).
        let thin = conflict(
            "captured_at",
            "thin",
            vec![side("exif", "0x9003"), side("xmp", "CreateDate")],
        );
        let also_thin = conflict(
            "captured_at",
            "also thin",
            vec![side("xmp", "CreateDate"), side("exif", "0x9003")],
        );
        let out = dedupe_conflicts(vec![thin.clone(), also_thin]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].message, "thin");
    }
}
