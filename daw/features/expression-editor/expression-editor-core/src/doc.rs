//! The editable document: notes that own expression curves.
//!
//! One model serves two domains. A MIDI/MPE take and an analyzed audio
//! clip are the same shape once you accept the central claim:
//!
//! > A note sits on an integer pitch row and carries a continuous
//! > pitch curve measured in semitones **relative to that row**.
//!
//! For MPE that curve is the per-note pitch bend. For audio it is the
//! tracked f0 contour minus the rounded center. Both render identically,
//! both edit identically, and in both cases the note rectangle stays
//! anchored to its literal row while the curve shows the sounding
//! offset — the rule MPElodyne arrived at for microtonal display and
//! the rule Melodyne uses for a sung note that is 30 cents flat.
//!
//! Pressure and Timbre are normalized 0..1 lanes with no pitch center.
//! Their audio-domain meanings are dynamics and formant.

use crate::rows::Articulation;
use crate::shape::Shape;

/// Stable identity for a note across edits and re-analysis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NoteId(pub u64);

/// How document time units map to musical and wall-clock time.
///
/// The editor's `t` is always `f64` in these units; only the grid and
/// the audition need to know what they mean.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TimeBase {
    /// MIDI ticks, `ppq` per quarter note.
    Ppq { ppq: f64 },
    /// Analysis frames at a fixed rate (audio domain).
    Frames { frame_rate: f64 },
}

impl TimeBase {
    /// Units per quarter note at `bpm` (constant-tempo approximation;
    /// a tempo map lives in the host adapter, not here).
    pub fn units_per_beat(&self, bpm: f64) -> f64 {
        match *self {
            TimeBase::Ppq { ppq } => ppq,
            TimeBase::Frames { frame_rate } => frame_rate * 60.0 / bpm.max(1e-6),
        }
    }

    /// Units per second.
    pub fn units_per_second(&self, bpm: f64) -> f64 {
        match *self {
            TimeBase::Ppq { ppq } => ppq * bpm.max(1e-6) / 60.0,
            TimeBase::Frames { frame_rate } => frame_rate,
        }
    }
}

/// The three MPE expression dimensions. Audio reads them as pitch,
/// dynamics, and formant.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, facet::Facet)]
pub enum Dimension {
    Pitch,
    Pressure,
    Timbre,
}

impl Dimension {
    pub const ALL: [Dimension; 3] = [Dimension::Pitch, Dimension::Pressure, Dimension::Timbre];

    /// Value a dimension holds where nothing has been authored.
    ///
    /// Pitch rests at the note's own row; Pressure/Timbre default to
    /// the MPE convention of 100/127 rather than full scale.
    pub fn default_value(&self) -> f64 {
        match self {
            Dimension::Pitch => 0.0,
            Dimension::Pressure | Dimension::Timbre => 100.0 / 127.0,
        }
    }

    /// Pitch is unbounded (clamped later by bend range); the others are
    /// normalized.
    pub fn range(&self) -> (f64, f64) {
        match self {
            Dimension::Pitch => (-127.0, 127.0),
            Dimension::Pressure | Dimension::Timbre => (0.0, 1.0),
        }
    }

    pub fn clamp(&self, v: f64) -> f64 {
        let (lo, hi) = self.range();
        v.clamp(lo, hi)
    }
}

/// How a curve travels from one point to the next.
///
/// Mirrors `daw_proto::EnvelopeShape` variant for variant. Core cannot
/// depend on the DAW proto crate, so this is a deliberate copy rather
/// than a re-export — and it is exact precisely so the conversion at the
/// boundary is total and lossless. Adding a variant here that the DAW
/// cannot express would silently degrade on export.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CurveShape {
    /// Straight line to the next point. What every curve was before
    /// shapes existed, and still the default, so MIDI and CC are
    /// unaffected.
    #[default]
    Linear,
    /// Hold, then jump at the next point.
    ///
    /// Matters as much as [`CurveShape::Bezier`] for envelopes: a gate is
    /// on/off with an attack and a release, and forcing that through
    /// linear interpolation is what makes a decimated curve need far
    /// more points than the shape it is describing.
    Square,
    /// Ease in and out — an S-curve.
    SlowStartEnd,
    /// Fast at the start, logarithmic.
    FastStart,
    /// Fast at the end, exponential.
    FastEnd,
    /// Bezier, bent by the point's `tension`.
    Bezier,
}

