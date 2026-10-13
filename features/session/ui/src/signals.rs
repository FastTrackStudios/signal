//! Global Domain Signals and Session Access
//!
//! This module defines the global signals that represent the domain state for the UI,
//! and provides the `Session` singleton for accessing service clients.
//!
//! ## Signal Architecture
//!
//! - `SETLIST_STRUCTURE`: Song/section structure (updates infrequently)
//! - `ACTIVE_INDICES`: Current playback position (song/section indices, progress)
//! - `SONG_TRANSPORT`: Per-song transport state (playhead, tempo, time signature)
//! - `PLAYBACK_STATE`: Global playback state (playing, paused, stopped)
//!
//! Components subscribe only to the signals they need, preventing unnecessary rerenders.
//!
//! ## Session Access
//!
//! Service clients are accessed via `Session::get()`, following the same pattern as
//! `Daw::get()` in daw-control:
//!
//! ```rust,ignore
//! // During app startup (or on reconnection)
//! Session::init(setlist_client);
//!
//! // In UI components
//! let session = Session::get();
//! session.setlist().play().await;
//! ```

use crate::prelude::*;
use daw_proto::{MusicalPosition, PlayState};
use session_proto::{ActiveIndices, Setlist, SetlistServiceClient, SongChartHydration};
use std::collections::HashMap;

#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;

#[cfg(not(target_arch = "wasm32"))]
use std::sync::OnceLock;

// ============================================================================
// Latency Tracking
// ============================================================================

/// Tracks latency for roundtrip measurements (action -> state update received)
#[derive(Debug, Clone, Default)]
pub struct LatencyTracker {
    /// Timestamp when play/pause was triggered (waiting for `PlayState` change)
    pub pending_play_toggle: Option<f64>,
    /// Most recent measured latencies (last N measurements)
    pub recent_latencies: Vec<LatencyMeasurement>,
    /// Maximum number of measurements to keep
    max_history: usize,
}

/// A single latency measurement
#[derive(Debug, Clone)]
pub struct LatencyMeasurement {
    /// Type of action that was measured
    pub action: LatencyAction,
    /// Measured latency in milliseconds
    pub latency_ms: f64,
    /// Timestamp when measurement was recorded
    pub timestamp: f64,
}

/// Types of actions we track latency for
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LatencyAction {
    PlayToggle,
    LoopToggle,
    Seek,
}

impl LatencyTracker {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pending_play_toggle: None,
            recent_latencies: Vec::new(),
            max_history: 20,
        }
    }

    /// Record that a play/pause action was triggered
    pub fn start_play_toggle(&mut self) {
        self.pending_play_toggle = Some(now_ms());
    }

    /// Complete a play/pause measurement when state change is received
    pub fn complete_play_toggle(&mut self) -> Option<f64> {
        self.pending_play_toggle.take().and_then(|start| {
            let latency = now_ms() - start;
            self.record_measurement(LatencyAction::PlayToggle, latency);
            Some(latency)
        })
    }

    /// Record a latency measurement
    fn record_measurement(&mut self, action: LatencyAction, latency_ms: f64) {
        self.recent_latencies.push(LatencyMeasurement {
            action,
            latency_ms,
            timestamp: now_ms(),
        });

        // Keep only the last N measurements
        if self.recent_latencies.len() > self.max_history {
            self.recent_latencies.remove(0);
        }
    }

    /// Get average latency for a specific action type
    #[must_use]
    pub fn average_latency(&self, action: LatencyAction) -> Option<f64> {
        let matching: Vec<_> = self
            .recent_latencies
            .iter()
            .filter(|m| m.action == action)
            .collect();

        if matching.is_empty() {
            None
        } else {
            let sum: f64 = matching.iter().map(|m| m.latency_ms).sum();
            Some(sum / f64::from(u32::try_from(matching.len()).unwrap_or(1)))
        }
    }

    /// Get the most recent latency measurement
    #[must_use]
    pub fn last_latency(&self) -> Option<&LatencyMeasurement> {
        self.recent_latencies.last()
    }
}

/// Get current time in milliseconds
#[cfg(target_arch = "wasm32")]
fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map_or(0.0, |p| p.now())
}

