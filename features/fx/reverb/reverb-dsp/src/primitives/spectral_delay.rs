//! Spectral delay filter — cascade of stretched first-order allpass filters.
//!
//! Based on Välimäki, Parker, Abel — "Parametric Spring Reverberation Effect"
//! (JAES, 2010) and Parker — "Efficient Dispersion Generation Structures for
//! Spring Reverb Emulation" (EURASIP, 2011).
//!
//! The spectral delay filter creates frequency-dependent group delay, which
//! is the key to modeling the dispersive chirp of helical spring reverbs.
//! Higher frequencies are delayed more than lower frequencies, producing
//! the characteristic descending "drip" sound.
//!
//! # Stretched allpass
//!
//! Each section implements: `H(z) = (a + z^{-k}) / (1 + a·z^{-k})`
//!
//! In the time domain: `y[n] = a·x[n] + x[n-k] - a·y[n-k]`
//!
//! The stretch factor `k` reduces the number of sections needed by a factor
//! of `k` compared to unit-delay allpasses, at the cost of slightly different
//! dispersion characteristics. Parker (2011) shows k=2–8 works well.
//!
//! # Group delay
//!
//! For a cascade of N first-order allpasses with coefficient `a`:
//!   - Group delay at DC ≈ N·(1-a)/(1+a) samples
//!   - Group delay at Nyquist ≈ N·(1+a)/(1-a) samples
//!   - Ratio Nyquist/DC ≈ ((1+a)/(1-a))^2
//!
//! For `a = 0.6`: DC delay ≈ 0.25·N samples, Nyquist delay ≈ 4·N samples.
//! The frequency-dependent spread creates the chirp.

/// Maximum number of allpass sections in the cascade.
const MAX_SECTIONS: usize = 300;
/// Maximum stretch factor per section.
const MAX_STRETCH: usize = 8;

/// A single stretched first-order allpass section.
///
/// `H(z) = (a + z^{-k}) / (1 + a·z^{-k})`
struct StretchedAllpass {
    x_buf: [f64; MAX_STRETCH], // Input delay buffer (circular)
    y_buf: [f64; MAX_STRETCH], // Output delay buffer (circular)
    idx: usize,                // Write position in circular buffers
    k: usize,                  // Stretch factor
}

impl StretchedAllpass {
    fn new(k: usize) -> Self {
        Self {
            x_buf: [0.0; MAX_STRETCH],
            y_buf: [0.0; MAX_STRETCH],
            idx: 0,
            k: k.min(MAX_STRETCH),
        }
    }

    #[inline]
    fn tick(&mut self, input: f64, a: f64) -> f64 {
        // Read x[n-k] and y[n-k] from circular buffer
        let read_idx = if self.idx >= self.k {
            self.idx - self.k
        } else {
            self.idx + MAX_STRETCH - self.k
        };

        let x_delayed = self.x_buf[read_idx];
        let y_delayed = self.y_buf[read_idx];

        // y[n] = a·x[n] + x[n-k] - a·y[n-k]
        let output = a * input + x_delayed - a * y_delayed;

        // Store current input and output
        self.x_buf[self.idx] = input;
        self.y_buf[self.idx] = output;

        // Advance circular index
        self.idx += 1;
        if self.idx >= MAX_STRETCH {
            self.idx = 0;
        }

        output
    }

    fn clear(&mut self) {
        self.x_buf.fill(0.0);
        self.y_buf.fill(0.0);
    }
}

/// Cascade of stretched first-order allpass filters for spectral dispersion.
///
/// This creates frequency-dependent group delay that models the chirp
/// characteristic of helical spring reverbs.
pub struct SpectralDelay {
    sections: Vec<StretchedAllpass>,
    /// Allpass coefficient (0.0–1.0). Controls chirp shape.
    /// - ~0.5: moderate chirp (typical spring)
    /// - ~0.65: strong chirp (long spring, more "drippy")
    /// - ~0.3: mild chirp (short spring)
    pub coefficient: f64,
    /// Number of active sections in the cascade.
    pub active_sections: usize,
}

impl SpectralDelay {
    /// Create a new spectral delay filter.
    ///
    /// - `num_sections`: Number of allpass sections (80–300 typical)
    /// - `stretch`: Stretch factor k (1–8, higher = fewer sections needed)
    /// - `coefficient`: Allpass coefficient a (0.3–0.7 typical)
    pub fn new(num_sections: usize, stretch: usize, coefficient: f64) -> Self {
        let n = num_sections.min(MAX_SECTIONS);
        let k = stretch.clamp(1, MAX_STRETCH);
        let sections = (0..n).map(|_| StretchedAllpass::new(k)).collect();

        Self {
            sections,
            coefficient,
            active_sections: n,
        }
    }

    /// Process one sample through the allpass cascade.
    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        let a = self.coefficient;
        let n = self.active_sections.min(self.sections.len());
        let mut x = input;
        for i in 0..n {
            x = self.sections[i].tick(x, a);
        }
        x
    }

    /// Get the approximate group delay at DC in samples.
    pub fn group_delay_dc(&self) -> f64 {
        let a = self.coefficient;
        let k = self.sections.first().map(|s| s.k).unwrap_or(1) as f64;
        self.active_sections as f64 * k * (1.0 - a) / (1.0 + a)
    }

    /// Get the approximate group delay at Nyquist in samples.
    pub fn group_delay_nyquist(&self) -> f64 {
        let a = self.coefficient;
        let k = self.sections.first().map(|s| s.k).unwrap_or(1) as f64;
        self.active_sections as f64 * k * (1.0 + a) / (1.0 - a)
    }

    pub fn clear(&mut self) {
        for s in &mut self.sections {
            s.clear();
        }
    }

    pub fn reset(&mut self) {
        self.clear();
    }
}
