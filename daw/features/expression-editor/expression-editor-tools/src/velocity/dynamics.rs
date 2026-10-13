//! COMPRESS / EXPAND — squeeze velocities toward a pivot, or throw them apart.
//!
//! A compressor for a MIDI performance rather than for audio: pick a
//! pivot velocity, then pull every note toward it (compress, flattening
//! the dynamics) or push every note away from it (expand, exaggerating
//! them). One bipolar control does both, because they're the same
//! operation with opposite sign and that's how it should feel under the
//! hand.
//!
//! ## Two divergences from the Lua
//!
//! **The labels are swapped upstream.** MVelocity draws "Expand" above
//! the slider's midpoint, but the branch it takes there is
//! `base + (target - base) * t` — which moves notes *toward* the target,
//! i.e. compresses. Below the midpoint it moves them away, i.e. expands.
//! We implement the arithmetic and name it for what it does:
//! [`Dynamics::amount`] is negative to compress, positive to expand.
//!
//! **FACTOR does nothing upstream.** The script draws a FACTOR button
//! beside TARGET and toggles a border on it, but no code path reads the
//! mode — both buttons run the same fixed-pivot math. Here FACTOR is
//! [`Pivot::Mean`]: the pivot is the selection's own average velocity, so
//! compressing pulls a phrase toward its own center instead of toward a
//! number you had to guess. That's what an audio compressor does, and
//! it's the mode you want on a take you didn't play yourself.

use super::{MAX_VELOCITY, MIN_VELOCITY, Note, Range, VelocityEdit, targets};

/// How far expansion throws a note, per unit of amount.
///
/// Two, from MVelocity's `expandFactor`. Expansion needs more travel
/// than compression to feel symmetric: compression has a hard floor (all
/// notes land on the pivot and can go no further), while expansion has
/// none, so matching their slider throws one-to-one makes expansion feel
/// inert next to compression.
const EXPAND_FACTOR: f64 = 2.0;

/// What velocities are squeezed toward or thrown away from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pivot {
    /// A fixed velocity — MVelocity's TARGET, dragged or scrolled.
    Fixed(u8),
    /// The mean velocity of the notes being edited — MVelocity's FACTOR
    /// button, given the meaning its name implies. Recomputed per apply,
    /// so it tracks the material.
    Mean,
}

impl Default for Pivot {
    /// 80, MVelocity's `currentValue`.
    fn default() -> Self {
        Self::Fixed(80)
    }
}

/// The compress/expand engine.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dynamics {
    /// -1.0 = every note collapsed onto the pivot, 0.0 = untouched,
    /// +1.0 = every note thrown twice its distance from the pivot
    /// further out (`EXPAND_FACTOR`, private — the curve's shape is this
    /// module's business, not API).
    pub amount: f64,
    pub pivot: Pivot,
}

impl Default for Dynamics {
    fn default() -> Self {
        Self {
            amount: 0.0,
            pivot: Pivot::default(),
        }
    }
}

impl Dynamics {
    pub fn new(amount: f64, pivot: Pivot) -> Self {
        Self {
            amount: amount.clamp(-1.0, 1.0),
            pivot,
        }
    }

    /// The pivot velocity this engine will use against `notes`.
    ///
    /// Exposed so the UI can show where [`Pivot::Mean`] actually landed —
    /// an invisible pivot is an unpredictable control.
    pub fn pivot_velocity(&self, notes: &[Note]) -> f64 {
        match self.pivot {
            Pivot::Fixed(v) => f64::from(v.clamp(MIN_VELOCITY, MAX_VELOCITY)),
            Pivot::Mean => {
                let (sum, count) = targets(notes).fold((0.0, 0u32), |(s, c), (_, n)| {
                    (s + f64::from(n.velocity), c + 1)
                });
                if count == 0 {
                    f64::from(MIN_VELOCITY)
                } else {
                    sum / f64::from(count)
                }
            }
        }
    }

