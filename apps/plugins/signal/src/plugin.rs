//! The FTS Signal plugin shell (v0).
//!
//! ```text
//! CLAP/VST3 MIDI in ──┐
//! stereo audio in ────┴─ process() ─ in-gain → [rig render +=] → out-gain
//! ```
//!
//! ## Threading discipline (mirrors `signal-sampler-clap`)
//!
//! - **Audio thread** (`process`): gain-stages the passthrough, `try_lock`s
//!   the bank, dispatches MIDI at in-block offsets and renders the gaps
//!   (StrictLive style), sums the rig onto the dry signal. No loading, no
//!   I/O. The interleaved scratch is sized in `initialize`.
//! - **Loader thread** (spawned by `initialize`): resolves the rig config,
//!   builds a fresh `SamplerBank`, loads the pack (`SamplerBank::load_pack`
//!   itself streams sample preload on a background thread), then swaps the
//!   bank into the shared slot in one locked assignment. Until it finishes —
//!   or if no rig is configured — the plugin is a plain passthrough that
//!   drains and ignores MIDI.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use nice_plug::prelude::*;
use tracing_subscriber::{EnvFilter, fmt};

use signal_sampler::SamplerBank;

use crate::config::{INSTRUMENT_ID, RigConfig};

const PLUGIN_NAME: &str = "FTS Signal";

// ── Shared state (audio thread ↔ loader thread) ─────────────────────────────

pub struct SharedState {
    /// The engine. Audio thread `try_lock`s per block; the loader replaces
    /// the whole bank in one locked assignment when the rig is ready.
    pub bank: Mutex<SamplerBank>,
    /// Rig loaded and playable.
    pub rig_loaded: AtomicBool,
}

impl SharedState {
    fn new() -> Self {
        Self {
            bank: Mutex::new(SamplerBank::new(48_000)),
            rig_loaded: AtomicBool::new(false),
        }
    }
}

/// Loader: resolve the config and load the rig off the audio thread, then
/// swap it in. No rig configured is a supported state (passthrough mode).
fn load_rig(shared: &SharedState, sr: u32) {
    let cfg = match RigConfig::resolve() {
        Ok(Some(c)) => c,
        Ok(None) => {
            tracing::info!("no rig configured — passthrough mode");
            return;
        }
        Err(e) => {
            tracing::error!("rig config: {e} — passthrough mode");
            return;
        }
    };
    tracing::info!(pack = %cfg.pack, "loading rig");
    let mut bank = SamplerBank::with_cache_budget(sr, cfg.cache_budget_bytes());
    if let Err(e) = bank.load_pack(INSTRUMENT_ID, std::path::Path::new(&cfg.pack)) {
        tracing::error!("rig load failed: {e} — passthrough mode");
        return;
    }
    if let Some(ms) = cfg.attack_ms {
        bank.set_attack_ms(INSTRUMENT_ID, ms);
    }
    if let Some(ms) = cfg.release_ms {
        bank.set_release_ms(INSTRUMENT_ID, ms);
    }

    // Swap the finished bank in (one locked assignment). Samples keep
    // streaming in on load_pack's own preload thread; the engine plays what
    // is cached and streams the rest.
    *shared.bank.lock().unwrap() = bank;
    shared.rig_loaded.store(true, Ordering::Release);
    tracing::info!(sr, "rig ready");
}

// ── Parameters ──────────────────────────────────────────────────────────────

/// v0 exposes only I/O gain staging; rig-internal params come later via a
/// host param rescan once the facade exposes the rig's parameter surface.
#[derive(Params)]
pub struct SignalParams {
    /// Gain applied to the incoming audio before the rig sum.
    #[id = "in_gain"]
    pub input_gain: FloatParam,
    /// Gain applied to the summed output.
    #[id = "out_gain"]
    pub output_gain: FloatParam,
}

