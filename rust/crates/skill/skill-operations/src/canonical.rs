//! Canonical text and content hashes shared by Skill sources.

use unicode_normalization::UnicodeNormalization;

pub fn canonical_text(text: &str) -> String {
    let normalized: String = text.nfc().collect();
    let normalized = normalized.replace("\r\n", "\n").replace('\r', "\n");
    let trimmed = normalized.trim_end_matches('\n');
    format!("{trimmed}\n")
}

pub fn canonical_sha256(raw: &[u8]) -> Result<String, std::str::Utf8Error> {
    let bytes = raw.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(raw);
    let text = std::str::from_utf8(bytes)?;
    Ok(crate::sha256_hex(canonical_text(text).as_bytes()))
}

pub fn raw_sha256(raw: &[u8]) -> String {
    crate::sha256_hex(raw)
}

pub fn valid_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_hash_normalizes_unicode_and_line_endings() {
        assert_eq!(canonical_text("cafe\u{301}\r\n\n"), "café\n");
        assert_eq!(
            canonical_sha256(b"\xef\xbb\xbfhello\r\n").unwrap(),
            raw_sha256(b"hello\n")
        );
        assert!(canonical_sha256(b"\xff").is_err());
        assert!(valid_sha256_hex(&raw_sha256(b"hello")));
        assert!(!valid_sha256_hex("abc"));
        assert!(!valid_sha256_hex(&"A".repeat(64)));
    }
}
