//! Control-rate **modulation sources** — the runtime behind the ModMatrix
//! (keys-rig roadmap §2, Omnisphere compat §7).
//!
//! Sources produce one value per render block (block-rate control). Three
//! families:
//! - [`ControlLfo`] — free-running LFO.
//! - [`ControlEnv`] — an [`Adsr`] gated by the incoming note stream (any
//!   note-on retriggers; the envelope releases when the last note lifts).
//! - [`MidiMod`] — MIDI performance controllers (mod wheel, aftertouch,
//!   pitch bend, last note-on velocity, arbitrary CC).
//!
//! A [`ModSource`] wraps one of them; `tick(events, frames)` advances it
//! through one block and returns its current value. LFO/bend are bipolar
//! (−1..+1); envelopes/wheel/velocity are unipolar (0..1).

use signal_plugin_host::PluginEvents;

use super::adsr::{Adsr, AdsrParams};

/// LFO waveform (control-rate).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LfoWave {
    Sine,
    Triangle,
    Saw,
    Square,
    /// Sample & hold: a new random value each cycle.
    SampleHold,
}

/// A control LFO. Bipolar output −1..+1. Free-rate or tempo-synced, with
/// optional note-on retrigger.
#[derive(Clone, Copy, Debug)]
pub struct ControlLfo {
    pub wave: LfoWave,
    pub rate_hz: f32,
    /// When set, the rate follows tempo: one cycle per `sync_beats` beats.
    pub sync_beats: Option<f32>,
    /// Reset phase (and redraw S&H) on every note-on.
    pub retrigger: bool,
    pub(crate) sample_rate: f32,
    phase: f32,
    held: f32,
    rng: u32,
}

impl ControlLfo {
    pub fn new(wave: LfoWave, rate_hz: f32) -> Self {
        Self {
            wave,
            rate_hz,
            sync_beats: None,
            retrigger: false,
            sample_rate: 48_000.0,
            phase: 0.0,
            held: 0.0,
            rng: 0x02F6_E2B1,
        }
    }

    #[must_use]
    pub fn with_sync_beats(mut self, beats: f32) -> Self {
        self.sync_beats = (beats > 0.0).then_some(beats);
        self
    }

    #[must_use]
    pub fn with_retrigger(mut self, on: bool) -> Self {
        self.retrigger = on;
        self
    }

    fn draw(&mut self) -> f32 {
        self.rng = self.rng.wrapping_mul(0x9E37_79B9).wrapping_add(0x85EB_CA6B);
        let x = self.rng ^ (self.rng >> 15);
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    fn reset(&mut self) {
        self.phase = 0.0;
        self.held = self.draw();
    }

    /// Advance by `frames` at `sample_rate` and `tempo_bpm`; returns the
    /// value at the block start (one value per block — block-rate control).
    fn advance(&mut self, frames: usize, sample_rate: f32, tempo_bpm: f32) -> f32 {
        let v = match self.wave {
            LfoWave::Sine => (core::f32::consts::TAU * self.phase).sin(),
            LfoWave::Triangle => 4.0 * (self.phase - 0.5).abs() - 1.0,
            LfoWave::Saw => 2.0 * self.phase - 1.0,
            LfoWave::Square => {
                if self.phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            LfoWave::SampleHold => self.held,
        };
        let hz = match self.sync_beats {
            Some(beats) => (tempo_bpm.max(1.0) / 60.0) / beats.max(1e-3),
            None => self.rate_hz,
        };
        let next = self.phase + hz * frames as f32 / sample_rate.max(1.0);
        if next >= 1.0 {
            self.held = self.draw();
        }
        self.phase = next - next.floor();
        v
    }
}

/// A note-gated control envelope: retriggers on any note-on, releases when
/// the last held note lifts. Unipolar 0..1.
#[derive(Clone, Copy, Debug)]
pub struct ControlEnv {
    env: Adsr,
    held: u32,
}

impl ControlEnv {
    pub fn new(sample_rate: f32, params: AdsrParams) -> Self {
        Self {
            env: Adsr::new(sample_rate, params),
            held: 0,
        }
    }

    fn advance(&mut self, events: &PluginEvents<'_>, frames: usize) -> f32 {
        use midicore::MidiEvent;
        for ev in events.midi {
            match &ev.message {
                MidiEvent::NoteOn { velocity, .. } if velocity.get() > 0 => {
                    self.held += 1;
                    self.env.note_on();
                }
                MidiEvent::NoteOn { .. } | MidiEvent::NoteOff { .. } => {
                    self.held = self.held.saturating_sub(1);
                    if self.held == 0 {
                        self.env.note_off();
                    }
                }
                _ => {}
            }
        }
        // Advance through the block; block-rate consumers take the end value.
        let mut v = 0.0;
        for _ in 0..frames {
            v = self.env.tick();
        }
        v
    }
}

/// MIDI performance sources.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MidiMod {
    /// Mod wheel (CC1), 0..1.
    Wheel,
    /// Channel aftertouch, 0..1.
    Aftertouch,
    /// Pitch bend, −1..+1.
    Bender,
    /// Velocity of the most recent note-on, 0..1.
    Velocity,
    /// Note number of the most recent note-on, 0..1 (raw, uncentered).
    Key,
    /// Bipolar key-tracking centered at middle C (note 60): −1..+1 across
    /// ±48 semitones (±4 octaves), note 60 = 0. The centered form a
    /// key-track modulator wants (settable center/slope is a follow-up).
    KeyTrack,
    /// Sample-and-hold random, redrawn per note-on, −1..+1.
    Random,
    /// Alternates 0 / 1 on each note-on.
    Alt,
    /// Always 1.0 (route depth = a constant offset).
    Constant,
    /// MPE per-note pressure (latest across voices), 0..1.
    MpePressure,
    /// MPE per-note timbre / brightness (latest across voices), 0..1.
    MpeTimbre,
    /// MPE per-note pitch / glide (the X dimension), −1..+1 over ±48 semitones.
    MpeBend,
    /// An arbitrary CC, 0..1. Named performance controllers route through
    /// this: sustain = CC64, expression = CC11, breath = CC2.
    Cc(u8),
}

/// A **control-rate modulation source**: produces one value per render
/// block. Implement this to add a new source kind (a sequencer, a follower,
/// a macro…) — the ModMatrix engine only sees the trait.
pub trait ControlSource: Send {
    /// Live-update an envelope source's ADSR. `false` for non-envelope
    /// sources (the caller treats it as "not an env").
    fn set_env_params(&mut self, _sample_rate: f32, _params: crate::native::AdsrParams) -> bool {
        false
    }

