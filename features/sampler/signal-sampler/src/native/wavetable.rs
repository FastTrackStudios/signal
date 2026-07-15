//! Native **Wavetable** synth voice — the `Native` implementation of
//! `BlockType::Wavetable` (Omnisphere "Synth mode", Nord synth waves), grown
//! into the full oscillator stack:
//!
//! - **Morphing wave** — `shape` 0..1 crossfades sine → triangle → saw →
//!   square (PolyBLEP band-limited). Stands in for the 638 Omnisphere
//!   wavetables until table extraction lands.
//! - **Unison** — up to 8 detuned voices per note, symmetric cent spread,
//!   stereo width (pan spread), 1/√n level compensation.
//! - **Harmonia** — 4 extra oscillators per note with their own interval
//!   (semitones), level, pan and waveform.
//! - **FM** — a per-voice modulator oscillator (ratio + depth) phase-
//!   modulating the whole stack.
//! - **Ring mod** — key-tracked carrier multiplied in at `ring_mix`.
//!
//! Stereo out; per-note ADSR; polyphonic. Runtime params (mod-matrix
//! drivable): shape, unison detune, FM depth, ring mix. Voice-count /
//! width / harmonia are build-time block params.

use signal_plugin_host::{PluginDescriptor, PluginEvents, PluginFormat, PluginParamInfo};

use super::adsr::{Adsr, AdsrParams};
use crate::soundsource::{Soundsource, SoundsourceKind};

/// PolyBLEP residual for a discontinuity at phase 0 (t in 0..1, dt = inc).
#[inline]
fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

/// Morph 0..1 across sine → triangle → saw → square, band-limited.
/// `duty` is the square's pulse width (0.5 = symmetric; the Symmetry/PWM axis).
#[inline]
fn morph(phase: f32, dt: f32, shape: f32, duty: f32) -> f32 {
    let sine = (core::f32::consts::TAU * phase).sin();
    let tri = 4.0 * (phase - 0.5).abs() - 1.0;
    let saw = 2.0 * phase - 1.0 - poly_blep(phase, dt);
    let duty = duty.clamp(0.05, 0.95);
    let mut sq = if phase < duty { 1.0 } else { -1.0 };
    sq += poly_blep(phase, dt);
    sq -= poly_blep((phase + (1.0 - duty)).fract(), dt);
    let w = [sine, tri, saw, sq];
    let x = shape.clamp(0.0, 1.0) * 3.0;
    let i = (x as usize).min(2);
    let frac = x - i as f32;
    w[i] * (1.0 - frac) + w[i + 1] * frac
}

/// One Harmonia sub-oscillator's configuration.
#[derive(Clone, Copy, Debug, Default)]
pub struct HarmVoice {
    pub on: bool,
    /// Level 0..1.
    pub level: f32,
    /// Interval in semitones (±24).
    pub interval_semi: f32,
    /// Pan −1..+1.
    pub pan: f32,
    /// Waveform morph 0..1 (same axis as `shape`).
    pub shape: f32,
}

/// The full synth-voice configuration (build-time; runtime params modulate
/// shape / detune / FM / ring on top).
#[derive(Clone, Copy, Debug)]
pub struct SynthConfig {
    pub shape: f32,
    /// Unison voices per note, 1..=8.
    pub unison_voices: u32,
    /// Total detune spread in cents (voices spaced symmetrically).
    pub unison_detune_cents: f32,
    /// Stereo pan spread 0..1.
    pub unison_width: f32,
    /// Octave mode 0..1: alternate unison voices shift toward +12 semitones.
    pub unison_octave: f32,
    /// Analog mode 0..1: static per-voice random detune jitter (±15 cents).
    pub unison_analog: f32,
    /// Drift 0..1: slow per-voice pitch wander (±8 cents, sub-Hz rates).
    pub unison_drift: f32,
    /// FM depth 0..1 (phase-modulation index, scaled internally).
    pub fm_depth: f32,
    /// FM modulator ratio (modulator freq = note freq × ratio).
    pub fm_ratio: f32,
    /// FM modulator waveform morph 0..1 (same axis as `shape`; 0 = sine).
    pub fm_shape: f32,
    /// Ring-mod wet mix 0..1.
    pub ring_mix: f32,
    /// Ring carrier ratio (key-tracked).
    pub ring_ratio: f32,
    pub harmonia: [HarmVoice; 4],
    /// Per-note amplitude envelope.
    pub env: AdsrParams,
}

