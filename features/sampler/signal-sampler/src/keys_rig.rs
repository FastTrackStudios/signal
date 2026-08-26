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
use daw::standalone::metering::Meters;
use daw::standalone::Standalone;
#[cfg(not(target_arch = "wasm32"))]
use daw_audio_io::AudioIoPrefs;
use signal_plugin_host::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};
#[cfg(not(target_arch = "wasm32"))]
use signal_rig_host::RigHost;
use signal_rig_host::RigProject;

use crate::node_render::{GainCells, RenderNode};
use crate::rig_node::{Container, Role};
use crate::MidiMonitor;

/// Whether any sampler block in `tree` plays the library at `spec_path` —
/// the test for "does this lane care that this pack just arrived?" (see
/// [`KeysRig::reload_lanes_for_pack`]).
fn tree_uses_sample(tree: &Container, spec_path: &str) -> bool {
    use crate::rig_node::RigNode;
    tree.children.iter().any(|child| match child {
        RigNode::Block { block } => block.sample == spec_path,
        RigNode::Container { container } => tree_uses_sample(container, spec_path),
    })
}

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
#[cfg(not(target_arch = "wasm32"))]
const KEYS_TRACK_NAME: &str = "Keys";

/// A composition-tree preset wrapped as a daw instrument. The renderer runs it
/// each block; it ignores audio input and renders the tree from the block's MIDI.
pub struct KeysInstrument {
    render: RenderNode,
    /// The PREVIOUS render tree, still sounding its release tails after a
    /// swap. Kept until its last voice finishes, then dropped.
    ///
    /// Replacing the tree outright is what makes a patch change audible:
    /// every note still ringing is cut mid-tail. A piano or a pad swapped
    /// that way chops. Summing the outgoing tree alongside the new one for
    /// the few hundred ms its voices need is the difference between a
    /// switch you hear and one you do not.
    retiring: Option<RenderNode>,
    /// Scratch for the retiring tree's output (it sums into the main bus).
    /// Pre-allocated at `prepare`; the audio thread never allocates.
    retire_l: Vec<f32>,
    retire_r: Vec<f32>,
    /// What `prepare` was called with, so a tree swapped in later can be
    /// prepared identically without waiting for the host to call again.
    prepared_rate: f64,
    prepared_block: u32,
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
            retiring: None,
            retire_l: Vec::new(),
            retire_r: Vec::new(),
            prepared_rate: 48_000.0,
            prepared_block: PREPARE_BLOCK,
            prepared: false,
            gain,
            cells,
        }
    }

    /// Swap in an ALREADY-COMPILED render tree, keeping the outgoing one
    /// alive until its voices finish — a gapless patch change.
    ///
    /// The expensive half (compiling the tree, opening its zones) happens
    /// wherever the caller built `next`; on wasm that is a worker thread,
    /// so the audio thread only does this: two moves and a `Vec` swap.
    /// Notes held through the change keep sounding on the old tree while
    /// new notes play the new one.
    ///
    /// A swap arriving while a previous one is still retiring drops the
    /// older tail (two generations is already a stretch; three is a leak).
    pub fn begin_swap(&mut self, mut next: RenderNode, cells: GainCells) -> Option<RenderNode> {
        if self.prepared {
            // Match what `prepare` did, so the incoming tree can render on
            // the very next block.
            next.prepare(self.prepared_rate, self.prepared_block);
        }
        let previous = std::mem::replace(&mut self.render, next);
        self.cells = cells;
        // Whatever was already retiring has to go somewhere: hand it back
        // so a non-realtime thread can drop it. Dropping HERE would free on
        // the audio thread, which is the priority-inversion hazard this
        // whole design avoids.
        let displaced = self.retiring.take();
        self.retiring = Some(previous);
        displaced
    }

    /// Take the retired tree once it has finished sounding, so the caller
    /// can drop it somewhere that is allowed to free.
    pub fn take_retired_if_silent(&mut self) -> Option<RenderNode> {
        let done = self
            .retiring
            .as_mut()
            .map(|r| r.active_voices() == 0)
            .unwrap_or(false);
        if done {
            self.retiring.take()
        } else {
            None
        }
    }

    /// Whether a swapped-out tree is still sounding.
    pub fn is_retiring(&self) -> bool {
        self.retiring.is_some()
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
        // Scratch for a retiring tree, sized once here — `process_block`
        // runs on the audio thread and must never allocate.
        self.retire_l.resize(block_size as usize, 0.0);
        self.retire_r.resize(block_size as usize, 0.0);
        self.prepared_rate = sample_rate;
        self.prepared_block = block_size;
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
        // A tree swapped out is still finishing its notes: render it into
        // scratch and sum. It gets NO new events — new notes belong to the
        // new tree — so it drains as its voices release, and is dropped the
        // moment none are left.
        if let Some(retiring) = self.retiring.as_mut() {
            let n = out_l.len().min(self.retire_l.len());
            if n > 0 {
                self.retire_l[..n].fill(0.0);
                self.retire_r[..n].fill(0.0);
                let quiet = PluginEvents::default();
                retiring.process(
                    &in_l[..n.min(in_l.len())],
                    &in_r[..n.min(in_r.len())],
                    &mut self.retire_l[..n],
                    &mut self.retire_r[..n],
                    &quiet,
                );
                for i in 0..n {
                    out_l[i] += self.retire_l[i];
                    out_r[i] += self.retire_r[i];
                }
            }
            // NOTE: finished trees are NOT dropped here. `self.retiring`
            // is emptied by `take_retired_if_silent`, called from the
            // install path, which hands the tree to a worker to free —
            // freeing on this thread takes the shared allocator lock.
            let _ = retiring;
        }
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
            .map(|e| {
                (
                    e.name.clone(),
                    e.layers.iter().map(|l| l.name.clone()).collect(),
                )
            })
            .collect()
    }
}

