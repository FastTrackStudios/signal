//! The note handles, and the scope they act on.
//!
//! Vovious puts seven drag handles directly on and around the note
//! instead of a tool palette, which is the single biggest reason its
//! editing is faster than Melodyne's: no gesture costs a tool switch.
//! All seven are **vertical drags**; none is a click target that opens
//! anything.
//!
//! ```text
//!        left slope   fine pitch   right slope
//!                 ╲       │       ╱
//!            ┌────────────────────────┐
//!            │        note body       │  ← pitch
//!            └────────────────────────┘
//!                 ╱       │       ╲
//!            formant  amplitude  vibrato
//! ```
//!
//! ## Scope
//!
//! Every handle acts on a [`Scope`], which is either the whole note or
//! a range inside it. That is the entire mechanism behind Vovious's
//! "temporary note" mode — select a range by dragging horizontally and
//! the same seven handles now address only that span. Building the
//! handles scope-aware from the start means temporary notes are a
//! different *argument*, not a different code path.

use crate::doc::{Dimension, Note};

/// Which handle a press landed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Handle {
    /// The note body — coarse pitch, snapping to the row by default.
    Pitch,
    /// Fine pitch, in cents, ignoring snap.
    FinePitch,
    /// Tilt of the entry transition.
    LeftSlope,
    /// Tilt of the exit transition.
    RightSlope,
    Formant,
    /// Level, and right-click mutes.
    Amplitude,
    /// Vibrato depth — the modulation term's blend.
    Vibrato,
}

impl Handle {
    pub const ALL: [Handle; 7] = [
        Handle::Pitch,
        Handle::FinePitch,
        Handle::LeftSlope,
        Handle::RightSlope,
        Handle::Formant,
        Handle::Amplitude,
        Handle::Vibrato,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Handle::Pitch => "Pitch",
            Handle::FinePitch => "Fine",
            Handle::LeftSlope => "Slope L",
            Handle::RightSlope => "Slope R",
            Handle::Formant => "Formant",
            Handle::Amplitude => "Amplitude",
            Handle::Vibrato => "Vibrato",
        }
    }

    /// Which dimension the handle writes to, where it writes to one.
    ///
    /// Pitch, fine pitch and both slopes all shape the Pitch dimension;
    /// vibrato rewrites it through the decomposition rather than
    /// directly, so it has no single dimension in this sense.
    pub fn dimension(&self) -> Option<Dimension> {
        match self {
            Handle::Pitch | Handle::FinePitch | Handle::LeftSlope | Handle::RightSlope => {
                Some(Dimension::Pitch)
            }
            Handle::Formant => Some(Dimension::Timbre),
            Handle::Amplitude => Some(Dimension::Pressure),
            Handle::Vibrato => None,
        }
    }
}

/// What a handle drag addresses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scope {
    /// The note end to end.
    Note,
    /// A range inside it — the temporary note.
    Range { t0: f64, t1: f64 },
}

impl Scope {
    /// Resolve against a note, clamped to its bounds.
    ///
    /// Clamping rather than rejecting: a temporary-note drag that runs
    /// past the note edge should stop at it, not cancel.
    pub fn span(&self, note: &Note) -> (f64, f64) {
        match *self {
            Scope::Note => (note.start, note.end),
            Scope::Range { t0, t1 } => {
                let (lo, hi) = if t0 <= t1 { (t0, t1) } else { (t1, t0) };
                (lo.max(note.start), hi.min(note.end))
            }
        }
    }

    /// Whether the span is wide enough to edit.
    pub fn is_valid(&self, note: &Note) -> bool {
        let (lo, hi) = self.span(note);
        hi > lo
    }
}

/// A handle's hit rectangle, in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HandleRect {
    pub handle: Handle,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl HandleRect {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }

    pub fn center(&self) -> (f64, f64) {
        (self.x + self.w * 0.5, self.y + self.h * 0.5)
    }
}

/// Height of the handle strips above and below the note, in pixels.
pub const STRIP_H: f64 = 14.0;
/// A note narrower than this gets no corner handles — there is no room
/// to aim at three targets across it, and offering them anyway means
/// hitting the wrong one.
pub const MIN_W_FOR_CORNERS: f64 = 48.0;
/// Below this the note carries no handles at all beyond its body.
pub const MIN_W_FOR_STRIPS: f64 = 18.0;

