//! AI-generation prompt namespace decoder.
//!
//! Consumes `(keyword, body)` pairs lifted from PNG `tEXt` / `iTXt` / `zTXt`
//! chunks (or any other text-shaped carrier) and emits normalized
//! `ai_gen.*` `MetadataEntry`s plus parser `Issue`s.
//!
//! Per-shape sub-modules each expose a `try_decode` returning
//! `Option<DecodedAiGen>`; the top-level `decode_payload` dispatches by
//! `AiGenShape` and routes the `parameters` keyword through
//! Fooocus → A1111 fallback.

use xifty_core::{Issue, MetadataEntry};

mod a1111;
mod comfyui;
mod fooocus;
mod invokeai;
mod keys;
mod midjourney;

pub use keys::{NS, SUPPORTED_FIELDS};

/// One AI-shape's worth of decoded data, ready to be merged with siblings.
pub(crate) struct DecodedAiGen {
    pub entries: Vec<MetadataEntry>,
    pub issues: Vec<Issue>,
}

impl DecodedAiGen {
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
            issues: Vec::new(),
        }
    }
}

/// Recognised PNG text-chunk keywords for AI-gen shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiGenShape {
    /// `parameters` keyword, A1111/Forge plaintext shape. When dispatched via
    /// this variant, [`decode_payload`] still tries Fooocus JSON first (the
    /// two share the same keyword on disk), but consumers that have already
    /// disambiguated the body should prefer the explicit
    /// [`AiGenShape::FooocusParameters`] variant instead.
    A1111Parameters,
    /// `parameters` keyword, Fooocus JSON shape. Distinct from
    /// [`AiGenShape::A1111Parameters`] so downstream consumers can route
    /// Fooocus payloads explicitly when the body is already known to be JSON.
    FooocusParameters,
    /// ComfyUI API graph (`prompt` keyword).
    ComfyPrompt,
    /// ComfyUI editor graph (`workflow` keyword).
    ComfyWorkflow,
    /// InvokeAI metadata blob (`invokeai_metadata` keyword).
    InvokeAiMetadata,
    /// Midjourney prompt+UUID under the `Description` keyword.
    MidjourneyDescription,
    /// Midjourney prompt+UUID under the `Comment` keyword.
    MidjourneyComment,
}

/// Classify a text-chunk keyword as an AI-gen shape, if recognised.
pub fn classify_keyword(keyword: &str) -> Option<AiGenShape> {
    match keyword {
        "parameters" => Some(AiGenShape::A1111Parameters),
        "prompt" => Some(AiGenShape::ComfyPrompt),
        "workflow" => Some(AiGenShape::ComfyWorkflow),
        "invokeai_metadata" => Some(AiGenShape::InvokeAiMetadata),
        "Description" => Some(AiGenShape::MidjourneyDescription),
        "Comment" => Some(AiGenShape::MidjourneyComment),
        _ => None,
    }
}

/// Bounded list of `ai_gen.*` field templates (`<i>` / `<key>` placeholders for
/// list-shaped fields). Mirrors `CAPABILITIES.json#/namespaces/ai_gen/supported_tags`.
pub fn supported_fields() -> &'static [&'static str] {
    SUPPORTED_FIELDS
}

/// Per-payload decoder input. `path` and offsets are reused as-is in
/// `Provenance`; callers should set `path` to a stable label such as
/// `"png_text:parameters"` so downstream tooling can identify the source.
#[derive(Debug, Clone)]
pub struct AiGenPayload<'a> {
    pub shape: AiGenShape,
    pub keyword: &'a str,
    pub body: &'a [u8],
    pub container: &'a str,
    pub path: &'a str,
    pub offset_start: u64,
    pub offset_end: u64,
}

/// Decode an AI-gen payload into entries + issues. Never panics; malformed
/// JSON or unknown shapes turn into `Issue`s, never errors.
pub fn decode_payload(payload: AiGenPayload<'_>) -> (Vec<MetadataEntry>, Vec<Issue>) {
    let decoded = match payload.shape {
        AiGenShape::A1111Parameters => {
            // Try Fooocus JSON first; fall back to A1111 plaintext.
            if let Some(d) = fooocus::try_decode(&payload) {
                d
            } else {
                a1111::try_decode(&payload).unwrap_or_else(DecodedAiGen::empty)
            }
        }
        AiGenShape::FooocusParameters => fooocus::try_decode(&payload)
            .or_else(|| a1111::try_decode(&payload))
            .unwrap_or_else(DecodedAiGen::empty),
        AiGenShape::ComfyPrompt | AiGenShape::ComfyWorkflow => {
            comfyui::try_decode(&payload).unwrap_or_else(DecodedAiGen::empty)
        }
        AiGenShape::InvokeAiMetadata => {
            invokeai::try_decode(&payload).unwrap_or_else(DecodedAiGen::empty)
        }
        AiGenShape::MidjourneyDescription | AiGenShape::MidjourneyComment => {
            midjourney::try_decode(&payload).unwrap_or_else(DecodedAiGen::empty)
        }
    };
    (decoded.entries, decoded.issues)
}

// ---------- shared helpers ----------

pub(crate) fn make_provenance(payload: &AiGenPayload<'_>) -> xifty_core::Provenance {
    xifty_core::Provenance {
        container: payload.container.into(),
        namespace: NS.into(),
        path: Some(payload.path.into()),
        offset_start: Some(payload.offset_start),
        offset_end: Some(payload.offset_end),
        notes: Vec::new(),
    }
}

pub(crate) fn entry(
    payload: &AiGenPayload<'_>,
    tag: &str,
    value: xifty_core::TypedValue,
    confidence: f32,
    extra_notes: &[&str],
) -> MetadataEntry {
    let mut notes = vec![format!("confidence: {:.2}", confidence)];
    for n in extra_notes {
        notes.push((*n).to_string());
    }
    MetadataEntry {
        namespace: NS.into(),
        tag_id: tag.into(),
        tag_name: tag.into(),
        value,
        provenance: make_provenance(payload),
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_keyword_recognises_known_shapes() {
        assert_eq!(
            classify_keyword("parameters"),
            Some(AiGenShape::A1111Parameters)
        );
        assert_eq!(classify_keyword("prompt"), Some(AiGenShape::ComfyPrompt));
        assert_eq!(
            classify_keyword("workflow"),
            Some(AiGenShape::ComfyWorkflow)
        );
        assert_eq!(
            classify_keyword("invokeai_metadata"),
            Some(AiGenShape::InvokeAiMetadata)
        );
        assert_eq!(
            classify_keyword("Description"),
            Some(AiGenShape::MidjourneyDescription)
        );
        assert_eq!(
            classify_keyword("Comment"),
            Some(AiGenShape::MidjourneyComment)
        );
    }

    #[test]
    fn classify_keyword_rejects_unknown() {
        assert_eq!(classify_keyword(""), None);
        assert_eq!(classify_keyword("Software"), None);
        assert_eq!(classify_keyword("Raw profile type APP1"), None);
    }

    #[test]
    fn supported_fields_is_non_empty() {
        assert!(!supported_fields().is_empty());
        assert!(supported_fields().iter().any(|f| *f == "ai_gen.prompt"));
    }
}
