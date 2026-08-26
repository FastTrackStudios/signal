//! Shared mixer math — the mute / solo / group-mute folding and dB→linear
//! conversion every rig mixer re-implemented (keys lanes, drum strips,
//! guitar/bass trims).
//!
//! The semantics are daw's: a lane is audible when it isn't muted, its group
//! (engine / bus) isn't muted, and — if *any* lane is soloed — it is one of
//! the soloed lanes. The audible lane renders at its own fader gain; group
//! faders are separate cells applied by the engine above, exactly as a daw
//! track fader sits under its folder's.

/// dB → linear gain. `0.0` dB = unity.
pub fn db_to_linear(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// One lane's mixer state, as seen by the fold.
#[derive(Clone, Copy, Debug, Default)]
pub struct LaneMix {
    /// The lane's own fader (dB).
    pub gain_db: f32,
    pub muted: bool,
    pub soloed: bool,
    /// Whether the lane has anything to sound at all (an empty lane is
    /// silent regardless of its fader).
    pub live: bool,
}

/// Any lane soloed? (Solo silences every un-soloed lane.)
pub fn any_solo<'a>(lanes: impl IntoIterator<Item = &'a LaneMix>) -> bool {
    lanes.into_iter().any(|l| l.soloed)
}

/// The linear gain a lane should render at right now, folding in its own
/// mute, its group's mute, solo-exclusion, and liveness. Group *faders* are
/// their own cell — not folded in here.
pub fn lane_gain(lane: &LaneMix, group_muted: bool, any_solo: bool) -> f32 {
    let solo_excluded = any_solo && !lane.soloed;
    if lane.muted || group_muted || solo_excluded || !lane.live {
        0.0
    } else {
        db_to_linear(lane.gain_db)
    }
}

/// The linear gain a group (engine / bus) fader contributes: its fader when
/// unmuted, silence when muted.
pub fn group_gain(gain_db: f32, muted: bool) -> f32 {
    if muted {
        0.0
    } else {
        db_to_linear(gain_db)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lane(gain_db: f32) -> LaneMix {
        LaneMix {
            gain_db,
            muted: false,
            soloed: false,
            live: true,
        }
    }

    #[test]
    fn unity_and_db_conversion() {
        assert_eq!(db_to_linear(0.0), 1.0);
        assert!((db_to_linear(-6.0206) - 0.5).abs() < 1e-4);
    }

    #[test]
    fn mute_solo_and_group_fold() {
        let l = lane(0.0);
        assert_eq!(lane_gain(&l, false, false), 1.0);
        // Own mute silences.
        assert_eq!(lane_gain(&LaneMix { muted: true, ..l }, false, false), 0.0);
        // Group mute silences.
        assert_eq!(lane_gain(&l, true, false), 0.0);
        // Another lane's solo excludes this one…
        assert_eq!(lane_gain(&l, false, true), 0.0);
        // …but a soloed lane still sounds.
        assert_eq!(lane_gain(&LaneMix { soloed: true, ..l }, false, true), 1.0);
        // A dead lane is silent at any fader.
        assert_eq!(lane_gain(&LaneMix { live: false, ..l }, false, false), 0.0);
    }

    #[test]
    fn any_solo_scans_the_set() {
        let lanes = [
            lane(0.0),
            LaneMix {
                soloed: true,
                ..lane(0.0)
            },
        ];
        assert!(any_solo(lanes.iter()));
        assert!(!any_solo([lane(0.0)].iter()));
    }

    #[test]
    fn group_fader() {
        assert_eq!(group_gain(0.0, false), 1.0);
        assert_eq!(group_gain(0.0, true), 0.0);
    }
}
