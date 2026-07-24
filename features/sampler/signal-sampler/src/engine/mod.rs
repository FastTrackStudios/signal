//! Audio engine — MIDI event processing and real-time sample rendering.
//!
//! # Design
//!
//! `SampleEngine` is a single playback instance for one sample library patch. It owns:
//! - the `PlayerPatch` (spec + sample index)
//! - a `SampleCache` for decoded WAV data
//! - a `VoicePool` of active voices
//! - `RrCounters` to cycle through round-robin slots
//!
//! One `SampleEngine` per MIDI track / instrument section is the expected usage.
//!
//! # CC1 dynamics
//!
//! Two voices are kept alive simultaneously for the current note:
//! - `SustainLo` — the softer adjacent layer
//! - `SustainHi` — the louder adjacent layer
//!
//! Their gains crossfade linearly as CC1 moves through the overlap region
//! between adjacent dynamic layers. Gain updates are ramped over a short
//! window to avoid zipper noise.
//!
//! # Legato
//!
//! When a second note arrives while a note is held, `SampleEngine` enters
//! `LegatoState::Pending` and counts down `frames_remaining` (derived from
//! the velocity-based pre-delay in the spec). When the countdown expires the
//! old sustain is faded out and the legato transition sample fires.

pub mod cache;
pub mod filter;
pub mod pitch_shift;
pub mod rr;
pub mod trace;
pub mod voice;
mod midi;
mod dispatch;
mod helpers;
mod legato;

use std::cell::{Cell, RefCell};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::Path;

use crate::spec::{ArticulationKind, Cc1Layer};
use crate::{PlayerPatch, VoiceConfig};
use cache::{EvictStats, PreloadStats, SampleCache, SampleData};
use filter::BiquadFilter;
use rr::RrCounters;
pub use trace::{MissReason, RenderTrace, TraceEvent, TraceKind, VoiceSpawn as TraceVoiceSpawn};
pub use voice::ArticClass;
use voice::{DynLayer, FlexEnv, Voice, VoiceKind, VoicePool, VoiceStealPolicy};

// ── Constants ─────────────────────────────────────────────────────────────────

/// CC1 gain ramp length (ms). Smooths dynamic crossfade to avoid clicks.
/// CC-mod smoothing lags DECODED from the CSS group modulators
/// (GROUP_MODULATORS.md): every CC1/CC11 volume mod carries `lag_ms = 120`,
/// every CC2 (vibrato crossfade) mod `lag_ms = 1000`. These are Kontakt's
/// per-modulator smoothing — the reason CSS CC sweeps are glassy.
const CC1_RAMP_MS: u32 = 120;
const CC2_RAMP_MS: u32 = 1000;

/// Default release fade on note-off for sustain voices (ms).
///
/// This is the damper-down time: on key-up (sustain pedal NOT held) the note
/// must stop, not ring out. A short linear fade reads as an immediate cutoff
/// while avoiding a click. Sustain itself comes only from holding CC64, which
/// defers the note-off entirely (see `note_off_with_velocity`); the half-pedal
/// curve still scales this up for partial-pedal positions. Was 500 ms, which
/// made every key-up sound like a half-second sustain even with the pedal up.
const RELEASE_MS: u32 = 60;
/// Extra damping time at the top of the half-pedal range.
const HALF_PEDAL_MAX_RELEASE_MULTIPLIER: f32 = 4.0;

/// Short de-click fade for sample-playback instruments that have explicit
/// release/noise samples. The tone should stop on key-up; this only avoids a pop.
/// Damper-engagement time on note-off when a release_artic supplies the
/// natural tail (e.g. Keyscape pianos). Long enough that the body fades
/// naturally — short enough that holding the key is what determines note
/// length. Real Rhodes dampers ring out over ~50–100 ms.
const KEY_UP_DECLICK_MS: u32 = 80;

/// Cap on the lifetime of a `VoiceKind::Release` voice. The library's
/// release-tail FLACs can be very long (Keyscape ships 30 s clips) and
/// release voices play to sample-end, so without a cap each note-off
/// consumes a voice slot for 30 s and the 64-voice pool fills up after
/// ~20 key presses — causing voice-stealing and intermittent silence.
const RELEASE_MAX_LIFETIME_MS: u32 = 2_000;

// The legato/articulation POLICY numbers that used to live here as constants
// (note-off fades, retire crossfades, CC1 expression curve, legato sustain
// trim, output makeup, master tune, Overlap-Delay + `$1fvjk` start-offset
// curves, ENV_FLEX amp-envelope tables) are DATA now: see
// `crate::spec::{PerformanceSpec, LegatoEngineSpec, DynamicsSpec,
// Cc1ExpressionSpec, default_amp_env}`. Their defaults equal the historical
// hardcoded values, so specs that don't set them play identically; the
// engine keeps only the mechanism (zone resolution, scheduling,
// crossfading) and reads every number from `patch.spec`.
// r[impl signal.soundsource.declarative]

/// Minimum attack fade (ms) for a synthesized-loop sustain that starts mid-sample
/// at full level — just enough to avoid an onset click without slowing the attack.
const SUSTAIN_DECLICK_MS: u32 = 12;

/// CSS Expressive crossfade shape (KSP §3.1/§8, shipped values): the DESTINATION
/// sustain swells up under the transition, filling the bow-change dip and
/// reaching full at the arrival tick. Two-stage: a fast stage-1 to ~90 % then a
/// slow stage-2 swell to full.
// Exact shipped anchors from persistent_1.tsv (css-ksp-anchor-values.md).
const CSS_XTIME_MS: u32 = 225; // $a3zg3 (XTime) — FLAT 225 ms (all IOI anchors = 225)
const CSS_ATK_FADE_PCT: u32 = 50; // $igmiu — stage split %, kbqnb=0 (soft); 60 hard
/// Low-Latency legato crossfade (`$ocjln=0` standard engine, KSP §3.7): a short
/// SINGLE-stage fade, no two-stage swell — the snappy, responsive mode selected
/// by a CC58 0-5 keyswitch (default document legato is Expressive).
const CSS_LL_XFADE_MS: u32 = 80;

/// `$x444h` (Node-Vol) — the stage-1 fade divisor, and the ONLY IOI-scaled
/// crossfade param (css-ksp-anchor-values.md §3): 90 for IOI<150 ms, lerp
/// 90→60 over 150-300 ms, then flat 60. A smaller divisor lengthens stage-1
/// (`$mlnoy = XTime*igmiu/$x444h`) → a more gradual first stage on slow notes.
fn css_node_vol_div(ioi_ms: f32) -> u32 {
    if ioi_ms <= 150.0 {
        90
    } else if ioi_ms >= 300.0 {
        60
    } else {
        (90.0 - (90.0 - 60.0) * (ioi_ms - 150.0) / 150.0).round() as u32
    }
}

/// Portamento micro-glide (CSS `$ma0b1` on, KSP §3.2/§8, shipped): the incoming
/// note scoops to true pitch over `$1mwwo`=60 ms with a `$ruv02`=10 base bend
/// (`$jyttf = $ruv02*1000` millicents ⇒ ~10 cents; `$i1kki`=10 = no interval
/// scaling in the shipped state).
const CSS_PORTA_BTIME_MS: u32 = 60; // $1mwwo

/// CSS `$1fvjk` main-transition (`%jcxqm`) sample-start offset (ms) by
/// (hard-table, interval), css-ksp-anchor-values.md §6: soft/≤12 st = 60,
/// soft/>12 = 10, hard/≤12 = 20, hard/>12 = 0. How far INTO the transition
/// recording playback begins, skipping the sharp bow-attack head. Used as a
/// MINIMUM skip on the document path so the onset is never audible.
fn css_lt_min_skip_ms(kbqnb_hard: bool, interval: u32) -> u32 {
    match (kbqnb_hard, interval > 12) {
        (false, false) => 60,
        (false, true) => 10,
        (true, false) => 20,
        (true, true) => 0,
    }
}

/// Portamento bend depth (cents, unsigned) as a function of interval and IOI
/// (`$ruv02` interp, css-ksp-anchor-values.md §5a). Breakpoints 75/100/500 ms;
/// per-interval anchors. Notably interval-2 (whole-tone) bend is 0 for IOI ≥
/// 500 ms — a slow whole-tone line gets NO glide; the glide only appears on
/// fast and/or small-interval moves.
fn css_bend_cents(interval: u8, ioi_ms: f32) -> f32 {
    let (a, b, c) = match interval {
        0 | 1 => (40.0, 10.0, 10.0),
        2 => (30.0, 10.0, 0.0),
        _ => (20.0, 0.0, 0.0),
    };
    if ioi_ms <= 75.0 {
        a
    } else if ioi_ms <= 100.0 {
        a + (b - a) * (ioi_ms - 75.0) / 25.0
    } else if ioi_ms <= 500.0 {
        b + (c - b) * (ioi_ms - 100.0) / 400.0
    } else {
        c
    }
}

/// Attack-transient anti-machine-gun dip (dB, KSP §7.3): a connected note within
/// `$xu41m` (250 ms) of the previous onset plays quieter — from 0 dB at 250 ms
/// down to `$4lqhx` (~−3 dB shipped) at 0 ms — so rapid re-articulations don't
/// machine-gun. (The `$c2hkn` 1-2 s recovery is approximated by the per-note IOI
/// mapping: a note is dipped by how close it sits to the previous onset.)
/// DECODED CORRECTION (script_1.ksp 12716-12731, shipped persistent values):
/// the window is keyed on `%i35so[$EVENT_NOTE]` — the SAME PITCH's last onset,
/// not the line IOI — and shipped `$4lqhx`=−30/`$ee3a4`=0 make it a FLAT −3 dB
/// within `$xu41m`=250 ms of the same pitch (no slope, no recovery ramp since
/// ee3a4=0). Scale runs never trigger it; only rapid same-pitch repeats do.
/// Callers pass the SAME-PITCH gap; the line-IOI misuse was corrected at the
/// call sites (dispatch.rs re-attack path passes the same-note gap).
fn css_attack_transient_dip_db(same_pitch_gap_ms: f32) -> f32 {
    const WINDOW_MS: f32 = 250.0; // $xu41m
    const DIP_DB: f32 = -3.0; // $4lqhx=-30, flat ($ee3a4=0 → no slope)
    if same_pitch_gap_ms > 0.0 && same_pitch_gap_ms <= WINDOW_MS {
        DIP_DB
    } else {
        0.0
    }
}

/// Onset declick (ms) for legato transitions, release tails, and any voice that
/// starts mid-sample (`start_offset`). Long enough to remove the onset step
/// click, short enough to be inaudible on a recorded bow-change / release.
const ONSET_DECLICK_MS: u32 = 6;

/// Note-body peak (linear, ~-54 dBFS) below which a note-off fires NO release
/// tail: the note has already decayed to near-silence, so a release would just
/// be a "thump" on a gone note.
const RELEASE_BODY_FLOOR: f32 = 0.002;
/// Below this release velocity the release sample is suppressed entirely —
/// avoids producing an audible click on every note-off when a controller's
/// release-velocity is effectively 0.
const RELEASE_SAMPLE_VELOCITY_MIN: u8 = 1;
const RECENT_MISS_LIMIT: usize = 8;

