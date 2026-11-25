//! The FTS Signal CLAP/VST3: a thin nice-plug wrapper on `signal-core`.
//!
//! ```text
//! host audio (1, 2 or 16 ch) ┐
//! host MIDI ─────────────────┼─ process() ─▶ signal_core::Processor ─▶ host audio
//! params (gain, bypass, 8 macros) ┘              (the preset's graph)
//! state: the params + "signal-state", one SignalState blob (JSON)
//! ```
//!
//! - **Parameters** are the instance's own (`signal_core::param::host`): the
//!   same ids, ranges and text as Studio's linked core, so automation and
//!   presets move between them. What a macro drives is the preset's.
//! - **State** is one blob: the `SignalState` (preset + instance mode) in
//!   the `signal-state` field beside the parameter values. Restored before
//!   activation, `activate` builds it. Restored while active (a preset
//!   loaded mid-song, Studio's hot-reload `save_state` → `load_state`),
//!   nice-plug only writes the field, so the field counts its writes: the
//!   audio thread sees a new generation and hands the rebuild to the
//!   background executor, which builds the graph off the audio thread and
//!   installs it through signal-core's lock-free `Controller`.
//! - **Blocks**: Signal's own, compiled in (`BuiltinFactory`), plus the
//!   sampler (`crate::sampler`, desktop). A block's CLAP override and
//!   third-party plugins come through a CLAP factory over
//!   `signal-plugin-host` (signal#26 phase 5).
//! - **Files** (NAM captures, IRs, packs) are read from disk by
//!   [`DiskHost`]: this build has no host-actions extension to ask yet.

use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use nice_plug::params::internals::ParamPtr;
use nice_plug::params::persist::PersistentField;
use nice_plug::prelude::*;
use signal_core::block::ParamChange;
use signal_core::engine::{HostParam, HostParamEvent};
use signal_core::graph::{self, BuildOptions, BuildReport, Graph};
use signal_core::host::{AssetRef, HostError, SignalHost};
use signal_core::midi::{MidiEvent, MidiMessage};
use signal_core::param::{MACROS, ParamSpec, ValueText, host};
use signal_core::state::{Instance, Preset, SignalState};
use signal_core::{BlockFactory, BuiltinFactory, Controller, Processor, processor_pair};

use crate::config::RigConfig;

use crate::slots::{self, Mailbox, SLOTS, Shown, Table};

/// The development build (`--features dev`) is its own plugin, beside a
/// released FTS Signal: Studio loads it from a worktree.
#[cfg(feature = "dev")]
const PLUGIN_NAME: &str = "Signal Dev";
#[cfg(not(feature = "dev"))]
const PLUGIN_NAME: &str = "FTS Signal";
#[cfg(feature = "dev")]
const CLAP_ID: &str = "com.fasttrackstudio.signal.dev";
#[cfg(not(feature = "dev"))]
const CLAP_ID: &str = "com.fasttrackstudio.signal";
#[cfg(feature = "dev")]
const VST3_ID: [u8; 16] = *b"FtsSignalDev0001";
#[cfg(not(feature = "dev"))]
const VST3_ID: [u8; 16] = *b"FtsSignalPlug001";

/// How often a calibrating instance sets its make-ups, frames.
const CALIBRATE_STEP: usize = 24_000;

/// The most MIDI events taken per block; more are dropped.
const MIDI_CAPACITY: usize = 1024;

// ── Parameters ──────────────────────────────────────────────────────────────

fn float_param(spec: ParamSpec) -> FloatParam {
    FloatParam::new(
        spec.name,
        spec.default,
        FloatRange::Linear {
            min: spec.min,
            max: spec.max,
        },
    )
    .with_value_to_string(Arc::new(move |v| {
        ValueText::of(&spec, v).as_str().to_owned()
    }))
    .with_string_to_value(Arc::new(move |s| spec.parse(s)))
}