/// One point on an expression curve.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    /// Document time, absolute (not note-relative) — keeps edits that
    /// span note boundaries honest.
    pub t: f64,
    /// Dimension-native value. Pitch: semitones from the note row.
    pub value: f64,
    /// How the curve reaches the *next* point. Trailing points'
    /// shapes are never read, which matches how the DAW stores it.
    pub shape: CurveShape,
    /// Bezier bend, -1.0..=1.0. Read only by [`CurveShape::Bezier`].
    pub tension: f64,
}

impl Point {
    /// A linear point — the overwhelmingly common case.
    pub fn new(t: f64, value: f64) -> Self {
        Self {
            t,
            value,
            ..Self::default()
        }
    }

    pub fn shaped(t: f64, value: f64, shape: CurveShape) -> Self {
        Self {
            t,
            value,
            shape,
            tension: 0.0,
        }
    }

    pub fn bezier(t: f64, value: f64, tension: f64) -> Self {
        Self {
            t,
            value,
            shape: CurveShape::Bezier,
            tension: tension.clamp(-1.0, 1.0),
        }
    }
}

/// Map linear progress `x` in 0..=1 through a segment's shape.
///
/// Returns the fraction of the way from the left value to the right one,
/// so every shape is expressed as an easing and the caller's lerp is
/// unchanged. Every shape returns 0 at x=0 and 1 at x=1 except
/// [`CurveShape::Square`], which holds the left value until it arrives —
/// that discontinuity is the point of it.
fn shape_ease(shape: CurveShape, x: f64, tension: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    match shape {
        CurveShape::Linear => x,
        CurveShape::Square => {
            if x >= 1.0 {
                1.0
            } else {
                0.0
            }
        }
        // Smoothstep: zero slope at both ends.
        CurveShape::SlowStartEnd => x * x * (3.0 - 2.0 * x),
        // Fast off the mark, easing into the target.
        CurveShape::FastStart => 1.0 - (1.0 - x) * (1.0 - x),
        // Slow to leave, arriving quickly.
        CurveShape::FastEnd => x * x,
        // Rational bezier bend. Tension 0 is linear; positive pushes the
        // curve toward the far value early, negative holds it back.
        CurveShape::Bezier => {
            let k = tension.clamp(-1.0, 1.0);
            if k.abs() < f64::EPSILON {
                x
            } else {
                let w = 1.0 + k;
                (w * x) / (1.0 + (w - 1.0) * x)
            }
        }
    }
}

/// A sampled expression curve: points sorted by `t`, at most one per
/// `t`.
///
/// Interpolation is linear, matching what REAPER's linear CC shape
/// renders, so the native MIDI editor and this canvas agree.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Curve {
    points: Vec<Point>,
}