// ── Legato state ──────────────────────────────────────────────────────────────

enum LegatoState {
    Idle,
    Pending {
        frames_remaining: usize,
        from_note: u8,
        to_note: u8,
        to_note_velocity: u8,
        /// Use Port samples (portamento glide) instead of Leg samples.
        portamento: bool,
        /// Inter-onset interval (ms) at the moment the transition was armed —
        /// carried through the countdown so the reactive fire can apply the
        /// CSS `$1fvjk` sample-start offset (`LegatoEngineSpec::start_offset_ms`).
        ioi_ms: f32,
    },
}

/// # The two scheduling paths — READ THIS before touching legato timing
///
/// There are exactly two ways notes reach the legato engine, and they must
/// never be conflated (they were a constant source of "why is the timing
/// different" confusion):
///
/// **LIVE / reactive** ([`StrictLive`](PlayMode::StrictLive)) — real-time
/// keyboard input. The next note is UNKNOWN, so a transition can only fire
/// AFTER the new note-on, delayed by its velocity zone. Entry:
/// `note_on` → (`live_divisi_note_on` |) `note_on_line`'s sounding-line arm
/// → [`start_legato_transition`](Self::start_legato_transition) → a
/// `LegatoState::Pending` countdown drained by `advance_legato_countdowns`.
/// `note_off` may synthesise a fallback transition. Every reactive fire
/// bumps `reactive_legato_fires`. **This path is what the deployed live rig
/// (CLAP no-schedule branch, strings-TUI keyboard) plays on — do not break
/// it.**
///
/// **DOCUMENT / prefire** ([`Lookahead`](PlayMode::Lookahead)) — the score is
/// known ahead (ARA-style: `render_offline_document` / `RealtimeScheduler`).
/// The scheduler PREFIRES each note so the sample starts BEFORE the beat and
/// the arrival lands ON it. Entry: the annotator emits `LegatoPrefire` →
/// `legato_prefire_line_lead` → `fire_legato_with_lead` **directly** — it
/// never touches `start_legato_transition`, never arms a countdown, never
/// bumps `reactive_legato_fires`. Invariant: **during a document render
/// `reactive_fallbacks` MUST be 0** — a non-zero count means a schedule edge
/// leaked to the reactive path (a bug), and the report flags it in red.
///
/// Current CSS-legato work happens ON THE DOCUMENT PATH ONLY; the live path
/// is kept as-is and revisited later.
///
/// Automatic play-mode policy (see `docs/plan/document-mode.md`, "Mode
/// policy: strict live low-latency by default"): if the engine can see the
/// future it plays beautifully; if it can't, it plays NOW.
///
/// - [`StrictLive`](PlayMode::StrictLive) (default) — zero added latency, no
///   exceptions: reactive legato uses the `low_latency` velocity→delay
///   tables regardless of what CC58 requested, shorts fire immediately
///   (there is no pre-delay concept outside a schedule), and
///   [`latency_frames`](SampleEngine::latency_frames) reports 0.
/// - [`Lookahead`](PlayMode::Lookahead) — the MIDI is known ahead of time
///   (document playback / offline render): full `expressive` legato,
///   transitions prefired by the scheduler. Also reports 0 latency — the
///   engine anticipates rather than delays.
///
/// Selection is automatic: the document scheduler forces `Lookahead` for the
/// duration of a render and restores `StrictLive` after; live dispatch never
/// leaves `StrictLive` unless explicitly overridden
/// ([`set_legato_mode`](SampleEngine::set_legato_mode) with
/// `expressive = true`, or [`set_play_mode`](SampleEngine::set_play_mode) —
/// the CSS-parity harnesses use this to reproduce Kontakt's expressive
/// latency reactively).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayMode {
    #[default]
    StrictLive,
    Lookahead,
}

/// Identifies one monophonic legato line inside an engine. Lines are
/// first-class engine entities — they are NOT MIDI channels. Allocators sit
/// in front of the line pool and decide which line an incoming note belongs
/// to (see `docs/plan/document-mode.md`, "Auto-divisi"):
/// - the document scheduler currently maps channel N → line N (import path),
/// - lookahead auto-divisi (annotate-time) and live greedy auto-divisi are
///   future allocators over the same pool.
///
/// Live single-line play uses line 0 everywhere, which is bit-identical to
/// the pre-line engine.
pub type LineId = usize;

/// Size of the per-engine mono-line pool. Matches the 16 MIDI channels the
/// channel→line allocator can address; auto-divisi allocators never need
/// more simultaneous lines than a section has players.
pub const MAX_LINES: usize = 16;

/// Per-line monophonic legato state: the sounding note, key press order for
/// last-note-priority fallback, the pending reactive transition countdown,
/// and the line's CC1/CC2 dynamics (CC1 is per-channel in MIDI; a divisi
/// line's dynamics ride its own controller lane).
pub(crate) struct LegatoLine {
    /// The note currently sounding on this mono line (zoned legato mode).
    /// `None` when the line is silent. A new note transitions FROM this
    /// note; releasing it falls back to the most-recent still-held note.
    note: Option<u8>,
    /// Press order of held keys (most-recent last) for last-note-priority
    /// mono legato fall-back when the sounding note is released.
    order: Vec<u8>,
    /// Legato pre-delay countdown (reactive path).
    state: LegatoState,
    /// This line's CC1 value [0–127] — dynamic layer crossfade.
    cc1: u8,
    /// This line's CC2 value [0–127] — vibrato / non-vibrato crossfade.
    cc2: u8,
    /// The note that last ENDED this line (line went silent), for the live
    /// allocator's "just released" abutment gate.
    released_note: Option<u8>,
    /// Engine frame at which the line last went silent.
    last_release_frame: u64,
    /// Engine frame of the line's last allocation/trigger — LRU key for the
    /// live allocator's free-line search.
    last_activity: u64,
    /// Engine frame of this line's most recent note-on. The reactive legato
    /// path measures the inter-onset interval (IOI) as `now − last_onset_frame`
    /// to drive the Overlap-Delay (spec §2.1 — IOI-driven, not velocity).
    last_onset_frame: u64,
}

impl Default for LegatoLine {
    fn default() -> Self {
        Self {
            note: None,
            order: Vec::new(),
            state: LegatoState::Idle,
            cc1: 64,
            cc2: 0,
            released_note: None,
            last_release_frame: 0,
            last_activity: 0,
            last_onset_frame: 0,
        }
    }
}

/// One legato transition firing, recorded for tests / offline analysis when
/// the fire log is enabled (see [`SampleEngine::set_legato_fire_log_enabled`]).
/// `frame` is the engine's running render position ([`SampleEngine::frames_rendered`])
/// at the moment the transition voice spawned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegatoFireEvent {
    pub frame: u64,
    /// Mono line the transition fired on ([`LineId`], truncated to u8).
    pub line: u8,
    pub from_note: u8,
    pub to_note: u8,
    pub velocity: u8,
    pub portamento: bool,
    /// Predicted HEARD-ARRIVAL frame of the destination pitch, derived from
    /// the transition zone that actually spawned: the zone's measured
    /// `lead_in_ms` (the in-sample arrival marker — time from sample start
    /// until the pitch leaves the source; the loop region beyond it is the
    /// settled destination) minus the applied sample-start offset, converted
    /// to wall frames at the voice's playback rate, from `frame`. Equals
    /// `frame` for same-pitch re-bows (Legzero has no lead-in), legacy
    /// libraries without measured transitions, and non-zoned patches.
    /// Deterministic: no audio analysis involved — this is the schedule/zone
    /// metadata's own claim of when the note speaks.
    pub arrival: u64,
}

/// Cap on the fire log so an enabled log can never grow unbounded on the
/// audio thread (the Vec is pre-allocated to this capacity when enabled).
const LEGATO_FIRE_LOG_CAP: usize = 1024;

/// One PLAYBACK-EMITTED marker: the voice actually played through the
/// zone's marker position at this output frame — after every start-offset
/// skip, start hold, and playback-rate scaling. Nothing is estimated; the
/// emitted time IS what was heard (r[signal.sampling.markers.arrival]).
/// Collected per block from the voice pool when the log is enabled; the
/// schedule-derived marker timeline remains available as the INTENDED
/// times for comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmittedMarker {
    /// Absolute engine output frame of the crossing.
    pub frame: u64,
    /// The note the emitting voice belongs to.
    pub note: u8,
    /// Mono line of the emitting voice.
    pub line: u8,
}

/// Cap on the emitted-marker log (pre-allocated on enable; the audio
/// thread never allocates, and excess emissions are dropped, not grown).
const EMITTED_MARKER_LOG_CAP: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ZoneTrigger {
    Attack,
    Release,
    PedalDown,
    PedalUp,
    Cc,
    Aftertouch,
}

// ── SampleEngine ──────────────────────────────────────────────────────────────────

/// Real-time sample playback engine for one sample library section.
pub struct SampleEngine {
    patch: PlayerPatch,
    cache: SampleCache,
    voices: VoicePool,
    /// Round-robin counters. Interior-mutable so voice resolution
    /// (`make_voice`) can run as `&self` — it only advances RR and records
    /// miss telemetry, letting note-on pass `&self.articulation`/`section`/`mic`
    /// without cloning.
    rr: RefCell<RrCounters>,

    /// Audio sample rate (Hz).
    pub sample_rate: u32,

    /// Active section ID (e.g. `"1v"`, `"Va"`, `"Ce"`).
    section: String,
    /// Active articulation ID (e.g. `"Vibsus"`, `"Staccato"`).
    articulation: String,
    /// Active microphone position ID (e.g. `"Mix"`, `"Main"`).
    mic: String,
    /// Opt-in single-mic filter for multi-mic zone sets that declare no
    /// `mics` block (so mic_index folds everything to bus 0). When `Some`,
    /// only zones whose `mic` matches fire — otherwise every mic in the set
    /// sounds at once (Main + Mix doubling). `None` keeps the default
    /// play-all-mics behaviour used by multi-mic mixing + drum kits.
    solo_mic: Option<String>,
    /// MIDI note → index into `keyswitch.notes`: which incoming notes are
    /// velocity-sensitive keyswitches (selecting articulation / mode) rather
    /// than sounding. Built from the spec at construction.
    keyswitch_notes: HashMap<u8, usize>,
    /// Current legato transition direction (`"up"` / `"down"`) — selects the
    /// directional legato zone (CSS records a separate sample per direction).
    /// Defaults to `"up"`; updated per note from the interval played.
    play_direction: String,
    /// Pool of monophonic legato lines (see [`LineId`]). Line 0 is the
    /// default line used by the channel-less legacy API, so live single-line
    /// behavior is unchanged. The document scheduler (or a future
    /// auto-divisi allocator) addresses lines explicitly via the `_line`
    /// method variants.
    lines: Vec<LegatoLine>,
    /// The line the current dispatch is acting on. Set by
    /// [`set_active_line`](Self::set_active_line) at every public entry
    /// point; internal trigger helpers read it for voice tagging and
    /// line-state bookkeeping.
    cur_line: LineId,
    /// Count of REACTIVE legato-path triggers (countdown armed at note-on /
    /// note-off fallback) since the counter was last reset. Document playback
    /// must keep this at 0 — every transition arrives via
    /// [`legato_prefire_line`](Self::legato_prefire_line) instead.
    reactive_legato_fires: u64,
    /// Engine frame of the most recent LIVE note-on — the live allocator's
    /// chord-window ("simultaneity") gate clock.
    live_last_onset: Option<u64>,

