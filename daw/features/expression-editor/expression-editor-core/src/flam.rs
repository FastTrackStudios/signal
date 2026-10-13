//! Flams: the same drum, both hands, a hair apart.
//!
//! A flam is one piece struck by both hands with a small offset — the
//! grace note lands just before or just after the main hit. It is the
//! reason a drum row is *two-handed* at all: for most parts nobody cares
//! which stick hit the snare, so the roll shows one row, and it splits
//! only when something needs the distinction. Notated sticking is the
//! other reason.
//!
//! ## The offset
//!
//! 30 ms by default, and that number is measured rather than chosen.
//! #176's corpus work swept real flams and found they resolve into two
//! hits from about 40 ms apart, with 75% sitting at **25–35 ms** and
//! nothing below 15 ms. A "flam" written at 5 ms is a doubled hit; one
//! at 100 ms is two notes.
//!
//! Expressed in milliseconds and converted through the document's own
//! time base, because a flam is a physical gesture — two sticks and one
//! wrist — and does not scale with tempo the way a subdivision does.

use crate::doc::{ExpressionDoc, Note, NoteId};
use crate::edit::Edit;
use crate::rows::DrumMap;

/// Which side of the main hit the grace note falls on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FlamSide {
    /// Grace note before the beat — the ordinary flam, and the first
    /// step of the cycle.
    #[default]
    Before,
    /// Grace note after: a drag, or a deliberate reverse flam.
    After,
}

impl FlamSide {
    pub fn label(self) -> &'static str {
        match self {
            FlamSide::Before => "Flam before",
            FlamSide::After => "Flam after",
        }
    }
}

/// What one press of the key does to a hit.
///
/// The cycle is **none → before → after → none**, and it belongs to the
/// note rather than to a global mode. That matters: a global
/// before/after flag makes the same keypress mean different things
/// depending on what you did last to some *other* note, so you cannot
/// look at a hit and know what pressing the key will do. Reading the
/// state off the note itself means you always can.
///
/// The third press removing the flam is what makes it a cycle rather
/// than a trap — otherwise undoing a flam you did not want means
/// reaching for undo or hunting the grace note down by hand.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FlamStep {
    /// No grace note yet: add one before.
    Add(FlamSide),
    /// Grace note exists on the wrong side: move it across.
    Move { grace: NoteId, delta: f64 },
    /// Grace note is after the hit: the cycle closes and it goes.
    Remove(NoteId),
}

/// Measured from real playing: see the module docs.
pub const DEFAULT_FLAM_MS: f64 = 30.0;

/// How loud the grace note is relative to the main hit.
///
/// A flam's grace note is quieter — that is what makes it read as one
/// gesture rather than two hits. Not silent, or the flam disappears.
pub const GRACE_VELOCITY: f64 = 0.55;

/// Why a flam could not be made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlamError {
    /// Nothing selected, or the note is gone.
    NoNote,
    /// The row is a single-handed piece — a hi-hat has no other hand to
    /// flam with.
    NotTwoHanded,
}

/// The grace note attached to a hit, and which side it is on.
///
/// Read from the stored relationship, not guessed at by proximity. A
/// flam is a notated thing, so the document says which note ornaments
/// which — and that survives dragging the grace note around, which a
/// distance test would not. It also cannot mistake two ordinary hits
/// that happen to land close together for a flam nobody wrote.
pub fn existing_grace(doc: &ExpressionDoc, id: NoteId) -> Option<(NoteId, FlamSide)> {
    let principal = doc.note(id)?;
    let grace = doc.grace_notes_of(id).first().copied()?;
    let side = if grace.start < principal.start {
        FlamSide::Before
    } else {
        FlamSide::After
    };
    Some((grace.id, side))
}

/// What the next press should do to this hit.
pub fn next_step(
    doc: &ExpressionDoc,
    map: &DrumMap,
    id: NoteId,
    offset_ms: f64,
    bpm: f64,
) -> Result<FlamStep, FlamError> {
    let note = doc.note(id).ok_or(FlamError::NoNote)?;
    map.other_hand_row(note.row.max(0) as usize)
        .ok_or(FlamError::NotTwoHanded)?;
    let offset = offset_ticks(doc, offset_ms, bpm);

    Ok(match existing_grace(doc, id) {
        None => FlamStep::Add(FlamSide::Before),
        Some((grace, FlamSide::Before)) => FlamStep::Move {
            grace,
            // Across the hit and out the other side.
            delta: offset * 2.0,
        },
        Some((grace, FlamSide::After)) => FlamStep::Remove(grace),
    })
}

/// Ticks for a wall-clock offset in this document.
fn offset_ticks(doc: &ExpressionDoc, offset_ms: f64, bpm: f64) -> f64 {
    let units_per_second = doc.time_base.units_per_second(bpm).max(1e-9);
    (offset_ms.abs() / 1000.0) * units_per_second
}

/// Build the edit that turns a hit into a flam.
///
/// The grace note goes on the **other hand's row**, which is the whole
/// point: a flam played with one hand twice is a drag, and the roll
/// should show the difference. Its length matches the main hit, and its
/// velocity is lower so it reads as a grace note.
pub fn flam(
    doc: &ExpressionDoc,
    map: &DrumMap,
    id: NoteId,
    offset_ms: f64,
    bpm: f64,
) -> Result<Edit, FlamError> {
    let step = next_step(doc, map, id, offset_ms, bpm)?;
    let note = doc.note(id).ok_or(FlamError::NoNote)?;
    let other = map
        .other_hand_row(note.row.max(0) as usize)
        .ok_or(FlamError::NotTwoHanded)? as i32;
    let offset = offset_ticks(doc, offset_ms, bpm);

    Ok(match step {
        FlamStep::Add(side) => {
            let start = match side {
                FlamSide::Before => note.start - offset,
                FlamSide::After => note.start + offset,
            };
            let mut grace = Note::new(
                NoteId(next_id(doc)),
                start,
                start + (note.end - note.start),
                other,
            );
            grace.velocity = (note.velocity * GRACE_VELOCITY).clamp(0.0, 1.0);
            grace.channel = note.channel;
            // The link is the flam. Without it this is just a quiet note
            // near another one, and an engraver has no way to know it
            // should draw a grace note.
            grace.grace_of = Some(id);
            Edit::AddNote(Box::new(grace))
        }
        FlamStep::Move { grace, delta } => Edit::MoveTime {
            notes: vec![grace],
            delta,
        },
        FlamStep::Remove(grace) => Edit::DeleteNotes(vec![grace]),
    })
}

/// The next free note id in a document.
fn next_id(doc: &ExpressionDoc) -> u64 {
    doc.notes.iter().map(|n| n.id.0).max().unwrap_or(0) + 1
}
