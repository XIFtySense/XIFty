//! Automatic1111 / Forge `parameters` plaintext parser.
//!
//! The body convention is three sections separated by newlines:
//!
//! ```text
//! <positive prompt>
//! Negative prompt: <negative prompt>
//! Steps: 20, Sampler: "DPM++ 2M Karras", CFG scale: 7, Seed: 12345, Size: 512x512, ...
//! ```
//!
//! - `Negative prompt:` is optional.
//! - The trailing line is `Key: Value, Key: Value, ...` with double-quoted
//!   values for any value that contains a comma.
//! - `<lora:NAME:WEIGHT>` tags inline in the prompt body are extracted and
//!   surfaced as `ai_gen.lora.<i>.{name,weight}`.

use crate::{AiGenPayload, DecodedAiGen, entry};
use xifty_core::TypedValue;

pub(super) fn try_decode(payload: &AiGenPayload<'_>) -> Option<DecodedAiGen> {
    let text = std::str::from_utf8(payload.body).ok()?;
    if text.trim().is_empty() {
        return Some(DecodedAiGen::empty());
    }

    let (positive, negative, kv_line) = split_sections(text);

    let mut decoded = DecodedAiGen::empty();
    let mut have_strong = false;

    // Positive prompt + LoRA extraction.
    if !positive.trim().is_empty() {
        let (cleaned, loras) = extract_loras(positive);
        decoded.entries.push(entry(
            payload,
            "ai_gen.prompt",
            TypedValue::String(cleaned.trim().to_string()),
            0.7,
            &[],
        ));
        for (i, (name, weight)) in loras.iter().enumerate() {
            decoded.entries.push(entry(
                payload,
                &format!("ai_gen.lora.{i}.name"),
                TypedValue::String(name.clone()),
                0.95,
                &[],
            ));
            if let Some(w) = weight {
                decoded.entries.push(entry(
                    payload,
                    &format!("ai_gen.lora.{i}.weight"),
                    TypedValue::Float(*w),
                    0.95,
                    &[],
                ));
            }
        }
    }

    if let Some(neg) = negative {
        if !neg.trim().is_empty() {
            decoded.entries.push(entry(
                payload,
                "ai_gen.negative_prompt",
                TypedValue::String(neg.trim().to_string()),
                0.95,
                &[],
            ));
        }
    }

    if let Some(kv_line) = kv_line {
        let pairs = parse_kv_line(kv_line);
        for (key, value) in &pairs {
            match key.as_str() {
                "Steps" => {
                    if let Ok(n) = value.parse::<i64>() {
                        decoded.entries.push(entry(
                            payload,
                            "ai_gen.steps",
                            TypedValue::Integer(n),
                            0.95,
                            &[],
                        ));
                        have_strong = true;
                    }
                }
                "Sampler" => {
                    decoded.entries.push(entry(
                        payload,
                        "ai_gen.sampler",
                        TypedValue::String(value.clone()),
                        0.95,
                        &[],
                    ));
                    have_strong = true;
                }
                "Schedule type" | "Scheduler" => {
                    decoded.entries.push(entry(
                        payload,
                        "ai_gen.scheduler",
                        TypedValue::String(value.clone()),
                        0.95,
                        &[],
                    ));
                }
                "CFG scale" => {
                    if let Ok(f) = value.parse::<f64>() {
                        decoded.entries.push(entry(
                            payload,
                            "ai_gen.cfg_scale",
                            TypedValue::Float(f),
                            0.95,
                            &[],
                        ));
                    }
                }
                "Seed" => {
                    if let Ok(n) = value.parse::<i64>() {
                        decoded.entries.push(entry(
                            payload,
                            "ai_gen.seed",
                            TypedValue::Integer(n),
                            0.95,
                            &[],
                        ));
                        have_strong = true;
                    }
                }
                "Size" => {
                    if let Some((w, h)) = value.split_once('x') {
                        if let (Ok(w), Ok(h)) = (w.parse::<u32>(), h.parse::<u32>()) {
                            decoded.entries.push(entry(
                                payload,
                                "ai_gen.dimensions.width",
                                TypedValue::Integer(w as i64),
                                0.95,
                                &[],
                            ));
                            decoded.entries.push(entry(
                                payload,
                                "ai_gen.dimensions.height",
                                TypedValue::Integer(h as i64),
                                0.95,
                                &[],
                            ));
                        }
                    }
                }
                "Model" => {
                    decoded.entries.push(entry(
                        payload,
                        "ai_gen.model.name",
                        TypedValue::String(value.clone()),
                        0.95,
                        &[],
                    ));
                }
                "Model hash" => {
                    decoded.entries.push(entry(
                        payload,
                        "ai_gen.model.hash",
                        TypedValue::String(value.clone()),
                        0.95,
                        &[],
                    ));
                }
                "Lora hashes" => {
                    // `name1: hash1, name2: hash2` (already inside one quoted value)
                    for (i, pair) in value.split(',').enumerate() {
                        let pair = pair.trim();
                        if let Some((name, _hash)) = pair.split_once(':') {
                            let name = name.trim().trim_matches('"').to_string();
                            decoded.entries.push(entry(
                                payload,
                                &format!("ai_gen.lora.{i}.name"),
                                TypedValue::String(name),
                                0.95,
                                &["source: Lora hashes"],
                            ));
                        }
                    }
                }
                _ => {
                    // Preserve as tool_extra.
                    decoded.entries.push(entry(
                        payload,
                        &format!("ai_gen.tool_extra.{}", sanitize_key(key)),
                        TypedValue::String(value.clone()),
                        0.7,
                        &[],
                    ));
                }
            }
        }
    }

    let confidence = if have_strong { 0.95 } else { 0.7 };
    decoded.entries.push(entry(
        payload,
        "ai_gen.tool",
        TypedValue::String("automatic1111".to_string()),
        confidence,
        &[],
    ));

    Some(decoded)
}

