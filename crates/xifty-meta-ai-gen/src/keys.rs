//! Bounded `ai_gen.*` field surface.
//!
//! Surface is flat — `TypedValue` has no array/object variants — so list
//! fields like `lora` / `controlnet` are flattened with numeric infixes
//! (`ai_gen.lora.<i>.name`, `ai_gen.lora.<i>.weight`).

/// Namespace string used in `MetadataEntry::namespace` and `Provenance::namespace`.
pub const NS: &str = "ai_gen";

/// Bounded list of stable `ai_gen.*` tag names. Used by `supported_fields()`
/// for cross-checking against `CAPABILITIES.json`. List-shaped fields
/// (`lora.<i>.*`, `controlnet.<i>.*`) are represented by their template.
pub const SUPPORTED_FIELDS: &[&str] = &[
    "ai_gen.tool",
    "ai_gen.prompt",
    "ai_gen.negative_prompt",
    "ai_gen.seed",
    "ai_gen.steps",
    "ai_gen.cfg_scale",
    "ai_gen.sampler",
    "ai_gen.scheduler",
    "ai_gen.model.name",
    "ai_gen.model.hash",
    "ai_gen.dimensions.width",
    "ai_gen.dimensions.height",
    "ai_gen.lora.<i>.name",
    "ai_gen.lora.<i>.weight",
    "ai_gen.controlnet.<i>.name",
    "ai_gen.controlnet.<i>.weight",
    "ai_gen.midjourney.job_id",
    "ai_gen.workflow_json",
    "ai_gen.tool_extra.<key>",
];