/// The instance's parameters, its blocks' parameters ([`slots`]), and its
/// state.
pub struct SignalParams {
    pub input_gain: FloatParam,
    pub output_gain: FloatParam,
    pub bypass: BoolParam,
    pub macro_1: FloatParam,
    pub macro_2: FloatParam,
    pub macro_3: FloatParam,
    pub macro_4: FloatParam,
    pub macro_5: FloatParam,
    pub macro_6: FloatParam,
    pub macro_7: FloatParam,
    pub macro_8: FloatParam,
    /// The running state's block parameters, `b001`…: named, printed and
    /// parsed by [`Shown`]'s table.
    pub slots: Vec<FloatParam>,
    /// The `SignalState`, as JSON, persisted as `signal-state`. Empty:
    /// nothing saved yet.
    pub state: StateField,
    pub shown: Arc<Shown>,
    /// On: every phase is made gain neutral as the song plays (each
    /// instance on its own input, as its host routes it); off, the
    /// make-ups stay, and are saved.
    pub calibrate: BoolParam,
    /// Bumped whenever a parameter value changes (a set, automation, a
    /// macro): `signal.chain/1`'s `values_revision`.
    pub values_revision: Arc<AtomicU64>,
    /// Called when a state is loaded: builds it at once, so its slots are
    /// there before any process call.
    on_load: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>>,
}

/// The state key: the field the state blob is persisted under.
pub const STATE_KEY: &str = "signal-state";

/// A callback that moves `values_revision`.
fn bumps(counter: &Arc<AtomicU64>) -> Arc<dyn Fn(f32) + Send + Sync> {
    let c = counter.clone();
    Arc::new(move |_| {
        c.fetch_add(1, Ordering::AcqRel);
    })
}

fn slot_param(i: usize, shown: &Arc<Shown>) -> FloatParam {
    let (name, text, parse) = (shown.clone(), shown.clone(), shown.clone());
    FloatParam::new(
        format!("Block {}", i + 1),
        0.0,
        FloatRange::Linear { min: 0.0, max: 1.0 },
    )
    .with_dynamic_name(Arc::new(move || {
        name.get()
            .slots
            .get(i)
            .map_or_else(|| format!("(unused {})", i + 1), |s| s.name.clone())
    }))
    .with_value_to_string(Arc::new(move |n| {
        text.get().slots.get(i).map_or_else(String::new, |s| {
            ValueText::of(&s.spec, s.spec.denormalize(n))
                .as_str()
                .to_owned()
        })
    }))
    .with_string_to_value(Arc::new(move |t| {
        let table = parse.get();
        let s = table.slots.get(i)?;
        s.spec.parse(t).map(|v| s.spec.normalize(v))
    }))
}

// SAFETY: every pointer is to a field of `self` (the slots' `Vec` is never
// resized after construction), alive as long as nice-plug's
// `Arc<SignalParams>`.
unsafe impl Params for SignalParams {
    fn param_map(&self) -> Vec<(String, ParamPtr, String)> {
        let mut map = vec![
            (
                "in_gain".to_owned(),
                self.input_gain.as_ptr(),
                String::new(),
            ),
            (
                "out_gain".to_owned(),
                self.output_gain.as_ptr(),
                String::new(),
            ),
            ("bypass".to_owned(), self.bypass.as_ptr(), String::new()),
            (
                "calibrate".to_owned(),
                self.calibrate.as_ptr(),
                String::new(),
            ),
        ];
        let macros = [
            &self.macro_1,
            &self.macro_2,
            &self.macro_3,
            &self.macro_4,
            &self.macro_5,
            &self.macro_6,
            &self.macro_7,
            &self.macro_8,
        ];
        for (i, m) in macros.into_iter().enumerate() {
            map.push((format!("macro_{}", i + 1), m.as_ptr(), String::new()));
        }
        for (i, p) in self.slots.iter().enumerate() {
            map.push((slots::slot_id(i), p.as_ptr(), String::new()));
        }
        map
    }

    fn serialize_fields(&self) -> BTreeMap<String, String> {
        // What was dialled in on the slots goes into the state: a restored
        // state rebuilds its graph from it, and the slots follow.
        self.write_back();
        let mut out = BTreeMap::new();
        if let Ok(data) = self.state.map(nice_plug::params::persist::serialize_field) {
            out.insert(STATE_KEY.to_owned(), data);
        }
        out
    }

    fn deserialize_fields(&self, serialized: &BTreeMap<String, String>) {
        if let Some(text) = serialized
            .get(STATE_KEY)
            .and_then(|d| nice_plug::params::persist::deserialize_field::<String>(d).ok())
        {
            self.state.set(text);
            if let Some(load) = self.on_load.get() {
                load();
            }
        }
    }
}

/// The `signal-state` field: the JSON, and how many times a host has set
/// it (a restore while active is only this write).
#[derive(Default)]
pub struct StateField {
    text: Mutex<String>,
    generation: AtomicU64,
}

