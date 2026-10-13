//! ENST-Drums annotations — and testing the claim we want to make of
//! them.
//!
//! **Licence first, because it is the constraint that matters.**
//! ENST-Drums is CC BY-NC-ND 4.0 and its own site states "No commercial
//! use is possible." Nothing from it is vendored, nothing derived from
//! it goes into a release asset, and no fixture in this repository
//! contains any of its data. The fixture the tests here run against is
//! written by hand, in its format, by us. This module reads a local
//! copy someone fetched deliberately, for internal evaluation, and that
//! is the whole of its role.
//!
//! ## The inference under test
//!
//! ENST has no flam label. Its 20-label set names instruments, not
//! techniques. But annotation is *per onset per track* and ghost notes
//! were deliberately annotated (paper §3.5.2), so a flam **should**
//! appear as two `sd` events tens of milliseconds apart. #158 flagged
//! this explicitly: "that is an inference from the annotation method,
//! not an author claim — verify it on first fetch by histogramming
//! inter-`sd` intervals on the snare channel."
//!
//! [`histogram`] and [`FlamEvidence`] are that verification.
//!
//! ## Why the histogram alone does not settle it
//!
//! An interval of 10–60 ms between two snare onsets is consistent with
//! a flam. It is also consistent with a fast roll: a 32nd note at
//! 200 bpm is 37.5 ms, which lands squarely inside the window. The
//! annotations carry no velocity, so nothing in the file distinguishes
//! "ghost then accent" from "two even strokes".
//!
//! So [`FlamEvidence`] reports the count *and* whether the tempo makes
//! the window ambiguous, rather than returning a boolean it cannot
//! honestly justify. What a clean result looks like: a population in
//! the window, at a tempo whose shortest plausible subdivision is
//! outside it. What a contaminated result looks like: the same
//! population at 200 bpm, where it proves nothing.
//!
//! This is also the trap #158 recorded for any harness built on ENST —
//! quiet *time-keeping* strokes were **not** annotated, so a detector
//! that finds them scores false positives against a reference that
//! simply declined to mark them.

/// One annotated stroke.
#[derive(Clone, Debug, PartialEq)]
pub struct Onset {
    pub secs: f64,
    /// The instrument label, e.g. `sd`, `bd`, `chh`.
    pub label: String,
}

/// Parse an annotation file: one `<seconds> <label>` per line.
pub fn parse(text: &str) -> Result<Vec<Onset>, String> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let (Some(time), Some(label)) = (parts.next(), parts.next()) else {
            return Err(format!("line {}: want '<seconds> <label>'", n + 1));
        };
        out.push(Onset {
            secs: time
                .parse()
                .map_err(|e| format!("line {}: {time:?}: {e}", n + 1))?,
            label: label.to_string(),
        });
    }
    // Annotation files are in order, but nothing enforces it and every
    // interval below depends on it.
    out.sort_by(|a, b| a.secs.total_cmp(&b.secs));
    Ok(out)
}

/// The times of one instrument's strokes.
pub fn times_for(onsets: &[Onset], label: &str) -> Vec<f64> {
    onsets
        .iter()
        .filter(|o| o.label == label)
        .map(|o| o.secs)
        .collect()
}

/// Gaps between consecutive strokes, in milliseconds.
pub fn intervals_ms(times: &[f64]) -> Vec<f64> {
    times.windows(2).map(|w| (w[1] - w[0]) * 1000.0).collect()
}

/// Counts of intervals, binned.
#[derive(Clone, Debug, PartialEq)]
pub struct Histogram {
    pub bin_ms: f64,
    /// `counts[i]` covers `[i·bin_ms, (i+1)·bin_ms)`.
    pub counts: Vec<usize>,
    /// Intervals at or beyond `bin_ms · counts.len()`. Most of them, on
    /// real material — the musical gaps.
    pub over: usize,
}

/// Bin intervals up to `max_ms`.
pub fn histogram(intervals_ms: &[f64], bin_ms: f64, max_ms: f64) -> Histogram {
    let bin_ms = bin_ms.max(f64::EPSILON);
    let bins = (max_ms / bin_ms).ceil().max(1.0) as usize;
    let mut counts = vec![0usize; bins];
    let mut over = 0;
    for &v in intervals_ms {
        if v < 0.0 {
            continue;
        }
        let i = (v / bin_ms) as usize;
        match counts.get_mut(i) {
            Some(c) => *c += 1,
            None => over += 1,
        }
    }
    Histogram {
        bin_ms,
        counts,
        over,
    }
}

impl Histogram {
    /// Total intervals binned, including the overflow.
    pub fn total(&self) -> usize {
        self.counts.iter().sum::<usize>() + self.over
    }

    /// A plain-text plot, since the point of the exercise is that a
    /// human looks at the shape once and decides whether the inference
    /// holds.
    pub fn render(&self, width: usize) -> String {
        let peak = self.counts.iter().copied().max().unwrap_or(0).max(1);
        let mut s = String::new();
        for (i, &c) in self.counts.iter().enumerate() {
            let bar = c * width / peak;
            s.push_str(&format!(
                "{:6.1}–{:5.1} ms │{:<width$}│ {c}\n",
                i as f64 * self.bin_ms,
                (i + 1) as f64 * self.bin_ms,
                "█".repeat(bar),
                width = width
            ));
        }
        s.push_str(&format!(
            "        ≥{:5.1} ms   {}\n",
            self.counts.len() as f64 * self.bin_ms,
            self.over
        ));
        s
    }
}

