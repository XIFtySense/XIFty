//! C2PA (Content Provenance and Authenticity, ISO 22144) JUMBF manifest
//! decoder. Parse-only (level A): structural fields and signature metadata
//! are surfaced; cryptographic verification is left for a future level-B
//! follow-up. `c2pa.signature.verified` is always emitted as
//! `TypedValue::String("unknown")` so the schema is forward-compatible.
//!
//! Public surface:
//! - [`decode_jumbf`] — consumes a contiguous JUMBF byte stream (the payload
//!   of a PNG `caBX` chunk, a reassembled JPEG APP11 manifest, or an
//!   ISOBMFF C2PA `uuid` box) and returns flat `c2pa.*` / `ai.*`
//!   `MetadataEntry`s plus `Issue`s.
//! - [`NS`] / [`NS_AI`] — namespace constants.
//! - [`SUPPORTED_FIELDS`] / [`SUPPORTED_FIELDS_AI`] — bounded field templates
//!   used by `CAPABILITIES.json`.

use xifty_core::{Issue, MetadataEntry, Provenance, Severity, TypedValue};

mod assertion;
mod cbor;
mod claim;
mod derive;
pub mod jumbf;
mod keys;
mod signature;

pub use keys::{NS, NS_AI, SUPPORTED_FIELDS, SUPPORTED_FIELDS_AI};

/// Decoder input. `path` and offsets are reused as-is in `Provenance`.
#[derive(Debug, Clone)]
pub struct C2paPayload<'a> {
    pub bytes: &'a [u8],
    pub container: &'a str,
    pub path: &'a str,
    pub offset_start: u64,
    pub offset_end: u64,
}

/// Decode one JUMBF byte stream into entries + issues. Never panics.
pub fn decode_jumbf(payload: C2paPayload<'_>) -> (Vec<MetadataEntry>, Vec<Issue>) {
    let entries: Vec<MetadataEntry> = Vec::new();
    let mut issues = Vec::new();
    if payload.bytes.is_empty() {
        issues.push(Issue {
            severity: Severity::Info,
            code: "c2pa_jumbf_empty".into(),
            message: "C2PA payload was empty".into(),
            offset: Some(payload.offset_start),
            context: Some(payload.path.into()),
        });
        return (entries, issues);
    }

    let (boxes, parse_issues) = jumbf::walk(payload.bytes);
    issues.extend(parse_issues);

    // Top-level C2PA superbox.
    let Some(top) = boxes.iter().find(|b| {
        b.uuid
            .map(|u| u == jumbf::UUID_C2PA)
            .unwrap_or_else(|| matches!(&b.label, Some(l) if l == "c2pa"))
    }) else {
        // Some streams hand us the manifest superbox directly. Fall back to
        // walking whatever we have.
        return finish(&payload, &boxes, entries, issues);
    };
    if let jumbf::BoxPayload::Children(children) = &top.payload {
        finish(&payload, children, entries, issues)
    } else {
        (entries, issues)
    }
}

