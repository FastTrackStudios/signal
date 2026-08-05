//! The CLAP plugin shell.
//!
//! Thin adapter between the nice-plug host wrapper and signal-sampler:
//!
//! ```text
//! CLAP transport ─┐
//! CLAP MIDI in ───┤            ┌─ document mode ─ RealtimeScheduler::process_block
//! audio blocks ───┴─ process() ┤                  (offline walker driven by transport)
//!                              └─ StrictLive ──── bank.note_on/off/cc (live divisi
//!                                                 allocator + low_latency gates)
//! ```
//!
//! ## Threading discipline
//!
//! - **Audio thread** (`process`): only `try_lock`s the bank, swaps in the
//!   published `Arc<Schedule>` at the block boundary, walks, renders. No
//!   schedule building, no I/O. Steady-path allocation: none in this crate
//!   (scratch buffers are sized in `initialize`); known pre-existing
//!   engine-side exception: some trigger paths build short strings.
//! - **Loader thread** (spawned by `initialize`): builds a fresh
//!   `SamplerBank`, loads + preloads the patch (see [`crate::config`]), then
//!   swaps it into the shared slot in one assignment. Until it finishes,
//!   `process` outputs silence.
//! - **Document intake** ([`SharedState::set_document`], the phase-3 seam):
//!   annotates OFF the audio thread and publishes via `ArcSwapOption` — the
//!   audio thread adopts it at the next block boundary.
//!
//! While a document schedule is playing, incoming live MIDI is dropped
//! (phase 2; overdub arbitration is a later phase). In every other state the
//! plugin is a normal live sampler.

use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use arc_swap::ArcSwapOption;
use audiocore_core::prelude::*;
use tracing_subscriber::{EnvFilter, fmt};

use signal_sampler::spec::LibrarySpec;
use signal_sampler::{
    BlockTransport, RealtimeScheduler, SamplerBank, Schedule, TrackDocument, annotate,
};

use crate::config::{INSTRUMENT_ID, PatchConfig};

const PLUGIN_NAME: &str = "Signal Sampler";

// ── Shared state (audio thread ↔ background threads) ───────────────────────

pub struct SharedState {
    /// The engine. Audio thread `try_lock`s per block; the loader replaces
    /// the whole bank in one locked assignment when the patch is ready.
    pub bank: Mutex<SamplerBank>,
    /// Published schedule; audio thread adopts at block boundaries.
    pub schedule: ArcSwapOption<Schedule>,
    /// Patch loaded and playable.
    pub patch_loaded: AtomicBool,
    /// Engine sample rate (0 until `initialize`).
    pub sample_rate: AtomicU32,
    /// Loaded instrument's spec (for annotation).
    spec: Mutex<Option<LibrarySpec>>,
    /// Last document received — kept so a sample-rate change (or a document
    /// arriving before the patch) can (re-)annotate.
    doc: Mutex<Option<TrackDocument>>,
}

impl SharedState {
    fn new() -> Self {
        Self {
            bank: Mutex::new(SamplerBank::new(48_000)),
            schedule: ArcSwapOption::empty(),
            patch_loaded: AtomicBool::new(false),
            sample_rate: AtomicU32::new(0),
            spec: Mutex::new(None),
            doc: Mutex::new(None),
        }
    }

    /// **Phase-3 seam.** Hand the plugin its track's [`TrackDocument`]. Call
    /// from any non-audio thread: annotates against the loaded patch's spec
    /// and publishes the schedule; the audio thread swaps it in at the next
    /// block boundary. Returns `Err` (and queues the document) while the
    /// patch is still loading — the loader re-runs annotation when ready.
    pub fn set_document(&self, doc: TrackDocument) -> eyre::Result<()> {
        *self.doc.lock().unwrap() = Some(doc);
        self.annotate_stored()
    }

    /// Drop the document: next block boundary falls back to StrictLive.
    pub fn clear_document(&self) {
        *self.doc.lock().unwrap() = None;
        self.schedule.store(None);
    }

