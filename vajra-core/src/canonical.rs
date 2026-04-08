//! RFC 8785 (JCS) deterministic JSON serialization.
//!
//! Produces canonical JSON bytes where:
//! - Object keys are sorted by UTF-16 code unit comparison
//! - Numbers use the JCS formatting rules (ES6 `Number.toString()`)
//! - Strings use minimal JSON escaping
//! - No whitespace is emitted

use vajra_types::VajraError;

/// Canonicalize a JSON value into a deterministic byte string per RFC 8785.
///
/// # Errors
///
/// Returns [`VajraError::Analysis`] if serialization encounters an invalid number.
pub fn canonicalize(value: &serde_json::Value) -> Result<Vec<u8>, VajraError> {
    let mut buf = Vec::new();
    serialize_value(value, &mut buf)?;
    Ok(buf)
}

/// Serialize a JSON value to canonical form, appending to `buf`.
fn serialize_value(value: &serde_json::Value, buf: &mut Vec<u8>) -> Result<(), VajraError> {
    match value {
        serde_json::Value::Null => {
            buf.extend_from_slice(b"null");
        }
        serde_json::Value::Bool(b) => {
            if *b {
                buf.extend_from_slice(b"true");
            } else {
                buf.extend_from_slice(b"false");
            }
        }
        serde_json::Value::Number(n) => {
            serialize_number(n, buf)?;
        }
        serde_json::Value::String(s) => {
            serialize_string(s, buf);
        }
        serde_json::Value::Array(arr) => {
            buf.push(b'[');
            for (i, item) in arr.iter().enumerate() {
                if i > 0 {
                    buf.push(b',');
                }
                serialize_value(item, buf)?;
            }
            buf.push(b']');
        }
        serde_json::Value::Object(map) => {
            // RFC 8785: keys sorted by UTF-16 code unit comparison
            let sorted = sort_keys_utf16(map);
            buf.push(b'{');
            for (i, (key, val)) in sorted.iter().enumerate() {
                if i > 0 {
                    buf.push(b',');
                }
                serialize_string(key, buf);
                buf.push(b':');
                serialize_value(val, buf)?;
            }
            buf.push(b'}');
        }
    }
    Ok(())
}

/// Sort object keys by UTF-16 code unit comparison per RFC 8785 section 3.2.3.
///
/// This differs from lexicographic byte ordering for characters outside the BMP.
/// We compare strings by their UTF-16 encoded code unit sequences.
fn sort_keys_utf16(
    map: &serde_json::Map<String, serde_json::Value>,
) -> Vec<(&str, &serde_json::Value)> {
    let mut entries: Vec<(&str, &serde_json::Value)> =
        map.iter().map(|(k, v)| (k.as_str(), v)).collect();
    entries.sort_by(|(a, _), (b, _)| cmp_utf16(a, b));
    entries
}

/// Compare two strings by their UTF-16 code unit sequences.
///
/// This is the comparison order mandated by RFC 8785 (JCS) for object keys.
/// It matches the ECMAScript abstract relational comparison for strings.
fn cmp_utf16(a: &str, b: &str) -> std::cmp::Ordering {
    let a_units = a.encode_utf16();
    let b_units = b.encode_utf16();
    a_units.cmp(b_units)
}

