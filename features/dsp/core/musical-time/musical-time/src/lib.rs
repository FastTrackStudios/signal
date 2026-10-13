//! Note values, and what they are worth in seconds.
//!
//! A delay time, a pre-delay, a compressor's release — these are all the same
//! quantity wearing different labels. Each one is either a duration the user
//! dialled in milliseconds or a duration the *song* decides: an eighth note,
//! a dotted quarter, a sixteenth triplet. Every plugin that offers the second
//! kind needs the same table of note values, the same arithmetic against the
//! host's tempo, and the same names on screen — so it lives here once rather
//! than three times with three sets of rounding.
//!
//! ```
//! use musical_time::{MusicalTime, NoteValue, Flavour};
//!
//! // A dotted eighth at 120 BPM.
//! let t = MusicalTime::new(NoteValue::Eighth, Flavour::Dotted);
//! assert_eq!(t.label(), "1/8D");
//! assert!((t.seconds_at(120.0) - 0.375).abs() < 1e-12);
//! ```
//!
//! `no_std` and dependency-free on purpose: DSP crates in this repo have to
//! build for embedded targets, and a delay that wants to express its time in
//! beats should not have to pull in the world to do it.

// `no_std` for the core. The optional `params` module talks to nice-plug,
// which is not, so the attribute is conditional rather than absent — a DSP
// crate depending on this with default features still gets a `no_std` build.
#![cfg_attr(not(feature = "params"), no_std)]

/// A note value, as a fraction of a whole note.
///
/// Stops at a sixty-fourth because that is where the musically useful end is:
/// at 120 BPM a 1/64 triplet is already 5 ms, below the point where a delay
/// reads as a delay rather than a comb filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum NoteValue {
    /// A whole note — four beats in 4/4.
    Whole,
    Half,
    #[default]
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
    SixtyFourth,
}

impl NoteValue {
    /// Every note value, longest first.
    pub const ALL: [Self; 7] = [
        Self::Whole,
        Self::Half,
        Self::Quarter,
        Self::Eighth,
        Self::Sixteenth,
        Self::ThirtySecond,
        Self::SixtyFourth,
    ];

    /// How many quarter notes this is worth, straight.
    ///
    /// Quarter notes rather than whole notes because a quarter note is the
    /// beat the tempo is quoted in: at 120 BPM a quarter note is 60/120
    /// seconds, and every other value falls out of that without a second
    /// conversion factor to get backwards.
    #[must_use]
    pub const fn quarter_notes(self) -> f64 {
        match self {
            Self::Whole => 4.0,
            Self::Half => 2.0,
            Self::Quarter => 1.0,
            Self::Eighth => 0.5,
            Self::Sixteenth => 0.25,
            Self::ThirtySecond => 0.125,
            Self::SixtyFourth => 0.0625,
        }
    }

    /// The value on its own: "1/4".
    ///
    /// Separate from [`MusicalTime::label`] because a picker that offers the
    /// note value and its flavour as two controls has to name them
    /// separately — "1/4" next to a D and a T, rather than "1/4D" as one
    /// word.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Whole => "1/1",
            Self::Half => "1/2",
            Self::Quarter => "1/4",
            Self::Eighth => "1/8",
            Self::Sixteenth => "1/16",
            Self::ThirtySecond => "1/32",
            Self::SixtyFourth => "1/64",
        }
    }

    /// The denominator, the way the value is written: 4 for a quarter note.
    #[must_use]
    pub const fn denominator(self) -> u16 {
        match self {
            Self::Whole => 1,
            Self::Half => 2,
            Self::Quarter => 4,
            Self::Eighth => 8,
            Self::Sixteenth => 16,
            Self::ThirtySecond => 32,
            Self::SixtyFourth => 64,
        }
    }
}

/// What is done to a note value to make it longer or shorter.
///
/// The two things every sequencer offers, and the reason a plain list of note
/// values is not enough: a dotted eighth is the delay time behind most of the
/// guitar parts anyone has ever wanted to copy, and it is not on the straight
/// list at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Flavour {
    /// The note as written.
    #[default]
    Straight,
    /// Half again as long — a dot adds half the note's own value.
    Dotted,
    /// Two thirds as long — three in the space of two.
    Triplet,
}

impl Flavour {
    /// Every flavour, in the order a selector should offer them.
    pub const ALL: [Self; 3] = [Self::Straight, Self::Dotted, Self::Triplet];

    /// What this multiplies the note's length by.
    #[must_use]
    pub const fn factor(self) -> f64 {
        match self {
            Self::Straight => 1.0,
            Self::Dotted => 1.5,
            Self::Triplet => 2.0 / 3.0,
        }
    }

