//! Document mode — lookahead playback for the sampler (phase 1: offline).
//!
//! A [`TrackDocument`] is the full note/CC content of one track in musical
//! time (quarter notes) plus its tempo map. [`annotate`] turns it into a
//! [`Schedule`] of engine events in **absolute frames from the document
//! epoch**, inverting the classic legato-latency problem: every
//! legato-followed note is triggered `delay_ms` (the spec's expressive
//! velocity→delay curve) *before* its tick via
//! [`LegatoPrefire`](DocEvent::LegatoPrefire), so the transition's audible
//! arrival lands exactly on the grid — no negative track delay, no mirrored
//! timing copy. Short notes are pre-rolled by the spec's
//! `short_note_timing.pre_delay_ms` the same way.
//!
//! See `docs/plan/document-mode.md` for the full design, including the hard
//! **determinism** requirement this module implements: same document + same
//! parameters + same seed → byte-identical audio. Every stochastic choice
//! (round-robin) is a pure hash of `(seed, note identity, purpose)` — never a
//! mutable counter — so playback is position-independent (starting mid-piece
//! picks the same RR slot for every note) and edit-stable (inserting a note
//! re-rolls only that note).
//!
//! The articulation / legato-edge / re-bow inference rules are the shared
//! `keyflow-annotate` model — the same implementation keyflow-orchestra's
//! mirror pass runs (where it is parity-tested against the CSS reference
//! engine in keyflow's `tests/mirror_parity.rs`). The adapter-level
//! equivalence of the two consumers is asserted over the whole MusicXML
//! corpus in `tests/annotation_parity.rs`.

use keyflow_annotate::{
    CcTimeline, EPS, EdgeParams, LineNote, annotate_line, annotate_line_with,
    default_blocks_rebow, ks_is_marcato,
};

use crate::engine::{ArticClass, LineId};
use crate::spec::{ArticulationKind, LatchedCcSelector, LibrarySpec};

/// Stage-1 annotation of one document note — the shared model's type
/// (`ks_val`, `legato_from`, `legato_to`, `re_bow_to`), index-aligned with
/// `TrackDocument::notes` when returned by [`stage1_annotations`].
pub use keyflow_annotate::NoteAnnotation;

/// Same-pitch abutment tolerance (QN) under which two connectable notes read
/// as a re-bow. Mirrors keyflow's `Config::break_gap_qn` default (1/64) used
/// as `gap.abs() <= break_gap_qn * 2` — i.e. gaps up to ~1/32 QN connect.
const BREAK_GAP_QN: f64 = 1.0 / 64.0;

/// Engine fallback when the spec has no matching velocity zone (matches
/// `SampleEngine::legato_timing`).
const LEGATO_DELAY_FALLBACK_MS: u32 = 100;

/// Auto-divisi hand-off window (QN): a same-channel note ending within this
/// of another note's onset reads as a legato hand-off (the line's previous
/// note letting go), not as a note held above it. ¼ QN comfortably covers
/// performed / generated legato overlaps while staying far below any real
/// held-note duration.
const DIVISI_HANDOFF_QN: f64 = 0.25;

// ── Document types ────────────────────────────────────────────────────────────

/// One tempo point: piecewise-constant BPM from `qn` onward.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoPoint {
    pub qn: f64,
    pub bpm: f64,
}

/// One note in the document (QN domain, 0-based channel).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DocNote {
    pub start_qn: f64,
    pub end_qn: f64,
    pub chan: u8,
    pub pitch: u8,
    pub vel: u8,
}

/// One CC event in the document (QN domain, 0-based channel).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DocCc {
    pub qn: f64,
    pub chan: u8,
    pub cc: u8,
    pub val: u8,
}

/// The full MIDI document of one track, given to the sampler ahead of time
/// (ARA-style) so it can *anticipate* instead of being compensated.
#[derive(Debug, Clone, Default)]
pub struct TrackDocument {
    /// Monotonic version; replaced wholesale on change (documents are small).
    pub version: u64,
    /// Determinism seed — all stochastic choices (RR) hash from this.
    /// Persisted with the project; re-roll to get a new "take".
    pub seed: u64,
    pub notes: Vec<DocNote>,
    pub ccs: Vec<DocCc>,
    /// Tempo map (piecewise-constant BPM). Empty ⇒ 120 BPM.
    pub tempo: Vec<TempoPoint>,
    /// Lookahead auto-divisi (see `docs/plan/document-mode.md`,
    /// "Auto-divisi"): when true, `annotate` ranks simultaneous same-channel
    /// notes top→bottom into monophonic engine lines BEFORE legato/re-bow
    /// inference — for documents whose channels don't already encode voice
    /// separation (e.g. a chordal part on one channel). Pure function of the
    /// document: deterministic and seed-independent. When false (default),
    /// channels are respected as-is: channel N → line N.
    pub auto_divisi: bool,
}

impl Default for TempoPoint {
    fn default() -> Self {
        Self {
            qn: 0.0,
            bpm: 120.0,
        }
    }
}

// ── Time conversion ───────────────────────────────────────────────────────────

/// Seconds from the document epoch (QN 0) to `qn`, integrating the
/// piecewise-constant tempo map. Before the first point, the first point's
/// BPM applies; an empty map means 120 BPM.
pub fn qn_to_sec(tempo: &[TempoPoint], qn: f64) -> f64 {
    let mut bpm = tempo.first().map(|t| t.bpm).unwrap_or(120.0);
    let mut sec = 0.0;
    let mut cur_qn = 0.0;
    for t in tempo {
        if t.qn >= qn {
            break;
        }
        if t.qn > cur_qn {
            sec += (t.qn - cur_qn) * 60.0 / bpm;
            cur_qn = t.qn;
        }
        bpm = t.bpm;
    }
    sec + (qn - cur_qn) * 60.0 / bpm
}

/// Absolute frame (from the document epoch) for a QN position.
pub fn qn_to_frame(tempo: &[TempoPoint], qn: f64, sample_rate: u32) -> i64 {
    (qn_to_sec(tempo, qn) * sample_rate as f64).round() as i64
}

fn ms_to_frames_i64(ms: u32, sample_rate: u32) -> i64 {
    (ms as i64 * sample_rate as i64) / 1000
}

// ── Deterministic round-robin ─────────────────────────────────────────────────

/// What an RR slot is being drawn for. Part of the hash so a note's body,
/// transition, and release each get an independent (but stable) slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u64)]
pub enum RrPurpose {
    /// The note's body trigger (sustain layers / short one-shot).
    Body = 1,
    /// The legato transition into the note.
    Transition = 2,
    /// The recorded release tail at note-off.
    Release = 3,
}

/// splitmix64 finalizer — a FIXED, documented mix function. Do NOT replace
/// with `std::hash::DefaultHasher` (unstable across Rust releases): rendered
/// projects must reproduce byte-identically forever.
fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Number of abstract RR positions the stable hash draws from. The engine
/// reduces the pinned slot modulo the actual RR-group size at trigger time
/// (`select_zone_rr_slot` / `find_layer_zone`), so any group size divides in.
pub const RR_SLOT_SPACE: u32 = 4096;

/// The document-mode round-robin choice: a pure function of the seed and the
/// note's identity — **no mutable counter**. Consequences (per the design
/// doc's "Determinism"): position independence (starting playback at bar 17
/// gives every note the same RR as playing from the top) and edit stability
/// (inserting a note re-rolls only that note).
pub fn stable_rr_slot(seed: u64, start_qn: f64, pitch: u8, chan: u8, purpose: RrPurpose) -> u32 {
    let mut h = seed;
    for v in [
        start_qn.to_bits(),
        pitch as u64,
        chan as u64,
        purpose as u64,
    ] {
        h = splitmix64(h ^ v);
    }
    (h % RR_SLOT_SPACE as u64) as u32
}

// ── Schedule ──────────────────────────────────────────────────────────────────

/// One engine action in the schedule.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DocEvent {
    /// Plain note-on (first-of-phrase sustains, shorts). `lead` is the
    /// pre-roll: wall frames from this trigger to the note's grid tick
    /// (`tick = frame + lead`). Per note it is the LARGEST measured
    /// heard-arrival (`ZoneSpec::arrival_ms`) over the zones the trigger can
    /// fire — the engine holds each spawned voice back by `lead − its own
    /// zone's arrival`, so the note is HEARD on the tick per round-robin /
    /// mic / dynamic layer. Falls back to `short_note_timing.pre_delay_ms`
    /// for shorts and `0` (trigger == tick) for sustains when the library
    /// carries no measurements — the historical behaviour.
    NoteOn { note: u8, vel: u8, rr: u32, lead: u32 },
    /// Note-off (fires the release tail; `rr` pins its round-robin).
    NoteOff { note: u8, rr: u32 },
    /// CC pass-through.
    Cc { cc: u8, val: u8 },
    /// Fire the legato transition into `note` NOW — scheduled `delay_ms`
    /// before the note's tick so the arrival lands on the grid. Emitted
    /// INSTEAD of a `NoteOn` for legato-followed notes. `lead` is the frame
    /// distance to the destination tick (`tick = frame + lead`), kept for
    /// diagnostics/analysis of the schedule.
    LegatoPrefire {
        note: u8,
        vel: u8,
        rr: u32,
        lead: u32,
    },
}

/// Kind of a [`RenderMarker`] — one point on the render's marker timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerKind {
    /// Trigger of a fresh attack: a phrase-start sustain fires ON its grid
    /// tick; a short fires pre-rolled (`pre_delay_ms` early).
    NoteStart,
    /// A legato transition sample begins — the prefire trigger; everything
    /// between this and [`MarkerKind::Arrival`] is the audible pre-bow.
    TransitionStart,
    /// Same-pitch re-bow trigger (Legzero re-trigger; no lead-in, so its
    /// arrival coincides with the trigger).
    Rebow,
    /// The note is HEARD — the destination pitch arrives / the rhythmic
    /// peak lands. By construction this is the note's grid tick: for
    /// transitions it is the in-sample arrival marker's (`lead_in_ms`)
    /// playback time; for shorts, `pre_delay_ms` past the pre-rolled
    /// trigger; for fresh sustains, the trigger itself.
    Arrival,
    /// Note release (note-off dispatch / release-sample spawn point).
    /// Re-bow SOURCES have none — the transition into the repeat replaces
    /// their release.
    Release,
}

/// One typed marker on a document render's timeline: the GUI-renderable
/// waveform-marker set (note starts, transition starts, arrivals, re-bows,
/// releases) and the substrate the deterministic timing tests assert on.
/// Built during [`annotate`] (frames are absolute document frames) and
/// carried through [`Schedule::markers`] → [`DocumentRenderResult::markers`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderMarker {
    /// Absolute frame from the document epoch.
    pub frame: u64,
    /// Engine mono line ([`LineId`], truncated).
    pub line: u8,
    /// The note this marker belongs to.
    pub note: u8,
    pub kind: MarkerKind,
}