// ── The LaneProgram wire shape ──────────────────────────────────────────────
//
// `LaneProgram` itself carries no derives (it holds compiled trees), so the
// browser boundary speaks a Facet mirror: the engine serializes one to JSON
// (`KeysRig::lane_program_wire` on the keys service) and the worklet
// (`signal-keys-worklet`) parses the same JSON back. The `tree` leaves are
// the same `Container` the native styx profiles serialize, so the mirror
// lives HERE — the one crate both the backend (signal-keys) and the worklet
// depend on.

/// One layer: its name + composition subtree (zone included).
#[derive(Debug, Clone, facet::Facet)]
pub struct WireLayer {
    pub name: String,
    pub tree: Container,
}

/// One engine: a folder of layers.
#[derive(Debug, Clone, facet::Facet)]
pub struct WireEngine {
    pub name: String,
    pub layers: Vec<WireLayer>,
}

/// A whole lane program — the JSON the worklet's `open_lanes` message
/// carries.
#[derive(Debug, Clone, facet::Facet)]
pub struct WireProgram {
    pub name: String,
    pub engines: Vec<WireEngine>,
    pub tail: Option<Container>,
}

impl WireProgram {
    /// Mirror a compiled [`LaneProgram`] for the wire.
    pub fn from_program(p: &LaneProgram) -> Self {
        WireProgram {
            name: p.name.clone(),
            engines: p
                .engines
                .iter()
                .map(|e| WireEngine {
                    name: e.name.clone(),
                    layers: e
                        .layers
                        .iter()
                        .map(|l| WireLayer {
                            name: l.name.clone(),
                            tree: l.tree.clone(),
                        })
                        .collect(),
                })
                .collect(),
            tail: p.tail.clone(),
        }
    }