/// Lay the handles out over a note rectangle.
///
/// The six strip handles sit in bands above and below the note rather
/// than on it, so the body stays one large target — that is what makes
/// coarse pitch, the most common gesture, also the easiest to hit.
///
/// Handles are dropped rather than shrunk when a note is short. Six
/// three-pixel targets on a 32nd note are worse than none: every press
/// becomes a coin toss, and the body handle still works.
pub fn layout(x: f64, y: f64, w: f64, h: f64) -> Vec<HandleRect> {
    let mut out = vec![HandleRect {
        handle: Handle::Pitch,
        x,
        y,
        w,
        h,
    }];
    if w < MIN_W_FOR_STRIPS {
        return out;
    }

    let top = y - STRIP_H;
    let bottom = y + h;
    if w < MIN_W_FOR_CORNERS {
        // Only the centres survive, at full width: fine pitch above and
        // amplitude below are the two worth keeping when there is room
        // for exactly one each.
        out.push(rect(Handle::FinePitch, x, top, w));
        out.push(rect(Handle::Amplitude, x, bottom, w));
        return out;
    }

    let third = w / 3.0;
    out.push(rect(Handle::LeftSlope, x, top, third));
    out.push(rect(Handle::FinePitch, x + third, top, third));
    out.push(rect(Handle::RightSlope, x + third * 2.0, top, third));
    out.push(rect(Handle::Formant, x, bottom, third));
    out.push(rect(Handle::Amplitude, x + third, bottom, third));
    out.push(rect(Handle::Vibrato, x + third * 2.0, bottom, third));
    out
}

fn rect(handle: Handle, x: f64, y: f64, w: f64) -> HandleRect {
    HandleRect {
        handle,
        x,
        y,
        w,
        h: STRIP_H,
    }
}

/// The handle at `(x, y)`, if any.
///
/// Strip handles are tested before the body, so a press in the band
/// above a note reaches fine pitch rather than falling through to the
/// note behind it.
pub fn hit(rects: &[HandleRect], x: f64, y: f64) -> Option<Handle> {
    rects
        .iter()
        .find(|r| r.handle != Handle::Pitch && r.contains(x, y))
        .or_else(|| rects.iter().find(|r| r.contains(x, y)))
        .map(|r| r.handle)
}

/// A handle drag in progress.
///
/// Holds the note's lanes as they were when the gesture opened, and
/// every frame rewrites from *those* rather than from what is on
/// screen. The alternative — applying an incremental delta each move —
/// compounds: a tilt applied sixty times is a cliff, and a level set
/// from the live value drifts. This is the same rule the Multi Tool and
/// the modulation drawer already follow.
#[derive(Clone, Debug, PartialEq)]
pub struct HandleDrag {
    pub handle: Handle,
    pub note: crate::doc::NoteId,
    pub scope: Scope,
    /// Pointer y where the gesture began.
    pub origin_y: f64,
    /// Captured lanes: pitch, timbre, pressure.
    pub base: [Curve; 3],
    /// Row as captured, for the pitch handle.
    pub base_row: i32,
    /// Last amount written, for the readout.
    pub applied: f64,
    /// Amplitude drags address only the unvoiced spans in the scope.
    ///
    /// Captured when the gesture opens rather than read live, so
    /// releasing Shift mid-drag cannot change which spans a gesture
    /// already started writing.
    pub sibilants: bool,
}

use crate::doc::Curve;

impl HandleDrag {
    /// Open a drag on `note`.
    pub fn begin(handle: Handle, note: &Note, scope: Scope, origin_y: f64) -> Self {
        Self::begin_with(handle, note, scope, origin_y, false)
    }

    /// As [`HandleDrag::begin`], with the sibilant scope armed.
    pub fn begin_with(
        handle: Handle,
        note: &Note,
        scope: Scope,
        origin_y: f64,
        sibilants: bool,
    ) -> Self {
        Self {
            sibilants,
            handle,
            note: note.id,
            scope,
            origin_y,
            base: [
                note.pitch.clone(),
                note.timbre.clone(),
                note.pressure.clone(),
            ],
            base_row: note.row,
            applied: 0.0,
        }
    }

    /// The captured curve for a dimension.
    pub fn base_of(&self, dimension: Dimension) -> &Curve {
        match dimension {
            Dimension::Pitch => &self.base[0],
            Dimension::Timbre => &self.base[1],
            Dimension::Pressure => &self.base[2],
        }
    }

    /// Signed drag amount for a pointer at `y`, in the handle's units.
    ///
    /// Up is positive, which is the only direction that reads as "more"
    /// for pitch, level and depth alike.
    pub fn amount(&self, y: f64, viewport_h: f64) -> f64 {
        let f = (self.origin_y - y) / viewport_h.max(1.0);
        f * drag_range(self.handle)
    }
}

/// How far a full-viewport vertical drag travels, per handle.
///
/// These set the *feel*, and they are not arbitrary: coarse pitch wants
/// to cross an octave comfortably, fine pitch wants a whole drag to
/// cover a couple of semitones so cents are reachable, and the trims
/// want their whole range.
pub fn drag_range(handle: Handle) -> f64 {
    match handle {
        Handle::Pitch => 24.0,
        Handle::FinePitch => 2.0,
        Handle::LeftSlope | Handle::RightSlope => 12.0,
        // The dimension handles are 0..1 normalized, so a full drag is the
        // full dimension either way from where it started.
        Handle::Formant | Handle::Amplitude => 1.0,
        // Vibrato blends 0..2: silent to twice as deep.
        Handle::Vibrato => 2.0,
    }
}
