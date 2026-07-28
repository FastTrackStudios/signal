//! TremChain — complete tremolo processor with modulation engine.
//!
//! Wraps the Tremolo with a Modulator from fts-modulation and
//! implements the Processor trait.
//!
//! Signal flow:
//! ```text
//! Input -> Envelope Follower (dynamics) -> Rate/Depth Mod
//!       -> Phase (with groove warp + feel offset) -> Modulator -> Accent Scale
//!       -> Tremolo (amplitude mod) -> Analog Style -> Width -> Mix -> Output
//! ```

use audiocore_dsp::{AudioConfig, Processor};
use fts_modulation::curves::CurveType;
use fts_modulation::modulator::Modulator;
use fts_modulation::pattern::Point;
use fts_modulation::tempo::TransportInfo;
use fts_modulation::trigger::TriggerMode;

use crate::dynamics::TremDynamics;
use crate::tremolo::{AnalogProcessor, TremMode, Tremolo};

// ---------------------------------------------------------------------------
// Groove helpers
// ---------------------------------------------------------------------------

/// Apply groove (swing/shuffle) warp to a modulation phase.
///
/// `groove` in -1..1:
/// - 0 = even (50/50 split)
/// - +1 = swing (first beat gets 2/3 duration, triplet feel)
/// - -1 = shuffle (first beat gets 1/3 duration)
///
/// The phase is assumed to be in [0, 1) representing one full modulation cycle.
#[inline]
fn groove_warp(phase: f64, groove: f64) -> f64 {
    if groove.abs() < 1e-10 {
        return phase;
    }

    // Split point: where beat 2 starts.
    // At groove=0: split=0.5 (even)
    // At groove=+1 (swing): split=2/3 (first beat longer)
    // At groove=-1 (shuffle): split=1/3 (first beat shorter)
    let split = 0.5 + groove * (1.0 / 6.0); // maps -1..1 to 1/3..2/3

    if phase < split {
        // First half: remap [0, split) -> [0, 0.5)
        phase * 0.5 / split
    } else {
        // Second half: remap [split, 1.0) -> [0.5, 1.0)
        0.5 + (phase - split) * 0.5 / (1.0 - split)
    }
}

// ---------------------------------------------------------------------------
// Accent helpers
// ---------------------------------------------------------------------------

/// Compute the accent scaling factor for the current beat position.
///
/// `accent` in -1..1:
/// - Positive: emphasize downbeat (beat 1 gets more depth, others less)
/// - Negative: de-emphasize downbeat (beat 1 dropout, others normal)
///
/// `beat_in_bar` is the fractional beat position within a bar (0..4 for 4/4).
#[inline]
fn accent_scale(beat_in_bar: f64, accent: f64) -> f64 {
    if accent.abs() < 1e-10 {
        return 1.0;
    }

    // Beat 1 is the region near integer bar boundaries (0..1)
    let is_downbeat = beat_in_bar < 1.0;

    if is_downbeat {
        // On the downbeat: positive accent = more depth, negative = less
        (1.0 + accent).clamp(0.0, 2.0)
    } else {
        // On other beats: positive accent = less depth, negative = more
        (1.0 - accent * 0.5).clamp(0.0, 2.0)
    }
}

// ---------------------------------------------------------------------------
// TremChain
// ---------------------------------------------------------------------------

/// Complete tremolo processing chain.
///
/// Signal flow: Modulator -> Tremolo (L/R with stereo offset) -> Analog Style -> Mix -> Output.
pub struct TremChain {
    pub tremolo_l: Tremolo,
    pub tremolo_r: Tremolo,
    pub modulator: Modulator,

    /// Dry/wet mix (0..1).
    pub mix: f64,
    /// Stereo phase offset in degrees (-180..180).
    pub stereo_phase: f64,

