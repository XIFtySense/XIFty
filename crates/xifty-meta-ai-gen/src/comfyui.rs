//! ComfyUI graph-walker.
//!
//! Two flavours of JSON ride on PNG `tEXt` chunks:
//!
//! - `prompt` keyword — API JSON (object map of `node_id → {class_type, inputs}`).
//! - `workflow` keyword — editor JSON (graph with `nodes[]`).
//!
//! We accept both. The `prompt` (API) shape is the richest and is the
//! primary source for prompts/seed/steps/sampler. The `workflow` JSON is
//! preserved verbatim as `ai_gen.workflow_json` (as bytes — `TypedValue`
//! has no JSON variant; we mark it with `format: json`).
//!
//! Prompt-role disambiguation: `KSampler.inputs.positive` and
//! `KSampler.inputs.negative` reference upstream nodes by `[node_id, slot]`.
//! When they resolve to `CLIPTextEncode` nodes, we emit `ai_gen.prompt` and
//! `ai_gen.negative_prompt` with confidence 0.95. If link resolution is
//! ambiguous we fall back to the heuristic "first `CLIPTextEncode` is
//! positive, second is negative" and emit a `comfy_prompt_role: heuristic`
//! note.

use crate::{AiGenPayload, AiGenShape, DecodedAiGen, entry};
use serde_json::Value;
use xifty_core::{Issue, Severity, TypedValue};

pub(super) fn try_decode(payload: &AiGenPayload<'_>) -> Option<DecodedAiGen> {
    let value: Value = match serde_json::from_slice(payload.body) {
        Ok(v) => v,
        Err(e) => {
            return Some(DecodedAiGen {
                entries: Vec::new(),
                issues: vec![Issue {
                    severity: Severity::Warning,
                    code: "ai_gen_json_invalid".into(),
                    message: format!("ComfyUI JSON could not be parsed: {}", e),
                    offset: Some(payload.offset_start),
                    context: Some(payload.keyword.into()),
                }],
            });
        }
    };

    let mut decoded = DecodedAiGen::empty();

    match payload.shape {
        AiGenShape::ComfyPrompt => decode_api_graph(&value, payload, &mut decoded),
        AiGenShape::ComfyWorkflow => {
            // The `workflow` keyword can also be an API-shape JSON in some
            // exporters; try API first, fall back to preserving the raw JSON.
            if value.is_object()
                && value.as_object().map_or(false, |o| {
                    o.values()
                        .any(|v| v.get("class_type").and_then(|c| c.as_str()).is_some())
                })
            {
                decode_api_graph(&value, payload, &mut decoded);
            }
            // Preserve raw workflow JSON.
            decoded.entries.push(entry(
                payload,
                "ai_gen.workflow_json",
                TypedValue::Bytes(payload.body.to_vec()),
                0.95,
                &["format: json"],
            ));
        }
        _ => {}
    }

    decoded.entries.push(entry(
        payload,
        "ai_gen.tool",
        TypedValue::String("comfyui".to_string()),
        0.95,
        &[],
    ));

    Some(decoded)
}

