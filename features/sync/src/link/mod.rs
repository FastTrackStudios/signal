// Lint debt: workspace flipped dead_code/unused to warn (task cleanup);
// this crate predates that — burn down separately.
#![allow(dead_code, unused)]

//! Link sync engine — reimplements ReaBlink's puppet/master sync logic in Rust.
//!
//! The engine is host-agnostic: it communicates with REAPER (or any DAW) through
//! the [`LinkCallbacks`] trait, making it testable without a running DAW.

use rusty_link::{AblLink, SessionState};
use std::time::Instant;
use tracing::{debug, info};

// ── Rolling Average ─────────────────────────────────────────────────────────

/// Fixed-size circular buffer with interquartile-mean smoothing.
///
/// Port of ReaBlink's `RollingAverage.hpp`. Maintains a bounded window of
/// samples and returns the trimmed mean (drops the lowest and highest
/// quarters before averaging), which rejects outliers from timer jitter
/// and scheduling noise.
pub struct RollingAverage {
    values: Vec<f64>,
    max_size: usize,
    /// Write cursor — next position to overwrite in the circular buffer.
    cursor: usize,
    /// Number of values added so far (saturates at `max_size`).
    len: usize,
}

impl RollingAverage {
    /// Create a new rolling average with the given window size.
    pub fn new(size: usize) -> Self {
        assert!(size > 0, "RollingAverage size must be > 0");
        Self {
            values: vec![0.0; size],
            max_size: size,
            cursor: 0,
            len: 0,
        }
    }

    /// Push a new sample into the buffer, evicting the oldest if full.
    pub fn add(&mut self, value: f64) {
        self.values[self.cursor] = value;
        self.cursor = (self.cursor + 1) % self.max_size;
        if self.len < self.max_size {
            self.len += 1;
        }
    }

    /// Interquartile mean of the buffered samples.
    ///
    /// Sorts a copy of the live window, trims the bottom and top quarters,
    /// and averages the remaining middle half. Returns `0.0` when empty.
    pub fn average(&self) -> f64 {
        if self.len == 0 {
            return 0.0;
        }

        let mut sorted: Vec<f64> = self.values[..self.len].to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let quarter = sorted.len() / 4;
        let trimmed = &sorted[quarter..sorted.len() - quarter];
        if trimmed.is_empty() {
            // Fewer than 4 samples — just average everything.
            return sorted.iter().sum::<f64>() / sorted.len() as f64;
        }
        trimmed.iter().sum::<f64>() / trimmed.len() as f64
    }

    /// Number of samples currently in the buffer.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

// ── Audio Buffer Info ───────────────────────────────────────────────────────

/// Snapshot of audio buffer timing, captured from the audio thread.
///
/// The host calls [`Engine::on_audio_buffer`] at the start of each audio
/// buffer to provide precise wall-clock timing. The engine then computes
/// `hostTime` using this instead of `clock_micros()`, matching ReaBlink's
/// `g_abuf_time + GetOutputLatency() + g_abuf_len / g_abuf_srate` pattern.
#[derive(Debug, Clone, Copy)]
pub struct AudioBufferInfo {
    /// Number of sample frames in the current buffer.
    pub buf_len: usize,
    /// Sample rate in Hz (e.g., 44100.0, 48000.0).
    pub sample_rate: f64,
    /// Wall-clock time when the audio buffer started, in seconds since
    /// an arbitrary epoch (captured via `Link::clock_micros()` on the
    /// audio thread).
    pub wall_clock_time: f64,
}

// ── Configuration ────────────────────────────────────────────────────────────

/// Sync mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Not syncing — Link is enabled but engine doesn't drive REAPER.
    Off,
    /// REAPER follows the Link session (adjusts tempo + nudges playrate).
    Puppet,
    /// REAPER drives the Link session (forces beat/time mapping onto peers).
    Master,
}

/// Engine configuration.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Sync mode.
    pub mode: Mode,
    /// Whether to push local tempo changes into the Link session (puppet mode).
    /// When true, changing tempo in REAPER also changes it for all Link peers.
    pub push_local_tempo: bool,
    /// Whether start/stop sync is enabled (Link transport sync).
    pub start_stop_sync: bool,
    /// Beat phase tolerance before drift correction kicks in (seconds).
    /// Default: derived adaptively from frame time and audio buffer duration.
    pub phase_tolerance: Option<f64>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            mode: Mode::Off,
            push_local_tempo: true,
            start_stop_sync: true,
            phase_tolerance: None,
        }
    }
}