fn finish(
    payload: &C2paPayload<'_>,
    manifests: &[jumbf::JumbfBox<'_>],
    mut entries: Vec<MetadataEntry>,
    mut issues: Vec<Issue>,
) -> (Vec<MetadataEntry>, Vec<Issue>) {
    let mut claim_generator: Option<String> = None;
    let mut format: Option<String> = None;
    let mut instance_id: Option<String> = None;
    let mut sig_alg: Option<String> = None;
    let mut sig_issuer: Option<String> = None;
    let mut assertion_entries: Vec<assertion::AssertionEntry> = Vec::new();

    for manifest in manifests {
        let manifest_children = match &manifest.payload {
            jumbf::BoxPayload::Children(c) => c.as_slice(),
            _ => continue,
        };
        for child in manifest_children {
            match (child.uuid, child.label.as_deref()) {
                (Some(u), _) if u == jumbf::UUID_C2CL => {
                    if let Some(bytes) = child_data_box_cbor(child) {
                        if let Some(info) = claim::decode(bytes) {
                            claim_generator = info.claim_generator.or(claim_generator);
                            format = info.format.or(format);
                            instance_id = info.instance_id.or(instance_id);
                        }
                    }
                }
                (Some(u), _) if u == jumbf::UUID_C2AS => {
                    if let jumbf::BoxPayload::Children(items) = &child.payload {
                        for item in items {
                            if let Some(bytes) = child_data_box_cbor(item) {
                                let label = item.label.clone();
                                let decoded = assertion::decode(label.as_deref(), bytes);
                                assertion_entries.extend(decoded);
                            }
                        }
                    }
                }
                (Some(u), _) if u == jumbf::UUID_C2CS => {
                    if let Some(bytes) = child_data_box_cbor(child) {
                        let info = signature::decode(bytes);
                        sig_alg = info.alg.or(sig_alg);
                        sig_issuer = info.issuer.or(sig_issuer);
                    }
                }
                _ => {}
            }
        }
    }

    if let Some(v) = &claim_generator {
        entries.push(entry(payload, "c2pa.claim_generator", string(v)));
    }
    if let Some(v) = &format {
        entries.push(entry(payload, "c2pa.format", string(v)));
    }
    if let Some(v) = &instance_id {
        entries.push(entry(payload, "c2pa.instance_id", string(v)));
    }
    if let Some(v) = &sig_alg {
        entries.push(entry(payload, "c2pa.signature.alg", string(v)));
    }
    entries.push(entry(
        payload,
        "c2pa.signature.issuer",
        string(sig_issuer.as_deref().unwrap_or("unknown")),
    ));
    entries.push(entry(payload, "c2pa.signature.verified", string("unknown")));

    for (i, a) in assertion_entries.iter().enumerate() {
        push_optional(
            &mut entries,
            payload,
            &format!("c2pa.assertions.{i}.label"),
            a.label.as_deref(),
        );
        push_optional(
            &mut entries,
            payload,
            &format!("c2pa.assertions.{i}.action"),
            a.action.as_deref(),
        );
        push_optional(
            &mut entries,
            payload,
            &format!("c2pa.assertions.{i}.description"),
            a.description.as_deref(),
        );
        push_optional(
            &mut entries,
            payload,
            &format!("c2pa.assertions.{i}.digitalSourceType"),
            a.digital_source_type.as_deref(),
        );
        push_optional(
            &mut entries,
            payload,
            &format!("c2pa.assertions.{i}.softwareAgent"),
            a.software_agent.as_deref(),
        );
        push_optional(
            &mut entries,
            payload,
            &format!("c2pa.assertions.{i}.when"),
            a.when.as_deref(),
        );
    }

    let derived = derive::derive(claim_generator.as_deref(), &assertion_entries);
    entries.push(entry(
        payload,
        "c2pa.ai_generated",
        TypedValue::String(derived.ai_generated.to_string()),
    ));
    if let Some(v) = &derived.source_type {
        entries.push(ai_entry(payload, "ai.source_type", string(v)));
    }
    if let Some(v) = &derived.generator {
        entries.push(ai_entry(payload, "ai.generator", string(v)));
    }
    if derived.synthid_disclosed {
        entries.push(ai_entry(
            payload,
            "ai.synthid_disclosed",
            TypedValue::String("true".into()),
        ));
    }

    if entries.is_empty() {
        issues.push(Issue {
            severity: Severity::Info,
            code: "c2pa_jumbf_empty".into(),
            message: "C2PA JUMBF stream produced no recognized fields".into(),
            offset: Some(payload.offset_start),
            context: Some(payload.path.into()),
        });
    }

    (entries, issues)
}

/// A C2PA claim/assertion/signature box wraps a single content data box
/// (typically `cbor` for these three; `json` is also valid for assertions).
/// Return the CBOR or JSON payload bytes if present.
fn child_data_box_cbor<'a>(parent: &jumbf::JumbfBox<'a>) -> Option<&'a [u8]> {
    if let jumbf::BoxPayload::Children(children) = &parent.payload {
        for child in children {
            match &child.payload {
                jumbf::BoxPayload::Cbor(b) | jumbf::BoxPayload::Json(b) => return Some(*b),
                _ => {}
            }
        }
    }
    None
}

fn provenance(payload: &C2paPayload<'_>, namespace: &str) -> Provenance {
    Provenance {
        container: payload.container.into(),
        namespace: namespace.into(),
        path: Some(payload.path.into()),
        offset_start: Some(payload.offset_start),
        offset_end: Some(payload.offset_end),
        notes: Vec::new(),
    }
}

fn entry(payload: &C2paPayload<'_>, tag: &str, value: TypedValue) -> MetadataEntry {
    MetadataEntry {
        namespace: NS.into(),
        tag_id: tag.into(),
        tag_name: tag.into(),
        value,
        provenance: provenance(payload, NS),
        notes: Vec::new(),
    }
}

fn ai_entry(payload: &C2paPayload<'_>, tag: &str, value: TypedValue) -> MetadataEntry {
    MetadataEntry {
        namespace: NS_AI.into(),
        tag_id: tag.into(),
        tag_name: tag.into(),
        value,
        provenance: provenance(payload, NS_AI),
        notes: Vec::new(),
    }
}

fn push_optional(
    entries: &mut Vec<MetadataEntry>,
    payload: &C2paPayload<'_>,
    tag: &str,
    value: Option<&str>,
) {
    if let Some(v) = value {
        entries.push(entry(payload, tag, string(v)));
    }
}

fn string(s: &str) -> TypedValue {
    TypedValue::String(s.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_payload_emits_info_issue() {
        let (entries, issues) = decode_jumbf(C2paPayload {
            bytes: &[],
            container: "png",
            path: "caBX",
            offset_start: 0,
            offset_end: 0,
        });
        assert!(entries.is_empty());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "c2pa_jumbf_empty");
    }
}