impl StateField {
    /// Read by the audio thread: an atomic load.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    pub(crate) fn text(&self) -> String {
        self.text
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// What's running, without counting as a restore.
    pub(crate) fn store_running(&self, text: String) {
        *self.text.lock().unwrap_or_else(PoisonError::into_inner) = text;
    }
}

impl<'a> PersistentField<'a, String> for StateField {
    fn set(&self, new_value: String) {
        *self.text.lock().unwrap_or_else(PoisonError::into_inner) = new_value;
        self.generation.fetch_add(1, Ordering::AcqRel);
    }

    fn map<F, R>(&self, f: F) -> R
    where
        F: Fn(&String) -> R,
    {
        f(&self.text.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

impl Default for SignalParams {
    fn default() -> Self {
        let shown = Arc::new(Shown::default());
        let values = Arc::new(AtomicU64::new(0));
        let tracked = |p: FloatParam| p.with_callback(bumps(&values));
        Self {
            slots: (0..SLOTS).map(|i| tracked(slot_param(i, &shown))).collect(),
            shown,
            calibrate: BoolParam::new("Calibrate (gain neutral)", false),
            on_load: std::sync::OnceLock::new(),
            input_gain: tracked(float_param(host::INPUT_GAIN)),
            output_gain: tracked(float_param(host::OUTPUT_GAIN)),
            bypass: BoolParam::new(host::BYPASS.name, false).make_bypass(),
            macro_1: tracked(float_param(host::macro_spec(0))),
            macro_2: tracked(float_param(host::macro_spec(1))),
            macro_3: tracked(float_param(host::macro_spec(2))),
            macro_4: tracked(float_param(host::macro_spec(3))),
            macro_5: tracked(float_param(host::macro_spec(4))),
            macro_6: tracked(float_param(host::macro_spec(5))),
            macro_7: tracked(float_param(host::macro_spec(6))),
            macro_8: tracked(float_param(host::macro_spec(7))),
            state: StateField::default(),
            values_revision: values.clone(),
        }
    }
}

/// The CLAP state the Signal plugin loads (nice-plug's stream: an 8-byte
/// little-endian length, then the JSON) holding `state`, its parameters
/// left as they are: what a host restores to put a preset in an instance.
#[must_use]
pub fn clap_state(state: &SignalState) -> Vec<u8> {
    let blob = String::from_utf8(state.to_blob()).unwrap_or_default();
    let mut fields = BTreeMap::new();
    fields.insert(
        STATE_KEY.to_owned(),
        nice_plug::params::persist::serialize_field(&blob).unwrap_or_default(),
    );
    let json = serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "params": {},
        "fields": fields,
    });
    let body = serde_json::to_vec(&json).unwrap_or_default();
    let mut stream = (body.len() as u64).to_le_bytes().to_vec();
    stream.extend_from_slice(&body);
    stream
}

impl SignalParams {
    /// Write the slots' values into the state's preset (its block
    /// parameters, by lane, block and parameter id).
    pub(crate) fn write_back(&self) {
        let table = self.shown.get();
        if table.slots.is_empty() {
            return;
        }
        let text = self.state.text();
        let Ok(mut state) = SignalState::from_blob(text.as_bytes()) else {
            return;
        };
        for (slot, p) in table.slots.iter().zip(&self.slots) {
            let value = slot.spec.denormalize(p.unmodulated_normalized_value());
            let block = state
                .preset
                .lanes
                .iter_mut()
                .find(|l| l.name == slot.lane_name)
                .and_then(|l| l.blocks.iter_mut().find(|b| b.id == slot.block_id));
            if let Some(b) = block {
                b.params.insert(slot.spec.id.to_owned(), value);
            }
        }
        // The phase make-ups, as calibrated.
        for (l, name) in table.lanes.iter().enumerate() {
            let Some(lane) = state.preset.lanes.iter_mut().find(|x| &x.name == name) else {
                continue;
            };
            for (phase, gain_db) in table.trims_of(l) {
                match lane.trims.iter_mut().find(|t| t.phase == phase) {
                    Some(t) => t.gain_db = gain_db,
                    None => lane
                        .trims
                        .push(signal_core::state::PhaseTrim { phase, gain_db }),
                }
            }
        }
        self.state
            .store_running(String::from_utf8(state.to_blob()).unwrap_or_default());
    }