    /// Rate change (voices/coefficients survive).
    fn set_sample_rate(&mut self, _sample_rate: f32) {}
    /// Advance through one block; returns the source's current value.
    /// Bipolar sources return −1..+1, unipolar 0..1.
    fn tick(&mut self, events: &PluginEvents<'_>, frames: usize, tempo_bpm: f32) -> f32;
}

impl ControlSource for ControlLfo {
    fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
    }

    fn tick(&mut self, events: &PluginEvents<'_>, frames: usize, tempo_bpm: f32) -> f32 {
        if self.retrigger
            && events.midi.iter().any(|ev| {
                matches!(
                    &ev.message,
                    midicore::MidiEvent::NoteOn { velocity, .. } if velocity.get() > 0
                )
            })
        {
            self.reset();
        }
        let sr = self.sample_rate;
        self.advance(frames, sr, tempo_bpm)
    }
}

impl ControlSource for ControlEnv {
    fn set_env_params(&mut self, sample_rate: f32, params: crate::native::AdsrParams) -> bool {
        self.env.set_params(sample_rate, params);
        true
    }

    fn set_sample_rate(&mut self, sample_rate: f32) {
        self.env.set_sample_rate(sample_rate);
    }

    fn tick(&mut self, events: &PluginEvents<'_>, frames: usize, _tempo_bpm: f32) -> f32 {
        self.advance(events, frames)
    }
}

/// A [`MidiMod`] with its held value — MIDI sources are event-driven and
/// hold their last value between blocks.
pub struct MidiSource {
    mode: MidiMod,
    value: f32,
}

impl MidiSource {
    pub fn new(mode: MidiMod) -> Self {
        Self {
            mode,
            value: if mode == MidiMod::Constant { 1.0 } else { 0.0 },
        }
    }
}

impl ControlSource for MidiSource {
    fn tick(&mut self, events: &PluginEvents<'_>, _frames: usize, _tempo_bpm: f32) -> f32 {
        use midicore::MidiEvent;
        let m = self.mode;
        let mut v = self.value;
        for ev in events.midi {
            match (m, &ev.message) {
                (
                    MidiMod::Wheel,
                    MidiEvent::ControlChange {
                        controller, value, ..
                    },
                ) if controller.get() == 1 => {
                    v = value.get() as f32 / 127.0;
                }
                (
                    MidiMod::Cc(n),
                    MidiEvent::ControlChange {
                        controller, value, ..
                    },
                ) if controller.get() == n => {
                    v = value.get() as f32 / 127.0;
                }
                (MidiMod::Aftertouch, MidiEvent::ChannelPressure { pressure, .. }) => {
                    v = pressure.get() as f32 / 127.0;
                }
                (MidiMod::Bender, MidiEvent::PitchBend { bend, .. }) => {
                    // −8192..8191 → −1..+1.
                    v = bend.offset() as f32 / 8192.0;
                }
                (MidiMod::Velocity, MidiEvent::NoteOn { velocity, .. }) if velocity.get() > 0 => {
                    v = velocity.get() as f32 / 127.0;
                }
                (MidiMod::Key, MidiEvent::NoteOn { key, velocity, .. }) if velocity.get() > 0 => {
                    v = key.get() as f32 / 127.0;
                }
                (MidiMod::KeyTrack, MidiEvent::NoteOn { key, velocity, .. })
                    if velocity.get() > 0 =>
                {
                    v = ((key.get() as f32 - 60.0) / 48.0).clamp(-1.0, 1.0);
                }
                (MidiMod::Random, MidiEvent::NoteOn { velocity, .. }) if velocity.get() > 0 => {
                    // Redraw from a running hash of the previous value.
                    let bits = (v.to_bits() ^ 0x9E37_79B9).wrapping_mul(0xC2B2_AE35);
                    v = ((bits >> 8) as f32 / (u32::MAX >> 8) as f32) * 2.0 - 1.0;
                }
                (MidiMod::Alt, MidiEvent::NoteOn { velocity, .. }) if velocity.get() > 0 => {
                    v = if v > 0.5 { 0.0 } else { 1.0 };
                }
                _ => {}
            }
        }
        // MPE dimensions ride the note-expression stream.
        for ex in events.note_expressions {
            use daw::service::midi::NoteExpressionDim as Dim;
            match (m, ex.dimension) {
                (MidiMod::MpePressure, Dim::Pressure) | (MidiMod::MpeTimbre, Dim::Brightness) => {
                    v = (ex.value as f32).clamp(0.0, 1.0);
                }
                (MidiMod::MpeBend, Dim::Tuning) => {
                    // Per-note tuning is semitones (−120..+120); map the usual
                    // ±48 st MPE glide range to a bipolar −1..+1 source.
                    v = (ex.value as f32 / 48.0).clamp(-1.0, 1.0);
                }
                _ => {}
            }
        }
        self.value = v;
        v
    }
}

/// One compiled modulation source — a boxed [`ControlSource`] with the
/// convenience constructors the compiler and tests use.
pub struct ModSource(Box<dyn ControlSource>);

impl ModSource {
    /// Live-update this source's ADSR when it is an envelope.
    pub fn set_env_params(&mut self, sample_rate: f32, params: crate::native::AdsrParams) -> bool {
        self.0.set_env_params(sample_rate, params)
    }

