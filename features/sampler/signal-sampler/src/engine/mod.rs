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

/// `$foyeb=1000` (wait), `$g4dbu=1000` (fade) from `CSS 1st Violins.nki`.
///
/// DECODED CORRECTION (`script_1.ksp`, verified): these `wait/fade_in` values do
/// NOT time the main held sustain. A CSS legato note spawns THREE voices:
///   * `%grhcg` — the MAIN held sustain: `play_note(note,$jabns,0,-1)` → offset
///     0, length −1 (loops), plays IMMEDIATELY at full level, then
///     `change_vol(%grhcg,$3tsb0*100,1)` = −6 dB legato makeup. THIS carries the
///     note. It is NOT muted and NOT faded in. (Modeled: see `fire_legato_with_lead`
///     — spawned immediately with a declick only, gained −6 dB via `legato_sustain`.)
///   * `%ftriy` — the bow-change TRANSITION: instant-muted then CSS_W-faded with
///     ITS OWN timing (`$ohdjc/$wtxmh`) — the attack ornament. (Modeled as the
///     `VoiceKind::Legato` transition voice.)
///   * `%1wcdh` — a secondary bloom/OVERLAY: instant-muted then `wait $foyeb=1000;
///     fade_in $g4dbu=1000`. A slow secondary layer, NOT the main tone.
///
/// So `$foyeb/$g4dbu` (1000/1000) describe the `%1wcdh` secondary overlay, which
/// we currently leave UNMODELED (a subtle bloom the transition + immediate
/// sustain already cover). They are NOT the main-sustain fade — the earlier code
/// mis-applied them to `%grhcg`, muting the held tone ~1 s so legato notes were
/// carried only by the quiet transition (~10 dB below the first note). Retained
/// as documentation of the overlay's real timing.
#[allow(dead_code)]
const CSS_W_WAIT_MS: u32 = 1000; // $foyeb — %1wcdh secondary overlay wait (unmodeled)
#[allow(dead_code)]
const CSS_W_FADE_MS: u32 = 1000; // $g4dbu — %1wcdh secondary overlay fade (unmodeled)

/// CSS held-sustain note-off overlap fade (`$tukcw`, spec §6): on key-up (pedal
/// not latched) the looping sustain is note_off'd immediately and fades out
/// over this window. Real persistent value: **400 ms**.
const SUSTAIN_NOTEOFF_MS: u32 = 400; // $tukcw

/// CSS legato-retire crossfades (spec §2.1 step 4) — how the PREVIOUS pair is
/// faded out as the new pair starts. Real persistent values, indexed by
/// attack-velocity range (`$xp1ku` 1/2/3). The old 30 ms fade was the source
/// of the inter-note "tick"; these long overlapping fades remove it.
const RETIRE_TRANS_MS: [u32; 3] = [150, 281, 281]; // $fjtlu / $hbi2j / $2ebzd
const RETIRE_SUS_MS: [u32; 3] = [550, 500, 500]; // $tdjzq / $3ivkj / $u0t23

/// Overall CC1 → loudness roll-off for CSS sustains. DATA-DERIVED, not a taste
/// knob: the decoded per-layer `CC_VOLUME cc=1` tables (nkx-extract
/// `CSS_GROUP_MOD.md` §3 / `groups.json`) are a bipolar TIMBRE crossfade whose
/// per-layer `y` amounts sum to a nearly-flat total — confirmed by the
/// reference render (`css_ab_css.wav`, onset-aligned nonvib G4 sweep):
/// CC1=20 → −3.0 dB, CC1=50/80/110 → 0.0 dB relative to full. So CC1 changes
/// which recorded layer's colour dominates (handled by the equal-power layer
/// crossfade) while total level stays ≈flat, gently rolling off only at the
/// very bottom. The old −12 dB linear floor over-attenuated everything below
/// CC1=127 — the cause of the SUS-DYN/SUS-PITCH −7…−10 dB deficit.
///
/// Piecewise: 0 dB at/above the knee, linear to `CC1_FLOOR_DB` at CC1=0.
/// Both constants are pinned to the one sanctioned calibration render (the
/// nonvib sweep above): knee at CC1=45 (flat by CC1=50), floor set so
/// CC1=20 → −3.0 dB.
const CC1_KNEE: u8 = 45;
const CC1_FLOOR_DB: f32 = -5.4; // −3.0 dB at CC1=20 on the 0→45 ramp

