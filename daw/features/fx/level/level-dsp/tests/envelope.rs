//! Envelope generation over synthesised audio.
//!
//! These assert that a curve was produced with the right *shape* and the
//! right invariants — not that a threshold is musically correct. Whether
//! a gate is set well is a listening judgement and is not what a test
//! can settle.

use level_dsp::classify::ClassifyConfig;
use level_dsp::envelope::{analyze, gate_envelope};

const SR: f64 = 48_000.0;

/// Phrase, silence, phrase — the shape a gate exists for.
fn phrase_gap_phrase() -> Vec<f64> {
    let mut out = Vec::new();
    let tone = |n: usize, amp: f64, out: &mut Vec<f64>| {
        for i in 0..n {
            let t = i as f64 / SR;
            out.push(amp * (2.0 * core::f64::consts::PI * 220.0 * t).sin());
        }
    };
    tone((SR * 0.5) as usize, 0.5, &mut out);
    out.extend(core::iter::repeat_n(0.0, (SR * 0.5) as usize));
    tone((SR * 0.5) as usize, 0.5, &mut out);
    out
}

#[test]
fn the_gate_emits_a_curve_rather_than_only_treated_audio() {
    let audio = phrase_gap_phrase();
    let analysis = analyze(&audio, SR, ClassifyConfig::default());
    let env = gate_envelope(&analysis, -40.0, -60.0, 0.25);

    assert!(!env.is_empty(), "the gate's decision is now visible");
    assert!(
        env.iter().any(|p| p.db < -30.0),
        "it closed somewhere — the gap"
    );
    assert!(
        env.iter().any(|p| p.db > -1.0),
        "and it opened somewhere — the phrases"
    );
}

#[test]
fn the_curve_is_sparse_not_one_point_per_block() {
    // 1.5 s at a 10 ms block is 150 blocks. A curve with one point each
    // is what makes a five-minute vocal 2.4 MB, so this is the property
    // that lets envelopes live in the item at all.
    let audio = phrase_gap_phrase();
    let analysis = analyze(&audio, SR, ClassifyConfig::default());
    let env = gate_envelope(&analysis, -40.0, -60.0, 0.25);

    assert!(
        env.len() < analysis.blocks() / 4,
        "{} points for {} blocks is not sparse",
        env.len(),
        analysis.blocks()
    );
}

#[test]
fn the_edges_survive_decimation() {
    let audio = phrase_gap_phrase();
    let analysis = analyze(&audio, SR, ClassifyConfig::default());
    let env = gate_envelope(&analysis, -40.0, -60.0, 0.25);

    // Two closings and two openings at most, but at least one of each,
    // and they must be *adjacent* pairs — an edge smoothed into a ramp
    // over half a second would be a gate that no longer sounds like one.
    let mut sharpest = f64::INFINITY;
    for w in env.windows(2) {
        if (w[0].db - w[1].db).abs() > 30.0 {
            sharpest = sharpest.min(w[1].t_s - w[0].t_s);
        }
    }
    assert!(
        sharpest < 0.05,
        "the gate's edge was smoothed to {sharpest}s"
    );
}

#[test]
fn a_gate_curve_holds_rather_than_ramping() {
    let audio = phrase_gap_phrase();
    let analysis = analyze(&audio, SR, ClassifyConfig::default());
    let env = gate_envelope(&analysis, -40.0, -60.0, 0.25);
    assert!(
        env.iter().all(|p| p.hold),
        "a gate steps; linear interpolation between its points would \
         need far more of them to describe the same shape"
    );
}

#[test]
fn silence_alone_produces_a_closed_curve_and_no_edges() {
    let audio = vec![0.0; (SR * 1.0) as usize];
    let analysis = analyze(&audio, SR, ClassifyConfig::default());
    let env = gate_envelope(&analysis, -40.0, -60.0, 0.25);

    assert!(env.len() <= 2, "nothing happened, so nothing to describe");
    assert!(env.iter().all(|p| p.db < -30.0), "and it stayed shut");
}