impl Default for SynthConfig {
    fn default() -> Self {
        Self {
            shape: 2.0 / 3.0, // saw
            unison_voices: 1,
            unison_detune_cents: 12.0,
            unison_width: 0.7,
            unison_octave: 0.0,
            unison_analog: 0.0,
            unison_drift: 0.0,
            fm_depth: 0.0,
            fm_ratio: 1.0,
            fm_shape: 0.0,
            ring_mix: 0.0,
            ring_ratio: 1.0,
            harmonia: [HarmVoice::default(); 4],
            env: AdsrParams::default(),
        }
    }
}

/// Cheap deterministic per-voice random in −1..+1 (seeded by index).
fn jitter(seed: u32) -> f32 {
    let mut x = seed.wrapping_mul(0x9E37_79B9).wrapping_add(0x85EB_CA6B);
    x ^= x >> 13;
    x = x.wrapping_mul(0xC2B2_AE35);
    x ^= x >> 16;
    (x as f32 / u32::MAX as f32) * 2.0 - 1.0
}

/// One rendered oscillator line (a unison voice or a Harmonia voice).
struct Sub {
    phase: f32,
    inc: f32,
    /// Unmodulated increment (drift wobbles around it).
    base_inc: f32,
    /// Drift LFO state: phase + per-block increment (0 = no drift).
    drift_phase: f32,
    drift_inc: f32,
    drift_cents: f32,
    /// Equal-power pan gains.
    gain_l: f32,
    gain_r: f32,
    level: f32,
    shape: f32,
}

fn pan_gains(pan: f32) -> (f32, f32) {
    // pan −1..+1 → equal-power.
    let x = (pan.clamp(-1.0, 1.0) + 1.0) * 0.25 * core::f32::consts::PI;
    (x.cos(), x.sin())
}

struct Voice {
    note: u8,
    amp: f32,
    env: Adsr,
    subs: Vec<Sub>,
    fm_phase: f32,
    fm_inc: f32,
    ring_phase: f32,
    ring_inc: f32,
    /// Base frequency / sample-rate (for runtime detune recompute).
    base_inc: f32,
}

/// The polyphonic synth-voice oscillator.
pub struct NativeWavetable {
    sample_rate: f32,
    cfg: SynthConfig,
    /// Runtime pitch multiplier (param 4 "tune": 0.5 center, ±24 semitones).
    pitch_mult: f32,
    /// Square pulse width (param 5 "symmetry": 0.5 = symmetric).
    duty: f32,
    /// Harmonia level scale (param 6 "harm_mix").
    harm_mix: f32,
    voices: Vec<Voice>,
}

impl NativeWavetable {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate: sample_rate.max(1) as f32,
            cfg: SynthConfig::default(),
            pitch_mult: 1.0,
            duty: 0.5,
            harm_mix: 1.0,
            voices: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_config(mut self, cfg: SynthConfig) -> Self {
        self.cfg = cfg;
        self.cfg.unison_voices = self.cfg.unison_voices.clamp(1, 8);
        self
    }

    #[must_use]
    pub fn with_shape(mut self, shape: f32) -> Self {
        self.cfg.shape = shape.clamp(0.0, 1.0);
        self
    }

    pub fn config(&self) -> &SynthConfig {
        &self.cfg
    }

    pub fn active_voices(&self) -> usize {
        self.voices.len()
    }

