//! CLAP plugin implementation.
//!
//! Stereo passthrough that controls other FX on the same track.

use std::num::NonZeroU32;
use std::sync::atomic::{AtomicI32, AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;

use atomic_float::AtomicF32;
use fts_plugin_core::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};

use crate::param_queue::{self, ParamQueueConsumer, ParamQueueProducer};
use signal_proto::ParamWriteRequest;

/// Base MIDI note for scene switching (C1 = note 36).
/// Scene N uses note (BASE + N - 1), matching scene_midi.rs.
const SCENE_SWITCH_BASE_NOTE: u8 = 36;

/// Maximum number of scenes we support.
const MAX_SCENES: u8 = 24;

const PLUGIN_NAME: &str = "FTS Signal Controller";

/// Number of macro knobs exposed as automatable parameters.
pub const NUM_MACROS: usize = 8;

// ── Macro Banks ─────────────────────────────────────────────────────

/// Configuration for a single macro slot within a bank.
#[derive(Clone)]
pub struct MacroSlotConfig {
    pub name: &'static str,
}

/// A macro bank — a named set of 8 macro slot configurations.
pub struct MacroBank {
    pub name: &'static str,
    pub slots: [MacroSlotConfig; NUM_MACROS],
}

/// The available macro banks. Add more as needed.
pub static MACRO_BANKS: &[MacroBank] = &[
    MacroBank {
        name: "Default",
        slots: [
            MacroSlotConfig { name: "Macro 1" },
            MacroSlotConfig { name: "Macro 2" },
            MacroSlotConfig { name: "Macro 3" },
            MacroSlotConfig { name: "Macro 4" },
            MacroSlotConfig { name: "Macro 5" },
            MacroSlotConfig { name: "Macro 6" },
            MacroSlotConfig { name: "Macro 7" },
            MacroSlotConfig { name: "Macro 8" },
        ],
    },
];

// ── Parameters ──────────────────────────────────────────────────────

#[derive(Params)]
pub struct ControllerParams {
    #[id = "macro_0"]
    pub macro_0: FloatParam,
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
}

impl Default for ControllerParams {
    fn default() -> Self {
        let mk = |name: &'static str| {
            FloatParam::new(name, 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage())
        };
        Self {
            macro_0: mk("Macro 1"),
            macro_1: mk("Macro 2"),
            macro_2: mk("Macro 3"),
            macro_3: mk("Macro 4"),
            macro_4: mk("Macro 5"),
            macro_5: mk("Macro 6"),
            macro_6: mk("Macro 7"),
            macro_7: mk("Macro 8"),
        }
    }
}

impl ControllerParams {
    pub fn macros(&self) -> [&FloatParam; NUM_MACROS] {
        [
            &self.macro_0, &self.macro_1, &self.macro_2, &self.macro_3,
            &self.macro_4, &self.macro_5, &self.macro_6, &self.macro_7,
        ]
    }

    pub fn apply_bank(&self, _bank: &MacroBank) {
        // TODO: Restore when fts-plugin-core re-adds set_display_name to FloatParam
    }

    pub fn clear_bank(&self) {
        // TODO: Restore when fts-plugin-core re-adds clear_display_name to FloatParam
    }
}

// ── UI State ────────────────────────────────────────────────────────

pub struct ControllerUiState {
    pub params: Arc<ControllerParams>,
    pub active_bank: AtomicU8,
    pub pending_write_count: AtomicU32,
    pub shm_connected: AtomicU32,
    pub macro_activity: [AtomicF32; NUM_MACROS],
    macro_labels: std::sync::RwLock<[String; NUM_MACROS]>,
    macro_colors: std::sync::RwLock<[String; NUM_MACROS]>,
    pub config_loaded: std::sync::atomic::AtomicBool,
    pub requested_scene: AtomicI32,
    pub active_scene: AtomicI32,
}

impl ControllerUiState {
    fn new(params: Arc<ControllerParams>) -> Self {
        Self {
            params,
            active_bank: AtomicU8::new(0),
            pending_write_count: AtomicU32::new(0),
            shm_connected: AtomicU32::new(0),
            macro_activity: std::array::from_fn(|_| AtomicF32::new(0.0)),
            macro_labels: std::sync::RwLock::new(std::array::from_fn(|i| format!("Macro {}", i + 1))),
            macro_colors: std::sync::RwLock::new(std::array::from_fn(|_| String::new())),
            config_loaded: std::sync::atomic::AtomicBool::new(false),
            requested_scene: AtomicI32::new(-1),
            active_scene: AtomicI32::new(0),
        }
    }

    pub fn macro_ptrs(&self) -> [ParamPtr; NUM_MACROS] {
        [
            self.params.macro_0.as_ptr(), self.params.macro_1.as_ptr(),
            self.params.macro_2.as_ptr(), self.params.macro_3.as_ptr(),
            self.params.macro_4.as_ptr(), self.params.macro_5.as_ptr(),
            self.params.macro_6.as_ptr(), self.params.macro_7.as_ptr(),
        ]
    }

    pub fn set_macro_label(&self, index: usize, label: &str) {
        if index < NUM_MACROS {
            if let Ok(mut labels) = self.macro_labels.write() {
                labels[index] = label.to_string();
            }
        }
    }