/// Serialize a JSON number per JCS rules (ES6 `Number.toString()`).
///
/// Rules:
/// - Integers (no fractional part): emit as decimal integer, no trailing ".0"
/// - `-0` becomes `"0"` (JCS rule: negative zero is serialized as positive zero)
/// - Floats: use `ryu` for deterministic shortest-representation formatting
/// - NaN/Infinity are not valid JSON and rejected
fn serialize_number(n: &serde_json::Number, buf: &mut Vec<u8>) -> Result<(), VajraError> {
    // Try as i64 first (most common case for integers)
    if let Some(i) = n.as_i64() {
        buf.extend_from_slice(i.to_string().as_bytes());
        return Ok(());
    }

    // Try as u64 (for large positive integers beyond i64 range)
    if let Some(u) = n.as_u64() {
        buf.extend_from_slice(u.to_string().as_bytes());
        return Ok(());
    }

    // Must be a float
    if let Some(f) = n.as_f64() {
        if f.is_nan() || f.is_infinite() {
            return Err(VajraError::Analysis {
                path: String::new(),
                message: format!("JCS does not support NaN or Infinity: {n}"),
            });
        }

        // JCS rule: -0 is serialized as "0"
        if f == 0.0 {
            buf.push(b'0');
            return Ok(());
        }

        // Use ryu for deterministic float formatting
        let mut ryu_buf = ryu::Buffer::new();
        let formatted = ryu_buf.format_finite(f);

        // ryu may produce "3.14E2" style -- JCS uses ES6 Number.toString() rules.
        // We need to convert ryu's output to JCS-compliant format.
        let jcs_str = jcs_format_float(formatted, f);

        // Ensure serde_json round-trip idempotency: if serde_json parses
        // our output to a different f64 (due to parser differences between
        // ryu and serde_json), re-format using that f64 instead. This
        // guarantees: canon(serde_parse(canon(x))) == canon(x).
        if let Ok(serde_val) = serde_json::from_str::<serde_json::Value>(&jcs_str) {
            if let Some(rt) = serde_val.as_f64() {
                if rt.to_bits() != f.to_bits() {
                    let mut ryu_buf2 = ryu::Buffer::new();
                    let formatted2 = ryu_buf2.format_finite(rt);
                    buf.extend_from_slice(jcs_format_float(formatted2, rt).as_bytes());
                    return Ok(());
                }
            }
        }
        buf.extend_from_slice(jcs_str.as_bytes());

        return Ok(());
    }

    // Should not happen with valid serde_json::Number
    Err(VajraError::Analysis {
        path: String::new(),
        message: format!("cannot serialize number: {n}"),
    })
}

