//! Performance scripting — `docs/sampler-trait-design.md` §5.
//!
//! A [`PerformanceScript`] transforms incoming MIDI *before* voicing. This is
//! the scripting framework: keyswitch routing, CC→articulation, true-legato,
//! humanize, velocity curves. Built-ins are *configured values*; custom logic
//! is a *type* you write. Scripts are pure (no audio), so they're trivial to
//! test and hot-swap.
//!
//! Phase A ships:
//! - [`Performance`] — the command surface + shared state (held notes, last
//!   note, current articulation, RR counter).
//! - [`ScriptPipeline`] — runs scripts in order, each seeing the prior's edits.
//! - [`KeyswitchRouter`] — a **real** built-in: keyswitch notes →
//!   `set_articulation` (the other notes pass through).
//! - [`VelocityCurve`] — a **real** built-in scaling note-on velocity.
//!
//! Phase-B stubs (defined, but no-op so the surface is complete): `TrueLegato`
//! (reads `Legato`/`VelCurve`), `SustainPedal`, `CcArticulation`,
//! `RoundRobinReset`, `Humanize`.

use std::collections::HashMap;

use super::model::Legato;
use super::prim::{ArticulationId, Cc, Note, Seconds, U7, Velocity};

/// A raw MIDI message entering the pipeline.
#[derive(Clone, Copy, Debug)]
pub enum MidiMessage {
    NoteOn { note: Note, vel: Velocity },
    NoteOff { note: Note, vel: Velocity },
    Cc { cc: Cc, value: U7 },
}

/// A resolved action a script emits — what the voicer/engine should do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Play { note: Note, vel: Velocity },
    Stop { note: Note },
    Legato { from: Note, to: Note, vel: Velocity },
    Articulate { id_index: usize },
}

/// The script's command surface + shared state. Scripts compose: the pipeline
/// runs them in order, each seeing the prior's edits.
///
/// `articulations` is the ordered id list (so `set_articulation` can resolve a
/// name to the index the engine/voicer uses); `actions` accumulates the emitted
/// commands for this message.
pub struct Performance<'a> {
    held: Vec<Note>,
    last_note: Option<Note>,
    articulation: usize,
    round_robin: u32,
    articulations: &'a [ArticulationId],
    actions: Vec<Action>,
}

impl<'a> Performance<'a> {
    pub fn new(articulations: &'a [ArticulationId]) -> Self {
        Performance {
            held: Vec::new(),
            last_note: None,
            articulation: 0,
            round_robin: 0,
            articulations,
            actions: Vec::new(),
        }
    }

    // ── command surface ──
    pub fn play(&mut self, note: Note, vel: Velocity) {
        if !self.held.contains(&note) {
            self.held.push(note);
        }
        self.last_note = Some(note);
        self.actions.push(Action::Play { note, vel });
    }

    pub fn legato_to(&mut self, from: Note, to: Note, vel: Velocity) {
        if !self.held.contains(&to) {
            self.held.push(to);
        }
        self.last_note = Some(to);
        self.actions.push(Action::Legato { from, to, vel });
    }

    pub fn stop(&mut self, note: Note) {
        self.held.retain(|&n| n != note);
        self.actions.push(Action::Stop { note });
    }

    pub fn set_articulation(&mut self, id: ArticulationId) {
        if let Some(i) = self.articulations.iter().position(|a| *a == id) {
            self.articulation = i;
            self.actions.push(Action::Articulate { id_index: i });
        }
    }

    // ── shared state queries ──
    pub fn held(&self) -> &[Note] {
        &self.held
    }
    pub fn last_note(&self) -> Option<Note> {
        self.last_note
    }
    pub fn active_articulation(&self) -> usize {
        self.articulation
    }
    pub fn round_robin(&self) -> u32 {
        self.round_robin
    }
    pub fn bump_round_robin(&mut self, modulo: u32) {
        if modulo > 0 {
            self.round_robin = (self.round_robin + 1) % modulo;
        }
    }

    /// Emitted actions for the message just processed.
    pub fn actions(&self) -> &[Action] {
        &self.actions
    }
    /// Drain accumulated actions (the host applies them after the pipeline).
    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }
}

/// Transform incoming MIDI *before* voicing.
pub trait PerformanceScript: Send {
    /// Return zero or more actions (via `perf`) for one raw message.
    fn on_message(&mut self, msg: MidiMessage, perf: &mut Performance);
    /// Optional per-block tick for time-based behavior (arps, ramps).
    fn tick(&mut self, _frames: usize, _perf: &mut Performance) {}
}

/// Runs scripts in order over a [`Performance`] — each script sees the edits of
/// the prior ones.
#[derive(Default)]
pub struct ScriptPipeline {
    scripts: Vec<Box<dyn PerformanceScript>>,
}

impl ScriptPipeline {
    pub fn new() -> Self {
        ScriptPipeline {
            scripts: Vec::new(),
        }
    }