#[test]
fn one_analysis_pass_serves_every_stage() {
    // The shared pass is what makes four independent curves affordable —
    // and what lets the rider know where silence is without consulting
    // the gate.
    let audio = phrase_gap_phrase();
    let analysis = analyze(&audio, SR, ClassifyConfig::default());

    assert_eq!(analysis.blocks(), analysis.rms_db.len());
    assert!(analysis.blocks() > 100, "1.5s at 10ms blocks");
    assert!(
        analysis.silence_db < 0.0 && analysis.silence_db.is_finite(),
        "an adaptive floor was found for this material: {}",
        analysis.silence_db
    );
    assert!(analysis.time_of(0) > 0.0);
    assert!(analysis.time_of(analysis.blocks() - 1) < 1.6);
}

// ── The other three (#204) ───────────────────────────────────────────

use level_dsp::envelope::{breath_envelope, ride_envelope, sibilance_spans};

/// Loud tonal phrase, quiet breath, loud tonal phrase.
fn phrase_breath_phrase() -> Vec<f64> {
    let mut out = Vec::new();
    let tone = |n: usize, amp: f64, hz: f64, out: &mut Vec<f64>| {
        for i in 0..n {
            let t = i as f64 / SR;
            out.push(amp * (2.0 * core::f64::consts::PI * hz * t).sin());
        }
    };
    tone((SR * 0.4) as usize, 0.5, 220.0, &mut out);
    // A breath: quiet broadband noise, not bright enough to be an ess.
    let mut seed = 12345u32;
    for _ in 0..(SR * 0.3) as usize {
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        let n = ((seed >> 16) as f64 / 32768.0) - 1.0;
        out.push(n * 0.01);
    }
    tone((SR * 0.4) as usize, 0.5, 220.0, &mut out);
    out
}

#[test]
fn every_stage_emits_its_own_curve_from_one_pass() {
    let audio = phrase_breath_phrase();
    let a = analyze(&audio, SR, ClassifyConfig::default());

    let gate = gate_envelope(&a, -40.0, -60.0, 0.25);
    let breath = breath_envelope(&a, 12.0, -30.0, -70.0, 0.25);
    let ride = ride_envelope(&a, -18.0, 1.0, 6.0, 6.0, 0.25);
    let sib = sibilance_spans(&a, 4.0);

    assert!(!gate.is_empty());
    assert!(!breath.is_empty());
    assert!(!ride.is_empty());
    // Sibilance may legitimately find nothing in this material; the
    // point is that it ran off the same pass.
    let _ = sib;
}

#[test]
fn sibilance_is_spans_with_gain_not_a_dense_curve() {
    let audio = phrase_breath_phrase();
    let a = analyze(&audio, SR, ClassifyConfig::default());
    let spans = sibilance_spans(&a, 4.0);

    assert!(
        spans.len() < a.blocks() / 10,
        "{} spans for {} blocks is not a discrete-event representation",
        spans.len(),
        a.blocks()
    );
    for s in &spans {
        assert!(s.to_s > s.from_s, "a span has width");
        assert!(s.db < 0.0, "sibilance reduces");
    }
}

#[test]
fn the_ride_does_not_track_the_noise_floor_between_phrases() {
    // The reason the stages share an analysis pass: without the silence
    // floor the rider would boost room tone in the gap by the full
    // correction, which is exactly what gating-first used to prevent.
    let audio = phrase_breath_phrase();
    let a = analyze(&audio, SR, ClassifyConfig::default());
    let ride = ride_envelope(&a, -18.0, 1.0, 24.0, 6.0, 0.25);

    let peak = ride.iter().map(|p| p.db).fold(f64::MIN, f64::max);
    assert!(
        peak < 24.0,
        "the ride hit its ceiling, which means it chased the floor: {peak}"
    );
}

