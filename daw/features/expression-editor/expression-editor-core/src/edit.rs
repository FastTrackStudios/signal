//! Edits and undo.
//!
//! Every mutation goes through [`Edit`] so the UI stays a pure view and
//! the host (MIDI writer, audio render job) sees a describable change
//! rather than a mutated document it has to diff.
//!
//! History is snapshot-based and bounded. Inverse-operation undo would
//! be leaner, but a freehand pen stroke's inverse is the entire prior
//! curve anyway, and zone/target changes have to be captured alongside
//! note and expression changes in a single step — which snapshots get
//! right for free.

use crate::blob;
use crate::doc::{Curve, Dimension, ExpressionDoc, Note, NoteId, Point, Target};
use crate::modulation::Stack;
use crate::rows::{Articulation, RowSpace};
use crate::shape::Shape;

/// One describable change to the document.
#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    /// Replace `[t0, t1]` of a dimension with drawn points (pen, curve).
    /// Points outside the interval survive untouched.
    DrawDimension {
        note: NoteId,
        dimension: Dimension,
        t0: f64,
        t1: f64,
        points: Vec<Point>,
    },
    /// Erase a dimension over `[t0, t1]`.
    EraseDimension {
        note: NoteId,
        dimension: Dimension,
        t0: f64,
        t1: f64,
    },
    /// Restyle `[t0, t1]` between its existing endpoints.
    ReshapeDimension {
        note: NoteId,
        dimension: Dimension,
        t0: f64,
        t1: f64,
        shape: Shape,
        samples: usize,
    },
    /// Scale a dimension about its effective center — the alt-drag gesture.
    /// `factor` below zero inverts.
    ScaleDimension {
        note: NoteId,
        dimension: Dimension,
        t0: f64,
        t1: f64,
        factor: f64,
    },
    /// Scale drift and vibrato independently (the Melodyne sliders).
    /// Rewrites the pitch curve from its decomposition.
    ReblendPitch {
        note: NoteId,
        t0: f64,
        t1: f64,
        drift_amount: f64,
        modulation_amount: f64,
    },
    /// Move a dimension's values up or down over a span, keeping its shape.
    ///
    /// The pitch handles: dragging a note body or its fine-tune handle
    /// changes where the contour sits without flattening the scoop and
    /// vibrato that make it sound sung.
    ShiftDimension {
        note: NoteId,
        dimension: Dimension,
        t0: f64,
        t1: f64,
        delta: f64,
    },
    /// Hold a dimension at one value across a span.
    ///
    /// The formant and amplitude handles, which are per-note trims
    /// rather than contours — there is nothing to preserve the shape
    /// of, and a drag sets a level.
    SetDimensionLevel {
        note: NoteId,
        dimension: Dimension,
        t0: f64,
        t1: f64,
        value: f64,
    },
    /// Tilt one end of a span, pivoting on the other.
    ///
    /// The left and right slope handles: `amount` in semitones, applied
    /// at the anchored end and decaying linearly to zero at the far end,
    /// so the transition into or out of a note steepens without moving
    /// the note's body.
    TiltDimension {
        note: NoteId,
        dimension: Dimension,
        t0: f64,
        t1: f64,
        amount: f64,
        /// Tilt hinges at `t1` and moves `t0` when true.
        from_start: bool,
    },
    /// Fold whole semitones of pitch offset into the note's row.
    ///
    /// Keeps the invariant the whole surface depends on: the row is the
    /// *rounded* pitch centre and the curve carries only the remainder.
    /// A pitch drag can then work purely on the curve and restore the
    /// invariant once, on release, rather than juggling both every
    /// frame.
    NormalizeRow {
        notes: Vec<NoteId>,
    },
    /// Add a modulation stack over a span — programmatic vibrato and
    /// swells. `taper` is the fraction of the span eased in and out.
    ApplyModulation {
        note: NoteId,
        dimension: Dimension,
        t0: f64,
        t1: f64,
        stack: Stack,
        taper: f64,
        samples: usize,
    },
    /// Replace a dimension's span with given points — the drawer's cancel
    /// path, restoring exactly what was captured.
    RestoreDimension {
        note: NoteId,
        dimension: Dimension,
        t0: f64,
        t1: f64,
        points: Vec<Point>,
    },
    /// Transpose notes. The pitch gesture moves rigidly with the row.
    Transpose {
        notes: Vec<NoteId>,
        semitones: i32,
    },
    /// Move notes in time, carrying their owned expression.
    MoveTime {
        notes: Vec<NoteId>,
        delta: f64,
    },
    /// Resize a note; owned expression stretches to the new bounds.
    Resize {
        note: NoteId,
        start: f64,
        end: f64,
    },
    /// Set a note's sounding pitch offset from its row — how a
    /// microtonal target is stored.
    SetPitchOffset {
        note: NoteId,
        semitones: f64,
    },
    /// Velocity, 0..1.
    SetVelocity {
        notes: Vec<NoteId>,
        velocity: f64,
    },
    /// Nudge velocity — the drag path, so a gesture can accumulate.
    NudgeVelocity {
        notes: Vec<NoteId>,
        delta: f64,
    },
    SetOffVelocity {
        notes: Vec<NoteId>,
        velocity: f64,
    },
    SetMuted {
        notes: Vec<NoteId>,
        muted: bool,
    },
    ToggleMuted {
        notes: Vec<NoteId>,
    },
    /// Step the MPE member channel, wrapping within 2..=16.
    NudgeChannel {
        notes: Vec<NoteId>,
        delta: i32,
    },
    /// Scale note lengths about their own starts.
    ScaleLength {
        notes: Vec<NoteId>,
        factor: f64,
    },
    /// Scale note *positions* about `pivot` — arpeggiate.
    StretchPositions {
        notes: Vec<NoteId>,
        pivot: f64,
        factor: f64,
    },
    /// Duplicate notes at an offset; the copies become the selection.
    CopyNotes {
        notes: Vec<NoteId>,
        time_delta: f64,
        row_delta: i32,
    },
    /// Snap note starts to a grid, blending by `strength` (1 = full).
    Quantize {
        notes: Vec<NoteId>,
        step: f64,
        origin: f64,
        strength: f64,
    },
    /// Extend each note to meet the next one on its row.
    Legato {
        notes: Vec<NoteId>,
        gap: f64,
    },
    /// Vocal editor: the syllable carried by the note.
    SetText {
        note: NoteId,
        text: Option<String>,
    },
    /// Guitar/bass technique.
    SetArticulation {
        notes: Vec<NoteId>,
        articulation: Option<Articulation>,
    },
    /// Unpitched audio: move the band splits and re-sort every slice.
    ///
    /// Lossless — each slice keeps the centroid it was analysed with,
    /// so this is a re-sort rather than a re-analysis.
    SetBands {
        bands: crate::rows::SliceBands,
    },
    /// Guitar/bass: move to another string, keeping sounding pitch.
    SetString {
        note: NoteId,
        string: i32,
    },
    SetFret {
        notes: Vec<NoteId>,
        fret: u8,
    },
    AddNote(Box<Note>),
    /// Insert clipboard contents, minting fresh ids. The notes arrive
    /// already positioned — [`crate::clipboard::Clipboard::placed`] does
    /// the placing, so paste-at-pointer and paste-in-place are the same
    /// edit with different arguments.
    PasteNotes(Vec<Note>),
    DeleteNotes(Vec<NoteId>),
    SplitNote {
        note: NoteId,
        t: f64,
    },
    /// Q zones.
    AddZoneSplit {
        note: NoteId,
        t: f64,
    },
    RemoveZoneSplit {
        note: NoteId,
        t: f64,
        tolerance: f64,
    },
    MoveZoneSplit {
        note: NoteId,
        from: f64,
        to: f64,
    },
    SetTarget {
        note: NoteId,
        target: Target,
    },
    /// Reassign MPE member channels so overlapping notes never share
    /// one.
    AssignChannels {
        notes: Vec<NoteId>,
        seed: u64,
    },
    SetBendRange(f64),
    /// Replace a controller's values over `[t0, t1]`.
    DrawCc {
        number: u8,
        t0: f64,
        t1: f64,
        points: Vec<Point>,
    },
    /// Erase a controller over `[t0, t1]`.
    EraseCc {
        number: u8,
        t0: f64,
        t1: f64,
    },
    /// Restyle `[t0, t1]` between its own endpoints.
    ShapeCc {
        number: u8,
        t0: f64,
        t1: f64,
        shape: Shape,
        samples: usize,
    },
    /// Scale a controller about a fixed value — the ride-the-fader
    /// gesture, where 0.5 keeps the shape but halves the depth.
    ScaleCc {
        number: u8,
        t0: f64,
        t1: f64,
        pivot: f64,
        factor: f64,
    },
}