#[cfg(not(target_arch = "wasm32"))]
fn now_ms() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

/// Global latency tracker signal
pub static LATENCY_TRACKER: GlobalSignal<LatencyTracker> = Signal::global(LatencyTracker::new);

/// Audio output latency in seconds (from DAW audio engine)
///
/// This is used to compensate visual display during playback so that
/// progress bars align with what's actually being heard through the speakers.
/// Only applied when transport is playing.
pub static AUDIO_LATENCY_SECONDS: GlobalSignal<f64> = Signal::global(|| 0.0);

/// Complete latency information for display
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LatencyInfo {
    /// Input latency in milliseconds
    pub input_ms: f64,
    /// Output latency in milliseconds
    pub output_ms: f64,
    /// Estimated network round-trip latency in milliseconds
    pub network_rtt_ms: f64,
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Whether audio engine is running
    pub is_running: bool,
}

impl LatencyInfo {
    /// Total latency for visual compensation (output + half network RTT)
    #[must_use]
    pub fn total_compensation_ms(&self) -> f64 {
        self.output_ms + (self.network_rtt_ms / 2.0)
    }
}

/// Complete latency info for UI display
pub static LATENCY_INFO: GlobalSignal<LatencyInfo> = Signal::global(LatencyInfo::default);

/// Global setlist structure (songs, sections, timing)
/// Updates when setlist is rebuilt or structure changes
pub static SETLIST_STRUCTURE: GlobalSignal<Setlist> = Signal::global(Setlist::default);

/// Current playback position and progress
/// Updates frequently during playback (10-60 times per second)
pub static ACTIVE_INDICES: GlobalSignal<ActiveIndices> = Signal::global(ActiveIndices::default);

/// Per-song transport state (playhead position, tempo, time signature)
/// Key is song index, updates when transport state changes for that song
pub static SONG_TRANSPORT: GlobalSignal<HashMap<usize, TransportState>> =
    Signal::global(HashMap::new);

/// Active song musical transport position (measure/beat/subdivision).
///
/// This is a lightweight signal for chart cursor rendering so the renderer
/// doesn't need to subscribe to the full `SONG_TRANSPORT` map.
pub static ACTIVE_PLAYBACK_MUSICAL: GlobalSignal<Option<MusicalPosition>> = Signal::global(|| None);

/// Whether active song transport is currently playing.
///
/// Kept separate from full transport state to minimize render fanout.
pub static ACTIVE_PLAYBACK_IS_PLAYING: GlobalSignal<bool> = Signal::global(|| false);

/// Hydrated chart payload cache keyed by `project_guid`.
///
/// Kept separate from `SETLIST_STRUCTURE` so chart text blobs are not cloned
/// through structural setlist update events.
pub static SONG_CHARTS: GlobalSignal<HashMap<String, SongChartHydration>> =
    Signal::global(HashMap::new);

/// A non-destructive chart display override for one song.
///
/// Transpose to a key, re-spell in Nashville numbers / Roman numerals, and/or
/// finger with a capo. It NEVER changes the keyflow source; the chart is
/// re-engraved through `keyflow::transpose::apply_view`. Keyed by `project_guid`
/// in [`SONG_VIEWS`].
#[derive(Clone, PartialEq, Eq)]
pub struct SongView {
    /// Render sounding in this key (keyflow key string, e.g. `"G"`); `None` =
    /// the song's own key.
    pub target_key: Option<String>,
    /// How chord roots are spelled.
    pub notation: keyflow::NotationSystem,
    /// Capo fret (0 = none); the chart then renders in the fingered shape key.
    pub capo: u8,
}

impl Default for SongView {
    fn default() -> Self {
        Self {
            target_key: None,
            notation: keyflow::NotationSystem::AsWritten,
            capo: 0,
        }
    }
}

