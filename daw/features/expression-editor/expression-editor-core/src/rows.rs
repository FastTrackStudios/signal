//! What a row *means*.
//!
//! Every product this editor serves differs mainly in one thing: what
//! the vertical axis is.
//!
//! | product        | row              | note label |
//! |----------------|------------------|------------|
//! | audio pitch    | MIDI pitch       | note name  |
//! | MPE / MIDI     | MIDI pitch       | note name  |
//! | vocal lyrics   | MIDI pitch       | syllable   |
//! | drums          | named drum dimension  | drum name  |
//! | guitar / bass  | **string**       | **fret**   |
//!
//! So the row axis is abstracted here rather than hard-coded as pitch.
//! A guitar "string roll" (Ample's Riffer) is the interesting case: the
//! row is the string, the note carries a fret, and sounding pitch is
//! `open_pitch[string] + fret`. Everything else in the editor —
//! gestures, zones, curves, the camera — is unchanged, because it all
//! works in *row* space already.

use crate::doc::Note;
use crate::tuning;

/// A guitar or bass tuning: open pitch of each string.
///
/// Index 0 is the **lowest** string, matching how tunings are written
/// (E A D G B E). The roll draws them inverted so the high string is on
/// top, the way tab is read.
#[derive(Clone, Debug, PartialEq)]
pub struct StringTuning {
    pub name: &'static str,
    pub open_pitches: Vec<i32>,
    /// Highest fret the instrument has.
    pub frets: u8,
    /// Capo position; open strings sound this many semitones higher.
    pub capo: u8,
}

impl StringTuning {
    pub fn guitar_standard() -> Self {
        Self {
            name: "Guitar (E A D G B E)",
            open_pitches: vec![40, 45, 50, 55, 59, 64],
            frets: 22,
            capo: 0,
        }
    }

    pub fn guitar_drop_d() -> Self {
        Self {
            name: "Guitar (Drop D)",
            open_pitches: vec![38, 45, 50, 55, 59, 64],
            frets: 22,
            capo: 0,
        }
    }

    pub fn bass_4() -> Self {
        Self {
            name: "Bass (E A D G)",
            open_pitches: vec![28, 33, 38, 43],
            frets: 24,
            capo: 0,
        }
    }

    pub fn bass_5() -> Self {
        Self {
            name: "Bass 5 (B E A D G)",
            open_pitches: vec![23, 28, 33, 38, 43],
            frets: 24,
            capo: 0,
        }
    }

    pub fn strings(&self) -> usize {
        self.open_pitches.len()
    }

    /// Open pitch of a string, capo included.
    pub fn open(&self, string: usize) -> i32 {
        self.open_pitches.get(string).copied().unwrap_or(40) + self.capo as i32
    }

    /// Sounding pitch of a fingered position.
    pub fn pitch(&self, string: usize, fret: u8) -> i32 {
        self.open(string) + fret as i32
    }

    /// Fret that puts `pitch` on `string`, if it is reachable.
    pub fn fret_for(&self, string: usize, pitch: i32) -> Option<u8> {
        let f = pitch - self.open(string);
        (f >= 0 && f <= self.frets as i32).then_some(f as u8)
    }

    /// The playable position for `pitch` nearest `preferred_fret`.
    ///
    /// Guitarists do not play the lowest available fret; they play the
    /// one nearest the hand. Biasing toward the current position is
    /// what makes imported MIDI land somewhere playable instead of
    /// scattered down at the nut.
    pub fn best_position(&self, pitch: i32, preferred_fret: u8) -> Option<(usize, u8)> {
        (0..self.strings())
            .filter_map(|s| self.fret_for(s, pitch).map(|f| (s, f)))
            .min_by_key(|&(_, f)| (f as i32 - preferred_fret as i32).abs())
    }
}

/// A named drum dimension.
#[derive(Clone, Debug, PartialEq)]
pub struct DrumLane {
    pub pitch: i32,
    pub name: String,
    /// Lanes sharing a group choke each other (hi-hat open/closed).
    pub group: Option<u8>,
    /// Which hand plays this row, when the kit distinguishes them.
    ///
    /// `None` is the ordinary case and the default: for most parts it
    /// does not matter which hand hit the snare, and a roll that always
    /// asked would be twice as tall for no reason. A piece becomes
    /// two-handed only when something needs it — a flam, or notation
    /// that specifies sticking.
    pub hand: Option<Hand>,
    /// The **piece** this row belongs to — `T1`, `S`, `K`.
    ///
    /// The piece is what the player thinks of as a drum; the hands are
    /// how it splits. So a collapsed row is labelled with this, and a
    /// split one is labelled `L` or `R` *under* it, rather than the roll
    /// showing two rows called `T1 L` and `T1` and leaving you to work
    /// out that they are the same drum.
    pub piece: Option<String>,
    /// The pitch of the same piece played by the other hand.
    ///
    /// This is what makes a split reversible and a flam possible: the
    /// row knows its counterpart rather than a gesture having to guess
    /// at one.
    pub other_hand: Option<i32>,
}

