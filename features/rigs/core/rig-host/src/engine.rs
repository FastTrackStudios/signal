//! One engine per process, rigs as tracks in it (docs/design/one-engine.md).
//!
//! An [`Engine`] is the DAW a process runs: one `Standalone`, one project,
//! one realtime audio engine on one device, the project's own transport. A
//! [`Rig`] is a folder of tracks in that project — its FX slots hold the
//! rig's `PluginInstance`s — so any number of rigs play at once, beside
//! whatever else the project holds (Session's tracks, when the engine is
//! Session's). More engines means running the same engine again, in another
//! process.
//!
//! ```no_run
//! use signal_rig_host::engine::Engine;
//! # fn main() -> eyre::Result<()> {
//! let engine = Engine::new("Signal");
//! let guitar = engine.add_rig("Guitar")?;
//! let di = guitar.add_track("DI")?;
//! guitar.arm_input(&di, 0)?;
//! let amp = guitar.add_fx_slot(&di, "amp")?;
//! let keys = engine.add_rig("Keys")?;
//! let layer = keys.add_track("Layer 1")?;
//! engine.start_audio(&audio::io::AudioIoPrefs::default())?;
//! # let _ = (amp, layer); Ok(()) }
//! ```

use std::sync::{Arc, Mutex};

#[cfg(not(target_arch = "wasm32"))]
use audio::io::AudioIoPrefs;
#[cfg(not(target_arch = "wasm32"))]
use audio::io::duplex::EngineStats;
use daw::service::handle::DawHandle as _;
use daw::service::{ProjectContext, ProjectInfo, TrackRef, Tracks as _};
use daw::standalone::Standalone;
#[cfg(not(target_arch = "wasm32"))]
use daw::standalone::metering::Meters;
use daw::standalone::transport_engine::TransportShared;

#[cfg(not(target_arch = "wasm32"))]
use crate::{DuplexEngine, HostedEngine};
use crate::{lock::LockExt as _, uuid_string};

/// The process's engine, once the app installs one ([`install_shared`]).
static SHARED: Mutex<Option<Arc<Engine>>> = Mutex::new(None);

/// Make `engine` the process's engine: every rig opened from here on joins
/// it instead of opening an engine (and a device) of its own. An app calls
/// this once at start-up, before its rigs open; without it each rig keeps
/// an engine to itself (tests, command-line tools).
pub fn install_shared(engine: Arc<Engine>) {
    *SHARED.lock_ok() = Some(engine);
}

/// The process's engine, if the app installed one.
#[must_use]
pub fn shared() -> Option<Arc<Engine>> {
    SHARED.lock_ok().clone()
}

/// A running device, whatever engine type opened it.
#[cfg(not(target_arch = "wasm32"))]
trait Running: Send {
    fn sample_rate(&self) -> u32;
    fn stats(&self) -> Option<Arc<EngineStats>>;
}

#[cfg(not(target_arch = "wasm32"))]
impl<E: HostedEngine + Send> Running for E {
    fn sample_rate(&self) -> u32 {
        HostedEngine::sample_rate(self)
    }
    fn stats(&self) -> Option<Arc<EngineStats>> {
        HostedEngine::stats(self)
    }
}

/// The DAW a process runs: one project, one device, one transport — the
/// home of every rig in the process.
pub struct Engine {
    daw: Standalone,
    project_guid: String,
    /// The project's own transport (what the daw Transport service drives),
    /// fetched when first needed: making it starts the project's soft clock,
    /// which a headless engine (the browser's, a test's) has no runtime for.
    shared: std::sync::OnceLock<Arc<TransportShared>>,
    /// The device, once [`start_audio`](Self::start_audio) opened it, or
    /// `None` while the engine is someone else's (see [`on`](Self::on)) or
    /// not started.
    #[cfg(not(target_arch = "wasm32"))]
    audio: Mutex<Option<Box<dyn Running>>>,
}

impl Engine {
    /// A fresh engine: its own `Standalone` and a project named `name`.
    /// Rigs can be added before or after [`start_audio`](Self::start_audio).
    #[must_use]
    pub fn new(name: &str) -> Arc<Self> {
        Self::new_on(&Standalone::new(), name)
    }

    /// A fresh project named `name` on an existing `Standalone`, made
    /// current, as an engine — the browser worklet's: its `WebRenderer`
    /// renders the daw it was given, so the rigs go into a project there.
    #[must_use]
    pub fn new_on(daw: &Standalone, name: &str) -> Arc<Self> {
        let project_guid = uuid_string();
        daw.seed_project(ProjectInfo {
            guid: project_guid.clone(),
            name: name.to_string(),
            path: String::new(),
        });
        daw.set_current_project(&project_guid);
        Self::on(daw, &project_guid)
    }