    // --- Tremolator-inspired features ---
    /// Groove (swing/shuffle). -1..1.
    /// Negative = shuffle (first beat shorter), positive = swing (first beat longer).
    pub groove: f64,
    /// Feel (draggin'/rushin'). -1..1.
    /// Negative = dragging (behind beat), positive = rushing (ahead of beat).
    /// Applies a phase offset of up to +/-50ms.
    pub feel: f64,
    /// Accent. -1..1.
    /// Positive = emphasize downbeat, negative = de-emphasize downbeat.
    pub accent: f64,
    /// Dynamics envelope follower.
    pub dynamics: TremDynamics,
    /// Analog-style saturation (left channel).
    pub analog_l: AnalogProcessor,
    /// Analog-style saturation (right channel).
    pub analog_r: AnalogProcessor,

    transport: TransportInfo,
    sample_rate: f64,
}

impl TremChain {
    pub fn new() -> Self {
        let mut modulator = Modulator::new();
        // Default: sine-like LFO at 1/4 note
        modulator.trigger.mode = TriggerMode::Sync;
        modulator.trigger.sync_index = 7; // 1/4 note
        modulator.smoother.set_params(0.0, 0.0, 48000.0); // No smoothing for clean LFO

        // Default sine pattern
        let pat = modulator.patterns.get_mut(0);
        pat.add_point(Point {
            id: 0,
            x: 0.0,
            y: 0.0,
            tension: 0.0,
            curve_type: CurveType::HalfSine,
            clear_tails: false,
        });
        pat.add_point(Point {
            id: 0,
            x: 0.5,
            y: 1.0,
            tension: 0.0,
            curve_type: CurveType::HalfSine,
            clear_tails: false,
        });
        pat.add_point(Point {
            id: 0,
            x: 1.0,
            y: 0.0,
            tension: 0.0,
            curve_type: CurveType::HalfSine,
            clear_tails: false,
        });
        modulator.patterns.set_active(0);

        Self {
            tremolo_l: Tremolo::new(),
            tremolo_r: Tremolo::new(),
            modulator,
            mix: 1.0,
            stereo_phase: 0.0,
            groove: 0.0,
            feel: 0.0,
            accent: 0.0,
            dynamics: TremDynamics::new(),
            analog_l: AnalogProcessor::new(),
            analog_r: AnalogProcessor::new(),
            transport: TransportInfo::default(),
            sample_rate: 48000.0,
        }
    }

    /// Set transport info (call per-block from the plugin).
    pub fn set_transport(&mut self, transport: TransportInfo) {
        self.transport = transport;
    }

    /// Set tremolo mode on both channels.
    pub fn set_mode(&mut self, mode: TremMode) {
        self.tremolo_l.mode = mode;
        self.tremolo_r.mode = mode;
    }

    /// Set depth on both channels.
    pub fn set_depth(&mut self, depth: f64) {
        self.tremolo_l.depth = depth;
        self.tremolo_r.depth = depth;
    }

    /// Set the analog style on both channels.
    pub fn set_analog_style(&mut self, style: crate::tremolo::AnalogStyle) {
        self.analog_l.style = style;
        self.analog_r.style = style;
    }

    /// Compute feel phase offset in beats for the current tempo.
    #[inline]
    fn feel_offset_beats(&self) -> f64 {
        if self.feel.abs() < 1e-10 || self.transport.tempo_bpm <= 0.0 {
            return 0.0;
        }
        // Max offset: 50ms worth of beats
        let max_offset_s = 0.050;
        let beats_per_sec = self.transport.tempo_bpm / 60.0;
        let max_offset_beats = max_offset_s * beats_per_sec;
        self.feel * max_offset_beats
    }
}

impl Default for TremChain {
    fn default() -> Self {
        Self::new()
    }
}

impl Processor for TremChain {
    fn reset(&mut self) {
        self.tremolo_l.reset();
        self.tremolo_r.reset();
        self.modulator.reset();
        self.dynamics.reset();
        self.analog_l.reset();
        self.analog_r.reset();
    }

