//! Derived `c2pa.ai_generated` + cross-cutting `ai.*` fields.

use crate::assertion::AssertionEntry;

#[derive(Debug, Default, Clone)]
pub struct Derived {
    pub ai_generated: bool,
    pub source_type: Option<String>,
    pub generator: Option<String>,
    pub synthid_disclosed: bool,
}

const AI_NEWSCODES: &[&str] = &[
    "trainedAlgorithmicMedia",
    "compositeWithTrainedAlgorithmicMedia",
    "algorithmicMedia",
    "trainedAlgorithmicData",
    "compositeOfTrainedAlgorithmicMedia",
];

/// Derive AI-related fields from the assertion list and a claim generator
/// hint.
pub fn derive(claim_generator: Option<&str>, assertions: &[AssertionEntry]) -> Derived {
    let mut out = Derived::default();
    for entry in assertions {
        if let Some(dst) = entry.digital_source_type.as_deref() {
            // dst is typically a full IPTC newscode URL; pick the trailing
            // segment for the bare label.
            let leaf = dst.rsplit('/').next().unwrap_or(dst);
            if AI_NEWSCODES.iter().any(|n| n.eq_ignore_ascii_case(leaf)) {
                out.ai_generated = true;
                if out.source_type.is_none() {
                    out.source_type = Some(leaf.to_string());
                }
            }
        }
        if let Some(desc) = entry.description.as_deref() {
            if desc.to_ascii_lowercase().contains("synthid") {
                out.synthid_disclosed = true;
            }
        }
    }
    if let Some(generator) = claim_generator {
        if let Some(short) = generator_label(generator) {
            out.generator = Some(short);
            if !out.ai_generated && generator_implies_ai(generator) {
                out.ai_generated = true;
            }
        }
    }
    out
}

fn generator_label(generator: &str) -> Option<String> {
    let lower = generator.to_ascii_lowercase();
    let table = [
        ("google c2pa", "Google"),
        ("notebooklm", "Google"),
        ("microsoft", "Microsoft"),
        ("copilot", "Microsoft"),
        ("adobe firefly", "Adobe Firefly"),
        ("firefly", "Adobe Firefly"),
        ("openai", "OpenAI"),
        ("dall", "OpenAI"),
        ("leica m11-p", "Leica M11-P"),
        ("leica", "Leica"),
        ("sony alpha", "Sony Alpha"),
    ];
    for (needle, label) in table {
        if lower.contains(needle) {
            return Some(label.to_string());
        }
    }
    None
}

fn generator_implies_ai(generator: &str) -> bool {
    let lower = generator.to_ascii_lowercase();
    [
        "notebooklm",
        "copilot",
        "firefly",
        "openai",
        "dall",
        "google c2pa",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assertion_with_dst(dst: &str) -> AssertionEntry {
        AssertionEntry {
            digital_source_type: Some(dst.into()),
            ..AssertionEntry::default()
        }
    }

    #[test]
    fn detects_trained_algorithmic_media() {
        let a = assertion_with_dst(
            "http://cv.iptc.org/newscodes/digitalsourcetype/trainedAlgorithmicMedia",
        );
        let d = derive(Some("XIFty Test Suite/0.1"), &[a]);
        assert!(d.ai_generated);
        assert_eq!(d.source_type.as_deref(), Some("trainedAlgorithmicMedia"));
    }

    #[test]
    fn google_generator_label() {
        let d = derive(Some("Google C2PA Core Generator Library"), &[]);
        assert_eq!(d.generator.as_deref(), Some("Google"));
        assert!(d.ai_generated, "google c2pa generator should imply AI");
    }

    #[test]
    fn synthid_disclosure_in_description() {
        let mut a = AssertionEntry::default();
        a.description = Some("Generated with SynthID watermark".into());
        let d = derive(None, &[a]);
        assert!(d.synthid_disclosed);
    }

    #[test]
    fn microsoft_generator_label() {
        let d = derive(Some("Microsoft Copilot Designer 1.0"), &[]);
        assert_eq!(d.generator.as_deref(), Some("Microsoft"));
        assert!(d.ai_generated);
    }

    #[test]
    fn non_ai_generator_does_not_force_flag() {
        let d = derive(Some("Leica M11-P/2.0"), &[]);
        assert_eq!(d.generator.as_deref(), Some("Leica M11-P"));
        assert!(!d.ai_generated);
    }
}