#[test]
fn the_ride_holds_between_phrases_rather_than_returning_to_unity() {
    let audio = phrase_breath_phrase();
    let a = analyze(&audio, SR, ClassifyConfig::default());
    let ride = ride_envelope(&a, -18.0, 1.0, 6.0, 6.0, 0.25);

    // A ride that snapped to 0 dB in every gap would swell at each
    // phrase edge.
    let zeros = ride.iter().filter(|p| p.db.abs() < 1e-9).count();
    assert!(
        zeros <= 1,
        "{zeros} points at unity suggests it reset between phrases"
    );
}

#[test]
fn the_curves_are_independent_of_each_other() {
    // Each must be bypassable on its own, so none may be derived from
    // another's output. Generating one twice with a different *other*
    // stage's settings must change nothing.
    let audio = phrase_breath_phrase();
    let a = analyze(&audio, SR, ClassifyConfig::default());

    let ride_a = ride_envelope(&a, -18.0, 1.0, 6.0, 6.0, 0.25);
    // Nothing about the gate's configuration is an input to the ride.
    let _tight_gate = gate_envelope(&a, -10.0, -90.0, 0.25);
    let ride_b = ride_envelope(&a, -18.0, 1.0, 6.0, 6.0, 0.25);

    assert_eq!(ride_a, ride_b, "the ride does not know what the gate did");
}

#[test]
fn a_breath_ducks_and_the_phrases_do_not() {
    let audio = phrase_breath_phrase();
    let a = analyze(&audio, SR, ClassifyConfig::default());
    let breath = breath_envelope(&a, 12.0, -30.0, -70.0, 0.25);

    assert!(breath.iter().any(|p| p.db < -6.0), "something got ducked");
    assert!(
        breath.iter().any(|p| p.db.abs() < 1e-9),
        "and something did not"
    );
}

#[test]
fn a_breath_curve_ramps_rather_than_stepping() {
    // Unlike a gate: a breath ducks in and out over a slew.
    let audio = phrase_breath_phrase();
    let a = analyze(&audio, SR, ClassifyConfig::default());
    let breath = breath_envelope(&a, 12.0, -30.0, -70.0, 0.25);
    assert!(breath.iter().all(|p| !p.hold));
}

// ── The composite (#205) ─────────────────────────────────────────────

use level_dsp::envelope::{composite, Contributions, EnvPoint, GainSpan, COMPOSITE_FLOOR_DB};

fn flat(db: f64) -> Vec<EnvPoint> {
    vec![EnvPoint::new(0.0, db), EnvPoint::new(1.0, db)]
}

/// Read the composite at a time, the way a consumer would.
fn at(env: &[EnvPoint], t: f64) -> f64 {
    match env.iter().position(|p| p.t_s >= t) {
        None => env.last().unwrap().db,
        Some(0) => env[0].db,
        Some(i) => {
            let (a, b) = (env[i - 1], env[i]);
            let span = b.t_s - a.t_s;
            if span.abs() < 1e-12 {
                b.db
            } else {
                a.db + (b.db - a.db) * ((t - a.t_s) / span)
            }
        }
    }
}

#[test]
fn the_composite_is_the_sum_in_db() {
    // Exact, not approximate: summing dB is multiplying linear gains.
    let parts = Contributions {
        gate: flat(-3.0),
        breath: flat(-2.0),
        ride: flat(4.0),
        sibilance: Vec::new(),
        ..Default::default()
    };
    let env = composite(&parts);
    assert!((at(&env, 0.5) - (-1.0)).abs() < 1e-9, "-3 -2 +4 = -1");
}

#[test]
fn sibilance_only_applies_across_its_span() {
    let parts = Contributions {
        gate: Vec::new(),
        breath: Vec::new(),
        ride: flat(0.0),
        sibilance: vec![GainSpan {
            from_s: 0.4,
            to_s: 0.6,
            db: -5.0,
        }],
        ..Default::default()
    };
    let env = composite(&parts);
    assert!((at(&env, 0.2) - 0.0).abs() < 1e-9, "before the ess");
    assert!((at(&env, 0.5) - (-5.0)).abs() < 1e-9, "during it");
    assert!((at(&env, 0.8) - 0.0).abs() < 1e-9, "after it");
}

