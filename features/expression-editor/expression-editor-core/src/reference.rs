//! The MIDI reference track — a tune-to target that is not a scale.
//!
//! A MIDI part loaded alongside the audio, drawn behind the sung notes
//! as outlines, and used as the thing the vocal should agree with. The
//! manual's controls: pick a track inside the file, show or hide it,
//! transpose it, and adopt its beat and tempo.
//!
//! ## Why this shares an interface with the scale
//!
//! Three things answer the same question — *what pitch should this note
//! be?* — and answering it is the whole job of tuning:
//!
//! - the chromatic grid: the nearest semitone;
//! - a scale: the nearest degree of it;
//! - a MIDI reference: whatever the reference part is playing *at that
//!   moment in time*.
//!
//! Only the third depends on time, which is the one real difference and
//! the reason a single `target(t, pitch)` covers all three. Auto-correct
//! and double-click-to-snap then take a [`SnapSource`] and neither has
//! to know which kind it was given.

use crate::doc::NoteId;
use crate::tuning::Tuning;

/// One note of the reference part.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RefNote {
    pub start: f64,
    pub end: f64,
    /// Integer MIDI row, before transposition.
    pub row: i32,
}

/// A loaded MIDI reference.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MidiReference {
    /// File name, for the panel. Not a path: the data is stored in the
    /// project, so the file need not still be there tomorrow.
    pub name: String,
    /// Track names inside the file, in file order.
    pub tracks: Vec<String>,
    /// Which track is in use.
    pub active: usize,
    /// Notes of the active track only.
    notes: Vec<RefNote>,
    /// Semitones applied on top — a reference written in a different
    /// key is still the right shape.
    pub transpose: i32,
    /// Drawn behind the sung notes.
    pub visible: bool,
    /// The file's own tempo and metre, for "use this beat and BPM".
    pub bpm: Option<f64>,
    pub beats_per_bar: Option<f64>,
}

impl MidiReference {
    pub fn new(name: impl Into<String>, tracks: Vec<String>, notes: Vec<RefNote>) -> Self {
        Self {
            name: name.into(),
            tracks,
            active: 0,
            notes,
            transpose: 0,
            visible: true,
            bpm: None,
            beats_per_bar: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }

    /// Notes with the transposition applied — what is drawn, and what
    /// snapping aims at.
    ///
    /// Applied here rather than stored, so changing the transpose is
    /// non-destructive and a reference can be nudged into key and back
    /// out without accumulating error.
    pub fn notes(&self) -> impl Iterator<Item = RefNote> + '_ {
        let t = self.transpose;
        self.notes.iter().map(move |n| RefNote {
            row: (n.row + t).clamp(0, 127),
            ..*n
        })
    }

    /// Replace the notes when a different track is selected.
    pub fn set_track(&mut self, index: usize, notes: Vec<RefNote>) -> bool {
        if index >= self.tracks.len() {
            return false;
        }
        self.active = index;
        self.notes = notes;
        true
    }

    /// The reference note sounding at `t`.
    ///
    /// Where several overlap — a chord in the reference — the one
    /// nearest `pitch` wins, so a harmony line tunes to its own part
    /// rather than to whichever voice happens to be listed first.
    pub fn at(&self, t: f64, pitch: f64) -> Option<RefNote> {
        self.notes()
            .filter(|n| t >= n.start && t <= n.end)
            .min_by(|a, b| {
                (a.row as f64 - pitch)
                    .abs()
                    .total_cmp(&(b.row as f64 - pitch).abs())
            })
    }
}

/// What a note should be tuned to.
///
/// The three sources answer the same question, so they present the same
/// way and callers stay ignorant of which they were handed.
#[derive(Clone, Copy, Debug)]
pub enum SnapSource<'a> {
    /// The chromatic grid or the active scale, whichever the tuning is
    /// configured for.
    Tuning(&'a Tuning),
    /// The loaded reference part.
    Reference(&'a MidiReference),
}

impl SnapSource<'_> {
    /// The pitch `pitch` should move to at time `t`, if this source has
    /// an opinion.
    ///
    /// `None` means "leave it alone" — a reference with nothing sounding
    /// at `t` has no target, and inventing one would drag a note to
    /// whatever was nearest in a bar it does not belong to.
    pub fn target(&self, t: f64, pitch: f64) -> Option<f64> {
        match self {
            SnapSource::Tuning(tuning) => Some(tuning.snap(pitch).pitch),
            SnapSource::Reference(r) => r.at(t, pitch).map(|n| n.row as f64),
        }
    }

    /// Whether the source can answer at all.
    pub fn is_available(&self) -> bool {
        match self {
            SnapSource::Tuning(_) => true,
            SnapSource::Reference(r) => !r.is_empty(),
        }
    }
}

/// A note's correction toward a target, as a pitch delta.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Correction {
    pub note: NoteId,
    /// Semitones to move the note's centre.
    pub delta: f64,
}

/// Plan corrections for `notes` against a source.
///
/// `amount` blends: 0 leaves everything, 1 lands exactly on target.
/// Partial correction is the default a vocal wants — pinning every note
/// to the grid is what makes a tuned take sound like a machine, and the
/// manual's own Amount control exists for the same reason.
pub fn plan_corrections(
    source: SnapSource<'_>,
    notes: impl Iterator<Item = (NoteId, f64, f64)>,
    amount: f64,
) -> Vec<Correction> {
    let amount = amount.clamp(0.0, 1.0);
    notes
        .filter_map(|(id, t, pitch)| {
            let target = source.target(t, pitch)?;
            let delta = (target - pitch) * amount;
            (delta.abs() > 1e-9).then_some(Correction { note: id, delta })
        })
        .collect()
}
