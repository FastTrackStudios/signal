//! Chords from scale degrees — the ChordGun idea.
//!
//! ChordGun (pandabot, `benjohnson2001/ChordGun`) is a REAPER script that
//! turns the number row into a chord instrument: pick a key and a scale,
//! then `1`..`7` fire the diatonic chord on that degree into the editor,
//! and a few keys cycle the tonic, the scale, the chord depth and the
//! inversion. It is the fastest way anybody has found to sketch a
//! progression, because you are choosing *degrees* rather than spelling
//! chords.
//!
//! ## What this is, and is not
//!
//! It is not a music theory library. Keyflow already has one, and
//! [`crate::chord`] makes the argument at length for why there must not
//! be a second: two models of what a chord is will eventually disagree,
//! and then the chord box and the chord *gun* would name the same notes
//! differently.
//!
//! So the theory is `keyflow_proto::key::scale` — three scale families,
//! every mode of each, and `ScaleHarmonization` which already builds the
//! chord on each degree at any depth from triads to thirteenths. What is
//! here is the part keyflow has no opinion about: which octave the chord
//! lands in, how it is voiced, and turning the result into MIDI pitches
//! this editor can insert.

pub use keyflow_proto::key::scale::{HarmonizationDepth, ScaleMode};

use keyflow_proto::key::scale::ScaleHarmonization;
use keyflow_proto::key::scale::diatonic::DiatonicMode;
use keyflow_proto::primitives::MusicalNote;
// `name()` comes from the `Note` trait rather than the struct.
use keyflow_proto::Note;

/// The chord gun's settings: what a degree currently means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChordGun {
    /// Tonic as a pitch class, 0 = C.
    pub tonic_pc: i32,
    pub mode: ScaleMode,
    /// Triads, sevenths, ninths…
    pub depth: HarmonizationDepth,
    /// Which octave the chord's root lands in. 4 puts it around middle C.
    pub octave: i32,
    /// How many chord tones have been lifted an octave.
    ///
    /// Not a fixed set of named inversions, because the depth is
    /// variable: a thirteenth has seven tones and six useful rotations,
    /// and an enum of "first/second/third" would only describe triads
    /// and sevenths.
    pub inversion: u8,
}

impl Default for ChordGun {
    fn default() -> Self {
        Self {
            tonic_pc: 0,
            mode: ScaleMode::Diatonic(DiatonicMode::Ionian),
            depth: HarmonizationDepth::Triads,
            octave: 4,
            inversion: 0,
        }
    }
}

impl ChordGun {
    /// The harmonized scale these settings describe.
    fn harmonization(&self) -> ScaleHarmonization {
        let pc = self.tonic_pc.rem_euclid(12) as u8;
        // Sharps for the sharp side of the circle, flats for the flat
        // side — keyflow spells the scale from the root's accidental, so
        // handing it the wrong one gives a correct scale with wrong
        // names, which is what a user reads.
        let prefer_sharp = !matches!(pc, 1 | 3 | 6 | 8 | 10);
        let root = MusicalNote::from_semitone(pc, prefer_sharp);
        ScaleHarmonization::new(self.mode, root, self.depth)
    }

    /// The MIDI pitches of the chord on `degree` (1..=7).
    ///
    /// Empty for a degree outside the scale, rather than a panic or a
    /// silent wrap: a caller bound to a key row can be handed an 8.
    pub fn pitches(&self, degree: usize) -> Vec<i32> {
        let harmony = self.harmonization();
        let Some(chord) = harmony.chord_at_degree(degree) else {
            return Vec::new();
        };
        // The degree's own scale tone is the chord's root; keyflow gives
        // the chord as intervals above it.
        let Some(&scale_step) = harmony.scale_semitones.get(degree - 1) else {
            return Vec::new();
        };
        let root = (self.octave + 1) * 12 + self.tonic_pc.rem_euclid(12) + scale_step as i32;

        let mut pitches: Vec<i32> = chord
            .intervals()
            .iter()
            .map(|i| root + i.semitones() as i32)
            .collect();
        pitches.sort_unstable();
        pitches.dedup();

        // Invert by lifting the lowest tones an octave, one per step.
        // Modulo the chord's own size, so a fifth inversion of a triad
        // is the second one rather than nothing — the setting is shared
        // across depths and should stay meaningful when the depth
        // changes underneath it.
        if !pitches.is_empty() {
            let n = pitches.len();
            for _ in 0..(self.inversion as usize % n) {
                let low = pitches.remove(0);
                pitches.push(low + 12);
            }
        }
        pitches.retain(|p| (0..=127).contains(p));
        pitches
    }