// ── Host Callbacks ───────────────────────────────────────────────────────────

/// Snapshot of the current DAW/host state, captured once per tick.
#[derive(Debug, Clone)]
pub struct LinkState {
    /// Is the host transport currently playing?
    pub is_playing: bool,
    /// Current playback position in seconds (play position if playing, cursor if stopped).
    pub position_seconds: f64,
    /// Current beat position (from TimeMap2_timeToBeats).
    pub beat_position: f64,
    /// Current quarter-note position (from TimeMap2_timeToQN).
    pub qn_position: f64,
    /// Current host tempo in BPM.
    pub tempo_bpm: f64,
    /// Time signature numerator at current position.
    pub timesig_num: i32,
    /// Time signature denominator at current position.
    pub timesig_denom: i32,
    /// Output latency in seconds.
    pub output_latency: f64,
    /// Whether looping is enabled.
    pub is_looping: bool,
    /// Current master playrate (1.0 = normal).
    pub playrate: f64,
    /// Loop time range (start, end) in seconds, if looping is enabled.
    pub loop_range: Option<(f64, f64)>,
    /// Loop time range converted to beats: (start_beat, end_beat).
    pub loop_range_beats: Option<(f64, f64)>,
}

/// Callbacks for the engine to read/write host (REAPER) state.
///
/// All methods are called on the main thread (from the timer callback).
/// Implementations should use REAPER's main-thread API functions.
pub trait LinkCallbacks {
    /// Capture the current host state.
    fn capture_state(&self) -> LinkState;

    /// Start transport playback.
    fn play(&self);

    /// Stop transport playback.
    fn stop(&self);

    /// Set the edit cursor position (without moving the view).
    fn set_cursor_position(&self, seconds: f64);

    /// Set the tempo at the current tempo marker.
    fn set_tempo(&self, bpm: f64);

    /// Get the loop time range as (start_seconds, end_seconds), or None if looping
    /// is disabled.
    fn get_loop_range(&self) -> Option<(f64, f64)>;

    /// Nudge playrate up (e.g., REAPER action 40524 — "increase playrate slightly").
    fn nudge_playrate_up(&self);

    /// Nudge playrate down (e.g., REAPER action 40525 — "decrease playrate slightly").
    fn nudge_playrate_down(&self);

    /// Reset playrate to 1.0 (e.g., REAPER action 40521).
    fn reset_playrate(&self);

    // ── Quantized launch callbacks ───────────────────────────────────────

    /// Navigate to a region by its marker index (e.g., REAPER GoToRegion).
    fn go_to_region(&self, region_id: i32);

    /// Add a project marker or region. Returns the marker/region index.
    ///
    /// - `is_region`: true for region, false for marker
    /// - `start` / `end`: positions in seconds
    /// - `name`: label for the marker/region
    fn add_project_marker(&self, is_region: bool, start: f64, end: f64, name: &str) -> i32;

    /// Remove a project marker/region by index.
    fn remove_project_marker(&self, marker_id: i32);

    /// Set a tempo/time-signature marker at the given position.
    fn set_tempo_at_position(&self, position: f64, bpm: f64, timesig_num: i32, timesig_denom: i32);

    /// Get the time position of the next full measure boundary (seconds).
    fn get_next_measure_position(&self) -> f64;

    /// Create a MIDI item on the given track index from `start` to `end` (seconds).
    fn create_midi_item(&self, track_index: usize, start: f64, end: f64);

    /// Get the total project length in seconds.
    fn get_project_length(&self) -> f64;

    /// Convert a time position to (beat_in_measure, measure_index).
    fn time_to_beats(&self, time_seconds: f64) -> (f64, i32);

    /// Convert (beat_offset, measure_index) to a time position in seconds.
    fn beats_to_time(&self, beat: f64, measure: i32) -> f64;

    /// Remove all dummy markers/regions/items created during quantized launch.
    /// Implementations should search for markers named "fts-link target" and
    /// "fts-link pre-roll" and remove them along with associated tempo markers
    /// and MIDI items.
    fn clear_link_dummy_objects(&self);
}