    pub fn set_macro_color(&self, index: usize, color: &str) {
        if index < NUM_MACROS {
            if let Ok(mut colors) = self.macro_colors.write() {
                colors[index] = color.to_string();
            }
        }
    }

    pub fn macro_labels(&self) -> Vec<String> {
        if self.config_loaded.load(Ordering::Relaxed) {
            if let Ok(labels) = self.macro_labels.read() {
                return labels.to_vec();
            }
        }
        let bank_idx = self.active_bank.load(Ordering::Relaxed) as usize;
        let bank = &MACRO_BANKS[bank_idx];
        bank.slots.iter().map(|s| s.name.to_string()).collect()
    }

    pub fn macro_colors(&self) -> Vec<String> {
        if let Ok(colors) = self.macro_colors.read() {
            colors.to_vec()
        } else {
            vec![String::new(); NUM_MACROS]
        }
    }
}

// ── Plugin ──────────────────────────────────────────────────────────

pub struct FtsSignalController {
    params: Arc<ControllerParams>,
    pub(crate) ui_state: Arc<ControllerUiState>,
    queue_consumer: ParamQueueConsumer,
    queue_producer: ParamQueueProducer,
    pending_writes: Vec<ParamWriteRequest>,
    prev_macros: [f32; NUM_MACROS],
}

impl Default for FtsSignalController {
    fn default() -> Self {
        let params = Arc::new(ControllerParams::default());
        let ui_state = Arc::new(ControllerUiState::new(params.clone()));
        let (producer, consumer) = param_queue::param_queue();
        Self {
            params,
            ui_state,
            queue_consumer: consumer,
            queue_producer: producer,
            pending_writes: Vec::with_capacity(64),
            prev_macros: [f32::NAN; NUM_MACROS],
        }
    }
}

impl FtsSignalController {
    pub fn queue_producer(&self) -> ParamQueueProducer {
        self.queue_producer.clone()
    }

    fn read_macros(&self) -> [f32; NUM_MACROS] {
        [
            self.params.macro_0.value(), self.params.macro_1.value(),
            self.params.macro_2.value(), self.params.macro_3.value(),
            self.params.macro_4.value(), self.params.macro_5.value(),
            self.params.macro_6.value(), self.params.macro_7.value(),
        ]
    }
}

impl Plugin for FtsSignalController {
    const NAME: &'static str = PLUGIN_NAME;
    const VENDOR: &'static str = "FastTrackStudio";
    const URL: &'static str = "https://fasttrackstudio.com";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];
    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        _buffer_config: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        // Set up file-based logging
        let log_path = "/tmp/fts-signal-controller.log";
        if let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
        {
            let _ = fmt::Subscriber::builder()
                .with_env_filter(
                    EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| EnvFilter::new("fts_signal_controller=debug,warn")),
                )
                .with_writer(file)
                .with_ansi(false)
                .try_init();
        }

        // Initialize DAW API — only once across all instances
        static TIMERS_REGISTERED: std::sync::atomic::AtomicBool =
            std::sync::atomic::AtomicBool::new(false);

        if daw::init(context.raw_host_context())
            && !TIMERS_REGISTERED.swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            daw::register_timer(crate::scene_timer::poll);
            daw::register_timer(crate::macro_timer::poll);
            tracing::info!("{PLUGIN_NAME}: DAW API initialized, timers registered");
        }

        // Spawn background SHM bridge
        crate::shm_bridge::spawn_bridge(self.ui_state.clone());

        tracing::info!("{PLUGIN_NAME}: initialized");
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // ── 1. Drain SHM parameter writes ──────────────────────────
        self.pending_writes.clear();
        self.queue_consumer.drain(&mut self.pending_writes);

        self.ui_state
            .pending_write_count
            .store(self.pending_writes.len() as u32, Ordering::Relaxed);

        for write in &self.pending_writes {
            // TODO: Call reaper TrackFX_SetParamNormalized for each write.
            let _ = write;
        }

        // ── 2. Process MIDI note events for scene switching ────────
        while let Some(event) = context.next_event() {
            if let NoteEvent::NoteOn { note, .. } = event {
                if note >= SCENE_SWITCH_BASE_NOTE
                    && note < SCENE_SWITCH_BASE_NOTE + MAX_SCENES
                {
                    let scene = (note - SCENE_SWITCH_BASE_NOTE + 1) as i32;
                    self.ui_state.requested_scene.store(scene, Ordering::Relaxed);
                }
            }
        }

        // ── 3. Macro change detection ──────────────────────────────
        let macros = self.read_macros();
        for i in 0..NUM_MACROS {
            let delta = (macros[i] - self.prev_macros[i]).abs();
            if delta > 1e-5 {
                self.ui_state.macro_activity[i].store(delta.min(1.0), Ordering::Relaxed);
            } else {
                let prev = self.ui_state.macro_activity[i].load(Ordering::Relaxed);
                self.ui_state.macro_activity[i].store((prev * 0.95).max(0.0), Ordering::Relaxed);
            }
        }
        self.prev_macros = macros;

        // ── 4. Passthrough audio ───────────────────────────────────
        let _ = buffer;

        ProcessStatus::Normal
    }
}

impl ClapPlugin for FtsSignalController {
    const CLAP_ID: &'static str = "com.fasttrackstudio.fts-signal-controller";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Per-track signal chain controller — rig setup, macros, cross-track routing");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::Utility, ClapFeature::Mixing];
}
