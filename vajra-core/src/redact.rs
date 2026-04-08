//! Redaction replaces sensitive values with deterministic placeholders
//! BEFORE they reach essence rendering. The same input always produces
//! the same redacted output.

use regex::Regex;

/// A redaction engine that applies rules to replace sensitive values with
/// deterministic placeholders.
pub struct Redactor {
    rules: Vec<RedactionRule>,
}

/// A single redaction rule consisting of a name, a matching pattern, and a
/// replacement strategy.
pub struct RedactionRule {
    /// Human-readable name for this rule (e.g. "SSN", "EMAIL").
    pub name: String,
    /// Regex pattern that identifies sensitive values.
    pub pattern: Regex,
    /// How to replace matched values.
    pub replacement: RedactionReplacement,
    /// Optional validator that runs after a regex match to confirm redaction.
    /// Return `true` if the match should be redacted.
    pub validator: Option<fn(&str) -> bool>,
}

/// Strategy for replacing redacted values.
pub enum RedactionReplacement {
    /// Replace with `[REDACTED:{name}]`.
    Label,
    /// Replace with deterministic hash (first 8 chars of BLAKE3 hash of original
    /// value). Allows correlation without revealing the value.
    DeterministicHash,
    /// Replace with a fixed string.
    Fixed(String),
}

impl Redactor {
    /// Creates a new `Redactor` with no rules.
    #[must_use]
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Creates a new `Redactor` pre-loaded with all built-in patterns:
    /// SSN, EMAIL, PHONE, `CREDIT_CARD`, and `DATE_OF_BIRTH`.
    #[must_use]
    pub fn with_builtins() -> Self {
        let mut r = Self::new();

        // SSN: XXX-XX-XXXX or XXXXXXXXX (9 contiguous digits)
        r.add_rule(RedactionRule {
            name: "SSN".into(),
            pattern: Regex::new(r"\b(\d{3}-\d{2}-\d{4}|\d{9})\b")
                .unwrap_or_else(|e| unreachable!("SSN regex is valid: {e}")),
            replacement: RedactionReplacement::Label,
            validator: None,
        });

        // EMAIL: simple common pattern
        r.add_rule(RedactionRule {
            name: "EMAIL".into(),
            pattern: Regex::new(r"(?i)\b[A-Z0-9._%+\-]+@[A-Z0-9.\-]+\.[A-Z]{2,}\b")
                .unwrap_or_else(|e| unreachable!("EMAIL regex is valid: {e}")),
            replacement: RedactionReplacement::Label,
            validator: None,
        });

        // PHONE: US patterns
        //   (XXX) XXX-XXXX
        //   XXX-XXX-XXXX
        //   +1XXXXXXXXXX
        r.add_rule(RedactionRule {
            name: "PHONE".into(),
            pattern: Regex::new(r"(?:\(\d{3}\)\s*\d{3}-\d{4}|\b\d{3}-\d{3}-\d{4}\b|\+1\d{10}\b)")
                .unwrap_or_else(|e| unreachable!("PHONE regex is valid: {e}")),
            replacement: RedactionReplacement::Label,
            validator: None,
        });

        // CREDIT_CARD: 13-19 digit sequences with optional spaces/dashes, plus Luhn check
        r.add_rule(RedactionRule {
            name: "CREDIT_CARD".into(),
            pattern: Regex::new(r"\b(\d[\d \-]{11,22}\d)\b")
                .unwrap_or_else(|e| unreachable!("CREDIT_CARD regex is valid: {e}")),
            replacement: RedactionReplacement::Label,
            validator: Some(|matched: &str| {
                let digits: Vec<u8> = matched
                    .bytes()
                    .filter(|b| b.is_ascii_digit())
                    .map(|b| b - b'0')
                    .collect();
                (13..=19).contains(&digits.len()) && luhn_check(&digits)
            }),
        });

        // DATE_OF_BIRTH: dates in common formats, context-aware (relies on key
        // matching done in `redact_json` / caller).  As a standalone value
        // pattern we match MM/DD/YYYY, YYYY-MM-DD, MM-DD-YYYY.
        r.add_rule(RedactionRule {
            name: "DATE_OF_BIRTH".into(),
            pattern: Regex::new(
                r"\b(?:\d{1,2}[/\-]\d{1,2}[/\-]\d{2,4}|\d{4}[/\-]\d{1,2}[/\-]\d{1,2})\b",
            )
            .unwrap_or_else(|e| unreachable!("DOB regex is valid: {e}")),
            replacement: RedactionReplacement::Label,
            validator: None,
        });

        r
    }

