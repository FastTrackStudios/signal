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
pub mod rr;
pub mod voice;

use std::cell::{Cell, RefCell};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::Path;

use crate::spec::{ArticulationKind, Cc1Layer};
use crate::{PlayerPatch, VoiceConfig};
use cache::{EvictStats, PreloadStats, SampleCache};
use filter::BiquadFilter;
use rr::RrCounters;
pub use voice::ArticClass;
use voice::{DynLayer, Voice, VoiceKind, VoicePool, VoiceStealPolicy};

// ── Constants ─────────────────────────────────────────────────────────────────

/// CC1 gain ramp length (ms). Smooths dynamic crossfade to avoid clicks.
const CC1_RAMP_MS: u32 = 20;

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

/// Default legato crossfade — old sustain ramps out over this many ms.
const LEGATO_FADE_MS: u32 = 30;

/// Loudness (dB) at CC1=0 relative to CC1=127 for the continuous dynamics
/// curve. CSS sweeps a wide dynamic range; the recorded layers add timbre on
/// top. Tune for more/less dramatic mod-wheel dynamics.
const CC1_DYN_FLOOR_DB: f32 = -12.0;

/// Global output makeup (dB) applied to every spawned voice. CSS's Kontakt
/// instrument plays its samples ~+6 dB above their raw file level (measured:
/// raw ff sustain steady −32.3 dB → CSS default render −26.5 dB). Matching it
/// puts our default output at CSS's default level so the two render at parity.
const OUTPUT_MAKEUP: f32 = 1.995_262; // +6 dB = 10^(6/20)

/// Minimum attack fade (ms) for a synthesized-loop sustain that starts mid-sample
/// at full level — just enough to avoid an onset click without slowing the attack.
const SUSTAIN_DECLICK_MS: u32 = 12;

/// Gain scale for recorded release-tail samples (NVrel/Vsusrel). They're
/// normalised loud; played at unity they spike louder than the note. CSS's
/// release is a subtle tail under the decay — keep it well below the note.
const RELEASE_GAIN: f32 = 0.3;

/// Max semitones to pitch-shift from the nearest recorded zone when no zone
/// spans a note. CSS samples a whole-tone grid (±1 to fill); 2 covers grid
/// gaps + edge rounding without obviously detuning.
const ZONE_PITCH_TOLERANCE: u8 = 2;

/// Velocity used for the legato transition back to a held note when the
/// sounding note is released (a medium transition speed — there's no real
/// "release velocity" for a fall-back).
const LEGATO_FALLBACK_VELOCITY: u8 = 80;

/// Maximum gain for a release-tail voice at MIDI release-velocity 127. The
/// per-fire gain scales linearly down with release velocity so soft releases
/// produce a quiet damper click while hard releases ring out.
const RELEASE_SAMPLE_GAIN_MAX: f32 = 0.35;
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
    },
}

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
struct LegatoLine {
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
}

