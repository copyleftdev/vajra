//! Jensen-Shannon Divergence and related information-theoretic metrics.
//!
//! All computations use log base 2 so the JSD is bounded in \[0, 1\].

use std::fmt;

/// Errors that can occur during divergence computation.
#[derive(Debug, Clone, PartialEq)]
pub enum JsdError {
    /// The two distributions have different lengths.
    LengthMismatch { p_len: usize, q_len: usize },
}

impl fmt::Display for JsdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LengthMismatch { p_len, q_len } => {
                write!(
                    f,
                    "distribution length mismatch: p has {p_len} elements, q has {q_len}"
                )
            }
        }
    }
}

impl std::error::Error for JsdError {}

/// Compute the Kullback-Leibler divergence KL(P || Q).
///
/// If `p[i] == 0`, that term contributes 0 (by convention 0 * log(0/q) = 0).
/// If `q[i] == 0` and `p[i] > 0`, the KL divergence is infinite;
/// however, in our JSD usage this cannot happen because M = 0.5*(P+Q)
/// is never zero when P or Q is nonzero.
fn kl_divergence(p: &[f64], q: &[f64]) -> f64 {
    let mut sum = 0.0;
    for (i, &pi) in p.iter().enumerate() {
        if pi > 0.0 {
            // Safety: in JSD context, q[i] > 0 when pi > 0
            // because m[i] = 0.5*(pi + qi) >= 0.5*pi > 0.
            if let Some(&qi) = q.get(i) {
                if qi > 0.0 {
                    sum += pi * (pi / qi).log2();
                }
                // If qi == 0 and pi > 0, this is technically infinite.
                // We skip it to avoid NaN propagation in non-JSD callers.
            }
        }
    }
    sum
}

/// Compute the Jensen-Shannon Divergence between two probability distributions.
///
/// JSD(P, Q) = 0.5 * KL(P || M) + 0.5 * KL(Q || M)
/// where M = 0.5 * (P + Q).
///
/// Uses log base 2, so the result is bounded in \[0, 1\].
///
/// # Errors
///
/// Returns [`JsdError::LengthMismatch`] if `p` and `q` have different lengths.
///
/// # Edge cases
///
/// - Empty distributions (both length 0) return 0.0.
pub fn jensen_shannon_divergence(p: &[f64], q: &[f64]) -> Result<f64, JsdError> {
    if p.len() != q.len() {
        return Err(JsdError::LengthMismatch {
            p_len: p.len(),
            q_len: q.len(),
        });
    }

    if p.is_empty() {
        return Ok(0.0);
    }

    // Compute M = 0.5 * (P + Q)
    let m: Vec<f64> = p.iter().zip(q.iter()).map(|(&pi, &qi)| 0.5 * (pi + qi)).collect();

    let kl_pm = kl_divergence(p, &m);
    let kl_qm = kl_divergence(q, &m);

    let jsd = 0.5 * kl_pm + 0.5 * kl_qm;

    // Clamp to [0, 1] to handle floating-point rounding.
    Ok(jsd.clamp(0.0, 1.0))
}