    /// Build the sub-oscillator set for one note at `freq`.
    fn build_subs(&self, freq: f32, note: u8) -> Vec<Sub> {
        let mut subs = Vec::new();
        let n = self.cfg.unison_voices.clamp(1, 8);
        let comp = 1.0 / (n as f32).sqrt();
        for i in 0..n {
            // Symmetric spread: offsets in −1..+1 across the voices.
            let off = if n == 1 {
                0.0
            } else {
                (i as f32 / (n - 1) as f32) * 2.0 - 1.0
            };
            let mut cents = off * self.cfg.unison_detune_cents * 0.5;
            // Octave mode: odd voices pull toward +1200 cents.
            if self.cfg.unison_octave > 0.0 && i % 2 == 1 {
                cents += 1200.0 * self.cfg.unison_octave;
            }
            // Analog mode: static per-voice random jitter (±15 cents max).
            if self.cfg.unison_analog > 0.0 {
                cents += jitter(i.wrapping_add(note as u32 * 31)) * self.cfg.unison_analog * 15.0;
            }
            let f = freq * 2f32.powf(cents / 1200.0);
            let (gain_l, gain_r) = pan_gains(off * self.cfg.unison_width);
            let inc = f / self.sample_rate;
            // Drift: each voice wanders at its own sub-Hz rate.
            let (drift_inc, drift_cents) = if self.cfg.unison_drift > 0.0 {
                let rate = 0.1 + (jitter(i.wrapping_mul(7).wrapping_add(3)) * 0.5 + 0.5) * 0.6;
                (rate / self.sample_rate, self.cfg.unison_drift * 8.0)
            } else {
                (0.0, 0.0)
            };
            subs.push(Sub {
                phase: (i as f32) * 0.37 % 1.0, // decorrelate phases
                inc,
                base_inc: inc,
                drift_phase: jitter(i.wrapping_add(11)) * 0.5 + 0.5,
                drift_inc,
                drift_cents,
                gain_l,
                gain_r,
                level: comp,
                shape: self.cfg.shape,
            });
        }
        for h in self.cfg.harmonia.iter().filter(|h| h.on && h.level > 0.0) {
            let f = freq * 2f32.powf(h.interval_semi / 12.0);
            let (gain_l, gain_r) = pan_gains(h.pan);
            let inc = f / self.sample_rate;
            subs.push(Sub {
                phase: 0.0,
                inc,
                base_inc: inc,
                drift_phase: 0.0,
                drift_inc: 0.0,
                drift_cents: 0.0,
                gain_l,
                gain_r,
                level: h.level,
                shape: h.shape,
            });
        }
        subs
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        if velocity == 0 {
            return self.note_off(note);
        }
        let freq = 440.0 * 2f32.powf((note as f32 - 69.0) / 12.0);
        let amp = (velocity as f32 / 127.0) * 0.15;
        let base_inc = freq / self.sample_rate;
        if let Some(v) = self.voices.iter_mut().find(|v| v.note == note) {
            v.amp = amp;
            v.env.note_on();
        } else {
            let mut env = Adsr::new(self.sample_rate, self.cfg.env);
            env.note_on();
            self.voices.push(Voice {
                note,
                amp,
                env,
                subs: self.build_subs(freq, note),
                fm_phase: 0.0,
                fm_inc: base_inc * self.cfg.fm_ratio,
                ring_phase: 0.0,
                ring_inc: base_inc * self.cfg.ring_ratio,
                base_inc,
            });
        }
    }

    fn note_off(&mut self, note: u8) {
        for v in self.voices.iter_mut().filter(|v| v.note == note) {
            v.env.note_off();
        }
    }

    fn apply_midi(&mut self, message: &midicore::MidiEvent) {
        use midicore::MidiEvent;
        match message {
            MidiEvent::NoteOn { key, velocity, .. } => self.note_on(key.get(), velocity.get()),
            MidiEvent::NoteOff { key, .. } => self.note_off(key.get()),
            _ => {}
        }
    }
}

