//! What a tool needs to know about the thing it is editing.
//!
//! The editor serves seven modes over three kinds of thing that all sit
//! somewhere in time: a **MIDI note**, an **audio transient**, and a
//! **pitch-detected note**. Quantize, Align, Velocity and Arp each want
//! to do the same job to all three. Writing each tool three times is how
//! the three copies end up disagreeing about what "on the beat" means.
//!
//! So the tools are written once, over two traits:
//!
//! - [`Timed`] — every tool requires it. An event is *somewhere*, it is
//!   worth *something*, and it can be *put somewhere else*.
//! - [`Sustained`] — optional, and only some events have it. An event
//!   that occupies time, not just a moment.
//!
//! ## Why the split is here and not somewhere else
//!
//! Because not all audio has an end. A transient is the instant a stick
//! meets a head; there is no second number, and inventing one (decay
//! time? the next transient?) would be a lie the tools then act on. A
//! MIDI note has a real note-off, and a pitch-detected note has a real
//! last voiced frame. The trait boundary is drawn exactly where the
//! truth changes.
//!
//! ## Why moving is a method and not a field
//!
//! [`Timed::move_to`] is the only way a tool moves anything, and that is
//! what keeps length out of every tool's body. A [`Note`] stores its end
//! absolutely and owns curves and zone splits in document time, so it
//! moves all of them together; a transient has nothing to carry. Neither
//! tool has to know which it is holding:
//!
//! ```ignore
//! for m in &plan.moves {
//!     events[m.index].move_to(m.to);   // length preserved, or absent
//! }
//! ```
//!
//! ## Time units
//!
//! Positions are `f64` in **the events' own time unit** — seconds for a
//! transient, document units (ticks or analysis frames) for a note. A
//! tool never mixes two collections, so it never needs to know which it
//! has. This is why the tool configs talk about a `grid` and not about
//! `grid_secs`: a musical grid under a tempo map is evenly spaced in
//! ticks and is *not* evenly spaced in seconds, and a seconds-only seam
//! would quietly get MIDI wrong the first time the tempo moved.
//!
//! [`Note`]: expression_editor_core::doc::Note

use expression_editor_core::doc::Note;

/// Something a tool can move in time.
///
/// The base requirement: every tool in this crate takes `E: Timed`.
pub trait Timed {
    /// Where the event begins, in the events' own time unit.
    fn onset(&self) -> f64;

    /// Put the event's onset at `to`, carrying everything that belongs
    /// to it.
    ///
    /// "Everything that belongs to it" is the whole point of this being
    /// a method. A note's end, its expression curves and its zone
    /// splits are all stored in absolute document time, so a bare
    /// `start = to` leaves a note's own expression behind — a bug that
    /// looks like the pitch curve drifting off its note. The impl knows
    /// what it carries; the tool does not have to.
    fn move_to(&mut self, to: f64);

    /// How much this event is worth against its neighbours, `0.0..=1.0`.
    ///
    /// Velocity for a MIDI note, detected loudness for a transient,
    /// analysed level for a pitch-detected note. It is a **ranking**,
    /// not a measurement: tools use it to pick a winner when two events
    /// want the same grid division, and to drop events below the
    /// sensitivity filter so a buzz roll or a ghost note is not dragged
    /// onto a beat it was never near. No tool converts it to dB or
    /// writes it back.
    fn strength(&self) -> f64;

    /// How long the event lasts, or `None` for something that is only a
    /// moment.
    ///
    /// Defaulted, so a momentary event says nothing and no impl has to
    /// think about it. Every [`Sustained`] event forwards to
    /// [`length_of`] in one line, which is what keeps the two answers
    /// from ever disagreeing.
    ///
    /// Tools *read* this — to draw the preview, to report what a move
    /// will produce — and never set it. Quantize preserves length by
    /// never touching it: the length rides along inside
    /// [`move_to`](Timed::move_to).
    fn length(&self) -> Option<f64> {
        None
    }
}

/// An event that occupies time rather than marking a moment.
///
/// The bound a tool takes when it genuinely *needs* an end rather than
/// merely reporting one — drawing a plan as rectangles, matching sung
/// syllables to a reference, deciding whether a quantized note now runs
/// into the next one. A tool that takes this bound is declaring it
/// cannot serve transients, which is honest and checked at compile time
/// rather than discovered as a zero-length rectangle.
///
/// Query only. Nothing sets an end through this trait: a pitch-detected
/// note's length is the length of its per-frame drift and modulation
/// vectors, so a setter here would be a way to corrupt it.
pub trait Sustained: Timed {
    /// Where the event stops, in the same unit as [`Timed::onset`].
    fn end(&self) -> f64;
}

/// The [`Timed::length`] answer for a [`Sustained`] event.
///
/// Exists so every implementor writes the same one-liner —
/// `fn length(&self) -> Option<f64> { length_of(self) }` — and the two
/// traits cannot drift into disagreeing about how long an event is.
pub fn length_of<E: Sustained + ?Sized>(event: &E) -> Option<f64> {
    Some((event.end() - event.onset()).max(0.0))
}

// ── MIDI / MPE notes ──────────────────────────────────────────────────

impl Timed for Note {
    fn onset(&self) -> f64 {
        self.start
    }

    /// Moves the note-off, the curves and the zone splits with the
    /// note-on — `Note::shift_time` is the document's own rule for
    /// this, and reusing it means a quantize can never move a note in a
    /// way a drag could not.
    fn move_to(&mut self, to: f64) {
        self.shift_time(to - self.start);
    }

    fn strength(&self) -> f64 {
        self.velocity
    }

    fn length(&self) -> Option<f64> {
        length_of(self)
    }
}

impl Sustained for Note {
    fn end(&self) -> f64 {
        self.end
    }
}

// ── Pitch-detected notes ──────────────────────────────────────────────

/// A detected note's onset and end are frame indices, so the seam works
/// in **analysis frames** for this type. That is the same unit the audio
/// domain's document uses (`TimeBase::Frames`), so a quantize of a sung
/// take and a quantize of its document agree without a conversion.
#[cfg(feature = "pitch")]
mod pitch {
    use super::{Sustained, Timed, length_of};
    use tune_dsp::model::NoteBlob;

    impl Timed for NoteBlob {
        fn onset(&self) -> f64 {
            self.start_frame as f64
        }

        /// Frames are integers, so a move rounds — a detected note
        /// cannot land between two analysis frames however precisely
        /// the grid asks. At the default hop that is under 6 ms, and it
        /// is a real limit of the analysis rather than of the tool.
        ///
        /// The end moves with the start. The blob's `drift` and
        /// `modulation` are one value *per frame of this note*, so its
        /// frame count is not the tool's to change: shift both edges or
        /// corrupt the note.
        fn move_to(&mut self, to: f64) {
            let frames = self.end_frame.saturating_sub(self.start_frame);
            let start = to.round().max(0.0) as usize;
            self.start_frame = start;
            self.end_frame = start + frames;
        }

        /// Mean frame RMS. The analysed level *is* the ranking here —
        /// a sung note's velocity does not exist.
        fn strength(&self) -> f64 {
            self.rms
        }

        fn length(&self) -> Option<f64> {
            length_of(self)
        }
    }

    impl Sustained for NoteBlob {
        /// Inclusive last frame, so a one-frame note has length zero and
        /// a two-frame note length one. Consistent with
        /// `NoteBlob::start_frame..=end_frame`, which is what the
        /// renderer walks.
        fn end(&self) -> f64 {
            self.end_frame as f64
        }
    }
}
