//! Folding a run of notes back into chords.
//!
//! A MIDI take is a flat list of notes; an arpeggiator needs to know
//! which of them were struck together. That's a grouping problem, and
//! getting it wrong is the difference between arpeggiating a progression
//! and arpeggiating one enormous cluster.

use crate::velocity::{MAX_VELOCITY, MIN_VELOCITY};

/// A note as the grouper sees it — position and pitch, no take index.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimedNote {
    pub start_ppq: f64,
    pub length_ppq: f64,
    pub pitch: u8,
    pub velocity: u8,
}

impl TimedNote {
    pub fn end_ppq(&self) -> f64 {
        self.start_ppq + self.length_ppq
    }
}

/// One note of a chord.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChordNote {
    pub pitch: u8,
    pub velocity: u8,
}

/// Notes struck together, and the span they occupy.
///
/// `notes` is always sorted low to high. Sorting here rather than per
/// direction is what lets [`super::Cursor`] be a pure index sequence:
/// "up" is ascending index, and nothing downstream has to re-sort.
#[derive(Clone, Debug, PartialEq)]
pub struct Chord {
    pub start_ppq: f64,
    pub end_ppq: f64,
    pub notes: Vec<ChordNote>,
}

impl Chord {
    pub fn length_ppq(&self) -> f64 {
        self.end_ppq - self.start_ppq
    }
}

/// Default slop for "struck together", in PPQ.
///
/// A 32nd note at 960 PPQ. Generous on purpose: a played chord is never
/// perfectly simultaneous, and the alternative — treating a hand-rolled
/// chord as four separate one-note "chords" — turns an arpeggio into four
/// stuttering repeats. Upstream uses 10 ticks, which is about a
/// millisecond and only groups quantized input.
pub const DEFAULT_GAP_PPQ: f64 = 120.0;

/// Group `notes` into chords by **onset**.
///
/// A note joins the current chord if it starts within `gap_ppq` of that
/// chord's *first* note; otherwise it starts a new one. Input need not be
/// sorted.
///
/// ## Why onsets and not "is the chord still sounding"
///
/// Grouping by whether the previous chord has ended is the tempting rule,
/// and it's what upstream does (`startpos - chord[#chord].endpos > 10`).
/// It's wrong for the most ordinary input there is: a block-chord
/// progression, where each chord is held right up to the next one. Am
/// ending exactly where F begins reads as a single eight-note cluster,
/// and the arpeggio comes out as one run over both chords instead of two.
///
/// The cost is that a note struck *later* over a still-held chord — a
/// melody note over a sustained bass — becomes its own chord rather than
/// joining. That's the right trade: it's rarer, it's genuinely ambiguous,
/// and being predictable matters more here than being clever. Widen
/// `gap_ppq` if you want a rolled chord to group anyway.
pub fn group_chords(notes: &[TimedNote], gap_ppq: f64) -> Vec<Chord> {
    let mut sorted: Vec<TimedNote> = notes.to_vec();
    sorted.sort_by(|a, b| {
        a.start_ppq
            .partial_cmp(&b.start_ppq)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.pitch.cmp(&b.pitch))
    });

    let mut chords: Vec<Chord> = Vec::new();
    for note in sorted {
        match chords.last_mut() {
            // Against the chord's own onset, so how *long* the chord is
            // held has no bearing on what belongs to it. See the fn docs.
            Some(chord) if note.start_ppq <= chord.start_ppq + gap_ppq => {
                chord.end_ppq = chord.end_ppq.max(note.end_ppq());
                chord.notes.push(ChordNote {
                    pitch: note.pitch,
                    velocity: note.velocity.clamp(MIN_VELOCITY, MAX_VELOCITY),
                });
            }
            _ => chords.push(Chord {
                start_ppq: note.start_ppq,
                end_ppq: note.end_ppq(),
                notes: vec![ChordNote {
                    pitch: note.pitch,
                    velocity: note.velocity.clamp(MIN_VELOCITY, MAX_VELOCITY),
                }],
            }),
        }
    }

    for chord in &mut chords {
        chord.notes.sort_by_key(|n| n.pitch);
        chord.notes.dedup_by_key(|n| n.pitch);
    }
    chords
}

#[cfg(test)]
mod tests {
    use super::super::PPQ;
    use super::*;

    fn note(start: f64, len: f64, pitch: u8) -> TimedNote {
        TimedNote {
            start_ppq: start,
            length_ppq: len,
            pitch,
            velocity: 96,
        }
    }

    #[test]
    fn simultaneous_notes_form_one_chord() {
        let notes = [note(0.0, PPQ, 60), note(0.0, PPQ, 64), note(0.0, PPQ, 67)];
        let chords = group_chords(&notes, DEFAULT_GAP_PPQ);
        assert_eq!(chords.len(), 1);
        assert_eq!(
            chords[0].notes.iter().map(|n| n.pitch).collect::<Vec<_>>(),
            [60, 64, 67]
        );
    }

