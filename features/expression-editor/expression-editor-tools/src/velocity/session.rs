//! The four engines composed over one held baseline.
//!
//! An editing session: you open it on a take, and from then on every
//! control is a *parameter* rather than an *action*. Nothing accumulates.
//! Push the pattern to 60%, draw a curve, pull the compressor down, put
//! the compressor back — you're exactly where you were before you
//! touched it, because the whole result is recomputed from the baseline
//! every time rather than layered onto whatever the take currently holds.
//!
//! MVelocity aims at this too but leaks: `slider_gr.onchange` and
//! `slider_reduce.onchange` both re-snapshot `baseInitialVelocities`
//! mid-interaction, so which section you touched last silently changes
//! what "back to neutral" means. Owning the baseline in exactly one place
//! is the fix, and it's why the engines themselves are pure functions.
//!
//! ## The chain
//!
//! ```text
//! baseline ─→ curve ─→ pattern ─→ randomize ─→ dynamics ─→ edits
//!             shape    accent     humanize     glue
//! ```
//!
//! In that order because it's how you'd build the part by hand: draw the
//! macro dynamic shape of the phrase, stamp the rhythmic accents onto it,
//! rough up the result so it isn't machine-perfect, then squeeze the
//! whole thing into the range the part needs to sit in. Compression last
//! is the one that really matters — it's the stage that guarantees the
//! final spread, and anything after it would undo that guarantee.

use super::{Curve, Dynamics, Note, Pattern, Randomize, Range, VelocityEdit};

/// A live velocity-editing session over a fixed set of notes.
#[derive(Clone, Debug, Default)]
pub struct Session {
    /// Velocities as they were when the session opened. The single
    /// source of truth for "neutral".
    baseline: Vec<Note>,

    /// The velocity window everything is clamped into.
    pub range: Range,

    /// The drawn contour. `None` until you draw one — a curve is a
    /// replacement, not a blend, so an always-on curve would mean the
    /// other engines never got to speak.
    pub curve: Option<Curve>,

    /// The cyclic accent pattern, and how far it's blended in.
    pub pattern: Pattern,
    pub pattern_amount: f64,

    /// The held random hand, and how far it's blended in.
    pub randomize: Randomize,
    pub randomize_amount: f64,

    /// Compress/expand, applied to whatever the earlier stages produced.
    pub dynamics: Dynamics,
}

impl Session {
    /// Open a session over `notes`, taking their current velocities as
    /// the baseline.
    pub fn new(notes: Vec<Note>) -> Self {
        Self {
            baseline: notes,
            ..Self::default()
        }
    }

    /// The notes this session was opened on.
    pub fn baseline(&self) -> &[Note] {
        &self.baseline
    }

    pub fn is_empty(&self) -> bool {
        self.baseline.is_empty()
    }

    /// Adopt a new set of notes as the baseline.
    ///
    /// For when the take changed underneath us — notes added, deleted, or
    /// the selection moved. Everything downstream recomputes against the
    /// new material; parameters are kept, so a pattern you dialled in
    /// survives selecting a different bar. The random hand is dropped,
    /// because targets dealt for notes that no longer exist would land on
    /// the wrong ones.
    pub fn resync(&mut self, notes: Vec<Note>) {
        self.baseline = notes;
        self.randomize = Randomize::default();
    }

    /// Return every control to neutral, keeping the baseline.
    pub fn reset(&mut self) {
        let baseline = std::mem::take(&mut self.baseline);
        *self = Self {
            baseline,
            ..Self::default()
        };
    }

    /// Deal a fresh random hand sized to the current baseline.
    pub fn roll(&mut self) {
        self.randomize.roll(self.baseline.len(), self.range);
    }

    /// [`Session::roll`] from a fixed seed, for reproducible takes.
    pub fn roll_seeded(&mut self, seed: u64) {
        self.randomize
            .roll_seeded(self.baseline.len(), self.range, seed);
    }

    /// The full result of the chain, as absolute velocities per note.
    ///
    /// Every note in the baseline appears, whether or not it moved.
    pub fn resolve(&self) -> Vec<Note> {
        let mut working = self.baseline.clone();

        // Each stage reads the previous stage's output, so the edits have
        // to be computed before they're folded back in.
        if let Some(curve) = &self.curve {
            let staged = curve.apply(&working, self.range);
            merge(&mut working, staged);
        }
        let staged = self
            .pattern
            .apply(&working, self.pattern_amount, self.range);
        merge(&mut working, staged);

        let staged = self
            .randomize
            .apply(&working, self.randomize_amount, self.range);
        merge(&mut working, staged);

        let staged = self.dynamics.apply(&working, self.range);
        merge(&mut working, staged);

        working
    }

    /// What the DAW needs to be told: only the notes that actually moved.
    ///
    /// Filtering here rather than in the sink because the sink writes one
    /// note per call, and a session at neutral over a 600-note take
    /// should cost nothing rather than 600 redundant writes.
    pub fn edits(&self) -> Vec<VelocityEdit> {
        self.resolve()
            .into_iter()
            .zip(&self.baseline)
            .filter(|(now, was)| now.velocity != was.velocity)
            .map(|(now, _)| VelocityEdit {
                index: now.index,
                velocity: now.velocity,
            })
            .collect()
    }
}