impl Curve {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_points(mut points: Vec<Point>) -> Self {
        points.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(core::cmp::Ordering::Equal));
        points.dedup_by(|a, b| a.t == b.t);
        Self { points }
    }

    pub fn points(&self) -> &[Point] {
        &self.points
    }

    /// Mutable access to values. Times must stay sorted, so callers may
    /// change `value` freely but should not reorder `t`.
    pub fn points_mut(&mut self) -> &mut [Point] {
        &mut self.points
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn clear(&mut self) {
        self.points.clear();
    }

    /// Insert or replace the value at `t`.
    ///
    /// Replacing rather than appending is what lets a freehand pen
    /// stroke double back over itself without stacking duplicate events
    /// at the same tick.
    pub fn set(&mut self, t: f64, value: f64) {
        match self.index_of(t) {
            Ok(i) => self.points[i].value = value,
            Err(i) => self.points.insert(
                i,
                Point {
                    t,
                    value,
                    ..Point::default()
                },
            ),
        }
    }

    /// Remove every point in `[t0, t1]`, returning how many went.
    pub fn remove_range(&mut self, t0: f64, t1: f64) -> usize {
        let before = self.points.len();
        self.points.retain(|p| p.t < t0 || p.t > t1);
        before - self.points.len()
    }

    /// Clear `[t0, t1]` so it reads as unauthored.
    ///
    /// Deleting the points inside a range is not the same as clearing
    /// it: the curve holds a straight line between whatever survives on
    /// either side, so a swell defined by two points *outside* the
    /// range sails straight through it with nothing to delete. Splicing
    /// the default in at both edges is what makes the region actually
    /// read as cleared.
    ///
    /// Clearing the whole curve is the exception — there is nothing
    /// outside to bleed in, so it empties rather than leaving two
    /// default points behind and calling an untouched lane "authored".
    ///
    /// Returns whether anything changed.
    pub fn clear_range(&mut self, t0: f64, t1: f64, default: f64) -> bool {
        if self.points.is_empty() {
            return false;
        }
        let covers_all = self.points.iter().all(|p| p.t >= t0 && p.t <= t1);
        let removed = self.remove_range(t0, t1) > 0;
        if covers_all {
            return removed;
        }
        self.set(t0, default);
        self.set(t1, default);
        true
    }

    /// Linear sample. Before the first / after the last point the curve
    /// holds that endpoint's value — it never falls back to the default
    /// mid-note, which is what stops an authored curve from snapping to
    /// center at the note edges.
    pub fn sample(&self, t: f64, default: f64) -> f64 {
        if self.points.is_empty() {
            return default;
        }
        match self.index_of(t) {
            Ok(i) => self.points[i].value,
            Err(0) => self.points[0].value,
            Err(i) if i >= self.points.len() => self.points[self.points.len() - 1].value,
            Err(i) => {
                let (a, b) = (self.points[i - 1], self.points[i]);
                let span = b.t - a.t;
                if span <= 0.0 {
                    b.value
                } else {
                    // The *left* point owns the segment's shape, matching
                    // how the DAW stores it: a point describes how the
                    // curve leaves it, not how it arrives.
                    let x = (t - a.t) / span;
                    a.value + (b.value - a.value) * shape_ease(a.shape, x, a.tension)
                }
            }
        }
    }

    /// First/last authored time, if any.
    pub fn bounds(&self) -> Option<(f64, f64)> {
        match (self.points.first(), self.points.last()) {
            (Some(a), Some(b)) => Some((a.t, b.t)),
            _ => None,
        }
    }

    /// Min/max authored value, if any.
    pub fn value_bounds(&self) -> Option<(f64, f64)> {
        let mut lo = f64::MAX;
        let mut hi = f64::MIN;
        for p in &self.points {
            lo = lo.min(p.value);
            hi = hi.max(p.value);
        }
        (lo <= hi).then_some((lo, hi))
    }

    /// Replace `[t0, t1]` with `replacement`, splicing so the points
    /// outside the interval survive byte-for-byte.
    pub fn splice(&mut self, t0: f64, t1: f64, replacement: &[Point]) {
        self.remove_range(t0, t1);
        for p in replacement {
            self.set(p.t, p.value);
        }
    }

    /// Rewrite `[t0, t1]` as an eased ramp between its own current
    /// endpoints, at `count` samples. Endpoints are preserved exactly,
    /// so reshaping never moves the boundary values.
    pub fn reshape(&mut self, t0: f64, t1: f64, shape: Shape, count: usize, default: f64) {
        if t1 <= t0 || count < 2 {
            return;
        }
        let v0 = self.sample(t0, default);
        let v1 = self.sample(t1, default);
        let pts: Vec<Point> = (0..count)
            .map(|i| {
                let x = i as f64 / (count - 1) as f64;
                Point {
                    t: t0 + (t1 - t0) * x,
                    value: v0 + (v1 - v0) * shape.amount(x),
                    // The pen bakes its curve into the point *values*,
                    // so each segment between them is linear.
                    ..Point::default()
                }
            })
            .collect();
        self.splice(t0, t1, &pts);
    }

    /// Scale values in `[t0, t1]` about `pivot`. `factor` below zero
    /// reconstructs the gesture inverted — the far side of the
    /// alt-drag flatten.
    pub fn scale_about(&mut self, t0: f64, t1: f64, pivot: f64, factor: f64) {
        for p in self.points.iter_mut() {
            if p.t >= t0 && p.t <= t1 {
                p.value = pivot + (p.value - pivot) * factor;
            }
        }
    }

    /// Shift values in `[t0, t1]` by `delta`.
    pub fn offset(&mut self, t0: f64, t1: f64, delta: f64) {
        for p in self.points.iter_mut() {
            if p.t >= t0 && p.t <= t1 {
                p.value += delta;
            }
        }
    }

    /// Move every point in `[t0, t1]` in time by `delta`, re-sorting.
    pub fn shift_time(&mut self, t0: f64, t1: f64, delta: f64) {
        for p in self.points.iter_mut() {
            if p.t >= t0 && p.t <= t1 {
                p.t += delta;
            }
        }
        self.points
            .sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(core::cmp::Ordering::Equal));
        self.points.dedup_by(|a, b| a.t == b.t);
    }

    /// Map `[from0, from1]` onto `[to0, to1]` — the note-resize case,
    /// where owned expression stretches with the new bounds.
    pub fn remap_time(&mut self, from0: f64, from1: f64, to0: f64, to1: f64) {
        let src = from1 - from0;
        if src.abs() < 1e-9 {
            return;
        }
        let scale = (to1 - to0) / src;
        for p in self.points.iter_mut() {
            p.t = to0 + (p.t - from0) * scale;
        }
        self.points
            .sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(core::cmp::Ordering::Equal));
        self.points.dedup_by(|a, b| a.t == b.t);
    }

    fn index_of(&self, t: f64) -> Result<usize, usize> {
        self.points
            .binary_search_by(|p| p.t.partial_cmp(&t).unwrap_or(core::cmp::Ordering::Equal))
    }
}

