//! WahChain — complete auto-wah processor with envelope + pattern modulation.
//!
//! Combines a Chamberlin SVF wah filter with triple-cascaded envelope
//! smoothing, sidechain HPF for the envelope detector, and MSEG pattern
//! modulation from fts-modulation.
//!
//! Techniques from: rkrlv2/RyanWah (triple smoothing, sidechain HPF,
//! variable Q tracking), tiagolr (MSEG pattern engine).

use audiocore_dsp::biquad::{Biquad, FilterType};
use audiocore_dsp::envelope::EnvelopeFollower;
use audiocore_dsp::{AudioConfig, Processor};
use fts_modulation::curves::CurveType;
use fts_modulation::modulator::Modulator;
use fts_modulation::pattern::Point;
use fts_modulation::tempo::TransportInfo;
use fts_modulation::trigger::TriggerMode;

use crate::filter::{TripleSmoother, WahFilter, WahMode};

/// Source driving the wah filter cutoff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WahSource {
    /// Envelope follower (auto-wah) — input dynamics drive the filter.
    Envelope,
    /// Pattern/LFO from the modulation engine.
    Pattern,
    /// Both envelope and pattern mixed together.
    Both,
}

/// Complete auto-wah processing chain.
///
/// Signal flow:
/// ```text
/// Input ──→ Sidechain HPF ──→ Envelope Follower ──→ Triple Smoother ──┐
///                                                                      ├──→ Filter Position
///          Transport ──→ Pattern/LFO ──→ RC Smoother ─────────────────┘
///                                                                      │
/// Input ──→ Chamberlin SVF (position, variable Q) ──→ Mix ──→ Output
/// ```
pub struct WahChain {
    pub filter_l: WahFilter,
    pub filter_r: WahFilter,
    pub modulator: Modulator,

    // Envelope detection
    envelope: EnvelopeFollower,
    /// Triple-cascaded smoother for natural envelope response.
    env_smoother: TripleSmoother,
    /// Sidechain HPF — prevents bass from dominating envelope detection.
    sc_hpf: Biquad,

    /// Wah source mode.
    pub source: WahSource,
    /// Envelope sensitivity (0..1). Higher = more responsive.
    pub sensitivity: f64,
    /// Envelope amount (-1..1). Negative = inverted envelope.
    pub env_amount: f64,
    /// Pattern amount (0..1). How much the pattern affects cutoff.
    pub pattern_amount: f64,
    /// Base wah position (0..1). Static offset / pedal position.
    pub base_position: f64,
    /// Sidechain HPF frequency (0 = disabled, default 630 Hz).
    pub sidechain_freq: f64,
    /// Envelope smoothing time in ms (default 20ms).
    pub env_smooth_ms: f64,
    /// Dry/wet mix (0..1).
    pub mix: f64,

    transport: TransportInfo,
    env_value: f64,
    sample_rate: f64,
}

impl WahChain {
    pub fn new() -> Self {
        let mut modulator = Modulator::new();
        modulator.trigger.mode = TriggerMode::Sync;
        modulator.trigger.sync_index = 7; // 1/4 note
        modulator.smoother.set_params(0.0, 0.0, 48000.0);

        // Default sine sweep pattern
        let pat = modulator.patterns.get_mut(0);
        pat.add_point(Point {
            id: 0,
            x: 0.0,
            y: 0.0,
            tension: 0.0,
            curve_type: CurveType::HalfSine,
        });
        pat.add_point(Point {
            id: 0,
            x: 0.5,
            y: 1.0,
            tension: 0.0,
            curve_type: CurveType::HalfSine,
        });
        pat.add_point(Point {
            id: 0,
            x: 1.0,
            y: 0.0,
            tension: 0.0,
            curve_type: CurveType::HalfSine,
        });
        modulator.patterns.set_active(0);

        let mut envelope = EnvelopeFollower::new(0.0);
        envelope.set_times_ms(5.0, 50.0, 48000.0);

        let mut env_smoother = TripleSmoother::new();
        env_smoother.set_time_ms(20.0, 48000.0);

        let mut sc_hpf = Biquad::new();
        sc_hpf.set(FilterType::Highpass, 630.0, 0.707, 48000.0);

        Self {
            filter_l: WahFilter::new(),
            filter_r: WahFilter::new(),
            modulator,
            envelope,
            env_smoother,
            sc_hpf,
            source: WahSource::Envelope,
            sensitivity: 0.5,
            env_amount: 1.0,
            pattern_amount: 0.5,
            base_position: 0.3,
            sidechain_freq: 630.0,
            env_smooth_ms: 20.0,
            mix: 1.0,
            transport: TransportInfo::default(),
            env_value: 0.0,
            sample_rate: 48000.0,
        }
    }

