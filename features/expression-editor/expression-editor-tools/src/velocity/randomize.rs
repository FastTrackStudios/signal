//! RANGE + UPDATE — humanize by rolling a target per note, then blending.
//!
//! The subtlety worth preserving from MVelocity: the random targets are
//! **rolled once and held**, not re-rolled on every parameter change.
//! Pressing UPDATE deals a new hand; dragging the amount slider walks
//! toward that same hand. If the targets re-rolled per frame the slider
//! would jitter under your hand and you could never audition a
//! particular randomization — you'd just be shaking the notes.
//!
//! So [`Randomize`] is stateful (it owns the dealt targets) while the
//! blend stays a pure function of `amount`.

use rand::Rng;
use rand::SeedableRng;
use rand::rngs::StdRng;

use super::{Note, Range, VelocityEdit, lerp, targets};

/// A held set of random target velocities, one per note.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Randomize {
    /// Indexed by note ordinal, parallel to the slice passed to
    /// [`Randomize::apply`].
    dealt: Vec<u8>,
}

impl Randomize {
    /// Deal `count` fresh targets uniformly inside `range`.
    ///
    /// This is the UPDATE button.
    pub fn roll(&mut self, count: usize, range: Range) {
        self.roll_with(count, range, &mut StdRng::from_entropy());
    }

    /// [`Randomize::roll`] against a caller-supplied RNG, so tests (and
    /// anything wanting a reproducible take) can pin the hand.
    pub fn roll_with(&mut self, count: usize, range: Range, rng: &mut impl Rng) {
        self.dealt.clear();
        self.dealt
            .extend((0..count).map(|_| rng.gen_range(range.min..=range.max)));
    }

    /// Deal from a fixed seed. Two calls with the same seed, count and
    /// range give the same hand.
    pub fn roll_seeded(&mut self, count: usize, range: Range, seed: u64) {
        self.roll_with(count, range, &mut StdRng::seed_from_u64(seed));
    }

    /// The targets currently held, by note ordinal.
    pub fn dealt(&self) -> &[u8] {
        &self.dealt
    }

    pub fn is_empty(&self) -> bool {
        self.dealt.is_empty()
    }

    /// Blend each note from its baseline toward its dealt target.
    ///
    /// Notes past the end of the dealt hand are left alone rather than
    /// dealt a target on the fly: a take that grew since the last UPDATE
    /// should wait for the next one, not have its new notes randomized
    /// behind your back.
    pub fn apply(&self, notes: &[Note], amount: f64, range: Range) -> Vec<VelocityEdit> {
        let amount = amount.clamp(0.0, 1.0);
        targets(notes)
            .filter_map(|(ordinal, note)| {
                let target = *self.dealt.get(ordinal)?;
                Some(VelocityEdit {
                    index: note.index,
                    velocity: range.clamp(lerp(
                        f64::from(note.velocity),
                        f64::from(target),
                        amount,
                    )),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notes(n: usize) -> Vec<Note> {
        (0..n).map(|i| Note::new(i as u32, 64)).collect()
    }

    #[test]
    fn dealt_targets_stay_inside_the_range() {
        let mut r = Randomize::default();
        r.roll_seeded(200, Range::new(40, 80), 7);
        assert!(r.dealt().iter().all(|&v| (40..=80).contains(&v)));
    }

    #[test]
    fn the_same_seed_deals_the_same_hand() {
        let (mut a, mut b) = (Randomize::default(), Randomize::default());
        a.roll_seeded(32, Range::default(), 42);
        b.roll_seeded(32, Range::default(), 42);
        assert_eq!(a, b);
    }

    #[test]
    fn zero_amount_is_the_identity_even_with_a_hand_dealt() {
        let mut r = Randomize::default();
        r.roll_seeded(8, Range::default(), 1);
        let ns = notes(8);
        let out = r.apply(&ns, 0.0, Range::default());
        assert!(out.iter().all(|e| e.velocity == 64));
    }

    #[test]
    fn full_amount_lands_exactly_on_the_dealt_targets() {
        let mut r = Randomize::default();
        r.roll_seeded(8, Range::new(20, 100), 3);
        let out = r.apply(&notes(8), 1.0, Range::default());
        let vels: Vec<u8> = out.iter().map(|e| e.velocity).collect();
        assert_eq!(vels, r.dealt());
    }

    #[test]
    fn dragging_the_amount_does_not_re_roll() {
        let mut r = Randomize::default();
        r.roll_seeded(8, Range::default(), 5);
        let ns = notes(8);
        let first = r.apply(&ns, 0.5, Range::default());
        let second = r.apply(&ns, 0.5, Range::default());
        assert_eq!(first, second);
    }

    #[test]
    fn notes_added_since_the_last_roll_are_left_alone() {
        let mut r = Randomize::default();
        r.roll_seeded(4, Range::default(), 9);
        let out = r.apply(&notes(6), 1.0, Range::default());
        assert_eq!(out.len(), 4);
    }

    #[test]
    fn an_undealt_randomize_is_a_no_op() {
        let r = Randomize::default();
        assert!(r.apply(&notes(4), 1.0, Range::default()).is_empty());
    }
}
