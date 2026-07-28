//! Live **Keys rig** — hosts a composition-tree preset (a [`RenderNode`]) as one
//! MIDI-driven instrument on daw's audio engine. The MIDI analog of
//! [`GuitarRig`](crate::rig::GuitarRig), but it plays the Nord-style composition
//! tree: the central MIDI input feeds the tree, whose [`Zone`](crate::rig_node::Zone)s
//! route notes to layers (splits + velocity crossfades), and the live oscillators
//! (and, later, the rest of the native DSP) render the audio.
//!
//! Same engine pattern as [`SamplerRig`](crate::SamplerRig): an output-only daw
//! project with one track (bootstrapped by the shared
//! [`RigHost`]); the preset is wrapped as a
//! [`KeysInstrument`] (`PluginInstance`) inserted into that track's fx slot, and
//! hardware/UI MIDI is pushed into daw's live-MIDI ring keyed by the track — so
//! the renderer hands it to the instrument each block. Swapping the preset is a
//! glitch-free `insert_plugin_instance` under the renderer lock.

use std::sync::Arc;

use daw::service::handle::DawHandle as _;
use daw::standalone::Standalone;
use daw::standalone::metering::Meters;
use daw_audio_io::AudioIoPrefs;
use signal_plugin_host::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};
use signal_rig_host::{RigHost, RigProject};

use crate::MidiMonitor;
use crate::node_render::{GainCells, RenderNode};
use crate::rig_node::{Container, Role};

/// Build a channel-0 `MidiEvent::NoteOn` from raw note/velocity.
fn ev_note_on(note: u8, vel: u8) -> midicore::MidiEvent {
    use midicore::{Channel, KeyNumber, MidiEvent, Velocity};
    MidiEvent::NoteOn {
        channel: Channel::new(0),
        key: KeyNumber::new(note),
        velocity: Velocity::new(vel),
    }
}

/// Build a channel-0 `MidiEvent::NoteOff` from a raw note.
fn ev_note_off(note: u8) -> midicore::MidiEvent {
    use midicore::{Channel, KeyNumber, MidiEvent, Velocity};
    MidiEvent::NoteOff {
        channel: Channel::new(0),
        key: KeyNumber::new(note),
        velocity: Velocity::new(0),
    }
}

/// Build a channel-0 `MidiEvent::ControlChange` from raw controller/value.
fn ev_cc(controller: u8, value: u8) -> midicore::MidiEvent {
    use midicore::{Channel, ControllerNumber, ControllerValue, MidiEvent};
    MidiEvent::ControlChange {
        channel: Channel::new(0),
        controller: ControllerNumber::new(controller),
        value: ControllerValue::new(value),
    }
}

/// Block size the keys instrument is prepared for.
const PREPARE_BLOCK: u32 = 1024;
const KEYS_PROJECT_NAME: &str = "FTS Keys";
const KEYS_TRACK_NAME: &str = "Keys";

/// A composition-tree preset wrapped as a daw instrument. The renderer runs it
/// each block; it ignores audio input and renders the tree from the block's MIDI.
pub struct KeysInstrument {
    render: RenderNode,
    prepared: bool,
    /// Master output gain (linear, f32 bits), shared with the owning [`KeysRig`]
    /// so it's adjustable live — a summed multi-layer patch can otherwise clip.
    gain: Arc<std::sync::atomic::AtomicU32>,
    /// Per-Engine/Layer live fader cells from the compile — the mixer's
    /// handles on this instrument.
    cells: GainCells,
}

impl KeysInstrument {
    pub fn new(tree: &Container, sample_rate: u32) -> Self {
        Self::with_gain(
            tree,
            sample_rate,
            Arc::new(std::sync::atomic::AtomicU32::new(1.0f32.to_bits())),
        )
    }

