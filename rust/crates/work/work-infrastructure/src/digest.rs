//! Integrity digest of stored content. This is the only place that names the
//! algorithm (SHA-256, lowercase hex); the model and other layers only carry
//! the resulting text.

use sha2::{Digest, Sha256};

/// Digest of arbitrary bytes as lowercase hex text.
pub fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests;
