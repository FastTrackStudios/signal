//! Shared daw-backed host for signal live rigs.
//!
//! Every live rig (guitar, bass, drums, keys, synth) is **a tiny daw project
//! running on daw's realtime audio engine**: a seeded one-off project, one or
//! more tracks whose FX slots carry the rig's `PluginInstance`s, a
//! [`Meters`] bank for post-fader peaks, and a rolling transport so the
//! renderer runs every block. Before this crate each rig engine hand-rolled
//! that bootstrap (`GuitarRig`, `SamplerRig`, `KeysRig` — three private
//! copies); [`RigProject`] + [`RigHost`] are the one shared implementation.
//!
//! ```no_run
//! use signal_rig_host::RigProject;
//! # fn main() -> eyre::Result<()> {
//! let project = RigProject::new("FTS Keys");
//! let track = project.add_track("Keys")?;
//! let fx = project.add_fx_slot(&track, "keys")?;
//! let host = project.start_output(&daw_audio_io::AudioIoPrefs::default())?;
//! let meters = host.install_meters(1);
//! // … insert PluginInstances into `fx` via host.daw() …
//! host.play();
//! # Ok(()) }
//! ```
//!
//! The build order is the one every rig already used and must keep:
//! **tracks + slots → engine → meters → instruments → play**. [`RigProject`]
//! (pre-engine) and [`RigHost`] (post-engine) encode that ordering in types.
//!
//! Also home to the small cross-rig utilities every backend copy-pasted:
//! poison-tolerant locking ([`lock::LockExt`]), panic payload text
//! ([`lock::panic_message`]), the styx config-dir store ([`store`]), and the
//! process-unique project guid ([`uuid_string`]).

use std::sync::{Arc, Mutex};

use daw::service::{
    FxChainContext, FxChains, ProjectContext, ProjectInfo, RecordInput, TrackRef, Tracks,
};
use daw::standalone::Standalone;
use daw::standalone::audio_engine::AudioEngine;
#[cfg(all(feature = "pipewire", target_os = "linux"))]
use daw::standalone::audio_engine::DuplexAudioEngine;
use daw::standalone::metering::Meters;
use daw::standalone::transport_engine::{PlayStateRepr, TransportShared};
use daw_audio_io::AudioIoPrefs;
use daw_audio_io::duplex::EngineStats;

pub mod gestures;
pub mod lock;
pub mod mixer;
pub mod store;

/// A realtime engine the host can run on — implemented by daw's cpal
/// [`AudioEngine`] and (on Linux with the `pipewire` feature) the native
/// duplex `pw_filter` engine. The engine type is a [`RigHost`] type parameter
/// so output-only hosts stay `Sync` (the duplex engine owns raw PipeWire
/// pointers and is `Send`-only — rigs that use it already serialize through a
/// `Mutex`).
pub trait HostedEngine: Sized {
    fn open(
        daw: Standalone,
        project_guid: String,
        shared: Arc<TransportShared>,
        prefs: &AudioIoPrefs,
    ) -> eyre::Result<Self>;
    fn sample_rate(&self) -> u32;
    /// Live realtime metrics (render time / block size / xruns) — only the
    /// native duplex engine surfaces these.
    fn stats(&self) -> Option<Arc<EngineStats>> {
        None
    }
}

impl HostedEngine for AudioEngine {
    fn open(
        daw: Standalone,
        project_guid: String,
        shared: Arc<TransportShared>,
        prefs: &AudioIoPrefs,
    ) -> eyre::Result<Self> {
        AudioEngine::with_project_prefs(daw, project_guid, shared, prefs)
            .map_err(|e| eyre::eyre!("rig host: audio engine failed: {e}"))
    }
    fn sample_rate(&self) -> u32 {
        AudioEngine::sample_rate(self)
    }
}

#[cfg(all(feature = "pipewire", target_os = "linux"))]
impl HostedEngine for DuplexAudioEngine {
    fn open(
        daw: Standalone,
        project_guid: String,
        shared: Arc<TransportShared>,
        prefs: &AudioIoPrefs,
    ) -> eyre::Result<Self> {
        DuplexAudioEngine::with_project_prefs(daw, project_guid, shared, prefs)
            .map_err(|e| eyre::eyre!("rig host: audio engine failed: {e}"))
    }
    fn sample_rate(&self) -> u32 {
        DuplexAudioEngine::sample_rate(self)
    }
    fn stats(&self) -> Option<Arc<EngineStats>> {
        Some(DuplexAudioEngine::stats(self))
    }
}

/// The duplex host's engine: the native PipeWire `pw_filter` engine where
/// available, the cpal engine (with a live input stream) elsewhere.
#[cfg(all(feature = "pipewire", target_os = "linux"))]
pub type DuplexEngine = DuplexAudioEngine;
#[cfg(not(all(feature = "pipewire", target_os = "linux")))]
pub type DuplexEngine = AudioEngine;

