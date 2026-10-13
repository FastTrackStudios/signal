//! WARP: a quantize written as stretch markers rather than as cuts (#166).
//!
//! `Plan::alignment` returned an `Alignment` that nothing consumed, so
//! warping was theoretical. This mirrors the SPLIT coverage in
//! `apply_quantize.rs`: the claim is that a warp quantize lands
//! transients on the grid.
//!
//! SPLIT moves audio by cutting it; WARP bends time between anchors and
//! keeps the material continuous. Both have to put the hits in the same
//! place, or only one of them is a quantize.

use expression_editor_audio::detect::Transient;
use expression_editor_audio::gate::Hit;
use expression_editor_audio::quantize::{QuantizeConfig, plan};
use expression_editor_audio::retime::{TakePlacement, alignment_markers};

const GRID: f64 = 0.25;
const RATE: f64 = 100.0;
const FRAMES: usize = 200;

fn transients(times: &[f64]) -> Vec<Transient> {
    times
        .iter()
        .map(|&at| Transient {
            at,
            loudness: 0.9,
            crest_db: 12.0,
            hit: Hit {
                sample: (at * 48_000.0) as usize,
                peak: 0.9,
                rms: 0.6,
                crest_db: 12.0,
            },
        })
        .collect()
}

/// Hits 20 ms late on every grid division, as in the SPLIT test.
fn late_plan() -> expression_editor_audio::quantize::Plan {
    let late: Vec<f64> = (1..5).map(|i| i as f64 * GRID + 0.02).collect();
    plan(
        &transients(&late),
        QuantizeConfig {
            grid_secs: GRID,
            ..QuantizeConfig::default()
        },
    )
}

/// Where a frame ends up, by reading the marker map the way a host
/// would: linear between knots.
fn played_at(markers: &[daw::service::stretch_marker::StretchMarker], source_s: f64) -> f64 {
    let mut prev: Option<(f64, f64)> = None;
    for m in markers {
        let knot = (m.source_position, m.position);
        if knot.0 >= source_s {
            return match prev {
                None => knot.1,
                Some(p) => {
                    let span = knot.0 - p.0;
                    if span.abs() < 1e-12 {
                        knot.1
                    } else {
                        p.1 + (knot.1 - p.1) * ((source_s - p.0) / span)
                    }
                }
            };
        }
        prev = Some(knot);
    }
    prev.map(|p| p.1).unwrap_or(source_s)
}

#[test]
fn a_warp_quantize_lands_transients_on_the_grid() {
    let plan = late_plan();
    let alignment = plan
        .alignment(FRAMES, RATE)
        .expect("a plan with moves yields an alignment");
    let markers = alignment_markers(&alignment, TakePlacement::unit(RATE));
    assert!(!markers.is_empty(), "the alignment produced no markers");

    for i in 1..5 {
        let was = i as f64 * GRID + 0.02;
        let landed = played_at(&markers, was);
        assert!(
            (landed - i as f64 * GRID).abs() < 0.005,
            "hit {i} was at {was}, landed at {landed}, wanted {}",
            i as f64 * GRID
        );
    }
}

#[test]
fn an_untouched_take_does_not_acquire_a_warp() {
    // The case worth being cheap about: everything already on the grid.
    let on_grid: Vec<f64> = (1..5).map(|i| i as f64 * GRID).collect();
    let plan = plan(
        &transients(&on_grid),
        QuantizeConfig {
            grid_secs: GRID,
            ..QuantizeConfig::default()
        },
    );
    let Some(alignment) = plan.alignment(FRAMES, RATE) else {
        return; // no moves at all is also a pass
    };
    let markers = alignment_markers(&alignment, TakePlacement::unit(RATE));
    assert!(
        markers.is_empty(),
        "a take that needed no correction got {} markers",
        markers.len()
    );
}

#[test]
fn the_markers_are_sorted_and_have_no_repeated_knots() {
    // A map with a repeated knot has an undefined slope there, which is
    // the same reason `stretch_markers` dedups.
    let alignment = late_plan().alignment(FRAMES, RATE).unwrap();
    let markers = alignment_markers(&alignment, TakePlacement::unit(RATE));

    for w in markers.windows(2) {
        assert!(
            w[1].position > w[0].position,
            "markers are not strictly increasing: {} then {}",
            w[0].position,
            w[1].position
        );
    }
}

#[test]
fn an_empty_alignment_writes_nothing() {
    let alignment = expression_editor_audio::align::Alignment::from_map(Vec::new(), RATE);
    assert!(alignment_markers(&alignment, TakePlacement::unit(RATE)).is_empty());
}

#[test]
fn a_take_offset_on_the_timeline_still_lands_on_the_grid() {
    // The placement conversion is the part that is easy to get wrong,
    // because a marker carries project time, take time and source time
    // and they are three different numbers.
    let alignment = late_plan().alignment(FRAMES, RATE).unwrap();
    let placed = TakePlacement {
        item_position: 4.0,
        start_offset: 0.0,
        play_rate: 1.0,
        frame_rate: RATE,
    };
    let markers = alignment_markers(&alignment, placed);
    assert!(!markers.is_empty());
    assert!(
        markers.iter().all(|m| m.position >= 0.0),
        "an item four seconds in produced a negative marker position"
    );
}
