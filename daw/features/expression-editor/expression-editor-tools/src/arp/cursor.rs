//! Which note of the chord comes next.
//!
//! Pulled out of the arp loop because direction is the one part of an
//! arpeggiator with genuinely fiddly edge cases — what a two-note
//! ping-pong does, whether the turnaround repeats the top note — and
//! those are much easier to pin down as a sequence of indices than as
//! branches inside a note-emitting loop.

/// The order the arpeggiator walks the chord in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    /// Low to high, then back to the bottom.
    #[default]
    Up,
    /// High to low, then back to the top.
    Down,
    /// Up then down, without playing the top or bottom twice in a row.
    UpDown,
    /// Down then up, same turnaround rule.
    DownUp,
    /// A different note each time, never the same one twice running.
    Random,
}

impl Direction {
    pub const ALL: [Direction; 5] = [
        Direction::Up,
        Direction::Down,
        Direction::UpDown,
        Direction::DownUp,
        Direction::Random,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Direction::Up => "Up",
            Direction::Down => "Down",
            Direction::UpDown => "Up / Down",
            Direction::DownUp => "Down / Up",
            Direction::Random => "Random",
        }
    }
}

/// Yields chord-note indices in the chosen order.
#[derive(Clone, Debug)]
pub struct Cursor {
    direction: Direction,
    len: usize,
    /// Index the *next* call will return, for the linear directions.
    at: usize,
    /// Ping-pong travel direction.
    ascending: bool,
    /// Last index handed out, so Random can avoid an immediate repeat.
    last: Option<usize>,
    /// Deterministic per-cursor PRNG state for [`Direction::Random`].
    ///
    /// A cursor-local counter rather than a thread RNG so that
    /// arpeggiating the same chords twice gives the same part. An
    /// arpeggiator that produces something different every time you nudge
    /// a slider is impossible to dial in — which is exactly what
    /// upstream's `math.random` does.
    seed: u64,
}

impl Cursor {
    pub fn new(direction: Direction, len: usize) -> Self {
        Self {
            direction,
            len,
            at: match direction {
                Direction::Down | Direction::DownUp => len.saturating_sub(1),
                _ => 0,
            },
            ascending: !matches!(direction, Direction::DownUp),
            last: None,
            seed: 0x2545_F491_4F6C_DD1D,
        }
    }

    /// Seed the random direction, so a given seed reproduces a part.
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed | 1;
        self
    }

    /// SplitMix64. Inlined rather than pulling `rand` into the hot path:
    /// this needs to be reproducible and self-contained, not
    /// cryptographic.
    fn next_u64(&mut self) -> u64 {
        self.seed = self.seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.seed;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// The next chord-note index.
    pub fn next_index(&mut self) -> usize {
        if self.len <= 1 {
            self.last = Some(0);
            return 0;
        }
        let i = match self.direction {
            Direction::Up => {
                let i = self.at;
                self.at = (self.at + 1) % self.len;
                i
            }
            Direction::Down => {
                let i = self.at;
                self.at = if self.at == 0 {
                    self.len - 1
                } else {
                    self.at - 1
                };
                i
            }
            Direction::UpDown | Direction::DownUp => {
                let i = self.at;
                // Turn around *before* stepping, so the endpoint is
                // played once rather than twice — 0,1,2,1,0 not
                // 0,1,2,2,1,0.
                if self.ascending && self.at + 1 >= self.len {
                    self.ascending = false;
                } else if !self.ascending && self.at == 0 {
                    self.ascending = true;
                }
                self.at = if self.ascending {
                    self.at + 1
                } else {
                    self.at - 1
                };
                i
            }
            Direction::Random => {
                let mut i = (self.next_u64() % self.len as u64) as usize;
                if Some(i) == self.last {
                    // One nudge is enough to break a repeat and keeps the
                    // distribution close to uniform; rejection-sampling
                    // in a loop can stall on a short chord.
                    i = (i + 1) % self.len;
                }
                i
            }
        };
        self.last = Some(i);
        i
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn take(direction: Direction, len: usize, n: usize) -> Vec<usize> {
        let mut c = Cursor::new(direction, len);
        (0..n).map(|_| c.next_index()).collect()
    }

    #[test]
    fn up_cycles_upward() {
        assert_eq!(take(Direction::Up, 3, 7), [0, 1, 2, 0, 1, 2, 0]);
    }

    #[test]
    fn down_cycles_downward() {
        assert_eq!(take(Direction::Down, 3, 7), [2, 1, 0, 2, 1, 0, 2]);
    }

    #[test]
    fn updown_plays_the_turnaround_note_once() {
        assert_eq!(
            take(Direction::UpDown, 4, 10),
            [0, 1, 2, 3, 2, 1, 0, 1, 2, 3]
        );
    }

    #[test]
    fn downup_starts_at_the_top() {
        assert_eq!(
            take(Direction::DownUp, 4, 10),
            [3, 2, 1, 0, 1, 2, 3, 2, 1, 0]
        );
    }

    #[test]
    fn a_two_note_pingpong_alternates() {
        assert_eq!(take(Direction::UpDown, 2, 6), [0, 1, 0, 1, 0, 1]);
    }

    #[test]
    fn a_single_note_chord_always_yields_zero() {
        for d in Direction::ALL {
            assert_eq!(take(d, 1, 5), [0; 5], "{d:?}");
        }
    }

    #[test]
    fn random_never_repeats_a_note_back_to_back() {
        let seq = take(Direction::Random, 4, 200);
        assert!(seq.windows(2).all(|w| w[0] != w[1]), "{seq:?}");
    }

    #[test]
    fn random_stays_in_range_and_uses_the_whole_chord() {
        let seq = take(Direction::Random, 5, 400);
        assert!(seq.iter().all(|&i| i < 5));
        for i in 0..5 {
            assert!(seq.contains(&i), "index {i} never came up");
        }
    }

    #[test]
    fn random_is_reproducible_for_a_given_seed() {
        let run = |seed| {
            let mut c = Cursor::new(Direction::Random, 5).with_seed(seed);
            (0..50).map(|_| c.next_index()).collect::<Vec<_>>()
        };
        assert_eq!(run(7), run(7));
        assert_ne!(run(7), run(8));
    }
}
