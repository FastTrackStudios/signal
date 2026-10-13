//! Note names — the one spelling every rig, editor and tool shows.
//!
//! Sharps, and MIDI 60 is `C4` (key 0 is `C-1`), the convention the DAWs
//! FTS sits beside use.

/// The twelve pitch-class names, sharps, from C.
pub const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// A MIDI key as a note name: `60` → `"C4"`, `0` → `"C-1"`.
#[must_use]
pub fn note_name(key: u8) -> String {
    format!(
        "{}{}",
        NOTE_NAMES[usize::from(key % 12)],
        i32::from(key / 12) - 1
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn middle_c_is_c4_and_key_zero_is_c_minus_one() {
        assert_eq!(note_name(60), "C4");
        assert_eq!(note_name(0), "C-1");
        assert_eq!(note_name(61), "C#4");
        assert_eq!(note_name(127), "G9");
    }
}
