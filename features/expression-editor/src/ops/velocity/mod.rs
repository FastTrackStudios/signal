//! Velocity shaping — four independent engines over one note selection.
//!
//! Ported from mrtnz's MVelocity (ReaperScripts, `MIDI Editor/
//! mrtnz_MVelocity.lua`), which is a REAPER Lua script with the DAW calls
//! and the rtk widget tree braided together. Here they're pulled apart:
//! this module is arithmetic over `&[Note]`, knows nothing about REAPER,
//! Dioxus, or `daw`, and every engine is a pure function you can unit
//! test. `midi-tools-daw` supplies the notes and writes the result back;
//! `midi-tools-ui` drives the parameters.
//!
//! ## The four engines
//!
//! | module       | MVelocity section  | what it does                          |
//! |--------------|--------------------|---------------------------------------|
//! | [`pattern`]  | STEP VELOCITY      | cycles an N-slot velocity pattern     |
//! | [`randomize`]| RANGE + UPDATE     | random per-note target, morphed into  |
//! | [`dynamics`] | COMPRESS / EXPAND  | pulls toward or pushes away from a pivot |
//! | [`curve`]    | the Bézier widget  | draws a velocity ramp across the span |
//!
//! ## The one invariant they share
//!
//! Every engine is **non-destructive in the parameter**: it maps a
//! *baseline* velocity to a new one, so moving a slider to 40 and back to
//! 0 restores exactly what you started with. That's why nothing here
//! reads the note's *current* velocity — the caller holds the baseline
//! (see [`Session`]) and the engines are pure. The Lua does this too, via
//! a `baseInitialVelocities` array, but leaks: several of its handlers
//! re-snapshot the baseline mid-drag, so dragging back doesn't always
//! land where you began. Keeping the baseline in one owned place is the
//! fix.

pub mod curve;
pub mod dynamics;
pub mod pattern;
pub mod randomize;
mod session;

pub use curve::{Curve, CurvePreset, Point};
pub use dynamics::{Dynamics, Pivot};
pub use pattern::Pattern;
pub use randomize::Randomize;
pub use session::Session;

/// The lowest velocity a note may be given.
///
/// One, not zero: a zero-velocity note-on *is* a note-off in MIDI, so
/// clamping to 0 would silently delete notes.
pub const MIN_VELOCITY: u8 = 1;

/// The highest velocity a note may be given.
pub const MAX_VELOCITY: u8 = 127;

/// A note as the velocity engines see it.
///
/// Deliberately not `daw::MidiNote` — pitch, position and length are
/// irrelevant to every engine here, and depending on `daw-proto` for a
/// struct we'd use three fields of would put a DAW dependency in the one
/// crate that must not have one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Note {
    /// Index of the note within its take. Opaque here; it's the handle
    /// the sink writes back through.
    pub index: u32,
    /// The note's velocity at the moment the [`Session`] was opened.
    pub velocity: u8,
    /// Whether the note is selected in the editor.
    pub selected: bool,
}

impl Note {
    pub fn new(index: u32, velocity: u8) -> Self {
        Self {
            index,
            velocity,
            selected: false,
        }
    }

    pub fn selected(index: u32, velocity: u8) -> Self {
        Self {
            index,
            velocity,
            selected: true,
        }
    }
}

/// One note's new velocity. What an engine produces and a sink consumes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VelocityEdit {
    pub index: u32,
    pub velocity: u8,
}

/// The inclusive velocity window every engine clamps its output into.
///
/// MVelocity's RANGE slider, which is shared across the whole tool rather
/// than owned by any one section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    pub min: u8,
    pub max: u8,
}

impl Default for Range {
    fn default() -> Self {
        Self {
            min: MIN_VELOCITY,
            max: MAX_VELOCITY,
        }
    }
}

impl Range {
    /// Build a range, ordering the bounds and pinning them to 1..=127.
    ///
    /// Takes whatever a dragged slider hands it rather than rejecting
    /// inverted input — a UI that crosses its two thumbs means "the
    /// window between them", not "an error".
    pub fn new(min: u8, max: u8) -> Self {
        let (lo, hi) = if min <= max { (min, max) } else { (max, min) };
        Self {
            min: lo.clamp(MIN_VELOCITY, MAX_VELOCITY),
            max: hi.clamp(MIN_VELOCITY, MAX_VELOCITY),
        }
    }

    /// Round `v` to the nearest integer velocity inside this range.
    ///
    /// Rounds rather than truncating. The Lua floors, which biases every
    /// engine's output half a step low and makes a 50% blend between 100
    /// and 101 read as 100 — invisible on one note, a visible tilt across
    /// a ramp.
    pub fn clamp(&self, v: f64) -> u8 {
        let r = v.round();
        if r <= f64::from(self.min) {
            self.min
        } else if r >= f64::from(self.max) {
            self.max
        } else {
            r as u8
        }
    }
}

/// Which notes an engine should touch.
///
/// MVelocity's rule, and a good one: operate on the selection if there is
/// one, otherwise on everything. It means the tool is useful without
/// making you select first, but never surprises you by ignoring a
/// selection you did make.
pub fn targets(notes: &[Note]) -> impl Iterator<Item = (usize, &Note)> {
    let any_selected = notes.iter().any(|n| n.selected);
    notes
        .iter()
        .enumerate()
        .filter(move |(_, n)| n.selected || !any_selected)
}

/// Linear blend from `from` to `to` by `amount` in 0.0..=1.0.
pub(crate) fn lerp(from: f64, to: f64, amount: f64) -> f64 {
    from + (to - from) * amount
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_orders_and_pins_its_bounds() {
        assert_eq!(Range::new(120, 10), Range { min: 10, max: 120 });
        assert_eq!(Range::new(0, 200), Range { min: 1, max: 127 });
    }

    #[test]
    fn clamp_rounds_to_nearest() {
        let r = Range::default();
        assert_eq!(r.clamp(100.4), 100);
        assert_eq!(r.clamp(100.5), 101);
        assert_eq!(r.clamp(-5.0), 1);
        assert_eq!(r.clamp(999.0), 127);
    }

    #[test]
    fn targets_are_everything_when_nothing_is_selected() {
        let notes = [Note::new(0, 64), Note::new(1, 64), Note::new(2, 64)];
        assert_eq!(targets(&notes).count(), 3);
    }

    #[test]
    fn targets_narrow_to_the_selection_when_there_is_one() {
        let notes = [Note::new(0, 64), Note::selected(1, 64), Note::new(2, 64)];
        let picked: Vec<u32> = targets(&notes).map(|(_, n)| n.index).collect();
        assert_eq!(picked, [1]);
    }

    #[test]
    fn targets_keep_their_position_in_the_full_note_list() {
        // The ordinal matters: `pattern` indexes its steps by it, so a
        // selection must not renumber from zero.
        let notes = [
            Note::new(0, 64),
            Note::selected(1, 64),
            Note::selected(2, 64),
        ];
        let ordinals: Vec<usize> = targets(&notes).map(|(i, _)| i).collect();
        assert_eq!(ordinals, [1, 2]);
    }
}
