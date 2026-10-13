//! Adapter between the expression editor's document and `tune_dsp`'s
//! analyzed pitch model.
//!
//! The audio counterpart of `expression-editor-daw`: that one converts
//! a MIDI take, this converts an analysis, and both produce the same
//! [`ExpressionDoc`]. That is the entire reason one editor can be both
//! an MPE editor and a Melodyne competitor — the surface never learns
//! which domain it is serving.
//!
//! ## Where the two models line up, and where they do not
//!
//! They agree on the important thing. `tune_dsp::NoteBlob` stores
//! `center + drift·amount + modulation·amount`; the editor stores an
//! integer row plus a curve of semitones relative to it. Those are the
//! same decomposition with the origin in a different place, so the
//! conversion is arithmetic rather than interpretation.
//!
//! They disagree on three things, each handled once here:
//!
//! - **Time.** A blob is indexed by analysis *frame*; the document is
//!   in `TimeBase::Frames`, so `t` and frame index are the same number.
//!   Keeping them identical is deliberate — a scaling factor between
//!   them would have to be undone at every edit.
//! - **Pitch origin.** A blob's `center_midi` is absolute and
//!   fractional. The document splits that into a rounded integer `row`
//!   plus the fractional remainder folded into the curve, so a note sung
//!   30 cents flat sits on its literal row with the flatness in the
//!   curve — which is also how microtonal MIDI is drawn.
//! - **Per-note trims.** `formant_shift` and `gain_db` have no MIDI
//!   equivalent, so they ride in the document's `Timbre` and `Pressure`
//!   lanes as flat curves. They are round-tripped exactly rather than
//!   re-derived, because a trim the user set is not something to infer.
//!
//! [`to_doc`] and [`apply_to`] are pure functions over snapshots, so
//! the whole path is testable with no audio, no DSP and no UI.

use expression_editor_core::doc::{Dimension, ExpressionDoc, Note, NoteId, TimeBase};
use tune_dsp::model::{NoteBlob, PitchDoc, WarpMarker};

pub mod align;
pub mod align_hits;
pub mod analyze;
#[cfg(feature = "daw")]
pub mod apply_quantize;
pub mod detect;
pub mod dynamics;
pub mod frames;
pub mod gate;
pub mod group_detect;
pub mod lanes;
pub mod onsets;
pub mod panel_bridge;
pub mod percussive;
pub mod quantize;
#[cfg(feature = "daw")]
pub mod retime;
#[cfg(feature = "daw")]
pub mod session;
#[cfg(feature = "daw")]
pub mod slip;
pub mod spans;
#[cfg(feature = "daw")]
pub mod stretch;
#[cfg(feature = "daw")]
pub mod write_dynamics;

pub use align::{AlignConfig, Alignment, Anchor, AnchorKind, Material, align, align_to_audio};
pub use align_hits::{HitAlignConfig, align_hits, pair_hits};
pub use analyze::{Analysis, TakeConfig, analyze_take, to_mono};
#[cfg(feature = "daw")]
pub use apply_quantize::{Applied, GroupError, apply_split, group_start};
pub use detect::{DetectConfig, Filters, Scale, Transient, transients};
pub use dynamics::{Detection, Dynamics, DynamicsConfig, GainPoint, Region};
pub use frames::{FrameFeature, frame_features};
pub use gate::{GateConfig, Hit};
pub use lanes::{DynamicsLane, Lanes};
pub use onsets::{Onset, OnsetConfig, detect as detect_onsets};
pub use panel_bridge::{HitPreview, PanelDetect, PanelTarget, PanelWrite};
pub use percussive::{Percussion, PercussiveConfig, analyze_percussive, looks_percussive};
pub use quantize::{Move, Piece, Plan, QuantizeConfig, SplitConfig, plan};
#[cfg(feature = "daw")]
pub use retime::{TakePlacement, stretch_markers};
#[cfg(feature = "daw")]
pub use session::{AudioSession, AudioTakeLocation, WriteError, WriteOutcome};
pub use spans::{Span, unvoiced_spans};
#[cfg(feature = "daw")]
pub use write_dynamics::DynamicsWritten;

/// How a per-note trim is stored in a dimension.
///
/// The lanes are 0..1 normalized, and both trims are signed, so each
/// needs a range and a centre. These are display ranges, not limits on
/// what the DSP will accept — they only decide how far a drag travels.
pub const FORMANT_RANGE: f64 = 12.0;
/// Gain trim range in dB, ± this value.
pub const GAIN_RANGE_DB: f64 = 24.0;

/// Samples used to find a note's pitch centre on write-back. Dense
/// enough that a vibrato cycle cannot bias the median, cheap enough to
/// run for every note on every write.
const CENTER_SAMPLES: usize = 128;

/// The decomposition's drift/vibrato split is a *frequency* split, so
/// it needs to know how document time maps to seconds. In
/// `TimeBase::Frames` that is exact and this value is unused; it is
/// only a fallback for a tick-based document, which the audio path does
/// not produce.
const REFERENCE_BPM: f64 = 120.0;

