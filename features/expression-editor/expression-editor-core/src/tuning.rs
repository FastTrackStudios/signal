//! Temperaments and microtonal pitch centers.
//!
//! A tuning shifts where a pitch class *sounds* without changing its
//! MIDI note number. The note rectangle stays on its integer row; the
//! offset lives in the pitch curve, and downstream that becomes an
//! integer note plus the required bend.

/// Cents offset from 12-TET for each of the twelve pitch classes,
/// relative to the key's tonic.
#[derive(Clone, Debug, PartialEq)]
pub struct Temperament {
    pub name: &'static str,
    pub description: &'static str,
    /// Index 0 is the tonic.
    pub offsets: [f64; 12],
}

impl Temperament {
    /// Cents offset for `midi` in the key of `key_pc` (0 = C).
    pub fn offset_cents(&self, midi: i32, key_pc: i32) -> f64 {
        let degree = (midi - key_pc).rem_euclid(12) as usize;
        self.offsets[degree]
    }

    /// The sounding center of `midi`, in fractional MIDI.
    pub fn center(&self, midi: i32, key_pc: i32) -> f64 {
        midi as f64 + self.offset_cents(midi, key_pc) / 100.0
    }

    pub fn is_equal(&self) -> bool {
        self.offsets.iter().all(|c| c.abs() < 1e-9)
    }
}

/// Ratio → cents, for deriving just and Pythagorean tables.
const fn _ratio_doc() {}

pub const EQUAL: Temperament = Temperament {
    name: "12-TET",
    description: "Equal temperament",
    offsets: [0.0; 12],
};

/// Pythagorean: a chain of pure 3:2 fifths.
pub const PYTHAGOREAN: Temperament = Temperament {
    name: "Pythagorean",
    description: "Chain of pure 3:2 fifths",
    offsets: [
        0.0, 13.685, 3.910, -5.865, 7.820, -1.955, 11.730, 1.955, 15.641, 5.865, -3.910, 9.775,
    ],
};

/// 5-limit just intonation.
pub const JUST_5_LIMIT: Temperament = Temperament {
    name: "5-limit Just",
    description: "Chromatic 5-limit ratio set",
    offsets: [
        0.0, 11.731, 3.910, 15.641, -13.686, -1.955, -9.776, 1.955, 13.686, -15.641, 17.596,
        -11.731,
    ],
};

/// Quarter-comma meantone — pure major thirds, fifths narrowed by a
/// quarter comma. Values follow the Scala/Huygens-Fokker reference
/// example: <https://www.huygens-fokker.org/scala/scl_format.html>
pub const MEANTONE: Temperament = Temperament {
    name: "1/4-comma Meantone",
    description: "Pure major thirds",
    offsets: [
        0.0, -24.0, -6.8, 10.3, -13.7, 3.4, -20.5, -3.4, -27.4, -10.3, 6.8, -17.1,
    ],
};

/// Maqam Rast as a practical 24-EDO approximation — the third and
/// seventh sit a quarter-tone low.
///
/// Structure per MaqamWorld
/// (<https://www.maqamworld.com/en/maqam/rast.php>). Real maqam
/// intonation is performance-dependent; this is a usable center, not a
/// universal table.
pub const RAST: Temperament = Temperament {
    name: "Maqam Rast",
    description: "24-EDO approximation (half-flat 3rd and 7th)",
    // Rast is C D E♭↑ F G A B♭↑ C — the half-flats sit on the major
    // third (4 semitones) and the major seventh (11), not on the minor
    // degrees.
    offsets: [
        0.0, 0.0, 0.0, 0.0, -50.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -50.0,
    ],
};

/// Maqam Bayati — half-flat second.
/// <https://www.maqamworld.com/en/maqam/bayati.php>
pub const BAYATI: Temperament = Temperament {
    name: "Maqam Bayati",
    description: "24-EDO approximation (half-flat 2nd)",
    offsets: [0.0, 0.0, -50.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
};

/// Resolve a temperament by name, for state that was stored by name.
///
/// Tuning cannot be persisted by value: `Temperament` holds
/// `&'static str` and a table of offsets, so round-tripping it would
/// mean storing the whole table and trusting it. Storing the name and
/// resolving it here means a preset whose offsets are corrected in a
/// later build takes effect, which is what you want from a preset.
///
/// Returns `None` for a name this build does not know, so the caller can
/// fall back rather than guess at a tuning.
pub fn by_name(name: &str) -> Option<&'static Temperament> {
    PRESETS.iter().copied().find(|t| t.name == name)
}