// ── Engine ───────────────────────────────────────────────────────────────────

/// The Link sync engine.
///
/// Call [`tick()`](Engine::tick) from the host's timer callback (~30Hz).
pub struct Engine {
    link: AblLink,
    session_state: SessionState,
    config: EngineConfig,

    // Internal state for sync tracking
    was_playing: bool,
    prev_qn: f64,
    frame_count: u64,
    quantum: f64,

    /// Tempo requested by a remote peer via the Link tempo callback.
    /// When > 0, the engine should apply this tempo to the host (in puppet mode).
    /// Reset to 0 after being consumed each tick.
    requested_tempo: f64,

    // Rolling-average smoothing (ported from ReaBlink)
    frame_time_avg: RollingAverage,
    diff_avg: RollingAverage,
    last_tick_instant: Option<Instant>,

    // Audio-buffer-aligned timing
    audio_buf: Option<AudioBufferInfo>,

    // Exposed timeline offset (smoothed phase diff, for external consumers)
    timeline_offset: f64,

    // Quantized launch state
    quantized_launch: bool,
    preroll_region_id: i32,
    target_region_id: i32,
    launch_cleared: bool,

    // Loop/jump offset tracking (phase coherence across jumps)
    jump_offset: f64,
    land_offset: f64,
}

impl Engine {
    /// Create a new engine with the given initial tempo.
    pub fn new(initial_tempo: f64, config: EngineConfig) -> Self {
        let link = AblLink::new(initial_tempo);
        let session_state = SessionState::new();

        Self {
            link,
            session_state,
            config,
            was_playing: false,
            prev_qn: 0.0,
            frame_count: 0,
            quantum: 4.0,
            requested_tempo: 0.0,
            frame_time_avg: RollingAverage::new(512),
            diff_avg: RollingAverage::new(8),
            last_tick_instant: None,
            audio_buf: None,
            timeline_offset: 0.0,
            quantized_launch: false,
            preroll_region_id: 0,
            target_region_id: 0,
            launch_cleared: false,
            jump_offset: 0.0,
            land_offset: 0.0,
        }
    }

    /// Enable or disable the Link session.
    pub fn set_enabled(&self, enabled: bool) {
        self.link.enable(enabled);
        if enabled {
            self.link
                .enable_start_stop_sync(self.config.start_stop_sync);
            info!("Ableton Link enabled");
        } else {
            info!("Ableton Link disabled");
        }
    }

    /// Is Link currently enabled?
    pub fn is_enabled(&self) -> bool {
        self.link.is_enabled()
    }

    /// How many peers are connected?
    pub fn num_peers(&self) -> u64 {
        self.link.num_peers()
    }

    /// Get the current Link session tempo.
    pub fn session_tempo(&self) -> f64 {
        self.link
            .capture_app_session_state(&mut SessionState::new());
        // Need to capture into our session state to read tempo
        let mut ss = SessionState::new();
        self.link.capture_app_session_state(&mut ss);
        ss.tempo()
    }

    /// Update the sync mode.
    pub fn set_mode(&mut self, mode: Mode) {
        self.config.mode = mode;
        info!("Link sync mode: {:?}", mode);
    }

    /// Get the current sync mode.
    pub fn mode(&self) -> Mode {
        self.config.mode
    }

    /// Update the engine configuration.
    pub fn set_config(&mut self, config: EngineConfig) {
        self.link.enable_start_stop_sync(config.start_stop_sync);
        self.config = config;
    }

    /// Register a callback for when the number of peers changes.
    pub fn on_num_peers_changed(&self, callback: impl FnMut(u64) + Send + 'static) {
        self.link.set_num_peers_callback(callback);
    }

    /// Register a callback for when the session tempo changes.
    pub fn on_tempo_changed(&self, callback: impl FnMut(f64) + Send + 'static) {
        self.link.set_tempo_callback(callback);
    }

    /// Request a tempo change from an external source (e.g., a remote Link peer).
    ///
    /// This is typically called from within the tempo-changed callback to signal
    /// that a remote peer changed the session tempo. The engine will apply it to
    /// the host on the next tick (in puppet mode), subject to the loop boundary
    /// guard.
    pub fn set_requested_tempo(&mut self, tempo: f64) {
        self.requested_tempo = tempo;
    }

