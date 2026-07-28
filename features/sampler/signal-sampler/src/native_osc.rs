//! Native polyphonic oscillator — a real subtractive **synth voice** per
//! note, not a paraphonic block chain.
//!
//! Each sounding note owns the classic voice path:
//!
//! ```text
//! osc (wave, bend + vibrato) → per-voice SVF lowpass (own Filter Env)
//!                            → per-voice amp (own Amp Env, velocity)
//! ```
//!
//! so a fresh note attacks while an older one releases, each with its own
//! filter sweep — the behavior a synth player expects, and what a shared
//! module-level filter/amp cannot do.
//!
//! **Performance controls** (from the MIDI stream): pitch bend (smooth,
//! `bend_range` semitones) and the mod wheel (CC1), which scales the vibrato
//! LFO's depth. **Parameters** are exposed via [`Soundsource::params`] and
//! applied from `events.params` (normalized 0..1) — the render tree's live
//! overlay ([`RenderNode::set_leaf_param`](crate::node_render::RenderNode))
//! and the mod matrix both reach them; construction reads the same values as
//! raw block params (`amp_attack` ms, `cutoff` normalized, …).

use signal_plugin_host::{PluginDescriptor, PluginEvents, PluginFormat, PluginParamInfo};

use crate::native::{Adsr, AdsrParams, NativeFilter};
use crate::soundsource::{Soundsource, SoundsourceKind};

/// Waveforms the oscillator can produce.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OscWave {
    #[default]
    Sine,
    Saw,
    Square,
    Triangle,
}