#[test]
fn a_closed_gate_is_silent_but_finite() {
    let parts = Contributions {
        gate: flat(-400.0),
        breath: Vec::new(),
        ride: Vec::new(),
        sibilance: Vec::new(),
        ..Default::default()
    };
    let env = composite(&parts);
    assert!(env.iter().all(|p| p.db.is_finite()), "no infinities escape");
    assert!(env.iter().all(|p| p.db >= COMPOSITE_FLOOR_DB - 1e-9));
    assert!((at(&env, 0.5) - COMPOSITE_FLOOR_DB).abs() < 1e-9);
}

#[test]
fn a_hand_dragged_zero_does_not_produce_an_infinity_downstream() {
    // Nothing stops a user dragging a curve to silence by hand, so the
    // floor has to exist independently of what the gate does.
    let parts = Contributions {
        gate: Vec::new(),
        breath: flat(f64::NEG_INFINITY),
        ride: Vec::new(),
        sibilance: Vec::new(),
        ..Default::default()
    };
    let env = composite(&parts);
    assert!(env.iter().all(|p| p.db.is_finite()));
}

#[test]
fn editing_one_contribution_recomputes_the_composite() {
    let mut parts = Contributions {
        gate: flat(0.0),
        breath: Vec::new(),
        ride: flat(0.0),
        sibilance: Vec::new(),
        ..Default::default()
    };
    let before = at(&composite(&parts), 0.5);
    parts.ride = flat(6.0);
    let after = at(&composite(&parts), 0.5);
    assert!((after - before - 6.0).abs() < 1e-9);
}

#[test]
fn bypassing_a_stage_is_leaving_it_out() {
    let with = Contributions {
        gate: flat(-10.0),
        breath: Vec::new(),
        ride: flat(3.0),
        sibilance: Vec::new(),
        ..Default::default()
    };
    let without = Contributions {
        gate: Vec::new(),
        ..with.clone()
    };
    assert!((at(&composite(&with), 0.5) - (-7.0)).abs() < 1e-9);
    assert!(
        (at(&composite(&without), 0.5) - 3.0).abs() < 1e-9,
        "the gate's contribution is gone and the ride's is untouched"
    );
}

#[test]
fn a_gate_step_survives_into_the_composite() {
    // The gate holds, so the composite must not ramp through its edge.
    let gate = vec![
        EnvPoint {
            t_s: 0.0,
            db: -60.0,
            hold: true,
        },
        EnvPoint {
            t_s: 0.5,
            db: -60.0,
            hold: true,
        },
        EnvPoint {
            t_s: 0.51,
            db: 0.0,
            hold: true,
        },
    ];
    let env = composite(&Contributions {
        gate,
        ..Default::default()
    });
    assert!(at(&env, 0.25) < -50.0, "still shut a quarter of the way in");
    assert!(at(&env, 0.49) < -50.0, "and right up to the edge");
}

#[test]
fn every_contributions_breakpoints_reach_the_result() {
    let parts = Contributions {
        gate: vec![EnvPoint::new(0.0, 0.0), EnvPoint::new(0.3, -6.0)],
        breath: vec![EnvPoint::new(0.1, 0.0), EnvPoint::new(0.7, -3.0)],
        ride: Vec::new(),
        sibilance: vec![GainSpan {
            from_s: 0.2,
            to_s: 0.4,
            db: -2.0,
        }],
        ..Default::default()
    };
    let env = composite(&parts);
    for t in [0.0, 0.1, 0.2, 0.3, 0.4, 0.7] {
        assert!(
            env.iter().any(|p| (p.t_s - t).abs() < 1e-9),
            "a breakpoint at {t} is where the sum can change slope"
        );
    }
}

#[test]
fn nothing_at_all_composites_to_nothing() {
    assert!(composite(&Contributions::default()).is_empty());
}

