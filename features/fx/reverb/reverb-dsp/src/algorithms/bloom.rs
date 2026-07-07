//! Bloom reverb — Greyhole-inspired diffuser + modulated feedback delay network.
//!
//! Architecture: Input → allpass diffuser cascade → split to 4 modulated delay lines
//! per channel (8 total). Each delay line has its own LP damping filter and allpass
//! diffuser in the feedback path, creating progressive density buildup — the "bloom"
//! effect. Cross-coupling between L/R channels provides stereo width.
//!
//! Key characteristics:
//! - Progressive density: starts sparse, builds to full density over time
//! - Modulated delays prevent metallic ringing
//! - Diffusers in feedback paths mean each echo pass gets progressively smeared
//! - Longer feedback = more bloom time

use crate::algorithm::{AlgorithmParams, ReverbAlgorithm};
use crate::primitives::allpass_diffuser::AllpassDiffuser;
use crate::primitives::one_pole::Lp1;
use audiocore_dsp::delay_line::DelayLine;

/// Number of delay lines per channel.
const NUM_LINES: usize = 4;

/// Base delay lengths in samples at 48kHz (prime numbers for maximal density).
/// These are the core delay times that define the spacing between bloom echoes.
const BASE_DELAYS_L: [usize; NUM_LINES] = [1453, 2311, 3571, 4909];
const BASE_DELAYS_R: [usize; NUM_LINES] = [1607, 2539, 3797, 5101];

/// DC blocker: y[n] = x[n] - x[n-1] + R * y[n-1], R close to 1.
struct DcBlocker {
    x1: f64,
    y1: f64,
    r: f64,
}

impl DcBlocker {
    fn new() -> Self {
        Self {
            x1: 0.0,
            y1: 0.0,
            r: 0.9995,
        }
    }

    #[inline]
    fn tick(&mut self, x: f64) -> f64 {
        self.y1 = x - self.x1 + self.r * self.y1;
        self.x1 = x;
        self.y1
    }

    fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }
}

/// A single feedback delay voice with its own diffuser, damping, and modulation.
struct BloomVoice {
    delay: DelayLine,
    diffuser: AllpassDiffuser,
    damping: Lp1,
    dc_block: DcBlocker,
    current_delay: f64,
}

impl BloomVoice {
    fn new(base_delay_48k: usize, sample_rate: f64, diffuser_seed: u64) -> Self {
        let scale = sample_rate / 48000.0;
        let base_delay = (base_delay_48k as f64 * scale) as usize;
        let max_delay = base_delay * 3 + 256; // headroom for size scaling

        let mut diffuser = AllpassDiffuser::with_defaults(sample_rate, 0.6);
        diffuser.set_seed(diffuser_seed);
        diffuser.set_active_stages(4);
        diffuser.set_feedback(0.4);
        diffuser.set_modulation(0.5, 4.0, sample_rate);

        let mut damping = Lp1::new();
        damping.set_freq(8000.0, sample_rate);

        Self {
            delay: DelayLine::new(max_delay + 1),
            diffuser,
            damping,
            dc_block: DcBlocker::new(),
            current_delay: base_delay as f64,
        }
    }

    fn reset(&mut self) {
        self.delay.clear();
        self.diffuser.reset();
        self.damping.reset();
        self.dc_block.reset();
    }
}

/// Greyhole-inspired bloom reverb with progressive density buildup.
pub struct Bloom {
    /// Input diffuser cascade (pre-diffusion before delay network)
    input_diffuser_l: AllpassDiffuser,
    input_diffuser_r: AllpassDiffuser,

    /// 4 feedback delay voices per channel
    voices_l: Vec<BloomVoice>,
    voices_r: Vec<BloomVoice>,

    /// Feedback state per voice (stored between ticks for cross-coupling)
    fb_l: [f64; NUM_LINES],
    fb_r: [f64; NUM_LINES],

    /// Output tone filter
    tone_lp_l: Lp1,
    tone_lp_r: Lp1,

    /// Parameters (cached for per-sample use)
    decay_gain: f64,
    size_scale: f64,
    stereo_width: f64,

    sample_rate: f64,
}

