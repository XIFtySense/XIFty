//! Claim CBOR decoder. Lifts `claim_generator`, `format`, and `instance_id`
//! from the C2PA claim map.

use serde_json::Value as JsonValue;

use crate::cbor;

#[derive(Debug, Default, Clone)]
pub struct ClaimInfo {
    pub claim_generator: Option<String>,
    pub format: Option<String>,
    pub instance_id: Option<String>,
}

pub fn decode(bytes: &[u8]) -> Option<ClaimInfo> {
    let value = cbor::from_slice(bytes)?;
    let json = cbor::cbor_to_json(&value);
    Some(ClaimInfo {
        claim_generator: pick_string(&json, &["claim_generator"])
            .or_else(|| pick_generator_info(&json)),
        format: pick_string(&json, &["format", "dc:format"]),
        instance_id: pick_string(&json, &["instance_id", "instanceID", "dc:instanceID"]),
    })
}

/// C2PA 2.x replaces the bare `claim_generator` string with
/// `claim_generator_info`, which can be either a single `{name, version}`
/// object or an array of such objects. Pick the first `name` (with
/// `version` appended when present) so downstream label/AI heuristics
/// continue to work.
fn pick_generator_info(json: &JsonValue) -> Option<String> {
    let info = json.as_object()?.get("claim_generator_info")?;
    let entry = match info {
        JsonValue::Array(items) => items.first()?,
        JsonValue::Object(_) => info,
        _ => return None,
    };
    let obj = entry.as_object()?;
    let name = obj.get("name").and_then(JsonValue::as_str)?;
    if let Some(version) = obj.get("version").and_then(JsonValue::as_str) {
        Some(format!("{name}/{version}"))
    } else {
        Some(name.to_string())
    }
}

fn pick_string(json: &JsonValue, keys: &[&str]) -> Option<String> {
    let obj = json.as_object()?;
    for k in keys {
        if let Some(JsonValue::String(s)) = obj.get(*k) {
            return Some(s.clone());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use ciborium::value::Value;

    fn encode_map(pairs: Vec<(&str, &str)>) -> Vec<u8> {
        let map = Value::Map(
            pairs
                .into_iter()
                .map(|(k, v)| (Value::Text(k.into()), Value::Text(v.into())))
                .collect(),
        );
        let mut out = Vec::new();
        ciborium::ser::into_writer(&map, &mut out).unwrap();
        out
    }

    fn encode_value(v: Value) -> Vec<u8> {
        let mut out = Vec::new();
        ciborium::ser::into_writer(&v, &mut out).unwrap();
        out
    }

    #[test]
    fn decodes_claim_generator_info_object() {
        // C2PA 2.x: claim_generator_info as a single {name, version} map.
        let info = Value::Map(vec![
            (
                Value::Text("name".into()),
                Value::Text("Google C2PA Core Generator Library".into()),
            ),
            (Value::Text("version".into()), Value::Text("1.2.3".into())),
        ]);
        let claim = Value::Map(vec![(Value::Text("claim_generator_info".into()), info)]);
        let decoded = decode(&encode_value(claim)).unwrap();
        assert_eq!(
            decoded.claim_generator.as_deref(),
            Some("Google C2PA Core Generator Library/1.2.3")
        );
    }

    #[test]
    fn decodes_claim_generator_info_array() {
        let entry = Value::Map(vec![(
            Value::Text("name".into()),
            Value::Text("Adobe Firefly".into()),
        )]);
        let claim = Value::Map(vec![(
            Value::Text("claim_generator_info".into()),
            Value::Array(vec![entry]),
        )]);
        let decoded = decode(&encode_value(claim)).unwrap();
        assert_eq!(decoded.claim_generator.as_deref(), Some("Adobe Firefly"));
    }

    #[test]
    fn decodes_minimal_claim() {
        let bytes = encode_map(vec![
            ("claim_generator", "XIFty Test/0.1"),
            ("format", "image/png"),
            ("instance_id", "urn:uuid:abc"),
        ]);
        let info = decode(&bytes).unwrap();
        assert_eq!(info.claim_generator.as_deref(), Some("XIFty Test/0.1"));
        assert_eq!(info.format.as_deref(), Some("image/png"));
        assert_eq!(info.instance_id.as_deref(), Some("urn:uuid:abc"));
    }
}
