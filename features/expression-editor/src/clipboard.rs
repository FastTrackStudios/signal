//! Cut / copy / paste.
//!
//! Notes are captured **relative to the earliest one**, not at their
//! absolute position, so a paste lands where the pointer is and the
//! internal spacing of a phrase survives the trip. Curves, zone splits,
//! articulations and lyrics all ride along — a copied note is the whole
//! note, not a rectangle.

use crate::doc::{ExpressionDoc, Note, NoteId};

/// Notes lifted out of a document, normalized to their own origin.
///
/// Deliberately not tied to the system clipboard: this surface runs as
/// a plugin editor inside hosts that route keyboard and clipboard
/// unpredictably, and a paste that silently picks up a spreadsheet cell
/// is worse than one that only ever sees notes. The host adapter can
/// bridge to the OS clipboard where it makes sense.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Clipboard {
    /// Notes with `start` rebased so the earliest sits at 0, and `row`
    /// rebased so the lowest sits at 0.
    notes: Vec<Note>,
    /// Row of the lowest captured note, so a paste that does not
    /// specify a row can put the phrase back at its own pitch.
    origin_row: i32,
}

impl Clipboard {
    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }

    pub fn len(&self) -> usize {
        self.notes.len()
    }

    /// The row the phrase was copied from.
    pub fn origin_row(&self) -> i32 {
        self.origin_row
    }

    /// The phrase's own duration — how much room a paste needs.
    pub fn span(&self) -> f64 {
        self.notes
            .iter()
            .map(|n| n.end)
            .fold(0.0_f64, |a, b| a.max(b))
    }

    /// Capture `ids` from `doc`. Ids that no longer exist are skipped;
    /// capturing nothing leaves the previous contents alone, so a
    /// mis-aimed copy does not destroy what you had.
    pub fn copy_from(&mut self, doc: &ExpressionDoc, ids: &[NoteId]) -> bool {
        let mut notes: Vec<Note> = ids.iter().filter_map(|id| doc.note(*id).cloned()).collect();
        if notes.is_empty() {
            return false;
        }
        let t0 = notes.iter().map(|n| n.start).fold(f64::MAX, f64::min);
        let row0 = notes.iter().map(|n| n.row).min().unwrap_or(0);
        for n in &mut notes {
            n.shift_time(-t0);
            n.row -= row0;
        }
        notes.sort_by(|a, b| a.start.total_cmp(&b.start));
        self.notes = notes;
        self.origin_row = row0;
        true
    }

    /// The notes a paste at `(t, row)` would produce, with placeholder
    /// ids — [`crate::edit::Edit::PasteNotes`] mints the real ones.
    pub fn placed(&self, t: f64, row: i32) -> Vec<Note> {
        self.notes
            .iter()
            .map(|n| {
                let mut c = n.clone();
                c.shift_time(t);
                c.row = (c.row + row).clamp(0, 127);
                c
            })
            .collect()
    }
}