/// The part of the kit a lane belongs to.
///
/// Grouping is what makes a real kit navigable: the FTS map is 39 rows,
/// and a wall of evenly-striped lanes gives the eye nothing to steer by.
/// Banding by family means you find the toms by *shape* rather than by
/// reading every label.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DrumFamily {
    Kick,
    Snare,
    Tom,
    HiHat,
    Cymbal,
    /// Ride is its own band rather than a cymbal, because it is played
    /// like a hat — a continuous part — not struck like a crash.
    Ride,
    Other,
}

impl DrumFamily {
    /// Background tint for the row band.
    ///
    /// Deliberately near-black: this sits *under* the notes, and a band
    /// strong enough to notice on its own would compete with the
    /// material it exists to organise. The hue is the signal; the value
    /// stays almost flat.
    pub fn band(self) -> &'static str {
        match self {
            DrumFamily::Kick => "#1e1416",
            DrumFamily::Snare => "#1e1a13",
            DrumFamily::Tom => "#161c14",
            DrumFamily::HiHat => "#131b1e",
            DrumFamily::Cymbal => "#1c1520",
            DrumFamily::Ride => "#14181f",
            DrumFamily::Other => "#16161a",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DrumFamily::Kick => "Kick",
            DrumFamily::Snare => "Snare",
            DrumFamily::Tom => "Toms",
            DrumFamily::HiHat => "Hi-hat",
            DrumFamily::Cymbal => "Cymbals",
            DrumFamily::Ride => "Ride",
            DrumFamily::Other => "Other",
        }
    }
}

/// Which family a lane name belongs to.
///
/// Reads the FTS abbreviations first and exactly — `K`, `S`, `T1`, `H-`,
/// `C-`, `R-` — then falls back to the words a General MIDI map uses.
/// Matching a single letter by substring would put half the kit in the
/// wrong band.
pub fn drum_family(name: &str) -> DrumFamily {
    let n = name.to_ascii_lowercase();
    let head = n.split([' ', '-']).next().unwrap_or("");
    match head {
        "k" | "kl" => return DrumFamily::Kick,
        "s" | "sr" => return DrumFamily::Snare,
        "t1" | "t2" | "t3" | "t4" => return DrumFamily::Tom,
        "h" => return DrumFamily::HiHat,
        "r" => return DrumFamily::Ride,
        "c" => return DrumFamily::Cymbal,
        _ => {}
    }
    if n.contains("kick") {
        DrumFamily::Kick
    } else if n.contains("snare") || n.contains("stick") || n.contains("clap") {
        DrumFamily::Snare
    } else if n.contains("tom") {
        DrumFamily::Tom
    } else if n.contains("hh") || n.contains("hat") {
        DrumFamily::HiHat
    } else if n.contains("ride") {
        DrumFamily::Ride
    } else if n.contains("crash") || n.contains("china") || n.contains("splash") {
        DrumFamily::Cymbal
    } else {
        DrumFamily::Other
    }
}

/// Which hand plays a drum.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Hand {
    Left,
    Right,
}

impl Hand {
    pub fn other(self) -> Hand {
        match self {
            Hand::Left => Hand::Right,
            Hand::Right => Hand::Left,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Hand::Left => "L",
            Hand::Right => "R",
        }
    }
}

/// An ordered set of drum lanes — the rows of a drum editor.
#[derive(Clone, Debug, PartialEq)]
pub struct DrumMap {
    pub name: &'static str,
    /// Ordered low-to-high as drawn (kick at the bottom).
    pub lanes: Vec<DrumLane>,
}

/// One row of the `fts()` table: pitch, name, group, hand, the pitch of
/// the other hand, and the piece both hands belong to.
type LaneRow = (
    i32,
    &'static str,
    Option<u8>,
    Option<Hand>,
    Option<i32>,
    Option<&'static str>,
);