    /// Show `table`, the slots at `values` (normalized): what the graph
    /// just built has. Main or background thread.
    fn show(&self, table: Arc<Table>, values: &[f32]) {
        for (i, slot) in self.slots.iter().enumerate() {
            let v = values.get(i).copied().unwrap_or(0.0);
            // SAFETY: the slot lives as long as `self`.
            unsafe { slot.as_ptr()._internal_set_normalized_value(v) };
        }
        self.shown.set(table);
    }

    fn macros(&self) -> [f32; MACROS] {
        [
            self.macro_1.value(),
            self.macro_2.value(),
            self.macro_3.value(),
            self.macro_4.value(),
            self.macro_5.value(),
            self.macro_6.value(),
            self.macro_7.value(),
            self.macro_8.value(),
        ]
    }
}

// ── The host side ───────────────────────────────────────────────────────────

/// Files by path, from the local disk. Called while a graph is built
/// (`activate`), never on the audio thread.
pub struct DiskHost;

impl SignalHost for DiskHost {
    fn load_file(&self, asset: &AssetRef) -> Result<Vec<u8>, HostError> {
        std::fs::read(&asset.path).map_err(|e| HostError::NotFound(format!("{}: {e}", asset.path)))
    }
}

// ── Plugin ──────────────────────────────────────────────────────────────────

/// What the background rebuild needs: the controller of the processor,
/// and what the graph is built for.
pub struct Shared {
    pub(crate) params: Arc<SignalParams>,
    controller: Mutex<Controller>,
    /// Sample rate and max block of the current activation; `None` before.
    config: Mutex<Option<(f32, usize)>>,
    /// A rebuilt state's slots, for the audio thread.
    tables: Mailbox,
    /// The state generation the newest build covers.
    built: AtomicU64,
    /// FastTrackStudio's extensions' side of it.
    pub(crate) ext: crate::ext::Ext,
}

impl Shared {
    /// The running state, with what was dialled in on the slots.
    pub(crate) fn state(&self) -> SignalState {
        self.params.write_back();
        state_to_build(&self.params.state.text())
    }

    /// The rate the instance runs at (48 kHz before it's activated).
    pub(crate) fn sample_rate(&self) -> f32 {
        self.config
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .map_or(48_000.0, |c| c.0)
    }

    /// Hand piece `piece`'s placed hits to the audio side.
    pub(crate) fn send_planned(&self, piece: u16, hits: Vec<(u64, f32)>) {
        let mut controller = self
            .controller
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        controller.collect();
        let _ = controller.send(signal_core::engine::Command::PlannedHits {
            piece,
            hits: hits.into_boxed_slice(),
        });
    }

    /// Set parameter `param` of block `block` on lane `lane` (the running
    /// graph's) to `value` (plain) where it runs. Inactive, nothing runs:
    /// activation builds from the state.
    pub(crate) fn send_param(&self, lane: usize, block: usize, param: usize, value: f32) {
        if self
            .config
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_none()
        {
            return;
        }
        let mut controller = self
            .controller
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        controller.collect();
        if controller
            .send(signal_core::engine::Command::Param {
                lane,
                block,
                param,
                change: ParamChange::Set(value),
            })
            .is_err()
        {
            tracing::warn!(
                lane,
                block,
                param,
                "block parameter not sent: the processor's queue is full"
            );
        }
    }