    /// (Re-)annotate the stored document against the current spec + sample
    /// rate. No-op without a stored document.
    fn annotate_stored(&self) -> eyre::Result<()> {
        let Some(doc) = self.doc.lock().unwrap().clone() else {
            return Ok(());
        };
        if !self.patch_loaded.load(Ordering::Acquire) {
            eyre::bail!("patch not loaded yet — document queued");
        }
        let sr = self.sample_rate.load(Ordering::Acquire);
        let spec = self
            .spec
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| eyre::eyre!("no instrument spec — document queued"))?;
        let sched = annotate(&doc, &spec, sr);
        tracing::info!(
            events = sched.events.len(),
            notes = sched.note_count,
            legato = sched.legato_count,
            shorts = sched.short_count,
            seed = sched.seed,
            "schedule published"
        );
        self.schedule.store(Some(Arc::new(sched)));
        Ok(())
    }
}

/// Loader: build + preload the patch off the audio thread, then swap it in.
fn load_patch(shared: &SharedState, sr: u32) {
    let cfg = match PatchConfig::resolve() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("patch config: {e}");
            return;
        }
    };
    tracing::info!(zones = %cfg.zones, "loading patch");
    let mut bank = SamplerBank::with_cache_budget(sr, cfg.cache_budget_bytes());
    let section = cfg.section.clone().unwrap_or_default();
    let mic = cfg.mic.clone().unwrap_or_default();
    let zones = std::path::PathBuf::from(&cfg.zones);
    let res = match (&cfg.config, &cfg.root) {
        (Some(config), Some(root)) => bank.load_instrument_with_config(
            INSTRUMENT_ID,
            std::path::Path::new(config),
            &zones,
            std::path::Path::new(root),
            section,
            mic,
        ),
        _ => bank.load_instrument(
            INSTRUMENT_ID,
            &zones,
            cfg.root.as_deref().map(std::path::Path::new),
            section,
            mic,
        ),
    };
    if let Err(e) = res {
        tracing::error!("patch load failed: {e}");
        return;
    }
    if let Some(mic) = &cfg.solo_mic {
        bank.set_solo_mic(INSTRUMENT_ID, Some(mic.clone()));
    }
    if let Some(ms) = cfg.attack_ms {
        bank.set_attack_ms(INSTRUMENT_ID, ms);
    }
    if let Some(ms) = cfg.release_ms {
        bank.set_release_ms(INSTRUMENT_ID, ms);
    }
    match bank.preload_instrument(INSTRUMENT_ID) {
        Ok(stats) => tracing::info!(?stats, "patch preloaded"),
        Err(e) => tracing::warn!("preload failed (will stream cold): {e}"),
    }
    let spec = bank.instrument_spec(INSTRUMENT_ID);

    // Swap the finished bank in (one locked assignment).
    *shared.bank.lock().unwrap() = bank;
    *shared.spec.lock().unwrap() = spec;
    shared.sample_rate.store(sr, Ordering::Release);
    shared.patch_loaded.store(true, Ordering::Release);
    tracing::info!(sr, "patch ready");

    // A document may have arrived while loading (or survives a sample-rate
    // change) — annotate it now.
    if let Err(e) = shared.annotate_stored() {
        tracing::warn!("post-load annotate: {e}");
    }
}

// ── Parameters (none yet — patch/doc come from config, see crate docs) ─────

#[derive(Params)]
pub struct SamplerParams {}

// ── Plugin ──────────────────────────────────────────────────────────────────

pub struct SignalSamplerClap {
    params: Arc<SamplerParams>,
    pub shared: Arc<SharedState>,
    scheduler: RealtimeScheduler,
    /// Interleaved stereo scratch the bank renders into (sized in
    /// `initialize` from the host's max block).
    scratch: Vec<f32>,
    /// Schedule currently adopted by the scheduler (ptr-compared against the
    /// published one at each block boundary).
    adopted: Option<Arc<Schedule>>,
    /// Sample rate the loader was last started for (0 = never).
    loaded_sr: u32,
    watcher_spawned: bool,
}

impl Default for SignalSamplerClap {
    fn default() -> Self {
        Self {
            params: Arc::new(SamplerParams {}),
            shared: Arc::new(SharedState::new()),
            scheduler: RealtimeScheduler::new(INSTRUMENT_ID),
            scratch: Vec::new(),
            adopted: None,
            loaded_sr: 0,
            watcher_spawned: false,
        }
    }
}