/// The formant trim a note carries, in semitones.
pub fn formant_of(note: &Note, t: f64) -> f64 {
    from_lane(
        note.timbre.sample(t, Dimension::Timbre.default_value()),
        FORMANT_RANGE,
    )
}

/// The gain trim a note carries, in dB.
pub fn gain_of(note: &Note, t: f64) -> f64 {
    from_lane(
        note.pressure.sample(t, Dimension::Pressure.default_value()),
        GAIN_RANGE_DB,
    )
}

/// Encode a signed value into a 0..1 dimension position.
fn to_lane(v: f64, range: f64) -> f64 {
    (0.5 + v / (range * 2.0)).clamp(0.0, 1.0)
}

/// Inverse of [`to_lane`].
fn from_lane(v: f64, range: f64) -> f64 {
    (v - 0.5) * range * 2.0
}

/// Convert an analysis into an editable document, with waveform.
///
/// `rms` is the per-frame level of the whole take, indexed by the same
/// frame numbers the blobs use. Each note takes the slice covering it,
/// normalized against the loudest frame in the take rather than its own
/// peak — normalizing per note would make a whispered word draw as
/// large as a belted one, which is exactly the comparison the display
/// exists to support.
pub fn to_doc_with_envelope(pitch: &PitchDoc, frame_rate: f64, rms: &[f32]) -> ExpressionDoc {
    to_doc_with_audio(pitch, frame_rate, rms, &[])
}

/// The full audio conversion: notes, waveforms and unvoiced spans.
///
/// `f0` is the per-frame detected fundamental, `None` where there was
/// none. Those gaps become the document's unvoiced spans, which is what
/// lets the surface break the pitch track across a consonant instead of
/// drawing a line through it, and what gives sibilant editing something
/// to aim at.
pub fn to_doc_with_audio(
    pitch: &PitchDoc,
    frame_rate: f64,
    rms: &[f32],
    f0: &[Option<f64>],
) -> ExpressionDoc {
    let mut doc = to_doc(pitch, frame_rate);
    let peak = rms.iter().copied().fold(0.0_f32, f32::max);
    if peak > 0.0 {
        // The take's own waveform, for the backdrop.
        doc.peaks = rms.iter().map(|v| v / peak).collect();
        for (i, blob) in pitch.blobs.iter().enumerate() {
            let Some(note) = doc.note_mut(NoteId(i as u64 + 1)) else {
                continue;
            };
            let lo = blob.start_frame.min(rms.len());
            let hi = (blob.end_frame + 1).min(rms.len());
            if hi > lo {
                note.envelope = rms[lo..hi].iter().map(|v| v / peak).collect();
            }
        }
    }
    // Frame index and document time are the same number here, so the
    // spans need no conversion — see the module docs.
    doc.unvoiced = spans::unvoiced_spans(f0)
        .into_iter()
        .map(|s| (s.start as f64, s.end as f64))
        .collect();
    doc
}

/// Convert an analysis into an editable document.
///
/// `frame_rate` is `sample_rate / hop` — the rate the blobs' frame
/// indices tick at.
pub fn to_doc(pitch: &PitchDoc, frame_rate: f64) -> ExpressionDoc {
    let end = pitch
        .blobs
        .iter()
        .map(|b| b.end_frame as f64)
        .fold(0.0_f64, f64::max)
        .max(frame_rate);
    let mut doc = ExpressionDoc::new(TimeBase::Frames { frame_rate }, 0.0, end);

    for (i, blob) in pitch.blobs.iter().enumerate() {
        doc.push(blob_to_note(NoteId(i as u64 + 1), blob));
    }
    doc
}

/// One blob as an editor note.
pub fn blob_to_note(id: NoteId, blob: &NoteBlob) -> Note {
    let start = blob.start_frame as f64;
    let end = blob.end_frame as f64;
    // The row is the *rounded* centre and the remainder goes into the
    // curve. A sung note 30 cents flat then draws on its literal row
    // with the flatness visible as curve offset, rather than sitting
    // between two rows where nothing lines up.
    let row = blob.center_midi.round();
    let offset = blob.center_midi - row;

    let mut note = Note::new(id, start, end.max(start + 1.0), row as i32);
    note.channel = None;
    note.weight = blob.rms;

    // The sounding contour, sampled per frame: exactly what
    // `NoteBlob::target_midi` reports, expressed relative to the row.
    for frame in blob.start_frame..=blob.end_frame {
        let Some(midi) = blob.target_midi(frame) else {
            continue;
        };
        note.pitch.set(frame as f64, midi - row);
    }
    // A blob with no frames still has a centre, and a note with an
    // empty curve would read as "exactly on the row" rather than as
    // however flat it was sung.
    if note.pitch.is_empty() {
        note.pitch.set(start, offset);
        note.pitch.set(end, offset);
    }

    set_flat(
        &mut note,
        Dimension::Timbre,
        to_lane(blob.formant_shift, FORMANT_RANGE),
    );
    set_flat(
        &mut note,
        Dimension::Pressure,
        to_lane(blob.gain_db, GAIN_RANGE_DB),
    );
    note
}