    #[test]
    fn a_gap_starts_a_new_chord() {
        let notes = [
            note(0.0, PPQ, 60),
            note(0.0, PPQ, 64),
            note(PPQ * 2.0, PPQ, 62),
            note(PPQ * 2.0, PPQ, 65),
        ];
        let chords = group_chords(&notes, DEFAULT_GAP_PPQ);
        assert_eq!(chords.len(), 2);
        assert_eq!(chords[1].start_ppq, PPQ * 2.0);
    }

    #[test]
    fn a_played_chord_with_sloppy_timing_still_groups() {
        // Attacks smeared over ~30 ticks — well inside a 32nd of slop.
        let notes = [note(0.0, PPQ, 60), note(11.0, PPQ, 64), note(29.0, PPQ, 67)];
        assert_eq!(group_chords(&notes, DEFAULT_GAP_PPQ).len(), 1);
    }

    #[test]
    fn notes_are_sorted_low_to_high_regardless_of_input_order() {
        let notes = [note(0.0, PPQ, 67), note(0.0, PPQ, 60), note(0.0, PPQ, 64)];
        let chords = group_chords(&notes, DEFAULT_GAP_PPQ);
        assert_eq!(
            chords[0].notes.iter().map(|n| n.pitch).collect::<Vec<_>>(),
            [60, 64, 67]
        );
    }

    #[test]
    fn a_chord_spans_to_its_longest_note() {
        let notes = [note(0.0, PPQ * 4.0, 48), note(0.0, PPQ, 60)];
        let chords = group_chords(&notes, DEFAULT_GAP_PPQ);
        assert_eq!(chords[0].end_ppq, PPQ * 4.0);
    }

    #[test]
    fn back_to_back_block_chords_stay_separate() {
        // The case that matters, and the one an end-based rule gets
        // wrong: Am held for a bar, then F starting exactly where it
        // ended. Nothing separates them in time, but they are plainly two
        // chords and must arpeggiate as two.
        let notes = [
            note(0.0, PPQ * 4.0, 57),
            note(0.0, PPQ * 4.0, 60),
            note(0.0, PPQ * 4.0, 64),
            note(PPQ * 4.0, PPQ * 4.0, 53),
            note(PPQ * 4.0, PPQ * 4.0, 57),
            note(PPQ * 4.0, PPQ * 4.0, 60),
        ];
        let chords = group_chords(&notes, DEFAULT_GAP_PPQ);
        assert_eq!(chords.len(), 2);
        assert_eq!(chords[0].notes.len(), 3);
        assert_eq!(chords[1].start_ppq, PPQ * 4.0);
    }

    #[test]
    fn a_later_note_over_a_held_chord_is_its_own_chord() {
        // The documented cost of grouping by onset: a melody note landing
        // on beat 3 over a sustained bass does not join it. Pinned so the
        // trade-off is a decision rather than a surprise.
        let notes = [note(0.0, PPQ * 4.0, 36), note(PPQ * 2.0, PPQ, 72)];
        assert_eq!(group_chords(&notes, DEFAULT_GAP_PPQ).len(), 2);
    }

    #[test]
    fn duplicate_pitches_within_a_chord_collapse() {
        let notes = [note(0.0, PPQ, 60), note(5.0, PPQ, 60)];
        let chords = group_chords(&notes, DEFAULT_GAP_PPQ);
        assert_eq!(chords[0].notes.len(), 1);
    }

    #[test]
    fn a_single_note_is_a_chord_of_one() {
        let chords = group_chords(&[note(0.0, PPQ, 36)], DEFAULT_GAP_PPQ);
        assert_eq!(chords.len(), 1);
        assert_eq!(chords[0].notes.len(), 1);
    }

    #[test]
    fn no_notes_means_no_chords() {
        assert!(group_chords(&[], DEFAULT_GAP_PPQ).is_empty());
    }

    #[test]
    fn a_zero_gap_only_groups_exactly_simultaneous_notes() {
        let notes = [note(0.0, PPQ, 60), note(1.0, PPQ, 64)];
        assert_eq!(group_chords(&notes, 0.0).len(), 2);
        assert_eq!(group_chords(&notes, 4.0).len(), 1);
    }

    #[test]
    fn the_gap_is_measured_from_the_chords_first_note_not_the_last() {
        // Otherwise a slow roll chains indefinitely: each note is within
        // slop of the one before it, so an arpeggiated bar collapses into
        // a single chord.
        let notes = [
            note(0.0, PPQ, 60),
            note(100.0, PPQ, 64),
            note(200.0, PPQ, 67),
            note(300.0, PPQ, 72),
        ];
        assert!(group_chords(&notes, 120.0).len() > 1);
    }
}
