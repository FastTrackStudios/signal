//! A section's groove: where in the bar each piece plays, bar after bar.
//!
//! Most of a song's drumming is a pattern — through a breakdown the snare
//! is on 2 and 4 every bar, the kick plays the same figure — and that is
//! the strongest evidence of which grid line a hit belongs to. A kick 45
//! ms late on the "a" of 2 is nearer the next 16th than its own; the
//! nearest line moves it a 16th too far, where the same kick in the bars
//! around it says plainly where it goes.
//!
//! So the groove is learned from the section itself: each piece's hits
//! laid over one bar (or a two-bar cycle, when the figure takes two),
//! gathered where they bunch, and the bunches played in most bars kept as
//! the pattern's **slots** — each with its grid line (a 16th or a
//! triplet's), how far behind or ahead of it the drummer sits there (the
//! feel), and how much that wanders. A hit inside a slot goes to the
//! slot's line however far it drifted; a hit in no slot is not the
//! pattern's to place. What departs from the pattern — hits in no slot,
//! slots left out — is where the drummer leaves it: a fill, a variation.
//!
//! Everything here is in beats, the bar's own time; the caller turns
//! seconds into bars and beats and back.

/// A gap this wide (beats) between two of a piece's hits, laid over the
/// cycle, starts a new bunch: under a 16th triplet's 0.33 and a 16th's
/// 0.25 less a drummer's wander, so neighbouring lines stay apart.
const GAP: f64 = 0.1;

/// A bunch wider than this (standard deviation, beats) is not one line
/// played loosely but several: no slot.
const LOOSE: f64 = 0.08;

/// A slot is the pattern's when it is played in at least this share of
/// the cycles, and in at least [`LEAST_CYCLES`].
const MOST: f64 = 0.5;
const LEAST_CYCLES: u32 = 3;

/// A hit this near the next bar's start (beats) is that bar's downbeat
/// played early, not the end of its own bar.
const EARLY_DOWNBEAT: f64 = 0.2;

/// How far from a slot's middle a hit may be and still be the slot's:
/// this many of its spreads…
const REACH_SPREADS: f64 = 3.0;
/// …but never under this, nor over [`REACH_MOST`] (beats).
const REACH_LEAST: f64 = 0.06;
const REACH_MOST: f64 = 0.2;

/// A triplet's line is a slot's only when it is this much nearer than the
/// nearest 16th's (a share of the 16th's distance).
const TRIPLET_PLAINLY: f64 = 0.5;

/// A slot takes the line its written notes vote for when more than this
/// share of its hits that have one agree, and at least [`LEAST_VOTES`].
const VOTES_AGREE: f64 = 0.5;
const LEAST_VOTES: usize = 2;

/// A two-bar cycle is the groove only when its slots are played this
/// much more regularly than one bar's ([`Groove::regular`]) — a figure
/// that comes every other bar is a one-bar slot missing half the time —
/// and it places no less of the section, give or take [`PLACED_SLACK`].
const TWO_BARS_BETTER: f64 = 0.05;
const PLACED_SLACK: f64 = 0.02;

/// One hit, in the bar: its piece, its bar, and how far into the bar
/// (beats from the downbeat).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub piece: usize,
    pub bar: i32,
    pub beat: f64,
}

/// Where in the cycle a piece plays, bar after bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slot {
    pub piece: usize,
    /// Its grid line, beats from the cycle's start.
    pub line: f64,
    /// Where it is played on average, beats from its line (late
    /// positive): the drummer's feel there.
    pub feel: f64,
    /// How much that wanders, beats (a standard deviation).
    pub spread: f64,
    /// How many of the cycles play it.
    pub cycles: u32,
}

impl Slot {
    fn middle(&self) -> f64 {
        self.line + self.feel
    }

    fn reach(&self) -> f64 {
        (self.spread * REACH_SPREADS).clamp(REACH_LEAST, REACH_MOST)
    }
}

/// A section's groove.
#[derive(Clone, Debug, PartialEq)]
pub struct Groove {
    /// Beats in a bar.
    pub beats: f64,
    /// Bars in a cycle of the pattern: 1, or 2.
    pub cycle_bars: i32,
    /// The bar a cycle starts on.
    pub first_bar: i32,
    /// How many cycles the section holds.
    pub cycles: u32,
    pub slots: Vec<Slot>,
    /// The share of the section's hits a slot places.
    pub placed: f64,
    /// The share of the slots' cycles that play them: 1 when every slot
    /// is played in every cycle.
    pub regular: f64,
}

