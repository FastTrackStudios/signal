//! MIDI note name ↔ number conversion utilities.
//!
//! Uses scientific pitch notation: C4 = MIDI 60.
//! Accidentals: `#` (sharp) and `b` (flat).
//! Examples: "G2" = 43, "C#4" = 61, "A#1" = 34.

use crate::SamplerError;

const NOTE_NAMES: &[(&str, u8)] = &[
    ("C", 0),
    ("C#", 1),
    ("Db", 1),
    ("D", 2),
    ("D#", 3),
    ("Eb", 3),
    ("E", 4),
    ("F", 5),
    ("F#", 6),
    ("Gb", 6),
    ("G", 7),
    ("G#", 8),
    ("Ab", 8),
    ("A", 9),
    ("A#", 10),
    ("Bb", 10),
    ("B", 11),
];

/// Parse a note name like "G2", "C#4", "A#-1" to a MIDI note number.
/// Middle C (C4) = 60.
pub fn note_name_to_midi(name: &str) -> Result<u8, SamplerError> {
    let bad = || SamplerError::BadNoteName(name.to_string());

    // Split into pitch class + octave (e.g. "C#" + "4", "C" + "-1"). The
    // octave begins at the first ASCII digit, or a leading '-' for negative
    // octaves like "C-1". Using `char_indices` (not byte slicing) keeps this
    // panic-free for empty and non-ASCII input — callers rely on `Err`, not a
    // panic, for malformed note tokens (e.g. an empty token from a `__` in a
    // sample filename, or a stray unicode character).
    let split = name
        .char_indices()
        .find(|&(i, c)| c.is_ascii_digit() || (c == '-' && i > 0))
        .map(|(i, _)| i)
        .ok_or_else(bad)?;
    let (pc, oct_str) = name.split_at(split);
    if pc.is_empty() || oct_str.is_empty() {
        return Err(bad());
    }

    let semitone = NOTE_NAMES
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(pc))
        .map(|(_, s)| *s)
        .ok_or_else(bad)?;

    let octave: i32 = oct_str.parse().map_err(|_| bad())?;

    let midi = (octave + 1) * 12 + semitone as i32;
    if (0..=127).contains(&midi) {
        Ok(midi as u8)
    } else {
        Err(bad())
    }
}

/// Convert a MIDI note number to its standard name in the note grid.
/// Returns e.g. "G#" for note class 8.
pub fn midi_to_note_class(midi: u8) -> &'static str {
    const CLASSES: &[&str] = &[
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    CLASSES[(midi % 12) as usize]
}

/// Octave number for a MIDI note (C4=60 → octave 4).
pub fn midi_to_octave(midi: u8) -> i32 {
    (midi as i32 / 12) - 1
}

/// Round a MIDI note to the nearest sampled note in `grid`.
///
/// `grid` is a list of note class names (e.g. `["G","A","B","C#","D#","F"]`),
/// describing which pitch classes are sampled. The function finds the closest
/// note in the grid (in semitone distance) across all octaves within the range
/// `[lowest, highest]`, then returns that MIDI note number.
pub fn nearest_grid_note(target: u8, grid: &[String], lowest: u8, highest: u8) -> u8 {
    let mut best = lowest;
    let mut best_dist = u8::MAX;

    let mut candidate = lowest;
    while candidate <= highest {
        let class = midi_to_note_class(candidate);
        if grid.iter().any(|g| g.eq_ignore_ascii_case(class)) {
            let dist = target.abs_diff(candidate);
            if dist < best_dist {
                best_dist = dist;
                best = candidate;
            }
        }
        candidate = candidate.saturating_add(1);
        if candidate == 0 {
            break;
        } // overflow guard
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_note_parsing() {
        assert_eq!(note_name_to_midi("C4").unwrap(), 60);
        assert_eq!(note_name_to_midi("G2").unwrap(), 43);
        assert_eq!(note_name_to_midi("A#1").unwrap(), 34);
        assert_eq!(note_name_to_midi("C#4").unwrap(), 61);
        assert_eq!(note_name_to_midi("C1").unwrap(), 24);
        // Flats and the lowest negative octave.
        assert_eq!(note_name_to_midi("Db4").unwrap(), 61);
        assert_eq!(note_name_to_midi("C-1").unwrap(), 0);
    }

    #[test]
    fn test_note_parsing_rejects_malformed_without_panicking() {
        // Previously these panicked (byte-slicing `&name[..1]`); now they must
        // return Err. Empty, non-ASCII-leading, digit-leading, and out-of-range.
        assert!(note_name_to_midi("").is_err());
        assert!(note_name_to_midi("é4").is_err());
        assert!(note_name_to_midi("♯4").is_err());
        assert!(note_name_to_midi("4").is_err());
        assert!(note_name_to_midi("C").is_err());
        assert!(note_name_to_midi("Z9").is_err());
        assert!(note_name_to_midi("C99").is_err());
    }

    #[test]
    fn test_nearest_grid() {
        // G whole-tone grid (CSS violins)
        let grid: Vec<String> = ["G", "A", "B", "C#", "D#", "F"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        // G2 = 43, A2 = 45, B2 = 47
        let lowest = note_name_to_midi("G2").unwrap();
        let highest = note_name_to_midi("C#6").unwrap();
        // G#2 (44) should round to G2 (43) or A2 (45) — both 1 semitone away; prefer lower
        let nearest = nearest_grid_note(44, &grid, lowest, highest);
        assert!(nearest == 43 || nearest == 45);
    }
}
