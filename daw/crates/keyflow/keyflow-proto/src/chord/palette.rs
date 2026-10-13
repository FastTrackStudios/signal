//! The chords available in a key, grouped by what they're *for*.
//!
//! Three families, because that's how the choice actually gets made when
//! you're writing:
//!
//! - [`ChordRole::Diatonic`] — in the key. The stable furniture.
//! - [`ChordRole::ParallelKey`] — borrowed from the parallel major/minor
//!   (modal interchange). Same tonic, other mode: the ♭VI, ♭VII and iv
//!   that colour a major key without leaving it.
//! - [`ChordRole::Approach`] — chords that point somewhere. Each carries
//!   the degree it targets, because an approach chord is meaningless
//!   without saying what it approaches.
//!
//! The approach set is the secondary dominant (V7/x) and the tritone
//! substitution (♭II7/x) of every diatonic degree — the two routes that
//! do most of the work in the "stable chord, altered dominant, stable
//! chord" alternation. Alterations on top of those (♭9, ♯9, ♯11, ♭13)
//! are voicing choices layered over the same function, not separate
//! chords, so they belong to a voicing layer rather than here.
//!
//! Pitch sets rather than [`Chord`](super::Chord) values: this crate sits
//! below the chord *parser*, and what a chord-firing UI needs is a label,
//! a role and some notes.

use crate::key::Key;
use crate::primitives::MusicalNote;

/// What a chord is doing in the current key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChordRole {
    /// Built from the key's own scale.
    Diatonic,
    /// Borrowed from the parallel major/minor — same tonic, other mode.
    ParallelKey,
    /// Points at a diatonic degree (1-7). See [`ApproachKind`].
    Approach {
        target_degree: u8,
        kind: ApproachKind,
    },
}

/// How an approach chord reaches its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApproachKind {
    /// V7 of the target — a fifth above, resolving down a fifth.
    SecondaryDominant,
    /// ♭II7 of the target — a semitone above, resolving down a semitone.
    /// Shares its tritone with the secondary dominant, which is why the
    /// two are interchangeable.
    TritoneSub,
}

/// One offer in the palette: what to call it, what it's for, and what to
/// play.
#[derive(Debug, Clone, PartialEq)]
pub struct ChordCandidate {
    /// Display name, e.g. `"Fm"`, `"A♭"`, `"G7"`.
    pub label: String,
    pub role: ChordRole,
    /// Root pitch class, 0-11.
    pub root_pc: u8,
    /// Semitones above the root.
    pub semitones: Vec<u8>,
    /// Whether every note lands on a scale tone.
    ///
    /// Out-of-scale chords are still offered rather than hidden — you
    /// reach for a chromatic chord on purpose, and a grid that drops rows
    /// as the key changes reflows under you. The flag is what lets a view
    /// show them as the outsiders they are.
    pub in_scale: bool,
}

impl ChordCandidate {
    /// Concrete MIDI notes with the root in `octave` (4 → middle C).
    ///
    /// Anything that would leave MIDI range is dropped rather than
    /// wrapped, so an extreme octave thins the chord instead of
    /// scrambling its voicing.
    /// Notes with `inversion` voices rotated up an octave — 0 is root
    /// position, 1 puts the root on top, and so on. Rotating past the
    /// voice count wraps, so a caller can increment freely.
    pub fn notes_inverted(&self, octave: i32, inversion: usize) -> Vec<u8> {
        let mut notes = self.notes(octave);
        if notes.is_empty() {
            return notes;
        }
        for _ in 0..(inversion % notes.len()) {
            let lowest = notes.remove(0);
            let raised = i32::from(lowest) + 12;
            if raised > 127 {
                notes.insert(0, lowest);
                break;
            }
            notes.push(raised as u8);
        }
        notes
    }

    pub fn notes(&self, octave: i32) -> Vec<u8> {
        let root = (octave + 1) * 12 + i32::from(self.root_pc);
        self.semitones
            .iter()
            .filter_map(|s| {
                let n = root + i32::from(*s);
                (0..=127).contains(&n).then_some(n as u8)
            })
            .collect()
    }
}

const MAJOR: [u8; 3] = [0, 4, 7];
const MINOR: [u8; 3] = [0, 3, 7];
const DIM: [u8; 3] = [0, 3, 6];
const AUG: [u8; 3] = [0, 4, 8];
const DOM7: [u8; 4] = [0, 4, 7, 10];

