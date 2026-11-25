//! The CLAP plugin shell.
//!
//! Thin adapter between the nice-plug host wrapper and the sampler's
//! [`DocumentPlayer`]:
//!
//! ```text
//! host timeline ─ ext::read_timeline ─ annotator thread ─ set_document ─┐
//!                 (host-timeline/1)                    (Arc<Schedule>) │
//! CLAP transport ─┐                                                    ▼
//! CLAP MIDI in ───┴─ process() ── DocumentPlayer::process ── document bank (ahead of the transport)
//!                                                         └─ live bank (live input beside it)
//! ```
//!
//! ## Threading discipline
//!
//! - **Audio thread** (`process`): `try_lock`s the player (the loader swaps
//!   a new one in), loads the published schedule (lock-free), collects the
//!   block's MIDI into a preallocated list, plays. No allocation, no I/O.
//! - **Loader thread** (spawned by `activate`): parses the patch once and
//!   builds three banks of it sharing one sample cache — the document's,
//!   the live input's and the seek worker's — then swaps the player in.
//!   Until then `process` outputs silence.
//! - **Main thread** (`activate`, the host's `timeline` events): reads the
//!   host's timeline ([`crate::ext`]); an annotator thread turns it into a
//!   schedule and publishes it ([`SharedState::set_document`]). A retired
//!   schedule is freed by the next publish once the audio thread let go
//!   of it, never on the audio thread.
//!
//! ## Latency
//!
//! None, in document mode or not: a note's lead is read ahead from the
//! document, not delayed into the host's compensation.

use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use arc_swap::ArcSwapOption;
use plugin_core::prelude::*;
use tracing_subscriber::{EnvFilter, fmt};

use sampler::spec::LibrarySpec;
use sampler::{
    BlockTransport, DocumentPlayer, Input, LiveEvent, PlayerPatch, SamplerBank, Schedule,
    SeekPolicy, TrackDocument, annotate,
};

use crate::config::{INSTRUMENT_ID, PatchConfig};

const PLUGIN_NAME: &str = "Signal Sampler";

/// The round-robin seed of a document read from the host. Fixed, so a
/// track plays the same take every time it is opened.
pub const SEED: u64 = 0x5167_A15A_3D0C_0001;

/// The most MIDI events a block keeps (more are dropped).
const MAX_EVENTS: usize = 2048;

// ── Shared state (audio thread ↔ background threads) ───────────────────────

pub struct SharedState {
    /// The player. The audio thread `try_lock`s it per block; the loader
    /// replaces it whole when a patch is ready.
    pub player: Mutex<Option<DocumentPlayer>>,
    /// Published schedule; the audio thread adopts it at block boundaries.
    pub schedule: ArcSwapOption<Schedule>,
    /// Schedules published before, kept until the audio thread let go.
    retired: Mutex<Vec<Arc<Schedule>>>,
    /// The version of the document last published.
    published: AtomicU64,
    /// Patch loaded and playable.
    pub patch_loaded: AtomicBool,
    /// Engine sample rate (0 until `activate`).
    pub sample_rate: AtomicU32,
    /// Loaded instrument's spec (for annotation).
    spec: Mutex<Option<LibrarySpec>>,
    /// Last document received — kept so a sample-rate change (or a document
    /// arriving before the patch) can (re-)annotate.
    doc: Mutex<Option<TrackDocument>>,
    /// The instance's `clap_host` (`clap_init`).
    pub host: AtomicPtr<nice_plug::wrapper::clap::clap_host>,
    /// The host's extensions, found at activation.
    pub link: Mutex<Option<crate::ext::HostLink>>,
    /// The timeline revision last read.
    pub timeline_read: AtomicU64,
    /// The host stopped sending the track's notes: what arrives is live.
    pub claimed: AtomicBool,
    /// A fresh bank of the loaded patch (sharing its samples): what a
    /// reference render plays on ([`crate::reference`]).
    pub(crate) bank_maker: Mutex<Option<BankMaker>>,
}

/// Makes a fresh bank of the loaded patch.
pub(crate) type BankMaker = Arc<dyn Fn() -> SamplerBank + Send + Sync>;