/// A running duplex (live-input) rig host — `Send`-only under the native
/// PipeWire engine; serialize access through a `Mutex` like the guitar rig.
pub type DuplexRigHost = RigHost<DuplexEngine>;

/// A seeded rig project **before** the engine opens: add tracks and reserve
/// FX slots here, then [`start`](Self::start) the engine to get a [`RigHost`].
pub struct RigProject {
    daw: Standalone,
    project_guid: String,
}

impl RigProject {
    /// Seed a fresh one-off project named `project_name` and make it current
    /// (so the track/FX services target it).
    pub fn new(project_name: &str) -> Self {
        let daw = Standalone::new();
        let project_guid = uuid_string();
        daw.seed_project(ProjectInfo {
            guid: project_guid.clone(),
            name: project_name.to_string(),
            path: String::new(),
        });
        daw.set_current_project(&project_guid);
        Self { daw, project_guid }
    }

    /// The underlying daw service handle (cheap to clone).
    pub fn daw(&self) -> &Standalone {
        &self.daw
    }

    /// Add a track; returns its guid.
    pub fn add_track(&self, name: &str) -> eyre::Result<String> {
        add_track(&self.daw, name)
    }

    /// Arm `track` to monitor hardware input `channel` — what makes daw's
    /// engine open a live input stream and feed the track's bus.
    pub fn arm_input(&self, track_guid: &str, channel: u32) -> eyre::Result<()> {
        <Standalone as Tracks>::set_record_input(
            &self.daw,
            ProjectContext::Current,
            TrackRef::guid(track_guid),
            RecordInput::Audio { channel },
        )
        .map_err(|e| eyre::eyre!("rig host: set record input failed: {e}"))
    }

    /// Reserve one FX slot on `track` (the add + guid-fetch dance); returns
    /// the slot's guid — constant for the project's life, so swapping the
    /// instance behind it never rebuilds the renderer's snapshot.
    pub fn add_fx_slot(&self, track_guid: &str, label: &str) -> eyre::Result<String> {
        add_fx_slot(&self.daw, track_guid, label)
    }

    /// Open the output-only realtime engine (a sampler/synth generates,
    /// never records) and return the running host. `prefs.sample_rate == 0`
    /// requests 48 kHz (the rig default); `want_input` is forced off.
    pub fn start_output(self, prefs: &AudioIoPrefs) -> eyre::Result<RigHost> {
        let mut io = prefs.clone();
        io.want_input = false;
        self.start_with::<AudioEngine>(&io)
    }

    /// Open the duplex (live input → FX chain → output) engine — the native
    /// PipeWire `pw_filter` engine on Linux with the `pipewire` feature, the
    /// cpal engine elsewhere. `want_input` is forced on.
    pub fn start_duplex(self, prefs: &AudioIoPrefs) -> eyre::Result<DuplexRigHost> {
        let mut io = prefs.clone();
        io.want_input = true;
        self.start_with::<DuplexEngine>(&io)
    }

    fn start_with<E: HostedEngine>(self, io: &AudioIoPrefs) -> eyre::Result<RigHost<E>> {
        request_low_latency_quantum(io);
        let req_rate = if io.sample_rate != 0 {
            io.sample_rate
        } else {
            48_000
        };
        let shared = Arc::new(TransportShared::new(req_rate, 120.0));
        let engine = E::open(self.daw.clone(), self.project_guid.clone(), shared.clone(), io)?;
        let sample_rate = engine.sample_rate();
        Ok(RigHost {
            daw: self.daw,
            project_guid: self.project_guid,
            shared,
            engine,
            meters: Mutex::new(Meters::new(0)),
            sample_rate,
        })
    }
}

/// A running rig host: the daw project + realtime engine + meters + transport
/// behind every live rig. Owns the engine (drop = stop audio).
///
/// The default engine parameter is the output-only cpal engine (`Sync`, so a
/// sampler rig can share the host freely); [`DuplexRigHost`] is the live-input
/// variant.
pub struct RigHost<E: HostedEngine = AudioEngine> {
    daw: Standalone,
    project_guid: String,
    shared: Arc<TransportShared>,
    engine: E,
    /// Latest installed meter bank (re-installable when the track set grows).
    meters: Mutex<Arc<Meters>>,
    sample_rate: u32,
}

impl<E: HostedEngine> RigHost<E> {
    /// The underlying daw service handle (cheap to clone) — insert plugin
    /// instances, push live MIDI, drive `Tracks`/`FxChains`/`Routing` here.
    pub fn daw(&self) -> &Standalone {
        &self.daw
    }

