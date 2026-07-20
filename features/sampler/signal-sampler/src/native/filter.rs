//! Native **Filter** block — a stereo state-variable filter (Cytomic/Zavalishin
//! TPT SVF), the built-in DSP for `BlockType::Filter`.
//!
//! Covers the Nord filter menu's core shapes (LP/HP/BP; LP24 later by cascading
//! two sections). Defaults are transparent-ish (LP just under Nyquist) so a
//! placeholder-parameterized preset keeps passing audio.

use signal_plugin_host::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterMode {
    Lowpass,
    Highpass,
    Bandpass,
    Notch,
}

impl FilterMode {
    /// Parse a mode name (`"lp"`, `"highpass"`, `"bp"`, `"notch"`, …).
    pub fn parse(s: &str) -> Option<Self> {
        let k = s.to_ascii_lowercase();
        Some(match () {
            _ if k.starts_with("lp") || k.starts_with("low") => FilterMode::Lowpass,
            _ if k.starts_with("hp") || k.starts_with("high") => FilterMode::Highpass,
            _ if k.starts_with("bp") || k.starts_with("band") => FilterMode::Bandpass,
            _ if k.starts_with("notch") => FilterMode::Notch,
            _ => return None,
        })
    }
}

/// One TPT state-variable filter section (mono).
#[derive(Clone, Copy, Debug, Default)]
pub struct Svf {
    // Coefficients.
    a1: f32,
    a2: f32,
    a3: f32,
    k: f32,
    // State.
    ic1: f32,
    ic2: f32,
}

impl Svf {
    /// Set cutoff/resonance. `q` ≥ ~0.5; 0.707 = flat.
    pub fn set(&mut self, cutoff_hz: f32, q: f32, sample_rate: f32) {
        let sr = sample_rate.max(1.0);
        let fc = cutoff_hz.clamp(10.0, sr * 0.45);
        let g = (core::f32::consts::PI * fc / sr).tan();
        let k = 1.0 / q.max(0.1);
        let a1 = 1.0 / (1.0 + g * (g + k));
        self.a1 = a1;
        self.a2 = g * a1;
        self.a3 = g * self.a2;
        self.k = k;
    }

    pub fn reset(&mut self) {
        self.ic1 = 0.0;
        self.ic2 = 0.0;
    }

    /// Process one sample, returning `(lowpass, bandpass, highpass)`.
    #[inline]
    pub fn tick(&mut self, v0: f32) -> (f32, f32, f32) {
        let v3 = v0 - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (v2, v1, v0 - self.k * v1 - v2)
    }
}

/// A saturating 4-stage ladder (Moog-style): four one-pole lowpasses with
/// global resonance feedback and a tanh nonlinearity at the input — the
/// "character" engine behind the Juicy/Moogie/OB/Jupiter/FATBOY families.
#[derive(Clone, Copy, Debug, Default)]
pub struct Ladder {
    a: f32,
    k: f32,
    y: [f32; 4],
}

impl Ladder {
    pub fn set(&mut self, cutoff_hz: f32, resonance: f32, sample_rate: f32) {
        let sr = sample_rate.max(1.0);
        let fc = cutoff_hz.clamp(10.0, sr * 0.45);
        self.a = 1.0 - (-core::f32::consts::TAU * fc / sr).exp();
        // 0..1 resonance → 0..4 feedback (self-oscillation at ~4).
        self.k = resonance.clamp(0.0, 1.0) * 3.8;
    }

    pub fn reset(&mut self) {
        self.y = [0.0; 4];
    }

    #[inline]
    pub fn tick(&mut self, x: f32) -> f32 {
        let t = (x - self.k * self.y[3]).tanh();
        self.y[0] += self.a * (t - self.y[0]);
        self.y[1] += self.a * (self.y[0] - self.y[1]);
        self.y[2] += self.a * (self.y[1] - self.y[2]);
        self.y[3] += self.a * (self.y[2] - self.y[3]);
        // Makeup for the passband loss the feedback causes.
        self.y[3] * (1.0 + self.k * 0.5)
    }
}

/// Which engine realizes the filter: the clean SVF cascade or the
/// saturating ladder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FilterCharacter {
    #[default]
    Clean,
    Ladder,
}

