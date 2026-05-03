//! InvokeAI `invokeai_metadata` (iTXt) JSON shape.

use crate::{AiGenPayload, DecodedAiGen, entry};
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
                    message: format!("InvokeAI metadata JSON could not be parsed: {}", e),
                    offset: Some(payload.offset_start),
                    context: Some(payload.keyword.into()),
                }],
            });
        }
    };
    let obj = value.as_object()?;
    let mut decoded = DecodedAiGen::empty();

    if let Some(s) = obj
        .get("positive_prompt")
        .or_else(|| obj.get("prompt"))
        .and_then(|v| v.as_str())
    {
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
    if let Some(n) = obj.get("seed").and_then(|v| v.as_i64()) {
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
    if let Some(f) = obj.get("cfg_scale").and_then(|v| v.as_f64()) {
        decoded.entries.push(entry(
            payload,
            "ai_gen.cfg_scale",
            TypedValue::Float(f),
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
    if let Some(model) = obj.get("model") {
        if let Some(s) = model.get("name").and_then(|v| v.as_str()) {
            decoded.entries.push(entry(
                payload,
                "ai_gen.model.name",
                TypedValue::String(s.to_string()),
                0.95,
                &[],
            ));
        }
        if let Some(s) = model.get("hash").and_then(|v| v.as_str()) {
            decoded.entries.push(entry(
                payload,
                "ai_gen.model.hash",
                TypedValue::String(s.to_string()),
                0.95,
                &[],
            ));
        }
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
            if let Some(s) = item
                .get("lora")
                .and_then(|v| v.get("name"))
                .and_then(|v| v.as_str())
                .or_else(|| item.get("name").and_then(|v| v.as_str()))
                .or_else(|| item.get("model_name").and_then(|v| v.as_str()))
            {
                decoded.entries.push(entry(
                    payload,
                    &format!("ai_gen.lora.{i}.name"),
                    TypedValue::String(s.to_string()),
                    0.95,
                    &[],
                ));
            }
            if let Some(w) = item
                .get("weight")
                .and_then(|v| v.as_f64())
                .or_else(|| item.get("strength").and_then(|v| v.as_f64()))
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
    if let Some(arr) = obj.get("controlnets").and_then(|v| v.as_array()) {
        for (i, item) in arr.iter().enumerate() {
            if let Some(s) = item
                .get("control_model")
                .and_then(|v| v.get("name"))
                .and_then(|v| v.as_str())
                .or_else(|| item.get("name").and_then(|v| v.as_str()))
            {
                decoded.entries.push(entry(
                    payload,
                    &format!("ai_gen.controlnet.{i}.name"),
                    TypedValue::String(s.to_string()),
                    0.95,
                    &[],
                ));
            }
            if let Some(w) = item
                .get("weight")
                .and_then(|v| v.as_f64())
                .or_else(|| item.get("control_weight").and_then(|v| v.as_f64()))
            {
                decoded.entries.push(entry(
                    payload,
                    &format!("ai_gen.controlnet.{i}.weight"),
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
        TypedValue::String("invokeai".to_string()),
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
            shape: crate::AiGenShape::InvokeAiMetadata,
            keyword: "invokeai_metadata",
            body,
            container: "png",
            path: "png_text:invokeai_metadata",
            offset_start: 0,
            offset_end: body.len() as u64,
        }
    }

    #[test]
    fn parses_invokeai_metadata() {
        let body = br#"{
            "positive_prompt":"a cat",
            "negative_prompt":"blurry",
            "seed":42,
            "steps":30,
            "cfg_scale":7.5,
            "scheduler":"euler",
            "model":{"name":"sdxl_base","hash":"abc123"},
            "width":1024,"height":1024,
            "loras":[{"name":"foo","weight":0.8}],
            "controlnets":[{"name":"depth","weight":0.5}]
        }"#;
        let d = try_decode(&pl(body)).unwrap();
        let names: Vec<&str> = d.entries.iter().map(|e| e.tag_name.as_str()).collect();
        for tag in [
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
            "ai_gen.tool",
        ] {
            assert!(names.contains(&tag), "missing {tag}");
        }
    }

    #[test]
    fn malformed_json_emits_issue() {
        let body = b"not json";
        let d = try_decode(&pl(body)).unwrap();
        assert!(d.entries.is_empty());
        assert_eq!(d.issues.len(), 1);
        assert_eq!(d.issues[0].code, "ai_gen_json_invalid");
    }
}