fn decode_api_graph(value: &Value, payload: &AiGenPayload<'_>, decoded: &mut DecodedAiGen) {
    let Some(obj) = value.as_object() else {
        return;
    };

    // First pass: collect typed nodes (by class_type) for cross-reference.
    let mut clip_text_nodes: Vec<(&String, String)> = Vec::new();
    let mut checkpoint_loader: Option<&serde_json::Map<String, Value>> = None;
    let mut lora_loaders: Vec<&serde_json::Map<String, Value>> = Vec::new();

    for (node_id, node_value) in obj {
        let Some(node) = node_value.as_object() else {
            continue;
        };
        let class_type = match node.get("class_type").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => continue,
        };
        match class_type {
            "CLIPTextEncode" => {
                let text = node
                    .get("inputs")
                    .and_then(|v| v.get("text"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                clip_text_nodes.push((node_id, text));
            }
            "CheckpointLoaderSimple" | "CheckpointLoader" => {
                checkpoint_loader = Some(node);
            }
            "LoraLoader" => {
                lora_loaders.push(node);
            }
            _ => {}
        }
    }

    // Second pass: pull KSampler params and resolve link refs.
    let mut positive_ref: Option<String> = None;
    let mut negative_ref: Option<String> = None;
    for (_node_id, node_value) in obj {
        let Some(node) = node_value.as_object() else {
            continue;
        };
        let Some(class_type) = node.get("class_type").and_then(|v| v.as_str()) else {
            continue;
        };
        if class_type != "KSampler" && class_type != "KSamplerAdvanced" {
            continue;
        }
        let Some(inputs) = node.get("inputs").and_then(|v| v.as_object()) else {
            continue;
        };
        if let Some(seed) = inputs
            .get("seed")
            .and_then(|v| v.as_i64())
            .or_else(|| inputs.get("noise_seed").and_then(|v| v.as_i64()))
        {
            decoded.entries.push(entry(
                payload,
                "ai_gen.seed",
                TypedValue::Integer(seed),
                0.95,
                &[],
            ));
        }
        if let Some(steps) = inputs.get("steps").and_then(|v| v.as_i64()) {
            decoded.entries.push(entry(
                payload,
                "ai_gen.steps",
                TypedValue::Integer(steps),
                0.95,
                &[],
            ));
        }
        if let Some(cfg) = inputs.get("cfg").and_then(|v| v.as_f64()) {
            decoded.entries.push(entry(
                payload,
                "ai_gen.cfg_scale",
                TypedValue::Float(cfg),
                0.95,
                &[],
            ));
        }
        if let Some(s) = inputs.get("sampler_name").and_then(|v| v.as_str()) {
            decoded.entries.push(entry(
                payload,
                "ai_gen.sampler",
                TypedValue::String(s.to_string()),
                0.95,
                &[],
            ));
        }
        if let Some(s) = inputs.get("scheduler").and_then(|v| v.as_str()) {
            decoded.entries.push(entry(
                payload,
                "ai_gen.scheduler",
                TypedValue::String(s.to_string()),
                0.95,
                &[],
            ));
        }
        positive_ref = link_target(inputs.get("positive"));
        negative_ref = link_target(inputs.get("negative"));
        break;
    }

    // Resolve prompt nodes.
    let (positive_text, negative_text, role_note) =
        if let (Some(pr), Some(nr)) = (positive_ref.as_deref(), negative_ref.as_deref()) {
            let p = clip_text_nodes
                .iter()
                .find(|(id, _)| id.as_str() == pr)
                .map(|(_, t)| t.clone());
            let n = clip_text_nodes
                .iter()
                .find(|(id, _)| id.as_str() == nr)
                .map(|(_, t)| t.clone());
            (p, n, None)
        } else if clip_text_nodes.len() >= 2 {
            (
                Some(clip_text_nodes[0].1.clone()),
                Some(clip_text_nodes[1].1.clone()),
                Some("comfy_prompt_role: heuristic"),
            )
        } else if clip_text_nodes.len() == 1 {
            (
                Some(clip_text_nodes[0].1.clone()),
                None,
                Some("comfy_prompt_role: heuristic"),
            )
        } else {
            (None, None, None)
        };

    if let Some(p) = positive_text {
        if !p.is_empty() {
            let notes: &[&str] = if let Some(n) = role_note { &[n] } else { &[] };
            decoded.entries.push(entry(
                payload,
                "ai_gen.prompt",
                TypedValue::String(p),
                0.95,
                notes,
            ));
        }
    }
    if let Some(n) = negative_text {
        if !n.is_empty() {
            let notes: &[&str] = if let Some(rn) = role_note { &[rn] } else { &[] };
            decoded.entries.push(entry(
                payload,
                "ai_gen.negative_prompt",
                TypedValue::String(n),
                0.95,
                notes,
            ));
        }
    }

    // Checkpoint.
    if let Some(node) = checkpoint_loader {
        if let Some(name) = node
            .get("inputs")
            .and_then(|v| v.get("ckpt_name"))
            .and_then(|v| v.as_str())
        {
            decoded.entries.push(entry(
                payload,
                "ai_gen.model.name",
                TypedValue::String(name.to_string()),
                0.95,
                &[],
            ));
        }
    }

    // LoRA loaders.
    for (i, node) in lora_loaders.iter().enumerate() {
        if let Some(inputs) = node.get("inputs").and_then(|v| v.as_object()) {
            if let Some(name) = inputs.get("lora_name").and_then(|v| v.as_str()) {
                decoded.entries.push(entry(
                    payload,
                    &format!("ai_gen.lora.{i}.name"),
                    TypedValue::String(name.to_string()),
                    0.95,
                    &[],
                ));
            }
            if let Some(w) = inputs
                .get("strength_model")
                .and_then(|v| v.as_f64())
                .or_else(|| inputs.get("strength").and_then(|v| v.as_f64()))
            {
                decoded.entries.push(entry(
                    payload,
                    &format!("ai_gen.lora.{i}.weight"),
                    TypedValue::Float(w),
                    0.95,
                    &[],
                ));
            }
        }
    }
}

/// Resolve a ComfyUI link reference of the form `[node_id, slot]` to its
/// node id (as a string).
fn link_target(value: Option<&Value>) -> Option<String> {
    let arr = value?.as_array()?;
    let first = arr.first()?;
    if let Some(s) = first.as_str() {
        Some(s.to_string())
    } else if let Some(n) = first.as_i64() {
        Some(n.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pl<'a>(body: &'a [u8], shape: AiGenShape) -> AiGenPayload<'a> {
        AiGenPayload {
            shape,
            keyword: match shape {
                AiGenShape::ComfyPrompt => "prompt",
                AiGenShape::ComfyWorkflow => "workflow",
                _ => "prompt",
            },
            body,
            container: "png",
            path: "png_text:prompt",
            offset_start: 0,
            offset_end: body.len() as u64,
        }
    }

    #[test]
    fn parses_minimal_api_graph() {
        let body = br#"{
            "1": {"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"sd_xl_base_1.0.safetensors"}},
            "2": {"class_type":"CLIPTextEncode","inputs":{"text":"a cat"}},
            "3": {"class_type":"CLIPTextEncode","inputs":{"text":"blurry"}},
            "4": {"class_type":"KSampler","inputs":{"seed":12345,"steps":20,"cfg":7.0,"sampler_name":"euler","scheduler":"normal","positive":["2",0],"negative":["3",0]}},
            "5": {"class_type":"LoraLoader","inputs":{"lora_name":"foo.safetensors","strength_model":0.8}}
        }"#;
        let d = try_decode(&pl(body, AiGenShape::ComfyPrompt)).unwrap();
        let names: Vec<&str> = d.entries.iter().map(|e| e.tag_name.as_str()).collect();
        assert!(names.contains(&"ai_gen.prompt"));
        assert!(names.contains(&"ai_gen.negative_prompt"));
        assert!(names.contains(&"ai_gen.seed"));
        assert!(names.contains(&"ai_gen.steps"));
        assert!(names.contains(&"ai_gen.cfg_scale"));
        assert!(names.contains(&"ai_gen.sampler"));
        assert!(names.contains(&"ai_gen.scheduler"));
        assert!(names.contains(&"ai_gen.model.name"));
        assert!(names.contains(&"ai_gen.lora.0.name"));
        assert!(names.contains(&"ai_gen.lora.0.weight"));
        assert!(names.contains(&"ai_gen.tool"));
    }

    #[test]
    fn workflow_keyword_preserves_raw_json() {
        let body = br#"{"nodes":[]}"#;
        let d = try_decode(&pl(body, AiGenShape::ComfyWorkflow)).unwrap();
        assert!(
            d.entries
                .iter()
                .any(|e| e.tag_name == "ai_gen.workflow_json")
        );
    }

    #[test]
    fn malformed_json_emits_issue() {
        let body = b"not json";
        let d = try_decode(&pl(body, AiGenShape::ComfyPrompt)).unwrap();
        assert!(d.entries.is_empty());
        assert_eq!(d.issues.len(), 1);
        assert_eq!(d.issues[0].code, "ai_gen_json_invalid");
    }
}