impl DrumMap {
    /// General MIDI, trimmed to the kit pieces people actually
    /// sequence. A full 47-dimension GM map is unusable as a roll.
    /// The FTS kit, as `midi-note-maps/FTS Drum Map.txt` defines it.
    ///
    /// Kick, snare and all four toms are **two-handed pieces**: each has
    /// a left and a right note. Everything else is a single row, because
    /// nothing about a hi-hat or a crash depends on which stick got
    /// there.
    ///
    /// The two-handed pieces are collapsed by default; passing a piece
    /// in [`DrumMap::visible_rows`]'s `split` set opens it when a part
    /// needs both hands. A roll that showed both hands for every drum
    /// would be twice as tall for no reason on most material.
    pub fn fts() -> Self {
        // (pitch, name, group, hand, other-hand pitch, piece)
        let lanes: Vec<LaneRow> = vec![
            // Kick is a pair for the same reason the toms are: a double
            // pedal is two feet, and a part that specifies which is
            // notated the same way sticking is.
            (23, "K L", None, Some(Hand::Left), Some(24), Some("K")),
            (24, "K R", None, Some(Hand::Right), Some(23), Some("K")),
            (25, "S-Cross", None, None, None, None),
            // The snare is deliberately **not** paired. The map has
            // `S`, `SR`, `S-Buzz`, `S-Cross` and no `S L` — `SR` is the
            // *rim*, not the right hand. Pairing them would put a
            // rimshot on the other stick every time you asked for
            // sticking. Add an `S L` to the map and give it
            // `Some(Hand::Left)` here to turn the snare two-handed.
            (26, "S", None, None, None, None),
            (27, "S-Buzz", None, None, None, None),
            (28, "S Rim", None, None, None, None),
            (29, "T1 L", None, Some(Hand::Left), Some(30), Some("T1")),
            (30, "T1 R", None, Some(Hand::Right), Some(29), Some("T1")),
            (31, "T2 L", None, Some(Hand::Left), Some(32), Some("T2")),
            (32, "T2 R", None, Some(Hand::Right), Some(31), Some("T2")),
            (33, "T3 L", None, Some(Hand::Left), Some(34), Some("T3")),
            (34, "T3 R", None, Some(Hand::Right), Some(33), Some("T3")),
            (35, "T4 L", None, Some(Hand::Left), Some(36), Some("T4")),
            (36, "T4 R", None, Some(Hand::Right), Some(35), Some("T4")),
            (37, "H-Tight Tip", Some(1), None, None, None),
            (38, "H-Tight Edg", Some(1), None, None, None),
            (39, "H-Clsd Tip", Some(1), None, None, None),
            (40, "H-Clsd Edg", Some(1), None, None, None),
            (41, "H-Open1", Some(1), None, None, None),
            (42, "H-Open2", Some(1), None, None, None),
            (43, "H-Open3", Some(1), None, None, None),
            (44, "H-Chick", Some(1), None, None, None),
            (45, "H-Ching", Some(1), None, None, None),
            (48, "C-L", Some(2), None, None, None),
            (49, "C-L Choke", Some(2), None, None, None),
            (50, "C-C", Some(3), None, None, None),
            (51, "C-C Choke", Some(3), None, None, None),
            (52, "C-R", Some(4), None, None, None),
            (53, "C-R Choke", Some(4), None, None, None),
            (54, "H-Bell", Some(1), None, None, None),
            (56, "R-Bow Lo", Some(5), None, None, None),
            (57, "R-Bell", Some(5), None, None, None),
            (58, "R-Crash", Some(5), None, None, None),
            (59, "R-Choke", Some(5), None, None, None),
            (62, "China", Some(6), None, None, None),
            (63, "China Choke", Some(6), None, None, None),
            (64, "Splash", Some(7), None, None, None),
            (65, "Splash Choke", Some(7), None, None, None),
            (71, "Stack", None, None, None, None),
        ];
        Self {
            name: "FTS",
            lanes: lanes
                .into_iter()
                .map(|(pitch, name, group, hand, other_hand, piece)| DrumLane {
                    pitch,
                    name: name.to_string(),
                    group,
                    hand,
                    other_hand,
                    piece: piece.map(str::to_string),
                })
                .collect(),
        }
    }

    /// Whether a row is one hand of a two-handed piece.
    pub fn is_two_handed(&self, row: usize) -> bool {
        self.lanes.get(row).is_some_and(|l| l.other_hand.is_some())
    }

    /// The row holding the other hand of `row`'s piece.
    pub fn other_hand_row(&self, row: usize) -> Option<usize> {
        let other = self.lanes.get(row)?.other_hand?;
        self.lanes.iter().position(|l| l.pitch == other)
    }