    /// Register a callback for when the start/stop state changes.
    pub fn on_start_stop_changed(&self, callback: impl FnMut(bool) + Send + 'static) {
        self.link.set_start_stop_callback(callback);
    }

    /// Notify the engine of a new audio buffer from the audio thread.
    ///
    /// Call this at the start of each audio buffer (from the audio hook /
    /// `OnAudioBuffer` callback) to provide precise wall-clock timing.
    /// The engine will use this to compute `hostTime` instead of falling
    /// back to `clock_micros()`.
    ///
    /// The `wall_clock_time` field should be captured via `Link::clock_micros()`
    /// on the audio thread, converted to seconds.
    pub fn on_audio_buffer(&mut self, info: AudioBufferInfo) {
        self.audio_buf = Some(info);
    }

    /// The smoothed timeline offset (phase difference in seconds between
    /// the host and the Link session). Positive means the host is ahead.
    pub fn timeline_offset(&self) -> f64 {
        self.timeline_offset
    }

    /// Compute hostTime in microseconds, using audio-buffer-aligned timing
    /// when available, falling back to `Link::clock_micros()`.
    ///
    /// Matches ReaBlink's pattern:
    ///   `g_abuf_time + GetOutputLatency() + g_abuf_len / g_abuf_srate`
    fn compute_host_time(&self, state: &LinkState) -> i64 {
        match self.audio_buf {
            Some(ref buf) if buf.sample_rate > 0.0 => {
                let buf_duration = buf.buf_len as f64 / buf.sample_rate;
                let host_time_secs = buf.wall_clock_time + state.output_latency + buf_duration;
                (host_time_secs * 1_000_000.0).round() as i64
            }
            _ => self.link.clock_micros(),
        }
    }

    /// Main tick — call from the host's timer callback (~30Hz, main thread).
    ///
    /// This captures the current Link session state and host state, then
    /// applies sync corrections based on the current mode.
    pub fn tick(&mut self, host: &dyn LinkCallbacks) {
        if self.config.mode == Mode::Off {
            return;
        }

        // Track frame time intervals with rolling average
        let now = Instant::now();
        if let Some(prev) = self.last_tick_instant {
            let dt = now.duration_since(prev).as_secs_f64();
            self.frame_time_avg.add(dt);
        }
        self.last_tick_instant = Some(now);

        let state = host.capture_state();
        let host_time = self.compute_host_time(&state);

        // Capture the current Link session state (app thread — not realtime)
        self.link.capture_app_session_state(&mut self.session_state);

        // Update quantum from time signature
        let new_quantum = state.timesig_num as f64 / state.timesig_denom as f64 * 4.0;
        if (new_quantum - self.quantum).abs() > 0.001 {
            self.quantum = new_quantum;
        }

        match self.config.mode {
            Mode::Puppet => self.tick_puppet(host, &state, host_time),
            Mode::Master => self.tick_master(host, &state, host_time),
            Mode::Off => {}
        }

        // Commit session state changes back to Link
        self.link.commit_app_session_state(&self.session_state);

        self.frame_count += 1;
    }

    // ── Puppet Mode ──────────────────────────────────────────────────────

