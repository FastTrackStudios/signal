//! Native **Wavetable** synth voice — the `Native` implementation of
//! `BlockType::Wavetable` (Omnisphere "Synth mode", Nord synth waves), grown
//! into the full oscillator stack:
//!
//! - **Morphing wave** — `shape` 0..1 crossfades sine → triangle → saw →
//!   square (`PolyBLEP` band-limited). Stands in for the 638 Omnisphere
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

/// The envelope time range the normalized ADSR params span (seconds).
const ENV_RANGE_S: f32 = 8.0;

use super::adsr::{Adsr, AdsrParams};
use crate::soundsource::{Soundsource, SoundsourceKind};

/// `PolyBLEP` residual for a discontinuity at phase 0 (t in 0..1, dt = inc).
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

/// One of the morph's four band-limited waves: 0 sine, 1 triangle, 2 saw,
/// 3 square (`duty` its pulse width).
#[inline]
fn wave(k: usize, phase: f32, dt: f32, duty: f32) -> f32 {
    match k {
        0 => (core::f32::consts::TAU * phase).sin(),
        1 => 4.0 * (phase - 0.5).abs() - 1.0,
        2 => 2.0 * phase - 1.0 - poly_blep(phase, dt),
        _ => {
            let duty = duty.clamp(0.05, 0.95);
            let mut sq = if phase < duty { 1.0 } else { -1.0 };
            sq += poly_blep(phase, dt);
            sq -= poly_blep((phase + (1.0 - duty)).fract(), dt);
            sq
        }
    }
}

/// Morph 0..1 across sine → triangle → saw → square, band-limited.
/// `duty` is the square's pulse width (0.5 = symmetric; the Symmetry/PWM axis).
///
/// Only the two waves either side of `shape` are computed — it used to
/// compute all four every sample, a `sin` included, and throw two away (a
/// saw-shaped bass paid for a sine per sub-oscillator per sample it never
/// used: a fifth of a keys rig's audio time on one lane).
#[inline]
fn morph(phase: f32, dt: f32, shape: f32, duty: f32) -> f32 {
    let x = shape.clamp(0.0, 1.0) * 3.0;
    let i = (x as usize).min(2);
    let frac = x - i as f32;
    wave(i, phase, dt, duty) * (1.0 - frac) + wave(i + 1, phase, dt, duty) * frac
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
    /// Drift 0..1: slow per-voice pitch wander (±60 cents at full, sub-Hz
    /// rates; Omnisphere's coherent stack decorrelates in ~1 s at 0.1).
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
    /// Hard sync: the slave's phase, reset each master cycle.
    sync_phase: f32,
}

fn pan_gains(pan: f32) -> (f32, f32) {
    // pan −1..+1 → equal-power.
    let x = (pan.clamp(-1.0, 1.0) + 1.0) * 0.25 * core::f32::consts::PI;
    (x.cos(), x.sin())
}

/// A voice's amp envelope: the ADSR, or an imported breakpoint envelope.
enum VoiceEnv {
    Adsr(Adsr),
    Points {
        bp: std::sync::Arc<super::breakpoints::Breakpoints>,
        player: super::breakpoints::EnvPlayer,
        dt: f32,
    },
}

impl VoiceEnv {
    fn note_on(&mut self) {
        match self {
            Self::Adsr(a) => a.note_on(),
            Self::Points { player, .. } => player.note_on(),
        }
    }
    fn note_off(&mut self) {
        match self {
            Self::Adsr(a) => a.note_off(),
            Self::Points { bp, player, .. } => player.note_off(bp),
        }
    }
    #[inline]
    fn tick(&mut self) -> f32 {
        match self {
            Self::Adsr(a) => a.tick(),
            Self::Points { bp, player, dt } => player.tick(bp, *dt),
        }
    }
    fn is_idle(&self) -> bool {
        match self {
            Self::Adsr(a) => a.is_idle(),
            Self::Points { bp, player, .. } => player.is_idle(bp),
        }
    }
    /// Live ADSR edits (a breakpoint envelope keeps its imported shape).
    fn set_params(&mut self, sr: f32, p: AdsrParams) {
        if let Self::Adsr(a) = self {
            a.set_params(sr, p);
        }
    }
    fn set_sample_rate(&mut self, sr: f32) {
        match self {
            Self::Adsr(a) => a.set_sample_rate(sr),
            Self::Points { dt, .. } => *dt = 1.0 / sr.max(1.0),
        }
    }
}

struct Voice {
    note: u8,
    amp: f32,
    env: VoiceEnv,
    subs: Vec<Sub>,
    fm_phase: f32,
    fm_inc: f32,
    ring_phase: f32,
    ring_inc: f32,
    /// Base frequency / sample-rate (for runtime detune recompute).
    base_inc: f32,
    /// Glide: the offset (semitones) it starts from and its progress 0..1.
    glide_semis: f32,
    glide_pos: f32,
}

