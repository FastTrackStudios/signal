//! Scale degree → concrete MIDI notes.
//!
//! This is the whole theory surface of the tool, and it is deliberately
//! thin: keyflow already knows what a Dorian scale is, which triad sits on
//! its fourth degree, and how to spell a chord. The port of ChordGun's
//! `scales.lua` / `chords.lua` / `scaleData.lua` / `scaleFunctions.lua` is
//! therefore *no port at all* — those describe a private copy of music
//! theory this tree already has one of, and a second copy would drift from
//! the charts, the guide and the transposer that use the first.
//!
//! What's left is the part ChordGun actually contributes: turning "degree
//! 4 of the current scale, as a seventh, at octave 3" into note numbers a
//! DAW can take.

use crate::chord::Chord;
use crate::key::Key;
use crate::key::scale::harmonization::{HarmonizationDepth, harmonize_scale};

/// How many notes to stack on each scale degree.
///
/// Mirrors [`HarmonizationDepth`]; kept as its own type so the tool's
/// wire/UI surface doesn't pin itself to keyflow's enum shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChordSize {
    #[default]
    Triad,
    Seventh,
    Ninth,
    Eleventh,
    Thirteenth,
}

impl ChordSize {
    fn depth(self) -> HarmonizationDepth {
        match self {
            Self::Triad => HarmonizationDepth::Triads,
            Self::Seventh => HarmonizationDepth::Sevenths,
            Self::Ninth => HarmonizationDepth::Ninths,
            Self::Eleventh => HarmonizationDepth::Elevenths,
            Self::Thirteenth => HarmonizationDepth::Thirteenths,
        }
    }

    /// Step up/down the stack, saturating — the increment/decrement
    /// actions bind to this.
    pub fn step(self, delta: i32) -> Self {
        let all = [
            Self::Triad,
            Self::Seventh,
            Self::Ninth,
            Self::Eleventh,
            Self::Thirteenth,
        ];
        let i = all.iter().position(|s| *s == self).unwrap_or(0) as i32;
        all[(i + delta).clamp(0, all.len() as i32 - 1) as usize]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Triad => "Triad",
            Self::Seventh => "7th",
            Self::Ninth => "9th",
            Self::Eleventh => "11th",
            Self::Thirteenth => "13th",
        }
    }
}

/// The seven diatonic chords of `key`, at `size`.
///
/// Index 0 is degree 1. Straight from keyflow — this exists so callers
/// don't have to know about `HarmonizationDepth`.
pub fn scale_chords(key: &Key, size: ChordSize) -> Vec<Chord> {
    harmonize_scale(&key.mode, &key.root, size.depth())
}

/// Semitone offsets of `key`'s scale degrees from its root, one octave.
///
/// `interval_pattern()` is already cumulative — Ionian is
/// `[0, 2, 4, 5, 7, 9, 11]`, not the step list `[2, 2, 1, 2, 2, 2, 1]`.
/// Summing it (the obvious misreading) yields a scale that is wrong from
/// the second degree on.
fn scale_offsets(key: &Key) -> Vec<u8> {
    key.mode.interval_pattern()
}

/// The MIDI note numbers for scale `degree` (1-7) of `key`, as a chord of
/// `size`, rooted in `octave`.
///
/// `octave` is MIDI's: octave 4 puts middle C at 60, matching REAPER's
/// default note naming.
/// Returns an empty vec for a degree outside 1..=7.
///
/// Notes stack upward from the chord root without folding back into one
/// octave — a 13th chord genuinely spans two, and flattening it would
/// change the voicing rather than preserve it.
pub fn chord_notes(key: &Key, degree: u8, size: ChordSize, octave: i32) -> Vec<u8> {
    if !(1..=7).contains(&degree) {
        return Vec::new();
    }
    let offsets = scale_offsets(key);
    let Some(degree_offset) = offsets.get(usize::from(degree - 1)).copied() else {
        return Vec::new();
    };
    let chords = scale_chords(key, size);
    let Some(chord) = chords.get(usize::from(degree - 1)) else {
        return Vec::new();
    };

    let root = root_midi(key, octave).saturating_add(degree_offset);
    chord
        .semitone_sequence()
        .into_iter()
        .filter_map(|semis| {
            let note = i32::from(root) + i32::from(semis);
            (0..=127).contains(&note).then_some(note as u8)
        })
        .collect()
}