    /// The engine of a `Standalone` and project that already exist — Session's
    /// own, whose audio Session opened: rigs go into that project and play
    /// on that device. [`start_audio`](Self::start_audio) is not needed (and
    /// must not be called while another engine drives the project).
    #[must_use]
    pub fn on(daw: &Standalone, project_guid: &str) -> Arc<Self> {
        Arc::new(Self {
            daw: daw.clone(),
            project_guid: project_guid.to_string(),
            shared: std::sync::OnceLock::new(),
            #[cfg(not(target_arch = "wasm32"))]
            audio: Mutex::new(None),
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    /// Open the device: one duplex engine (input and output in one realtime
    /// callback) rendering the whole project. Live input and FX run while
    /// the transport is stopped, so rigs sound without it rolling.
    /// `prefs.sample_rate == 0` requests 48 kHz.
    ///
    /// # Errors
    ///
    /// Returns an error if the device fails to open.
    pub fn start_audio(&self, prefs: &AudioIoPrefs) -> eyre::Result<()> {
        crate::request_low_latency_quantum(prefs);
        let bundle = self.daw.transport_engine_for(&self.project_guid);
        bundle.disable_soft_clock();
        let shared = self.shared.get_or_init(|| bundle.shared.clone()).clone();
        let tracks = self
            .daw
            .read_project(&self.project_guid, |p| p.tracks.len())
            .unwrap_or(0);
        // A bank for the tracks there now; it grows as rigs add theirs.
        self.daw.set_meters(Meters::new(tracks));
        let engine =
            DuplexEngine::open(self.daw.clone(), self.project_guid.clone(), shared, prefs)?;
        *self.audio.lock_ok() = Some(Box::new(engine));
        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    /// Open the device with `prefs` unless one is already open — what a rig
    /// that does not own the device settings asks for.
    ///
    /// # Errors
    ///
    /// Returns an error if the device fails to open.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn ensure_audio(&self, prefs: &AudioIoPrefs) -> eyre::Result<()> {
        if self.audio.lock_ok().is_some() {
            return Ok(());
        }
        self.start_audio(prefs)
    }

    /// Reopen the device with `prefs` — what the rig that owns the device
    /// settings does when they change. Every rig on the engine drops out
    /// for the moment the device takes to reopen.
    ///
    /// # Errors
    ///
    /// Returns an error if the device fails to open.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn restart_audio(&self, prefs: &AudioIoPrefs) -> eyre::Result<()> {
        self.stop_audio();
        self.start_audio(prefs)
    }

    /// Close the device (the project and its rigs stay).
    pub fn stop_audio(&self) {
        self.audio.lock_ok().take();
    }

    /// The daw service handle (cheap to clone).
    #[must_use]
    pub const fn daw(&self) -> &Standalone {
        &self.daw
    }

    /// The engine's project.
    #[must_use]
    pub fn project_guid(&self) -> &str {
        &self.project_guid
    }

    /// The project's transport.
    #[must_use]
    pub fn transport(&self) -> &Arc<TransportShared> {
        self.shared.get_or_init(|| {
            self.daw
                .transport_engine_for(&self.project_guid)
                .shared
                .clone()
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    /// Roll the transport — for an engine a rig owns, so its renderer runs
    /// every block on engines that render only while rolling (the cpal one).
    /// An engine shared with Session leaves its transport to Session.
    pub fn play(&self) {
        self.transport()
            .set_play_state(daw::standalone::transport_engine::PlayStateRepr::Playing);
    }

    /// The device's sample rate, once [`start_audio`](Self::start_audio)
    /// opened it.
    #[must_use]
    pub fn sample_rate(&self) -> Option<u32> {
        self.audio.lock_ok().as_ref().map(|a| a.sample_rate())
    }

    #[cfg(not(target_arch = "wasm32"))]
    /// Live realtime metrics, from engines that report them.
    #[must_use]
    pub fn stats(&self) -> Option<Arc<EngineStats>> {
        self.audio.lock_ok().as_ref().and_then(|a| a.stats())
    }

    /// Add a rig: a folder track named `name` at the end of the project.
    ///
    /// # Errors
    ///
    /// Returns an error if the folder track cannot be added.
    pub fn add_rig(self: &Arc<Self>, name: &str) -> eyre::Result<Rig> {
        let folder = self
            .daw
            .project(self.project_guid.clone())
            .add_track(name)
            .map(|t| t.guid().to_string())
            .map_err(|e| eyre::eyre!("engine: add rig '{name}' failed: {e}"))?;
        Ok(Rig {
            engine: self.clone(),
            folder,
            tracks: Mutex::new(Vec::new()),
        })
    }

    fn context(&self) -> ProjectContext {
        ProjectContext::Project(self.project_guid.clone())
    }

    fn index_of(&self, guid: &str) -> Option<u32> {
        self.daw
            .get(self.context(), TrackRef::Guid(guid.to_string()))
            .map(|t| t.index)
    }
}

/// A rig in an [`Engine`]: a folder track and the tracks under it, folders
/// within folders as deep as the rig needs.
///
/// Its tracks are inserted inside its folder, wherever other rigs have put
/// theirs since — so a track's position (and its meter cell) can move;
/// address tracks by guid. Dropping a `Rig` leaves its tracks in place;
/// [`remove`](Self::remove) takes them out.
pub struct Rig {
    engine: Arc<Engine>,
    folder: String,
    /// The rig's tracks below its folder, in project order, each with its
    /// parent (the rig's folder or another of its tracks).
    tracks: Mutex<Vec<(String, String)>>,
}

impl Rig {
    /// The engine this rig plays on.
    #[must_use]
    pub const fn engine(&self) -> &Arc<Engine> {
        &self.engine
    }

    /// The daw service handle (cheap to clone) — insert plugin instances,
    /// push live MIDI, drive the services here.
    #[must_use]
    pub fn daw(&self) -> &Standalone {
        &self.engine.daw
    }

    /// The engine's project (every rig of the engine shares it).
    #[must_use]
    pub fn project_guid(&self) -> &str {
        &self.engine.project_guid
    }

    /// The rig's folder track.
    #[must_use]
    pub fn folder_guid(&self) -> &str {
        &self.folder
    }

    /// The rig's tracks below its folder, in project order.
    #[must_use]
    pub fn tracks(&self) -> Vec<String> {
        self.tracks
            .lock_ok()
            .iter()
            .map(|(g, _)| g.clone())
            .collect()
    }

    /// Add a track at the end of the rig's folder; returns its guid.
    ///
    /// # Errors
    ///
    /// Returns an error if the track cannot be added or placed.
    pub fn add_track(&self, name: &str) -> eyre::Result<String> {
        let folder = self.folder.clone();
        self.add_track_in(&folder, name)
    }

    /// Add a track at the end of `parent` — the rig's folder or one of its
    /// tracks, which becomes a folder; returns its guid.
    ///
    /// # Errors
    ///
    /// Returns an error if `parent` is not the rig's, or the track cannot be
    /// added or placed.
    pub fn add_track_in(&self, parent: &str, name: &str) -> eyre::Result<String> {
        let mut tracks = self.tracks.lock_ok();
        if parent != self.folder && !tracks.iter().any(|(g, _)| g == parent) {
            return Err(eyre::eyre!("rig: {parent} is not one of this rig's tracks"));
        }
        // After the parent's last descendant (or the parent itself).
        let mut pos = tracks
            .iter()
            .position(|(g, _)| g == parent)
            .map_or(0, |p| p + 1);
        while pos < tracks.len() && is_below(&tracks, &tracks[pos].0, parent, &self.folder) {
            pos += 1;
        }
        let after = if pos == 0 {
            self.folder.clone()
        } else {
            tracks[pos - 1].0.clone()
        };
        let at = self
            .engine
            .index_of(&after)
            .ok_or_else(|| eyre::eyre!("rig: track {after} is gone"))?
            .saturating_add(1);
        let guid = self
            .engine
            .daw
            .add(self.engine.context(), name, Some(at))
            .map_err(|e| eyre::eyre!("rig: add track '{name}' failed: {e}"))?;
        tracks.insert(pos, (guid.clone(), parent.to_string()));
        self.set_depths(&tracks)?;
        // Held until here on purpose: two adds to one rig must not interleave
        // their folder depths.
        drop(tracks);
        Ok(guid)
    }

    /// Remove one of the rig's tracks, with any tracks below it; the
    /// folder depths of the rest are put right.
    ///
    /// # Errors
    ///
    /// Returns an error if `guid` is not one of the rig's tracks, or a
    /// track cannot be removed.
    pub fn remove_track(&self, guid: &str) -> eyre::Result<()> {
        let mut tracks = self.tracks.lock_ok();
        if !tracks.iter().any(|(g, _)| g == guid) {
            return Err(eyre::eyre!("rig: {guid} is not one of this rig's tracks"));
        }
        let gone: Vec<String> = tracks
            .iter()
            .filter(|(g, _)| g == guid || is_below(&tracks, g, guid, &self.folder))
            .map(|(g, _)| g.clone())
            .collect();
        let ctx = self.engine.context();
        for g in gone.iter().rev() {
            self.engine
                .daw
                .remove(ctx.clone(), TrackRef::Guid(g.clone()))
                .map_err(|e| eyre::eyre!("rig: remove track failed: {e}"))?;
        }
        tracks.retain(|(g, _)| !gone.contains(g));
        self.set_depths(&tracks)?;
        drop(tracks);
        Ok(())
    }

    /// REAPER's folder depths over the rig: a track's depth change is the
    /// next track's nesting level minus its own (a folder opens with +1, the
    /// last track of a folder closes as many levels as end there), and the
    /// rig's last track closes back to the rig folder's level.
    fn set_depths(&self, tracks: &[(String, String)]) -> eyre::Result<()> {
        let mut levels: Vec<(String, i32)> = vec![(self.folder.clone(), 0)];
        for (guid, parent) in tracks {
            let parent_level = levels
                .iter()
                .find(|(g, _)| g == parent)
                .map_or(0, |(_, l)| *l);
            levels.push((guid.clone(), parent_level + 1));
        }
        let ctx = self.engine.context();
        for (k, (guid, level)) in levels.iter().enumerate() {
            let next = levels.get(k + 1).map_or(0, |(_, l)| *l);
            self.engine
                .daw
                .set_folder_depth(ctx.clone(), TrackRef::Guid(guid.clone()), next - level)
                .map_err(|e| eyre::eyre!("rig: folder depth failed: {e}"))?;
        }
        Ok(())
    }

    /// Reserve one FX slot on `track`; returns the slot's guid — constant for
    /// the project's life, so swapping the instance behind it never rebuilds
    /// the renderer's snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error if the slot cannot be added.
    pub fn add_fx_slot(&self, track_guid: &str, label: &str) -> eyre::Result<String> {
        self.engine
            .daw
            .project(self.engine.project_guid.clone())
            .track(track_guid)
            .add_fx_slot(label)
            .map(daw::service::handle::FxSlotHandle::into_guid)
            .map_err(|e| eyre::eyre!("rig: reserve fx slot '{label}' failed: {e}"))
    }

    /// Arm `track` to monitor hardware input `channel`.
    ///
    /// # Errors
    ///
    /// Returns an error if the input cannot be armed.
    pub fn arm_input(&self, track_guid: &str, channel: u32) -> eyre::Result<()> {
        self.engine
            .daw
            .project(self.engine.project_guid.clone())
            .track(track_guid)
            .arm_audio_input(channel)
            .map_err(|e| eyre::eyre!("rig: arm input failed: {e}"))
    }

    /// A rig track's post-fader peak (linear, per channel) from the engine's
    /// meter bank — looked up by guid, since the track's position moves as
    /// other rigs add theirs. `None` before the engine meters, or for a
    /// track that is gone.
    #[must_use]
    pub fn peak(&self, track_guid: &str) -> Option<(f32, f32)> {
        let index = usize::try_from(self.engine.index_of(track_guid)?).ok()?;
        let meters = self.engine.daw.meters();
        let cell = meters.cell(index)?;
        Some((cell.peak(0), cell.peak(1)))
    }

    /// Take the rig out of the engine: its tracks, then its folder.
    ///
    /// # Errors
    ///
    /// Returns an error if a track cannot be removed.
    pub fn remove(&self) -> eyre::Result<()> {
        let ctx = self.engine.context();
        let tracks = self.tracks();
        for guid in tracks.iter().rev().chain(std::iter::once(&self.folder)) {
            self.engine
                .daw
                .remove(ctx.clone(), TrackRef::Guid(guid.clone()))
                .map_err(|e| eyre::eyre!("rig: remove track failed: {e}"))?;
        }
        Ok(())
    }
}

/// Whether `guid` sits (at any depth) below `ancestor` in the rig's tree.
fn is_below(tracks: &[(String, String)], guid: &str, ancestor: &str, root: &str) -> bool {
    let mut at = guid;
    loop {
        let Some((_, parent)) = tracks.iter().find(|(g, _)| g == at) else {
            return false;
        };
        if parent == ancestor {
            return true;
        }
        if parent == root {
            return false;
        }
        at = parent;
    }
}
