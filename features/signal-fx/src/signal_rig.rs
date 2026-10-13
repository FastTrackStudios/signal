//! `FTS Signal Rig`: signal's guitar or bass rig, whole, on a track.
//!
//! The track's FX is the rig's audio: signal's rig service built
//! [`hosted`](signal_guitar::GuitarRigBackend::hosted) — every profile,
//! stack and patch signal has, its crossfades and tails — played a block
//! at a time by the track ([`RigHost::process`]). The service itself is
//! kept here, by the rig's id ([`rig`]), for what drives it: the window's
//! footswitches and browser, and the actions that load a profile or pick a
//! tone. The same rig a CLAP would hold.
//!
//! Its state (JSON) says which rig it is and what it plays: its id, the
//! instrument (`guitar`, `bass`), the profile and the patch. The library it
//! plays from is Session's own copy of signal's, one per instrument
//! (`<config>/fts/<instrument>-rig`; `FTS_SIGNAL_RIG_ROOT` overrides the
//! folder they are in), seeded from signal's shipped defaults — never the
//! player's live rig.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use daw_standalone::plugin::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};
use signal_guitar::instrument::Instrument;
use signal_guitar::proto::rig::Rig as _;
use signal_guitar::{GuitarRigBackend, RigHome, RigHost};

/// A rig playing on a track: its service, and where its audio is played.
pub struct Playing {
    pub id: String,
    pub instrument: Instrument,
    pub service: GuitarRigBackend,
    host: Arc<RigHost>,
    /// The patch the Patch parameter asks for (its index; [`NONE`] when
    /// nothing has), written by the audio thread, switched to by the
    /// rig's switcher ([`Playing::switcher`]).
    wanted: AtomicU32,
    /// How many patches the profile has, for the audio thread to read the
    /// parameter by without asking the service.
    patch_count: AtomicU32,
}

/// No patch asked for.
const NONE: u32 = u32::MAX;

/// The Patch parameter's id.
pub const PATCH: u32 = 0;

/// How often the switcher looks for a patch asked for, ms: a switch is
/// heard within this of its automation point, then crossfaded as any.
const SWITCH_POLL_MS: u64 = 4;

/// Every rig playing, by id.
static RIGS: LazyLock<Mutex<HashMap<String, Arc<Playing>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// The rig `id` playing on a track, if it is.
#[must_use]
pub fn rig(id: &str) -> Option<Arc<Playing>> {
    RIGS.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(id)
        .cloned()
}

/// The folder Session keeps its copies of signal's libraries in.
fn root() -> PathBuf {
    if let Some(dir) = std::env::var_os("FTS_SIGNAL_RIG_ROOT").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("fts")
}

/// Where Session's copy of `instrument`'s library is.
#[must_use]
pub fn library_dir(instrument: Instrument) -> PathBuf {
    root().join(format!("{}-rig", instrument.id()))
}

/// A fresh rig id: unique in the process and unlikely across processes.
fn fresh_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("rig-{nanos:x}-{n}")
}

/// What a rig is and plays, as saved with the song.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RigState {
    /// The rig's id: what its service is found by ([`rig`]).
    #[serde(default)]
    pub id: String,
    /// `guitar` or `bass`.
    #[serde(default)]
    pub instrument: String,
    /// The profile it plays, by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// The patch playing, by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
}

impl RigState {
    fn instrument(&self) -> Instrument {
        Instrument::from_id(&self.instrument)
    }
}

/// Point `service` at `profile` and `patch`, those it has.
fn choose(service: &GuitarRigBackend, profile: Option<&str>, patch: Option<&str>) {
    if let Some(profile) = profile.filter(|p| !p.is_empty()) {
        let applied = service.select_profile(profile.to_owned());
        if !applied.ok {
            tracing::warn!(rig.profile = profile, why = %applied.message, "signal rig: no such profile");
        }
    }
    if let Some(patch) = patch.filter(|p| !p.is_empty()) {
        let index = service
            .patches()
            .iter()
            .position(|p| p.name.eq_ignore_ascii_case(patch))
            .and_then(|i| u32::try_from(i).ok());
        if let Some(index) = index {
            let _ = service.select_patch(index);
        } else {
            tracing::warn!(rig.patch = patch, "signal rig: no such patch");
        }
    }
}

