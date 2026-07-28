//! Per-track drum-kit hosting — a `.signalpreset` kit as real daw tracks.
//!
//! The fully daw-based drum mixer (#65): each piece's **close mics** are
//! instrument tracks direct to master, each **bus mic** (Overhead / Room …)
//! is an instrument track sending into a shared bus track, and every strip
//! op (fader / mute / solo) is a daw track op the standalone renderer
//! resolves natively — replacing the in-engine `DrumMixer`.
//!
//! ## One engine per mic, correlated
//!
//! A kit piece is multi-mic recordings of the same hit, so its mics MUST pick
//! the same round-robin. Each mic gets its own [`SampleEngine`] over the same
//! pack ([`SampleEngine::set_solo_mic`] restricts it to its mic's zones);
//! correlation holds because RR selection is deterministic — the cycle
//! counter advances per note-on and the random modes run a xorshift seeded
//! with the same constant in every engine — and every mic engine of a piece
//! receives exactly the same trigger sequence.
//!
//! ## Direct dispatch
//!
//! Kit triggers bypass the per-track MIDI queues: the routing table resolves
//! a note to its target pieces (articulation + transpose + choke config ride
//! along from the preset) and drives each mic engine directly under its
//! shared lock — the same contention policy as the bank instrument (the
//! audio thread `try_lock`s and renders silence for a block on contention).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use signal_plugin_host::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};

use crate::engine::SampleEngine;

/// A shared handle on one mic's engine.
pub type SharedEngine = Arc<Mutex<SampleEngine>>;

/// One mic's render-only instrument: the engine is driven by the kit
/// dispatcher (direct calls under the lock); the renderer only pulls blocks.
pub struct KitMicInstrument {
    engine: SharedEngine,
    /// Interleaved-stereo render scratch, reused across blocks.
    scratch: Vec<f32>,
    prepared: bool,
}

impl KitMicInstrument {
    pub fn new(engine: SharedEngine) -> Self {
        Self {
            engine,
            scratch: Vec::new(),
            prepared: false,
        }
    }
}

impl PluginInstance for KitMicInstrument {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "signal.sampler.kit_mic".into(),
            name: "Kit Mic".into(),
            vendor: "Signal".into(),
            version: String::new(),
            format: PluginFormat::Synthetic,
        }
    }
    fn params(&mut self) -> Vec<PluginParamInfo> {
        Vec::new()
    }
    fn param_value(&mut self, _id: u32) -> Option<f64> {
        None
    }
    fn value_to_text(&mut self, _id: u32, _v: f64) -> Option<String> {
        None
    }
    fn text_to_value(&mut self, _id: u32, _t: &str) -> Option<f64> {
        None
    }
    fn latency(&mut self) -> u32 {
        0
    }
    fn prepare(&mut self, _sr: f64, block_size: u32) -> Result<(), PluginError> {
        self.scratch.resize(block_size as usize * 2, 0.0);
        self.prepared = true;
        Ok(())
    }
    fn is_prepared(&self) -> bool {
        self.prepared
    }
    fn process_block(
        &mut self,
        _in_l: &[f32],
        _in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        _events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        let frames = out_l.len().min(out_r.len());
        // Never block the audio thread on the dispatcher's engine lock.
        let mut engine = match self.engine.try_lock() {
            Ok(e) => e,
            Err(_) => {
                out_l[..frames].fill(0.0);
                out_r[..frames].fill(0.0);
                return Ok(());
            }
        };
        let want = frames * 2;
        if self.scratch.len() < want {
            self.scratch.resize(want, 0.0);
        }
        self.scratch[..want].fill(0.0);
        engine.render(&mut self.scratch[..want]);
        drop(engine);
        for f in 0..frames {
            out_l[f] = self.scratch[f * 2];
            out_r[f] = self.scratch[f * 2 + 1];
        }
        Ok(())
    }
    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

/// One routed target of a MIDI note.
#[derive(Clone, Debug)]
pub struct RouteTarget {
    /// Engine-ref id within the preset (`"kick"`, `"hats-closed"`).
    pub piece: String,
    /// Per-route articulation override (fired ignoring key). Empty = keyed.
    pub articulation: String,
}

/// The preset's note → pieces dispatch table.
#[derive(Clone, Debug, Default)]
pub struct KitRouting {
    routes: HashMap<u8, Vec<RouteTarget>>,
}

impl KitRouting {
    pub fn from_preset(preset: &crate::preset_spec::PresetSpec) -> Self {
        let mut routes: HashMap<u8, Vec<RouteTarget>> = HashMap::new();
        for r in &preset.note_routing {
            let entry = routes.entry(r.note).or_default();
            for target in &r.targets {
                entry.push(RouteTarget {
                    piece: target.clone(),
                    articulation: r.articulation.clone(),
                });
            }
        }
        Self { routes }
    }

    /// The targets for `note`. An empty routing table routes every note to
    /// every piece (each engine's own zones filter), mirroring the bank's
    /// default-instrument behavior.
    pub fn targets(&self, note: u8) -> Option<&[RouteTarget]> {
        self.routes.get(&note).map(|v| v.as_slice())
    }

    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }
}