    /// Put `text` in as the state and build it here (the main thread: an
    /// action, a host's call).
    pub(crate) fn load_state_text(&self, text: String, on_main: bool) {
        self.params.state.set(text);
        rebuild(self, on_main);
    }
}

/// Build the state in the field now (main or background thread): show
/// its slots, hand them and the graph to the audio side. Inactive, a
/// graph is built only to show the slots (activation builds the one that
/// runs).
fn rebuild(shared: &Shared, on_main: bool) {
    let generation = shared.params.state.generation();
    let config = *shared.config.lock().unwrap_or_else(PoisonError::into_inner);
    let (sr, max_block) = config.unwrap_or((48_000.0, 1024));
    // Files through the host when it offers them (its extension is the
    // main thread's), else from disk.
    let client = if on_main {
        *shared
            .ext
            .actions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    } else {
        None
    };
    let (state, mut graph, report) = match client {
        Some(c) => build_from(
            &shared.params.state.text(),
            sr,
            max_block,
            &crate::ext::HostFiles(c),
        ),
        None => build_from(&shared.params.state.text(), sr, max_block, &DiskHost),
    };
    crate::offline::arm(shared, &state, &mut graph);
    let table = Arc::new(Table::of(&graph));
    shared.params.show(table.clone(), &table.values(&graph));
    shared.built.store(generation, Ordering::Release);
    crate::ext::after_build(shared, &report, on_main);
    if config.is_none() {
        return;
    }
    shared.tables.post(table);
    let mut controller = shared
        .controller
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if controller.install(graph).is_err() {
        tracing::error!("restored state not installed: the processor's queue is full");
        return;
    }
    drop(controller);
    // The new graph starts without the host's placed hits: hand them again.
    let planned = shared
        .ext
        .planned
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    for (piece, hits) in planned {
        shared.send_planned(piece, hits);
    }
    tracing::info!(preset = %state.preset.name, blocks = report.blocks.len(), all_active = report.all_active(), "{PLUGIN_NAME}: state restored while active");
}

/// Background work: rebuild the graph from the state field.
pub struct Rebuild;

pub struct FtsSignal {
    params: Arc<SignalParams>,
    shared: Arc<Shared>,
    processor: Processor,
    /// The state field's generation the running graph was built from.
    built_generation: u64,
    midi: Vec<MidiEvent>,
    host_events: Vec<HostParamEvent>,
    /// What the processor last had, so a block hands over only changes.
    last: [f32; 3 + MACROS],
    in_channels: usize,
    /// The running graph's slots, and what each was last applied at (the
    /// host value's bits).
    table: Arc<Table>,
    applied: Vec<u32>,
    /// The latency the host was told.
    latency: u32,
    /// The editor window's size and whether it's open.
    editor_state: Arc<plugin_core::prelude::DioxusState>,
    /// Calibrating: since when (frames), and when it last stepped.
    calibrating: bool,
    calibrate_frames: usize,
}

impl Default for FtsSignal {
    fn default() -> Self {
        let params = Arc::new(SignalParams::default());
        let (controller, processor) = processor_pair();
        let shared = Arc::new(Shared {
            params: params.clone(),
            controller: Mutex::new(controller),
            config: Mutex::new(None),
            tables: Mailbox::default(),
            built: AtomicU64::new(0),
            ext: crate::ext::Ext::default(),
        });
        crate::ext::register(&shared);
        let weak = Arc::downgrade(&shared);
        let _ = params.on_load.set(Box::new(move || {
            if let Some(shared) = weak.upgrade() {
                rebuild(&shared, true);
            }
        }));
        Self {
            params,
            shared,
            processor,
            built_generation: 0,
            midi: Vec::with_capacity(MIDI_CAPACITY),
            host_events: Vec::with_capacity(3 + MACROS),
            last: [f32::NAN; 3 + MACROS],
            in_channels: 2,
            table: Arc::new(Table::default()),
            applied: vec![0; SLOTS],
            latency: 0,
            editor_state: plugin_core::prelude::DioxusState::new(|| crate::editor::SIZE),
            calibrating: false,
            calibrate_frames: 0,
        }
    }
}

impl FtsSignal {
    /// What the instance shares with its extensions and editor.
    #[must_use]
    pub(crate) fn shared(&self) -> Arc<Shared> {
        self.shared.clone()
    }
}

/// The state to build: what was saved, else the `FTS_SIGNAL_RIG` sampler
/// pack (the v0 plugin's configuration), else a passthrough.
fn state_to_build(saved: &str) -> SignalState {
    if !saved.is_empty() {
        match SignalState::from_blob(saved.as_bytes()) {
            Ok(s) => return s,
            Err(e) => tracing::error!(error = %e, "saved Signal state refused: starting empty"),
        }
    }
    match RigConfig::resolve() {
        Ok(Some(cfg)) => SignalState::new(crate::sampler::preset_for(&cfg)),
        Ok(None) => SignalState::new(Preset::passthrough()),
        Err(e) => {
            tracing::error!(error = %e, "rig config");
            SignalState::new(Preset::passthrough())
        }
    }
}

/// Build the state in the field (or the fresh default) for `sample_rate`
/// and blocks of `max_block`. Off the audio thread.
fn build_from(
    saved: &str,
    sample_rate: f32,
    max_block: usize,
    host: &dyn SignalHost,
) -> (SignalState, Graph, BuildReport) {
    let state = state_to_build(saved);
    let port = match &state.instance {
        Instance::MultiChannel => None,
        Instance::Port(link) => Some(link.port.0.as_str()),
    };
    let factories: [&dyn BlockFactory; 3] = [
        &BuiltinFactory,
        &crate::sampler::SamplerFactory,
        &crate::drum::DrumFactory,
    ];
    let opts = BuildOptions {
        sample_rate,
        max_block: max_block.clamp(1, signal_core::MAX_BLOCK),
        port,
        factories: &factories,
        host,
    };
    let (mut graph, report) = graph::build(&state.preset, &opts);
    // A port of a session: its hits go to, and come from, the session's
    // other instances in this process (the kit's other tracks).
    if let Instance::Port(link) = &state.instance {
        graph.join(signal_core::session::Registry::join(&link.session));
        tracing::info!(session = %link.session.0, port = %link.port.0, remote_keys = graph.remote_keys(), "joined the session");
    }
    for b in &report.blocks {
        if let Some(reason) = b.status.reason() {
            tracing::warn!(block.lane = %b.lane, block.id = %b.block, block.reason = %reason, "block not as the preset has it");
        }
    }
    for w in &report.warnings {
        tracing::warn!(warning = %w, "preset");
    }
    (state, graph, report)
}

impl Plugin for FtsSignal {
    const NAME: &'static str = PLUGIN_NAME;
    const VENDOR: &'static str = "FastTrackStudio";
    const URL: &'static str = "https://fasttrackstudio.com";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    /// Stereo (a chain, a keys rig), mono in (a guitar), and a kit's mics.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(16),
            main_output_channels: NonZeroU32::new(16),
            ..AudioIOLayout::const_default()
        },
    ];
    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;

    // The kit's mixer (crate::editor); the host's own view sets parameters.
    type Editor = plugin_core::nice_plug_dioxus::editor::DioxusEditor;
    type SysExMessage = ();
    type BackgroundTask = Rebuild;

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        plugin_core::prelude::create_dioxus_editor_with_state(
            self.editor_state.clone(),
            Arc::new(crate::editor::EditorState {
                shared: self.shared.clone(),
            }),
            crate::editor::App,
        )
    }

    fn task_executor(&mut self) -> TaskExecutor<Self> {
        let shared = self.shared.clone();
        Box::new(move |Rebuild| rebuild(&shared, false))
    }

    fn activate(
        &mut self,
        audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl ActivateContext<Self>,
    ) -> bool {
        init_logging();
        self.in_channels = audio_io_layout
            .main_input_channels
            .map_or(0, |n| n.get() as usize);
        let generation = self.params.state.generation();
        let max_block = buffer_config.max_buffer_size as usize;
        *self
            .shared
            .config
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some((buffer_config.sample_rate, max_block));
        let client = *self
            .shared
            .ext
            .actions
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let text = self.params.state.text();
        let (state, mut graph, report) = match client {
            Some(c) => build_from(
                &text,
                buffer_config.sample_rate,
                max_block,
                &crate::ext::HostFiles(c),
            ),
            None => build_from(&text, buffer_config.sample_rate, max_block, &DiskHost),
        };
        crate::offline::arm(&self.shared, &state, &mut graph);
        let table = Arc::new(Table::of(&graph));
        let values = table.values(&graph);
        self.params.show(table.clone(), &values);
        // Active now, so the host runs it: its subscriptions made again
        // (one made while it was loaded may be forgotten).
        crate::ext::renew_subscriptions(&self.shared);
        crate::ext::after_build(&self.shared, &report, true);
        self.applied = (0..SLOTS)
            .map(|i| values.get(i).copied().unwrap_or(0.0).to_bits())
            .collect();
        self.table = table;
        drop(self.processor.install(graph));
        // The host's placed hits, for the new graph (applied on its first
        // block).
        let planned = self
            .shared
            .ext
            .planned
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        for (piece, hits) in planned {
            self.shared.send_planned(piece, hits);
        }
        self.built_generation = generation;
        self.shared.built.store(generation, Ordering::Release);
        self.last = [f32::NAN; 3 + MACROS];
        self.latency = self.processor.latency();
        context.set_latency_samples(self.latency);
        // Keep what's running as the state (a fresh instance saves its
        // starting preset).
        self.params
            .state
            .store_running(String::from_utf8(state.to_blob()).unwrap_or_default());
        self.shared
            .controller
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .collect();
        tracing::info!(
            sr = buffer_config.sample_rate,
            preset = %state.preset.name,
            blocks = report.blocks.len(),
            all_active = report.all_active(),
            "{PLUGIN_NAME}: activated"
        );
        true
    }

    /// The host's reset (a transport jump, a render primed again): every
    /// block's tails and envelopes cleared, the graph's delays too.
    fn reset(&mut self) {
        self.processor.reset();
        // A render primed again: its meters' summaries start over.
        self.shared.ext.stats_clear.store(true, Ordering::Release);
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let frames = buffer.samples();
        // A state restored while active and not built where it was loaded:
        // rebuild off this thread.
        let generation = self.params.state.generation();
        if generation != self.built_generation {
            self.built_generation = generation;
            if self.shared.built.load(Ordering::Acquire) != generation {
                context.execute_background(Rebuild);
            }
        }
        // The instance's parameters, as events where they changed.
        self.host_events.clear();
        let p = &self.params;
        let macros = p.macros();
        let now = {
            let mut v = [0.0_f32; 3 + MACROS];
            v[0] = p.input_gain.value();
            v[1] = p.output_gain.value();
            v[2] = if p.bypass.value() { 1.0 } else { 0.0 };
            v[3..].copy_from_slice(&macros);
            v
        };
        for (i, (now, last)) in now.iter().zip(self.last.iter_mut()).enumerate() {
            if now.to_bits() != last.to_bits() {
                *last = *now;
                let param = match i {
                    0 => HostParam::InputGain,
                    1 => HostParam::OutputGain,
                    2 => HostParam::Bypass,
                    m => HostParam::Macro(u8::try_from(m - 3).unwrap_or(0)),
                };
                self.host_events.push(HostParamEvent {
                    offset: 0,
                    param,
                    value: *now,
                });
            }
        }
        // MIDI, within capacity.
        self.midi.clear();
        while let Some(ev) = context.next_event() {
            let offset = ev.timing();
            let message = match ev {
                // A wildcard key names no single note (CLAP allows it): skipped.
                NoteEvent::NoteOn {
                    key,
                    velocity,
                    channel,
                    ..
                } => key.number().map(|note| {
                    (
                        channel.number().unwrap_or(0),
                        MidiMessage::NoteOn {
                            note,
                            velocity: ((velocity * 127.0).round() as u8).clamp(1, 127),
                        },
                    )
                }),
                NoteEvent::NoteOff { key, channel, .. } => key
                    .number()
                    .map(|note| (channel.number().unwrap_or(0), MidiMessage::NoteOff { note })),
                NoteEvent::MidiCC {
                    cc, value, channel, ..
                } => Some((
                    channel,
                    MidiMessage::Cc {
                        cc,
                        value: (value * 127.0).round().clamp(0.0, 127.0) as u8,
                    },
                )),
                NoteEvent::MidiPitchBend { value, channel, .. } => Some((
                    channel,
                    MidiMessage::PitchBend(((value - 0.5) * 16_383.0).round() as i16),
                )),
                _ => None,
            };
            if let Some((channel, message)) = message {
                if self.midi.len() < self.midi.capacity() {
                    self.midi.push(MidiEvent {
                        offset,
                        channel,
                        message,
                    });
                }
            }
        }
        // The session's clock: the song position, while the host plays
        // (every port of a session is told the same).
        let transport = context.transport();
        let song_pos = if transport.playing {
            transport.pos_samples().and_then(|p| u64::try_from(p).ok())
        } else if self.shared.ext.transport_playing.load(Ordering::Acquire) {
            // The host's transport events, where the plugin API gives none.
            Some(self.shared.ext.transport_frame.load(Ordering::Acquire))
        } else {
            None
        };
        if let Some(pos) = song_pos {
            self.processor.set_position(pos);
        }
        // A rebuilt state's slots: what the host shows now is what the new
        // graph has, so nothing is applied until it moves.
        if let Some(mut new) = self.shared.tables.take() {
            // The old table goes back in the box the new one came in: no
            // allocation, and the main thread drops it.
            std::mem::swap(&mut self.table, &mut *new);
            self.shared.tables.retire(new);
            for (a, p) in self.applied.iter_mut().zip(&self.params.slots) {
                *a = p.modulated_normalized_value().to_bits();
            }
        }
        // The blocks' parameters the host moved.
        for (i, slot) in self.table.slots.iter().enumerate() {
            let (Some(p), Some(a)) = (self.params.slots.get(i), self.applied.get_mut(i)) else {
                break;
            };
            let n = p.modulated_normalized_value();
            if n.to_bits() != *a {
                *a = n.to_bits();
                self.processor.apply_param(
                    slot.lane,
                    slot.block,
                    slot.param,
                    ParamChange::Set(slot.spec.denormalize(n)),
                );
                // An offline block's lane, played by the host: its value is
                // the one at this block's start, so it lands now (an apply
                // stage otherwise ramps to a value over the next piece, which
                // is what Signal's own playback asks of it).
                if self
                    .processor
                    .graph()
                    .is_some_and(|g| g.is_offline(slot.lane, slot.block))
                {
                    self.processor.settle(slot.lane, slot.block);
                }
            }
        }
        let io = buffer.as_slice();
        self.processor
            .process(io, self.in_channels, frames, &self.midi, &self.host_events);
        // A state restored while active builds a new graph: its latency
        // (a trigger's lookahead, a session's) to the host.
        // The meters and keys, for the host (atomics only).
        if let Some(g) = self.processor.graph() {
            crate::ext::store_meters(&self.shared.ext, &self.table, g, song_pos, frames);
        }
        // Calibrating: every phase made gain neutral on what it has heard
        // since the switch went on, every half second.
        let calibrate = self.params.calibrate.value();
        if calibrate && !self.calibrating {
            if let Some(g) = self.processor.graph_mut() {
                g.clear_phase_levels();
            }
            self.calibrate_frames = 0;
        }
        self.calibrating = calibrate;
        if calibrate {
            self.calibrate_frames += frames;
            if self.calibrate_frames >= CALIBRATE_STEP {
                self.calibrate_frames = 0;
                if let Some(g) = self.processor.graph_mut() {
                    g.calibrate_phases();
                    self.table.store_trims(g);
                }
            }
        }
        let latency = self.processor.latency();
        if latency != self.latency {
            self.latency = latency;
            context.set_latency_samples(latency);
        }
        ProcessStatus::Normal
    }
}

