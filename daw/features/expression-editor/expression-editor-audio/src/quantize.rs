//! Quantizing audio to the project grid.
//!
//! Same shape as the alignment paths — detect, choose targets, produce a
//! time map — with the targets coming from the grid instead of from
//! another take. See `spec/grid-quantize.md`.
//!
//! **The matching itself is not here.** It lives in
//! [`expression_editor_tools::quantize`], written once over
//! [`expression_editor_tools::Timed`], so a drum take and a MIDI
//! take are put on the grid by the same code. What is left here is the
//! part that is genuinely audio: seconds, and the two ways a plan
//! becomes sound — [`Plan::alignment`] warps, [`Plan::splits`] cuts.
//! Both are built from the same decisions, so the two modes can never
//! disagree about where a hit belongs.

use std::ops::Deref;

use expression_editor_tools::Timed;
use expression_editor_tools::quantize as tools;

use crate::align::Alignment;
use crate::detect::Transient;

pub use expression_editor_tools::quantize::Move;

/// How transients are matched to grid divisions.
///
/// The audio face of [`tools::QuantizeConfig`]. It exists so this domain
/// can say `secs` and mean it: audio is sampled at a rate, so seconds
/// are its natural unit, where a MIDI grid under a tempo map is only
/// evenly spaced in ticks. Converting at the boundary is a line; a seam
/// that assumed seconds would be wrong the first time the tempo moved.
#[derive(Clone, Copy, Debug)]
pub struct QuantizeConfig {
    /// Seconds between grid divisions.
    pub grid_secs: f64,
    /// Where the grid starts, in seconds from the take's start.
    ///
    /// Takes do not begin on a division, and assuming they do puts every
    /// hit out by the remainder.
    pub grid_offset_secs: f64,
    /// Half-width of the window each division scans, in seconds. `None`
    /// turns grid scan off: every transient snaps to its own nearest
    /// division instead.
    pub tolerance_secs: Option<f64>,
    /// `0.0` leaves the take alone, `1.0` puts every hit on its
    /// division.
    pub strength: f64,
}

impl Default for QuantizeConfig {
    fn default() -> Self {
        Self {
            // 1/16 at 120 bpm.
            grid_secs: 0.125,
            grid_offset_secs: 0.0,
            tolerance_secs: Some(0.05),
            strength: 1.0,
        }
    }
}

impl From<QuantizeConfig> for tools::QuantizeConfig {
    fn from(cfg: QuantizeConfig) -> Self {
        Self {
            grid: cfg.grid_secs,
            grid_offset: cfg.grid_offset_secs,
            tolerance: cfg.tolerance_secs,
            strength: cfg.strength,
            // Audio gets its sensitivity filter upstream: the detector's
            // gate never reports the bleed, so by the time transients
            // reach the planner they are all real hits. MIDI, which has
            // no detector, filters here instead.
            min_strength: 0.0,
        }
    }
}

/// What quantizing will do to a take, before anything is written.
///
/// The generic plan plus the two ways audio can be rendered from it.
/// Derefs to [`tools::Plan`], so `moves` and `unmatched` are the same
/// list every other mode reads.
#[derive(Clone, Debug, Default)]
pub struct Plan(pub tools::Plan);

impl Deref for Plan {
    type Target = tools::Plan;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Match transients to grid divisions.
///
/// With a tolerance set, **each division takes at most one transient**,
/// the loudest inside its window. That one rule does most of the work: a
/// buzz roll cannot produce eight hits fighting over one division, a
/// ghost note between divisions is left alone rather than dragged onto a
/// beat it was never near, and a division with nothing near it stays
/// empty — silence is not quantized onto.
///
/// Without one, every transient snaps to its own nearest division, which
/// is right for material that is not on a grid and wrong for a kit.
///
/// Concrete in `Transient` on purpose: a caller here always has
/// transients, and the seam is one call down.
pub fn plan(transients: &[Transient], cfg: QuantizeConfig) -> Plan {
    Plan(tools::plan(transients, cfg.into()))
}

/// A transient is a *measurement* of the take, so the seam's
/// [`Timed::move_to`] only records where the hit is now expected to
/// land — it does not move any audio. The audio moves when the plan is
/// rendered, as a warp map or as split pieces.
impl Timed for Transient {
    fn onset(&self) -> f64 {
        self.at
    }

    fn move_to(&mut self, to: f64) {
        self.at = to;
    }

    /// Detected loudness, on whichever scale the detector was configured
    /// for — the same number the one-per-division contest already ranked
    /// hits by.
    fn strength(&self) -> f64 {
        self.loudness
    }

    // No `length`: a transient is the instant a stick meets a head.
    // Inventing an end for it — the decay? the next hit? — would be a
    // number the tools then acted on. So `Transient` is deliberately
    // not `Sustained`, and that is the whole difference between the
    // audio and MIDI sides of this seam.
}

impl Plan {
    /// The plan as a time map, for WARP mode.
    ///
    /// `frames` and `frame_rate` describe the map the caller needs, not
    /// the analysis: this is what the renderer walks, so it has to cover
    /// the take end to end at whatever resolution the caller is working
    /// in.
    ///
    /// Between moved transients the map is linear, which spreads the
    /// correction across the decay of one hit and the space before the
    /// next — the part with no features of its own to preserve. Outside
    /// the first and last it holds constant rather than decaying to
    /// zero, so a lead-in arrives with the hit it leads into.
    pub fn alignment(&self, frames: usize, frame_rate: f64) -> Option<Alignment> {
        if self.moves.is_empty() || frames == 0 || frame_rate <= 0.0 {
            return None;
        }
        let mut anchors: Vec<(f64, f64)> = Vec::with_capacity(self.moves.len() + 2);
        let first = &self.moves[0];
        anchors.push((0.0, first.shift() * frame_rate));
        for m in &self.moves {
            anchors.push((m.from * frame_rate, m.shift() * frame_rate));
        }
        let last = self.moves.last().expect("non-empty");
        anchors.push(((frames - 1) as f64, last.shift() * frame_rate));
        anchors.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        anchors.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-9);