impl SongView {
    /// True when this view renders the chart exactly as written.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.target_key.is_none()
            && self.capo == 0
            && self.notation == keyflow::NotationSystem::AsWritten
    }

    /// Build the keyflow transposition view (target key parsed from the string).
    #[must_use]
    pub fn to_chart_view(&self) -> keyflow::ChartView {
        keyflow::ChartView {
            target_key: self
                .target_key
                .as_deref()
                .and_then(|s| keyflow::Key::parse(s).ok()),
            notation: self.notation,
            capo: self.capo,
        }
    }

    /// Discriminant for a layout cache key (so switching the view re-renders).
    #[must_use]
    pub fn cache_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.target_key.hash(&mut h);
        let n = match self.notation {
            keyflow::NotationSystem::Letters => 0u8,
            keyflow::NotationSystem::Nashville => 1,
            keyflow::NotationSystem::Roman => 2,
            keyflow::NotationSystem::AsWritten => 3,
        };
        n.hash(&mut h);
        self.capo.hash(&mut h);
        h.finish()
    }

    /// The effective sounding key label shown to the player: the fingered shape
    /// key when a capo is set (that's what they play), else the target key.
    /// `None` when the view leaves the song in its own key.
    #[must_use]
    pub fn display_key_label(&self) -> Option<String> {
        let target = self.target_key.as_deref()?;
        let key = keyflow::Key::parse(target).ok()?;
        if self.capo > 0 {
            Some(keyflow::transpose::shape_key_for_capo(key, self.capo).to_string())
        } else {
            Some(key.to_string())
        }
    }
}

/// Per-song chart-view overrides (`project_guid` → [`SongView`]). A display
/// layer only — never persisted to the song file.
pub static SONG_VIEWS: GlobalSignal<HashMap<String, SongView>> = Signal::global(HashMap::new);

/// The pass being reviewed, if there is one.
///
/// Set when a take ends and cleared when the next one starts. The
/// review panel draws whatever is here, so what feeds it — a
/// recording that just stopped, a take picked out of the list — is the
/// caller's business and not the panel's.
pub static TAKE_UNDER_REVIEW: GlobalSignal<Option<session_proto::review::Pass>> =
    Signal::global(|| None);

/// The consolidated peaks of the reviewed pass, for the performer this
/// device belongs to. Empty means "not read yet", and the panel draws a
/// line rather than a shape it made up.
pub static TAKE_PEAKS: GlobalSignal<Vec<f32>> = Signal::global(Vec::new);

/// Which ROLE this device is for — "Guitar 1", "Vocalist 2", "Drums".
///
/// A role and not a person: the tablet on the drum riser is the
/// drummer's whoever is sitting there, the tracks are grouped by role
/// already, and a dep playing tonight is still Guitar 1.
///
/// Per device and not per session — six tablets around a room are six
/// answers.
pub static ROLE: GlobalSignal<Option<String>> = Signal::global(|| None);

/// The roles on this session, for the picker.
pub static ROLES: GlobalSignal<Vec<String>> = Signal::global(Vec::new);

/// Global playback state
/// Updates when play/pause/stop state changes
pub static PLAYBACK_STATE: GlobalSignal<PlayState> = Signal::global(|| PlayState::Stopped);

/// Number of tracks currently record-armed in the active project.
///
/// Fed by a live `daw::service::Tracks` subscription (not the setlist
/// service — session's own `SetlistService` has no per-track visibility),
/// so it stays accurate whether tracks were armed from this app, from
/// REAPER's own UI, or from a control surface. `0` before that bridge has
/// connected (Live Mode's daw-standalone backend has no armable tracks;
/// Recording Mode wires this up once its REAPER connection is live).
pub static ARMED_TRACK_COUNT: GlobalSignal<usize> = Signal::global(|| 0);

/// Chart split view mode - when true, the main content area shows chart in top half
/// This is controlled by the desktop app for hybrid WGPU/WebView rendering
pub static SHOW_CHART_SPLIT: GlobalSignal<bool> = Signal::global(|| false);

/// Chart area bounds in window coordinates (x, y, width, height)
/// Updated when the chart area is resized or the split view changes
/// Used by WGPU renderer to know where to draw the chart
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChartAreaBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Device pixel ratio (e.g. 2.0 on Retina displays).
    pub dpr: f64,
}

impl Default for ChartAreaBounds {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            dpr: 1.0,
        }
    }
}

impl ChartAreaBounds {
    #[must_use]
    pub const fn new(x: f64, y: f64, width: f64, height: f64, dpr: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
            dpr,
        }
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.width > 0.0 && self.height > 0.0
    }
}

