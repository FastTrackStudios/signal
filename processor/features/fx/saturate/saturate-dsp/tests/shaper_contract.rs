//! The contract every [`SideShaper`] claims to honour, checked against the
//! real curves rather than against prose.
//!
//! `SideShaper`'s own doc comment states it: "All pass through the origin
//! with unity small-signal slope, so `Clean`/`Clean` at Q = 0 is
//! transparent." A saturator additionally owes the mix engineer monotonicity
//! — louder in must never mean quieter out. Neither property was tested, and
//! `Diode` honours neither.

use saturate_dsp::preamp::SideShaper;

const SHAPERS: [(&str, SideShaper); 6] = [
    ("Clean", SideShaper::Clean),
    ("OpAmp", SideShaper::OpAmp),
    ("Tube", SideShaper::Tube),
    ("Transformer", SideShaper::Transformer),
    ("Diode", SideShaper::Diode),
    ("Hard", SideShaper::Hard),
];

/// Central difference at the origin — the small-signal slope the enum's
/// contract pins at unity.
fn slope_at_origin(s: SideShaper) -> f32 {
    let h = 1.0e-4_f32;
    (s.shape(h) - s.shape(-h)) / (2.0 * h)
}

#[test]
fn every_shaper_has_unity_small_signal_slope() {
    let mut broken = Vec::new();
    for (name, s) in SHAPERS {
        let slope = slope_at_origin(s);
        if (slope - 1.0).abs() > 0.02 {
            broken.push(format!(
                "{name}: slope {slope:.4} at the origin ({:+.2} dB), contract says 1.0",
                20.0 * slope.log10()
            ));
        }
    }
    assert!(
        broken.is_empty(),
        "shapers break the documented unity-slope contract:\n  {}",
        broken.join("\n  ")
    );
}

#[test]
fn every_shaper_is_monotonic() {
    // Up to 8 shaper units: `drive` reaches 8x, so this is the range a
    // full-scale sample actually visits at max drive.
    //
    // The tolerance is 1e-4 of full scale, not zero: `tanh_approx` clamps its
    // argument at |x| = 3 and so sits on an exact plateau above it, where the
    // rational evaluation wobbles in the last float bits. That is flat, not
    // folded. What this test is looking for is a curve that gives back real
    // level — measured as the drop from its own running maximum.
    const FLAT: f32 = 1.0e-4;
    let mut broken = Vec::new();
    for (name, s) in SHAPERS {
        let mut peak = s.shape(0.0);
        let mut worst_drop = 0.0_f32;
        let mut worst_x = 0.0_f32;
        for i in 1..=8000 {
            let x = i as f32 / 1000.0;
            let y = s.shape(x);
            peak = peak.max(y);
            let drop = peak - y;
            if drop > worst_drop {
                worst_drop = drop;
                worst_x = x;
            }
        }
        if worst_drop > FLAT {
            broken.push(format!(
                "{name}: peaks at {peak:.4} then gives back {worst_drop:.4} by x={worst_x:.3}; \
                 at x=8 it is {:.4} — a louder input yields a quieter output",
                s.shape(8.0)
            ));
        }
    }
    assert!(
        broken.is_empty(),
        "shapers fold back instead of saturating:\n  {}",
        broken.join("\n  ")
    );
}