impl OscWave {
    fn sample(self, phase: f32) -> f32 {
        match self {
            OscWave::Sine => (phase * core::f32::consts::TAU).sin(),
            OscWave::Saw => 2.0 * phase - 1.0,
            OscWave::Square => {
                if phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            OscWave::Triangle => 4.0 * (phase - 0.5).abs() - 1.0,
        }
    }
}

/// The voice-level parameter set — shared by every voice, editable live.
#[derive(Clone, Copy, Debug)]
struct SynthParams {
    amp: AdsrParams,
    filter: AdsrParams,
    /// Filter cutoff, normalized on [`NativeFilter`]'s log scale.
    cutoff_norm: f32,
    /// SVF resonance 0..1.
    resonance: f32,
    /// How far the Filter Env opens the cutoff (−1..1 of the normalized range).
    filter_env_depth: f32,
    /// Pitch-bend range in semitones (full wheel throw).
    bend_range_st: f32,
    /// Vibrato LFO rate (Hz) and depth (semitones at full mod wheel).
    vib_rate_hz: f32,
    vib_depth_st: f32,
}

impl Default for SynthParams {
    fn default() -> Self {
        Self {
            amp: AdsrParams::default(),
            filter: AdsrParams { attack_s: 0.005, decay_s: 0.3, sustain: 0.7, release_s: 0.2 },
            cutoff_norm: 1.0,
            resonance: 0.0,
            filter_env_depth: 0.0,
            bend_range_st: 2.0,
            vib_rate_hz: 5.0,
            vib_depth_st: 0.5,
        }
    }
}

// Parameter ids — the [`Soundsource::params`] surface. Values arrive
// NORMALIZED 0..1 (the mod-matrix / live-overlay convention) and are mapped
// onto each parameter's range here.
const P_AMP_ATTACK: u32 = 0;
const P_AMP_DECAY: u32 = 1;
const P_AMP_SUSTAIN: u32 = 2;
const P_AMP_RELEASE: u32 = 3;
const P_FLT_ATTACK: u32 = 4;
const P_FLT_DECAY: u32 = 5;
const P_FLT_SUSTAIN: u32 = 6;
const P_FLT_RELEASE: u32 = 7;
const P_CUTOFF: u32 = 8;
const P_RESONANCE: u32 = 9;
const P_ENV_AMT: u32 = 10;
const P_BEND_RANGE: u32 = 11;
const P_VIB_RATE: u32 = 12;
const P_VIB_DEPTH: u32 = 13;

/// Longest envelope segment the normalized params map onto (seconds).
const MAX_SEG_S: f32 = 8.0;
/// Bend-range ceiling (semitones).
const MAX_BEND_ST: f32 = 24.0;
/// Vibrato ranges.
const MAX_VIB_HZ: f32 = 12.0;
const MAX_VIB_ST: f32 = 1.0;

/// One sounding note — its own oscillator phase, filter and envelopes.
struct Voice {
    note: u8,
    phase: f32,
    amp: f32,
    amp_env: Adsr,
    filter_env: Adsr,
    /// Chamberlin SVF state (lowpass output taken).
    svf_low: f32,
    svf_band: f32,
}

impl Voice {
    fn new(note: u8, amp: f32, sample_rate: f32, p: &SynthParams) -> Self {
        let mut amp_env = Adsr::new(sample_rate, p.amp);
        amp_env.note_on();
        let mut filter_env = Adsr::new(sample_rate, p.filter);
        filter_env.note_on();
        Self {
            note,
            phase: 0.0,
            amp,
            amp_env,
            filter_env,
            svf_low: 0.0,
            svf_band: 0.0,
        }
    }
}

/// A polyphonic native oscillator with true per-voice envelopes + filters.
pub struct NativeOscillator {
    sample_rate: f32,
    wave: OscWave,
    params: SynthParams,
    voices: Vec<Voice>,
    /// Current pitch bend in semitones (already range-scaled).
    bend_st: f32,
    /// Mod wheel 0..1 — scales the vibrato depth.
    wheel: f32,
    /// Shared vibrato LFO phase (0..1).
    vib_phase: f32,
}

impl NativeOscillator {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate: sample_rate.max(1) as f32,
            wave: OscWave::Sine,
            params: SynthParams::default(),
            voices: Vec::new(),
            bend_st: 0.0,
            wheel: 0.0,
            vib_phase: 0.0,
        }
    }

    #[must_use]
    pub fn with_wave(mut self, wave: OscWave) -> Self {
        self.wave = wave;
        self
    }

    /// Apply raw build-time block params (`amp_attack` ms, `cutoff`
    /// normalized, …) — the construction-side twin of the normalized live
    /// parameter surface.
    #[must_use]
    pub fn with_block_params(mut self, block: &crate::rig::RigBlock) -> Self {
        let p = &mut self.params;
        let ms = |v: f32| (v / 1000.0).max(0.0);
        if let Some(v) = block.param_f32("amp_attack") {
            p.amp.attack_s = ms(v);
        }
        if let Some(v) = block.param_f32("amp_decay") {
            p.amp.decay_s = ms(v);
        }
        if let Some(v) = block.param_f32("amp_sustain") {
            p.amp.sustain = v.clamp(0.0, 1.0);
        }
        if let Some(v) = block.param_f32("amp_release") {
            p.amp.release_s = ms(v);
        }
        if let Some(v) = block.param_f32("filter_attack") {
            p.filter.attack_s = ms(v);
        }
        if let Some(v) = block.param_f32("filter_decay") {
            p.filter.decay_s = ms(v);
        }
        if let Some(v) = block.param_f32("filter_sustain") {
            p.filter.sustain = v.clamp(0.0, 1.0);
        }
        if let Some(v) = block.param_f32("filter_release") {
            p.filter.release_s = ms(v);
        }
        if let Some(v) = block.param_f32("cutoff") {
            p.cutoff_norm = v.clamp(0.0, 1.0);
        }
        if let Some(v) = block.param_f32("resonance") {
            p.resonance = v.clamp(0.0, 1.0);
        }
        if let Some(v) = block.param_f32("env_amt") {
            p.filter_env_depth = v.clamp(-1.0, 1.0);
        }
        if let Some(v) = block.param_f32("bend_range") {
            p.bend_range_st = v.clamp(0.0, MAX_BEND_ST);
        }
        if let Some(v) = block.param_f32("vib_rate") {
            p.vib_rate_hz = v.clamp(0.1, MAX_VIB_HZ);
        }
        if let Some(v) = block.param_f32("vib_depth") {
            p.vib_depth_st = v.clamp(0.0, MAX_VIB_ST);
        }
        self
    }

    /// Number of sounding voices (for tests / metering).
    pub fn active_voices(&self) -> usize {
        self.voices.len()
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        if velocity == 0 {
            return self.note_off(note);
        }
        // Keep total level sane under polyphony.
        let amp = (velocity as f32 / 127.0) * 0.15;
        // Retrigger if the note is already held (envelopes continue from
        // their current level, so no click).
        if let Some(v) = self.voices.iter_mut().find(|v| v.note == note) {
            v.amp = amp;
            v.amp_env.note_on();
            v.filter_env.note_on();
        } else {
            self.voices
                .push(Voice::new(note, amp, self.sample_rate, &self.params));
        }
    }

    fn note_off(&mut self, note: u8) {
        // Enter the release tail; the render loop drops the voice at idle.
        for v in self.voices.iter_mut().filter(|v| v.note == note) {
            v.amp_env.note_off();
            v.filter_env.note_off();
        }
    }

    fn apply_midi(&mut self, message: &midicore::MidiEvent) {
        use midicore::MidiEvent;
        match message {
            MidiEvent::NoteOn { key, velocity, .. } => self.note_on(key.get(), velocity.get()),
            MidiEvent::NoteOff { key, .. } => self.note_off(key.get()),
            // Pitch wheel: 14-bit, centered — scaled by the bend range.
            MidiEvent::PitchBend { bend, .. } => {
                let norm = (bend.get() as f32 - 8192.0) / 8192.0;
                self.bend_st = norm.clamp(-1.0, 1.0) * self.params.bend_range_st;
            }
            // Mod wheel: vibrato depth.
            MidiEvent::ControlChange { controller, value, .. } if controller.get() == 1 => {
                self.wheel = value.get() as f32 / 127.0;
            }
            _ => {}
        }
    }

    /// Apply one normalized (0..1) parameter write.
    fn apply_param(&mut self, id: u32, v: f64) {
        let v = v.clamp(0.0, 1.0) as f32;
        let p = &mut self.params;
        let mut env_changed = false;
        match id {
            P_AMP_ATTACK => {
                p.amp.attack_s = v * MAX_SEG_S;
                env_changed = true;
            }
            P_AMP_DECAY => {
                p.amp.decay_s = v * MAX_SEG_S;
                env_changed = true;
            }
            P_AMP_SUSTAIN => {
                p.amp.sustain = v;
                env_changed = true;
            }
            P_AMP_RELEASE => {
                p.amp.release_s = v * MAX_SEG_S;
                env_changed = true;
            }
            P_FLT_ATTACK => {
                p.filter.attack_s = v * MAX_SEG_S;
                env_changed = true;
            }
            P_FLT_DECAY => {
                p.filter.decay_s = v * MAX_SEG_S;
                env_changed = true;
            }
            P_FLT_SUSTAIN => {
                p.filter.sustain = v;
                env_changed = true;
            }
            P_FLT_RELEASE => {
                p.filter.release_s = v * MAX_SEG_S;
                env_changed = true;
            }
            P_CUTOFF => p.cutoff_norm = v,
            P_RESONANCE => p.resonance = v,
            P_ENV_AMT => p.filter_env_depth = v * 2.0 - 1.0,
            P_BEND_RANGE => p.bend_range_st = v * MAX_BEND_ST,
            P_VIB_RATE => p.vib_rate_hz = (v * MAX_VIB_HZ).max(0.1),
            P_VIB_DEPTH => p.vib_depth_st = v * MAX_VIB_ST,
            _ => {}
        }
        if env_changed {
            // Live envelopes take the new times from here on (stage and
            // level survive — a held pad glides into the new shape).
            for voice in &mut self.voices {
                voice.amp_env.set_params(self.sample_rate, p.amp);
                voice.filter_env.set_params(self.sample_rate, p.filter);
            }
        }
    }

    fn info(id: u32, name: &'static str, default: f64) -> PluginParamInfo {
        PluginParamInfo {
            id,
            name: name.to_string(),
            min: 0.0,
            max: 1.0,
            default,
        }
    }
}