fn quality_suffix(semitones: &[u8]) -> &'static str {
    match semitones {
        s if s == MAJOR => "",
        s if s == MINOR => "m",
        s if s == DIM => "°",
        s if s == AUG => "+",
        s if s == DOM7 => "7",
        _ => "",
    }
}

/// Note name for a pitch class. Flats in flat keys, sharps otherwise —
/// so a borrowed ♭VI in C reads `A♭`, not `G♯`.
fn pc_name(pc: u8, prefer_sharp: bool) -> String {
    MusicalNote::from_semitone(pc % 12, prefer_sharp).name
}

/// Whether a key spells its own diatonic chords with sharps. Rough but
/// right for the common cases: flat tonics spell flats.
fn prefers_sharps(key: &Key) -> bool {
    !matches!(key.root.semitone, 1 | 3 | 5 | 8 | 10)
}

/// Spelling follows *function*, not just the key.
///
/// A borrowed chord and a tritone sub are flat-side by construction —
/// ♭VI, ♭VII, ♭II — so they spell flat even in a sharp key. Getting this
/// from the key alone gives `C♯7` where the chart says `D♭7`.
fn spell_sharp(key: &Key, role: ChordRole) -> bool {
    match role {
        ChordRole::ParallelKey => false,
        ChordRole::Approach {
            kind: ApproachKind::TritoneSub,
            ..
        } => false,
        _ => prefers_sharps(key),
    }
}

/// Stack thirds within `scale` (cumulative semitone offsets) starting at
/// `degree_index`, taking `count` notes.
fn stack_thirds(scale: &[u8], degree_index: usize, count: usize) -> Vec<u8> {
    let len = scale.len();
    if len == 0 {
        return Vec::new();
    }
    let root = i32::from(scale[degree_index % len]);
    (0..count)
        .map(|i| {
            let idx = degree_index + i * 2;
            let octaves = (idx / len) as i32;
            let offset = i32::from(scale[idx % len]) + octaves * 12;
            (offset - root).rem_euclid(12 * 4) as u8
        })
        .collect()
}

/// The seven diatonic triads of `key`.
pub fn diatonic(key: &Key) -> Vec<ChordCandidate> {
    let scale = key.mode.interval_pattern();
    let sharp = prefers_sharps(key);
    (0..scale.len().min(7))
        .map(|i| {
            let semis = stack_thirds(&scale, i, 3);
            let root_pc = (key.root.semitone + scale[i]) % 12;
            ChordCandidate {
                label: format!("{}{}", pc_name(root_pc, sharp), quality_suffix(&semis)),
                role: ChordRole::Diatonic,
                root_pc,
                semitones: semis,
                in_scale: true,
            }
        })
        .collect()
}

/// Triads borrowed from the parallel mode, minus anything already
/// diatonic — those aren't borrowed, they're shared.
pub fn parallel_key(key: &Key) -> Vec<ChordCandidate> {
    let parallel = if is_minor_ish(key) {
        Key::major(key.root.clone())
    } else {
        Key::minor(key.root.clone())
    };
    let own: Vec<(u8, Vec<u8>)> = diatonic(key)
        .into_iter()
        .map(|c| (c.root_pc, c.semitones))
        .collect();

    diatonic(&parallel)
        .into_iter()
        .filter(|c| {
            !own.iter()
                .any(|(pc, s)| *pc == c.root_pc && *s == c.semitones)
        })
        .map(|mut c| {
            c.role = ChordRole::ParallelKey;
            c.in_scale = false;
            c.label = format!(
                "{}{}",
                pc_name(c.root_pc, spell_sharp(key, ChordRole::ParallelKey)),
                quality_suffix(&c.semitones)
            );
            c
        })
        .collect()
}

/// Whether `key`'s third is minor — good enough to pick the parallel.
fn is_minor_ish(key: &Key) -> bool {
    key.mode
        .interval_pattern()
        .get(2)
        .is_some_and(|third| *third == 3)
}

