//! Byte-level Shannon entropy computation.
//!
//! Local helper — intentionally not reusing vajra-stats (different computation
//! shape: byte distribution of a single string, not value distribution across paths).

/// Compute Shannon entropy over the byte distribution of a byte slice.
/// Returns bits (0.0 for constant input, up to 8.0 for uniformly distributed bytes).
pub fn byte_entropy(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }

    let mut counts = [0u64; 256];
    for &b in data {
        counts[b as usize] += 1;
    }

    let len = data.len() as f64;
    let mut entropy = 0.0;
    for &count in &counts {
        if count > 0 {
            let p = count as f64 / len;
            entropy -= p * p.log2();
        }
    }
    entropy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_has_zero_entropy() {
        assert!((byte_entropy(b"") - 0.0).abs() < 1e-10);
    }

    #[test]
    fn constant_has_zero_entropy() {
        assert!((byte_entropy(b"aaaaaaaaaa") - 0.0).abs() < 1e-10);
    }

    #[test]
    fn two_equally_likely_bytes_has_one_bit() {
        let data: Vec<u8> = (0..1000)
            .map(|i| if i % 2 == 0 { b'a' } else { b'b' })
            .collect();
        assert!((byte_entropy(&data) - 1.0).abs() < 0.01);
    }

    #[test]
    fn high_entropy_random_looking() {
        // All 256 byte values equally represented
        let data: Vec<u8> = (0..256)
            .flat_map(|b| std::iter::repeat(b as u8).take(10))
            .collect();
        assert!(byte_entropy(&data) > 7.9);
    }
}
