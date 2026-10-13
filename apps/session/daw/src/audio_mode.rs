//! Where this window's sound comes from — the audio mode — as a STATE, not
//! only a launch choice (issue #142).
//!
//! Three modes, ordered by how much of the audio is made here:
//!
//! - **Remote** — nothing. This window drives another system (REAPER
//!   today, another Session engine soon) through the `daw` facade, and that
//!   system owns the project and plays it.
//! - **Cue** — Remote, plus a small local engine that plays the click and
//!   the guide, sample-locked to the remote (`crate::cue`; it reports
//!   through [`set_cue_ready`]).
//! - **Engine** — everything: the in-process daw-standalone engine owns the
//!   project and plays it through this machine's device.
//!
//! What was ASKED for (`requested`) and what is POSSIBLE right now
//! (`effective`) are kept apart, because an Engine whose media is still
//! arriving — streamed from Task or from a peer — cannot play yet but can
//! already show everything that is data: the session doc, the arrangement,
//! the peaks, the chart, the transport following the session. So it starts
//! as Remote and climbs:
//!
//! ```text
//!   Remote ──(click/guide assets ready)──▶ Cue ──(every proxy loaded)──▶ Engine
//! ```
//!
//! The loader drives the climb with [`begin_asset_load`],
//! [`set_cue_ready`] and [`set_assets_progress`]; the effective mode is
//! never more than the requested one ([`ModeState::effective`]). An Engine
//! opened the ordinary way loads its media before the window and never
//! calls any of them, so it is Engine from the first frame — as before.
//!
//! Which BACKEND the facade points at follows the request, not the
//! effective mode: a streamed Engine is the local engine filling up, not a
//! remote one ([`ModeState::owns_project`]). That is what every call site
//! that used to reach the local engine directly now asks
//! (`crate::open::with_local_engine`).
//!
//! Platform-neutral (the web page's top bar reads it too); the native
//! launch that SETS it lives in `crate::open`.

use std::path::PathBuf;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

/// How much of the audio this window makes. Ordered: `Remote < Cue < Engine`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AudioMode {
    /// Drives another system; plays nothing here.
    Remote,
    /// Remote, plus a local click and guide.
    Cue,
    /// The in-process engine owns the project and plays it.
    Engine,
}

impl AudioMode {
    pub const ALL: [Self; 3] = [Self::Engine, Self::Cue, Self::Remote];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Engine => "Engine",
            Self::Cue => "Cue",
            Self::Remote => "Remote",
        }
    }

    /// What the mode does, for the picker.
    #[must_use]
    pub const fn blurb(self) -> &'static str {
        match self {
            Self::Engine => "This machine plays the session",
            Self::Cue => "Drive another system; click and guide here",
            Self::Remote => "Drive another system; no audio here",
        }
    }

    /// `engine` / `remote` / `cue`, any case — the `--audio` flag and
    /// `FTS_AUDIO_MODE`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "engine" | "local" => Some(Self::Engine),
            "remote" => Some(Self::Remote),
            "cue" => Some(Self::Cue),
            _ => None,
        }
    }

    /// The indicator's colour.
    #[must_use]
    pub const fn color(self) -> &'static str {
        match self {
            Self::Engine => "#4ac26b",
            Self::Cue => "#e3b341",
            Self::Remote => "#3aa0ff",
        }
    }
}

/// The system a Remote or Cue window drives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteTarget {
    /// A REAPER running the FTS extension, over its Unix socket. `None`
    /// discovers one (in `/tmp`); `Some` names it, for when several are up.
    Reaper { socket: Option<PathBuf> },
    /// Another Session engine — `fasttrackstudio --engine`, or another
    /// Session app — by its address (a vox WebSocket URL, or an iroh
    /// ticket). Named so the mode can carry it; attaching to one is not
    /// wired yet, and asking for it says so rather than falling back.
    Session { address: String },
}

impl RemoteTarget {
    /// Short, for the top bar: `REAPER`, `Session`.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Reaper { .. } => "REAPER",
            Self::Session { .. } => "Session",
        }
    }

    /// Long, for the menu: which one, and how it is reached.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Reaper { socket: None } => "REAPER (discovered socket)".to_owned(),
            Self::Reaper { socket: Some(path) } => format!("REAPER at {}", path.display()),
            Self::Session { address } => format!("Session engine at {address}"),
        }
    }
}

/// How far a streamed engine's media has got.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Assets {
    pub loaded: usize,
    pub total: usize,
}