/// How many samples a reshape or reblend writes per second of audio.
/// Dense enough that instruments which ignore sparse expression still
/// track the gesture.
pub const DEFAULT_SAMPLES: usize = 64;

impl Edit {
    /// Apply to `doc`. Returns false if the edit could not be applied
    /// (missing note, degenerate span) — the caller should then not
    /// push a history entry.
    pub fn apply(&self, doc: &mut ExpressionDoc) -> bool {
        let units_per_second = doc.time_base.units_per_second(120.0);
        match self {
            Edit::DrawDimension {
                note,
                dimension,
                t0,
                t1,
                points,
            } => {
                let Some(n) = doc.note_mut(*note) else {
                    return false;
                };
                let (lo, hi) = ordered(*t0, *t1);
                n.curve_mut(*dimension).splice(lo, hi, points);
                extend_to_note_edges(n, *dimension);
                true
            }
            Edit::EraseDimension {
                note,
                dimension,
                t0,
                t1,
            } => {
                let Some(n) = doc.note_mut(*note) else {
                    return false;
                };
                let (lo, hi) = ordered(*t0, *t1);
                let default = dimension.default_value();
                n.curve_mut(*dimension).clear_range(lo, hi, default)
            }
            Edit::ReshapeDimension {
                note,
                dimension,
                t0,
                t1,
                shape,
                samples,
            } => {
                let Some(n) = doc.note_mut(*note) else {
                    return false;
                };
                let (lo, hi) = ordered(*t0, *t1);
                let default = dimension.default_value();
                n.curve_mut(*dimension)
                    .reshape(lo, hi, *shape, (*samples).max(2), default);
                true
            }
            Edit::ScaleDimension {
                note,
                dimension,
                t0,
                t1,
                factor,
            } => {
                let Some(n) = doc.note_mut(*note) else {
                    return false;
                };
                let (lo, hi) = ordered(*t0, *t1);
                let default = dimension.default_value();
                let pivot =
                    blob::effective_center(n.curve(*dimension), lo, hi, DEFAULT_SAMPLES, default);
                n.curve_mut(*dimension).scale_about(lo, hi, pivot, *factor);
                true
            }
            Edit::ShiftDimension {
                note,
                dimension,
                t0,
                t1,
                delta,
            } => {
                let Some(n) = doc.note_mut(*note) else {
                    return false;
                };
                let (lo, hi) = ordered(*t0, *t1);
                if hi - lo <= 0.0 || *delta == 0.0 {
                    return false;
                }
                let default = dimension.default_value();
                // Sample the ends before moving anything: a partial
                // shift has to leave the values *outside* the span
                // where they were, and the only way to hold them is to
                // pin the boundary explicitly.
                let (v0, v1) = {
                    let c = n.curve(*dimension);
                    (c.sample(lo, default), c.sample(hi, default))
                };
                let dimension = *dimension;
                let curve = n.curve_mut(dimension);
                for p in curve.points_mut() {
                    if p.t >= lo && p.t <= hi {
                        p.value = dimension.clamp(p.value + delta);
                    }
                }
                curve.set(lo, dimension.clamp(v0 + delta));
                curve.set(hi, dimension.clamp(v1 + delta));
                true
            }
            Edit::SetDimensionLevel {
                note,
                dimension,
                t0,
                t1,
                value,
            } => {
                let Some(n) = doc.note_mut(*note) else {
                    return false;
                };
                let (lo, hi) = ordered(*t0, *t1);
                if hi - lo <= 0.0 {
                    return false;
                }
                let v = dimension.clamp(*value);
                n.curve_mut(*dimension).splice(
                    lo,
                    hi,
                    &[
                        Point {
                            t: lo,
                            value: v,
                            ..Point::default()
                        },
                        Point {
                            t: hi,
                            value: v,
                            ..Point::default()
                        },
                    ],
                );
                true
            }
            Edit::TiltDimension {
                note,
                dimension,
                t0,
                t1,
                amount,
                from_start,
            } => {
                let Some(n) = doc.note_mut(*note) else {
                    return false;
                };
                let (lo, hi) = ordered(*t0, *t1);
                let span = hi - lo;
                if span <= 0.0 || *amount == 0.0 {
                    return false;
                }
                let default = dimension.default_value();
                let dimension = *dimension;
                // Resample: a tilt is a continuous weighting, and
                // applying it only to existing points would leave a
                // sparse curve tilting in straight segments between
                // them rather than as a slope.
                let n_samples = DEFAULT_SAMPLES;
                let curve = n.curve(dimension);
                let pts: Vec<Point> = (0..n_samples)
                    .map(|i| {
                        let f = i as f64 / (n_samples - 1) as f64;
                        let t = lo + span * f;
                        // Full at the moving end, zero at the hinge.
                        let w = if *from_start { 1.0 - f } else { f };
                        Point {
                            t,
                            value: dimension.clamp(curve.sample(t, default) + amount * w),
                            ..Point::default()
                        }
                    })
                    .collect();
                n.curve_mut(dimension).splice(lo, hi, &pts);
                true
            }
            Edit::NormalizeRow { notes } => {
                let mut any = false;
                for id in notes {
                    let Some(n) = doc.note_mut(*id) else { continue };
                    let (lo, hi) = (n.start, n.end);
                    if hi <= lo {
                        continue;
                    }
                    let center =
                        blob::decompose(&n.pitch, lo, hi, DEFAULT_SAMPLES, units_per_second, 0.0)
                            .center;
                    let whole = center.round();
                    if whole == 0.0 {
                        continue;
                    }
                    // The row absorbs the whole semitones and the curve
                    // gives them up, so the sounding pitch is unchanged
                    // and only its bookkeeping moves.
                    n.row = (n.row + whole as i32).clamp(0, 127);
                    for p in n.pitch.points_mut() {
                        p.value -= whole;
                    }
                    any = true;
                }
                any
            }
            Edit::ReblendPitch {
                note,
                t0,
                t1,
                drift_amount,
                modulation_amount,
            } => {
                let Some(n) = doc.note_mut(*note) else {
                    return false;
                };
                let (lo, hi) = ordered(*t0, *t1);
                if hi - lo <= 0.0 {
                    return false;
                }
                let d = blob::decompose(&n.pitch, lo, hi, DEFAULT_SAMPLES, units_per_second, 0.0);
                let rebuilt = d.recompose(d.center, *drift_amount, *modulation_amount);
                n.pitch.splice(lo, hi, rebuilt.points());
                true
            }
            Edit::ApplyModulation {
                note,
                dimension,
                t0,
                t1,
                stack,
                taper,
                samples,
            } => {
                let Some(n) = doc.note_mut(*note) else {
                    return false;
                };
                let (lo, hi) = ordered(*t0, *t1);
                if hi - lo <= 0.0 {
                    return false;
                }
                let count = (*samples).max(8);
                let default = dimension.default_value();
                let curve = n.curve(*dimension);
                let times: Vec<f64> = (0..count)
                    .map(|i| lo + (hi - lo) * (i as f64 / (count - 1) as f64))
                    .collect();
                let mut values: Vec<f64> =
                    times.iter().map(|&t| curve.sample(t, default)).collect();
                crate::modulation::apply(&mut values, stack, *taper);
                let points: Vec<Point> = times
                    .iter()
                    .zip(&values)
                    .map(|(&t, &v)| Point {
                        t,
                        value: dimension.clamp(v),
                        ..Point::default()
                    })
                    .collect();
                // Splice, so data outside the target range survives.
                n.curve_mut(*dimension).splice(lo, hi, &points);
                true
            }
            Edit::RestoreDimension {
                note,
                dimension,
                t0,
                t1,
                points,
            } => {
                let Some(n) = doc.note_mut(*note) else {
                    return false;
                };
                let (lo, hi) = ordered(*t0, *t1);
                n.curve_mut(*dimension).splice(lo, hi, points);
                true
            }
            Edit::Transpose { notes, semitones } => {
                let mut any = false;
                for id in notes {
                    if let Some(n) = doc.note_mut(*id) {
                        n.row = (n.row + semitones).clamp(0, 127);
                        any = true;
                    }
                }
                any
            }
            Edit::MoveTime { notes, delta } => {
                let mut any = false;
                for id in notes {
                    if let Some(n) = doc.note_mut(*id) {
                        n.shift_time(*delta);
                        any = true;
                    }
                }
                any
            }
            Edit::Resize { note, start, end } => {
                let Some(n) = doc.note_mut(*note) else {
                    return false;
                };
                let (new_s, new_e) = ordered(*start, *end);
                if new_e - new_s <= 0.0 {
                    return false;
                }
                let (old_s, old_e) = (n.start, n.end);
                if (old_e - old_s).abs() < 1e-9 {
                    return false;
                }
                let scale = (new_e - new_s) / (old_e - old_s);
                for s in n.splits.iter_mut() {
                    *s = new_s + (*s - old_s) * scale;
                }
                for dimension in Dimension::ALL {
                    n.curve_mut(dimension)
                        .remap_time(old_s, old_e, new_s, new_e);
                }
                n.start = new_s;
                n.end = new_e;
                true
            }
            Edit::SetPitchOffset { note, semitones } => {
                let Some(n) = doc.note_mut(*note) else {
                    return false;
                };
                let (s, e) = (n.start, n.end);
                if n.pitch.is_empty() {
                    n.pitch.set(s, *semitones);
                    n.pitch.set(e, *semitones);
                } else {
                    // Move the whole gesture rigidly — a microtonal
                    // target shifts the note without reshaping how it
                    // was sung or drawn.
                    let current = blob::effective_center(&n.pitch, s, e, DEFAULT_SAMPLES, 0.0);
                    n.pitch.offset(s, e, *semitones - current);
                }
                true
            }
            Edit::SetVelocity { notes, velocity } => {
                map_notes(doc, notes, |n| n.velocity = velocity.clamp(0.0, 1.0))
            }
            Edit::NudgeVelocity { notes, delta } => map_notes(doc, notes, |n| {
                n.velocity = (n.velocity + delta).clamp(0.0, 1.0)
            }),
            Edit::SetOffVelocity { notes, velocity } => {
                map_notes(doc, notes, |n| n.off_velocity = velocity.clamp(0.0, 1.0))
            }
            Edit::SetMuted { notes, muted } => map_notes(doc, notes, |n| n.muted = *muted),
            Edit::ToggleMuted { notes } => map_notes(doc, notes, |n| n.muted = !n.muted),
            Edit::NudgeChannel { notes, delta } => map_notes(doc, notes, |n| {
                // Wrap inside the member range; channel 1 stays the MPE
                // master and must never be assigned to a note.
                let cur = n.channel.unwrap_or(2) as i32;
                let wrapped = (cur - 2 + delta).rem_euclid(15) + 2;
                n.channel = Some(wrapped as u8);
            }),
            Edit::ScaleLength { notes, factor } => {
                let f = factor.max(1e-3);
                let mut any = false;
                for id in notes {
                    let Some(n) = doc.note(*id).cloned() else {
                        continue;
                    };
                    let end = n.start + (n.end - n.start) * f;
                    any |= Edit::Resize {
                        note: *id,
                        start: n.start,
                        end,
                    }
                    .apply(doc);
                }
                any
            }
            Edit::StretchPositions {
                notes,
                pivot,
                factor,
            } => {
                let mut any = false;
                for id in notes {
                    let Some(n) = doc.note(*id).cloned() else {
                        continue;
                    };
                    let delta = (pivot + (n.start - pivot) * factor) - n.start;
                    any |= Edit::MoveTime {
                        notes: vec![*id],
                        delta,
                    }
                    .apply(doc);
                }
                any
            }
            Edit::CopyNotes {
                notes,
                time_delta,
                row_delta,
            } => {
                let sources: Vec<Note> = notes
                    .iter()
                    .filter_map(|id| doc.note(*id).cloned())
                    .collect();
                if sources.is_empty() {
                    return false;
                }
                for src in sources {
                    let id = doc.mint_id();
                    let mut copy = src.clone();
                    copy.id = id;
                    copy.row = (copy.row + row_delta).clamp(0, 127);
                    doc.push(copy);
                    Edit::MoveTime {
                        notes: vec![id],
                        delta: *time_delta,
                    }
                    .apply(doc);
                }
                true
            }
            Edit::Quantize {
                notes,
                step,
                origin,
                strength,
            } => {
                if *step <= 0.0 {
                    return false;
                }
                let mut any = false;
                for id in notes {
                    let Some(n) = doc.note(*id).cloned() else {
                        continue;
                    };
                    let target = origin + ((n.start - origin) / step).round() * step;
                    // Partial strength is how a part keeps its feel:
                    // pulled toward the grid without being nailed to it.
                    let delta = (target - n.start) * strength.clamp(0.0, 1.0);
                    if delta.abs() > 1e-9 {
                        any |= Edit::MoveTime {
                            notes: vec![*id],
                            delta,
                        }
                        .apply(doc);
                    }
                }
                any
            }
            Edit::Legato { notes, gap } => {
                let mut any = false;
                for id in notes {
                    let Some(n) = doc.note(*id).cloned() else {
                        continue;
                    };
                    // The next note starting on the same row.
                    let next = doc
                        .notes
                        .iter()
                        .filter(|o| o.row == n.row && o.start > n.start)
                        .map(|o| o.start)
                        .fold(f64::INFINITY, f64::min);
                    if !next.is_finite() {
                        continue;
                    }
                    let end = (next - gap).max(n.start + 1.0);
                    any |= Edit::Resize {
                        note: *id,
                        start: n.start,
                        end,
                    }
                    .apply(doc);
                }
                any
            }
            Edit::SetText { note, text } => {
                let Some(n) = doc.note_mut(*note) else {
                    return false;
                };
                n.text = text.clone();
                true
            }
            Edit::SetArticulation {
                notes,
                articulation,
            } => map_notes(doc, notes, |n| {
                n.articulation = *articulation;
                n.legato = articulation.is_some_and(|a| a.is_legato());
            }),
            Edit::SetString { note, string } => {
                let RowSpace::Strings(tuning) = doc.row_space.clone() else {
                    return false;
                };
                let target = (*string).clamp(0, tuning.strings() as i32 - 1);
                let Some(n) = doc.note_mut(*note) else {
                    return false;
                };
                // Re-fingering only. The row is the sounding pitch and
                // must not move: playing the same note on another
                // string is a different fret, not a different note.
                // Refused when the pitch is not reachable there — the
                // 5th fret of the low E is not on the high E at all.
                let fret = n.row - tuning.open(target as usize);
                if fret < 0 || fret > tuning.frets as i32 {
                    return false;
                }
                n.string = Some(target as u8);
                true
            }
            Edit::SetBands { bands } => {
                if !matches!(doc.row_space, RowSpace::Bands(_)) {
                    return false;
                }
                // Every slice re-sorts into its new band from the
                // centroid it already carries. No analysis re-runs, and
                // a slice with no centroid stays where it is rather
                // than collapsing onto band zero.
                for n in &mut doc.notes {
                    if let Some(hz) = n.centroid_hz {
                        n.row = bands.band_of(hz) as i32;
                    }
                }
                doc.row_space = RowSpace::Bands(bands.clone());
                true
            }
            Edit::SetFret { notes, fret } => {
                let RowSpace::Strings(tuning) = doc.row_space.clone() else {
                    return false;
                };
                let fret = (*fret).min(tuning.frets) as i32;
                // The fret is derived, so setting one *transposes*: it
                // keeps the string and moves the pitch, which is what
                // sliding a finger up the neck does.
                //
                // A note with no string has no fret to set — under the
                // current model that is a legitimate plain MIDI note on
                // a guitar track, not an error. Reporting the edit as
                // applied would push an undo step that undoes nothing,
                // so the result is whether any note actually moved.
                let mut moved = false;
                map_notes(doc, notes, |n| {
                    if let Some(s) = n.string {
                        n.row = tuning.open(s as usize) + fret;
                        moved = true;
                    }
                });
                moved
            }
            Edit::AddNote(note) => {
                doc.push((**note).clone());
                true
            }
            Edit::PasteNotes(notes) => {
                if notes.is_empty() {
                    return false;
                }
                for n in notes {
                    let mut c = n.clone();
                    c.id = doc.mint_id();
                    doc.push(c);
                }
                true
            }
            Edit::DeleteNotes(ids) => {
                let before = doc.notes.len();
                doc.notes.retain(|n| !ids.contains(&n.id));
                doc.notes.len() != before
            }
            Edit::SplitNote { note, t } => split_note(doc, *note, *t),
            Edit::AddZoneSplit { note, t } => doc
                .note_mut(*note)
                .map(|n| n.add_split(*t))
                .unwrap_or(false),
            Edit::RemoveZoneSplit { note, t, tolerance } => doc
                .note_mut(*note)
                .map(|n| n.remove_split_near(*t, *tolerance))
                .unwrap_or(false),
            Edit::MoveZoneSplit { note, from, to } => {
                let Some(n) = doc.note_mut(*note) else {
                    return false;
                };
                let Some(i) = n.splits.iter().position(|s| (s - from).abs() < 1e-6) else {
                    return false;
                };
                // Keep splits ordered and strictly interior; a boundary
                // dragged past its neighbour would silently reorder the
                // zones under the pointer.
                let lo = n.splits.get(i.wrapping_sub(1)).copied().unwrap_or(n.start);
                let hi = n.splits.get(i + 1).copied().unwrap_or(n.end);
                let eps = (n.end - n.start) * 1e-4;
                n.splits[i] = to.clamp(lo + eps, hi - eps);
                true
            }
            Edit::SetTarget { note, target } => {
                let Some(n) = doc.note_mut(*note) else {
                    return false;
                };
                n.target = match *target {
                    Target::Zone(i) if i >= n.zone_count() => Target::WholeNote,
                    t => t,
                };
                true
            }
            Edit::AssignChannels { notes, seed } => assign_channels(doc, notes, *seed),
            Edit::SetBendRange(r) => {
                doc.bend_range = r.max(1.0);
                true
            }
            Edit::DrawCc {
                number,
                t0,
                t1,
                points,
            } => {
                let i = doc.cc.ensure(*number);
                let (lo, hi) = ordered(*t0, *t1);
                doc.cc.lanes[i].curve.splice(lo, hi, points);
                true
            }
            Edit::EraseCc { number, t0, t1 } => {
                let Some(l) = doc.cc.get_mut(*number) else {
                    return false;
                };
                let (lo, hi) = ordered(*t0, *t1);
                let default = l.default_value();
                l.curve.clear_range(lo, hi, default)
            }
            Edit::ShapeCc {
                number,
                t0,
                t1,
                shape,
                samples,
            } => {
                let Some(l) = doc.cc.get_mut(*number) else {
                    return false;
                };
                let (lo, hi) = ordered(*t0, *t1);
                let default = l.default_value();
                l.curve.reshape(lo, hi, *shape, (*samples).max(2), default);
                true
            }
            Edit::ScaleCc {
                number,
                t0,
                t1,
                pivot,
                factor,
            } => {
                let Some(l) = doc.cc.get_mut(*number) else {
                    return false;
                };
                let (lo, hi) = ordered(*t0, *t1);
                l.curve.scale_about(lo, hi, *pivot, *factor);
                // A controller is bounded; scaling must not push it
                // past the wire range and clip on export.
                for p in l.curve.points_mut() {
                    p.value = p.value.clamp(0.0, 1.0);
                }
                true
            }
        }
    }
}

