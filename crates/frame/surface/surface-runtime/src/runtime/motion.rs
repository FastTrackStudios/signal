//! Motion: the surface's nodes animated by `Node::motion`.
//!
//! The evaluator is `motion-runtime`'s [`Animator`]; the runtime owns one
//! per surface, feeds it this runtime's params, feeds and pointer state
//! (the [`Inputs`] below), and runs it at the end of [`Runtime::frame`] on
//! the render copy — after the controls, so a motion can layer physics on
//! a part a control already moves.

use document_proto::NodeId;
use document_proto::motion::{FeedRead, Source};
use motion_runtime::{Animator, Inputs, ShapeFn};
use surface_proto::{EntryId, ParamAddr, StreamKind};

use super::Runtime;

/// dBFS a level source reads as 0.
pub const LEVEL_FLOOR_DB: f64 = -60.0;
/// dB of reduction a `gr()` source reads as 1.
pub const GR_SPAN_DB: f64 = 20.0;

impl Runtime {
    /// Whether anything on screen moves by itself: meters falling, a
    /// gesture in flight, or a motion still moving (a spring settling, a
    /// resonator ringing, an LFO running). Settled motions let hosts sleep.
    #[must_use]
    pub fn is_animating(&self) -> bool {
        self.animating()
    }

    /// How many motions the surface runs.
    #[must_use]
    pub fn motions(&self) -> usize {
        self.motion.lock().map_or(0, |a| a.len())
    }

    /// Register (or override) a shape provider motions can draw with —
    /// an engine's exact room model under `reverb.room`, say.
    pub fn register_shape(&mut self, name: impl Into<String>, f: ShapeFn) {
        if let Ok(a) = self.motion.get_mut() {
            a.register_shape(name, f);
        }
    }

    /// Advance the motions to `t` (ms) and write them into `scene`.
    pub(super) fn animate(&self, scene: &mut document_proto::Scene, t: f64) {
        let Ok(mut a) = self.motion.lock() else { return };
        if a.is_empty() {
            return;
        }
        a.advance(t / 1000.0, &Sampler(self));
        a.apply(scene);
    }

    pub(super) fn motion_animating(&self) -> bool {
        self.motion.lock().is_ok_and(|a| a.is_animating())
    }

    /// The control `node` belongs to: itself or its nearest ancestor that
    /// is a control.
    fn control_of(&self, node: NodeId) -> Option<NodeId> {
        let mut at = Some(node);
        while let Some(id) = at {
            if self.surface.controls.contains_key(&id) {
                return Some(id);
            }
            at = self.surface.scene.node(id).and_then(|n| n.parent);
        }
        None
    }
}

/// A fresh animator for a surface's scene.
pub(super) fn animator(scene: &document_proto::Scene) -> std::sync::Mutex<Animator> {
    std::sync::Mutex::new(Animator::new(scene))
}

struct Sampler<'a>(&'a Runtime);

impl Inputs for Sampler<'_> {
    fn source(&self, source: &Source) -> Option<f64> {
        let rt = self.0;
        match source {
            Source::Param { addr, normalized } => {
                let addr = ParamAddr::new(addr.clone());
                let info = rt.catalog.param(&addr);
                let norm = rt.values.get(&addr).copied().or_else(|| info.map(|p| p.default))?;
                if *normalized { Some(norm) } else { Some(info.map_or(norm, |p| p.range.to_real(norm))) }
            }
            Source::Feed { id, read } => {
                let id = EntryId::new(id.clone());
                let values = rt.streams.get(&id)?;
                let kind = rt.catalog.stream(&id).map(|s| &s.kind);
                match read {
                    FeedRead::Raw(i) => values.get(*i as usize).copied(),
                    FeedRead::Level(ch) => {
                        let db = match ch {
                            Some(c) => *values.get(usize::from(*c))?,
                            None => values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                        };
                        db.is_finite().then(|| ((db - LEVEL_FLOOR_DB) / -LEVEL_FLOOR_DB).clamp(0.0, 1.0)).or(Some(0.0))
                    }
                    FeedRead::Gr => values.first().map(|db| (db.abs() / GR_SPAN_DB).clamp(0.0, 1.0)),
                    FeedRead::Scalar => {
                        let v = *values.first()?;
                        Some(match kind {
                            Some(StreamKind::Scalar { range }) => range.to_normalized(v),
                            _ => v,
                        })
                    }
                    FeedRead::Band(lo, hi) => {
                        let Some(StreamKind::Spectrum { bins, f_min, f_max, floor_db }) = kind else { return None };
                        let n = usize::from(*bins).min(values.len());
                        if n == 0 {
                            return None;
                        }
                        let centre = |i: usize| f_min * (f_max / f_min).powf((i as f64 + 0.5) / n as f64);
                        let (lo, hi) = (f64::from(*lo), f64::from(*hi));
                        let inside = (0..n).filter(|i| (lo..=hi).contains(&centre(*i)));
                        let db = inside.map(|i| values[i]).fold(f64::NEG_INFINITY, f64::max);
                        // A band narrower than a bin reads the nearest bin.
                        let db = if db.is_finite() {
                            db
                        } else {
                            let mid = (lo * hi).sqrt().max(1e-9);
                            let i = (0..n).min_by(|a, b| (centre(*a) / mid).ln().abs().total_cmp(&(centre(*b) / mid).ln().abs()))?;
                            values[i]
                        };
                        let floor = floor_db.min(-1.0);
                        Some(((db - floor) / -floor).clamp(0.0, 1.0))
                    }
                }
            }
        }
    }

    fn hover(&self, node: NodeId) -> bool {
        let rt = self.0;
        rt.hovered.is_some_and(|h| rt.control_of(node) == Some(h))
    }

    fn press(&self, node: NodeId) -> bool {
        let rt = self.0;
        rt.captured.is_some_and(|c| rt.control_of(node) == Some(c))
    }
}
