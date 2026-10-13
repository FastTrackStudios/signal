//! Modulator — combines pattern, trigger, and smoother into a single unit.
//!
//! This is the primary interface for plugins. A Modulator produces a smoothed
//! 0..1 output each sample that can be mapped to any plugin parameter
//! (gain, cutoff, delay time, reverb level, etc.).
//!
//! Based on tiagolr's modulation pipeline shared across gate12, filtr, time12, reevr.

use crate::pattern::PatternBank;
use crate::smoother::RcSmoother;
use crate::tempo::TransportInfo;
use crate::transient::TransientDetector;
use crate::trigger::{TriggerEngine, TriggerMode};

/// A complete modulation source: pattern bank + trigger + smoother.
///
/// Plugins create one `Modulator` per modulation target. For example,
/// filtr creates two (cutoff + resonance), reevr creates two (reverb + send),
/// gate12 creates one (volume).
pub struct Modulator {
    /// Pattern bank (12 selectable patterns).
    pub patterns: PatternBank,
    /// Trigger engine (sync/free/MIDI/audio).
    pub trigger: TriggerEngine,
    /// Output smoother.
    pub smoother: RcSmoother,
    /// Transient detector for audio trigger mode.
    pub transient: TransientDetector,

    /// Output range minimum (0..1).
    pub min: f64,
    /// Output range maximum (0..1).
    pub max: f64,
    /// Static offset added to the pattern output.
    pub offset: f64,
    /// Whether to invert the pattern output (1 - y).
    pub invert: bool,

    /// Stereo offset in phase units (-0.5..0.5).
    pub stereo_offset: f64,

    // Internal state
    /// Current raw pattern value (before smoothing).
    raw_y: f64,
    /// Current smoothed output.
    output: f64,
    /// Secondary output for stereo offset.
    output_stereo: f64,

    sample_rate: f64,
}

impl Modulator {
    pub fn new() -> Self {
        Self {
            patterns: PatternBank::new(),
            trigger: TriggerEngine::new(),
            smoother: RcSmoother::new(0.5),
            transient: TransientDetector::new(),
            min: 0.0,
            max: 1.0,
            offset: 0.0,
            invert: false,
            stereo_offset: 0.0,
            raw_y: 0.5,
            output: 0.5,
            output_stereo: 0.5,
            sample_rate: 48000.0,
        }
    }

    /// Update for new sample rate.
    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.trigger.update(sample_rate);
        self.transient.update(sample_rate);
    }

    /// Process one sample. Returns the smoothed modulation value (0..1).
    ///
    /// `transport` provides DAW tempo/position info.
    /// `audio_input` is used for audio trigger detection (can be 0.0 if unused).
    pub fn tick(&mut self, transport: &TransportInfo, audio_input: f64) -> f64 {
        // Check for queued pattern switches
        if let Some(idx) = self.trigger.poll_pattern_switch() {
            self.patterns.set_active(idx);
        }

        // Audio trigger detection
        if self.trigger.mode == TriggerMode::Audio {
            let filtered = self.transient.filter(audio_input, 0);
            if self.transient.tick(filtered) {
                self.trigger.audio_trigger();
            }
        }

        // Advance phase
        let phase = self.trigger.tick(transport);

        // Evaluate pattern
        let pattern = self.patterns.active();
        let raw = pattern.get_y(phase);

        // Apply inversion, range, and offset
        let inverted = if self.invert { 1.0 - raw } else { raw };
        let scaled = self.min + (self.max - self.min) * inverted + self.offset;
        self.raw_y = scaled.clamp(0.0, 1.0);

        // Smooth
        self.output = self.smoother.tick(self.raw_y);

        // Anti-click blend on retrigger
        let ac = self.trigger.anticlick_factor();
        if ac < 1.0 {
            self.output = self.trigger.anticlick_from() * (1.0 - ac) + self.output * ac;
        }

        // Stereo offset calculation
        if self.stereo_offset.abs() > 1e-10 {
            let phase2 = (phase + self.stereo_offset).fract();
            let raw2 = pattern.get_y(if phase2 < 0.0 { phase2 + 1.0 } else { phase2 });
            let inv2 = if self.invert { 1.0 - raw2 } else { raw2 };
            let scaled2 = (self.min + (self.max - self.min) * inv2 + self.offset).clamp(0.0, 1.0);
            // Use a separate smoother instance would be ideal, but for simplicity
            // we approximate with the same smoothing characteristic
            self.output_stereo = self.output_stereo
                + (scaled2 - self.output_stereo)
                    * (self.smoother.value() / self.output.max(1e-10)) // crude ratio
                        .clamp(0.0, 1.0);
            // Actually, let's just do direct smoothing with a fixed coefficient
            let coeff = RcSmoother::coeff(0.0, self.sample_rate); // instant for now
            self.output_stereo += (scaled2 - self.output_stereo) * coeff;
        } else {
            self.output_stereo = self.output;
        }

        self.output
    }

    /// Get the current smoothed output (primary channel).
    #[inline]
    pub fn output(&self) -> f64 {
        self.output
    }

    /// Get the stereo-offset output (secondary channel).
    #[inline]
    pub fn output_stereo(&self) -> f64 {
        self.output_stereo
    }

    /// Get the raw (pre-smoothing) pattern value.
    #[inline]
    pub fn raw(&self) -> f64 {
        self.raw_y
    }

    /// Trigger from MIDI note-on.
    pub fn midi_trigger(&mut self) {
        self.trigger.midi_trigger();
    }

    /// Select a pattern by MIDI note number (note % 12).
    pub fn midi_select_pattern(&mut self, note: u8) {
        self.patterns.set_active((note % 12) as usize);
    }

    pub fn reset(&mut self) {
        self.trigger.reset();
        self.transient.reset();
        self.smoother.reset(0.5);
        self.raw_y = 0.5;
        self.output = 0.5;
        self.output_stereo = 0.5;
    }
}