/// Convert ryu's float output to JCS-compliant ES6 `Number.toString()` format.
///
/// ES6 rules (simplified):
/// - If the number is an integer value (e.g., 1e2 = 100), emit as integer
/// - Use exponential notation when the exponent is >= 21 or <= -7
/// - Exponential form uses lowercase 'e' with explicit '+' for positive exponents
/// - No trailing zeros in the coefficient
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap
)]
fn jcs_format_float(ryu_str: &str, value: f64) -> String {
    // Check if value is actually an integer (no fractional part)
    // This handles cases like 1.0, 100.0, etc.
    if value.fract() == 0.0 && value.abs() < 1e21 && value.abs() >= 1.0 {
        // Format as integer
        if value >= 0.0 && value <= u64::MAX as f64 {
            return format!("{}", value as u64);
        }
        if value >= i64::MIN as f64 && value < 0.0 {
            return format!("{}", value as i64);
        }
    }

    // Parse ryu's output which may be in the form "1.23E4" or "1.23e4" or "1.23"
    // ryu uses capital E
    if let Some(e_pos) = ryu_str.find('E').or_else(|| ryu_str.find('e')) {
        let mantissa_str = &ryu_str[..e_pos];
        let exp_str = &ryu_str[e_pos + 1..];
        let exponent: i32 = match exp_str.parse() {
            Ok(e) => e,
            Err(_) => return ryu_str.to_string(),
        };

        // Determine sign and unsigned mantissa parts
        let (negative, unsigned_mantissa) = mantissa_str
            .strip_prefix('-')
            .map_or((false, mantissa_str), |stripped| (true, stripped));

        // Split mantissa into integer and fractional parts
        let (int_part, frac_part) =
            unsigned_mantissa
                .find('.')
                .map_or((unsigned_mantissa, ""), |dot_pos| {
                    (
                        &unsigned_mantissa[..dot_pos],
                        &unsigned_mantissa[dot_pos + 1..],
                    )
                });

        // Combine all significant digits
        let mut digits = String::from(int_part);
        digits.push_str(frac_part);

        // The actual decimal point position:
        // digits represent a number like d0.d1d2d3 * 10^exponent
        // So decimal point is after position (int_part.len() + exponent) from start
        let int_part_len = int_part.len() as i32;
        let decimal_pos = int_part_len + exponent;
        let total_digits = digits.len() as i32;

        // ES6 Number.toString rules:
        // 1. If decimal_pos > 0 and decimal_pos <= 21: no exponent notation
        //    a. If decimal_pos >= total_digits: integer (pad with zeros)
        //    b. Else: insert decimal point
        // 2. If decimal_pos <= 0 and decimal_pos > -6: "0.000...digits"
        // 3. Otherwise: exponential notation

        let result = if decimal_pos > 0 && decimal_pos <= 21 {
            if decimal_pos >= total_digits {
                // Integer form: pad with zeros
                let mut s = digits;
                for _ in 0..(decimal_pos - total_digits) {
                    s.push('0');
                }
                s
            } else {
                // Insert decimal point
                let dp = decimal_pos as usize;
                let mut s = digits[..dp].to_string();
                s.push('.');
                // Trim trailing zeros from fractional part
                let frac = digits[dp..].trim_end_matches('0');
                if frac.is_empty() {
                    // No fractional part after trimming -- this is an integer
                    s.pop(); // remove the '.'
                } else {
                    s.push_str(frac);
                }
                s
            }
        } else if decimal_pos <= 0 && decimal_pos > -6 {
            // "0.000...digits"
            let mut s = String::from("0.");
            for _ in 0..(-decimal_pos) {
                s.push('0');
            }
            // Trim trailing zeros
            let trimmed = digits.trim_end_matches('0');
            s.push_str(trimmed);
            s
        } else {
            // Exponential notation
            // ES6 format: d.ddd...e+N or d.ddd...e-N
            let actual_exp = decimal_pos - 1;
            if total_digits == 1 {
                let exp_sign = if actual_exp >= 0 { "+" } else { "" };
                format!("{}e{exp_sign}{actual_exp}", &digits[..1])
            } else {
                let trimmed = digits[1..].trim_end_matches('0');
                let exp_sign = if actual_exp >= 0 { "+" } else { "" };
                if trimmed.is_empty() {
                    format!("{}e{exp_sign}{actual_exp}", &digits[..1])
                } else {
                    format!("{}.{}e{exp_sign}{actual_exp}", &digits[..1], trimmed)
                }
            }
        };

        if negative {
            format!("-{result}")
        } else {
            result
        }
    } else if ryu_str.contains('.') {
        // No exponent in ryu output -- it's a simple decimal
        // Trim trailing zeros from the fractional part
        let trimmed = ryu_str.trim_end_matches('0');
        trimmed.strip_suffix('.').unwrap_or(trimmed).to_string()
    } else {
        ryu_str.to_string()
    }
}