    /// The family a row belongs to.
    pub fn family_of(&self, row: usize) -> Option<DrumFamily> {
        self.lanes.get(row).map(|l| drum_family(&l.name))
    }

    /// Whether `row` is the first of its family, reading upward.
    ///
    /// What a renderer needs to draw one divider per group instead of
    /// one per row — the line that makes a band read as a band.
    pub fn starts_family(&self, row: usize) -> bool {
        let Some(here) = self.family_of(row) else {
            return false;
        };
        match row.checked_sub(1).and_then(|r| self.family_of(r)) {
            Some(below) => below != here,
            None => true,
        }
    }

    /// What to write in the row header.
    ///
    /// Collapsed, a two-handed piece shows the **piece** — `T1` — because
    /// that is the drum. Split, each row shows its hand — `L`, `R` —
    /// with the piece carried by [`DrumMap::group_name`] so a renderer
    /// can bracket the pair. Showing `T1 L` and `T1` side by side was
    /// the thing that made you work out they were one drum.
    pub fn display_name(&self, row: usize, split: bool) -> String {
        let Some(lane) = self.lanes.get(row) else {
            return String::new();
        };
        match (&lane.piece, split) {
            (Some(piece), false) => piece.clone(),
            (Some(_), true) => self
                .hand_of(row)
                .map(|h| h.label().to_string())
                .unwrap_or_else(|| lane.name.clone()),
            (None, _) => lane.name.clone(),
        }
    }

    /// The piece a row belongs to, for a renderer that brackets the two
    /// hands together.
    pub fn group_name(&self, row: usize) -> Option<&str> {
        self.lanes.get(row)?.piece.as_deref()
    }

    /// The row for a piece played by a given hand.
    pub fn row_for_hand(&self, row: usize, hand: Hand) -> Option<usize> {
        if self.hand_of(row) == Some(hand) {
            return Some(row);
        }
        self.other_hand_row(row)
            .filter(|&o| self.hand_of(o) == Some(hand))
    }

    /// Which hand `row` is played with, for a two-handed piece.
    ///
    /// Both halves of a pair carry their hand explicitly. An earlier
    /// version inferred "unmarked means right", which quietly made both
    /// halves of the snare pair report Right — the bug that revealed the
    /// snare was not a pair at all.
    pub fn hand_of(&self, row: usize) -> Option<Hand> {
        let lane = self.lanes.get(row)?;
        lane.other_hand?;
        lane.hand
    }

    /// Rows currently shown: every single-handed piece, plus both halves
    /// of any piece in `split`.
    ///
    /// A two-handed piece the caller has not split contributes only its
    /// unmarked row, so the roll stays the height it was.
    pub fn visible_rows(&self, split: &[usize]) -> Vec<usize> {
        (0..self.lanes.len())
            .filter(|&i| {
                let Some(lane) = self.lanes.get(i) else {
                    return false;
                };
                match lane.other_hand {
                    None => true,
                    Some(_) => {
                        // Split: both halves show. Collapsed: the right
                        // hand stands for the piece, because that is the
                        // hand most parts lead with — and *some* row has
                        // to be the one you draw on when sticking does
                        // not matter.
                        split.contains(&i)
                            || self.other_hand_row(i).is_some_and(|o| split.contains(&o))
                            || lane.hand == Some(Hand::Right)
                    }
                }
            })
            .collect()
    }

    pub fn general_midi() -> Self {
        let lanes = [
            (35, "Kick 2", None),
            (36, "Kick", None),
            (37, "Side Stick", None),
            (38, "Snare", None),
            (39, "Clap", None),
            (40, "Snare 2", None),
            (41, "Tom Floor L", None),
            (42, "HH Closed", Some(1)),
            (43, "Tom Floor H", None),
            (44, "HH Pedal", Some(1)),
            (45, "Tom Low", None),
            (46, "HH Open", Some(1)),
            (47, "Tom Low-Mid", None),
            (48, "Tom Hi-Mid", None),
            (49, "Crash", None),
            (50, "Tom High", None),
            (51, "Ride", None),
            (52, "China", None),
            (53, "Ride Bell", None),
            (54, "Tambourine", None),
            (55, "Splash", None),
            (57, "Crash 2", None),
            (59, "Ride 2", None),
        ];
        Self {
            name: "General MIDI",
            lanes: lanes
                .iter()
                .map(|&(pitch, name, group)| DrumLane {
                    pitch,
                    name: name.to_string(),
                    group,
                    hand: None,
                    other_hand: None,
                    piece: None,
                })
                .collect(),
        }
    }