fn init_logging() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let log_path = std::env::temp_dir().join("fts-signal-plugin.log");
        if let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
        {
            let _ = tracing_subscriber::fmt::Subscriber::builder()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                        tracing_subscriber::EnvFilter::new(
                            "signal_plugin=info,signal_core=info,signal::usage=info,warn",
                        )
                    }),
                )
                .with_writer(file)
                .with_ansi(false)
                .try_init();
        }
    });
}

impl ClapPlugin for FtsSignal {
    const CLAP_ID: &'static str = CLAP_ID;

    fn clap_extension(id: &std::ffi::CStr) -> Option<*const std::ffi::c_void> {
        if id == daw_clap_ext::sys::FTS_EXT_SIGNAL {
            return Some(daw_clap_ext::plugin::SignalVtable::<crate::ext::Lookup>::as_ptr());
        }
        if id == crate::ext::FTS_EXT_SIGNAL_REVISIONS {
            let v: &'static crate::ext::SignalRevisions = &crate::ext::REVISIONS;
            return Some(std::ptr::from_ref(v).cast());
        }
        if id == daw_clap_ext::sys::FTS_EXT_HOST_EVENTS {
            let v: &'static daw_clap_ext::sys::fts_plugin_host_events = &crate::ext::HOST_EVENTS;
            return Some(std::ptr::from_ref(v).cast());
        }
        None
    }

    fn extension_state(&self) -> Option<Arc<dyn std::any::Any + Send + Sync>> {
        Some(self.shared.clone())
    }

    fn clap_init(
        state: &Arc<dyn std::any::Any + Send + Sync>,
        plugin: *const nice_plug::wrapper::clap::clap_plugin,
        host: *const nice_plug::wrapper::clap::clap_host,
    ) {
        if let Ok(shared) = state.clone().downcast::<Shared>() {
            crate::ext::init(&shared, plugin, host);
        }
    }
    const CLAP_DESCRIPTION: Option<&'static str> = Some(
        "FTS Signal: guitar and keys rigs, DSP chains and (next) the drum submixer, the same core Studio runs",
    );
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Surround,
    ];
}

impl Vst3Plugin for FtsSignal {
    const VST3_CLASS_ID: [u8; 16] = VST3_ID;
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Fx];
}
