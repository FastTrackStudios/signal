//! What the flam sweep actually says about `onsets.rs`.
//!
//! The deliverable of #176: `onsets.rs` documents its own conservatism
//! in prose — "the second strike of a flam rises out of the first one's
//! decay rather than out of silence, so its spectral change is small
//! and it falls below the threshold" — and this turns that sentence
//! into numbers a regression can move.
//!
//! Four things are asserted, and they are deliberately different in
//! kind:
//!
//! - **Structural** properties that must hold whatever the detector
//!   does: the accent is always found, recall never goes backwards as
//!   the spacing opens up. A failure here is a bug.
//! - **The knee**, checked as a band rather than a point, because the
//!   exact spacing is a measurement and not a specification.
//! - **The committed baseline**, row by row, so any change to the
//!   detector shows as a diff against a recorded curve rather than as
//!   a pass or a fail.
//! - **The defaults**, which cannot see a flam at all — worth an
//!   assertion because it is a surprising and load-bearing fact.
//!
//! Every number here is measured on the synthetic sweep. See
//! `synth.rs`: it is an optimistic bound, because a synthetic strike is
//! cleaner than a real one and there is no bleed. The real kit render
//! is what `fetch-corpus.sh` produces, and it can only be worse.

use expression_editor_audio::onsets::OnsetConfig;
use expression_editor_corpus::flam::{FlamSweep, Side};
use expression_editor_corpus::recall::{
    Curve, Tolerance, accent_lag, flam_config, measure, recall_curve,
};

/// The curve this crate's default sweep produces, as recorded.
const BASELINE: &str = include_str!("../fixtures/flam-recall-baseline.csv");

fn measured(cfg: OnsetConfig) -> Curve {
    let rendered = FlamSweep::default().render();
    let results = measure(
        &rendered.samples,
        rendered.sample_rate,
        &rendered.cases,
        cfg,
        Tolerance::default(),
    );
    recall_curve(&results)
}

fn series(curve: &Curve, side: Side) -> Vec<(f64, f64)> {
    curve
        .0
        .iter()
        .filter(|p| p.side == side)
        .map(|p| (p.spacing_ms, p.flam_recall()))
        .collect()
}

#[test]
fn the_accent_is_never_the_strike_that_goes_missing_when_it_comes_first() {
    // Structural. In the ghost-after ordering nothing precedes the
    // accent, so a detector that misses it has a problem far worse than
    // flam sensitivity.
    let curve = measured(flam_config());
    for p in curve.0.iter().filter(|p| p.side == Side::After) {
        assert_eq!(
            p.accent_found, p.cases,
            "accent missed at {} ms in the ghost-after ordering",
            p.spacing_ms
        );
    }
}

#[test]
fn recall_never_goes_backwards_as_the_spacing_opens_up() {
    // Structural, and the one property that makes the curve a curve.
    // A dip means the detector is doing something spacing-dependent
    // that is not "more room is easier".
    let curve = measured(flam_config());
    for side in [Side::Before, Side::After] {
        let s = series(&curve, side);
        for pair in s.windows(2) {
            assert!(
                pair[1].1 >= pair[0].1 - 1e-9,
                "{}: recall fell from {:.0}% at {} ms to {:.0}% at {} ms",
                side.as_str(),
                pair[0].1 * 100.0,
                pair[0].0,
                pair[1].1 * 100.0,
                pair[1].0
            );
        }
    }
}

#[test]
fn a_grace_before_the_accent_is_resolved_from_around_forty_milliseconds() {
    // The knee, as a band. The measured value is 40 ms; asserting the
    // band records where it is without pretending 40 is a requirement.
    let curve = measured(flam_config());
    let knee = curve
        .knee_ms(Side::Before, 1.0)
        .expect("the grace-first ordering resolves somewhere in 5–60 ms");
    assert!(
        (25.0..=50.0).contains(&knee),
        "knee moved to {knee} ms; curve:\n{}",
        curve.to_csv()
    );
    // And it is genuinely graded rather than a step: something is
    // resolved well before the knee.
    let at_20 = series(&curve, Side::Before)
        .into_iter()
        .find(|(ms, _)| *ms == 20.0)
        .expect("20 ms is in the grid")
        .1;
    assert!(at_20 > 0.0, "nothing resolved at 20 ms");
}