impl Assets {
    /// Everything there is to load has loaded — true of nothing to load.
    #[must_use]
    pub const fn complete(self) -> bool {
        self.loaded >= self.total
    }
}

/// The mode, as asked for and as possible.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModeState {
    /// What the user (or the launch) asked for.
    pub requested: AudioMode,
    /// The system driven, in Remote and Cue. `None` in Engine.
    pub target: Option<RemoteTarget>,
    /// The small local assets — click and guide — are ready to play.
    pub cue_ready: bool,
    /// A streamed engine's media. Complete (0 of 0) unless a loader said
    /// otherwise.
    pub assets: Assets,
    /// Which of a song's tracks the local engine loads and plays (the
    /// template groups; Cue is `guide`).
    pub selection: session::load_selection::LoadSelection,
}

impl Default for ModeState {
    /// Engine, loaded: the window as it has always opened.
    fn default() -> Self {
        Self::engine()
    }
}

impl ModeState {
    /// The in-process engine, its media all loaded.
    #[must_use]
    pub const fn engine() -> Self {
        Self {
            requested: AudioMode::Engine,
            target: None,
            cue_ready: true,
            assets: Assets {
                loaded: 0,
                total: 0,
            },
            selection: session::load_selection::LoadSelection::All,
        }
    }

    /// Engine, streamed in from `target`: the data comes from it at once
    /// (Remote), the click and guide play here as soon as they can (Cue),
    /// and the selected media streams in until this engine plays the song
    /// (Engine) — its transport locked to `target`'s throughout.
    #[must_use]
    pub const fn streamed(
        target: RemoteTarget,
        selection: session::load_selection::LoadSelection,
    ) -> Self {
        Self {
            requested: AudioMode::Engine,
            target: Some(target),
            cue_ready: false,
            assets: Assets {
                loaded: 0,
                total: 0,
            },
            selection,
        }
    }

    /// Driving `target`; `cue` adds the local click and guide.
    #[must_use]
    pub fn remote(target: RemoteTarget, cue: bool) -> Self {
        Self {
            requested: if cue {
                AudioMode::Cue
            } else {
                AudioMode::Remote
            },
            target: Some(target),
            // Nothing local is loaded until the cue engine says so.
            cue_ready: false,
            assets: Assets {
                loaded: 0,
                total: 0,
            },
            selection: if cue {
                session::load_selection::LoadSelection::cue()
            } else {
                session::load_selection::LoadSelection::All
            },
        }
    }

    /// Whether the local engine owns the project (and the facade points
    /// at it). Follows the REQUEST: a streamed Engine still loading is the
    /// local engine filling up, not a remote one.
    #[must_use]
    pub fn owns_project(&self) -> bool {
        self.requested == AudioMode::Engine && self.target.is_none()
    }

    /// Whether this window streams a song in from `target` into its own
    /// engine — Cue, or a streamed Engine — rather than only driving it.
    #[must_use]
    pub const fn streams_in(&self) -> bool {
        self.target.is_some() && !matches!(self.requested, AudioMode::Remote)
    }

    /// What is possible now — never more than was asked for.
    ///
    /// Engine needs every asset; short of that, the cue assets give Cue;
    /// short of those, it is Remote: the data, silent.
    #[must_use]
    pub fn effective(&self) -> AudioMode {
        // A fully loaded engine plays its own click: for it, cue readiness
        // is only a milestone on the way up.
        // (Only a local Engine: a streamed one is Remote until its own
        // engine is up, however little it has to load.)
        let local_engine = self.requested == AudioMode::Engine && self.target.is_none();
        let possible = if self.assets.complete() && (self.cue_ready || local_engine) {
            AudioMode::Engine
        } else if self.cue_ready {
            AudioMode::Cue
        } else {
            AudioMode::Remote
        };
        possible.min(self.requested)
    }

    /// Still climbing toward what was asked for.
    #[must_use]
    pub fn loading(&self) -> bool {
        self.effective() < self.requested
    }