/// One scheduled event at an absolute frame from the document epoch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScheduledEvent {
    pub frame: u64,
    /// Engine mono line ([`LineId`], truncated) this event dispatches to.
    /// The line allocator runs at annotation time: channel→line identity by
    /// default, or the auto-divisi ranking when
    /// [`TrackDocument::auto_divisi`] is set.
    pub line: u8,
    pub kind: DocEvent,
}

/// Annotated, frame-domain schedule for one document + library spec.
#[derive(Debug, Clone, Default)]
pub struct Schedule {
    pub sample_rate: u32,
    /// Sorted by `(frame, dispatch priority)`.
    pub events: Vec<ScheduledEvent>,
    /// Frame of the last event.
    pub end_frame: u64,
    /// The document's seed (carried through for render reports).
    pub seed: u64,
    pub note_count: usize,
    /// Notes that will arrive via `LegatoPrefire`.
    pub legato_count: usize,
    /// Notes pre-rolled as shorts.
    pub short_count: usize,
    /// Largest [`DocEvent::LegatoPrefire::lead`] in the schedule (frames).
    pub max_prefire_lead: u64,
    /// The document's tempo map (copied through) so the realtime scheduler
    /// can compare it against the host-reported tempo. Empty ⇒ 120 BPM.
    pub tempo: Vec<TempoPoint>,
    /// Typed marker timeline (sorted by frame): note starts, transition
    /// starts, arrivals, re-bows, releases — see [`RenderMarker`].
    pub markers: Vec<RenderMarker>,
    /// Merged engine-activity spans `(first_trigger, last_ring_out)`,
    /// sorted: the union of every note's `[trigger_frame, note_off +
    /// RECONSTRUCT_TAIL]`. Between spans the engine is provably quiescent.
    /// Used by [`Schedule::reconstruction_start`] to bound the deterministic
    /// replay that makes playback start-position invariant (see
    /// "Playback-start invariance" in `docs/plan/document-mode.md`).
    pub busy: Vec<(u64, u64)>,
}

impl Schedule {
    /// Where voice reconstruction must REPLAY from for a playback start at
    /// frame `p`: the beginning of the continuous activity span containing
    /// `p`, or `p` itself when the engine is quiescent there.
    ///
    /// Replaying the schedule from this frame — through the real render
    /// path, output discarded — reconstructs every voice the full render
    /// would have alive at `p` (transition voices mid-window, sustains
    /// mid-sample, release tails mid-ring) with BIT-EXACT state. Replay is
    /// the only mechanism that can be exact: per-voice state (fractional
    /// playback `position += rate`, recursive gain-ramp updates, loop-wrap
    /// arithmetic) is accumulated per rendered frame, and float
    /// accumulation is only reproducible by performing the identical
    /// accumulation — a closed-form "advance envelope to offset n" would
    /// round differently. The window is bounded by the span length: chained
    /// legato is replayed from its phrase start (a transition's identity
    /// depends on the line's sounding note, so the chain context must be
    /// rebuilt, not just the straddling voice).
    pub fn reconstruction_start(&self, p: u64) -> u64 {
        let i = self.busy.partition_point(|s| s.0 <= p);
        if i > 0 {
            let (start, end) = self.busy[i - 1];
            if p < end {
                return start;
            }
        }
        p
    }
}

/// Conservative bound (seconds) on how long a note keeps voices alive after
/// its note-off: release envelope + recorded release-sample tails + legato
/// fade-outs. Overestimating only widens (merges) activity spans — replay
/// gets longer but stays exact; underestimating would silently drop a
/// still-ringing tail from reconstruction. 6 s comfortably exceeds any
/// release sample in the supported libraries.
pub const RECONSTRUCT_TAIL_SEC: f64 = 6.0;

// ── Annotation (shared model: keyflow-annotate) ───────────────────────────────

/// The channel's CC timeline for one controller (state machine over events).
fn cc_timeline(ccs: &[DocCc], chan: u8, cc: u8) -> CcTimeline {
    CcTimeline::from_events(
        ccs.iter()
            .filter(|e| e.chan == chan && e.cc == cc)
            .map(|e| (e.qn, e.val)),
    )
}

/// What the spec says a CC58 state plays. `None` label / no articulation
/// match (mode switches, bare state) ⇒ the engine's default long
/// articulation, i.e. sustain-like.
fn kind_for_ks(spec: &LibrarySpec, ks_val: Option<u8>) -> ArticulationKind {
    let Some(val) = ks_val else {
        return ArticulationKind::Sustain;
    };
    let Some(label) = spec.keyswitch.as_ref().and_then(|ks| ks.cc58_function(val)) else {
        return ArticulationKind::Sustain;
    };
    spec.articulations
        .iter()
        .find(|a| a.id == label || a.label == label)
        .map(|a| a.kind.clone())
        .unwrap_or(ArticulationKind::Sustain)
}

/// What the spec says a latched-CC selector code plays. Unmatched code /
/// no `ks_val` ⇒ the engine's default long articulation, i.e. sustain-like.
fn kind_for_selector(
    spec: &LibrarySpec,
    sel: &LatchedCcSelector,
    ks_val: Option<u8>,
) -> ArticulationKind {
    ks_val
        .and_then(|v| sel.artic_for(v))
        .and_then(|id| spec.articulation(id))
        .map(|a| a.kind.clone())
        .unwrap_or(ArticulationKind::Sustain)
}

/// The articulation SPEC a CC58 state resolves to, when the value maps to a
/// concrete articulation (not a mode/toggle band). Mirrors [`kind_for_ks`]
/// but keeps the identity — the per-note arrival pre-roll needs the actual
/// zone articulation ids, not just the kind.
fn artic_for_ks(
    spec: &LibrarySpec,
    ks_val: Option<u8>,
) -> Option<&crate::spec::ArticulationSpec> {
    let val = ks_val?;
    let label = spec.keyswitch.as_ref()?.cc58_function(val)?;
    spec.articulations
        .iter()
        .find(|a| a.id == label || a.label == label)
}

/// DIAGNOSTIC arrival-semantics sweep knob (`SIGNAL_ARRIVAL_SEMANTICS` =
/// `"fraction[,bias_ms]"`): re-interprets every transition arrival marker as
/// `lt_offset + fraction × (marker − lt_offset) + bias_ms` at RENDER time —
/// the ear-calibration sweep for "where in the sample is the note heard"
/// (see `signal-orchestra/examples/sweep_arrival_semantics.rs`). Read live
/// (no caching) so one process can render several lanes; unset = `(1, 0)` =
/// the markers as measured. Offline/diagnostic only — never part of a
/// shipped configuration.
pub(crate) fn arrival_semantics_env() -> (f32, f32) {
    match std::env::var("SIGNAL_ARRIVAL_SEMANTICS") {
        Ok(v) => {
            let mut it = v.split(',');
            let f = it.next().and_then(|x| x.trim().parse().ok()).unwrap_or(1.0);
            let b = it.next().and_then(|x| x.trim().parse().ok()).unwrap_or(0.0);
            (f, b)
        }
        Err(_) => (1.0, 0.0),
    }
}

/// The default long articulation a fresh sustain plays when no keyswitch has
/// selected one — mirrors the engine's initial pick (first `Sustain`-kind
/// articulation, else the first playable one).
fn default_sustain_artic(spec: &LibrarySpec) -> Option<&crate::spec::ArticulationSpec> {
    spec.articulations
        .iter()
        .find(|a| a.kind == ArticulationKind::Sustain)
        .or_else(|| {
            spec.articulations.iter().find(|a| {
                !matches!(
                    a.kind,
                    ArticulationKind::Release | ArticulationKind::Legato
                )
            })
        })
}

/// Whether a latched-CC selector code names a marcato-technique
/// articulation (no sampled pre-delay → zero-lead prefire, mirroring the
/// CC58 marcato-band rule).
fn selector_is_marcato(spec: &LibrarySpec, sel: &LatchedCcSelector, val: u8) -> bool {
    sel.artic_for(val)
        .and_then(|id| spec.articulation(id))
        .is_some_and(|a| {
            let m = |s: &str| s.to_ascii_lowercase().contains("marcato");
            m(&a.id) || m(&a.label)
        })
}

/// Working copy of a note through annotation.
struct ANote {
    src: DocNote,
    /// CC58 state at note-on (articulation identity) — filtered of
    /// legato-toggle / sordino presses, which are state, not articulation
    /// (parity: mirror.rs `MNote::ks_val`).
    ks_val: Option<u8>,
    kind: ArticulationKind,
    /// This note is reached by a legato transition (different-pitch overlap
    /// or same-pitch re-bow) → arrives via `LegatoPrefire`.
    legato_from: bool,
    /// Legato SPEED mode active at this note: `true` = Expressive (CC58 6-10),
    /// `false` = Low Latency (CSS default / CC58 0-5). Selects the pre-delay
    /// table for the prefire lead.
    legato_expressive: bool,
    /// This note flows into ANY legato transition (different-pitch move or
    /// same-pitch re-bow) → its note-off is dropped. The scheduled transition
    /// into the next note fades this voice out; emitting a note-off instead
    /// races that prefire and can double the arrived voice (or spawn a spurious
    /// mid-phrase release). Only genuine phrase-ends keep their note-off.
    legato_to: bool,
    /// Marcato technique at this note (no sampled pre-delay → zero-lead
    /// prefire): CC58 marcato band, or a latched-CC selector code resolving
    /// to a marcato articulation.
    marcato: bool,
}

impl ANote {
    fn is_marcato(&self) -> bool {
        self.marcato
    }

    fn is_sustain_like(&self) -> bool {
        matches!(
            self.kind,
            ArticulationKind::Sustain | ArticulationKind::Looped | ArticulationKind::Trill
        )
    }

    fn is_short(&self) -> bool {
        matches!(
            self.kind,
            ArticulationKind::Short | ArticulationKind::OneShot
        )
    }
}

/// Stage 1 of [`annotate`] on its own: infer each note's articulation state
/// (CC58 at note-on) and the legato/re-bow edges, per engine line — a thin
/// grouping adapter over [`keyflow_annotate::annotate_line`] (the ONE shared
/// implementation, also consumed by keyflow-orchestra's mirror pass, where
/// it is parity-tested against the CSS reference engine). `legato_capable`
/// is `spec.legato_engine.is_some()` in [`annotate`]. The adapter-level
/// parity with the mirror pass is asserted in `tests/annotation_parity.rs`.
pub fn stage1_annotations(doc: &TrackDocument, legato_capable: bool) -> Vec<NoteAnnotation> {
    stage1_annotations_impl(doc, legato_capable, None, None)
}