        let mut map = Vec::with_capacity(frames);
        let mut seg = 0usize;
        for i in 0..frames {
            let x = i as f64;
            while seg + 1 < anchors.len() && anchors[seg + 1].0 < x {
                seg += 1;
            }
            let (x0, d0) = anchors[seg];
            let (x1, d1) = anchors[(seg + 1).min(anchors.len() - 1)];
            let offset = if (x1 - x0).abs() < 1e-9 {
                d0
            } else {
                let t = ((x - x0) / (x1 - x0)).clamp(0.0, 1.0);
                d0 + (d1 - d0) * t
            };
            map.push((x + offset).max(0.0));
        }
        for i in 1..map.len() {
            if map[i] < map[i - 1] {
                map[i] = map[i - 1];
            }
        }
        // The knots are already known — they are the moves, plus the two
        // held ends — so the alignment carries them rather than making
        // the write path rediscover them from the map. One stretch
        // marker per quantized hit, not one per analysis frame.
        let last_index = anchors.len().saturating_sub(1);
        let warp_anchors = anchors
            .iter()
            .enumerate()
            .map(|(i, &(x, shift))| crate::align::Anchor {
                dub: x as usize,
                // Clamped exactly as the map above is: there is no
                // global offset in a quantize plan, so a target before
                // the take's own start is not a take that belongs
                // earlier — it is a correction with nowhere to go.
                reference: (x + shift).max(0.0),
                confidence: 1.0,
                kind: match i {
                    0 => crate::align::AnchorKind::Start,
                    i if i == last_index => crate::align::AnchorKind::End,
                    _ => crate::align::AnchorKind::Onset,
                },
            })
            .collect();

        Some(Alignment {
            map,
            anchors: warp_anchors,
            offset: crate::align::Offset::NONE,
            frame_rate,
        })
    }

    /// The plan as a list of pieces to cut and move, for SPLIT mode.
    ///
    /// `take_secs` is the item's length; the last piece runs to it.
    pub fn splits(&self, take_secs: f64, cfg: SplitConfig) -> Vec<Piece> {
        if self.moves.is_empty() {
            return Vec::new();
        }
        let pad = cfg.leading_pad_secs.max(0.0);
        let mut pieces = Vec::with_capacity(self.moves.len() + 1);

        // The audio before the first transient is a piece too, and it
        // does not move: nothing in it was detected, so nothing about it
        // is late.
        let first_cut = (self.moves[0].from - pad).max(0.0);
        if first_cut > 0.0 {
            pieces.push(Piece {
                cut: 0.0,
                end: first_cut,
                shift: 0.0,
                transient: None,
            });
        }

        for (i, m) in self.moves.iter().enumerate() {
            // The cut goes *before* the transient; the shift is measured
            // at the transient. Conflating the two is the classic way to
            // get this wrong — the audio then arrives `pad` early and
            // every hit is flammed against the rest of the kit.
            let cut = (m.from - pad).max(0.0);
            let end = match self.moves.get(i + 1) {
                Some(next) => (next.from - pad).max(cut),
                None => take_secs.max(cut),
            };
            pieces.push(Piece {
                cut,
                end,
                shift: m.shift(),
                transient: Some(m.from),
            });
        }
        pieces
    }
}

/// How SPLIT mode cuts.
#[derive(Clone, Copy, Debug)]
pub struct SplitConfig {
    /// Cut this far *before* each transient, without moving the snap
    /// point.
    ///
    /// A cut exactly on an attack clips the front of it. A cut 5–10 ms
    /// early leaves the attack whole and puts the join in the previous
    /// hit's decay, where a crossfade is inaudible.
    pub leading_pad_secs: f64,
    /// Crossfade length at each join. Moving pieces leaves a gap or an
    /// overlap at every cut, and a hard edge there is a click.
    pub crossfade_secs: f64,
}

impl Default for SplitConfig {
    fn default() -> Self {
        Self {
            leading_pad_secs: 0.007,
            crossfade_secs: 0.007,
        }
    }
}

/// One piece of a split item.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    /// Where the piece is cut from the source, in seconds from the
    /// take's start.
    pub cut: f64,
    /// Where it ends, in seconds from the take's start.
    pub end: f64,
    /// How far it moves. Positive is later.
    pub shift: f64,
    /// The transient this piece carries, if any — the lead-in piece has
    /// none.
    pub transient: Option<f64>,
}

impl Piece {
    pub fn len(&self) -> f64 {
        (self.end - self.cut).max(0.0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() <= 0.0
    }

    /// Where the piece lands.
    pub fn placed(&self) -> f64 {
        self.cut + self.shift
    }
}