/// Secondary dominants and tritone subs for each diatonic degree.
///
/// Both are dominant sevenths; they differ only in where they sit
/// relative to the target, and they share a tritone, which is why either
/// resolves.
pub fn approach(key: &Key) -> Vec<ChordCandidate> {
    let mut out = Vec::new();
    for (i, target) in diatonic(key).into_iter().enumerate() {
        let degree = (i + 1) as u8;
        for (kind, offset) in [
            (ApproachKind::SecondaryDominant, 7u8),
            (ApproachKind::TritoneSub, 1u8),
        ] {
            let root_pc = (target.root_pc + offset) % 12;
            let role = ChordRole::Approach {
                target_degree: degree,
                kind,
            };
            out.push(ChordCandidate {
                label: format!("{}7", pc_name(root_pc, spell_sharp(key, role))),
                role,
                root_pc,
                semitones: DOM7.to_vec(),
                in_scale: false,
            });
        }
    }
    out
}

/// Everything, in the order a panel should show it.
pub fn palette(key: &Key) -> Vec<ChordCandidate> {
    let mut out = diatonic(key);
    out.extend(parallel_key(key));
    out.extend(approach(key));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c_major() -> Key {
        Key::major(MusicalNote::from_string("C").expect("C"))
    }

    #[test]
    fn c_major_diatonic_is_the_familiar_seven() {
        let labels: Vec<String> = diatonic(&c_major()).into_iter().map(|c| c.label).collect();
        assert_eq!(labels, ["C", "Dm", "Em", "F", "G", "Am", "B°"]);
    }

    /// The borrowed chords that give a major key its minor colour. None
    /// of them are already in C major, or they wouldn't be borrowed.
    #[test]
    fn parallel_minor_lends_the_flat_side() {
        let labels: Vec<String> = parallel_key(&c_major())
            .into_iter()
            .map(|c| c.label)
            .collect();
        for expected in ["Fm", "Ab", "Bb"] {
            assert!(
                labels.iter().any(|l| l == expected),
                "expected {expected} among {labels:?}"
            );
        }
        assert!(
            !labels.iter().any(|l| l == "C"),
            "C is shared, not borrowed"
        );
    }

    /// The secondary dominant of ii in C is A7 — a fifth above D.
    #[test]
    fn secondary_dominant_sits_a_fifth_above_its_target() {
        let a7 = approach(&c_major())
            .into_iter()
            .find(|c| {
                c.role
                    == ChordRole::Approach {
                        target_degree: 2,
                        kind: ApproachKind::SecondaryDominant,
                    }
            })
            .expect("V7/ii exists");
        assert_eq!(a7.label, "A7");
        assert_eq!(a7.semitones, DOM7.to_vec());
    }

    /// The tritone sub sits a semitone above its target and shares the
    /// secondary dominant's tritone — that shared tritone is the whole
    /// reason the substitution works, so assert it rather than the name.
    #[test]
    fn tritone_sub_shares_the_dominant_tritone() {
        let all = approach(&c_major());
        let find = |kind| {
            all.iter()
                .find(|c| {
                    c.role
                        == ChordRole::Approach {
                            target_degree: 1,
                            kind,
                        }
                })
                .expect("approach exists")
        };
        let dom = find(ApproachKind::SecondaryDominant);
        let sub = find(ApproachKind::TritoneSub);

        assert_eq!(dom.label, "G7", "V7/I");
        assert_eq!(sub.label, "Db7", "bII7/I");
        assert_eq!((sub.root_pc + 6) % 12, dom.root_pc, "a tritone apart");

        let tritone = |c: &ChordCandidate| {
            let third = (c.root_pc + 4) % 12;
            let seventh = (c.root_pc + 10) % 12;
            let mut t = [third, seventh];
            t.sort_unstable();
            t
        };
        assert_eq!(tritone(dom), tritone(sub), "same third/seventh pair");
    }

    #[test]
    fn every_degree_gets_both_approaches() {
        let approaches = approach(&c_major());
        assert_eq!(approaches.len(), 14, "7 degrees x 2 routes");
        for degree in 1..=7u8 {
            for kind in [ApproachKind::SecondaryDominant, ApproachKind::TritoneSub] {
                assert!(
                    approaches.iter().any(|c| c.role
                        == ChordRole::Approach {
                            target_degree: degree,
                            kind
                        }),
                    "missing {kind:?} for degree {degree}"
                );
            }
        }
    }

    #[test]
    fn tonic_triad_realizes_to_middle_c() {
        let tonic = &diatonic(&c_major())[0];
        assert_eq!(tonic.notes(4), vec![60, 64, 67]);
    }

    #[test]
    fn flat_keys_spell_with_flats() {
        let e_flat = Key::major(MusicalNote::from_string("Eb").expect("Eb"));
        let labels: Vec<String> = diatonic(&e_flat).into_iter().map(|c| c.label).collect();
        assert!(
            labels.iter().any(|l| l.contains('b')),
            "E♭ major should spell flats, got {labels:?}"
        );
    }
}