pub const PRESETS: [&Temperament; 6] = [
    &EQUAL,
    &PYTHAGOREAN,
    &JUST_5_LIMIT,
    &MEANTONE,
    &RAST,
    &BAYATI,
];

/// Active tuning context for the editor.
#[derive(Clone, Debug, PartialEq)]
pub struct Tuning {
    pub temperament: Temperament,
    /// Tonic pitch class, 0 = C.
    pub key_pc: i32,
    /// Offer plain semitone centers alongside the microtonal ones.
    /// Off snaps exclusively to the temperament.
    pub snap_12tet: bool,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            temperament: EQUAL,
            key_pc: 0,
            snap_12tet: true,
        }
    }
}

/// A candidate the pointer can snap to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapTarget {
    /// Sounding pitch in fractional MIDI.
    pub pitch: f64,
    /// The integer row this target belongs to.
    pub row: i32,
    /// Cents away from 12-TET (0 for an ordinary semitone).
    pub cents: f64,
}

impl SnapTarget {
    /// Off-ET targets get a distinct highlight in the UI.
    pub fn is_microtonal(&self) -> bool {
        self.cents.abs() > 0.5
    }
}

impl Tuning {
    /// Sounding center of a row.
    pub fn center(&self, row: i32) -> f64 {
        self.temperament.center(row, self.key_pc)
    }

    /// Cents this row is detuned from 12-TET.
    pub fn cents(&self, row: i32) -> f64 {
        self.temperament.offset_cents(row, self.key_pc)
    }

    /// Every snap candidate within `radius` semitones of `pitch`.
    pub fn targets_near(&self, pitch: f64, radius: f64) -> Vec<SnapTarget> {
        let lo = (pitch - radius).floor() as i32 - 1;
        let hi = (pitch + radius).ceil() as i32 + 1;
        let mut out = Vec::new();
        for row in lo..=hi {
            if !(0..=127).contains(&row) {
                continue;
            }
            let cents = self.cents(row);
            let tuned = row as f64 + cents / 100.0;
            if (tuned - pitch).abs() <= radius {
                out.push(SnapTarget {
                    pitch: tuned,
                    row,
                    cents,
                });
            }
            // With `snap_12tet` the plain semitone is offered as well,
            // so a tuned session can still land on ordinary centers.
            if self.snap_12tet && cents.abs() > 0.5 && (row as f64 - pitch).abs() <= radius {
                out.push(SnapTarget {
                    pitch: row as f64,
                    row,
                    cents: 0.0,
                });
            }
        }
        out
    }

    /// Nearest snap candidate to `pitch`.
    pub fn snap(&self, pitch: f64) -> SnapTarget {
        self.targets_near(pitch, 1.5)
            .into_iter()
            .min_by(|a, b| {
                (a.pitch - pitch)
                    .abs()
                    .partial_cmp(&(b.pitch - pitch).abs())
                    .unwrap_or(core::cmp::Ordering::Equal)
            })
            .unwrap_or(SnapTarget {
                pitch: pitch.round(),
                row: pitch.round() as i32,
                cents: 0.0,
            })
    }
}

const NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// `C4` for MIDI 60.
pub fn note_name(midi: i32) -> String {
    let pc = midi.rem_euclid(12) as usize;
    let octave = midi.div_euclid(12) - 1;
    format!("{}{}", NAMES[pc], octave)
}

pub fn pitch_class_name(pc: i32) -> &'static str {
    NAMES[pc.rem_euclid(12) as usize]
}

/// A 14-bit pitch-bend word for `semitones` at `bend_range`.
pub fn semitones_to_bend14(semitones: f64, bend_range: f64) -> u16 {
    let norm = (semitones / bend_range.max(1e-6)).clamp(-1.0, 1.0);
    ((norm * 8191.0).round() as i32 + 8192).clamp(0, 16383) as u16
}

/// Inverse of [`semitones_to_bend14`].
pub fn bend14_to_semitones(raw: u16, bend_range: f64) -> f64 {
    (raw as f64 - 8192.0) / 8191.0 * bend_range
}
