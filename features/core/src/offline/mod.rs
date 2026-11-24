//! Offline blocks: DSP that runs once over a track's whole audio, like
//! ARA, and leaves raw automation behind.
//!
//! A block kind is [`Mode::Realtime`] (it processes what arrives, live)
//! or [`Mode::Offline`]. An offline block has two halves:
//!
//! - **analysis** (control thread, once): reads the audio arriving at the
//!   block over the song or a range of it, and makes either **lanes**
//!   (curves on the block's own parameters: [`Curve`]) or static
//!   **values** (an alignment). Kept in the block's spec as an
//!   [`Analysis`], with what it was made from, so an edit upstream marks
//!   it stale.
//! - **apply** (audio thread, every block): a cheap stage that plays those
//!   parameters — a gain, a band's gain, a delay, a polarity, an all-pass.
//!   It sits in the chain like any block: bypassed, moved, removed.
//!
//! Who plays the lanes ([`Playback`]):
//!
//! - **the host**: written as its ordinary automation of the Signal FX's
//!   own parameters through `com.fasttrackstudio.host-automation/1`
//!   (Studio), then shown, edited, undone and carried with edits there;
//! - **Signal itself**: where the host offers no such extension (another
//!   DAW), the graph plays them against the transport ([`SelfPlay`] in the
//!   graph), exactly as host automation would set them; a "print" writes
//!   them into the host's automation while it records.
//!
//! The kinds: [`level`] (the vocal leveler: gate, breath, ride and de-ess
//! lanes) and [`align`] (delay, polarity and phase rotation, as values).

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

pub mod align;
pub mod capture;
pub mod level;

/// How a block kind runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Processes what arrives, live.
    Realtime,
    /// Analysed once; its apply stage plays the result.
    Offline,
}

impl Mode {
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Realtime => "realtime",
            Self::Offline => "offline",
        }
    }
}

/// The mode of Signal's block `name`.
#[must_use]
pub fn mode_of(name: &str) -> Mode {
    match name {
        level::NAME | align::NAME => Mode::Offline,
        _ => Mode::Realtime,
    }
}

/// A span of the song, project seconds: `[from, to)`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Range {
    pub from: f64,
    pub to: f64,
}

impl Range {
    #[must_use]
    pub const fn new(from: f64, to: f64) -> Self {
        Self { from, to }
    }

    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        self.from < other.to && other.from < self.to
    }

    /// The smallest range holding both.
    #[must_use]
    pub const fn union(&self, other: &Self) -> Self {
        Self::new(self.from.min(other.from), self.to.max(other.to))
    }
}

// ── Curves ──────────────────────────────────────────────────────────────

/// A lane: breakpoints `(seconds, plain value)`, time-sorted, ramping
/// linearly between, held before the first and after the last. A step is
/// two points a hair apart ([`EDGE_S`]), so a host's linear lane and the
/// graph's own playback draw the same thing.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Curve(pub Vec<(f64, f32)>);

/// How far before a step its pre-step point sits: the dsp crate's
/// composite uses the same, so a step lands where the composite has it.
pub const EDGE_S: f64 = 1e-4;