/// A scaling zone boundary inside a note (MPElodyne's "Q split").
///
/// Editor-local metadata: zones never become MIDI events. They divide
/// a note so that scaling, transposition, and drawing target one
/// segment while the rest of the melodic contour is left alone.
pub type ZoneSplit = f64;

/// What an edit gesture applies to within a note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// The complete note (every zone moves together).
    WholeNote,
    /// One zone, by index into the note's segment list.
    Zone(usize),
}

/// A note plus the expression it owns.
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub id: NoteId,
    /// Note-on time, document units.
    pub start: f64,
    /// Note-off time, document units.
    pub end: f64,
    /// The integer pitch row the rectangle is drawn on. Microtonal and
    /// sung-flat offsets live in the Pitch curve, never here.
    pub row: i32,
    /// MPE member channel (2..=16), or `None` in the audio domain.
    pub channel: Option<u8>,
    /// 0..1.
    pub velocity: f64,
    /// Note-off velocity, 0..1 — release character on instruments that
    /// respond to it (Ample's "Fingered Release").
    pub off_velocity: f64,
    /// Muted notes stay in the document and stay editable, but do not
    /// sound. Not the same as deleting.
    pub muted: bool,
    /// Free text carried by the note. In a vocal editor this is the
    /// lyric syllable, and it is the note's identity — the row already
    /// shows the pitch.
    pub text: Option<String>,
    /// Playing technique (guitar/bass, and percussion dead notes).
    pub articulation: Option<Articulation>,
    /// This note is the **grace note of a flam**, and the hit it
    /// ornaments.
    ///
    /// Stored rather than inferred from two notes landing near each
    /// other. A flam is a *notated* thing — engraving draws it as a
    /// small slashed grace note slurred to its principal — so the
    /// document has to carry the relationship, not a renderer's guess
    /// at it. Proximity would also be fragile in both directions: a
    /// grace note nudged by hand stops being one, and two ordinary hits
    /// that happen to fall close together become a flam nobody wrote.
    ///
    /// Which side it falls on is not stored, because the note's own
    /// start already says: before the principal is an ordinary flam,
    /// after is a drag.
    pub grace_of: Option<NoteId>,
    /// Joined to the following note on the same string. Riffer's rule:
    /// the legato is marked on the *first* note of the pair.
    pub legato: bool,
    /// Unpitched audio: the slice's energy-weighted mean frequency.
    ///
    /// Kept on the note so band splits can be moved without re-running
    /// the analysis — which is the whole point of `reband` being cheap
    /// and lossless. Before this the centroids lived only inside the
    /// analyser's own struct, so nothing the editor held could reband
    /// and the operation was unreachable.
    pub centroid_hz: Option<f64>,
    /// Guitar/bass: which string the note is fingered on.
    ///
    /// The row is the **sounding MIDI pitch**, exactly as in every other
    /// mode — a guitar roll is a full pitch roll, not six lanes. The
    /// string is an annotation on top of it: it colours the note and it
    /// is what makes the fret computable, because a pitch alone does
    /// not say where on the neck it was played.
    ///
    /// The fret is therefore *derived*, never stored — `row -
    /// tuning.open(string)`. Storing both invites the two to disagree,
    /// and a note whose fret and pitch disagree is unplayable.
    pub string: Option<u8>,
    pub pitch: Curve,
    pub pressure: Curve,
    pub timbre: Curve,
    /// Interior split times, sorted, strictly inside (start, end).
    pub splits: Vec<ZoneSplit>,
    /// Which zone (if any) gestures currently target.
    pub target: Target,
    /// Expression ownership could not be resolved — another sounding
    /// note shares this channel. The writer must refuse rather than
    /// guess. Drawn red.
    pub ambiguous: bool,
    /// Display weight 0..1 (analysis RMS in the audio domain).
    pub weight: f64,
    /// Per-frame amplitude across the note, 0..1, for drawing a sung
    /// note's body as the waveform it actually is.
    ///
    /// Deliberately separate from the Pressure dimension. Pressure is an
    /// *edit* — a smooth trim the user applies — where this is what was
    /// recorded, and it is jagged in a way a curve of control points
    /// could not represent without thousands of them. Empty outside the
    /// audio domain, where there is no waveform to show.
    pub envelope: Vec<f32>,
}