impl SharedState {
    fn new() -> Self {
        Self {
            player: Mutex::new(None),
            schedule: ArcSwapOption::empty(),
            retired: Mutex::new(Vec::new()),
            published: AtomicU64::new(0),
            patch_loaded: AtomicBool::new(false),
            sample_rate: AtomicU32::new(0),
            spec: Mutex::new(None),
            doc: Mutex::new(None),
            host: AtomicPtr::new(std::ptr::null_mut()),
            link: Mutex::new(None),
            timeline_read: AtomicU64::new(0),
            claimed: AtomicBool::new(false),
            bank_maker: Mutex::new(None),
        }
    }

    /// Hand the plugin its track's [`TrackDocument`]. Call from any
    /// non-audio thread: annotates against the loaded patch's spec and
    /// publishes the schedule; the audio thread swaps it in at the next
    /// block boundary. A document older than the one published is
    /// ignored. Returns `Err` (and queues the document) while the patch is
    /// still loading — the loader annotates it when ready.
    ///
    /// # Errors
    ///
    /// Returns an error if the patch is not yet loaded or annotation fails.
    pub fn set_document(&self, doc: TrackDocument) -> eyre::Result<()> {
        *self.doc.lock().unwrap_or_else(PoisonError::into_inner) = Some(doc);
        self.annotate_stored()
    }

    /// Drop the document: next block boundary falls back to live.
    pub fn clear_document(&self) {
        *self.doc.lock().unwrap_or_else(PoisonError::into_inner) = None;
        self.publish(None);
    }

    /// (Re-)annotate the stored document against the current spec + sample
    /// rate. No-op without a stored document.
    fn annotate_stored(&self) -> eyre::Result<()> {
        let Some(doc) = self
            .doc
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
        else {
            return Ok(());
        };
        if !self.patch_loaded.load(Ordering::Acquire) {
            eyre::bail!("patch not loaded yet — document queued");
        }
        let sr = self.sample_rate.load(Ordering::Acquire);
        let spec = self
            .spec
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or_else(|| eyre::eyre!("no instrument spec — document queued"))?;
        let started = std::time::Instant::now();
        let sched = annotate(&doc, &spec, sr);
        if doc.version < self.published.load(Ordering::Acquire) {
            return Ok(()); // a newer one was published meanwhile
        }
        self.published.store(doc.version, Ordering::Release);
        tracing::info!(
            doc.version = doc.version,
            events = sched.events.len(),
            notes = sched.note_count,
            legato = sched.legato_count,
            shorts = sched.short_count,
            max_prefire_ms = sched.max_prefire_lead * 1000 / u64::from(sr.max(1)),
            annotate_ms = started.elapsed().as_millis() as u64,
            "schedule published"
        );
        let sched = Arc::new(sched);
        self.publish(Some(Arc::clone(&sched)));
        crate::reference::write_if_asked(self, &doc, &sched);
        Ok(())
    }

