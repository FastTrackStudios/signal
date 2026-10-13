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

use dsp_core::num;

/// Maximum number of allpass sections in the cascade.
const MAX_SECTIONS: usize = 300;
/// Maximum stretch factor per section.
const MAX_STRETCH: usize = 8;

/// A single stretched first-order allpass section.
///
/// `H(z) = (a + z^{-k}) / (1 + a·z^{-k})`
struct StretchedAllpass {
    hist: [Delayed; MAX_STRETCH], // Input/output history (circular)
    idx: usize,                   // Write position in the circular buffer
    k: usize,                     // Stretch factor
}

/// One sample of the section's history. `x_buf` and `y_buf`, two
/// parallel `[f64; MAX_STRETCH]` circular buffers sharing one index,
/// before.
#[derive(Clone, Copy, Default)]
struct Delayed {
    /// x[n] — the input at this position.
    x: f64,
    /// y[n] — the output at this position.
    y: f64,
}

impl StretchedAllpass {
    fn new(k: usize) -> Self {
        Self {
            hist: [Delayed::default(); MAX_STRETCH],
            idx: 0,
            k: k.min(MAX_STRETCH),
        }
    }

    #[inline]
    fn tick(&mut self, input: f64, a: f64) -> f64 {
        // Read x[n-k] and y[n-k] from circular buffer
        let read_idx = if self.idx >= self.k {
            self.idx.saturating_sub(self.k)
        } else {
            self.idx.saturating_add(MAX_STRETCH).saturating_sub(self.k)
        };

        // `read_idx` stays inside the buffer by the wrapping above, so
        // the zero fallback is unreachable.
        let Delayed {
            x: x_delayed,
            y: y_delayed,
        } = self.hist.get(read_idx).copied().unwrap_or_default();

        // y[n] = a·x[n] + x[n-k] - a·y[n-k]
        let output = a.mul_add(-y_delayed, a.mul_add(input, x_delayed));

        // Store current input and output
        if let Some(slot) = self.hist.get_mut(self.idx) {
            slot.x = input;
            slot.y = output;
        }

        // Advance circular index
        self.idx = self.idx.saturating_add(1);
        if self.idx >= MAX_STRETCH {
            self.idx = 0;
        }

        output
    }

    fn clear(&mut self) {
        self.hist.fill(Delayed::default());
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
    #[must_use]
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
        for section in self.sections.iter_mut().take(n) {
            x = section.tick(x, a);
        }
        x
    }

    /// Get the approximate group delay at DC in samples.
    #[must_use]
    pub fn group_delay_dc(&self) -> f64 {
        let a = self.coefficient;
        let k = num::count_to_f64(self.sections.first().map_or(1, |s| s.k));
        num::count_to_f64(self.active_sections) * k * (1.0 - a) / (1.0 + a)
    }

    /// Get the approximate group delay at Nyquist in samples.
    #[must_use]
    pub fn group_delay_nyquist(&self) -> f64 {
        let a = self.coefficient;
        let k = num::count_to_f64(self.sections.first().map_or(1, |s| s.k));
        num::count_to_f64(self.active_sections) * k * (1.0 + a) / (1.0 - a)
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