// r[impl signal.soundsource.oscillator]
impl Soundsource for NativeOscillator {
    fn kind(&self) -> SoundsourceKind {
        SoundsourceKind::Oscillator
    }

    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "signal.native.oscillator".into(),
            name: "Oscillator".into(),
            vendor: "Signal".into(),
            version: String::new(),
            format: PluginFormat::Synthetic,
        }
    }

    fn params(&self) -> Vec<PluginParamInfo> {
        let p = &self.params;
        vec![
            Self::info(P_AMP_ATTACK, "amp_attack", (p.amp.attack_s / MAX_SEG_S) as f64),
            Self::info(P_AMP_DECAY, "amp_decay", (p.amp.decay_s / MAX_SEG_S) as f64),
            Self::info(P_AMP_SUSTAIN, "amp_sustain", p.amp.sustain as f64),
            Self::info(P_AMP_RELEASE, "amp_release", (p.amp.release_s / MAX_SEG_S) as f64),
            Self::info(P_FLT_ATTACK, "filter_attack", (p.filter.attack_s / MAX_SEG_S) as f64),
            Self::info(P_FLT_DECAY, "filter_decay", (p.filter.decay_s / MAX_SEG_S) as f64),
            Self::info(P_FLT_SUSTAIN, "filter_sustain", p.filter.sustain as f64),
            Self::info(P_FLT_RELEASE, "filter_release", (p.filter.release_s / MAX_SEG_S) as f64),
            Self::info(P_CUTOFF, "cutoff", p.cutoff_norm as f64),
            Self::info(P_RESONANCE, "resonance", p.resonance as f64),
            Self::info(P_ENV_AMT, "env_amt", ((p.filter_env_depth + 1.0) / 2.0) as f64),
            Self::info(P_BEND_RANGE, "bend_range", (p.bend_range_st / MAX_BEND_ST) as f64),
            Self::info(P_VIB_RATE, "vib_rate", (p.vib_rate_hz / MAX_VIB_HZ) as f64),
            Self::info(P_VIB_DEPTH, "vib_depth", (p.vib_depth_st / MAX_VIB_ST) as f64),
        ]
    }

    fn set_param(&mut self, id: u32, value: f64) {
        self.apply_param(id, value);
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    fn prepare(&mut self, sample_rate: f32, _block_size: usize) {
        let new_sr = sample_rate.max(1.0);
        if (new_sr - self.sample_rate).abs() > f32::EPSILON {
            self.sample_rate = new_sr;
            for v in &mut self.voices {
                v.amp_env.set_sample_rate(new_sr);
                v.filter_env.set_sample_rate(new_sr);
            }
        }
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        // Inherent `NativeOscillator::note_on` (inherent methods win over the
        // trait method of the same name, so this is not a recursion).
        NativeOscillator::note_on(self, note, velocity);
    }

    fn note_off(&mut self, note: u8) {
        NativeOscillator::note_off(self, note);
    }

    fn render(
        &mut self,
        _in_l: &[f32],
        _in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) {
        // Parameter writes (live overlay / mod matrix) land first, then this
        // block's MIDI (notes, wheels).
        for &(id, value) in events.params {
            self.apply_param(id, value);
        }
        for ev in events.midi {
            self.apply_midi(&ev.message);
        }

        let frames = out_l.len().min(out_r.len());
        let sr = self.sample_rate;
        let p = self.params;
        let vib_inc = p.vib_rate_hz / sr;
        // SVF drive from resonance: q from ~0.9 (none) down toward 0.08.
        let q = (1.0 - p.resonance.clamp(0.0, 0.98) * 0.92).max(0.08);

        for f in 0..frames {
            // Shared vibrato LFO, depth scaled by the wheel.
            let vib_st =
                self.wheel * p.vib_depth_st * (self.vib_phase * core::f32::consts::TAU).sin();
            self.vib_phase += vib_inc;
            if self.vib_phase >= 1.0 {
                self.vib_phase -= 1.0;
            }
            let pitch = 2f32.powf((self.bend_st + vib_st) / 12.0);

            let mut s = 0.0f32;
            for v in &mut self.voices {
                let freq = 440.0 * 2f32.powf((v.note as f32 - 69.0) / 12.0) * pitch;
                v.phase += freq / sr;
                if v.phase >= 1.0 {
                    v.phase -= 1.0;
                }
                let raw = self.wave.sample(v.phase);

                // Per-voice filter: cutoff = base + env·depth on the
                // normalized log scale, through a Chamberlin SVF lowpass.
                let env = v.filter_env.tick();
                let norm = (p.cutoff_norm + p.filter_env_depth * env).clamp(0.0, 1.0);
                let fc = NativeFilter::cutoff_from_norm(norm).min(sr * 0.45);
                let g = (core::f32::consts::TAU * fc / sr).min(1.2);
                v.svf_low += g * v.svf_band;
                let high = raw - v.svf_low - q * v.svf_band;
                v.svf_band += g * high;
                let filtered = v.svf_low;

                s += filtered * v.amp * v.amp_env.tick();
            }
            out_l[f] = s;
            out_r[f] = s;
        }
        // Reap voices whose release finished.
        self.voices.retain(|v| !v.amp_env.is_idle());
    }

    fn reset(&mut self) {
        self.voices.clear();
        self.bend_st = 0.0;
        self.wheel = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use signal_plugin_host::PluginMidiEvent;

    fn note_on(note: u8, vel: u8) -> PluginMidiEvent {
        use midicore::{Channel, KeyNumber, MidiEvent, Velocity};
        PluginMidiEvent {
            offset: 0,
            message: MidiEvent::NoteOn {
                channel: Channel::new(0),
                key: KeyNumber::new(note),
                velocity: Velocity::new(vel),
            },
        }
    }

    fn note_off(note: u8) -> PluginMidiEvent {
        use midicore::{Channel, KeyNumber, MidiEvent, Velocity};
        PluginMidiEvent {
            offset: 0,
            message: MidiEvent::NoteOff {
                channel: Channel::new(0),
                key: KeyNumber::new(note),
                velocity: Velocity::new(0),
            },
        }
    }

    fn bend(raw: u16) -> PluginMidiEvent {
        use midicore::{Channel, MidiEvent, PitchBend};
        PluginMidiEvent {
            offset: 0,
            message: MidiEvent::PitchBend {
                channel: Channel::new(0),
                bend: PitchBend::new(raw),
            },
        }
    }

    fn cc(controller: u8, value: u8) -> PluginMidiEvent {
        use midicore::{Channel, ControllerNumber, ControllerValue, MidiEvent};
        PluginMidiEvent {
            offset: 0,
            message: MidiEvent::ControlChange {
                channel: Channel::new(0),
                controller: ControllerNumber::new(controller),
                value: ControllerValue::new(value),
            },
        }
    }

    fn run(osc: &mut NativeOscillator, midi: &[PluginMidiEvent], frames: usize) -> Vec<f32> {
        let mut l = vec![0.0f32; frames];
        let mut r = vec![0.0f32; frames];
        let ev = PluginEvents { params: &[], midi, note_expressions: &[] };
        osc.render(&[], &[], &mut l, &mut r, &ev);
        l
    }

    fn rms(s: &[f32]) -> f32 {
        (s.iter().map(|x| x * x).sum::<f32>() / s.len().max(1) as f32).sqrt()
    }

    /// Dominant frequency estimate by zero crossings.
    fn zero_crossings(s: &[f32]) -> usize {
        s.windows(2).filter(|w| (w[0] <= 0.0) != (w[1] <= 0.0)).count()
    }

    #[test]
    fn note_on_generates_non_silent_audio() {
        let mut osc = NativeOscillator::new(48_000);
        let out = run(&mut osc, &[note_on(69, 100)], 4800);
        assert!(rms(&out) > 1e-3);
    }

    #[test]
    fn note_off_releases_then_silences() {
        let mut osc = NativeOscillator::new(48_000);
        let _ = run(&mut osc, &[note_on(60, 100)], 4800);
        // 1.5 s ≫ the 150 ms default release (the one-pole tail needs
        // ~7 time constants to cross the idle threshold).
        let _ = run(&mut osc, &[note_off(60)], 72_000);
        assert_eq!(osc.active_voices(), 0, "released voice reaped");
        let tail = run(&mut osc, &[], 4800);
        assert!(rms(&tail) < 1e-6);
    }

    /// The live parameter surface reaches the voices: a slow attack makes
    /// the onset far quieter than the default instant attack.
    #[test]
    fn amp_attack_param_shapes_the_onset() {
        let onset = |attack_norm: f64| {
            let mut osc = NativeOscillator::new(48_000);
            let params = [(P_AMP_ATTACK, attack_norm)];
            let midi = [note_on(69, 100)];
            let mut l = vec![0.0f32; 2400];
            let mut r = vec![0.0f32; 2400];
            let ev = PluginEvents { params: &params, midi: &midi, note_expressions: &[] };
            osc.render(&[], &[], &mut l, &mut r, &ev);
            rms(&l)
        };
        let instant = onset(0.0);
        let slow = onset(0.5); // 4 s attack — barely opened after 50 ms
        assert!(instant > 1e-3);
        assert!(
            slow < instant * 0.1,
            "slow attack quiets the onset: instant={instant} slow={slow}"
        );
    }

    /// Per-voice envelopes: a note released into its tail keeps ringing
    /// while a NEW note attacks with its own fresh envelope.
    #[test]
    fn each_note_owns_its_envelope() {
        let mut osc = NativeOscillator::new(48_000);
        // Long release so the released note audibly overlaps the new one.
        let params = [(P_AMP_RELEASE, 0.25)]; // 2 s release
        let ev = PluginEvents { params: &params, midi: &[], note_expressions: &[] };
        let (mut l, mut r) = (vec![0.0f32; 8], vec![0.0f32; 8]);
        osc.render(&[], &[], &mut l, &mut r, &ev);

        let _ = run(&mut osc, &[note_on(60, 100)], 4800);
        let _ = run(&mut osc, &[note_off(60)], 480);
        assert_eq!(osc.active_voices(), 1, "released voice still in its tail");
        let overlap = run(&mut osc, &[note_on(72, 100)], 4800);
        assert_eq!(osc.active_voices(), 2, "old tail + new attack coexist");
        assert!(rms(&overlap) > 1e-3);
    }

    /// Per-voice filter: closing the cutoff darkens (removes high-frequency
    /// content ⇒ far fewer zero crossings for a high note).
    #[test]
    fn cutoff_param_filters_per_voice() {
        let content = |cutoff_norm: f64| {
            let mut osc = NativeOscillator::new(48_000).with_wave(OscWave::Saw);
            let params = [(P_CUTOFF, cutoff_norm), (P_RESONANCE, 0.0)];
            let midi = [note_on(93, 110)]; // A6, 1760 Hz — lots to remove
            let mut l = vec![0.0f32; 9600];
            let mut r = vec![0.0f32; 9600];
            let ev = PluginEvents { params: &params, midi: &midi, note_expressions: &[] };
            osc.render(&[], &[], &mut l, &mut r, &ev);
            rms(&l)
        };
        let open = content(1.0);
        let closed = content(0.05);
        assert!(open > 1e-3);
        assert!(
            closed < open * 0.35,
            "closed cutoff attenuates the note: open={open} closed={closed}"
        );
    }

    /// The pitch wheel bends the note: +full wheel at a 2 st range raises
    /// the frequency by ~12% (2^(2/12)).
    #[test]
    fn pitch_bend_shifts_frequency() {
        let freq_of = |midi: &[PluginMidiEvent]| {
            let mut osc = NativeOscillator::new(48_000);
            let _ = run(&mut osc, midi, 4800);
            let steady = run(&mut osc, &[], 48_000);
            zero_crossings(&steady) as f32 / 2.0 // Hz over 1 s
        };
        let plain = freq_of(&[note_on(69, 100)]);
        let bent = freq_of(&[note_on(69, 100), bend(16_383)]);
        assert!((plain - 440.0).abs() < 5.0, "A4 sits at 440, got {plain}");
        let ratio = bent / plain;
        assert!(
            (ratio - 2f32.powf(2.0 / 12.0)).abs() < 0.01,
            "full bend ≈ +2 st: ratio={ratio}"
        );
    }

    /// The mod wheel brings in vibrato: with the wheel up, the frequency
    /// wobbles around the note instead of sitting still.
    #[test]
    fn mod_wheel_adds_vibrato() {
        let wobble = |with_wheel: bool| {
            let mut osc = NativeOscillator::new(48_000);
            let mut midi = vec![note_on(69, 100)];
            if with_wheel {
                midi.push(cc(1, 127));
            }
            let _ = run(&mut osc, &midi, 4800);
            // Instantaneous frequency per 50 ms window, via zero crossings.
            let mut freqs = Vec::new();
            for _ in 0..10 {
                let w = run(&mut osc, &[], 2400);
                freqs.push(zero_crossings(&w) as f32 / 2.0 * 20.0);
            }
            let mean = freqs.iter().sum::<f32>() / freqs.len() as f32;
            freqs.iter().map(|f| (f - mean).abs()).fold(0.0f32, f32::max)
        };
        let still = wobble(false);
        let vibrato = wobble(true);
        assert!(
            vibrato > still + 2.0,
            "wheel-up vibrato wobbles the pitch: still={still} vibrato={vibrato}"
        );
    }
}