/// `$3tsb0` legato makeup — the −6 dB `change_vol` on the CSS legato held
/// SUSTAIN (`%grhcg`), NOT the transition.
/// DATA-DERIVED: KSP `change_vol(%grhcg[...],$3tsb0*100,1)` on the legato
/// (`VRange`) branch (`script_1.ksp` ~12945/13004); persistent `$3tsb0=−60`
/// → −6.0 dB (millidecibel law: change_vol = dB×1000, persistent vol vars in
/// 0.1 dB units). `%grhcg` is the main held tone, so this trim lands on the
/// legato SUSTAIN voice (via the `legato_sustain` flag in `spawn_zone_voice_at`),
/// leaving a legato-connected note ~6 dB below a fresh first note — the real,
/// subtle CSS handoff. (Previously mis-applied to the transition voice.)
const CSS_LEGATO_MAKEUP_DB: f32 = -6.0; // $3tsb0

/// Global output makeup applied to looping sustain-layer voices. STILL NOT
/// instrument-derived — and the ENV_FLEX work proved its old rationale wrong.
///
/// The prior doc claimed this compensated a ~6 dB bloom-peak-vs-plateau gap.
/// Measured directly on the decoded CSS Mix sustain samples (nonvib/vibsus G4),
/// the smoothed-RMS bloom peak sits only **~1.7–2.8 dB** above the steady-loop
/// plateau — NOT 6 dB. So the ENV_FLEX (which holds level 1.0 = the sample
/// as-recorded) does NOT hold a level 6 dB above our looped plateau, and
/// removing this constant drops every SUS-DYN note ~6 dB (regressing the A/B
/// from ~0 dB to ~−6 dB). At most ~2.5 dB is a real bloom/plateau ratio; the
/// residual ~3.5 dB is a flat level offset between our looped-plateau playback
/// and CSS's Kontakt render that is NOT present in the decoded GroupList (all
/// groups ship 0 dB static, change_vol 0) — it lives in Kontakt's instrument
/// output stage / the shipped sample normalization, which our GroupList decode
/// does not capture. Kept because it makes the CALIB/SUS-DYN anchor match;
/// flagged in the audit as the ONE constant still not instrument-derived.
const OUTPUT_MAKEUP: f32 = 1.995_262; // +6 dB = 10^(6/20)

/// CSS master tune, global on every playable group: `tune=1.00521`
/// (`CSS_GROUP_MOD.md` §1) = 1200·log₂(1.00521) ≈ **+9.0 cents**. NOT baked into
/// the styx zones (`tune_cents` ships 0.000 for all 32175 zones — verified), so
/// applied here globally on top of the per-note transpose.
const CSS_MASTER_TUNE_CENTS: f64 = 9.0;

/// Minimum attack fade (ms) for a synthesized-loop sustain that starts mid-sample
/// at full level — just enough to avoid an onset click without slowing the attack.
const SUSTAIN_DECLICK_MS: u32 = 12;

/// Onset declick (ms) for legato transitions, release tails, and any voice that
/// starts mid-sample (`start_offset`). Long enough to remove the onset step
/// click, short enough to be inaudible on a recorded bow-change / release.
const ONSET_DECLICK_MS: u32 = 6;

/// Declick fade (ms) for a voice that starts DEEP inside a sample via
/// `start_offset` — a Low-Latency legato prefire skips ~300 ms into the
/// transition recording and begins partway up the steep bow-change swell. A
/// 6 ms fade leaves that entry abrupt (perceived as a click on every note); a
/// longer fade eases into the steep material without moving the arrival tick.
const SKIP_DECLICK_MS: u32 = 25;

/// Seamless loop crossfade (ms). A held/looped body (CSS synth-loop sustains,
/// legato tails) wraps from `loop_end` back to `loop_start`; without a fade the
/// waveform discontinuity clicks once per loop. Blend the loop tail into the
/// pre-`loop_start` material over this window. Long enough to smooth the seam
/// on evolving string timbre, short enough not to audibly smear the loop.
const LOOP_XFADE_MS: u32 = 150;