/// Where a hit sits in a cycle `cycle_bars` long from `first_bar`: which
/// cycle, and how far into it (beats). A hit just before a bar's start
/// is that bar's.
fn in_cycle(hit: Hit, beats: f64, first_bar: i32, cycle_bars: i32) -> (i32, f64) {
    let (bar, beat) = if hit.beat > beats - EARLY_DOWNBEAT {
        (hit.bar.saturating_add(1), hit.beat - beats)
    } else {
        (hit.bar, hit.beat)
    };
    let from_first = bar.saturating_sub(first_bar);
    let cycle = from_first.div_euclid(cycle_bars.max(1));
    let bar_in = from_first.rem_euclid(cycle_bars.max(1));
    (cycle, f64::from(bar_in).mul_add(beats, beat))
}

/// The grid line a slot played around `at` (beats) is on: the nearest
/// 16th's, unless a triplet's is plainly nearer — a drummer sitting half
/// a 16th off a straight line is nearer a triplet than either 16th, and
/// is still playing straight. The MIDI, where it is written, decides
/// better ([`Groove::settle_lines`]).
fn line_near(at: f64, triplets: bool) -> f64 {
    let sixteenth = (at * 4.0).round() / 4.0;
    let triplet = (at * 3.0).round() / 3.0;
    if triplets && (triplet - at).abs() < (sixteenth - at).abs() * TRIPLET_PLAINLY {
        triplet
    } else {
        sixteenth
    }
}

fn mean(xs: &[f64]) -> f64 {
    let n = f64::from(u32::try_from(xs.len()).unwrap_or(u32::MAX).max(1));
    xs.iter().sum::<f64>() / n
}

fn deviation(xs: &[f64], around: f64) -> f64 {
    let n = f64::from(u32::try_from(xs.len()).unwrap_or(u32::MAX).max(1));
    (xs.iter().map(|x| (x - around).powi(2)).sum::<f64>() / n).sqrt()
}

/// One piece's slots in a `cycle_bars` cycle, from its hits as `(cycle,
/// beats into it)`.
fn slots_of(piece: usize, mut at: Vec<(i32, f64)>, (cycles, triplets): (u32, bool)) -> Vec<Slot> {
    at.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut bunches: Vec<Vec<(i32, f64)>> = Vec::new();
    for hit in at {
        match bunches.last_mut() {
            Some(bunch) if bunch.last().is_some_and(|last| hit.1 - last.1 <= GAP) => {
                bunch.push(hit);
            }
            _ => bunches.push(vec![hit]),
        }
    }
    let least = (f64::from(cycles) * MOST).ceil();
    bunches
        .into_iter()
        .filter_map(|bunch| {
            let mut played: Vec<i32> = bunch.iter().map(|h| h.0).collect();
            played.sort_unstable();
            played.dedup();
            let count = u32::try_from(played.len()).unwrap_or(u32::MAX);
            if count < LEAST_CYCLES || f64::from(count) < least {
                return None;
            }
            let beats: Vec<f64> = bunch.iter().map(|h| h.1).collect();
            let middle = mean(&beats);
            let spread = deviation(&beats, middle);
            (spread <= LOOSE).then(|| {
                let line = line_near(middle, triplets);
                Slot {
                    piece,
                    line,
                    feel: middle - line,
                    spread,
                    cycles: count,
                }
            })
        })
        .collect()
}

impl Groove {
    /// The groove of `hits`, a section of `bars` bars of `beats` beats
    /// from `first_bar` — one bar's cycle, or two bars' when that places
    /// plainly more. `None` with too few bars to tell.
    #[must_use]
    pub fn learn(hits: &[Hit], beats: f64, first_bar: i32, bars: u32) -> Option<Self> {
        Self::learn_on(hits, beats, (first_bar, bars), true)
    }

    /// [`Self::learn`], its slots on 16ths only unless `triplets` — a
    /// section edited on a straight grid is straight, however late its
    /// drummer sits. The MIDI may still settle a slot on a triplet
    /// ([`Self::settle_lines`]), where that is what is written.
    #[must_use]
    pub fn learn_on(
        hits: &[Hit],
        beats: f64,
        (first_bar, bars): (i32, u32),
        triplets: bool,
    ) -> Option<Self> {
        let one = Self::learn_cycle(hits, beats, (first_bar, bars), 1, triplets)?;
        match Self::learn_cycle(hits, beats, (first_bar, bars), 2, triplets) {
            Some(two)
                if two.regular >= one.regular + TWO_BARS_BETTER
                    && two.placed >= one.placed - PLACED_SLACK =>
            {
                Some(two)
            }
            _ => Some(one),
        }
    }