    /// Appends a custom rule to this redactor.
    pub fn add_rule(&mut self, rule: RedactionRule) {
        self.rules.push(rule);
    }

    /// Attempt to redact `value`. Returns `Some(redacted_string)` if any rule
    /// matched, or `None` if no rule applied.
    #[must_use]
    pub fn redact_value(&self, value: &str) -> Option<String> {
        self.redact_value_contextual(value, None)
    }

    /// Redact all string values in a JSON tree in-place.
    ///
    /// For objects, the `DATE_OF_BIRTH` rule is context-aware: it only applies
    /// when the key contains "dob", "birth", or "born" (case-insensitive).
    pub fn redact_json(&self, value: &mut serde_json::Value) {
        self.redact_json_inner(value, None);
    }

    /// Returns a redacted clone of the given JSON value without modifying the
    /// original.
    #[must_use]
    pub fn redact_json_cloned(&self, value: &serde_json::Value) -> serde_json::Value {
        let mut cloned = value.clone();
        self.redact_json(&mut cloned);
        cloned
    }

    // ------------------------------------------------------------------
    // Private helpers
    // ------------------------------------------------------------------

    /// Internal recursive walker for JSON redaction.
    fn redact_json_inner(&self, value: &mut serde_json::Value, key_hint: Option<&str>) {
        match value {
            serde_json::Value::String(s) => {
                if let Some(redacted) = self.redact_value_contextual(s, key_hint) {
                    *s = redacted;
                }
            }
            serde_json::Value::Array(arr) => {
                for item in arr {
                    self.redact_json_inner(item, None);
                }
            }
            serde_json::Value::Object(map) => {
                let keys: Vec<String> = map.keys().cloned().collect();
                for k in keys {
                    if let Some(v) = map.get_mut(&k) {
                        self.redact_json_inner(v, Some(&k));
                    }
                }
            }
            _ => {}
        }
    }

    /// Like `redact_value`, but with context-awareness for `DATE_OF_BIRTH`.
    fn redact_value_contextual(&self, value: &str, key_hint: Option<&str>) -> Option<String> {
        for rule in &self.rules {
            // DATE_OF_BIRTH is context-aware: only fire when the key suggests
            // it is a date-of-birth field.
            if rule.name == "DATE_OF_BIRTH" {
                if let Some(key) = key_hint {
                    let lower = key.to_lowercase();
                    if !lower.contains("dob") && !lower.contains("birth") && !lower.contains("born")
                    {
                        continue;
                    }
                } else {
                    // No key context -- skip DOB rule
                    continue;
                }
            }

            if rule.pattern.find(value).is_none() {
                continue;
            }

            // At least one regex match exists. Walk matches, apply optional
            // validator, and build the replacement string.
            let mut result = String::with_capacity(value.len());
            let mut last_end = 0;
            let mut any_replaced = false;

            for mat in rule.pattern.find_iter(value) {
                let mat_str = mat.as_str();

                if let Some(validate) = rule.validator {
                    if !validate(mat_str) {
                        continue;
                    }
                }

                let replacement_text = self.replacement_for(mat_str, &rule.name, &rule.replacement);

                result.push_str(&value[last_end..mat.start()]);
                result.push_str(&replacement_text);
                last_end = mat.end();
                any_replaced = true;
            }

            if any_replaced {
                result.push_str(&value[last_end..]);
                return Some(result);
            }
        }
        None
    }

    /// Produce the replacement string for a single matched span.
    fn replacement_for(
        &self,
        matched: &str,
        name: &str,
        replacement: &RedactionReplacement,
    ) -> String {
        match replacement {
            RedactionReplacement::Label => {
                format!("[REDACTED:{name}]")
            }
            RedactionReplacement::DeterministicHash => {
                let hash = blake3::hash(matched.as_bytes());
                let hex = hash.to_hex();
                format!("[HASH:{}]", &hex[..8])
            }
            RedactionReplacement::Fixed(s) => s.clone(),
        }
    }
}