    /// Compress or expand `notes` around the pivot.
    pub fn apply(&self, notes: &[Note], range: Range) -> Vec<VelocityEdit> {
        let amount = self.amount.clamp(-1.0, 1.0);
        let pivot = self.pivot_velocity(notes);
        targets(notes)
            .map(|(_, note)| {
                let base = f64::from(note.velocity);
                let distance = base - pivot;
                // One expression for both directions: compression scales
                // the distance down toward zero, expansion scales it up.
                let scaled = if amount >= 0.0 {
                    distance * (1.0 + amount * EXPAND_FACTOR)
                } else {
                    distance * (1.0 + amount)
                };
                VelocityEdit {
                    index: note.index,
                    velocity: range.clamp(pivot + scaled),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notes(vels: &[u8]) -> Vec<Note> {
        vels.iter()
            .enumerate()
            .map(|(i, &v)| Note::new(i as u32, v))
            .collect()
    }

    fn vels(edits: &[VelocityEdit]) -> Vec<u8> {
        edits.iter().map(|e| e.velocity).collect()
    }

    #[test]
    fn zero_amount_is_the_identity() {
        let ns = notes(&[20, 64, 100]);
        let out = Dynamics::new(0.0, Pivot::Fixed(80)).apply(&ns, Range::default());
        assert_eq!(vels(&out), [20, 64, 100]);
    }

    #[test]
    fn full_compression_collapses_everything_onto_the_pivot() {
        let ns = notes(&[20, 64, 100, 127]);
        let out = Dynamics::new(-1.0, Pivot::Fixed(80)).apply(&ns, Range::default());
        assert_eq!(vels(&out), [80, 80, 80, 80]);
    }

    #[test]
    fn half_compression_halves_the_distance_to_the_pivot() {
        let ns = notes(&[40, 120]);
        let out = Dynamics::new(-0.5, Pivot::Fixed(80)).apply(&ns, Range::default());
        assert_eq!(vels(&out), [60, 100]);
    }

    #[test]
    fn expansion_pushes_away_from_the_pivot() {
        // +0.5 => distance scaled by 1 + 0.5*2 = 2.0
        let ns = notes(&[70, 90]);
        let out = Dynamics::new(0.5, Pivot::Fixed(80)).apply(&ns, Range::default());
        assert_eq!(vels(&out), [60, 100]);
    }

    #[test]
    fn a_note_sitting_on_the_pivot_never_moves() {
        let ns = notes(&[80]);
        for amount in [-1.0, -0.3, 0.0, 0.4, 1.0] {
            let out = Dynamics::new(amount, Pivot::Fixed(80)).apply(&ns, Range::default());
            assert_eq!(vels(&out), [80], "amount {amount}");
        }
    }

    #[test]
    fn mean_pivot_centers_on_the_material() {
        let ns = notes(&[60, 100]); // mean 80
        let out = Dynamics::new(-1.0, Pivot::Mean).apply(&ns, Range::default());
        assert_eq!(vels(&out), [80, 80]);
    }

    #[test]
    fn mean_pivot_only_averages_the_selection() {
        let mut ns = notes(&[1, 40, 60, 127]);
        ns[1].selected = true;
        ns[2].selected = true;
        // mean of the *selected* pair is 50, not of all four (57).
        assert_eq!(Dynamics::new(0.0, Pivot::Mean).pivot_velocity(&ns), 50.0);
    }

    #[test]
    fn the_range_catches_expansion_overshoot() {
        let ns = notes(&[10, 120]);
        let out = Dynamics::new(1.0, Pivot::Fixed(80)).apply(&ns, Range::new(30, 110));
        assert_eq!(vels(&out), [30, 110]);
    }

    #[test]
    fn expansion_then_matching_compression_is_not_claimed_to_round_trip() {
        // Documenting the shape rather than asserting an inverse: these
        // are two independent scalings of the same distance, and integer
        // velocities lose precision in between. Callers get exact undo
        // from the baseline (see `Session`), not from inverting a knob.
        let ns = notes(&[64]);
        let expanded = Dynamics::new(0.5, Pivot::Fixed(80)).apply(&ns, Range::default());
        assert_ne!(vels(&expanded), [64]);
    }
}
