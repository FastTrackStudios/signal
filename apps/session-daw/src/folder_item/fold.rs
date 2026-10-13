//! The fold: many children's take peaks onto one column grid.
//!
//! One function, [`fold`], with a group-by of role or side. The rule is
//! `min(min)` / `max(max)` per column and per group — never a mean. The
//! mean is the expression editor's lane rule (`drums.lanes.summed`), a
//! drum-mic special case that flattens an arrangement's hits into one
//! decay wash; the min/max fold is never narrower than any child and is
//! what REAPER would draw for the same lanes. Decided on the prototype
//! (issue #27, `prototype/folder-items`).

use expression_editor_core::kit::LaneRole;

/// How many groups a column can hold: four roles, or two sides.
pub const SLOTS: usize = 4;

/// Which half of a stereo waveform a child's audio belongs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    /// The slot a side occupies in a [`FoldColumn`].
    #[must_use]
    pub const fn slot(self) -> usize {
        match self {
            Self::Left => 0,
            Self::Right => 1,
        }
    }

    /// The side a Channel dimension names, when it names one.
    ///
    /// The Channel vocabulary is `L`/`R`/`C` and their long forms
    /// (`metadata_patterns.rs`); a centre channel is not a side and
    /// folds like any other child.
    #[must_use]
    pub fn parse(channel: &str) -> Option<Self> {
        match channel.trim().to_ascii_uppercase().as_str() {
            "L" | "LEFT" => Some(Self::Left),
            "R" | "RIGHT" => Some(Self::Right),
            _ => None,
        }
    }
}

/// What a column's groups mean.
///
/// The fold is the same function either way; only the key differs. Roles
/// are what a kit folds by — the kick, the snare, the toms, and
/// everything else that is heard rather than played
/// (`flow.drums.comping.folder-item-colours`). Sides are what a Channel
/// folds by — a double's L and R as the two halves of one stereo
/// waveform (`flow.guitars.folder-items`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GroupBy {
    Role,
    Side,
}

impl GroupBy {
    /// The name this grouping is committed under.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Role => "role",
            Self::Side => "side",
        }
    }
}

/// One child's peaks for one take, as the wire delivers them.
///
/// # The peak grid is derived, never declared
///
/// `TakePeakData` carries a `sample_rate` and a `samples_per_peak`, and
/// the obvious reading — the spacing is `samples_per_peak / sample_rate`
/// — is wrong on REAPER. A source at a rate the project is not running
/// at is served on the **project's** grid: a 44.1 kHz file in a 48 kHz
/// project answers twenty peaks a second whatever rate it was asked for
/// (daw#10). Believing the header there drifts about 5% across a take,
/// and a folder whose children are at different rates would fold one
/// child's hits against another's tails — silently, and worse the
/// further into the take you look.
///
/// So this type indexes by **fraction of the take**. Both backends
/// guarantee the blocks tile the take, so block `i` of `n` covers
/// `[i/n, (i+1)/n)` of it whatever grid produced them, and children at
/// different rates land on the same wall clock. The declared spacing is
/// kept only for [`TakePeaks::drift`], which says how far the header is
/// from the truth so an off-rate source is visible rather than silent.
#[derive(Clone, Debug, PartialEq)]
pub struct TakePeaks {
    /// `(min, max)` per channel per block, interleaved exactly as
    /// `TakePeakData::peaks` carries them:
    /// `[ch0_min, ch0_max, ch1_min, ch1_max, ...]`.
    pairs: Vec<f32>,
    channels: usize,
    blocks: usize,
    /// The spacing the wire declared, in seconds. Diagnostic only.
    declared_secs_per_peak: f64,
}