/// Cap on the fire log so an enabled log can never grow unbounded on the
/// audio thread (the Vec is pre-allocated to this capacity when enabled).
const LEGATO_FIRE_LOG_CAP: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ZoneTrigger {
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

    /// Notes currently held down: MIDI note → velocity. Shared across lines
    /// (keys are physical); per-line press order lives in `LegatoLine::order`.
    held_notes: HashMap<u8, u8>,
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
    /// Recorded legato transition firings (see [`LegatoFireEvent`]).
    legato_fire_log: Vec<LegatoFireEvent>,

    /// Con Sordino bus-level filter (placeholder lowpass — see filter.rs).
    sord_filter: BiquadFilter,

    /// Fade duration (frames) applied to old sustain when legato fires.
    legato_fade_frames: usize,
    /// Ramp length (frames) for CC1 gain updates.
    cc1_ramp_frames: usize,
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
        let is_aux_layer = |id: &str| -> bool {
            let l = id.to_ascii_lowercase();
            l.contains("mch") || l.contains("mech") || l.contains("ped")
        };
        let articulation = patch
            .spec
            .articulations
            .iter()
            .find(|a| a.kind == ArticulationKind::Sustain && !is_aux_layer(&a.id))
            .or_else(|| {
                patch.spec.articulations.iter().find(|a| {
                    !matches!(a.kind, ArticulationKind::Release | ArticulationKind::Legato)
                        && !is_aux_layer(&a.id)
                })
            })
            .or_else(|| {
                patch.spec.articulations.iter().find(|a| {
                    !matches!(a.kind, ArticulationKind::Release | ArticulationKind::Legato,)
                })
            })
            .or_else(|| patch.spec.articulations.first())
            .map(|a| a.id.clone())
            .unwrap_or_default();

        let legato_fade_frames = ms_to_frames(LEGATO_FADE_MS, sample_rate);
        let cc1_ramp_frames = ms_to_frames(CC1_RAMP_MS, sample_rate);
        let release_frames = ms_to_frames(RELEASE_MS, sample_rate);
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
            cc11_volume: 1.0,
            cc5_porta_volume: 1.0,
            cc_values: [0; 128],
            channel_aftertouch: 0,
            poly_aftertouch: [0; 128],
            cc64_held: false,
            cc64_value: 0,
            no_pedal_articulation: None,
            con_sordino: false,
            legato_enabled: true,
            legato_expressive: false, // default: low-latency mode
            play_mode: PlayMode::StrictLive,
            sord_filter: BiquadFilter::lowpass(filter::SORD_FC, filter::SORD_Q, sample_rate),
            // Pre-size note-keyed maps to the full MIDI range so note-on never
            // reallocates them on the audio thread.
            held_notes: HashMap::with_capacity(128),
            body_voiced: std::collections::HashSet::with_capacity(128),
            deferred_note_off_velocities: HashMap::with_capacity(128),
            frames_rendered: 0,
            legato_fire_log_enabled: false,
            legato_fire_log: Vec::new(),
            legato_fade_frames,
            cc1_ramp_frames,
            release_frames,
            attack_frames: 0,
            unison: (1, 0.0, 0.0),
            zone_rr_counter: 0,
            zone_rr_random_state: 0x9e37_79b9_7f4a_7c15,
            zone_rr_last_slots: HashMap::with_capacity(128),
            forced_rr: None,
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
                .resolve(
                    &self.section,
                    &self.articulation,
                    &self.mic,
                    &dynamic,
                    note,
                    "",
                    0,
                )
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

    /// How many REACTIVE legato-path triggers (note-on countdown / note-off
    /// fallback) occurred since the fire log was last enabled. Document
    /// playback schedules every transition via prefire, so this must read 0
    /// after a document render — any other value means the annotator missed
    /// an edge and the engine degraded to live-mode timing for it.
    pub fn reactive_legato_fires(&self) -> u64 {
        self.reactive_legato_fires
    }

    // ── Mono legato lines ─────────────────────────────────────────────────────

    /// Make `line` the active line for the current dispatch: line-scoped
    /// state (mono note, press order, pending countdown) and the CC1/CC2
    /// mirrors now refer to it. Out-of-range ids clamp into the pool.
    fn set_active_line(&mut self, line: LineId) {
        let l = line.min(MAX_LINES - 1);
        self.cur_line = l;
        self.cc1 = self.lines[l].cc1;
        self.cc2 = self.lines[l].cc2;
    }

    fn line(&self) -> &LegatoLine {
        &self.lines[self.cur_line]
    }

    fn line_mut(&mut self) -> &mut LegatoLine {
        &mut self.lines[self.cur_line]
    }

    /// Running render position in frames since construction.
    pub fn frames_rendered(&self) -> u64 {
        self.frames_rendered
    }

    /// Document-mode note-on for a legato-followed note: fire the transition
    /// NOW instead of arming the reactive countdown. The document scheduler
    /// calls this `delay_ms` (the spec's expressive velocity→delay) *before*
    /// the destination note's tick, so the transition's audible arrival lands
    /// exactly on the grid — the inversion of the reactive path, where the
    /// countdown makes the note speak `delay_ms` *after* its tick
    /// (see `docs/plan/document-mode.md`).
    ///
    /// The scheduler emits this INSTEAD of a note-on for the note; all
    /// held-note / mono-line bookkeeping happens here. Degrades gracefully:
    /// non-zoned patches, legato-off, or an empty legato line fall back to
    /// the plain note-on behaviour.
    pub fn legato_prefire(&mut self, note: u8, velocity: u8) {
        self.legato_prefire_line(0, note, velocity);
    }

    /// Line-addressed [`legato_prefire`](Self::legato_prefire) — the document
    /// scheduler resolves each scheduled event to a [`LineId`] (currently the
    /// channel→line allocator: chan N → line N) so every divisi line runs its
    /// own prefired mono legato.
    pub fn legato_prefire_line(&mut self, line: LineId, note: u8, velocity: u8) {
        self.legato_prefire_line_lead(line, note, velocity, None);
    }

    /// [`legato_prefire_line`](Self::legato_prefire_line) carrying the
    /// schedule's prefire lead (frames from this call to the destination
    /// tick). The engine aligns the transition sample's MEASURED lead-in to
    /// it: a sample with a longer lead-in starts partway in, a shorter one
    /// is held back on a countdown — either way the pitch change lands
    /// exactly on the tick. `None` = fire now, arrival lands wherever the
    /// sample's own lead-in puts it (live/back-compat behaviour).
    pub fn legato_prefire_line_lead(
        &mut self,
        line: LineId,
        note: u8,
        velocity: u8,
        sched_lead: Option<u64>,
    ) {
        self.set_active_line(line);
        if velocity == 0 {
            self.note_off_line(line, note);
            return;
        }
        if self.try_keyswitch(note, velocity) {
            return;
        }
        if !self.patch.is_zoned() || !self.legato_enabled {
            self.note_on_line(line, note, velocity);
            return;
        }
        self.held_notes.insert(note, velocity);
        let l = self.line_mut();
        l.order.retain(|&n| n != note);
        l.order.push(note);
        match self.line().note {
            // No sounding line (document started mid-phrase, or annotation
            // was optimistic): start the note plainly.
            None => {
                self.play_direction = "up".to_string();
                self.trigger_zoned_sustain(note);
                self.line_mut().note = Some(note);
            }
            // Fire the transition — no reactive countdown. The scheduler
            // subtracted the measured lead-in from the trigger time; if the
            // zone that will actually play has a SHORTER lead-in (e.g. a
            // different CC1 layer than the schedule's estimate), hold the
            // fire back by the difference so the arrival still lands on the
            // destination tick. (NOT counted as a reactive fire.)
            Some(cur) => {
                let (_delay_ms, portamento) = self.legato_timing(velocity);
                self.play_direction = if note >= cur { "up" } else { "down" }.to_string();
                if let Some(lead) = sched_lead {
                    if let Some(zone_lead) = self.transition_lead_frames(cur, note, portamento) {
                        if zone_lead < lead {
                            // Hold the fire back so the arrival lands ON the
                            // tick (fires via the render countdown; NOT a
                            // reactive fire — the counter is untouched).
                            self.line_mut().state = LegatoState::Pending {
                                frames_remaining: (lead - zone_lead) as usize,
                                from_note: cur,
                                to_note: note,
                                to_note_velocity: velocity,
                                portamento,
                            };
                            return;
                        }
                        // Longer lead-in than scheduled: fire now, skipping
                        // the surplus off the sample's front.
                        self.fire_legato_with_lead(cur, note, velocity, portamento, Some(lead));
                        return;
                    }
                }
                // No measurement (legacy library) or live prefire: fire now,
                // the sample's own lead-in lands wherever it lands.
                self.fire_legato_with_lead(cur, note, velocity, portamento, None);
            }
        }
    }

    /// Sample rate (frames/s) — for ms↔frame conversion by callers.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Warm (decode into cache, on the caller's thread) every attack zone a
    /// note-on at `note` could trigger under the CURRENT articulation pin +
    /// solo mic — across all dynamic layers and round-robins — so the first
    /// hit at any velocity is never silent. Selection mirrors the live trigger
    /// path (unlike [`warm_note_samples`](Self::warm_note_samples), which uses
    /// the pin/mic-agnostic `resolve_zone` and can warm the wrong articulation).
    /// Returns how many samples were decoded; a no-op for non-zoned patches.
    pub fn warm_note(&self, note: u8) -> crate::engine::cache::PreloadStats {
        use crate::engine::cache::PreloadStats;
        if !self.patch.is_zoned() {
            return PreloadStats::default();
        }
        let artic = self.effective_articulation().map(|s| s.to_string());
        // Also warm the vibrato pair — CC2 can blend it in at any time, and the
        // directional legato samples come in up/down pairs (warmed by ignoring
        // direction below), so any combination is ready without a cold first hit.
        let vib_pair = artic.as_deref().and_then(|a| self.find_vibrato_pair_id(a));
        // And the recorded RELEASE samples (e.g. NVrel/Vsusrel) for both the main
        // articulation and its vib pair — otherwise note-off cache-misses and the
        // release tail is silent (CSS rings out ~0.7s). These are separate
        // articulations triggered on key-up by `spawn_release`.
        let rel = |a: &str| {
            self.patch
                .spec
                .articulation(a)
                .and_then(|art| art.release_artic.clone())
        };
        // And the legato + portamento TRANSITION articulations (Leg/NVLeg/
        // Legzero/NVLegzero/Port), which `spawn_legato_transition` fires on
        // overlapping notes — same cache-miss-→-silent trap as releases.
        // warm_note ignores direction, so both up/down transitions warm.
        // BOTH sides of each CC2 pair warm (the vibrato crossfade can call
        // either at any time — a CC2-low prefire plays NVLeg even when the
        // pinned articulation is the vibrato sustain).
        let (leg_nv, leg_vib) = self.legato_pair_ids(false);
        let (zero_nv, zero_vib) = self.legato_pair_ids(true);
        let leg_rel = leg_nv
            .as_deref()
            .and_then(rel)
            .or_else(|| leg_vib.as_deref().and_then(rel));
        let warm_ids: Vec<String> = [
            artic.clone(),
            vib_pair.clone(),
            artic.as_deref().and_then(rel),
            vib_pair.as_deref().and_then(rel),
            leg_nv,
            leg_vib,
            zero_nv,
            zero_vib,
            leg_rel,
            self.find_port_artic_id(),
        ]
        .into_iter()
        .flatten()
        .collect();
        let mut stats = PreloadStats::default();
        for (i, z) in self.patch.spec.zones.iter().enumerate() {
            if !zone_trigger_matches(z, ZoneTrigger::Attack) {
                continue;
            }
            // Within the zone's range, or within pitch-shift tolerance of its
            // recorded key (whole-tone grid → even notes warm their neighbour).
            let in_range = note >= z.key_min && note <= z.key_max;
            let near =
                (z.root_key as i32 - note as i32).unsigned_abs() as u8 <= ZONE_PITCH_TOLERANCE;
            if !in_range && !near {
                continue;
            }
            if artic.is_some() {
                let matches_artic = z.articulation.is_empty()
                    || warm_ids
                        .iter()
                        .any(|id| z.articulation.eq_ignore_ascii_case(id));
                if !matches_artic {
                    continue;
                }
            }
            if let Some(solo) = &self.solo_mic {
                if !z.mic.eq_ignore_ascii_case(solo) {
                    continue;
                }
            }
            let path = &self.patch.zone_paths[i];
            if self.cache.get_loaded(path).is_some() {
                continue;
            }
            match self.cache.get(path) {
                Ok(data) => {
                    stats.loaded += 1;
                    stats.bytes += data.decoded_bytes();
                }
                Err(_) => stats.failed += 1,
            }
        }
        stats
    }

    /// Returns the currently active articulation ID.
    pub fn articulation(&self) -> &str {
        &self.articulation
    }

    /// Directly set the active articulation. Used in tests; production code
    /// should generally go through CC58 / keyswitches.
    pub fn set_articulation(&mut self, artic_id: impl Into<String>) {
        self.articulation = artic_id.into();
    }

    /// Pin this engine to a single articulation (percussion kits). When set,
    /// only zones whose `articulation` matches fire, regardless of the
    /// incoming key — so one drum pack can be split across several
    /// performance notes (hats Closed vs Open, snare Hit vs Cross Stick).
    /// `None` (or an empty string) clears the pin.
    pub fn pin_articulation(&mut self, artic: Option<String>) {
        self.pinned_articulation = artic.filter(|s| !s.is_empty());
    }

    /// Set an engine-wide choke group. Voices join it so they can be silenced.
    /// `choke_on` lists the articulations that actually trigger the choke:
    /// empty = monophonic (every hit chokes — hi-hats); non-empty = only those
    /// articulations choke (cymbals: `["Choke"]` so crashes ring but the choke
    /// stops them). `None`/empty group clears it (engine stays polyphonic).
    pub fn set_choke_group(&mut self, group: Option<&str>, choke_on: &[String]) {
        self.engine_choke_group = group.filter(|s| !s.is_empty()).map(stable_group_hash);
        self.engine_choke_on = choke_on
            .iter()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_ascii_lowercase())
            .collect();
    }

    /// Whether this note-on should silence the engine choke group: true when a
    /// group is set and either it's monophonic (`choke_on` empty) or the active
    /// articulation is one of the configured choke triggers.
    fn should_engine_choke(&self) -> bool {
        if self.engine_choke_group.is_none() {
            return false;
        }
        if self.engine_choke_on.is_empty() {
            return true;
        }
        match self
            .trigger_articulation
            .as_deref()
            .or(self.pinned_articulation.as_deref())
        {
            Some(a) => self
                .engine_choke_on
                .iter()
                .any(|c| c.eq_ignore_ascii_case(a)),
            None => false,
        }
    }

    /// Whether a zone should fire for this `(note, velocity, trigger)`,
    /// honoring percussion mode and any pinned articulation.
    ///
    /// - Pinned articulation: match by articulation only, key ignored.
    /// - Single-key percussion (kick, tom): match any note.
    /// - Otherwise: the zone's key range gates as usual (pitched samplers,
    ///   multi-articulation drum packs addressed by their native keys).
    fn zone_selected(
        &self,
        zone: &crate::spec::ZoneSpec,
        note: u8,
        velocity: u8,
        trigger: ZoneTrigger,
    ) -> bool {
        if !zone_trigger_matches(zone, trigger) {
            return false;
        }
        if velocity < zone.vel_min || velocity > zone.vel_max {
            return false;
        }
        // An explicit pin (percussion kits / routed triggers) matches by
        // articulation ONLY, ignoring the key — any routed note plays the pinned
        // articulation's zone.
        if let Some(pin) = self
            .trigger_articulation
            .as_ref()
            .or(self.pinned_articulation.as_ref())
        {
            return zone.articulation.eq_ignore_ascii_case(pin);
        }
        if self.percussion && self.single_attack_key {
            return true;
        }
        // Melodic libraries: only the currently-selected articulation fires
        // (set by keyswitch / CC58 / `set_articulation`). Without this filter,
        // every articulation in a multi-articulation zone set (e.g. CSS ships
        // ~20 in one set) would sound at once. An empty selection = no filter
        // (legacy behaviour for libraries that don't switch articulations).
        if !self.articulation.is_empty()
            && !zone.articulation.is_empty()
            && !zone.articulation.eq_ignore_ascii_case(&self.articulation)
        {
            return false;
        }
        // Directional zones (CSS legato records up- vs down-transitions
        // separately): only the current play direction fires. Non-directional
        // zones (shorts, sustains) carry an empty `direction` and are unaffected.
        if !zone.direction.is_empty() && !zone.direction.eq_ignore_ascii_case(&self.play_direction)
        {
            return false;
        }
        // CC1 dynamic layer: CSS sustains/legato record p/mf/ff at full velocity
        // range and crossfade them by CC1 — without picking one layer they all
        // stack. Velocity-driven (short) articulations keep their vel_min/max
        // gating and are left alone here.
        if !zone.dynamic.is_empty() {
            if let Some(dyn_label) = self.current_zone_dynamic_cc1() {
                if !zone.dynamic.eq_ignore_ascii_case(&dyn_label) {
                    return false;
                }
            }
        }
        note >= zone.key_min && note <= zone.key_max
    }

    /// The dominant CC1 dynamic-layer label for the current articulation, or
    /// `None` for articulations that aren't CC1-controlled (shorts) or have no
    /// declared dynamic layers. Used to pick a single zoned dynamic layer.
    fn current_zone_dynamic_cc1(&self) -> Option<String> {
        let kind = self
            .patch
            .spec
            .articulation(&self.articulation)
            .map(|a| a.kind.clone());
        if matches!(
            kind,
            Some(ArticulationKind::Short | ArticulationKind::OneShot)
        ) {
            return None;
        }
        let layers = self.active_cc1_layers();
        if layers.is_empty() {
            return None;
        }
        let (lo, hi, blend) = Self::cc1_blend(layers, self.cc1);
        Some(if blend >= 0.5 { hi } else { lo })
    }

    /// The articulation that drives zone selection / warming: an explicit pin
    /// wins (percussion / routed triggers), otherwise the live selection set by
    /// keyswitch / CC58 / [`set_articulation`](Self::set_articulation). `None`
    /// when nothing is selected (legacy "all articulations" behaviour).
    fn effective_articulation(&self) -> Option<&str> {
        self.trigger_articulation
            .as_deref()
            .or(self.pinned_articulation.as_deref())
            .or(if self.articulation.is_empty() {
                None
            } else {
                Some(self.articulation.as_str())
            })
    }

    /// Toggle Con Sordino mode.
    ///
    /// When enabled the engine remaps the current articulation to its sordino
    /// counterpart (`"Vibsus"` → `"SordVibsus"`, etc.) using the `"Sord"`
    /// prefix convention. When disabled it strips the prefix. If no
    /// counterpart exists in the spec the articulation is left unchanged.
    pub fn set_con_sordino(&mut self, active: bool) {
        if self.con_sordino == active {
            return;
        }
        self.con_sordino = active;
        self.articulation = self.remap_sordino(&self.articulation, active);
        if !active {
            // Clear filter state so stale tail doesn't bleed into dry output.
            self.sord_filter.reset();
        }
    }

    /// Returns whether Con Sordino mode is currently active.
    pub fn con_sordino(&self) -> bool {
        self.con_sordino
    }

    /// Number of voices currently active.
    pub fn active_voices(&self) -> usize {
        self.voices.active_count()
    }

    pub fn set_voice_config(&mut self, config: &VoiceConfig) {
        if config.polyphony > 0 {
            self.voices.set_max_voices(config.polyphony as usize);
        }
        if !config.voice_steal.trim().is_empty() {
            self.voices
                .set_steal_policy(VoiceStealPolicy::from_str(&config.voice_steal));
        }
    }

    pub fn max_voices(&self) -> usize {
        self.voices.max_voices()
    }

    pub fn voice_steal_policy(&self) -> &'static str {
        match self.voices.steal_policy() {
            VoiceStealPolicy::ReleaseFirstQuietest => "release_first_quietest",
            VoiceStealPolicy::Oldest => "oldest",
            VoiceStealPolicy::Quietest => "quietest",
            VoiceStealPolicy::SameNoteFirst => "same_note_first",
            VoiceStealPolicy::DropNew => "drop_new",
        }
    }

    pub fn stolen_voices(&self) -> usize {
        self.voices.stolen_count()
    }

    pub fn cache_misses(&self) -> usize {
        self.cache_misses.get()
    }

    pub fn sample_misses(&self) -> usize {
        self.sample_misses.get()
    }

    pub fn recent_cache_misses(&self) -> Vec<String> {
        self.recent_cache_misses.borrow().iter().cloned().collect()
    }

    pub fn recent_sample_misses(&self) -> Vec<String> {
        self.recent_sample_misses.borrow().iter().cloned().collect()
    }

    // ── MIDI input ────────────────────────────────────────────────────────────

    /// Process a MIDI note-on event.
    /// Note-on with a per-trigger articulation override (percussion routing).
    /// `articulation` fires only the matching articulation's zones for this
    /// hit, ignoring key — letting one shared drum pack serve many routed
    /// notes. `None` behaves like [`note_on`](Self::note_on).
    pub fn note_on_articulated(&mut self, note: u8, velocity: u8, articulation: Option<&str>) {
        let prev = self.trigger_articulation.take();
        self.trigger_articulation = articulation
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        self.note_on(note, velocity);
        self.trigger_articulation = prev;
    }

    pub fn note_on(&mut self, note: u8, velocity: u8) {
        // Live auto-divisi (StrictLive + mono-legato patch): the greedy line
        // allocator decides which mono line the note belongs to and whether
        // it may legato — see `live_divisi_note_on`. Lookahead mode has no
        // such gates (the document knows the actual voice-leading), and
        // non-legato material (shorts, percussion, pianos) is line-agnostic.
        if velocity > 0
            && self.play_mode == PlayMode::StrictLive
            && self.live_divisi_applicable()
            && !self.keyswitch_notes.contains_key(&note)
        {
            self.live_divisi_note_on(note, velocity);
            return;
        }
        self.note_on_line(0, note, velocity);
    }

    /// Whether the live auto-divisi allocator governs note-ons right now:
    /// zoned patch, legato enabled, and a sustain-family articulation
    /// selected (the mono-legato branch of `note_on_line`).
    fn live_divisi_applicable(&self) -> bool {
        if !self.patch.is_zoned() || !self.legato_enabled {
            return false;
        }
        matches!(
            self.patch
                .spec
                .articulation(&self.articulation)
                .map(|a| &a.kind),
            Some(
                ArticulationKind::Legato
                    | ArticulationKind::Sustain
                    | ArticulationKind::Looped
                    | ArticulationKind::Trill
            )
        )
    }

    /// Live auto-divisi with legato gating (see `docs/plan/document-mode.md`,
    /// "Auto-divisi" → live gating). Greedy, reactive, deterministic given
    /// the same input event stream:
    ///
    /// - **Simultaneity gate**: notes within `live_chord_window_ms` of the
    ///   previous onset are a chord — fresh sustain attacks on separate
    ///   lines (allocated in arrival order; a zero-latency engine cannot
    ///   buffer to rank a chord it hasn't finished hearing), never legato.
    /// - **Interval gate**: a note continues an existing line as LEGATO only
    ///   if within `live_legato_interval_max` semitones of that line's
    ///   sounding note (nearest line wins; ties → lowest line id), or of a
    ///   line's just-released note (abutment within the chord window).
    ///   Transitions run through the reactive countdown, which StrictLive
    ///   times from the `low_latency` tables.
    /// - Otherwise: fresh attack on a free line (LRU-silent first; if every
    ///   line is sounding, the least-recently-active line is retagged).
    fn live_divisi_note_on(&mut self, note: u8, velocity: u8) {
        let now = self.frames_rendered;
        let window = ms_to_frames(
            self.patch.spec.live_chord_window_ms.max(0.0).round() as u32,
            self.sample_rate,
        ) as u64;
        let interval_max = self.patch.spec.live_legato_interval_max as i16;
        let chord = self
            .live_last_onset
            .is_some_and(|f| now.saturating_sub(f) <= window);
        self.live_last_onset = Some(now);

        // Legato continuation target: nearest sounding line within the
        // interval gate, else a just-released line whose note abuts.
        // `(interval, line, released-from)`; ascending line order breaks ties.
        let mut target: Option<(i16, LineId, Option<u8>)> = None;
        if !chord {
            for (li, l) in self.lines.iter().enumerate() {
                if let Some(cur) = l.note {
                    let iv = (cur as i16 - note as i16).abs();
                    if iv <= interval_max && target.is_none_or(|(best, _, _)| iv < best) {
                        target = Some((iv, li, None));
                    }
                }
            }
            if target.is_none() {
                for (li, l) in self.lines.iter().enumerate() {
                    if l.note.is_some() || now.saturating_sub(l.last_release_frame) > window {
                        continue;
                    }
                    if let Some(rel) = l.released_note {
                        let iv = (rel as i16 - note as i16).abs();
                        if iv <= interval_max && target.is_none_or(|(best, _, _)| iv < best) {
                            target = Some((iv, li, Some(rel)));
                        }
                    }
                }
            }
        }

        let li = match target {
            Some((_, li, _)) => li,
            None => self.alloc_free_line(),
        };
        self.set_active_line(li);
        self.held_notes.insert(note, velocity);
        let l = self.line_mut();
        l.order.retain(|&n| n != note);
        l.order.push(note);
        l.last_activity = now;
        l.released_note = None;

        match target {
            // Continue the line: reactive transition (low_latency-timed).
            Some((_, _, None)) => {
                let cur = self.line().note.expect("sounding target line");
                self.start_legato_transition(cur, note, velocity);
            }
            // Abutting re-entry on a just-released line: transition from the
            // released note (the recorded slur carries the connection).
            Some((_, _, Some(rel))) => {
                self.start_legato_transition(rel, note, velocity);
            }
            // Fresh sustain attack on its own line.
            None => {
                self.play_direction = "up".to_string();
                self.trigger_zoned_sustain(note);
                self.line_mut().note = Some(note);
            }
        }
    }

    /// Pick a line for a fresh live attack: the least-recently-active SILENT
    /// line (fresh lines have activity 0, so from silence a chord fans out
    /// as line 0, 1, 2, …). If every line is sounding, retag the
    /// least-recently-active one (its old voices ring on until their own
    /// note-offs; the global fallback in `note_off` still finds them).
    fn alloc_free_line(&mut self) -> LineId {
        let mut best_free: Option<(u64, LineId)> = None;
        let mut best_any: Option<(u64, LineId)> = None;
        for (li, l) in self.lines.iter().enumerate() {
            let free = l.note.is_none() && matches!(l.state, LegatoState::Idle);
            if free && best_free.is_none_or(|(k, _)| l.last_activity < k) {
                best_free = Some((l.last_activity, li));
            }
            if best_any.is_none_or(|(k, _)| l.last_activity < k) {
                best_any = Some((l.last_activity, li));
            }
        }
        let li = best_free.or(best_any).map(|(_, li)| li).unwrap_or(0);
        // Stealing a sounding line: clear its bookkeeping so the new note
        // owns it (the old note's voices release via the global fallback).
        let l = &mut self.lines[li];
        l.note = None;
        l.order.clear();
        l.state = LegatoState::Idle;
        li
    }

    /// The line that currently owns `note` (sounding on it, or held in its
    /// press order — which includes pending transition targets).
    fn line_owning(&self, note: u8) -> Option<LineId> {
        self.lines
            .iter()
            .position(|l| l.note == Some(note))
            .or_else(|| self.lines.iter().position(|l| l.order.contains(&note)))
    }

    /// Line-addressed note-on: mono-line legato bookkeeping happens on
    /// `line`, and spawned voices are tagged with it. The channel-less
    /// [`note_on`](Self::note_on) uses line 0 (live single-line play).
    pub fn note_on_line(&mut self, line: LineId, note: u8, velocity: u8) {
        self.set_active_line(line);
        if velocity == 0 {
            self.note_off_line(line, note);
            return;
        }

        // Velocity-sensitive keyswitches (CSS-style): a keyswitch note selects
        // an articulation / mode and does NOT sound.
        if self.try_keyswitch(note, velocity) {
            return;
        }

        if self.patch.is_zoned() {
            // CSS-style legato: a legato articulation, with another note already
            // held, plays a delayed directional transition (the famous latency).
            // The first note of a phrase — or any non-legato articulation —
            // sounds immediately.
            let kind = self
                .patch
                .spec
                .articulation(&self.articulation)
                .map(|a| a.kind.clone());
            let is_legato = matches!(kind, Some(ArticulationKind::Legato));
            // Held + CC1-crossfaded: sustains, tremolo, harmonics, and trills.
            let is_sustain = matches!(
                kind,
                Some(
                    ArticulationKind::Sustain | ArticulationKind::Looped | ArticulationKind::Trill
                )
            );
            // One-shot, velocity-picked dynamic: spiccato/staccato/sfz/pizz/etc.
            let is_short = matches!(
                kind,
                Some(ArticulationKind::Short | ArticulationKind::OneShot)
            );
            self.held_notes.insert(note, velocity);

            if (is_legato || is_sustain) && self.legato_enabled {
                // Monophonic legato line (CSS default). CSS plays its *sustain*
                // articulation (e.g. "Nonvib") legato — the Leg/NVLeg samples are
                // the sustain body's transitions — so any long articulation, not
                // just a Legato-kind one, takes this path when legato is enabled.
                // Track press order for last-note-priority fall-back on release.
                let l = self.line_mut();
                l.order.retain(|&n| n != note);
                l.order.push(note);
                match self.line().note {
                    // First note of the phrase: sounds immediately, no transition.
                    None => {
                        self.play_direction = "up".to_string();
                        self.trigger_zoned_sustain(note);
                        self.line_mut().note = Some(note);
                    }
                    // Transition from the currently-sounding note to this one,
                    // delayed by the velocity-mapped legato latency. A note that
                    // arrives mid-transition just re-targets it (fast runs
                    // collapse to the latest note — never dropped, never stacked).
                    Some(cur) => self.start_legato_transition(cur, note, velocity),
                }
            } else if is_legato || is_sustain {
                // Held: polyphonic sustain / trill (legato OFF for legato artics),
                // full CC1 dynamic + CC2 vibrato blend, loops to hold.
                self.play_direction = "up".to_string();
                self.trigger_zoned_sustain(note);
            } else if is_short {
                // One-shot short: velocity picks the dynamic layer, nearest-key
                // pitch-shift, plays to completion.
                self.trigger_zoned_short(note, velocity);
            } else {
                // Anything unusual: fall back to the generic zoned trigger.
                self.trigger_zoned(note, velocity, ZoneTrigger::Attack, true);
            }
            return;
        }

        // Legzero: same note re-trigger while sustain pedal is held.
        let legzero = self.cc64_held && self.held_notes.contains_key(&note);

        // Whether any other note is currently held (legato condition).
        let other_held = self.held_notes.keys().any(|&n| n != note);

        self.held_notes.insert(note, velocity);
        self.deferred_note_off_velocities.remove(&note);

        let artic_kind = self
            .patch
            .spec
            .articulation(&self.articulation)
            .map(|a| a.kind.clone());

        match artic_kind {
            Some(ArticulationKind::Sustain | ArticulationKind::Looped) => {
                if legzero {
                    self.trigger_legzero(note, velocity);
                } else if other_held && self.legato_enabled {
                    self.initiate_legato(note, velocity);
                } else {
                    self.trigger_sustain(note);
                }
            }
            Some(ArticulationKind::Short | ArticulationKind::OneShot) => {
                self.trigger_short(note, velocity);
            }
            Some(ArticulationKind::Legato | ArticulationKind::Release) => {
                // These are not triggered directly by note-on.
            }
            Some(ArticulationKind::Trill | ArticulationKind::Special) => {
                // Treat special/trill as sustain for basic playback.
                self.trigger_sustain(note);
            }
            None => {
                tracing::warn!(
                    artic = %self.articulation,
                    "note_on: unknown articulation — skipping"
                );
            }
        }
    }

    /// Process a MIDI note-off event. Channel-less (live) note-offs route to
    /// the line that owns the note — the live allocator may have placed it
    /// anywhere; single-line play always resolves to line 0.
    pub fn note_off(&mut self, note: u8) {
        let line = self.line_owning(note).unwrap_or(0);
        self.note_off_line(line, note);
    }

    /// Line-addressed note-off (see [`note_on_line`](Self::note_on_line)).
    pub fn note_off_line(&mut self, line: LineId, note: u8) {
        self.set_active_line(line);
        let release_velocity = self.cc1;
        self.note_off_with_velocity_on_line(note, release_velocity);
    }

    pub fn note_off_with_velocity(&mut self, note: u8, release_velocity: u8) {
        let line = self.line_owning(note).unwrap_or(0);
        self.set_active_line(line);
        self.note_off_with_velocity_on_line(note, release_velocity);
    }

    fn note_off_with_velocity_on_line(&mut self, note: u8, release_velocity: u8) {
        let release_frames = self.pedal_release_frames();
        if self.cc64_held {
            // Sustain pedal held — defer release.
            self.deferred_note_off_velocities
                .insert(note, release_velocity);
            tracing::debug!(note, release_velocity, "pedal defer");
            return;
        }
        self.held_notes.remove(&note);
        self.deferred_note_off_velocities.remove(&note);
        if self.patch.is_zoned() {
            let cur_line = self.cur_line as u8;
            self.line_mut().order.retain(|&n| n != note);
            // Monophonic legato: releasing the SOUNDING note falls back to the
            // most-recent still-held note via a legato transition; releasing the
            // line's last note ends it. Lifting a held-but-silent key is a no-op.
            if self.legato_enabled && self.line().note.is_some() {
                if self.line().note != Some(note) {
                    return;
                }
                if let Some(&prev) = self.line().order.last() {
                    // Fall back at a medium transition speed.
                    self.start_legato_transition(note, prev, LEGATO_FALLBACK_VELOCITY);
                    return;
                }
                // Line ends — remember what/when for the live allocator's
                // "just released" abutment gate.
                let now = self.frames_rendered;
                let l = self.line_mut();
                l.note = None;
                l.released_note = Some(note);
                l.last_release_frame = now;
            }
            // Play the recorded release tail (CSS Vsusrel/NVrel) and fade the
            // looping sustain underneath it — this line's voices only, so a
            // unison note held by another divisi line keeps sounding.
            self.spawn_release(note);
            self.voices
                .note_off_line(cur_line, note, Some(release_frames));
            return;
        }
        self.do_note_off_with_release_frames(note, release_velocity, release_frames);
    }

    /// Release every held/sounding note and reset pedal-deferred state.
    pub fn all_notes_off(&mut self) {
        self.held_notes.clear();
        self.deferred_note_off_velocities.clear();
        self.cc64_held = false;
        self.cc64_value = 0;
        if let Some(orig) = self.no_pedal_articulation.take() {
            self.articulation = orig;
        }
        self.reset_lines();
        self.voices.all_notes_off();
    }

    /// End every mono line: sounding note, press order, pending countdowns.
    /// (CC1/CC2 values persist — controllers outlive notes.)
    fn reset_lines(&mut self) {
        for l in &mut self.lines {
            l.state = LegatoState::Idle;
            l.note = None;
            l.order.clear();
            l.released_note = None;
            l.last_release_frame = 0;
            l.last_activity = 0;
        }
        self.live_last_onset = None;
    }

    pub fn panic(&mut self) {
        self.held_notes.clear();
        self.deferred_note_off_velocities.clear();
        self.cc64_held = false;
        self.cc64_value = 0;
        if let Some(orig) = self.no_pedal_articulation.take() {
            self.articulation = orig;
        }
        self.reset_lines();
        self.voices.panic();
    }

    /// Zone-mode trigger. Looks up every zone matching `(note, velocity)`
    /// and spawns one `Zoned` voice per **mic group** within the match
    /// set. RR-cycling happens within each mic group so a kick with 7
    /// mics × 8 RR slots fires 7 simultaneous voices that share the
    /// same RR index.
    fn trigger_zoned(
        &mut self,
        note: u8,
        velocity: u8,
        trigger: ZoneTrigger,
        record_empty_miss: bool,
    ) {
        // Bucket matching zones by mic id so each mic gets its own
        // round-robin within the candidate set.
        let mut by_mic: std::collections::BTreeMap<String, Vec<usize>> =
            std::collections::BTreeMap::new();
        for (i, z) in self.patch.spec.zones.iter().enumerate() {
            if self.zone_selected(z, note, velocity, trigger) {
                by_mic.entry(z.mic.clone()).or_default().push(i);
            }
        }
        self.trigger_zoned_groups(by_mic, Some(note), velocity, trigger, record_empty_miss);
    }

    fn trigger_cc_zones(&mut self, controller: u8, old_value: u8, value: u8) {
        let mut by_mic: std::collections::BTreeMap<String, Vec<usize>> =
            std::collections::BTreeMap::new();
        for (i, z) in self.patch.spec.zones.iter().enumerate() {
            if zone_cc_trigger_crossed(z, controller, old_value, value) {
                by_mic.entry(z.mic.clone()).or_default().push(i);
            }
        }
        self.trigger_zoned_groups(by_mic, None, value, ZoneTrigger::Cc, false);
    }

    fn trigger_aftertouch_zones(&mut self, note: Option<u8>, old_value: u8, value: u8) {
        let mut by_mic: std::collections::BTreeMap<String, Vec<usize>> =
            std::collections::BTreeMap::new();
        for (i, z) in self.patch.spec.zones.iter().enumerate() {
            if zone_aftertouch_trigger_crossed(z, note, old_value, value) {
                by_mic.entry(z.mic.clone()).or_default().push(i);
            }
        }
        self.trigger_zoned_groups(by_mic, note, value, ZoneTrigger::Aftertouch, false);
    }

    fn trigger_event_zones(&mut self, trigger: ZoneTrigger, velocity: u8) {
        let mut by_mic: std::collections::BTreeMap<String, Vec<usize>> =
            std::collections::BTreeMap::new();
        for (i, z) in self.patch.spec.zones.iter().enumerate() {
            if zone_trigger_matches(z, trigger) {
                by_mic.entry(z.mic.clone()).or_default().push(i);
            }
        }
        self.trigger_zoned_groups(by_mic, None, velocity, trigger, false);
    }

    /// Trigger a zoned sustain/legato note with the full CSS expressive blend:
    /// CC1 crossfades across ALL dynamic layers, CC2 crossfades the non-vibrato
    /// and vibrato sample sets. Spawns one `SustainLayer` voice per dynamic
    /// layer per vib side; `render()` re-levels them live as CC1/CC2 move (see
    /// [`update_sustain_gains`](Self::update_sustain_gains)), so a held note
    /// swells the full dynamic range. `self.articulation` is the non-vibrato
    /// base; the vibrato pair is found and blended in by CC2.
    fn trigger_zoned_sustain(&mut self, note: u8) {
        let direction = self.play_direction.clone();
        // Honour the forced-RR pin like every other RR-bearing trigger —
        // document mode pins a stable per-note slot here so playback never
        // consults the mutable counter (position-independent determinism).
        let rr = self
            .forced_rr
            .map(|f| f as usize)
            .unwrap_or(self.zone_rr_counter);
        self.zone_rr_counter = self.zone_rr_counter.wrapping_add(1);

        // CC2 picks the non-vib vs vib balance (equal-power).
        let (nv_scale, vb_scale) = Self::equal_power(self.cc2_blend());
        let nv_artic = self.articulation.clone();
        let vib_artic = self.find_vibrato_pair_id(&nv_artic);

        self.spawn_sustain_layers(&nv_artic, false, nv_scale, &direction, note, rr);
        if let Some(vib_id) = vib_artic {
            self.spawn_sustain_layers(&vib_id, true, vb_scale, &direction, note, rr);
        }
    }

    /// Trigger a one-shot short note (spiccato / staccato / sfz / pizz / …):
    /// velocity picks the dynamic layer, nearest recorded key is pitch-shifted,
    /// and it plays to completion (no CC1 crossfade, no loop).
    fn trigger_zoned_short(&mut self, note: u8, velocity: u8) {
        let rr = self
            .forced_rr
            .map(|f| f as usize)
            .unwrap_or(self.zone_rr_counter);
        self.zone_rr_counter = self.zone_rr_counter.wrapping_add(1);
        let artic = self.articulation.clone();
        let dynamic = self.short_note_dynamic(velocity);
        // Multi-dynamic shorts (CSS: pp/p/mf/ff) have the loudness baked into the
        // velocity-selected sample — applying a velocity² gain on top would
        // double-attenuate (mirrors the non-zoned path's n_dyn unity rule). Only
        // articulations with a single recorded dynamic get a velocity curve.
        let n_dyn = self
            .patch
            .spec
            .articulation(&artic)
            .map(|a| a.dynamics.len())
            .unwrap_or(0);
        let gain = if n_dyn > 1 {
            1.0
        } else {
            velocity_gain(velocity)
        };
        if let Some(idx) = self.find_layer_zone(&artic, "", &dynamic, note, rr) {
            self.spawn_zone_voice(idx, note, VoiceKind::Short, gain, None, 0.0);
        }
    }

    /// Spawn EVERY dynamic layer of `artic` (one `SustainLayer` voice each),
    /// gained by the current CC1 crossfade. Holding all layers — even the silent
    /// ones — is what lets a held note swell the full dynamic range as CC1 moves
    /// (`update_sustain_gains` re-levels them live), the way CSS does.
    fn spawn_sustain_layers(
        &mut self,
        artic: &str,
        vib: bool,
        side_scale: f32,
        direction: &str,
        note: u8,
        rr: usize,
    ) {
        let labels: Vec<String> = self
            .cc1_layers_for(artic)
            .iter()
            .map(|l| l.label.clone())
            .collect();
        if labels.is_empty() {
            // No declared CC1 layers — a single layer, loudness from CC1.
            let (lo, _, _) = self.layers_for_artic(artic);
            let gain = side_scale * Self::cc1_expression(self.cc1);
            if let Some(idx) = self.find_layer_zone(artic, direction, &lo, note, rr) {
                self.spawn_zone_voice(
                    idx,
                    note,
                    VoiceKind::SustainLayer,
                    gain,
                    Some(DynLayer { vib, index: 0 }),
                    0.0,
                );
            }
            return;
        }
        let (lo_idx, hi_idx, blend) = Self::cc1_blend_idx(self.cc1_layers_for(artic), self.cc1);
        let (lo_g, hi_g) = Self::equal_power(blend);
        let expr = Self::cc1_expression(self.cc1);
        for (i, label) in labels.iter().enumerate() {
            let gain = side_scale * expr * layer_gain(i, lo_idx, hi_idx, lo_g, hi_g);
            if let Some(idx) = self.find_layer_zone(artic, direction, label, note, rr) {
                self.spawn_zone_voice(
                    idx,
                    note,
                    VoiceKind::SustainLayer,
                    gain,
                    Some(DynLayer {
                        vib,
                        index: i as u8,
                    }),
                    0.0,
                );
            }
        }
    }

    /// Start a monophonic legato transition `from` → `to` at `velocity`. The
    /// note is delayed by the velocity-mapped legato latency (CSS's three
    /// transition speeds); when it elapses, `render()` calls `fire_legato`,
    /// which fades the old note and plays the directional transition zone. A
    /// zero delay (portamento) fires immediately.
    fn start_legato_transition(&mut self, from: u8, to: u8, velocity: u8) {
        // Every entry here is the REACTIVE path (live note-on countdown or
        // note-off fallback) — document playback must never reach it (it
        // schedules `legato_prefire_line` instead), which tests assert via
        // this counter.
        self.reactive_legato_fires = self.reactive_legato_fires.saturating_add(1);
        let (delay_ms, portamento) = self.legato_timing(velocity);
        let frames = ms_to_frames(delay_ms, self.sample_rate);
        if frames == 0 {
            self.play_direction = if to >= from { "up" } else { "down" }.to_string();
            self.fire_legato(from, to, velocity, portamento);
        } else {
            self.line_mut().state = LegatoState::Pending {
                frames_remaining: frames,
                from_note: from,
                to_note: to,
                to_note_velocity: velocity,
                portamento,
            };
        }
    }

    /// Stem class of an articulation id: Short/OneShot kinds are Shorts,
    /// everything long (Sustain/Legato/Looped/Trill/Release/Special) is
    /// Longs. Percussion engines class every unmatched tag as Shorts (a drum
    /// hit is a short by nature).
    fn artic_class_for(&self, artic_id: &str) -> ArticClass {
        match self.patch.spec.articulation(artic_id).map(|a| &a.kind) {
            Some(ArticulationKind::Short | ArticulationKind::OneShot) => ArticClass::Shorts,
            Some(_) => ArticClass::Longs,
            None => {
                if self.percussion {
                    ArticClass::Shorts
                } else {
                    ArticClass::Longs
                }
            }
        }
    }

    /// The legato transition delay (ms) + portamento flag for a target
    /// velocity: portamento below the threshold, else the expressive or
    /// low-latency velocity→delay curve from the spec — chosen by the
    /// [`PlayMode`] policy: Lookahead → expressive (full authenticity),
    /// StrictLive → low_latency, NO exceptions (a CC58 "expressive" request
    /// only takes effect once the mode is Lookahead).
    fn legato_timing(&self, velocity: u8) -> (u32, bool) {
        let port_thresh = self
            .patch
            .spec
            .legato_engine
            .as_ref()
            .and_then(|le| le.portamento.as_ref())
            .map(|p| p.trigger_vel_max)
            .unwrap_or(0);
        let portamento = port_thresh > 0 && velocity <= port_thresh;
        let delay_ms = if portamento {
            0
        } else if self.play_mode == PlayMode::Lookahead {
            self.patch.legato_delay_expressive(velocity).unwrap_or(100)
        } else {
            self.patch.legato_delay_low_latency(velocity).unwrap_or(100)
        };
        (delay_ms, portamento)
    }

    /// Spawn the legato TRANSITION sample(s) for the move `from → to` (the
    /// one-shot `Leg`/`NVLeg`/`Port` recordings carrying the bow change; the
    /// long-form sample also holds the arrived note by looping its tail).
    ///
    /// CSS transition naming (verified empirically — see the generator in
    /// sample-collector's `sc-import css-legato-fix`): the sample's
    /// `root_key` is the LOWER pitch of the pair, `direction` says which end
    /// is the source (`up` = root→root+interval, `down` = the reverse), and
    /// `interval` is the semitone distance (1..=12, whole-tone root grid).
    /// So the zone for `from → to` is: direction `sign(to-from)`, named note
    /// `min(from,to)`, interval `|to-from|` clamped to an octave. The chosen
    /// zone is pitch-shifted so the DESTINATION lands exactly on `to` —
    /// guaranteed by construction even when the interval or root had to be
    /// approximated (only the lead-in's source end is then off, and the real
    /// source voice is still sounding over it).
    ///
    /// Same-pitch re-bows (`from == to`) use the `Legzero` re-trigger
    /// samples (destination note only, no lead-in, 3 RRs).
    ///
    /// Vibrato: like the sustains, the Leg/NVLeg transition sets are a CC2
    /// crossfade pair — both sides spawn at equal-power gains.
    ///
    /// `sched_lead` is the document scheduler's prefire lead in frames (the
    /// transition fires that early so the pitch change lands on the
    /// destination tick). When the chosen zone's measured lead-in is LONGER
    /// than the scheduled lead, the surplus is skipped off the front of the
    /// sample so the arrival still lands on the tick.
    fn spawn_legato_transition(
        &mut self,
        from: u8,
        to: u8,
        velocity: u8,
        portamento: bool,
        sched_lead: Option<u64>,
    ) {
        let _ = velocity;
        if portamento {
            // Portamento glides: single `Port` articulation (one `f` dynamic,
            // no vibrato pair), volume from CC5 "Portamento Volume".
            let id = self
                .find_port_artic_id()
                .or_else(|| self.find_legato_artic_id(false));
            if let Some(id) = id {
                self.spawn_transition_voice(&id, from, to, self.cc5_porta_volume, sched_lead);
            }
            return;
        }
        let (nv_id, vib_id) = self.legato_pair_ids(from == to);
        let (nv_scale, vb_scale) = Self::equal_power(self.cc2_blend());
        let mut spawned = false;
        for (id, scale) in [(nv_id, nv_scale), (vib_id, vb_scale)] {
            let Some(id) = id else { continue };
            // A fully-crossfaded-out side is skipped — unless it is the only
            // side present (mono-set libraries keep sounding regardless of CC2).
            if scale <= 0.001 && spawned {
                continue;
            }
            self.spawn_transition_voice(&id, from, to, scale.max(0.001), sched_lead);
            spawned = true;
        }
    }

    /// The non-vibrato / vibrato legato articulation pair for the CC2
    /// crossfade — (`NVLeg`, `Leg`), or the `*zero` re-trigger pair when
    /// `retrigger`. Either side may be absent.
    fn legato_pair_ids(&self, retrigger: bool) -> (Option<String>, Option<String>) {
        let want_sord = self.articulation.starts_with("Sord");
        let mut nv = None;
        let mut vib = None;
        for a in &self.patch.spec.articulations {
            if a.kind != ArticulationKind::Legato {
                continue;
            }
            if a.id.starts_with("Sord") != want_sord {
                continue;
            }
            let id_lower = a.id.to_lowercase();
            if id_lower.contains("port") || id_lower.contains("zero") != retrigger {
                continue;
            }
            let is_nv = id_lower.contains("nv") || id_lower.contains("nonvib");
            let slot = if is_nv { &mut nv } else { &mut vib };
            if slot.is_none() {
                *slot = Some(a.id.clone());
            }
        }
        (nv, vib)
    }

    /// Spawn one transition voice from articulation `leg_id` for `from → to`
    /// (see [`spawn_legato_transition`](Self::spawn_legato_transition) for
    /// the selection rules).
    fn spawn_transition_voice(
        &mut self,
        leg_id: &str,
        from: u8,
        to: u8,
        gain: f32,
        sched_lead: Option<u64>,
    ) {
        // Dynamic layer for the transition from CC1 (single dominant layer).
        let (lo, hi, blend) = self.layers_for_artic(leg_id);
        let dynamic = if blend >= 0.5 { hi } else { lo };
        if from == to {
            // Re-bow: `Legzero` destination-note re-trigger (3 RRs, no lead-in).
            let rr = self
                .forced_rr
                .map(|f| f as usize)
                .unwrap_or(self.zone_rr_counter);
            if let Some(idx) = self.find_layer_zone(leg_id, "", &dynamic, to, rr) {
                self.spawn_zone_voice(idx, to, VoiceKind::Legato, gain, None, 0.0);
            }
            return;
        }
        let direction = if to > from { "up" } else { "down" };
        let named = from.min(to);
        // CSS samples nothing beyond an octave: clamp and pitch-shift so the
        // destination stays exact (the lead-in's source end lands an octave
        // off, under the still-sounding real source voice).
        let interval = u32::from(from.abs_diff(to)).min(12);
        let Some(idx) = self.find_transition_zone(leg_id, direction, &dynamic, named, interval)
        else {
            return;
        };
        let z = &self.patch.spec.zones[idx];
        // The sample's destination pitch is root (down) or root+interval
        // (up); the voice plays at `to - root + pitch_offset` semitones, so
        // this offset makes the destination land exactly on `to` no matter
        // which zone was chosen.
        let pitch_offset = if z.direction.eq_ignore_ascii_case("up") {
            -f64::from(z.interval)
        } else {
            0.0
        };
        // The measured lead-in is in SAMPLE time; playback runs at `rate`,
        // so the surplus skipped off the front must be `sched_lead` WALL
        // frames short of the lead-in in sample frames.
        let rate = 2.0f64.powf((f64::from(to) - f64::from(z.root_key) + pitch_offset) / 12.0);
        let lead_sample_frames =
            ms_to_frames(z.lead_in_ms.max(0.0).round() as u32, self.sample_rate) as u64;
        // Prefired short? Skip the surplus lead-in so the arrival lands on
        // the destination tick anyway.
        let start_offset = match sched_lead {
            Some(lead) => {
                let lead_in_sample = (lead as f64 * rate) as u64;
                lead_sample_frames.saturating_sub(lead_in_sample) as usize
            }
            None => 0,
        };
        let ok = self.spawn_zone_voice_at(
            idx,
            to,
            VoiceKind::Legato,
            gain,
            None,
            pitch_offset,
            start_offset,
        );
        if std::env::var_os("SIGNAL_LEGATO_DEBUG").is_some() {
            eprintln!(
                "LEGATO {}→{} zone={} root={} int={} dir={} lead_ms={} sched={:?} offset={} pitch_off={} spawned={}",
                from,
                to,
                self.patch.spec.zones[idx].file,
                self.patch.spec.zones[idx].root_key,
                self.patch.spec.zones[idx].interval,
                self.patch.spec.zones[idx].direction,
                self.patch.spec.zones[idx].lead_in_ms,
                sched_lead,
                start_offset,
                pitch_offset,
                ok,
            );
        }
    }

    /// Measured lead-in (frames) of the transition zone that WOULD be picked
    /// for `from → to` right now (dominant CC2 side, current CC1 layer) —
    /// the document prefire path uses this to decide whether to hold the
    /// fire back so the pitch change lands exactly on the destination tick.
    ///
    /// `None` = no measurement available (legacy library without per-zone
    /// `lead_in_ms`): callers keep the legacy fire-at-prefire behaviour.
    fn transition_lead_frames(&self, from: u8, to: u8, portamento: bool) -> Option<u64> {
        if from == to {
            // Legzero re-triggers genuinely have no lead-in — but only for
            // libraries that carry measurements at all.
            return self.patch.spec.has_measured_legato().then_some(0);
        }
        let id = if portamento {
            self.find_port_artic_id()
                .or_else(|| self.find_legato_artic_id(false))
        } else {
            let (nv, vib) = self.legato_pair_ids(false);
            let (nv_scale, vb_scale) = Self::equal_power(self.cc2_blend());
            if vb_scale >= nv_scale {
                vib.or(nv)
            } else {
                nv.or(vib)
            }
        };
        let id = id?;
        let (lo, hi, blend) = self.layers_for_artic(&id);
        let dynamic = if blend >= 0.5 { hi } else { lo };
        let direction = if to > from { "up" } else { "down" };
        let named = from.min(to);
        let interval = u32::from(from.abs_diff(to)).min(12);
        let idx = self.find_transition_zone(&id, direction, &dynamic, named, interval)?;
        let z = &self.patch.spec.zones[idx];
        if z.lead_in_ms <= 0.0 {
            return None;
        }
        // Convert the sample-time lead-in to WALL frames at the playback
        // rate this zone will actually get (destination pinned to `to`).
        let pitch_offset = if z.direction.eq_ignore_ascii_case("up") {
            -f64::from(z.interval)
        } else {
            0.0
        };
        let rate = 2.0f64.powf((f64::from(to) - f64::from(z.root_key) + pitch_offset) / 12.0);
        let sample_frames = ms_to_frames(z.lead_in_ms.round() as u32, self.sample_rate) as f64;
        Some((sample_frames / rate) as u64)
    }

    /// Pick the transition zone for (articulation, direction, dynamic,
    /// named lower note, interval): exact interval at the nearest recorded
    /// root wins; otherwise nearest interval, nearest root. Honours the solo
    /// mic. Destination-pitch correctness never depends on this choice (the
    /// caller re-tunes), only lead-in realism does.
    fn find_transition_zone(
        &self,
        artic: &str,
        direction: &str,
        dynamic: &str,
        named: u8,
        interval: u32,
    ) -> Option<usize> {
        let mut best: Option<(u32, usize)> = None;
        for (i, z) in self.patch.spec.zones.iter().enumerate() {
            if !zone_trigger_matches(z, ZoneTrigger::Attack) {
                continue;
            }
            if z.interval == 0 || !z.articulation.eq_ignore_ascii_case(artic) {
                continue;
            }
            if !z.direction.eq_ignore_ascii_case(direction) {
                continue;
            }
            if !dynamic.is_empty()
                && !z.dynamic.is_empty()
                && !z.dynamic.eq_ignore_ascii_case(dynamic)
            {
                continue;
            }
            if let Some(solo) = &self.solo_mic {
                if !z.mic.eq_ignore_ascii_case(solo) {
                    continue;
                }
            }
            let d_int = z.interval.abs_diff(interval);
            let d_root = u32::from(z.root_key.abs_diff(named));
            let score = d_int * 100 + d_root;
            if best.is_none_or(|(s, _)| score < s) {
                best = Some((score, i));
            }
        }
        best.map(|(_, i)| i)
    }

    /// Spawn the recorded RELEASE sample (e.g. `NVrel`/`Vsusrel`) for the
    /// current sustain articulation when a note is released — CSS's release
    /// tail. No-op if the articulation declares no `release_artic`.
    fn spawn_release(&mut self, note: u8) {
        let rel_id = self
            .patch
            .spec
            .articulation(&self.articulation)
            .and_then(|a| a.release_artic.clone());
        let Some(rel_id) = rel_id else {
            return;
        };
        let (lo, hi, blend) = self.layers_for_artic(&rel_id);
        let dynamic = if blend >= 0.5 { hi } else { lo };
        let rr = self
            .forced_rr
            .map(|f| f as usize)
            .unwrap_or(self.zone_rr_counter);
        // Release samples are non-directional → pass "" (no direction filter).
        // CSS's recorded releases are a subtle bow-off tail UNDER the note's
        // decay — but these samples are normalised loud, so at unity (×makeup)
        // they spike louder than the note itself ("note-off noise"). Trim them.
        if let Some(idx) = self.find_layer_zone(&rel_id, "", &dynamic, note, rr) {
            self.spawn_zone_voice(idx, note, VoiceKind::Release, RELEASE_GAIN, None, 0.0);
        }
    }

    /// Pick the zone index for (articulation, direction, dynamic layer, note),
    /// honouring the solo mic, with simple round-robin over matches by `rr`.
    fn find_layer_zone(
        &self,
        artic: &str,
        direction: &str,
        dynamic: &str,
        note: u8,
        rr: usize,
    ) -> Option<usize> {
        let mut matches: Vec<usize> = Vec::new();
        // Nearest single-key zone when no zone spans the note: CSS records a
        // whole-tone grid (every other semitone) and pitch-shifts ±1 to fill,
        // and the extracted zones are single-key (key_min == key_max), so even
        // notes have no exact zone — fall back to the closest recorded pitch.
        let mut nearest: Option<(u8, usize)> = None;
        for (i, z) in self.patch.spec.zones.iter().enumerate() {
            if !zone_trigger_matches(z, ZoneTrigger::Attack) {
                continue;
            }
            if !z.articulation.eq_ignore_ascii_case(artic) {
                continue;
            }
            if !z.direction.is_empty() && !z.direction.eq_ignore_ascii_case(direction) {
                continue;
            }
            if !dynamic.is_empty()
                && !z.dynamic.is_empty()
                && !z.dynamic.eq_ignore_ascii_case(dynamic)
            {
                continue;
            }
            if let Some(solo) = &self.solo_mic {
                if !z.mic.eq_ignore_ascii_case(solo) {
                    continue;
                }
            }
            if note >= z.key_min && note <= z.key_max {
                matches.push(i);
            } else {
                let dist = (z.root_key as i32 - note as i32).unsigned_abs() as u8;
                if nearest.is_none_or(|(d, _)| dist < d) {
                    nearest = Some((dist, i));
                }
            }
        }
        if !matches.is_empty() {
            return Some(matches[rr % matches.len()]);
        }
        // No exact zone — use the nearest recorded pitch within range and let
        // spawn_zone_voice pitch-shift it (note - root_key semitones).
        nearest
            .filter(|(d, _)| *d <= ZONE_PITCH_TOLERANCE)
            .map(|(_, i)| i)
    }

    /// Spawn a voice from zone `idx` for `note` with a layer `kind` and
    /// crossfade `gain_scale` (multiplied into the zone's own gain). Returns
    /// false if the sample isn't loaded yet.
    fn spawn_zone_voice(
        &mut self,
        idx: usize,
        note: u8,
        kind: VoiceKind,
        gain_scale: f32,
        dyn_layer: Option<DynLayer>,
        pitch_offset: f64,
    ) -> bool {
        self.spawn_zone_voice_at(idx, note, kind, gain_scale, dyn_layer, pitch_offset, 0)
    }

    /// [`spawn_zone_voice`](Self::spawn_zone_voice) starting `start_offset`
    /// frames INTO the zone's window — the legato prefire path skips the
    /// surplus of a transition sample's measured lead-in so the pitch change
    /// lands exactly on the destination tick.
    #[allow(clippy::too_many_arguments)]
    fn spawn_zone_voice_at(
        &mut self,
        idx: usize,
        note: u8,
        kind: VoiceKind,
        gain_scale: f32,
        dyn_layer: Option<DynLayer>,
        pitch_offset: f64,
        start_offset: usize,
    ) -> bool {
        // Copy out the zone fields up front so no borrow of `self.patch`
        // outlives the `&mut self` cache-miss bookkeeping below.
        let z = &self.patch.spec.zones[idx];
        let (root_key, tune_cents, gain_db, mic, pan) =
            (z.root_key, z.tune_cents, z.gain_db, z.mic.clone(), z.pan);
        let (sample_start, sample_end) = (z.sample_start, z.sample_end);
        let playback_mode = z.playback_mode.clone();
        let (loop_start, loop_end) = (z.loop_start, z.loop_end);
        let alternating = zone_is_alternating_loop(z);
        // Stem class: a Release voice follows its PARENT articulation (a
        // short's release lands in the Shorts stem); everything else is
        // classed by the zone's own articulation.
        let artic_class = if matches!(kind, VoiceKind::Release) {
            self.artic_class_for(&self.articulation)
        } else {
            self.artic_class_for(&z.articulation)
        };
        let line = self.cur_line as u8;
        let path = self.patch.zone_paths[idx].clone();

        let Some(data) = self.cache.get_loaded(&path) else {
            self.cache_misses
                .set(self.cache_misses.get().saturating_add(1));
            self.record_cache_miss(&path);
            return false;
        };
        let num_frames = data.num_frames;

        // `pitch_offset` lets legato keep the correct note-tag (`to`, for
        // note-off/silence) while shifting the recorded transition so it lands
        // on the target (CSS legato samples are source-labelled a grid step away).
        let semitones = note as f64 - root_key as f64 + pitch_offset;
        let total_cents = semitones * 100.0 + tune_cents as f64;
        let rate = 2.0f64.powf(total_cents / 1200.0);
        let gain = 10.0f32.powf(gain_db / 20.0) * gain_scale * OUTPUT_MAKEUP;
        let mic_index = self.mic_index_for(&mic);
        let is_sustain_layer = matches!(
            kind,
            VoiceKind::SustainNVLo
                | VoiceKind::SustainNVHi
                | VoiceKind::SustainVibLo
                | VoiceKind::SustainVibHi
                | VoiceKind::SustainLo
                | VoiceKind::SustainHi
                | VoiceKind::SustainLayer
        );

        // CSS-style sustains ship no loop points but have a slow ~0.8s natural
        // attack pre-roll that CSS skips (Kontakt sample-start) for a fast attack.
        // Start such bodies at the loud steady region so onsets aren't sluggish.
        let synth_loop = is_sustain_layer
            && loop_end <= loop_start
            && playback_mode.is_empty()
            && !alternating
            && num_frames > 0;
        let (sus_lo, sus_hi) = if synth_loop {
            let lo = num_frames / 6;
            (lo, ((num_frames as f32 * 0.55) as usize).max(lo + 2))
        } else {
            (0, 0)
        };
        // A CSS legato TRANSITION sample is long-form (recorded bow change +
        // the sustained target note, ~2.3s). It must carry the held note by
        // itself — playing it AND a separate sustain body doubles the note into
        // a chorus. So play it from the start (transition intact) and loop its
        // sustained TAIL (back portion, past the transition) to hold.
        let legato_hold = matches!(kind, VoiceKind::Legato)
            && loop_end <= loop_start
            && playback_mode.is_empty()
            && !alternating
            && num_frames > 0;
        let (leg_lo, leg_hi) = if legato_hold {
            // Loop a STABLE mid-sustain region — past the bow-change transition
            // at the front, but before the recorded note's end-swell, which made
            // the held note pulse loudly every loop. ~0.4..0.65 of the long-form
            // sample sits in the steady arrived-note body.
            let lo = (num_frames as f32 * 0.4) as usize;
            (lo, ((num_frames as f32 * 0.65) as usize).max(lo + 2))
        } else {
            (0, 0)
        };
        // Play from the sample's actual start so the recorded attack (the bow
        // onset) is heard — we want the start of the sample, not a mid-sample
        // jump. The loop below still holds the body for indefinite sustain.
        // (`start_offset` shifts into the window for prefired legato
        // transitions whose measured lead-in exceeds the scheduled lead.)
        let start_frame = sample_start as usize + start_offset;

        // Attack envelope on the sustained body only (the legato transition and
        // shorts keep their natural recorded attack).
        let _ = SUSTAIN_DECLICK_MS;
        let attack = if is_sustain_layer {
            self.attack_frames
        } else {
            0
        };
        // Unison: spawn N copies spread across ±detune/2 cents + a pan spread,
        // level-compensated. copies == 1 is the normal single-voice path.
        let (copies, det_cents, width) = self.unison;
        let copies = copies.max(1);
        let comp = 1.0 / (copies as f32).sqrt();
        for k in 0..copies {
            let off = if copies == 1 {
                0.0
            } else {
                (k as f32 / (copies - 1) as f32) * 2.0 - 1.0
            };
            let u_rate = rate * 2f64.powf((off * det_cents * 0.5) as f64 / 1200.0);
            let u_pan = (pan + off * width).clamp(-1.0, 1.0);
            let mut voice = Voice::with_rate(
                data.clone(),
                note,
                kind.clone(),
                u_rate,
                gain * comp,
                self.release_frames,
            )
            .with_mic_index(mic_index)
            .with_line(line)
            .with_artic_class(artic_class)
            .with_pan(u_pan)
            .with_attack(attack)
            .with_sample_window(start_frame, (sample_end > 0).then_some(sample_end as usize));
            if let Some(layer) = dyn_layer {
                voice = voice.with_dyn_layer(layer);
            }
            if playback_mode.eq_ignore_ascii_case("reverse") {
                voice = voice.reversed();
            } else if alternating {
                voice = voice.with_alternating_loop(loop_start as usize, loop_end as usize);
            } else if loop_end > loop_start {
                voice = voice.with_forward_loop(loop_start as usize, loop_end as usize);
            } else if synth_loop {
                // CSS sustain samples ship no loop points but must hold indefinitely.
                // Loop the loud steady region just past the attack — NOT the back half,
                // which dilutes into the sample's natural end-decay and made held notes
                // a few dB too quiet over time. Playback also STARTS at sus_lo (above),
                // skipping the slow natural attack for a CSS-fast onset.
                voice = voice.with_forward_loop(sus_lo, sus_hi);
            } else if legato_hold {
                // Legato transition: keep the slur at the front, loop the sustained
                // tail so the note holds without a doubling sustain body.
                voice = voice.with_forward_loop(leg_lo, leg_hi);
            }
            self.voices.spawn(voice);
        }
        true
    }

    fn trigger_zoned_groups(
        &mut self,
        mut by_mic: std::collections::BTreeMap<String, Vec<usize>>,
        event_note: Option<u8>,
        velocity: u8,
        trigger: ZoneTrigger,
        record_empty_miss: bool,
    ) {
        // Single-mic solo: keep only the requested mic's bucket so multi-mic
        // zone sets (CSS ships Main + Mix in one set, no `mics` block) don't
        // fold every mic to bus 0 and double. Centralised here so it applies to
        // every trigger path (attack / release / CC / aftertouch / event).
        if let Some(solo) = &self.solo_mic {
            by_mic.retain(|mic, _| mic.eq_ignore_ascii_case(solo));
        }
        if by_mic.is_empty() {
            if record_empty_miss {
                self.sample_misses
                    .set(self.sample_misses.get().saturating_add(1));
                self.record_sample_miss(format!(
                    "zone note={} velocity={velocity}",
                    event_note
                        .map(|note| note.to_string())
                        .unwrap_or_else(|| "event".to_string())
                ));
                tracing::debug!(note = ?event_note, velocity, trigger = ?trigger, "zone miss");
            }
            return;
        }

        let rr_idx = self.zone_rr_counter;
        self.zone_rr_counter = self.zone_rr_counter.wrapping_add(1);
        // Reuse scratch buffers across note-ons: `mem::take` swaps in an empty
        // Vec (no allocation) and we restore the grown buffer afterwards, so
        // these allocate only on the first few note-ons, never steady-state.
        let mut all_indices = std::mem::take(&mut self.zone_indices_scratch);
        all_indices.clear();
        all_indices.extend(by_mic.values().flatten().copied());
        // Packed key: trigger discriminant | note (+1, 0 = None) | velocity.
        let rr_key = ((trigger as u8 as u64) << 16)
            | ((event_note.map(|n| n as u64 + 1).unwrap_or(0)) << 8)
            | velocity as u64;
        let last_slot = self.zone_rr_last_slots.get(&rr_key).copied();
        let selected_rr_slot = select_zone_rr_slot(
            &self.patch.spec.zones,
            &all_indices,
            rr_idx,
            last_slot,
            &mut self.zone_rr_random_state,
            self.forced_rr,
        );
        self.zone_rr_last_slots.insert(rr_key, selected_rr_slot);
        self.zone_indices_scratch = all_indices;

        let mut choked_groups = std::mem::take(&mut self.zone_choked_scratch);
        choked_groups.clear();
        for indices in by_mic.values() {
            let z = &self.patch.spec.zones
                [select_zone_rr_index_by_slot(&self.patch.spec.zones, indices, selected_rr_slot)];
            for group in z.off_by.iter().filter(|group| !group.is_empty()) {
                push_unique_u64(&mut choked_groups, stable_group_hash(group));
            }
            if !z.choke_group.is_empty() {
                push_unique_u64(&mut choked_groups, stable_group_hash(&z.choke_group));
            }
        }
        // Engine-wide choke: silence the group when this hit is a choking one
        // (mono for hi-hats; only the "Choke" articulation for cymbals).
        if let Some(group) = self.engine_choke_group {
            if self.should_engine_choke() {
                push_unique_u64(&mut choked_groups, group);
            }
        }
        for &group in &choked_groups {
            self.voices
                .silence_choke_group(group, self.legato_fade_frames);
        }
        self.zone_choked_scratch = choked_groups;

        let mut capped_groups = std::mem::take(&mut self.zone_capped_scratch);
        capped_groups.clear();
        for indices in by_mic.values() {
            let z = &self.patch.spec.zones
                [select_zone_rr_index_by_slot(&self.patch.spec.zones, indices, selected_rr_slot)];
            if z.group_polyphony > 0 {
                if let Some(group) = zone_choke_group(z) {
                    push_unique_group_limit(&mut capped_groups, group, z.group_polyphony as usize);
                }
            }
        }
        for &(group, max_voices) in &capped_groups {
            if self.voices.active_choke_group_count(group) >= max_voices {
                self.voices
                    .silence_choke_group(group, self.legato_fade_frames);
            }
        }
        self.zone_capped_scratch = capped_groups;

        for (mic_id, indices) in by_mic {
            let pick =
                select_zone_rr_index_by_slot(&self.patch.spec.zones, &indices, selected_rr_slot);
            let z = &self.patch.spec.zones[pick];
            // Tag the new voice into the engine-wide choke group (if any) so
            // the next hit can silence it; an explicit zone choke wins.
            let choke_group = zone_choke_group(z).or(self.engine_choke_group);
            let path = self.patch.zone_paths[pick].clone();
            let Some(data) = self.cache.get_loaded(&path) else {
                self.cache_misses
                    .set(self.cache_misses.get().saturating_add(1));
                self.record_cache_miss(&path);
                tracing::trace!("zone sample not yet loaded: {}", path.display());
                continue;
            };

            let note = event_note.unwrap_or(z.root_key);
            // Percussion (and articulation-pinned) engines play at natural
            // pitch — the routed note is a trigger selector, not a transpose.
            // Without this a drum routed to any note other than its zone key
            // would detune (e.g. a tom on note 45 vs root 50 = -5 semitones).
            let semitones = if self.percussion || self.pinned_articulation.is_some() {
                0.0
            } else {
                note as f64 - z.root_key as f64
            };
            let total_cents = semitones * 100.0 + z.tune_cents as f64;
            let rate = 2.0f64.powf(total_cents / 1200.0);
            let gain = 10.0f32.powf(z.gain_db / 20.0);
            let mic_index = self.mic_index_for(&mic_id);

            // Percussion plays one-shot: the sample rings to its natural end
            // and note-off never cuts it (a drum is struck, not held). Pitched
            // samplers keep held/zoned semantics unless the zone says one-shot.
            let voice_kind = if trigger == ZoneTrigger::Release {
                VoiceKind::Release
            } else if zone_is_one_shot(z) || self.percussion {
                VoiceKind::Short
            } else {
                VoiceKind::Zoned
            };
            // Stem class: releases follow the parent articulation; direct
            // triggers are classed by their zone's articulation.
            let artic_class = if matches!(voice_kind, VoiceKind::Release) {
                self.artic_class_for(&self.articulation)
            } else {
                self.artic_class_for(&z.articulation)
            };
            let line = self.cur_line as u8;
            // Unison: spawn N detuned/panned copies (copies == 1 = normal).
            let (copies, det_cents, width) = self.unison;
            let copies = copies.max(1);
            let comp = 1.0 / (copies as f32).sqrt();
            for k in 0..copies {
                let off = if copies == 1 {
                    0.0
                } else {
                    (k as f32 / (copies - 1) as f32) * 2.0 - 1.0
                };
                let u_rate = rate * 2f64.powf((off * det_cents * 0.5) as f64 / 1200.0);
                let u_pan = (z.pan + off * width).clamp(-1.0, 1.0);
                let mut voice = Voice::with_rate(
                    data.clone(),
                    note,
                    voice_kind.clone(),
                    u_rate,
                    gain * comp,
                    self.release_frames,
                )
                .with_mic_index(mic_index)
                .with_line(line)
                .with_artic_class(artic_class)
                .with_choke_group(choke_group)
                .with_pan(u_pan)
                .with_attack(self.attack_frames)
                .with_sample_window(
                    z.sample_start as usize,
                    (z.sample_end > 0).then_some(z.sample_end as usize),
                );
                if z.playback_mode.eq_ignore_ascii_case("reverse") {
                    voice = voice.reversed();
                } else if zone_is_alternating_loop(z) {
                    voice = voice.with_alternating_loop(z.loop_start as usize, z.loop_end as usize);
                } else {
                    voice = voice.with_forward_loop(z.loop_start as usize, z.loop_end as usize);
                }
                self.voices.spawn(voice);
            }
        }
    }

    /// Process a MIDI CC event.
    pub fn cc(&mut self, controller: u8, value: u8) {
        self.cc_line(0, controller, value);
    }

    /// Line-addressed CC. CC1 (dynamics) and CC2 (vibrato) are per-line
    /// state — a divisi line's expression rides its own controller lane and
    /// re-levels only that line's held voices. Every other controller
    /// (CC58 articulation/mode, CC64 pedal, CC11 volume, …) is engine-global:
    /// divisi desks of one section share articulation, pedal, and output
    /// level. Document CCs carry their channel; the scheduler resolves it to
    /// a line before calling this.
    pub fn cc_line(&mut self, line: LineId, controller: u8, value: u8) {
        self.set_active_line(line);
        let old_value = self
            .cc_values
            .get(controller as usize)
            .copied()
            .unwrap_or(0);
        if let Some(slot) = self.cc_values.get_mut(controller as usize) {
            *slot = value;
        }
        if self.patch.is_zoned() {
            self.trigger_cc_zones(controller, old_value, value);
        }
        match controller {
            1 => {
                self.cc1 = value;
                self.line_mut().cc1 = value;
                // Short-note articulations use CC1 to select sub-type (spiccato/
                // staccato/pizzicato/etc.); sustain articulations use it for dynamics.
                let is_short = self
                    .patch
                    .spec
                    .articulation(&self.articulation)
                    .map(|a| a.kind == ArticulationKind::Short)
                    .unwrap_or(false);
                if is_short {
                    self.apply_cc1_short_select();
                } else {
                    self.update_sustain_gains();
                }
            }
            2 => {
                self.cc2 = value;
                self.line_mut().cc2 = value;
                self.update_sustain_gains();
            }
            58 => {
                self.cc58 = value;
                self.apply_cc58();
            }
            59 => {
                // CC59: round-robin reset (v1.7). Value is the 0-based starting
                // index. Resets all RR counters so the next short-note passage
                // plays back the same RR sequence every time.
                self.rr.borrow_mut().reset_to(value as usize);
                self.zone_rr_counter = value as usize;
                self.zone_rr_last_slots.clear();
            }
            64 => {
                let was_held = self.cc64_held;
                self.cc64_value = value;
                self.cc64_held = value >= 64;

                // Sustain-pedal articulation swap (Keyscape-style libraries
                // ship distinct pedal-down body samples — e.g. `lacrm` strike
                // becomes `lacrped` while pedal is held). Keep `lacrm` as the
                // "no-pedal" default and snap back when the pedal lifts.
                if !was_held && self.cc64_held {
                    let restored = self.voices.repedal_releasing();
                    if restored > 0 {
                        tracing::debug!(restored, "pedal down repedal");
                    }
                    if self.patch.is_zoned() {
                        self.trigger_event_zones(ZoneTrigger::PedalDown, value);
                    }
                    if let Some(pedal_id) = self.find_pedal_pair(&self.articulation) {
                        self.no_pedal_articulation = Some(self.articulation.clone());
                        self.articulation = pedal_id;
                        // Mechanical pedal-down click — one-shot ambience layer
                        // (e.g. `lacrmechped`). Best-effort: skip silently if
                        // the pack doesn't ship one.
                        self.trigger_mechanical_pedal();
                    }
                } else if was_held && !self.cc64_held {
                    if let Some(orig) = self.no_pedal_articulation.take() {
                        self.articulation = orig;
                    }
                    if self.patch.is_zoned() {
                        self.trigger_event_zones(ZoneTrigger::PedalUp, value);
                    }
                }

                if was_held && !self.cc64_held {
                    let release_frames = self.pedal_release_frames();
                    // Pedal released — release only notes that received note-off
                    // while the pedal was down. Notes still physically held have
                    // no deferred note-off entry and must keep sounding.
                    let notes: Vec<(u8, u8)> = self
                        .deferred_note_off_velocities
                        .iter()
                        .map(|(&note, &release_velocity)| (note, release_velocity))
                        .collect();
                    tracing::debug!(
                        deferred = notes.len(),
                        still_held = self.held_notes.len().saturating_sub(notes.len()),
                        "pedal up release"
                    );
                    self.deferred_note_off_velocities.clear();
                    for (note, velocity) in notes {
                        self.held_notes.remove(&note);
                        if self.patch.is_zoned() {
                            self.trigger_zoned(note, velocity, ZoneTrigger::Release, false);
                            self.voices
                                .note_off_with_release_frames(note, Some(release_frames));
                        } else {
                            self.do_note_off_with_release_frames(note, velocity, release_frames);
                        }
                    }
                }
            }
            // CSS "Volume" — master output level (separate from CC1 dynamics).
            11 => self.cc11_volume = value as f32 / 127.0,
            // CSS "Portamento Volume" — level of the portamento glide.
            5 => self.cc5_porta_volume = value as f32 / 127.0,
            // All Sound Off (immediate) / All Notes Off (release held) — standard
            // MIDI panic CCs, so they work through the one MIDI dispatch path.
            120 => self.panic(),
            123 => self.all_notes_off(),
            _ => {}
        }
    }

    pub fn channel_aftertouch(&mut self, value: u8) {
        let old_value = self.channel_aftertouch;
        self.channel_aftertouch = value;
        if self.patch.is_zoned() {
            self.trigger_aftertouch_zones(None, old_value, value);
        }
    }

    pub fn poly_aftertouch(&mut self, note: u8, value: u8) {
        let old_value = self
            .poly_aftertouch
            .get(note as usize)
            .copied()
            .unwrap_or(0);
        if let Some(slot) = self.poly_aftertouch.get_mut(note as usize) {
            *slot = value;
        }
        if self.patch.is_zoned() {
            self.trigger_aftertouch_zones(Some(note), old_value, value);
        }
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
                    ..
                } = std::mem::replace(&mut self.lines[li].state, LegatoState::Idle)
                {
                    self.set_active_line(li);
                    self.fire_legato(from_note, to_note, to_note_velocity, portamento);
                }
            }
        }
    }

    // ── Private — note trigger helpers ────────────────────────────────────────

    fn trigger_sustain(&mut self, note: u8) {
        let vib_blend = self.cc2_blend();
        let nv_scale = 1.0 - vib_blend;
        let vb_scale = vib_blend;

        // `make_voice` and the layer/vibrato helpers are `&self`, so the
        // current selection (`articulation`/`section`/`mic`) is passed by
        // reference directly — no per-note `String` clones.
        let vib_artic = self.find_vibrato_pair_id(&self.articulation);
        let release_frames = self.release_frames;

        // Compute CC1 layers separately for each articulation so that
        // Vibsus (4 dyns: ppp/p/mf/ff) and Nonvib (3 dyns: p/mf/ff) each
        // use their own crossfade map. Without this, Nonvib gets asked for
        // "ppp" samples that don't exist at very low CC1 values.
        let (nv_lo, nv_hi, nv_cc1_blend) = self.layers_for_artic(&self.articulation);
        let nv_lo_gain = nv_scale * (1.0 - nv_cc1_blend);
        let nv_hi_gain = nv_scale * nv_cc1_blend;

        if let Some(v) = self.make_voice(
            &self.articulation,
            &self.section,
            &self.mic,
            &nv_lo,
            note,
            "",
            VoiceKind::SustainNVLo,
            nv_lo_gain,
            release_frames,
        ) {
            self.voices.spawn(v);
        }
        if nv_hi != nv_lo {
            if let Some(v) = self.make_voice(
                &self.articulation,
                &self.section,
                &self.mic,
                &nv_hi,
                note,
                "",
                VoiceKind::SustainNVHi,
                nv_hi_gain,
                release_frames,
            ) {
                self.voices.spawn(v);
            }
        }

        // Vibrato voices — only if a vibrato-pair articulation exists.
        if let Some(vib_id) = vib_artic {
            let (vb_lo, vb_hi, vb_cc1_blend) = self.layers_for_artic(&vib_id);
            let vb_lo_gain = vb_scale * (1.0 - vb_cc1_blend);
            let vb_hi_gain = vb_scale * vb_cc1_blend;

            if let Some(v) = self.make_voice(
                &vib_id,
                &self.section,
                &self.mic,
                &vb_lo,
                note,
                "",
                VoiceKind::SustainVibLo,
                vb_lo_gain,
                release_frames,
            ) {
                self.voices.spawn(v);
            }
            if vb_hi != vb_lo {
                if let Some(v) = self.make_voice(
                    &vib_id,
                    &self.section,
                    &self.mic,
                    &vb_hi,
                    note,
                    "",
                    VoiceKind::SustainVibHi,
                    vb_hi_gain,
                    release_frames,
                ) {
                    self.voices.spawn(v);
                }
            }
        }
    }

    fn trigger_short(&mut self, note: u8, velocity: u8) {
        // Pick dynamic layer based on velocity and spec short_note_cc1_map.
        let dynamic = self.short_note_dynamic(velocity);
        let artic_kind = self
            .patch
            .spec
            .articulation(&self.articulation)
            .map(|a| a.kind.clone());
        let has_release_artic = self
            .patch
            .spec
            .articulation(&self.articulation)
            .and_then(|a| a.release_artic.as_ref())
            .is_some();
        // Voice kind + release length:
        // - has_release_artic (Keyscape pianos, orchestral shorts):
        //   body releases on note-off in KEY_UP_DECLICK_MS (effectively
        //   immediate). The dedicated release_artic supplies the tail.
        // - no release_artic: long natural decay; sample's tail IS the
        //   release.
        let is_oneshot = matches!(artic_kind, Some(ArticulationKind::OneShot));
        let voice_kind = if is_oneshot || has_release_artic {
            VoiceKind::SustainLo
        } else {
            VoiceKind::Short
        };
        let release_frames = if has_release_artic {
            ms_to_frames(KEY_UP_DECLICK_MS, self.sample_rate)
        } else {
            self.release_frames
        };
        // For multi-dynamic articulations (lacrm has 21 dyn-layers), the
        // velocity-selected sample IS already recorded at that loudness,
        // so applying velocity² gain on top double-attenuates the body.
        // Use unity gain when the artic has multiple dynamic layers and
        // fall back to a velocity curve when there's just one layer.
        let n_dyn = self
            .patch
            .spec
            .articulation(&self.articulation)
            .map(|a| a.dynamics.len())
            .unwrap_or(0);
        let gain = if n_dyn > 1 {
            1.0
        } else {
            velocity_gain(velocity)
        };
        if let Some(v) = self.make_voice(
            &self.articulation,
            &self.section,
            &self.mic,
            &dynamic,
            note,
            "",
            voice_kind.clone(),
            gain,
            release_frames,
        ) {
            self.voices.spawn(v);
            self.body_voiced.insert(note);
        } else {
            // Primary articulation has no sample for this note (e.g.
            // Keyscape LA Custom: `lacrm` covers notes 21,22,28-108 but
            // notes 23-27 live in `lacrmsp`). Try every other non-pedal,
            // non-Release OneShot articulation in the spec and use the
            // first that resolves a sample for this note. Without this
            // the user hears only the release-tail click on those notes.
            let primary = self.articulation.clone();
            let alt_ids: Vec<String> = self
                .patch
                .spec
                .articulations
                .iter()
                .filter(|a| matches!(a.kind, ArticulationKind::OneShot | ArticulationKind::Short))
                .filter(|a| {
                    let lower = a.id.to_ascii_lowercase();
                    !lower.contains("ped") && !lower.contains("mech")
                })
                .filter(|a| a.id != primary)
                .map(|a| a.id.clone())
                .collect();
            for alt_id in alt_ids {
                let alt_dyn = self.dynamic_for_artic(&alt_id, velocity);
                if let Some(v) = self.make_voice(
                    &alt_id,
                    &self.section,
                    &self.mic,
                    &alt_dyn,
                    note,
                    "",
                    voice_kind.clone(),
                    gain,
                    release_frames,
                ) {
                    self.voices.spawn(v);
                    self.body_voiced.insert(note);
                    break;
                }
            }
        }
    }

    fn trigger_legzero(&mut self, note: u8, _velocity: u8) {
        // Find a Legato-kind articulation for same-note retrigger.
        let Some(rz_id) = self.find_legato_artic_id(true) else {
            return;
        };
        let (lo_dyn, _, _) = self.current_layers_owned();
        let release_frames = self.release_frames;

        if let Some(v) = self.make_voice(
            &rz_id,
            &self.section,
            &self.mic,
            &lo_dyn,
            note,
            "",
            VoiceKind::Legato,
            1.0,
            release_frames,
        ) {
            self.voices.spawn(v);
        }
    }

    fn initiate_legato(&mut self, to_note: u8, velocity: u8) {
        let from_note = *self
            .held_notes
            .keys()
            .find(|&&n| n != to_note)
            .unwrap_or(&to_note);

        // Check portamento threshold (default 20, velocity ≤ threshold triggers glide).
        let port_thresh = self
            .patch
            .spec
            .legato_engine
            .as_ref()
            .and_then(|le| le.portamento.as_ref())
            .map(|p| p.trigger_vel_max)
            .unwrap_or(0); // 0 disables portamento
        let portamento = port_thresh > 0 && velocity <= port_thresh;

        let delay_ms = if portamento {
            0 // portamento fires immediately — the glide pitch ramp is the "delay"
        } else if self.play_mode == PlayMode::Lookahead {
            self.patch.legato_delay_expressive(velocity).unwrap_or(100)
        } else {
            // StrictLive: low-latency tables, no exceptions (see PlayMode).
            self.patch.legato_delay_low_latency(velocity).unwrap_or(100)
        };

        let frames_remaining = ms_to_frames(delay_ms, self.sample_rate);

        // Reactive path (see start_legato_transition) — count it.
        self.reactive_legato_fires = self.reactive_legato_fires.saturating_add(1);
        self.line_mut().state = LegatoState::Pending {
            frames_remaining,
            from_note,
            to_note,
            to_note_velocity: velocity,
            portamento,
        };
    }

    fn fire_legato(&mut self, from_note: u8, to_note: u8, velocity: u8, portamento: bool) {
        self.fire_legato_with_lead(from_note, to_note, velocity, portamento, None);
    }

    /// [`fire_legato`](Self::fire_legato) with the document scheduler's
    /// prefire lead (frames until the destination tick) — the transition
    /// sample's surplus lead-in is skipped so the pitch change lands exactly
    /// on the tick, and the old note crossfades out across the audible
    /// lead-in.
    fn fire_legato_with_lead(
        &mut self,
        from_note: u8,
        to_note: u8,
        velocity: u8,
        portamento: bool,
        sched_lead: Option<u64>,
    ) {
        if self.legato_fire_log_enabled && self.legato_fire_log.len() < LEGATO_FIRE_LOG_CAP {
            self.legato_fire_log.push(LegatoFireEvent {
                frame: self.frames_rendered,
                line: self.cur_line as u8,
                from_note,
                to_note,
                velocity,
                portamento,
            });
        }
        let direction = if to_note > from_note { "up" } else { "down" };

        // Fade out old sustain voice — on THIS line only, so a unison note
        // held by another divisi line keeps sounding. The fade spans the
        // transition sample's audible lead-in (during which the transition
        // itself sounds the source note), i.e. a true crossfade: old voice
        // out, recorded bow-change in, pitch change on the tick.
        let zone_lead = self.transition_lead_frames(from_note, to_note, portamento);
        let audible_lead = zone_lead
            .map(|zl| sched_lead.map_or(zl, |l| l.min(zl)) as usize)
            .unwrap_or(0);
        let fade = self.legato_fade_frames.max(audible_lead);
        self.voices
            .silence_note_line(self.cur_line as u8, from_note, fade);

        // Zoned libraries (CSS): play the directional legato TRANSITION sample
        // (the recorded bow change into the target) as a one-shot, then start
        // the sustained body (Nonvib/Vibsus) which holds. The transition gives
        // the slur; the sustain holds the note.
        if self.patch.is_zoned() {
            self.play_direction = direction.to_string();
            // The long-form legato sample carries both the bow change AND the
            // sustained target note (it loops its tail to hold). Do NOT also start
            // a sustain body — that doubles the note into a chorus ("weird"
            // legato). If no legato sample exists, fall back to the sustain body.
            let before = self.voices.active_count();
            self.spawn_legato_transition(from_note, to_note, velocity, portamento, sched_lead);
            self.line_mut().note = Some(to_note);
            if self.voices.active_count() == before {
                self.trigger_zoned_sustain(to_note);
            }
            return;
        }

        // For portamento, look for Port-type articulation; otherwise Leg/NVLeg.
        let leg_id = if portamento {
            self.find_port_artic_id()
                .or_else(|| self.find_legato_artic_id(false))
        } else {
            self.find_legato_artic_id(false)
        };

        let Some(leg_id) = leg_id else {
            self.trigger_sustain(to_note);
            return;
        };

        let (lo_dyn, _, _) = self.current_layers_owned();
        let release_frames = self.release_frames;

        // Try directional first; fall back to directionless if not found.
        let v = self
            .make_voice(
                &leg_id,
                &self.section,
                &self.mic,
                &lo_dyn,
                to_note,
                direction,
                VoiceKind::Legato,
                1.0,
                release_frames,
            )
            .or_else(|| {
                self.make_voice(
                    &leg_id,
                    &self.section,
                    &self.mic,
                    &lo_dyn,
                    to_note,
                    "",
                    VoiceKind::Legato,
                    1.0,
                    release_frames,
                )
            });

        if let Some(v) = v {
            self.voices.spawn(v);
        }
        // Always trigger a background sustain so the note doesn't go silent
        // when the legato transition sample finishes. The Leg sample provides
        // the attack character; the Vibsus/Nonvib body takes over after it ends.
        self.trigger_sustain(to_note);
    }

    /// Find the Port articulation matching the current sordino state.
    fn find_port_artic_id(&self) -> Option<String> {
        let want_sord = self.articulation.starts_with("Sord");
        self.patch
            .spec
            .articulations
            .iter()
            .filter(|a| a.kind == ArticulationKind::Legato)
            .filter(|a| a.id.starts_with("Sord") == want_sord)
            .find(|a| a.id.to_lowercase().contains("port"))
            .map(|a| a.id.clone())
    }

    /// Find a sustain-pedal-down sibling of `base` in the spec.
    ///
    /// Naming conventions across libraries vary; we try in priority:
    ///   1. `<base>ped`            — `lacrm` → `lacrmped`
    ///   2. `<base>` with trailing `m` removed + `ped` — `lacrm` → `lacrped`
    ///   3. any non-Release / non-mechanical articulation containing
    ///      `ped` (case-insensitive) — generic fallback
    /// Mechanical-pedal articulations (`mech`/`mechped`) are excluded so the
    /// pedal-down body and the pedal-down mechanical action stay separate.
    fn find_pedal_pair(&self, base: &str) -> Option<String> {
        let id = |s: &str| -> Option<String> {
            self.patch
                .spec
                .articulation(s)
                .filter(|a| !matches!(a.kind, ArticulationKind::Release))
                .map(|a| a.id.clone())
        };
        if let Some(v) = id(&format!("{base}ped")) {
            return Some(v);
        }
        let trimmed = base.strip_suffix('m').unwrap_or(base);
        if trimmed != base {
            if let Some(v) = id(&format!("{trimmed}ped")) {
                return Some(v);
            }
        }
        self.patch
            .spec
            .articulations
            .iter()
            .filter(|a| {
                !matches!(a.kind, ArticulationKind::Release)
                    && a.id.to_ascii_lowercase().contains("ped")
                    && !a.id.to_ascii_lowercase().contains("mech")
            })
            .map(|a| a.id.clone())
            .next()
    }

    /// Fire the mechanical pedal-down click (e.g. `lacrmechped`) once,
    /// at a moderate gain. Pure ambience layer — silently no-ops when the
    /// pack doesn't ship a mechanical-pedal articulation.
    fn trigger_mechanical_pedal(&mut self) {
        let id = self
            .patch
            .spec
            .articulations
            .iter()
            .find(|a| {
                a.id.to_ascii_lowercase().contains("mech")
                    && a.id.to_ascii_lowercase().contains("ped")
            })
            .map(|a| a.id.clone());
        let Some(mech_id) = id else {
            return;
        };
        let dyn_id = self.dynamic_for_artic(&mech_id, 100);
        let release_frames = self.release_frames;
        // Note doesn't matter — these patches map a fixed sample to any
        // input note, so we pick a stable middle-C anchor.
        if let Some(v) = self.make_voice(
            &mech_id,
            &self.section,
            &self.mic,
            &dyn_id,
            60,
            "",
            VoiceKind::Release,
            0.4,
            release_frames,
        ) {
            self.voices.spawn(v);
        }
    }

    fn do_note_off_with_release_frames(&mut self, note: u8, velocity: u8, release_frames: usize) {
        // Trigger release trail if the current articulation specifies one.
        let release_artic = self
            .patch
            .spec
            .articulation(&self.articulation)
            .and_then(|a| a.release_artic.clone());

        if let Some(rel_id) = release_artic
            .as_ref()
            .filter(|_| velocity >= RELEASE_SAMPLE_VELOCITY_MIN)
        {
            // For release-trail dynamics, MIDI controllers commonly send
            // release-velocity 64 as a "no info" default. If we got that
            // exact value, fall through to the body's note-on velocity
            // for layer pick — otherwise the picker would always use the
            // same dynamic regardless of how hard the player struck.
            let dyn_vel = if velocity == 64 {
                self.held_notes.get(&note).copied().unwrap_or(velocity)
            } else {
                velocity
            };
            let rel_dyn = self.dynamic_for_artic(rel_id, dyn_vel);
            // Scale by release velocity so soft note-offs whisper and
            // firm note-offs click clearly. (v/127)^1.5 sits between the
            // body's v² curve and a perceptually-flat linear ramp.
            let v_norm = (velocity as f32 / 127.0).clamp(0.0, 1.0);
            let gain = RELEASE_SAMPLE_GAIN_MAX * v_norm.powf(1.5);
            tracing::debug!(note, velocity, gain, dyn = %rel_dyn, "release sample");

            // Only fire release-tail if the body voice actually sounded —
            // otherwise the user hears just the mechanical key-up click in
            // isolation (a body-cache miss with no audible attack).
            if self.body_voiced.remove(&note) {
                if let Some(v) = self.make_voice(
                    &rel_id,
                    &self.section,
                    &self.mic,
                    &rel_dyn,
                    note,
                    "",
                    VoiceKind::Release,
                    gain,
                    release_frames,
                ) {
                    self.voices.spawn(v);
                }
            }
        } else {
            // No release_artic at all — still need to clear the tracker
            // so a future re-trigger doesn't carry stale state.
            self.body_voiced.remove(&note);
        }

        // Body voices were spawned with a short release window when the
        // articulation has a release_artic (the release sample supplies
        // the natural tail). Use the short cutoff here regardless of the
        // caller's `release_frames` — that parameter is meant for the
        // release voice's own decay, not for the body's fade.
        let body_release = if release_artic.is_some() {
            ms_to_frames(KEY_UP_DECLICK_MS, self.sample_rate)
        } else {
            release_frames
        };
        self.voices
            .note_off_with_release_frames(note, Some(body_release));
    }

    // ── Private — CC handlers ─────────────────────────────────────────────────

    fn pedal_release_frames(&self) -> usize {
        half_pedal_release_frames(
            self.release_frames,
            self.cc64_value,
            &self.patch.spec.dynamics.half_pedal_curve,
            self.patch.spec.dynamics.half_pedal_max_release_multiplier,
        )
    }

    /// Recompute and ramp sustain voice gains when the ACTIVE line's CC1 or
    /// CC2 changes. Only voices belonging to the active line re-level — each
    /// divisi line's dynamics ride its own controller lane. (Live play keeps
    /// everything on line 0, so behavior is unchanged.)
    fn update_sustain_gains(&mut self) {
        let cur_line = self.cur_line as u8;
        // CC2 → non-vib/vib balance (equal-power).
        let (nv, vb) = Self::equal_power(self.cc2_blend());
        let ramp = self.cc1_ramp_frames;
        let nv_artic = self.articulation.clone();
        let vib_artic = self.find_vibrato_pair_id(&nv_artic);

        // Per-side CC1 crossfade across ALL dynamic layers: the active pair
        // (lo,hi) get equal-power gains, every other layer is silent. Held
        // SustainLayer voices are re-levelled by index → full-range swell.
        let (nv_lo, nv_hi, nv_b) = Self::cc1_blend_idx(self.cc1_layers_for(&nv_artic), self.cc1);
        let (nv_lo_g, nv_hi_g) = Self::equal_power(nv_b);
        let (vb_lo, vb_hi, vb_lo_g, vb_hi_g) = match vib_artic.as_deref() {
            Some(vib) => {
                let (l, h, b) = Self::cc1_blend_idx(self.cc1_layers_for(vib), self.cc1);
                let (lg, hg) = Self::equal_power(b);
                (l, h, lg, hg)
            }
            None => (0, 0, 0.0, 0.0),
        };

        // Continuous loudness sweep on top of the (short) timbre crossfade.
        let expr = Self::cc1_expression(self.cc1);
        for v in self.voices.voices_mut() {
            if v.line != cur_line {
                continue;
            }
            if let Some(layer) = v.dyn_layer {
                let i = layer.index as usize;
                let g = if layer.vib {
                    vb * layer_gain(i, vb_lo, vb_hi, vb_lo_g, vb_hi_g)
                } else {
                    nv * layer_gain(i, nv_lo, nv_hi, nv_lo_g, nv_hi_g)
                };
                v.ramp_gain(g * expr, ramp);
            }
        }

        // Legacy 2-layer kinds (non-zoned trigger_sustain path) — unchanged.
        self.voices.update_sustain_blend(
            cur_line,
            nv * nv_lo_g,
            nv * nv_hi_g,
            vb * vb_lo_g,
            vb * vb_hi_g,
            ramp,
        );
    }

    /// Equal-power crossfade gains for a blend in `[0,1]`: returns `(lo, hi)` =
    /// `(cos, sin)` of the quarter-turn. Keeps perceived loudness constant
    /// across the fade — the smooth curve CSS uses for dynamics/vibrato, vs a
    /// linear fade which dips ~3 dB through the middle.
    fn equal_power(blend: f32) -> (f32, f32) {
        let b = blend.clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2;
        (b.cos(), b.sin())
    }

    /// Compute the vibrato blend factor [0.0, 1.0] from the current CC2 value.
    ///
    /// `"on_off"` mode (CSSS): snaps at 64. All other libraries: linear.
    fn cc2_blend(&self) -> f32 {
        match self.patch.spec.dynamics.vibrato_mode.as_deref() {
            Some("on_off") => {
                if self.cc2 >= 64 {
                    1.0
                } else {
                    0.0
                }
            }
            _ => self.cc2 as f32 / 127.0,
        }
    }

    /// Find the vibrato counterpart of `artic_id`.
    ///
    /// CSS/CSSS convention: if the current artic has no "NV"/"Nonvib" in its
    /// name we look for one that does (and vice-versa), staying within the
    /// same family (Con Sordino vs regular).
    fn find_vibrato_pair_id(&self, artic_id: &str) -> Option<String> {
        // Only applies when CC2 is the vibrato controller.
        self.patch.spec.dynamics.vibrato_controller.as_deref()?;

        let id_lower = artic_id.to_lowercase();
        let is_sord = id_lower.contains("sord");
        let is_nv = id_lower.contains("nv") || id_lower.contains("nonvib");

        // The vibrato pair lives in the same articulation family — Sustain
        // (Vibsus↔Nonvib) or Legato (Leg↔NVLeg) — so legato gets CC2 vibrato too.
        let want_kind = self
            .patch
            .spec
            .articulation(artic_id)
            .map(|a| a.kind.clone());
        self.patch
            .spec
            .articulations
            .iter()
            .filter(|a| a.id != artic_id)
            .filter(|a| Some(&a.kind) == want_kind.as_ref())
            .filter(|a| matches!(a.kind, ArticulationKind::Sustain | ArticulationKind::Legato))
            .filter(|a| {
                let other = a.id.to_lowercase();
                // Same family (sord vs non-sord)
                other.contains("sord") == is_sord
                    // Opposite vibrato side
                    && (other.contains("nv") || other.contains("nonvib")) != is_nv
            })
            .map(|a| a.id.clone())
            .next()
    }

    /// Map CC58 → action and execute it.
    ///
    /// Several CC58 values are **mode switches** rather than articulation selectors:
    /// - `"Con Sordino On"` / `"Con Sordino Off"` → toggle sordino sample set
    /// - `"Legato On"` / `"Legato Off"` → enable/disable legato processing
    /// - `"Sustain: Low Latency Legato"` / `"Sustain: Expressive Legato"` → select
    ///   legato pre-delay mode (latency/expressive) without changing the articulation
    ///
    /// All other labels are treated as articulation IDs or display labels. If Con
    /// Sordino mode is active, the matched articulation is remapped to its sordino
    /// counterpart.
    /// If `note` is a configured keyswitch, apply its velocity-mapped value and
    /// return `true` (the note is consumed — keyswitches don't sound).
    fn try_keyswitch(&mut self, note: u8, velocity: u8) -> bool {
        let Some(&idx) = self.keyswitch_notes.get(&note) else {
            return false;
        };
        let value = self
            .patch
            .spec
            .keyswitch
            .as_ref()
            .and_then(|ks| ks.notes.get(idx))
            .and_then(|kn| kn.value_for(velocity))
            .map(|s| s.to_string());
        if let Some(v) = value {
            self.apply_keyswitch_value(&v);
        }
        true
    }

    /// Apply a keyswitch value: `+`-joined tokens, each either an `@mode` token
    /// (`@legato-on`, `@legato-expressive`, `@sordino-off`, …) or a zone
    /// articulation tag (`Spiccato`, `Leg`, …).
    fn apply_keyswitch_value(&mut self, value: &str) {
        for tok in value.split('+').map(str::trim).filter(|t| !t.is_empty()) {
            match tok {
                "@ignore" => {}
                "@legato-on" => self.legato_enabled = true,
                "@legato-off" => self.legato_enabled = false,
                "@legato-low" => {
                    self.legato_enabled = true;
                    self.legato_expressive = false;
                }
                "@legato-expressive" => {
                    self.legato_enabled = true;
                    self.legato_expressive = true;
                }
                "@sordino-on" => self.set_con_sordino(true),
                "@sordino-off" => self.set_con_sordino(false),
                // Force full non-vibrato (the CSS Non-Vib keyswitch): CC2 → 0.
                "@novib" => {
                    self.cc2 = 0;
                    self.line_mut().cc2 = 0;
                    self.update_sustain_gains();
                }
                t if t.starts_with('@') => tracing::debug!("unknown keyswitch token {t:?}"),
                tag => self.select_articulation_tag(tag),
            }
        }
    }

    /// Select an articulation by tag/id/label: prefer a matching declared
    /// articulation, else use the tag verbatim (zone tags like `"Leg"` aren't
    /// always declared as articulations). Honours Con Sordino remapping.
    fn select_articulation_tag(&mut self, tag: &str) {
        let id = self
            .patch
            .spec
            .articulations
            .iter()
            .find(|a| a.id.eq_ignore_ascii_case(tag) || a.label.eq_ignore_ascii_case(tag))
            .map(|a| a.id.clone())
            .unwrap_or_else(|| tag.to_string());
        self.articulation = self.remap_sordino(&id, self.con_sordino);
    }

    fn apply_cc58(&mut self) {
        let Some(ks) = self.patch.spec.keyswitch.as_ref() else {
            return;
        };
        let Some(label) = ks.cc58_function(self.cc58) else {
            return;
        };
        let label = label.to_string();

        // ── Mode switches ────────────────────────────────────────────────────
        match label.as_str() {
            "Con Sordino On" => {
                self.set_con_sordino(true);
                return;
            }
            "Con Sordino Off" => {
                self.set_con_sordino(false);
                return;
            }
            "Legato On" => {
                self.legato_enabled = true;
                return;
            }
            "Legato Off" => {
                self.legato_enabled = false;
                return;
            }
            "Sustain: Low Latency Legato" => {
                self.legato_enabled = true;
                self.legato_expressive = false;
                self.select_articulation_tag("Nonvib");
                return;
            }
            "Sustain: Expressive Legato" => {
                self.legato_enabled = true;
                self.legato_expressive = true;
                self.select_articulation_tag("Nonvib");
                return;
            }
            "Measured Tremolo" => {
                // Scripted mode — no dedicated samples. Cannot replicate without a
                // built-in scripted repeating trigger. Ignore for now.
                return;
            }
            _ => {}
        }

        // ── Articulation selection ───────────────────────────────────────────
        let matched = self
            .patch
            .spec
            .articulations
            .iter()
            .find(|a| a.id == label || a.label == label)
            .map(|a| a.id.clone());

        if let Some(id) = matched {
            self.articulation = self.remap_sordino(&id, self.con_sordino);
        }
    }

    // ── Private — helpers ─────────────────────────────────────────────────────

    /// Build a `Voice` for a resolved sample, or `None` if the sample can't
    /// be found or loaded.
    #[allow(clippy::too_many_arguments)]
    fn make_voice(
        &self,
        artic_id: &str,
        section: &str,
        mic: &str,
        dynamic: &str,
        note: u8,
        direction: &str,
        kind: VoiceKind,
        gain: f32,
        release_frames: usize,
    ) -> Option<Voice> {
        let max_rr = self
            .patch
            .spec
            .articulation(artic_id)
            .map(|a| a.rr)
            .unwrap_or(1);

        let rr_idx = self
            .rr
            .borrow_mut()
            .next(section, artic_id, dynamic, max_rr);

        let (path, sampled_note) = match self
            .patch
            .resolve(section, artic_id, mic, dynamic, note, direction, rr_idx)
        {
            Some(resolved) => resolved,
            None => {
                self.sample_misses
                    .set(self.sample_misses.get().saturating_add(1));
                self.record_sample_miss(format!(
                    "section={section} artic={artic_id} mic={mic} dynamic={dynamic} note={note} direction={direction:?} rr={rr_idx}"
                ));
                tracing::debug!(
                    section,
                    artic = artic_id,
                    mic,
                    dynamic,
                    note,
                    direction = ?direction,
                    rr = rr_idx,
                    "sample miss"
                );
                return None;
            }
        };

        // Audio-thread fast path: skip silently when not yet preloaded.
        let Some(data) = self.cache.get_loaded(&path) else {
            self.cache_misses
                .set(self.cache_misses.get().saturating_add(1));
            self.record_cache_miss(&path);
            tracing::trace!("sample not yet loaded: {}", path.display());
            return None;
        };

        let semitone_offset = note as i16 - sampled_note as i16;
        let mic_index = self.mic_index_for(mic);
        // Cap Release-voice lifetime. lacr release-tail FLACs are 30 s of
        // mostly silence; without this they live in the pool until the
        // sample's natural end and chew through the 64-voice budget, which
        // forces voice-stealing on every subsequent key press and produces
        // intermittent silence/clicks. 2 s is plenty for any real damper
        // click + decay tail.
        let release_lifetime_frames =
            (RELEASE_MAX_LIFETIME_MS as usize) * (self.sample_rate as usize) / 1000;
        // Stem class: releases follow the parent articulation (see
        // `artic_class_for`); direct triggers are classed by their own.
        let artic_class = if matches!(kind, VoiceKind::Release) {
            self.artic_class_for(&self.articulation)
        } else {
            self.artic_class_for(artic_id)
        };
        let voice = Voice::new(
            data,
            note,
            kind.clone(),
            semitone_offset.clamp(i8::MIN as i16, i8::MAX as i16) as i8,
            gain,
            release_frames,
        )
        .with_mic_index(mic_index)
        .with_line(self.cur_line as u8)
        .with_artic_class(artic_class);
        let voice = if matches!(kind, VoiceKind::Release) {
            let end = release_lifetime_frames.min(voice.data_num_frames());
            voice.with_sample_window(0, Some(end))
        } else {
            voice
        };
        Some(voice)
    }

    /// Returns `(lo_label, hi_label, hi_blend)` for a specific articulation ID
    /// at the current CC1 value, using that articulation's own dynamics count.
    fn layers_for_artic(&self, artic_id: &str) -> (String, String, f32) {
        let n = self
            .patch
            .spec
            .articulation(artic_id)
            .map(|a| a.dynamics.len())
            .unwrap_or(0);
        let d = &self.patch.spec.dynamics;
        let layers: &[Cc1Layer] = match n {
            2 => &d.cc1_layers_2,
            3 => &d.cc1_layers_3,
            4 => &d.cc1_layers_4,
            5 => &d.cc1_layers_5,
            6 => &d.cc1_layers_6,
            _ => return ("p".into(), "p".into(), 0.0),
        };
        Self::cc1_blend(layers, self.cc1)
    }

    fn record_cache_miss(&self, path: &Path) {
        push_recent(
            &mut self.recent_cache_misses.borrow_mut(),
            path.display().to_string(),
            RECENT_MISS_LIMIT,
        );
    }

    fn record_sample_miss(&self, label: String) {
        push_recent(
            &mut self.recent_sample_misses.borrow_mut(),
            label,
            RECENT_MISS_LIMIT,
        );
    }

    /// Returns `(lo_label, hi_label, hi_blend)` for the current CC1 value,
    /// using the active articulation's own layer set.
    fn current_layers_owned(&self) -> (String, String, f32) {
        Self::cc1_blend(self.active_cc1_layers(), self.cc1)
    }

    /// Core crossfade algorithm: walk adjacent layer pairs and return
    /// `(lo_label, hi_label, hi_blend)` for a given CC value.
    fn cc1_blend(layers: &[Cc1Layer], cc1: u8) -> (String, String, f32) {
        if layers.is_empty() {
            return ("p".into(), "p".into(), 0.0);
        }
        // Walk through adjacent pairs. The crossfade region between layer i
        // and layer i+1 is [layers[i+1].cc_range[0], layers[i].cc_range[1]].
        for i in 0..layers.len().saturating_sub(1) {
            let lo = &layers[i];
            let hi = &layers[i + 1];
            let xfade_start = hi.cc_range[0];
            let xfade_end = lo.cc_range[1];

            if cc1 <= xfade_end {
                if cc1 < xfade_start {
                    return (lo.label.clone(), lo.label.clone(), 0.0);
                } else {
                    let span = (xfade_end - xfade_start + 1).max(1) as f32;
                    let blend = (cc1 - xfade_start) as f32 / span;
                    return (lo.label.clone(), hi.label.clone(), blend);
                }
            }
        }
        let top = &layers[layers.len() - 1];
        (top.label.clone(), top.label.clone(), 0.0)
    }

    /// Active layer pair `(lo, hi, blend)` for the N-layer zoned crossfade.
    /// Crossfade only within each layer pair's overlap (`cc_range`) — kept SHORT
    /// because CSS crossfades differently-recorded dynamic samples briefly to
    /// avoid phasing/doubling. The continuous loudness sweep is handled
    /// separately by [`cc1_expression`](Self::cc1_expression), so short timbre
    /// crossfades don't make CC1 feel stepped.
    fn cc1_blend_idx(layers: &[Cc1Layer], cc1: u8) -> (usize, usize, f32) {
        if layers.is_empty() {
            return (0, 0, 0.0);
        }
        for i in 0..layers.len() - 1 {
            let xfade_start = layers[i + 1].cc_range[0];
            let xfade_end = layers[i].cc_range[1];
            if cc1 <= xfade_end {
                if cc1 < xfade_start {
                    return (i, i, 0.0);
                }
                let span = (xfade_end - xfade_start + 1).max(1) as f32;
                return (i, i + 1, (cc1 - xfade_start) as f32 / span);
            }
        }
        let top = layers.len() - 1;
        (top, top, 0.0)
    }

    /// Continuous CC1 → loudness curve applied on top of the (short) dynamic
    /// crossfade — this is what makes the dynamics feel smooth across the whole
    /// 0–127 range rather than stepping between the recorded layers. A dB ramp
    /// from `CC1_DYN_FLOOR_DB` at CC1=0 up to 0 dB at CC1=127.
    fn cc1_expression(cc1: u8) -> f32 {
        let t = cc1 as f32 / 127.0;
        let db = CC1_DYN_FLOOR_DB * (1.0 - t);
        10f32.powf(db / 20.0)
    }

    /// The CC1 layer slice for a given articulation (by its dynamics count).
    fn cc1_layers_for(&self, artic_id: &str) -> &[Cc1Layer] {
        let Some(artic) = self.patch.spec.articulation(artic_id) else {
            return &[];
        };
        let d = &self.patch.spec.dynamics;
        match artic.dynamics.len() {
            2 => &d.cc1_layers_2,
            3 => &d.cc1_layers_3,
            4 => &d.cc1_layers_4,
            5 => &d.cc1_layers_5,
            6 => &d.cc1_layers_6,
            _ => &[],
        }
    }

    /// Return the correct CC1 layer slice for the current articulation.
    fn active_cc1_layers(&self) -> &[Cc1Layer] {
        let Some(artic) = self.patch.spec.articulation(&self.articulation) else {
            return &[];
        };
        let n = artic.dynamics.len();
        let d = &self.patch.spec.dynamics;
        match n {
            2 => &d.cc1_layers_2,
            3 => &d.cc1_layers_3,
            4 => &d.cc1_layers_4,
            5 => &d.cc1_layers_5,
            6 => &d.cc1_layers_6,
            _ => &[],
        }
    }

    /// Determine the dynamic layer label for a short note from velocity.
    ///
    /// Named orchestral dynamics (`pp`, `mf`, `fff`, etc.) divide the MIDI
    /// velocity range evenly across the articulation's `dynamics` array.
    /// Numeric Keyscape-style dynamics are different: the labels are the
    /// recorded source velocities, so pick the nearest recorded value.
    fn short_note_dynamic(&self, velocity: u8) -> String {
        self.dynamic_for_artic(&self.articulation, velocity)
    }

    fn dynamic_for_artic(&self, artic_id: &str, velocity: u8) -> String {
        let Some(artic) = self.patch.spec.articulation(artic_id) else {
            return "p".into();
        };
        if artic.dynamics.is_empty() {
            return "p".into();
        }
        dynamic_label_for_velocity(&artic.dynamics, velocity).unwrap_or_else(|| "p".into())
    }

    /// When CC1 moves and the active articulation is a short-note type, switch
    /// to the sub-type that corresponds to the new CC1 value.
    ///
    /// CSS maps:
    ///   `short_note_cc1_map`  — Spiccato / Staccatissimo / Staccato / Sfz
    ///   `pizzicato_cc1_map`   — Pizzicato / Bartokpizz / Clegno
    fn apply_cc1_short_select(&mut self) {
        let d = &self.patch.spec.dynamics;

        // Determine which map to consult based on current articulation family.
        let in_pizz_family = d
            .pizzicato_cc1_map
            .values()
            .any(|id| id == &self.articulation);
        let in_short_family = d
            .short_note_cc1_map
            .values()
            .any(|id| id == &self.articulation);

        if !in_short_family && !in_pizz_family {
            return; // not in a switchable short-note family
        }

        let map = if in_pizz_family {
            &d.pizzicato_cc1_map
        } else {
            &d.short_note_cc1_map
        };
        let cc1 = self.cc1;

        for (range_str, artic_id) in map {
            if let Some((lo, hi)) = crate::spec::parse_range(range_str) {
                if cc1 >= lo && cc1 <= hi {
                    if self.patch.spec.articulation(artic_id).is_some() {
                        self.articulation = artic_id.clone();
                    }
                    return;
                }
            }
        }
    }

    /// Find the ID of a Legato-type articulation appropriate for the current
    /// section. `retrigger` selects the same-note (Legzero) variant.
    ///
    /// Automatically matches:
    /// - Sordino state: `"Sord"` prefix ↔ current articulation prefix.
    /// - Vibrato state: if the current articulation is NV-type (Nonvib/NVLeg),
    ///   prefer `NVLeg`; otherwise prefer `Leg`. Falls back to any match if
    ///   the preferred variant is absent.
    fn find_legato_artic_id(&self, retrigger: bool) -> Option<String> {
        let want_sord = self.articulation.starts_with("Sord");
        let artic_lower = self.articulation.to_lowercase();
        let prefer_nv = artic_lower.contains("nv") || artic_lower.contains("nonvib");

        let candidates: Vec<&crate::spec::ArticulationSpec> = self
            .patch
            .spec
            .articulations
            .iter()
            .filter(|a| a.kind == ArticulationKind::Legato)
            .filter(|a| {
                a.instrument_filter.is_empty() || a.instrument_filter.contains(&self.section)
            })
            .filter(|a| a.id.starts_with("Sord") == want_sord)
            .filter(|a| {
                let id_lower = a.id.to_lowercase();
                if retrigger {
                    id_lower.contains("zero")
                } else {
                    !id_lower.contains("zero")
                }
            })
            .collect();

        // Prefer NVLeg when in non-vibrato mode, Leg otherwise.
        let preferred = if prefer_nv {
            candidates
                .iter()
                .find(|a| a.id.to_lowercase().contains("nv"))
        } else {
            candidates
                .iter()
                .find(|a| !a.id.to_lowercase().contains("nv"))
        };

        preferred
            .or_else(|| candidates.first())
            .map(|a| a.id.clone())
    }

    /// Map an articulation ID to/from its Con Sordino counterpart.
    ///
    /// `"Vibsus"` + `active=true`  → `"SordVibsus"` (if it exists in the spec)
    /// `"SordVibsus"` + `active=false` → `"Vibsus"` (if it exists in the spec)
    /// Returns the original ID unchanged if no counterpart is found.
    fn remap_sordino(&self, artic_id: &str, active: bool) -> String {
        if active {
            if !artic_id.starts_with("Sord") {
                let sord_id = format!("Sord{artic_id}");
                if self.patch.spec.articulation(&sord_id).is_some() {
                    return sord_id;
                }
            }
        } else if let Some(base) = artic_id.strip_prefix("Sord") {
            if self.patch.spec.articulation(base).is_some() {
                return base.to_string();
            }
        }
        artic_id.to_string()
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
mod tests {
    use super::*;

    #[test]
    fn ms_to_frames_44100() {
        assert_eq!(ms_to_frames(0, 44100), 0);
        assert_eq!(ms_to_frames(1000, 44100), 44100);
        assert_eq!(ms_to_frames(100, 44100), 4410);
    }

    #[test]
    fn numeric_dynamic_labels_use_nearest_recorded_velocity() {
        let dynamics = [
            "24", "35", "42", "45", "54", "61", "68", "76", "82", "84", "89", "96", "102", "106",
            "110", "114", "118", "121", "124", "126", "127",
        ]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();

        assert_eq!(
            dynamic_label_for_velocity(&dynamics, 40).as_deref(),
            Some("42")
        );
        assert_eq!(
            dynamic_label_for_velocity(&dynamics, 100).as_deref(),
            Some("102")
        );
        assert_eq!(
            dynamic_label_for_velocity(&dynamics, 120).as_deref(),
            Some("121")
        );
    }

    #[test]
    fn named_dynamic_labels_keep_even_velocity_bands() {
        let dynamics = ["pp", "mp", "f", "fff"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();

        assert_eq!(
            dynamic_label_for_velocity(&dynamics, 31).as_deref(),
            Some("pp")
        );
        assert_eq!(
            dynamic_label_for_velocity(&dynamics, 32).as_deref(),
            Some("mp")
        );
        assert_eq!(
            dynamic_label_for_velocity(&dynamics, 96).as_deref(),
            Some("fff")
        );
    }

    fn test_zone(rr_index: u32) -> crate::spec::ZoneSpec {
        crate::spec::ZoneSpec {
            file: format!("rr-{rr_index}.wav"),
            key_min: 60,
            key_max: 60,
            root_key: 60,
            vel_min: 0,
            vel_max: 127,
            rr_index,
            rr_mode: String::new(),
            gain_db: 0.0,
            pan: 0.0,
            tune_cents: 0.0,
            sample_start: 0,
            sample_end: 0,
            loop_start: 0,
            loop_end: 0,
            playback_mode: String::new(),
            trigger_mode: String::new(),
            trigger_cc: 0,
            trigger_value_min: 0,
            trigger_value_max: 0,
            mic: String::new(),
            articulation: String::new(),
            dynamic: String::new(),
            direction: String::new(),
            interval: 0,
            lead_in_ms: 0.0,
            group: String::new(),
            group_polyphony: 0,
            choke_group: String::new(),
            off_by: Vec::new(),
            section: String::new(),
            variant: String::new(),
        }
    }

    #[test]
    fn zone_rr_selection_uses_declared_rr_index() {
        let zones = vec![test_zone(10), test_zone(20), test_zone(30)];
        let indices = vec![0, 1, 2];
        let mut rng = 1;

        assert_eq!(
            select_zone_rr_slot(&zones, &indices, 0, None, &mut rng, None),
            10
        );
        assert_eq!(
            select_zone_rr_slot(&zones, &indices, 1, None, &mut rng, None),
            20
        );
        assert_eq!(
            select_zone_rr_slot(&zones, &indices, 2, None, &mut rng, None),
            30
        );
        assert_eq!(
            select_zone_rr_slot(&zones, &indices, 3, None, &mut rng, None),
            10
        );
        assert_eq!(select_zone_rr_index_by_slot(&zones, &indices, 20), 1);
    }

    #[test]
    fn zone_rr_selection_keeps_multimic_slots_aligned() {
        let zones = vec![test_zone(2), test_zone(0), test_zone(2), test_zone(0)];
        let left_mic = vec![0, 1];
        let right_mic = vec![2, 3];
        let mut rng = 1;
        let first_slot = select_zone_rr_slot(&zones, &left_mic, 0, None, &mut rng, None);
        let second_slot = select_zone_rr_slot(&zones, &left_mic, 1, None, &mut rng, None);

        assert_eq!(first_slot, 0);
        assert_eq!(second_slot, 2);
        assert_eq!(
            select_zone_rr_index_by_slot(&zones, &left_mic, first_slot),
            1
        );
        assert_eq!(
            select_zone_rr_index_by_slot(&zones, &right_mic, first_slot),
            3
        );
        assert_eq!(
            select_zone_rr_index_by_slot(&zones, &left_mic, second_slot),
            0
        );
        assert_eq!(
            select_zone_rr_index_by_slot(&zones, &right_mic, second_slot),
            2
        );
    }

    #[test]
    fn zone_rr_no_repeat_random_avoids_previous_slot() {
        let mut zones = vec![test_zone(0), test_zone(1), test_zone(2)];
        for zone in &mut zones {
            zone.rr_mode = "no-repeat-random".to_string();
        }
        let indices = vec![0, 1, 2];
        let mut rng = 0x9e37_79b9_7f4a_7c15;
        let mut last = None;

        for _ in 0..64 {
            let slot = select_zone_rr_slot(&zones, &indices, 0, last, &mut rng, None);
            assert_ne!(Some(slot), last);
            last = Some(slot);
        }
    }

    #[test]
    fn half_pedal_scales_release_frames() {
        let base = 24_000;

        assert_eq!(half_pedal_release_frames(base, 0, "", 0.0), base);
        assert!(half_pedal_release_frames(base, 32, "", 0.0) > base);
        assert_eq!(
            half_pedal_release_frames(base, 63, "", 0.0),
            (base as f32 * HALF_PEDAL_MAX_RELEASE_MULTIPLIER).round() as usize
        );
        assert_eq!(half_pedal_release_frames(base, 64, "", 0.0), base);
    }

    #[test]
    fn half_pedal_uses_authored_curve_and_multiplier() {
        let base = 24_000;
        let linear = half_pedal_release_frames(base, 32, "linear", 3.0);
        let squared = half_pedal_release_frames(base, 32, "squared", 3.0);
        let sqrt = half_pedal_release_frames(base, 32, "sqrt", 3.0);

        assert!(squared < linear);
        assert!(sqrt > linear);
        assert_eq!(half_pedal_release_frames(base, 63, "linear", 2.0), base * 2);
    }

    #[test]
    fn pedal_zone_triggers_are_exclusive_to_pedal_events() {
        let mut pedal_down = test_zone(0);
        pedal_down.trigger_mode = "pedal-down".to_string();
        let mut pedal_up = test_zone(0);
        pedal_up.trigger_mode = "pedal-up".to_string();

        assert!(zone_trigger_matches(&pedal_down, ZoneTrigger::PedalDown));
        assert!(!zone_trigger_matches(&pedal_down, ZoneTrigger::Attack));
        assert!(!zone_trigger_matches(&pedal_down, ZoneTrigger::Release));
        assert!(zone_trigger_matches(&pedal_up, ZoneTrigger::PedalUp));
        assert!(!zone_trigger_matches(&pedal_up, ZoneTrigger::Attack));
        assert!(!zone_trigger_matches(&pedal_up, ZoneTrigger::Release));
    }

    #[test]
    fn cc_zone_trigger_fires_on_threshold_entry_only() {
        let mut zone = test_zone(0);
        zone.trigger_mode = "cc-threshold".to_string();
        zone.trigger_cc = 11;
        zone.trigger_value_min = 40;
        zone.trigger_value_max = 80;

        assert!(zone_cc_trigger_crossed(&zone, 11, 39, 40));
        assert!(!zone_cc_trigger_crossed(&zone, 11, 40, 60));
        assert!(!zone_cc_trigger_crossed(&zone, 11, 20, 90));
        assert!(!zone_cc_trigger_crossed(&zone, 1, 39, 40));
        assert!(!zone_trigger_matches(&zone, ZoneTrigger::Attack));
    }

    #[test]
    fn aftertouch_zone_trigger_fires_on_threshold_entry_only() {
        let mut zone = test_zone(0);
        zone.trigger_mode = "aftertouch".to_string();
        zone.trigger_value_min = 30;
        zone.trigger_value_max = 90;

        assert!(zone_aftertouch_trigger_crossed(&zone, None, 29, 30));
        assert!(!zone_aftertouch_trigger_crossed(&zone, None, 40, 80));
        assert!(!zone_aftertouch_trigger_crossed(&zone, None, 29, 100));
        assert!(zone_aftertouch_trigger_crossed(&zone, Some(60), 29, 30));
        assert!(!zone_aftertouch_trigger_crossed(&zone, Some(61), 29, 30));
        assert!(!zone_trigger_matches(&zone, ZoneTrigger::Attack));
    }

    #[test]
    fn cc1_layer_selection() {
        // Simulate a 3-layer [p=0-42, mf=33-94, ff=85-127] setup.
        use crate::spec::Cc1Layer;

        // Build a minimal PlayerPatch-less engine via a stub spec.
        // We test current_layers_owned() logic in isolation by constructing
        // a mock layers slice and exercising the algorithm directly.
        let layers: &[Cc1Layer] = &[
            Cc1Layer {
                label: "p".into(),
                cc_range: [0, 42],
            },
            Cc1Layer {
                label: "mf".into(),
                cc_range: [33, 94],
            },
            Cc1Layer {
                label: "ff".into(),
                cc_range: [85, 127],
            },
        ];

        // Inline the algorithm (same logic as current_layers_owned).
        let probe = |cc1: u8| -> (String, String, f32) {
            for i in 0..layers.len().saturating_sub(1) {
                let lo = &layers[i];
                let hi = &layers[i + 1];
                let xs = hi.cc_range[0];
                let xe = lo.cc_range[1];
                if cc1 <= xe {
                    if cc1 < xs {
                        return (lo.label.clone(), lo.label.clone(), 0.0);
                    } else {
                        let span = (xe - xs + 1).max(1) as f32;
                        let blend = (cc1 - xs) as f32 / span;
                        return (lo.label.clone(), hi.label.clone(), blend);
                    }
                }
            }
            let top = &layers[layers.len() - 1];
            (top.label.clone(), top.label.clone(), 0.0)
        };

        let (lo, hi, blend) = probe(10);
        assert_eq!(lo, "p");
        assert_eq!(hi, "p");
        assert_eq!(blend, 0.0);

        let (lo, hi, blend) = probe(33);
        assert_eq!(lo, "p");
        assert_eq!(hi, "mf");
        assert!(blend >= 0.0 && blend <= 1.0);

        let (lo, hi, blend) = probe(50);
        assert_eq!(lo, "mf");
        assert_eq!(hi, "mf");
        assert_eq!(blend, 0.0);

        let (lo, hi, _blend) = probe(127);
        assert_eq!(lo, "ff");
        assert_eq!(hi, "ff");
    }

    #[test]
    fn recent_miss_ring_keeps_bounded_latest_values() {
        let mut recent = VecDeque::new();
        for idx in 0..10 {
            push_recent(&mut recent, format!("miss-{idx}"), 4);
        }
        push_recent(&mut recent, "miss-9".to_string(), 4);

        assert_eq!(recent.len(), 4);
        assert_eq!(recent.front().map(String::as_str), Some("miss-6"));
        assert_eq!(recent.back().map(String::as_str), Some("miss-9"));
    }

    #[test]
    fn con_sordino_remap() {
        // Load the CSS spec and exercise the sordino switch logic using the
        // real articulation list.
        let specs_dir = {
            let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
            std::path::Path::new(&manifest)
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("specs")
        };
        let spec_path = specs_dir.join("cinematic-strings.toml");
        if !spec_path.exists() {
            return;
        }

        let spec = crate::LibrarySpec::from_file(&spec_path).expect("load CSS spec");
        let patch = crate::PlayerPatch::from_spec(spec);

        let mut engine = SampleEngine::new(patch, 44100, "1v", "Mix");
        engine.set_articulation("Vibsus");
        assert!(!engine.con_sordino());

        // Enable Con Sordino — should remap to SordVibsus.
        engine.set_con_sordino(true);
        assert!(engine.con_sordino());
        assert_eq!(engine.articulation(), "SordVibsus");

        // Disable — should revert to Vibsus.
        engine.set_con_sordino(false);
        assert_eq!(engine.articulation(), "Vibsus");

        // Nonvib → SordNonvib and back.
        engine.set_articulation("Nonvib");
        engine.set_con_sordino(true);
        assert_eq!(engine.articulation(), "SordNonvib");
        engine.set_con_sordino(false);
        assert_eq!(engine.articulation(), "Nonvib");
    }

    fn engine_from_styx(styx: &str) -> SampleEngine {
        let spec = crate::LibrarySpec::from_styx(styx).expect("parse styx");
        let patch = crate::PlayerPatch::from_spec(spec);
        SampleEngine::new(patch, 48_000, "", "")
    }

    #[test]
    fn percussion_single_key_fires_on_any_note() {
        // Kick-style pack: drum-kit, one articulation on a single key.
        let eng = engine_from_styx(
            "name \"k\"\n\
             category \"drum-kit\"\n\
             zones (\n\
               {\n\
                 file \"a.wav\"\n\
                 key_min 36\n\
                 key_max 36\n\
                 root_key 36\n\
                 vel_min 0\n\
                 vel_max 127\n\
                 articulation \"Hit\"\n\
               }\n\
             )\n",
        );
        assert!(eng.percussion && eng.single_attack_key);
        let z = &eng.patch().spec.zones[0];
        // Plays on the routed note even though it's off the zone's key.
        assert!(eng.zone_selected(z, 35, 100, ZoneTrigger::Attack));
        assert!(eng.zone_selected(z, 48, 100, ZoneTrigger::Attack));
        // Velocity still gates.
        assert!(!eng.zone_selected(
            &crate::spec::ZoneSpec {
                vel_min: 64,
                vel_max: 127,
                ..z.clone()
            },
            35,
            10,
            ZoneTrigger::Attack
        ));
    }

    #[test]
    fn keyswitch_note_selects_articulation_velocity_sensitive() {
        // Melodic patch (no drum category) with two-articulation zones + a
        // velocity-sensitive keyswitch note (CSS-style).
        let mut eng = engine_from_styx(
            "name \"s\"\n\
             keyswitch {\n\
               notes (\n\
                 {\n\
                   note \"C0\"\n\
                   label \"Sustain\"\n\
                   vel_map { 0-127 \"Leg\" }\n\
                 }\n\
                 {\n\
                   note \"C#0\"\n\
                   label \"Shorts\"\n\
                   vel_map {\n\
                     0-64 \"Spiccato\"\n\
                     65-127 \"Staccato\"\n\
                   }\n\
                 }\n\
               )\n\
             }\n\
             zones (\n\
               {file \"leg.wav\", key_min 60, key_max 60, root_key 60, vel_min 0, vel_max 127, articulation \"Leg\"}\n\
               {file \"spic.wav\", key_min 60, key_max 60, root_key 60, vel_min 0, vel_max 127, articulation \"Spiccato\"}\n\
               {file \"stac.wav\", key_min 60, key_max 60, root_key 60, vel_min 0, vel_max 127, articulation \"Staccato\"}\n\
             )\n",
        );
        let leg = eng.patch().spec.zones[0].clone();
        let spic = eng.patch().spec.zones[1].clone();
        let stac = eng.patch().spec.zones[2].clone();

        // A keyswitch note is consumed (returns true) and selects its artic.
        assert!(eng.try_keyswitch(12, 100)); // C0 = MIDI 12 → Leg
        assert_eq!(eng.articulation(), "Leg");
        assert!(eng.zone_selected(&leg, 60, 100, ZoneTrigger::Attack));
        assert!(!eng.zone_selected(&spic, 60, 100, ZoneTrigger::Attack));

        // Velocity on C#0 picks the variant: soft = Spiccato, hard = Staccato.
        assert!(eng.try_keyswitch(13, 30));
        assert_eq!(eng.articulation(), "Spiccato");
        assert!(eng.zone_selected(&spic, 60, 100, ZoneTrigger::Attack));
        assert!(!eng.zone_selected(&stac, 60, 100, ZoneTrigger::Attack));

        assert!(eng.try_keyswitch(13, 120));
        assert_eq!(eng.articulation(), "Staccato");
        assert!(eng.zone_selected(&stac, 60, 100, ZoneTrigger::Attack));

        // A normal (non-keyswitch) note is NOT consumed — it should sound.
        assert!(!eng.try_keyswitch(60, 100));
    }

    /// Build a zoned NVLeg legato engine with directional zones for a set of
    /// notes (synthetic paths so it reads as zoned; samples never load).
    fn mono_legato_engine(notes: &[u8]) -> SampleEngine {
        let mut styx = String::from(
            "name \"s\"\n\
             articulations ( {id NVLeg, label NVLeg, kind @Legato} )\n\
             zones (\n",
        );
        for &n in notes {
            for dir in ["up", "down"] {
                styx.push_str(&format!(
                    "{{file \"{n}_{dir}.wav\", key_min {n}, key_max {n}, root_key {n}, vel_min 0, vel_max 127, articulation \"NVLeg\", direction \"{dir}\"}}\n"
                ));
            }
        }
        styx.push_str(")\n");
        let spec = crate::LibrarySpec::from_styx(&styx).expect("parse styx");
        let mut patch = crate::PlayerPatch::from_spec(spec);
        patch.zone_paths = (0..patch.spec.zones.len())
            .map(|i| std::path::PathBuf::from(format!("z{i}.wav")))
            .collect();
        let mut eng = SampleEngine::new(patch, 48_000, "", "");
        eng.set_articulation("NVLeg");
        eng
    }

    fn render_ms(eng: &mut SampleEngine, ms: usize) {
        let frames = (eng.sample_rate as usize / 1000) * ms;
        let mut out = vec![0.0f32; frames * 2];
        eng.render(&mut out);
    }

    #[test]
    fn mono_legato_falls_back_to_held_on_release() {
        let mut eng = mono_legato_engine(&[60, 62]);

        // First note sounds immediately and becomes the line head.
        eng.note_on(60, 100);
        assert_eq!(eng.lines[0].note, Some(60));

        // Second note transitions (delayed), then becomes the head. Played
        // past the live chord window so it reads as a line, not a chord.
        render_ms(&mut eng, 50);
        eng.note_on(62, 100);
        assert!(matches!(eng.lines[0].state, LegatoState::Pending { .. }));
        render_ms(&mut eng, 200);
        assert_eq!(eng.lines[0].note, Some(62));

        // Releasing the SOUNDING note (62) while 60 is still held falls back to
        // 60 via a legato transition — a true monophonic line.
        eng.note_off(62);
        assert!(matches!(
            eng.lines[0].state,
            LegatoState::Pending { to_note: 60, .. }
        ));
        render_ms(&mut eng, 200);
        assert_eq!(eng.lines[0].note, Some(60));

        // Releasing the last held note ends the line.
        eng.note_off(60);
        assert_eq!(eng.lines[0].note, None);
    }

    #[test]
    fn per_line_mono_legato_is_independent() {
        // Two divisi lines on one engine: each keeps its own mono cursor,
        // and a prefire on line 1 must not disturb line 0's sounding note.
        let mut eng = mono_legato_engine(&[60, 62, 64, 67]);

        eng.note_on_line(0, 60, 100);
        eng.note_on_line(1, 64, 100);
        assert_eq!(eng.lines[0].note, Some(60));
        assert_eq!(eng.lines[1].note, Some(64));

        // Reactive transition on line 0 only.
        eng.note_on_line(0, 62, 100);
        assert!(matches!(eng.lines[0].state, LegatoState::Pending { .. }));
        assert!(matches!(eng.lines[1].state, LegatoState::Idle));

        // Document prefire on line 1 fires immediately, line 0 unaffected.
        eng.legato_prefire_line(1, 67, 100);
        assert_eq!(eng.lines[1].note, Some(67));
        assert!(matches!(eng.lines[0].state, LegatoState::Pending { .. }));

        render_ms(&mut eng, 200);
        assert_eq!(eng.lines[0].note, Some(62));
        assert_eq!(eng.lines[1].note, Some(67));

        // Note-offs on line 1 end only line 1 (release the silent key first
        // so the mono line doesn't fall back to it).
        eng.note_off_line(1, 64);
        eng.note_off_line(1, 67);
        assert_eq!(eng.lines[1].note, None);
        assert_eq!(eng.lines[0].note, Some(62));

        // Exactly one reactive trigger: line 0's overlapping note-on. The
        // prefire and the plain note-offs are not reactive.
        assert_eq!(eng.reactive_legato_fires(), 1);
    }

    // ── Live auto-divisi gating (StrictLive) ─────────────────────────────────

    #[test]
    fn live_chord_within_window_fans_out_as_fresh_attacks() {
        let mut eng = mono_legato_engine(&[60, 64, 67]);
        eng.set_legato_fire_log_enabled(true);

        // A triad struck at once: three fresh attacks on three lines, in
        // arrival order — never legato.
        eng.note_on(60, 100);
        eng.note_on(64, 100);
        eng.note_on(67, 100);
        assert_eq!(eng.lines[0].note, Some(60));
        assert_eq!(eng.lines[1].note, Some(64));
        assert_eq!(eng.lines[2].note, Some(67));

        render_ms(&mut eng, 200);
        assert!(
            eng.legato_fire_log().is_empty(),
            "a chord must not fire legato transitions"
        );
        assert_eq!(eng.reactive_legato_fires(), 0);
    }

    #[test]
    fn live_stepwise_line_stays_legato_on_one_line() {
        let mut eng = mono_legato_engine(&[60, 62, 64]);
        eng.set_legato_fire_log_enabled(true);

        eng.note_on(60, 100);
        render_ms(&mut eng, 50);
        eng.note_on(62, 100); // whole step ≤ live_legato_interval_max (2)
        render_ms(&mut eng, 200); // countdown elapses (low_latency)
        eng.note_off(60);
        eng.note_on(64, 100);
        render_ms(&mut eng, 200);

        let log = eng.legato_fire_log();
        assert_eq!(log.len(), 2, "both steps transitioned");
        assert!(log.iter().all(|e| e.line == 0), "one mono line throughout");
        assert_eq!(eng.lines[0].note, Some(64));
        assert!(eng.lines[1].note.is_none(), "no divisi for a mono line");
    }

    #[test]
    fn live_wide_leap_takes_a_fresh_line_not_legato() {
        let mut eng = mono_legato_engine(&[60, 69]);
        eng.set_legato_fire_log_enabled(true);

        eng.note_on(60, 100);
        render_ms(&mut eng, 50);
        eng.note_on(69, 100); // major 6th > interval gate
        render_ms(&mut eng, 200);

        assert_eq!(eng.lines[0].note, Some(60), "held note keeps its line");
        assert_eq!(eng.lines[1].note, Some(69), "leap starts a fresh line");
        assert!(
            eng.legato_fire_log().is_empty(),
            "wide leaps must not legato in strict live mode"
        );
    }

    #[test]
    fn live_held_top_with_moving_bass_keeps_lines_apart() {
        let mut eng = mono_legato_engine(&[72, 50, 52]);
        eng.set_legato_fire_log_enabled(true);

        eng.note_on(72, 100); // held top → line 0
        render_ms(&mut eng, 50);
        eng.note_on(50, 100); // far below → its own line
        render_ms(&mut eng, 50);
        eng.note_on(52, 100); // whole step from the bass → bass line legato
        render_ms(&mut eng, 200);

        assert_eq!(eng.lines[0].note, Some(72), "top never stolen");
        assert_eq!(eng.lines[1].note, Some(52), "bass moved on its own line");
        let log = eng.legato_fire_log();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].line, 1);
        assert_eq!(log[0].from_note, 50);
        assert_eq!(log[0].to_note, 52);
    }

    #[test]
    fn zoned_legato_delays_then_fires_directional() {
        // Legato articulation with directional zones (up/down) for two notes.
        // Build with synthetic zone_paths so the patch reads as zoned (is_zoned
        // checks resolved paths; the samples themselves never load in a test).
        let spec = crate::LibrarySpec::from_styx(
            "name \"s\"\n\
             articulations (\n\
               {id Leg, label Leg, kind @Legato}\n\
             )\n\
             zones (\n\
               {file \"u60.wav\", key_min 60, key_max 60, root_key 60, vel_min 0, vel_max 127, articulation \"Leg\", direction \"up\"}\n\
               {file \"d60.wav\", key_min 60, key_max 60, root_key 60, vel_min 0, vel_max 127, articulation \"Leg\", direction \"down\"}\n\
               {file \"u62.wav\", key_min 62, key_max 62, root_key 62, vel_min 0, vel_max 127, articulation \"Leg\", direction \"up\"}\n\
               {file \"d62.wav\", key_min 62, key_max 62, root_key 62, vel_min 0, vel_max 127, articulation \"Leg\", direction \"down\"}\n\
             )\n",
        )
        .expect("parse styx");
        let mut patch = crate::PlayerPatch::from_spec(spec);
        patch.zone_paths = (0..patch.spec.zones.len())
            .map(|i| std::path::PathBuf::from(format!("z{i}.wav")))
            .collect();
        let mut eng = SampleEngine::new(patch, 48_000, "", "");
        eng.set_articulation("Leg");
        assert!(eng.legato_enabled, "legato on by default");
        assert_eq!(eng.articulation(), "Leg");
        assert!(
            matches!(
                eng.patch().spec.articulation("Leg").map(|a| a.kind.clone()),
                Some(ArticulationKind::Legato)
            ),
            "Leg parsed as @Legato"
        );
        let u62 = eng.patch().spec.zones[2].clone();
        let d62 = eng.patch().spec.zones[3].clone();

        // First note sounds immediately — no pending legato.
        eng.note_on(60, 100);
        assert!(matches!(eng.lines[0].state, LegatoState::Idle));

        // Second note while the first is held → delayed (the CSS latency):
        // pending, not yet fired. (Past the live chord window: overlapping
        // successive notes legato; simultaneous ones are a chord.)
        render_ms(&mut eng, 50);
        eng.note_on(62, 100);
        assert!(
            matches!(eng.lines[0].state, LegatoState::Pending { .. }),
            "overlapping legato note should delay (latency), not fire instantly"
        );

        // Render past the (default 100ms) delay → fires; direction up (62 > 60).
        let frames = (eng.sample_rate as usize / 1000) * 200; // 200 ms
        let mut out = vec![0.0f32; frames * 2];
        eng.render(&mut out);
        assert!(
            matches!(eng.lines[0].state, LegatoState::Idle),
            "legato fired"
        );
        assert_eq!(eng.play_direction, "up");
        // The up-zone for the target is now selected; the down-zone is not.
        assert!(eng.zone_selected(&u62, 62, 100, ZoneTrigger::Attack));
        assert!(!eng.zone_selected(&d62, 62, 100, ZoneTrigger::Attack));
    }

    #[test]
    fn legato_pairs_nonvib_and_vibrato_for_cc2() {
        // CC2 vibrato crossfades a non-vib / vib pair. CSS has them for legato
        // (Leg ↔ NVLeg) as well as sustains — the pair lookup must find both.
        let eng = engine_from_styx(
            "name \"s\"\n\
             dynamics { vibrato_controller CC2 }\n\
             articulations (\n\
               {id NVLeg, label NVLeg, kind @Legato}\n\
               {id Leg, label Leg, kind @Legato}\n\
             )\n\
             zones (\n\
               {file \"a.wav\", key_min 60, key_max 60, root_key 60, vel_min 0, vel_max 127, articulation \"NVLeg\"}\n\
             )\n",
        );
        assert_eq!(eng.find_vibrato_pair_id("NVLeg").as_deref(), Some("Leg"));
        assert_eq!(eng.find_vibrato_pair_id("Leg").as_deref(), Some("NVLeg"));
    }

    #[test]
    fn pinned_articulation_selects_by_artic_ignoring_key() {
        // Hats-style pack: multiple articulations on different keys.
        let mut eng = engine_from_styx(
            "name \"h\"\n\
             category \"drum-kit\"\n\
             zones (\n\
               {\n\
                 file \"c.wav\"\n\
                 key_min 42\n\
                 key_max 42\n\
                 root_key 42\n\
                 vel_min 0\n\
                 vel_max 127\n\
                 articulation \"Closed Tip\"\n\
               }\n\
               {\n\
                 file \"o.wav\"\n\
                 key_min 46\n\
                 key_max 46\n\
                 root_key 46\n\
                 vel_min 0\n\
                 vel_max 127\n\
                 articulation \"Open 1\"\n\
               }\n\
             )\n",
        );
        assert!(eng.percussion && !eng.single_attack_key);
        let closed = eng.patch().spec.zones[0].clone();
        let open = eng.patch().spec.zones[1].clone();

        // No pin: multi-key drum is addressed by its native keys.
        assert!(eng.zone_selected(&closed, 42, 100, ZoneTrigger::Attack));
        assert!(!eng.zone_selected(&closed, 49, 100, ZoneTrigger::Attack));

        // Pin "Open 1": fires Open on any note, never Closed.
        eng.pin_articulation(Some("open 1".to_string())); // case-insensitive
        assert!(eng.zone_selected(&open, 53, 100, ZoneTrigger::Attack));
        assert!(eng.zone_selected(&open, 99, 100, ZoneTrigger::Attack));
        assert!(!eng.zone_selected(&closed, 53, 100, ZoneTrigger::Attack));

        // Per-trigger (per-route) articulation takes precedence over the pin,
        // so one shared engine can serve many routed notes; cleared after.
        eng.trigger_articulation = Some("Closed Tip".to_string());
        assert!(eng.zone_selected(&closed, 49, 100, ZoneTrigger::Attack));
        assert!(!eng.zone_selected(&open, 49, 100, ZoneTrigger::Attack));
        eng.trigger_articulation = None;
        // Falls back to the pin once the transient clears.
        assert!(eng.zone_selected(&open, 49, 100, ZoneTrigger::Attack));

        // note_on_articulated must leave no residual state.
        eng.note_on_articulated(49, 100, Some("Closed Tip"));
        assert!(eng.trigger_articulation.is_none());
    }

    #[test]
    fn choke_group_setter_and_percussion_one_shot() {
        let mut eng = engine_from_styx(
            "name \"h\"\n\
             category \"drum-kit\"\n\
             zones (\n\
               {\n\
                 file \"c.wav\"\n\
                 key_min 42\n\
                 key_max 42\n\
                 root_key 42\n\
                 vel_min 0\n\
                 vel_max 127\n\
                 articulation \"Closed Tip\"\n\
               }\n\
             )\n",
        );
        // Percussion → voices spawn one-shot (Short) and ignore note-off.
        assert!(eng.percussion);
        // Mono choke (hats): group set, no choke_on → every hit chokes.
        assert!(eng.engine_choke_group.is_none());
        eng.set_choke_group(Some("hats"), &[]);
        assert_eq!(eng.engine_choke_group, Some(stable_group_hash("hats")));
        assert!(eng.should_engine_choke(), "mono: any hit chokes");
        eng.set_choke_group(Some(""), &[]); // empty clears
        assert!(eng.engine_choke_group.is_none());
        assert!(!eng.should_engine_choke());

        // Selective choke (cymbals): only the "Choke" articulation chokes;
        // crashes ring and overlap.
        eng.set_choke_group(Some("ride"), &["Choke".to_string()]);
        eng.trigger_articulation = Some("Crash".to_string());
        assert!(!eng.should_engine_choke(), "crash must not choke the ring");
        eng.trigger_articulation = Some("choke".to_string()); // case-insensitive
        assert!(
            eng.should_engine_choke(),
            "choke articulation stops the ring"
        );
    }

    #[test]
    fn pitched_sampler_still_gates_on_key() {
        // No drum category → pitched: key range gates, no collapse.
        let eng = engine_from_styx(
            "name \"p\"\n\
             instrument \"piano\"\n\
             zones (\n\
               {\n\
                 file \"p.wav\"\n\
                 key_min 60\n\
                 key_max 72\n\
                 root_key 60\n\
                 vel_min 0\n\
                 vel_max 127\n\
               }\n\
             )\n",
        );
        assert!(!eng.percussion);
        let z = &eng.patch().spec.zones[0];
        assert!(eng.zone_selected(z, 65, 100, ZoneTrigger::Attack));
        assert!(!eng.zone_selected(z, 40, 100, ZoneTrigger::Attack));
    }
}