/// Split A1111 body into `(positive, negative, kv_line)`.
///
/// Strategy: the *last* line that begins with a known A1111 token (`Steps:`,
/// `Sampler:`, `Seed:`, `CFG scale:`, `Size:`, `Model:`, `Model hash:`) is
/// the kv line. Everything before is the prompt body, which may itself
/// contain a `\nNegative prompt:` separator.
fn split_sections(text: &str) -> (&str, Option<&str>, Option<&str>) {
    let mut kv_split: Option<usize> = None;
    for (i, line) in text.match_indices('\n') {
        let after = &text[i + 1..];
        if looks_like_kv_line(after) {
            kv_split = Some(i + 1);
            break;
        }
        let _ = line;
    }
    // Also accept the very first line being a kv line (no prompt).
    if kv_split.is_none() && looks_like_kv_line(text) {
        kv_split = Some(0);
    }

    let (prompt_section, kv_line) = match kv_split {
        Some(idx) => (&text[..idx.saturating_sub(1)], Some(text[idx..].trim_end())),
        None => (text, None),
    };

    if let Some(neg_idx) = prompt_section.find("\nNegative prompt:") {
        let positive = &prompt_section[..neg_idx];
        let negative = &prompt_section[neg_idx + "\nNegative prompt:".len()..];
        (positive, Some(negative), kv_line)
    } else {
        (prompt_section, None, kv_line)
    }
}

fn looks_like_kv_line(s: &str) -> bool {
    const TOKENS: &[&str] = &[
        "Steps:",
        "Sampler:",
        "Seed:",
        "CFG scale:",
        "Size:",
        "Model:",
        "Model hash:",
        "Schedule type:",
        "Scheduler:",
    ];
    let trimmed = s.trim_start();
    TOKENS.iter().any(|t| trimmed.starts_with(t))
}

/// Tokenize a comma-separated `Key: Value` line, respecting double-quoted
/// values (so commas inside `"DPM++ 2M, Karras"` are preserved).
fn parse_kv_line(line: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0usize;
    let n = bytes.len();
    while i < n {
        // skip leading whitespace and commas.
        while i < n && (bytes[i] == b' ' || bytes[i] == b',' || bytes[i] == b'\t') {
            i += 1;
        }
        if i >= n {
            break;
        }
        // read key up to ':'.
        let key_start = i;
        while i < n && bytes[i] != b':' {
            i += 1;
        }
        if i >= n {
            break;
        }
        let key = line[key_start..i].trim().to_string();
        i += 1; // skip ':'
        // skip leading whitespace.
        while i < n && bytes[i] == b' ' {
            i += 1;
        }
        // value: quoted or unquoted.
        let value;
        if i < n && bytes[i] == b'"' {
            i += 1;
            let v_start = i;
            while i < n && bytes[i] != b'"' {
                i += 1;
            }
            value = line[v_start..i].to_string();
            if i < n {
                i += 1; // skip closing quote.
            }
        } else {
            let v_start = i;
            while i < n && bytes[i] != b',' {
                i += 1;
            }
            value = line[v_start..i].trim().to_string();
        }
        out.push((key, value));
    }
    out
}