    /// The loaded instrument's spec.
    pub(crate) fn spec_copy(&self) -> Option<LibrarySpec> {
        self.spec
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Swap `sched` in; free the retired schedules the audio thread no
    /// longer holds (here, never on the audio thread).
    fn publish(&self, sched: Option<Arc<Schedule>>) {
        let old = self.schedule.swap(sched);
        let mut retired = self.retired.lock().unwrap_or_else(PoisonError::into_inner);
        retired.extend(old);
        retired.retain(|s| Arc::strong_count(s) > 1);
    }
}

/// Three banks of the patch `cfg` names at `sr`, sharing one sample cache:
/// the document's, the live input's and the seek worker's, and the spec.
fn load_banks(
    cfg: &PatchConfig,
    sr: u32,
) -> eyre::Result<([SamplerBank; 3], Option<LibrarySpec>, BankMaker)> {
    let zones = std::path::PathBuf::from(&cfg.zones);
    let mut patch = match (&cfg.config, &cfg.root) {
        (Some(config), Some(root)) => PlayerPatch::load_merged(
            std::path::Path::new(config),
            &zones,
            std::path::Path::new(root),
        )?,
        (_, Some(root)) => PlayerPatch::load(&zones, std::path::Path::new(root))?,
        (_, None) => PlayerPatch::from_spec(sampler::LibrarySpec::from_file(&zones)?),
    };
    let section = cfg.section.clone().unwrap_or_default();
    let mic = cfg.mic.clone().unwrap_or_default();
    let name = zones
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("block")
        .to_string();
    let budget = cfg.cache_budget_bytes();
    let (solo, attack, release) = (cfg.solo_mic.clone(), cfg.attack_ms, cfg.release_ms);
    let make = move |patch: PlayerPatch| {
        let mut bank = SamplerBank::with_cache_budget(sr, budget);
        bank.load_patch(
            INSTRUMENT_ID,
            patch,
            name.clone(),
            section.clone(),
            mic.clone(),
        );
        if let Some(mic) = &solo {
            bank.set_solo_mic(INSTRUMENT_ID, Some(mic.clone()));
        }
        if let Some(ms) = attack {
            bank.set_attack_ms(INSTRUMENT_ID, ms);
        }
        if let Some(ms) = release {
            bank.set_release_ms(INSTRUMENT_ID, ms);
        }
        bank
    };
    let mut doc = make(patch.clone());
    match doc.preload_instrument(INSTRUMENT_ID) {
        Ok(stats) => tracing::info!(?stats, "patch preloaded"),
        Err(e) => tracing::warn!("preload failed (will stream cold): {e}"),
    }
    // The other two read the document bank's decoded samples.
    patch.shared_cache = doc.cache_handle_for(INSTRUMENT_ID);
    let live = make(patch.clone());
    let shadow = make(patch.clone());
    let spec = doc.instrument_spec(INSTRUMENT_ID);
    let maker: BankMaker = Arc::new(move || make(patch.clone()));
    Ok(([doc, live, shadow], spec, maker))
}

/// Loader: build + preload the patch off the audio thread, then swap the
/// player in.
fn load_patch(shared: &SharedState, sr: u32, max_block: usize) {
    let cfg = match PatchConfig::resolve() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("patch config: {e}");
            return;
        }
    };
    tracing::info!(zones = %cfg.zones, "loading patch");
    let ([doc, live, shadow], spec, maker) = match load_banks(&cfg, sr) {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("patch load failed: {e}");
            return;
        }
    };
    let player = DocumentPlayer::new(INSTRUMENT_ID, doc, Some(live), max_block);
    let player = match player.with_worker(shadow, SeekPolicy::live(sr)) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("no seek worker: {e}");
            return;
        }
    };
    // Swap the finished player in (one locked assignment; the old one is
    // dropped here, on the loader).
    let old = shared
        .player
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .replace(player);
    drop(old);
    *shared.spec.lock().unwrap_or_else(PoisonError::into_inner) = spec;
    *shared
        .bank_maker
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some(maker);
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
    /// Interleaved stereo scratch the player renders into (sized in
    /// `activate` from the host's max block).
    scratch: Vec<f32>,
    /// The block's MIDI, collected (capacity [`MAX_EVENTS`]).
    events: Vec<(u32, LiveEvent)>,
    /// Sample rate the loader was last started for (0 = never).
    loaded_sr: u32,
    watcher_spawned: bool,
}