impl Bloom {
    pub fn new(sample_rate: f64) -> Self {
        // Input diffusers with different seeds for L/R decorrelation
        let mut input_diffuser_l = AllpassDiffuser::with_defaults(sample_rate, 1.0);
        input_diffuser_l.set_seed(23456);
        input_diffuser_l.set_active_stages(8);
        input_diffuser_l.set_feedback(0.5);
        input_diffuser_l.set_modulation(0.6, 6.0, sample_rate);

        let mut input_diffuser_r = AllpassDiffuser::with_defaults(sample_rate, 1.0);
        input_diffuser_r.set_seed(34567);
        input_diffuser_r.set_active_stages(8);
        input_diffuser_r.set_feedback(0.5);
        input_diffuser_r.set_modulation(0.6, 6.0, sample_rate);

        // Create delay voices with unique diffuser seeds
        let voices_l: Vec<BloomVoice> = (0..NUM_LINES)
            .map(|i| BloomVoice::new(BASE_DELAYS_L[i], sample_rate, 45678 + i as u64 * 111))
            .collect();

        let voices_r: Vec<BloomVoice> = (0..NUM_LINES)
            .map(|i| BloomVoice::new(BASE_DELAYS_R[i], sample_rate, 56789 + i as u64 * 111))
            .collect();

        let mut tone_lp_l = Lp1::new();
        let mut tone_lp_r = Lp1::new();
        tone_lp_l.set_freq(12000.0, sample_rate);
        tone_lp_r.set_freq(12000.0, sample_rate);

        Self {
            input_diffuser_l,
            input_diffuser_r,
            voices_l,
            voices_r,
            fb_l: [0.0; NUM_LINES],
            fb_r: [0.0; NUM_LINES],
            tone_lp_l,
            tone_lp_r,
            decay_gain: 0.7,
            size_scale: 1.0,
            stereo_width: 0.5,
            sample_rate,
        }
    }

    /// Mix matrix for cross-coupling between delay lines within a channel.
    /// Rotates outputs to create density: each line feeds partially into the next.
    #[inline]
    fn rotate_mix(vals: &[f64; NUM_LINES], amount: f64) -> [f64; NUM_LINES] {
        let direct = 1.0 - amount * 0.5;
        let cross = amount * 0.5 / (NUM_LINES - 1) as f64;
        let mut out = [0.0; NUM_LINES];
        for i in 0..NUM_LINES {
            out[i] = vals[i] * direct;
            for j in 0..NUM_LINES {
                if j != i {
                    out[i] += vals[j] * cross;
                }
            }
        }
        out
    }
}

impl ReverbAlgorithm for Bloom {
    fn reset(&mut self) {
        self.input_diffuser_l.reset();
        self.input_diffuser_r.reset();
        for v in &mut self.voices_l {
            v.reset();
        }
        for v in &mut self.voices_r {
            v.reset();
        }
        self.fb_l = [0.0; NUM_LINES];
        self.fb_r = [0.0; NUM_LINES];
        self.tone_lp_l.reset();
        self.tone_lp_r.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        *self = Self::new(sample_rate);
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        let sr = self.sample_rate;

        // -- Decay: feedback gain of delay lines (bloom sustain) --
        // Map 0..1 to ~0.3..0.97 for useful reverb range, with a long tail at high values
        self.decay_gain = 0.3 + params.decay * 0.67;

        // -- Size: scale delay line lengths --
        // Map 0..1 to 0.3..2.5 for small-to-massive space
        self.size_scale = 0.3 + params.size * 2.2;
        for (i, voice) in self.voices_l.iter_mut().enumerate() {
            let scaled = (BASE_DELAYS_L[i] as f64 * (sr / 48000.0) * self.size_scale) as usize;
            voice.current_delay = scaled as f64;
        }
        for (i, voice) in self.voices_r.iter_mut().enumerate() {
            let scaled = (BASE_DELAYS_R[i] as f64 * (sr / 48000.0) * self.size_scale) as usize;
            voice.current_delay = scaled as f64;
        }

        // -- Diffusion: input diffuser stages and feedback (bloom density) --
        let input_stages = 2 + (params.diffusion * 6.0) as usize; // 2..8 stages
        let input_fb = 0.3 + params.diffusion * 0.45; // 0.3..0.75
        self.input_diffuser_l.set_active_stages(input_stages);
        self.input_diffuser_r.set_active_stages(input_stages);
        self.input_diffuser_l.set_feedback(input_fb);
        self.input_diffuser_r.set_feedback(input_fb);

        // -- Damping: LP cutoff in feedback (bloom brightness) --
        // Map 0..1 to 12kHz..1.5kHz (higher damping = darker)
        let damp_freq = 12000.0 * (1.0 - params.damping * 0.875);
        for voice in self.voices_l.iter_mut().chain(self.voices_r.iter_mut()) {
            voice.damping.set_freq(damp_freq, sr);
        }

        // -- Modulation: delay modulation depth (chorus in bloom tail) --
        let mod_rate = 0.3 + params.modulation * 1.2; // 0.3..1.5 Hz
        let mod_depth = params.modulation * 12.0; // 0..12 samples
                                                  // Input diffusers
        self.input_diffuser_l
            .set_modulation(mod_rate, mod_depth * 0.5, sr);
        self.input_diffuser_r
            .set_modulation(mod_rate * 1.07, mod_depth * 0.5, sr);
        // Feedback diffusers — slightly different rates per voice for decorrelation
        for (i, voice) in self
            .voices_l
            .iter_mut()
            .chain(self.voices_r.iter_mut())
            .enumerate()
        {
            let rate_offset = 1.0 + (i as f64) * 0.05;
            voice
                .diffuser
                .set_modulation(mod_rate * rate_offset, mod_depth * 0.7, sr);
        }

        // -- Tone: output LP filter --
        // -1..1 mapped to 2kHz..20kHz
        let tone_freq = 2000.0 * (10.0_f64).powf((params.tone + 1.0) * 0.5);
        self.tone_lp_l.set_freq(tone_freq, sr);
        self.tone_lp_r.set_freq(tone_freq, sr);

        // -- Extra A: bloom rate (diffuser strength in feedback path) --
        // Controls how quickly density builds: more feedback diffuser stages + stronger feedback
        let fb_stages = 2 + (params.extra_a * 6.0) as usize; // 2..8
        let fb_diffuser_fb = 0.2 + params.extra_a * 0.5; // 0.2..0.7
        for voice in self.voices_l.iter_mut().chain(self.voices_r.iter_mut()) {
            voice.diffuser.set_active_stages(fb_stages);
            voice.diffuser.set_feedback(fb_diffuser_fb);
        }

        // -- Extra B: stereo width (cross-coupling between L/R) --
        self.stereo_width = params.extra_b;
    }