// ── Bypass (#206) ────────────────────────────────────────────────────

use level_dsp::envelope::{config_for, generate, Bypass, Cached, GenerationConfig};

#[test]
fn bypassing_removes_a_contribution_and_leaves_the_rest() {
    let mut parts = Contributions {
        gate: flat(-10.0),
        breath: flat(-2.0),
        ride: flat(3.0),
        sibilance: Vec::new(),
        bypass: Bypass::default(),
    };
    assert!((at(&composite(&parts), 0.5) - (-9.0)).abs() < 1e-9);

    parts.bypass.gate = true;
    assert!(
        (at(&composite(&parts), 0.5) - 1.0).abs() < 1e-9,
        "the gate is out; breath and ride are untouched"
    );
}

#[test]
fn a_bypassed_curve_is_retained_and_un_bypassing_restores_it_exactly() {
    let mut parts = Contributions {
        gate: flat(-10.0),
        ride: flat(3.0),
        ..Default::default()
    };
    let before = composite(&parts);

    parts.bypass.gate = true;
    let _ = composite(&parts);
    parts.bypass.gate = false;

    assert_eq!(
        composite(&parts),
        before,
        "nothing was regenerated, so it comes back bit for bit"
    );
    assert_eq!(parts.gate, flat(-10.0), "the curve was never discarded");
}

#[test]
fn bypassing_does_not_alter_any_other_curve() {
    let mut parts = generate(
        &analyze(&phrase_breath_phrase(), SR, ClassifyConfig::default()),
        &GenerationConfig::default(),
    );
    let ride_before = parts.ride.clone();
    let breath_before = parts.breath.clone();

    parts.bypass.gate = true;
    let _ = composite(&parts);

    assert_eq!(parts.ride, ride_before);
    assert_eq!(parts.breath, breath_before);
}

#[test]
fn all_four_bypassed_is_a_flat_unity_composite() {
    let parts = Contributions {
        gate: flat(-10.0),
        breath: flat(-2.0),
        ride: flat(3.0),
        sibilance: vec![GainSpan {
            from_s: 0.1,
            to_s: 0.2,
            db: -5.0,
        }],
        bypass: Bypass {
            gate: true,
            breath: true,
            sibilance: true,
            ride: true,
        },
    };
    let env = composite(&parts);
    assert!(
        env.is_empty() || env.iter().all(|p| p.db.abs() < 1e-9),
        "nothing contributes, so nothing is applied"
    );
}

// ── Generation policy (#207) ─────────────────────────────────────────

#[test]
fn the_same_config_hits_the_cache() {
    let a = analyze(&phrase_breath_phrase(), SR, ClassifyConfig::default());
    let cfg = GenerationConfig::default();
    let cached = Cached::generate(&a, &cfg);
    assert!(
        cached.is_valid_for(&cfg),
        "reopening an item you already tuned does not re-run analysis"
    );
}

#[test]
fn changing_a_threshold_invalidates_the_cache() {
    let a = analyze(&phrase_breath_phrase(), SR, ClassifyConfig::default());
    let cfg = GenerationConfig::default();
    let cached = Cached::generate(&a, &cfg);

    let changed = GenerationConfig {
        gate_threshold_db: -35.0,
        ..cfg
    };
    assert!(!cached.is_valid_for(&changed));
}

#[test]
fn nothing_else_invalidates_it() {
    // The digest covers the settings and only the settings — not the
    // audio, which does not change under an item, and hashing megabytes
    // on every open would cost more than the analysis it avoids.
    let cfg = GenerationConfig::default();
    let same = GenerationConfig::default();
    assert_eq!(cfg.digest(), same.digest());
}