#[test]
fn a_ghost_inside_the_previous_decay_is_never_resolved_in_the_flam_range() {
    // The documented conservatism, measured. Across the whole 5–60 ms
    // sweep, at every ghost velocity, a ghost landing on the accent's
    // decay is not separated even once.
    //
    // Widening the spacing axis by hand puts that knee near 200 ms:
    //   drum-corpus recall --spacings-ms 60,80,100,140,200,300
    // That is not a threshold worth hunting — it is a statement that
    // spectral flux, log-compressed, cannot see a quiet strike inside a
    // loud one's decay, and that the gate (which runs sample by sample
    // and races two envelopes) is the engine that has to.
    let curve = measured(flam_config());
    for p in curve.0.iter().filter(|p| p.side == Side::After) {
        assert_eq!(
            p.both_found, 0,
            "the ghost-after ordering resolved {} of {} at {} ms — the curve has \
             improved and the baseline needs regenerating",
            p.both_found, p.cases, p.spacing_ms
        );
    }
}

#[test]
fn the_default_config_cannot_separate_a_flam_at_any_spacing_below_its_floor() {
    // Load-bearing and easy to forget: `OnsetConfig::default` sets
    // `min_spacing_secs` to 50 ms, so for anything tighter than that the
    // answer is decided by a policy rule before the audio is examined.
    // Any future work on flam sensitivity that leaves this at its
    // default is measuring nothing.
    let cfg = OnsetConfig::default();
    assert_eq!(cfg.min_spacing_secs, 0.05);
    let curve = measured(cfg);
    for p in curve.0.iter().filter(|p| p.spacing_ms < 50.0) {
        assert_eq!(
            p.both_found,
            0,
            "{} at {} ms resolved under the default 50 ms spacing floor",
            p.side.as_str(),
            p.spacing_ms
        );
    }
}

#[test]
fn detections_lag_their_strike_by_about_a_hop() {
    // Not a flam property, but the sweep is the only place it gets
    // measured, and quantize cares: a detector that reports every hit
    // 6 ms late moves every note 6 ms late.
    let rendered = FlamSweep::default().render();
    let results = measure(
        &rendered.samples,
        rendered.sample_rate,
        &rendered.cases,
        flam_config(),
        Tolerance::default(),
    );
    let lag = accent_lag(&results);
    assert!(lag.matched > 60, "only {} accents matched", lag.matched);
    assert!(
        (2.0..=9.0).contains(&lag.median_ms),
        "median lag {:.1} ms",
        lag.median_ms
    );
    assert!(lag.worst_ms <= 15.0, "worst lag {:.1} ms", lag.worst_ms);
}

#[test]
fn the_curve_matches_the_committed_baseline() {
    // The regression proper. Compared row by row with a tolerance of
    // one case, because the detector's decisions are threshold
    // comparisons on FFT output and a borderline case can flip on a
    // machine with different floating-point contraction. Anything
    // larger than one case in one row is a real behavioural change and
    // wants the baseline regenerated deliberately:
    //
    //   drum-corpus recall --csv .../fixtures/flam-recall-baseline.csv
    //
    // The baseline is tied to the *default* grid. Changing the spacing
    // or velocity axes changes the global maximum the detection
    // function is normalised against, so the numbers move for reasons
    // that have nothing to do with the detector.
    let expected = Curve::parse_csv(BASELINE).expect("baseline parses");
    let actual = measured(flam_config());
    assert_eq!(
        actual.0.len(),
        expected.0.len(),
        "the grid changed; regenerate the baseline"
    );
    for (a, e) in actual.0.iter().zip(&expected.0) {
        assert_eq!((a.side, a.spacing_ms), (e.side, e.spacing_ms));
        assert_eq!(a.cases, e.cases);
        for (name, got, want) in [
            ("flam", a.both_found, e.both_found),
            ("ghost", a.ghost_found, e.ghost_found),
            ("accent", a.accent_found, e.accent_found),
        ] {
            assert!(
                got.abs_diff(want) <= 1,
                "{} at {} ms: {name} {got}/{} against a baseline of {want}\nmeasured:\n{}",
                a.side.as_str(),
                a.spacing_ms,
                a.cases,
                actual.to_csv()
            );
        }
    }
}