impl Default for Redactor {
    fn default() -> Self {
        Self::new()
    }
}

/// Standard Luhn algorithm check.
///
/// `digits` must be a slice of single-digit values (0-9).
fn luhn_check(digits: &[u8]) -> bool {
    if digits.is_empty() {
        return false;
    }
    let mut sum: u32 = 0;
    let parity = digits.len() % 2;
    for (i, &d) in digits.iter().enumerate() {
        let mut val = u32::from(d);
        if i % 2 == parity {
            val *= 2;
            if val > 9 {
                val -= 9;
            }
        }
        sum += val;
    }
    sum.is_multiple_of(10)
}

// ======================================================================
// Tests
// ======================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ---- SSN tests ----

    #[test]
    fn ssn_with_dashes() {
        let r = Redactor::with_builtins();
        let result = r.redact_value("123-45-6789");
        assert_eq!(result, Some("[REDACTED:SSN]".into()));
    }

    #[test]
    fn ssn_without_dashes() {
        let r = Redactor::with_builtins();
        let result = r.redact_value("123456789");
        assert_eq!(result, Some("[REDACTED:SSN]".into()));
    }

    #[test]
    fn ssn_wrong_format_not_redacted() {
        let r = Redactor::with_builtins();
        let result = r.redact_value("12-345-6789");
        // Should NOT match SSN pattern
        assert!(
            result.is_none() || result.as_deref() != Some("[REDACTED:SSN]"),
            "12-345-6789 should not match SSN"
        );
    }

    // ---- Email tests ----

    #[test]
    fn email_redacted() {
        let r = Redactor::with_builtins();
        let result = r.redact_value("user@example.com");
        assert_eq!(result, Some("[REDACTED:EMAIL]".into()));
    }

    #[test]
    fn not_an_email() {
        let r = Redactor::with_builtins();
        let result = r.redact_value("not-an-email");
        assert!(result.is_none(), "not-an-email should not match");
    }

    // ---- Phone tests ----

    #[test]
    fn phone_with_parens() {
        let r = Redactor::with_builtins();
        let result = r.redact_value("(555) 123-4567");
        assert_eq!(result, Some("[REDACTED:PHONE]".into()));
    }

    #[test]
    fn phone_with_dashes() {
        let r = Redactor::with_builtins();
        let result = r.redact_value("555-123-4567");
        assert_eq!(result, Some("[REDACTED:PHONE]".into()));
    }

    #[test]
    fn phone_plus_one() {
        let r = Redactor::with_builtins();
        let result = r.redact_value("+15551234567");
        assert_eq!(result, Some("[REDACTED:PHONE]".into()));
    }

    // ---- Credit card tests ----

    #[test]
    fn credit_card_valid_luhn() {
        let r = Redactor::with_builtins();
        let result = r.redact_value("4111 1111 1111 1111");
        assert_eq!(result, Some("[REDACTED:CREDIT_CARD]".into()));
    }

    #[test]
    fn credit_card_fails_luhn() {
        let r = Redactor::with_builtins();
        let result = r.redact_value("1234567890123");
        assert!(
            result.is_none() || result.as_deref() != Some("[REDACTED:CREDIT_CARD]"),
            "1234567890123 should fail Luhn and not be redacted as credit card"
        );
    }

    // ---- Determinism tests ----

    #[test]
    fn deterministic_hash_same_value() {
        let mut r = Redactor::new();
        r.add_rule(RedactionRule {
            name: "TEST".into(),
            pattern: Regex::new(r"\bsecret\b").unwrap_or_else(|e| unreachable!("{e}")),
            replacement: RedactionReplacement::DeterministicHash,
            validator: None,
        });
        let a = r.redact_value("secret");
        let b = r.redact_value("secret");
        assert!(a.is_some());
        assert_eq!(a, b, "same input must produce same output");
    }

    #[test]
    fn deterministic_label_same_value() {
        let r = Redactor::with_builtins();
        let a = r.redact_value("123-45-6789");
        let b = r.redact_value("123-45-6789");
        assert_eq!(a, b);
    }

    // ---- JSON redaction tests ----

    #[test]
    fn redact_json_walks_tree() {
        let r = Redactor::with_builtins();
        let mut doc = json!({
            "name": "Alice",
            "ssn": "123-45-6789",
            "contact": {
                "email": "alice@example.com",
                "phone": "(555) 123-4567"
            },
            "scores": [1, 2, 3]
        });

        r.redact_json(&mut doc);

        assert_eq!(doc["ssn"], "[REDACTED:SSN]");
        assert_eq!(doc["contact"]["email"], "[REDACTED:EMAIL]");
        assert_eq!(doc["contact"]["phone"], "[REDACTED:PHONE]");
        // Non-matching fields should be unchanged
        assert_eq!(doc["name"], "Alice");
        assert_eq!(doc["scores"], json!([1, 2, 3]));
    }

    #[test]
    fn redact_json_cloned_does_not_modify_original() {
        let r = Redactor::with_builtins();
        let original = json!({
            "ssn": "123-45-6789",
            "email": "alice@example.com"
        });

        let redacted = r.redact_json_cloned(&original);

        // Original should be unmodified
        assert_eq!(original["ssn"], "123-45-6789");
        assert_eq!(original["email"], "alice@example.com");

        // Redacted copy should have replacements
        assert_eq!(redacted["ssn"], "[REDACTED:SSN]");
        assert_eq!(redacted["email"], "[REDACTED:EMAIL]");
    }

    // ---- Date of birth context-aware tests ----

    #[test]
    fn dob_redacted_in_context() {
        let r = Redactor::with_builtins();
        let mut doc = json!({
            "dob": "01/15/1990",
            "birthday": "1990-01-15",
            "born_date": "01-15-1990",
            "created_at": "01/15/1990"
        });

        r.redact_json(&mut doc);

        assert_eq!(doc["dob"], "[REDACTED:DATE_OF_BIRTH]");
        assert_eq!(doc["birthday"], "[REDACTED:DATE_OF_BIRTH]");
        assert_eq!(doc["born_date"], "[REDACTED:DATE_OF_BIRTH]");
        // "created_at" does NOT contain dob/birth/born, so the date stays
        assert_eq!(doc["created_at"], "01/15/1990");
    }

    // ---- Luhn algorithm unit tests ----

    #[test]
    fn luhn_valid() {
        // 4111111111111111 is valid
        let digits: Vec<u8> = "4111111111111111".bytes().map(|b| b - b'0').collect();
        assert!(luhn_check(&digits));
    }

    #[test]
    fn luhn_invalid() {
        let digits: Vec<u8> = "1234567890123".bytes().map(|b| b - b'0').collect();
        assert!(!luhn_check(&digits));
    }

    #[test]
    fn luhn_empty() {
        assert!(!luhn_check(&[]));
    }

    // ---- Fixed replacement test ----

    #[test]
    fn fixed_replacement() {
        let mut r = Redactor::new();
        r.add_rule(RedactionRule {
            name: "TOKEN".into(),
            pattern: Regex::new(r"tok_[a-zA-Z0-9]+").unwrap_or_else(|e| unreachable!("{e}")),
            replacement: RedactionReplacement::Fixed("***".into()),
            validator: None,
        });
        let result = r.redact_value("bearer tok_abc123xyz");
        assert_eq!(result, Some("bearer ***".into()));
    }

    // ---- Default trait ----

    #[test]
    fn default_is_empty() {
        let r = Redactor::default();
        assert!(r.redact_value("123-45-6789").is_none());
    }

    // ---- Embedded sensitive data in longer strings ----

    #[test]
    fn ssn_embedded_in_text() {
        let r = Redactor::with_builtins();
        let result = r.redact_value("My SSN is 123-45-6789 please");
        assert_eq!(result, Some("My SSN is [REDACTED:SSN] please".into()));
    }

    #[test]
    fn email_embedded_in_text() {
        let r = Redactor::with_builtins();
        let result = r.redact_value("Contact me at user@example.com for info");
        assert_eq!(
            result,
            Some("Contact me at [REDACTED:EMAIL] for info".into())
        );
    }
}