// ── The degree × variation grid ─────────────────────────────────────────
//
// The shape ChordGun puts on screen, and the reason it's quick to use:
// seven columns, one per scale degree, and down each column the chord
// types that actually *fit* the scale there. No dead options — if a type
// would need a note outside the key it simply isn't offered, which is
// what makes scanning a column a musical choice rather than a filter
// exercise.

/// A chord type as a display suffix plus semitones above the root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChordType {
    /// Suffix appended to the root name — `""` for a plain major triad.
    pub display: &'static str,
    pub semitones: &'static [u8],
}

/// The chord vocabulary offered per degree, in the order it should be
/// listed. Ported from ChordGun's `chords.lua`, whose binary patterns
/// (`"10001001"` = root, major third, fifth) are just semitone sets.
pub const CHORD_TYPES: &[ChordType] = &[
    ChordType {
        display: "",
        semitones: &[0, 4, 7],
    },
    ChordType {
        display: "m",
        semitones: &[0, 3, 7],
    },
    ChordType {
        display: "5",
        semitones: &[0, 7],
    },
    ChordType {
        display: "sus2",
        semitones: &[0, 2, 7],
    },
    ChordType {
        display: "sus4",
        semitones: &[0, 5, 7],
    },
    ChordType {
        display: "dim",
        semitones: &[0, 3, 6],
    },
    ChordType {
        display: "aug",
        semitones: &[0, 4, 8],
    },
    ChordType {
        display: "6",
        semitones: &[0, 4, 7, 9],
    },
    ChordType {
        display: "m6",
        semitones: &[0, 3, 7, 9],
    },
    ChordType {
        display: "7",
        semitones: &[0, 4, 7, 10],
    },
    ChordType {
        display: "maj7",
        semitones: &[0, 4, 7, 11],
    },
    ChordType {
        display: "m7",
        semitones: &[0, 3, 7, 10],
    },
    ChordType {
        display: "b5",
        semitones: &[0, 4, 6],
    },
];

/// Pitch classes of `key`'s scale.
fn scale_pcs(key: &Key) -> Vec<u8> {
    key.mode
        .interval_pattern()
        .into_iter()
        .map(|offset| (key.root.semitone + offset) % 12)
        .collect()
}

/// Every chord type at `degree` (1-7), each flagged for whether it fits
/// the scale.
///
/// All of them, not just the fitting ones: hiding the rest makes the grid
/// reflow as the key changes, and a chromatic chord is something you
/// reach for deliberately. `in_scale` is how a view tells them apart.
pub fn variations(key: &Key, degree: u8) -> Vec<ChordCandidate> {
    if !(1..=7).contains(&degree) {
        return Vec::new();
    }
    let scale = key.mode.interval_pattern();
    let Some(offset) = scale.get(usize::from(degree - 1)).copied() else {
        return Vec::new();
    };
    let pcs = scale_pcs(key);
    let root_pc = (key.root.semitone + offset) % 12;
    let sharp = prefers_sharps(key);

    CHORD_TYPES
        .iter()
        .map(|ty| {
            let in_scale = ty
                .semitones
                .iter()
                .all(|s| pcs.contains(&((root_pc + s) % 12)));
            ChordCandidate {
                label: format!("{}{}", pc_name(root_pc, sharp), ty.display),
                role: ChordRole::Diatonic,
                root_pc,
                semitones: ty.semitones.to_vec(),
                in_scale,
            }
        })
        .collect()
}

/// The whole grid: seven columns of in-scale variations, degree 1 first.
pub fn grid(key: &Key) -> Vec<Vec<ChordCandidate>> {
    (1..=7).map(|degree| variations(key, degree)).collect()
}

#[cfg(test)]
mod grid_tests {
    use super::*;

    fn c_major() -> Key {
        Key::major(MusicalNote::from_string("C").expect("C"))
    }

