//! Assertion CBOR decoder. Walks `actions[]` and lifts the bounded fields
//! into a flat indexed list.

use serde_json::Value as JsonValue;

use crate::cbor;

#[derive(Debug, Default, Clone)]
pub struct AssertionEntry {
    pub label: Option<String>,
    pub action: Option<String>,
    pub description: Option<String>,
    pub digital_source_type: Option<String>,
    pub software_agent: Option<String>,
    pub when: Option<String>,
}

/// Decode an assertion CBOR payload (e.g. `c2pa.actions.v2`). `label` is
/// supplied externally (from the JUMBF description box) and stamped on every
/// extracted action for convenience.
pub fn decode(label: Option<&str>, bytes: &[u8]) -> Vec<AssertionEntry> {
    let Some(value) = cbor::from_slice(bytes) else {
        return Vec::new();
    };
    let json = cbor::cbor_to_json(&value);
    let mut out = Vec::new();
    if let Some(actions) = json.get("actions").and_then(|v| v.as_array()) {
        for action in actions {
            let mut entry = AssertionEntry {
                label: label.map(|s| s.to_string()),
                ..AssertionEntry::default()
            };
            entry.action = pick_string(action, &["action"]);
            entry.description = pick_string(action, &["description"]);
            entry.digital_source_type =
                pick_string(action, &["digitalSourceType", "digital_source_type"]);
            entry.software_agent = pick_string(action, &["softwareAgent", "software_agent"])
                .or_else(|| pick_software_agent_object(action));
            entry.when = pick_string(action, &["when"]);
            out.push(entry);
        }
    }
    if out.is_empty() {
        // Some assertions are not action-shaped; preserve a single label-only
        // entry so the assertion still surfaces in the field list.
        if let Some(label) = label {
            out.push(AssertionEntry {
                label: Some(label.to_string()),
                ..AssertionEntry::default()
            });
        }
    }
    out
}

fn pick_string(value: &JsonValue, keys: &[&str]) -> Option<String> {
    let obj = value.as_object()?;
    for k in keys {
        if let Some(JsonValue::String(s)) = obj.get(*k) {
            return Some(s.clone());
        }
    }
    None
}

fn pick_software_agent_object(value: &JsonValue) -> Option<String> {
    // C2PA 2.0 allows `softwareAgent` to be either a string or a
    // `{name, version, ...}` object.
    let obj = value.get("softwareAgent")?.as_object()?;
    let name = obj.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let version = obj.get("version").and_then(|v| v.as_str()).unwrap_or("");
    if name.is_empty() && version.is_empty() {
        None
    } else if version.is_empty() {
        Some(name.to_string())
    } else {
        Some(format!("{name}/{version}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ciborium::value::Value;

    fn encode_actions(actions: Vec<Vec<(&str, &str)>>) -> Vec<u8> {
        let arr = Value::Array(
            actions
                .into_iter()
                .map(|pairs| {
                    Value::Map(
                        pairs
                            .into_iter()
                            .map(|(k, v)| (Value::Text(k.into()), Value::Text(v.into())))
                            .collect(),
                    )
                })
                .collect(),
        );
        let map = Value::Map(vec![(Value::Text("actions".into()), arr)]);
        let mut out = Vec::new();
        ciborium::ser::into_writer(&map, &mut out).unwrap();
        out
    }

    #[test]
    fn decodes_two_actions_in_order() {
        let bytes = encode_actions(vec![
            vec![
                ("action", "c2pa.created"),
                (
                    "digitalSourceType",
                    "http://cv.iptc.org/newscodes/digitalsourcetype/trainedAlgorithmicMedia",
                ),
                ("softwareAgent", "TestAgent/1.0"),
                ("when", "2026-01-01T00:00:00Z"),
                ("description", "Created by test"),
            ],
            vec![("action", "c2pa.edited"), ("description", "edit step")],
        ]);
        let entries = decode(Some("c2pa.actions.v2"), &bytes);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].action.as_deref(), Some("c2pa.created"));
        assert_eq!(entries[0].label.as_deref(), Some("c2pa.actions.v2"));
        assert!(
            entries[0]
                .digital_source_type
                .as_deref()
                .unwrap()
                .ends_with("trainedAlgorithmicMedia")
        );
        assert_eq!(entries[0].software_agent.as_deref(), Some("TestAgent/1.0"));
        assert_eq!(entries[1].action.as_deref(), Some("c2pa.edited"));
    }
}
