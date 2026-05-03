//! Sidecar abstraction.
//!
//! Sibling to the `xifty-container-*` and `xifty-meta-*` crate families: this
//! crate defines the trait + registry that allow XIFty to fold metadata
//! discovered in *external* files (e.g. Sony NRT `.XML`, Panasonic `.CPI`,
//! ARRI `.ale`) into the same `MetadataEntry` stream the embedded decoders
//! produce. Adapters live in their own `xifty-sidecar-<vendor>` crates so
//! shipping a new vendor never requires touching the core abstraction.
//!
//! Phase 1 ships a single adapter (`xifty-sidecar-sony-nrt`) — see issue #122
//! and the parent epic #121.
//!
//! Buffer-only callers always see a no-op: discovery requires a primary path.

use std::path::{Path, PathBuf};

use xifty_core::{Issue, MetadataEntry, Severity};

/// Issue codes emitted by the sidecar layer. They are stable identifiers,
/// surfaced through the standard `report.issues` channel as `info` severity.
///
/// Phase 1 only emits `SIDECAR_UNKNOWN_SCHEMA_VERSION` from the Sony NRT
/// adapter, but the other two codes are defined now so Phase 2's MEDIAPRO
/// cross-clip lookup adapter can reuse them without churn.
pub const SIDECAR_TARGET_MISSING: &str = "sidecar_target_missing";
pub const SIDECAR_NO_INDEX_ENTRY: &str = "sidecar_no_index_entry";
pub const SIDECAR_UNKNOWN_SCHEMA_VERSION: &str = "sidecar_unknown_schema_version";
/// Emitted by a sidecar adapter when its underlying file fails to parse
/// (e.g. malformed XML). Severity is left to the adapter — Sony NRT raises
/// it as `Warning` because the embedded MP4 metadata still flows through.
pub const SIDECAR_PARSE_ERROR: &str = "sidecar_parse_error";

/// Code emitted by the WASM surface when a caller asks for sidecar discovery
/// in a context with no filesystem access. Defined here so adapters and
/// surfaces share the same vocabulary.
pub const SIDECAR_DISCOVERY_UNAVAILABLE_IN_WASM: &str = "sidecar_discovery_unavailable_in_wasm";

/// How an adapter wants its parsed entries reconciled with embedded metadata.
///
/// `Override`: sidecar entries silently replace embedded entries for the same
/// field. The embedded value does NOT reach the conflict detector — only the
/// sidecar value does. Use this for sidecars whose entire purpose is overriding
/// embedded metadata (e.g. Adobe XMP sidecars carrying non-destructive
/// Lightroom edits).
///
/// `Complement`: lets both entries flow through the normal pipeline; the
/// existing conflict-detector flags overlap if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergePolicy {
    Override,
    Complement,
}

/// A reference to a discovered sidecar file that has not yet been read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidecarRef {
    /// Filesystem path to the sidecar.
    pub path: PathBuf,
    /// A short, adapter-defined label (e.g. the schema version, the basename
    /// stem) carried through into provenance notes.
    pub label: String,
}

/// Per-call context handed to `Sidecar::parse`.
///
/// Adapters do not need it today, but stamping it as a struct (instead of
/// individual arguments) keeps the trait stable when later phases need to
/// thread additional state — e.g. the decoded primary file's `Format`.
#[derive(Debug, Clone, Default)]
pub struct MergeContext;

/// What an adapter returns from `parse`.
#[derive(Debug, Clone, Default)]
pub struct SidecarPayload {
    pub entries: Vec<MetadataEntry>,
    pub issues: Vec<Issue>,
}

/// A vendor-specific sidecar parser.
pub trait Sidecar {
    /// Stable adapter name. Surfaces in `MetadataEntry::namespace`.
    fn name(&self) -> &'static str;

    /// Locate sidecar files for `primary_path`. Buffer-only callers pass
    /// `None` and adapters return an empty vector.
    fn discover(&self, primary_path: Option<&Path>) -> Vec<SidecarRef>;

    /// Parse `bytes` (the contents of the file at `sidecar.path`) into
    /// metadata entries + issues.
    fn parse(&self, sidecar: &SidecarRef, bytes: &[u8], ctx: &MergeContext) -> SidecarPayload;

    /// Higher value = considered first when multiple adapters return hits
    /// for the same primary file. Default is `100`.
    fn priority(&self) -> u8 {
        100
    }

    /// How this adapter's entries reconcile with embedded metadata.
    fn merge_policy(&self) -> MergePolicy {
        MergePolicy::Complement
    }
}

/// A single hit from the discovery sweep, carrying enough context for the
/// merge step to call back into the right adapter.
pub struct DiscoveredSidecar<'a> {
    pub adapter: &'a dyn Sidecar,
    pub sidecar: SidecarRef,
}