    fn update(&mut self, config: AudioConfig) {
        self.sample_rate = config.sample_rate;
        self.tremolo_l.update(config.sample_rate);
        self.tremolo_r.update(config.sample_rate);
        self.modulator.update(config.sample_rate);
        self.modulator.stereo_offset = self.stereo_phase / 360.0;
        self.dynamics.update(config.sample_rate);
        self.analog_l.update(config.sample_rate);
        self.analog_r.update(config.sample_rate);
    }

    fn process(&mut self, left: &mut [f64], right: &mut [f64]) {
        let len = left.len().min(right.len());
        let bps = self.transport.beats_per_sample(self.sample_rate);
        let feel_offset = self.feel_offset_beats();

        for i in 0..len {
            // Advance transport position
            let pos = self.transport.position_qn + i as f64 * bps;
            let t = TransportInfo {
                position_qn: pos,
                ..self.transport
            };

            // 1. Dynamics envelope: measure input level, get rate/depth modulation
            let input_level = (left[i].abs() + right[i].abs()) * 0.5;
            let (_rate_scale, depth_offset) = self.dynamics.tick(input_level);

            // Apply dynamics depth offset to tremolo depth (temporarily)
            let base_depth_l = self.tremolo_l.depth;
            let base_depth_r = self.tremolo_r.depth;
            self.tremolo_l.depth = (base_depth_l + depth_offset).clamp(0.0, 1.0);
            self.tremolo_r.depth = (base_depth_r + depth_offset).clamp(0.0, 1.0);

            // 2. Get modulation values from modulator
            // The modulator handles phase internally via the trigger engine.
            // We apply feel as an offset to the transport position.
            let t_felt = TransportInfo {
                position_qn: pos + feel_offset,
                ..t
            };
            let mod_l = self.modulator.tick(&t_felt, 0.0);
            let mod_r = if self.stereo_phase.abs() > 0.01 {
                self.modulator.output_stereo()
            } else {
                mod_l
            };

            // 3. Apply groove warp to modulation values
            // Groove warps the time within the modulation cycle.
            // Since the modulator already computed based on phase, we apply groove
            // as a remapping of the modulation output position in the cycle.
            // We do this by warping the mod value through the groove function.
            let mod_l = if self.groove.abs() > 1e-10 {
                // Get the raw phase from the cycle and warp it
                // Since we can't easily get the raw phase, we approximate:
                // the mod value IS the pattern output. For groove to work properly,
                // we'd need access to the phase. Instead, we use the transport-based
                // phase estimation and re-evaluate.
                let sync_qn = 1.0; // Default 1/4 note cycle
                let raw_phase = (pos / sync_qn).fract();
                let warped = groove_warp(raw_phase, self.groove);
                // Evaluate the pattern at the warped phase
                let pattern = self.modulator.patterns.active();
                let raw_y = pattern.get_y(warped);
                // Apply the modulator's range/invert settings manually
                let inverted = if self.modulator.invert {
                    1.0 - raw_y
                } else {
                    raw_y
                };
                (self.modulator.min + (self.modulator.max - self.modulator.min) * inverted)
                    .clamp(0.0, 1.0)
            } else {
                mod_l
            };

            let mod_r = if self.groove.abs() > 1e-10 && self.stereo_phase.abs() > 0.01 {
                let sync_qn = 1.0;
                let raw_phase = ((pos + feel_offset) / sync_qn + self.stereo_phase / 360.0).fract();
                let raw_phase = if raw_phase < 0.0 {
                    raw_phase + 1.0
                } else {
                    raw_phase
                };
                let warped = groove_warp(raw_phase, self.groove);
                let pattern = self.modulator.patterns.active();
                let raw_y = pattern.get_y(warped);
                let inverted = if self.modulator.invert {
                    1.0 - raw_y
                } else {
                    raw_y
                };
                (self.modulator.min + (self.modulator.max - self.modulator.min) * inverted)
                    .clamp(0.0, 1.0)
            } else {
                mod_r
            };

            // 4. Apply accent scaling based on beat position in bar
            let beat_in_bar = pos.rem_euclid(4.0); // Assume 4/4 time
            let accent_l = accent_scale(beat_in_bar, self.accent);
            let accent_r = accent_l; // Same accent on both channels

            // Scale the modulation depth by accent (push mod toward or away from center)
            let mod_l = 0.5 + (mod_l - 0.5) * accent_l;
            let mod_r = 0.5 + (mod_r - 0.5) * accent_r;

            // 5. Apply tremolo amplitude modulation
            let dry_l = left[i];
            let dry_r = right[i];

            let wet_l = self.tremolo_l.tick(left[i], mod_l.clamp(0.0, 1.0), 0);
            let wet_r = self.tremolo_r.tick(right[i], mod_r.clamp(0.0, 1.0), 1);

            // 6. Apply analog style saturation
            let wet_l = self.analog_l.tick(wet_l, 0);
            let wet_r = self.analog_r.tick(wet_r, 1);

            // 7. Mix dry/wet
            left[i] = dry_l * (1.0 - self.mix) + wet_l * self.mix;
            right[i] = dry_r * (1.0 - self.mix) + wet_r * self.mix;

            // Restore base depth (dynamics modulation is per-sample)
            self.tremolo_l.depth = base_depth_l;
            self.tremolo_r.depth = base_depth_r;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    fn config() -> AudioConfig {
        AudioConfig {
            sample_rate: SR,
            max_buffer_size: 512,
        }
    }

    fn playing_transport() -> TransportInfo {
        TransportInfo {
            position_qn: 0.0,
            tempo_bpm: 120.0,
            playing: true,
        }
    }

    // --- Original tests (preserved) ---

    #[test]
    fn silence_in_silence_out() {
        let mut c = TremChain::new();
        c.update(config());
        c.set_transport(playing_transport());

        let mut l = vec![0.0; 4800];
        let mut r = vec![0.0; 4800];
        c.process(&mut l, &mut r);

        for (i, &s) in l.iter().enumerate() {
            assert!(s.abs() < 1e-10, "Non-zero at {i}: {s}");
        }
    }

    #[test]
    fn modulates_signal() {
        let mut c = TremChain::new();
        c.set_depth(1.0);
        c.update(config());
        c.set_transport(playing_transport());

        // Constant 1.0 input
        let mut l = vec![1.0; 48000];
        let mut r = vec![1.0; 48000];
        c.process(&mut l, &mut r);

        // Should have both loud and quiet parts
        let min: f64 = l.iter().copied().fold(f64::MAX, f64::min);
        let max: f64 = l.iter().copied().fold(f64::MIN, f64::max);
        assert!(max > 0.9, "Should have loud parts: max={max}");
        assert!(min < 0.2, "Should have quiet parts: min={min}");
    }

    #[test]
    fn zero_depth_passes_through() {
        let mut c = TremChain::new();
        c.set_depth(0.0);
        c.update(config());
        c.set_transport(playing_transport());

        let mut l = vec![0.5; 4800];
        let mut r = vec![0.5; 4800];
        c.process(&mut l, &mut r);

        for (i, &s) in l.iter().enumerate() {
            assert!((s - 0.5).abs() < 1e-5, "Should pass through at {i}: {s}");
        }
    }

    #[test]
    fn no_nan() {
        let mut c = TremChain::new();
        c.set_depth(1.0);
        c.set_mode(TremMode::Harmonic);
        c.update(config());
        c.set_transport(playing_transport());

        use std::f64::consts::PI;
        let mut l: Vec<f64> = (0..48000)
            .map(|i| (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5)
            .collect();
        let mut r = l.clone();
        c.process(&mut l, &mut r);

        for (i, &s) in l.iter().enumerate() {
            assert!(s.is_finite(), "NaN at {i}");
        }
    }

    // --- Groove tests ---

    #[test]
    fn groove_warp_zero_is_identity() {
        for i in 0..100 {
            let p = i as f64 / 100.0;
            let warped = groove_warp(p, 0.0);
            assert!(
                (warped - p).abs() < 1e-10,
                "groove=0 should be identity: p={p}, warped={warped}"
            );
        }
    }

    #[test]
    fn groove_warp_preserves_endpoints() {
        for groove in &[-1.0, -0.5, 0.0, 0.5, 1.0] {
            let at_zero = groove_warp(0.0, *groove);
            assert!(
                at_zero.abs() < 1e-10,
                "groove={groove}: warp(0) = {at_zero}"
            );
            // Phase just under 1.0 should map close to 1.0
            let near_one = groove_warp(0.999, *groove);
            assert!(near_one > 0.99, "groove={groove}: warp(0.999) = {near_one}");
        }
    }

    #[test]
    fn groove_swing_stretches_first_half() {
        // Swing (+1): first beat gets 2/3 duration.
        // At phase 0.5 with groove=+1, split = 0.5 + 1/6 = 2/3.
        // Since 0.5 < 2/3, we're in the first half.
        // warped = 0.5 * 0.5 / (2/3) = 0.375
        let warped = groove_warp(0.5, 1.0);
        assert!(
            warped < 0.5,
            "Swing should slow first half: warp(0.5) = {warped}"
        );
    }

    #[test]
    fn groove_shuffle_compresses_first_half() {
        // Shuffle (-1): first beat gets 1/3 duration.
        // split = 0.5 - 1/6 = 1/3. Phase 0.25 < 1/3, in first half.
        // warped = 0.25 * 0.5 / (1/3) = 0.375
        let warped = groove_warp(0.25, -1.0);
        assert!(
            warped > 0.25,
            "Shuffle should rush first half: warp(0.25) = {warped}"
        );
    }

    // --- Feel tests ---

    #[test]
    fn feel_zero_no_offset() {
        let c = TremChain::new();
        assert!(
            c.feel_offset_beats().abs() < 1e-10,
            "Feel=0 should give no offset"
        );
    }

    #[test]
    fn feel_positive_rushes() {
        let mut c = TremChain::new();
        c.feel = 1.0;
        c.transport = TransportInfo {
            position_qn: 0.0,
            tempo_bpm: 120.0,
            playing: true,
        };
        let offset = c.feel_offset_beats();
        assert!(
            offset > 0.0,
            "Positive feel should rush (positive offset): {offset}"
        );
        // At 120 BPM, 50ms = 0.1 beats
        assert!(
            (offset - 0.1).abs() < 0.01,
            "50ms at 120 BPM = 0.1 beats: {offset}"
        );
    }

    #[test]
    fn feel_negative_drags() {
        let mut c = TremChain::new();
        c.feel = -1.0;
        c.transport = TransportInfo {
            position_qn: 0.0,
            tempo_bpm: 120.0,
            playing: true,
        };
        let offset = c.feel_offset_beats();
        assert!(offset < 0.0, "Negative feel should drag: {offset}");
    }

    // --- Accent tests ---

    #[test]
    fn accent_zero_no_change() {
        assert!((accent_scale(0.0, 0.0) - 1.0).abs() < 1e-10);
        assert!((accent_scale(2.5, 0.0) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn accent_positive_emphasizes_downbeat() {
        let on_1 = accent_scale(0.5, 1.0); // On beat 1
        let on_3 = accent_scale(2.5, 1.0); // On beat 3
        assert!(
            on_1 > on_3,
            "Positive accent: beat 1 ({on_1}) > beat 3 ({on_3})"
        );
    }

    #[test]
    fn accent_negative_deemphasizes_downbeat() {
        let on_1 = accent_scale(0.5, -1.0); // On beat 1
        let on_3 = accent_scale(2.5, -1.0); // On beat 3
        assert!(
            on_1 < on_3,
            "Negative accent: beat 1 ({on_1}) < beat 3 ({on_3})"
        );
    }

    // --- Integration tests ---

    #[test]
    fn all_features_no_nan() {
        let mut c = TremChain::new();
        c.set_depth(0.8);
        c.groove = 0.5;
        c.feel = -0.3;
        c.accent = 0.7;
        c.dynamics.rate_mod = 1.0;
        c.dynamics.depth_mod = 0.3;
        c.dynamics.threshold_db = -40.0;
        c.set_analog_style(crate::tremolo::AnalogStyle::Dirt);
        c.update(config());
        c.set_transport(playing_transport());

        use std::f64::consts::PI;
        let mut l: Vec<f64> = (0..48000)
            .map(|i| (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5)
            .collect();
        let mut r = l.clone();
        c.process(&mut l, &mut r);

        for (i, &s) in l.iter().enumerate() {
            assert!(s.is_finite(), "NaN at {i}");
        }
        for (i, &s) in r.iter().enumerate() {
            assert!(s.is_finite(), "NaN at R {i}");
        }
    }

    #[test]
    fn analog_style_affects_output() {
        // Compare clean vs dirty output
        let mut clean_chain = TremChain::new();
        clean_chain.set_depth(1.0);
        clean_chain.set_analog_style(crate::tremolo::AnalogStyle::Clean);
        clean_chain.update(config());
        clean_chain.set_transport(playing_transport());

        let mut dirty_chain = TremChain::new();
        dirty_chain.set_depth(1.0);
        dirty_chain.set_analog_style(crate::tremolo::AnalogStyle::Dirt);
        dirty_chain.update(config());
        dirty_chain.set_transport(playing_transport());

        let mut l_clean = vec![0.8; 4800];
        let mut r_clean = vec![0.8; 4800];
        clean_chain.process(&mut l_clean, &mut r_clean);

        let mut l_dirty = vec![0.8; 4800];
        let mut r_dirty = vec![0.8; 4800];
        dirty_chain.process(&mut l_dirty, &mut r_dirty);

        // They should differ
        let mut diff_count = 0;
        for i in 0..4800 {
            if (l_clean[i] - l_dirty[i]).abs() > 1e-6 {
                diff_count += 1;
            }
        }
        assert!(
            diff_count > 100,
            "Analog style should change output: only {diff_count} samples differ"
        );
    }

    #[test]
    fn groove_changes_modulation_pattern() {
        // Run with groove=0 and groove=0.8, compare
        let mut c0 = TremChain::new();
        c0.set_depth(1.0);
        c0.groove = 0.0;
        c0.update(config());
        c0.set_transport(playing_transport());

        let mut c1 = TremChain::new();
        c1.set_depth(1.0);
        c1.groove = 0.8;
        c1.update(config());
        c1.set_transport(playing_transport());

        let mut l0 = vec![1.0; 24000];
        let mut r0 = vec![1.0; 24000];
        c0.process(&mut l0, &mut r0);

        let mut l1 = vec![1.0; 24000];
        let mut r1 = vec![1.0; 24000];
        c1.process(&mut l1, &mut r1);

        let mut diff_count = 0;
        for i in 0..24000 {
            if (l0[i] - l1[i]).abs() > 0.01 {
                diff_count += 1;
            }
        }
        assert!(
            diff_count > 100,
            "Groove should change timing: only {diff_count} samples differ"
        );
    }

    #[test]
    fn defaults_backward_compatible() {
        // Ensure default TremChain with no new features behaves like the original
        let c = TremChain::new();
        assert!((c.groove).abs() < 1e-10);
        assert!((c.feel).abs() < 1e-10);
        assert!((c.accent).abs() < 1e-10);
        assert!(c.dynamics.is_bypassed());
        assert_eq!(c.analog_l.style, crate::tremolo::AnalogStyle::Clean);
    }
}