    /// Append a script (builder style).
    pub fn then(mut self, script: impl PerformanceScript + 'static) -> Self {
        self.scripts.push(Box::new(script));
        self
    }

    /// Run every script over one message, in order.
    pub fn on_message(&mut self, msg: MidiMessage, perf: &mut Performance) {
        for s in self.scripts.iter_mut() {
            s.on_message(msg, perf);
        }
    }

    /// Tick every script for time-based behavior.
    pub fn tick(&mut self, frames: usize, perf: &mut Performance) {
        for s in self.scripts.iter_mut() {
            s.tick(frames, perf);
        }
    }

    pub fn len(&self) -> usize {
        self.scripts.len()
    }
    pub fn is_empty(&self) -> bool {
        self.scripts.is_empty()
    }
}

// ── Built-in: KeyswitchRouter (REAL) ─────────────────────────────────────

/// Maps keyswitch notes → `set_articulation`; all other notes pass through as
/// `play`. The headline built-in proving the pipeline (design doc §5:
/// `KeyswitchRouter::from(&model.keyswitches)`).
pub struct KeyswitchRouter {
    /// keyswitch note → articulation id.
    map: HashMap<u8, ArticulationId>,
}

impl KeyswitchRouter {
    pub fn new() -> Self {
        KeyswitchRouter {
            map: HashMap::new(),
        }
    }

    /// Map a keyswitch note to an articulation.
    pub fn route(mut self, ks: Note, artic: ArticulationId) -> Self {
        self.map.insert(ks.get(), artic);
        self
    }

    /// Build from `(keyswitch, articulation)` pairs.
    pub fn from_pairs(pairs: impl IntoIterator<Item = (Note, ArticulationId)>) -> Self {
        let mut r = KeyswitchRouter::new();
        for (ks, a) in pairs {
            r.map.insert(ks.get(), a);
        }
        r
    }
}

impl Default for KeyswitchRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl PerformanceScript for KeyswitchRouter {
    fn on_message(&mut self, msg: MidiMessage, perf: &mut Performance) {
        match msg {
            MidiMessage::NoteOn { note, vel } => {
                if let Some(artic) = self.map.get(&note.get()) {
                    // A keyswitch press selects an articulation, no audible note.
                    perf.set_articulation(artic.clone());
                } else {
                    perf.play(note, vel);
                }
            }
            MidiMessage::NoteOff { note, .. } => {
                // Keyswitch releases are swallowed; real notes stop.
                if !self.map.contains_key(&note.get()) {
                    perf.stop(note);
                }
            }
            MidiMessage::Cc { .. } => {}
        }
    }
}

// ── Built-in: VelocityCurve (REAL) ───────────────────────────────────────

/// Reshapes note-on velocity through a power curve (`exp` < 1 = brighten soft
/// playing, > 1 = soften). A small, real transform proving non-routing scripts.
pub struct VelocityCurve {
    exp: f32,
}

impl VelocityCurve {
    /// `gamma` is the curve exponent (`0.7` ≈ the doc's `VelocityCurve::cubic`).
    pub fn new(gamma: f32) -> Self {
        VelocityCurve {
            exp: gamma.max(0.01),
        }
    }
    pub fn cubic(gamma: f32) -> Self {
        Self::new(gamma)
    }

    fn shape(&self, v: Velocity) -> Velocity {
        let unit = v.unit().powf(self.exp);
        Velocity::new((unit * 127.0).round() as u8)
    }
}

impl PerformanceScript for VelocityCurve {
    fn on_message(&mut self, msg: MidiMessage, perf: &mut Performance) {
        if let MidiMessage::NoteOn { note, vel } = msg {
            perf.play(note, self.shape(vel));
        }
    }
}

// ── Phase-B stubs ────────────────────────────────────────────────────────
// These are defined so the built-in set named in the design doc exists and the
// pipeline can be assembled, but their behavior is a TODO for Phase B.

/// **Phase B**: reads the per-velocity [`Legato`] (`VelCurve` pre_delay /
/// portamento / crossfade), picks the transition recording, and emits
/// `legato_to`. Today it passes notes through unchanged.
pub struct TrueLegato {
    _cfg: Legato,
}
impl TrueLegato {
    pub fn new(cfg: Legato) -> Self {
        TrueLegato { _cfg: cfg }
    }
}
impl PerformanceScript for TrueLegato {
    fn on_message(&mut self, _msg: MidiMessage, _perf: &mut Performance) {
        // TODO(Phase B): detect overlapping note-ons → legato_to(from,to,vel),
        // reading pre_delay/portamento/crossfade VelCurves at (vel, interval).
    }
}

