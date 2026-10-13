//! Aligning a drum take to another by its hits.
//!
//! Pairs are built from hit lists whose correspondence is known by
//! construction, so "did it match the right hits" is checked rather than
//! assumed.

use expression_editor_audio::Onset;
use expression_editor_audio::align_hits::{HitAlignConfig, Pair, align_hits, pair_hits};

const FPS: f64 = 172.265625; // 44100 / 256, the default onset hop.

fn hits(frames: &[usize]) -> Vec<Onset> {
    frames
        .iter()
        .map(|&frame| Onset {
            frame,
            strength: 1.0,
            centroid_hz: 300.0,
            peak: 0.8,
        })
        .collect()
}

fn secs(frames: f64) -> f64 {
    frames / FPS
}

/// Seconds the map moves dub frame `f` by.
fn shift_at(a: &expression_editor_audio::Alignment, f: usize) -> f64 {
    (a.map[f.min(a.map.len() - 1)] - f as f64) / a.frame_rate
}

#[test]
fn a_late_dub_hit_is_pulled_onto_the_reference() {
    let reference = hits(&[100, 200, 300, 400]);
    // Every hit 5 frames (~29 ms) late.
    let dub = hits(&[105, 205, 305, 405]);
    let a = align_hits(&reference, &dub, 500, FPS, HitAlignConfig::default()).expect("aligned");

    for f in [105, 205, 305, 405] {
        let want = secs(-5.0);
        let got = shift_at(&a, f);
        assert!(
            (got - want).abs() < secs(0.5),
            "dub hit at frame {f} moved {got}s, wanted {want}s"
        );
    }
}

#[test]
fn a_hit_with_no_partner_in_range_is_left_where_it_was() {
    // A ghost note the reference does not have, a long way from any of
    // its hits. Dragging it somewhere would be worse than leaving it.
    let reference = hits(&[100, 300]);
    let dub = hits(&[102, 200, 302]);
    let pairs = pair_hits(&reference, &dub, HitAlignConfig::default(), FPS);

    assert_eq!(
        pairs,
        vec![
            Pair {
                dub: 102,
                reference: 100.0
            },
            Pair {
                dub: 302,
                reference: 300.0
            },
        ],
        "the orphan at 200 must not claim a partner"
    );
}

#[test]
fn one_reference_hit_cannot_absorb_a_whole_roll() {
    // The failure straight nearest-neighbour has: every hit of a buzz
    // points at the same reference hit, and retiming stacks them on top
    // of each other.
    let reference = hits(&[200]);
    let dub = hits(&[196, 200, 204, 208]);
    let pairs = pair_hits(&reference, &dub, HitAlignConfig::default(), FPS);

    assert_eq!(pairs.len(), 1, "one reference hit, one pair: {pairs:?}");
    assert_eq!(pairs[0].dub, 200, "the closest hit should win the claim");
}

#[test]
fn nothing_moves_further_than_the_configured_limit() {
    // A hit 40 frames (~230 ms) out with a 50 ms limit. It *could* be
    // matched — there is a reference hit there — but the user asked for
    // corrections no larger than 50 ms, and that is a promise about the
    // result.
    let reference = hits(&[100, 300]);
    let dub = hits(&[140, 300]);
    let a = align_hits(&reference, &dub, 400, FPS, HitAlignConfig::default()).expect("aligned");
    assert!(
        a.max_shift_secs() <= 0.06,
        "moved {}s, limit was 0.05s",
        a.max_shift_secs()
    );
}