/// The `Filter` block: stereo multi-pole SVF processor. `sections` cascades
/// 12 dB TPT sections (1..=4 → 12/24/36/48 dB); resonance lives on the first
/// section, the rest stay flat so the cascade doesn't compound Q.
pub struct NativeFilter {
    sample_rate: f32,
    mode: FilterMode,
    cutoff_hz: f32,
    q: f32,
    sections: usize,
    character: FilterCharacter,
    left: [Svf; 4],
    right: [Svf; 4],
    ladder_l: Ladder,
    ladder_r: Ladder,
    prepared: bool,
}

impl NativeFilter {
    pub fn new(sample_rate: u32) -> Self {
        let mut f = Self {
            sample_rate: sample_rate.max(1) as f32,
            mode: FilterMode::Lowpass,
            cutoff_hz: 20_000.0,
            q: core::f32::consts::FRAC_1_SQRT_2,
            sections: 1,
            character: FilterCharacter::Clean,
            left: [Svf::default(); 4],
            right: [Svf::default(); 4],
            ladder_l: Ladder::default(),
            ladder_r: Ladder::default(),
            prepared: false,
        };
        f.update_coeffs();
        f
    }

    /// Select the saturating ladder engine (lowpass only; other modes fall
    /// back to the clean cascade).
    #[must_use]
    pub fn with_character(mut self, character: FilterCharacter) -> Self {
        self.character = character;
        self.update_coeffs();
        self
    }

    /// Pole count 1..=8 → cascade sections (12 dB per section, rounded up).
    #[must_use]
    pub fn with_poles(mut self, poles: u32) -> Self {
        self.sections = poles.clamp(1, 8).div_ceil(2) as usize;
        self.update_coeffs();
        self
    }

    #[must_use]
    pub fn with_mode(mut self, mode: FilterMode) -> Self {
        self.mode = mode;
        self
    }

    #[must_use]
    pub fn with_cutoff(mut self, hz: f32) -> Self {
        self.cutoff_hz = hz;
        self.update_coeffs();
        self
    }

    #[must_use]
    pub fn with_q(mut self, q: f32) -> Self {
        self.q = q;
        self.update_coeffs();
        self
    }

    fn update_coeffs(&mut self) {
        for i in 0..self.sections {
            // Resonance on the first section only; the cascade stays flat.
            let q = if i == 0 {
                self.q
            } else {
                core::f32::consts::FRAC_1_SQRT_2
            };
            self.left[i].set(self.cutoff_hz, q, self.sample_rate);
            self.right[i].set(self.cutoff_hz, q, self.sample_rate);
        }
        let res_norm = ((self.q - 0.5) / 11.5).clamp(0.0, 1.0);
        self.ladder_l
            .set(self.cutoff_hz, res_norm, self.sample_rate);
        self.ladder_r
            .set(self.cutoff_hz, res_norm, self.sample_rate);
    }

    /// Run one sample through the cascade of one channel.
    #[inline]
    fn tick_chain(chain: &mut [Svf], sections: usize, mode: FilterMode, x: f32) -> f32 {
        let mut y = x;
        for svf in chain.iter_mut().take(sections) {
            let (lp, bp, hp) = svf.tick(y);
            y = Self::pick(mode, lp, bp, hp);
        }
        y
    }

    /// Normalized 0..1 → 20 Hz..20 kHz (exponential).
    pub fn cutoff_from_norm(v: f32) -> f32 {
        20.0 * 1000f32.powf(v.clamp(0.0, 1.0))
    }

    /// 20 Hz..20 kHz → normalized 0..1.
    pub fn norm_from_cutoff(hz: f32) -> f32 {
        ((hz / 20.0).max(1.0).log10() / 3.0).clamp(0.0, 1.0)
    }

    /// Normalized 0..1 → Q 0.5..12 (flat ≈ 0.018).
    pub fn q_from_norm(v: f32) -> f32 {
        0.5 + 11.5 * v.clamp(0.0, 1.0)
    }

    #[inline]
    fn pick(mode: FilterMode, lp: f32, bp: f32, hp: f32) -> f32 {
        match mode {
            FilterMode::Lowpass => lp,
            FilterMode::Highpass => hp,
            FilterMode::Bandpass => bp,
            FilterMode::Notch => lp + hp,
        }
    }
}