impl Curve {
    #[must_use]
    pub fn points(&self) -> &[(f64, f32)] {
        &self.0
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The value at `t`.
    #[must_use]
    pub fn value_at(&self, t: f64) -> Option<f32> {
        let at = self.0.partition_point(|p| p.0 <= t);
        segment_value(&self.0, at, t)
    }

    /// The first breakpoint after `t`.
    #[must_use]
    pub fn next_after(&self, t: f64) -> Option<f64> {
        let at = self.0.partition_point(|p| p.0 <= t);
        self.0.get(at).map(|p| p.0)
    }

    /// The points inside `range`, and the value at each end (so a part of
    /// a lane can be written back over the same range).
    #[must_use]
    pub fn within(&self, range: Range) -> Self {
        let mut out: Vec<(f64, f32)> = Vec::new();
        if let Some(v) = self.value_at(range.from) {
            out.push((range.from, v));
        }
        out.extend(
            self.0
                .iter()
                .filter(|p| p.0 > range.from && p.0 < range.to)
                .copied(),
        );
        Self(out)
    }

    /// `self` with `[range)` replaced by `part`'s points there.
    #[must_use]
    pub fn spliced(&self, range: Range, part: &Self) -> Self {
        let mut out: Vec<(f64, f32)> = self
            .0
            .iter()
            .filter(|p| p.0 < range.from || p.0 >= range.to)
            .copied()
            .chain(
                part.0
                    .iter()
                    .filter(|p| p.0 >= range.from && p.0 < range.to)
                    .copied(),
            )
            .collect();
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        Self(out)
    }
}

/// The value at `t` given `at`, the index of the first point after it.
fn segment_value(points: &[(f64, f32)], at: usize, t: f64) -> Option<f32> {
    let after = points.get(at);
    let before = at.checked_sub(1).and_then(|i| points.get(i));
    Some(match (before, after) {
        (Some(a), Some(b)) if b.0 > a.0 => {
            let k = (t - a.0) / (b.0 - a.0);
            #[allow(clippy::cast_possible_truncation)]
            let v = k.mul_add(f64::from(b.1) - f64::from(a.1), f64::from(a.1)) as f32;
            v
        }
        (Some(a), _) => a.1,
        (None, Some(b)) => b.1,
        (None, None) => return None,
    })
}

/// Reads a [`Curve`] forwards without searching: what the audio thread
/// holds while the transport plays on. A jump (a seek) searches once.
#[derive(Clone, Copy, Debug, Default)]
pub struct Cursor {
    /// The first point after the last time read.
    at: usize,
    last: f64,
}

impl Cursor {
    /// The value at `t`, moving the cursor there. Allocation-free.
    pub fn value(&mut self, curve: &Curve, t: f64) -> Option<f32> {
        let p = &curve.0;
        if t < self.last {
            self.at = p.partition_point(|q| q.0 <= t);
        } else {
            while p.get(self.at).is_some_and(|q| q.0 <= t) {
                self.at += 1;
            }
        }
        self.last = t;
        segment_value(p, self.at, t)
    }

    /// The first breakpoint after `t` (`t` at or past the cursor).
    #[must_use]
    pub fn next_after(&self, curve: &Curve, t: f64) -> Option<f64> {
        curve
            .0
            .get(self.at..)?
            .iter()
            .find(|q| q.0 > t)
            .map(|q| q.0)
    }
}

// ── What an offline block keeps ────────────────────────────────────────

/// Where an offline block's analysis stands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    /// Never analysed: the apply stage plays its parameters' own values.
    #[default]
    None,
    /// Made from the audio as it is.
    Fresh,
    /// The audio arriving changed since (an edit upstream, a block before
    /// it): what plays is the old result until it is analysed again.
    Stale,
    /// Being analysed (or, in another DAW, captured while the song plays).
    Running,
}

impl State {
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Fresh => "fresh",
            Self::Stale => "stale",
            Self::Running => "running",
        }
    }
}

/// Who plays an offline block's lanes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Playback {
    /// The host, as its own automation of the FX's parameters
    /// (`host-automation/1`).
    Host,
    /// Signal, against the transport: a host without the extension.
    #[default]
    #[serde(rename = "self")]
    Signal,
}

impl Playback {
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Signal => "self",
        }
    }
}

/// An offline block's result and what it was made from: kept in its
/// [`BlockSpec`](crate::state::BlockSpec), so it saves with the preset and
/// survives a rebuild.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Analysis {
    #[serde(default)]
    pub state: State,
    /// The span analysed, project seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<Range>,
    /// When, Unix seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<f64>,
    /// The host's revision of the audio it was made from (Studio: the
    /// track's render fingerprint): a different one now means stale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_revision: Option<String>,
    #[serde(default)]
    pub playback: Playback,
    /// The block's lanes, by parameter id, in plain units.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub lanes: BTreeMap<String, Curve>,
    /// Static values by parameter id (an alignment), and what an analysis
    /// keeps for a later one over part of the song (a leveler's target).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values: BTreeMap<String, f64>,
    /// Where the audio changed since (stale), to analyse again first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stale: Vec<Range>,
    /// Where the user redrew a lane in the host, by parameter id: kept
    /// through a new analysis unless they say otherwise.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub edited: BTreeMap<String, Vec<Range>>,
}

impl Analysis {
    /// The audio arriving changed over `range`: stale there.
    pub fn mark_stale(&mut self, range: Range) {
        if self.state == State::None {
            return;
        }
        self.state = State::Stale;
        let merged = self
            .stale
            .iter()
            .filter(|r| r.overlaps(&range))
            .fold(range, |acc, r| acc.union(r));
        self.stale.retain(|r| !r.overlaps(&range));
        self.stale.push(merged);
        self.stale.sort_by(|a, b| a.from.total_cmp(&b.from));
    }