/// Gain for recorded CSS release-tail voices (NVrel/Vsusrel). DATA-DERIVED (was
/// a guessed 0.3): the release groups ship **0 dB static** volume
/// (`CSS_GROUP_MOD.md` §1) and now carry their decoded release ENV_FLEX
/// (`FLEX_RELEASE`, 1 ms→0.99 → 4007 ms→1.0 → 1250 ms→0) which shapes the tail —
/// so the correct base gain is unity, the envelope does the shaping. Kept at
/// `RELEASE_MAX_LIFETIME_MS` so the tail cannot bleed unbounded into the next
/// note.
const RELEASE_GAIN: f32 = 1.0;

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
        /// Inter-onset interval (ms) at the moment the transition was armed —
        /// carried through the countdown so the reactive fire can apply the
        /// CSS `$1fvjk` sample-start offset ([`lt_start_offset_ms`]).
        ioi_ms: f32,
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
}

/// Cap on the fire log so an enabled log can never grow unbounded on the
/// audio thread (the Vec is pre-allocated to this capacity when enabled).
const LEGATO_FIRE_LOG_CAP: usize = 1024;

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
    sustain_fade_in: Option<(usize, usize)>,
    /// When true, sustain-layer voices spawned during this dispatch carry the
    /// −6 dB `$3tsb0` legato makeup (`CSS_LEGATO_MAKEUP_DB`) — the KSP
    /// `change_vol(%grhcg,$3tsb0*100)` on the held legato tone. Set only around
    /// the `trigger_zoned_sustain` inside a legato handoff so a legato-connected
    /// note ends up ~6 dB below a fresh first note. `false` = first note /
    /// polyphonic sustain (full `OUTPUT_MAKEUP`).
    legato_sustain: bool,

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
    /// When true, every voice spawn / note-off / transition is appended to
    /// `trace` — the structured render trace (see [`trace`]). Off by default.
    trace_enabled: bool,
    /// Structured render trace: which files played, when, loop points, gains,
    /// transitions. Populated only while `trace_enabled`. Behind a `RefCell`
    /// so the `&self` voice-resolution path can record spawns/misses.
    trace: RefCell<RenderTrace>,
    /// Monotonic voice id source for trace correlation.
    next_voice_id: Cell<u64>,

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
            pending_cc58_group: None,
            cc11_volume: 1.0,
            cc5_porta_volume: 1.0,
            cc_values: [0; 128],
            channel_aftertouch: 0,
            poly_aftertouch: [0; 128],
            cc64_held: false,
            cc64_value: 0,
            recent_velocity: 90,
            note_strike_vel: [0; 128],
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
            sord_filter: BiquadFilter::lowpass(filter::SORD_FC, filter::SORD_Q, sample_rate),
            // Pre-size note-keyed maps to the full MIDI range so note-on never
            // reallocates them on the audio thread.
            held_notes: HashMap::with_capacity(128),
            body_voiced: std::collections::HashSet::with_capacity(128),
            deferred_note_off_velocities: HashMap::with_capacity(128),
            frames_rendered: 0,
            legato_fire_log_enabled: false,
            legato_fire_log: Vec::new(),
            trace_enabled: false,
            trace: RefCell::new(RenderTrace::default()),
            next_voice_id: Cell::new(0),
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

    /// Enable/disable the structured render trace ([`RenderTrace`]) — which
    /// files play, when, loop points, gains, transitions. Clears on enable.
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

    /// Record one trace event on the active line (no-op unless tracing is on).
    /// Takes `&self` — the trace sits behind a `RefCell` so the `&self`
    /// voice-resolution path (`make_voice`) can record spawns and misses.
    fn trace_push(&self, kind: TraceKind) {
        if self.trace_enabled {
            self.trace.borrow_mut().events.push(TraceEvent {
                frame: self.frames_rendered,
                line: self.cur_line as u8,
                kind,
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

// ── ENV_FLEX amp envelopes (decoded per articulation family) ────────────────────
//
// The instrument's real live amp AHDSR, from `GroupList (0x33)` — see
// `nkx-extract/CSS_GROUP_MOD.md` §2 / `scratchpad/groups_out/groups.json`. Each
// entry is a shipped `(time_ms, level, curve)` segment; segment 0 is the attack.
// These are the literal decoded values (Main mic; all mics carry byte-identical
// envelopes), NOT approximations. Applied per-voice by [`FlexEnv`].

/// Sustain family (vibsus / nonvib / trills / tremolo / harmonics): fast bake
/// attack, hold, then a 20 s decay-to-0 (the bow eventually running out). Held
/// indefinitely via the [`FlexEnv`] sustain-hold freeze. (mf layer ships a 20 ms
/// attack vs 4 ms elsewhere — negligible under the $mmirg 198 ms CC_ATTACK
/// bloom applied on top, so a single 4 ms table is used.)
const FLEX_SUSTAIN: &[(f32, f32, f32)] =
    &[(4.0, 1.0, 0.505), (1000.0, 1.0, 0.9), (20000.0, 0.0, 0.05)];
/// Legato / NVlegato / legato-zero transition body.
const FLEX_LEGATO: &[(f32, f32, f32)] = &[
    (80.0, 1.0, 0.499),
    (480.0, 1.0, 0.72),
    (442.3, 1.0, 0.5),
    (1002.3, 0.0, 0.33),
    (152.0, 0.0, 0.5),
    (342.0, 0.0, 0.75),
];
/// Portamento / NVportamento glide.
const FLEX_PORTAMENTO: &[(f32, f32, f32)] = &[
    (88.0, 0.466, 0.499),
    (472.0, 1.0, 0.8),
    (1240.0, 0.0, 0.5),
    (152.0, 0.0, 0.5),
    (342.0, 0.0, 0.75),
];
/// Marcato-legato / marc-port.
const FLEX_MARC_LEG: &[(f32, f32, f32)] = &[
    (68.0, 0.493, 0.499),
    (492.0, 1.0, 0.72),
    (1440.0, 0.0, 0.33),
    (156.6, 0.0, 0.5),
    (342.0, 0.0, 0.75),
];
/// Marcato-mod overlay.
const FLEX_MARCATO_MOD: &[(f32, f32, f32)] = &[
    (1.0, 1.0, 0.685),
    (1499.0, 1.0, 0.5),
    (104.0, 1.0, 0.45),
    (1000.0, 0.0, 0.63),
];
/// Short family (spicc / staccatissimo / stacc / sfz / marcato / pizz / bartók /
/// col legno): one-shot, plays to natural end shaped by the 8/604/7381 decay.
const FLEX_SHORT: &[(f32, f32, f32)] =
    &[(8.0, 1.0, 0.505), (604.0, 1.0, 0.45), (7381.0, 0.0, 0.65)];
/// Release tails (rel *): 0 dB static group, shaped by this decoded envelope.
const FLEX_RELEASE: &[(f32, f32, f32)] =
    &[(1.0, 0.986, 0.125), (4007.0, 1.0, 0.9), (1250.0, 0.0, 0.7)];

/// Select the decoded ENV_FLEX amplitude envelope for a voice from its
/// articulation id + voice kind. Returns `None` for families with no decoded
/// envelope (legacy / non-CSS libraries) — those keep flat unity.
fn flex_env_for(
    artic_id: &str,
    kind: &VoiceKind,
    is_sustain_layer: bool,
    sample_rate: u32,
) -> Option<FlexEnv> {
    let id = artic_id.to_ascii_lowercase();
    let (segs, hold): (&[(f32, f32, f32)], bool) =
        if matches!(kind, VoiceKind::Release) || id.contains("rel") {
            (FLEX_RELEASE, false)
        } else if matches!(kind, VoiceKind::Short) {
            (FLEX_SHORT, false)
        } else if id.contains("port") {
            (FLEX_PORTAMENTO, false)
        } else if id.contains("marc") && id.contains("leg") {
            (FLEX_MARC_LEG, false)
        } else if id.contains("marcato") && id.contains("mod") {
            (FLEX_MARCATO_MOD, false)
        } else if matches!(kind, VoiceKind::Legato) || id.contains("legato") {
            (FLEX_LEGATO, false)
        } else if is_sustain_layer {
            // vibsus / nonvib / trills / tremolo / harmonics — the held families.
            (FLEX_SUSTAIN, true)
        } else {
            return None;
        };
    FlexEnv::from_segments(segs, 0.0, sample_rate, hold)
}

/// Frames → milliseconds (the inverse of [`ms_to_frames`]).
pub fn frames_to_ms(frames: u64, sample_rate: u32) -> f32 {
    if sample_rate == 0 {
        return 0.0;
    }
    frames as f32 * 1000.0 / sample_rate as f32
}

// ── CSS legato Overlap-Delay (real persistent values) ──────────────────────────
//
// `legtrans_OD` waits `$b0n3s` ms before the transition fires. `$b0n3s`
// interpolates across the IOI thresholds to per-(mode, velocity-range) anchors.
// These are the ACTUAL values read from `CSS 1st Violins.nki`'s persistent
// snapshot (extracted from the BParScript store, not the compiled `:=`
// defaults) — see `scratchpad/css_persistent_values.md`. The delay is
// near-ZERO everywhere except soft+fast playing (max 77–83 ms), which INVERTS
// the earlier spec approximation.

/// Attack-velocity range `$xp1ku` (spec §1.3): 1 = [0..`$eluxs`], 2 =
/// [`$eluxs`+1..`$0uhls`], 3 = rest. Real splits: `$eluxs=64`, `$0uhls=100`.
const VEL_SPLIT_1: u8 = 64; // $eluxs
const VEL_SPLIT_2: u8 = 100; // $0uhls
fn velocity_range(vel: u8) -> u8 {
    if vel <= VEL_SPLIT_1 {
        1
    } else if vel <= VEL_SPLIT_2 {
        2
    } else {
        3
    }
}

// Low-Latency O+D: thresholds $deey3/$fxiox/$jystg/$zvaet; anchors per range.
const OD_LL_THR: [f32; 4] = [75.0, 100.0, 800.0, 1100.0];
const OD_LL_VR1_ANC: [f32; 4] = [77.0, 0.0, 0.0, 0.0]; // $nbkqa/$mih5r/$yzpsq/$myv02
const OD_LL_VR23_ANC: [f32; 4] = [0.0, 0.0, 0.0, 0.0]; // $55anl/$umt5l/$cffjr/$yeo2q
// Expressive O+D: thresholds $g45yq/$bwkdm/$waq1e/$whtm2; anchors per range.
const OD_EX_THR: [f32; 4] = [200.0, 300.0, 800.0, 800.0];
const OD_EX_VR1_ANC: [f32; 4] = [83.0, 0.0, 0.0, 0.0]; // $kadcz/$nug53/$tfwqt/$xvurx
const OD_EX_VR23_ANC: [f32; 4] = [0.0, 0.0, 0.0, 0.0]; // vr2 + vr3 both all-zero

/// Piecewise-linear interpolation of `ioi` across 4 ascending `thr` to 4 `anc`.
/// Below `thr[0]` → `anc[0]`; above `thr[3]` → `anc[3]`.
fn interp_od(ioi: f32, thr: [f32; 4], anc: [f32; 4]) -> f32 {
    if ioi < thr[0] {
        return anc[0];
    }
    for k in 0..3 {
        if ioi < thr[k + 1] {
            let span = (thr[k + 1] - thr[k]).max(1e-6);
            let t = (ioi - thr[k]) / span;
            return anc[k] + (anc[k + 1] - anc[k]) * t;
        }
    }
    anc[3]
}

/// CSS legato Overlap-Delay (ms) — the KSP `legtrans_OD` model (spec §2.1,
/// corrected to the real persistent values). Driven by the inter-onset
/// interval (IOI, ms), the attack `velocity` (→ range `$xp1ku`), and the
/// legato mode. Near-zero except soft (vel ≤ 64) + fast (small IOI) playing.
pub fn ioi_legato_delay_ms(ioi_ms: f32, velocity: u8, expressive: bool) -> u32 {
    let vr = velocity_range(velocity);
    let (thr, anc) = if expressive {
        (
            OD_EX_THR,
            if vr == 1 {
                OD_EX_VR1_ANC
            } else {
                OD_EX_VR23_ANC
            },
        )
    } else {
        (
            OD_LL_THR,
            if vr == 1 {
                OD_LL_VR1_ANC
            } else {
                OD_LL_VR23_ANC
            },
        )
    };
    interp_od(ioi_ms, thr, anc).round().max(0.0) as u32
}

// ── CSS legato transition sample-start offset `$1fvjk` (real persistent values) ──
//
// The main-path (`$ocjln=6`) legato transition is spawned
// `play_note(note, RR, $1fvjk*1000, 0)` — `play_note`'s 3rd arg is the sample-start
// offset in µs, so `$1fvjk` (ms) is how far INTO the transition recording playback
// begins. It is IOI-interpolated: FAST lines start DEEPER in (skip more of the
// recorded bow-change swell → less audible pre-bow), SLOW lines start SHALLOWER
// (more pre-bow lead-in). Read from `CSS 1st Violins.nki`'s persistent snapshot
// (BParScript store, not the compiled `:=` defaults):
//   thresholds  $yam53 = 100, $nzsuf = 150, $5c2um = 500  (ms)
//   anchors     $ggt00 = 177, $v0rbb = 177, $5exar = 117  (ms)
// i.e. flat 177 ms up to a 150 ms IOI, then a linear ramp 177 → 117 ms across
// 150…500 ms, then flat 117 ms. (The `$yam53 = 100` breakpoint sits inside the
// flat 177 ms region, so it is a no-op for the curve shape but kept for parity.)
//
// A per-velocity-range `$ocjln = 4` variant exists — base offsets 0/83/177 ms +
// the Overlap-Delay `$b0n3s` — but `$ocjln = 6` (this IOI curve) is CSS's primary
// legato path and the one modeled here.
const LT_OFF_THR: [f32; 3] = [100.0, 150.0, 500.0]; // $yam53 / $nzsuf / $5c2um
const LT_OFF_ANC: [f32; 3] = [177.0, 177.0, 117.0]; // $ggt00 / $v0rbb / $5exar

/// Reference transition-arrival point (ms) = the deepest `$1fvjk` (`$ggt00`): at a
/// fast line the sample starts essentially AT the destination-pitch arrival, so
/// the audible pre-bow is ~0. Documentary constant only; the document scheduler
/// derives the actual pre-bow from the per-move MEASURED arrival minus `$1fvjk`.
const LT_ARRIVAL_REF_MS: f32 = 177.0; // $ggt00

/// CSS legato transition sample-start offset (ms) — the decoded `$1fvjk` IOI curve
/// (`$ocjln = 6`). Below `$yam53` → `$ggt00` (177); above `$5c2um` → `$5exar`
/// (117); piecewise-linear between. `ioi_ms` is the inter-onset interval on the
/// line (frames since the previous onset), the same IOI clock as
/// [`ioi_legato_delay_ms`].
pub fn lt_start_offset_ms(ioi_ms: f32) -> f32 {
    if ioi_ms <= LT_OFF_THR[0] {
        return LT_OFF_ANC[0];
    }
    for k in 0..2 {
        if ioi_ms < LT_OFF_THR[k + 1] {
            let span = (LT_OFF_THR[k + 1] - LT_OFF_THR[k]).max(1e-6);
            let t = (ioi_ms - LT_OFF_THR[k]) / span;
            return LT_OFF_ANC[k] + (LT_OFF_ANC[k + 1] - LT_OFF_ANC[k]) * t;
        }
    }
    LT_OFF_ANC[2]
}

/// Documentary reference pre-bow (ms) for the `$1fvjk` offset, assuming the
/// reference arrival [`LT_ARRIVAL_REF_MS`]: `max(0, LT_ARRIVAL_REF_MS − $1fvjk)`.
/// The document scheduler prefers the per-move MEASURED arrival (see
/// [`crate::spec::LibrarySpec::legato_lead_ms`]); this is the zone-agnostic
/// fallback shape. Fast lines → ~0 ms; slow lines → ~60 ms.
pub fn lt_prebow_ms(ioi_ms: f32) -> f32 {
    (LT_ARRIVAL_REF_MS - lt_start_offset_ms(ioi_ms)).max(0.0)
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
    for i in 0..n {
        let a = i.saturating_sub(win);
        let b = (i + win + 1).min(n);
        let slice = &fine[a..b];
        sm[i] = slice.iter().copied().sum::<f32>() / slice.len() as f32;
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