    /// The suffix on a label: "1/8", "1/8D", "1/8T".
    ///
    /// `D` and `T` rather than the engraver's dot, because these are read at
    /// nine pixels on a plugin face where a trailing period is a smudge.
    #[must_use]
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::Straight => "",
            Self::Dotted => "D",
            Self::Triplet => "T",
        }
    }
}

/// A note value with its flavour: the thing a user actually picks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct MusicalTime {
    pub value: NoteValue,
    pub flavour: Flavour,
}

impl MusicalTime {
    /// How many entries [`MusicalTime::ALL`] has.
    pub const COUNT: usize = NoteValue::ALL.len() * Flavour::ALL.len();

    /// Every combination, ordered by the note value and then by flavour.
    ///
    /// Grouped by note value rather than sorted by duration on purpose. A
    /// duration-sorted list interleaves the families — 1/4T lands between 1/8D
    /// and 1/4 — and someone reaching for "the eighths" has to hunt. Grouped,
    /// the list reads down in halves the way the values are named, and the
    /// three flavours of each sit together.
    pub const ALL: [Self; Self::COUNT] = {
        let mut out = [Self {
            value: NoteValue::Quarter,
            flavour: Flavour::Straight,
        }; Self::COUNT];
        let mut i = 0;
        // `for` loops are not allowed in const fn, and this table has to be
        // const so a parameter's range can be written in terms of it.
        while i < NoteValue::ALL.len() {
            let mut j = 0;
            while j < Flavour::ALL.len() {
                out[i * Flavour::ALL.len() + j] = Self {
                    value: NoteValue::ALL[i],
                    flavour: Flavour::ALL[j],
                };
                j += 1;
            }
            i += 1;
        }
        out
    };

    #[must_use]
    pub const fn new(value: NoteValue, flavour: Flavour) -> Self {
        Self { value, flavour }
    }

    /// How many quarter notes this is worth.
    #[must_use]
    pub fn quarter_notes(self) -> f64 {
        self.value.quarter_notes() * self.flavour.factor()
    }

    /// How long this is, in seconds, at `bpm`.
    ///
    /// Returns 0 for a tempo that is zero or negative, which is what a host
    /// reports when it has no transport rather than a tempo anyone means —
    /// the caller's fallback should be its free-running time, not an infinity.
    #[must_use]
    pub fn seconds_at(self, bpm: f64) -> f64 {
        if bpm <= 0.0 || !bpm.is_finite() {
            return 0.0;
        }
        self.quarter_notes() * 60.0 / bpm
    }

    /// How long this is, in milliseconds, at `bpm`.
    #[must_use]
    pub fn millis_at(self, bpm: f64) -> f64 {
        self.seconds_at(bpm) * 1000.0
    }

    /// Its place in [`MusicalTime::ALL`] — how it is stored in a parameter.
    #[must_use]
    pub fn index(self) -> usize {
        let v = NoteValue::ALL
            .iter()
            .position(|v| *v == self.value)
            .unwrap_or(0);
        let f = Flavour::ALL
            .iter()
            .position(|f| *f == self.flavour)
            .unwrap_or(0);
        v * Flavour::ALL.len() + f
    }

    /// The entry at `index`, clamped into range.
    ///
    /// Clamped rather than optional because the caller is a parameter read: a
    /// host is free to send anything, and a delay whose division parameter
    /// went out of range should land on the nearest note, not stop.
    #[must_use]
    pub fn from_index(index: usize) -> Self {
        Self::ALL[index.min(Self::COUNT - 1)]
    }

    /// Same, from the `i32` an integer parameter carries.
    #[must_use]
    pub fn from_param(index: i32) -> Self {
        Self::from_index(index.max(0) as usize)
    }

    /// What to print: "1/4", "1/8D", "1/16T".
    #[must_use]
    pub fn label(self) -> &'static str {
        // A lookup rather than formatting, so this stays allocation-free and
        // usable from a `no_std` DSP crate as well as from a UI.
        LABELS[self.index()]
    }

    /// Every label, in [`MusicalTime::ALL`] order — for a selector's items.
    #[must_use]
    pub const fn labels() -> &'static [&'static str; Self::COUNT] {
        &LABELS
    }
}

/// Kept next to [`MusicalTime::ALL`] and in the same order; the test below
/// checks they agree, since nothing in the type system makes them.
static LABELS: [&str; MusicalTime::COUNT] = [
    "1/1", "1/1D", "1/1T", //
    "1/2", "1/2D", "1/2T", //
    "1/4", "1/4D", "1/4T", //
    "1/8", "1/8D", "1/8T", //
    "1/16", "1/16D", "1/16T", //
    "1/32", "1/32D", "1/32T", //
    "1/64", "1/64D", "1/64T",
];