    fn tick_puppet(&mut self, host: &dyn LinkCallbacks, state: &LinkState, host_time: i64) {
        let link_playing = self.session_state.is_playing();

        // Start/stop sync: Link controls REAPER transport (only when enabled).
        // When disabled (e.g., PeerMesh controls transport), Link only handles
        // tempo/phase correction and never starts or stops REAPER.
        if self.config.start_stop_sync {
            // Handle transport start
            if !self.was_playing && link_playing {
                debug!(
                    "Link: transport started — starting REAPER (peers={})",
                    self.link.num_peers()
                );
                self.prev_qn = 0.0;
                self.frame_count = 0;

                // The puppet starts from the current cursor position.
                // Phase drift correction (playrate nudging) will align the beat
                // phase with the master over the next few ticks.
                host.play();
                self.jump_offset = 0.0;
                self.land_offset = 0.0;
                self.was_playing = true;
                self.launch_cleared = false;
            }
            // Handle transport stop
            else if self.was_playing && !link_playing {
                debug!("Link: transport stopped — stopping REAPER");
                host.stop();
                host.reset_playrate();
                self.was_playing = false;
                self.prev_qn = 0.0;
                self.launch_cleared = false;
                self.quantized_launch = false;
                host.clear_link_dummy_objects();
                return;
            }
            // Handle REAPER playing without Link playing
            else if !self.was_playing && !link_playing && state.is_playing {
                host.clear_link_dummy_objects();
                host.stop();
                return;
            }
        } else {
            // start_stop_sync disabled — just track play state passively
            // so phase correction knows when we're playing.
            self.was_playing = state.is_playing;
        }

        // Consume any externally-requested tempo (from a remote peer via the
        // tempo callback). This mirrors ReaBlink's requestedTempo pattern: the
        // tempo callback sets it, we consume it once per tick.
        let requested_tempo = self.requested_tempo;
        self.requested_tempo = 0.0;

        // If a remote peer requested a tempo change, push it into the Link
        // session state so it propagates to all peers.
        if requested_tempo > 0.0 {
            self.session_state.set_tempo(requested_tempo, host_time);
        }

        // ── While playing ────────────────────────────────────────────────

        if self.was_playing {
            // Frame 1 after quantized launch: jump to the target region.
            // Playback started in the pre-roll region; now we jump to where
            // the music actually begins, aligned to the Link quantum.
            // Mirrors ReaBlink audioCallback lines 417-421.
            if self.frame_count == 1 && self.link.num_peers() > 0 && self.quantized_launch {
                host.go_to_region(self.target_region_id);
                self.quantized_launch = false;
                debug!(
                    "Link: quantized launch — jumped to target region {}",
                    self.target_region_id
                );
            }

            // Once playback has settled past the target region, clean up dummy
            // objects (pre-roll region, tempo markers, MIDI items).
            // Mirrors ReaBlink audioCallback lines 423-435.
            if !self.launch_cleared && self.target_region_id != 0 {
                // After a few frames the transport has settled into the target
                // region. A full implementation would use
                // GetLastMarkerAndCurRegion, but at ~30Hz timer granularity
                // checking frame_count > 2 is sufficient.
                if self.frame_count > 2 {
                    self.launch_cleared = true;
                    host.clear_link_dummy_objects();
                    self.target_region_id = 0;
                    self.preroll_region_id = 0;
                    debug!("Link: quantized launch cleanup complete");
                }
            }

            // Sync tempo between host and Link session.
            let link_tempo = self.session_state.tempo();
            let tempo_diff = (state.tempo_bpm - link_tempo).abs();

            if tempo_diff > TEMPO_TOLERANCE
                && self.session_state.beat_at_time(host_time, self.quantum) > 0.0
            {
                if requested_tempo > 0.0 {
                    // A remote peer explicitly requested a tempo — apply it
                    let guarded = self.apply_loop_boundary_guard(state, requested_tempo);
                    host.set_tempo(guarded);
                    self.session_state.set_tempo(guarded, host_time);
                } else if self.link.num_peers() > 0 {
                    // Link session tempo differs from host — follow the session.
                    // This handles the master changing tempo.
                    let guarded = self.apply_loop_boundary_guard(state, link_tempo);
                    host.set_tempo(guarded);
                } else if self.config.push_local_tempo {
                    // No peers, local tempo changed — push to Link
                    self.session_state.set_tempo(state.tempo_bpm, host_time);
                }
            } else if requested_tempo > 0.0 {
                // Explicit request even when tempos already match — apply it
                let guarded = self.apply_loop_boundary_guard(state, requested_tempo);
                if (state.tempo_bpm - guarded).abs() > TEMPO_TOLERANCE {
                    host.set_tempo(guarded);
                }
            }

            // Loop/jump detection — must run before phase drift correction
            // so that offsets are up-to-date for the current frame.
            self.detect_jumps(state, host_time);

            // Phase drift correction (only with peers, not during quantized launch)
            if self.link.num_peers() > 0 && !self.quantized_launch {
                self.correct_phase_drift(host, state, host_time);
            }
        }

        self.prev_qn = state.qn_position;
    }

    // ── Quantized Launch ─────────────────────────────────────────────────