/// [`stage1_annotations`], selector-aware: when the spec configures a
/// latched-CC articulation selector (`selector uacc`), the keyswitch
/// timeline is the SELECTOR CC (not CC58), every selector value is an
/// articulation code (no CC58 toggle-band filtering), and re-bow blocking
/// derives from the RESOLVED articulation's kind (shorts don't connect)
/// instead of the CC58 band classification. Without a selector this is
/// byte-identical to [`stage1_annotations`] (parity-locked path).
fn stage1_annotations_impl(
    doc: &TrackDocument,
    legato_capable: bool,
    sel: Option<&LatchedCcSelector>,
    spec: Option<&LibrarySpec>,
) -> Vec<NoteAnnotation> {
    let params = EdgeParams {
        break_gap_qn: BREAK_GAP_QN,
        rebow_capable: legato_capable,
        // Performance-domain source: same-pitch overlaps deeper than the
        // abutment window still connect — on a mono line an overlapping
        // repeat can only be a re-bow; treating it as a break would leave
        // the line sounding at the next note-on and push it down the
        // reactive path.
        connect_same_pitch_overlap: true,
    };
    let (line_of, _blocks) = assign_lines(doc);
    let mut ann = vec![NoteAnnotation::default(); doc.notes.len()];
    let mut by_line: std::collections::BTreeMap<u8, Vec<usize>> = std::collections::BTreeMap::new();
    for (i, &line) in line_of.iter().enumerate().take(doc.notes.len()) {
        by_line.entry(line).or_default().push(i);
    }
    for list in by_line.values_mut() {
        list.sort_by(|&a, &b| doc.notes[a].start_qn.total_cmp(&doc.notes[b].start_qn));
    }
    for list in by_line.values() {
        // Keyswitch state comes from the line's SOURCE channel (every note
        // in a line shares one source channel under both allocators).
        let ch = doc.notes[list[0]].chan;
        let ks = cc_timeline(&doc.ccs, ch, sel.map(|s| s.cc).unwrap_or(58));
        let line: Vec<LineNote> = list
            .iter()
            .map(|&ni| LineNote {
                start_qn: doc.notes[ni].start_qn,
                end_qn: doc.notes[ni].end_qn,
                pitch: doc.notes[ni].pitch as i32,
            })
            .collect();
        // No notation hints here — the document is MIDI-domain only.
        let line_ann = match (sel, spec) {
            (Some(sel), Some(spec)) => annotate_line_with(
                &line,
                &ks,
                // Shorts never connect, so a same-pitch abutment between
                // them is a break, not a re-bow (kind-driven counterpart of
                // the CC58 `ks_blocks_rebow` bands).
                |_, val| {
                    matches!(
                        kind_for_selector(spec, sel, val),
                        ArticulationKind::Short | ArticulationKind::OneShot
                    )
                },
                // Every latched-CC selector value is an articulation code —
                // the convention has no toggle/mode presses to filter.
                |_| true,
                &params,
            ),
            _ => annotate_line(&line, &ks, default_blocks_rebow, &params),
        };
        for (w, &ni) in list.iter().enumerate() {
            ann[ni] = line_ann[w];
        }
    }
    ann
}