/// `FTS Signal Rig`.
pub struct SignalRig {
    state: RigState,
    playing: Option<Arc<Playing>>,
    sample_rate: f64,
    prepared: bool,
}

impl SignalRig {
    #[must_use]
    pub const fn new(sample_rate: f64) -> Self {
        Self {
            state: RigState {
                id: String::new(),
                instrument: String::new(),
                profile: None,
                patch: None,
            },
            playing: None,
            sample_rate,
            prepared: false,
        }
    }

    /// The rig as the state says: the one playing under its id if it plays
    /// this instrument at this rate, else one built and opened — off the
    /// audio thread, as `prepare` and `load_state` are.
    fn open(&mut self) {
        if self.state.id.is_empty() {
            self.state.id = fresh_id();
        }
        let instrument = self.state.instrument();
        let rate = num_traits::ToPrimitive::to_u32(&self.sample_rate.round()).unwrap_or(48_000);
        let same = self.playing.as_ref().is_some_and(|p| {
            p.id == self.state.id && p.instrument == instrument && p.host_rate() == rate
        });
        if !same {
            let home = RigHome::new(instrument, library_dir(instrument));
            let (service, host) = GuitarRigBackend::hosted(home, rate);
            service.open_blocking();
            let count = u32::try_from(service.patches().len()).unwrap_or(0);
            let playing = Arc::new(Playing {
                id: self.state.id.clone(),
                instrument,
                service,
                host,
                wanted: AtomicU32::new(NONE),
                patch_count: AtomicU32::new(count),
            });
            Playing::switcher(Arc::downgrade(&playing));
            RIGS.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(self.state.id.clone(), Arc::clone(&playing));
            self.playing = Some(playing);
        }
        if let Some(playing) = &self.playing {
            choose(
                &playing.service,
                self.state.profile.as_deref(),
                self.state.patch.as_deref(),
            );
        }
    }
}

impl Playing {
    fn host_rate(&self) -> u32 {
        self.host.sample_rate()
    }

    /// The profile's patches, as the service says them.
    #[must_use]
    pub fn service_patches(&self) -> Vec<signal_guitar::proto::PatchInfo> {
        self.service.patches()
    }

    /// Switch to patch `index` now, as a footswitch does; whether it took.
    #[must_use]
    pub fn select(&self, index: u32) -> bool {
        self.service.select_patch(index).ok
    }

    /// The patches, by name, in order: the Patch parameter's choices.
    #[must_use]
    pub fn patch_names(&self) -> Vec<String> {
        self.service.patches().into_iter().map(|p| p.name).collect()
    }

    /// The patch playing, by index.
    fn active(&self) -> Option<usize> {
        self.service.patches().iter().position(|p| p.active)
    }

    /// Switch to the patch the Patch parameter asks for, off the audio
    /// thread, as long as the rig plays: the service changes what plays
    /// as a footswitch would, crossfaded. Keeps the patch count current.
    fn switcher(rig: std::sync::Weak<Self>) {
        let spawned = std::thread::Builder::new()
            .name("signal-rig-switch".into())
            .spawn(move || {
                let mut last = NONE;
                while let Some(rig) = rig.upgrade() {
                    let wanted = rig.wanted.load(Ordering::Acquire);
                    if wanted != NONE && wanted != last {
                        let _ = rig.service.select_patch(wanted);
                        last = wanted;
                    }
                    let count = u32::try_from(rig.service.patches().len()).unwrap_or(0);
                    rig.patch_count.store(count, Ordering::Relaxed);
                    drop(rig);
                    std::thread::sleep(std::time::Duration::from_millis(SWITCH_POLL_MS));
                }
            });
        if let Err(e) = spawned {
            tracing::warn!(error = %e, "signal rig: no switcher, the Patch parameter will not switch");
        }
    }

    /// The profile and the patch it plays now.
    #[must_use]
    pub fn playing(&self) -> (String, Option<String>) {
        let profile = self.service.perf().profile_name;
        let patch = self
            .service
            .patches()
            .into_iter()
            .find(|p| p.active)
            .map(|p| p.name);
        (profile, patch)
    }
}