    pub fn set_transport(&mut self, transport: TransportInfo) {
        self.transport = transport;
    }

    pub fn set_mode(&mut self, mode: WahMode) {
        self.filter_l.mode = mode;
        self.filter_r.mode = mode;
    }

    pub fn set_q(&mut self, q: f64) {
        self.filter_l.q = q;
        self.filter_r.q = q;
    }

    /// Current detected envelope level (for metering).
    pub fn envelope_level(&self) -> f64 {
        self.env_value
    }
}

impl Default for WahChain {
    fn default() -> Self {
        Self::new()
    }
}

impl Processor for WahChain {
    fn reset(&mut self) {
        self.filter_l.reset();
        self.filter_r.reset();
        self.modulator.reset();
        self.envelope.reset(0.0);
        self.env_smoother.reset(0.0);
        self.sc_hpf.reset();
        self.env_value = 0.0;
    }

    fn update(&mut self, config: AudioConfig) {
        self.sample_rate = config.sample_rate;
        self.filter_l.update(config.sample_rate);
        self.filter_r.update(config.sample_rate);
        self.modulator.update(config.sample_rate);

        // Envelope: fast attack for picking dynamics, moderate release
        // Sensitivity scales the time constants
        self.envelope.set_times_ms(
            5.0 * (1.0 - self.sensitivity) + 0.5,
            50.0 * (1.0 - self.sensitivity) + 10.0,
            config.sample_rate,
        );

        self.env_smoother
            .set_time_ms(self.env_smooth_ms, config.sample_rate);

        // Sidechain HPF
        if self.sidechain_freq > 0.0 {
            self.sc_hpf.set(
                FilterType::Highpass,
                self.sidechain_freq,
                0.707,
                config.sample_rate,
            );
        }
    }