    pub fn lfo(mut lfo: ControlLfo, sample_rate: f32) -> Self {
        lfo.sample_rate = sample_rate;
        Self(Box::new(lfo))
    }

    pub fn env(mut env: ControlEnv, sample_rate: f32) -> Self {
        env.env.set_sample_rate(sample_rate);
        Self(Box::new(env))
    }

    pub fn midi(m: MidiMod) -> Self {
        Self(Box::new(MidiSource::new(m)))
    }

    /// Wrap any custom source implementation.
    pub fn custom(source: Box<dyn ControlSource>) -> Self {
        Self(source)
    }

    /// Map a MIDI source name (ours or Omnisphere's) to a [`MidiMod`].
    pub fn midi_by_name(name: &str) -> Option<MidiMod> {
        Some(match name.to_ascii_lowercase().as_str() {
            "wheel" | "mod wheel" | "modwheel" => MidiMod::Wheel,
            "after" | "aftertouch" | "pressure" => MidiMod::Aftertouch,
            "bender" | "bend" | "pitchbend" => MidiMod::Bender,
            "velo" | "velocity" => MidiMod::Velocity,
            "key" => MidiMod::Key,
            "keytrack" | "key track" | "keytracking" => MidiMod::KeyTrack,
            "random" | "random2" | "random unipolar" => MidiMod::Random,
            "alt" => MidiMod::Alt,
            "constant" | "bias" | "bias1" | "bias2" => MidiMod::Constant,
            "mpev" | "mpepressure" | "mpe pressure" => MidiMod::MpePressure,
            "mpe3" | "mpetimbre" | "mpe timbre" => MidiMod::MpeTimbre,
            "mpex" | "mpebend" | "mpe pitch" | "mpe glide" | "mpe x" => MidiMod::MpeBend,
            // Named performance controllers → their CC.
            "sustain" | "pedal" | "sustain pedal" => MidiMod::Cc(64),
            "expression" | "expr" => MidiMod::Cc(11),
            "breath" | "breath control" => MidiMod::Cc(2),
            other => {
                let n = other.strip_prefix("cc")?.parse().ok()?;
                MidiMod::Cc(n)
            }
        })
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.0.set_sample_rate(sample_rate);
    }

