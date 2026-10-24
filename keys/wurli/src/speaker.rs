/// Wurlitzer 200A speaker cabinet model — Hammerstein nonlinearity + HPF/LPF.
///
/// The 200A uses two 4"x8" oval ceramic-magnet speakers in an open-backed
/// ABS plastic lid. This produces:
///   - Bass rolloff: open-baffle cancellation + speaker resonance (~85-100 Hz)
///   - Treble rolloff: cone breakup (~7-8 kHz)
///   - Cone nonlinearity: Kms hardening (odd harmonics) + BL asymmetry
///     (even harmonics). Modeled as a direct memoryless polynomial — harmonics
///     are phase-coherent with the input signal by construction.
///   - Thermal voice coil compression: slow power-dependent gain reduction (~5s τ)
///
/// Architecture: polynomial waveshaper → tanh excursion limiter → thermal
/// compression → linear filters (HPF + LPF). A generalized Hammerstein-like
/// model. The polynomial is memoryless (no internal filters), so generated
/// harmonics maintain natural phase relationships with the fundamental.
///
/// "Speaker Character" parameter blends from bypass (flat, linear) to authentic
/// (full nonlinearity + HPF + LPF). At character=0.0 all nonlinearity
/// coefficients are zero — pure linear passthrough.
use crate::filters::Biquad;

/// HPF cutoff at fully authentic position.
const HPF_AUTHENTIC_HZ: f64 = 95.0;
/// HPF Q (slightly underdamped for speaker resonance bump).
const HPF_Q: f64 = 0.75;
/// LPF cutoff at fully authentic position.
/// 4x8" ceramic speakers roll off well below 7500 Hz. OBM A/B comparison
/// shows real treble centroids at 0.44-0.48x of f0, consistent with ~5500 Hz.
const LPF_AUTHENTIC_HZ: f64 = 5500.0;
/// LPF Q (Butterworth).
const LPF_Q: f64 = 0.707;
/// HPF cutoff at bypass position (effectively transparent).
const HPF_BYPASS_HZ: f64 = 20.0;
/// LPF cutoff at bypass position (effectively transparent).
const LPF_BYPASS_HZ: f64 = 20000.0;

/// Thermal compression time constant (seconds).
const THERMAL_TAU: f64 = 5.0;

pub struct Speaker {
    hpf: Biquad,
    lpf: Biquad,
    character: f64,
    sample_rate: f64,
    // Polynomial coefficients (scaled by character)
    a2: f64, // x² — BL asymmetry → even harmonics (H2, H4)
    a3: f64, // x³ — Kms hardening → odd harmonics (H3, H5)
    thermal_coeff: f64,
    thermal_alpha: f64,
    thermal_state: f64,
}

impl Speaker {
    pub fn new(sample_rate: f64) -> Self {
        let mut s = Self {
            hpf: Biquad::highpass(HPF_AUTHENTIC_HZ, HPF_Q, sample_rate),
            lpf: Biquad::lowpass(LPF_AUTHENTIC_HZ, LPF_Q, sample_rate),
            character: 1.0,
            sample_rate,
            a2: 0.0,
            a3: 0.0,
            thermal_coeff: 0.0,
            thermal_alpha: 1.0 / (THERMAL_TAU * sample_rate),
            thermal_state: 0.0,
        };
        s.update_coefficients();
        s
    }

    /// Set speaker character: 0.0 = bypass (flat, linear), 1.0 = authentic.
    pub fn set_character(&mut self, character: f64) {
        let c = character.clamp(0.0, 1.0);
        if (c - self.character).abs() > 0.002 {
            self.character = c;
            self.update_coefficients();
        }
    }

    fn update_coefficients(&mut self) {
        let c = self.character;
        // Logarithmic interpolation of cutoff frequencies
        let hpf_hz = HPF_BYPASS_HZ * (HPF_AUTHENTIC_HZ / HPF_BYPASS_HZ).powf(c);
        let lpf_hz = LPF_BYPASS_HZ * (LPF_AUTHENTIC_HZ / LPF_BYPASS_HZ).powf(c);
        self.hpf.set_highpass(hpf_hz, HPF_Q, self.sample_rate);
        self.lpf.set_lowpass(lpf_hz, LPF_Q, self.sample_rate);

        // Polynomial coefficients — all zero at character=0
        self.a2 = 0.2 * c; // BL asymmetry (even harmonics)
        self.a3 = 0.6 * c; // Kms hardening (odd harmonics)
        self.thermal_coeff = 2.0 * c;
    }

    pub fn process(&mut self, input: f64) -> f64 {
        // 1. Static polynomial waveshaper (memoryless — phase-coherent harmonics)
        //    y = (x + a2·x² + a3·x³) / (1 + a2 + a3)
        //    Normalization ensures y(1) = 1 (unity gain at positive full scale).
        //    The even-order term (a2) introduces asymmetry, so y(-1) != -1.
        //    x² → H2, H4 (BL asymmetry, even harmonics)
        //    x³ → H3, H5 (Kms hardening, odd harmonics) + fundamental compression
        let x2 = input * input;
        let x3 = x2 * input;
        let shaped = (input + self.a2 * x2 + self.a3 * x3) / (1.0 + self.a2 + self.a3);

        // 2. Cone excursion limit (Xmax soft stop)
        //    Real speaker cones have physical excursion limits where the spider
        //    and surround stiffen rapidly. tanh models the soft mechanical stop.
        //    Scaled by character: at 0.0 this is a pure passthrough.
        let limited = if self.character < 0.001 {
            shaped
        } else {
            shaped.tanh()
        };

        // 3. Thermal voice coil compression (slow envelope follower)
        let power = x2;
        self.thermal_state += (power - self.thermal_state) * self.thermal_alpha;
        let thermal_gain = 1.0 / (1.0 + self.thermal_coeff * self.thermal_state.sqrt());

        // 4. Linear filters (HPF + LPF)
        let filtered = self.hpf.process(limited * thermal_gain);
        self.lpf.process(filtered)
    }

    pub fn reset(&mut self) {
        self.hpf.reset();
        self.lpf.reset();
        self.thermal_state = 0.0;
    }
}
