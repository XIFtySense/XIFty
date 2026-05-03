//! Fooocus `parameters` JSON shape.
//!
//! Fooocus stores its `parameters` body as a JSON object — distinguished
//! from A1111's plaintext shape by a successful `serde_json` parse plus the
//! presence of `prompt` and a `base_model` / `base_model_name` key.

use crate::{AiGenPayload, DecodedAiGen, entry};
use serde_json::Value;
use xifty_core::TypedValue;

pub(super) fn try_decode(payload: &AiGenPayload<'_>) -> Option<DecodedAiGen> {
    let value: Value = serde_json::from_slice(payload.body).ok()?;
    let obj = value.as_object()?;
    if !obj.contains_key("prompt") {
        return None;
    }
    if !obj.contains_key("base_model") && !obj.contains_key("base_model_name") {
        return None;
    }

    let mut decoded = DecodedAiGen::empty();

    if let Some(s) = obj.get("prompt").and_then(|v| v.as_str()) {
        decoded.entries.push(entry(
            payload,
            "ai_gen.prompt",
            TypedValue::String(s.to_string()),
            0.95,
            &[],
        ));
    }
    if let Some(s) = obj.get("negative_prompt").and_then(|v| v.as_str()) {
        decoded.entries.push(entry(
            payload,
            "ai_gen.negative_prompt",
            TypedValue::String(s.to_string()),
            0.95,
            &[],
        ));
    }
    if let Some(n) = obj.get("seed").and_then(|v| v.as_i64()).or_else(|| {
        obj.get("seed")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<i64>().ok())
    }) {
        decoded.entries.push(entry(
            payload,
            "ai_gen.seed",
            TypedValue::Integer(n),
            0.95,
            &[],
        ));
    }
    if let Some(n) = obj.get("steps").and_then(|v| v.as_i64()) {
        decoded.entries.push(entry(
            payload,
            "ai_gen.steps",
            TypedValue::Integer(n),
            0.95,
            &[],
        ));
    }
    if let Some(f) = obj
        .get("guidance_scale")
        .or_else(|| obj.get("cfg_scale"))
        .and_then(|v| v.as_f64())
    {
        decoded.entries.push(entry(
            payload,
            "ai_gen.cfg_scale",
            TypedValue::Float(f),
            0.95,
            &[],
        ));
    }
    if let Some(s) = obj.get("sampler").and_then(|v| v.as_str()) {
        decoded.entries.push(entry(
            payload,
            "ai_gen.sampler",
            TypedValue::String(s.to_string()),
            0.95,
            &[],
        ));
    }
    if let Some(s) = obj.get("scheduler").and_then(|v| v.as_str()) {
        decoded.entries.push(entry(
            payload,
            "ai_gen.scheduler",
            TypedValue::String(s.to_string()),
            0.95,
            &[],
        ));
    }
    if let Some(s) = obj
        .get("base_model_name")
        .or_else(|| obj.get("base_model"))
        .and_then(|v| v.as_str())
    {
        decoded.entries.push(entry(
            payload,
            "ai_gen.model.name",
            TypedValue::String(s.to_string()),
            0.95,
            &[],
        ));
    }
    if let Some(s) = obj.get("base_model_hash").and_then(|v| v.as_str()) {
        decoded.entries.push(entry(
            payload,
            "ai_gen.model.hash",
            TypedValue::String(s.to_string()),
            0.95,
            &[],
        ));
    }
    if let (Some(w), Some(h)) = (
        obj.get("width").and_then(|v| v.as_i64()),
        obj.get("height").and_then(|v| v.as_i64()),
    ) {
        decoded.entries.push(entry(
            payload,
            "ai_gen.dimensions.width",
            TypedValue::Integer(w),
            0.95,
            &[],
        ));
        decoded.entries.push(entry(
            payload,
            "ai_gen.dimensions.height",
            TypedValue::Integer(h),
            0.95,
            &[],
        ));
    }
    if let Some(arr) = obj.get("loras").and_then(|v| v.as_array()) {
        for (i, item) in arr.iter().enumerate() {
            // Accept either {name, weight} or [name, weight].
            let (name, weight) = match item {
                Value::Object(map) => (
                    map.get("name").and_then(|v| v.as_str()).map(String::from),
                    map.get("weight").and_then(|v| v.as_f64()),
                ),
                Value::Array(a) => (
                    a.first().and_then(|v| v.as_str()).map(String::from),
                    a.get(1).and_then(|v| v.as_f64()),
                ),
                _ => (None, None),
            };
            if let Some(name) = name {
                decoded.entries.push(entry(
                    payload,
                    &format!("ai_gen.lora.{i}.name"),
                    TypedValue::String(name),
                    0.95,
                    &[],
                ));
            }
            if let Some(w) = weight {
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

    decoded.entries.push(entry(
        payload,
        "ai_gen.tool",
        TypedValue::String("fooocus".to_string()),
        0.95,
        &[],
    ));

    Some(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pl<'a>(body: &'a [u8]) -> AiGenPayload<'a> {
        AiGenPayload {
            shape: crate::AiGenShape::A1111Parameters,
            keyword: "parameters",
            body,
            container: "png",
            path: "png_text:parameters",
            offset_start: 0,
            offset_end: body.len() as u64,
        }
    }

    #[test]
    fn detects_fooocus_json() {
        let body = br#"{"prompt":"a cat","negative_prompt":"blurry","seed":42,"steps":30,"guidance_scale":7.5,"sampler":"dpmpp_2m","scheduler":"karras","base_model_name":"juggernaut.safetensors","width":1024,"height":1024}"#;
        let d = try_decode(&pl(body)).unwrap();
        let tool = d
            .entries
            .iter()
            .find(|e| e.tag_name == "ai_gen.tool")
            .unwrap();
        if let TypedValue::String(s) = &tool.value {
            assert_eq!(s, "fooocus");
        } else {
            panic!();
        }
    }

    #[test]
    fn rejects_a1111_plaintext() {
        let body = b"a cat\nNegative prompt: blurry\nSteps: 20";
        assert!(try_decode(&pl(body)).is_none());
    }

    #[test]
    fn rejects_json_without_base_model() {
        let body = br#"{"prompt":"a cat"}"#;
        assert!(try_decode(&pl(body)).is_none());
    }
}
