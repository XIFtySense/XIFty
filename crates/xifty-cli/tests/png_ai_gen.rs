//! Integration tests for issue #137: PNG AI-gen text-chunk decoders.
//!
//! Drives the CLI dispatcher in `xifty-cli` over the synthetic PNG fixtures
//! produced by `tools/gen_ai_gen_fixtures.py` and asserts that
//! `ai_gen.*` entries surface with `container = "png"` provenance.

use std::path::Path;
use xifty_core::ViewMode;

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/minimal")
        .join(name)
}

fn ai_gen_entries(analysis: &xifty_core::AnalysisOutput) -> Vec<(String, String)> {
    analysis
        .raw
        .as_ref()
        .map(|raw| raw.metadata.as_slice())
        .unwrap_or(&[])
        .iter()
        .filter(|e| e.namespace == "ai_gen")
        .map(|e| (e.tag_name.clone(), format!("{:?}", e.value)))
        .collect()
}

fn assert_png_provenance(analysis: &xifty_core::AnalysisOutput) {
    let metadata = analysis
        .raw
        .as_ref()
        .map(|raw| raw.metadata.as_slice())
        .unwrap_or(&[]);
    for entry in metadata.iter().filter(|e| e.namespace == "ai_gen") {
        assert_eq!(entry.provenance.container, "png");
        assert_eq!(entry.provenance.namespace, "ai_gen");
        let path = entry.provenance.path.as_deref().unwrap_or_default();
        assert!(
            path.starts_with("png_text:"),
            "ai_gen entry path should start with png_text:, got {path:?}"
        );
    }
}

#[test]
fn a1111_parameters_surface_ai_gen_entries() {
    let analysis = xifty_cli::extract_path(fixture("ai_gen_a1111.png"), ViewMode::Raw)
        .expect("ai_gen_a1111.png decodes");
    let entries = ai_gen_entries(&analysis);
    let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
    for tag in [
        "ai_gen.tool",
        "ai_gen.prompt",
        "ai_gen.negative_prompt",
        "ai_gen.steps",
        "ai_gen.sampler",
        "ai_gen.cfg_scale",
        "ai_gen.seed",
        "ai_gen.dimensions.width",
        "ai_gen.dimensions.height",
        "ai_gen.model.name",
        "ai_gen.model.hash",
    ] {
        assert!(names.contains(&tag), "missing {tag}; got {names:?}");
    }
    assert_png_provenance(&analysis);
}

#[test]
fn comfyui_prompt_and_workflow_surface_ai_gen_entries() {
    let analysis = xifty_cli::extract_path(fixture("ai_gen_comfyui.png"), ViewMode::Raw)
        .expect("ai_gen_comfyui.png decodes");
    let entries = ai_gen_entries(&analysis);
    let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
    for tag in [
        "ai_gen.tool",
        "ai_gen.prompt",
        "ai_gen.negative_prompt",
        "ai_gen.seed",
        "ai_gen.steps",
        "ai_gen.cfg_scale",
        "ai_gen.sampler",
        "ai_gen.scheduler",
        "ai_gen.model.name",
        "ai_gen.lora.0.name",
        "ai_gen.lora.0.weight",
        "ai_gen.workflow_json",
    ] {
        assert!(names.contains(&tag), "missing {tag}; got {names:?}");
    }
    assert_png_provenance(&analysis);
}

#[test]
fn invokeai_metadata_surfaces_ai_gen_entries() {
    let analysis = xifty_cli::extract_path(fixture("ai_gen_invokeai.png"), ViewMode::Raw)
        .expect("ai_gen_invokeai.png decodes");
    let entries = ai_gen_entries(&analysis);
    let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
    for tag in [
        "ai_gen.tool",
        "ai_gen.prompt",
        "ai_gen.negative_prompt",
        "ai_gen.seed",
        "ai_gen.steps",
        "ai_gen.cfg_scale",
        "ai_gen.scheduler",
        "ai_gen.model.name",
        "ai_gen.model.hash",
        "ai_gen.dimensions.width",
        "ai_gen.dimensions.height",
        "ai_gen.lora.0.name",
        "ai_gen.lora.0.weight",
        "ai_gen.controlnet.0.name",
        "ai_gen.controlnet.0.weight",
    ] {
        assert!(names.contains(&tag), "missing {tag}; got {names:?}");
    }
    assert_png_provenance(&analysis);
}

#[test]
fn midjourney_description_surfaces_prompt_and_job_id() {
    let analysis = xifty_cli::extract_path(fixture("ai_gen_midjourney.png"), ViewMode::Raw)
        .expect("ai_gen_midjourney.png decodes");
    let entries = ai_gen_entries(&analysis);
    let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
    for tag in ["ai_gen.tool", "ai_gen.prompt", "ai_gen.midjourney.job_id"] {
        assert!(names.contains(&tag), "missing {tag}; got {names:?}");
    }
    assert_png_provenance(&analysis);
}

#[test]
fn fooocus_parameters_chosen_over_a1111_fallback() {
    let analysis = xifty_cli::extract_path(fixture("ai_gen_fooocus.png"), ViewMode::Raw)
        .expect("ai_gen_fooocus.png decodes");
    let entries = ai_gen_entries(&analysis);
    let tool_value = entries
        .iter()
        .find(|(n, _)| n == "ai_gen.tool")
        .expect("ai_gen.tool emitted");
    assert!(
        tool_value.1.contains("fooocus"),
        "expected Fooocus shape; got {tool_value:?}"
    );
    let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
    for tag in [
        "ai_gen.prompt",
        "ai_gen.seed",
        "ai_gen.cfg_scale",
        "ai_gen.model.name",
        "ai_gen.dimensions.width",
        "ai_gen.lora.0.name",
    ] {
        assert!(names.contains(&tag), "missing {tag}; got {names:?}");
    }
    assert_png_provenance(&analysis);
}
