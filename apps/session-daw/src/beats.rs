//! What a Time face's Beat switch locks to: the note lengths a delay
//! repeats at, a reverb's tail rings for and a modulation cycles over.
//!
//! The tables are signal's (`signal-guitar-proto::time`), so a face reads
//! the same divisions here as in the rig: `REVERB` and `MOD` start with a
//! "Free" entry (index 0 is running free); `DELAY` is the rig's own
//! division codes, with [`FREE_DIV`] for free.

/// A reverb's tail, locked: (label, length in beats).
pub const REVERB: [(&str, f64); 8] = [
    ("Free", 0.0),
    ("1/8", 0.5),
    ("1/4", 1.0),
    ("1/2", 2.0),
    ("1 bar", 4.0),
    ("2 bars", 8.0),
    ("4 bars", 16.0),
    ("8 bars", 32.0),
];

/// A delay's repeat, locked: (label, the rig's division code, length in
/// beats).
pub const DELAY: [(&str, usize, f64); 8] = [
    ("1/16", 4, 0.25),
    ("1/8T", 3, 1.0 / 3.0),
    ("1/8", 2, 0.5),
    ("1/8.", 1, 0.75),
    ("1/4T", 10, 2.0 / 3.0),
    ("1/4", 0, 1.0),
    ("1/4.", 8, 1.5),
    ("1/2", 9, 2.0),
];

/// The rig's division code for a delay running free.
pub const FREE_DIV: usize = 7;

/// A modulation's cycle, locked: (label, length in beats).
pub const MOD: [(&str, f64); 9] = [
    ("Free", 0.0),
    ("1/16", 0.25),
    ("1/8T", 1.0 / 3.0),
    ("1/8", 0.5),
    ("1/8.", 0.75),
    ("1/4", 1.0),
    ("1/2", 2.0),
    ("1 bar", 4.0),
    ("2 bars", 8.0),
];

/// The tempo the faces are locked to until they follow the song's: the
/// template's.
pub const BPM: f64 = 120.0;

/// Seconds a beat lasts at `bpm`.
#[must_use]
pub fn beat(bpm: f64) -> f64 {
    60.0 / bpm.max(1.0)
}

/// The entry of `lengths` (skipping the first `from`) nearest `beats`,
/// on a log scale — a dotted eighth is nearer a quarter than an eighth
/// is twice as long.
fn nearest(lengths: impl Iterator<Item = f64>, from: usize, beats: f64) -> usize {
    let beats = beats.max(1e-6).ln();
    lengths
        .enumerate()
        .skip(from)
        .min_by(|(_, a), (_, b)| (a.ln() - beats).abs().total_cmp(&(b.ln() - beats).abs()))
        .map_or(from, |(i, _)| i)
}

/// The reverb length nearest a tail of `seconds`.
#[must_use]
pub fn nearest_reverb(seconds: f64, bpm: f64) -> usize {
    nearest(REVERB.iter().map(|(_, b)| *b), 1, seconds / beat(bpm))
}

/// The delay division nearest a repeat every `ms`.
#[must_use]
pub fn nearest_delay(ms: f64, bpm: f64) -> usize {
    nearest(DELAY.iter().map(|(.., b)| *b), 0, ms / 1000.0 / beat(bpm))
}

/// The modulation length nearest a cycle of `hz`.
#[must_use]
pub fn nearest_mod(hz: f64, bpm: f64) -> usize {
    nearest(
        MOD.iter().map(|(_, b)| *b),
        1,
        1.0 / (hz.max(0.01) * beat(bpm)),
    )
}

/// Seconds as a Time face reads them.
#[must_use]
pub fn seconds_text(s: f64) -> String {
    if s < 1.0 {
        format!("{:.0} ms", s * 1000.0)
    } else if s < 10.0 {
        format!("{s:.1} s")
    } else {
        format!("{s:.0} s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_half_second_repeat_at_120_is_a_quarter() {
        assert_eq!(DELAY[nearest_delay(500.0, 120.0)].0, "1/4");
        assert_eq!(DELAY[nearest_delay(375.0, 120.0)].0, "1/8.");
    }

    #[test]
    fn a_two_second_tail_at_120_is_a_bar() {
        assert_eq!(REVERB[nearest_reverb(2.0, 120.0)].0, "1 bar");
        // Never "Free": the switch is to the beat.
        assert_ne!(nearest_reverb(0.0001, 120.0), 0);
    }
}