// r[impl signal.soundsource.oscillator]
impl Soundsource for NativeWavetable {
    fn kind(&self) -> SoundsourceKind {
        SoundsourceKind::Oscillator
    }

    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "signal.native.wavetable".into(),
            name: "Wavetable".into(),
            vendor: "Signal".into(),
            version: String::new(),
            format: PluginFormat::Synthetic,
        }
    }

    fn params(&self) -> Vec<PluginParamInfo> {
        // Defaults report the CURRENT values, so the leaf's `param_value`
        // (a `params()` lookup) reads back live state.
        let mk = |id, name: &str, default: f64| PluginParamInfo {
            id,
            name: name.into(),
            min: 0.0,
            max: 1.0,
            default,
        };
        vec![
            mk(0, "shape", self.cfg.shape as f64),
            // Normalized 0..1 → 0..100 cents.
            mk(
                1,
                "unison_detune",
                (self.cfg.unison_detune_cents / 100.0) as f64,
            ),
            mk(2, "fm_depth", self.cfg.fm_depth as f64),
            mk(3, "ring_mix", self.cfg.ring_mix as f64),
            // 0.5 center → ±24 semitones.
            mk(4, "tune", (self.pitch_mult.log2() * 12.0 / 48.0 + 0.5) as f64),
            // Square pulse width (0.5 symmetric).
            mk(5, "symmetry", self.duty as f64),
            // Harmonia level scale.
            mk(6, "harm_mix", self.harm_mix as f64),
        ]
    }

    fn set_param(&mut self, id: u32, value: f64) {
        // Reuse the render-time param path: with zero-length buffers it only
        // updates state (no rendering, no allocation).
        let params = [(id, value)];
        let events = PluginEvents {
            params: &params,
            midi: &[],
            note_expressions: &[],
        };
        self.render(&[], &[], &mut [], &mut [], &events);
    }

    fn prepare(&mut self, sample_rate: f32, _block_size: usize) {
        let new_sr = sample_rate.max(1.0);
        if (new_sr - self.sample_rate).abs() > f32::EPSILON {
            let ratio = self.sample_rate / new_sr;
            for v in &mut self.voices {
                v.base_inc *= ratio;
                v.fm_inc *= ratio;
                v.ring_inc *= ratio;
                for s in &mut v.subs {
                    s.inc *= ratio;
                    s.base_inc *= ratio;
                    s.drift_inc *= ratio;
                }
                v.env.set_sample_rate(new_sr);
            }
            self.sample_rate = new_sr;
        }
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        // Inherent method wins over the trait method of the same name.
        NativeWavetable::note_on(self, note, velocity);
    }

    fn note_off(&mut self, note: u8) {
        NativeWavetable::note_off(self, note);
    }

    fn render(
        &mut self,
        _in_l: &[f32],
        _in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) {
        for &(id, value) in events.params {
            let v = (value as f32).clamp(0.0, 1.0);
            match id {
                0 => {
                    self.cfg.shape = v;
                    for voice in &mut self.voices {
                        // Unison subs follow the layer shape; harmonia keep theirs.
                        let n = self.cfg.unison_voices.clamp(1, 8) as usize;
                        for s in voice.subs.iter_mut().take(n) {
                            s.shape = v;
                        }
                    }
                }
                1 => self.cfg.unison_detune_cents = v * 100.0,
                2 => self.cfg.fm_depth = v,
                3 => self.cfg.ring_mix = v,
                4 => self.pitch_mult = 2f32.powf((v - 0.5) * 48.0 / 12.0),
                5 => self.duty = v,
                6 => self.harm_mix = v,
                _ => {}
            }
        }
        for ev in events.midi {
            self.apply_midi(&ev.message);
        }
        let frames = out_l.len().min(out_r.len());
        let fm_index = self.cfg.fm_depth * 4.0; // phase-mod index scale
        let fm_shape = self.cfg.fm_shape;
        let ring_mix = self.cfg.ring_mix;
        // Drift: block-rate pitch wander per sub (slow, so block-rate is fine).
        for v in &mut self.voices {
            for s in &mut v.subs {
                if s.drift_inc > 0.0 {
                    s.drift_phase = (s.drift_phase + s.drift_inc * frames as f32).fract();
                    let cents = (core::f32::consts::TAU * s.drift_phase).sin() * s.drift_cents;
                    s.inc = s.base_inc * 2f32.powf(cents / 1200.0);
                }
            }
        }
        for f in 0..frames {
            let (mut sl, mut sr) = (0.0f32, 0.0f32);
            for v in &mut self.voices {
                let e = v.env.tick() * v.amp;
                if e == 0.0 {
                    continue;
                }
                // FM: one modulator per note phase-offsets every sub. The
                // modulator is itself a morphing wave (fm_shape; 0 = sine).
                let pm = if fm_index > 0.0 {
                    let m = morph(v.fm_phase, v.fm_inc, fm_shape, 0.5);
                    v.fm_phase = (v.fm_phase + v.fm_inc).fract();
                    m * fm_index * 0.15
                } else {
                    0.0
                };
                let (mut vl, mut vr) = (0.0f32, 0.0f32);
                let n_unison = self.cfg.unison_voices.clamp(1, 8) as usize;
                for (si, s) in v.subs.iter_mut().enumerate() {
                    let inc = s.inc * self.pitch_mult;
                    let ph = (s.phase + pm).rem_euclid(1.0);
                    // Harmonia subs (past the unison set) scale by harm_mix.
                    let lvl = if si >= n_unison {
                        s.level * self.harm_mix
                    } else {
                        s.level
                    };
                    let smp = morph(ph, inc, s.shape, self.duty) * lvl;
                    vl += smp * s.gain_l;
                    vr += smp * s.gain_r;
                    s.phase += inc;
                    if s.phase >= 1.0 {
                        s.phase -= 1.0;
                    }
                }
                // Ring mod: key-tracked carrier.
                if ring_mix > 0.0 {
                    let carrier = (core::f32::consts::TAU * v.ring_phase).sin();
                    v.ring_phase = (v.ring_phase + v.ring_inc).fract();
                    let g = (1.0 - ring_mix) + ring_mix * carrier;
                    vl *= g;
                    vr *= g;
                }
                sl += vl * e;
                sr += vr * e;
            }
            out_l[f] = sl;
            out_r[f] = sr;
        }
        self.voices.retain(|v| !v.env.is_idle());
    }

    fn reset(&mut self) {
        self.voices.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use signal_plugin_host::PluginMidiEvent;

    fn ev_note_on(note: u8, vel: u8) -> midicore::MidiEvent {
        use midicore::{Channel, KeyNumber, MidiEvent, Velocity};
        MidiEvent::NoteOn {
            channel: Channel::new(0),
            key: KeyNumber::new(note),
            velocity: Velocity::new(vel),
        }
    }

    fn render_cfg(cfg: SynthConfig, n: usize) -> (Vec<f32>, Vec<f32>) {
        let mut osc = NativeWavetable::new(48_000).with_config(cfg);
        Soundsource::prepare(&mut osc, 48_000.0, n);
        let (inl, inr) = (vec![0.0; n], vec![0.0; n]);
        let (mut outl, mut outr) = (vec![0.0; n], vec![0.0; n]);
        let midi = [PluginMidiEvent {
            offset: 0,
            message: ev_note_on(69, 100),
        }];
        let ev = PluginEvents {
            params: &[],
            midi: &midi,
            note_expressions: &[],
        };
        osc.render(&inl, &inr, &mut outl, &mut outr, &ev);
        (outl, outr)
    }

    fn rms(b: &[f32]) -> f32 {
        (b.iter().map(|s| s * s).sum::<f32>() / b.len() as f32).sqrt()
    }

    fn hf(b: &[f32]) -> f32 {
        let d: Vec<f32> = b.windows(2).map(|w| w[1] - w[0]).collect();
        rms(&d)
    }

    #[test]
    fn all_shapes_are_audible() {
        for shape in [0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0] {
            let (l, _) = render_cfg(
                SynthConfig {
                    shape,
                    ..Default::default()
                },
                4_096,
            );
            assert!(rms(&l) > 1e-3, "shape {shape} audible");
        }
    }

    #[test]
    fn saw_is_brighter_than_sine() {
        let (sine, _) = render_cfg(
            SynthConfig {
                shape: 0.0,
                ..Default::default()
            },
            4_096,
        );
        let (saw, _) = render_cfg(
            SynthConfig {
                shape: 2.0 / 3.0,
                ..Default::default()
            },
            4_096,
        );
        assert!(hf(&saw) > hf(&sine) * 2.0, "saw carries harmonics");
    }

    #[test]
    fn unison_widens_the_stereo_image() {
        let mono = SynthConfig {
            unison_voices: 1,
            ..Default::default()
        };
        let wide = SynthConfig {
            unison_voices: 6,
            unison_detune_cents: 30.0,
            unison_width: 1.0,
            ..Default::default()
        };
        let (ml, mr) = render_cfg(mono, 8_192);
        let (wl, wr) = render_cfg(wide, 8_192);
        // Side signal = L−R. Mono unison has none; wide unison plenty.
        let side_mono: Vec<f32> = ml.iter().zip(&mr).map(|(l, r)| l - r).collect();
        let side_wide: Vec<f32> = wl.iter().zip(&wr).map(|(l, r)| l - r).collect();
        assert!(rms(&side_mono) < 1e-4, "single voice is centered");
        assert!(
            rms(&side_wide) > rms(&ml) * 0.1,
            "unison spread produces side energy: side={} mid={}",
            rms(&side_wide),
            rms(&wl)
        );
    }

    #[test]
    fn harmonia_adds_the_interval_voice() {
        let mut cfg = SynthConfig {
            shape: 0.0,
            ..Default::default()
        };
        let (base, _) = render_cfg(cfg, 8_192);
        cfg.harmonia[0] = HarmVoice {
            on: true,
            level: 1.0,
            interval_semi: 12.0,
            pan: 0.0,
            shape: 0.0,
        };
        let (with, _) = render_cfg(cfg, 8_192);
        // The octave-up sine roughly doubles the high-frequency content.
        assert!(
            hf(&with) > hf(&base) * 1.5,
            "harmonia octave voice audible: base hf={} with hf={}",
            hf(&base),
            hf(&with)
        );
    }

    #[test]
    fn fm_brightens_a_sine() {
        let plain = SynthConfig {
            shape: 0.0,
            ..Default::default()
        };
        let fm = SynthConfig {
            shape: 0.0,
            fm_depth: 0.8,
            fm_ratio: 2.0,
            ..Default::default()
        };
        let (p, _) = render_cfg(plain, 8_192);
        let (m, _) = render_cfg(fm, 8_192);
        assert!(
            hf(&m) > hf(&p) * 1.5,
            "FM adds sidebands: plain hf={} fm hf={}",
            hf(&p),
            hf(&m)
        );
    }

    #[test]
    fn ring_mod_reshapes_the_spectrum() {
        let plain = SynthConfig {
            shape: 0.0,
            ..Default::default()
        };
        let ring = SynthConfig {
            shape: 0.0,
            ring_mix: 1.0,
            ring_ratio: 1.5,
            ..Default::default()
        };
        let (p, _) = render_cfg(plain, 8_192);
        let (r, _) = render_cfg(ring, 8_192);
        assert!(rms(&r) > 1e-3, "ring-modulated output audible");
        // Full ring mod at a non-integer ratio changes the waveform shape.
        let diff: Vec<f32> = p.iter().zip(&r).map(|(a, b)| a - b).collect();
        assert!(rms(&diff) > rms(&p) * 0.3, "spectrum audibly reshaped");
    }

    #[test]
    fn square_is_band_limited_enough() {
        let cfg = SynthConfig {
            shape: 1.0,
            ..Default::default()
        };
        let mut osc = NativeWavetable::new(48_000).with_config(cfg);
        Soundsource::prepare(&mut osc, 48_000.0, 2_048);
        let (inl, inr) = (vec![0.0; 2_048], vec![0.0; 2_048]);
        let (mut outl, mut outr) = (vec![0.0; 2_048], vec![0.0; 2_048]);
        let midi = [PluginMidiEvent {
            offset: 0,
            message: ev_note_on(108, 100),
        }];
        let ev = PluginEvents {
            params: &[],
            midi: &midi,
            note_expressions: &[],
        };
        osc.render(&inl, &inr, &mut outl, &mut outr, &ev);
        let peak = outl.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak < 0.4, "bounded output at C8, peak={peak}");
    }
}