/// Annotate a document against a library spec into a frame-domain
/// [`Schedule`].
///
/// Inference (ported from `keyflow-orchestra/src/mirror.rs::mirror_part`,
/// stage 1 — keep in parity):
/// - **articulation** = CC58 keyswitch state at each note-on
/// - **legato edge** = different-pitch overlap with the previous note on the
///   same channel
/// - **re-bow** = same-pitch abutment (gap ≤ ~1/32 QN) between connectable
///   (sustain-family) notes
///
/// Timing inversion (this module's contribution):
/// - legato-followed sustains → [`DocEvent::LegatoPrefire`] at
///   `start − mode.delay_for_velocity(vel)`, where `mode` is the CC58-selected
///   legato speed (Low Latency by default, Expressive on CC58 6-10)
/// - shorts → note-on pre-rolled by `short_note_timing.pre_delay_ms`
/// - marcato keyswitch state → no sampled pre-delay → no pull (mirror parity)
pub fn annotate(doc: &TrackDocument, spec: &LibrarySpec, sample_rate: u32) -> Schedule {
    // Legato speed modes. CSS's DEFAULT is Low Latency (manual: "the easiest to
    // use, and I recommend using this mode for most work"); Expressive is only
    // active while a CC58 keyswitch selects it (0-5 = Low Latency, 6-10 =
    // Expressive). We pick the delay table per note from the last legato-mode
    // keyswitch, defaulting to Low Latency. `primary_mode` covers single-mode
    // libraries (brass, etc.) that ship flat zones instead.
    let low_latency = spec
        .legato_engine
        .as_ref()
        .and_then(|le| le.low_latency.clone());
    let expressive = spec
        .legato_engine
        .as_ref()
        .and_then(|le| le.expressive.clone());
    let primary = spec.legato_engine.as_ref().and_then(|le| le.primary_mode());
    let ll_range = low_latency
        .as_ref()
        .and_then(|m| m.enabled_cc58_range.as_deref())
        .and_then(crate::spec::parse_range);
    let exp_range = expressive
        .as_ref()
        .and_then(|m| m.enabled_cc58_range.as_deref())
        .and_then(crate::spec::parse_range);
    let in_range = |r: Option<(u8, u8)>, v: u8| r.is_some_and(|(lo, hi)| v >= lo && v <= hi);
    let porta_vel_max = spec
        .legato_engine
        .as_ref()
        .and_then(|le| le.portamento.as_ref())
        .map(|p| p.trigger_vel_max)
        .unwrap_or(0);
    let short_pre_frames = spec
        .short_note_timing
        .as_ref()
        .map(|s| ms_to_frames_i64(s.pre_delay_ms, sample_rate))
        .unwrap_or(0);
    let legato_capable = spec.legato_engine.is_some();

    // Line allocation FIRST (channel→line identity, or the auto-divisi
    // ranking), so all inference below runs per assigned mono line.
    let (line_of, chan_blocks) = assign_lines(doc);

    // Latched-CC articulation selector (spec `selector uacc`), if any: the
    // articulation identity then rides the selector CC instead of CC58.
    // r[impl signal.sampling.articulation.select]
    let selector = spec.latched_cc_selector();

    // Stage 1 — articulation state + legato/re-bow edges (mirror.rs parity),
    // per line. Keyswitch state comes from the line's SOURCE channel (every
    // note in a line shares one source channel under both allocators).
    let ann = stage1_annotations_impl(doc, legato_capable, selector.as_ref(), Some(spec));

    // Working notes grouped per LINE, sorted by source start.
    let mut notes: Vec<ANote> = doc
        .notes
        .iter()
        .enumerate()
        .map(|(i, &src)| ANote {
            src,
            ks_val: ann[i].ks_val,
            kind: match &selector {
                Some(sel) => kind_for_selector(spec, sel, ann[i].ks_val),
                None => kind_for_ks(spec, ann[i].ks_val),
            },
            legato_from: ann[i].legato_from,
            legato_to: ann[i].legato_to,
            legato_expressive: false,
            marcato: match &selector {
                Some(sel) => ann[i]
                    .ks_val
                    .is_some_and(|v| selector_is_marcato(spec, sel, v)),
                None => ann[i].ks_val.map(ks_is_marcato).unwrap_or(false),
            },
        })
        .collect();
    let mut by_line: std::collections::BTreeMap<u8, Vec<usize>> = std::collections::BTreeMap::new();
    for (i, &line) in line_of.iter().enumerate().take(notes.len()) {
        by_line.entry(line).or_default().push(i);
    }
    for list in by_line.values_mut() {
        list.sort_by(|&a, &b| notes[a].src.start_qn.total_cmp(&notes[b].src.start_qn));
    }

    // Legato speed mode. DOCUMENT (Lookahead) rendering ALWAYS uses the
    // EXPRESSIVE table: offline scoring has no latency budget, so it takes the
    // musical, fully-sampled transition (the CSS Expressive velocity zones —
    // slow/medium/fast → 333/250/100 ms). The Low-Latency table is the LIVE
    // (StrictLive) responsiveness compromise and is applied on that path only
    // (`SampleEngine::legato_expressive`, default false). A CC58 that explicitly
    // selects Low-Latency (0-5) still honors the author's intent; absent any
    // keyswitch, document defaults to Expressive (NOT Low-Latency). Velocity —
    // not the mode — always drives the slow/medium/fast zone within the table.
    for list in by_line.values() {
        let ch = notes[list[0]].src.chan;
        let ks = cc_timeline(&doc.ccs, ch, 58);
        for &ni in list {
            let mode_v = ks.last_matching(notes[ni].src.start_qn, |v| {
                in_range(ll_range, v) || in_range(exp_range, v)
            });
            // Default true (Expressive) for document; only an explicit
            // Low-Latency keyswitch (CC58 in ll_range) drops it to false.
            notes[ni].legato_expressive = mode_v.map_or(true, |v| !in_range(ll_range, v));
        }
    }

    // Stage 2 — schedule emission with the timing inversion.
    let mut events: Vec<(u64, u8, u8, DocEvent)> = Vec::new(); // (frame, prio, line, ev)
    let mut legato_count = 0usize;
    let mut short_count = 0usize;

    for e in &doc.ccs {
        // CC64 (sustain pedal) is dropped from the schedule: its note-off
        // DEFERRAL semantics contradict a fully scheduled document (every
        // note-off is already explicit, and same-pitch re-bows are inferred
        // structurally above). Keyflow/CSS use CC64 pulses as a re-bow
        // accent hint; forwarding them would hold the mono line open and
        // push later note-ons down the reactive path.
        if e.cc == 64 {
            continue;
        }
        let frame = qn_to_frame(&doc.tempo, e.qn, sample_rate).max(0) as u64;
        // A CC event addresses its source channel's whole line block: under
        // auto-divisi one channel fans out into several lines, and its CC1
        // expression lane must drive ALL of them (CC58/mode CCs are
        // engine-global at dispatch, so replication is harmless there).
        let (base, width) = chan_blocks
            .get(&e.chan)
            .copied()
            .unwrap_or((line_capped(e.chan), 1));
        for line in base..base.saturating_add(width.max(1)) {
            events.push((
                frame,
                0,
                line,
                DocEvent::Cc {
                    cc: e.cc,
                    val: e.val,
                },
            ));
        }
    }

    // Per-note engine-activity spans (trigger → ring-out) for the
    // reconstruction map, merged below.
    let tail_frames = (RECONSTRUCT_TAIL_SEC * sample_rate as f64).round() as u64;
    let mut spans: Vec<(u64, u64)> = Vec::with_capacity(doc.notes.len());
    // Marker timeline: ~2–3 markers per note (start/arrival [+ release]).
    let mut markers: Vec<RenderMarker> = Vec::with_capacity(doc.notes.len() * 3);

    for (&line, list) in &by_line {
        // Previous trigger frame on this line — keeps the mono line's
        // trigger order strict even when a pre-roll would cross it.
        let mut prev_trigger: i64 = -1;
        // Previous note-on frame on this line — the IOI clock for the
        // Overlap-Delay (spec §2.1: legato delay is IOI-driven, not velocity).
        let mut prev_start: Option<i64> = None;
        // Previous note PITCH on this line — the legato `from` for the per-move
        // measured-arrival lookup that sets the `$1fvjk` pre-bow lead.
        let mut prev_pitch: Option<u8> = None;
        // Frame of the previous note's scheduled NOTE-OFF on this line. A
        // fresh sustain's arrival pre-roll must not cross it: dispatching a
        // sustain note-on while the line still sounds reads as a legato
        // continuation and would fire the REACTIVE transition path. Clamping
        // here keeps the mono line's phrase boundaries intact (the arrival
        // lands late by the shortfall — you cannot pre-roll into the
        // previous note).
        let mut prev_off: Option<i64> = None;
        for &ni in list {
            let n = &notes[ni];
            let start = qn_to_frame(&doc.tempo, n.src.start_qn, sample_rate);
            let end = qn_to_frame(&doc.tempo, n.src.end_qn, sample_rate);
            let body_rr = stable_rr_slot(
                doc.seed,
                n.src.start_qn,
                n.src.pitch,
                n.src.chan,
                RrPurpose::Body,
            );

            // Per-note fresh-attack pre-roll: the LARGEST measured
            // heard-arrival over the zones this trigger can fire (the
            // per-RR/mic/layer replacement for the single global
            // `pre_delay_ms`); the engine holds each spawned voice back by
            // `lead − its own zone's arrival` so the note is HEARD on the
            // tick. Fallbacks keep the historical behaviour: shorts →
            // `pre_delay_ms`, sustains → trigger-on-tick.
            let attack_ids: Vec<String> = {
                let named = match &selector {
                    Some(sel) => n
                        .ks_val
                        .and_then(|v| sel.artic_for(v))
                        .and_then(|id| spec.articulation(id)),
                    None => artic_for_ks(spec, n.ks_val),
                };
                let base = if n.is_sustain_like() {
                    named.or_else(|| default_sustain_artic(spec))
                } else {
                    named
                };
                let mut ids: Vec<String> = base.iter().map(|a| a.id.clone()).collect();
                // A sustain plays BOTH CC2 vibrato sides — the pre-roll must
                // bound both articulations' zones.
                if n.is_sustain_like() {
                    if let Some(pair) = ids
                        .first()
                        .and_then(|id| spec.vibrato_counterpart(id))
                    {
                        ids.push(pair);
                    }
                }
                ids
            };
            let attack_lead_frames = {
                let id_refs: Vec<&str> = attack_ids.iter().map(String::as_str).collect();
                spec.max_attack_arrival_ms(&id_refs, n.src.pitch)
                    .map(|ms| ((f64::from(ms) / 1000.0) * f64::from(sample_rate)).round() as i64)
            };
            // Fresh-attack grid policy (`performance.attack_placement`):
            // `start_at_tick` starts the sustain sample ON the tick — no
            // audio before the click the note starts on; the bloom speaks
            // naturally after. Shorts are unaffected (their pre-roll is the
            // recorded attack noise before the rhythmic peak, and their
            // per-RR arrival compensation stays).
            let sustain_start_at_tick = spec
                .performance
                .attack_placement
                .eq_ignore_ascii_case("start_at_tick");

            let (trigger_frame, kind) = if n.is_short() {
                // Shorts: pre-roll by the measured per-zone arrival bound,
                // falling back to the global recorded pre-delay.
                short_count += 1;
                (
                    start - attack_lead_frames.unwrap_or(short_pre_frames),
                    DocEvent::NoteOn {
                        note: n.src.pitch,
                        vel: n.src.vel,
                        rr: body_rr,
                        lead: 0, // filled in below, after the mono-order clamp
                    },
                )
            } else if n.legato_from && n.is_sustain_like() && legato_capable {
                // THE INVERSION: fire the transition early so the arrival
                // lands on the tick. The lead is the MEASURED lead-in of the
                // transition sample for this `from → to` move (per-zone
                // `lead_in_ms`, written by the CSS generator; 0 for a
                // same-pitch re-bow — Legzero has no lead-in), falling back
                // to the spec's expressive velocity curve for libraries
                // without measured zones. Portamento (vel ≤ threshold) and
                // marcato (no sampled pre-delay — mirror parity: "no pull")
                // prefire with ZERO lead: the transition fires exactly on
                // the tick, never through the reactive countdown.
                legato_count += 1;
                let vel = n.src.vel;
                // CSS: the trigger→arrival delay is the VELOCITY-ZONE value
                // (Expressive 333/250/100 ms; Low Latency 150/100), NOT the
                // recorded swell length. Prefire by exactly that so the arrival
                // lands on the grid; the engine truncates the sample's measured
                // lead-in to this delay via `start_offset` (skips the surplus
                // swell). Using the full measured lead instead made fast
                // passages overlap — consecutive swells stacked into a doubled,
                // "sudden" transition. Marcato and portamento (vel ≤ threshold)
                // have no sampled pre-delay → zero lead, fire on the tick.
                // Overlap-Delay is IOI-driven (spec §2.1): the delay follows the
                // inter-onset interval — time since the previous note-on on the
                // line — interpolated across the KSP thresholds, NOT the
                // velocity zone. Marcato and portamento (vel ≤ threshold) have
                // no sampled pre-delay → fire on the tick.
                let lead_ms = if n.is_marcato() || (porta_vel_max > 0 && vel <= porta_vel_max) {
                    0
                } else {
                    let ioi_frames = prev_start.map(|p| (start - p).max(0)).unwrap_or(0);
                    let ioi_ms = crate::engine::frames_to_ms(ioi_frames as u64, sample_rate);
                    // The spec's velocity-zone delay for the mode this note
                    // resolved (Expressive / Low-Latency / flat) — the
                    // library's own trigger→arrival latency ceiling.
                    let mode_delay_ms = if n.legato_expressive {
                        expressive.as_ref()
                    } else {
                        low_latency.as_ref()
                    }
                    .or(primary.as_ref())
                    .and_then(|m| m.delay_for_velocity(vel))
                    .unwrap_or(LEGATO_DELAY_FALLBACK_MS);
                    // CSS `$1fvjk`: the transition sample starts `start_offset_ms`
                    // in (IOI-interpolated 177 → 117 ms). To land the destination
                    // pitch on the tick with that offset applied, prefire by the
                    // AUDIBLE pre-bow that still plays before the arrival =
                    // (per-move measured arrival − $1fvjk) — CAPPED at the
                    // mode's velocity-zone delay: the measured lead-ins spread
                    // wide (median ~330 ms, outliers > 1 s), and an uncapped
                    // lead pulls the whole handoff (previous-voice retire +
                    // destination sustain) that far early, collapsing the
                    // phrase. The cap is the library's own advertised
                    // latency; the engine's `start_offset = arrival − lead·rate`
                    // still lands the arrival exactly ON the tick (it just
                    // skips deeper into the swell). Libraries without measured
                    // transitions keep the Overlap-Delay curve
                    // (`LegatoEngineSpec::overlap_delay_ms`).
                    // r[impl signal.soundsource.legato.offline]
                    // CSS-FAITHFUL prefire (Kontakt model). The decoded KSP
                    // stores NO per-sample arrival: it applies one uniform
                    // policy (Overlap-Delay wait + a single `$1fvjk` start-offset
                    // curve) and the recordings are edited so the destination
                    // speaks a FIXED trigger→arrival latency per velocity zone —
                    // Expressive 333/250/100 ms, Low-Latency 150/100 ms. So
                    // prefire by exactly that constant (`mode_delay_ms`) and let
                    // the recording — played from its `$1fvjk` offset, applied
                    // engine-side in `spawn_transition_voice` — deliver the
                    // arrival. Do NOT second-guess with per-zone measured
                    // `transition_arrival_ms`: that median-of-nearest-root scheme
                    // spread the leads (270-290 ms, never the manual's flat
                    // number) and was only compensating for the old playback-RATE
                    // error, now fixed by the time-preserving pitch shift.
                    //
                    // Same-pitch re-bow (Legzero) is a different mechanism — no
                    // swell to skip — so it keeps its measured re-attack onset.
                    let want = if prev_pitch == Some(n.src.pitch) {
                        spec.legato_lead_ms(n.src.pitch, n.src.pitch)
                            .unwrap_or(mode_delay_ms) as f32
                    } else {
                        mode_delay_ms as f32
                    };
                    // Room clamp for FAST passages only: a note played faster
                    // than its own zone delay cannot prefire by the full delay
                    // without the handoff preceding the previous note. Leave the
                    // previous note max(150 ms, 35 % of the IOI) of its own sound;
                    // at normal spacing this never bites, so the lead stays the
                    // uniform velocity-zone constant.
                    let room = if ioi_ms > 0.0 {
                        (ioi_ms - (ioi_ms * 0.35).max(150.0)).max(0.0)
                    } else {
                        want
                    };
                    want.min(room).round() as u32
                };
                if std::env::var_os("SIGNAL_ANNOTATE_DEBUG").is_some() {
                    eprintln!(
                        "ANNOTATE legato pitch={} vel={} expr={} marcato={} lead_ms={} prev={:?} lookup={:?}",
                        n.src.pitch, vel, n.legato_expressive, n.is_marcato(), lead_ms, prev_pitch,
                        prev_pitch.and_then(|from| spec.legato_lead_ms(from, n.src.pitch))
                    );
                }
                let rr = stable_rr_slot(
                    doc.seed,
                    n.src.start_qn,
                    n.src.pitch,
                    n.src.chan,
                    RrPurpose::Transition,
                );
                (
                    start - ms_to_frames_i64(lead_ms, sample_rate),
                    DocEvent::LegatoPrefire {
                        note: n.src.pitch,
                        vel,
                        rr,
                        lead: 0, // filled in below, after the mono-order clamp
                    },
                )
            } else {
                // Fresh sustain attack: under `arrive_at_tick`, pre-roll by
                // the measured perceptual-onset bound so the note SPEAKS on
                // the tick; under `start_at_tick` (or without measurements)
                // the trigger IS the tick and the bloom follows naturally.
                (
                    // Clamped at the previous note-off (see `prev_off`).
                    (start
                        - if sustain_start_at_tick {
                            0
                        } else {
                            attack_lead_frames.unwrap_or(0)
                        })
                    .max(prev_off.unwrap_or(i64::MIN)),
                    DocEvent::NoteOn {
                        note: n.src.pitch,
                        vel: n.src.vel,
                        rr: body_rr,
                        lead: 0, // filled in below, after the mono-order clamp
                    },
                )
            };

            let trigger_frame = trigger_frame.max(prev_trigger + 1).max(0);
            prev_trigger = trigger_frame;
            // Every trigger's lead is measured from its FINAL (clamped)
            // trigger frame, so `frame + lead == grid tick` holds exactly.
            let kind = match kind {
                DocEvent::LegatoPrefire { note, vel, rr, .. } => DocEvent::LegatoPrefire {
                    note,
                    vel,
                    rr,
                    lead: (start - trigger_frame).max(0) as u32,
                },
                DocEvent::NoteOn { note, vel, rr, .. } => DocEvent::NoteOn {
                    note,
                    vel,
                    rr,
                    lead: (start - trigger_frame).max(0) as u32,
                },
                other => other,
            };
            let prio = match kind {
                DocEvent::LegatoPrefire { .. } => 2,
                _ => 3,
            };
            events.push((trigger_frame as u64, prio, line, kind));
            // Marker timeline: the trigger, and the heard ARRIVAL on the
            // note's grid tick (trigger + lead for transitions, trigger +
            // pre_delay for pre-rolled shorts, the trigger itself for fresh
            // sustains — all equal `start` by construction).
            let start_marker = match kind {
                DocEvent::LegatoPrefire { .. } if prev_pitch == Some(n.src.pitch) => {
                    MarkerKind::Rebow
                }
                DocEvent::LegatoPrefire { .. } => MarkerKind::TransitionStart,
                _ => MarkerKind::NoteStart,
            };
            markers.push(RenderMarker {
                frame: trigger_frame as u64,
                line,
                note: n.src.pitch,
                kind: start_marker,
            });
            markers.push(RenderMarker {
                frame: start.max(trigger_frame) as u64,
                line,
                note: n.src.pitch,
                kind: MarkerKind::Arrival,
            });
            spans.push((
                trigger_frame as u64,
                end.max(trigger_frame + 1).max(0) as u64 + tail_frames,
            ));

            // Note-off: dropped for re-bow sources — the transition into the
            // next same-pitch note replaces the release (fading this note is
            // `fire_legato`'s job, not a release tail's).
            if !n.legato_to {
                let rr = stable_rr_slot(
                    doc.seed,
                    n.src.start_qn,
                    n.src.pitch,
                    n.src.chan,
                    RrPurpose::Release,
                );
                prev_off = Some(end.max(trigger_frame + 1).max(0));
                events.push((
                    end.max(trigger_frame + 1).max(0) as u64,
                    1,
                    line,
                    DocEvent::NoteOff {
                        note: n.src.pitch,
                        rr,
                    },
                ));
                markers.push(RenderMarker {
                    frame: end.max(trigger_frame + 1).max(0) as u64,
                    line,
                    note: n.src.pitch,
                    kind: MarkerKind::Release,
                });
            }

            // IOI clock: the next note on this line measures its Overlap-Delay
            // from this note's (source) start.
            prev_start = Some(start);
            prev_pitch = Some(n.src.pitch);
        }
    }

    // Cc(0) < NoteOff(1) < LegatoPrefire(2) < NoteOn(3) at equal frames, so
    // keyswitch state lands before the notes it governs and releases precede
    // re-triggers. Vec::sort_by is stable → equal keys keep emission order.
    events.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    let end_frame = events.last().map(|e| e.0).unwrap_or(0);
    let max_prefire_lead = events
        .iter()
        .filter_map(|(_, _, _, kind)| match kind {
            DocEvent::LegatoPrefire { lead, .. } => Some(*lead as u64),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    Schedule {
        sample_rate,
        events: events
            .into_iter()
            .map(|(frame, _prio, line, kind)| ScheduledEvent { frame, line, kind })
            .collect(),
        end_frame,
        seed: doc.seed,
        note_count: doc.notes.len(),
        legato_count,
        short_count,
        max_prefire_lead,
        tempo: doc.tempo.clone(),
        markers: {
            markers.sort_by(|a, b| a.frame.cmp(&b.frame));
            markers
        },
        busy: merge_spans(spans),
    }
}

/// Merge overlapping/adjacent activity spans into a sorted disjoint set.
fn merge_spans(mut spans: Vec<(u64, u64)>) -> Vec<(u64, u64)> {
    spans.sort_unstable();
    let mut merged: Vec<(u64, u64)> = Vec::with_capacity(spans.len());
    for (a, b) in spans {
        match merged.last_mut() {
            Some((_, end)) if a <= *end => *end = (*end).max(b),
            _ => merged.push((a, b)),
        }
    }
    merged
}

// ── Line allocation ───────────────────────────────────────────────────────────

/// The channel→line allocator (import path): a document whose notes carry
/// meaningful MIDI channels (e.g. keyflow's divisi export) maps channel N →
/// engine line N. Lines are first-class engine entities, NOT channels —
/// this is merely the first of several allocators in front of the line pool
/// (lookahead auto-divisi in `annotate` and live greedy auto-divisi assign
/// [`LineId`]s from note ranking instead; see `docs/plan/document-mode.md`,
/// "Auto-divisi").
pub fn line_for_chan(chan: u8) -> LineId {
    chan as LineId
}

/// `line_for_chan`, clamped into the engine's line pool as a u8.
fn line_capped(chan: u8) -> u8 {
    line_for_chan(chan).min(crate::engine::MAX_LINES - 1) as u8
}

/// Per-note engine-line assignment, plus each source channel's line block
/// `(base, width)` — CC events replicate across their channel's block so a
/// channel's expression lane drives every line allocated from it.
///
/// Without `auto_divisi`: channel N → line N (blocks of width 1).
///
/// With `auto_divisi`: the lookahead allocator — a port of
/// keyflow-orchestra's `assign_channels`
/// (`crates/keyflow-orchestra/src/engine/mod.rs`, parity-tested there
/// against the CSS reference engine). Each note's rank = 1 + how many
/// same-channel notes are actually SOUNDING at its onset with a higher
/// pitch (ties: earlier onset wins) — a held top note never loses its line
/// to a re-articulated lower note. Rank 1 = top → first line of the
/// channel's block; channels get disjoint blocks in ascending order. Pure
/// function of the document: deterministic and seed-independent.
fn assign_lines(doc: &TrackDocument) -> (Vec<u8>, std::collections::BTreeMap<u8, (u8, u8)>) {
    use std::collections::BTreeMap;
    let notes = &doc.notes;
    let max_line = (crate::engine::MAX_LINES - 1) as u32;

    if !doc.auto_divisi {
        let mut blocks: BTreeMap<u8, (u8, u8)> = BTreeMap::new();
        for n in notes {
            blocks.insert(n.chan, (line_capped(n.chan), 1));
        }
        for e in &doc.ccs {
            blocks.entry(e.chan).or_insert((line_capped(e.chan), 1));
        }
        return (notes.iter().map(|n| line_capped(n.chan)).collect(), blocks);
    }

    let mut chans: Vec<u8> = notes.iter().map(|n| n.chan).collect();
    chans.extend(doc.ccs.iter().map(|e| e.chan));
    chans.sort_unstable();
    chans.dedup();

    let ranks: Vec<u32> = (0..notes.len())
        .map(|i| {
            let n = &notes[i];
            let mut rank = 1u32;
            for (j, m) in notes.iter().enumerate() {
                if j != i
                    && m.chan == n.chan
                    && m.start_qn <= n.start_qn + EPS
                    // "Sounding at n's onset". One deviation from the
                    // notation-domain original (which sees abutting notes):
                    // documents are performance-domain, so legato pairs
                    // OVERLAP — a note that ends within the hand-off window
                    // after n starts is n's line-predecessor letting go, not
                    // a held note above it, and must not displace n's rank
                    // (otherwise every descending legato step would hop
                    // lines).
                    && m.end_qn > n.start_qn + DIVISI_HANDOFF_QN
                    && (m.pitch > n.pitch
                        || (m.pitch == n.pitch && m.start_qn < n.start_qn - EPS))
                {
                    rank += 1;
                }
            }
            rank
        })
        .collect();

    let mut width: BTreeMap<u8, u32> = chans.iter().map(|&c| (c, 1)).collect();
    for (i, n) in notes.iter().enumerate() {
        let w = width.get_mut(&n.chan).expect("chan registered");
        *w = (*w).max(ranks[i]);
    }

    let mut blocks: BTreeMap<u8, (u8, u8)> = BTreeMap::new();
    let mut base = 0u32;
    for &c in &chans {
        let w = width[&c];
        let b = base.min(max_line);
        let w_capped = w.min(max_line + 1 - b).max(1);
        blocks.insert(c, (b as u8, w_capped as u8));
        base += w;
    }

    let line_of = notes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let (b, _) = blocks[&n.chan];
            (b as u32 + ranks[i] - 1).min(max_line) as u8
        })
        .collect();
    (line_of, blocks)
}