impl Note {
    pub fn new(id: NoteId, start: f64, end: f64, row: i32) -> Self {
        Self {
            id,
            start,
            end,
            row,
            channel: None,
            velocity: 100.0 / 127.0,
            off_velocity: 64.0 / 127.0,
            muted: false,
            text: None,
            articulation: None,
            grace_of: None,
            legato: false,
            centroid_hz: None,
            string: None,
            pitch: Curve::new(),
            pressure: Curve::new(),
            timbre: Curve::new(),
            splits: Vec::new(),
            target: Target::WholeNote,
            ambiguous: false,
            weight: 1.0,
            envelope: Vec::new(),
        }
    }

    pub fn len(&self) -> f64 {
        self.end - self.start
    }

    /// Move the note and everything anchored to its timeline.
    ///
    /// The curves and zone splits live in document time, so shifting
    /// the rectangle alone would leave a note's own expression behind —
    /// exactly the bug a bare `start += delta` produces.
    pub fn shift_time(&mut self, delta: f64) {
        let (s, e) = (self.start, self.end);
        self.start += delta;
        self.end += delta;
        for split in self.splits.iter_mut() {
            *split += delta;
        }
        for dimension in Dimension::ALL {
            self.curve_mut(dimension).shift_time(s, e, delta);
        }
    }

    pub fn curve(&self, dimension: Dimension) -> &Curve {
        match dimension {
            Dimension::Pitch => &self.pitch,
            Dimension::Pressure => &self.pressure,
            Dimension::Timbre => &self.timbre,
        }
    }

    pub fn curve_mut(&mut self, dimension: Dimension) -> &mut Curve {
        match dimension {
            Dimension::Pitch => &mut self.pitch,
            Dimension::Pressure => &mut self.pressure,
            Dimension::Timbre => &mut self.timbre,
        }
    }

    /// Sounding pitch in MIDI at `t` — row plus the pitch curve.
    pub fn sounding_midi(&self, t: f64) -> f64 {
        self.row as f64 + self.pitch.sample(t, 0.0)
    }

    /// Zone boundaries as `[start, split.., end]`.
    pub fn zone_edges(&self) -> Vec<f64> {
        let mut edges = Vec::with_capacity(self.splits.len() + 2);
        edges.push(self.start);
        edges.extend(self.splits.iter().copied());
        edges.push(self.end);
        edges
    }

    /// `(t0, t1)` for each zone. A note with no splits has one zone
    /// spanning the whole note, so callers never special-case.
    pub fn zones(&self) -> Vec<(f64, f64)> {
        let edges = self.zone_edges();
        edges.windows(2).map(|w| (w[0], w[1])).collect()
    }

    pub fn zone_count(&self) -> usize {
        self.splits.len() + 1
    }