/// Where a time control gets its value from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimeMode {
    /// The number the user dialled, in milliseconds.
    #[default]
    Free,
    /// A note value, against the host's tempo.
    Synced,
}

impl TimeMode {
    /// Read from a parameter that stores the mode as a switch.
    #[must_use]
    pub fn from_param(v: f32) -> Self {
        if v > 0.5 { Self::Synced } else { Self::Free }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Free => "Free",
            Self::Synced => "Sync",
        }
    }
}

/// One time control's full state: which mode, and both possible sources.
///
/// The free-running value is kept even while synced, and vice versa, so
/// flipping the mode returns you to the number you had rather than to a
/// default — the same reason a sequencer remembers your last grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SyncedTime {
    pub mode: TimeMode,
    /// The free-running time, in milliseconds.
    pub free_ms: f64,
    pub division: MusicalTime,
}

impl SyncedTime {
    /// The time this resolves to in milliseconds, given the host's tempo.
    ///
    /// `tempo` is an `Option` because that is how a host reports it: a plugin
    /// opened outside a transport gets `None`, and the honest thing to do
    /// then is fall back to the free-running time rather than invent a tempo.
    /// A synced delay in a host with no tempo should keep making its sound.
    #[must_use]
    pub fn resolve_ms(self, tempo: Option<f64>) -> f64 {
        match (self.mode, tempo) {
            (TimeMode::Synced, Some(bpm)) if bpm > 0.0 && bpm.is_finite() => {
                self.division.millis_at(bpm)
            }
            _ => self.free_ms,
        }
    }