// ── Offline schedule playback ─────────────────────────────────────────────────

/// Options for [`render_schedule`] /
/// [`SamplerRig::render_offline_document`](crate::SamplerRig::render_offline_document).
#[derive(Debug, Clone)]
pub struct DocumentRenderOptions {
    /// Render chunk size (frames). Chunking is deterministic (it depends only
    /// on the schedule), so this affects performance, not output.
    pub block_frames: usize,
    /// Extra tail rendered after the last event (release/reverb ring-out).
    pub tail_sec: f64,
    /// Start the render at this absolute frame. **Playback-start
    /// invariance**: the output is the FULL render sliced at this frame,
    /// bit-exactly — voices alive across it (sustains mid-sample, legato
    /// transitions mid-window, release tails mid-ring) are reconstructed by
    /// deterministic replay from [`Schedule::reconstruction_start`].
    pub start_frame: u64,
}

impl Default for DocumentRenderOptions {
    fn default() -> Self {
        Self {
            block_frames: 512,
            tail_sec: 3.0,
            start_frame: 0,
        }
    }
}

/// Result of one offline document render.
#[derive(Debug, Clone, Default)]
pub struct DocumentRenderResult {
    /// Interleaved stereo audio, starting at `start_frame`.
    pub audio: Vec<f32>,
    pub sample_rate: u32,
    pub start_frame: u64,
    pub seed: u64,
    pub note_count: usize,
    /// Legato transitions actually fired by the engine, with frames relative
    /// to `start_frame`.
    pub transitions: Vec<crate::engine::LegatoFireEvent>,
    /// REACTIVE legato-path triggers observed during playback. The schedule
    /// prefires every transition, so this must be 0 — anything else means an
    /// edge the annotator missed degraded to live-mode (late) timing.
    pub reactive_fallbacks: u64,
    /// Trigger events replayed (audio discarded) before `start_frame` to
    /// reconstruct the voices alive across it.
    pub reconstructed_events: u64,
    /// Typed marker timeline of this render (absolute document frames,
    /// `>= start_frame`; see [`RenderMarker`]) — note starts, transition
    /// starts, arrivals, re-bows, releases, ready for a GUI waveform
    /// overlay. Arrivals here are the SCHEDULE's grid-tick claims — the
    /// INTENDED times.
    pub markers: Vec<RenderMarker>,
    /// Markers EMITTED BY PLAYBACK (absolute document frames): each one is
    /// a voice's real playhead crossing its zone's arrival position at a
    /// real output frame — after every start-offset skip, hold, and rate
    /// scaling. This is the ground truth of when notes were HEARD; diffing
    /// it against [`Self::markers`] (intended) exposes every class of
    /// timing bug at once (wrong pack markers, offset double-counting,
    /// rate errors, scheduling errors).
    pub emitted_markers: Vec<crate::engine::EmittedMarker>,
}

