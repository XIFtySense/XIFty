use serde::Serialize;
use std::path::PathBuf;

pub const SCHEMA_VERSION: &str = "0.1.0";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Jpeg,
    Tiff,
    Dng,
    Png,
    Webp,
    Heif,
    Mp4,
    Mov,
    M4a,
    Flac,
    Aiff,
}

impl Format {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Jpeg => "jpeg",
            Self::Tiff => "tiff",
            Self::Dng => "dng",
            Self::Png => "png",
            Self::Webp => "webp",
            Self::Heif => "heif",
            Self::Mp4 => "mp4",
            Self::Mov => "mov",
            Self::M4a => "m4a",
            Self::Flac => "flac",
            Self::Aiff => "aiff",
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SourceRef {
    pub path: PathBuf,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Issue {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Conflict {
    pub field: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<ConflictSide>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ConflictSide {
    pub namespace: String,
    pub tag_id: String,
    pub tag_name: String,
    pub value: TypedValue,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Provenance {
    pub container: String,
    pub namespace: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset_start: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset_end: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum TypedValue {
    String(String),
    Integer(i64),
    Float(f64),
    Rational { numerator: i64, denominator: i64 },
    RationalList(Vec<RationalValue>),
    Bytes(Vec<u8>),
    Timestamp(String),
    Coordinates { latitude: f64, longitude: f64 },
    Dimensions { width: u32, height: u32 },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RationalValue {
    pub numerator: i64,
    pub denominator: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MetadataEntry {
    pub namespace: String,
    pub tag_id: String,
    pub tag_name: String,
    pub value: TypedValue,
    pub provenance: Provenance,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ContainerNode {
    pub kind: String,
    pub label: String,
    pub offset_start: u64,
    pub offset_end: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct NormalizedField {
    pub field: String,
    pub value: TypedValue,
    pub confidence: f32,
    pub sources: Vec<Provenance>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProbeInput {
    pub path: PathBuf,
    pub detected_format: String,
    pub container: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProbeOutput {
    pub schema_version: String,
    pub input: ProbeInput,
    pub containers: Vec<ContainerNode>,
    pub report: Report,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct RawView {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub containers: Vec<ContainerNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metadata: Vec<MetadataEntry>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct InterpretedView {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metadata: Vec<MetadataEntry>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct NormalizedView {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<NormalizedField>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct Report {
    #[serde(default)]
    pub issues: Vec<Issue>,
    #[serde(default)]
    pub conflicts: Vec<Conflict>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AnalysisOutput {
    pub schema_version: String,
    pub input: ProbeInput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<RawView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interpreted: Option<InterpretedView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normalized: Option<NormalizedView>,
    pub report: Report,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Full,
    Raw,
    Interpreted,
    Normalized,
    Report,
}

#[derive(Debug, Error)]
pub enum XiftyError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported format")]
    UnsupportedFormat,
    #[error("parse error: {message}")]
    Parse { message: String },
}

pub fn issue(severity: Severity, code: impl Into<String>, message: impl Into<String>) -> Issue {
    Issue {
        severity,
        code: code.into(),
        message: message.into(),
        offset: None,
        context: None,
    }
}

use thiserror::Error;