    pub fn project_guid(&self) -> &str {
        &self.project_guid
    }

    /// The engine's resolved sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Install a fresh [`Meters`] bank with `cells` cells (one per metered
    /// track, in project track order) and hand it to the renderer. Re-install
    /// after adding tracks; the renderer reads the bank fresh each block.
    pub fn install_meters(&self, cells: usize) -> Arc<Meters> {
        let meters = Meters::new(cells);
        self.daw.set_meters(meters.clone());
        *self.meters.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = meters.clone();
        meters
    }

    /// The most recently installed meter bank.
    pub fn meters(&self) -> Arc<Meters> {
        self.meters
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Roll the transport so the renderer runs (and audio flows) every block.
    pub fn play(&self) {
        self.shared.set_play_state(PlayStateRepr::Playing);
    }

    /// The shared transport handle.
    pub fn transport(&self) -> &Arc<TransportShared> {
        &self.shared
    }

    /// Live realtime metrics (render time / block size / xruns) — only the
    /// native duplex engine surfaces these; `None` under cpal.
    pub fn stats(&self) -> Option<Arc<EngineStats>> {
        self.engine.stats()
    }

    /// Add a track after the engine opened (the per-track sampler API grows
    /// its project live); returns its guid.
    pub fn add_track(&self, name: &str) -> eyre::Result<String> {
        add_track(&self.daw, name)
    }

    /// Reserve one FX slot on `track` after the engine opened.
    pub fn add_fx_slot(&self, track_guid: &str, label: &str) -> eyre::Result<String> {
        add_fx_slot(&self.daw, track_guid, label)
    }
}

fn add_track(daw: &Standalone, name: &str) -> eyre::Result<String> {
    <Standalone as Tracks>::add(daw, ProjectContext::Current, name, None)
        .map_err(|e| eyre::eyre!("rig host: add track '{name}' failed: {e}"))
}

fn add_fx_slot(daw: &Standalone, track_guid: &str, label: &str) -> eyre::Result<String> {
    let fx_ctx = FxChainContext::track(track_guid.to_string());
    let idx = <Standalone as FxChains>::add(daw, fx_ctx.clone(), label)
        .map_err(|e| eyre::eyre!("rig host: reserve fx slot '{label}' failed: {e}"))?;
    <Standalone as FxChains>::get(daw, fx_ctx, idx)
        .map(|fx| fx.guid)
        .ok_or_else(|| eyre::eyre!("rig host: fx slot '{label}' vanished after add"))
}

/// Low latency under the JACK/PipeWire shim: ask PipeWire for the quantum
/// before the JACK client connects (the client can't set the buffer there).
/// No-op unless built with the `jack` feature, or when `PIPEWIRE_LATENCY` is
/// already set / no buffer size is requested.
pub fn request_low_latency_quantum(prefs: &AudioIoPrefs) {
    #[cfg(feature = "jack")]
    {
        if prefs.buffer_size != 0 && std::env::var_os("PIPEWIRE_LATENCY").is_none() {
            let rate = if prefs.sample_rate != 0 {
                prefs.sample_rate
            } else {
                48_000
            };
            let buf = prefs.buffer_size;
            unsafe { std::env::set_var("PIPEWIRE_LATENCY", format!("{buf}/{rate}")) };
            tracing::info!(quantum = %format!("{buf}/{rate}"), "rig host: requesting PipeWire low-latency quantum");
        }
    }
    #[cfg(not(feature = "jack"))]
    let _ = prefs;
}

/// Process-unique guid string for rig project guids — shared by every rig
/// host so ids never collide within one engine process.
pub fn uuid_string() -> String {
    uuid::new_v4_string()
}

mod uuid {
    //! Minimal UUIDv4-ish string generator — avoids pulling the `uuid` crate
    //! just for rig project guids. Not cryptographically strong; only needs
    //! to be unique within this process.
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    pub fn new_v4_string() -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let c = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id() as u64;
        format!("signal-rig-{nanos:x}-{pid:x}-{c:x}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_strings_are_process_unique() {
        let a = uuid_string();
        let b = uuid_string();
        assert_ne!(a, b);
        assert!(a.starts_with("signal-rig-"));
    }

    /// Device-free: the pre-engine stage seeds a project and builds tracks +
    /// constant-guid FX slots.
    #[test]
    fn project_builds_tracks_and_slots() {
        let project = RigProject::new("Rig Host Test");
        let track = project.add_track("In").expect("add track");
        let a = project.add_fx_slot(&track, "slot-0").expect("slot 0");
        let b = project.add_fx_slot(&track, "slot-1").expect("slot 1");
        assert_ne!(a, b, "each reserved slot gets its own guid");
    }
}