/// A single scale note (no chord) at `degree`, for the note-firing
/// actions.
pub fn scale_note(key: &Key, degree: u8, octave: i32) -> Option<u8> {
    if !(1..=7).contains(&degree) {
        return None;
    }
    let offset = scale_offsets(key).get(usize::from(degree - 1)).copied()?;
    let note = i32::from(root_midi(key, octave)) + i32::from(offset);
    (0..=127).contains(&note).then_some(note as u8)
}

/// MIDI number of `key`'s tonic in `octave`. Octave 4 → middle C = 60.
fn root_midi(key: &Key, octave: i32) -> u8 {
    let n = (octave + 1) * 12 + i32::from(key.root.semitone);
    n.clamp(0, 127) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::MusicalNote;

    fn c_major() -> Key {
        Key::major(MusicalNote::from_string("C").expect("C is a note"))
    }

    fn a_minor() -> Key {
        Key::minor(MusicalNote::from_string("A").expect("A is a note"))
    }

    #[test]
    fn tonic_triad_of_c_major_is_middle_c_e_g() {
        assert_eq!(
            chord_notes(&c_major(), 1, ChordSize::Triad, 4),
            vec![60, 64, 67]
        );
    }

    /// Degree 5 of C major is G major — the point of harmonizing rather
    /// than transposing a fixed shape is that each degree gets its own
    /// quality for free.
    #[test]
    fn degree_five_of_c_major_is_g_major() {
        assert_eq!(
            chord_notes(&c_major(), 5, ChordSize::Triad, 4),
            vec![67, 71, 74]
        );
    }

    /// Degree 2 is minor and degree 7 diminished, without anyone saying so.
    #[test]
    fn diatonic_qualities_come_from_the_scale() {
        let ii = chord_notes(&c_major(), 2, ChordSize::Triad, 4);
        assert_eq!(ii, vec![62, 65, 69], "D minor");
        let vii = chord_notes(&c_major(), 7, ChordSize::Triad, 4);
        assert_eq!(vii, vec![71, 74, 77], "B diminished");
    }

    #[test]
    fn sevenths_add_a_fourth_note() {
        let triad = chord_notes(&c_major(), 1, ChordSize::Triad, 4);
        let seventh = chord_notes(&c_major(), 1, ChordSize::Seventh, 4);
        assert_eq!(triad.len(), 3);
        assert_eq!(seventh.len(), 4);
        assert_eq!(
            &seventh[..3],
            &triad[..],
            "the triad is preserved underneath"
        );
    }

    #[test]
    fn octave_shifts_by_twelve() {
        let low = chord_notes(&c_major(), 1, ChordSize::Triad, 3);
        let high = chord_notes(&c_major(), 1, ChordSize::Triad, 4);
        for (l, h) in low.iter().zip(high.iter()) {
            assert_eq!(i32::from(*h) - i32::from(*l), 12);
        }
    }

    /// A minor is C major's relative — same seven pitch classes, different
    /// tonic. If this drifts, the mode handling is wrong.
    #[test]
    fn relative_minor_shares_the_pitch_classes() {
        let major: std::collections::BTreeSet<u8> = (1..=7)
            .filter_map(|d| scale_note(&c_major(), d, 4))
            .map(|n| n % 12)
            .collect();
        let minor: std::collections::BTreeSet<u8> = (1..=7)
            .filter_map(|d| scale_note(&a_minor(), d, 4))
            .map(|n| n % 12)
            .collect();
        assert_eq!(major, minor);
    }

    #[test]
    fn degrees_outside_one_to_seven_yield_nothing() {
        assert!(chord_notes(&c_major(), 0, ChordSize::Triad, 4).is_empty());
        assert!(chord_notes(&c_major(), 8, ChordSize::Triad, 4).is_empty());
        assert!(scale_note(&c_major(), 0, 4).is_none());
    }

    /// Nothing may escape MIDI range, however extreme the octave.
    #[test]
    fn notes_stay_in_midi_range() {
        for octave in -2..=9 {
            for note in chord_notes(&c_major(), 1, ChordSize::Thirteenth, octave) {
                assert!(note <= 127, "octave {octave} produced {note}");
            }
        }
    }

    #[test]
    fn chord_size_steps_and_saturates() {
        assert_eq!(ChordSize::Triad.step(1), ChordSize::Seventh);
        assert_eq!(ChordSize::Triad.step(-1), ChordSize::Triad);
        assert_eq!(ChordSize::Thirteenth.step(1), ChordSize::Thirteenth);
    }
}