/// Result of one offline document render split into articulation-class
/// output buses (stems), keyed by bus id.
#[derive(Debug, Clone, Default)]
pub struct DocumentBusRenderResult {
    /// Interleaved stereo audio per bus, starting at `start_frame`. With the
    /// default `all → main` routing there is a single `"main"` bus whose
    /// audio is bit-identical to [`render_schedule`]'s.
    pub buses: std::collections::BTreeMap<String, Vec<f32>>,
    pub sample_rate: u32,
    pub start_frame: u64,
    pub seed: u64,
    pub note_count: usize,
    pub transitions: Vec<crate::engine::LegatoFireEvent>,
    pub reactive_fallbacks: u64,
    /// Trigger events replayed (audio discarded) before `start_frame` to
    /// reconstruct the voices alive across it.
    pub reconstructed_events: u64,
    /// Typed marker timeline (see [`DocumentRenderResult::markers`]).
    pub markers: Vec<RenderMarker>,
    /// Playback-emitted markers (see [`DocumentRenderResult::emitted_markers`]).
    pub emitted_markers: Vec<crate::engine::EmittedMarker>,
}

/// Dispatch one scheduled event into the bank — the single translation from
/// [`DocEvent`] to engine calls, shared verbatim by the offline walker
/// (below) and the realtime scheduler ([`crate::document_rt`]). The
/// determinism guarantee between them is exactly this parity.
pub(crate) fn dispatch_event(bank: &mut crate::bank::SamplerBank, id: &str, ev: &ScheduledEvent) {
    let line = ev.line as LineId;
    match ev.kind {
        DocEvent::Cc { cc, val } => {
            bank.cc_instrument_line(id, line, cc, val);
            // Document mode owns the legato mode: a low-latency CC58
            // press (0–5) must not demote the expressive curve the
            // schedule's prefire leads were computed from.
            if cc == 58 && val <= 5 {
                bank.set_legato_mode(id, true, true);
            }
        }
        DocEvent::NoteOn {
            note,
            vel,
            rr,
            lead,
        } => {
            bank.set_forced_rr(id, Some(rr));
            // The pre-roll lead rides along so the engine can hold each
            // spawned voice back to its zone's measured heard-arrival.
            bank.note_on_instrument_line_lead(id, line, note, vel, u64::from(lead));
        }
        DocEvent::NoteOff { note, rr } => {
            bank.set_forced_rr(id, Some(rr));
            bank.note_off_instrument_line(id, line, note);
        }
        DocEvent::LegatoPrefire {
            note,
            vel,
            rr,
            lead,
        } => {
            bank.set_forced_rr(id, Some(rr));
            // The lead (frames to the destination tick) rides along so the
            // engine can align the sample's measured lead-in to it exactly.
            bank.legato_prefire_line_lead(id, line, note, vel, u64::from(lead));
        }
    }
}

/// True for events that spawn/steer voices (counted as reconstruction work
/// when replayed before the start frame).
pub(crate) fn is_trigger(ev: &ScheduledEvent) -> bool {
    matches!(
        ev.kind,
        DocEvent::NoteOn { .. } | DocEvent::NoteOff { .. } | DocEvent::LegatoPrefire { .. }
    )
}

/// Shared schedule walker: resets playback state, dispatches every event at
/// its exact frame (chunks split at event boundaries, so placement is
/// sample-accurate regardless of `block_frames`), pins round-robin per event
/// via the forced-RR path, and harvests the fire log + reactive counter.
/// `render_chunk(bank, frames, discard)` renders exactly `frames` frames;
/// `discard == true` chunks are reconstruction warm-up (advance the engine,
/// throw the audio away).
///
/// **Playback-start invariance** (`opts.start_frame > 0`): events before the
/// activity span containing the start are dispatched state-only (CC
/// pre-roll — their voices are provably dead); from the span start
/// ([`Schedule::reconstruction_start`]) the walk replays normally with
/// discarded audio, so every voice alive across `start_frame` carries the
/// exact accumulated state of the full render. Output then begins at
/// `start_frame` as a bit-exact slice of the full render.
///
/// Events carry their engine mono line from annotation time (the allocator
/// — channel→line identity or auto-divisi ranking — ran in [`annotate`]);
/// each divisi line runs its own prefired legato.
fn walk_schedule(
    bank: &mut crate::bank::SamplerBank,
    id: &str,
    schedule: &Schedule,
    opts: &DocumentRenderOptions,
    mut render_chunk: impl FnMut(&mut crate::bank::SamplerBank, usize, bool),
) -> (
    Vec<crate::engine::LegatoFireEvent>,
    u64,
    u64,
    Vec<crate::engine::EmittedMarker>,
) {
    // Reset playback state; document mode always plays the full expressive
    // legato (the whole point is that latency no longer costs anything).
    // set_legato_mode(.., expressive: true) also flips the engine into
    // PlayMode::Lookahead — the automatic mode policy: Lookahead ONLY while
    // the MIDI is known ahead of time, i.e. for the duration of this walk.
    bank.panic(id);
    bank.set_legato_mode(id, true, true);
    bank.set_legato_fire_log_enabled(id, true);
    bank.set_emitted_marker_log_enabled(id, true);

    let tail_frames = (opts.tail_sec * schedule.sample_rate as f64).round() as u64;
    let end_frame = schedule.end_frame + tail_frames;
    let recon_from = schedule.reconstruction_start(opts.start_frame);
    let mut cursor = recon_from;
    let mut reconstructed_events = 0u64;
    let base_engine_frame = engine_frames_rendered(bank, id);

    let mut render_until =
        |bank: &mut crate::bank::SamplerBank, cursor: &mut u64, target: u64, discard: bool| {
            while *cursor < target {
                let frames = ((target - *cursor) as usize).min(opts.block_frames.max(1));
                render_chunk(bank, frames, discard);
                *cursor += frames as u64;
            }
        };

    for ev in &schedule.events {
        if ev.frame < recon_from {
            // Provably-quiescent past: voices are gone, but controller state
            // (CC1/CC2 dynamics, CC58 keyswitch) persists — pre-roll it.
            if matches!(ev.kind, DocEvent::Cc { .. }) {
                dispatch_event(bank, id, ev);
            }
            continue;
        }
        if ev.frame < opts.start_frame {
            // Reconstruction replay: advance the engine through the span
            // (audio discarded) so voices alive across start_frame carry
            // their exact accumulated state.
            render_until(bank, &mut cursor, ev.frame, true);
            if is_trigger(ev) {
                reconstructed_events += 1;
            }
            dispatch_event(bank, id, ev);
            continue;
        }
        // Audible window: finish any warm-up up to start_frame, then render
        // for real up to the event.
        render_until(bank, &mut cursor, opts.start_frame, true);
        render_until(bank, &mut cursor, ev.frame, false);
        dispatch_event(bank, id, ev);
    }
    // Flush any remaining warm-up (start_frame past the last event)…
    render_until(bank, &mut cursor, opts.start_frame.min(end_frame), true);
    // …then render the audible window.
    render_until(bank, &mut cursor, end_frame, false);
    bank.set_forced_rr(id, None);

    let reactive_fallbacks = bank.reactive_legato_fires(id);
    // Fire log frames are engine-relative; re-base to document frames (the
    // engine started rendering at `recon_from`) and report only fires inside
    // the audible window.
    let transitions = bank
        .legato_fire_log(id)
        .into_iter()
        .map(|mut e| {
            e.arrival = (e.arrival - e.frame) + (e.frame - base_engine_frame) + recon_from;
            e.frame = (e.frame - base_engine_frame) + recon_from;
            e
        })
        .filter(|e| e.frame >= opts.start_frame)
        .collect();
    bank.set_legato_fire_log_enabled(id, false);
    // Playback-emitted markers, re-based to document frames the same way.
    let emitted: Vec<crate::engine::EmittedMarker> = bank
        .emitted_markers(id)
        .into_iter()
        .map(|mut m| {
            m.frame = (m.frame - base_engine_frame) + recon_from;
            m
        })
        .filter(|m| m.frame >= opts.start_frame)
        .collect();
    bank.set_emitted_marker_log_enabled(id, false);
    // The document has been played out — live dispatch resumes under the
    // strict zero-latency policy (see PlayMode).
    bank.set_play_mode(id, crate::engine::PlayMode::StrictLive);
    (transitions, reactive_fallbacks, reconstructed_events, emitted)
}

/// Walk a [`Schedule`] through a bank instrument, rendering block-sized
/// chunks that split at every event's exact frame — events therefore land
/// sample-accurately regardless of `block_frames`. Round-robin is pinned per
/// event via the forced-RR path; the engine never consults a mutable counter.
pub fn render_schedule(
    bank: &mut crate::bank::SamplerBank,
    id: &str,
    schedule: &Schedule,
    opts: &DocumentRenderOptions,
) -> DocumentRenderResult {
    let mut audio: Vec<f32> = Vec::new();
    let mut buf: Vec<f32> = Vec::new();
    let (transitions, reactive_fallbacks, reconstructed_events, emitted_markers) =
        walk_schedule(bank, id, schedule, opts, |bank, frames, discard| {
            buf.clear();
            buf.resize(frames * 2, 0.0);
            bank.render(&mut buf);
            if !discard {
                audio.extend_from_slice(&buf);
            }
        });

    DocumentRenderResult {
        audio,
        sample_rate: schedule.sample_rate,
        start_frame: opts.start_frame,
        seed: schedule.seed,
        note_count: schedule.note_count,
        transitions,
        reactive_fallbacks,
        reconstructed_events,
        markers: schedule
            .markers
            .iter()
            .filter(|m| m.frame >= opts.start_frame)
            .copied()
            .collect(),
        emitted_markers,
    }
}

