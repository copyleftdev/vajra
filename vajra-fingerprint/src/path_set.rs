//! Path set fingerprinting: BLAKE3 hash of the sorted set of distinct wildcard paths.

use vajra_types::Document;

/// Compute a BLAKE3 hash of the sorted set of distinct wildcard paths from the document's trie.
///
/// The trie already stores paths in deterministic (`BTreeMap`) order, so
/// `all_paths()` yields a sorted, deduplicated list. We hash their
/// canonical string representations to produce a 32-byte fingerprint.
#[must_use]
pub fn path_set_fingerprint(doc: &Document) -> [u8; 32] {
    let paths = doc.trie().all_paths();
    let mut hasher = blake3::Hasher::new();
    for path in &paths {
        hasher.update(path.as_str().as_bytes());
        // Separator to prevent ambiguity between adjacent paths.
        hasher.update(b"\x00");
    }
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use vajra_core::parse::parse_str;

    #[test]
    fn deterministic_same_doc() -> Result<(), Box<dyn std::error::Error>> {
        let input = r#"{"a": 1, "b": [2, 3], "c": {"d": true}}"#;
        let doc = parse_str(input)?;
        let h1 = path_set_fingerprint(&doc);
        let h2 = path_set_fingerprint(&doc);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 32);
        Ok(())
    }

    #[test]
    fn ignores_key_ordering() -> Result<(), Box<dyn std::error::Error>> {
        let input_a = r#"{"z": 1, "a": 2, "m": 3}"#;
        let input_b = r#"{"a": 2, "m": 3, "z": 1}"#;
        let doc_a = parse_str(input_a)?;
        let doc_b = parse_str(input_b)?;
        assert_eq!(path_set_fingerprint(&doc_a), path_set_fingerprint(&doc_b));
        Ok(())
    }

    #[test]
    fn different_structures_differ() -> Result<(), Box<dyn std::error::Error>> {
        let doc_a = parse_str(r#"{"a": 1}"#)?;
        let doc_b = parse_str(r#"{"a": 1, "b": 2}"#)?;
        assert_ne!(path_set_fingerprint(&doc_a), path_set_fingerprint(&doc_b));
        Ok(())
    }

    #[test]
    fn hash_is_32_bytes() -> Result<(), Box<dyn std::error::Error>> {
        let doc = parse_str(r#"{"x": 1}"#)?;
        let h = path_set_fingerprint(&doc);
        assert_eq!(h.len(), 32);
        Ok(())
    }
}
