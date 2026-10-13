//! Align on the tool seam (#154).
//!
//! The point of the tool is that both sides are only `Timed`, so a MIDI
//! part aligns to audio and audio to MIDI with no special case. These
//! use two deliberately different event types to keep that honest.

use expression_editor_tools::align::{AlignConfig, plan_align, plan_align_sustained};
use expression_editor_tools::event::{Sustained, Timed, length_of};
use expression_editor_tools::quantize::apply;

/// Stands in for an audio transient: a moment, no length.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Hit(f64);

impl Timed for Hit {
    fn onset(&self) -> f64 {
        self.0
    }
    fn move_to(&mut self, to: f64) {
        self.0 = to;
    }
    fn strength(&self) -> f64 {
        1.0
    }
}

/// Stands in for a MIDI note: a span that carries its length.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Note {
    start: f64,
    len: f64,
}

impl Timed for Note {
    fn onset(&self) -> f64 {
        self.start
    }
    fn move_to(&mut self, to: f64) {
        self.start = to;
    }
    fn strength(&self) -> f64 {
        1.0
    }
    fn length(&self) -> Option<f64> {
        length_of(self)
    }
}

impl Sustained for Note {
    fn end(&self) -> f64 {
        self.start + self.len
    }
}

fn cfg(window: f64, strength: f64) -> AlignConfig {
    AlignConfig { window, strength }
}

#[test]
fn a_target_is_pulled_onto_its_nearest_reference() {
    let mut targets = vec![Hit(103.0), Hit(199.0)];
    let reference = vec![Hit(100.0), Hit(200.0)];

    let plan = plan_align(&targets, &reference, cfg(20.0, 1.0));
    apply(&mut targets, &plan);

    assert_eq!(targets, vec![Hit(100.0), Hit(200.0)]);
}

#[test]
fn strength_is_a_partial_pull_not_a_clone() {
    // The setting that tightens a double without making it identical.
    let mut targets = vec![Hit(110.0)];
    let plan = plan_align(&targets, &[Hit(100.0)], cfg(50.0, 0.5));
    apply(&mut targets, &plan);
    assert_eq!(targets, vec![Hit(105.0)]);
}

#[test]
fn strength_zero_moves_nothing() {
    let mut targets = vec![Hit(110.0)];
    let plan = plan_align(&targets, &[Hit(100.0)], cfg(50.0, 0.0));
    apply(&mut targets, &plan);
    assert_eq!(targets, vec![Hit(110.0)]);
}

#[test]
fn a_target_outside_the_window_is_left_alone_and_reported() {
    // Not dragged somewhere: "why did that not move" is the first
    // question a user asks, and the answer has to be available.
    let mut targets = vec![Hit(100.0), Hit(5000.0)];
    let plan = plan_align(&targets, &[Hit(100.0)], cfg(20.0, 1.0));

    assert_eq!(plan.moves.len(), 1);
    assert_eq!(plan.unmatched, vec![5000.0]);

    apply(&mut targets, &plan);
    assert_eq!(targets[1], Hit(5000.0), "the far one stayed put");
}

#[test]
fn each_reference_takes_at_most_one_target() {
    // A flam must not collapse onto one reference hit — that would
    // destroy the exact timing detail being aligned.
    let mut targets = vec![Hit(98.0), Hit(102.0)];
    let plan = plan_align(&targets, &[Hit(100.0)], cfg(20.0, 1.0));

    assert_eq!(plan.moves.len(), 1, "only the nearest was claimed");
    assert_eq!(plan.unmatched.len(), 1);

    apply(&mut targets, &plan);
    assert_ne!(targets[0], targets[1], "the flam survived");
}

#[test]
fn the_nearest_target_wins_a_contested_reference() {
    let plan = plan_align(&[Hit(90.0), Hit(101.0)], &[Hit(100.0)], cfg(50.0, 1.0));
    assert_eq!(plan.moves.len(), 1);
    assert_eq!(plan.moves[0].index, 1, "101 is nearer than 90");
}

#[test]
fn midi_aligns_to_audio_and_keeps_its_lengths() {
    // The genericity the tool exists to prove: a Note target against a
    // Hit reference, with no special case for either.
    let mut targets = vec![
        Note {
            start: 103.0,
            len: 40.0,
        },
        Note {
            start: 198.0,
            len: 60.0,
        },
    ];
    let reference = vec![Hit(100.0), Hit(200.0)];

    let plan = plan_align_sustained(&targets, &reference, cfg(20.0, 1.0));
    assert_eq!(plan.moves[0].length, Some(40.0));

    apply(&mut targets, &plan);
    assert_eq!(targets[0].start, 100.0);
    assert_eq!(targets[0].len, 40.0, "a note keeps its length");
    assert_eq!(targets[1].start, 200.0);
    assert_eq!(targets[1].len, 60.0);
}

#[test]
fn audio_aligns_to_midi_too() {
    // The other direction, which the old audio-typed functions could
    // not express at all.
    let mut targets = vec![Hit(97.0)];
    let reference = vec![Note {
        start: 100.0,
        len: 10.0,
    }];
    let plan = plan_align(&targets, &reference, cfg(20.0, 1.0));
    apply(&mut targets, &plan);
    assert_eq!(targets, vec![Hit(100.0)]);
}

#[test]
fn no_reference_means_nothing_moves() {
    let targets = vec![Hit(100.0), Hit(200.0)];
    let plan = plan_align(&targets, &[] as &[Hit], cfg(20.0, 1.0));
    assert!(plan.moves.is_empty());
    assert_eq!(plan.unmatched.len(), 2, "and both are accounted for");
}

#[test]
fn no_window_considers_every_reference() {
    // window 0 means unlimited, matching quantize's "no tolerance"
    // reading: every event snaps to its own nearest.
    let plan = plan_align(&[Hit(5000.0)], &[Hit(100.0)], cfg(0.0, 1.0));
    assert_eq!(plan.moves.len(), 1);
}

#[test]
fn a_plan_reports_the_reference_it_matched() {
    // So a preview can draw the pairing, not just the movement.
    let plan = plan_align(&[Hit(103.0)], &[Hit(100.0)], cfg(20.0, 0.5));
    assert_eq!(plan.moves[0].division, 100.0, "the reference position");
    assert_eq!(plan.moves[0].to, 101.5, "and where it actually lands");
}