#[test]
fn strength_scales_the_correction() {
    let reference = hits(&[100, 200]);
    let dub = hits(&[106, 206]);
    let full = align_hits(&reference, &dub, 300, FPS, HitAlignConfig::default()).unwrap();
    let half = align_hits(
        &reference,
        &dub,
        300,
        FPS,
        HitAlignConfig {
            strength: 0.5,
            ..HitAlignConfig::default()
        },
    )
    .unwrap();
    let none = align_hits(
        &reference,
        &dub,
        300,
        FPS,
        HitAlignConfig {
            strength: 0.0,
            ..HitAlignConfig::default()
        },
    )
    .unwrap();

    let (f, h) = (shift_at(&full, 106), shift_at(&half, 106));
    assert!((h - f * 0.5).abs() < secs(0.5), "half is half: {f} vs {h}");
    assert!(none.max_shift_secs() < 1e-9, "zero strength moves nothing");
}

#[test]
fn the_map_never_goes_backwards() {
    // A backwards step renders as a stutter — the same audio played
    // twice.
    let reference = hits(&[100, 150, 400, 420]);
    let dub = hits(&[104, 148, 396, 424]);
    let a = align_hits(&reference, &dub, 500, FPS, HitAlignConfig::default()).expect("aligned");
    for pair in a.map.windows(2) {
        assert!(pair[1] >= pair[0], "map went backwards: {pair:?}");
    }
}

#[test]
fn hits_that_would_cross_are_dropped_rather_than_reordered() {
    // Two dub hits whose nearest reference hits are in the opposite
    // order. Honouring both would need the time map to run backwards
    // between them.
    let reference = hits(&[100, 110]);
    let dub = hits(&[108, 112]);
    let pairs = pair_hits(&reference, &dub, HitAlignConfig::default(), FPS);
    for pair in pairs.windows(2) {
        assert!(
            pair[1].dub > pair[0].dub && pair[1].reference > pair[0].reference,
            "pairs must agree on order: {pairs:?}"
        );
    }
}

#[test]
fn the_lead_in_holds_its_correction_rather_than_decaying_to_none() {
    // A cymbal swell before the first hit has to arrive with it. Letting
    // the correction fade to zero at the start puts the swell and the
    // crash out of step.
    let reference = hits(&[200, 300]);
    let dub = hits(&[206, 306]);
    let a = align_hits(&reference, &dub, 400, FPS, HitAlignConfig::default()).expect("aligned");

    let at_hit = shift_at(&a, 206);
    let before = shift_at(&a, 20);
    assert!(
        (before - at_hit).abs() < secs(0.5),
        "lead-in shifted {before}s but the hit it leads into shifted {at_hit}s"
    );
}

#[test]
fn no_hits_declines_rather_than_inventing_a_map() {
    let reference = hits(&[100]);
    assert!(align_hits(&reference, &[], 300, FPS, HitAlignConfig::default()).is_none());
    assert!(align_hits(&[], &reference, 300, FPS, HitAlignConfig::default()).is_none());
    assert!(align_hits(&reference, &reference, 0, FPS, HitAlignConfig::default()).is_none());
}

#[test]
fn the_map_covers_every_dub_frame() {
    // The renderer walks it end to end; a short map is a truncated take.
    let reference = hits(&[100, 200]);
    let dub = hits(&[104, 204]);
    let a = align_hits(&reference, &dub, 512, FPS, HitAlignConfig::default()).expect("aligned");
    assert_eq!(a.map.len(), 512);
}

#[test]
fn a_whole_kit_lines_up_against_a_reference_groove() {
    // The end-to-end case: a dub playing behind the beat by a varying
    // amount, which is what a human drummer does and what a constant
    // offset cannot fix.
    let reference: Vec<usize> = (0..8).map(|b| 100 + b * 50).collect();
    let dub: Vec<usize> = (0..8).map(|b| 100 + b * 50 + b).collect();
    let a = align_hits(
        &hits(&reference),
        &hits(&dub),
        600,
        FPS,
        HitAlignConfig::default(),
    )
    .expect("aligned");

    // Each hit lands on its reference, and the drift is corrected
    // progressively rather than as one lump.
    for (r, d) in reference.iter().zip(&dub) {
        let landed = a.map[*d];
        assert!(
            (landed - *r as f64).abs() < 0.6,
            "dub hit {d} landed at {landed}, wanted {r}"
        );
    }
}