    fn learn_cycle(
        hits: &[Hit],
        beats: f64,
        (first_bar, bars): (i32, u32),
        cycle_bars: i32,
        triplets: bool,
    ) -> Option<Self> {
        let per = u32::try_from(cycle_bars).ok()?.max(1);
        let cycles = bars.div_ceil(per);
        if cycles < LEAST_CYCLES {
            return None;
        }
        let pieces = hits
            .iter()
            .map(|h| h.piece)
            .max()
            .map_or(0, |p| p.saturating_add(1));
        let slots: Vec<Slot> = (0..pieces)
            .flat_map(|piece| {
                let at = hits
                    .iter()
                    .filter(|h| h.piece == piece)
                    .map(|h| in_cycle(*h, beats, first_bar, cycle_bars))
                    .collect();
                slots_of(piece, at, (cycles, triplets))
            })
            .collect();
        let mut groove = Self {
            beats,
            cycle_bars,
            first_bar,
            cycles,
            slots,
            placed: 0.0,
            regular: 0.0,
        };
        let placed = hits
            .iter()
            .filter(|h| groove.slot_of(**h).is_some())
            .count();
        groove.placed = f64::from(u32::try_from(placed).unwrap_or(u32::MAX))
            / f64::from(u32::try_from(hits.len()).unwrap_or(u32::MAX).max(1));
        let kept: u32 = groove.slots.iter().map(|s| s.cycles).sum();
        let possible = u32::try_from(groove.slots.len())
            .unwrap_or(u32::MAX)
            .saturating_mul(cycles)
            .max(1);
        groove.regular = f64::from(kept) / f64::from(possible);
        Some(groove)
    }

    /// The slot `hit` is played in, if any: its piece's slot whose middle
    /// is nearest, within that slot's reach.
    #[must_use]
    pub fn slot_of(&self, hit: Hit) -> Option<&Slot> {
        let (_, at) = in_cycle(hit, self.beats, self.first_bar, self.cycle_bars);
        self.slots
            .iter()
            .filter(|s| s.piece == hit.piece)
            .map(|s| (s, (at - s.middle()).abs()))
            .filter(|(s, off)| *off <= s.reach())
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(s, _)| s)
    }

    /// Settle each slot's line by what is written: the drum MIDI's notes
    /// (`written`, as hits) nearest the slot's hits, within `window`
    /// beats, vote for the line they are on, and a slot whose hits'
    /// notes mostly agree takes it — the pattern then carries it to the
    /// bars where a hit drifted, or the part was played otherwise.
    pub fn settle_lines(&mut self, hits: &[Hit], written: &[Hit], window: f64) {
        let along =
            |h: &Hit| f64::from(h.bar.saturating_sub(self.first_bar)).mul_add(self.beats, h.beat);
        let mut settled: Vec<(usize, f64)> = Vec::new();
        for (index, slot) in self.slots.iter().enumerate() {
            let mut votes: Vec<(f64, usize)> = Vec::new();
            let mut with_notes = 0_usize;
            for hit in hits.iter().filter(|h| self.slot_of(**h) == Some(slot)) {
                let at = along(hit);
                let Some(note) = written
                    .iter()
                    .filter(|n| n.piece == hit.piece)
                    .map(|n| (n, (along(n) - at).abs()))
                    .filter(|(_, off)| *off <= window)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(n, _)| *n)
                else {
                    continue;
                };
                with_notes = with_notes.saturating_add(1);
                let (_, line) = in_cycle(note, self.beats, self.first_bar, self.cycle_bars);
                let line = (line * 12.0).round() / 12.0;
                match votes.iter_mut().find(|(l, _)| (*l - line).abs() < 1e-6) {
                    Some((_, n)) => *n = n.saturating_add(1),
                    None => votes.push((line, 1)),
                }
            }
            if let Some(&(line, n)) = votes.iter().max_by_key(|(_, n)| *n) {
                let share = f64::from(u32::try_from(n).unwrap_or(u32::MAX))
                    / f64::from(u32::try_from(with_notes).unwrap_or(u32::MAX).max(1));
                if n >= LEAST_VOTES && share > VOTES_AGREE {
                    settled.push((index, line));
                }
            }
        }
        for (index, line) in settled {
            if let Some(slot) = self.slots.get_mut(index) {
                let middle = slot.middle();
                slot.line = line;
                slot.feel = middle - line;
            }
        }
    }

    /// How far `hit` is from where its slot is usually played, beats
    /// (late negative): nearer that than another line, it is the slot's.
    #[must_use]
    pub fn from_middle(&self, hit: Hit) -> Option<f64> {
        let (_, at) = in_cycle(hit, self.beats, self.first_bar, self.cycle_bars);
        self.slot_of(hit).map(|s| s.middle() - at)
    }

    /// How far `hit` moves to its slot's line, beats (late negative); none
    /// when it is in no slot.
    #[must_use]
    pub fn pull(&self, hit: Hit) -> Option<f64> {
        let (_, at) = in_cycle(hit, self.beats, self.first_bar, self.cycle_bars);
        self.slot_of(hit).map(|s| s.line - at)
    }

    /// Where the section departs from the groove: each cycle's hits in no
    /// slot, and its slots it does not play — `(cycle, extra, missing)`,
    /// for the cycles that depart at all.
    #[must_use]
    pub fn departures(&self, hits: &[Hit]) -> Vec<(i32, Vec<Hit>, Vec<Slot>)> {
        let mut out: Vec<(i32, Vec<Hit>, Vec<Slot>)> = Vec::new();
        let cycles = i32::try_from(self.cycles).unwrap_or(i32::MAX);
        for cycle in 0..cycles {
            let mine: Vec<Hit> = hits
                .iter()
                .copied()
                .filter(|h| in_cycle(*h, self.beats, self.first_bar, self.cycle_bars).0 == cycle)
                .collect();
            let extra: Vec<Hit> = mine
                .iter()
                .copied()
                .filter(|h| self.slot_of(*h).is_none())
                .collect();
            let missing: Vec<Slot> = self
                .slots
                .iter()
                .copied()
                .filter(|s| !mine.iter().any(|h| self.slot_of(*h) == Some(s)))
                .collect();
            if !extra.is_empty() || !missing.is_empty() {
                out.push((cycle, extra, missing));
            }
        }
        out
    }
}