    /// True when the source pack is a percussion / drum-kit library
    /// (`category` ~ "drum-kit", or a percussion `instrument`). Percussion
    /// engines always play zones at natural pitch — the incoming note is a
    /// trigger selector, never a transpose.
    percussion: bool,
    /// True when every zone shares a single `key_min` (single-articulation
    /// drum: kick, tom). Such an engine fires on *any* routed note, so the
    /// preset's `note_routing` is the sole authority for which note plays it.
    single_attack_key: bool,
    /// When set, this engine fires only zones whose `articulation` matches
    /// (case-insensitive), ignoring the incoming key. Lets one drum pack be
    /// addressed as several performance pieces (hats Closed vs Open, …).
    pinned_articulation: Option<String>,
    /// Per-trigger articulation override, set for the duration of a single
    /// `note_on_articulated` call so one shared engine can serve many routed
    /// notes (each route picks the articulation). Takes precedence over
    /// `pinned_articulation`; always cleared after the trigger.
    trigger_articulation: Option<String>,
    /// Engine-wide choke group (pre-hashed). When set, voices join this group
    /// so they can be silenced by a later choking hit. `None` = polyphonic
    /// (kick/snare/toms ring freely).
    engine_choke_group: Option<u64>,
    /// Which articulations actually *trigger* the choke (silence the group).
    /// Empty + a set group = monophonic: every hit chokes (hi-hats — any hit,
    /// incl. pedal, cuts the ringing hat). Non-empty = only these articulations
    /// choke (cymbals: only "Choke" stops the ringing crash; crashes overlap).
    /// Lowercased for case-insensitive matching.
    engine_choke_on: Vec<String>,

    /// The ACTIVE line's CC1 value [0–127] — a mirror of
    /// `lines[cur_line].cc1`, refreshed by `set_active_line` so the many
    /// internal readers stay line-correct without threading a line id
    /// through every helper. Writes go through both (see `cc_line`).
    cc1: u8,
    /// The ACTIVE line's CC2 value — same mirror discipline as `cc1`.
    cc2: u8,
    /// Current CC58 value, selects articulation / legato mode.
    cc58: u8,
    /// Pending CC58 keyswitch GROUP (index into `spec.keyswitch.notes`) whose
    /// concrete articulation is resolved by NOTE velocity at note-on — for CC58
    /// bands that name a velocity-split group rather than one articulation
    /// (Trills → HTrills/WTrills, Marcato overlay variants → Marcato). `None`
    /// when the last CC58 selected a single articulation directly. Mirrors how
    /// the velocity-sensitive keyswitch NOTES resolve (`try_keyswitch`), giving
    /// CC58 the same per-note-velocity behaviour as playing the KS note.
    pending_cc58_group: Option<usize>,
    /// Latched-CC articulation selector, resolved once from the spec
    /// (`selector uacc` → CC number + code → articulation-id map). The CC's
    /// latched VALUE selects the articulation for subsequent notes, exactly
    /// like a keyswitch latch. `None` = not configured (default) — behaviour
    /// is bit-identical to before the selector existed.
    // r[impl signal.sampling.articulation.select]
    latched_cc_selector: Option<crate::spec::LatchedCcSelector>,
    /// Master output volume [0.0, 1.0] from CC11 (CSS "Volume"). 1.0 = full.
    cc11_volume: f32,
    /// Portamento glide volume [0.0, 1.0] from CC5 (CSS "Portamento Volume").
    cc5_porta_volume: f32,
    cc_values: [u8; 128],
    channel_aftertouch: u8,
    poly_aftertouch: [u8; 128],
    /// CC64 (sustain pedal) held state.
    cc64_held: bool,
    /// Raw CC64 value. 1..63 is treated as half-pedal damping, >=64 as full hold.
    cc64_value: u8,
    /// Lightly-smoothed recent note-on velocity — a proxy for how hard the
    /// player is currently playing. Drives velocity-scaling of note-independent
    /// ambience (pedal / mechanical noise) so soft passages get soft noise.
    recent_velocity: u8,
    /// Per-note strike (note-on) velocity, kept until the note releases. The
    /// release tail's dynamic + gain follow this, NOT the note-off velocity —
    /// controllers routinely send 0/64 "no info" on note-off, and a soft note
    /// must get a soft release or the key-up/mechanical noise drowns it out.
    note_strike_vel: [u8; 128],
    /// Release/key-up-noise level *relative to the note body* that played
    /// (linear; default -10 dB, matching Keyscape). Applied to the body's
    /// measured peak at note-off so the release always sits under the note.
    release_gain: f32,
    /// Mechanical pedal-noise level (linear; default -20 dB). Absolute, scaled
    /// by recent playing velocity.
    mech_noise_gain: f32,
    /// Felt / sustain pedal-noise level (linear; default -20 dB).
    pedal_noise_gain: f32,
    /// While the sustain pedal is held, libraries with distinct pedal-down
    /// body samples (e.g. Keyscape `lacrped`) swap `articulation` to the
    /// pedal variant. The original (no-pedal) ID lives here so we can snap
    /// back on pedal-up. `None` when the active articulation isn't a pedal
    /// swap — i.e. normal play.
    no_pedal_articulation: Option<String>,
    /// Whether Con Sordino mode is active. When true, `articulation` holds the
    /// sordino artic ID (e.g. `"SordVibsus"`); switching modes remaps it.
    con_sordino: bool,
    /// Whether legato processing is enabled. When false, every note-on triggers
    /// a fresh sustain even if notes are held (equivalent to "Legato Off" in CSS).
    legato_enabled: bool,
    /// True = expressive mode (3 zones, 333/250/100ms), false = low-latency (2 zones, 100/150ms).
    /// What CC58 / keyswitches REQUESTED; [`PlayMode`] decides whether the
    /// request is honored — StrictLive plays low-latency no matter what.
    legato_expressive: bool,
    /// Automatic mode policy — see [`PlayMode`]. Default: StrictLive.
    play_mode: PlayMode,

    /// CSS "Releases on/off" (`$4p5kj`, spec §6). Default **off** — a released
    /// held note fades naturally with no recorded release-tail sample. When
    /// enabled the sustain articulation's `release_artic` fires on note-off.
    releases_enabled: bool,
    /// When `Some((delay_frames, fade_frames))`, sustain-layer voices spawned
    /// during this dispatch start muted and fade in underneath — the CSS
    /// legato handoff (spec §2.1 step 7 / `CSS_W`). Set for the duration of the
    /// legato sustain spawn and cleared immediately after. `None` = normal
    /// attack (first note / polyphonic sustain).
    /// CSS two-stage destination swell params `(delay, stage1_run, stage1_denom,
    /// stage2)` in frames (KSP §3). A single-stage fade is `(delay, f, f, 0)`.
    sustain_fade_in: Option<(usize, usize, usize, usize)>,
    /// When true, sustain-layer voices spawned during this dispatch carry the
    /// −6 dB `$3tsb0` legato makeup (`LegatoEngineSpec::sustain_trim_db`) — the KSP
    /// `change_vol(%grhcg,$3tsb0*100)` on the held legato tone. Set only around
    /// the `trigger_zoned_sustain` inside a legato handoff so a legato-connected
    /// note ends up ~6 dB below a fresh first note. `false` = first note /
    /// polyphonic sustain (full `PerformanceSpec::sustain_makeup_db`).
    legato_sustain: bool,
    /// The `$3tsb0` −6 dB connected-sustain TRIM (+ its bloom) is CONDITIONAL:
    /// CSS applies it only to connected notes in velocity zones 1-2, and EXEMPTS
    /// zone-3 hard attacks (`$x0jlu`=0) and the first note of a phrase (KSP §7.2).
    /// Set alongside `legato_sustain` but only when the attack velocity is in
    /// zone 1-2; the crossfade-fill still applies to every connected note.
    legato_trim: bool,
    /// Extra dB dip on the connected note from the attack-transient envelope
    /// (`css_attack_transient_dip_db`, KSP §7.3) — 0 unless the note falls within
    /// 250 ms of the previous onset. Set alongside `legato_trim`.
    legato_attack_dip_db: f32,
    /// Portamento micro-glide `(start_cents, frames)` for the incoming legato
    /// voices (CSS `$ma0b1`/`$1mwwo`/`$ruv02`, KSP §3.2) — the arriving note
    /// starts detuned toward the departed note and scoops to true pitch. Set
    /// around the legato spawn, applied in `spawn_zone_voice_at`.
    legato_glide: Option<(f32, usize)>,
    /// CSS `%jcxqm` two-stage transition fade-in `(stage1_run, stage1_denom,
    /// stage2)` in frames — the transition voice EMERGES via the swell, not a
    /// 25 ms declick (the NVLeg sample has no silent head; it starts ~-16 dB, so
    /// a fast declick reads as an artificial onset). Set around the transition
    /// spawn; the `start_hold` provides the silent pre-roll before it.
    transition_fade: Option<(usize, usize, usize)>,

    /// Notes currently held down: MIDI note → velocity. Shared across lines
    /// (keys are physical); per-line press order lives in `LegatoLine::order`.
    held_notes: HashMap<u8, u8>,
    /// Per-note onset frame (engine time) — drives the decoded release-sample
    /// held-time gain curve (`%ru5pa`, §11: −6 dB at 10 ms held → 0 dB ≥ 1 s).
    note_on_frame: HashMap<u8, u64>,
    /// Notes for which `trigger_short`'s body voice actually spawned
    /// (i.e. the body sample was decoded and resolve succeeded). Used to
    /// gate the release-tail voice on note-off: if the body never sounded
    /// we don't want to play just the mechanical release click in
    /// isolation, which sounds buggy.
    body_voiced: std::collections::HashSet<u8>,
    /// Note-off velocities captured while the sustain pedal is held.
    deferred_note_off_velocities: HashMap<u8, u8>,

