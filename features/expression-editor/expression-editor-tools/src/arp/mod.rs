//! Chords in, arpeggios out.
//!
//! Ported from mrtnz's MArpeggiator (`MIDI Editor/
//! mrtnz_Arpeggiator(chord to arp).lua`, forum thread 283743). Same
//! treatment as [`crate::velocity`]: the DAW calls and the rtk widget
//! tree come out, and what's left is arithmetic you can unit test.
//!
//! ## The model
//!
//! 1. [`group_chords`] folds a run of notes into [`Chord`]s by time —
//!    notes that overlap or nearly touch are one chord, a gap starts the
//!    next.
//! 2. [`Arp::arpeggiate`] walks each chord from its start to its end,
//!    emitting one note per step, choosing the pitch by [`Direction`].
//!
//! The chord's own extent is the arp's extent: a whole-note C major
//! becomes a bar of sixteenths, and the chord after it picks up where it
//! ended. Nothing here decides what to do with the original notes —
//! replacing them is the caller's business (see `midi-tools-daw`).
//!
//! ## Divergences from the Lua
//!
//! Three of these are bug fixes, not preferences:
//!
//! - **Every arp note was emitted twice.** Upstream's multi-note branch
//!   calls `MIDI_InsertNote` for the gated note *and then* calls it again
//!   unconditionally at full length, so every step lands as a pair of
//!   stacked notes. It's why gate/length appears not to work there.
//! - **Ratchet duplicated its remainder.** `splitNote` puts the
//!   leftover-tail insert *inside* the per-ratchet loop, emitting it
//!   `ratchet` times at the same position.
//! - **Chord sorting read a nil.** `gatherChords` sorts each chord with
//!   `current_mode`, a global that is still nil at gather time (the real
//!   one is a local declared later inside `insertArpeggios`), so chords
//!   were always sorted descending regardless of direction.
//!
//! And two deliberate design changes:
//!
//! - **Steps cycle.** Upstream scans its step list in reverse and takes
//!   the first whose counter is divisible by its `step` field. Every step
//!   ships with `step = 1`, which the last entry matches every time — so
//!   in the default configuration the other three never fire. Here steps
//!   cycle in order, like [`crate::velocity::Pattern`], which is what the
//!   grid looks like it does.
//! - **Octaves are a range, not a dice roll.** Upstream shifts each note
//!   by `math.random(-octave, octave) * 12`, so the same settings give a
//!   different part every run and nothing is reproducible. [`Arp::octaves`]
//!   is the standard arpeggiator control instead: the pitch pool repeats
//!   an octave higher, so a 3-note chord at `octaves = 2` is a 6-note
//!   climb. Per-step [`Step::octave`] covers deliberate jumps.

mod chord;
mod cursor;
mod session;

pub use chord::{Chord, ChordNote, DEFAULT_GAP_PPQ, TimedNote, group_chords};
pub use cursor::{Cursor, Direction};
pub use session::ArpSession;

use crate::velocity::{MAX_VELOCITY, MIN_VELOCITY};

/// Ticks per quarter note. REAPER's MIDI resolution, and the unit every
/// `_ppq` field here is in.
pub const PPQ: f64 = 960.0;

/// A note the arpeggiator produced.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArpNote {
    pub start_ppq: f64,
    pub length_ppq: f64,
    pub pitch: u8,
    pub velocity: u8,
}

impl ArpNote {
    pub fn end_ppq(&self) -> f64 {
        self.start_ppq + self.length_ppq
    }
}

/// One entry in the step grid.
///
/// Steps cycle over the arp's positions, so a 3-step grid over a bar of
/// sixteenths gives a 3-against-4 pattern — the same idea as the velocity
/// tool's [`crate::velocity::Pattern`], carrying more than velocity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    /// Step length in PPQ. This is the *rate* — how long until the next
    /// note starts.
    pub rate_ppq: f64,
    /// Velocity for this step. `None` inherits the chord note's own.
    pub velocity: Option<u8>,
    /// Octave offset applied on top of the direction's pitch choice.
    pub octave: i8,
    /// Subdivide this step into N equal notes. 0 and 1 both mean "one".
    pub ratchet: u8,
    /// Fraction of the step the note sounds for. 1.0 is legato-to-the-
    /// next-step, 0.5 is staccato. Values above 1.0 overlap deliberately.
    pub gate: f64,
}