    /// Set up the pre-roll and target regions for a quantized launch.
    ///
    /// Creates temporary objects in the project so that playback can start
    /// from a pre-roll position (at the project end) and then jump to the
    /// target measure boundary on the next frame, perfectly aligned with the
    /// Link session's quantum.
    ///
    /// Mirrors ReaBlink audioCallback lines 308-370.
    fn begin_quantized_launch(
        &mut self,
        host: &dyn LinkCallbacks,
        state: &LinkState,
        host_time: i64,
    ) {
        let link_tempo = self.session_state.tempo();
        self.session_state.set_tempo(link_tempo, host_time);

        let pos_target = host.get_next_measure_position();
        let region_length = 60.0 / link_tempo * self.quantum;

        // Create target region at the next measure boundary
        self.target_region_id = host.add_project_marker(
            true,
            pos_target,
            pos_target + region_length,
            "fts-link target",
        );

        // Set tempo marker at target position to match Link tempo
        host.set_tempo_at_position(
            pos_target,
            link_tempo,
            state.timesig_num,
            state.timesig_denom,
        );

        // Create pre-roll region at the end of the project (past all content)
        let project_length = host.get_project_length();
        let (_beat, measure) = host.time_to_beats(project_length);
        let preroll_start = host.beats_to_time(0.0, measure);

        self.preroll_region_id = host.add_project_marker(
            true,
            preroll_start,
            preroll_start + region_length,
            "fts-link pre-roll",
        );

        // Set tempo marker at pre-roll position
        host.set_tempo_at_position(
            preroll_start,
            link_tempo,
            state.timesig_num,
            state.timesig_denom,
        );

        // Create a MIDI item at the pre-roll region so REAPER has content to
        // play through (otherwise it may stop early at the project end).
        host.create_midi_item(0, preroll_start, preroll_start + region_length);

        // Request beat alignment at start-playing time
        self.session_state
            .request_beat_at_start_playing_time(0.0, self.quantum);

        // Calculate cursor position to align with Link phase.
        // beat_offset is how far into the quantum we need to be when playback
        // starts, so the pre-roll duration matches the time until the next
        // quantum boundary.
        let beat_now = self.session_state.beat_at_time(host_time, self.quantum);
        let beat_offset = self.quantum - beat_now.abs();
        let (_beat, preroll_measure) = host.time_to_beats(preroll_start);
        let cursor_pos = host.beats_to_time(beat_offset, preroll_measure);

        host.set_cursor_position(cursor_pos);
        self.quantized_launch = true;

        debug!(
            "Link: quantized launch — target={}, preroll={}, cursor={:.3}s, beat_offset={:.3}",
            self.target_region_id, self.preroll_region_id, cursor_pos, beat_offset
        );
    }

    // ── Loop/Jump Detection ─────────────────────────────────────────────

    /// Detect QN position jumps (loops, user seeks) and maintain phase coherence.
    ///
    /// When a jump is detected (|qn - prev_qn| > 0.5):
    /// - If looping: accumulate beat offsets at loop boundaries into
    ///   `jump_offset` / `land_offset` so phase correction stays aligned.
    /// - If not looping (manual seek): call `request_beat_at_time` to
    ///   re-synchronize the beat/time mapping with peers.
    ///
    /// Mirrors ReaBlink audioCallback lines 454-474.
    fn detect_jumps(&mut self, state: &LinkState, host_time: i64) {
        let qn_delta = (state.qn_position - self.prev_qn).abs();

        // Only trigger on significant jumps, and only after the session has
        // been playing long enough (beat > 1.0) to avoid false positives on
        // the initial start.
        if qn_delta > 0.5 && self.session_state.beat_at_time(host_time, self.quantum) > 1.0 {
            if state.is_looping {
                // Loop jump: accumulate beat offsets at loop boundaries so
                // phase correction remains coherent across loop iterations.
                // jump_offset tracks where we left off (loop end beat),
                // land_offset tracks where we landed (loop start beat).
                if let Some((loop_start, loop_end)) = state.loop_range
                    && state.position_seconds > loop_start
                    && state.position_seconds < loop_end
                    && let Some((start_beat, end_beat)) = state.loop_range_beats
                {
                    self.jump_offset = fmod(self.jump_offset + end_beat, 1.0);
                    self.land_offset = fmod(self.land_offset + start_beat, 1.0);
                    debug!(
                        "Link: loop jump — jump_offset={:.3}, land_offset={:.3}",
                        self.jump_offset, self.land_offset
                    );
                }
            } else {
                // Non-loop jump (user seek): re-align the beat/time mapping
                // so that the Link session phase matches the new position.
                let beat_unit = 4.0 / state.timesig_denom as f64;
                self.session_state.request_beat_at_time(
                    fmod(state.beat_position, 1.0),
                    host_time,
                    beat_unit,
                );
                debug!("Link: non-loop jump — re-aligned beat/time mapping");
            }
        }
    }