/// What the inter-`sd` intervals say about whether flams are visible.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlamEvidence {
    /// The interval window a flam would fall in, in ms.
    pub window_ms: (f64, f64),
    /// Intervals inside it.
    pub candidates: usize,
    /// Intervals altogether.
    pub total: usize,
}

/// Flams sit here. Below the low edge two strikes are one thickened
/// attack; above the high edge they are two notes.
pub const FLAM_WINDOW_MS: (f64, f64) = (10.0, 60.0);

impl FlamEvidence {
    pub fn measure(intervals_ms: &[f64], window_ms: (f64, f64)) -> Self {
        Self {
            window_ms,
            candidates: intervals_ms
                .iter()
                .filter(|v| **v >= window_ms.0 && **v <= window_ms.1)
                .count(),
            total: intervals_ms.len(),
        }
    }

    pub fn fraction(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.candidates as f64 / self.total as f64
        }
    }

    /// Whether the window is ambiguous at this tempo — see the module
    /// docs. `subdivision` is the shortest one the material plausibly
    /// plays, as a divisor of the quarter note (8 = 32nd notes).
    pub fn confounded_at(&self, bpm: f64, subdivision: f64) -> bool {
        if bpm <= 0.0 || subdivision <= 0.0 {
            return false;
        }
        let note_ms = 60_000.0 / bpm / subdivision;
        note_ms >= self.window_ms.0 && note_ms <= self.window_ms.1
    }

    /// What can honestly be said.
    pub fn verdict(&self, bpm: f64, subdivision: f64) -> Verdict {
        if self.total == 0 {
            Verdict::NoData
        } else if self.candidates == 0 {
            Verdict::NoCandidates
        } else if self.confounded_at(bpm, subdivision) {
            Verdict::Confounded
        } else {
            Verdict::Supported
        }
    }
}

/// The outcome of the verification, named rather than left as a
/// boolean, because three of the four outcomes are not "yes" or "no".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// No `sd` intervals at all — wrong file, or a one-stroke phrase.
    NoData,
    /// Intervals, but none in the flam window: the inference does not
    /// hold for this sequence and ENST cannot be used for flam timing.
    NoCandidates,
    /// Candidates exist, but the tempo puts a plain subdivision inside
    /// the window, so they are not evidence of anything.
    Confounded,
    /// Candidates exist and no ordinary subdivision explains them.
    Supported,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Written by hand, in ENST's format, containing no ENST data —
    /// see the module docs on the licence.
    const FIXTURE: &str = include_str!("../fixtures/annotation-format-sample.txt");

    #[test]
    fn the_fixture_parses_into_ordered_onsets() {
        let onsets = parse(FIXTURE).expect("parses");
        assert!(!onsets.is_empty());
        assert!(onsets.windows(2).all(|w| w[0].secs <= w[1].secs));
        assert!(onsets.iter().any(|o| o.label == "sd"));
        assert!(onsets.iter().any(|o| o.label == "bd"));
    }

    #[test]
    fn a_malformed_line_is_an_error_rather_than_a_silent_skip() {
        assert!(parse("0.5 sd\nnonsense\n").is_err());
        assert!(parse("nope sd\n").is_err());
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let onsets = parse("# header\n\n1.0 sd\n").expect("parses");
        assert_eq!(onsets.len(), 1);
    }

    #[test]
    fn the_flam_in_the_fixture_shows_up_as_a_short_sd_interval() {
        // The fixture is a 100 bpm backbeat with one deliberate flam,
        // grace 25 ms before the snare on beat 4 of the second bar.
        let onsets = parse(FIXTURE).expect("parses");
        let intervals = intervals_ms(&times_for(&onsets, "sd"));
        let evidence = FlamEvidence::measure(&intervals, FLAM_WINDOW_MS);
        assert_eq!(evidence.candidates, 1, "intervals: {intervals:?}");
        // 16ths at 100 bpm are 150 ms, nowhere near the window.
        assert_eq!(evidence.verdict(100.0, 4.0), Verdict::Supported);
    }

    #[test]
    fn a_fast_subdivision_makes_the_same_count_worthless() {
        // The confound the module docs describe: 32nds at 200 bpm are
        // 37.5 ms, inside the window, so the same candidates prove
        // nothing.
        let onsets = parse(FIXTURE).expect("parses");
        let intervals = intervals_ms(&times_for(&onsets, "sd"));
        let evidence = FlamEvidence::measure(&intervals, FLAM_WINDOW_MS);
        assert!(evidence.confounded_at(200.0, 8.0));
        assert_eq!(evidence.verdict(200.0, 8.0), Verdict::Confounded);
    }

    #[test]
    fn a_channel_with_no_short_intervals_reports_no_candidates() {
        let onsets = parse(FIXTURE).expect("parses");
        let intervals = intervals_ms(&times_for(&onsets, "bd"));
        let evidence = FlamEvidence::measure(&intervals, FLAM_WINDOW_MS);
        assert_eq!(evidence.verdict(100.0, 4.0), Verdict::NoCandidates);
    }

    #[test]
    fn every_interval_lands_in_exactly_one_bin() {
        let intervals = [4.9, 5.0, 25.0, 59.9, 60.0, 1000.0];
        let h = histogram(&intervals, 5.0, 60.0);
        assert_eq!(h.total(), intervals.len());
        assert_eq!(h.counts[0], 1); // 4.9
        assert_eq!(h.counts[1], 1); // 5.0
        assert_eq!(h.counts[5], 1); // 25.0
        assert_eq!(h.counts[11], 1); // 59.9
        assert_eq!(h.over, 2); // 60.0 and 1000.0
    }
}
