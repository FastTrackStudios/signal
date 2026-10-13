//! A small in-place radix-2 complex FFT (no dependency). Plans allocate
//! their tables once; [`Fft::forward`] never allocates.

use std::f64::consts::PI;

/// A planned FFT of one power-of-two size.
pub struct Fft {
    n: usize,
    /// `e^{-2πik/n}` for `k < n/2`.
    twiddles: Box<[(f64, f64)]>,
    /// Bit-reversed index of each position.
    rev: Box<[u32]>,
}

impl Fft {
    /// # Panics
    /// When `n` is not a power of two ≥ 2.
    #[must_use]
    pub fn new(n: usize) -> Self {
        assert!(
            n >= 2 && n.is_power_of_two(),
            "FFT size must be a power of two ≥ 2, got {n}"
        );
        let bits = n.trailing_zeros();
        let twiddles = (0..n / 2)
            .map(|k| {
                let a = -2.0 * PI * k as f64 / n as f64;
                (a.cos(), a.sin())
            })
            .collect();
        let rev = (0..n as u32)
            .map(|i| i.reverse_bits() >> (32 - bits))
            .collect();
        Self { n, twiddles, rev }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.n
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        false
    }

    /// Forward transform of `re + i·im` in place (unnormalized).
    ///
    /// # Panics
    /// When the buffers aren't the planned length.
    pub fn forward(&self, re: &mut [f64], im: &mut [f64]) {
        let n = self.n;
        assert!(
            re.len() == n && im.len() == n,
            "FFT buffers must be {n} long"
        );
        for i in 0..n {
            let j = self.rev[i] as usize;
            if i < j {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let mut size = 2;
        while size <= n {
            let half = size / 2;
            let step = n / size;
            for start in (0..n).step_by(size) {
                for k in 0..half {
                    let (wr, wi) = self.twiddles[k * step];
                    let (a, b) = (start + k, start + k + half);
                    let (xr, xi) = (re[b] * wr - im[b] * wi, re[b] * wi + im[b] * wr);
                    re[b] = re[a] - xr;
                    im[b] = im[a] - xi;
                    re[a] += xr;
                    im[a] += xi;
                }
            }
            size *= 2;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_a_naive_dft() {
        let n = 16;
        let x: Vec<f64> = (0..n)
            .map(|i| ((i * 7 % 5) as f64 - 2.0) * 0.3 + (i as f64 * 0.4).sin())
            .collect();
        let (mut re, mut im) = (x.clone(), vec![0.0; n]);
        Fft::new(n).forward(&mut re, &mut im);
        for k in 0..n {
            let (mut dr, mut di) = (0.0, 0.0);
            for (t, v) in x.iter().enumerate() {
                let a = -2.0 * PI * (k * t) as f64 / n as f64;
                dr += v * a.cos();
                di += v * a.sin();
            }
            assert!(
                (re[k] - dr).abs() < 1e-9 && (im[k] - di).abs() < 1e-9,
                "bin {k}"
            );
        }
    }
}