impl Default for Step {
    fn default() -> Self {
        Self {
            rate_ppq: PPQ / 4.0,
            velocity: None,
            octave: 0,
            ratchet: 1,
            gate: 0.9,
        }
    }
}

impl Step {
    /// The step's ratchet count, normalized — 0 and 1 both mean one note.
    fn ratchet_count(&self) -> u32 {
        u32::from(self.ratchet.max(1))
    }
}

/// The arpeggiator.
#[derive(Clone, Debug, PartialEq)]
pub struct Arp {
    pub direction: Direction,
    /// How many octaves the pitch pool spans. 1 is the chord as played;
    /// 2 adds the same chord an octave up, and so on.
    pub octaves: u8,
    /// The step grid. Empty means one default step, i.e. a uniform arp.
    pub steps: Vec<Step>,
}

impl Default for Arp {
    fn default() -> Self {
        Self {
            direction: Direction::Up,
            octaves: 1,
            steps: vec![Step::default()],
        }
    }
}

impl Arp {
    /// A uniform arp at `rate_ppq` — the common case, no step grid.
    pub fn uniform(direction: Direction, rate_ppq: f64) -> Self {
        Self {
            direction,
            octaves: 1,
            steps: vec![Step {
                rate_ppq,
                ..Step::default()
            }],
        }
    }

    fn step_at(&self, i: usize) -> Step {
        if self.steps.is_empty() {
            Step::default()
        } else {
            self.steps[i % self.steps.len()]
        }
    }

    /// The pitches available to the cursor, low to high.
    ///
    /// The chord repeated up by [`Arp::octaves`]. Pitches that would run
    /// past 127 are dropped rather than wrapped — a wrapped octave is a
    /// wrong note, and silently transposing the top of a climb down two
    /// octaves is worse than the climb being short.
    fn pitch_pool(&self, chord: &Chord) -> Vec<ChordNote> {
        let mut pool = Vec::with_capacity(chord.notes.len() * usize::from(self.octaves.max(1)));
        for octave in 0..i16::from(self.octaves.max(1)) {
            for note in &chord.notes {
                let pitch = i16::from(note.pitch) + octave * 12;
                if pitch <= i16::from(MAX_VELOCITY) {
                    pool.push(ChordNote {
                        pitch: pitch as u8,
                        velocity: note.velocity,
                    });
                }
            }
        }
        pool
    }

    /// Arpeggiate one chord across its own extent.
    pub fn arpeggiate_chord(&self, chord: &Chord) -> Vec<ArpNote> {
        let pool = self.pitch_pool(chord);
        if pool.is_empty() || chord.length_ppq() <= 0.0 {
            return Vec::new();
        }

        let mut out = Vec::new();
        let mut cursor = Cursor::new(self.direction, pool.len());
        let mut at = chord.start_ppq;
        let mut i = 0usize;

        // A step whose rate rounds to nothing would spin forever; the
        // guard is on the rate rather than an iteration counter so the
        // failure is "no notes" rather than "10,000 notes", which is what
        // upstream's `infiniteLoopProtection` leaves you with.
        while at < chord.end_ppq {
            let step = self.step_at(i);
            let rate = step.rate_ppq;
            // NaN included deliberately: a NaN rate must break, and
            // `rate <= 0.0` is false for NaN.
            if rate.is_nan() || rate <= 0.0 {
                break;
            }

            // The last step is trimmed to the chord rather than allowed
            // to hang past it.
            let remaining = chord.end_ppq - at;
            let span = rate.min(remaining);

            let note = pool[cursor.next_index()];
            let pitch = (i16::from(note.pitch) + i16::from(step.octave) * 12)
                .clamp(0, i16::from(MAX_VELOCITY)) as u8;
            let velocity = step
                .velocity
                .unwrap_or(note.velocity)
                .clamp(MIN_VELOCITY, MAX_VELOCITY);

            let ratchets = step.ratchet_count();
            let sub = span / f64::from(ratchets);
            for r in 0..ratchets {
                let start = at + sub * f64::from(r);
                let length = (sub * step.gate.max(0.01)).min(chord.end_ppq - start);
                if length > 0.0 {
                    out.push(ArpNote {
                        start_ppq: start,
                        length_ppq: length,
                        pitch,
                        velocity,
                    });
                }
            }

            at += span;
            i += 1;
        }

        out
    }