    pub fn row_of_pitch(&self, pitch: i32) -> Option<usize> {
        self.lanes.iter().position(|l| l.pitch == pitch)
    }

    pub fn pitch_of_row(&self, row: usize) -> Option<i32> {
        self.lanes.get(row).map(|l| l.pitch)
    }
}

/// What the vertical axis means.
/// The bands a percussive track's hits are sorted into.
///
/// Split points are spectral centroids in Hz, ascending, and there is
/// one more band than there are splits. Three by default — kick low,
/// snare middle, cymbals high — which is enough to read a kit at a
/// glance without pretending to identify the drums.
///
/// Deliberately not a [`DrumMap`]: this makes no claim to know a kick
/// from a floor tom, only that one hit is darker than another. Naming
/// lanes is what [`crate::Mode::Drums`] is for, and that mode has MIDI
/// note numbers to name them from.
#[derive(Clone, Debug, PartialEq)]
pub struct SliceBands {
    /// Ascending centroid boundaries in Hz. `n` splits, `n + 1` bands.
    pub splits: Vec<f64>,
    pub names: Vec<String>,
}

impl Default for SliceBands {
    fn default() -> Self {
        Self {
            // 250 Hz and 2 kHz: below the first is body, above the
            // second is almost entirely cymbal and snare snap. The
            // exact numbers matter less than that they are editable —
            // a shaker track wants both moved up.
            splits: vec![250.0, 2000.0],
            names: vec!["Low".into(), "Mid".into(), "High".into()],
        }
    }
}

impl SliceBands {
    pub fn count(&self) -> usize {
        self.splits.len() + 1
    }

    /// Which band a hit with this spectral centroid belongs to.
    ///
    /// Banding is a view, not a commitment: a slice keeps its measured
    /// centroid, so moving a split re-sorts without losing anything.
    pub fn band_of(&self, centroid_hz: f64) -> usize {
        self.splits
            .iter()
            .position(|&s| centroid_hz < s)
            .unwrap_or(self.splits.len())
    }