    /// Convert into the rig's native [`LaneProgram`].
    pub fn into_lane_program(self) -> LaneProgram {
        LaneProgram {
            name: self.name,
            engines: self
                .engines
                .into_iter()
                .map(|e| LaneEngine {
                    name: e.name,
                    layers: e
                        .layers
                        .into_iter()
                        .map(|l| LaneLayer {
                            name: l.name,
                            tree: l.tree,
                        })
                        .collect(),
                })
                .collect(),
            tail: self.tail,
        }
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
// wasm32 builds only ever construct `Lanes` (single-tree `open` is a
// native, device-backed entry) — the variant still participates in every
// match, so keep it rather than cfg-splitting the enum.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
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
    /// stop audio. `None` for a headless rig
    /// ([`open_headless`](Self::open_headless)) — the caller owns rendering.
    #[cfg(not(target_arch = "wasm32"))]
    _host: Option<RigHost>,
    /// The rig project's guid — a headless caller renders it through daw's
    /// own render path (`ProjectRenderer`).
    project_guid: String,
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
    #[cfg(not(target_arch = "wasm32"))]
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

        let project_guid = host.project_guid().to_string();
        Ok(Self {
            daw,
            _host: Some(host),
            project_guid,
            hosting: Hosting::Single {
                track_guid,
                fx_guid,
                cells,
            },
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
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open_lanes(prefs: &AudioIoPrefs, program: &LaneProgram) -> eyre::Result<Self> {
        let project = RigProject::new(KEYS_PROJECT_NAME);
        let lanes = build_lane_tracks(project.daw(), program)?;
        let track_count = 1 + lanes.engines.len() + lanes.layers.len();
        let host = project.start_output(prefs)?;
        let sample_rate = host.sample_rate();
        let meters = host.install_meters(track_count);
        let daw = host.daw().clone();

        let gain = Arc::new(std::sync::atomic::AtomicU32::new(1.0f32.to_bits()));
        let project_guid = host.project_guid().to_string();
        let mut rig = Self {
            daw,
            _host: Some(host),
            project_guid,
            hosting: Hosting::Lanes(lanes),
            meters,
            sample_rate,
            preset_name: program.name.clone(),
            midi_monitor: MidiMonitor::default(),
            gain,
        };
        rig.install_lane_instruments(program);
        if let Some(host) = &rig._host {
            host.play();
        }
        tracing::info!(
            sample_rate,
            profile = %program.name,
            engines = program.engines.len(),
            layers = program.engines.iter().map(|e| e.layers.len()).sum::<usize>(),
            "keys rig started on daw engine (lane tracks)"
        );
        Ok(rig)
    }

    /// Host `program` as per-layer daw tracks WITHOUT opening any audio
    /// device — the SAME `Hosting::Lanes` topology as
    /// [`open_lanes`](Self::open_lanes) (rig folder → engine folders →
    /// layer tracks, one [`KeysInstrument`] per lane installed through the
    /// plugin seam), but the caller owns rendering: natively a
    /// `ProjectRenderer` over [`daw`](Self::daw) +
    /// [`project_guid`](Self::project_guid); in the browser the AudioWorklet
    /// (see [`open_headless_on`](Self::open_headless_on)).
    ///
    /// Lane packs resolve by spec path as usual; a browser (or any caller
    /// with no filesystem) installs in-memory packs first via
    /// [`crate::pack_registry::install`] — `build_sample_source` consults
    /// the registry before touching disk.
    pub fn open_headless(sample_rate: u32, program: &LaneProgram) -> eyre::Result<Self> {
        let project = RigProject::new(KEYS_PROJECT_NAME);
        Self::open_headless_impl(project, sample_rate, program)
    }

    /// [`open_headless`](Self::open_headless) on an EXISTING `Standalone` —
    /// the browser worklet path: the rig project is seeded into the
    /// worklet's own daw (the one its `WebRenderer` renders), so a project
    /// select + transport play makes the lanes live.
    pub fn open_headless_on(
        daw: &Standalone,
        sample_rate: u32,
        program: &LaneProgram,
    ) -> eyre::Result<Self> {
        let project = RigProject::on(daw, KEYS_PROJECT_NAME);
        Self::open_headless_impl(project, sample_rate, program)
    }

    fn open_headless_impl(
        project: RigProject,
        sample_rate: u32,
        program: &LaneProgram,
    ) -> eyre::Result<Self> {
        let lanes = build_lane_tracks(project.daw(), program)?;
        let track_count = 1 + lanes.engines.len() + lanes.layers.len();
        let daw = project.daw().clone();
        let meters = Meters::new(track_count);
        daw.set_meters(meters.clone());

        let gain = Arc::new(std::sync::atomic::AtomicU32::new(1.0f32.to_bits()));
        let mut rig = Self {
            daw,
            #[cfg(not(target_arch = "wasm32"))]
            _host: None,
            project_guid: project.project_guid().to_string(),
            hosting: Hosting::Lanes(lanes),
            meters,
            sample_rate,
            preset_name: program.name.clone(),
            midi_monitor: MidiMonitor::default(),
            gain,
        };
        rig.install_lane_instruments(program);
        tracing::info!(
            sample_rate,
            profile = %program.name,
            engines = program.engines.len(),
            layers = program.engines.iter().map(|e| e.layers.len()).sum::<usize>(),
            "keys rig opened headless (lane tracks, no audio device)"
        );
        Ok(rig)
    }

    /// The backing daw handle (cheap to clone) — the headless caller's
    /// render seam.
    pub fn daw(&self) -> &Standalone {
        &self.daw
    }

    /// The rig project's guid.
    pub fn project_guid(&self) -> &str {
        &self.project_guid
    }

    /// True when the rig hosts per-layer daw tracks.
    pub fn is_lanes(&self) -> bool {
        matches!(self.hosting, Hosting::Lanes(_))
    }

    /// Compile + insert every lane's instrument (and the tail) — glitch-free
    /// per-track swaps under the renderer lock.
    fn install_lane_instruments(&mut self, program: &LaneProgram) {
        let sr = self.sample_rate;
        let Hosting::Lanes(lanes) = &mut self.hosting else {
            return;
        };
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
                self.daw
                    .insert_plugin_instance(track.fx.clone(), Box::new(inst));
            }
        }
        if let (Some(fx), Some(tail)) = (&lanes.tail_fx, &program.tail) {
            let mut inst = KeysInstrument::new(tail, sr);
            let _ = inst.prepare(sr as f64, PREPARE_BLOCK);
            self.daw.insert_plugin_instance(fx.clone(), Box::new(inst));
        }
    }

    /// Re-install ONLY the lane instruments whose tree references
    /// `spec_path` (the key a just-attached pack was installed under).
    ///
    /// [`load_lanes`](Self::load_lanes) rebuilds every lane, which means
    /// opening every attached pack and re-parsing its index. In the browser
    /// that runs on the AUDIO THREAD (the worklet's message handler), and
    /// with the nine Worship packs it measured over 500 ms per call — half a
    /// second of stalled render, repeated after every pack attach and every
    /// progressive attach. A pack only ever affects the lanes that name it,
    /// so this rebuilds just those: bounded work, and untouched lanes keep
    /// playing through it.
    ///
    /// Returns how many lanes were re-installed.
    pub fn reload_lanes_for_pack(&mut self, program: &LaneProgram, spec_path: &str) -> usize {
        let sr = self.sample_rate;
        let mut done = 0;
        for engine in &program.engines {
            for layer in &engine.layers {
                if !tree_uses_sample(&layer.tree, spec_path) {
                    continue;
                }
                let Hosting::Lanes(lanes) = &mut self.hosting else {
                    return done;
                };
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
                self.daw
                    .insert_plugin_instance(track.fx.clone(), Box::new(inst));
                done += 1;
            }
        }
        done
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
            // Same as `RigHost::install_meters`, but valid for a headless
            // rig too: install a freshly-sized bank on the daw; the
            // renderer reads it per block.
            let meters = Meters::new(track_count);
            self.daw.set_meters(meters.clone());
            self.meters = meters;
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
        let Hosting::Lanes(l) = &self.hosting else {
            return None;
        };
        match role {
            Role::Engine => l
                .engines
                .iter()
                .find(|(n, _, _)| n == name)
                .map(|(_, g, _)| g.as_str()),
            Role::Layer => l
                .layers
                .iter()
                .find(|t| t.name == name)
                .map(|t| t.guid.as_str()),
            _ => None,
        }
    }

    /// Set a lane's fader (linear; the daw track volume).
    pub fn set_lane_volume(&self, role: Role, name: &str, linear: f32) {
        if let Some(guid) = self.lane_guid(role, name) {
            let _ = self
                .daw
                .current()
                .track(guid)
                .set_volume(linear.max(0.0) as f64);
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
        let Hosting::Lanes(l) = &self.hosting else {
            return None;
        };
        l.layers
            .iter()
            .find(|t| t.name == layer)
            .map(|t| t.cells.clone())
    }

    /// Run `f` against the live [`KeysInstrument`] hosting `layer` — the
    /// realtime parameter-edit seam (filter cutoff, envelope ADSR, unison…),
    /// serialized against the renderer by the host's plugin-map lock. In
    /// single-tree mode the one instrument hosts every layer, so the layer
    /// name only picks the fx slot in lane mode.
    pub fn edit_lane<R>(&self, layer: &str, f: impl FnOnce(&mut KeysInstrument) -> R) -> Option<R> {
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
        self.daw
            .insert_plugin_instance(fx_guid.clone(), Box::new(inst));
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

    /// Decode, on the calling thread, the samples `note`/`velocity` will
    /// need across every lane's sampler sources — the on-demand half of the
    /// preload budget: the audio thread DROPS a voice whose sample is not
    /// resident, so a key outside preload coverage would otherwise stay
    /// silent forever. Call this BEFORE dispatching the note-on (the browser
    /// worklet's control side does, between render quanta); the first press
    /// of a cold key may still decode audibly late once — after that it is
    /// cached. Cheap no-op when everything the note needs is resident.
    ///
    /// Budget interaction: the warm charges the process-wide decoded-PCM
    /// budget even past its ceiling; well past it, the warmed engine sheds
    /// its largest decoded samples back toward the limit (see
    /// `RenderNode::warm_note_samples`).
    pub fn warm_note(&self, note: u8, velocity: u8) {
        let layers: Vec<String> = match &self.hosting {
            // `edit_lane` ignores the layer name in single mode.
            Hosting::Single { .. } => vec![String::new()],
            Hosting::Lanes(l) => l.layers.iter().map(|t| t.name.clone()).collect(),
        };
        for layer in &layers {
            self.edit_lane(layer, |inst| {
                inst.render_mut().warm_note_samples(note, velocity);
            });
        }
    }
    pub fn note_off(&self, note: u8) {
        self.dispatch(ev_note_off(note));
    }

    /// The `(layer, sample path)` pairs a note-on for `note`/`velocity`
    /// needs that are NOT resident — resolve only, never a decode. The
    /// browser worklet calls this instead of [`warm_note`](Self::warm_note)
    /// (whose synchronous decode would starve the audio thread it runs on)
    /// and ships the list to the decoder worker; the PCM comes back through
    /// [`insert_decoded`](Self::insert_decoded).
    pub fn missing_note_samples(
        &self,
        note: u8,
        velocity: u8,
    ) -> Vec<(String, std::path::PathBuf)> {
        let layers: Vec<String> = match &self.hosting {
            Hosting::Single { .. } => vec![String::new()],
            Hosting::Lanes(l) => l.layers.iter().map(|t| t.name.clone()).collect(),
        };
        let mut out = Vec::new();
        for layer in &layers {
            let mut paths = Vec::new();
            self.edit_lane(layer, |inst| {
                inst.render_mut()
                    .missing_note_sample_paths(note, velocity, &mut paths);
            });
            out.extend(paths.into_iter().map(|p| (layer.clone(), p)));
        }
        out
    }

    /// WORKER SIDE: compile the lanes of `program` that play `spec_path`
    /// and publish them for the audio thread to install.
    ///
    /// This is the expensive work — compiling trees, opening zones — done
    /// somewhere the audio thread is not. It needs no `KeysRig` (which is
    /// `!Send` on wasm); it is an associated function precisely so a worker
    /// can call it with only the program.
    #[cfg(all(target_arch = "wasm32", target_feature = "atomics"))]
    pub fn build_lanes_for_pack(program: &LaneProgram, spec_path: &str, sample_rate: u32) -> usize {
        let mut built = 0;
        for engine in &program.engines {
            for layer in &engine.layers {
                if !tree_uses_sample(&layer.tree, spec_path) {
                    continue;
                }
                let (render, cells) = RenderNode::compile_with_cells(&layer.tree, sample_rate);
                crate::built_lanes::publish(crate::built_lanes::BuiltLane {
                    layer: layer.name.clone(),
                    render,
                    cells,
                });
                built += 1;
            }
        }
        built
    }

    /// AUDIO SIDE: install every lane a worker has finished compiling.
    ///
    /// Cheap by construction — `begin_swap` is two moves — and gapless:
    /// the tree being replaced keeps sounding until its voices release, so
    /// notes held across the change are not cut.
    #[cfg(all(target_arch = "wasm32", target_feature = "atomics"))]
    pub fn install_built_lanes(&self) -> usize {
        let mut installed = 0;
        while let Some(mut built) = crate::built_lanes::take() {
            // Move the new tree out, install it, and put whatever it
            // displaced back INTO THE SAME BOX. The swap then neither
            // allocates nor frees on this thread: one box in, one box out,
            // dropped later by a worker.
            let layer = std::mem::take(&mut built.layer);
            let render = std::mem::replace(&mut built.render, RenderNode::Serial(Vec::new()));
            let cells = built.cells.clone();
            let displaced = self
                .edit_lane(&layer, |inst| inst.begin_swap(render, cells))
                .flatten();
            match displaced {
                Some(old) => {
                    built.render = old;
                    crate::built_lanes::retire(built);
                    installed += 1;
                }
                None => {
                    // Lane not found, or nothing displaced: the box still
                    // holds a tree either way, so it goes to the reaper
                    // rather than being freed here.
                    crate::built_lanes::retire(built);
                }
            }
        }
        // Collect trees that have finished sounding since the last pass.
        installed
    }

    /// Queue every zone this note needs but has not opened onto the shared
    /// streamer queue (wasm + threads only). Returns how many were queued.
    ///
    /// The audio thread calls this on note-on INSTEAD of decoding: a worker
    /// opens the zones into the same cache maps this rig reads, so the note
    /// sounds on a subsequent press with nothing copied between threads.
    #[cfg(all(target_arch = "wasm32", target_feature = "atomics"))]
    pub fn queue_note_opens(&self, note: u8, velocity: u8) -> usize {
        let layers: Vec<String> = match &self.hosting {
            Hosting::Single { .. } => vec![String::new()],
            Hosting::Lanes(l) => l.layers.iter().map(|t| t.name.clone()).collect(),
        };
        let mut queued = 0;
        for layer in &layers {
            queued += self
                .edit_lane(layer, |inst| {
                    inst.render_mut().queue_note_opens(note, velocity)
                })
                .unwrap_or(0);
        }
        queued
    }

    /// Insert PCM decoded out-of-process into `layer`'s instrument (see
    /// `RenderNode::insert_decoded_sample`). Returns whether the lane
    /// accepted it — `false` with `charge_past_ceiling: false` means the
    /// decoded-PCM budget is full and background fill should pause.
    pub fn insert_decoded(
        &self,
        layer: &str,
        path: &std::path::Path,
        data: std::sync::Arc<crate::engine::cache::SampleData>,
        charge_past_ceiling: bool,
    ) -> bool {
        self.edit_lane(layer, |inst| {
            inst.render_mut()
                .insert_decoded_sample(path, &data, charge_past_ceiling)
        })
        .unwrap_or(false)
    }

    /// Decode `path` in `layer`'s instrument on the calling thread and take
    /// the PCM out of the decoding cache (budget-flat) — the decoder-worker
    /// side of the seam.
    pub fn decode_sample_take(
        &self,
        layer: &str,
        path: &std::path::Path,
    ) -> Option<std::sync::Arc<crate::engine::cache::SampleData>> {
        self.edit_lane(layer, |inst| inst.render_mut().decode_sample_take(path))
            .flatten()
    }

    /// Every lane's coverage-first sample list (playable order, middle-out
    /// from `center`), as `(layer, path)` pairs — the decoder worker's
    /// background fill plan. Lanes are interleaved round-robin so every
    /// lane becomes playable together instead of one finishing first.
    pub fn coverage_samples(&self, center: u8) -> Vec<(String, std::path::PathBuf)> {
        let layers: Vec<String> = match &self.hosting {
            Hosting::Single { .. } => vec![String::new()],
            Hosting::Lanes(l) => l.layers.iter().map(|t| t.name.clone()).collect(),
        };
        let mut per_lane: Vec<(String, Vec<std::path::PathBuf>)> = layers
            .iter()
            .map(|layer| {
                let mut paths = Vec::new();
                self.edit_lane(layer, |inst| {
                    inst.render_mut().coverage_sample_paths(center, &mut paths);
                });
                (layer.clone(), paths)
            })
            .collect();
        let mut out = Vec::new();
        let mut i = 0;
        loop {
            let mut any = false;
            for (layer, paths) in per_lane.iter_mut() {
                if let Some(p) = paths.get(i) {
                    out.push((layer.clone(), p.clone()));
                    any = true;
                }
            }
            if !any {
                break;
            }
            i += 1;
        }
        out
    }

    /// Voices currently alive across every lane's **sampler** sources
    /// (synth backends keep private voice vecs and are not counted) — a
    /// cheap diagnostic read for load panels, reached through the same
    /// plugin-map lock as [`edit_lane`](Self::edit_lane). Single-threaded
    /// on the worklet; on native it briefly serializes with the renderer,
    /// so poll it at panel rates (a few Hz), not per block.
    pub fn active_voices(&self) -> usize {
        let layers: Vec<String> = match &self.hosting {
            // `edit_lane` ignores the layer name in single mode.
            Hosting::Single { .. } => vec![String::new()],
            Hosting::Lanes(l) => l.layers.iter().map(|t| t.name.clone()).collect(),
        };
        layers
            .iter()
            .filter_map(|layer| self.edit_lane(layer, |inst| inst.render_mut().active_voices()))
            .sum()
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
    #[cfg(not(target_arch = "wasm32"))]
    pub fn midi_input_ports() -> Vec<String> {
        midicore::midir::input_ports()
    }

    /// The rig's live-MIDI sink (monitor tap + per-target dispatch), detached
    /// from `self`: everything it captures is a cheap clone, so a caller can
    /// build it under a lock and then open ports with the lock released —
    /// opening hardware ports takes seconds and can stall outright while the
    /// PipeWire graph reconfigures, and that stall must not pin a rig mutex.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn midi_sink(&self) -> impl Fn(midicore::TimedEvent) + Send + Clone + 'static {
        let daw = self.daw.clone();
        let targets = self.midi_targets();
        midicore::attach::tap_sink(self.midi_monitor.clone(), move |ev| {
            for track in &targets {
                daw.push_live_midi(track, ev.clone());
            }
        })
    }

    /// Open a hardware MIDI keyboard and forward its events into the rig
    /// (monitor tap + live-MIDI sink wired by `midicore::attach`).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn attach_midi(
        &self,
        selection: midicore::PortSelector,
    ) -> eyre::Result<midicore::midir::MidiInput> {
        midicore::midir::MidiInput::open(selection, self.midi_sink())
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
    let rig_name = if program.name.is_empty() {
        "Keys Rig"
    } else {
        &program.name
    };
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
        let eng = tree
            .folder(&engine.name)
            .map_err(|e| err("engine folder", e))?;
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

    Ok(LaneHost {
        rig_guid,
        tail_fx,
        engines,
        layers,
    })
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
                        LaneLayer {
                            name: "Piano".into(),
                            tree: Container::layer("Piano"),
                        },
                        LaneLayer {
                            name: "Pad".into(),
                            tree: Container::layer("Pad"),
                        },
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
        assert!(
            lanes.tail_fx.is_some(),
            "tail chain gets a rig-track fx slot"
        );

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
    /// A patch swap must not cut the notes that are still ringing.
    ///
    /// This is what "gapless" means in practice: hold a note, swap the
    /// tree, and the held note keeps sounding on the outgoing tree while
    /// the new one takes over. Replacing the tree outright — what the code
    /// did before `begin_swap` — silences it in the same block, which is
    /// the chop you hear on every patch change.
    #[test]
    fn a_swap_keeps_the_held_note_ringing() {
        let preset = crate::nord::layering_demo();
        let mut inst = KeysInstrument::new(&preset, 48_000);
        inst.prepare(48_000.0, 256).unwrap();

        let (inl, inr) = (vec![0.0f32; 256], vec![0.0f32; 256]);
        let (mut outl, mut outr) = (vec![0.0f32; 256], vec![0.0f32; 256]);
        let midi = [PluginMidiEvent {
            offset: 0,
            message: ev_note_on(72, 110),
        }];
        let held = PluginEvents {
            params: &[],
            midi: &midi,
            note_expressions: &[],
        };
        inst.process_block(&inl, &inr, &mut outl, &mut outr, &held)
            .unwrap();
        let rms = |b: &[f32]| (b.iter().map(|s| s * s).sum::<f32>() / b.len() as f32).sqrt();
        let before = rms(&outl);
        assert!(
            before > 1e-3,
            "note should sound before the swap, rms={before}"
        );

        // Swap to a freshly compiled tree while that note is still held.
        let (next, cells) = RenderNode::compile_with_cells(&preset, 48_000);
        inst.begin_swap(next, cells);
        assert!(inst.is_retiring(), "the outgoing tree should be retiring");

        // No new events — only the held note's tail can be sounding, and
        // it must still be there.
        let quiet = PluginEvents::default();
        let mut after = 0.0f32;
        for _ in 0..4 {
            outl.fill(0.0);
            outr.fill(0.0);
            inst.process_block(&inl, &inr, &mut outl, &mut outr, &quiet)
                .unwrap();
            after = after.max(rms(&outl));
        }
        assert!(
            after > 1e-4,
            "held note must survive the swap (gapless), rms after={after}"
        );
    }

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