    /// Arpeggiate every chord, in order.
    pub fn arpeggiate(&self, chords: &[Chord]) -> Vec<ArpNote> {
        chords
            .iter()
            .flat_map(|c| self.arpeggiate_chord(c))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(start: f64, end: f64, pitches: &[u8]) -> Chord {
        Chord {
            start_ppq: start,
            end_ppq: end,
            notes: pitches
                .iter()
                .map(|&pitch| ChordNote {
                    pitch,
                    velocity: 96,
                })
                .collect(),
        }
    }

    /// C major triad over one bar.
    fn c_major_bar() -> Chord {
        chord(0.0, PPQ * 4.0, &[60, 64, 67])
    }

    #[test]
    fn a_uniform_arp_fills_the_chord_with_evenly_spaced_notes() {
        let out = Arp::uniform(Direction::Up, PPQ / 4.0).arpeggiate_chord(&c_major_bar());
        assert_eq!(out.len(), 16, "a bar of sixteenths");
        for (i, note) in out.iter().enumerate() {
            assert_eq!(note.start_ppq, i as f64 * (PPQ / 4.0));
        }
    }

    #[test]
    fn up_climbs_the_chord_and_wraps() {
        let out = Arp::uniform(Direction::Up, PPQ / 4.0).arpeggiate_chord(&c_major_bar());
        let pitches: Vec<u8> = out.iter().take(7).map(|n| n.pitch).collect();
        assert_eq!(pitches, [60, 64, 67, 60, 64, 67, 60]);
    }

    #[test]
    fn down_descends() {
        let out = Arp::uniform(Direction::Down, PPQ / 4.0).arpeggiate_chord(&c_major_bar());
        let pitches: Vec<u8> = out.iter().take(6).map(|n| n.pitch).collect();
        assert_eq!(pitches, [67, 64, 60, 67, 64, 60]);
    }

    #[test]
    fn updown_turns_around_without_repeating_the_ends() {
        let out = Arp::uniform(Direction::UpDown, PPQ / 4.0).arpeggiate_chord(&c_major_bar());
        let pitches: Vec<u8> = out.iter().take(8).map(|n| n.pitch).collect();
        assert_eq!(pitches, [60, 64, 67, 64, 60, 64, 67, 64]);
    }

    #[test]
    fn octaves_extend_the_pitch_pool_upward() {
        let arp = Arp {
            octaves: 2,
            ..Arp::uniform(Direction::Up, PPQ / 4.0)
        };
        let out = arp.arpeggiate_chord(&c_major_bar());
        let pitches: Vec<u8> = out.iter().take(7).map(|n| n.pitch).collect();
        assert_eq!(pitches, [60, 64, 67, 72, 76, 79, 60]);
    }

    #[test]
    fn octaves_never_wrap_past_the_top_of_the_range() {
        let arp = Arp {
            octaves: 4,
            ..Arp::uniform(Direction::Up, PPQ / 4.0)
        };
        let out = arp.arpeggiate_chord(&chord(0.0, PPQ * 4.0, &[120]));
        assert!(out.iter().all(|n| n.pitch <= 127), "{out:?}");
        // 120 and 120+12=132 is out of range, so the pool is just 120.
        assert!(out.iter().all(|n| n.pitch == 120));
    }

    #[test]
    fn gate_shortens_the_note_without_moving_the_next_one() {
        let arp = Arp {
            steps: vec![Step {
                rate_ppq: PPQ / 4.0,
                gate: 0.5,
                ..Step::default()
            }],
            ..Arp::default()
        };
        let out = arp.arpeggiate_chord(&c_major_bar());
        assert_eq!(out[0].length_ppq, PPQ / 8.0, "half the step");
        assert_eq!(out[1].start_ppq, PPQ / 4.0, "but the grid is unchanged");
    }

    #[test]
    fn each_step_is_emitted_exactly_once() {
        // The upstream bug: every note inserted twice, once gated and
        // once at full length. Stacked duplicates at the same position
        // and pitch are what this pins against.
        let out = Arp::uniform(Direction::Up, PPQ / 4.0).arpeggiate_chord(&c_major_bar());
        for pair in out.windows(2) {
            assert!(
                pair[0].start_ppq != pair[1].start_ppq || pair[0].pitch != pair[1].pitch,
                "duplicate note at {:?}",
                pair[0]
            );
        }
    }

    #[test]
    fn ratchet_subdivides_a_step_into_equal_notes() {
        let arp = Arp {
            steps: vec![Step {
                rate_ppq: PPQ,
                ratchet: 3,
                gate: 1.0,
                ..Step::default()
            }],
            ..Arp::default()
        };
        let out = arp.arpeggiate_chord(&chord(0.0, PPQ, &[60]));
        assert_eq!(out.len(), 3, "one step, three ratchets — no remainder note");
        assert_eq!(out[0].start_ppq, 0.0);
        assert_eq!(out[1].start_ppq, PPQ / 3.0);
        assert_eq!(out[2].start_ppq, PPQ / 3.0 * 2.0);
        assert!(out.iter().all(|n| (n.length_ppq - PPQ / 3.0).abs() < 1e-9));
    }

    #[test]
    fn a_ratchet_of_zero_means_one_note() {
        let arp = Arp {
            steps: vec![Step {
                rate_ppq: PPQ,
                ratchet: 0,
                ..Step::default()
            }],
            ..Arp::default()
        };
        assert_eq!(arp.arpeggiate_chord(&chord(0.0, PPQ, &[60])).len(), 1);
    }

    #[test]
    fn steps_cycle_in_order() {
        let arp = Arp {
            steps: vec![
                Step {
                    rate_ppq: PPQ / 2.0,
                    velocity: Some(100),
                    ..Step::default()
                },
                Step {
                    rate_ppq: PPQ / 2.0,
                    velocity: Some(40),
                    ..Step::default()
                },
            ],
            ..Arp::default()
        };
        let out = arp.arpeggiate_chord(&chord(0.0, PPQ * 2.0, &[60]));
        let vels: Vec<u8> = out.iter().map(|n| n.velocity).collect();
        assert_eq!(vels, [100, 40, 100, 40], "every step must get a turn");
    }

    #[test]
    fn a_step_with_no_velocity_inherits_the_chord_note() {
        let mut c = c_major_bar();
        c.notes[0].velocity = 30;
        let out = Arp::uniform(Direction::Up, PPQ / 4.0).arpeggiate_chord(&c);
        assert_eq!(out[0].velocity, 30, "the low C keeps its own velocity");
    }

    #[test]
    fn nothing_is_emitted_past_the_chord() {
        let out = Arp::uniform(Direction::Up, PPQ / 3.0).arpeggiate_chord(&c_major_bar());
        let end = PPQ * 4.0;
        assert!(out.iter().all(|n| n.end_ppq() <= end + 1e-9), "{out:?}");
    }

    #[test]
    fn a_zero_rate_terminates_instead_of_spinning() {
        let arp = Arp {
            steps: vec![Step {
                rate_ppq: 0.0,
                ..Step::default()
            }],
            ..Arp::default()
        };
        assert!(arp.arpeggiate_chord(&c_major_bar()).is_empty());
    }

    #[test]
    fn an_empty_chord_produces_nothing() {
        let out = Arp::default().arpeggiate_chord(&chord(0.0, PPQ, &[]));
        assert!(out.is_empty());
    }

    #[test]
    fn a_single_note_arpeggiates_into_a_repeat() {
        // MArpeggiator's drum-pattern case: one note becomes a pulse.
        let out = Arp::uniform(Direction::Up, PPQ / 4.0).arpeggiate_chord(&chord(0.0, PPQ, &[36]));
        assert_eq!(out.len(), 4);
        assert!(out.iter().all(|n| n.pitch == 36));
    }

    #[test]
    fn chords_are_arpeggiated_independently_and_in_order() {
        let chords = [chord(0.0, PPQ, &[60, 64]), chord(PPQ, PPQ * 2.0, &[62, 65])];
        let out = Arp::uniform(Direction::Up, PPQ / 2.0).arpeggiate(&chords);
        let pitches: Vec<u8> = out.iter().map(|n| n.pitch).collect();
        assert_eq!(pitches, [60, 64, 62, 65]);
        // The second chord's arp restarts at its own start, not where the
        // first one's cursor happened to be.
        assert_eq!(out[2].start_ppq, PPQ);
    }
}