    /// The same, clamped to what the control can actually reach.
    ///
    /// A 1/1 note at 60 BPM is four seconds, and most of these controls do not
    /// go that far. Clamping here rather than at each call site keeps the
    /// division selectable — it just stops where the control stops — instead
    /// of a long note silently reading as the shortest one.
    #[must_use]
    pub fn resolve_ms_clamped(self, tempo: Option<f64>, min_ms: f64, max_ms: f64) -> f64 {
        self.resolve_ms(tempo).clamp(min_ms, max_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The label table is written out by hand next to `ALL`; nothing but this
    /// keeps the two in step.
    #[test]
    fn every_entry_has_the_label_its_value_and_flavour_spell() {
        for t in MusicalTime::ALL {
            let expected_head = match t.value {
                NoteValue::Whole => "1/1",
                NoteValue::Half => "1/2",
                NoteValue::Quarter => "1/4",
                NoteValue::Eighth => "1/8",
                NoteValue::Sixteenth => "1/16",
                NoteValue::ThirtySecond => "1/32",
                NoteValue::SixtyFourth => "1/64",
            };
            let expected = match t.flavour {
                Flavour::Straight => expected_head,
                Flavour::Dotted => match t.value {
                    NoteValue::Whole => "1/1D",
                    NoteValue::Half => "1/2D",
                    NoteValue::Quarter => "1/4D",
                    NoteValue::Eighth => "1/8D",
                    NoteValue::Sixteenth => "1/16D",
                    NoteValue::ThirtySecond => "1/32D",
                    NoteValue::SixtyFourth => "1/64D",
                },
                Flavour::Triplet => match t.value {
                    NoteValue::Whole => "1/1T",
                    NoteValue::Half => "1/2T",
                    NoteValue::Quarter => "1/4T",
                    NoteValue::Eighth => "1/8T",
                    NoteValue::Sixteenth => "1/16T",
                    NoteValue::ThirtySecond => "1/32T",
                    NoteValue::SixtyFourth => "1/64T",
                },
            };
            assert_eq!(t.label(), expected, "wrong label for {t:?}");
        }
    }

    #[test]
    fn the_table_holds_every_combination_exactly_once() {
        assert_eq!(MusicalTime::ALL.len(), 21);
        for value in NoteValue::ALL {
            for flavour in Flavour::ALL {
                let hits = MusicalTime::ALL
                    .iter()
                    .filter(|t| t.value == value && t.flavour == flavour)
                    .count();
                assert_eq!(hits, 1, "{value:?}/{flavour:?} appears {hits} times");
            }
        }
    }

    /// The index is how a parameter stores the choice, so it has to survive
    /// the trip out to a host and back.
    #[test]
    fn index_round_trips_through_the_table() {
        for (i, t) in MusicalTime::ALL.iter().enumerate() {
            assert_eq!(t.index(), i);
            assert_eq!(MusicalTime::from_index(i), *t);
        }
    }

    #[test]
    fn an_out_of_range_index_lands_on_the_last_entry_rather_than_panicking() {
        assert_eq!(
            MusicalTime::from_index(9999),
            MusicalTime::ALL[MusicalTime::COUNT - 1]
        );
        assert_eq!(MusicalTime::from_param(-5), MusicalTime::ALL[0]);
    }

    /// The arithmetic everyone actually checks a tempo-sync against: at 120
    /// BPM a quarter note is half a second.
    #[test]
    fn a_quarter_note_at_120_bpm_is_half_a_second() {
        let q = MusicalTime::new(NoteValue::Quarter, Flavour::Straight);
        assert!((q.seconds_at(120.0) - 0.5).abs() < 1e-12);
        assert!((q.millis_at(120.0) - 500.0).abs() < 1e-9);
    }

    #[test]
    fn a_dot_adds_half_and_a_triplet_takes_a_third() {
        let bpm = 120.0;
        let eighth = MusicalTime::new(NoteValue::Eighth, Flavour::Straight).millis_at(bpm);
        let dotted = MusicalTime::new(NoteValue::Eighth, Flavour::Dotted).millis_at(bpm);
        let triplet = MusicalTime::new(NoteValue::Eighth, Flavour::Triplet).millis_at(bpm);

        assert!((eighth - 250.0).abs() < 1e-9);
        assert!((dotted - eighth * 1.5).abs() < 1e-9, "a dot adds half");
        // Three triplets fill the space of two straight notes.
        assert!((triplet * 3.0 - eighth * 2.0).abs() < 1e-9);
    }

    /// Halving the note value halves the time, all the way down. Catches a
    /// typo in the `quarter_notes` table, which is otherwise 7 magic numbers.
    #[test]
    fn each_note_value_is_half_the_one_above_it() {
        for pair in NoteValue::ALL.windows(2) {
            let (longer, shorter) = (pair[0].quarter_notes(), pair[1].quarter_notes());
            assert!(
                (longer - shorter * 2.0).abs() < 1e-12,
                "{:?} is not twice {:?}",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn doubling_the_tempo_halves_every_note() {
        for t in MusicalTime::ALL {
            let slow = t.millis_at(60.0);
            let fast = t.millis_at(120.0);
            assert!((slow - fast * 2.0).abs() < 1e-9, "{t:?} did not scale");
        }
    }

    /// A host with no transport reports no tempo. Musical time is meaningless
    /// then, and the caller's free-running value is the only honest answer.
    #[test]
    fn a_missing_or_nonsense_tempo_falls_back_to_the_free_running_time() {
        let t = SyncedTime {
            mode: TimeMode::Synced,
            free_ms: 375.0,
            division: MusicalTime::new(NoteValue::Quarter, Flavour::Straight),
        };
        assert!((t.resolve_ms(None) - 375.0).abs() < 1e-9);
        assert!((t.resolve_ms(Some(0.0)) - 375.0).abs() < 1e-9);
        assert!((t.resolve_ms(Some(-120.0)) - 375.0).abs() < 1e-9);
        assert!((t.resolve_ms(Some(f64::NAN)) - 375.0).abs() < 1e-9);
        // And with a real tempo it syncs.
        assert!((t.resolve_ms(Some(120.0)) - 500.0).abs() < 1e-9);
    }

    #[test]
    fn free_mode_ignores_the_tempo_entirely() {
        let t = SyncedTime {
            mode: TimeMode::Free,
            free_ms: 375.0,
            division: MusicalTime::new(NoteValue::Whole, Flavour::Straight),
        };
        assert!((t.resolve_ms(Some(120.0)) - 375.0).abs() < 1e-9);
    }

    /// A whole note at 60 BPM is four seconds; a delay that only reaches two
    /// has to stop at two rather than wrap or read as silence.
    #[test]
    fn a_note_longer_than_the_control_clamps_to_its_maximum() {
        let t = SyncedTime {
            mode: TimeMode::Synced,
            free_ms: 100.0,
            division: MusicalTime::new(NoteValue::Whole, Flavour::Straight),
        };
        assert!((t.resolve_ms(Some(60.0)) - 4000.0).abs() < 1e-9);
        assert!((t.resolve_ms_clamped(Some(60.0), 1.0, 2000.0) - 2000.0).abs() < 1e-9);
    }

    #[test]
    fn the_mode_switch_reads_the_way_a_parameter_stores_it() {
        assert_eq!(TimeMode::from_param(0.0), TimeMode::Free);
        assert_eq!(TimeMode::from_param(1.0), TimeMode::Synced);
        assert_eq!(TimeMode::from_param(0.49), TimeMode::Free);
        assert_eq!(TimeMode::from_param(0.51), TimeMode::Synced);
    }
}

#[cfg(feature = "params")]
pub mod params;