#[test]
fn every_setting_is_actually_in_the_digest() {
    // A field left out of the digest is a stale cache nobody can
    // reproduce, so check each one moves it.
    let base = GenerationConfig::default();
    let mutations: Vec<GenerationConfig> = vec![
        GenerationConfig {
            gate_threshold_db: -1.0,
            ..base
        },
        GenerationConfig {
            gate_floor_db: -1.0,
            ..base
        },
        GenerationConfig {
            breath_reduction_db: 1.0,
            ..base
        },
        GenerationConfig {
            breath_max_level_db: -1.0,
            ..base
        },
        GenerationConfig {
            breath_min_level_db: -1.0,
            ..base
        },
        GenerationConfig {
            sibilance_reduction_db: 1.0,
            ..base
        },
        GenerationConfig {
            ride_target_db: -1.0,
            ..base
        },
        GenerationConfig {
            ride_amount: 0.5,
            ..base
        },
        GenerationConfig {
            ride_max_gain_db: 1.0,
            ..base
        },
        GenerationConfig {
            ride_max_cut_db: 1.0,
            ..base
        },
        GenerationConfig {
            tolerance_db: 1.0,
            ..base
        },
    ];
    for m in mutations {
        assert_ne!(
            base.digest(),
            m.digest(),
            "a setting is missing from the digest"
        );
    }
}

#[test]
fn an_item_inherits_its_tracks_config_and_can_override_it() {
    let track = GenerationConfig {
        breath_reduction_db: 20.0,
        ..Default::default()
    };
    // Thirty comps on that track all start from its threshold.
    assert_eq!(config_for(None, Some(track)).breath_reduction_db, 20.0);

    // Until one of them is tuned.
    let tuned = GenerationConfig {
        breath_reduction_db: 3.0,
        ..Default::default()
    };
    assert_eq!(
        config_for(Some(tuned), Some(track)).breath_reduction_db,
        3.0
    );

    // And with neither, the defaults.
    assert_eq!(
        config_for(None, None).breath_reduction_db,
        GenerationConfig::default().breath_reduction_db
    );
}

#[test]
fn tuning_one_item_does_not_change_another_on_the_same_track() {
    let track = GenerationConfig {
        ride_target_db: -12.0,
        ..Default::default()
    };
    let tuned = GenerationConfig {
        ride_target_db: -24.0,
        ..Default::default()
    };
    assert_eq!(config_for(Some(tuned), Some(track)).ride_target_db, -24.0);
    assert_eq!(config_for(None, Some(track)).ride_target_db, -12.0);
}

#[test]
fn generation_produces_all_four_from_one_pass() {
    let a = analyze(&phrase_breath_phrase(), SR, ClassifyConfig::default());
    let curves = generate(&a, &GenerationConfig::default());
    assert!(!curves.gate.is_empty());
    assert!(!curves.breath.is_empty());
    assert!(!curves.ride.is_empty());
    assert_eq!(curves.bypass, Bypass::default(), "nothing starts bypassed");
}

// ── Absorbing an external edit (#209) ────────────────────────────────

use level_dsp::envelope::absorb_into_ride;

#[test]
fn a_dragged_composite_is_absorbed_into_the_ride() {
    let mut parts = Contributions {
        gate: flat(-3.0),
        breath: flat(-1.0),
        ride: flat(0.0),
        ..Default::default()
    };
    // Someone drags the take volume envelope up 5 dB in the DAW.
    let edited: Vec<EnvPoint> = composite(&parts)
        .into_iter()
        .map(|p| EnvPoint::new(p.t_s, p.db + 5.0))
        .collect();

    absorb_into_ride(&mut parts, &edited);

    assert!(
        (at(&parts.ride, 0.5) - 5.0).abs() < 1e-9,
        "the ride took the whole delta"
    );
}

#[test]
fn after_absorbing_the_four_sum_to_the_composite_again() {
    // The invariant that removes any need for a stale state.
    let mut parts = Contributions {
        gate: flat(-3.0),
        breath: flat(-1.0),
        ride: flat(2.0),
        sibilance: vec![GainSpan {
            from_s: 0.3,
            to_s: 0.4,
            db: -4.0,
        }],
        ..Default::default()
    };
    let edited: Vec<EnvPoint> = composite(&parts)
        .into_iter()
        .map(|p| EnvPoint::new(p.t_s, p.db - 2.5))
        .collect();

    absorb_into_ride(&mut parts, &edited);

    for t in [0.05, 0.2, 0.35, 0.6, 0.9] {
        assert!(
            (at(&composite(&parts), t) - at(&edited, t)).abs() < 1e-6,
            "the sum does not match the user's composite at {t}"
        );
    }
}