impl Plugin for SignalSamplerClap {
    const NAME: &'static str = PLUGIN_NAME;
    const VENDOR: &'static str = "FastTrackStudio";
    const URL: &'static str = "https://fasttrackstudio.com";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    /// Instrument: no audio in, stereo out.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];
    /// MidiCCs: the live path needs CC1/CC2 dynamics + CC58 keyswitches.
    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    // Headless sampler shell — the host shows its generic parameter UI.
    type Editor = ();
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn activate(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl ActivateContext<Self>,
    ) -> bool {
        init_logging();
        let sr = buffer_config.sample_rate.round() as u32;
        self.scratch
            .resize((buffer_config.max_buffer_size as usize).max(1) * 2, 0.0);

        // (Re)load the patch when the sample rate changes (first init included).
        if self.loaded_sr != sr {
            self.loaded_sr = sr;
            self.shared.patch_loaded.store(false, Ordering::Release);
            self.shared.sample_rate.store(sr, Ordering::Release);
            let shared = self.shared.clone();
            std::thread::Builder::new()
                .name("signal-sampler-clap-loader".into())
                .spawn(move || load_patch(&shared, sr))
                .ok();
        }

        // Dev-only document file watcher (phase-3 seam — see doc_watch).
        if !self.watcher_spawned {
            self.watcher_spawned = true;
            if let Ok(path) = std::env::var(crate::doc_watch::DOC_ENV) {
                if !path.is_empty() {
                    let shared = self.shared.clone();
                    std::thread::Builder::new()
                        .name("signal-sampler-clap-doc-watch".into())
                        .spawn(move || crate::doc_watch::watch_document_file(path, shared))
                        .ok();
                }
            }
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
        let output = buffer.as_slice();
        if frames == 0 || output.len() < 2 {
            return ProcessStatus::Normal;
        }

        if !self.shared.patch_loaded.load(Ordering::Acquire) {
            // Still loading: drain events, output silence.
            while context.next_event().is_some() {}
            for ch in output.iter_mut() {
                ch[..frames].fill(0.0);
            }
            return ProcessStatus::Normal;
        }

        let Ok(mut bank) = self.shared.bank.try_lock() else {
            // Loader is swapping the bank in — one silent block.
            while context.next_event().is_some() {}
            for ch in output.iter_mut() {
                ch[..frames].fill(0.0);
            }
            return ProcessStatus::Normal;
        };

        // Block boundary: adopt a newly published schedule (atomic swap).
        let published = self.shared.schedule.load_full();
        let changed = match (&self.adopted, &published) {
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            (None, None) => false,
            _ => true,
        };
        if changed {
            self.adopted = published.clone();
            self.scheduler.set_schedule(published);
        }

        let transport = context.transport();
        let t = BlockTransport {
            playing: transport.playing,
            pos_frame: transport.pos_samples().unwrap_or(0),
            tempo_bpm: transport.tempo,
        };

        if self.scratch.len() < frames * 2 {
            // Host exceeded its declared max block (shouldn't happen) —
            // documented allocation exception.
            self.scratch.resize(frames * 2, 0.0);
        }
        let scratch = &mut self.scratch[..frames * 2];

        let doc_active = self.scheduler.process_block(&mut bank, &t, scratch);
        if doc_active {
            // Document owns the engine: live input is dropped (phase 2).
            while context.next_event().is_some() {}
        } else {
            // StrictLive: dispatch events at their in-block offsets, rendering
            // the gaps between them (scratch was cleared by process_block).
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
        }

        // De-interleave into the host buffer.
        for (i, frame) in scratch.chunks_exact(2).enumerate().take(frames) {
            output[0][i] = frame[0];
            output[1][i] = frame[1];
        }

        ProcessStatus::Normal
    }
}

fn init_logging() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let log_path = "/tmp/signal-sampler-clap.log";
        if let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
        {
            let _ = fmt::Subscriber::builder()
                .with_env_filter(
                    EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| EnvFilter::new("signal_sampler_clap=debug,warn")),
                )
                .with_writer(file)
                .with_ansi(false)
                .try_init();
        }
    });
}

impl ClapPlugin for SignalSamplerClap {
    const CLAP_ID: &'static str = "com.fasttrackstudio.signal-sampler";
    const CLAP_DESCRIPTION: Option<&'static str> = Some(
        "Sample engine with document-mode lookahead legato — plays the score \
         ahead of the transport so transitions arrive on the grid",
    );
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Sampler,
        ClapFeature::Stereo,
    ];
}