    /// Running render position in frames since construction — advanced by
    /// every `render`/`render_multi` call. Document-mode schedulers and the
    /// legato fire log use it as a stable per-engine clock.
    frames_rendered: u64,
    /// When true, every legato transition firing is appended to
    /// `legato_fire_log` (up to `LEGATO_FIRE_LOG_CAP`). Off by default —
    /// enabled by tests and offline document renders only.
    legato_fire_log_enabled: bool,
    /// Playback-emitted marker log (see [`EmittedMarker`]); enabled by
    /// offline document renders / tests, drained per rendered block.
    emitted_marker_log_enabled: bool,
    emitted_markers: Vec<EmittedMarker>,
    /// Recorded legato transition firings (see [`LegatoFireEvent`]).
    legato_fire_log: Vec<LegatoFireEvent>,
    /// Predicted heard-arrival frame of the most recent transition spawn
    /// (see [`LegatoFireEvent::arrival`]); written by
    /// `spawn_transition_voice`, harvested by `fire_legato_with_lead` into
    /// the fire log entry.
    last_arrival_prediction: u64,
    /// When true, every voice spawn / note-off / transition is appended to
    /// `trace` — the structured render trace (see [`trace`]). Off by default.
    trace_enabled: bool,
    /// Pure sample-playback mode: strips the naturalism layers so a held note
    /// is ONE looped sample at a straight gain (no CC1 multi-layer dynamic
    /// crossfade, no ENV_FLEX amp envelope, no legato −6 dB sustain trim +
    /// slow bloom). CC1 still SELECTS the dynamic layer. A clean
    /// Kontakt-CSS-correct baseline to build naturalism back onto. Off by
    /// default (the full engine behaviour).
    pure_playback: bool,
    /// Structured render trace: which files played, when, loop points, gains,
    /// transitions. Populated only while `trace_enabled`. Behind a `RefCell`
    /// so the `&self` voice-resolution path can record spawns/misses.
    trace: RefCell<RenderTrace>,
    /// Monotonic voice id source for trace correlation.
    next_voice_id: Cell<u64>,
    /// Trace ids of voices whose VoiceSpawn was recorded and whose VoiceEnd
    /// hasn't fired yet (the per-block end sweep drains this).
    traced_alive: RefCell<std::collections::BTreeSet<u64>>,
    /// Scratch for the end sweep (avoids per-block allocation).
    trace_alive_scratch: RefCell<Vec<u64>>,

    /// Con Sordino bus-level filter (placeholder lowpass — see filter.rs).
    sord_filter: BiquadFilter,

    /// Fade duration (frames) applied to old sustain when legato fires.
    legato_fade_frames: usize,
    /// Ramp length (frames) for CC1 gain updates.
    cc1_ramp_frames: usize,
    /// One-shot ramp override for the NEXT `update_sustain_gains` — set by the
    /// CC handlers so a CC2 change re-levels over its decoded 1000 ms lag while
    /// CC1 keeps the 120 ms lag. `None` = the CC1 default.
    next_cc_ramp: Option<usize>,
    /// Velocity of the most recent note-on — drives the fresh-sustain attack
    /// scaling (velocity zone × CC1, param-test S13 calibration).
    last_velocity: u8,
    /// Default release duration (frames) for sustain voices.
    release_frames: usize,
    /// Attack envelope (frames) ramped in on sustain onset. 0 = the sample's
    /// natural attack (CSS attack parameter; user-adjustable).
    attack_frames: usize,
    /// Unison playback: `(voices, detune cents, stereo width)`. Every zone
    /// trigger spawns `voices` copies spread symmetrically across ±detune/2
    /// cents and panned by `width`, level-compensated 1/√n. `(1, _, _)` = off.
    unison: (u8, f32, f32),

    /// Round-robin counter for zone mode. Increments on every zoned note-on
    /// regardless of (note, velocity) so RR cycling within a matching zone set
    /// behaves as expected when the same key is repeatedly struck.
    zone_rr_counter: usize,
    zone_rr_random_state: u64,
    /// Last-used RR slot per (trigger, note, velocity), keyed by a packed
    /// integer so note-on allocates nothing (was a `format!`-built String key).
    zone_rr_last_slots: HashMap<u64, u32>,
    /// Test/render override: when `Some(slot)`, every RR-bearing trigger (shorts,
    /// legato transitions, releases) is pinned to this slot instead of cycling /
    /// randomising. Used by the A/B null harness to sweep round-robins and align
    /// our RR ordering with a deterministic CSS render (CC59 cycle). `None` =
    /// normal CC59 / cycle / random behaviour.
    forced_rr: Option<u32>,
    /// Document-scheduler arrival alignment for the CURRENT dispatch: wall
    /// frames from now to the note's grid tick (the schedule's pre-roll
    /// lead). While `Some`, every attack voice spawned is held back by
    /// `lead − its zone's measured heard-arrival` (see
    /// `ZoneSpec::arrival_ms`) so the heard arrival lands exactly ON the
    /// tick — per round-robin, per mic, per dynamic layer. `None` = live /
    /// legacy dispatch (voices start immediately).
    spawn_align_lead: Option<u64>,
    /// Transition-spawn arrival override (ms of sample time) for the
    /// diagnostic semantics sweep — set by `spawn_transition_voice` so the
    /// alignment hold uses the SAME re-interpreted arrival the prefire lead
    /// was computed from. `None` in normal operation.
    spawn_arrival_override_ms: Option<f32>,

    /// Reusable scratch for the zoned trigger path so note-on doesn't allocate.
    /// Drained/refilled each note-on via `mem::take` + restore.
    zone_indices_scratch: Vec<usize>,
    zone_choked_scratch: Vec<u64>,
    zone_capped_scratch: Vec<(u64, usize)>,

    /// Mic ids in spec declaration order. Empty when the library has no
    /// `mics` array. Used to map a zone's `mic` string to a stable index
    /// the renderer can splay across per-mic buffers.
    mic_ids: Vec<String>,

    // Miss telemetry — interior-mutable so `make_voice`/`record_*` are `&self`.
    cache_misses: Cell<usize>,
    sample_misses: Cell<usize>,
    recent_cache_misses: RefCell<VecDeque<String>>,
    recent_sample_misses: RefCell<VecDeque<String>>,
}

