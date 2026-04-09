//! Minimal trial decoders for encoding detection.
//!
//! These are NOT full-featured decoders. They exist only to verify
//! decodability and extract bytes for entropy analysis. Capped at
//! 4096 decoded bytes to bound memory.

const DECODE_CAP: usize = 4096;

/// Base64 decode table (standard alphabet).
static B64_TABLE: [i8; 256] = {
    let mut table = [-1i8; 256];
    let mut i = 0u8;
    while i < 26 {
        table[(b'A' + i) as usize] = i as i8;
        table[(b'a' + i) as usize] = (i + 26) as i8;
        i += 1;
    }
    let mut d = 0u8;
    while d < 10 {
        table[(b'0' + d) as usize] = (d + 52) as i8;
        d += 1;
    }
    table[b'+' as usize] = 62;
    table[b'/' as usize] = 63;
    table[b'-' as usize] = 62; // URL-safe
    table[b'_' as usize] = 63; // URL-safe
    table
};

/// Try to decode base64 (standard or URL-safe). Returns decoded bytes up to DECODE_CAP.
/// Returns `None` if the input is not valid base64.
pub fn try_base64_decode(input: &str) -> Option<Vec<u8>> {
    let trimmed = input.trim_end_matches('=');
    if trimmed.is_empty() {
        return None;
    }

    let mut output = Vec::with_capacity(trimmed.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;

    for &b in trimmed.as_bytes() {
        let val = B64_TABLE[b as usize];
        if val < 0 {
            return None; // invalid character
        }
        buf = (buf << 6) | (val as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
            if output.len() >= DECODE_CAP {
                return Some(output);
            }
        }
    }

    Some(output)
}

/// Try to percent-decode a string. Returns decoded bytes up to DECODE_CAP.
/// Returns `None` if it contains no percent sequences.
pub fn try_percent_decode(input: &str) -> Option<Vec<u8>> {
    if !input.contains('%') {
        return None;
    }

    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = hex_digit(bytes[i + 1])?;
            let lo = hex_digit(bytes[i + 2])?;
            output.push((hi << 4) | lo);
            i += 3;
        } else {
            output.push(bytes[i]);
            i += 1;
        }
        if output.len() >= DECODE_CAP {
            return Some(output);
        }
    }

    Some(output)
}

/// Try to hex-decode a string. Returns decoded bytes up to DECODE_CAP.
/// Returns `None` if the input is not valid even-length hex.
pub fn try_hex_decode(input: &str) -> Option<Vec<u8>> {
    let bytes = input.as_bytes();
    if bytes.len() % 2 != 0 || bytes.is_empty() {
        return None;
    }

    let mut output = Vec::with_capacity(bytes.len() / 2);
    let mut i = 0;

    while i + 1 < bytes.len() {
        let hi = hex_digit(bytes[i])?;
        let lo = hex_digit(bytes[i + 1])?;
        output.push((hi << 4) | lo);
        i += 2;
        if output.len() >= DECODE_CAP {
            return Some(output);
        }
    }

    Some(output)
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_hello_world() {
        // "Hello World" = "SGVsbG8gV29ybGQ="
        let decoded = try_base64_decode("SGVsbG8gV29ybGQ=");
        assert_eq!(decoded.as_deref(), Some(b"Hello World".as_slice()));
    }

    #[test]
    fn base64_invalid() {
        assert!(try_base64_decode("!!!").is_none());
        assert!(try_base64_decode("").is_none());
    }

    #[test]
    fn base64_url_safe() {
        // URL-safe variant uses - and _
        let result = try_base64_decode("SGVsbG8-V29ybGQ_");
        assert!(result.is_some());
    }

    #[test]
    fn percent_decode_basic() {
        let decoded = try_percent_decode("hello%20world%21");
        assert_eq!(decoded.as_deref(), Some(b"hello world!".as_slice()));
    }

    #[test]
    fn percent_decode_no_encoding() {
        assert!(try_percent_decode("hello world").is_none());
    }

    #[test]
    fn hex_decode_basic() {
        let decoded = try_hex_decode("48656c6c6f");
        assert_eq!(decoded.as_deref(), Some(b"Hello".as_slice()));
    }

    #[test]
    fn hex_decode_odd_length() {
        assert!(try_hex_decode("48656c6c6").is_none());
    }

    #[test]
    fn hex_decode_invalid_chars() {
        assert!(try_hex_decode("gggg").is_none());
    }

    #[test]
    fn decode_cap_respected() {
        // Large base64 should cap at DECODE_CAP bytes
        let large = "A".repeat(8192); // valid base64 alphabet
        let result = try_base64_decode(&large);
        if let Some(decoded) = result {
            assert!(decoded.len() <= DECODE_CAP);
        }
    }
}
