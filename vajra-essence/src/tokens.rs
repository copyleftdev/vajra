//! Simple token estimation without external dependencies.
//!
//! Uses the approximation: tokens ~ word_count * 1.3.
//! This is deliberately simple -- no tiktoken dependency needed.

/// Estimate token count for a string.
///
/// Uses the approximation: tokens ~ word_count * 1.3.
/// This is deliberately simple -- no tiktoken dependency needed.
#[must_use]
pub fn estimate_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    let word_count = text.split_whitespace().count();
    // Also count punctuation-heavy tokens (JSON braces, colons, etc.)
    let punct_extra = text
        .chars()
        .filter(|c| matches!(c, '{' | '}' | '[' | ']' | ':' | ',' | '"'))
        .count();
    // Each punctuation cluster is roughly 1 token per 2 punctuation chars
    let punct_tokens = punct_extra / 2;
    let base = ((word_count as f64) * 1.3).ceil() as usize;
    base + punct_tokens
}

/// Estimate token count for a JSON value (serialized form).
#[must_use]
pub fn estimate_json_tokens(value: &serde_json::Value) -> usize {
    let serialized = serde_json::to_string(value).unwrap_or_default();
    estimate_tokens(&serialized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string_zero_tokens() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn single_word() {
        let tokens = estimate_tokens("hello");
        assert!(
            tokens >= 1,
            "single word should be at least 1 token, got {tokens}"
        );
    }

    #[test]
    fn known_sentence_reasonable_estimate() {
        // "The quick brown fox jumps over the lazy dog" = 9 words
        // Expected: ~9 * 1.3 = ~12 tokens
        let tokens = estimate_tokens("The quick brown fox jumps over the lazy dog");
        assert!(
            (8..=18).contains(&tokens),
            "9-word sentence should yield 8-18 tokens, got {tokens}"
        );
    }

    #[test]
    fn json_tokens_reasonable() {
        let val = serde_json::json!({"key": "value", "num": 42});
        let tokens = estimate_json_tokens(&val);
        assert!(tokens > 0, "JSON value should have non-zero tokens");
    }

    #[test]
    fn longer_text_scales_linearly() {
        let short = estimate_tokens("hello world");
        let long = estimate_tokens("hello world and many more words added to this sentence here");
        assert!(
            long > short,
            "longer text should have more tokens: short={short}, long={long}"
        );
    }
}