/// [`render_schedule`], split into articulation-class output buses per
/// `routing` (class → bus id). Buses are rendered in ONE pass with the same
/// voice iteration order as the plain render, so:
/// - default routing (every class → `"main"`) is bit-identical to
///   [`render_schedule`], and
/// - with split routing, each voice lands wholly in its class's bus and the
///   buses sum back to the main render.
///
/// Unlike [`render_schedule`] (which renders the whole bank mix), this
/// renders instrument `id` alone — identical audio when it is the only
/// loaded instrument, which is the document-render arrangement.
pub fn render_schedule_buses(
    bank: &mut crate::bank::SamplerBank,
    id: &str,
    schedule: &Schedule,
    opts: &DocumentRenderOptions,
    routing: &std::collections::BTreeMap<ArticClass, String>,
) -> DocumentBusRenderResult {
    let bus_of = |class: ArticClass| -> String {
        routing
            .get(&class)
            .cloned()
            .unwrap_or_else(|| "main".to_string())
    };
    let longs_bus = bus_of(ArticClass::Longs);
    let shorts_bus = bus_of(ArticClass::Shorts);
    let mut names: Vec<String> = vec![longs_bus.clone(), shorts_bus.clone()];
    names.sort();
    names.dedup();
    let route_longs = names.iter().position(|n| *n == longs_bus).unwrap_or(0);
    let route_shorts = names.iter().position(|n| *n == shorts_bus).unwrap_or(0);

    let mut buses: Vec<Vec<f32>> = names.iter().map(|_| Vec::new()).collect();
    let mut chunk: Vec<Vec<f32>> = names.iter().map(|_| Vec::new()).collect();
    let (transitions, reactive_fallbacks, reconstructed_events, emitted_markers) =
        walk_schedule(bank, id, schedule, opts, |bank, frames, discard| {
            for c in chunk.iter_mut() {
                c.clear();
                c.resize(frames * 2, 0.0);
            }
            bank.render_instrument_routed_buses(id, &mut chunk, route_longs, route_shorts);
            if !discard {
                for (bus, c) in buses.iter_mut().zip(chunk.iter()) {
                    bus.extend_from_slice(c);
                }
            }
        });

    DocumentBusRenderResult {
        buses: names.into_iter().zip(buses).collect(),
        sample_rate: schedule.sample_rate,
        start_frame: opts.start_frame,
        seed: schedule.seed,
        note_count: schedule.note_count,
        transitions,
        reactive_fallbacks,
        reconstructed_events,
        markers: schedule
            .markers
            .iter()
            .filter(|m| m.frame >= opts.start_frame)
            .copied()
            .collect(),
        emitted_markers,
    }
}