impl SampleEngine {
    /// Create a new engine for the given patch, sample rate, section, and mic.
    ///
    /// `section_id` — one of the spec's `[[section]]` IDs (e.g. `"1v"`).
    /// `mic_id`     — one of the spec's `[[mic]]` IDs (e.g. `"Mix"`).
    pub fn new(
        patch: PlayerPatch,
        sample_rate: u32,
        section_id: impl Into<String>,
        mic_id: impl Into<String>,
    ) -> Self {
        let section = section_id.into();
        let mic = mic_id.into();

        // Default to a playable articulation. Sustain is preferred; if the
        // spec has no Sustain (e.g. Keyscape Rhodes — all samples are
        // OneShot strikes plus Release tails), fall back to any kind that
        // can be triggered directly by note-on. Release/Legato are NOT
        // triggered by note-on (they fire on note-off / from another voice),
        // so picking one of those as the default produces silence.
        // Skip articulations whose IDs name mechanical-action / key-release
        // layers — those are companions, not the playable body. Without
        // this, Keyscape Classic ships `clrmchr03` (mechanical) alphabetically
        // before `clrr10` (the actual Rhodes body) and the picker locks onto
        // the mechanical noise.
        let articulation = default_articulation(&patch.spec).unwrap_or_default();

        let legato_fade_frames =
            ms_to_frames(patch.spec.legato_cfg().transition_fade_ms, sample_rate);
        let cc1_ramp_frames = ms_to_frames(CC1_RAMP_MS, sample_rate);
        // Library-authored defaults (CSS: 198 ms arco-attack bloom / 400 ms
        // note-off overlap) — callers can still override via
        // `set_attack_frames` / `set_release_frames`.
        let release_frames =
            ms_to_frames(patch.spec.performance.release_ms.unwrap_or(RELEASE_MS), sample_rate);
        let spec_attack_frames = patch
            .spec
            .performance
            .attack_ms
            .map(|ms| ms_to_frames(ms, sample_rate))
            .unwrap_or(0);
        let cache = if let Some(pack) = patch.pack.clone() {
            SampleCache::with_pack(pack)
        } else {
            SampleCache::with_prepared(patch.prepared_cache_dir.as_deref())
        };

        let mic_ids: Vec<String> = patch.spec.mics.iter().map(|m| m.id.clone()).collect();

        let percussion = spec_is_percussion(&patch.spec);
        // Single-articulation drum: every zone sits on one key (kick, tom).
        // Such a pack should fire on whatever note the preset routes to it.
        let single_attack_key = {
            let mut iter = patch.spec.zones.iter().map(|z| z.key_min);
            match iter.next() {
                Some(first) => iter.all(|k| k == first),
                None => false,
            }
        };

        // Resolve keyswitch note names → MIDI numbers once (C0 = 12).
        let keyswitch_notes: HashMap<u8, usize> = patch
            .spec
            .keyswitch
            .as_ref()
            .map(|ks| {
                ks.notes
                    .iter()
                    .enumerate()
                    .filter_map(|(i, kn)| {
                        crate::midi::note_name_to_midi(&kn.note)
                            .ok()
                            .map(|n| (n, i))
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Resolve the latched-CC articulation selector once (control-path
        // lookups stay allocation-free at runtime).
        let latched_cc_selector = patch.spec.latched_cc_selector();

        Self {
            patch,
            cache,
            voices: VoicePool::new(),
            rr: RefCell::new(RrCounters::new()),
            sample_rate,
            section,
            articulation,
            mic,
            solo_mic: None,
            keyswitch_notes,
            play_direction: "up".to_string(),
            lines: (0..MAX_LINES).map(|_| LegatoLine::default()).collect(),
            cur_line: 0,
            reactive_legato_fires: 0,
            live_last_onset: None,
            percussion,
            single_attack_key,
            pinned_articulation: None,
            trigger_articulation: None,
            engine_choke_group: None,
            engine_choke_on: Vec::new(),
            cc1: 64,
            cc2: 0,
            cc58: 0,
            pending_cc58_group: None,
            latched_cc_selector,
            cc11_volume: 1.0,
            cc5_porta_volume: 1.0,
            cc_values: [0; 128],
            channel_aftertouch: 0,
            poly_aftertouch: [0; 128],
            cc64_held: false,
            cc64_value: 0,
            recent_velocity: 90,
            note_strike_vel: [0; 128],
            // Keyscape defaults: release -10 dB, mechanical -20 dB, pedal -20 dB.
            release_gain: db_to_gain(-10.0),
            mech_noise_gain: db_to_gain(-20.0),
            pedal_noise_gain: db_to_gain(-20.0),
            no_pedal_articulation: None,
            con_sordino: false,
            legato_enabled: true,
            legato_expressive: false, // default: low-latency mode
            play_mode: PlayMode::StrictLive,
            // CSS "Releases" — REAL persistent value `$4p5kj = 1` (ON). The
            // recorded release tail plays on note-off (spec §6 quoted the
            // compiled default 0; the shipped preset ships it enabled).
            releases_enabled: true,
            sustain_fade_in: None,
            legato_sustain: false,
            legato_trim: false,
            legato_attack_dip_db: 0.0,
            legato_glide: None,
            transition_fade: None,
            sord_filter: BiquadFilter::lowpass(filter::SORD_FC, filter::SORD_Q, sample_rate),
            // Pre-size note-keyed maps to the full MIDI range so note-on never
            // reallocates them on the audio thread.
            held_notes: HashMap::with_capacity(128),
            note_on_frame: HashMap::with_capacity(128),
            body_voiced: std::collections::HashSet::with_capacity(128),
            deferred_note_off_velocities: HashMap::with_capacity(128),
            frames_rendered: 0,
            legato_fire_log_enabled: false,
            emitted_marker_log_enabled: false,
            emitted_markers: Vec::new(),
            legato_fire_log: Vec::new(),
            last_arrival_prediction: 0,
            trace_enabled: false,
            pure_playback: false,
            trace: RefCell::new(RenderTrace::default()),
            next_voice_id: Cell::new(0),
            traced_alive: RefCell::new(std::collections::BTreeSet::new()),
            trace_alive_scratch: RefCell::new(Vec::new()),
            legato_fade_frames,
            cc1_ramp_frames,
            next_cc_ramp: None,
            last_velocity: 90,
            release_frames,
            attack_frames: spec_attack_frames,
            unison: (1, 0.0, 0.0),
            zone_rr_counter: 0,
            zone_rr_random_state: 0x9e37_79b9_7f4a_7c15,
            zone_rr_last_slots: HashMap::with_capacity(128),
            forced_rr: None,
            spawn_align_lead: None,
            spawn_arrival_override_ms: None,
            zone_indices_scratch: Vec::with_capacity(32),
            zone_choked_scratch: Vec::with_capacity(16),
            zone_capped_scratch: Vec::with_capacity(16),
            mic_ids,
            cache_misses: Cell::new(0),
            sample_misses: Cell::new(0),
            recent_cache_misses: RefCell::new(VecDeque::with_capacity(RECENT_MISS_LIMIT)),
            recent_sample_misses: RefCell::new(VecDeque::with_capacity(RECENT_MISS_LIMIT)),
        }
    }

    /// Mic ids in declaration order, parallel to the multi-buffer slice
    /// expected by [`render_multi`]. Empty when the library has no `mics`.
    pub fn mic_ids(&self) -> &[String] {
        &self.mic_ids
    }

    /// Resolve a mic id (string) to its index in `mic_ids()`.
    /// `""` or `"default"` resolves to 0 when at least one mic exists.
    pub fn mic_index_for(&self, mic_id: &str) -> Option<u8> {
        if self.mic_ids.is_empty() {
            return None;
        }
        if mic_id.is_empty() || mic_id == "default" {
            return Some(0);
        }
        self.mic_ids
            .iter()
            .position(|m| m == mic_id)
            .map(|i| i as u8)
    }

    /// Cheap clone of the underlying sample cache. Hand this to a
    /// background thread to populate the cache without blocking the
    /// audio thread.
    pub fn cache_handle(&self) -> SampleCache {
        self.cache.clone_handle()
    }

    /// Owned copy of every sample path the patch references — lets a
    /// background-preloader thread iterate without borrowing the engine.
    pub fn sample_paths_owned(&self) -> Vec<std::path::PathBuf> {
        self.patch.sample_paths().cloned().collect()
    }

    /// Sample paths ordered for "middle-out" preload — closest to
    /// `center` (middle C if you pass 60) first, extremes last. Grooves +
    /// wavetables are appended at the end. Use this when feeding the
    /// background preloader so the most-played range becomes audible first.
    pub fn sample_paths_centered(&self, center: u8) -> Vec<std::path::PathBuf> {
        self.patch.sample_paths_centered(center)
    }

    /// Coverage-first preload order — see
    /// [`PlayerPatch::sample_paths_playable`].
    pub fn sample_paths_playable(&self, center: u8) -> Vec<std::path::PathBuf> {
        self.patch.sample_paths_playable(center)
    }

    /// How many of `total_samples()` are currently decoded into the cache.
    pub fn loaded_sample_count(&self) -> usize {
        self.cache.len()
    }

    pub fn loaded_sample_bytes(&self) -> usize {
        self.cache.bytes()
    }

    pub fn evict_cache_until_under_budget(&self, budget_bytes: usize) -> EvictStats {
        self.cache.evict_until_under_budget(budget_bytes)
    }

    /// Total number of samples the patch references (loaded or not).
    pub fn total_sample_count(&self) -> usize {
        self.patch.total_samples()
    }

    /// Read-only access to the engine's underlying [`PlayerPatch`] — used
    /// by callers that need to inspect the loaded library spec (tags,
    /// instrument, sections) for things like preload prioritization.
    pub fn patch(&self) -> &crate::PlayerPatch {
        &self.patch
    }

    /// Decode all indexed samples into the cache before live playback.
    pub fn preload_samples(&mut self) -> PreloadStats {
        let start = std::time::Instant::now();
        let total = self.patch.total_samples();
        let stats = self
            .cache
            .preload(self.patch.sample_paths().map(|p| p.as_path()));
        tracing::info!(
            "signal-sampler: preloaded {}/{} samples ({:.1} MiB PCM) in {:.2}s",
            self.cache.len(),
            total,
            stats.bytes as f64 / 1024.0 / 1024.0,
            start.elapsed().as_secs_f64()
        );
        stats
    }

    /// Decode the primary sample needed for a pending note-on on the caller's
    /// thread. This keeps the audio callback non-blocking while avoiding the
    /// "first note is silent until background preload reaches it" failure mode.
    pub fn warm_note_samples(&self, note: u8, velocity: u8) -> PreloadStats {
        let path = if self.patch.is_zoned() {
            self.patch.resolve_zone(note, velocity, 0).map(|z| z.path)
        } else {
            let dynamic = self.short_note_dynamic(velocity);
            self.patch
                .resolve(&crate::sample_map::SampleQuery {
                    section_id: &self.section,
                    articulation_id: &self.articulation,
                    mic_id: &self.mic,
                    dynamic: &dynamic,
                    target_note: note,
                    direction: "",
                    rr: 0,
                })
                .map(|(path, _)| path)
        };
        let Some(path) = path else {
            return PreloadStats {
                failed: 1,
                ..PreloadStats::default()
            };
        };
        if self.cache.get_loaded(&path).is_some() {
            return PreloadStats::default();
        }
        match self.cache.get(&path) {
            Ok(data) => PreloadStats {
                loaded: 1,
                bytes: data.decoded_bytes(),
                failed: 0,
            },
            Err(_) => PreloadStats {
                failed: 1,
                ..PreloadStats::default()
            },
        }
    }

    // ── Configuration ─────────────────────────────────────────────────────────

    /// Switch to a different section. Resets RR counters.
    pub fn set_section(&mut self, section_id: impl Into<String>) {
        self.section = section_id.into();
        self.rr.borrow_mut().reset();
        self.zone_rr_counter = 0;
        self.zone_rr_last_slots.clear();
    }

    /// Switch to a different microphone position.
    pub fn set_mic(&mut self, mic_id: impl Into<String>) {
        self.mic = mic_id.into();
    }

    /// Restrict zoned playback to a single mic. `Some("Mix")` makes only zones
    /// tagged with that mic fire — the fix for multi-mic libraries (like CSS)
    /// that ship every mic in one zone set but declare no `mics` block, so
    /// without this every mic folds to bus 0 and sounds at once. `None` (the
    /// default) keeps the play-all-mics behaviour for multi-mic mixing.
    pub fn set_solo_mic(&mut self, mic_id: Option<String>) {
        self.solo_mic = mic_id.filter(|m| !m.is_empty());
    }

    /// Attack envelope length in frames for sustained notes (CSS attack
    /// parameter). 0 = the sample's natural attack.
    pub fn set_attack_frames(&mut self, frames: usize) {
        self.attack_frames = frames;
    }

    /// Release fade length in frames on note-off (CSS release parameter); the
    /// recorded release sample plays underneath.
    pub fn set_release_frames(&mut self, frames: usize) {
        self.release_frames = frames;
    }

    /// Unison playback for zone triggers: `voices` copies per note, spread
    /// symmetrically across ±`detune_cents`/2 and panned by `width` (0..1),
    /// level-compensated 1/√n. `voices <= 1` disables.
    pub fn set_unison(&mut self, voices: u8, detune_cents: f32, width: f32) {
        self.unison = (
            voices.clamp(1, 8),
            detune_cents.max(0.0),
            width.clamp(0.0, 1.0),
        );
    }

    /// Test/render override: pin every RR-bearing trigger (shorts, legato,
    /// releases) to a specific round-robin slot, or `None` to restore normal
    /// CC59 / cycle / random behaviour. The A/B null harness uses this to sweep
    /// round-robins and align our RR ordering with a deterministic CSS render.
    pub fn set_forced_rr(&mut self, slot: Option<u32>) {
        self.forced_rr = slot;
    }

    /// Explicitly set the legato mode (document mode forces
    /// `enabled = true, expressive = true` — the full-authenticity mode —
    /// regardless of the CC58 stream, per the document-mode design).
    ///
    /// As the explicit HOST-level override, this also sets the [`PlayMode`]
    /// policy: `expressive = true` ⇒ Lookahead (the caller vouches that
    /// latency is acceptable — document renders and the CSS-parity
    /// harnesses), `false` ⇒ StrictLive. CC58 / keyswitch "expressive"
    /// requests do NOT reach here — they only set the preference flag, which
    /// StrictLive ignores.
    pub fn set_legato_mode(&mut self, enabled: bool, expressive: bool) {
        self.legato_enabled = enabled;
        self.legato_expressive = expressive;
        self.play_mode = if expressive {
            PlayMode::Lookahead
        } else {
            PlayMode::StrictLive
        };
    }

    /// Explicitly set the play-mode policy — see [`PlayMode`].
    pub fn set_play_mode(&mut self, mode: PlayMode) {
        self.play_mode = mode;
    }

    /// Current play-mode policy.
    pub fn play_mode(&self) -> PlayMode {
        self.play_mode
    }

    /// CSS "Releases on/off" (`$4p5kj`, spec §6). Default **off**. When on, a
    /// released held note plays its articulation's recorded release tail.
    pub fn set_releases_enabled(&mut self, enabled: bool) {
        self.releases_enabled = enabled;
    }

    /// Whether the recorded release tail fires on note-off (see
    /// [`set_releases_enabled`](Self::set_releases_enabled)).
    pub fn releases_enabled(&self) -> bool {
        self.releases_enabled
    }

    /// Added latency this engine imposes, in frames: 0 in BOTH modes.
    /// StrictLive plays now; Lookahead anticipates (transitions are prefired
    /// from the schedule) rather than delaying — the tradeoff moves from
    /// latency to transition authenticity, per the design doc.
    pub fn latency_frames(&self) -> usize {
        0
    }

    /// Enable/disable the legato transition fire log (tests / offline
    /// document renders). Enabling clears any previous entries and
    /// pre-allocates the capped log so the audio thread never allocates.
    /// Enabling also resets [`reactive_legato_fires`](Self::reactive_legato_fires)
    /// so a document render observes only its own playback.
    pub fn set_legato_fire_log_enabled(&mut self, enabled: bool) {
        self.legato_fire_log_enabled = enabled;
        self.legato_fire_log.clear();
        if enabled {
            self.legato_fire_log.reserve(LEGATO_FIRE_LOG_CAP);
            self.reactive_legato_fires = 0;
        } else {
            self.legato_fire_log.shrink_to_fit();
        }
    }

    /// Recorded legato transition firings since the log was enabled.
    pub fn legato_fire_log(&self) -> &[LegatoFireEvent] {
        &self.legato_fire_log
    }

    /// Enable/disable the playback-emitted marker log. Enabling clears and
    /// pre-allocates the capped buffer (the audio thread never allocates).
    pub fn set_emitted_marker_log_enabled(&mut self, enabled: bool) {
        self.emitted_marker_log_enabled = enabled;
        self.emitted_markers.clear();
        if enabled {
            self.emitted_markers.reserve(EMITTED_MARKER_LOG_CAP);
        } else {
            self.emitted_markers.shrink_to_fit();
        }
    }

    /// Markers EMITTED BY PLAYBACK since the log was enabled — each one a
    /// real playhead crossing at a real output frame.
    pub fn emitted_markers(&self) -> &[EmittedMarker] {
        &self.emitted_markers
    }

    /// Drain per-voice emissions into the log (called after each rendered
    /// block; one Option read per voice, capped push, no alloc).
    fn drain_emitted_markers(&mut self) {
        if !self.emitted_marker_log_enabled {
            return;
        }
        for v in self.voices.voices_mut() {
            if let Some(frame) = v.take_emitted_arrival() {
                if self.emitted_markers.len() < EMITTED_MARKER_LOG_CAP {
                    self.emitted_markers.push(EmittedMarker {
                        frame,
                        note: v.note,
                        line: v.line,
                    });
                }
            }
        }
    }

    /// Enable/disable the structured render trace ([`RenderTrace`]) — which
    /// files play, when, loop points, gains, transitions. Clears on enable.
    /// Per-note solo filter for offline analysis renders — only voices whose
    /// `note` is in the set are audible; muted voices still advance so legato
    /// timing is identical. `None` = full mix.
    pub fn set_solo_notes(&mut self, notes: Option<std::collections::BTreeSet<u8>>) {
        self.voices.set_solo_notes(notes);
    }

    /// Enable/disable pure sample-playback mode (see [`pure_playback`]).
    pub fn set_pure_playback(&mut self, on: bool) {
        self.pure_playback = on;
    }

    pub fn set_trace_enabled(&mut self, enabled: bool) {
        self.trace_enabled = enabled;
        let mut trace = self.trace.borrow_mut();
        trace.events.clear();
        if !enabled {
            trace.events.shrink_to_fit();
        }
    }

    /// The render trace recorded since it was enabled.
    pub fn render_trace(&self) -> RenderTrace {
        self.trace.borrow().clone()
    }

    /// Release/key-up-noise level relative to the note body, in dB (0 = as loud
    /// as the note; Keyscape default -10). Clamped to a sane range.
    pub fn set_release_gain_db(&mut self, db: f32) {
        self.release_gain = db_to_gain(db.clamp(-60.0, 0.0));
    }
    /// Mechanical pedal-noise level, dB (Keyscape default -20).
    pub fn set_mech_noise_gain_db(&mut self, db: f32) {
        self.mech_noise_gain = db_to_gain(db.clamp(-60.0, 6.0));
    }
    /// Felt / sustain pedal-noise level, dB (Keyscape default -20).
    pub fn set_pedal_noise_gain_db(&mut self, db: f32) {
        self.pedal_noise_gain = db_to_gain(db.clamp(-60.0, 6.0));
    }
    /// Current noise levels as dB (release-relative, mechanical, pedal).
    pub fn noise_gains_db(&self) -> (f32, f32, f32) {
        (
            gain_to_db(self.release_gain),
            gain_to_db(self.mech_noise_gain),
            gain_to_db(self.pedal_noise_gain),
        )
    }

    /// Record one trace event on the active line (no-op unless tracing is on).
    /// Takes `&self` — the trace sits behind a `RefCell` so the `&self`
    /// voice-resolution path (`make_voice`) can record spawns and misses.
    fn trace_push(&self, kind: TraceKind) {
        if self.trace_enabled {
            if let TraceKind::VoiceSpawn(v) = &kind {
                self.traced_alive.borrow_mut().insert(v.voice_id);
            }
            self.trace.borrow_mut().events.push(TraceEvent {
                frame: self.frames_rendered,
                line: self.cur_line as u8,
                kind,
            });
        }
    }

    /// Per-block sweep: emit `TraceKind::VoiceEnd` for every traced voice
    /// that is no longer alive in the pool (played out, faded, or stolen).
    /// Block-accurate timestamps; no-op unless tracing is on.
    fn sweep_traced_voice_ends(&self) {
        if !self.trace_enabled || self.traced_alive.borrow().is_empty() {
            return;
        }
        let mut scratch = self.trace_alive_scratch.borrow_mut();
        self.voices.alive_trace_ids_into(&mut scratch);
        let ended: Vec<u64> = self
            .traced_alive
            .borrow()
            .iter()
            .copied()
            .filter(|id| !scratch.contains(id))
            .collect();
        if ended.is_empty() {
            return;
        }
        let mut alive = self.traced_alive.borrow_mut();
        for id in ended {
            alive.remove(&id);
            self.trace.borrow_mut().events.push(TraceEvent {
                frame: self.frames_rendered,
                line: self.cur_line as u8,
                kind: TraceKind::VoiceEnd { voice_id: id },
            });
        }
    }

    /// Next monotonic voice id for the trace (wraps the `Cell` so `&self` paths
    /// can allocate ids).
    fn next_trace_voice_id(&self) -> u64 {
        let id = self.next_voice_id.get();
        self.next_voice_id.set(id + 1);
        id
    }

    /// How many REACTIVE legato-path triggers (note-on countdown / note-off
    /// fallback) occurred since the fire log was last enabled. Document
    /// playback schedules every transition via prefire, so this must read 0
    /// after a document render — any other value means the annotator missed
    /// an edge and the engine degraded to live-mode timing for it.
    pub fn reactive_legato_fires(&self) -> u64 {
        self.reactive_legato_fires
    }

    // ── Render ────────────────────────────────────────────────────────────────

    /// Mix all active voices into `output` (interleaved stereo, += accumulates).
    ///
    /// Also advances the legato countdown and fires legato samples when due.
    pub fn render(&mut self, output: &mut [f32]) {
        let block_frames = output.len() / 2;

        // Advance every line's legato countdown.
        self.advance_legato_countdowns(block_frames);

        self.voices.render(output);
        self.frames_rendered += block_frames as u64;
        self.drain_emitted_markers();
        self.sweep_traced_voice_ends();

        // CSS "Volume" (CC11) — master output level.
        if self.cc11_volume != 1.0 {
            for s in output.iter_mut() {
                *s *= self.cc11_volume;
            }
        }

        // Apply Con Sordino placeholder filter to the entire output bus.
        if self.con_sordino {
            self.sord_filter.process(output);
        }
    }

    /// Render voices into per-mic stereo buffers. `outputs.len()` should
    /// match `mic_ids().len()` (or be 1 for libraries without an explicit
    /// mics array). Voices route to their `mic_index`. Buffers are NOT
    /// zeroed here — the caller owns clearing if accumulation isn't
    /// desired. No allocation occurs.
    pub fn render_multi(&mut self, outputs: &mut [Vec<f32>]) {
        let block_frames = outputs.first().map(|b| b.len() / 2).unwrap_or(0);

        // Advance every line's legato countdown.
        self.advance_legato_countdowns(block_frames);

        self.voices.render_multi(outputs);
        self.frames_rendered += block_frames as u64;
        self.drain_emitted_markers();
        self.sweep_traced_voice_ends();

        // CSS "Volume" (CC11) — master output level, all mic buses.
        if self.cc11_volume != 1.0 {
            for buf in outputs.iter_mut() {
                for s in buf.iter_mut() {
                    *s *= self.cc11_volume;
                }
            }
        }

        // Con Sordino filter: apply only to mic 0 (sord is a section/bus
        // effect, not a per-mic effect). Multi-mic libraries rarely use
        // sord, but keep behavior stable for the single-mic case.
        if self.con_sordino {
            if let Some(buf) = outputs.first_mut() {
                self.sord_filter.process(buf);
            }
        }
    }

    /// [`render_multi`](Self::render_multi), split by articulation class into
    /// a flat (bus × mic) matrix: `outputs[bus * nmics + mic]`
    /// (`route_longs`/`route_shorts` are bus indices). Voice iteration order
    /// is identical, so routing both classes to the SAME bus reproduces
    /// `render_multi`'s buffers bit for bit — the default `all → main`
    /// mapping is a no-op by construction.
    ///
    /// CC11 volume scales every buffer (as in `render_multi`). The Con
    /// Sordino filter (stateful, bus-level) is applied to the Longs bus's
    /// mic 0 — sordino shapes sustained string bodies; split rendering with
    /// sordino engaged is therefore only sum-exact while shorts are silent.
    pub fn render_matrix(
        &mut self,
        outputs: &mut [Vec<f32>],
        nmics: usize,
        route_longs: usize,
        route_shorts: usize,
    ) {
        let block_frames = outputs.first().map(|b| b.len() / 2).unwrap_or(0);
        self.advance_legato_countdowns(block_frames);
        self.voices
            .render_matrix(outputs, nmics, [route_longs, route_shorts]);
        self.frames_rendered += block_frames as u64;

        if self.cc11_volume != 1.0 {
            for buf in outputs.iter_mut() {
                for s in buf.iter_mut() {
                    *s *= self.cc11_volume;
                }
            }
        }
        if self.con_sordino {
            if let Some(buf) = outputs.get_mut(route_longs * nmics) {
                self.sord_filter.process(buf);
            }
        }
    }

    /// Advance every line's reactive legato countdown by one block, firing
    /// transitions that elapse (at the head of the block, matching the
    /// pre-line engine's timing). Lines are visited in ascending [`LineId`]
    /// order — deterministic when several fire in the same block.
    fn advance_legato_countdowns(&mut self, block_frames: usize) {
        for li in 0..self.lines.len() {
            let fire = match &mut self.lines[li].state {
                LegatoState::Pending {
                    frames_remaining, ..
                } => {
                    if *frames_remaining <= block_frames {
                        true
                    } else {
                        *frames_remaining -= block_frames;
                        false
                    }
                }
                LegatoState::Idle => false,
            };
            if fire {
                if let LegatoState::Pending {
                    from_note,
                    to_note,
                    to_note_velocity,
                    portamento,
                    ioi_ms,
                    ..
                } = std::mem::replace(&mut self.lines[li].state, LegatoState::Idle)
                {
                    self.set_active_line(li);
                    self.fire_legato(from_note, to_note, to_note_velocity, portamento, ioi_ms);
                }
            }
        }
    }

}

// ── VoicePool extension ───────────────────────────────────────────────────────

impl VoicePool {
    /// Ramp all four sustain voice kinds to their new gains over `ramp_frames`.
    ///
    /// Called whenever CC1 or CC2 changes so the dynamic/vibrato blend updates
    /// smoothly without zipper noise.
    pub fn update_sustain_blend(
        &mut self,
        line: u8,
        nv_lo: f32,
        nv_hi: f32,
        vib_lo: f32,
        vib_hi: f32,
        ramp_frames: usize,
    ) {
        for v in self.voices_mut() {
            if v.line != line {
                continue;
            }
            match v.kind {
                VoiceKind::SustainNVLo => v.ramp_gain(nv_lo, ramp_frames),
                VoiceKind::SustainNVHi => v.ramp_gain(nv_hi, ramp_frames),
                VoiceKind::SustainVibLo => v.ramp_gain(vib_lo, ramp_frames),
                VoiceKind::SustainVibHi => v.ramp_gain(vib_hi, ramp_frames),
                // Legacy kinds still accepted; treat as NV Lo/Hi.
                VoiceKind::SustainLo => v.ramp_gain(nv_lo, ramp_frames),
                VoiceKind::SustainHi => v.ramp_gain(nv_hi, ramp_frames),
                _ => {}
            }
        }
    }
}

// ── Utilities ─────────────────────────────────────────────────────────────────

/// Gain for dynamic layer `index` in an N-layer CC1 crossfade: the active pair
/// `(lo, hi)` carry the equal-power gains, every other layer is silent.
#[inline]
fn layer_gain(index: usize, lo: usize, hi: usize, lo_g: f32, hi_g: f32) -> f32 {
    if index == lo {
        lo_g
    } else if index == hi {
        hi_g
    } else {
        0.0
    }
}

/// Convert milliseconds to audio frames at the given sample rate.
#[inline]
pub fn ms_to_frames(ms: u32, sample_rate: u32) -> usize {
    (ms as f64 * sample_rate as f64 / 1000.0).round() as usize
}

/// Decibels → linear amplitude gain (`10^(dB/20)`). Used for the data-derived
/// CSS `change_vol` makeup constants (`$3tsb0`/`$xfnyt`/`$kuqqb`).
#[inline]
pub fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Linear amplitude gain → decibels. Inverse of [`db_to_gain`]; `-inf` for 0.
#[inline]
pub fn gain_to_db(gain: f32) -> f32 {
    if gain <= 0.0 {
        f32::NEG_INFINITY
    } else {
        20.0 * gain.log10()
    }
}

/// Per-articulation base `change_vol` makeup (dB) for CSS SHORT articulations,
/// traced from `script_1.ksp` (the note-id arrays are shared across branches;
/// the makeup is per-branch). The ONLY non-zero short makeup in CSS 1st Violins
/// is Staccatissimo: `change_vol(%grhcg[...],$xfnyt*100+$arhiq,1)` (~line 15911)
/// with persistent `$xfnyt=−95` → −9.5 dB. Every other short (spiccato,
/// staccato, sfz, marcato, pizz, col legno, bartók) routes through branches
/// whose makeup source ($vxi3e←%2ng55, $pbahy, $3ahzi) ships 0 dB — only the
/// per-event velocity term `$arhiq` applies, which is the sample's own dynamic
/// selection, not a fixed makeup. (Measured Tremolo's −14 dB `$kuqqb` is a
/// distinct KSP-scripted articulation, not the sampled `Tremolo`.)
fn css_short_makeup_db(artic_id: &str) -> f32 {
    if artic_id.eq_ignore_ascii_case("Staccatissimo") {
        -9.5 // $xfnyt
    } else {
        0.0
    }
}

// ── Amp envelopes ────────────────────────────────────────────────────────────
//
// Per-voice ENV_FLEX amp envelopes are POLICY read from the spec: an
// articulation's `amp_env` segments when authored, else the family defaults
// in `crate::spec::default_amp_env` (the decoded CSS GroupList tables, kept
// as defaults so legacy libraries play unchanged). The engine only maps a
// `VoiceKind` to a role and builds the `FlexEnv` mechanism.
// r[impl signal.soundsource.declarative]

/// Select the amp envelope for a voice: the zone articulation's authored
/// `amp_env` first, else the spec-layer family default. `None` = flat unity.
fn amp_env_for(
    spec: &crate::spec::LibrarySpec,
    artic_id: &str,
    kind: &VoiceKind,
    is_sustain_layer: bool,
    sample_rate: u32,
) -> Option<FlexEnv> {
    use crate::spec::AmpEnvRole;
    let role = if matches!(kind, VoiceKind::Release) {
        AmpEnvRole::Release
    } else if matches!(kind, VoiceKind::Short) {
        AmpEnvRole::Short
    } else if matches!(kind, VoiceKind::Legato) {
        AmpEnvRole::Legato
    } else if is_sustain_layer {
        AmpEnvRole::SustainLayer
    } else {
        AmpEnvRole::Other
    };
    // Authored per-articulation envelope wins.
    let authored = spec
        .articulations
        .iter()
        .find(|a| a.id.eq_ignore_ascii_case(artic_id))
        .filter(|a| !a.amp_env.is_empty());
    let (segs, hold): (&[crate::spec::EnvSegmentSpec], bool) = match authored {
        Some(a) => (
            a.amp_env.as_slice(),
            a.amp_env_hold.unwrap_or(role == AmpEnvRole::SustainLayer),
        ),
        None => crate::spec::default_amp_env(artic_id, role)?,
    };
    let tuples: Vec<(f32, f32, f32)> = segs.iter().map(|s| (s.time_ms, s.level, s.curve)).collect();
    FlexEnv::from_segments(&tuples, 0.0, sample_rate, hold)
}

/// Frames → milliseconds (the inverse of [`ms_to_frames`]).
pub fn frames_to_ms(frames: u64, sample_rate: u32) -> f32 {
    if sample_rate == 0 {
        return 0.0;
    }
    frames as f32 * 1000.0 / sample_rate as f32
}

/// Locate the steady-state sustain **plateau** of a decoded sample so a hold
/// loop sits on flat tone. CSS-style sustains bloom in (a slow swell), hold,
/// then swell/decay out; looping through the bloom pulses (each wrap drops
/// back to the still-rising level — the artifact heard when "holding a note")
/// and looping the tail drifts quieter. This scans a short-term RMS envelope
/// across `[lo, hi)` and returns the longest contiguous run whose level stays
/// within a threshold of the sample's peak, trimmed one hop inside each edge so
/// the loop — and the crossfade material just before `loop_start` — are all on
/// the plateau. `None` when no run at least `min_len` frames long exists (the
/// caller then falls back to fractional loop points).
fn steady_loop_region(
    data: &SampleData,
    lo: usize,
    hi: usize,
    min_len: usize,
) -> Option<(usize, usize)> {
    const HOP: usize = 1024;
    let hi = hi.min(data.num_frames);
    if hi <= lo + min_len + 4 * HOP {
        return None;
    }
    // Fine short-term RMS envelope (mid channel) over the search range.
    let mut fine: Vec<f32> = Vec::new();
    let mut f = lo;
    while f + HOP <= hi {
        let mut sum = 0.0f64;
        for i in f..f + HOP {
            let (l, r) = data.frame(i);
            let m = 0.5 * (l as f64 + r as f64);
            sum += m * m;
        }
        fine.push((sum / HOP as f64).sqrt() as f32);
        f += HOP;
    }
    let n = fine.len();
    if n < 4 {
        return None;
    }
    // Smooth to a macro-envelope so bow vibrato / tremolo ripple (~6 Hz) and
    // fine bow noise are averaged out — otherwise the plateau is chopped into
    // sub-cycle fragments. Window ≈ 250 ms (a few vibrato periods).
    let win = ((data.sample_rate as usize * 250 / 1000 / HOP).max(1)).min(n / 2 + 1);
    let mut sm = vec![0.0f32; n];
    for (i, out) in sm.iter_mut().enumerate().take(n) {
        let a = i.saturating_sub(win);
        let b = (i + win + 1).min(n);
        let slice = &fine[a..b];
        *out = slice.iter().copied().sum::<f32>() / slice.len() as f32;
    }
    // Take the WIDEST steady body — the longest contiguous run of the smoothed
    // envelope at/above a fraction of its peak. A LONG loop is the key to not
    // sounding "loopy": a short window repeats ~1–2×/s and the ear locks onto
    // it as a pulse, whereas a multi-second loop reads as natural bow evolution
    // (one seam every few seconds, hidden by the crossfade). The threshold is
    // loose (on the vibrato-averaged macro-envelope) so the run spans the whole
    // body between the attack bloom and the end decay, not a narrow plateau.
    let peak = sm.iter().copied().fold(0.0f32, f32::max);
    if peak <= 0.0 {
        return None;
    }
    let thr = 0.55 * peak;
    let (mut best_start, mut best_len) = (0usize, 0usize);
    let (mut run_start, mut run_len, mut in_run) = (0usize, 0usize, false);
    for (i, &v) in sm.iter().enumerate() {
        if v >= thr {
            if !in_run {
                in_run = true;
                run_start = i;
                run_len = 0;
            }
            run_len += 1;
            if run_len > best_len {
                best_len = run_len;
                best_start = run_start;
            }
        } else {
            in_run = false;
        }
    }
    if best_len == 0 {
        return None;
    }
    // Pull one smoothing window inside each edge so the loop — and the crossfade
    // material just before `loop_start` — stay off the ramp shoulders.
    let a = best_start + win;
    let b = (best_start + best_len).saturating_sub(win);
    if b <= a {
        return None;
    }
    let start_f = lo + a * HOP;
    let end_f = lo + b * HOP;
    (end_f > start_f + min_len).then_some((start_f, end_f))
}

/// MIDI velocity → linear gain. Standard square-curve mapping
/// (`(v/127)^2`), which approximates the perceptual MIDI velocity curve
/// without needing a per-library response table.
#[inline]
pub fn velocity_gain(velocity: u8) -> f32 {
    let v = (velocity as f32 / 127.0).clamp(0.0, 1.0);
    v * v
}

fn push_recent(recent: &mut VecDeque<String>, value: String, limit: usize) {
    if recent.back() == Some(&value) {
        return;
    }
    if recent.len() >= limit {
        recent.pop_front();
    }
    recent.push_back(value);
}

fn zone_choke_group(zone: &crate::spec::ZoneSpec) -> Option<u64> {
    if !zone.choke_group.is_empty() {
        Some(stable_group_hash(&zone.choke_group))
    } else if !zone.group.is_empty() {
        Some(stable_group_hash(&zone.group))
    } else {
        None
    }
}

fn stable_group_hash(value: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn push_unique_u64(values: &mut Vec<u64>, value: u64) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn push_unique_group_limit(values: &mut Vec<(u64, usize)>, group: u64, limit: usize) {
    if let Some((_, existing)) = values.iter_mut().find(|(existing, _)| *existing == group) {
        *existing = (*existing).min(limit);
    } else {
        values.push((group, limit));
    }
}

fn zone_is_one_shot(zone: &crate::spec::ZoneSpec) -> bool {
    matches!(
        zone.trigger_mode.trim().to_ascii_lowercase().as_str(),
        "one-shot" | "one_shot" | "oneshot"
    )
}

fn zone_is_release_trigger(zone: &crate::spec::ZoneSpec) -> bool {
    matches!(
        zone.trigger_mode.trim().to_ascii_lowercase().as_str(),
        "release" | "note-release" | "note_release" | "key-up" | "key_up"
    )
}

fn zone_is_pedal_down_trigger(zone: &crate::spec::ZoneSpec) -> bool {
    matches!(
        zone.trigger_mode.trim().to_ascii_lowercase().as_str(),
        "pedal-down" | "pedal_down" | "pedaldown" | "sustain-down" | "sustain_down"
    )
}

fn zone_is_pedal_up_trigger(zone: &crate::spec::ZoneSpec) -> bool {
    matches!(
        zone.trigger_mode.trim().to_ascii_lowercase().as_str(),
        "pedal-up" | "pedal_up" | "pedalup" | "sustain-up" | "sustain_up"
    )
}

fn zone_is_cc_trigger(zone: &crate::spec::ZoneSpec) -> bool {
    matches!(
        zone.trigger_mode.trim().to_ascii_lowercase().as_str(),
        "cc" | "cc-threshold" | "cc_threshold" | "controller"
    )
}

fn zone_is_aftertouch_trigger(zone: &crate::spec::ZoneSpec) -> bool {
    matches!(
        zone.trigger_mode.trim().to_ascii_lowercase().as_str(),
        "aftertouch"
            | "channel-aftertouch"
            | "channel_aftertouch"
            | "poly-aftertouch"
            | "poly_aftertouch"
            | "pressure"
    )
}

fn trigger_value_range(zone: &crate::spec::ZoneSpec) -> std::ops::RangeInclusive<u8> {
    let min = if zone.trigger_value_min == 0 {
        64
    } else {
        zone.trigger_value_min
    };
    let max = if zone.trigger_value_max == 0 {
        127
    } else {
        zone.trigger_value_max
    };
    min..=max
}

fn zone_cc_trigger_crossed(
    zone: &crate::spec::ZoneSpec,
    controller: u8,
    old_value: u8,
    value: u8,
) -> bool {
    if !zone_is_cc_trigger(zone) || zone.trigger_cc != controller {
        return false;
    }
    let range = trigger_value_range(zone);
    !range.contains(&old_value) && range.contains(&value)
}

fn zone_aftertouch_trigger_crossed(
    zone: &crate::spec::ZoneSpec,
    note: Option<u8>,
    old_value: u8,
    value: u8,
) -> bool {
    if !zone_is_aftertouch_trigger(zone) {
        return false;
    }
    if let Some(note) = note {
        if !(zone.key_min..=zone.key_max).contains(&note) {
            return false;
        }
    }
    let range = trigger_value_range(zone);
    !range.contains(&old_value) && range.contains(&value)
}

/// Whether a library is a percussion / drum-kit (plays at natural pitch).
/// Driven by the retag-derived `category` / `instrument` so no per-pack flag
/// is needed; sampled instruments (default empty/melodic) stay pitched.
fn spec_is_percussion(spec: &crate::spec::LibrarySpec) -> bool {
    let cat = spec.category.to_ascii_lowercase();
    if cat.contains("drum") || cat.contains("percussion") {
        return true;
    }
    let inst = spec.instrument.to_ascii_lowercase();
    const PERC: &[&str] = &[
        "kick", "snare", "tom", "hat", "ride", "crash", "china", "splash", "cymbal", "clap",
        "cowbell", "perc", "drum",
    ];
    PERC.iter().any(|p| inst.contains(p))
}

fn zone_trigger_matches(zone: &crate::spec::ZoneSpec, trigger: ZoneTrigger) -> bool {
    match trigger {
        ZoneTrigger::Attack => {
            !zone_is_release_trigger(zone)
                && !zone_is_pedal_down_trigger(zone)
                && !zone_is_pedal_up_trigger(zone)
                && !zone_is_cc_trigger(zone)
                && !zone_is_aftertouch_trigger(zone)
        }
        ZoneTrigger::Release => zone_is_release_trigger(zone),
        ZoneTrigger::PedalDown => zone_is_pedal_down_trigger(zone),
        ZoneTrigger::PedalUp => zone_is_pedal_up_trigger(zone),
        ZoneTrigger::Cc => zone_is_cc_trigger(zone),
        ZoneTrigger::Aftertouch => zone_is_aftertouch_trigger(zone),
    }
}

fn zone_is_alternating_loop(zone: &crate::spec::ZoneSpec) -> bool {
    matches!(
        zone.playback_mode.trim().to_ascii_lowercase().as_str(),
        "alternate" | "alternating" | "ping-pong" | "ping_pong"
    )
}

fn dynamic_label_for_velocity(dynamics: &[String], velocity: u8) -> Option<String> {
    if dynamics.is_empty() {
        return None;
    }

    let numeric = dynamics
        .iter()
        .map(|label| label.parse::<u8>())
        .collect::<Result<Vec<_>, _>>();
    if let Ok(values) = numeric {
        let (_, label) = values
            .iter()
            .zip(dynamics.iter())
            .min_by_key(|(value, _)| value.abs_diff(velocity))?;
        return Some(label.clone());
    }

    let n = dynamics.len();
    let idx = (velocity as usize * n / 128).min(n - 1);
    Some(dynamics[idx].clone())
}

fn select_zone_rr_slot(
    zones: &[crate::spec::ZoneSpec],
    indices: &[usize],
    rr_counter: usize,
    last_slot: Option<u32>,
    random_state: &mut u64,
    forced: Option<u32>,
) -> u32 {
    debug_assert!(!indices.is_empty());
    let mut rr_slots = indices
        .iter()
        .map(|&idx| zones[idx].rr_index)
        .collect::<Vec<_>>();
    rr_slots.sort_unstable();
    rr_slots.dedup();
    if rr_slots.len() == 1 {
        return rr_slots[0];
    }
    // Test/render override: pin to a specific slot. Index into the available
    // slots modulo their count so a 0..N sweep is always valid (the harness
    // sweeps positions, not raw rr_index values, which may be sparse).
    if let Some(f) = forced {
        return rr_slots[(f as usize) % rr_slots.len()];
    }

    let mode = zones[indices[0]].rr_mode.trim().to_ascii_lowercase();
    match mode.as_str() {
        "random" => rr_slots[next_zone_random(random_state) % rr_slots.len()],
        "no-repeat-random" | "no_repeat_random" | "norepeat" | "no-repeat" => {
            let mut slot = rr_slots[next_zone_random(random_state) % rr_slots.len()];
            if Some(slot) == last_slot {
                let pos = rr_slots
                    .iter()
                    .position(|candidate| *candidate == slot)
                    .unwrap_or(0);
                slot = rr_slots[(pos + 1) % rr_slots.len()];
            }
            slot
        }
        _ => rr_slots[rr_counter % rr_slots.len()],
    }
}

fn select_zone_rr_index_by_slot(
    zones: &[crate::spec::ZoneSpec],
    indices: &[usize],
    selected: u32,
) -> usize {
    debug_assert!(!indices.is_empty());
    indices
        .iter()
        .copied()
        .find(|&idx| zones[idx].rr_index == selected)
        .unwrap_or(indices[0])
}

fn next_zone_random(state: &mut u64) -> usize {
    let mut x = *state;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *state = x;
    ((x.wrapping_mul(0x2545_f491_4f6c_dd1d)) >> 32) as usize
}

fn half_pedal_release_frames(
    base_frames: usize,
    cc64_value: u8,
    curve: &str,
    max_multiplier: f32,
) -> usize {
    if cc64_value == 0 || cc64_value >= 64 {
        return base_frames;
    }
    let position = (cc64_value as f32 / 63.0).clamp(0.0, 1.0);
    let shaped = match curve.trim().to_ascii_lowercase().as_str() {
        "squared" | "square" | "pow2" => position * position,
        "sqrt" | "square-root" | "square_root" => position.sqrt(),
        _ => position,
    };
    let max_multiplier = if max_multiplier > 0.0 {
        max_multiplier
    } else {
        HALF_PEDAL_MAX_RELEASE_MULTIPLIER
    }
    .max(1.0);
    let multiplier = 1.0 + (max_multiplier - 1.0) * shaped;
    ((base_frames as f32) * multiplier).round() as usize
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;

/// Pick the articulation a patch should start on: a *playable* body layer.
///
/// Rules, in order — Sustain first, then anything note-on can trigger
/// (Release/Legato fire from note-off or another voice, so defaulting to one
/// is silence), skipping mechanical / pedal companion layers, and — the part
/// that matters most — only ever choosing an articulation that HAS ZONES.
/// Specs ship declared-but-empty articulations (Keyscape's C7 lists
/// `grndpnopdl` with zero zones); starting there means every note sounds as
/// a release click with no body.
pub fn default_articulation(spec: &crate::spec::LibrarySpec) -> Option<String> {
    // `pdl` is Keyscape's pedal spelling — "ped" alone misses `grndpnopdl`.
    let is_aux = |id: &str| {
        let l = id.to_ascii_lowercase();
        l.contains("mch") || l.contains("mech") || l.contains("ped") || l.contains("pdl")
    };
    let playable = |a: &crate::spec::ArticulationSpec| {
        !matches!(a.kind, ArticulationKind::Release | ArticulationKind::Legato)
    };
    // Convention-mode packs (Keyscape's) declare articulations with NO zones
    // — the mapping comes from filenames at load. Zone counts are therefore
    // only a *preference*, never a requirement: rank zoned candidates first,
    // then the same filters without the zone test.
    let has_zones = |id: &str| spec.zones.iter().any(|z| z.articulation == id);
    let pick = |want_zones: bool| -> Option<&crate::spec::ArticulationSpec> {
        let zoned = |a: &crate::spec::ArticulationSpec| !want_zones || has_zones(&a.id);
        spec.articulations
            .iter()
            .find(|a| a.kind == ArticulationKind::Sustain && !is_aux(&a.id) && zoned(a))
            .or_else(|| {
                spec.articulations
                    .iter()
                    .find(|a| playable(a) && !is_aux(&a.id) && zoned(a))
            })
            .or_else(|| spec.articulations.iter().find(|a| playable(a) && zoned(a)))
    };
    pick(true)
        .or_else(|| pick(false))
        // Nothing playable at all: take the first declared articulation so a
        // caller still has an id to work with.
        .or_else(|| spec.articulations.first())
        .map(|a| a.id.clone())
}