/// Pull `<lora:NAME:WEIGHT>` tokens out of a prompt body. Returns the cleaned
/// body and a list of `(name, weight)` pairs.
fn extract_loras(text: &str) -> (String, Vec<(String, Option<f64>)>) {
    let mut cleaned = String::with_capacity(text.len());
    let mut loras = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("<lora:") {
        cleaned.push_str(&rest[..start]);
        let after = &rest[start + "<lora:".len()..];
        if let Some(end) = after.find('>') {
            let inner = &after[..end];
            let mut parts = inner.splitn(2, ':');
            let name = parts.next().unwrap_or("").to_string();
            let weight = parts.next().and_then(|w| w.parse::<f64>().ok());
            if !name.is_empty() {
                loras.push((name, weight));
            }
            rest = &after[end + 1..];
        } else {
            // unterminated; bail out and append the rest verbatim.
            cleaned.push_str(rest);
            return (cleaned, loras);
        }
    }
    cleaned.push_str(rest);
    (cleaned, loras)
}

fn sanitize_key(key: &str) -> String {
    key.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
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
    fn parses_full_a1111_body() {
        let body = b"a cute cat\nNegative prompt: blurry\nSteps: 20, Sampler: \"DPM++ 2M, Karras\", CFG scale: 7, Seed: 12345, Size: 512x768, Model hash: abc123, Model: sd_xl_base_1.0";
        let d = try_decode(&pl(body)).unwrap();
        let names: Vec<&str> = d.entries.iter().map(|e| e.tag_name.as_str()).collect();
        assert!(names.contains(&"ai_gen.prompt"));
        assert!(names.contains(&"ai_gen.negative_prompt"));
        assert!(names.contains(&"ai_gen.steps"));
        assert!(names.contains(&"ai_gen.sampler"));
        assert!(names.contains(&"ai_gen.cfg_scale"));
        assert!(names.contains(&"ai_gen.seed"));
        assert!(names.contains(&"ai_gen.dimensions.width"));
        assert!(names.contains(&"ai_gen.dimensions.height"));
        assert!(names.contains(&"ai_gen.model.name"));
        assert!(names.contains(&"ai_gen.model.hash"));
        assert!(names.contains(&"ai_gen.tool"));

        let sampler = d
            .entries
            .iter()
            .find(|e| e.tag_name == "ai_gen.sampler")
            .unwrap();
        if let TypedValue::String(s) = &sampler.value {
            assert_eq!(s, "DPM++ 2M, Karras");
        } else {
            panic!("sampler not string");
        }
    }

    #[test]
    fn extracts_inline_lora() {
        let body =
            b"a cat <lora:foo:0.8> with hat\nNegative prompt: blurry\nSteps: 20, Seed: 1, Sampler: Euler";
        let d = try_decode(&pl(body)).unwrap();
        let names: Vec<&str> = d.entries.iter().map(|e| e.tag_name.as_str()).collect();
        assert!(names.contains(&"ai_gen.lora.0.name"));
        assert!(names.contains(&"ai_gen.lora.0.weight"));
    }

    #[test]
    fn empty_body_yields_no_entries() {
        let body = b"";
        let d = try_decode(&pl(body)).unwrap();
        assert!(d.entries.is_empty());
    }

    #[test]
    fn prompt_only_low_confidence() {
        let body = b"a cat";
        let d = try_decode(&pl(body)).unwrap();
        let tool = d
            .entries
            .iter()
            .find(|e| e.tag_name == "ai_gen.tool")
            .unwrap();
        assert!(
            tool.notes
                .iter()
                .any(|n| n.contains("0.70") || n.contains("0.7"))
        );
    }
}
