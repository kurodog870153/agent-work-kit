//! Typed text encoding of stored records. The text form is the formal record;
//! callers keep the encoded text verbatim and never re-normalise it.

use serde::Serialize;
use serde::de::DeserializeOwned;
use work_model::{Reason, WorkError};

/// Encode one record as canonical pretty text ending in a newline.
pub fn encode<T: Serialize>(record: &T) -> Result<String, WorkError> {
    let mut text = serde_json::to_string_pretty(record)
        .map_err(|error| WorkError::new(Reason::InvalidRecord, error.to_string()))?;
    text.push('\n');
    Ok(text)
}

/// Decode a stored record, rejecting unknown fields and malformed text.
pub fn decode<T: DeserializeOwned>(text: &str) -> Result<T, WorkError> {
    serde_json::from_str(text)
        .map_err(|error| WorkError::new(Reason::InvalidRecord, error.to_string()))
}

#[cfg(test)]
mod tests;
