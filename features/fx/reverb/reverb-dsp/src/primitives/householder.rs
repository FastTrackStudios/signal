//! Householder reflection matrix for FDN mixing.
//!
//! `A = I - (2/N) * u * u^T` where `u = [1, 1, ..., 1]^T`
//!
//! This is a unitary (energy-preserving) matrix that cross-couples
//! all delay lines with only 2N-1 additions — no multiplications
//! except the final scaling.

/// Apply Householder reflection in-place.
///
/// For N channels: compute mean, then `out[i] = 2*mean - in[i]`.
/// This is equivalent to `I - (2/N)*ones*ones^T`.
#[inline]
pub fn mix(channels: &mut [f64]) {
    let n = channels.len();
    if n == 0 {
        return;
    }
    let sum: f64 = channels.iter().sum();
    let scale = 2.0 / n as f64;
    for ch in channels.iter_mut() {
        *ch = sum * scale - *ch;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn energy_preserved() {
        let mut ch = [1.0, 0.0, 0.0, 0.0];
        let energy_in: f64 = ch.iter().map(|x| x * x).sum();
        mix(&mut ch);
        let energy_out: f64 = ch.iter().map(|x| x * x).sum();
        assert!(
            (energy_in - energy_out).abs() < 1e-10,
            "Energy should be preserved: {energy_in} vs {energy_out}"
        );
    }

    #[test]
    fn involution() {
        // Householder is its own inverse: H * H = I
        let original = [0.3, -0.7, 0.5, 0.1];
        let mut ch = original;
        mix(&mut ch);
        mix(&mut ch);
        for (a, b) in ch.iter().zip(original.iter()) {
            assert!((a - b).abs() < 1e-10, "H*H should equal I");
        }
    }
}