    /// Advance through one block; returns the source's current value.
    pub fn tick(&mut self, events: &PluginEvents<'_>, frames: usize) -> f32 {
        self.0.tick(events, frames, 120.0)
    }

    /// [`tick`](Self::tick) with an explicit tempo (for synced LFOs).
    pub fn tick_at(&mut self, events: &PluginEvents<'_>, frames: usize, tempo_bpm: f32) -> f32 {
        self.0.tick(events, frames, tempo_bpm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use midicore::{Channel, ControllerNumber, ControllerValue, KeyNumber, MidiEvent, Velocity};
    use signal_plugin_host::PluginMidiEvent;

    fn ev_note_on(note: u8, vel: u8) -> MidiEvent {
        MidiEvent::NoteOn {
            channel: Channel::new(0),
            key: KeyNumber::new(note),
            velocity: Velocity::new(vel),
        }
    }
    fn ev_note_off(note: u8) -> MidiEvent {
        MidiEvent::NoteOff {
            channel: Channel::new(0),
            key: KeyNumber::new(note),
            velocity: Velocity::new(0),
        }
    }
    fn ev_cc(controller: u8, value: u8) -> MidiEvent {
        MidiEvent::ControlChange {
            channel: Channel::new(0),
            controller: ControllerNumber::new(controller),
            value: ControllerValue::new(value),
        }
    }

    fn no_events() -> PluginEvents<'static> {
        PluginEvents {
            params: &[],
            midi: &[],
            note_expressions: &[],
        }
    }

    #[test]
    fn lfo_cycles_bipolar() {
        // 1 Hz at 48 kHz, 512-frame blocks: sweep one second, track extremes.
        let mut src = ModSource::lfo(ControlLfo::new(LfoWave::Sine, 1.0), 48_000.0);
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for _ in 0..(48_000 / 512 + 1) {
            let v = src.tick(&no_events(), 512);
            lo = lo.min(v);
            hi = hi.max(v);
        }
        assert!(hi > 0.9 && lo < -0.9, "full bipolar swing, got {lo}..{hi}");
    }

    #[test]
    fn env_gates_on_notes() {
        let mut src = ModSource::env(
            ControlEnv::new(48_000.0, crate::native::AdsrParams::default()),
            48_000.0,
        );
        let on = [PluginMidiEvent {
            offset: 0,
            message: ev_note_on(60, 100),
        }];
        let ev_on = PluginEvents {
            params: &[],
            midi: &on,
            note_expressions: &[],
        };
        let v = src.tick(&ev_on, 4_800); // 100 ms — past the 3 ms attack
        assert!(v > 0.5, "gated envelope rises, v={v}");
        let off = [PluginMidiEvent {
            offset: 0,
            message: ev_note_off(60),
        }];
        let ev_off = PluginEvents {
            params: &[],
            midi: &off,
            note_expressions: &[],
        };
        let mut v = 1.0;
        for _ in 0..40 {
            let e = if v == 1.0 { &ev_off } else { &no_events() };
            v = src.tick(e, 4_800);
            if v < 0.01 {
                break;
            }
        }
        assert!(v < 0.01, "released envelope decays, v={v}");
    }

    #[test]
    fn synced_lfo_follows_tempo() {
        // 1 cycle per beat at 120 BPM = 2 Hz. One second → 2 full cycles.
        let mut src = ModSource::lfo(
            ControlLfo::new(LfoWave::Saw, 0.0).with_sync_beats(1.0),
            48_000.0,
        );
        let mut wraps = 0;
        let mut last = -1.0f32;
        // 1.1 s of 10 ms blocks — catches both wraps of a 2 Hz saw.
        for _ in 0..110 {
            let v = src.tick_at(&no_events(), 480, 120.0);
            if v < last - 1.0 {
                wraps += 1; // saw reset
            }
            last = v;
        }
        assert_eq!(wraps, 2, "2 Hz at 120 BPM 1-beat sync, got {wraps} wraps");
    }

    #[test]
    fn sample_hold_steps_per_cycle() {
        let mut src = ModSource::lfo(ControlLfo::new(LfoWave::SampleHold, 4.0), 48_000.0);
        // 4 Hz over 1 s in 100 ms blocks → value changes several times but
        // holds within a cycle.
        let mut values = Vec::new();
        for _ in 0..10 {
            values.push(src.tick_at(&no_events(), 4_800, 120.0));
        }
        values.dedup();
        assert!(values.len() >= 3, "S&H redraws per cycle, got {values:?}");
        assert!(values.iter().all(|v| (-1.0..=1.0).contains(v)));
    }

    #[test]
    fn retriggered_lfo_resets_on_note_on() {
        let mut src = ModSource::lfo(
            ControlLfo::new(LfoWave::Saw, 1.0).with_retrigger(true),
            48_000.0,
        );
        // Run a quarter cycle, then hit a note: phase resets to 0 → value −1.
        src.tick_at(&no_events(), 12_000, 120.0);
        let on = [PluginMidiEvent {
            offset: 0,
            message: ev_note_on(60, 100),
        }];
        let ev = PluginEvents {
            params: &[],
            midi: &on,
            note_expressions: &[],
        };
        let v = src.tick_at(&ev, 64, 120.0);
        assert!(v < -0.98, "saw restarts at −1 after retrigger, got {v}");
    }

    #[test]
    fn wheel_tracks_cc1() {
        let mut src = ModSource::midi(MidiMod::Wheel);
        let cc = [PluginMidiEvent {
            offset: 0,
            message: ev_cc(1, 64),
        }];
        let ev = PluginEvents {
            params: &[],
            midi: &cc,
            note_expressions: &[],
        };
        let v = src.tick(&ev, 64);
        assert!((v - 64.0 / 127.0).abs() < 1e-3);
        // Holds its value across empty blocks.
        assert_eq!(src.tick(&no_events(), 64), v);
    }

    #[test]
    fn named_performance_sources_resolve() {
        assert_eq!(ModSource::midi_by_name("sustain"), Some(MidiMod::Cc(64)));
        assert_eq!(ModSource::midi_by_name("expression"), Some(MidiMod::Cc(11)));
        assert_eq!(ModSource::midi_by_name("breath"), Some(MidiMod::Cc(2)));
        assert_eq!(ModSource::midi_by_name("bias"), Some(MidiMod::Constant));
        assert_eq!(ModSource::midi_by_name("mpex"), Some(MidiMod::MpeBend));
        assert_eq!(ModSource::midi_by_name("cc74"), Some(MidiMod::Cc(74)));
        assert_eq!(ModSource::midi_by_name("nope"), None);
    }

    #[test]
    fn keytrack_is_bipolar_around_middle_c() {
        let mut src = ModSource::midi(MidiMod::KeyTrack);
        let at = |note: u8| {
            [PluginMidiEvent {
                offset: 0,
                message: ev_note_on(note, 100),
            }]
        };
        let mid = at(60);
        let ev = PluginEvents {
            params: &[],
            midi: &mid,
            note_expressions: &[],
        };
        assert!(src.tick(&ev, 64).abs() < 1e-3, "note 60 centers at 0");
        let hi = at(108);
        let ev = PluginEvents {
            params: &[],
            midi: &hi,
            note_expressions: &[],
        };
        assert!((src.tick(&ev, 64) - 1.0).abs() < 1e-3, "+48 st → +1");
        let lo = at(12);
        let ev = PluginEvents {
            params: &[],
            midi: &lo,
            note_expressions: &[],
        };
        assert!((src.tick(&ev, 64) + 1.0).abs() < 1e-3, "-48 st → -1");
    }

    #[test]
    fn sustain_source_tracks_cc64_only() {
        let mut src = ModSource::midi(ModSource::midi_by_name("sustain").unwrap());
        let down = [PluginMidiEvent {
            offset: 0,
            message: ev_cc(64, 127),
        }];
        let ev = PluginEvents {
            params: &[],
            midi: &down,
            note_expressions: &[],
        };
        assert!((src.tick(&ev, 64) - 1.0).abs() < 1e-3);
        // A different CC must not disturb the held value.
        let other = [PluginMidiEvent {
            offset: 0,
            message: ev_cc(11, 0),
        }];
        let ev2 = PluginEvents {
            params: &[],
            midi: &other,
            note_expressions: &[],
        };
        assert!((src.tick(&ev2, 64) - 1.0).abs() < 1e-3);
    }
}