    pub fn name(&self, band: usize) -> &str {
        self.names.get(band).map(String::as_str).unwrap_or("")
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum RowSpace {
    /// Rows are MIDI pitches. Audio, MPE, plain MIDI, vocals.
    #[default]
    Pitch,
    /// Rows are drum lanes; `row` indexes [`DrumMap::lanes`].
    Drums(DrumMap),
    /// Rows are strings; the note's `fret` completes the position.
    Strings(StringTuning),
    /// Rows are spectral bands; a note is a percussive hit, not a
    /// pitch. See [`SliceBands`].
    Bands(SliceBands),
}

impl RowSpace {
    /// Whether two row spaces are the same *kind*, ignoring their
    /// contents.
    ///
    /// What re-applying a mode preset needs to know: a document already
    /// in band space keeps its own splits, because the user may have
    /// moved them and a preset must not undo that. A document in some
    /// other space is converted.
    pub fn same_kind(&self, other: &RowSpace) -> bool {
        core::mem::discriminant(self) == core::mem::discriminant(other)
    }

    /// Inclusive row range the roll can show.
    pub fn bounds(&self) -> (i32, i32) {
        match self {
            RowSpace::Pitch => (0, 127),
            RowSpace::Drums(m) => (0, m.lanes.len().saturating_sub(1) as i32),
            // A string roll is a *pitch* roll: rows are MIDI pitches
            // and the string is an annotation on the note. Bounded by
            // the neck rather than by 0..127, because a note above the
            // top fret is not playable on this instrument.
            RowSpace::Strings(t) => (
                t.open(0),
                t.open(t.strings().saturating_sub(1)) + t.frets as i32,
            ),
            RowSpace::Bands(b) => (0, b.count().saturating_sub(1) as i32),
        }
    }

    /// Rows are drawn with the *last* index on top for pitch (higher
    /// pitch is higher), but strings read like tab — string 0 is the
    /// lowest and belongs at the bottom, which is the same direction.
    /// Drum lanes follow their declared order, kick at the bottom.
    pub fn row_label(&self, row: i32) -> String {
        match self {
            RowSpace::Pitch => tuning::note_name(row),
            // Collapsed by default here; the editor's own labelling
            // knows which pieces are open and calls `display_name`
            // directly.
            RowSpace::Drums(m) => m.display_name(row.max(0) as usize, false),
            // Rows are pitches here too, so they are named as pitches.
            // Which string a note is on is the note's business, not the
            // row's — one pitch is reachable on several strings.
            RowSpace::Strings(_) => tuning::note_name(row),
            RowSpace::Bands(b) => b.name(row.max(0) as usize).to_string(),
        }
    }

    /// Rows that get a heavier divider (C in pitch space, every string
    /// in a string roll, group boundaries in drums).
    pub fn is_major_row(&self, row: i32) -> bool {
        match self {
            RowSpace::Pitch => row.rem_euclid(12) == 0,
            RowSpace::Drums(_) => false,
            RowSpace::Strings(_) => row.rem_euclid(12) == 0,
            // Every band boundary is a real division of the material.
            RowSpace::Bands(_) => true,
        }
    }

    /// How many semitones one row is worth, when a pitch curve is drawn
    /// against it.
    ///
    /// In pitch space a row *is* a semitone and this is 1. Everywhere
    /// else the row axis is not pitch at all, and drawing a semitone
    /// curve at one-row-per-semitone is a unit error: a whole-tone bend
    /// on a string roll would deflect the line across two neighbouring
    /// strings.
    ///
    /// Two semitones per string row is the whole-step bend a guitarist
    /// actually plays, drawn as exactly one row of deflection — the
    /// largest interval that reads as "a bend" rather than "a note
    /// somewhere else".
    pub fn semitones_per_row(&self) -> f64 {
        match self {
            // A string roll is a pitch roll, so a row is a semitone
            // here exactly as it is in pitch space.
            RowSpace::Pitch | RowSpace::Strings(_) => 1.0,
            // Neither space has a pitch axis to scale against; the
            // curves drawn over them are decoration, not a reading.
            RowSpace::Drums(_) | RowSpace::Bands(_) => 1.0,
        }
    }

    /// Black-key shading only means anything in pitch space.
    pub fn is_accidental(&self, row: i32) -> bool {
        match self {
            RowSpace::Pitch | RowSpace::Strings(_) => {
                matches!(row.rem_euclid(12), 1 | 3 | 6 | 8 | 10)
            }
            _ => false,
        }
    }

    /// Sounding MIDI pitch of a note in this space.
    pub fn pitch_of(&self, note: &Note) -> i32 {
        match self {
            RowSpace::Pitch => note.row,
            RowSpace::Drums(m) => m.pitch_of_row(note.row.max(0) as usize).unwrap_or(note.row),
            RowSpace::Strings(_) => note.row,
            // A slice has no pitch. Callers that need one for playback
            // get silence rather than a number that would sound.
            RowSpace::Bands(_) => 0,
        }
    }

    /// Row a sounding pitch belongs on, for import.
    pub fn row_of_pitch(&self, pitch: i32) -> Option<i32> {
        match self {
            RowSpace::Pitch => Some(pitch.clamp(0, 127)),
            RowSpace::Drums(m) => m.row_of_pitch(pitch).map(|r| r as i32),
            RowSpace::Strings(_) => Some(pitch),
            // Nothing imports *into* band space by pitch: a band comes
            // from a measured centroid, and a MIDI note has none.
            RowSpace::Bands(_) => None,
        }
    }

    /// What the note body prints.
    ///
    /// A lyric always wins: in a vocal editor the syllable *is* the
    /// note's identity, and the pitch is already shown by its row.
    pub fn note_label(&self, note: &Note) -> Option<String> {
        if let Some(text) = &note.text {
            return Some(text.clone());
        }
        match self {
            RowSpace::Pitch => Some(tuning::note_name(note.row)),
            RowSpace::Drums(_) => None,
            // The fret number is the whole point of a tab view — and it
            // is computed, because the note stores the string and the
            // row is the pitch.
            RowSpace::Strings(t) => Some(
                fret_of(note, t)
                    .map(|f| f.to_string())
                    .unwrap_or_else(|| "?".into()),
            ),
            // The band is already the row, and a hit is too narrow to
            // print in anyway.
            RowSpace::Bands(_) => None,
        }
    }

    /// How a note body is drawn in this space.
    pub fn note_shape(&self) -> NoteShape {
        match self {
            RowSpace::Drums(_) => NoteShape::Triangle,
            // A hit is an attack with a decay, which is what the
            // triangle already says — and it puts the flat edge on the
            // onset, the one part of a slice you align against.
            RowSpace::Bands(_) => NoteShape::Triangle,
            _ => NoteShape::Bar,
        }
    }

    /// Per-row colour, when the row itself carries meaning.
    ///
    /// A string roll colours by *string*, not by pitch class: on a
    /// guitar the string a note is played on is the thing you are
    /// tracking, and pitch-class colour would scatter one string's part
    /// across six hues. Drums colour by kit section for the same
    /// reason. Pitch space returns `None` and keeps pitch-class colour.
    /// Background tint for a row band.
    ///
    /// Only drum rolls have one: a piano roll already has black and
    /// white keys to steer by, and a string roll has six rows. A kit has
    /// thirty-nine and needs the grouping.
    pub fn row_background(&self, row: i32) -> Option<&'static str> {
        match self {
            RowSpace::Drums(m) => m.family_of(row.max(0) as usize).map(|f| f.band()),
            _ => None,
        }
    }

    /// Whether a row opens a new family — where a divider goes.
    pub fn starts_group(&self, row: i32) -> bool {
        match self {
            RowSpace::Drums(m) => m.starts_family(row.max(0) as usize),
            _ => false,
        }
    }

    pub fn row_color(&self, row: i32) -> Option<&'static str> {
        match self {
            RowSpace::Pitch => None,
            // A row is a pitch, and a pitch is reachable on several
            // strings — so the colour belongs to the *note*, via
            // [`string_color`], not to the row it sits on.
            RowSpace::Strings(_) => None,
            RowSpace::Drums(m) => {
                let name = m.lanes.get(row.max(0) as usize)?.name.as_str();
                Some(drum_color(name))
            }
            // Dark to bright as the band rises, so a kit reads the way
            // it sounds without naming a single drum.
            RowSpace::Bands(_) => Some(BAND_COLORS[row.max(0) as usize % BAND_COLORS.len()]),
        }
    }
}

