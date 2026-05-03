//! Thin wrapper over `ciborium::value::Value` with a `serde_json::Value`
//! bridge. The signature module reads raw `ciborium::value::Value` directly
//! to preserve CBOR tags (e.g. tag 18 for COSE_Sign1).

use ciborium::value::{Integer, Value};
use serde_json::Value as JsonValue;

/// Decode a CBOR byte stream into a `ciborium::value::Value`.
pub fn from_slice(bytes: &[u8]) -> Option<Value> {
    ciborium::de::from_reader(bytes).ok()
}

/// Convert a CBOR Value into a serde_json::Value, dropping CBOR tags. Bytes
/// become base16 strings; integers become i64 (or strings if they overflow).
pub fn cbor_to_json(value: &Value) -> JsonValue {
    match value {
        Value::Null => JsonValue::Null,
        Value::Bool(b) => JsonValue::Bool(*b),
        Value::Integer(i) => integer_to_json(i),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null),
        Value::Bytes(b) => JsonValue::String(hex(b)),
        Value::Text(s) => JsonValue::String(s.clone()),
        Value::Array(items) => JsonValue::Array(items.iter().map(cbor_to_json).collect()),
        Value::Map(pairs) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in pairs {
                let key = match k {
                    Value::Text(s) => s.clone(),
                    Value::Integer(i) => integer_display(i),
                    Value::Bool(b) => b.to_string(),
                    Value::Null => "null".to_string(),
                    other => format!("{:?}", other),
                };
                obj.insert(key, cbor_to_json(v));
            }
            JsonValue::Object(obj)
        }
        Value::Tag(_, inner) => cbor_to_json(inner),
        _ => JsonValue::Null,
    }
}

fn integer_to_json(i: &Integer) -> JsonValue {
    if let Ok(v) = i64::try_from(*i) {
        JsonValue::Number(v.into())
    } else if let Ok(v) = u64::try_from(*i) {
        JsonValue::Number(v.into())
    } else {
        JsonValue::String(integer_display(i))
    }
}

fn integer_display(i: &Integer) -> String {
    if let Ok(v) = i64::try_from(*i) {
        v.to_string()
    } else if let Ok(v) = u64::try_from(*i) {
        v.to_string()
    } else {
        format!("{:?}", i)
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_simple_map() {
        // Encode {"hello": "world"} via ciborium and decode back.
        let mut out = Vec::new();
        ciborium::ser::into_writer(
            &Value::Map(vec![(
                Value::Text("hello".into()),
                Value::Text("world".into()),
            )]),
            &mut out,
        )
        .unwrap();
        let parsed = from_slice(&out).unwrap();
        let json = cbor_to_json(&parsed);
        assert_eq!(json["hello"], JsonValue::String("world".into()));
    }

    #[test]
    fn tag_is_preserved_in_raw_value() {
        // Build CBOR tag 18 wrapping an empty array.
        let val = Value::Tag(18, Box::new(Value::Array(Vec::new())));
        let mut out = Vec::new();
        ciborium::ser::into_writer(&val, &mut out).unwrap();
        let parsed = from_slice(&out).unwrap();
        match parsed {
            Value::Tag(t, inner) => {
                assert_eq!(t, 18);
                assert!(matches!(*inner, Value::Array(_)));
            }
            other => panic!("expected tag, got {:?}", other),
        }
    }
}