pub static CHART_AREA_BOUNDS: GlobalSignal<ChartAreaBounds> =
    Signal::global(ChartAreaBounds::default);

// ============================================================================
// Performance Chart Viewport
// ============================================================================

/// Viewport state for the performance chart view (independent of chart editor).
///
/// When `auto_follow` is true, the render effect computes scroll automatically
/// to track the cursor. When false, the user has manually panned/zoomed and
/// the stored `scroll_x/scroll_y/zoom` are used directly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PerfChartViewport {
    pub scroll_x: f64,
    pub scroll_y: f64,
    /// Zoom multiplier on top of fit-to-width base scale. 1.0 = fit page width.
    pub zoom: f64,
    /// When true, render effect auto-scrolls to follow the cursor.
    pub auto_follow: bool,
}

impl Default for PerfChartViewport {
    fn default() -> Self {
        Self {
            scroll_x: 0.0,
            scroll_y: 0.0,
            zoom: 1.0,
            auto_follow: true,
        }
    }
}

pub static PERF_CHART_VIEWPORT: GlobalSignal<PerfChartViewport> =
    Signal::global(PerfChartViewport::default);

/// Hover point in scene coordinates for the performance chart view.
/// Separate from `CHART_HOVER_SCENE_POINT` (which is for the chart editor).
pub static PERF_CHART_HOVER: GlobalSignal<Option<(f64, f64)>> = Signal::global(|| None);

/// Click point in scene coordinates for the performance chart view.
/// Written by the UI click handler, consumed by the desktop render loop.
pub static PERF_CHART_CLICK: GlobalSignal<Option<(f64, f64)>> = Signal::global(|| None);

/// Base scale for the performance chart (fit-to-width scale).
/// Written by the render effect, read by mouse event handlers for coordinate conversion.
pub static PERF_CHART_BASE_SCALE: GlobalSignal<f64> = Signal::global(|| 1.0);

// ============================================================================
// Session singleton
// ============================================================================

/// Thread-local storage for the Session (allows re-initialization on reconnect)
/// We use thread_local + RefCell because:
/// 1. WASM is single-threaded, so thread_local is effectively global
/// 2. RefCell allows interior mutability for replacing the client on reconnect
#[cfg(target_arch = "wasm32")]
thread_local! {
    static GLOBAL_SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
}

/// Native: the same replaceable slot wasm has, for the same reason.
///
/// This was a `OnceLock` on the assumption that native never reconnects.
/// Recording Mode makes that false: it dials a REAPER extension's socket, and
/// REAPER can be quit and reopened under a new pid while the app stays up. A
/// `OnceLock` meant a re-dial had nowhere to go — the UI would hold a client
/// pointing at a dead vox link forever.
///
/// The `Session` behind the pointer is leaked rather than dropped, so
/// [`Session::get`] can keep handing out `&'static Self` and its ~30 call
/// sites stay as they are. What leaks is one small struct holding a client
/// handle, once per REAPER restart within a single app run — bounded by how
/// often someone bounces REAPER, and the alternative (an owned return) makes
/// every `Session::get().setlist()` a borrow of a temporary.
#[cfg(not(target_arch = "wasm32"))]
static GLOBAL_SESSION: std::sync::RwLock<Option<&'static Session>> = std::sync::RwLock::new(None);

/// Session provides access to session service clients
///
/// Similar to `Daw::get()` in daw-control, this provides a global singleton
/// for accessing service clients from UI components.
///
/// # Reconnection Support
///
/// On WASM targets, `init()` can be called multiple times to update the client
/// after a reconnection. On native targets, it can only be called once.
///
/// # Example
///
/// ```rust,ignore
/// use session_ui::Session;
///
/// // During app startup (or on reconnection)
/// Session::init(setlist_client);
///
/// // In UI components - call service methods directly
/// Session::with(|session| {
///     session.setlist().play().await;
/// });
/// ```
#[derive(Clone)]
pub struct Session {
    setlist_client: SetlistServiceClient,
}