impl PluginInstance for SignalRig {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "fts.signal.rig".into(),
            name: crate::SIGNAL_RIG.into(),
            vendor: "FastTrackStudio".into(),
            version: String::new(),
            format: PluginFormat::Synthetic,
        }
    }

    fn params(&mut self) -> Vec<PluginParamInfo> {
        vec![PluginParamInfo {
            id: PATCH,
            name: "Patch".into(),
            min: 0.0,
            max: 1.0,
            default: 0.0,
        }]
    }

    fn param_value(&mut self, id: u32) -> Option<f64> {
        let playing = self.playing.as_ref().filter(|_| id == PATCH)?;
        let count = playing.patch_names().len();
        Some(crate::profile::normalized(playing.active()?, count))
    }

    fn value_to_text(&mut self, id: u32, value: f64) -> Option<String> {
        let playing = self.playing.as_ref().filter(|_| id == PATCH)?;
        let names = playing.patch_names();
        names
            .get(crate::profile::scene_at(value, names.len()))
            .cloned()
    }

    fn text_to_value(&mut self, id: u32, text: &str) -> Option<f64> {
        let playing = self.playing.as_ref().filter(|_| id == PATCH)?;
        let names = playing.patch_names();
        names
            .iter()
            .position(|n| n.eq_ignore_ascii_case(text.trim()))
            .map(|i| crate::profile::normalized(i, names.len()))
    }

    fn param_choices(&mut self, id: u32) -> Vec<(f64, String)> {
        let Some(playing) = self.playing.as_ref().filter(|_| id == PATCH) else {
            return Vec::new();
        };
        let names = playing.patch_names();
        let count = names.len();
        names
            .into_iter()
            .enumerate()
            .map(|(i, name)| (crate::profile::normalized(i, count), name))
            .collect()
    }

    fn latency(&mut self) -> u32 {
        0
    }

    fn prepare(&mut self, sample_rate: f64, _block_size: u32) -> Result<(), PluginError> {
        self.sample_rate = sample_rate;
        self.open();
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
        if let Some(playing) = &self.playing {
            // The Patch parameter (automation, a write): which patch it
            // asks for, for the switcher — nothing here waits on it.
            if let Some(&(_, value)) = events.params.iter().rev().find(|(id, _)| *id == PATCH) {
                let count =
                    usize::try_from(playing.patch_count.load(Ordering::Relaxed)).unwrap_or(0);
                if count > 0 {
                    let index = crate::profile::scene_at(value, count);
                    playing
                        .wanted
                        .store(u32::try_from(index).unwrap_or(NONE), Ordering::Release);
                }
            }
            playing.host.process(in_l, in_r, out_l, out_r);
        } else {
            out_l.iter_mut().zip(in_l).for_each(|(o, i)| *o = *i);
            out_r.iter_mut().zip(in_r).for_each(|(o, i)| *o = *i);
        }
        Ok(())
    }

    fn load_state(&mut self, state: &[u8]) -> Result<(), PluginError> {
        let read: RigState = serde_json::from_slice(state)
            .map_err(|e| PluginError::LoadFailed(format!("signal rig state: {e}")))?;
        // A state with no id keeps the rig's own (an action naming only
        // what to play).
        self.state = RigState {
            id: if read.id.is_empty() {
                std::mem::take(&mut self.state.id)
            } else {
                read.id
            },
            ..read
        };
        // Opened now, prepared or not: its service is what the window and
        // the actions drive, and a song opened silent prepares nothing.
        // `prepare` opens it again only at another rate.
        self.open();
        Ok(())
    }

    fn save_state(&mut self) -> Result<Vec<u8>, PluginError> {
        if let Some(playing) = &self.playing {
            let (profile, patch) = playing.playing();
            self.state.profile = Some(profile).filter(|p| !p.is_empty());
            self.state.patch = patch;
        }
        serde_json::to_vec(&self.state).map_err(|e| PluginError::LoadFailed(e.to_string()))
    }

    fn deactivate(&mut self) {
        self.prepared = false;
    }
}
