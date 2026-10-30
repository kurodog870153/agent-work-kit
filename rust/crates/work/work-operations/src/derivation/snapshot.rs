//! Canonical raw-byte snapshots for transaction evidence.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};

use crate::canonical::sha256_hex;
use crate::protocol::valid_sha256;
use crate::specification::transaction::TransactionIssue;

fn issue(reason_code: &'static str, message: &'static str) -> TransactionIssue {
    TransactionIssue {
        reason_code,
        message,
        details: json!({}),
    }
}

fn strict(value: &Value, required: &[&str], optional: &[&str]) -> Result<(), TransactionIssue> {
    let Some(object) = value.as_object() else {
        return Err(issue(
            "invalid_contract_value",
            "The JSON contract contains an invalid value.",
        ));
    };
    if required.iter().any(|field| !object.contains_key(*field))
        || object
            .keys()
            .any(|field| !required.contains(&field.as_str()) && !optional.contains(&field.as_str()))
    {
        return Err(issue(
            "invalid_object_fields",
            "The JSON object has missing or unknown fields.",
        ));
    }
    Ok(())
}

fn sha(value: &Value) -> bool {
    value.as_str().is_some_and(valid_sha256)
}

fn base64_encode(raw: &[u8]) -> String {
    STANDARD.encode(raw)
}

fn base64_decode(value: &str) -> Option<Vec<u8>> {
    let decoded = STANDARD.decode(value).ok()?;
    (STANDARD.encode(&decoded) == value).then_some(decoded)
}

pub fn encode_snapshot(raw: &[u8]) -> Value {
    json!({"raw_sha256": sha256_hex(raw), "base64": base64_encode(raw)})
}

pub fn decode_snapshot(value: &Value) -> Result<Vec<u8>, TransactionIssue> {
    strict(value, &["raw_sha256", "base64"], &[])?;
    let raw = base64_decode(value["base64"].as_str().ok_or_else(|| {
        issue(
            "invalid_contract_value",
            "Transaction bytes must use canonical base64.",
        )
    })?)
    .ok_or_else(|| {
        issue(
            "invalid_contract_value",
            "Transaction bytes must use canonical base64.",
        )
    })?;
    if !sha(&value["raw_sha256"]) || value["raw_sha256"] != sha256_hex(&raw) {
        return Err(issue(
            "invalid_contract_value",
            "Transaction bytes do not match their fingerprint.",
        ));
    }
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_base64_round_trips_and_rejects_noncanonical_input() {
        for raw in [b"".as_slice(), b"f", b"fo", b"foo", &[0, 255]] {
            assert_eq!(decode_snapshot(&encode_snapshot(raw)).unwrap(), raw);
        }
        for invalid in ["Zg", "Zh==", "Zg===", "Zg==\n", "Zg--"] {
            let mut snapshot = encode_snapshot(b"f");
            snapshot["base64"] = json!(invalid);
            assert_eq!(
                decode_snapshot(&snapshot).unwrap_err().reason_code,
                "invalid_contract_value"
            );
        }
    }
}
