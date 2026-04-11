//! Lempel-Ziv complexity: structural complexity beyond Shannon entropy.
//!
//! Shannon entropy measures **average information per symbol**. LZ complexity
//! measures **the number of distinct subpatterns** needed to describe a sequence.
//!
//! This separates three classes that entropy conflates:
//! - Random data: high entropy, **high** LZ complexity
//! - Structured identifiers (`PROJ-001`, `PROJ-002`): high entropy, **low** LZ complexity
//! - Constant values: low entropy, low LZ complexity
//!
//! Reference: Lempel, A. & Ziv, J. (1976). "On the complexity of finite sequences."

use serde::{Deserialize, Serialize};

/// Result of Lempel-Ziv complexity analysis for a sequence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LzResult {
    /// Raw phrase count from LZ76 parsing.
    pub phrase_count: usize,
    /// Normalized complexity in [0, 1].
    /// 0 = maximally compressible (constant), 1 = incompressible (random).
    pub normalized: f64,
    /// Length of the input sequence in bytes.
    pub input_length: usize,
}

impl Default for LzResult {
    fn default() -> Self {
        Self {
            phrase_count: 0,
            normalized: 0.0,
            input_length: 0,
        }
    }
}

/// Count the number of distinct phrases in the LZ76 factorization of a byte sequence.
///
/// The LZ76 algorithm scans left-to-right, extending the current phrase until
/// it has not been seen before. Each new phrase is added to the dictionary.
///
/// Returns 0 for empty input.
#[must_use]
pub fn lz76_phrase_count(data: &[u8]) -> usize {
    if data.is_empty() {
        return 0;
    }

    let n = data.len();
    let mut phrases: usize = 0;
    let mut i: usize = 0;

    while i < n {
        // Find the longest prefix of data[i..] that appears in data[0..i]
        let mut max_match = 0;
        let haystack_end = i;

        if haystack_end > 0 {
            // Search for the longest match of data[i..] in data[0..haystack_end]
            let remaining = n - i;
            for start in 0..haystack_end {
                let mut len = 0;
                while len < remaining
                    && start + len < haystack_end
                    && data[start + len] == data[i + len]
                {
                    len += 1;
                }
                if len > max_match {
                    max_match = len;
                }
            }
        }

        // New phrase is the matched prefix + one novel byte (or just one byte if no match)
        let phrase_len = max_match + 1;
        phrases += 1;
        i += phrase_len;
    }

    phrases
}

/// Compute the normalized Lempel-Ziv complexity of a byte sequence.
///
/// Normalization: `C = phrase_count / (n / log₂(n))` where n is the sequence
/// length. This gives a value in approximately [0, 1]:
/// - Near 0 for highly compressible (constant/periodic) sequences
/// - Near 1 for incompressible (random) sequences
///
/// Returns 0.0 for empty input or sequences shorter than 2 bytes.
#[must_use]
pub fn lz76_complexity(data: &[u8]) -> f64 {
    let n = data.len();
    if n < 2 {
        return 0.0;
    }

    let phrases = lz76_phrase_count(data);
    if phrases == 0 {
        return 0.0;
    }

    // Theoretical maximum phrase count for a random sequence of length n
    // over a binary alphabet: n / log₂(n)
    #[allow(clippy::cast_precision_loss)]
    let n_f = n as f64;
    let max_phrases = n_f / n_f.log2();

    if max_phrases <= 0.0 {
        return 0.0;
    }

    #[allow(clippy::cast_precision_loss)]
    let normalized = phrases as f64 / max_phrases;

    // Clamp to [0, 1] — random sequences can slightly exceed 1.0
    normalized.clamp(0.0, 1.0)
}

/// Compute full LZ complexity analysis for a byte sequence.
#[must_use]
pub fn lz_analyze(data: &[u8]) -> LzResult {
    LzResult {
        phrase_count: lz76_phrase_count(data),
        normalized: lz76_complexity(data),
        input_length: data.len(),
    }
}

/// Compute LZ complexity for a collection of string values (e.g., from a JSON field).
///
/// Concatenates all values with a separator byte (0x00) and computes
/// complexity on the combined sequence.
#[must_use]
pub fn lz_complexity_for_values(values: &[&str]) -> LzResult {
    if values.is_empty() {
        return LzResult::default();
    }

    // Concatenate with null byte separator (deterministic)
    let mut combined = Vec::new();
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            combined.push(0x00);
        }
        combined.extend_from_slice(v.as_bytes());
    }

    lz_analyze(&combined)
}