    #[test]
    fn the_grid_has_a_column_per_degree() {
        let g = grid(&c_major());
        assert_eq!(g.len(), 7);
        assert!(
            g.iter().all(|col| col.len() == CHORD_TYPES.len()),
            "every column offers the whole vocabulary, flagged"
        );
    }

    /// Degree 1 of C major: C, Csus2, Csus4, C6, Cmaj7 all sit in the
    /// key. Cm and Caug do not, and must not be offered.
    #[test]
    fn first_degree_flags_in_key_types() {
        let labels: Vec<String> = variations(&c_major(), 1)
            .into_iter()
            .filter(|c| c.in_scale)
            .map(|c| c.label)
            .collect();
        for expected in ["C", "Csus2", "Csus4", "C6", "Cmaj7"] {
            assert!(
                labels.iter().any(|l| l == expected),
                "missing {expected} in {labels:?}"
            );
        }
        // Still offered, but marked as outsiders rather than removed.
        let out: Vec<String> = variations(&c_major(), 1)
            .into_iter()
            .filter(|c| !c.in_scale)
            .map(|c| c.label)
            .collect();
        for absent in ["Cm", "Caug", "Cdim", "C7"] {
            assert!(
                out.iter().any(|l| l == absent),
                "{absent} should be flagged out-of-scale"
            );
        }
    }

    /// The fifth degree is where the dominant seventh lives, and nowhere
    /// else in a major key.
    #[test]
    fn only_the_fifth_degree_offers_a_dominant_seventh() {
        let with_dom7: Vec<u8> = (1..=7)
            .filter(|d| {
                variations(&c_major(), *d)
                    .iter()
                    .any(|c| c.in_scale && c.semitones == vec![0, 4, 7, 10])
            })
            .collect();
        assert_eq!(with_dom7, vec![5], "G7 only");
    }

    /// Every offered chord must be playable entirely within the scale —
    /// the property the column filter exists to guarantee.
    #[test]
    fn every_offered_chord_is_in_the_scale() {
        let key = c_major();
        let pcs = scale_pcs(&key);
        for column in grid(&key) {
            for chord in column.into_iter().filter(|c| c.in_scale) {
                for note in chord.notes(4) {
                    assert!(
                        pcs.contains(&(note % 12)),
                        "{} plays {} which is outside the key",
                        chord.label,
                        note
                    );
                }
            }
        }
    }

    #[test]
    fn the_seventh_degree_is_diminished_not_major() {
        let labels: Vec<String> = variations(&c_major(), 7)
            .into_iter()
            .filter(|c| c.in_scale)
            .map(|c| c.label)
            .collect();
        assert!(labels.iter().any(|l| l == "Bdim"), "got {labels:?}");
        assert!(!labels.iter().any(|l| l == "B"), "B major is out of key");
    }
}

#[cfg(test)]
mod inversion_tests {
    use super::*;

    fn c_triad() -> ChordCandidate {
        variations(&Key::major(MusicalNote::from_string("C").expect("C")), 1)
            .into_iter()
            .next()
            .expect("C major triad")
    }

    #[test]
    fn root_position_is_inversion_zero() {
        assert_eq!(c_triad().notes_inverted(4, 0), vec![60, 64, 67]);
    }

    #[test]
    fn each_inversion_lifts_the_lowest_voice_an_octave() {
        assert_eq!(c_triad().notes_inverted(4, 1), vec![64, 67, 72]);
        assert_eq!(c_triad().notes_inverted(4, 2), vec![67, 72, 76]);
    }

    /// Wrapping past the voice count returns to root position, so a
    /// caller can increment without bounds-checking.
    #[test]
    fn inversion_wraps_at_the_voice_count() {
        assert_eq!(
            c_triad().notes_inverted(4, 3),
            c_triad().notes_inverted(4, 0)
        );
    }

    /// Inversion must never push a voice out of MIDI range.
    #[test]
    fn inversion_stops_rather_than_overflowing() {
        let high = ChordCandidate {
            label: "top".into(),
            role: ChordRole::Diatonic,
            root_pc: 0,
            semitones: vec![0, 4, 7],
            in_scale: true,
        };
        for note in high.notes_inverted(9, 2) {
            assert!(note <= 127, "{note} is out of range");
        }
    }
}