/// The polyphonic synth-voice oscillator.
pub struct NativeWavetable {
    sample_rate: f32,
    cfg: SynthConfig,
    /// Real wavetables (Omnisphere's wavetable oscillator): the two waves a
    /// patch names, crossfaded by `wt_mix` and scanned by `wt_position`.
    /// `None` = the generated classic shapes.
    waves: Option<[std::sync::Arc<super::wavebank::WaveBank>; 2]>,
    wt_position: f32,
    wt_mix: f32,
    /// Runtime pitch multiplier (param 4 "tune": 0.5 center, ±24 semitones).
    pitch_mult: f32,
    /// A fixed transposition (a patch's octave/semitone/tune offsets), any
    /// size, on top of the live `tune`.
    transpose_mult: f32,
    /// Glide (portamento): each new note starts at the last note's pitch
    /// and arrives over `glide_s` along Omnisphere's measured curve (see
    /// [`glide_remaining`]). 0 = off.
    glide_s: f32,
    last_note: Option<u8>,
    /// An imported breakpoint amp envelope (replaces the ADSR).
    amp_points: Option<std::sync::Arc<super::breakpoints::Breakpoints>>,
    /// Hard-sync ratio (1 = off).
    sync_ratio: f32,
    /// Velocity sensitivity of the amplitude, `None` = linear in velocity
    /// (the generic voice); `Some(s)` = Omnisphere's measured law,
    /// `1 − s + s·(vel/127)²`.
    vel_sens: Option<f32>,
    /// Vibrato: rate (Hz), depth (0..1 → up to 50 cents), phase.
    vib_rate: f32,
    vib_depth: f32,
    vib_phase: f32,
    /// Square pulse width (param 5 "symmetry": 0.5 = symmetric).
    duty: f32,
    /// Harmonia level scale (param 6 "`harm_mix`").
    harm_mix: f32,
    voices: Vec<Voice>,
}

