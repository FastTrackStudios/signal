//! Four editable lanes, one audible sum.
//!
//! Each concern — gate, compressor, breath, sibilance — keeps its own
//! gain curve, and the take's **volume envelope is their sum**. Editing
//! any one lane and recomputing the sum is what makes the result
//! audible without a plugin in the signal path: the volume envelope is
//! the one per-item gain every host already applies.
//!
//! ## Why not four live FX lanes
//!
//! REAPER has exactly four built-in take envelopes — Volume, Pan, Mute,
//! Pitch — and only one of them is a gain. Four *separately visible*
//! per-item lanes therefore need a plugin on the take to own them, and
//! a plugin is a dependency the editor should not require to do its
//! job. Summing into the volume envelope needs nothing, works in
//! standalone, and survives the item being moved to a machine that does
//! not have our plugins installed.
//!
//! The cost is that the four lanes are edited *here* rather than in the
//! host's envelope panel, and that the sum is recomputed rather than
//! being live. Both are worth it for a write path with no dependencies.
//!
//! ## Unity is a hole, not a value
//!
//! A lane that is off contributes nothing, which is not the same as
//! contributing 0 dB — though the two sum identically. The difference
//! shows when *every* lane is off: there is then no envelope to write,
//! and writing a flat one at unity would leave dead automation on the
//! item that the user has to find and delete.

use crate::dynamics::{Dynamics, GainPoint};

/// Which concern a lane carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DynamicsLane {
    Gate,
    Compressor,
    Breath,
    Sibilance,
}

impl DynamicsLane {
    pub const ALL: [DynamicsLane; 4] = [
        DynamicsLane::Gate,
        DynamicsLane::Compressor,
        DynamicsLane::Breath,
        DynamicsLane::Sibilance,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            DynamicsLane::Gate => "Gate",
            DynamicsLane::Compressor => "Comp",
            DynamicsLane::Breath => "Breath",
            DynamicsLane::Sibilance => "Sibilance",
        }
    }
}

/// The four lanes and their sum.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lanes {
    gate: Vec<GainPoint>,
    compressor: Vec<GainPoint>,
    breath: Vec<GainPoint>,
    sibilance: Vec<GainPoint>,
    /// Frames the lanes span. Kept so an empty lane still knows how
    /// long the take is.
    frames: usize,
}

impl Lanes {
    /// Take the curves a detection pass produced.
    pub fn from_dynamics(d: &Dynamics, frames: usize) -> Self {
        Self {
            gate: d.gate.clone(),
            compressor: d.compressor.clone(),
            breath: d.breath.clone(),
            sibilance: d.sibilance.clone(),
            frames,
        }
    }

    pub fn frames(&self) -> usize {
        self.frames
    }

    pub fn get(&self, lane: DynamicsLane) -> &[GainPoint] {
        match lane {
            DynamicsLane::Gate => &self.gate,
            DynamicsLane::Compressor => &self.compressor,
            DynamicsLane::Breath => &self.breath,
            DynamicsLane::Sibilance => &self.sibilance,
        }
    }

    /// Replace one lane. The sum follows on the next read, which is
    /// what makes "edit any of them and hear it" true.
    pub fn set(&mut self, lane: DynamicsLane, points: Vec<GainPoint>) {
        let slot = match lane {
            DynamicsLane::Gate => &mut self.gate,
            DynamicsLane::Compressor => &mut self.compressor,
            DynamicsLane::Breath => &mut self.breath,
            DynamicsLane::Sibilance => &mut self.sibilance,
        };
        *slot = points;
    }

    /// Switch one lane off without losing the others.
    pub fn clear(&mut self, lane: DynamicsLane) {
        self.set(lane, Vec::new());
    }

    pub fn is_active(&self, lane: DynamicsLane) -> bool {
        !self.get(lane).is_empty()
    }

    /// Whether anything at all is on.
    pub fn is_empty(&self) -> bool {
        DynamicsLane::ALL.iter().all(|&l| !self.is_active(l))
    }

    /// The audible curve: every active lane summed, in dB.
    ///
    /// `None` when nothing is on — see the note on unity above. A flat
    /// envelope at 0 dB and *no* envelope sound the same and are not the
    /// same thing, and the difference is whose job it is to clean up.
    pub fn sum(&self) -> Option<Vec<GainPoint>> {
        if self.is_empty() || self.frames == 0 {
            return None;
        }
        let mut db = vec![0.0f64; self.frames];
        for lane in DynamicsLane::ALL {
            for p in self.get(lane) {
                if let Some(slot) = db.get_mut(p.frame) {
                    *slot += p.db;
                }
            }
        }
        Some(
            db.into_iter()
                .enumerate()
                .map(|(frame, db)| GainPoint { frame, db })
                .collect(),
        )
    }
}

/// How a dB gain maps onto a take volume envelope value.
///
/// REAPER's take volume envelope is a **linear multiplier**, not dB, so
/// every point converts. Unity is 1.0.
pub fn db_to_take_volume(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

/// Inverse of [`db_to_take_volume`], for reading an envelope back.
pub fn take_volume_to_db(value: f64) -> f64 {
    20.0 * value.max(1e-9).log10()
}

/// Thin a gain curve to the points that actually describe it.
///
/// One envelope point per analysis frame is hundreds per second, which
/// makes an envelope that is technically correct and completely
/// unusable — the host draws every one, and a user who wants to nudge
/// the gate has to select a thousand points to do it.
///
/// Douglas–Peucker, which is the algorithm that actually **bounds** the
/// error: no discarded point ends up further than `tolerance_db` from
/// the line drawn between the points either side of it. A greedy
/// neighbour-by-neighbour pass is cheaper and does not bound anything —
/// error accumulates across a run of discarded points, and a smooth
/// curve comes back visibly wrong while every individual step looked
/// fine. That version shipped here briefly and a test caught it at four
/// times the tolerance it promised.
///
/// Iterative rather than recursive: a five-minute take is tens of
/// thousands of frames, and a curve that degenerates into a long thin
/// wedge recurses once per point.
pub fn thin(points: &[GainPoint], tolerance_db: f64) -> Vec<GainPoint> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;

    let mut stack = vec![(0usize, points.len() - 1)];
    while let Some((lo, hi)) = stack.pop() {
        if hi <= lo + 1 {
            continue;
        }
        let (a, b) = (points[lo], points[hi]);
        let span = (b.frame - a.frame) as f64;

        let mut worst = 0.0;
        let mut worst_at = lo;
        for (i, p) in points.iter().enumerate().take(hi).skip(lo + 1) {
            // Vertical distance to the chord, which is the right
            // measure here: the x axis is time and the y axis is dB,
            // and only the dB error is audible.
            let on_line = if span <= 0.0 {
                a.db
            } else {
                a.db + (b.db - a.db) * ((p.frame - a.frame) as f64 / span)
            };
            let d = (p.db - on_line).abs();
            if d > worst {
                worst = d;
                worst_at = i;
            }
        }

        if worst > tolerance_db {
            keep[worst_at] = true;
            stack.push((lo, worst_at));
            stack.push((worst_at, hi));
        }
    }

    points
        .iter()
        .zip(keep)
        .filter_map(|(p, k)| k.then_some(*p))
        .collect()
}