fn engine_frames_rendered(bank: &crate::bank::SamplerBank, id: &str) -> u64 {
    bank.engine_frames_rendered(id).unwrap_or(0)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn spec_with_legato() -> LibrarySpec {
        LibrarySpec::from_styx(
            r#"
name DocTest
articulations (
    { id Sus,  label Sustain,  kind @Sustain, rr 1 }
    { id Leg,  label Legato,   kind @Legato,  rr 4, directional true }
    { id Stac, label Staccato, kind @Short,   rr 4 }
)
legato_engine {
    expressive {
        zones (
            {vel_range (0 64),    label slow, delay_ms 333}
            {vel_range (65 127),  label fast, delay_ms 100}
        )
    }
    portamento { trigger_vel_max 10, volume_controller CC5 }
}
short_note_timing { pre_delay_ms 60 }
keyswitch {
    cc58_map {
        0-5   "Sustain: Low Latency Legato"
        21-25 Staccato
    }
}
"#,
        )
        .expect("parse test spec")
    }

    fn note(start_qn: f64, end_qn: f64, pitch: u8, vel: u8) -> DocNote {
        DocNote {
            start_qn,
            end_qn,
            chan: 0,
            pitch,
            vel,
        }
    }

    const SR: u32 = 48_000;

    #[test]
    fn qn_to_sec_integrates_tempo_map() {
        let tempo = vec![
            TempoPoint {
                qn: 0.0,
                bpm: 120.0,
            },
            TempoPoint { qn: 4.0, bpm: 60.0 },
        ];
        assert!((qn_to_sec(&tempo, 0.0) - 0.0).abs() < 1e-12);
        assert!((qn_to_sec(&tempo, 4.0) - 2.0).abs() < 1e-12);
        assert!((qn_to_sec(&tempo, 6.0) - 4.0).abs() < 1e-12);
        // empty map = 120 BPM
        assert!((qn_to_sec(&[], 2.0) - 1.0).abs() < 1e-12);
    }

    /// Golden values — this hash is FROZEN (rendered projects must reproduce
    /// forever). If this test fails, the hash function changed: revert it.
    #[test]
    fn stable_rr_slot_is_frozen() {
        let a = stable_rr_slot(42, 1.5, 62, 0, RrPurpose::Body);
        let b = stable_rr_slot(42, 1.5, 62, 0, RrPurpose::Transition);
        let c = stable_rr_slot(43, 1.5, 62, 0, RrPurpose::Body);
        // Distinct across purpose and seed…
        assert_ne!(a, b);
        assert_ne!(a, c);
        // …and identical across calls / positions (pure function).
        assert_eq!(a, stable_rr_slot(42, 1.5, 62, 0, RrPurpose::Body));
        // Frozen golden values (computed once at introduction).
        assert_eq!(a, 3847);
        assert_eq!(b, 3851);
        assert_eq!(c, 488);
    }

    #[test]
    fn legato_followed_note_becomes_prefire_with_expressive_lead() {
        let doc = TrackDocument {
            seed: 7,
            notes: vec![note(0.0, 2.1, 60, 90), note(2.0, 4.0, 62, 30)],
            ..Default::default()
        };
        let sched = annotate(&doc, &spec_with_legato(), SR);
        assert_eq!(sched.legato_count, 1);

        // 120 BPM ⇒ QN 2.0 = 1.0 s = 48000 frames. The 2nd note is soft
        // (vel 30) ⇒ the fixture's slow legato zone = 333 ms. The document
        // PREFIRES by exactly that fixed velocity-zone delay so the arrival
        // lands ON the tick; the 1.0 s IOI leaves ample room (no clamp).
        let tick = 48_000i64;
        let lead_frames = ms_to_frames_i64(333, SR) as u64;
        let prefire = sched
            .events
            .iter()
            .find(|e| matches!(e.kind, DocEvent::LegatoPrefire { .. }))
            .expect("second note arrives via prefire");
        assert_eq!(prefire.frame as i64, tick - lead_frames as i64);
        assert!(matches!(
            prefire.kind,
            DocEvent::LegatoPrefire { lead, .. } if lead == lead_frames as u32
        ));

        // The first note is a plain note-on at frame 0; only one NoteOn total.
        let note_ons: Vec<_> = sched
            .events
            .iter()
            .filter(|e| matches!(e.kind, DocEvent::NoteOn { .. }))
            .collect();
        assert_eq!(note_ons.len(), 1);
        assert_eq!(note_ons[0].frame, 0);
    }

    // r[verify signal.sampling.articulation.select]
    #[test]
    fn latched_cc_selector_drives_offline_schedule() {
        // A `selector uacc` pack: articulation identity rides CC32 codes in
        // the document. The scheduler must (a) forward the CC32 events so
        // the engine latches at playback, and (b) derive per-note timing
        // (short pre-roll) from the CODE-selected articulation's kind.
        let spec = LibrarySpec::from_styx(
            r#"
name UaccTest
selector uacc
articulations (
    { id Sus,   label Sustain,  kind @Sustain, rr 1 }
    { id Spicc, label Spiccato, kind @Short,   rr 1 }
)
short_note_timing { pre_delay_ms 60 }
"#,
        )
        .expect("parse uacc test spec");
        assert!(spec.latched_cc_selector().is_some());

        let cc = |qn: f64, val: u8| DocCc {
            qn,
            chan: 0,
            cc: 32,
            val,
        };
        let doc = TrackDocument {
            seed: 7,
            // Long (1) → sustain note at qn 0; Very Short (42) → short note
            // at qn 2 (120 BPM: qn 2.0 = 48000 frames at 48 kHz).
            ccs: vec![cc(0.0, 1), cc(1.5, 42)],
            notes: vec![note(0.0, 1.0, 60, 90), note(2.0, 2.5, 62, 90)],
            ..Default::default()
        };
        let sched = annotate(&doc, &spec, SR);

        // Both CC32 events survive into the schedule (the engine's latched
        // selector applies them at playback).
        let cc32: Vec<_> = sched
            .events
            .iter()
            .filter_map(|e| match e.kind {
                DocEvent::Cc { cc: 32, val } => Some(val),
                _ => None,
            })
            .collect();
        assert_eq!(cc32, vec![1, 42]);

        // The code-42 note is SHORT: pre-rolled by pre_delay_ms (60 ms =
        // 2880 frames at 48 kHz) ahead of its qn-2.0 tick.
        assert_eq!(sched.short_count, 1);
        let mut ons: Vec<u64> = sched
            .events
            .iter()
            .filter(|e| matches!(e.kind, DocEvent::NoteOn { .. }))
            .map(|e| e.frame)
            .collect();
        ons.sort_unstable();
        assert_eq!(ons, vec![0, 48_000 - 2_880]);

        // Same document WITHOUT the selector: CC32 means nothing, both
        // notes schedule as sustains on their ticks (defaults untouched).
        let mut plain = spec.clone();
        plain.selector = String::new();
        let sched2 = annotate(&doc, &plain, SR);
        assert_eq!(sched2.short_count, 0);
        let mut ons2: Vec<u64> = sched2
            .events
            .iter()
            .filter(|e| matches!(e.kind, DocEvent::NoteOn { .. }))
            .map(|e| e.frame)
            .collect();
        ons2.sort_unstable();
        assert_eq!(ons2, vec![0, 48_000]);
    }

    #[test]
    fn rebow_drops_source_note_off_and_prefires_target() {
        let doc = TrackDocument {
            seed: 7,
            // Same pitch, abutting within 1/32 QN ⇒ re-bow.
            notes: vec![note(0.0, 2.0, 60, 90), note(2.01, 4.0, 60, 90)],
            ..Default::default()
        };
        let sched = annotate(&doc, &spec_with_legato(), SR);
        assert_eq!(sched.legato_count, 1, "re-bow target arrives via prefire");
        let offs: Vec<_> = sched
            .events
            .iter()
            .filter(|e| matches!(e.kind, DocEvent::NoteOff { .. }))
            .collect();
        assert_eq!(offs.len(), 1, "re-bow source's note-off is dropped");
    }

    #[test]
    fn shorts_preroll_and_do_not_prefire() {
        let doc = TrackDocument {
            seed: 7,
            // CC58 = 23 (Staccato band) before both notes.
            ccs: vec![DocCc {
                qn: 0.0,
                chan: 0,
                cc: 58,
                val: 23,
            }],
            // Overlapping shorts must NOT read as legato.
            notes: vec![note(1.0, 2.1, 60, 90), note(2.0, 3.0, 62, 90)],
            ..Default::default()
        };
        let sched = annotate(&doc, &spec_with_legato(), SR);
        assert_eq!(sched.short_count, 2);
        assert_eq!(sched.legato_count, 0);
        // QN 1.0 = 24000 frames, minus 60 ms (2880 frames) pre-roll.
        let first_on = sched
            .events
            .iter()
            .find(|e| matches!(e.kind, DocEvent::NoteOn { .. }))
            .expect("note-on");
        assert_eq!(first_on.frame, 24_000 - 2_880);
    }

    #[test]
    fn same_pitch_break_beyond_gap_is_not_rebow() {
        let doc = TrackDocument {
            seed: 7,
            notes: vec![note(0.0, 1.0, 60, 90), note(1.5, 2.0, 60, 90)],
            ..Default::default()
        };
        let sched = annotate(&doc, &spec_with_legato(), SR);
        assert_eq!(sched.legato_count, 0);
    }

    /// The document prefire lead is the FIXED velocity-zone delay for the
    /// resolved legato mode (CSS: Expressive 333/250/100, Low-Latency 150/100),
    /// NOT an IOI-driven Overlap-Delay curve and NOT per-zone measurement. The
    /// arrival lands exactly on the tick (`frame + lead == tick`). A passage
    /// faster than its own delay clamps to fire-on-tick (the room clamp).
    // r[impl signal.soundsource.legato.offline]
    // r[impl signal.soundsource.mode.offline-lookahead]
    #[test]
    fn prefire_lead_is_fixed_velocity_zone_delay() {
        // Spec with BOTH modes: LL 150/100, EX 333/250 (split at vel 64).
        let spec = LibrarySpec::from_styx(
            r#"
name ModeTest
articulations (
    { id Sus, label Sustain, kind @Sustain, rr 1 }
    { id Leg, label Legato,  kind @Legato,  rr 4, directional true }
)
legato_engine {
    expressive  { enabled_cc58_range 6-10, zones ({vel_range (0 64), label slow, delay_ms 333}{vel_range (65 127), label fast, delay_ms 250}) }
    low_latency { enabled_cc58_range 0-5,  zones ({vel_range (0 64), label slow, delay_ms 150}{vel_range (65 127), label fast, delay_ms 100}) }
}
"#,
        )
        .expect("parse mode spec");

        // Generous spacing (2 QN = 1000 ms IOI) so the room clamp never binds.
        let lead_of = |ccs: Vec<DocCc>, vel: u8| -> (u64, u32) {
            let doc = TrackDocument {
                seed: 1,
                notes: vec![note(0.0, 2.1, 60, vel), note(2.0, 4.0, 62, vel)],
                ccs,
                ..Default::default()
            };
            let sched = annotate(&doc, &spec, SR);
            sched
                .events
                .iter()
                .find_map(|e| match e.kind {
                    DocEvent::LegatoPrefire { lead, .. } => Some((e.frame, lead)),
                    _ => None,
                })
                .expect("prefire")
        };
        let tick = qn_to_frame(&[], 2.0, SR) as u64;
        let cc58 = |v: u8| vec![DocCc { qn: 0.0, chan: 0, cc: 58, val: v }];

        // DOCUMENT DEFAULT (no CC58) = Expressive: vel 30 → 333, vel 90 → 250.
        // Velocity — not a keyswitch — drives the slow/medium/fast zone.
        let (f, l) = lead_of(vec![], 30);
        assert_eq!(l, ms_to_frames_i64(333, SR) as u32, "EX slow = 333 ms (document default)");
        assert_eq!(f + l as u64, tick, "arrival on the tick");
        assert_eq!(lead_of(vec![], 90).1, ms_to_frames_i64(250, SR) as u32, "EX medium = 250 ms");
        // An Expressive keyswitch (CC58 8) is a no-op vs the default.
        assert_eq!(lead_of(cc58(8), 30).1, ms_to_frames_i64(333, SR) as u32, "EX keyswitch = still 333");

        // An explicit Low-Latency keyswitch (CC58 2, in 0-5) overrides to the
        // Low-Latency table: vel 30 → 150, vel 90 → 100.
        assert_eq!(lead_of(cc58(2), 30).1, ms_to_frames_i64(150, SR) as u32, "LL keyswitch soft = 150 ms");
        assert_eq!(lead_of(cc58(2), 90).1, ms_to_frames_i64(100, SR) as u32, "LL keyswitch loud = 100 ms");

        // Fast passage (0.1 QN = 50 ms IOI, far below the 333 ms delay) → the
        // room clamp forces fire-on-tick.
        let doc_fast = TrackDocument {
            seed: 1,
            notes: vec![note(0.0, 0.12, 60, 30), note(0.1, 1.0, 62, 30)],
            ccs: vec![],
            ..Default::default()
        };
        let pf_fast = annotate(&doc_fast, &spec, SR)
            .events
            .iter()
            .find_map(|e| match e.kind {
                DocEvent::LegatoPrefire { lead, .. } => Some(lead),
                _ => None,
            })
            .expect("prefire");
        assert_eq!(pf_fast, 0, "fast passage clamps to fire-on-tick");
    }

    /// A pack can author its own Overlap-Delay curves; they drive the LIVE /
    /// reactive path (`overlap_delay_ms`, the KSP `wait($b0n3s)` before a
    /// real-time transition fires), NOT the document schedule lead — that is
    /// the fixed velocity-zone delay. This asserts the authored curve is
    /// honored at the spec level.
    // r[impl signal.soundsource.declarative]
    #[test]
    fn custom_overlap_delay_curve_is_honored() {
        let mut spec = spec_with_legato();
        let flat = |ms: f32| crate::spec::IoiCurveSpec {
            thresholds_ms: vec![10.0, 2000.0],
            anchors_ms: vec![ms, ms],
        };
        spec.legato_engine.as_mut().unwrap().overlap_delay =
            Some(crate::spec::OverlapDelaySpec {
                low_latency: crate::spec::OverlapDelayCurveSpec {
                    soft: flat(50.0),
                    loud: flat(0.0),
                },
                expressive: crate::spec::OverlapDelayCurveSpec {
                    soft: flat(90.0),
                    loud: flat(0.0),
                },
            });
        let cfg = spec.legato_cfg();
        // Low-latency soft (vel 30) → authored 50 ms; loud (vel 90) → 0 ms.
        assert_eq!(cfg.overlap_delay_ms(80.0, 30, false), 50);
        assert_eq!(cfg.overlap_delay_ms(80.0, 90, false), 0);
        // Expressive soft → authored 90 ms.
        assert_eq!(cfg.overlap_delay_ms(80.0, 30, true), 90);
    }

    /// A MEASURED transition zone does NOT change the schedule prefire lead:
    /// the KSP has no per-sample arrival, so the document fires by the fixed
    /// velocity-zone delay regardless of the recorded `lead_in_ms`/`arrival_ms`.
    /// (The measurement is consumed engine-side as the `$1fvjk` sample-start
    /// skip in `spawn_transition_voice`, not as the schedule lead.)
    // r[impl signal.soundsource.legato.offline]
    #[test]
    fn measured_zone_does_not_change_prefire_lead() {
        let mut spec = spec_with_legato();
        // One measured up-transition C4→D4 with a 300 ms lead-in.
        spec.zones.push(crate::spec::ZoneSpec {
            direction: "up".into(),
            interval: 2,
            lead_in_ms: 300.0,
            arrival_ms: 0.0,
            root_key: 60,
            key_min: 60,
            key_max: 60,
            articulation: "Leg".into(),
            ..crate::spec::ZoneSpec {
                file: "leg_up_C4_2.wav".into(),
                key_min: 0,
                key_max: 0,
                root_key: 0,
                vel_min: 0,
                vel_max: 127,
                rr_index: 0,
                rr_mode: String::new(),
                gain_db: 0.0,
                pan: 0.0,
                tune_cents: 0.0,
                sample_start: 0,
                sample_end: 0,
                loop_start: 0,
                loop_end: 0,
                loop_xfade: 0,
                fade_in: 0,
                release_start: 0,
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
                arrival_ms: 0.0,
                group: String::new(),
                group_polyphony: 0,
                choke_group: String::new(),
                off_by: vec![],
                section: String::new(),
                variant: String::new(),
            }
        });
        // 1 QN apart @120 BPM → IOI 500 ms → default $1fvjk curve = 117 ms.
        let doc = TrackDocument {
            seed: 1,
            notes: vec![note(0.0, 1.05, 60, 90), note(1.0, 2.0, 62, 90)],
            ..Default::default()
        };
        let sched = annotate(&doc, &spec, SR);
        let lead = sched
            .events
            .iter()
            .find_map(|e| match e.kind {
                DocEvent::LegatoPrefire { lead, .. } => Some(lead),
                _ => None,
            })
            .expect("prefire");
        // vel 90 → the fixture's fast zone (65-127) = 100 ms, NOT the recorded
        // 300 ms lead-in. The measurement is irrelevant to the schedule; the
        // fixed velocity-zone delay wins. (IOI 500 ms → room 325 ms; 100 < 325,
        // so the room clamp doesn't bind.)
        assert_eq!(
            lead,
            ms_to_frames_i64(100, SR) as u32,
            "schedule lead = fixed velocity-zone delay, independent of measurement"
        );

        // Authoring a start-offset curve must NOT move the schedule lead either
        // — `$1fvjk` is now applied engine-side (the sample-start skip), not in
        // the annotate lead.
        spec.legato_engine.as_mut().unwrap().start_offset = Some(crate::spec::IoiCurveSpec {
            thresholds_ms: vec![10.0, 2000.0],
            anchors_ms: vec![250.0, 250.0],
        });
        let sched2 = annotate(&doc, &spec, SR);
        let lead2 = sched2
            .events
            .iter()
            .find_map(|e| match e.kind {
                DocEvent::LegatoPrefire { lead, .. } => Some(lead),
                _ => None,
            })
            .expect("prefire");
        assert_eq!(
            lead2,
            ms_to_frames_i64(100, SR) as u32,
            "start-offset authoring does not change the schedule lead"
        );
    }

    #[test]
    fn annotation_is_deterministic() {
        let doc = TrackDocument {
            seed: 99,
            notes: vec![
                note(0.0, 2.1, 60, 90),
                note(2.0, 4.0, 62, 30),
                note(4.0, 5.0, 64, 110),
            ],
            ccs: vec![DocCc {
                qn: 3.9,
                chan: 0,
                cc: 1,
                val: 80,
            }],
            ..Default::default()
        };
        let spec = spec_with_legato();
        let a = annotate(&doc, &spec, SR);
        let b = annotate(&doc, &spec, SR);
        assert_eq!(a.events, b.events);
    }
}
