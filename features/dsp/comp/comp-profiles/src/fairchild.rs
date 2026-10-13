//! Fairchild 670 — the variable-mu valve limiter.
//!
//! Three controls per channel and nothing else: INPUT GAIN, THRESHOLD, and a
//! six-position TIME CONSTANT switch. The time constant is the interesting one
//! and the reason this profile exists as data rather than as a skin — each
//! position is a *pair* of attack and release times, positions 4–6 being
//! program-dependent (two-stage release), which is what the "glue" is.
//!
//! Positions, from the unit's own table:
//!
//! | # | attack | release |
//! |---|--------|---------|
//! | 1 | 0.2 ms | 0.3 s   |
//! | 2 | 0.2 ms | 0.8 s   |
//! | 3 | 0.4 ms | 2 s     |
//! | 4 | 0.8 ms | 5 s     |
//! | 5 | 0.4 ms | 2 s / 10 s program-dependent |
//! | 6 | 0.2 ms | 2 s / 25 s program-dependent |

use crate::{Constraint, ParamMapping, Profile, ProfileControl};

pub struct Fairchild670Profile;

/// How many detents the TIME CONSTANT switch has.
pub const TIME_CONSTANTS: usize = 6;

static CONTROLS: &[ProfileControl] = &[
    // A 20 dB input attenuator — clockwise is no attenuation at all.
    ProfileControl {
        id: "input_gain",
        label: "Input Gain",
        mapping: ParamMapping::Direct {
            param: "input_gain_db",
            range: -20.0..=0.0,
        },
    },
    ProfileControl {
        id: "threshold",
        label: "Threshold",
        mapping: ParamMapping::Direct {
            param: "threshold_db",
            range: -40.0..=0.0,
        },
    },
    // One switch, three engine params. Compound rather than Stepped because
    // Stepped drives a single param, and a time constant is by definition a
    // pair — plus the program-dependent positions also want inertia.
    ProfileControl {
        id: "time_constant",
        label: "Time Constant",
        mapping: ParamMapping::Compound {
            mappings: &[
                ("attack_ms", tc_attack),
                ("release_ms", tc_release),
                ("inertia", tc_inertia),
                ("inertia_decay", tc_inertia_decay),
            ],
            range: 0.0..=1.0,
        },
    },
    ProfileControl {
        id: "output",
        label: "Output",
        mapping: ParamMapping::Direct {
            param: "output_gain_db",
            range: 0.0..=20.0,
        },
    },
];

static CONSTRAINTS: &[Constraint] = &[
    // Variable-mu: soft knee, gentle ratio, tube stage always slightly lit.
    Constraint::Fixed {
        param: "style",
        value: 2.0,
    },
    Constraint::Clamped {
        param: "ratio",
        range: 1.5..=6.0,
    },
    Constraint::Clamped {
        param: "knee_db",
        range: 10.0..=24.0,
    },
    Constraint::Fixed {
        param: "detector_rms_mix",
        value: 0.7,
    },
    Constraint::Clamped {
        param: "drive",
        range: 0.05..=0.35,
    },
    Constraint::Fixed {
        param: "character_mode",
        value: 1.0,
    },
];

/// Which detent a 0..1 knob position lands on.
fn position(x: f64) -> usize {
    (x.clamp(0.0, 1.0) * (TIME_CONSTANTS - 1) as f64).round() as usize
}

fn tc_attack(x: f64) -> f64 {
    [0.2, 0.2, 0.4, 0.8, 0.4, 0.2][position(x)]
}

fn tc_release(x: f64) -> f64 {
    // Positions 5 and 6 take their *first* stage here; the long second stage
    // is the inertia below.
    [300.0, 800.0, 2000.0, 5000.0, 2000.0, 2000.0][position(x)]
}

/// Program dependence: none on 1–4, increasing on 5 and 6.
fn tc_inertia(x: f64) -> f64 {
    [0.0, 0.0, 0.0, 0.0, 0.55, 0.85][position(x)]
}

/// How slowly the second stage lets go — 10 s and 25 s on the unit.
fn tc_inertia_decay(x: f64) -> f64 {
    [0.0, 0.0, 0.0, 0.0, 0.9, 0.975][position(x)]
}

impl Profile for Fairchild670Profile {
    fn id(&self) -> &'static str {
        "fairchild670"
    }

    fn name(&self) -> &'static str {
        "Fairchild 670"
    }

    fn controls(&self) -> &[ProfileControl] {
        CONTROLS
    }

    fn constraints(&self) -> &[Constraint] {
        CONSTRAINTS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_time_constant_switch_has_six_detents_that_all_resolve() {
        for i in 0..TIME_CONSTANTS {
            let x = i as f64 / (TIME_CONSTANTS - 1) as f64;
            assert_eq!(position(x), i);
            assert!(tc_attack(x) > 0.0 && tc_release(x) > 0.0);
        }
    }

    #[test]
    fn only_the_last_two_positions_are_program_dependent() {
        let at = |i: usize| i as f64 / (TIME_CONSTANTS - 1) as f64;
        for i in 0..4 {
            assert_eq!(tc_inertia(at(i)), 0.0, "position {} should be fixed", i + 1);
        }
        assert!(tc_inertia(at(4)) > 0.0);
        assert!(tc_inertia(at(5)) > tc_inertia(at(4)));
        // …and the sixth lets go the most slowly of all.
        assert!(tc_inertia_decay(at(5)) > tc_inertia_decay(at(4)));
    }

    #[test]
    fn release_lengthens_across_the_first_four_positions() {
        let at = |i: usize| i as f64 / (TIME_CONSTANTS - 1) as f64;
        let releases: Vec<f64> = (0..4).map(|i| tc_release(at(i))).collect();
        assert!(releases.windows(2).all(|w| w[1] > w[0]), "{releases:?}");
    }
}
