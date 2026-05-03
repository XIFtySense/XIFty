//! C2PA external manifest (`<basename>.c2pa`) sidecar adapter.
//!
//! When a C2PA-aware producer cannot embed a manifest into the asset bytes
//! (legacy formats, intermediate exports), it deposits the JUMBF byte stream
//! into a sibling file with the `.c2pa` extension. This adapter discovers
//! that file and routes its bytes through [`xifty_meta_c2pa::decode_jumbf`].
//! The decoder's `c2pa.*` entries are re-namespaced to `c2pa_sidecar` so
//! consumers can disambiguate embedded vs sidecar provenance; cross-cutting
//! `ai.*` entries keep their namespace.

use std::path::Path;

use xifty_core::{Issue, MetadataEntry, Provenance};
use xifty_meta_c2pa::{C2paPayload, NS, decode_jumbf};
use xifty_sidecar::{MergeContext, MergePolicy, Sidecar, SidecarPayload, SidecarRef};

const NAMESPACE: &str = "c2pa_sidecar";
const CONTAINER: &str = "sidecar";

#[derive(Debug, Default)]
pub struct C2paSidecar;

impl C2paSidecar {
    pub const fn new() -> Self {
        Self
    }
}

impl Sidecar for C2paSidecar {
    fn name(&self) -> &'static str {
        NAMESPACE
    }

    fn discover(&self, primary_path: Option<&Path>) -> Vec<SidecarRef> {
        let Some(primary) = primary_path else {
            return Vec::new();
        };
        discover_in_dir(primary)
    }

    fn parse(&self, sidecar: &SidecarRef, bytes: &[u8], _ctx: &MergeContext) -> SidecarPayload {
        let path_label = sidecar.path.to_string_lossy().into_owned();
        let (mut entries, issues) = decode_jumbf(C2paPayload {
            bytes,
            container: CONTAINER,
            path: &path_label,
            offset_start: 0,
            offset_end: bytes.len() as u64,
        });
        for entry in &mut entries {
            relabel(entry);
        }
        SidecarPayload { entries, issues }
    }

    fn priority(&self) -> u8 {
        100
    }

    fn merge_policy(&self) -> MergePolicy {
        MergePolicy::Complement
    }
}

/// Re-stamp `c2pa.*` entries into the `c2pa_sidecar` namespace. Tag ids /
/// names retain their `c2pa.*` prefix so downstream consumers can index by
/// either dimension. Cross-cutting `ai.*` entries are left untouched.
fn relabel(entry: &mut MetadataEntry) {
    if entry.namespace == NS {
        entry.namespace = NAMESPACE.into();
        let prov = &mut entry.provenance;
        prov.namespace = NAMESPACE.into();
        prov.container = CONTAINER.into();
    } else {
        // ai.* entries stay in the ai namespace, but their provenance still
        // reflects the sidecar source.
        entry.provenance.container = CONTAINER.into();
    }
}

fn discover_in_dir(primary: &Path) -> Vec<SidecarRef> {
    let Some(dir) = primary.parent() else {
        return Vec::new();
    };
    let Some(stem) = primary.file_stem().and_then(|s| s.to_str()) else {
        return Vec::new();
    };
    let read = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    };
    for entry in read.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(dot) = name.rfind('.') else {
            continue;
        };
        let entry_stem = &name[..dot];
        let entry_ext = &name[dot + 1..];
        if entry_ext.eq_ignore_ascii_case("c2pa") && entry_stem.eq_ignore_ascii_case(stem) {
            return vec![SidecarRef {
                path,
                label: "c2pa".into(),
            }];
        }
    }
    Vec::new()
}

// Silences "unused" warning for Provenance import when feature gates change.
const _: fn() -> Option<Provenance> = || None;
const _: fn() -> Option<Issue> = || None;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn discover_finds_c2pa_sibling() {
        let dir = std::env::temp_dir().join("xifty-sidecar-c2pa-discover");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let primary = dir.join("clip.jpg");
        std::fs::write(&primary, b"x").unwrap();
        std::fs::write(dir.join("clip.c2pa"), b"x").unwrap();
        let sidecar = C2paSidecar::new();
        let hits = sidecar.discover(Some(&primary));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].label, "c2pa");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn discover_buffer_only_returns_empty() {
        let sidecar = C2paSidecar::new();
        assert!(sidecar.discover(None).is_empty());
    }

    #[test]
    fn discover_case_insensitive_extension() {
        let dir = std::env::temp_dir().join("xifty-sidecar-c2pa-case");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let primary = dir.join("clip.JPG");
        std::fs::write(&primary, b"x").unwrap();
        std::fs::write(dir.join("clip.C2PA"), b"x").unwrap();
        let sidecar = C2paSidecar::new();
        let hits = sidecar.discover(Some(&primary));
        assert_eq!(hits.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_bytes_returns_empty_entries_with_info_issue() {
        let sidecar = C2paSidecar::new();
        let payload = sidecar.parse(
            &SidecarRef {
                path: PathBuf::from("/tmp/x.c2pa"),
                label: "c2pa".into(),
            },
            &[],
            &MergeContext,
        );
        assert!(payload.entries.is_empty());
        assert_eq!(payload.issues.len(), 1);
        assert_eq!(payload.issues[0].code, "c2pa_jumbf_empty");
    }
}