fn ordered(a: f64, b: f64) -> (f64, f64) {
    if a <= b { (a, b) } else { (b, a) }
}

/// Hold a dimension's first and last authored values out to the note edges.
///
/// Without this a gesture drawn in the middle of a note leaves a gap
/// where the dimension falls back to its default — audible as a jump to
/// center on either side of the edit.
fn extend_to_note_edges(note: &mut Note, dimension: Dimension) {
    let (s, e) = (note.start, note.end);
    let curve = note.curve_mut(dimension);
    let Some((first, last)) = curve.bounds() else {
        return;
    };
    if first > s {
        let v = curve.sample(first, dimension.default_value());
        curve.set(s, v);
    }
    if last < e {
        let v = curve.sample(last, dimension.default_value());
        curve.set(e, v);
    }
}

fn split_note(doc: &mut ExpressionDoc, id: NoteId, t: f64) -> bool {
    let Some(src) = doc.note(id).cloned() else {
        return false;
    };
    if t <= src.start || t >= src.end {
        return false;
    }
    let new_id = doc.mint_id();
    let mut right = src.clone();
    right.id = new_id;
    right.start = t;
    right.splits.retain(|&s| s > t);
    right.target = Target::WholeNote;
    for dimension in Dimension::ALL {
        let held = right.curve(dimension).sample(t, dimension.default_value());
        let curve = right.curve_mut(dimension);
        curve.remove_range(f64::NEG_INFINITY, t);
        curve.set(t, held);
    }

    let Some(left) = doc.note_mut(id) else {
        return false;
    };
    left.end = t;
    left.splits.retain(|&s| s < t);
    left.target = Target::WholeNote;
    for dimension in Dimension::ALL {
        let held = left.curve(dimension).sample(t, dimension.default_value());
        let curve = left.curve_mut(dimension);
        curve.remove_range(t, f64::INFINITY);
        curve.set(t, held);
    }
    doc.push(right);
    true
}

