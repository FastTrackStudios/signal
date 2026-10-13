//! Beats and the timeline they are counted along, through the tempo map.

/// One beat of the project, and where it falls.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Beat {
    /// When it sounds, in seconds.
    pub at: f64,
    /// Which measure it is in, counted from one.
    pub measure: u32,
    /// Which beat of that measure, counted from one.
    pub beat: u32,
    /// How many beats this measure has — the signature in force.
    pub per_bar: u32,
    /// How long a beat lasts here, in seconds. A grid subdividing the
    /// beat needs this; multiplying one project tempo would put every
    /// line after a change in the wrong place.
    pub secs_per_beat: f64,
}

impl Beat {
    /// The first beat of a measure.
    #[must_use]
    pub const fn is_downbeat(self) -> bool {
        self.beat == 1
    }
}

/// The project's beat grid, walked through its tempo map.
///
/// Every bar line is where the tempo and the signature BEFORE it put
/// it. Counting the whole timeline by multiplying one nominal tempo
/// puts every bar after the first change somewhere it is not, and a
/// ruler that is wrong about where bar forty is, is a ruler nobody can
/// edit against.
///
/// Two rules, and they are the ones that matter:
///
/// - A **tempo** change alters how long the following beats take. The
///   count carries on through it: beat three is still beat three.
/// - A **signature** change starts a NEW MEASURE at that point, because
///   a bar of four and a bar of three cannot share a bar line. Anything
///   else would leave a measure that is part one signature and part
///   another, which is not a measure.
#[derive(Clone, Copy)]
pub struct Timeline<'a> {
    changes: &'a [daw_ui::studio::project::TempoChange],
}

impl<'a> Timeline<'a> {
    #[must_use]
    pub const fn new(changes: &'a [daw_ui::studio::project::TempoChange]) -> Self {
        Self { changes }
    }

    /// Every beat from the start of the project up to `to`, in order.
    ///
    /// From the start rather than from the visible left edge, because a
    /// measure number is a COUNT from the beginning — there is no way
    /// to know what bar you are looking at without having counted the
    /// ones before it. Stopped by `to`, and by `limit` so a corrupt
    /// tempo map cannot spin.
    #[must_use]
    pub fn beats(self, to: f64, limit: usize) -> Vec<Beat> {
        let mut out = Vec::new();
        let Some(first) = self.changes.first() else {
            return out;
        };
        let mut at = first.at;
        let mut measure = 1u32;
        let mut beat = 1u32;
        let mut index = 0usize;

        while at <= to && out.len() < limit {
            let change = &self.changes[index];
            let per_bar = change.beats_per_bar.max(1);
            let secs_per_beat = 60.0 / if change.bpm > 0.0 { change.bpm } else { 120.0 };
            out.push(Beat {
                at,
                measure,
                beat,
                per_bar,
                secs_per_beat,
            });

            let next_at = at + secs_per_beat;
            // Does a change fall inside the beat just laid down? If it
            // does, the grid restarts there rather than carrying the
            // old beat length across it.
            let upcoming = self
                .changes
                .iter()
                .enumerate()
                .skip(index + 1)
                .find(|(_, c)| c.at > at + 1e-9);
            match upcoming {
                Some((i, c)) if c.at <= next_at + 1e-9 => {
                    let signature_moved = c.beats_per_bar != change.beats_per_bar;
                    index = i;
                    at = c.at;
                    if signature_moved {
                        measure = measure.saturating_add(1);
                        beat = 1;
                    } else {
                        (measure, beat) = step(measure, beat, per_bar);
                    }
                }
                _ => {
                    at = next_at;
                    (measure, beat) = step(measure, beat, per_bar);
                }
            }
        }
        out
    }
}

/// The next measure and beat after one of `per_bar` beats.
pub(super) const fn step(measure: u32, beat: u32, per_bar: u32) -> (u32, u32) {
    if beat >= per_bar {
        (measure.saturating_add(1), 1)
    } else {
        (measure, beat + 1)
    }
}
