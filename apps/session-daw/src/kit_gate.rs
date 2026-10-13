//! Where each drum of a shut kit sounds, for its layer of the kit's
//! waveform.
//!
//! A drum's mics hear the whole kit: on Unbreakable the floor tom's mic
//! peaks at -8 dB through bars of kick and snare it never plays in, and
//! drawn as the toms, that bleed was a purple wall over the groove. Each
//! layer is drawn whole around its own hits — where they play now, after
//! every edit ([`crate::hit_overlay`]) — and faint between them, so the
//! kick reads as the kick, the snare as the snare.

use std::cell::RefCell;
use std::rc::Rc;

use expression_editor_core::kit::{LaneRole, kit_role};

/// How much of a layer shows away from its drum's hits: enough to see
/// the bleed is there, not enough to read as the drum.
pub const BLEED_SHOWN: f64 = 0.15;

/// How long before a hit's mark its layer opens, seconds: a marker sits
/// on the attack, a hair after the first of it.
const PRE: f64 = 0.004;

/// A drum's hits, where they play now, sorted, and how long each rings.
#[derive(Debug, PartialEq)]
pub struct Gate {
    hits: Vec<f64>,
    /// Drawn whole this long after a hit, seconds.
    hold: f64,
    /// Then down to [`BLEED_SHOWN`] over this long.
    fade: f64,
}

impl Gate {
    /// A gate on `hits` (any order) for a drum of `role`; `None` for the
    /// rest of the kit (cymbals, rooms), which is all wash.
    #[must_use]
    pub fn new(role: LaneRole, mut hits: Vec<f64>) -> Option<Self> {
        // A tom rings longest, a kick shortest: what the eye reads as the
        // drum, not every tail.
        let (hold, fade) = match role {
            LaneRole::Kick => (0.10, 0.10),
            LaneRole::Snare => (0.12, 0.12),
            LaneRole::Toms => (0.25, 0.20),
            LaneRole::Other => return None,
        };
        hits.retain(|t| t.is_finite());
        hits.sort_by(f64::total_cmp);
        Some(Self { hits, hold, fade })
    }

    /// How much of the layer shows at `t`: 1 on and after a hit, down to
    /// [`BLEED_SHOWN`] as it rings out; the latest hit opened before `t`
    /// says, so a hit in another's tail opens it again.
    #[must_use]
    pub fn at(&self, t: f64) -> f64 {
        let opened = self.hits.partition_point(|h| h - PRE <= t);
        let Some(hit) = opened.checked_sub(1).and_then(|k| self.hits.get(k)) else {
            return BLEED_SHOWN;
        };
        let after = t - hit;
        if after <= self.hold {
            1.0
        } else if after >= self.hold + self.fade {
            BLEED_SHOWN
        } else {
            let gone = (after - self.hold) / self.fade;
            gone.mul_add(BLEED_SHOWN - 1.0, 1.0)
        }
    }
}

/// Each drum's gate as made, `None` for one with no hit marked.
type Made = Vec<(LaneRole, Option<Rc<Gate>>)>;

thread_local! {
    /// The gates last made, and the overlay version they were made from.
    static MADE: RefCell<(u64, Made)> = const { RefCell::new((u64::MAX, Vec::new())) };
}

/// The overlay version a gate of `role` is made from now: what a waveform
/// folded with one keeps, to fold again when the hits move. 0 for a role
/// that is not gated.
#[must_use]
pub fn version(role: Option<LaneRole>) -> u64 {
    match role {
        Some(LaneRole::Kick | LaneRole::Snare | LaneRole::Toms) => crate::hit_overlay::version(),
        _ => 0,
    }
}

/// The gate for `role`'s layer on a shut kit, from the hit markers as they
/// are now.
///
/// `None` (drawn whole) for the rest of the kit, before the markers have
/// been read, or for a drum none of whose hits is marked — one not
/// detected yet is not one never played.
#[must_use]
pub fn of(role: LaneRole) -> Option<Rc<Gate>> {
    if !matches!(role, LaneRole::Kick | LaneRole::Snare | LaneRole::Toms) {
        return None;
    }
    let Some(overlay) = crate::hit_overlay::current() else {
        crate::hit_overlay::refresh();
        return None;
    };
    let version = crate::hit_overlay::version();
    MADE.with_borrow_mut(|(made_at, gates)| {
        if *made_at != version {
            *made_at = version;
            gates.clear();
        }
        if let Some((_, gate)) = gates.iter().find(|(r, _)| *r == role) {
            return gate.clone();
        }
        let hits: Vec<f64> = overlay
            .marks
            .iter()
            .filter(|m| {
                overlay
                    .pieces
                    .get(m.piece)
                    .is_some_and(|piece| kit_role(piece, &[]) == role)
            })
            .map(|m| m.time)
            .collect();
        let gate = (!hits.is_empty())
            .then(|| Gate::new(role, hits))
            .flatten()
            .map(Rc::new);
        gates.push((role, gate.clone()));
        gate
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drum_shows_whole_on_its_hits_and_faint_between() {
        let gate = Gate::new(LaneRole::Kick, vec![2.0, 1.0]).expect("the kick is gated");
        // Before any hit, and long after one: the bleed, faint.
        assert!((gate.at(0.5) - BLEED_SHOWN).abs() < 1e-9);
        assert!((gate.at(1.5) - BLEED_SHOWN).abs() < 1e-9);
        // On the attack (a hair before its mark) and while it rings: whole.
        assert!((gate.at(1.0 - 0.002) - 1.0).abs() < 1e-9);
        assert!((gate.at(1.05) - 1.0).abs() < 1e-9);
        // Ringing out: between.
        let out = gate.at(1.15);
        assert!(out < 1.0 && out > BLEED_SHOWN, "{out}");
        // The next hit opens it again.
        assert!((gate.at(2.01) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn the_wash_is_never_gated() {
        assert_eq!(Gate::new(LaneRole::Other, vec![1.0]), None);
    }
}