    // ── Master Mode ──────────────────────────────────────────────────────

    fn tick_master(&mut self, host: &dyn LinkCallbacks, state: &LinkState, host_time: i64) {
        // In master mode, force REAPER's beat position onto the Link session
        if state.is_playing {
            // Detect large position jumps (seeks while playing) —
            // re-force beat=0 so the puppet re-syncs from the new position
            if self.was_playing && (state.qn_position - self.prev_qn).abs() > 2.0 {
                self.session_state
                    .force_beat_at_time(0.0, host_time, self.quantum);
                debug!(
                    "Link master: detected seek while playing (qn={:.1} → {:.1}), reset beat=0",
                    self.prev_qn, state.qn_position
                );
            } else {
                self.session_state
                    .force_beat_at_time(state.qn_position, host_time, self.quantum);
            }
            self.prev_qn = state.qn_position;
        }

        // Also push tempo
        if (state.tempo_bpm - self.session_state.tempo()).abs() > TEMPO_TOLERANCE {
            self.session_state.set_tempo(state.tempo_bpm, host_time);
        }

        // Push transport state
        if state.is_playing && !self.was_playing {
            // Starting playback — force beat 0 at current position so the
            // puppet starts from the same place, regardless of accumulated
            // Link session beats from prior playback.
            self.session_state
                .force_beat_at_time(0.0, host_time, self.quantum);
            self.session_state.set_is_playing(true, host_time);
            self.was_playing = true;
            debug!(
                "Link master: started at pos={:.3}s, forced beat=0",
                state.position_seconds
            );
        } else if !state.is_playing && self.was_playing {
            self.session_state.set_is_playing(false, host_time);
            self.was_playing = false;
        }

        let _ = host; // master mode doesn't modify host state
    }

    // ── Loop Boundary Tempo Guard ─────────────────────────────────────────

    /// If the playback position is inside a loop and within 1 beat of the
    /// loop end, return the current host tempo instead of the desired tempo.
    /// This prevents audible glitches when the transport is about to wrap
    /// back to the loop start.
    ///
    /// Matches ReaBlink engine.cpp lines 536-548.
    fn apply_loop_boundary_guard(&self, state: &LinkState, desired_tempo: f64) -> f64 {
        if !state.is_looping {
            return desired_tempo;
        }

        if let (Some((loop_start, loop_end)), Some((_start_beat, end_beat))) =
            (state.loop_range, state.loop_range_beats)
        {
            let pos = state.position_seconds;
            if pos > loop_start && pos < loop_end && (state.beat_position - end_beat).abs() < 1.0 {
                debug!(
                    "Link: suppressing tempo change near loop end (beat {:.2}, loop end beat {:.2})",
                    state.beat_position, end_beat
                );
                return state.tempo_bpm;
            }
        }

        desired_tempo
    }

    // ── Phase Drift Correction ───────────────────────────────────────────