/// Classify the entropy-complexity relationship for a field.
///
/// Given normalized Shannon entropy and normalized LZ complexity,
/// returns one of four quadrants:
/// - `"random"`: high entropy, high complexity (truly random)
/// - `"structured"`: high entropy, low complexity (patterned identifiers)
/// - `"constant"`: low entropy, low complexity (repeated values)
/// - `"anomalous"`: low entropy, high complexity (theoretically unlikely)
#[must_use]
pub fn classify_entropy_complexity(normalized_entropy: f64, lz_complexity: f64) -> &'static str {
    let entropy_threshold = 0.5;
    let complexity_threshold = 0.5;

    match (
        normalized_entropy > entropy_threshold,
        lz_complexity > complexity_threshold,
    ) {
        (true, true) => "random",
        (true, false) => "structured",
        (false, false) => "constant",
        (false, true) => "anomalous",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- lz76_phrase_count ----

    #[test]
    fn empty_returns_zero() {
        assert_eq!(lz76_phrase_count(&[]), 0);
    }

    #[test]
    fn single_byte() {
        assert_eq!(lz76_phrase_count(&[42]), 1);
    }

    #[test]
    fn constant_sequence() {
        // "aaaa" → phrases: "a", "aa", "a" (leftover)
        // Low phrase count relative to length
        let data = vec![b'a'; 100];
        let phrases = lz76_phrase_count(&data);
        // Constant sequence should have very few phrases
        assert!(
            phrases < 20,
            "constant sequence should have few phrases, got {phrases}"
        );
    }

    #[test]
    fn two_distinct_bytes() {
        // "ab" → "a", "b" = 2 phrases
        assert_eq!(lz76_phrase_count(b"ab"), 2);
    }

    #[test]
    fn repeated_pattern() {
        // "abcabcabc" should have fewer phrases than random
        let data = b"abcabcabcabcabcabcabcabcabcabc"; // 30 bytes
        let phrases = lz76_phrase_count(data);
        // Should be quite compressible
        assert!(
            phrases < 15,
            "repeated pattern should compress well, got {phrases}"
        );
    }

    // ---- lz76_complexity ----

    #[test]
    fn empty_complexity_zero() {
        assert!((lz76_complexity(&[]) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn single_byte_complexity_zero() {
        assert!((lz76_complexity(&[42]) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn constant_low_complexity() {
        let data = vec![b'x'; 1000];
        let c = lz76_complexity(&data);
        assert!(
            c < 0.2,
            "constant sequence should have low complexity, got {c}"
        );
    }

    #[test]
    fn structured_identifiers_lower_than_random_bytes() {
        // Structured identifiers share a prefix pattern, so they should
        // compress better than random bytes of the same total length.
        let values: Vec<String> = (0..100).map(|i| format!("PROJ-{i:03}")).collect();
        let refs: Vec<&str> = values.iter().map(|s| s.as_str()).collect();
        let structured = lz_complexity_for_values(&refs);

        // Compare against random-looking values of similar length
        let random_vals: Vec<String> = (0..100)
            .map(|i| {
                // Pseudo-random looking strings with no shared prefix
                format!("{:08x}", (i * 2654435761_u64) & 0xFFFF_FFFF)
            })
            .collect();
        let random_refs: Vec<&str> = random_vals.iter().map(|s| s.as_str()).collect();
        let random = lz_complexity_for_values(&random_refs);

        assert!(
            structured.normalized <= random.normalized,
            "structured ({}) should be <= random ({})",
            structured.normalized,
            random.normalized
        );
    }

    // ---- lz_analyze ----

    #[test]
    fn analyze_returns_consistent() {
        let data = b"hello world hello world";
        let result = lz_analyze(data);
        assert_eq!(result.input_length, data.len());
        assert_eq!(result.phrase_count, lz76_phrase_count(data));
        assert!((result.normalized - lz76_complexity(data)).abs() < f64::EPSILON);
    }

    // ---- lz_complexity_for_values ----

    #[test]
    fn empty_values() {
        let result = lz_complexity_for_values(&[]);
        assert_eq!(result.phrase_count, 0);
        assert!((result.normalized - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn single_value() {
        let result = lz_complexity_for_values(&["hello"]);
        assert!(result.phrase_count > 0);
    }

    #[test]
    fn repeated_values_low_complexity() {
        let values = vec!["active"; 100];
        let refs: Vec<&str> = values.to_vec();
        let result = lz_complexity_for_values(&refs);
        assert!(
            result.normalized < 0.3,
            "repeated identical values should be low complexity, got {}",
            result.normalized
        );
    }

    // ---- classify_entropy_complexity ----

    #[test]
    fn classify_random() {
        assert_eq!(classify_entropy_complexity(0.9, 0.9), "random");
    }

    #[test]
    fn classify_structured() {
        assert_eq!(classify_entropy_complexity(0.9, 0.3), "structured");
    }

    #[test]
    fn classify_constant() {
        assert_eq!(classify_entropy_complexity(0.1, 0.1), "constant");
    }

    #[test]
    fn classify_anomalous() {
        assert_eq!(classify_entropy_complexity(0.1, 0.9), "anomalous");
    }
}