/// One hosted mic track of a piece.
pub struct KitMic {
    pub mic: String,
    /// Instrument id in the rig's track tables (`"kit:kick:Overhead"`).
    pub instrument_id: String,
    pub track_guid: String,
    pub fx_guid: String,
    pub meter_index: usize,
    /// The shared bus this mic sends into, or `None` for a close mic.
    pub bus: Option<String>,
    pub engine: SharedEngine,
}

/// One kit piece: its engine-ref id, display label, transpose, and mic tracks.
pub struct KitPiece {
    /// Engine-ref id within the preset (`"kick"`).
    pub id: String,
    /// Display label (the engine spec's name, falling back to the id).
    pub label: String,
    /// Preset-level note transpose for this piece.
    pub transpose: i8,
    /// Preset-level starting mute.
    pub muted: bool,
    pub mics: Vec<KitMic>,
}

impl KitPiece {
    /// Fan one trigger to every mic engine of the piece — identical event
    /// sequences keep the mics' RR selection correlated.
    pub fn note_on(&self, note: u8, velocity: u8, articulation: Option<&str>) {
        let note = transposed(note, self.transpose);
        for mic in &self.mics {
            if let Ok(mut e) = mic.engine.lock() {
                e.note_on_articulated(note, velocity, articulation);
            }
        }
    }

    pub fn note_off(&self, note: u8) {
        let note = transposed(note, self.transpose);
        for mic in &self.mics {
            if let Ok(mut e) = mic.engine.lock() {
                e.note_off(note);
            }
        }
    }

    pub fn cc(&self, controller: u8, value: u8) {
        for mic in &self.mics {
            if let Ok(mut e) = mic.engine.lock() {
                e.cc(controller, value);
            }
        }
    }
}

fn transposed(note: u8, transpose: i8) -> u8 {
    (note as i16 + transpose as i16).clamp(0, 127) as u8
}

/// A loaded per-track kit: the routing table + hosted pieces + bus tracks.
pub struct KitState {
    pub prefix: String,
    pub name: String,
    pub routing: KitRouting,
    pub pieces: Vec<KitPiece>,
    /// Shared bus tracks: `(bus id, track guid, meter index)`.
    pub buses: Vec<(String, String, usize)>,
}

impl KitState {
    pub fn piece(&self, id: &str) -> Option<&KitPiece> {
        self.pieces.iter().find(|p| p.id == id)
    }

    /// Dispatch one MIDI event through the routing table.
    pub fn dispatch(&self, ev: &midicore::MidiEvent) {
        use midicore::MidiEvent;
        match ev {
            MidiEvent::NoteOn { key, velocity, .. } => {
                self.dispatch_note(key.get(), velocity.get())
            }
            MidiEvent::NoteOff { key, .. } => {
                let note = key.get();
                match self.routing.targets(note) {
                    Some(targets) => {
                        for t in targets {
                            if let Some(p) = self.piece(&t.piece) {
                                p.note_off(note);
                            }
                        }
                    }
                    None if self.routing.is_empty() => {
                        for p in &self.pieces {
                            p.note_off(note);
                        }
                    }
                    None => {}
                }
            }
            MidiEvent::ControlChange {
                controller, value, ..
            } => {
                // CCs (hat pedal, mod) reach every piece — engines ignore
                // what they don't use.
                for p in &self.pieces {
                    p.cc(controller.get(), value.get());
                }
            }
            _ => {}
        }
    }

    /// Route one note-on. Velocity 0 is a note-off by convention (the
    /// engines route it themselves, but the targets must match note-on's).
    pub fn dispatch_note(&self, note: u8, velocity: u8) {
        match self.routing.targets(note) {
            Some(targets) => {
                for t in targets {
                    if let Some(p) = self.piece(&t.piece) {
                        let artic =
                            (!t.articulation.is_empty()).then_some(t.articulation.as_str());
                        p.note_on(note, velocity, artic);
                    }
                }
            }
            None if self.routing.is_empty() => {
                // No routing table: every piece sees the note (its own
                // zones / pinned articulation filter).
                for p in &self.pieces {
                    p.note_on(note, velocity, None);
                }
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_builds_from_preset_and_falls_back_to_all() {
        let preset = crate::preset_spec::PresetSpec {
            name: "Kit".into(),
            description: String::new(),
            engines: Vec::new(),
            note_routing: vec![crate::preset_spec::NoteRoute {
                note: 36,
                targets: vec!["kick".into(), "kick-b".into()],
                articulation: "Center".into(),
            }],
            modules: Vec::new(),
            routing: Vec::new(),
            master_fx: Vec::new(),
            macros: Vec::new(),
        };
        let routing = KitRouting::from_preset(&preset);
        let t = routing.targets(36).expect("routed note");
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].piece, "kick");
        assert_eq!(t[0].articulation, "Center");
        assert!(routing.targets(38).is_none());
        assert!(!routing.is_empty());
        assert!(KitRouting::default().is_empty());
    }

    #[test]
    fn transpose_clamps() {
        assert_eq!(transposed(36, 2), 38);
        assert_eq!(transposed(1, -12), 0);
        assert_eq!(transposed(120, 12), 127);
    }
}