/// Give each note an MPE member channel (2..=16) such that no two
/// overlapping, touching, or immediately consecutive notes share one.
///
/// Channel 1 stays free as the MPE master. The consecutive-note rule
/// matters as much as the overlap rule: reusing a channel the instant a
/// note ends means the incoming note's setup expression lands while the
/// outgoing note's release is still sounding.
fn assign_channels(doc: &mut ExpressionDoc, ids: &[NoteId], seed: u64) -> bool {
    let mut order: Vec<usize> = doc
        .notes
        .iter()
        .enumerate()
        .filter(|(_, n)| ids.contains(&n.id))
        .map(|(i, _)| i)
        .collect();
    if order.is_empty() {
        return false;
    }
    order.sort_by(|&a, &b| {
        doc.notes[a]
            .start
            .partial_cmp(&doc.notes[b].start)
            .unwrap_or(core::cmp::Ordering::Equal)
    });

    // Deterministic per-seed rotation: same input, same layout, but a
    // fresh seed reshuffles when an instrument dislikes a given spread.
    let mut rng = seed | 1;
    let mut assigned: Vec<(f64, f64, u8)> = Vec::new();
    for &i in &order {
        let (start, end) = (doc.notes[i].start, doc.notes[i].end);
        let mut taken = [false; 17];
        for &(s, e, ch) in &assigned {
            // Touching counts as conflicting.
            if s <= end && start <= e {
                taken[ch as usize] = true;
            }
        }
        rng = rng
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let offset = (rng >> 33) as usize % 15;
        let chosen = (0..15)
            .map(|k| 2 + ((offset + k) % 15) as u8)
            .find(|&ch| !taken[ch as usize])
            .unwrap_or(2);
        doc.notes[i].channel = Some(chosen);
        assigned.push((start, end, chosen));
    }
    doc.mark_ambiguity();
    true
}

