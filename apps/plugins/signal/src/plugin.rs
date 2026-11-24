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

use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use nice_plug::params::persist::PersistentField;
use nice_plug::prelude::*;
use signal_core::engine::{HostParam, HostParamEvent};
use signal_core::graph::{self, BuildOptions, BuildReport, Graph};
use signal_core::host::{AssetRef, HostError, SignalHost};
use signal_core::midi::{MidiEvent, MidiMessage};
use signal_core::param::{MACROS, ParamSpec, ValueText, host};
use signal_core::state::{Instance, Preset, SignalState};
use signal_core::{BlockFactory, BuiltinFactory, Controller, Processor, processor_pair};

use crate::config::RigConfig;

const PLUGIN_NAME: &str = "FTS Signal";

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

/// The instance's parameters, and its state.
#[derive(Params)]
pub struct SignalParams {
    #[id = "in_gain"]
    pub input_gain: FloatParam,
    #[id = "out_gain"]
    pub output_gain: FloatParam,
    #[id = "bypass"]
    pub bypass: BoolParam,
    #[id = "macro_1"]
    pub macro_1: FloatParam,
    #[id = "macro_2"]
    pub macro_2: FloatParam,
    #[id = "macro_3"]
    pub macro_3: FloatParam,
    #[id = "macro_4"]
    pub macro_4: FloatParam,
    #[id = "macro_5"]
    pub macro_5: FloatParam,
    #[id = "macro_6"]
    pub macro_6: FloatParam,
    #[id = "macro_7"]
    pub macro_7: FloatParam,
    #[id = "macro_8"]
    pub macro_8: FloatParam,
    /// The `SignalState`, as JSON. Empty: nothing saved yet.
    #[persist = "signal-state"]
    pub state: StateField,
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

    fn text(&self) -> String {
        self.text
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// What's running, without counting as a restore.
    fn store_running(&self, text: String) {
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
        Self {
            input_gain: float_param(host::INPUT_GAIN),
            output_gain: float_param(host::OUTPUT_GAIN),
            bypass: BoolParam::new(host::BYPASS.name, false).make_bypass(),
            macro_1: float_param(host::macro_spec(0)),
            macro_2: float_param(host::macro_spec(1)),
            macro_3: float_param(host::macro_spec(2)),
            macro_4: float_param(host::macro_spec(3)),
            macro_5: float_param(host::macro_spec(4)),
            macro_6: float_param(host::macro_spec(5)),
            macro_7: float_param(host::macro_spec(6)),
            macro_8: float_param(host::macro_spec(7)),
            state: StateField::default(),
        }
    }
}

impl SignalParams {
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
struct Shared {
    params: Arc<SignalParams>,
    controller: Mutex<Controller>,
    /// Sample rate and max block of the current activation; `None` before.
    config: Mutex<Option<(f32, usize)>>,
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
}

impl Default for FtsSignal {
    fn default() -> Self {
        let params = Arc::new(SignalParams::default());
        let (controller, processor) = processor_pair();
        let shared = Arc::new(Shared {
            params: params.clone(),
            controller: Mutex::new(controller),
            config: Mutex::new(None),
        });
        Self {
            params,
            shared,
            processor,
            built_generation: 0,
            midi: Vec::with_capacity(MIDI_CAPACITY),
            host_events: Vec::with_capacity(3 + MACROS),
            last: [f32::NAN; 3 + MACROS],
            in_channels: 2,
        }
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
) -> (SignalState, Graph, BuildReport) {
    let state = state_to_build(saved);
    let port = match &state.instance {
        Instance::MultiChannel => None,
        Instance::Port(link) => Some(link.port.0.as_str()),
    };
    let factories: [&dyn BlockFactory; 2] = [&BuiltinFactory, &crate::sampler::SamplerFactory];
    let opts = BuildOptions {
        sample_rate,
        max_block: max_block.clamp(1, signal_core::MAX_BLOCK),
        port,
        factories: &factories,
        host: &DiskHost,
    };
    let (graph, report) = graph::build(&state.preset, &opts);
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

    // No editor yet: the host's generic parameter UI.
    type Editor = ();
    type SysExMessage = ();
    type BackgroundTask = Rebuild;

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn task_executor(&mut self) -> TaskExecutor<Self> {
        let shared = self.shared.clone();
        Box::new(move |Rebuild| {
            let Some((sr, max_block)) =
                *shared.config.lock().unwrap_or_else(PoisonError::into_inner)
            else {
                return;
            };
            let (state, graph, report) = build_from(&shared.params.state.text(), sr, max_block);
            let mut controller = shared
                .controller
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if controller.install(graph).is_err() {
                tracing::error!("restored state not installed: the processor's queue is full");
                return;
            }
            tracing::info!(preset = %state.preset.name, blocks = report.blocks.len(), all_active = report.all_active(), "{PLUGIN_NAME}: state restored while active");
        })
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
        let (state, graph, report) = build_from(
            &self.params.state.text(),
            buffer_config.sample_rate,
            max_block,
        );
        drop(self.processor.install(graph));
        self.built_generation = generation;
        self.last = [f32::NAN; 3 + MACROS];
        context.set_latency_samples(self.processor.latency());
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

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let frames = buffer.samples();
        // A state restored while active: rebuild off this thread.
        let generation = self.params.state.generation();
        if generation != self.built_generation {
            self.built_generation = generation;
            context.execute_background(Rebuild);
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
                NoteEvent::NoteOn {
                    note,
                    velocity,
                    channel,
                    ..
                } => Some((
                    channel,
                    MidiMessage::NoteOn {
                        note,
                        velocity: ((velocity * 127.0).round() as u8).clamp(1, 127),
                    },
                )),
                NoteEvent::NoteOff { note, channel, .. } => {
                    Some((channel, MidiMessage::NoteOff { note }))
                }
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
        let io = buffer.as_slice();
        self.processor
            .process(io, self.in_channels, frames, &self.midi, &self.host_events);
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
    const CLAP_ID: &'static str = "com.fasttrackstudio.signal";
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
    const VST3_CLASS_ID: [u8; 16] = *b"FtsSignalPlug001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Fx];
}