impl NativeWavetable {
    #[must_use]
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate: sample_rate.max(1) as f32,
            cfg: SynthConfig::default(),
            waves: None,
            wt_position: 0.0,
            wt_mix: 0.0,
            pitch_mult: 1.0,
            transpose_mult: 1.0,
            glide_s: 0.0,
            last_note: None,
            amp_points: None,
            vel_sens: None,
            sync_ratio: 1.0,
            vib_rate: 5.0,
            vib_depth: 0.0,
            vib_phase: 0.0,
            duty: 0.5,
            harm_mix: 1.0,
            voices: Vec::new(),
        }
    }

    /// Shape every note with a breakpoint amp envelope (sustain at the
    /// penultimate point) instead of the ADSR.
    #[must_use]
    pub fn with_amp_points(mut self, points: Vec<super::breakpoints::SegPoint>) -> Self {
        if points.len() >= 2 {
            self.amp_points = Some(std::sync::Arc::new(
                super::breakpoints::Breakpoints::with_penultimate_sustain(points),
            ));
        }
        self
    }

    /// The amp envelope's times are beats (it follows the tempo).
    #[must_use]
    pub fn with_amp_sync(mut self, synced: bool) -> Self {
        if let Some(bp) = self.amp_points.take() {
            self.amp_points = Some(std::sync::Arc::new((*bp).clone().with_synced(synced)));
        }
        self
    }

    /// Omnisphere's amp velocity law (see `vel_sens`).
    #[must_use]
    pub fn with_velocity_sensitivity(mut self, sens: f32) -> Self {
        self.vel_sens = Some(sens.clamp(0.0, 1.0));
        self
    }

    /// Hard sync: the played wave runs at `ratio` × the note (≥ 1) and
    /// restarts every note cycle — Omnisphere's `hrdsnc`.
    #[must_use]
    pub fn with_sync_ratio(mut self, ratio: f32) -> Self {
        self.sync_ratio = ratio.max(1.0);
        self
    }

    /// Glide into each note over `seconds` (0 = off).
    #[must_use]
    pub fn with_glide(mut self, seconds: f32) -> Self {
        self.glide_s = seconds.max(0.0);
        self
    }

    /// Transpose by `semitones` (fixed; the live `tune` rides on top).
    #[must_use]
    pub fn with_transpose(mut self, semitones: f32) -> Self {
        self.transpose_mult = 2f32.powf(semitones / 12.0);
        self
    }

    /// Play real wavetables instead of the generated shapes: `a` and `b`
    /// crossfaded by `mix` (0 = all `a`), scanned by `position` (0..1 over
    /// the frames). Both live-settable (params 13, 14).
    #[must_use]
    pub fn with_waves(
        mut self,
        a: std::sync::Arc<super::wavebank::WaveBank>,
        b: std::sync::Arc<super::wavebank::WaveBank>,
        position: f32,
        mix: f32,
    ) -> Self {
        self.waves = Some([a, b]);
        self.wt_position = position.clamp(0.0, 1.0);
        self.wt_mix = mix.clamp(0.0, 1.0);
        self
    }

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

    #[must_use]
    pub fn config(&self) -> &SynthConfig {
        &self.cfg
    }

    #[must_use]
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
                (rate / self.sample_rate, self.cfg.unison_drift * 60.0)
            } else {
                (0.0, 0.0)
            };
            subs.push(Sub {
                // In phase: Omnisphere's unison starts coherent (measured: an
                // undetuned, drifting stack starts loud and settles as the
                // voices drift apart).
                phase: 0.0,
                inc,
                base_inc: inc,
                drift_phase: jitter(i.wrapping_add(11)) * 0.5 + 0.5,
                drift_inc,
                drift_cents,
                gain_l,
                gain_r,
                level: comp,
                shape: self.cfg.shape,
                sync_phase: 0.0,
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
                sync_phase: 0.0,
            });
        }
        subs
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        if velocity == 0 {
            return self.note_off(note);
        }
        let freq = 440.0 * 2f32.powf((note as f32 - 69.0) / 12.0);
        let v = velocity as f32 / 127.0;
        let amp = 0.15
            * match self.vel_sens {
                Some(s) => 1.0 - s + s * v * v,
                None => v,
            };
        let base_inc = freq / self.sample_rate;
        let glide_semis = match self.last_note {
            Some(prev) if self.glide_s > 0.0 && prev != note => f32::from(prev) - f32::from(note),
            _ => 0.0,
        };
        self.last_note = Some(note);
        if let Some(v) = self.voices.iter_mut().find(|v| v.note == note) {
            v.amp = amp;
            v.env.note_on();
            v.glide_semis = glide_semis;
            v.glide_pos = 0.0;
        } else {
            let mut env = match &self.amp_points {
                Some(bp) => VoiceEnv::Points {
                    bp: bp.clone(),
                    player: super::breakpoints::EnvPlayer::default(),
                    dt: 1.0 / self.sample_rate,
                },
                None => VoiceEnv::Adsr(Adsr::new(self.sample_rate, self.cfg.env)),
            };
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
                glide_semis,
                glide_pos: 0.0,
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

/// The share of a glide still to go at progress `x` (0..1): Omnisphere's
/// curve, measured — fast at first, arriving on time:
/// `(e^(−kx) − e^(−k)) / (1 − e^(−k))`, k = 4.5 (fits glides of 0.95 s and
/// 1.75 s within ~30 cents).
#[inline]
fn glide_remaining(x: f32) -> f32 {
    const K: f32 = 4.5;
    let end = (-K).exp();
    (((-K * x).exp() - end) / (1.0 - end)).max(0.0)
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
            mk(
                4,
                "tune",
                (self.pitch_mult.log2() * 12.0 / 48.0 + 0.5) as f64,
            ),
            // Square pulse width (0.5 symmetric).
            mk(5, "symmetry", self.duty as f64),
            // Harmonia level scale.
            mk(6, "harm_mix", self.harm_mix as f64),
            // The amp ADSR: times over an 8 s range (the native oscillator's
            // scale, so the rig's envelope controls drive both alike),
            // sustain 0..1. Held notes follow at once.
            mk(
                7,
                "amp_attack",
                (self.cfg.env.attack_s / ENV_RANGE_S) as f64,
            ),
            mk(8, "amp_decay", (self.cfg.env.decay_s / ENV_RANGE_S) as f64),
            mk(9, "amp_sustain", self.cfg.env.sustain as f64),
            mk(
                10,
                "amp_release",
                (self.cfg.env.release_s / ENV_RANGE_S) as f64,
            ),
            // Vibrato: rate over 0..12 Hz (the oscillator's scale), depth.
            mk(11, "vib_rate", (self.vib_rate / 12.0) as f64),
            mk(12, "vib_depth", self.vib_depth as f64),
            // Wavetable scan position and the wave-A/B crossfade.
            mk(13, "wt_position", self.wt_position as f64),
            mk(14, "wt_mix", self.wt_mix as f64),
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
        Self::note_on(self, note, velocity);
    }

    fn note_off(&mut self, note: u8) {
        Self::note_off(self, note);
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
                11 => self.vib_rate = (v * 12.0).max(0.05),
                12 => self.vib_depth = v,
                13 => self.wt_position = v,
                14 => self.wt_mix = v,
                7..=10 => {
                    match id {
                        7 => self.cfg.env.attack_s = v * ENV_RANGE_S,
                        8 => self.cfg.env.decay_s = v * ENV_RANGE_S,
                        9 => self.cfg.env.sustain = v,
                        _ => self.cfg.env.release_s = v * ENV_RANGE_S,
                    }
                    for voice in &mut self.voices {
                        voice.env.set_params(self.sample_rate, self.cfg.env);
                    }
                }
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
        let vib_inc = self.vib_rate / self.sample_rate;
        for f in 0..frames {
            let (mut sl, mut sr) = (0.0f32, 0.0f32);
            // Vibrato, shared by the voices: a parabolic sine, cents → ratio.
            let pitch = if self.vib_depth > 0.0 {
                self.vib_phase = (self.vib_phase + vib_inc).fract();
                let x = self.vib_phase * 2.0 - 1.0;
                let sine = 4.0 * x * (1.0 - x.abs());
                self.pitch_mult
                    * self.transpose_mult
                    * (1.0 + sine * self.vib_depth * 50.0 * (std::f32::consts::LN_2 / 1200.0))
            } else {
                self.pitch_mult * self.transpose_mult
            };
            let glide_step = if self.glide_s > 0.0 {
                1.0 / (self.glide_s * self.sample_rate)
            } else {
                1.0
            };
            for v in &mut self.voices {
                let e = v.env.tick() * v.amp;
                if e == 0.0 {
                    continue;
                }
                let pitch = if v.glide_semis != 0.0 && v.glide_pos < 1.0 {
                    let off = v.glide_semis * glide_remaining(v.glide_pos);
                    v.glide_pos += glide_step;
                    pitch * (off * (std::f32::consts::LN_2 / 12.0)).exp()
                } else {
                    pitch
                };
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
                let sync = self.sync_ratio;
                for (si, s) in v.subs.iter_mut().enumerate() {
                    let master_inc = s.inc * pitch;
                    // Hard sync reads a slave running `sync`× the note,
                    // restarted by the master's cycle (band-limited for the
                    // slave's rate).
                    let (inc, ph) = if sync > 1.0 {
                        (master_inc * sync, (s.sync_phase + pm).rem_euclid(1.0))
                    } else {
                        (master_inc, (s.phase + pm).rem_euclid(1.0))
                    };
                    // Harmonia subs (past the unison set) scale by harm_mix.
                    let lvl = if si >= n_unison {
                        s.level * self.harm_mix
                    } else {
                        s.level
                    };
                    let smp = match &self.waves {
                        Some([a, b]) => {
                            let x = a.sample(self.wt_position, ph, inc);
                            let y = if self.wt_mix > 0.0 {
                                b.sample(self.wt_position, ph, inc)
                            } else {
                                x
                            };
                            x + (y - x) * self.wt_mix
                        }
                        None => morph(ph, inc, s.shape, self.duty),
                    } * lvl;
                    vl += smp * s.gain_l;
                    vr += smp * s.gain_r;
                    s.phase += master_inc;
                    if sync > 1.0 {
                        s.sync_phase += inc;
                        if s.phase >= 1.0 {
                            // Restart the slave where the master cycle lands.
                            s.sync_phase = (s.phase - 1.0) * sync;
                        }
                        s.sync_phase = s.sync_phase.fract();
                    }
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

    /// The morph as it was: all four waves, blended.
    fn morph_all(phase: f32, dt: f32, shape: f32, duty: f32) -> f32 {
        let w = [
            wave(0, phase, dt, duty),
            wave(1, phase, dt, duty),
            wave(2, phase, dt, duty),
            wave(3, phase, dt, duty),
        ];
        let x = shape.clamp(0.0, 1.0) * 3.0;
        let i = (x as usize).min(2);
        let frac = x - i as f32;
        w[i] * (1.0 - frac) + w[i + 1] * frac
    }

    /// Computing only the two waves either side of the shape is the same
    /// sample, bit for bit, as computing all four.
    #[test]
    fn the_morph_computes_only_what_it_blends() {
        for pi in 0..400 {
            let phase = pi as f32 / 400.0;
            for si in 0..=30 {
                let shape = si as f32 / 30.0;
                for duty in [0.1f32, 0.5, 0.9] {
                    for dt in [0.001f32, 0.02] {
                        let (a, b) = (
                            morph(phase, dt, shape, duty),
                            morph_all(phase, dt, shape, duty),
                        );
                        assert_eq!(
                            a.to_bits(),
                            b.to_bits(),
                            "phase {phase} shape {shape} duty {duty} dt {dt}"
                        );
                    }
                }
            }
        }
    }
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