#[test]
fn only_the_ride_moves() {
    let mut parts = Contributions {
        gate: flat(-3.0),
        breath: flat(-1.0),
        ride: flat(0.0),
        sibilance: vec![GainSpan {
            from_s: 0.3,
            to_s: 0.4,
            db: -4.0,
        }],
        ..Default::default()
    };
    let gate_before = parts.gate.clone();
    let breath_before = parts.breath.clone();
    let sib_before = parts.sibilance.clone();

    let edited: Vec<EnvPoint> = composite(&parts)
        .into_iter()
        .map(|p| EnvPoint::new(p.t_s, p.db + 3.0))
        .collect();
    absorb_into_ride(&mut parts, &edited);

    assert_eq!(
        parts.gate, gate_before,
        "gain would be wrong when it is shut"
    );
    assert_eq!(parts.breath, breath_before);
    assert_eq!(
        parts.sibilance, sib_before,
        "and sibilance would gain outside its own spans"
    );
}

#[test]
fn absorbing_is_idempotent() {
    // Reloading without a further external edit must change nothing, or
    // the ride would drift on every open.
    let mut parts = Contributions {
        gate: flat(-3.0),
        ride: flat(1.0),
        ..Default::default()
    };
    let unchanged = composite(&parts);

    absorb_into_ride(&mut parts, &unchanged);
    let once = parts.ride.clone();
    let current = composite(&parts);
    absorb_into_ride(&mut parts, &current);
    let twice = parts.ride.clone();

    for t in [0.1, 0.5, 0.9] {
        assert!((at(&once, t) - at(&twice, t)).abs() < 1e-9);
        assert!(
            (at(&twice, t) - 1.0).abs() < 1e-6,
            "the ride did not drift from where it started"
        );
    }
}

#[test]
fn a_partial_edit_is_absorbed_only_where_it_happened() {
    let mut parts = Contributions {
        ride: flat(0.0),
        gate: flat(0.0),
        ..Default::default()
    };
    // Louder in the middle only.
    let edited = vec![
        EnvPoint::new(0.0, 0.0),
        EnvPoint::new(0.4, 0.0),
        EnvPoint::new(0.5, 6.0),
        EnvPoint::new(0.6, 0.0),
        EnvPoint::new(1.0, 0.0),
    ];
    absorb_into_ride(&mut parts, &edited);

    assert!((at(&parts.ride, 0.5) - 6.0).abs() < 1e-6, "the bump landed");
    assert!(at(&parts.ride, 0.1).abs() < 1e-6, "and nothing else moved");
    assert!(at(&parts.ride, 0.9).abs() < 1e-6);
}

#[test]
fn nothing_to_absorb_is_a_no_op() {
    let mut parts = Contributions {
        ride: flat(2.0),
        ..Default::default()
    };
    let before = parts.ride.clone();
    absorb_into_ride(&mut parts, &[]);
    assert_eq!(parts.ride, before);
}

#[test]
fn a_bypassed_stage_does_not_reappear_through_absorption() {
    // The composite excludes a bypassed stage, so the delta is measured
    // against what is actually playing.
    let mut parts = Contributions {
        gate: flat(-10.0),
        ride: flat(0.0),
        bypass: Bypass {
            gate: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let edited: Vec<EnvPoint> = composite(&parts)
        .into_iter()
        .map(|p| EnvPoint::new(p.t_s, p.db + 2.0))
        .collect();
    absorb_into_ride(&mut parts, &edited);

    assert!((at(&parts.ride, 0.5) - 2.0).abs() < 1e-6);
    assert!(parts.bypass.gate, "still bypassed");
}
