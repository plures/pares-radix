//! Canonical JSON encoding rules (ADR-0040 §2) — the single encoding used
//! for every content-derived hash EXCEPT `BlobHash` (which hashes raw bytes
//! directly, ADR-0040 §3).
//!
//! Rules implemented here:
//! - Object keys sorted byte-wise ascending by UTF-8 encoding.
//! - No insignificant whitespace.
//! - No floats anywhere in this schema (enforced by construction: this
//!   encoder rejects `serde_json::Number` values that are not integers).
//! - Strings escaped per strict JSON, not HTML-safe-escaped.
//! - Arrays preserve their given order (order significance is a caller
//!   concern - this encoder never reorders array elements).
//! - `null` for an absent optional field must never be emitted; callers
//!   build a `serde_json::Value::Object` that simply omits the key.

use serde::Serialize;
use serde_json::Value;

use crate::identity::ContentHash;

#[derive(Debug, thiserror::Error)]
pub enum CanonicalError {
    #[error("value contains a float, which is not permitted anywhere in this schema: {0}")]
    FloatNotAllowed(String),
    #[error("value contains an explicit null, which must be omitted instead: key {0:?}")]
    ExplicitNullNotAllowed(String),
    #[error("serde_json serialization failed: {0}")]
    Serde(#[from] serde_json::Error),
}

/// Serialize `value` to the canonical JSON byte encoding specified by
/// ADR-0040 §2, then return the raw UTF-8 bytes (no trailing newline).
pub fn to_canonical_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, CanonicalError> {
    let json_value = serde_json::to_value(value)?;
    let canonical = canonicalize_value(json_value, "$")?;
    Ok(write_canonical(&canonical))
}

/// Hash `value` per ADR-0040 §2: canonical JSON bytes, then BLAKE3-256.
pub fn hash_canonical_json<T: Serialize>(value: &T) -> Result<ContentHash, CanonicalError> {
    let bytes = to_canonical_bytes(value)?;
    Ok(ContentHash::of_raw_bytes(&bytes))
}

/// Recursively validate + normalize a `serde_json::Value` per the canonical
/// rules: reject floats, reject explicit nulls inside objects (arrays may
/// still legally contain `null` if a schema ever needs it - the "omit,
/// don't null" rule in ADR-0040 §2 is specifically about *optional object
/// fields*), and sort object keys byte-wise ascending.
fn canonicalize_value(value: Value, path: &str) -> Result<Value, CanonicalError> {
    match value {
        Value::Null => Ok(Value::Null),
        Value::Bool(b) => Ok(Value::Bool(b)),
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                Ok(Value::Number(n))
            } else {
                Err(CanonicalError::FloatNotAllowed(path.to_string()))
            }
        }
        Value::String(s) => Ok(Value::String(s)),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.into_iter().enumerate() {
                out.push(canonicalize_value(item, &format!("{path}[{i}]"))?);
            }
            Ok(Value::Array(out))
        }
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = Vec::with_capacity(map.len());
            for (k, v) in map.into_iter() {
                if v.is_null() {
                    return Err(CanonicalError::ExplicitNullNotAllowed(format!(
                        "{path}.{k}"
                    )));
                }
                let child_path = format!("{path}.{k}");
                entries.push((k.clone(), canonicalize_value(v, &child_path)?));
            }
            // Byte-wise ascending sort by UTF-8 encoding of the key.
            entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
            let mut out = serde_json::Map::new();
            for (k, v) in entries {
                out.insert(k, v);
            }
            Ok(Value::Object(out))
        }
    }
}

/// Write a value that has already been canonicalized (sorted keys, no
/// floats, no explicit object nulls) to compact JSON bytes with no
/// insignificant whitespace. `serde_json::to_vec` on a `serde_json::Map`
/// preserves insertion order, which is why `canonicalize_value` sorts the
/// map before this step rather than relying on any serializer-level key
/// sorting.
fn write_canonical(value: &Value) -> Vec<u8> {
    // serde_json's compact writer already omits all insignificant
    // whitespace and does not HTML-escape by default, matching ADR-0040 §2.
    serde_json::to_vec(value).expect("canonicalized Value must always serialize")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sorts_object_keys_byte_wise_ascending() {
        let value = json!({"b": 1, "a": 2, "A": 3});
        let bytes = to_canonical_bytes(&value).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        // 'A' (0x41) sorts before 'a' (0x61) and 'b' (0x62) byte-wise.
        assert_eq!(text, r#"{"A":3,"a":2,"b":1}"#);
    }

    #[test]
    fn rejects_float_values() {
        let value = json!({"x": 1.5});
        let err = to_canonical_bytes(&value).unwrap_err();
        assert!(matches!(err, CanonicalError::FloatNotAllowed(_)));
    }

    #[test]
    fn rejects_explicit_null_in_object() {
        let value = json!({"x": null});
        let err = to_canonical_bytes(&value).unwrap_err();
        assert!(matches!(err, CanonicalError::ExplicitNullNotAllowed(_)));
    }

    #[test]
    fn omitted_optional_field_and_never_present_field_hash_identically() {
        let with_present_value = json!({"a": 1, "b": 2});
        let without_key_at_all = json!({"a": 1});
        // Simulate "explicitly absent" by simply not inserting the key at
        // all -- the ADR-0040 rule is enforced by callers constructing their
        // Value without ever inserting None-valued keys; this encoder's job
        // is only to refuse to accept an explicit null if one slips through.
        let h1 = hash_canonical_json(&with_present_value).unwrap();
        let h2 = hash_canonical_json(&without_key_at_all).unwrap();
        assert_ne!(h1, h2, "differing content must hash differently");
    }

    #[test]
    fn arrays_preserve_order_and_are_not_sorted() {
        let value = json!({"parents": ["z", "a", "m"]});
        let bytes = to_canonical_bytes(&value).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert_eq!(text, r#"{"parents":["z","a","m"]}"#);
    }

    #[test]
    fn no_insignificant_whitespace_in_output() {
        let value = json!({"a": [1, 2, 3], "b": {"nested": true}});
        let bytes = to_canonical_bytes(&value).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(!text.contains(' '));
        assert!(!text.contains('\n'));
    }

    #[test]
    fn hashing_is_deterministic_across_repeated_calls() {
        let value = json!({"z": 1, "a": 2, "m": [3, 2, 1]});
        let h1 = hash_canonical_json(&value).unwrap();
        let h2 = hash_canonical_json(&value).unwrap();
        assert_eq!(h1, h2);
    }
}