    #[inline]
    fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        // === Step 1: Input diffusion ===
        let diffused_l = self.input_diffuser_l.tick(left);
        let diffused_r = self.input_diffuser_r.tick(right);

        // === Step 2: Read feedback from delay lines ===
        // Apply internal cross-coupling between voices for density
        let fb_l_mixed = Self::rotate_mix(&self.fb_l, 0.3);
        let fb_r_mixed = Self::rotate_mix(&self.fb_r, 0.3);

        // === Step 3: Stereo cross-coupling ===
        // Blend some of the opposite channel's feedback for width
        let width = self.stereo_width;
        let cross = width * 0.35; // How much opposite channel bleeds in
        let direct = 1.0 - cross * 0.5; // Compensate gain

        let mut new_fb_l = [0.0; NUM_LINES];
        let mut new_fb_r = [0.0; NUM_LINES];

        // === Step 4: Process each delay voice ===
        let decay = self.decay_gain;
        let inv_n = 1.0 / NUM_LINES as f64;

        for i in 0..NUM_LINES {
            // Left channel voice
            {
                let voice = &mut self.voices_l[i];
                let delay_samp = voice.current_delay;

                // Read from delay with fractional interpolation
                let delayed = voice.delay.read_linear(delay_samp);

                // Feedback path: damping → diffuser → DC block
                let damped = voice.damping.tick(delayed);
                let diffused = voice.diffuser.tick(damped);
                let clean = voice.dc_block.tick(diffused);

                // Mix: input + feedback (with stereo cross-feed)
                let fb_in = fb_l_mixed[i] * direct + fb_r_mixed[i] * cross;
                let write_val = diffused_l * inv_n + clean * decay + fb_in * decay * 0.15;

                voice.delay.write(write_val);
                new_fb_l[i] = clean * decay;
            }

            // Right channel voice
            {
                let voice = &mut self.voices_r[i];
                let delay_samp = voice.current_delay;

                let delayed = voice.delay.read_linear(delay_samp);

                let damped = voice.damping.tick(delayed);
                let diffused = voice.diffuser.tick(damped);
                let clean = voice.dc_block.tick(diffused);

                let fb_in = fb_r_mixed[i] * direct + fb_l_mixed[i] * cross;
                let write_val = diffused_r * inv_n + clean * decay + fb_in * decay * 0.15;

                voice.delay.write(write_val);
                new_fb_r[i] = clean * decay;
            }
        }

        // Store feedback for next sample
        self.fb_l = new_fb_l;
        self.fb_r = new_fb_r;

        // === Step 5: Sum outputs and apply tone filter ===
        // Tap outputs from delay lines at staggered points for richness
        let mut out_l = 0.0;
        let mut out_r = 0.0;

        for i in 0..NUM_LINES {
            let voice_l = &self.voices_l[i];
            let voice_r = &self.voices_r[i];

            // Read at the main delay point (already processed through feedback path above)
            let tap_l = voice_l.delay.read_linear(voice_l.current_delay * 0.73);
            let tap_r = voice_r.delay.read_linear(voice_r.current_delay * 0.73);

            // Alternate polarity for decorrelation
            let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
            out_l += tap_l * sign;
            out_r += tap_r * sign;
        }

        // Normalize by number of voices
        out_l *= inv_n;
        out_r *= inv_n;

        // Apply stereo width to output (mid-side processing)
        let mid = (out_l + out_r) * 0.5;
        let side = (out_l - out_r) * 0.5;
        let width_gain = 0.5 + self.stereo_width * 0.5; // 0.5..1.0
        out_l = mid + side * width_gain;
        out_r = mid - side * width_gain;

        // Tone filter
        out_l = self.tone_lp_l.tick(out_l);
        out_r = self.tone_lp_r.tick(out_r);

        (out_l, out_r)
    }
}