impl PluginInstance for NativeFilter {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "signal.native.filter".into(),
            name: "Filter".into(),
            vendor: "Signal".into(),
            version: String::new(),
            format: PluginFormat::Synthetic,
        }
    }

    fn params(&mut self) -> Vec<PluginParamInfo> {
        vec![
            PluginParamInfo {
                id: 0,
                name: "cutoff".into(),
                min: 0.0,
                max: 1.0,
                default: Self::norm_from_cutoff(20_000.0) as f64,
            },
            PluginParamInfo {
                id: 1,
                name: "resonance".into(),
                min: 0.0,
                max: 1.0,
                default: ((core::f32::consts::FRAC_1_SQRT_2 - 0.5) / 11.5) as f64,
            },
        ]
    }
    fn param_value(&mut self, id: u32) -> Option<f64> {
        match id {
            0 => Some(Self::norm_from_cutoff(self.cutoff_hz) as f64),
            1 => Some(((self.q - 0.5) / 11.5) as f64),
            _ => None,
        }
    }
    fn value_to_text(&mut self, _id: u32, _value: f64) -> Option<String> {
        None
    }
    fn text_to_value(&mut self, _id: u32, _text: &str) -> Option<f64> {
        None
    }
    fn latency(&mut self) -> u32 {
        0
    }

    fn prepare(&mut self, sample_rate: f64, _block_size: u32) -> Result<(), PluginError> {
        self.sample_rate = sample_rate.max(1.0) as f32;
        self.update_coeffs();
        for svf in self.left.iter_mut().chain(self.right.iter_mut()) {
            svf.reset();
        }
        self.ladder_l.reset();
        self.ladder_r.reset();
        self.prepared = true;
        Ok(())
    }

    fn is_prepared(&self) -> bool {
        self.prepared
    }

    fn process_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        // Param writes (mod matrix / UI) applied at block start.
        let mut dirty = false;
        for &(id, value) in events.params {
            match id {
                0 => {
                    self.cutoff_hz = Self::cutoff_from_norm(value as f32);
                    dirty = true;
                }
                1 => {
                    self.q = Self::q_from_norm(value as f32);
                    dirty = true;
                }
                _ => {}
            }
        }
        if dirty {
            self.update_coeffs();
        }
        let frames = out_l.len().min(out_r.len()).min(in_l.len()).min(in_r.len());
        if self.character == FilterCharacter::Ladder && self.mode == FilterMode::Lowpass {
            for f in 0..frames {
                out_l[f] = self.ladder_l.tick(in_l[f]);
                out_r[f] = self.ladder_r.tick(in_r[f]);
            }
            return Ok(());
        }
        let (mode, sections) = (self.mode, self.sections);
        for f in 0..frames {
            out_l[f] = Self::tick_chain(&mut self.left, sections, mode, in_l[f]);
            out_r[f] = Self::tick_chain(&mut self.right, sections, mode, in_r[f]);
        }
        Ok(())
    }

    fn deactivate(&mut self) {
        self.prepared = false;
        for svf in self.left.iter_mut().chain(self.right.iter_mut()) {
            svf.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(buf: &[f32]) -> f32 {
        (buf.iter().map(|s| s * s).sum::<f32>() / buf.len().max(1) as f32).sqrt()
    }

    /// Render a sine at `freq` through the filter, return output RMS.
    fn sine_response(filter: &mut NativeFilter, freq: f32, sr: f32) -> f32 {
        let n = 4_096;
        let input: Vec<f32> = (0..n)
            .map(|i| (core::f32::consts::TAU * freq * i as f32 / sr).sin())
            .collect();
        let (mut out_l, mut out_r) = (vec![0.0; n], vec![0.0; n]);
        let ev = PluginEvents {
            params: &[],
            midi: &[],
            note_expressions: &[],
        };
        filter
            .process_block(&input, &input, &mut out_l, &mut out_r, &ev)
            .unwrap();
        // Skip the transient at the head.
        rms(&out_l[n / 2..])
    }

    #[test]
    fn lowpass_passes_low_attenuates_high() {
        let sr = 48_000.0;
        let mut f = NativeFilter::new(48_000).with_cutoff(1_000.0);
        f.prepare(48_000.0, 4_096).unwrap();
        let low = sine_response(&mut f, 100.0, sr);
        f.prepare(48_000.0, 4_096).unwrap(); // reset state
        let high = sine_response(&mut f, 10_000.0, sr);
        assert!(low > 0.6, "passband ~unity, rms={low}");
        assert!(
            high < 0.1,
            "10 kHz through a 1 kHz LP is >20 dB down, rms={high}"
        );
    }

    #[test]
    fn highpass_mirrors() {
        let sr = 48_000.0;
        let mut f = NativeFilter::new(48_000)
            .with_mode(FilterMode::Highpass)
            .with_cutoff(1_000.0);
        f.prepare(48_000.0, 4_096).unwrap();
        let low = sine_response(&mut f, 100.0, sr);
        f.prepare(48_000.0, 4_096).unwrap();
        let high = sine_response(&mut f, 10_000.0, sr);
        assert!(high > 0.6, "highs pass, rms={high}");
        assert!(low < 0.1, "lows cut, rms={low}");
    }

    #[test]
    fn more_poles_roll_off_steeper() {
        let sr = 48_000.0;
        // 10 kHz through a 1 kHz LP: 24 dB/oct attenuates far more than 12.
        let mut f12 = NativeFilter::new(48_000).with_cutoff(1_000.0).with_poles(2);
        f12.prepare(48_000.0, 4_096).unwrap();
        let two_pole = sine_response(&mut f12, 10_000.0, sr);
        let mut f48 = NativeFilter::new(48_000).with_cutoff(1_000.0).with_poles(8);
        f48.prepare(48_000.0, 4_096).unwrap();
        let eight_pole = sine_response(&mut f48, 10_000.0, sr);
        assert!(
            eight_pole < two_pole * 0.05,
            "8-pole ≫ steeper than 2-pole: 2p={two_pole} 8p={eight_pole}"
        );
    }

    #[test]
    fn notch_cuts_the_center() {
        let sr = 48_000.0;
        let mut f = NativeFilter::new(48_000)
            .with_mode(FilterMode::Notch)
            .with_cutoff(1_000.0)
            .with_q(4.0);
        f.prepare(48_000.0, 4_096).unwrap();
        let at_center = sine_response(&mut f, 1_000.0, sr);
        f.prepare(48_000.0, 4_096).unwrap();
        let far_away = sine_response(&mut f, 100.0, sr);
        assert!(far_away > 0.6, "off-notch passes, rms={far_away}");
        assert!(
            at_center < far_away * 0.35,
            "notch cuts its center: center={at_center} off={far_away}"
        );
    }

    #[test]
    fn ladder_lowpasses_and_resonates() {
        let sr = 48_000.0;
        // Lowpass behavior: highs cut.
        let mut f = NativeFilter::new(48_000)
            .with_character(FilterCharacter::Ladder)
            .with_cutoff(1_000.0);
        f.prepare(48_000.0, 4_096).unwrap();
        let low = sine_response(&mut f, 100.0, sr);
        f.prepare(48_000.0, 4_096).unwrap();
        let high = sine_response(&mut f, 10_000.0, sr);
        assert!(low > 0.4, "ladder passband, rms={low}");
        assert!(high < low * 0.1, "ladder cuts highs: low={low} high={high}");

        // Resonance: a driven ladder boosts near the cutoff (full resonance
        // — the feedback peak grows sharply toward self-oscillation).
        let mut res = NativeFilter::new(48_000)
            .with_character(FilterCharacter::Ladder)
            .with_cutoff(1_000.0)
            .with_q(12.0);
        res.prepare(48_000.0, 4_096).unwrap();
        let at_cutoff = sine_response(&mut res, 1_000.0, sr);
        res.prepare(48_000.0, 4_096).unwrap();
        let below = sine_response(&mut res, 200.0, sr);
        assert!(
            at_cutoff > below * 1.3,
            "resonant peak at cutoff: peak={at_cutoff} passband={below}"
        );
        // Saturation keeps it bounded even when resonating.
        assert!(
            at_cutoff < 3.0,
            "tanh bounds the resonance, rms={at_cutoff}"
        );
    }

    #[test]
    fn default_filter_is_transparent_enough() {
        // A default (LP ~20 kHz) filter must not swallow a preset's audio.
        let sr = 48_000.0;
        let mut f = NativeFilter::new(48_000);
        f.prepare(48_000.0, 4_096).unwrap();
        let mid = sine_response(&mut f, 440.0, sr);
        assert!(mid > 0.6, "default filter passes midrange, rms={mid}");
    }
}