/// A bag of registered adapters, iterated in priority order.
#[derive(Default)]
pub struct SidecarRegistry {
    adapters: Vec<Box<dyn Sidecar + Send + Sync>>,
}

impl SidecarRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<S>(&mut self, adapter: S) -> &mut Self
    where
        S: Sidecar + Send + Sync + 'static,
    {
        self.adapters.push(Box::new(adapter));
        // Sort descending by priority so iteration order matches.
        self.adapters
            .sort_by(|a, b| b.priority().cmp(&a.priority()));
        self
    }

    pub fn adapters(&self) -> &[Box<dyn Sidecar + Send + Sync>] {
        &self.adapters
    }

    /// Walk every registered adapter, returning every (adapter, sidecar) pair
    /// that adapter reported as a hit. Returns an empty vector when
    /// `primary_path` is `None` (buffer-only callers).
    pub fn discover_for<'a>(&'a self, primary_path: Option<&Path>) -> Vec<DiscoveredSidecar<'a>> {
        let Some(path) = primary_path else {
            return Vec::new();
        };
        let mut hits = Vec::new();
        for adapter in &self.adapters {
            for sidecar in adapter.discover(Some(path)) {
                hits.push(DiscoveredSidecar {
                    adapter: adapter.as_ref(),
                    sidecar,
                });
            }
        }
        hits
    }

    /// Discover, read, parse, and merge every registered adapter's sidecars
    /// into `entries` + `issues`. Honors each adapter's [`MergePolicy`].
    ///
    /// Read errors are converted to `info`-severity issues with the code
    /// `sidecar_target_missing` — never an extraction error.
    pub fn merge_into(
        &self,
        primary_path: Option<&Path>,
        entries: &mut Vec<MetadataEntry>,
        issues: &mut Vec<Issue>,
    ) {
        let ctx = MergeContext;
        for hit in self.discover_for(primary_path) {
            let bytes = match std::fs::read(&hit.sidecar.path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    issues.push(Issue {
                        severity: Severity::Info,
                        code: SIDECAR_TARGET_MISSING.into(),
                        message: format!(
                            "sidecar referenced but unreadable: {} ({error})",
                            hit.sidecar.path.display()
                        ),
                        offset: None,
                        context: Some(hit.adapter.name().into()),
                    });
                    continue;
                }
            };
            let payload = hit.adapter.parse(&hit.sidecar, &bytes, &ctx);
            issues.extend(payload.issues);
            match hit.adapter.merge_policy() {
                MergePolicy::Override => {
                    // Silently replace embedded entries that target the same
                    // tag_name from a different namespace. The embedded value
                    // is dropped before downstream conflict detection runs,
                    // so only the sidecar value survives.
                    for sidecar_entry in payload.entries {
                        entries.retain(|existing| {
                            !(existing.tag_name == sidecar_entry.tag_name
                                && existing.namespace != sidecar_entry.namespace)
                        });
                        entries.push(sidecar_entry);
                    }
                }
                MergePolicy::Complement => {
                    entries.extend(payload.entries);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xifty_core::{Provenance, TypedValue};

    struct FixedAdapter {
        name: &'static str,
        priority: u8,
        policy: MergePolicy,
        discoveries: Vec<SidecarRef>,
        entries: Vec<MetadataEntry>,
    }

    impl Sidecar for FixedAdapter {
        fn name(&self) -> &'static str {
            self.name
        }
        fn discover(&self, primary_path: Option<&Path>) -> Vec<SidecarRef> {
            if primary_path.is_none() {
                return Vec::new();
            }
            self.discoveries.clone()
        }
        fn parse(&self, _: &SidecarRef, _: &[u8], _: &MergeContext) -> SidecarPayload {
            SidecarPayload {
                entries: self.entries.clone(),
                issues: Vec::new(),
            }
        }
        fn priority(&self) -> u8 {
            self.priority
        }
        fn merge_policy(&self) -> MergePolicy {
            self.policy
        }
    }

    fn prov(ns: &str) -> Provenance {
        Provenance {
            container: "sidecar".into(),
            namespace: ns.into(),
            path: None,
            offset_start: None,
            offset_end: None,
            notes: Vec::new(),
        }
    }

    fn entry(ns: &str, tag: &str, value: &str) -> MetadataEntry {
        MetadataEntry {
            namespace: ns.into(),
            tag_id: tag.into(),
            tag_name: tag.into(),
            value: TypedValue::String(value.into()),
            provenance: prov(ns),
            notes: Vec::new(),
        }
    }

    #[test]
    fn buffer_only_is_a_no_op() {
        let mut registry = SidecarRegistry::new();
        registry.register(FixedAdapter {
            name: "fixed",
            priority: 100,
            policy: MergePolicy::Complement,
            discoveries: vec![SidecarRef {
                path: PathBuf::from("/tmp/never-read.xml"),
                label: "x".into(),
            }],
            entries: vec![entry("fixed", "Foo", "bar")],
        });
        let mut entries: Vec<MetadataEntry> = Vec::new();
        let mut issues: Vec<Issue> = Vec::new();
        registry.merge_into(None, &mut entries, &mut issues);
        assert!(entries.is_empty(), "buffer-only must be a no-op");
        assert!(issues.is_empty(), "buffer-only must not emit issues");
    }

    #[test]
    fn missing_sidecar_target_emits_info_issue() {
        let nonexistent = std::env::temp_dir().join("xifty-sidecar-test-missing.xml");
        let _ = std::fs::remove_file(&nonexistent);
        let mut registry = SidecarRegistry::new();
        registry.register(FixedAdapter {
            name: "fixed",
            priority: 100,
            policy: MergePolicy::Complement,
            discoveries: vec![SidecarRef {
                path: nonexistent,
                label: "x".into(),
            }],
            entries: Vec::new(),
        });
        let mut entries: Vec<MetadataEntry> = Vec::new();
        let mut issues: Vec<Issue> = Vec::new();
        let primary = std::env::temp_dir().join("xifty-sidecar-test-primary.bin");
        std::fs::write(&primary, b"x").unwrap();
        registry.merge_into(Some(&primary), &mut entries, &mut issues);
        let _ = std::fs::remove_file(&primary);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, SIDECAR_TARGET_MISSING);
        assert_eq!(issues[0].severity, Severity::Info);
    }

    #[test]
    fn registry_iterates_in_priority_order() {
        let mut registry = SidecarRegistry::new();
        registry.register(FixedAdapter {
            name: "low",
            priority: 10,
            policy: MergePolicy::Complement,
            discoveries: Vec::new(),
            entries: Vec::new(),
        });
        registry.register(FixedAdapter {
            name: "high",
            priority: 200,
            policy: MergePolicy::Complement,
            discoveries: Vec::new(),
            entries: Vec::new(),
        });
        let names: Vec<&str> = registry.adapters().iter().map(|a| a.name()).collect();
        assert_eq!(names, vec!["high", "low"]);
    }

    #[test]
    fn complement_appends_entries_alongside_embedded() {
        let dir = std::env::temp_dir();
        let primary = dir.join("xifty-sidecar-test-complement-primary.bin");
        let sidecar = dir.join("xifty-sidecar-test-complement-sidecar.bin");
        std::fs::write(&primary, b"x").unwrap();
        std::fs::write(&sidecar, b"x").unwrap();

        let mut registry = SidecarRegistry::new();
        registry.register(FixedAdapter {
            name: "fixed",
            priority: 100,
            policy: MergePolicy::Complement,
            discoveries: vec![SidecarRef {
                path: sidecar.clone(),
                label: "x".into(),
            }],
            entries: vec![entry("sony_nrt", "CaptureFps", "59.94")],
        });

        let mut entries = vec![entry("quicktime", "CaptureFps", "59.94")];
        let mut issues = Vec::new();
        registry.merge_into(Some(&primary), &mut entries, &mut issues);

        let _ = std::fs::remove_file(&primary);
        let _ = std::fs::remove_file(&sidecar);

        assert_eq!(entries.len(), 2);
        let namespaces: Vec<&str> = entries.iter().map(|e| e.namespace.as_str()).collect();
        assert!(namespaces.contains(&"quicktime"));
        assert!(namespaces.contains(&"sony_nrt"));
    }

    #[test]
    fn override_drops_competing_embedded_entries() {
        let dir = std::env::temp_dir();
        let primary = dir.join("xifty-sidecar-test-override-primary.bin");
        let sidecar = dir.join("xifty-sidecar-test-override-sidecar.bin");
        std::fs::write(&primary, b"x").unwrap();
        std::fs::write(&sidecar, b"x").unwrap();

        let mut registry = SidecarRegistry::new();
        registry.register(FixedAdapter {
            name: "fixed",
            priority: 100,
            policy: MergePolicy::Override,
            discoveries: vec![SidecarRef {
                path: sidecar.clone(),
                label: "x".into(),
            }],
            entries: vec![entry("sony_nrt", "CaptureFps", "59.94")],
        });

        let mut entries = vec![entry("quicktime", "CaptureFps", "29.97")];
        let mut issues = Vec::new();
        registry.merge_into(Some(&primary), &mut entries, &mut issues);

        let _ = std::fs::remove_file(&primary);
        let _ = std::fs::remove_file(&sidecar);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].namespace, "sony_nrt");
    }
}