/// **Phase B**: CC64 hold + half-pedal + pedal-noise group.
pub struct SustainPedal {
    _cc: Cc,
}
impl SustainPedal {
    pub fn new(cc: Cc) -> Self {
        SustainPedal { _cc: cc }
    }
}
impl PerformanceScript for SustainPedal {
    fn on_message(&mut self, _msg: MidiMessage, _perf: &mut Performance) {
        // TODO(Phase B): hold note-offs while pedal down; release on pedal-up.
    }
}

/// **Phase B**: a CC value picks the articulation (e.g. CC58 ranges).
pub struct CcArticulation {
    _cc: Cc,
}
impl CcArticulation {
    pub fn new(cc: Cc) -> Self {
        CcArticulation { _cc: cc }
    }
}
impl PerformanceScript for CcArticulation {
    fn on_message(&mut self, _msg: MidiMessage, _perf: &mut Performance) {
        // TODO(Phase B): map cc.value ranges → set_articulation.
    }
}

/// **Phase B**: reset round-robin counters after a span of silence.
pub struct RoundRobinReset {
    _after: Seconds,
}
impl RoundRobinReset {
    pub fn on_silence(after: Seconds) -> Self {
        RoundRobinReset { _after: after }
    }
}
impl PerformanceScript for RoundRobinReset {
    fn on_message(&mut self, _msg: MidiMessage, _perf: &mut Performance) {
        // TODO(Phase B): track silence; reset perf.round_robin to 0.
    }
}

/// **Phase B**: humanize timing + velocity with small random jitter.
pub struct Humanize {
    pub timing_ms: f32,
    pub vel: u8,
}
impl PerformanceScript for Humanize {
    fn on_message(&mut self, _msg: MidiMessage, _perf: &mut Performance) {
        // TODO(Phase B): jitter note offset / velocity by configured amounts.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artics() -> Vec<ArticulationId> {
        vec![
            ArticulationId::new("Sustain"),
            ArticulationId::new("Staccato"),
        ]
    }

    #[test]
    fn keyswitch_router_selects_and_passes_through() {
        let artics = artics();
        let mut router = KeyswitchRouter::new()
            .route(Note(24), ArticulationId::new("Sustain"))
            .route(Note(25), ArticulationId::new("Staccato"));

        // A keyswitch press selects the articulation, emits no Play.
        let mut perf = Performance::new(&artics);
        router.on_message(
            MidiMessage::NoteOn {
                note: Note(25),
                vel: Velocity::new(1),
            },
            &mut perf,
        );
        assert_eq!(perf.active_articulation(), 1);
        assert_eq!(perf.actions(), &[Action::Articulate { id_index: 1 }]);

        // A normal note passes through as Play.
        let mut perf = Performance::new(&artics);
        router.on_message(
            MidiMessage::NoteOn {
                note: Note(60),
                vel: Velocity::new(100),
            },
            &mut perf,
        );
        assert_eq!(
            perf.actions(),
            &[Action::Play {
                note: Note(60),
                vel: Velocity::new(100)
            }]
        );
        assert_eq!(perf.held(), &[Note(60)]);
    }

    #[test]
    fn pipeline_runs_in_order() {
        // VelocityCurve then a router. The curve plays the (shaped) note; the
        // router (here with no keyswitches) also plays it. Order is observable:
        // the first emitted action comes from the first script.
        let artics = artics();
        let mut pipe = ScriptPipeline::new()
            .then(VelocityCurve::new(2.0)) // square: low vel → much lower
            .then(KeyswitchRouter::new());
        assert_eq!(pipe.len(), 2);

        let mut perf = Performance::new(&artics);
        pipe.on_message(
            MidiMessage::NoteOn {
                note: Note(60),
                vel: Velocity::new(64),
            },
            &mut perf,
        );
        let actions = perf.actions();
        // Two Plays: first from VelocityCurve (shaped down), second from router
        // (passthrough at original vel) — proving ordered execution.
        assert_eq!(actions.len(), 2);
        if let Action::Play { vel, .. } = actions[0] {
            assert!(vel.get() < 64, "curve should reduce vel, got {}", vel.get());
        } else {
            panic!("expected first action to be a Play from VelocityCurve");
        }
        assert_eq!(
            actions[1],
            Action::Play {
                note: Note(60),
                vel: Velocity::new(64)
            }
        );
    }

    #[test]
    fn stubs_compile_and_are_no_op() {
        let artics = artics();
        let mut perf = Performance::new(&artics);
        let mut ped = SustainPedal::new(Cc::SUSTAIN);
        ped.on_message(
            MidiMessage::Cc {
                cc: Cc::SUSTAIN,
                value: U7::new(127),
            },
            &mut perf,
        );
        assert!(perf.actions().is_empty());

        let mut rr = RoundRobinReset::on_silence(Seconds::from_ms(500.0));
        rr.on_message(
            MidiMessage::NoteOn {
                note: Note(60),
                vel: Velocity::new(64),
            },
            &mut perf,
        );
        assert!(perf.actions().is_empty());
    }
}