/// Band colours, dark to bright as the centroid rises.
const BAND_COLORS: [&str; 4] = ["#7a5cff", "#3fa9f5", "#4fd1a5", "#ffd166"];

/// How a note body is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteShape {
    /// A rectangle spanning the note's length.
    Bar,
    /// A right-pointing triangle with its flat edge on the onset.
    ///
    /// A drum hit has no meaningful length, so it needs a fixed-size
    /// head rather than a bar — a bar invites editing a duration
    /// nothing will hear. A triangle beats a diamond because its flat
    /// edge sits exactly on the attack, where a diamond's widest point
    /// is its middle and the onset has to be inferred.
    Triangle,
}

/// The colour of a note fingered on `string`.
///
/// This is what makes a pitch roll readable as a guitar part: the row
/// says which note, the colour says where it was played, and the two
/// together are what a fret number is computed from.
pub fn string_color(string: u8) -> &'static str {
    STRING_COLORS[string as usize % STRING_COLORS.len()]
}

/// The fret a note is played at: its pitch, less the string's open
/// pitch.
///
/// `None` when the note carries no string — a plain MIDI note on a
/// guitar track is a real thing, and it has no fret until someone says
/// which string it is on.
pub fn fret_of(note: &Note, tuning: &StringTuning) -> Option<i32> {
    let string = note.string?;
    Some(note.row - tuning.open(string as usize))
}

/// String colours, low to high. Warm at the bottom, cool at the top,
/// so the register reads without counting rows.
pub const STRING_COLORS: [&str; 8] = [
    "#f97316", // low — orange
    "#f59e0b", "#eab308", "#84cc16", "#22d3ee", "#60a5fa", // high — blue
    "#a78bfa", "#f472b6",
];

/// The kit-section palette. One set of hues for a drum wherever it
/// appears — a MIDI map row, an audio role lane, a legend — so "the
/// kick is red" is a fact about the editor, not about one view.
pub const DRUM_KICK_COLOR: &str = "#ef4444";
pub const DRUM_SNARE_COLOR: &str = "#f59e0b";
pub const DRUM_TOM_COLOR: &str = "#a3e635";
pub const DRUM_HAT_COLOR: &str = "#22d3ee";
pub const DRUM_RIDE_COLOR: &str = "#60a5fa";
pub const DRUM_CYMBAL_COLOR: &str = "#f472b6";
pub const DRUM_OTHER_COLOR: &str = "#c084fc";