impl TakePeaks {
    /// Build from the wire's own layout.
    ///
    /// A pair list that does not fill a whole block is truncated to the
    /// blocks it does fill, and a channel count of zero reads as one: a
    /// child whose media never materialized contributes nothing, which
    /// is not an error a folder row should refuse to draw over.
    #[must_use]
    pub fn new(peaks: &[f64], channels: u32, samples_per_peak: u32, sample_rate: f64) -> Self {
        let channels = usize::try_from(channels).unwrap_or(1).max(1);
        let stride = channels.saturating_mul(2);
        let blocks = peaks.len().checked_div(stride).unwrap_or(0);
        let kept = blocks.saturating_mul(stride);
        let pairs = peaks
            .get(..kept)
            .unwrap_or_default()
            .iter()
            .map(|v| crate::num::narrow(*v))
            .collect();
        let declared_secs_per_peak = if sample_rate > 0.0 {
            f64::from(samples_per_peak.max(1)) / sample_rate
        } else {
            0.0
        };
        Self {
            pairs,
            channels,
            blocks,
            declared_secs_per_peak,
        }
    }

    /// An empty take: no blocks, one channel. What a child with no
    /// materialized media folds as.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            pairs: Vec::new(),
            channels: 1,
            blocks: 0,
            declared_secs_per_peak: 0.0,
        }
    }

    /// How many blocks the take answered with.
    #[must_use]
    pub const fn blocks(&self) -> usize {
        self.blocks
    }

    /// How many channels each block carries.
    #[must_use]
    pub const fn channels(&self) -> usize {
        self.channels
    }

    /// A hash of the peaks themselves, for the folder's revision.
    ///
    /// Over the values rather than a counter beside them: peaks change
    /// when a take is re-recorded, edited or re-materialized, and none
    /// of those is a place that would remember to bump a number.
    #[must_use]
    pub fn fingerprint(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for value in &self.pairs {
            for byte in value.to_bits().to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        hash ^= u64::try_from(self.channels).unwrap_or(0);
        hash.wrapping_mul(0x0000_0100_0000_01b3)
    }

    /// How far the declared peak grid is from the one the take actually
    /// came back on, as a fraction — `0.0` when they agree, about `0.09`
    /// for a 44.1 kHz source served on a 48 kHz project's grid.
    ///
    /// Nothing in the fold consumes this: the fold works by fraction and
    /// is right either way. It exists so the loader can put a number on
    /// the span when a source is off-rate.
    #[must_use]
    pub fn drift(&self, length_secs: f64) -> f64 {
        if length_secs <= 0.0 || self.blocks == 0 || self.declared_secs_per_peak <= 0.0 {
            return 0.0;
        }
        let declared = self.declared_secs_per_peak * crate::num::coord(self.blocks);
        ((declared / length_secs) - 1.0).abs()
    }

    /// The `(min, max)` over every block touching `[u0, u1)` of the
    /// take, where `u` is a fraction in `0..1`. `None` when the window
    /// falls outside the take or the channel does not exist.
    #[must_use]
    pub fn window(&self, channel: usize, u0: f64, u1: f64) -> Option<(f32, f32)> {
        if self.blocks == 0 || channel >= self.channels || u1 <= 0.0 || u0 >= 1.0 {
            return None;
        }
        let n = crate::num::coord(self.blocks);
        let from = crate::num::index((u0.max(0.0) * n).floor()).min(self.blocks.saturating_sub(1));
        let to =
            crate::num::index((u1.min(1.0) * n).ceil()).clamp(from.saturating_add(1), self.blocks);
        let stride = self.channels.saturating_mul(2);
        let base = channel.saturating_mul(2);
        (from..to)
            .filter_map(|b| {
                let at = b.saturating_mul(stride).saturating_add(base);
                Some((*self.pairs.get(at)?, *self.pairs.get(at.saturating_add(1))?))
            })
            .reduce(|(lo, hi), (mn, mx)| (lo.min(mn), hi.max(mx)))
    }
}

/// One of a child's items, placed on the timeline.
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    /// Where the item starts, in project seconds.
    pub start_secs: f64,
    /// How long it runs, in project seconds.
    pub length_secs: f64,
    pub peaks: TakePeaks,
}