    /// The host says the audio is now at `revision`: stale over
    /// `changed` (the whole analysed range when it can't say) if that is
    /// not what it was made from.
    pub fn check_source(&mut self, revision: &str, changed: Option<Range>) {
        if self.source_revision.as_deref() == Some(revision) {
            return;
        }
        if let Some(r) = changed.or(self.range) {
            self.mark_stale(r);
        }
    }

    /// The user redrew `param` over `range`.
    pub fn mark_edited(&mut self, param: &str, range: Range) {
        let spans = self.edited.entry(param.into()).or_default();
        let merged = spans
            .iter()
            .filter(|r| r.overlaps(&range))
            .fold(range, |acc, r| acc.union(r));
        spans.retain(|r| !r.overlaps(&range));
        spans.push(merged);
    }
}

// ── Signal playing the lanes itself ────────────────────────────────────

/// One lane the graph plays itself, against the transport.
pub struct PlayedLane {
    pub lane: usize,
    pub block: usize,
    pub param: usize,
    pub curve: Curve,
    pub cursor: Cursor,
}

/// The lanes a graph plays (preallocated when it is built) and the clock
/// it expects next, to tell a seek from playing on.
#[derive(Default)]
pub struct SelfPlay {
    pub lanes: Vec<PlayedLane>,
    pub expected: Option<u64>,
    pub sample_rate: f64,
}

impl SelfPlay {
    /// The first breakpoint of any lane after song frame `clock`, as frames
    /// from it (at least 1): where the processor ends a piece.
    ///
    /// The piece ends at the last frame at or before the breakpoint, so the
    /// curve is one straight segment over it; the next piece, a frame
    /// long when the breakpoint falls between two frames, starts on the
    /// curve's own value there. Every frame then plays the curve at its
    /// own time, exactly.
    #[must_use]
    pub fn frames_to_next(&self, clock: u64) -> Option<usize> {
        if self.lanes.is_empty() || self.sample_rate <= 0.0 {
            return None;
        }
        #[allow(clippy::cast_precision_loss)]
        let t = clock as f64 / self.sample_rate;
        let next = self
            .lanes
            .iter()
            .filter_map(|l| l.cursor.next_after(&l.curve, t))
            .fold(f64::INFINITY, f64::min);
        if !next.is_finite() {
            return None;
        }
        let frames = libm::floor((next - t) * self.sample_rate).max(1.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Some(frames as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn curve() -> Curve {
        Curve(alloc::vec![(1.0, 0.0), (2.0, -10.0), (4.0, -10.0)])
    }

    #[test]
    fn a_curve_ramps_between_and_holds_outside() {
        let c = curve();
        assert_eq!(c.value_at(0.0), Some(0.0));
        assert_eq!(c.value_at(1.5), Some(-5.0));
        assert_eq!(c.value_at(3.0), Some(-10.0));
        assert_eq!(c.value_at(9.0), Some(-10.0));
        assert_eq!(Curve::default().value_at(1.0), None);
        assert_eq!(c.next_after(1.0), Some(2.0));
    }

    #[test]
    fn a_cursor_reads_what_a_search_does_forwards_and_after_a_seek() {
        let c = curve();
        let mut cur = Cursor::default();
        for t in [0.0, 0.5, 1.0, 1.25, 2.0, 3.0, 5.0, 1.5, 1.75] {
            assert_eq!(cur.value(&c, t), c.value_at(t), "{t}");
        }
    }

    #[test]
    fn a_splice_replaces_only_its_range() {
        let c = curve();
        let part = Curve(alloc::vec![(2.5, 3.0), (3.5, 3.0)]);
        let s = c.spliced(Range::new(2.0, 4.0), &part);
        assert_eq!(s.0, [(1.0, 0.0), (2.5, 3.0), (3.5, 3.0), (4.0, -10.0)]);
    }

    #[test]
    fn staleness_merges_ranges_and_waits_for_a_first_analysis() {
        let mut a = Analysis::default();
        a.mark_stale(Range::new(0.0, 1.0));
        assert_eq!(a.state, State::None, "nothing to be stale");
        a.state = State::Fresh;
        a.source_revision = Some("r1".into());
        a.range = Some(Range::new(0.0, 100.0));
        a.check_source("r1", None);
        assert_eq!(a.state, State::Fresh);
        a.check_source("r2", Some(Range::new(10.0, 12.0)));
        a.mark_stale(Range::new(11.0, 15.0));
        a.mark_stale(Range::new(50.0, 51.0));
        assert_eq!(a.state, State::Stale);
        assert_eq!(a.stale, [Range::new(10.0, 15.0), Range::new(50.0, 51.0)]);
    }
}
