//! Align: pull one source's events onto another's.
//!
//! The third tool, and the one that proves the seam generalises beyond
//! the grid. Quantize aligns to *divisions*; Align aligns to **another
//! performance** — a doubled guitar to the take it doubles, a MIDI part
//! to the audio it was played against, a kit's overheads to its snare.
//!
//! Both sides are only [`Timed`], so any pairing works: MIDI to audio,
//! audio to MIDI, either to either. That genericity is the point. The
//! previous implementation was audio-typed functions with no target
//! selection and no MIDI path, which is why it was not a tool.
//!
//! It produces the same [`Plan`] quantize does, so preview, apply and
//! the undo story are shared rather than reimplemented.

use crate::event::{Timed, length_of};
use crate::quantize::{Move, Plan};

/// How aggressively to pull, and how far to look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlignConfig {
    /// How far a target event may be from a reference event and still
    /// be considered its partner, in document units.
    ///
    /// The single most important control: too wide and a downbeat pairs
    /// with the wrong reference hit, which sounds far worse than not
    /// aligning at all.
    pub window: f64,
    /// 0..1. 1 lands exactly on the reference, 0.5 halves the error —
    /// the setting that tightens a double without making it a clone.
    pub strength: f64,
}

impl Default for AlignConfig {
    fn default() -> Self {
        Self {
            window: 0.0,
            strength: 1.0,
        }
    }
}

/// Pair each target event with the nearest reference event and plan the
/// moves.
///
/// **Each reference event takes at most one target**, the nearest — the
/// same rule quantize applies to divisions, and for the same reason: a
/// flam or a buzz would otherwise collapse several target hits onto one
/// reference hit and destroy the very timing detail being aligned.
///
/// A target with no reference inside the window is reported in
/// `unmatched` rather than dragged somewhere: "why did that not move" is
/// the first question a user asks.
pub fn plan_align<T: Timed, R: Timed>(targets: &[T], reference: &[R], cfg: AlignConfig) -> Plan {
    let mut plan = Plan::default();
    if targets.is_empty() || reference.is_empty() {
        plan.unmatched = targets.iter().map(|t| t.onset()).collect();
        return plan;
    }

    let strength = cfg.strength.clamp(0.0, 1.0);
    // Which target currently claims each reference event, and how far
    // away it is.
    let mut claimed: Vec<Option<(usize, f64)>> = vec![None; reference.len()];

    for (i, target) in targets.iter().enumerate() {
        let pos = target.onset();
        let mut best: Option<(usize, f64)> = None;
        for (j, r) in reference.iter().enumerate() {
            let d = (r.onset() - pos).abs();
            if cfg.window > 0.0 && d > cfg.window {
                continue;
            }
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((j, d));
            }
        }
        let Some((j, d)) = best else { continue };

        match claimed[j] {
            // A nearer target wins the reference event; the loser is
            // left alone rather than pushed onto a second-choice hit.
            Some((_, other)) if other <= d => {}
            _ => claimed[j] = Some((i, d)),
        }
    }

    let mut moved = vec![false; targets.len()];
    for (j, slot) in claimed.iter().enumerate() {
        let Some((i, _)) = *slot else { continue };
        let from = targets[i].onset();
        let to = from + (reference[j].onset() - from) * strength;
        moved[i] = true;
        plan.moves.push(Move {
            index: i,
            from,
            to,
            division: reference[j].onset(),
            length: targets[i].length(),
        });
    }
    plan.moves.sort_by_key(|m| m.index);

    for (i, t) in targets.iter().enumerate() {
        if !moved[i] {
            plan.unmatched.push(t.onset());
        }
    }
    plan
}

/// The same, for targets known to carry a length.
///
/// Align never changes a length: a note keeps it and its end moves with
/// its start, exactly as quantize does.
pub fn plan_align_sustained<T: crate::event::Sustained, R: Timed>(
    targets: &[T],
    reference: &[R],
    cfg: AlignConfig,
) -> Plan {
    let mut plan = plan_align(targets, reference, cfg);
    for m in &mut plan.moves {
        m.length = length_of(&targets[m.index]);
    }
    plan
}