impl Default for SignalParams {
    fn default() -> Self {
        Self {
            input_gain: FloatParam::new(
                "Input Gain",
                0.0,
                FloatRange::Linear {
                    min: -60.0,
                    max: 24.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            output_gain: FloatParam::new(
                "Output Gain",
                0.0,
                FloatRange::Linear {
                    min: -60.0,
                    max: 24.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
        }
    }
}

// ── Plugin ──────────────────────────────────────────────────────────────────

pub struct FtsSignal {
    params: Arc<SignalParams>,
    pub shared: Arc<SharedState>,
    /// Interleaved stereo scratch the bank renders into (sized in
    /// `initialize` from the host's max block).
    scratch: Vec<f32>,
    /// Sample rate the loader was last started for (0 = never).
    loaded_sr: u32,
}

impl Default for FtsSignal {
    fn default() -> Self {
        Self {
            params: Arc::new(SignalParams::default()),
            shared: Arc::new(SharedState::new()),
            scratch: Vec::new(),
            loaded_sr: 0,
        }
    }
}

impl Plugin for FtsSignal {
    const NAME: &'static str = PLUGIN_NAME;
    const VENDOR: &'static str = "FastTrackStudio";
    const URL: &'static str = "https://fasttrackstudio.com";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    /// Instrument+FX hybrid: rigs can be samplers (MIDI-driven) or, later,
    /// guitar/vocal FX chains — so stereo in AND stereo out.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];
    /// MidiCCs: rigs use CC dynamics/expression, not just notes.
    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        init_logging();
        let sr = buffer_config.sample_rate.round() as u32;
        self.scratch
            .resize((buffer_config.max_buffer_size as usize).max(1) * 2, 0.0);

        // (Re)load the rig when the sample rate changes (first init included).
        if self.loaded_sr != sr {
            self.loaded_sr = sr;
            self.shared.rig_loaded.store(false, Ordering::Release);
            let shared = self.shared.clone();
            std::thread::Builder::new()
                .name("fts-signal-rig-loader".into())
                .spawn(move || load_rig(&shared, sr))
                .ok();
        }

        tracing::info!(sr, "{PLUGIN_NAME}: initialized");
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let frames = buffer.samples();
        if frames == 0 {
            return ProcessStatus::Normal;
        }
        let in_gain = util::db_to_gain(self.params.input_gain.value());
        let out_gain = util::db_to_gain(self.params.output_gain.value());

        // Dry passthrough, gain-staged. (v0: audio does not flow THROUGH the
        // rig — the sampler engine has no audio input; FX-chain rigs are the
        // facade gap documented on the crate.)
        for mut frame in buffer.iter_samples() {
            for sample in frame.iter_mut() {
                *sample *= in_gain;
            }
        }

        // Rig: dispatch MIDI at in-block offsets, render the gaps, sum onto
        // the dry signal. Skipped (events drained) while loading, when no rig
        // is configured, or for the one block where the loader holds the lock.
        let rig_ready = self.shared.rig_loaded.load(Ordering::Acquire);
        let mut rendered = false;
        if rig_ready {
            if let Ok(mut bank) = self.shared.bank.try_lock() {
                if self.scratch.len() < frames * 2 {
                    // Host exceeded its declared max block (shouldn't happen)
                    // — documented allocation exception.
                    self.scratch.resize(frames * 2, 0.0);
                }
                let scratch = &mut self.scratch[..frames * 2];
                scratch.fill(0.0); // bank.render is `+=`

                let mut off = 0usize;
                let mut next_event = context.next_event();
                while let Some(ev) = next_event {
                    let at = (ev.timing() as usize).min(frames);
                    if at > off {
                        bank.render(&mut scratch[off * 2..at * 2]);
                        off = at;
                    }
                    match ev {
                        NoteEvent::NoteOn { note, velocity, .. } => {
                            let vel = (velocity * 127.0).round().clamp(0.0, 127.0) as u8;
                            bank.note_on(INSTRUMENT_ID, note, vel.max(1));
                        }
                        NoteEvent::NoteOff { note, .. } => {
                            bank.note_off(INSTRUMENT_ID, note);
                        }
                        NoteEvent::MidiCC { cc, value, .. } => {
                            let val = (value * 127.0).round().clamp(0.0, 127.0) as u8;
                            bank.cc(INSTRUMENT_ID, cc, val);
                        }
                        _ => {}
                    }
                    next_event = context.next_event();
                }
                if frames > off {
                    bank.render(&mut scratch[off * 2..frames * 2]);
                }
                rendered = true;
            }
        }
        if !rendered {
            while context.next_event().is_some() {}
        }

        // Sum + output gain.
        let output = buffer.as_slice();
        if rendered && output.len() >= 2 {
            let scratch = &self.scratch[..frames * 2];
            for (i, frame) in scratch.chunks_exact(2).enumerate().take(frames) {
                output[0][i] += frame[0];
                output[1][i] += frame[1];
            }
        }
        for ch in output.iter_mut() {
            for sample in ch[..frames].iter_mut() {
                *sample *= out_gain;
            }
        }

        ProcessStatus::Normal
    }
}

fn init_logging() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let log_path = "/tmp/fts-signal-plugin.log";
        if let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
        {
            let _ = fmt::Subscriber::builder()
                .with_env_filter(
                    EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| EnvFilter::new("signal_plugin=debug,warn")),
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
        "FTS Signal rig platform: hosts signal rigs (v0: signalpack sampler \
         rigs) inside the DAW — guitar/drum/vocal/keys rigs to follow",
    );
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsSignal {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsSignalPlug001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Fx];
}