    /// Zone index containing `t`, saturating at the ends.
    pub fn zone_at(&self, t: f64) -> usize {
        self.splits.iter().filter(|&&s| t >= s).count()
    }

    /// Time span the current [`Target`] covers.
    pub fn target_span(&self) -> (f64, f64) {
        match self.target {
            Target::WholeNote => (self.start, self.end),
            Target::Zone(i) => self
                .zones()
                .get(i)
                .copied()
                .unwrap_or((self.start, self.end)),
        }
    }

    /// Add an interior split, keeping the list sorted and rejecting
    /// duplicates or out-of-range positions.
    pub fn add_split(&mut self, t: f64) -> bool {
        if t <= self.start || t >= self.end || self.splits.iter().any(|&s| (s - t).abs() < 1e-6) {
            return false;
        }
        let i = self.splits.partition_point(|&s| s < t);
        self.splits.insert(i, t);
        // A split inserted before the active zone shifts its index.
        if let Target::Zone(z) = self.target
            && i <= z
        {
            self.target = Target::Zone(z + 1);
        }
        true
    }

    /// Remove the split nearest `t` within `tolerance`, merging its
    /// neighbours.
    pub fn remove_split_near(&mut self, t: f64, tolerance: f64) -> bool {
        let Some((i, _)) = self
            .splits
            .iter()
            .enumerate()
            .map(|(i, &s)| (i, (s - t).abs()))
            .filter(|&(_, d)| d <= tolerance)
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(core::cmp::Ordering::Equal))
        else {
            return false;
        };
        self.splits.remove(i);
        if let Target::Zone(z) = self.target {
            self.target = Target::Zone(z.min(self.zone_count() - 1));
        }
        true
    }
}

/// A read-only marker drawn on the timeline (bar lines, item edges,
/// section names) — supplied by the host, never edited here.
#[derive(Clone, Debug, PartialEq)]
pub struct Marker {
    pub t: f64,
    pub label: Option<String>,
}

/// A read-only named span on the timeline — the song's sections
/// (`INTRO`, `VS 1`, `CH 1`), supplied by the host, never edited here.
/// Times are document time, the same axis notes live on.
#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    pub start: f64,
    pub end: f64,
    pub label: String,
    /// `#rrggbb`, when the host assigned one.
    pub color: Option<String>,
}

/// The whole editable surface.
#[derive(Clone, Debug, PartialEq)]
pub struct ExpressionDoc {
    pub notes: Vec<Note>,
    pub time_base: TimeBase,
    /// Editable span (the item, or the analyzed clip).
    pub start: f64,
    pub end: f64,
    /// Semitones of MPE bend range the receiving instrument expects.
    /// Must match, or pitch reads wrong on playback.
    pub bend_range: f64,
    pub markers: Vec<Marker>,
    /// The song's sections, host-supplied and read-only — what the
    /// ruler shows so "where am I" has an answer better than a bar
    /// number.
    pub regions: Vec<Region>,
    /// Document-level controller lanes. Not per-note: an orchestral
    /// part rides CC1/CC11 across a phrase regardless of where the note
    /// boundaries fall.
    pub cc: crate::cc::CcSet,
    /// What the vertical axis means. Lives on the document because
    /// edits (fret, string) are meaningless without the tuning, and
    /// edits only ever see the document.
    pub row_space: crate::rows::RowSpace,
    /// The take's own amplitude, 0..1, sampled uniformly across
    /// `[start, end]`.
    ///
    /// The whole recording, including everything *between* notes —
    /// breaths, consonants, room. Drawn as a backdrop so silence and
    /// noise are visible as themselves rather than as absence, which is
    /// how you tell "the tracker missed a note" from "nothing was
    /// sung". Empty outside the audio domain.
    pub peaks: Vec<f32>,
    /// The analyzed pitch across the whole take, in **absolute** MIDI.
    ///
    /// Display-only, and deliberately so. The notes own the editable
    /// curves; this is what the tracker heard, kept because two things
    /// need it and neither is an edit:
    ///
    /// - the pitch track runs *between* notes as well as through them,
    ///   and there is no note there to own that stretch;
    /// - pitch drawing shows the original underneath as a thin line for
    ///   the whole session, which has to survive the edits made on top.
    ///
    /// Absolute rather than row-relative because it spans notes on
    /// different rows, and no single row is the right origin.
    pub contour: Curve,
    /// Spans with no detected pitch, in document time.
    ///
    /// Sibilants and silence. The pitch track is not drawn across
    /// these, because there was no pitch — drawing one would invent a
    /// reading, and a line through a consonant is the single most
    /// misleading thing this surface could show.
    pub unvoiced: Vec<(f64, f64)>,
    next_id: u64,
}