/// Bounded snapshot undo.
#[derive(Clone, Debug, PartialEq)]
pub struct History {
    past: Vec<ExpressionDoc>,
    future: Vec<ExpressionDoc>,
    limit: usize,
}

impl History {
    pub fn new(limit: usize) -> Self {
        Self {
            past: Vec::new(),
            future: Vec::new(),
            limit: limit.max(1),
        }
    }

    /// Apply `edit`, recording an undo step only if it changed
    /// anything.
    pub fn apply(&mut self, doc: &mut ExpressionDoc, edit: &Edit) -> bool {
        let snapshot = doc.clone();
        if !edit.apply(doc) {
            return false;
        }
        self.push_snapshot(snapshot);
        true
    }

    /// Record the pre-gesture state directly — for drags that stream
    /// many edits but should collapse into one undo step.
    pub fn begin_gesture(&mut self, doc: &ExpressionDoc) {
        self.push_snapshot(doc.clone());
    }

    /// The document as the open gesture found it.
    ///
    /// [`begin_gesture`](Self::begin_gesture) already clones the whole
    /// document so the gesture can be undone in one step; this hands
    /// that same snapshot back so a *destructive* drag can recompute
    /// from it every frame instead of accumulating damage. Nothing is
    /// popped — undo is unaffected.
    pub fn gesture_base(&self) -> Option<&ExpressionDoc> {
        self.past.last()
    }