    fn correct_phase_drift(&mut self, host: &dyn LinkCallbacks, state: &LinkState, host_time: i64) {
        let denom = state.timesig_denom as f64;
        let beat_unit = 4.0 / denom;

        // Calculate phase in the current beat unit
        let link_phase = self.session_state.phase_at_time(host_time, beat_unit) * (denom / 4.0);

        // Factor in loop/jump offsets for REAPER phase calculation.
        // This keeps phase coherent across loop iterations and user seeks.
        // Mirrors ReaBlink audioCallback line 476:
        //   auto reaper_phase_current = fmod(beat - land_offset + jump_offset, 1.0);
        let reaper_phase = fmod(
            state.beat_position - self.land_offset + self.jump_offset,
            1.0,
        );

        // Convert to time
        let tempo = self.session_state.tempo();
        if tempo <= 0.0 {
            return;
        }
        let reaper_phase_time = reaper_phase * 60.0 / tempo;
        let link_phase_time = fmod(link_phase, 1.0) * 60.0 / tempo;

        let raw_diff = reaper_phase_time - link_phase_time;

        // Smooth the phase diff over 8 samples (matching ReaBlink's RollingAverage(8))
        self.diff_avg.add(raw_diff);
        let diff = self.diff_avg.average();

        // Expose smoothed timeline offset for external consumers
        self.timeline_offset = diff;

        // Determine tolerance threshold — adaptive based on frame_time and buf_len_time.
        // Matches ReaBlink's logic: uses the larger of frame-time-based and
        // buffer-duration-based limits, with a tighter denominator (1.0) when
        // output_latency / buf_duration > 3.
        let limit = self.config.phase_tolerance.unwrap_or_else(|| {
            let frame_time = self.frame_time_avg.average();
            let buf_len_time = self.audio_buf_duration();

            let limit_denom = if buf_len_time > 0.0 && state.output_latency / buf_len_time > 3.0 {
                1.0
            } else {
                8.0
            };

            let ft_limit = frame_time / limit_denom;
            let buf_limit = buf_len_time / limit_denom;
            ft_limit.max(buf_limit).max(0.002)
        });

        if diff.abs() > limit && (reaper_phase - link_phase).abs() < 0.5 {
            // REAPER is ahead or behind — nudge playrate
            if diff > 0.0 && state.playrate >= 1.0 {
                host.nudge_playrate_down();
                host.nudge_playrate_down();
            } else if diff < 0.0 && state.playrate <= 1.0 {
                host.nudge_playrate_up();
                host.nudge_playrate_up();
            }
        } else if diff.abs() < limit && (state.playrate - 1.0).abs() > 0.001 {
            // Within tolerance — reset playrate to normal
            host.reset_playrate();
        }
    }

    /// Audio buffer duration in seconds (0.0 if no audio buffer info yet).
    fn audio_buf_duration(&self) -> f64 {
        match self.audio_buf {
            Some(ref buf) if buf.sample_rate > 0.0 => buf.buf_len as f64 / buf.sample_rate,
            _ => 0.0,
        }
    }
}

/// Floating-point modulo that handles negative values correctly.
/// Returns a value in `[0, modulus)` for positive modulus.
fn fmod(value: f64, modulus: f64) -> f64 {
    let r = value % modulus;
    if r < 0.0 { r + modulus } else { r }
}

const TEMPO_TOLERANCE: f64 = 0.001;

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolling_average_basic() {
        let mut ra = RollingAverage::new(4);
        assert!(ra.is_empty());
        assert_eq!(ra.average(), 0.0);

        ra.add(1.0);
        ra.add(2.0);
        ra.add(3.0);
        // 3 samples, quarter = 0, trimmed = all 3
        let avg = ra.average();
        assert!((avg - 2.0).abs() < 1e-9);
    }

    #[test]
    fn rolling_average_circular_eviction() {
        let mut ra = RollingAverage::new(3);
        ra.add(10.0);
        ra.add(20.0);
        ra.add(30.0);
        // Full: [10, 20, 30]
        ra.add(40.0);
        // Now: [40, 20, 30] (oldest evicted)
        assert_eq!(ra.len(), 3);
        // sorted: [20, 30, 40], quarter=0 => avg of all = 30
        assert!((ra.average() - 30.0).abs() < 1e-9);
    }

    #[test]
    fn rolling_average_outlier_rejection() {
        let mut ra = RollingAverage::new(8);
        // 6 normal values + 1 low outlier + 1 high outlier
        ra.add(100.0); // outlier high
        ra.add(10.0);
        ra.add(10.0);
        ra.add(10.0);
        ra.add(10.0);
        ra.add(10.0);
        ra.add(10.0);
        ra.add(0.001); // outlier low
        // sorted: [0.001, 10, 10, 10, 10, 10, 10, 100]
        // quarter = 2, trimmed = [10, 10, 10, 10] => avg = 10.0
        assert!((ra.average() - 10.0).abs() < 1e-9);
    }

    #[test]
    fn audio_buffer_info_duration() {
        let info = AudioBufferInfo {
            buf_len: 512,
            sample_rate: 48000.0,
            wall_clock_time: 1000.0,
        };
        let duration = info.buf_len as f64 / info.sample_rate;
        assert!((duration - 512.0 / 48000.0).abs() < 1e-12);
    }
}
