//! Bounded `c2pa.*` and `ai.*` field surface.

/// Namespace for C2PA-specific fields.
pub const NS: &str = "c2pa";

/// Namespace for cross-cutting AI-derived fields. Populated from C2PA
/// assertions today; future XMP `Iptc4xmpExt:DigitalSourceType` will populate
/// the same names.
pub const NS_AI: &str = "ai";

/// Bounded list of stable `c2pa.*` tag templates. List-shaped fields use
/// numeric `<i>` infixes per the flat-`TypedValue` constraint.
pub const SUPPORTED_FIELDS: &[&str] = &[
    "c2pa.claim_generator",
    "c2pa.format",
    "c2pa.instance_id",
    "c2pa.signature.alg",
    "c2pa.signature.issuer",
    "c2pa.signature.verified",
    "c2pa.assertions.<i>.label",
    "c2pa.assertions.<i>.action",
    "c2pa.assertions.<i>.description",
    "c2pa.assertions.<i>.digitalSourceType",
    "c2pa.assertions.<i>.softwareAgent",
    "c2pa.assertions.<i>.when",
    "c2pa.ingredients.<i>.title",
    "c2pa.ingredients.<i>.format",
    "c2pa.ingredients.<i>.relationship",
    "c2pa.ai_generated",
];

/// Cross-cutting `ai.*` derived field templates.
pub const SUPPORTED_FIELDS_AI: &[&str] =
    &["ai.source_type", "ai.generator", "ai.synthid_disclosed"];