impl Default for SignalSamplerClap {
    fn default() -> Self {
        Self {
            params: Arc::new(SamplerParams {}),
            shared: Arc::new(SharedState::new()),
            scratch: Vec::new(),
            events: Vec::with_capacity(MAX_EVENTS),
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
    /// `MidiCCs`: the live path needs CC1/CC2 dynamics + CC58 keyswitches.
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
        let max_block = (buffer_config.max_buffer_size as usize).max(1);
        self.scratch.resize(max_block * 2, 0.0);

        // (Re)load the patch when the sample rate changes (first init included).
        if self.loaded_sr != sr {
            self.loaded_sr = sr;
            self.shared.patch_loaded.store(false, Ordering::Release);
            self.shared.sample_rate.store(sr, Ordering::Release);
            let shared = self.shared.clone();
            std::thread::Builder::new()
                .name("signal-sampler-clap-loader".into())
                .spawn(move || load_patch(&shared, sr, max_block))
                .ok();
        }

        // The host's timeline, when it offers one (main thread).
        crate::ext::activate(&self.shared);

        // A document file, for hosts without a timeline (doc_watch).
        if !self.watcher_spawned {
            self.watcher_spawned = true;
            if let Ok(path) = std::env::var(crate::doc_watch::DOC_ENV)
                && !path.is_empty()
            {
                let shared = self.shared.clone();
                std::thread::Builder::new()
                    .name("signal-sampler-clap-doc-watch".into())
                    .spawn(move || crate::doc_watch::watch_document_file(path, shared))
                    .ok();
            }
        }

        tracing::info!(sr, "{PLUGIN_NAME}: initialized");
        true
    }

    fn deactivate(&mut self) {
        crate::ext::release(&self.shared);
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
        let silence = |output: &mut [&mut [f32]]| {
            for ch in output.iter_mut() {
                ch[..frames].fill(0.0);
            }
        };
        if !self.shared.patch_loaded.load(Ordering::Acquire) {
            while context.next_event().is_some() {}
            silence(output);
            return ProcessStatus::Normal;
        }
        let Ok(mut player) = self.shared.player.try_lock() else {
            // The loader is swapping the player in — one silent block.
            while context.next_event().is_some() {}
            silence(output);
            return ProcessStatus::Normal;
        };
        let Some(player) = player.as_mut() else {
            while context.next_event().is_some() {}
            silence(output);
            return ProcessStatus::Normal;
        };

        let transport = context.transport();
        let t = BlockTransport {
            playing: transport.playing,
            pos_frame: transport.pos_samples().unwrap_or(0),
            tempo_bpm: transport.tempo,
        };
        collect_events(context, &mut self.events, frames);
        let input = if self.shared.claimed.load(Ordering::Acquire) {
            Input::Live
        } else {
            Input::Track
        };

        if self.scratch.len() < frames * 2 {
            // Host exceeded its declared max block (shouldn't happen) —
            // documented allocation exception.
            self.scratch.resize(frames * 2, 0.0);
        }
        let scratch = &mut self.scratch[..frames * 2];
        let published = self.shared.schedule.load();
        player.process((*published).as_ref(), &t, input, &self.events, scratch);

        // De-interleave into the host buffer.
        for (i, frame) in scratch.chunks_exact(2).enumerate().take(frames) {
            output[0][i] = frame[0];
            output[1][i] = frame[1];
        }
        ProcessStatus::Normal
    }
}

/// The block's MIDI as the player takes it, at clamped frames, in order.
fn collect_events(
    context: &mut impl ProcessContext<SignalSamplerClap>,
    out: &mut Vec<(u32, LiveEvent)>,
    frames: usize,
) {
    out.clear();
    while let Some(ev) = context.next_event() {
        let at = ev.timing().min(frames.saturating_sub(1) as u32);
        let live = match ev {
            // A wildcard key names no single note (CLAP allows it): skipped.
            NoteEvent::NoteOn { key, velocity, .. } => key.number().map(|note| LiveEvent::NoteOn {
                note,
                velocity: (velocity * 127.0).round().clamp(1.0, 127.0) as u8,
            }),
            NoteEvent::NoteOff { key, .. } => key.number().map(|note| LiveEvent::NoteOff { note }),
            NoteEvent::MidiCC { cc, value, .. } => Some(LiveEvent::Cc {
                cc,
                value: (value * 127.0).round().clamp(0.0, 127.0) as u8,
            }),
            _ => None,
        };
        if let Some(live) = live
            && out.len() < out.capacity()
        {
            out.push((at, live));
        }
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
        "Sample engine with document-mode lookahead legato — plays the track's \
         timeline ahead of the transport so transitions arrive on the grid",
    );
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Sampler,
        ClapFeature::Stereo,
    ];

    fn clap_extension(id: &std::ffi::CStr) -> Option<*const std::ffi::c_void> {
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
        _plugin: *const nice_plug::wrapper::clap::clap_plugin,
        host: *const nice_plug::wrapper::clap::clap_host,
    ) {
        if let Ok(shared) = state.clone().downcast::<SharedState>() {
            crate::ext::init(&shared, host);
        }
    }
}