/// A line's name in the bar, as a drummer counts it: `2`, `2e`, `2&`,
/// `2a`, a triplet's `2t`/`2l` (trip-let), with the bar of a two-bar
/// cycle in front (`2:3&`).
#[must_use]
pub fn count(line: f64, beats: f64, cycle_bars: i32) -> String {
    let bar = (line / beats).floor();
    let in_bar = bar.mul_add(-beats, line);
    let beat = in_bar.floor();
    let part = in_bar - beat;
    let name = [
        (0.0, ""),
        (0.25, "e"),
        (0.5, "&"),
        (0.75, "a"),
        (1.0 / 3.0, "t"),
        (2.0 / 3.0, "l"),
    ]
    .iter()
    .find(|(at, _)| (part - at).abs() < 0.01)
    .map_or_else(|| format!("+{part:.2}"), |(_, n)| (*n).to_owned());
    let counted = format!("{}{name}", beat + 1.0);
    if cycle_bars > 1 {
        format!("{}:{counted}", bar + 1.0)
    } else {
        counted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KICK: usize = 0;
    const SNARE: usize = 1;

    /// Eight bars of 4/4: kick on 1, the "a" of 1 and the "&" of 2;
    /// snare on 2 and 4 — each hit `late` beats behind its line in every
    /// bar, with a little wander.
    fn groove(late: f64) -> Vec<Hit> {
        let mut hits = Vec::new();
        for bar in 1..=8 {
            let wander = f64::from(bar % 3) * 0.01;
            for (piece, line) in [
                (KICK, 0.0),
                (KICK, 0.75),
                (KICK, 1.5),
                (SNARE, 1.0),
                (SNARE, 3.0),
            ] {
                hits.push(Hit {
                    piece,
                    bar,
                    beat: line + late + wander,
                });
            }
        }
        hits
    }

    #[test]
    fn the_pattern_is_learned_with_its_feel() {
        let g = Groove::learn(&groove(0.04), 4.0, 1, 8).expect("a groove");
        assert_eq!(g.cycle_bars, 1);
        let mut lines: Vec<(usize, f64)> = g.slots.iter().map(|s| (s.piece, s.line)).collect();
        lines.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
        assert_eq!(
            lines,
            [
                (KICK, 0.0),
                (KICK, 0.75),
                (KICK, 1.5),
                (SNARE, 1.0),
                (SNARE, 3.0)
            ]
        );
        assert!(
            g.slots.iter().all(|s| (s.feel - 0.05).abs() < 0.02),
            "{g:?}"
        );
        assert!((g.placed - 1.0).abs() < 1e-9);
    }

    /// A kick dragged toward the next 16th still goes to its own line:
    /// the nearest line would take it a 16th too far.
    #[test]
    fn a_dragged_hit_goes_to_its_slot_not_the_nearest_line() {
        let mut hits = groove(0.03);
        let dragged = Hit {
            piece: KICK,
            bar: 5,
            beat: 0.75 + 0.14,
        };
        hits.push(dragged);
        let g = Groove::learn(&hits, 4.0, 1, 8).expect("a groove");
        let pull = g.pull(dragged).expect("in its slot");
        assert!((pull + 0.14).abs() < 1e-9, "pulled {pull}");
        assert!(
            ((dragged.beat * 4.0).round() / 4.0 - 1.0).abs() < 1e-9,
            "nearest is 1.0"
        );
    }

    #[test]
    fn a_downbeat_played_early_is_the_next_bars() {
        let mut hits = groove(0.0);
        let early = Hit {
            piece: KICK,
            bar: 4,
            beat: 3.93,
        };
        hits.push(early);
        let g = Groove::learn(&hits, 4.0, 1, 8).expect("a groove");
        assert!((g.pull(early).expect("bar 5's downbeat") - 0.07).abs() < 1e-9);
    }

    #[test]
    fn a_fill_departs_from_the_groove() {
        let mut hits: Vec<Hit> = groove(0.0)
            .into_iter()
            .filter(|h| !(h.bar == 8 && h.piece == SNARE && h.beat > 2.0))
            .collect();
        for k in 0..4 {
            hits.push(Hit {
                piece: SNARE,
                bar: 8,
                beat: f64::from(k).mul_add(0.375, 2.5),
            });
        }
        let g = Groove::learn(&hits, 4.0, 1, 8).expect("a groove");
        let away = g.departures(&hits);
        assert_eq!(away.len(), 1, "{away:?}");
        let (cycle, extra, missing) = &away[0];
        assert_eq!(*cycle, 7);
        assert_eq!(extra.len(), 4);
        assert_eq!(missing.len(), 1, "beat 4's snare");
    }

    #[test]
    fn a_two_bar_figure_is_one_cycle() {
        // The kick plays the "&" of 3 in odd bars only.
        let mut hits = groove(0.0);
        for bar in (1..=8).step_by(2) {
            hits.push(Hit {
                piece: KICK,
                bar,
                beat: 2.5,
            });
        }
        let g = Groove::learn(&hits, 4.0, 1, 8).expect("a groove");
        assert_eq!(g.cycle_bars, 2, "{g:?}");
        assert!((g.placed - 1.0).abs() < 1e-9);
    }

    /// A kick sitting half a 16th behind beat 2 is as near the "&" of 1's
    /// triplet as anything: alone it is taken as straight, and where the
    /// MIDI writes it on 2, on 2.
    #[test]
    fn a_line_in_doubt_is_straight_or_what_is_written() {
        let hits: Vec<Hit> = (1..=8)
            .map(|bar| Hit {
                piece: KICK,
                bar,
                beat: 0.6 + f64::from(bar % 2) * 0.01,
            })
            .collect();
        let mut g = Groove::learn(&hits, 4.0, 1, 8).expect("a groove");
        let line = |g: &Groove| g.slots.first().map(|s| s.line);
        assert_eq!(line(&g), Some(0.5), "the 16th's, not the triplet's 0.67");
        let written: Vec<Hit> = (1..=8)
            .map(|bar| Hit {
                piece: KICK,
                bar,
                beat: 0.75,
            })
            .collect();
        g.settle_lines(&hits, &written, 0.3);
        assert_eq!(line(&g), Some(0.75), "where it is written");
        let s = g.slots.first().expect("a slot");
        assert!(
            (s.middle() - 0.605).abs() < 0.01,
            "where it is played is kept"
        );
    }

    #[test]
    fn too_few_bars_is_no_groove() {
        let hits: Vec<Hit> = groove(0.0).into_iter().filter(|h| h.bar <= 2).collect();
        assert!(Groove::learn(&hits, 4.0, 1, 2).is_none());
    }

    #[test]
    fn lines_are_counted_as_a_drummer_counts() {
        assert_eq!(count(1.75, 4.0, 1), "2a");
        assert_eq!(count(0.0, 4.0, 1), "1");
        assert_eq!(count(2.0 + 1.0 / 3.0, 4.0, 1), "3t");
        assert_eq!(count(6.5, 4.0, 2), "2:3&");
    }
}