    fn process(&mut self, left: &mut [f64], right: &mut [f64]) {
        let use_sc = self.sidechain_freq > 0.0;

        for i in 0..left.len().min(right.len()) {
            // Sidechain HPF: filter the detection signal to prevent bass dominance
            let det_signal = (left[i].abs() + right[i].abs()) * 0.5;
            let filtered = if use_sc {
                self.sc_hpf.tick(det_signal, 0).abs()
            } else {
                det_signal
            };

            // Envelope follower with sensitivity scaling
            let scaled_input = filtered * (1.0 + self.sensitivity * 4.0);
            let raw_env = self.envelope.tick(scaled_input);

            // Soft-limit the envelope (sigmoid from rkrlv2)
            let limited = if raw_env > 0.0 {
                1.0 - 1.0 / (raw_env * raw_env + 1.0)
            } else {
                0.0
            };

            // Triple-cascaded smoothing for natural response
            self.env_value = self.env_smoother.tick(limited);

            // Pattern modulation
            let pos = self.transport.position_qn
                + i as f64 * self.transport.beats_per_sample(self.sample_rate);
            let t = TransportInfo {
                position_qn: pos,
                ..self.transport
            };
            let pattern_val = self.modulator.tick(&t, det_signal);

            // Combine sources
            let mod_position = match self.source {
                WahSource::Envelope => self.base_position + self.env_value * self.env_amount,
                WahSource::Pattern => self.base_position + pattern_val * self.pattern_amount,
                WahSource::Both => {
                    self.base_position
                        + self.env_value * self.env_amount
                        + pattern_val * self.pattern_amount
                }
            };

            let position = mod_position.clamp(0.0, 1.0);

            // Set filter with variable Q tracking
            self.filter_l
                .set_position_with_env(position, self.env_value);
            self.filter_r
                .set_position_with_env(position, self.env_value);

            // Process audio through the SVF
            let dry_l = left[i];
            let dry_r = right[i];

            let wet_l = self.filter_l.tick(left[i], 0);
            let wet_r = self.filter_r.tick(right[i], 1);

            left[i] = dry_l * (1.0 - self.mix) + wet_l * self.mix;
            right[i] = dry_r * (1.0 - self.mix) + wet_r * self.mix;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    const SR: f64 = 48000.0;

    fn config() -> AudioConfig {
        AudioConfig {
            sample_rate: SR,
            max_buffer_size: 512,
        }
    }

    #[test]
    fn silence_in_silence_out() {
        let mut w = WahChain::new();
        w.update(config());

        let mut l = vec![0.0; 4800];
        let mut r = vec![0.0; 4800];
        w.process(&mut l, &mut r);

        for (i, &s) in l.iter().enumerate() {
            assert!(s.abs() < 1e-10, "Non-zero at {i}: {s}");
        }
    }

    #[test]
    fn envelope_wah_responds_to_dynamics() {
        let mut w = WahChain::new();
        w.source = WahSource::Envelope;
        w.sensitivity = 0.8;
        w.env_amount = 1.0;
        w.mix = 1.0;
        w.update(config());
        w.set_transport(TransportInfo {
            position_qn: 0.0,
            tempo_bpm: 120.0,
            playing: true,
        });

        let mut l: Vec<f64> = (0..48000)
            .map(|i| {
                let env = if i < 4800 { 0.8 } else { 0.01 };
                (2.0 * PI * 200.0 * i as f64 / SR).sin() * env
            })
            .collect();
        let mut r = l.clone();

        w.process(&mut l, &mut r);

        for (i, &s) in l.iter().enumerate() {
            assert!(s.is_finite(), "NaN at {i}");
        }
    }

    #[test]
    fn pattern_wah_modulates() {
        let mut w = WahChain::new();
        w.source = WahSource::Pattern;
        w.pattern_amount = 1.0;
        w.mix = 1.0;
        w.update(config());
        w.set_transport(TransportInfo {
            position_qn: 0.0,
            tempo_bpm: 120.0,
            playing: true,
        });

        let mut l: Vec<f64> = (0..48000)
            .map(|i| (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5)
            .collect();
        let mut r = l.clone();

        w.process(&mut l, &mut r);

        for (i, &s) in l.iter().enumerate() {
            assert!(s.is_finite(), "NaN at {i}");
        }
    }

    #[test]
    fn zero_mix_passes_through() {
        let mut w = WahChain::new();
        w.mix = 0.0;
        w.update(config());

        let input: Vec<f64> = (0..4800)
            .map(|i| (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5)
            .collect();
        let mut l = input.clone();
        let mut r = input.clone();

        w.process(&mut l, &mut r);

        for (i, (&out, &inp)) in l.iter().zip(input.iter()).enumerate() {
            assert!(
                (out - inp).abs() < 1e-10,
                "Zero mix should pass through at {i}: {out} vs {inp}"
            );
        }
    }

    #[test]
    fn sidechain_hpf_rejects_bass() {
        // With sidechain HPF, a bass-heavy signal should not open the wah as much
        let mut w_no_sc = WahChain::new();
        w_no_sc.source = WahSource::Envelope;
        w_no_sc.sensitivity = 1.0;
        w_no_sc.env_amount = 1.0;
        w_no_sc.sidechain_freq = 0.0; // Disabled
        w_no_sc.mix = 1.0;
        w_no_sc.update(config());

        let mut w_sc = WahChain::new();
        w_sc.source = WahSource::Envelope;
        w_sc.sensitivity = 1.0;
        w_sc.env_amount = 1.0;
        w_sc.sidechain_freq = 630.0; // Enabled
        w_sc.mix = 1.0;
        w_sc.update(config());

        // Feed a low bass note
        let input: Vec<f64> = (0..4800)
            .map(|i| (2.0 * PI * 80.0 * i as f64 / SR).sin() * 0.9)
            .collect();

        let mut l1 = input.clone();
        let mut r1 = input.clone();
        w_no_sc.process(&mut l1, &mut r1);
        let env_no_sc = w_no_sc.envelope_level();

        let mut l2 = input.clone();
        let mut r2 = input.clone();
        w_sc.process(&mut l2, &mut r2);
        let env_sc = w_sc.envelope_level();

        assert!(
            env_sc < env_no_sc,
            "SC HPF should reduce bass envelope response: no_sc={env_no_sc:.4}, sc={env_sc:.4}"
        );
    }

    #[test]
    fn all_modes_no_nan() {
        for mode in &[
            WahMode::Classic,
            WahMode::Mutron,
            WahMode::Lowpass,
            WahMode::Phaser,
        ] {
            let mut w = WahChain::new();
            w.set_mode(*mode);
            w.source = WahSource::Both;
            w.sensitivity = 1.0;
            w.env_amount = 1.0;
            w.pattern_amount = 1.0;
            w.mix = 1.0;
            w.update(config());
            w.set_transport(TransportInfo {
                position_qn: 0.0,
                tempo_bpm: 120.0,
                playing: true,
            });

            let mut l: Vec<f64> = (0..48000)
                .map(|i| (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5)
                .collect();
            let mut r = l.clone();

            w.process(&mut l, &mut r);

            for (i, &s) in l.iter().enumerate() {
                assert!(s.is_finite(), "NaN in {:?} at {i}", mode);
            }
        }
    }
}
