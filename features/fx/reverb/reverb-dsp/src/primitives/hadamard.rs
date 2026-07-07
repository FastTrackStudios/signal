//! Hadamard mixing matrix for FDN.
//!
//! Recursive in-place Hadamard transform using only additions
//! and subtractions (no multiplications). Energy-preserving when
//! normalized by `1/sqrt(N)`.
//!
//! Supports N = power of 2 (2, 4, 8, 16).

/// In-place Hadamard transform with normalization.
///
/// `channels.len()` must be a power of 2.
#[inline]
pub fn mix(channels: &mut [f64]) {
    let n = channels.len();
    debug_assert!(n.is_power_of_two(), "Hadamard requires power-of-2 size");

    // Recursive butterfly
    let mut half = n;
    while half > 1 {
        half >>= 1;
        for i in (0..n).step_by(half * 2) {
            for j in i..i + half {
                let a = channels[j];
                let b = channels[j + half];
                channels[j] = a + b;
                channels[j + half] = a - b;
            }
        }
    }

    // Normalize to preserve energy
    let scale = 1.0 / (n as f64).sqrt();
    for ch in channels.iter_mut() {
        *ch *= scale;
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
            "Energy: {energy_in} vs {energy_out}"
        );
    }

    #[test]
    fn involution() {
        let original = [0.3, -0.7, 0.5, 0.1];
        let mut ch = original;
        mix(&mut ch);
        mix(&mut ch);
        for (a, b) in ch.iter().zip(original.iter()) {
            assert!((a - b).abs() < 1e-10, "H*H should equal I");
        }
    }

    #[test]
    fn size_8() {
        let mut ch = [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let energy_in: f64 = ch.iter().map(|x| x * x).sum();
        mix(&mut ch);
        let energy_out: f64 = ch.iter().map(|x| x * x).sum();
        assert!((energy_in - energy_out).abs() < 1e-10);
    }
}
