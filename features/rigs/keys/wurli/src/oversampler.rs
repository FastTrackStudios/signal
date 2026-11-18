//! 2x polyphase IIR half-band oversampler.
//!
//! Uses cascaded allpass sections in a polyphase decomposition for efficient
//! half-band filtering. This provides the antialiasing needed for the preamp's
//! nonlinear processing (BJT soft-clip generates harmonics that must not alias).
//!
//! Design: Regalia-Mitra allpass-based half-band IIR.
//! ~28 dB rejection at 30 kHz with 3 coefficients per branch (6 allpass sections).

/// Half-band IIR allpass coefficients (~28 dB rejection at 30 kHz).
///
/// These come from published tables for elliptic half-band IIR filters
/// decomposed into two parallel allpass branches. Each branch is a cascade
/// of first-order allpass sections: y = (a + z^-1) / (1 + a*z^-1).
///
/// Transition band: ~0.1*fs (fairly sharp for 2x oversampling).
const BRANCH_A_COEFFS: &[f64] = &[
    0.036_681_502_163_648,
    0.248_030_921_580_110,
    0.643_184_620_136_480,
];

const BRANCH_B_COEFFS: &[f64] = &[
    0.110_377_634_768_680,
    0.420_399_304_190_880,
    0.854_640_112_701_920,
];

/// First-order allpass section: y = (a + z^-1) / (1 + a*z^-1)
#[derive(Clone)]
struct AllpassSection {
    a: f64,
    state: f64,
}

impl AllpassSection {
    fn new(a: f64) -> Self {
        Self { a, state: 0.0 }
    }

    fn process(&mut self, x: f64) -> f64 {
        let y = self.a * x + self.state;
        self.state = x - self.a * y;
        y
    }

    fn reset(&mut self) {
        self.state = 0.0;
    }
}

/// Allpass branch: cascade of first-order allpass sections.
#[derive(Clone)]
struct AllpassBranch {
    sections: Vec<AllpassSection>,
}

impl AllpassBranch {
    fn new(coeffs: &[f64]) -> Self {
        Self {
            sections: coeffs.iter().map(|&a| AllpassSection::new(a)).collect(),
        }
    }

    fn process(&mut self, x: f64) -> f64 {
        let mut y = x;
        for section in &mut self.sections {
            y = section.process(y);
        }
        y
    }

    fn reset(&mut self) {
        for section in &mut self.sections {
            section.reset();
        }
    }
}

/// 2x polyphase IIR half-band oversampler.
pub struct Oversampler {
    /// Branch A (processes even samples)
    up_branch_a: AllpassBranch,
    /// Branch B (processes odd samples)
    up_branch_b: AllpassBranch,
    /// Branch A for downsampling
    down_branch_a: AllpassBranch,
    /// Branch B for downsampling
    down_branch_b: AllpassBranch,
    /// One-sample delay for the B branch in downsampling
    down_delay: f64,
}

impl Oversampler {
    pub fn new() -> Self {
        Self {
            up_branch_a: AllpassBranch::new(BRANCH_A_COEFFS),
            up_branch_b: AllpassBranch::new(BRANCH_B_COEFFS),
            down_branch_a: AllpassBranch::new(BRANCH_A_COEFFS),
            down_branch_b: AllpassBranch::new(BRANCH_B_COEFFS),
            down_delay: 0.0,
        }
    }

    /// Upsample by 2x: insert zeros between samples, filter.
    /// Input: N samples at base rate.
    /// Output: 2N samples at 2x rate (written into provided buffer).
    pub fn upsample_2x(&mut self, input: &[f64], output: &mut [f64]) {
        debug_assert!(output.len() >= input.len() * 2);

        for (i, &x) in input.iter().enumerate() {
            // Polyphase decomposition: feed x to both branches,
            // interleave outputs.
            let a = self.up_branch_a.process(x);
            let b = self.up_branch_b.process(x);

            // Branch A produces even samples, Branch B produces odd samples.
            output[i * 2] = a;
            output[i * 2 + 1] = b;
        }
    }

    /// Downsample by 2x: filter, decimate.
    /// Input: 2N samples at 2x rate.
    /// Output: N samples at base rate (written into provided buffer).
    pub fn downsample_2x(&mut self, input: &[f64], output: &mut [f64]) {
        debug_assert!(input.len() >= output.len() * 2);

        for (i, out) in output.iter_mut().enumerate() {
            // Feed even sample to branch A, odd sample to branch B
            let a = self.down_branch_a.process(input[i * 2]);
            let b = self.down_branch_b.process(input[i * 2 + 1]);

            // Average the two branches (half-band filter property)
            // B branch needs one sample delay for phase alignment
            *out = (a + self.down_delay) * 0.5;
            self.down_delay = b;
        }
    }

    pub fn reset(&mut self) {
        self.up_branch_a.reset();
        self.up_branch_b.reset();
        self.down_branch_a.reset();
        self.down_branch_b.reset();
        self.down_delay = 0.0;
    }
}

impl Default for Oversampler {
    fn default() -> Self {
        Self::new()
    }
}