    /// The indicator's words, as much as `room` allows: 0 = none, 1 = the
    /// mode, 2 = with the target or the progress.
    #[must_use]
    pub fn label(&self, room: u8) -> String {
        let effective = self.effective();
        if room == 0 {
            return String::new();
        }
        let name = effective.name();
        if room == 1 {
            return name.to_owned();
        }
        if self.requested == AudioMode::Engine && !self.assets.complete() {
            return format!(
                "{} · loading {}/{}",
                AudioMode::Engine.name(),
                self.assets.loaded,
                self.assets.total
            );
        }
        match (&self.target, self.loading()) {
            // Asked for Cue, and the cue engine is not there yet.
            (Some(target), true) => format!("{name} · {} · cue pending", target.kind()),
            (Some(target), false) => format!("{name} · {}", target.kind()),
            (None, _) => name.to_owned(),
        }
    }
}

/// Read the requested mode from the command line and the environment.
///
/// In order, the first that answers:
///
/// 1. `--audio <engine|remote|cue>`, then `FTS_AUDIO_MODE`.
/// 2. `--reaper [socket]`, then `SESSION_DAW_REAPER=1` (with `FTS_SOCKET`)
///    — Remote on REAPER (Cue when step 1 said Cue).
/// 3. A project or setlist named at launch (`project_named`) — Engine:
///    a file to open is a file to play.
/// 4. `remembered` — what the mode picker last chose.
/// 5. Engine.
///
/// Remote or Cue with no target given drives REAPER, at `FTS_SOCKET` or
/// discovered. `FTS_AUDIO_TARGET=<address>` names a Session engine instead.
/// Pure, so it is tested without a process to launch.
#[must_use]
pub fn from_launch(
    args: &[String],
    env: &dyn Fn(&str) -> Option<String>,
    project_named: bool,
    remembered: Option<AudioMode>,
) -> ModeState {
    let flag = args
        .iter()
        .position(|a| a == "--audio")
        .and_then(|at| args.get(at + 1))
        .and_then(|v| AudioMode::parse(v));
    let asked = flag.or_else(|| env(MODE_ENV).and_then(|v| AudioMode::parse(&v)));

    let reaper_flag = args.iter().position(|a| a == "--reaper").map(|at| {
        args.get(at + 1)
            .filter(|a| !a.starts_with("--"))
            .map(PathBuf::from)
    });
    let reaper_env = env(REAPER_ENV)
        .filter(|v| v != "0")
        .map(|_| env(SOCKET_ENV).map(PathBuf::from));
    let reaper = reaper_flag.or(reaper_env);

    let target = || {
        if let Some(socket) = reaper.clone() {
            return RemoteTarget::Reaper { socket };
        }
        match env(TARGET_ENV).filter(|v| !v.trim().is_empty()) {
            Some(address) => RemoteTarget::Session { address },
            None => RemoteTarget::Reaper {
                socket: env(SOCKET_ENV).map(PathBuf::from),
            },
        }
    };
    let chosen = match asked {
        Some(mode) => Some(mode),
        None if reaper.is_some() => Some(AudioMode::Remote),
        None if project_named => Some(AudioMode::Engine),
        None => remembered,
    };
    let selection = env(LOAD_ENV).map_or(session::load_selection::LoadSelection::All, |v| {
        session::load_selection::LoadSelection::parse(&v)
    });
    // Engine with somewhere to stream from is a streamed Engine: a named
    // target (a Session engine) or REAPER asked for.
    let streamed_from =
        env(TARGET_ENV).filter(|v| !v.trim().is_empty()).is_some() || reaper.is_some();
    match chosen.unwrap_or(AudioMode::Engine) {
        AudioMode::Engine if streamed_from => ModeState::streamed(target(), selection),
        AudioMode::Engine => ModeState::engine(),
        AudioMode::Remote => ModeState::remote(target(), false),
        AudioMode::Cue => ModeState::remote(target(), true),
    }
}

/// `engine` / `remote` / `cue`.
pub const MODE_ENV: &str = "FTS_AUDIO_MODE";
/// Attach to a live REAPER rather than opening a file.
pub const REAPER_ENV: &str = "SESSION_DAW_REAPER";
/// Which REAPER, when more than one is running.
pub const SOCKET_ENV: &str = "FTS_SOCKET";
/// A Session engine's address, to drive it rather than REAPER.
pub const TARGET_ENV: &str = "FTS_AUDIO_TARGET";
/// Which template groups the local engine loads (`all`, `guide, keys`).
pub const LOAD_ENV: &str = "FTS_LOAD";

// ── the process's mode ───────────────────────────────────────────────

static STATE: RwLock<Option<ModeState>> = RwLock::new(None);
/// Bumped on every change, so a view can poll cheaply for one.
static REVISION: AtomicU64 = AtomicU64::new(0);