/// One take of one child: every item it has on that take lane.
///
/// A take is a **lane**, not an item (`flow.drums.comping.lanes`: every
/// take of a source track is a take lane under it). A track with fixed
/// lanes has one take per lane; a track without them has one take
/// holding every item on it, which is what an arrangement that was never
/// comped looks like — a verse item, a chorus item and a fill, all of
/// the same pass.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChildTake {
    pub placements: Vec<Placement>,
}

impl ChildTake {
    /// A take of one item — what a hand-written fixture wants.
    #[must_use]
    pub fn one(start_secs: f64, length_secs: f64, peaks: TakePeaks) -> Self {
        Self {
            placements: vec![Placement {
                start_secs,
                length_secs,
                peaks,
            }],
        }
    }
}

/// A child of the folder: one source track, with its role, its side and
/// one take of peaks per recorded pass.
///
/// The folder owns no audio. Its item is a view of these
/// (`flow.drums.comping.folder-items`).
#[derive(Clone, Debug, PartialEq)]
pub struct Child {
    pub guid: String,
    pub name: String,
    pub role: LaneRole,
    /// The Channel dimension, when the track has one — a double's L and
    /// R. `None` means the child's own audio channels decide its sides.
    pub side: Option<Side>,
    /// Muted: out of the sum. The folder hears what the mix hears.
    pub muted: bool,
    /// Hidden from the TCP: still in the sum. Hiding is a view, muting
    /// is a signal — so hiding does not move the revision and the
    /// picture replays from cache byte for byte.
    pub hidden: bool,
    pub takes: Vec<ChildTake>,
}

/// One column of the folded picture: per group, the `(min, max)` over
/// every unmuted child of that group, and `None` where the group has no
/// audible child.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FoldColumn {
    pub slots: [Option<(f32, f32)>; SLOTS],
}

impl FoldColumn {
    /// The outer envelope: min of the minima, max of the maxima, across
    /// every group.
    #[must_use]
    pub fn outer(&self) -> Option<(f32, f32)> {
        self.slots
            .iter()
            .flatten()
            .copied()
            .reduce(|(lo, hi), (mn, mx)| (lo.min(mn), hi.max(mx)))
    }
}

/// The folded picture: what to draw, before anything about pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Fold {
    pub group_by: GroupBy,
    pub columns: Vec<FoldColumn>,
}

impl Fold {
    /// The fold as committed text: one line per column, every slot to
    /// six decimals, `-` where no child fed it.
    ///
    /// This is the exact half of a folder-item fixture (the #48
    /// amendment). It carries the decision — which children reached
    /// which column, at what level — it is the same on every machine,
    /// and it is compared byte for byte while the picture beside it is
    /// compared structurally.
    #[must_use]
    pub fn to_text(&self) -> String {
        use core::fmt::Write as _;
        let mut out = format!(
            "group-by {}\ncolumns {}\n",
            self.group_by.as_str(),
            self.columns.len()
        );
        for (i, col) in self.columns.iter().enumerate() {
            let _ = write!(out, "{i:>6}");
            for slot in &col.slots {
                match *slot {
                    Some((lo, hi)) => {
                        let _ = write!(out, "  {lo:>10.6} {hi:>10.6}");
                    }
                    None => {
                        let _ = write!(out, "  {:>10} {:>10}", "-", "-");
                    }
                }
            }
            out.push('\n');
        }
        out
    }
}

/// Where in a column a child's channel lands, or `None` when it lands
/// nowhere.
fn slot_of(group_by: GroupBy, child: &Child, channel: usize, channels: usize) -> Option<usize> {
    match group_by {
        // Roles merge a child's channels: a mono kick mic and a stereo
        // room are each one thing the kit sounds like.
        GroupBy::Role => LaneRole::ALL.iter().position(|r| *r == child.role),
        // Sides split them. A child that declares a Channel is wholly on
        // that side whatever its own channel count — that is a double:
        // two tracks, one per side. A child that declares none is read
        // as one stereo waveform, channel 0 left and channel 1 right.
        GroupBy::Side => child.side.map_or_else(
            || match channel {
                0 => Some(Side::Left.slot()),
                1 if channels > 1 => Some(Side::Right.slot()),
                _ => None,
            },
            |side| Some(side.slot()),
        ),
    }
}