    fn push_snapshot(&mut self, snapshot: ExpressionDoc) {
        self.past.push(snapshot);
        if self.past.len() > self.limit {
            self.past.remove(0);
        }
        self.future.clear();
    }

    pub fn undo(&mut self, doc: &mut ExpressionDoc) -> bool {
        let Some(prev) = self.past.pop() else {
            return false;
        };
        self.future.push(core::mem::replace(doc, prev));
        true
    }

    pub fn redo(&mut self, doc: &mut ExpressionDoc) -> bool {
        let Some(next) = self.future.pop() else {
            return false;
        };
        self.past.push(core::mem::replace(doc, next));
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new(10)
    }
}

/// Build the point list a freehand stroke commits, at one point per
/// document-time step (screen-pixel resolution at the current zoom).
pub fn stroke_points(samples: &[(f64, f64)], dimension: Dimension) -> Vec<Point> {
    let mut curve = Curve::new();
    for &(t, v) in samples {
        curve.set(t, dimension.clamp(v));
    }
    curve.points().to_vec()
}

/// Apply `f` to every named note, reporting whether any existed.
fn map_notes(doc: &mut ExpressionDoc, ids: &[NoteId], mut f: impl FnMut(&mut Note)) -> bool {
    let mut any = false;
    for id in ids {
        if let Some(n) = doc.note_mut(*id) {
            f(n);
            any = true;
        }
    }
    any
}