/// The process's mode now. Engine until a launch says otherwise.
#[must_use]
pub fn state() -> ModeState {
    STATE
        .read()
        .ok()
        .and_then(|slot| slot.clone())
        .unwrap_or_default()
}

/// A number that changes whenever the mode does.
#[must_use]
pub fn revision() -> u64 {
    REVISION.load(Ordering::Relaxed)
}

/// Whether the local engine owns the project — [`ModeState::owns_project`]
/// without cloning the state, for the call sites that ask every frame.
#[must_use]
pub fn owns_project() -> bool {
    STATE
        .read()
        .ok()
        .is_none_or(|slot| slot.as_ref().is_none_or(ModeState::owns_project))
}

/// Replace the mode — at launch, or when the picker changes it.
pub fn set(state: ModeState) {
    update(|slot| *slot = state);
}

fn update(f: impl FnOnce(&mut ModeState)) {
    if let Ok(mut slot) = STATE.write() {
        let mut next = slot.clone().unwrap_or_default();
        let before = next.clone();
        f(&mut next);
        if before != next {
            tracing::info!(
                audio.requested = next.requested.name(),
                audio.effective = next.effective().name(),
                audio.target = next.target.as_ref().map(RemoteTarget::kind),
                audio.loaded = next.assets.loaded,
                audio.total = next.assets.total,
                "audio mode changed"
            );
            *slot = Some(next);
            REVISION.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Ask for `to` over the same backend — Remote ⇄ Cue. Changing whether
/// the local engine owns the project is a relaunch, not this
/// (`crate::open::request_mode`).
pub fn update_requested(to: AudioMode) {
    update(|s| {
        if s.owns_project() == (to == AudioMode::Engine) {
            s.requested = to;
        }
    });
}

/// A streamed engine starts loading `total` assets: until they are in, it
/// is Remote (or Cue, once [`set_cue_ready`]) — its data shows, its items
/// are silent.
pub fn begin_asset_load(total: usize) {
    update(|s| {
        s.assets = Assets { loaded: 0, total };
        s.cue_ready = false;
    });
}

/// `loaded` of `total` assets are in. At `loaded >= total` the engine is
/// Engine. Items become audible as their own media lands — that is the
/// engine's business (an item with no media is silent); this is only the
/// mode the window reports.
pub fn set_assets_progress(loaded: usize, total: usize) {
    update(|s| s.assets = Assets { loaded, total });
}

/// The click and guide can play here: Cue, from Remote.
///
/// The hook the cue engine calls once its assets are loaded — for a Cue
/// window, the click/guide engine that follows the remote transport; for
/// a streamed Engine, the small assets that arrive first. Nothing calls it
/// for a Cue window yet: until the cue engine exists (a later issue), Cue
/// behaves as Remote and says `cue pending`.
pub fn set_cue_ready(ready: bool) {
    update(|s| s.cue_ready = ready);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect();
        move |key| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    }

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn a_streamed_engine_is_remote_until_its_own_engine_is_up() {
        let target = RemoteTarget::Session {
            address: "fts-engine:x".into(),
        };
        let mut state = ModeState::streamed(target, session::load_selection::LoadSelection::All);
        assert_eq!(
            state.effective(),
            AudioMode::Remote,
            "nothing loaded yet is not Engine"
        );
        state.cue_ready = true;
        state.assets = Assets {
            loaded: 0,
            total: 21,
        };
        assert_eq!(state.effective(), AudioMode::Cue);
        state.assets = Assets {
            loaded: 21,
            total: 21,
        };
        assert_eq!(state.effective(), AudioMode::Engine);
        assert!(!state.owns_project() && state.streams_in());
    }

    #[test]
    fn nothing_said_is_engine_as_before() {
        let state = from_launch(&[], &env_of(&[]), false, None);
        assert_eq!(state, ModeState::engine());
        assert!(state.owns_project());
        assert_eq!(state.effective(), AudioMode::Engine);
        assert!(!state.loading());
    }

    #[test]
    fn a_named_project_is_engine_even_when_remote_was_remembered() {
        let state = from_launch(&[], &env_of(&[]), true, Some(AudioMode::Remote));
        assert_eq!(state.requested, AudioMode::Engine);
        let state = from_launch(&[], &env_of(&[]), false, Some(AudioMode::Remote));
        assert_eq!(
            state.requested,
            AudioMode::Remote,
            "the picker's choice, with nothing named"
        );
    }

    #[test]
    fn reaper_flag_and_env_are_remote_on_reaper() {
        let state = from_launch(
            &args(&["--reaper", "/tmp/x.sock"]),
            &env_of(&[]),
            true,
            None,
        );
        assert_eq!(state.requested, AudioMode::Remote);
        assert_eq!(
            state.target,
            Some(RemoteTarget::Reaper {
                socket: Some("/tmp/x.sock".into())
            })
        );
        assert!(!state.owns_project());

        let state = from_launch(&args(&["--reaper", "--other"]), &env_of(&[]), false, None);
        assert_eq!(
            state.target,
            Some(RemoteTarget::Reaper { socket: None }),
            "a flag is not a socket"
        );

        let state = from_launch(
            &[],
            &env_of(&[(REAPER_ENV, "1"), (SOCKET_ENV, "/tmp/r.sock")]),
            false,
            None,
        );
        assert_eq!(
            state.target,
            Some(RemoteTarget::Reaper {
                socket: Some("/tmp/r.sock".into())
            })
        );
        let state = from_launch(&[], &env_of(&[(REAPER_ENV, "0")]), false, None);
        assert_eq!(state.requested, AudioMode::Engine, "0 is off");
    }

    #[test]
    fn the_audio_flag_wins_and_cue_rides_the_reaper_target() {
        let state = from_launch(
            &args(&["--audio", "cue", "--reaper"]),
            &env_of(&[(MODE_ENV, "engine")]),
            true,
            None,
        );
        assert_eq!(state.requested, AudioMode::Cue);
        assert_eq!(state.target, Some(RemoteTarget::Reaper { socket: None }));
        let state = from_launch(
            &[],
            &env_of(&[(MODE_ENV, "Remote"), (TARGET_ENV, "ws://studio:4040/vox")]),
            false,
            None,
        );
        assert_eq!(
            state.target,
            Some(RemoteTarget::Session {
                address: "ws://studio:4040/vox".into()
            })
        );
        // Engine with somewhere to stream from is a streamed Engine: this
        // engine plays, what it plays comes from REAPER's song.
        let state = from_launch(
            &args(&["--audio", "engine", "--reaper"]),
            &env_of(&[]),
            false,
            None,
        );
        assert_eq!(state.requested, AudioMode::Engine);
        assert!(
            !state.owns_project() && state.streams_in(),
            "Engine, streamed in from REAPER"
        );
        // Engine alone owns its project, as it always has.
        let state = from_launch(&args(&["--audio", "engine"]), &env_of(&[]), false, None);
        assert!(state.owns_project(), "asking for Engine outright is Engine");
    }

    #[test]
    fn cue_stays_remote_until_its_engine_is_ready() {
        let mut state = ModeState::remote(RemoteTarget::Reaper { socket: None }, true);
        assert_eq!(state.effective(), AudioMode::Remote);
        assert!(state.loading());
        assert_eq!(state.label(2), "Remote · REAPER · cue pending");
        state.cue_ready = true;
        assert_eq!(state.effective(), AudioMode::Cue);
        assert_eq!(state.label(2), "Cue · REAPER");
    }

    #[test]
    fn remote_never_climbs_past_what_was_asked() {
        let mut state = ModeState::remote(RemoteTarget::Reaper { socket: None }, false);
        state.cue_ready = true;
        assert_eq!(state.effective(), AudioMode::Remote);
        assert!(!state.loading());
        assert_eq!(state.label(1), "Remote");
    }

    #[test]
    fn a_streamed_engine_climbs_remote_cue_engine() {
        let mut state = ModeState::engine();
        state.assets = Assets {
            loaded: 0,
            total: 38,
        };
        state.cue_ready = false;
        assert_eq!(state.effective(), AudioMode::Remote, "data first, silent");
        assert!(state.owns_project(), "still the local engine filling up");
        state.cue_ready = true;
        state.assets.loaded = 12;
        assert_eq!(state.effective(), AudioMode::Cue);
        assert_eq!(state.label(2), "Engine · loading 12/38");
        state.assets.loaded = 38;
        assert_eq!(state.effective(), AudioMode::Engine);
        assert!(!state.loading());
        assert_eq!(state.label(2), "Engine");
    }

    #[test]
    fn a_loaded_engine_is_engine_without_a_cue_milestone() {
        let mut state = ModeState::engine();
        state.cue_ready = false;
        assert_eq!(state.effective(), AudioMode::Engine);
    }
}