    /// As [`new`](Self::new) but sharing an external master-gain cell.
    pub fn with_gain(
        tree: &Container,
        sample_rate: u32,
        gain: Arc<std::sync::atomic::AtomicU32>,
    ) -> Self {
        let (render, cells) = RenderNode::compile_with_cells(tree, sample_rate);
        Self {
            render,
            prepared: false,
            gain,
            cells,
        }
    }

    /// The live fader cells for this instrument's engines + layers.
    pub fn gain_cells(&self) -> GainCells {
        self.cells.clone()
    }

    /// The compiled render tree — the live-edit surface
    /// ([`RenderNode::set_leaf_param`] & friends). Control-thread only,
    /// reached through the host's plugin-map lock
    /// (see `KeysRig::edit_lane`).
    pub fn render_mut(&mut self) -> &mut RenderNode {
        &mut self.render
    }
}

impl PluginInstance for KeysInstrument {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "signal.keys.instrument".into(),
            name: "Keys".into(),
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
    fn prepare(&mut self, sample_rate: f64, block_size: u32) -> Result<(), PluginError> {
        self.render.prepare(sample_rate, block_size);
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
        self.render.process(in_l, in_r, out_l, out_r, events);
        let gain = f32::from_bits(self.gain.load(std::sync::atomic::Ordering::Relaxed));
        if (gain - 1.0).abs() > 1e-4 {
            for s in out_l.iter_mut().chain(out_r.iter_mut()) {
                *s *= gain;
            }
        }
        Ok(())
    }
    fn deactivate(&mut self) {
        self.prepared = false;
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

// ── Lane programs (per-layer daw tracks) ────────────────────────────────────

/// One layer's compiled program in lane mode: its own composition subtree
/// (zone included), hosted as one [`KeysInstrument`] on its own daw track.
pub struct LaneLayer {
    pub name: String,
    pub tree: Container,
}

/// One engine — a daw **folder track** whose children are its layer tracks
/// (folder summing IS the engine's mix bus; the folder fader/mute/solo are
/// the engine's mixer controls).
pub struct LaneEngine {
    pub name: String,
    pub layers: Vec<LaneLayer>,
}

/// A whole profile as per-lane daw tracks: engines of layers under one rig
/// folder, plus an optional serial FX tail (e.g. the master reverb) hosted on
/// the rig folder track — it processes the folder-summed rig exactly where
/// the single-tree mode's trailing Global module did.
pub struct LaneProgram {
    pub name: String,
    pub engines: Vec<LaneEngine>,
    pub tail: Option<Container>,
}

impl LaneProgram {
    /// Engine/layer shape signature — when it matches the hosted layout, a
    /// reload is a glitch-free per-track instrument swap; otherwise the track
    /// set is rebuilt.
    fn shape(&self) -> Vec<(String, Vec<String>)> {
        self.engines
            .iter()
            .map(|e| (e.name.clone(), e.layers.iter().map(|l| l.name.clone()).collect()))
            .collect()
    }
}

/// The hosted track set in lane mode.
struct LaneHost {
    /// The rig folder track (master fader + tail FX + rig meter cell 0).
    rig_guid: String,
    /// FX slot on the rig track carrying the tail chain, if any.
    tail_fx: Option<String>,
    /// Engine folder tracks: `(name, guid, meter index)`.
    engines: Vec<(String, String, usize)>,
    layers: Vec<LaneTrack>,
}

/// One layer's daw track.
struct LaneTrack {
    engine: String,
    name: String,
    guid: String,
    fx: String,
    meter: usize,
    /// The lane instrument's module fader/peak cells (module mixing stays
    /// in-tree; layer/engine mixing is the daw track).
    cells: GainCells,
}

/// How the rig hosts its program.
enum Hosting {
    /// One track, one instrument compiled from the whole tree (the synth
    /// rig's mode, and the fallback for trees that aren't engine/layer
    /// shaped — cross-layer sends, mod buses).
    Single {
        track_guid: String,
        fx_guid: String,
        /// Live Engine/Layer/Module fader cells for the loaded program.
        cells: GainCells,
    },
    /// Per-layer daw tracks under engine folders — the mixer IS daw.
    Lanes(LaneHost),
}

/// A live, device-backed keys rig playing a composition-tree preset.
pub struct KeysRig {
    daw: Standalone,
    /// The shared daw host (project + output engine + transport); drop =
    /// stop audio.
    _host: RigHost,
    hosting: Hosting,
    meters: Arc<Meters>,
    sample_rate: u32,
    preset_name: String,
    midi_monitor: MidiMonitor,
    /// Mirror of the master output gain (linear, f32 bits) — the gain itself
    /// is a daw fader (the keys track in single mode, the rig folder in lane
    /// mode), so it survives preset swaps for free.
    gain: Arc<std::sync::atomic::AtomicU32>,
}

impl KeysRig {
    /// Open a device, build the project, and host `tree` as the playable preset.
    // r[impl keys.rig.composition-tree]
    // r[impl keys.rig.output-only]
    pub fn open(prefs: &AudioIoPrefs, tree: &Container) -> eyre::Result<Self> {
        // One instrument track carrying the KeysInstrument, on the shared
        // output-only host (a synth generates, never records).
        let project = RigProject::new(KEYS_PROJECT_NAME);
        let track_guid = project.add_track(KEYS_TRACK_NAME)?;
        let fx_guid = project.add_fx_slot(&track_guid, "keys")?;
        let host = project.start_output(prefs)?;
        let sample_rate = host.sample_rate();
        let meters = host.install_meters(1);
        let daw = host.daw().clone();

        // Compile + install the preset instrument. The master gain is the
        // track's daw fader (not baked into the instrument), so it applies
        // post-instrument and carries across preset swaps.
        let gain = Arc::new(std::sync::atomic::AtomicU32::new(1.0f32.to_bits()));
        let mut inst = KeysInstrument::new(tree, sample_rate);
        let cells = inst.gain_cells();
        let _ = inst.prepare(sample_rate as f64, PREPARE_BLOCK);
        daw.insert_plugin_instance(fx_guid.clone(), Box::new(inst));

        host.play();
        tracing::info!(sample_rate, preset = %tree.name, "keys rig started on daw engine");

        Ok(Self {
            daw,
            _host: host,
            hosting: Hosting::Single { track_guid, fx_guid, cells },
            meters,
            sample_rate,
            preset_name: tree.name.clone(),
            midi_monitor: MidiMonitor::default(),
            gain,
        })
    }

    /// Open a device and host `program` as per-layer daw tracks — the fully
    /// daw-based mixer: engine folders sum their layer tracks, the rig folder
    /// sums the engines (and carries the tail FX + master fader), and every
    /// fader/mute/solo is a daw track op the renderer resolves natively
    /// (folder-aware solo, folder mute, per-track post-fader meters).
    // r[impl keys.rig.lane-tracks]
    pub fn open_lanes(prefs: &AudioIoPrefs, program: &LaneProgram) -> eyre::Result<Self> {
        let project = RigProject::new(KEYS_PROJECT_NAME);
        let lanes = build_lane_tracks(project.daw(), program)?;
        let track_count = 1 + lanes.engines.len() + lanes.layers.len();
        let host = project.start_output(prefs)?;
        let sample_rate = host.sample_rate();
        let meters = host.install_meters(track_count);
        let daw = host.daw().clone();

        let gain = Arc::new(std::sync::atomic::AtomicU32::new(1.0f32.to_bits()));
        let mut rig = Self {
            daw,
            _host: host,
            hosting: Hosting::Lanes(lanes),
            meters,
            sample_rate,
            preset_name: program.name.clone(),
            midi_monitor: MidiMonitor::default(),
            gain,
        };
        rig.install_lane_instruments(program);
        rig._host.play();
        tracing::info!(
            sample_rate,
            profile = %program.name,
            engines = program.engines.len(),
            layers = program.engines.iter().map(|e| e.layers.len()).sum::<usize>(),
            "keys rig started on daw engine (lane tracks)"
        );
        Ok(rig)
    }

    /// True when the rig hosts per-layer daw tracks.
    pub fn is_lanes(&self) -> bool {
        matches!(self.hosting, Hosting::Lanes(_))
    }

    /// Compile + insert every lane's instrument (and the tail) — glitch-free
    /// per-track swaps under the renderer lock.
    fn install_lane_instruments(&mut self, program: &LaneProgram) {
        let sr = self.sample_rate;
        let Hosting::Lanes(lanes) = &mut self.hosting else { return };
        for engine in &program.engines {
            for layer in &engine.layers {
                let Some(track) = lanes
                    .layers
                    .iter_mut()
                    .find(|t| t.engine == engine.name && t.name == layer.name)
                else {
                    continue;
                };
                let mut inst = KeysInstrument::new(&layer.tree, sr);
                track.cells = inst.gain_cells();
                let _ = inst.prepare(sr as f64, PREPARE_BLOCK);
                self.daw.insert_plugin_instance(track.fx.clone(), Box::new(inst));
            }
        }
        if let (Some(fx), Some(tail)) = (&lanes.tail_fx, &program.tail) {
            let mut inst = KeysInstrument::new(tail, sr);
            let _ = inst.prepare(sr as f64, PREPARE_BLOCK);
            self.daw.insert_plugin_instance(fx.clone(), Box::new(inst));
        }
    }

    /// Load a lane program. Same engine/layer shape → per-track instrument
    /// swaps (no audio gap). A different shape (profile edit) rebuilds the
    /// track set — a brief gap, on an explicit editing action.
    pub fn load_lanes(&mut self, program: &LaneProgram) -> eyre::Result<()> {
        let same_shape = match &self.hosting {
            Hosting::Lanes(l) => {
                let hosted: Vec<(String, Vec<String>)> = l
                    .engines
                    .iter()
                    .map(|(name, _, _)| {
                        (
                            name.clone(),
                            l.layers
                                .iter()
                                .filter(|t| &t.engine == name)
                                .map(|t| t.name.clone())
                                .collect(),
                        )
                    })
                    .collect();
                hosted == program.shape()
            }
            Hosting::Single { .. } => false,
        };
        if !same_shape {
            // Rebuild the whole track set on the running project.
            self.daw
                .current()
                .remove_all()
                .map_err(|e| eyre::eyre!("keys rig: clear tracks failed: {e}"))?;
            let lanes = build_lane_tracks(&self.daw, program)?;
            let track_count = 1 + lanes.engines.len() + lanes.layers.len();
            self.meters = self._host.install_meters(track_count);
            self.hosting = Hosting::Lanes(lanes);
            // The master fader carries over onto the fresh rig track.
            self.set_output_gain(self.output_gain());
        }
        self.install_lane_instruments(program);
        self.preset_name = program.name.clone();
        Ok(())
    }

    // ── Lane mixer (daw track ops; no-ops in single mode) ────────────────

    /// A lane track's guid by mixer address.
    fn lane_guid(&self, role: Role, name: &str) -> Option<&str> {
        let Hosting::Lanes(l) = &self.hosting else { return None };
        match role {
            Role::Engine => l
                .engines
                .iter()
                .find(|(n, _, _)| n == name)
                .map(|(_, g, _)| g.as_str()),
            Role::Layer => l.layers.iter().find(|t| t.name == name).map(|t| t.guid.as_str()),
            _ => None,
        }
    }

    /// Set a lane's fader (linear; the daw track volume).
    pub fn set_lane_volume(&self, role: Role, name: &str, linear: f32) {
        if let Some(guid) = self.lane_guid(role, name) {
            let _ = self.daw.current().track(guid).set_volume(linear.max(0.0) as f64);
        }
    }

    /// Mute a lane (daw track mute; muting an engine folder mutes its sum).
    pub fn set_lane_mute(&self, role: Role, name: &str, muted: bool) {
        if let Some(guid) = self.lane_guid(role, name) {
            let _ = self.daw.current().track(guid).set_muted(muted);
        }
    }

    /// Solo a lane (daw folder-aware solo: ancestors pass, siblings drop).
    pub fn set_lane_solo(&self, role: Role, name: &str, soloed: bool) {
        if let Some(guid) = self.lane_guid(role, name) {
            let _ = self.daw.current().track(guid).set_soloed(soloed);
        }
    }

    /// A lane instrument's module cells (module faders stay in-tree).
    pub fn lane_cells(&self, layer: &str) -> Option<GainCells> {
        let Hosting::Lanes(l) = &self.hosting else { return None };
        l.layers.iter().find(|t| t.name == layer).map(|t| t.cells.clone())
    }

    /// Run `f` against the live [`KeysInstrument`] hosting `layer` — the
    /// realtime parameter-edit seam (filter cutoff, envelope ADSR, unison…),
    /// serialized against the renderer by the host's plugin-map lock. In
    /// single-tree mode the one instrument hosts every layer, so the layer
    /// name only picks the fx slot in lane mode.
    pub fn edit_lane<R>(
        &self,
        layer: &str,
        f: impl FnOnce(&mut KeysInstrument) -> R,
    ) -> Option<R> {
        let fx = match &self.hosting {
            Hosting::Single { fx_guid, .. } => fx_guid.clone(),
            Hosting::Lanes(l) => l
                .layers
                .iter()
                .find(|t| t.name == layer)
                .map(|t| t.fx.clone())?,
        };
        self.daw
            .with_plugin_instance(&fx, |inst| {
                inst.as_any_mut()
                    .and_then(|any| any.downcast_mut::<KeysInstrument>())
                    .map(f)
            })
            .flatten()
    }

    /// The live fader cells for the loaded program's engines + layers
    /// (single mode; a mixer writes linear gain here — no rebuild, no audio
    /// gap). Empty in lane mode — the mixer is daw tracks there; module
    /// cells come per lane from [`lane_cells`](Self::lane_cells).
    pub fn gain_cells(&self) -> GainCells {
        match &self.hosting {
            Hosting::Single { cells, .. } => cells.clone(),
            Hosting::Lanes(_) => GainCells::default(),
        }
    }

    /// Set the master output gain (linear; 1.0 = unity). Applied as a daw
    /// fader — the keys track in single mode, the rig folder in lane mode —
    /// so it takes effect on the next block and survives preset swaps.
    pub fn set_output_gain(&self, gain: f32) {
        let gain = gain.max(0.0);
        self.gain
            .store(gain.to_bits(), std::sync::atomic::Ordering::Relaxed);
        let guid = match &self.hosting {
            Hosting::Single { track_guid, .. } => track_guid,
            Hosting::Lanes(l) => &l.rig_guid,
        };
        let _ = self.daw.current().track(guid).set_volume(gain as f64);
    }

    /// Current master output gain (linear).
    pub fn output_gain(&self) -> f32 {
        f32::from_bits(self.gain.load(std::sync::atomic::Ordering::Relaxed))
    }

    /// Swap the playable preset (glitch-free re-insert under the renderer
    /// lock). The master gain lives on the track fader, so it carries over.
    /// Single mode only — lane mode reloads via [`load_lanes`](Self::load_lanes).
    pub fn load_preset(&mut self, tree: &Container) {
        let sr = self.sample_rate;
        let Hosting::Single { fx_guid, cells, .. } = &mut self.hosting else {
            tracing::warn!("keys rig: load_preset ignored in lane mode — use load_lanes");
            return;
        };
        let mut inst = KeysInstrument::new(tree, sr);
        // The new program owns new cells — hand them out before it goes live.
        *cells = inst.gain_cells();
        let _ = inst.prepare(sr as f64, PREPARE_BLOCK);
        self.daw.insert_plugin_instance(fx_guid.clone(), Box::new(inst));
        self.preset_name = tree.name.clone();
    }

    // ── MIDI driving ─────────────────────────────────────────────────────

    /// The tracks live MIDI is delivered to: the one keys track, or every
    /// layer track (each lane's zone filters its own notes, exactly as the
    /// single tree's Zoned nodes did).
    fn midi_targets(&self) -> Vec<String> {
        match &self.hosting {
            Hosting::Single { track_guid, .. } => vec![track_guid.clone()],
            Hosting::Lanes(l) => l.layers.iter().map(|t| t.guid.clone()).collect(),
        }
    }

    fn dispatch(&self, msg: midicore::MidiEvent) {
        for track in self.midi_targets() {
            self.daw.push_live_midi(&track, msg.clone());
        }
    }

    pub fn note_on(&self, note: u8, velocity: u8) {
        self.dispatch(ev_note_on(note, velocity));
    }
    pub fn note_off(&self, note: u8) {
        self.dispatch(ev_note_off(note));
    }
    pub fn cc(&self, controller: u8, value: u8) {
        self.dispatch(ev_cc(controller, value));
    }
    /// Pitch wheel (14-bit raw, 8192 = center) — reaches every lane; synth
    /// voices bend per their `bend_range`, sampled voices per the engine's,
    /// percussion ignores it.
    pub fn pitch_bend(&self, raw: u16) {
        use midicore::{Channel, MidiEvent, PitchBend};
        self.dispatch(MidiEvent::PitchBend {
            channel: Channel::new(0),
            bend: PitchBend::new(raw.min(16_383)),
        });
    }
    /// All Notes Off (CC 123).
    pub fn all_notes_off(&self) {
        self.dispatch(ev_cc(123, 0));
    }
    /// Panic — All Sound Off (CC 120).
    pub fn panic(&self) {
        self.dispatch(ev_cc(120, 0));
    }

    /// Enumerate hardware MIDI input ports.
    pub fn midi_input_ports() -> Vec<String> {
        midicore::midir::input_ports()
    }

    /// Open a hardware MIDI keyboard and forward its events into the rig
    /// (monitor tap + live-MIDI sink wired by `midicore::attach`).
    pub fn attach_midi(
        &self,
        selection: midicore::PortSelector,
    ) -> eyre::Result<midicore::midir::MidiInput> {
        let daw = self.daw.clone();
        let targets = self.midi_targets();
        let sink = midicore::attach::tap_sink(self.midi_monitor.clone(), move |ev| {
            for track in &targets {
                daw.push_live_midi(track, ev.clone());
            }
        });
        midicore::midir::MidiInput::open(selection, sink)
    }

    pub fn midi_monitor(&self) -> MidiMonitor {
        self.midi_monitor.clone()
    }

    // ── Read-side ────────────────────────────────────────────────────────

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn preset_name(&self) -> &str {
        &self.preset_name
    }

    /// Post-fader output peak (linear) of the rig's output track (the keys
    /// track in single mode, the rig folder in lane mode), for a level meter.
    pub fn output_peak(&self) -> f32 {
        self.meters
            .cell(0)
            .map(|c| c.peak(0).max(c.peak(1)))
            .unwrap_or(0.0)
    }

    fn meter_peak(&self, idx: usize) -> f32 {
        self.meters
            .cell(idx)
            .map(|c| c.peak(0).max(c.peak(1)))
            .unwrap_or(0.0)
    }

    /// Every metered container in the loaded program — engines, layers and
    /// modules — with its post-fader peak (linear). The mixer's meters, at the
    /// same `(role, name)` addresses its faders use: an engine and a lane can
    /// share a name, so neither half of that pair is an address on its own.
    ///
    /// Single mode reads the in-tree cells; lane mode reads engine/layer
    /// peaks off the daw track meters (post-fader, the renderer's own
    /// metering) and module peaks off each lane instrument's cells.
    pub fn cell_peaks(&self) -> Vec<(Role, String, f32)> {
        match &self.hosting {
            Hosting::Single { cells, .. } => cells.peaks(),
            Hosting::Lanes(l) => {
                let mut out = Vec::new();
                for (name, _, meter) in &l.engines {
                    out.push((Role::Engine, name.clone(), self.meter_peak(*meter)));
                }
                for t in &l.layers {
                    out.push((Role::Layer, t.name.clone(), self.meter_peak(t.meter)));
                    out.extend(
                        t.cells
                            .peaks()
                            .into_iter()
                            .filter(|(role, _, _)| *role == Role::Module),
                    );
                }
                out
            }
        }
    }
}

/// Build the lane track set on a seeded project: the rig folder, engine
/// folders, layer tracks (each with one reserved FX slot), and the tail FX
/// slot on the rig track. The daw handle layer's [`TrackTree`] owns the
/// REAPER folder-depth bookkeeping; an engine folder that ends up with no
/// layers collapses to a plain (silent) track automatically.
///
/// [`TrackTree`]: daw::service::handle::TrackTree
fn build_lane_tracks(daw: &Standalone, program: &LaneProgram) -> eyre::Result<LaneHost> {
    let project = daw.current();
    let mut tree = project.tree();
    let err = |what: &str, e: daw::service::DawError| eyre::eyre!("keys rig: {what} failed: {e}");

    // Meter cells are indexed by project track order.
    let mut meter = 0usize;
    let rig_name = if program.name.is_empty() { "Keys Rig" } else { &program.name };
    let rig = tree.folder(rig_name).map_err(|e| err("rig folder", e))?;
    let tail_fx = match &program.tail {
        Some(_) => Some(
            rig.add_fx_slot("keys-tail")
                .map_err(|e| err("tail fx slot", e))?
                .into_guid(),
        ),
        None => None,
    };
    let rig_guid = rig.guid().to_string();
    meter += 1;

    let mut engines = Vec::new();
    let mut layers = Vec::new();
    for engine in &program.engines {
        let eng = tree.folder(&engine.name).map_err(|e| err("engine folder", e))?;
        engines.push((engine.name.clone(), eng.guid().to_string(), meter));
        meter += 1;
        for layer in &engine.layers {
            let track = tree.track(&layer.name).map_err(|e| err("layer track", e))?;
            let fx = track
                .add_fx_slot("keys-lane")
                .map_err(|e| err("lane fx slot", e))?
                .into_guid();
            layers.push(LaneTrack {
                engine: engine.name.clone(),
                name: layer.name.clone(),
                guid: track.guid().to_string(),
                fx,
                meter,
                cells: GainCells::default(),
            });
            meter += 1;
        }
        tree.end().map_err(|e| err("close engine folder", e))?;
    }
    tree.finish().map_err(|e| err("close rig folder", e))?;

    Ok(LaneHost { rig_guid, tail_fx, engines, layers })
}

#[cfg(test)]
mod tests {
    use super::*;
    use signal_plugin_host::PluginMidiEvent;

    /// Device-free: the lane track builder produces the rig/engine/layer
    /// folder layout with balanced folder depths and order-stable meter
    /// indices.
    #[test]
    fn lane_tracks_build_the_folder_layout() {
        use daw::service::{ProjectContext, Tracks};
        use signal_rig_host::RigProject;

        let program = LaneProgram {
            name: "Worship".into(),
            engines: vec![
                LaneEngine {
                    name: "Keys".into(),
                    layers: vec![
                        LaneLayer { name: "Piano".into(), tree: Container::layer("Piano") },
                        LaneLayer { name: "Pad".into(), tree: Container::layer("Pad") },
                    ],
                },
                LaneEngine {
                    name: "Organ".into(),
                    layers: vec![LaneLayer {
                        name: "Organ".into(),
                        tree: Container::layer("Organ"),
                    }],
                },
            ],
            tail: Some(Container::module("Global")),
        };
        let project = RigProject::new("Lane Layout Test");
        let lanes = build_lane_tracks(project.daw(), &program).expect("lane tracks build");

        assert_eq!(lanes.engines.len(), 2);
        assert_eq!(lanes.layers.len(), 3);
        assert!(lanes.tail_fx.is_some(), "tail chain gets a rig-track fx slot");

        let tracks = <Standalone as Tracks>::all(project.daw(), ProjectContext::Current);
        let names: Vec<&str> = tracks.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Worship", "Keys", "Piano", "Pad", "Organ", "Organ"]);
        // Meter indices follow project track order.
        assert_eq!(lanes.engines[0].2, 1);
        assert_eq!(lanes.layers[0].meter, 2);
        assert_eq!(lanes.layers[2].meter, 5);
        // Folder depths: rig + engines open, last children close — balanced
        // overall (sum of depths = 0 ⇒ every folder is closed).
        let depths: Vec<i32> = tracks.iter().map(|t| t.folder_depth).collect();
        assert_eq!(depths, [1, 1, 0, -1, 1, -2]);
        assert_eq!(depths.iter().sum::<i32>(), 0);
    }

    /// The instrument renders a preset's tree from MIDI — device-free, so it runs
    /// in CI. Plays a note through the layering demo and checks it's audible.
    #[test]
    fn keys_instrument_renders_a_preset_from_midi() {
        let preset = crate::nord::layering_demo();
        let mut inst = KeysInstrument::new(&preset, 48_000);
        inst.prepare(48_000.0, 256).unwrap();

        let (inl, inr) = (vec![0.0f32; 256], vec![0.0f32; 256]);
        let (mut outl, mut outr) = (vec![0.0f32; 256], vec![0.0f32; 256]);
        // A right-hand note, hard hit → the Bright Lead layer sounds.
        let midi = [PluginMidiEvent {
            offset: 0,
            message: ev_note_on(72, 110),
        }];
        let ev = PluginEvents {
            params: &[],
            midi: &midi,
            note_expressions: &[],
        };
        inst.process_block(&inl, &inr, &mut outl, &mut outr, &ev)
            .unwrap();
        let rms = (outl.iter().map(|s| s * s).sum::<f32>() / 256.0).sqrt();
        assert!(
            rms > 1e-3,
            "keys instrument should render audible output, rms={rms}"
        );
    }

    /// Machine-local: the full `just keys` render path plays the Keyscape-
    /// realized program (piano through the whole Nord tree). Run explicitly:
    /// `cargo test -p signal-sampler --lib piano_program -- --ignored`
    #[test]
    #[ignore = "requires the local Keyscape extraction on AudioHaven"]
    fn keys_instrument_renders_the_piano_program() {
        let Some(preset) = crate::nord::nord_stage_piano_preset() else {
            eprintln!("skipping: Keyscape extraction not present");
            return;
        };
        let mut inst = KeysInstrument::new(&preset, 48_000);
        inst.prepare(48_000.0, 512).unwrap();

        let (inl, inr) = (vec![0.0f32; 512], vec![0.0f32; 512]);
        let (mut outl, mut outr) = (vec![0.0f32; 512], vec![0.0f32; 512]);
        let midi = [PluginMidiEvent {
            offset: 0,
            message: ev_note_on(60, 100),
        }];
        // Samples decode on a background thread — retrigger until audible.
        let mut heard = 0.0f32;
        for _ in 0..600 {
            let ev = PluginEvents {
                params: &[],
                midi: &midi,
                note_expressions: &[],
            };
            inst.process_block(&inl, &inr, &mut outl, &mut outr, &ev)
                .unwrap();
            let rms = (outl.iter().map(|s| s * s).sum::<f32>() / 512.0).sqrt();
            heard = heard.max(rms);
            if heard > 1e-3 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(heard > 1e-3, "piano program should be audible, rms={heard}");
    }
}