impl Default for Modulator {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience: a dual-modulator for plugins that need two modulation targets
/// (e.g., filtr: cutoff + resonance, reevr: reverb + send).
pub struct DualModulator {
    /// Primary modulation target (e.g., cutoff, reverb level).
    pub primary: Modulator,
    /// Secondary modulation target (e.g., resonance, send level).
    pub secondary: Modulator,
    /// When true, pattern selection is linked between primary and secondary.
    pub link_patterns: bool,
}

impl DualModulator {
    pub fn new() -> Self {
        Self {
            primary: Modulator::new(),
            secondary: Modulator::new(),
            link_patterns: true,
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.primary.update(sample_rate);
        self.secondary.update(sample_rate);
    }

    /// Process one sample for both modulators.
    pub fn tick(&mut self, transport: &TransportInfo, audio_input: f64) -> (f64, f64) {
        let a = self.primary.tick(transport, audio_input);
        let b = self.secondary.tick(transport, audio_input);
        (a, b)
    }

    /// Select pattern by MIDI note, optionally linking both modulators.
    pub fn midi_select_pattern(&mut self, note: u8) {
        self.primary.midi_select_pattern(note);
        if self.link_patterns {
            self.secondary.midi_select_pattern(note);
        }
    }

    /// Trigger both modulators from MIDI.
    pub fn midi_trigger(&mut self) {
        self.primary.midi_trigger();
        self.secondary.midi_trigger();
    }

    pub fn reset(&mut self) {
        self.primary.reset();
        self.secondary.reset();
    }
}

impl Default for DualModulator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curves::CurveType;
    use crate::pattern::Point;

    const SR: f64 = 48000.0;

    fn transport_120bpm(pos_qn: f64) -> TransportInfo {
        TransportInfo {
            position_qn: pos_qn,
            tempo_bpm: 120.0,
            playing: true,
        }
    }

    #[test]
    fn basic_modulation_output() {
        let mut m = Modulator::new();
        m.update(SR);

        // Set up a simple ramp pattern
        m.patterns.get_mut(0).add_point(Point {
            id: 0,
            x: 0.0,
            y: 1.0,
            tension: 0.0,
            curve_type: CurveType::Curve,
            clear_tails: false,
        });
        m.patterns.get_mut(0).add_point(Point {
            id: 0,
            x: 1.0,
            y: 0.0,
            tension: 0.0,
            curve_type: CurveType::Curve,
            clear_tails: false,
        });
        m.patterns.set_active(0);

        // Smoother set to instant
        m.smoother.set_params(0.0, 0.0, SR);

        let transport = transport_120bpm(0.0);
        let out = m.tick(&transport, 0.0);
        assert!(
            (0.0..=1.0).contains(&out),
            "Output should be in range: {out}"
        );
    }

