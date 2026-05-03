//! Midjourney heuristic for `Description` / `Comment` chunks.
//!
//! `Description` is a generic PNG keyword used by many tools — to avoid
//! false-positives we accept the body as Midjourney **only** when:
//!
//! - a 36-character `8-4-4-4-12` UUID is present (the Midjourney Job ID), or
//! - the body contains a Midjourney-specific flag substring (`--ar`, `--v`,
//!   `--style`, `--niji`).
//!
//! When neither is present, we return `None` so other classifiers can try.

use crate::{AiGenPayload, DecodedAiGen, entry};
use xifty_core::TypedValue;

const MJ_FLAGS: &[&str] = &[
    "--ar ", "--v ", "--style ", "--niji ", "--chaos ", "--seed ",
];

pub(super) fn try_decode(payload: &AiGenPayload<'_>) -> Option<DecodedAiGen> {
    let text = std::str::from_utf8(payload.body).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let job_id = find_uuid(trimmed);
    let has_flag = MJ_FLAGS.iter().any(|f| trimmed.contains(*f));
    if job_id.is_none() && !has_flag {
        return None;
    }

    let prompt_body = match &job_id {
        Some(id) => trimmed
            .trim_end_matches(|c: char| c == ' ' || c == '\n' || c == '\t')
            .strip_suffix(id.as_str())
            .map(|s| s.trim_end().trim_end_matches("Job ID:").trim_end())
            .unwrap_or(trimmed)
            .trim()
            .to_string(),
        None => trimmed.to_string(),
    };

    let mut decoded = DecodedAiGen::empty();
    if !prompt_body.is_empty() {
        decoded.entries.push(entry(
            payload,
            "ai_gen.prompt",
            TypedValue::String(prompt_body),
            0.7,
            &[],
        ));
    }
    if let Some(id) = job_id {
        decoded.entries.push(entry(
            payload,
            "ai_gen.midjourney.job_id",
            TypedValue::String(id),
            0.95,
            &[],
        ));
    }

    decoded.entries.push(entry(
        payload,
        "ai_gen.tool",
        TypedValue::String("midjourney".to_string()),
        0.7,
        &[],
    ));

    Some(decoded)
}

/// Find a `8-4-4-4-12` hex UUID anywhere in the text. Returns the matched
/// substring on success.
fn find_uuid(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let n = bytes.len();
    if n < 36 {
        return None;
    }
    'outer: for start in 0..=n - 36 {
        let window = &bytes[start..start + 36];
        // pattern: 8 hex, '-', 4 hex, '-', 4 hex, '-', 4 hex, '-', 12 hex.
        let positions: [(usize, bool); 36] = make_pattern();
        for (i, (_, is_hex)) in positions.iter().enumerate() {
            let b = window[i];
            if *is_hex {
                if !b.is_ascii_hexdigit() {
                    continue 'outer;
                }
            } else if b != b'-' {
                continue 'outer;
            }
        }
        return Some(std::str::from_utf8(window).ok()?.to_string());
    }
    None
}

const fn make_pattern() -> [(usize, bool); 36] {
    let mut arr = [(0usize, true); 36];
    let dash_positions = [8, 13, 18, 23];
    let mut i = 0;
    while i < 36 {
        arr[i] = (i, true);
        i += 1;
    }
    let mut j = 0;
    while j < 4 {
        arr[dash_positions[j]] = (dash_positions[j], false);
        j += 1;
    }
    arr
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pl<'a>(body: &'a [u8]) -> AiGenPayload<'a> {
        AiGenPayload {
            shape: crate::AiGenShape::MidjourneyDescription,
            keyword: "Description",
            body,
            container: "png",
            path: "png_text:Description",
            offset_start: 0,
            offset_end: body.len() as u64,
        }
    }

    #[test]
    fn parses_description_with_uuid() {
        let body = b"a cute cat --ar 16:9 --v 6 Job ID: 12345678-1234-1234-1234-1234567890ab";
        let d = try_decode(&pl(body)).unwrap();
        let names: Vec<&str> = d.entries.iter().map(|e| e.tag_name.as_str()).collect();
        assert!(names.contains(&"ai_gen.prompt"));
        assert!(names.contains(&"ai_gen.midjourney.job_id"));
    }

    #[test]
    fn rejects_generic_description_without_signal() {
        let body = b"just a plain description of the image";
        assert!(try_decode(&pl(body)).is_none());
    }

    #[test]
    fn flags_only_no_uuid() {
        let body = b"a cat --ar 1:1";
        let d = try_decode(&pl(body)).unwrap();
        let names: Vec<&str> = d.entries.iter().map(|e| e.tag_name.as_str()).collect();
        assert!(names.contains(&"ai_gen.prompt"));
        assert!(!names.contains(&"ai_gen.midjourney.job_id"));
    }
}