/// The column grid a fold lands on: a window of the timeline, cut into
/// columns.
///
/// One type rather than three parameters, because the three are never
/// apart and because a grid is the thing a fold is *of*: the same
/// children over a different grid is a different picture, which is the
/// whole reason the zoom is in the cache key.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid {
    pub from: f64,
    pub to: f64,
    pub columns: usize,
}

impl Grid {
    /// A grid over `[from, to)` cut into `columns`.
    #[must_use]
    pub const fn over(from: f64, to: f64, columns: usize) -> Self {
        Self { from, to, columns }
    }

    /// How long the window is, in seconds.
    #[must_use]
    pub fn span(self) -> f64 {
        self.to - self.from
    }

    /// How long one column is, in seconds — zero for a degenerate grid.
    #[must_use]
    pub fn secs_per_column(self) -> f64 {
        if self.columns == 0 {
            return 0.0;
        }
        self.span() / crate::num::coord(self.columns)
    }
}

/// Fold `take` of every unmuted child onto `grid`.
///
/// `min(min)` / `max(max)` per column and per group — never a mean. A
/// muted child is skipped; a hidden one is not, because hiding is the
/// TCP's business and the mix still hears it.
///
/// A mono child with no declared side is heard on both sides, the way a
/// mono source in a stereo item is.
///
/// r[impl flow.drums.comping.folder-items]
/// r[impl flow.guitars.folder-items]
#[must_use]
pub fn fold(children: &[Child], take: usize, grid: Grid, group_by: GroupBy) -> Fold {
    let (start_secs, columns) = (grid.from, grid.columns);
    let mut out = vec![FoldColumn::default(); columns];
    let secs_per_col = grid.secs_per_column();
    if secs_per_col <= 0.0 {
        return Fold {
            group_by,
            columns: out,
        };
    }
    for child in children.iter().filter(|c| !c.muted) {
        let Some(lane) = child.takes.get(take) else {
            continue;
        };
        for placed in &lane.placements {
            if placed.length_secs <= 0.0 {
                continue;
            }
            let channels = placed.peaks.channels();
            // A mono child with no declared side is heard on both.
            let both = group_by == GroupBy::Side && child.side.is_none() && channels <= 1;
            // Only the columns the item actually covers: an item three
            // bars into a four-minute session must not cost a walk of
            // every column of it, once per child, per frame.
            let first =
                crate::num::index(((placed.start_secs - start_secs) / secs_per_col).floor());
            let last = crate::num::index(
                ((placed.start_secs + placed.length_secs - start_secs) / secs_per_col).ceil(),
            )
            .min(columns);
            for channel in 0..channels {
                let Some(slot) = slot_of(group_by, child, channel, channels) else {
                    continue;
                };
                for i in first..last {
                    let Some(col) = out.get_mut(i) else { continue };
                    let t0 = crate::num::coord(i).mul_add(secs_per_col, start_secs);
                    let u0 = (t0 - placed.start_secs) / placed.length_secs;
                    let u1 = (t0 + secs_per_col - placed.start_secs) / placed.length_secs;
                    let Some((mn, mx)) = placed.peaks.window(channel, u0, u1) else {
                        continue;
                    };
                    for at in [Some(slot), both.then_some(Side::Right.slot())]
                        .into_iter()
                        .flatten()
                    {
                        if let Some(cell) = col.slots.get_mut(at) {
                            *cell = Some(match *cell {
                                None => (mn, mx),
                                Some((lo, hi)) => (lo.min(mn), hi.max(mx)),
                            });
                        }
                    }
                }
            }
        }
    }
    Fold {
        group_by,
        columns: out,
    }
}
