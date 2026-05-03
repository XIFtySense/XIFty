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
        claim_generator: pick_string(&json, &["claim_generator"]),
        format: pick_string(&json, &["format", "dc:format"]),
        instance_id: pick_string(&json, &["instance_id", "instanceID", "dc:instanceID"]),
    })
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