/// Serialize a JSON string with minimal escaping per RFC 8785.
///
/// JCS requires escaping:
/// - `\b` (0x08), `\t` (0x09), `\n` (0x0A), `\f` (0x0C), `\r` (0x0D)
/// - `\"` (0x22), `\\` (0x5C)
/// - Control characters U+0000..U+001F as `\uXXXX`
/// - No escaping of `/` (unlike some JSON serializers)
fn serialize_string(s: &str, buf: &mut Vec<u8>) {
    buf.push(b'"');
    for ch in s.chars() {
        match ch {
            '"' => buf.extend_from_slice(b"\\\""),
            '\\' => buf.extend_from_slice(b"\\\\"),
            '\u{0008}' => buf.extend_from_slice(b"\\b"),
            '\u{000C}' => buf.extend_from_slice(b"\\f"),
            '\n' => buf.extend_from_slice(b"\\n"),
            '\r' => buf.extend_from_slice(b"\\r"),
            '\t' => buf.extend_from_slice(b"\\t"),
            c if c < '\u{0020}' => {
                // Other control characters: \u00XX
                let code = c as u32;
                buf.extend_from_slice(format!("\\u{code:04x}").as_bytes());
            }
            c => {
                let mut utf8_buf = [0u8; 4];
                buf.extend_from_slice(c.encode_utf8(&mut utf8_buf).as_bytes());
            }
        }
    }
    buf.push(b'"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Helper to canonicalize and return a String for comparison
    fn canon_str(value: &serde_json::Value) -> Result<String, Box<dyn std::error::Error>> {
        let bytes = canonicalize(value)?;
        Ok(String::from_utf8(bytes)?)
    }

    #[test]
    fn canonicalize_null() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(canon_str(&json!(null))?, "null");
        Ok(())
    }

    #[test]
    fn canonicalize_bool() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(canon_str(&json!(true))?, "true");
        assert_eq!(canon_str(&json!(false))?, "false");
        Ok(())
    }

    #[test]
    fn canonicalize_integer() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(canon_str(&json!(42))?, "42");
        assert_eq!(canon_str(&json!(0))?, "0");
        assert_eq!(canon_str(&json!(-1))?, "-1");
        assert_eq!(canon_str(&json!(1000000))?, "1000000");
        Ok(())
    }

    #[test]
    fn canonicalize_float() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(canon_str(&json!(2.75))?, "2.75");
        assert_eq!(canon_str(&json!(1.0))?, "1");
        assert_eq!(canon_str(&json!(0.5))?, "0.5");
        Ok(())
    }

    #[test]
    fn canonicalize_negative_zero() -> Result<(), Box<dyn std::error::Error>> {
        // JCS rule: -0 should be serialized as "0"
        let neg_zero: serde_json::Value = serde_json::from_str("-0.0")?;
        assert_eq!(canon_str(&neg_zero)?, "0");

        let neg_zero2: serde_json::Value = serde_json::from_str("-0")?;
        assert_eq!(canon_str(&neg_zero2)?, "0");
        Ok(())
    }

    #[test]
    fn canonicalize_string() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(canon_str(&json!("hello"))?, r#""hello""#);
        assert_eq!(canon_str(&json!(""))?, r#""""#);
        Ok(())
    }

    #[test]
    fn canonicalize_string_escaping() -> Result<(), Box<dyn std::error::Error>> {
        // Backslash and quote
        assert_eq!(canon_str(&json!("a\"b"))?, r#""a\"b""#);
        assert_eq!(canon_str(&json!("a\\b"))?, r#""a\\b""#);

        // Control characters
        assert_eq!(canon_str(&json!("a\nb"))?, r#""a\nb""#);
        assert_eq!(canon_str(&json!("a\tb"))?, r#""a\tb""#);
        assert_eq!(canon_str(&json!("a\rb"))?, r#""a\rb""#);

        // Forward slash should NOT be escaped per JCS
        assert_eq!(canon_str(&json!("a/b"))?, r#""a/b""#);
        Ok(())
    }

    #[test]
    fn canonicalize_empty_array() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(canon_str(&json!([]))?, "[]");
        Ok(())
    }

    #[test]
    fn canonicalize_array() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(canon_str(&json!([1, 2, 3]))?, "[1,2,3]");
        assert_eq!(
            canon_str(&json!([1, "two", true, null]))?,
            r#"[1,"two",true,null]"#
        );
        Ok(())
    }

    #[test]
    fn canonicalize_empty_object() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(canon_str(&json!({}))?, "{}");
        Ok(())
    }

    #[test]
    fn canonicalize_object_sorted_keys() -> Result<(), Box<dyn std::error::Error>> {
        // Keys must be sorted per UTF-16 code unit comparison
        let obj = json!({"b": 2, "a": 1, "c": 3});
        assert_eq!(canon_str(&obj)?, r#"{"a":1,"b":2,"c":3}"#);
        Ok(())
    }

    #[test]
    fn canonicalize_object_no_whitespace() -> Result<(), Box<dyn std::error::Error>> {
        let obj = json!({"key": "value"});
        let result = canon_str(&obj)?;
        // No spaces around colon or after comma
        assert!(!result.contains(": "));
        assert!(!result.contains(", "));
        Ok(())
    }

    #[test]
    fn canonicalize_nested() -> Result<(), Box<dyn std::error::Error>> {
        let obj = json!({"a": {"b": [1, 2]}, "c": true});
        assert_eq!(canon_str(&obj)?, r#"{"a":{"b":[1,2]},"c":true}"#);
        Ok(())
    }

    #[test]
    fn canonicalize_idempotent() -> Result<(), Box<dyn std::error::Error>> {
        // canon(parse(canon(x))) == canon(x)
        let values = vec![
            json!(null),
            json!(42),
            json!(2.75),
            json!("hello"),
            json!(true),
            json!([1, "two", null]),
            json!({"z": 1, "a": 2, "m": [true, false]}),
            json!({"nested": {"deep": {"value": 42}}}),
        ];

        for value in values {
            let first = canonicalize(&value)?;
            let first_str = String::from_utf8(first.clone())?;
            let reparsed: serde_json::Value = serde_json::from_str(&first_str)?;
            let second = canonicalize(&reparsed)?;
            assert_eq!(
                first,
                second,
                "idempotency failed for {value}: first={first_str}, second={}",
                String::from_utf8_lossy(&second)
            );
        }
        Ok(())
    }

    #[test]
    fn canonicalize_utf16_key_sorting() -> Result<(), Box<dyn std::error::Error>> {
        // RFC 8785 requires sorting by UTF-16 code units, not byte order.
        // Example: the key "\u{1D11E}" (U+1D11E, MUSICAL SYMBOL G CLEF) is a
        // surrogate pair in UTF-16 (D834 DD1E), which sorts differently from
        // some BMP characters.

        // More practical test: ASCII characters sort the same in UTF-8 and UTF-16,
        // but we verify the mechanism works.
        let obj = json!({"z": 1, "a": 2, "A": 3, "Z": 4});
        // UTF-16 ordering: uppercase letters (0x41-0x5A) come before
        // lowercase letters (0x61-0x7A)
        assert_eq!(canon_str(&obj)?, r#"{"A":3,"Z":4,"a":2,"z":1}"#);
        Ok(())
    }

    #[test]
    fn canonicalize_utf16_surrogate_pair_ordering() -> Result<(), Box<dyn std::error::Error>> {
        // Characters outside BMP have different UTF-16 vs UTF-8 ordering.
        // U+00E9 (e with accent, 1 UTF-16 unit: 00E9) vs U+1D11E (G clef, 2 UTF-16 units: D834 DD1E)
        // In UTF-16 code unit order: 00E9 < D834, so the accent char sorts first
        // But compare U+FFFF (1 unit: FFFF) vs U+10000 (2 units: D800 DC00)
        // UTF-16: FFFF > D800, so U+FFFF > U+10000 in UTF-16 order
        // UTF-8: EF BF BF vs F0 90 80 80, so EF < F0, meaning U+FFFF < U+10000 in byte order
        // This is where UTF-16 sorting differs from byte order!
        let obj_str = format!("{{\"\\uFFFF\": 1, \"{}\" : 2}}", '\u{10000}');
        let val: serde_json::Value = serde_json::from_str(&obj_str)?;
        let result = canonicalize(&val)?;
        let result_str = String::from_utf8(result)?;

        // In UTF-16 order: U+10000 (D800 DC00) < U+FFFF (FFFF)
        // So the key for U+10000 should come first
        // Find the positions of the values to determine key ordering
        let pos_1 = result_str.find(":1");
        let pos_2 = result_str.find(":2");
        assert!(
            pos_2.is_some() && pos_1.is_some(),
            "could not find values in result: {result_str}"
        );
        // U+10000 (value 2) should come before U+FFFF (value 1) in UTF-16 sort
        let p1 = pos_1.unwrap_or(0);
        let p2 = pos_2.unwrap_or(0);
        assert!(
            p2 < p1,
            "expected U+10000 key before U+FFFF key in UTF-16 order, got: {result_str}"
        );
        Ok(())
    }

    #[test]
    fn number_format_integer_42() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(canon_str(&json!(42))?, "42");
        Ok(())
    }

    #[test]
    fn number_format_float_3_14() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(canon_str(&json!(2.75))?, "2.75");
        Ok(())
    }

    #[test]
    fn number_format_large_integer() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(canon_str(&json!(9007199254740992_i64))?, "9007199254740992");
        Ok(())
    }

    #[test]
    fn number_format_small_float() -> Result<(), Box<dyn std::error::Error>> {
        // 0.000001 should not use exponential notation (exponent = -6, not <= -7)
        assert_eq!(canon_str(&json!(0.000001))?, "0.000001");
        Ok(())
    }

    #[test]
    fn number_format_very_small_float() -> Result<(), Box<dyn std::error::Error>> {
        // 1e-7 should use exponential notation per ES6 rules
        let val: serde_json::Value = serde_json::from_str("1e-7")?;
        assert_eq!(canon_str(&val)?, "1e-7");
        Ok(())
    }

    #[test]
    fn number_format_large_float_exponent() -> Result<(), Box<dyn std::error::Error>> {
        // Very large numbers use exponential notation
        let val: serde_json::Value = serde_json::from_str("1e21")?;
        assert_eq!(canon_str(&val)?, "1e+21");
        Ok(())
    }

    #[test]
    fn canonicalize_complex_document() -> Result<(), Box<dyn std::error::Error>> {
        let doc = json!({
            "numbers": [333_333_333.333_333_3, 1e30, 4.50, 2e-3, 0.000000000000000000000000001],
            "string": "\u{0019}",
            "bool": true,
            "null": null
        });
        // Just ensure it doesn't error and is valid JSON
        let result = canonicalize(&doc)?;
        let result_str = String::from_utf8(result)?;
        // Re-parse to validate it's valid JSON
        let _: serde_json::Value = serde_json::from_str(&result_str)?;
        Ok(())
    }

    #[test]
    fn canonicalize_control_character_escaping() -> Result<(), Box<dyn std::error::Error>> {
        // Control characters U+0000..U+001F should be escaped
        let s = String::from('\u{0001}');
        let val = json!(s);
        assert_eq!(canon_str(&val)?, r#""\u0001""#);

        let s = String::from('\u{001F}');
        let val = json!(s);
        assert_eq!(canon_str(&val)?, r#""\u001f""#);
        Ok(())
    }

    #[test]
    fn canonicalize_unicode_passthrough() -> Result<(), Box<dyn std::error::Error>> {
        // Non-ASCII characters should pass through without escaping
        let val = json!("Hello, 世界!");
        assert_eq!(canon_str(&val)?, r#""Hello, 世界!""#);
        Ok(())
    }

    #[test]
    fn canonicalize_float_idempotence_regression() -> Result<(), Box<dyn std::error::Error>> {
        // Regression: this f64 value triggered idempotence failure because
        // serde_json's float parser produces a different f64 than Rust's
        // native parser for the ryu-formatted string "91137.58076482713".
        // The fix ensures we round-trip through serde_json's parser.
        let val = json!(91137.58076482713_f64);
        let first = canonicalize(&val)?;
        let first_str = String::from_utf8(first.clone())?;
        let reparsed: serde_json::Value = serde_json::from_str(&first_str)?;
        let second = canonicalize(&reparsed)?;
        assert_eq!(
            first,
            second,
            "idempotency failed: first={first_str}, second={}",
            String::from_utf8_lossy(&second)
        );
        Ok(())
    }
}