impl ExpressionDoc {
    pub fn new(time_base: TimeBase, start: f64, end: f64) -> Self {
        Self {
            notes: Vec::new(),
            time_base,
            start,
            end,
            bend_range: 48.0,
            markers: Vec::new(),
            regions: Vec::new(),
            cc: crate::cc::CcSet::default(),
            row_space: crate::rows::RowSpace::Pitch,
            peaks: Vec::new(),
            contour: Curve::new(),
            unvoiced: Vec::new(),
            next_id: 1,
        }
    }

    /// Mint an id that no current note holds.
    pub fn mint_id(&mut self) -> NoteId {
        let id = NoteId(self.next_id);
        self.next_id += 1;
        id
    }

    pub fn push(&mut self, note: Note) {
        self.next_id = self.next_id.max(note.id.0 + 1);
        self.notes.push(note);
    }

    pub fn note(&self, id: NoteId) -> Option<&Note> {
        self.notes.iter().find(|n| n.id == id)
    }

    pub fn note_mut(&mut self, id: NoteId) -> Option<&mut Note> {
        self.notes.iter_mut().find(|n| n.id == id)
    }

    /// The grace notes ornamenting a hit.
    ///
    /// What an engraver asks: this note is a principal, and these are
    /// the small notes slurred to it. Returns them in time order, so a
    /// renderer does not have to sort.
    pub fn grace_notes_of(&self, id: NoteId) -> Vec<&Note> {
        let mut out: Vec<&Note> = self
            .notes
            .iter()
            .filter(|n| n.grace_of == Some(id))
            .collect();
        out.sort_by(|a, b| {
            a.start
                .partial_cmp(&b.start)
                .unwrap_or(core::cmp::Ordering::Equal)
        });
        out
    }

    /// Whether a hit is a flam — it has at least one grace note.
    pub fn is_flam(&self, id: NoteId) -> bool {
        self.notes.iter().any(|n| n.grace_of == Some(id))
    }

    pub fn remove(&mut self, id: NoteId) -> Option<Note> {
        let i = self.notes.iter().position(|n| n.id == id)?;
        Some(self.notes.remove(i))
    }

    /// Pitch extent of the content: integer rows *and* where the pitch
    /// curves actually travel, so Reset View frames a deep scoop
    /// instead of clipping it.
    pub fn pitch_extent(&self) -> Option<(f64, f64)> {
        let mut lo = f64::MAX;
        let mut hi = f64::MIN;
        for n in &self.notes {
            lo = lo.min(n.row as f64 - 0.5);
            hi = hi.max(n.row as f64 + 0.5);
            if let Some((vlo, vhi)) = n.pitch.value_bounds() {
                lo = lo.min(n.row as f64 + vlo);
                hi = hi.max(n.row as f64 + vhi);
            }
        }
        (lo <= hi).then_some((lo, hi))
    }

    /// Notes sounding at `t`, sharing `channel`. More than one means
    /// expression ownership in that overlap is undecidable.
    pub fn channel_conflicts(&self, t: f64, channel: u8) -> usize {
        self.notes
            .iter()
            .filter(|n| n.channel == Some(channel) && n.start <= t && n.end > t)
            .count()
    }

    /// Flag every note whose channel is shared with another sounding
    /// note. Cleared first, so this is idempotent.
    pub fn mark_ambiguity(&mut self) {
        for i in 0..self.notes.len() {
            self.notes[i].ambiguous = false;
        }
        for i in 0..self.notes.len() {
            for j in (i + 1)..self.notes.len() {
                let (a, b) = (&self.notes[i], &self.notes[j]);
                let overlaps = a.start < b.end && b.start < a.end;
                let shared = a.channel.is_some() && a.channel == b.channel;
                if overlaps && shared {
                    self.notes[i].ambiguous = true;
                    self.notes[j].ambiguous = true;
                }
            }
        }
    }
}
