//! STEP VELOCITY — a cyclic velocity pattern, blended in by amount.
//!
//! The most musically useful of MVelocity's four sections: give it
//! `[100, 20, 90, 25]` and a hi-hat line gets an accent every fourth
//! note. The pattern repeats across the notes in order, so its length is
//! the rhythm — four steps over sixteenths accents the downbeat, three
//! steps over the same notes gives you a 3-against-4 lilt.
//!
//! `amount` is how far to go: 0.0 leaves the take alone, 1.0 replaces
//! every velocity with its step value, and everything between keeps the
//! performance's own dynamics underneath the pattern. That blend is the
//! point — hard-setting velocities flattens a real take, blending
//! stamps a shape onto it.

use super::{Note, Range, VelocityEdit, lerp, targets};

/// A repeating velocity pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    /// The steps, cycled over the notes in order. Never empty — see
    /// [`Pattern::new`].
    steps: Vec<u8>,
}

impl Default for Pattern {
    /// MVelocity's shipped default: a strong-weak-strong-weak four.
    fn default() -> Self {
        Self {
            steps: vec![100, 20, 90, 25],
        }
    }
}

impl Pattern {
    /// Build a pattern from `steps`, clamping each into 1..=127.
    ///
    /// An empty `steps` yields the default rather than an empty pattern:
    /// the UI can delete steps down to nothing, and "no steps" has no
    /// meaningful reading — a zero-length cycle can't index.
    pub fn new(steps: impl IntoIterator<Item = u8>) -> Self {
        let steps: Vec<u8> = steps
            .into_iter()
            .map(|v| v.clamp(super::MIN_VELOCITY, super::MAX_VELOCITY))
            .collect();
        if steps.is_empty() {
            Self::default()
        } else {
            Self { steps }
        }
    }

    pub fn steps(&self) -> &[u8] {
        &self.steps
    }

    pub fn len(&self) -> usize {
        self.steps.len()
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    /// Append a step, seeded from the last one so a new slot starts
    /// somewhere sensible instead of at zero.
    pub fn push(&mut self) {
        let seed = self.steps.last().copied().unwrap_or(100);
        self.steps.push(seed);
    }

    /// Drop the last step. A no-op at one step — see [`Pattern::new`].
    pub fn pop(&mut self) {
        if self.steps.len() > 1 {
            self.steps.pop();
        }
    }

    /// Set step `i`, clamped. Out-of-bounds `i` is ignored.
    pub fn set(&mut self, i: usize, velocity: u8) {
        if let Some(step) = self.steps.get_mut(i) {
            *step = velocity.clamp(super::MIN_VELOCITY, super::MAX_VELOCITY);
        }
    }

    /// The step that applies to the note at position `ordinal`.
    pub fn step_for(&self, ordinal: usize) -> u8 {
        self.steps[ordinal % self.steps.len()]
    }

    /// Blend this pattern into `notes` by `amount` (0.0..=1.0).
    ///
    /// `notes` must carry each note's *baseline* velocity, not its
    /// current one — see the module docs on non-destructiveness.
    pub fn apply(&self, notes: &[Note], amount: f64, range: Range) -> Vec<VelocityEdit> {
        let amount = amount.clamp(0.0, 1.0);
        targets(notes)
            .map(|(ordinal, note)| VelocityEdit {
                index: note.index,
                velocity: range.clamp(lerp(
                    f64::from(note.velocity),
                    f64::from(self.step_for(ordinal)),
                    amount,
                )),
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

    #[test]
    fn zero_amount_is_the_identity() {
        let notes = notes(&[64, 70, 80, 90, 100]);
        let out = Pattern::default().apply(&notes, 0.0, Range::default());
        let vels: Vec<u8> = out.iter().map(|e| e.velocity).collect();
        assert_eq!(vels, [64, 70, 80, 90, 100]);
    }

    #[test]
    fn full_amount_replaces_with_the_cycled_steps() {
        let notes = notes(&[64; 6]);
        let out = Pattern::new([100, 20, 90, 25]).apply(&notes, 1.0, Range::default());
        let vels: Vec<u8> = out.iter().map(|e| e.velocity).collect();
        assert_eq!(vels, [100, 20, 90, 25, 100, 20]);
    }

    #[test]
    fn half_amount_sits_between_baseline_and_step() {
        let notes = notes(&[60, 60]);
        let out = Pattern::new([100, 20]).apply(&notes, 0.5, Range::default());
        let vels: Vec<u8> = out.iter().map(|e| e.velocity).collect();
        assert_eq!(vels, [80, 40]);
    }

    #[test]
    fn the_range_clamps_the_result() {
        let notes = notes(&[64, 64]);
        let out = Pattern::new([127, 1]).apply(&notes, 1.0, Range::new(40, 90));
        let vels: Vec<u8> = out.iter().map(|e| e.velocity).collect();
        assert_eq!(vels, [90, 40]);
    }

    #[test]
    fn steps_are_keyed_to_position_in_the_take_not_in_the_selection() {
        // Selecting notes 2 and 3 of a 4-note bar must still give them
        // steps 2 and 3 — otherwise nudging a selection re-phases the
        // whole pattern, which is exactly what you don't want mid-edit.
        let mut ns = notes(&[64; 4]);
        ns[2].selected = true;
        ns[3].selected = true;
        let out = Pattern::new([10, 20, 30, 40]).apply(&ns, 1.0, Range::default());
        assert_eq!(out.len(), 2);
        assert_eq!(
            out[0],
            VelocityEdit {
                index: 2,
                velocity: 30
            }
        );
        assert_eq!(
            out[1],
            VelocityEdit {
                index: 3,
                velocity: 40
            }
        );
    }

    #[test]
    fn a_pattern_always_has_at_least_one_step() {
        assert_eq!(Pattern::new([]).len(), 4); // falls back to the default
        let mut p = Pattern::new([64]);
        p.pop();
        assert_eq!(p.steps(), [64]);
    }

    #[test]
    fn push_seeds_from_the_last_step() {
        let mut p = Pattern::new([100, 20]);
        p.push();
        assert_eq!(p.steps(), [100, 20, 20]);
    }
}