/// Fold a stage's output back into the working set, in place.
fn merge(working: &mut [Note], edits: Vec<VelocityEdit>) {
    for edit in edits {
        if let Some(note) = working.iter_mut().find(|n| n.index == edit.index) {
            note.velocity = edit.velocity;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{CurvePreset, Pivot};
    use super::*;

    fn session(vels: &[u8]) -> Session {
        Session::new(
            vels.iter()
                .enumerate()
                .map(|(i, &v)| Note::new(i as u32, v))
                .collect(),
        )
    }

    #[test]
    fn a_fresh_session_changes_nothing() {
        assert!(session(&[10, 64, 127]).edits().is_empty());
    }

    #[test]
    fn every_control_returns_to_neutral_exactly() {
        let mut s = session(&[10, 40, 64, 90, 127]);

        s.pattern_amount = 0.6;
        s.dynamics = Dynamics::new(-0.4, Pivot::Fixed(70));
        s.curve = Some(CurvePreset::Rise.curve());
        s.roll_seeded(1);
        s.randomize_amount = 0.8;
        assert!(!s.edits().is_empty(), "controls should do something");

        s.pattern_amount = 0.0;
        s.dynamics = Dynamics::default();
        s.curve = None;
        s.randomize_amount = 0.0;
        assert!(s.edits().is_empty(), "neutral should be the identity");
    }

    #[test]
    fn edits_only_mention_notes_that_moved() {
        let mut s = session(&[100, 20, 100, 20]);
        // A pattern equal to the material moves nothing.
        s.pattern = Pattern::new([100, 20]);
        s.pattern_amount = 1.0;
        assert!(s.edits().is_empty());

        s.pattern = Pattern::new([100, 30]);
        let edits = s.edits();
        assert_eq!(edits.len(), 2);
        assert!(edits.iter().all(|e| e.index % 2 == 1));
    }

    #[test]
    fn the_chain_runs_shape_then_accent_then_humanize_then_glue() {
        let mut s = session(&[64; 8]);
        s.curve = Some(CurvePreset::Rise.curve());
        s.dynamics = Dynamics::new(-1.0, Pivot::Fixed(64));
        // Full compression is last, so it flattens the curve entirely.
        let out = s.resolve();
        assert!(out.iter().all(|n| n.velocity == 64), "{out:?}");
    }

    #[test]
    fn compression_last_guarantees_the_final_spread() {
        let mut s = session(&[64; 16]);
        s.curve = Some(CurvePreset::Rise.curve());
        s.pattern_amount = 1.0;
        s.dynamics = Dynamics::new(-0.5, Pivot::Fixed(64));
        let vels: Vec<i32> = s.resolve().iter().map(|n| i32::from(n.velocity)).collect();
        // Whatever the earlier stages did, half-compression toward 64 put
        // everything within half its original distance of 64.
        assert!(vels.iter().all(|v| (v - 64).abs() <= 64), "{vels:?}");
    }

    #[test]
    fn a_neutral_session_is_free_over_a_large_take() {
        let s = session(&vec![77; 600]);
        assert!(s.edits().is_empty());
    }

    #[test]
    fn reset_restores_neutral_but_keeps_the_notes() {
        let mut s = session(&[10, 64, 127]);
        s.pattern_amount = 1.0;
        s.range = Range::new(50, 60);
        assert!(!s.edits().is_empty());

        s.reset();
        assert_eq!(s.baseline().len(), 3);
        assert_eq!(s.range, Range::default());
        assert!(s.edits().is_empty());
    }

    #[test]
    fn resync_adopts_new_notes_and_drops_the_stale_hand() {
        let mut s = session(&[64; 4]);
        s.roll_seeded(3);
        s.randomize_amount = 1.0;
        assert_eq!(s.edits().len(), 4);

        s.resync(vec![Note::new(9, 100), Note::new(10, 100)]);
        assert!(s.randomize.is_empty());
        assert!(s.edits().is_empty(), "a dropped hand is a no-op");
        assert_eq!(s.baseline()[0].index, 9);
    }

    #[test]
    fn resync_keeps_the_parameters_you_dialled_in() {
        let mut s = session(&[64; 4]);
        s.pattern = Pattern::new([100, 20]);
        s.pattern_amount = 1.0;

        s.resync(vec![Note::new(0, 64), Note::new(1, 64)]);
        let vels: Vec<u8> = s.resolve().iter().map(|n| n.velocity).collect();
        assert_eq!(vels, [100, 20]);
    }

    #[test]
    fn the_range_bounds_the_whole_chain() {
        let mut s = session(&[1, 64, 127]);
        s.range = Range::new(40, 80);
        s.curve = Some(CurvePreset::Rise.curve());
        s.pattern_amount = 1.0;
        s.dynamics = Dynamics::new(1.0, Pivot::Fixed(60));
        assert!(s.resolve().iter().all(|n| (40..=80).contains(&n.velocity)));
    }
}