/// Compute the JSD metric (square root of JSD).
///
/// sqrt(JSD) is a proper distance metric satisfying the triangle inequality
/// (Endres & Schindelin 2003).
///
/// # Errors
///
/// Returns [`JsdError::LengthMismatch`] if `p` and `q` have different lengths.
pub fn jsd_metric(p: &[f64], q: &[f64]) -> Result<f64, JsdError> {
    let jsd = jensen_shannon_divergence(p, q)?;
    Ok(jsd.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_distributions_zero() {
        let p = [0.25, 0.25, 0.25, 0.25];
        let q = [0.25, 0.25, 0.25, 0.25];
        let jsd = jensen_shannon_divergence(&p, &q).unwrap_or(f64::NAN);
        assert!((jsd - 0.0).abs() < 1e-12, "JSD of identical distributions should be 0, got {jsd}");
    }

    #[test]
    fn completely_different_distributions_near_one() {
        // Two point distributions on different categories: JSD = 1.0
        let p = [1.0, 0.0];
        let q = [0.0, 1.0];
        let jsd = jensen_shannon_divergence(&p, &q).unwrap_or(f64::NAN);
        assert!(
            (jsd - 1.0).abs() < 1e-12,
            "JSD of completely different distributions should be 1.0, got {jsd}"
        );
    }

    #[test]
    fn symmetric() {
        let p = [0.6, 0.3, 0.1];
        let q = [0.1, 0.2, 0.7];
        let jsd_pq = jensen_shannon_divergence(&p, &q).unwrap_or(f64::NAN);
        let jsd_qp = jensen_shannon_divergence(&q, &p).unwrap_or(f64::NAN);
        assert!(
            (jsd_pq - jsd_qp).abs() < 1e-12,
            "JSD should be symmetric: JSD(P,Q)={jsd_pq} != JSD(Q,P)={jsd_qp}"
        );
    }

    #[test]
    fn bounded_zero_one() {
        let p = [0.9, 0.05, 0.05];
        let q = [0.05, 0.05, 0.9];
        let jsd = jensen_shannon_divergence(&p, &q).unwrap_or(f64::NAN);
        assert!(jsd >= 0.0, "JSD should be >= 0, got {jsd}");
        assert!(jsd <= 1.0, "JSD should be <= 1, got {jsd}");
    }

    #[test]
    fn metric_triangle_inequality() {
        // Triangle inequality: d(P,R) <= d(P,Q) + d(Q,R)
        let p = [0.5, 0.3, 0.2];
        let q = [0.3, 0.4, 0.3];
        let r = [0.1, 0.1, 0.8];

        let d_pr = jsd_metric(&p, &r).unwrap_or(f64::NAN);
        let d_pq = jsd_metric(&p, &q).unwrap_or(f64::NAN);
        let d_qr = jsd_metric(&q, &r).unwrap_or(f64::NAN);

        assert!(
            d_pr <= d_pq + d_qr + 1e-12,
            "Triangle inequality violated: d(P,R)={d_pr} > d(P,Q)+d(Q,R)={:.12}",
            d_pq + d_qr
        );
    }

    #[test]
    fn empty_distributions_return_zero() {
        let p: [f64; 0] = [];
        let q: [f64; 0] = [];
        let jsd = jensen_shannon_divergence(&p, &q).unwrap_or(f64::NAN);
        assert!((jsd - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn length_mismatch_error() {
        let p = [0.5, 0.5];
        let q = [1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0];
        let result = jensen_shannon_divergence(&p, &q);
        assert!(result.is_err());
        match result {
            Err(JsdError::LengthMismatch { p_len, q_len }) => {
                assert_eq!(p_len, 2);
                assert_eq!(q_len, 3);
            }
            _ => panic!("expected LengthMismatch error"),
        }
    }

    #[test]
    fn jsd_metric_identity() {
        let p = [0.25, 0.25, 0.25, 0.25];
        let d = jsd_metric(&p, &p).unwrap_or(f64::NAN);
        assert!((d - 0.0).abs() < 1e-12, "metric of identical distributions should be 0");
    }

    #[test]
    fn jsd_known_value() {
        // For two binary distributions [p, 1-p] and [q, 1-q], JSD has a known form.
        // P = [0.5, 0.5], Q = [1.0, 0.0]: JSD = 0.5 * log2(2) = 0.5... let's compute:
        // M = [0.75, 0.25]
        // KL(P||M) = 0.5*log2(0.5/0.75) + 0.5*log2(0.5/0.25) = 0.5*log2(2/3) + 0.5*log2(2)
        //          = 0.5*(-0.5849...) + 0.5*(1.0) = -0.2924... + 0.5 = 0.2075...
        // KL(Q||M) = 1.0*log2(1.0/0.75) = log2(4/3) = 0.4150...
        // JSD = 0.5*0.2075 + 0.5*0.4150 = 0.3112...
        let p = [0.5, 0.5];
        let q = [1.0, 0.0];
        let jsd = jensen_shannon_divergence(&p, &q).unwrap_or(f64::NAN);
        // Expected approximately 0.3112
        assert!(jsd > 0.31, "expected JSD ~0.31, got {jsd}");
        assert!(jsd < 0.32, "expected JSD ~0.31, got {jsd}");
    }
}