impl Session {
    /// Initialize or reinitialize the global Session with service clients.
    ///
    /// On WASM: Can be called multiple times (replaces the existing client).
    /// On native: Can only be called once.
    ///
    /// # Errors
    ///
    /// Returns an error if already initialized (native only).
    #[cfg(target_arch = "wasm32")]
    pub fn init(setlist_client: SetlistServiceClient) -> eyre::Result<()> {
        GLOBAL_SESSION.with(|cell| {
            *cell.borrow_mut() = Some(Self { setlist_client });
        });
        Ok(())
    }

    /// Point the global Session at `setlist_client`, replacing any previous
    /// one — a reconnect is exactly this call a second time.
    ///
    /// # Errors
    ///
    /// Returns an error only if the lock is poisoned.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn init(setlist_client: SetlistServiceClient) -> eyre::Result<()> {
        let session: &'static Self = Box::leak(Box::new(Self { setlist_client }));
        *GLOBAL_SESSION
            .write()
            .map_err(|_| eyre::eyre!("the global Session lock is poisoned"))? = Some(session);
        Ok(())
    }

    /// Forget the current Session, so `try_get` reports "not connected"
    /// again. Used when the backend behind it goes away — a REAPER that
    /// quit — so the UI stops rendering a player wired to a dead link.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn clear() {
        if let Ok(mut slot) = GLOBAL_SESSION.write() {
            *slot = None;
        }
    }

    /// Get the global Session instance.
    ///
    /// # Panics
    ///
    /// Panics if `init()` has not been called. There is no meaningful
    /// fallback `SetlistServiceClient` to construct (it needs a live
    /// `Caller`), and every UI call site (`Session::get().setlist()...`)
    /// assumes this always succeeds, so returning `Option` here would just
    /// push the same "must call `init()` first" panic into every caller.
    #[cfg(target_arch = "wasm32")]
    #[must_use]
    #[allow(clippy::expect_used)]
    pub fn get() -> Self {
        GLOBAL_SESSION.with(|cell| {
            cell.borrow()
                .clone()
                .expect("Session not initialized. Call Session::init() first.")
        })
    }

    /// # Panics
    ///
    /// Panics if `init()` has not been called. See the wasm `get()` above
    /// for why there is no non-panicking fallback here.
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(clippy::expect_used)]
    #[must_use]
    pub fn get() -> &'static Self {
        Self::try_get().expect("Session not initialized. Call Session::init() first.")
    }

    /// Get the global Session instance if initialized, or `None`.
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn try_get() -> Option<&'static Self> {
        *GLOBAL_SESSION.read().ok()?
    }

    /// Get the `SetlistService` client
    pub const fn setlist(&self) -> &SetlistServiceClient {
        &self.setlist_client
    }
}

// ============================================================================
// Transport State
// ============================================================================

/// Simplified transport state for UI display
#[derive(Debug, Clone, PartialEq)]
pub struct TransportState {
    /// Current position with time, musical, and MIDI representations
    pub position: daw_proto::Position,
    /// Current tempo in BPM
    pub bpm: f64,
    /// Time signature numerator
    pub time_sig_num: i32,
    /// Time signature denominator
    pub time_sig_denom: i32,
    /// Whether this song's project is currently playing
    pub is_playing: bool,
    /// Whether this song is currently looping
    pub is_looping: bool,
    /// Loop region start/end (if looping), as percentages (0.0-1.0)
    pub loop_region: Option<(f64, f64)>,
}

impl Default for TransportState {
    fn default() -> Self {
        Self {
            position: daw_proto::Position::default(),
            bpm: 120.0,
            time_sig_num: 4,
            time_sig_denom: 4,
            is_playing: false,
            is_looping: false,
            loop_region: None,
        }
    }
}

impl TransportState {
    /// Create a new transport state
    #[must_use]
    pub const fn new(
        position: daw_proto::Position,
        bpm: f64,
        time_sig_num: i32,
        time_sig_denom: i32,
    ) -> Self {
        Self {
            position,
            bpm,
            time_sig_num,
            time_sig_denom,
            is_playing: false,
            is_looping: false,
            loop_region: None,
        }
    }

    /// Set loop region (as time values in seconds)
    #[must_use]
    pub fn with_loop_region(mut self, start: f64, end: f64, song_duration: f64) -> Self {
        self.is_looping = true;
        self.loop_region = Some((start / song_duration, end / song_duration));
        self
    }
}