/// Hold a dimension at one value across the note.
///
/// Two points rather than one: the curve holds its endpoint value
/// outside the authored range, but a single point gives the UI nothing
/// to draw a span from.
fn set_flat(note: &mut Note, dimension: Dimension, value: f64) {
    let (s, e) = (note.start, note.end);
    let curve = note.curve_mut(dimension);
    curve.set(s, value);
    curve.set(e, value);
}

/// Write a document's edits back onto the analysis.
///
/// Only the fields the editor owns are touched. `drift` and
/// `modulation` — what was actually *sung* — are never rewritten, so a
/// re-analysis can replace them without discarding the user's edits.
/// That is the same reason `NoteBlob` keeps `analyzed_center_midi`.
///
/// Returns how many blobs matched a note. Notes with no counterpart are
/// skipped rather than appended: creating audio for a note that was
/// never sung is not something this adapter can do.
pub fn apply_to(doc: &ExpressionDoc, pitch: &mut PitchDoc) -> usize {
    let mut applied = 0;
    for (i, blob) in pitch.blobs.iter_mut().enumerate() {
        let Some(note) = doc.note(NoteId(i as u64 + 1)) else {
            continue;
        };
        // The centre is *not* the curve at the midpoint: that reading
        // includes whatever drift and vibrato are passing through at
        // the time, so a note with a scoop would write back a centre
        // pulled toward the scoop. It is the median of the contour —
        // the same "where the curve actually dwells" the core uses for
        // zone scaling, so the surface and the write-back agree.
        let default = Dimension::Pitch.default_value();
        let center = expression_editor_core::blob::decompose(
            &note.pitch,
            note.start,
            note.end,
            CENTER_SAMPLES,
            doc.time_base.units_per_second(REFERENCE_BPM),
            default,
        )
        .center;
        blob.center_midi = note.row as f64 + center;

        let mid = (note.start + note.end) * 0.5;
        blob.formant_shift = from_lane(
            note.timbre.sample(mid, Dimension::Timbre.default_value()),
            FORMANT_RANGE,
        );
        blob.gain_db = from_lane(
            note.pressure
                .sample(mid, Dimension::Pressure.default_value()),
            GAIN_RANGE_DB,
        );
        applied += 1;
    }
    applied
}

/// Derive warp markers from where the notes have been moved to.
///
/// Timing edits are expressed as note moves and resizes on the
/// document, deliberately: the same gesture then works on a MIDI take,
/// where there is nothing to warp and moving the notes *is* the whole
/// edit. The audio domain gets its warp by comparing where a note now
/// sits against where its blob was analyzed.
///
/// One marker per note edge, so a stretched note maps its whole
/// interior linearly and the material between notes takes up the slack.
/// `WarpMarker::d_time` is the offset from the analyzed position, which
/// is what `render_world_warped` reads.
pub fn warp_markers(doc: &ExpressionDoc, pitch: &PitchDoc) -> Vec<WarpMarker> {
    let hop = pitch.hop.max(1) as f64;
    let mut out = Vec::new();
    for (i, blob) in pitch.blobs.iter().enumerate() {
        let Some(note) = doc.note(NoteId(i as u64 + 1)) else {
            continue;
        };
        for (analyzed_frame, now) in [
            (blob.start_frame as f64, note.start),
            (blob.end_frame as f64, note.end),
        ] {
            let d_frames = now - analyzed_frame;
            // Markers are sample-anchored, and frame index times the
            // hop is the sample the frame began at.
            out.push(WarpMarker {
                sample: analyzed_frame * hop,
                d_time: d_frames * hop,
                pitch_bend: 0.0,
            });
        }
    }
    // Sorted and deduplicated: two notes that abut share an edge, and a
    // marker list with a repeated sample has an undefined slope there.
    out.sort_by(|a, b| a.sample.total_cmp(&b.sample));
    out.dedup_by(|a, b| (a.sample - b.sample).abs() < 1e-9);
    out
}

/// Push the drift and vibrato blend back onto the analysis.
///
/// Separate from [`apply_to`] because these two are the *only* fields
/// where the editor's curve and the blob's decomposition can disagree:
/// the editor may have redrawn the contour freehand, at which point
/// scaling the analyzed drift is no longer what is on screen. Callers
/// that have only moved the sliders want this; callers that have drawn
/// want [`apply_to`] alone.
pub fn apply_blend(blob: &mut NoteBlob, drift_amount: f64, modulation_amount: f64) {
    blob.drift_amount = drift_amount.max(0.0);
    blob.modulation_amount = modulation_amount.max(0.0);
}
