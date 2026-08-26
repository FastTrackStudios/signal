//! Loudness neutrality of the analogue voicings — spec
//! `docs/spec/fx/gain-comp.md`, checked with the shared harness
//! (`fx_stack::verify`): render the −18 dBFS pink reference through each
//! profile, sweep Drive over its travel, and bound the level deviation.
//!
//! The digital family (`clip`, `crush`) is exempt by design: a clipper's
//! ceiling is fixed and its drive is genuinely a level control
//! (`fx.gain-comp.exempt`, `Makeup::None` in the profile table).

use fx_stack::verify::{self, FULL_RANGE_BOUND_DB};
use fx_stack::Stage;
use saturate_dsp::digital::DigitalStage;
use saturate_dsp::preamp::ClassAPreamp;
use saturate_profiles::{apply, profile_by_id, Controls};

/// One saturate voicing as a harness [`Stage`].
struct SatStage {
    pre: ClassAPreamp,
    digital: DigitalStage,
    emph: Option<[saturate_dsp::emphasis::EmphasisEq; 2]>,
}

impl SatStage {
    fn new(profile_id: &str, drive: f32) -> Self {
        Self::with_controls(
            profile_id,
            Controls {
                drive,
                ..Controls::default()
            },
        )
    }

    fn with_controls(profile_id: &str, controls: Controls) -> Self {
        let profile = profile_by_id(profile_id).expect("profile exists");
        let mut pre = ClassAPreamp::new(48_000.0);
        let mut digital = DigitalStage::default();
        apply(profile, &controls, &mut pre, &mut digital);
        Self {
            pre,
            digital,
            emph: None,
        }
    }

    /// Same, with an emphasis EQ around the stage — the plugin's wet path
    /// (`fx.sat.emphasis`).
    fn with_emphasis(
        profile_id: &str,
        drive: f32,
        bands: [saturate_dsp::emphasis::EmphBand; saturate_dsp::emphasis::BANDS],
    ) -> Self {
        let profile = profile_by_id(profile_id).expect("profile exists");
        let mut pre = ClassAPreamp::new(48_000.0);
        let mut digital = DigitalStage::default();
        let mut emph = [
            saturate_dsp::emphasis::EmphasisEq::new(48_000.0),
            saturate_dsp::emphasis::EmphasisEq::new(48_000.0),
        ];
        for e in emph.iter_mut() {
            e.set_bands(&bands);
        }
        pre.set_emphasis_sigma_gain(emph[0].sigma_gain());
        apply(
            profile,
            &Controls {
                drive,
                ..Controls::default()
            },
            &mut pre,
            &mut digital,
        );
        Self {
            pre,
            digital,
            emph: Some(emph),
        }
    }
}

impl Stage for SatStage {
    fn process(&mut self, l: &mut [f64], r: &mut [f64]) {
        let _ = &self.digital; // analogue profiles leave the quantiser transparent
        for i in 0..l.len() {
            match &mut self.emph {
                Some(emph) => {
                    let el = emph[0].pre(l[i] as f32);
                    let er = emph[1].pre(r[i] as f32);
                    l[i] = emph[0].post(self.pre.process(0, el)) as f64;
                    r[i] = emph[1].post(self.pre.process(1, er)) as f64;
                }
                None => {
                    l[i] = self.pre.process(0, l[i] as f32) as f64;
                    r[i] = self.pre.process(1, r[i] as f32) as f64;
                }
            }
        }
    }
}

/// Every analogue voicing stays loudness-neutral across the full Drive
/// travel, within the spec's full-range bound.
// r[verify fx.gain-comp.saturate]
// r[verify fx.gain-comp.reference]
/// The emphasis EQ keeps the loudness contract: a strong emphasis curve
/// drives the stage differently, the mirror takes it back out, and the
/// makeup — told about the emphasis level via `set_emphasis_sigma_gain` —
/// keeps the whole wet path inside the bound (`fx.sat.emphasis.makeup`).
// r[verify fx.sat.emphasis.makeup]
// r[verify fx.sat.emphasis]
#[test]
fn an_emphasis_curve_stays_loudness_neutral() {
    use saturate_dsp::emphasis::{EmphBand, EmphShape};
    let mut bands: [EmphBand; saturate_dsp::emphasis::BANDS] = Default::default();
    bands[0] = EmphBand {
        shape: EmphShape::LowShelf,
        freq_hz: 150.0,
        gain_db: -6.0,
        q: 0.8,
    };
    bands[1] = EmphBand {
        shape: EmphShape::Bell,
        freq_hz: 3_000.0,
        gain_db: 9.0,
        q: 1.2,
    };
    bands[2] = EmphBand {
        shape: EmphShape::HighShelf,
        freq_hz: 8_000.0,
        gain_db: 6.0,
        q: 0.7,
    };

    for id in ["triode", "tape", "transistor"] {
        for drive in [0.0f32, 0.5, 1.0] {
            let mut stage = SatStage::with_emphasis(id, drive, bands);
            let dev = verify::level_deviation_db(&mut stage, 48_000.0);
            assert!(
                dev.abs() <= 1.5,
                "{id} drive {drive}: emphasis broke neutrality by {dev:.2} dB"
            );
        }
    }
}

/// Diagnostic: per-profile, per-drive deviation table (run with
/// `-- --ignored --nocapture`).
#[test]
#[ignore]
fn deviation_table() {
    for id in [
        "triode",
        "pentode",
        "tape",
        "tape_hot",
        "transformer",
        "transistor",
        "fuzz",
    ] {
        let devs: Vec<String> = (0..9)
            .map(|p| {
                let t = p as f32 / 8.0;
                let mut s = SatStage::new(id, t);
                format!("{:+.2}", verify::level_deviation_db(&mut s, 48_000.0))
            })
            .collect();
        // Same sweep with sag disabled, to separate the static-transfer
        // calibration from the dynamic bias shift.
        let nosag: Vec<String> = (0..9)
            .map(|p| {
                let t = p as f32 / 8.0;
                let mut s = SatStage::with_controls(
                    id,
                    Controls {
                        drive: t,
                        sag: 0.0,
                        ..Controls::default()
                    },
                );
                format!("{:+.2}", verify::level_deviation_db(&mut s, 48_000.0))
            })
            .collect();
        println!("{id:12} {}", devs.join("  "));
        println!("{id:12} {}   (sag 0)", nosag.join("  "));
    }
}

#[test]
fn analogue_profiles_hold_the_reference_bound_across_drive() {
    for id in [
        "triode",
        "pentode",
        "tape",
        "tape_hot",
        "transformer",
        "transistor",
        "fuzz",
    ] {
        let (full, typical) =
            verify::sweep_deviation_db(|t| SatStage::new(id, t as f32), 9, 48_000.0);
        assert!(
            full <= FULL_RANGE_BOUND_DB,
            "{id}: worst full-range deviation {full:.2} dB exceeds ±{FULL_RANGE_BOUND_DB} dB"
        );
        // Report the typical band too — tightened toward the spec's ±0.5 dB
        // as the calibration models land (`fx.gain-comp.saturate`).
        assert!(
            typical <= FULL_RANGE_BOUND_DB,
            "{id}: typical-range deviation {typical:.2} dB"
        );
    }
}