    /// What the chord on `degree` is called, for a readout.
    pub fn chord_name(&self, degree: usize) -> String {
        self.harmonization()
            .chord_at_degree(degree)
            .map(|c| format!("{c}"))
            .unwrap_or_default()
    }

    /// The names of all seven chords, for a which-key panel.
    pub fn degree_names(&self) -> Vec<String> {
        (1..=7).map(|d| self.chord_name(d)).collect()
    }

    /// The scale's name, for a readout.
    pub fn scale_name(&self) -> String {
        let pc = self.tonic_pc.rem_euclid(12) as u8;
        let prefer_sharp = !matches!(pc, 1 | 3 | 6 | 8 | 10);
        format!(
            "{} {}",
            MusicalNote::from_semitone(pc, prefer_sharp).name(),
            self.mode.name(),
        )
    }

    /// Step the depth: triads → sevenths → ninths → … and round again.
    pub fn cycle_depth(&mut self, forward: bool) {
        use HarmonizationDepth as D;
        const ALL: [D; 5] = [
            D::Triads,
            D::Sevenths,
            D::Ninths,
            D::Elevenths,
            D::Thirteenths,
        ];
        let i = ALL.iter().position(|d| *d == self.depth).unwrap_or(0);
        let n = ALL.len();
        self.depth = ALL[if forward {
            (i + 1) % n
        } else {
            (i + n - 1) % n
        }];
    }

    /// Step through the modes of the current family.
    ///
    /// Within the family rather than across all of them: the seven modes
    /// of the diatonic family are the ones you actually try against each
    /// other, and walking from Lydian into harmonic minor by pressing
    /// the same key twice more is a surprise.
    pub fn cycle_mode(&mut self, forward: bool) {
        let modes = family_modes(self.mode);
        let i = modes.iter().position(|m| *m == self.mode).unwrap_or(0);
        let n = modes.len();
        self.mode = modes[if forward {
            (i + 1) % n
        } else {
            (i + n - 1) % n
        }];
    }

    /// Move the tonic by a semitone.
    pub fn transpose(&mut self, semitones: i32) {
        self.tonic_pc = (self.tonic_pc + semitones).rem_euclid(12);
    }

    /// Step the inversion, wrapping at the chord's size.
    pub fn cycle_inversion(&mut self, forward: bool) {
        let n = self.depth.note_count() as u8;
        self.inversion = if forward {
            (self.inversion + 1) % n
        } else {
            (self.inversion + n - 1) % n
        };
    }
}

/// Every mode in the same family as `mode`, in rotation order.
fn family_modes(mode: ScaleMode) -> Vec<ScaleMode> {
    use keyflow_proto::key::scale::harmonic_minor::HarmonicMinorMode;
    use keyflow_proto::key::scale::melodic_minor::MelodicMinorMode;

    match mode {
        ScaleMode::Diatonic(_) => [
            DiatonicMode::Ionian,
            DiatonicMode::Dorian,
            DiatonicMode::Phrygian,
            DiatonicMode::Lydian,
            DiatonicMode::Mixolydian,
            DiatonicMode::Aeolian,
            DiatonicMode::Locrian,
        ]
        .into_iter()
        .map(ScaleMode::Diatonic)
        .collect(),
        ScaleMode::HarmonicMinor(_) => [
            HarmonicMinorMode::HarmonicMinor,
            HarmonicMinorMode::LocrianNatural6,
            HarmonicMinorMode::IonianSharp5,
            HarmonicMinorMode::DorianSharp4,
            HarmonicMinorMode::PhrygianDominant,
            HarmonicMinorMode::LydianSharp2,
            HarmonicMinorMode::SuperLocrianDoubleFlatSeven,
        ]
        .into_iter()
        .map(ScaleMode::HarmonicMinor)
        .collect(),
        ScaleMode::MelodicMinor(_) => [
            MelodicMinorMode::MelodicMinor,
            MelodicMinorMode::DorianFlat2,
            MelodicMinorMode::LydianAugmented,
            MelodicMinorMode::LydianDominant,
            MelodicMinorMode::MixolydianFlat6,
            MelodicMinorMode::LocrianNatural2,
            MelodicMinorMode::Altered,
        ]
        .into_iter()
        .map(ScaleMode::MelodicMinor)
        .collect(),
    }
}