    #[test]
    fn min_max_range() {
        let mut m = Modulator::new();
        m.min = 0.2;
        m.max = 0.8;
        m.update(SR);

        m.patterns.get_mut(0).add_point(Point {
            id: 0,
            x: 0.0,
            y: 0.0,
            tension: 0.0,
            curve_type: CurveType::Hold,
            clear_tails: false,
        });
        m.patterns.get_mut(0).add_point(Point {
            id: 0,
            x: 1.0,
            y: 0.0,
            tension: 0.0,
            curve_type: CurveType::Hold,
            clear_tails: false,
        });
        m.patterns.set_active(0);
        m.smoother.set_params(0.0, 0.0, SR);

        let transport = transport_120bpm(0.0);
        let out = m.tick(&transport, 0.0);
        assert!(
            (out - 0.2).abs() < 0.01,
            "With y=0, output should be near min: {out}"
        );
    }

    #[test]
    fn invert_flips_output() {
        let mut m = Modulator::new();
        m.invert = true;
        m.update(SR);

        m.patterns.get_mut(0).add_point(Point {
            id: 0,
            x: 0.0,
            y: 1.0,
            tension: 0.0,
            curve_type: CurveType::Hold,
            clear_tails: false,
        });
        m.patterns.get_mut(0).add_point(Point {
            id: 0,
            x: 1.0,
            y: 1.0,
            tension: 0.0,
            curve_type: CurveType::Hold,
            clear_tails: false,
        });
        m.patterns.set_active(0);
        m.smoother.set_params(0.0, 0.0, SR);

        let transport = transport_120bpm(0.0);
        let out = m.tick(&transport, 0.0);
        // y=1.0 inverted = 0.0, then min=0 + (1-0)*0 = 0.0
        assert!((out - 0.0).abs() < 0.01, "Inverted y=1 should be 0: {out}");
    }

    #[test]
    fn dual_modulator_linked() {
        let mut dm = DualModulator::new();
        dm.link_patterns = true;
        dm.update(SR);

        dm.midi_select_pattern(3); // note 3 % 12 = 3
        assert_eq!(dm.primary.patterns.active_index(), 3);
        assert_eq!(dm.secondary.patterns.active_index(), 3);
    }

    #[test]
    fn dual_modulator_unlinked() {
        let mut dm = DualModulator::new();
        dm.link_patterns = false;
        dm.update(SR);

        dm.primary.midi_select_pattern(5);
        dm.secondary.midi_select_pattern(7);
        assert_eq!(dm.primary.patterns.active_index(), 5);
        assert_eq!(dm.secondary.patterns.active_index(), 7);
    }

    #[test]
    fn output_stays_in_range() {
        let mut m = Modulator::new();
        m.update(SR);
        m.smoother.set_params(0.0, 0.0, SR);

        // Set up pattern
        m.patterns.get_mut(0).add_point(Point {
            id: 0,
            x: 0.0,
            y: 0.0,
            tension: 0.0,
            curve_type: CurveType::Curve,
            clear_tails: false,
        });
        m.patterns.get_mut(0).add_point(Point {
            id: 0,
            x: 0.5,
            y: 1.0,
            tension: 0.0,
            curve_type: CurveType::Curve,
            clear_tails: false,
        });
        m.patterns.get_mut(0).add_point(Point {
            id: 0,
            x: 1.0,
            y: 0.0,
            tension: 0.0,
            curve_type: CurveType::Curve,
            clear_tails: false,
        });
        m.patterns.set_active(0);

        // Run through one bar at 120 BPM
        let bps = 120.0 / (60.0 * SR);
        for i in 0..48000 {
            let pos = i as f64 * bps;
            let transport = transport_120bpm(pos);
            let out = m.tick(&transport, 0.0);
            assert!(
                (-0.01..=1.01).contains(&out),
                "Output out of range at sample {i}: {out}"
            );
        }
    }
}
