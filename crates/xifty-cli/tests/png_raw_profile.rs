//! Integration test for issue #136: PNG `Raw profile type *` decoder family.
//!
//! Loads `fixtures/minimal/raw_profile_app1.png` (synthesised by
//! `tools/gen_raw_profile_fixtures.py` from `fixtures/minimal/happy.jpg`) and
//! verifies the dispatcher in `xifty-cli` walks
//! `xifty_container_png::decode_raw_profile` and feeds the resulting bytes
//! into `xifty-meta-exif` so EXIF entries surface with PNG provenance.

use std::path::Path;
use xifty_core::ViewMode;

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/minimal")
        .join(name)
}

#[test]
fn png_raw_profile_app1_surfaces_exif_with_png_provenance() {
    let analysis = xifty_cli::extract_path(fixture("raw_profile_app1.png"), ViewMode::Raw)
        .expect("synthetic raw_profile_app1.png decodes");
    let metadata = analysis
        .raw
        .as_ref()
        .map(|raw| raw.metadata.as_slice())
        .unwrap_or(&[]);

    let exif_entries: Vec<_> = metadata
        .iter()
        .filter(|entry| entry.namespace == "exif")
        .collect();

    assert!(
        !exif_entries.is_empty(),
        "expected at least one EXIF entry from `Raw profile type APP1` chunk; got entries: {:?}",
        metadata
            .iter()
            .map(|e| (&e.namespace, &e.tag_name))
            .collect::<Vec<_>>()
    );

    for entry in &exif_entries {
        assert_eq!(
            entry.provenance.container, "png",
            "exif entry should be attributed to png container"
        );
        let path = entry.provenance.path.as_deref().unwrap_or_default();
        assert!(
            path.starts_with("png_profile_app1"),
            "exif entry provenance.path should start with png_profile_app1, got {:?}",
            path
        );
        assert!(
            entry
                .provenance
                .notes
                .iter()
                .any(|n| n.contains("Raw profile type")),
            "exif entry should carry a `Raw profile type` provenance note; got {:?}",
            entry.provenance.notes
        );
    }
}