/// Kit-section colour, so a groove reads at a glance.
fn drum_color(name: &str) -> &'static str {
    let n = name.to_ascii_lowercase();

    // The FTS map abbreviates: `K`, `S`, `T1`..`T4`, `H-`, `C-`, `R-`.
    // Matched first and exactly, because a substring test on one letter
    // would colour half the kit by accident — and without this every
    // FTS row falls through to the same purple, which is how the whole
    // kit came out one colour the first time it was drawn.
    let head = n.split([' ', '-']).next().unwrap_or("");
    match head {
        "k" | "kl" => return DRUM_KICK_COLOR,
        "s" | "sr" => return DRUM_SNARE_COLOR,
        "t1" | "t2" | "t3" | "t4" => return DRUM_TOM_COLOR,
        "h" => return DRUM_HAT_COLOR,
        "r" => return DRUM_RIDE_COLOR,
        "c" => return DRUM_CYMBAL_COLOR,
        _ => {}
    }

    if n.contains("kick") {
        DRUM_KICK_COLOR
    } else if n.contains("snare") || n.contains("stick") || n.contains("clap") {
        DRUM_SNARE_COLOR
    } else if n.contains("hh") || n.contains("hat") {
        DRUM_HAT_COLOR
    } else if n.contains("tom") {
        DRUM_TOM_COLOR
    } else if n.contains("ride") {
        DRUM_RIDE_COLOR
    } else {
        DRUM_OTHER_COLOR
    }
}

/// Playing techniques, following Ample Sound's Riffer set — the
/// vocabulary a sampled guitar/bass actually responds to.
///
/// Which of these an instrument supports varies; the editor offers them
/// all and the host filters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Articulation {
    Sustain,
    PalmMute,
    NaturalHarmonic,
    Staccato,
    Slap,
    Pop,
    Tap,
    SlideIn,
    SlideOut,
    HammerOn,
    PullOff,
    LegatoSlide,
    Bend,
    Vibrato,
    SlideGuitar,
    /// Vocal/percussion dead note.
    Mute,
}

impl Articulation {
    pub const ALL: [Articulation; 16] = [
        Articulation::Sustain,
        Articulation::PalmMute,
        Articulation::NaturalHarmonic,
        Articulation::Staccato,
        Articulation::Slap,
        Articulation::Pop,
        Articulation::Tap,
        Articulation::SlideIn,
        Articulation::SlideOut,
        Articulation::HammerOn,
        Articulation::PullOff,
        Articulation::LegatoSlide,
        Articulation::Bend,
        Articulation::Vibrato,
        Articulation::SlideGuitar,
        Articulation::Mute,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Articulation::Sustain => "Sustain",
            Articulation::PalmMute => "Palm Mute",
            Articulation::NaturalHarmonic => "Nat. Harmonic",
            Articulation::Staccato => "Staccato",
            Articulation::Slap => "Slap",
            Articulation::Pop => "Pop",
            Articulation::Tap => "Tap",
            Articulation::SlideIn => "Slide In",
            Articulation::SlideOut => "Slide Out",
            Articulation::HammerOn => "Hammer On",
            Articulation::PullOff => "Pull Off",
            Articulation::LegatoSlide => "Legato Slide",
            Articulation::Bend => "Bend",
            Articulation::Vibrato => "Vibrato",
            Articulation::SlideGuitar => "Slide Guitar",
            Articulation::Mute => "Dead Note",
        }
    }

    /// Single-glyph badge for the note body.
    pub fn glyph(&self) -> &'static str {
        match self {
            Articulation::Sustain => "",
            Articulation::PalmMute => "P.M.",
            Articulation::NaturalHarmonic => "◇",
            Articulation::Staccato => "·",
            Articulation::Slap => "S",
            Articulation::Pop => "P",
            Articulation::Tap => "T",
            Articulation::SlideIn => "╱",
            Articulation::SlideOut => "╲",
            Articulation::HammerOn => "H",
            Articulation::PullOff => "p",
            Articulation::LegatoSlide => "≈",
            Articulation::Bend => "↑",
            Articulation::Vibrato => "∿",
            Articulation::SlideGuitar => "⌇",
            Articulation::Mute => "✕",
        }
    }

    /// Techniques that only make sense joined to the following note on
    /// the same string. Riffer's rule: the legato is marked on the
    /// *first* note of the pair.
    pub fn is_legato(&self) -> bool {
        matches!(
            self,
            Articulation::HammerOn | Articulation::PullOff | Articulation::LegatoSlide
        )
    }

    /// Natural harmonics only speak at these frets.
    pub fn valid_frets(&self) -> Option<&'static [u8]> {
        match self {
            Articulation::NaturalHarmonic => Some(&[5, 7, 9, 12, 17, 19, 24]),
            _ => None,
        }
    }
}
