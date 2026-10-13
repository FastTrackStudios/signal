//! Quantizing audio to the grid.
//!
//! Every case builds a kit part from explicit hit times with a known
//! error against a known grid, so "did it fix the timing" is compared
//! against the answer rather than eyeballed.

use expression_editor_audio::detect::{DetectConfig, Filters, Scale, Transient, transients};
use expression_editor_audio::gate::{GateConfig, Hit};
use expression_editor_audio::quantize::{QuantizeConfig, SplitConfig, plan};

const SR: f64 = 44100.0;
/// 1/8 notes at 120 bpm.
const GRID: f64 = 0.25;

fn noise(seed: &mut u64) -> f64 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    ((*seed >> 33) as f64 / (1u64 << 31) as f64) - 1.0
}

/// A struck hit: fast attack, exponential decay.
fn strike(out: &mut [f64], at: f64, amp: f64, hz: f64, decay: f64, seed: &mut u64) {
    let start = (at * SR) as usize;
    let n = (decay * SR) as usize;
    for i in 0..n {
        let Some(slot) = out.get_mut(start + i) else {
            break;
        };
        let t = i as f64 / SR;
        let env = (-t / (decay * 0.3)).exp();
        let tone = (core::f64::consts::TAU * hz * t).sin();
        *slot += (0.7 * tone + 0.3 * noise(seed)) * env * amp;
    }
}

/// A slow swell — loud, but not struck. What the crest control is for.
fn swell(out: &mut [f64], at: f64, amp: f64, len: f64) {
    let start = (at * SR) as usize;
    let n = (len * SR) as usize;
    for i in 0..n {
        let Some(slot) = out.get_mut(start + i) else {
            break;
        };
        let t = i as f64 / SR;
        let env = (t / len * core::f64::consts::PI).sin();
        *slot += (core::f64::consts::TAU * 600.0 * t).sin() * env * amp;
    }
}

fn take(secs: f64) -> Vec<f64> {
    vec![0.0; (SR * secs) as usize]
}

/// Hits at the given times, all the same kind.
fn kit(times: &[f64], secs: f64) -> Vec<f64> {
    let mut out = take(secs);
    let mut seed = 0x51ceu64;
    for &t in times {
        strike(&mut out, t, 0.8, 110.0, 0.10, &mut seed);
    }
    out
}

fn found(audio: &[f64], cfg: DetectConfig) -> Vec<Transient> {
    transients(audio, SR, cfg)
}

/// Synthetic transients, for testing the planner without a detector.
fn fake(times: &[(f64, f64)]) -> Vec<Transient> {
    times
        .iter()
        .map(|&(at, loudness)| Transient {
            at,
            loudness,
            crest_db: 12.0,
            hit: Hit {
                sample: (at * SR) as usize,
                peak: loudness,
                rms: loudness * 0.7,
                crest_db: 12.0,
            },
        })
        .collect()
}

#[test]
fn a_late_hit_is_pulled_onto_its_division() {
    // Eighths, each 20 ms late.
    let times: Vec<f64> = (0..8).map(|i| i as f64 * GRID + 0.02).collect();
    let p = plan(
        &fake(&times.iter().map(|&t| (t, 0.9)).collect::<Vec<_>>()),
        QuantizeConfig {
            grid_secs: GRID,
            ..QuantizeConfig::default()
        },
    );
    assert_eq!(p.moves.len(), 8);
    for m in &p.moves {
        assert!(
            (m.to - m.division).abs() < 1e-9,
            "full strength should land exactly on the division: {m:?}"
        );
        assert!((m.shift() + 0.02).abs() < 1e-9);
    }
}

#[test]
fn strength_scales_the_correction_and_zero_moves_nothing() {
    let times = fake(&[(0.02, 0.9), (0.27, 0.9)]);
    let at = |strength: f64| {
        plan(
            &times,
            QuantizeConfig {
                grid_secs: GRID,
                strength,
                ..QuantizeConfig::default()
            },
        )
    };
    let full = at(1.0);
    let half = at(0.5);
    let none = at(0.0);

    assert!((half.moves[0].shift() - full.moves[0].shift() * 0.5).abs() < 1e-9);
    assert!(none.moves.iter().all(|m| m.shift().abs() < 1e-9));
    // Strength never changes which division a hit belongs to.
    assert_eq!(none.moves[0].division, full.moves[0].division);
}

#[test]
fn a_hit_already_on_the_grid_does_not_move_at_any_strength() {
    let times = fake(&[(0.0, 0.9), (GRID, 0.9)]);
    for strength in [0.0, 0.3, 1.0] {
        let p = plan(
            &times,
            QuantizeConfig {
                grid_secs: GRID,
                strength,
                ..QuantizeConfig::default()
            },
        );
        assert!(p.moves.iter().all(|m| m.shift().abs() < 1e-9));
    }
}

#[test]
fn a_division_takes_at_most_one_transient() {
    // A buzz roll: four hits inside one division's window. Without the
    // one-per-division rule all four quantize to the same instant and
    // stack on top of each other.
    let times = fake(&[(0.24, 0.4), (0.25, 0.9), (0.26, 0.3), (0.27, 0.2)]);
    let p = plan(
        &times,
        QuantizeConfig {
            grid_secs: GRID,
            tolerance_secs: Some(0.05),
            ..QuantizeConfig::default()
        },
    );
    assert_eq!(p.moves.len(), 1, "one division, one move: {:?}", p.moves);
    // And it is the loudest, not the nearest — a ghost note a hair
    // closer to the beat must not win over the hit itself.
    assert!((p.moves[0].from - 0.25).abs() < 1e-9);
    assert_eq!(p.unmatched.len(), 3);
}

#[test]
fn a_ghost_note_between_divisions_is_left_alone() {
    // Off-grid by more than the tolerance. Dragging it onto a beat it
    // was never near is worse than leaving it.
    let times = fake(&[(0.0, 0.9), (0.125, 0.5), (0.25, 0.9)]);
    let p = plan(
        &times,
        QuantizeConfig {
            grid_secs: GRID,
            tolerance_secs: Some(0.03),
            ..QuantizeConfig::default()
        },
    );
    assert_eq!(p.moves.len(), 2);
    assert_eq!(p.unmatched, vec![0.125]);
}

#[test]
fn a_division_with_nothing_near_it_stays_empty() {
    // Silence must not be quantized onto — a division with no transient
    // produces no move at all.
    let times = fake(&[(0.0, 0.9), (0.75, 0.9)]);
    let p = plan(
        &times,
        QuantizeConfig {
            grid_secs: GRID,
            tolerance_secs: Some(0.03),
            ..QuantizeConfig::default()
        },
    );
    assert_eq!(p.moves.len(), 2, "two hits, two moves — not four");
}

#[test]
fn grid_scan_off_snaps_every_transient_to_its_nearest_division() {
    // Right for material that is not on a grid at all. Each hit here is
    // nearest a *different* division — two hits nearest the same one
    // still cannot both have it, whatever the scan setting.
    let times = fake(&[(0.02, 0.9), (0.26, 0.5), (0.53, 0.9)]);
    let p = plan(
        &times,
        QuantizeConfig {
            grid_secs: GRID,
            tolerance_secs: None,
            ..QuantizeConfig::default()
        },
    );
    assert_eq!(p.moves.len(), 3, "nothing is left behind");
    assert!(p.unmatched.is_empty());
}

#[test]
fn grid_scan_decides_whether_an_off_grid_hit_moves() {
    // The switch, stated as the difference it makes. A hit halfway
    // between divisions is left alone when scanning (it belongs to no
    // division) and dragged to the nearest one when not.
    let times = fake(&[(0.0, 0.9), (0.125, 0.5), (0.25, 0.9)]);
    let at = |tolerance| {
        plan(
            &times,
            QuantizeConfig {
                grid_secs: GRID,
                tolerance_secs: tolerance,
                ..QuantizeConfig::default()
            },
        )
    };
    assert_eq!(at(Some(0.03)).unmatched, vec![0.125]);

    let off = at(None);
    assert!(
        off.moves.iter().any(|m| (m.from - 0.125).abs() < 1e-9),
        "with scan off the ghost gets a division too"
    );
}

#[test]
fn two_hits_nearest_the_same_division_cannot_both_have_it() {
    // Whatever the scan setting: the later one is left where it is
    // rather than stacked on top of the first.
    let times = fake(&[(0.13, 0.5), (0.27, 0.9)]);
    let p = plan(
        &times,
        QuantizeConfig {
            grid_secs: GRID,
            tolerance_secs: None,
            ..QuantizeConfig::default()
        },
    );
    assert_eq!(p.moves.len(), 1);
    assert_eq!(p.unmatched.len(), 1);
}

#[test]
fn a_grid_that_does_not_start_at_zero_is_honoured() {
    // Takes do not begin on a division. Assuming they do puts every hit
    // out by the remainder — and consistently, so it looks like the
    // detector is biased rather than like the grid is wrong.
    let offset = 0.06;
    let times = fake(&[(offset + 0.01, 0.9), (offset + GRID + 0.01, 0.9)]);
    let p = plan(
        &times,
        QuantizeConfig {
            grid_secs: GRID,
            grid_offset_secs: offset,
            ..QuantizeConfig::default()
        },
    );
    for m in &p.moves {
        assert!(
            ((m.division - offset) / GRID).fract().abs() < 1e-6,
            "{} is not on the offset grid",
            m.division
        );
        assert!((m.shift() + 0.01).abs() < 1e-9);
    }
}

#[test]
fn hits_never_quantize_past_each_other() {
    // A crossing would need the audio to run backwards.
    let times = fake(&[(0.24, 0.9), (0.26, 0.8)]);
    let p = plan(
        &times,
        QuantizeConfig {
            grid_secs: GRID,
            tolerance_secs: None,
            ..QuantizeConfig::default()
        },
    );
    for pair in p.moves.windows(2) {
        assert!(pair[1].to > pair[0].to, "moves crossed: {:?}", p.moves);
    }
}

// ── SPLIT mode ────────────────────────────────────────────────────────

#[test]
fn pieces_are_cut_before_the_transient_but_move_by_the_transient() {
    // The classic way to get this wrong: shifting by the padded cut
    // makes the audio arrive `pad` early and flams every hit against the
    // rest of the kit.
    let times = fake(&[(0.30, 0.9), (0.55, 0.9)]);
    let p = plan(
        &times,
        QuantizeConfig {
            grid_secs: GRID,
            ..QuantizeConfig::default()
        },
    );
    let cfg = SplitConfig {
        leading_pad_secs: 0.010,
        ..SplitConfig::default()
    };
    let pieces = p.splits(1.0, cfg);

    for piece in pieces.iter().filter(|p| p.transient.is_some()) {
        let t = piece.transient.unwrap();
        assert!(
            (t - piece.cut - 0.010).abs() < 1e-9,
            "cut should be 10 ms before the transient: {piece:?}"
        );
        // The move is the transient's move, not the cut's.
        let landed = t + piece.shift;
        assert!(
            (landed % GRID).min(GRID - landed % GRID) < 1e-6,
            "transient landed at {landed}, not on the grid"
        );
    }
}

#[test]
fn pieces_tile_the_take_from_zero_to_the_end() {
    let times = fake(&[(0.30, 0.9), (0.55, 0.9), (0.80, 0.9)]);
    let p = plan(
        &times,
        QuantizeConfig {
            grid_secs: GRID,
            ..QuantizeConfig::default()
        },
    );
    let pieces = p.splits(1.2, SplitConfig::default());

    assert_eq!(pieces.first().unwrap().cut, 0.0, "the lead-in is a piece");
    for pair in pieces.windows(2) {
        assert!(
            (pair[0].end - pair[1].cut).abs() < 1e-9,
            "pieces must meet: {pair:?}"
        );
    }
    assert!((pieces.last().unwrap().end - 1.2).abs() < 1e-9);
}

#[test]
fn the_lead_in_does_not_move() {
    // Nothing in it was detected, so nothing about it is late.
    let times = fake(&[(0.30, 0.9)]);
    let p = plan(
        &times,
        QuantizeConfig {
            grid_secs: GRID,
            ..QuantizeConfig::default()
        },
    );
    let pieces = p.splits(1.0, SplitConfig::default());
    let lead = pieces.first().unwrap();
    assert!(lead.transient.is_none());
    assert_eq!(lead.shift, 0.0);
}

// ── WARP mode ─────────────────────────────────────────────────────────

#[test]
fn the_warp_map_covers_the_take_and_never_goes_backwards() {
    let times = fake(&[(0.02, 0.9), (0.27, 0.9), (0.52, 0.9)]);
    let p = plan(
        &times,
        QuantizeConfig {
            grid_secs: GRID,
            ..QuantizeConfig::default()
        },
    );
    let a = p.alignment(400, 172.265625).expect("a map");
    assert_eq!(a.map.len(), 400);
    for pair in a.map.windows(2) {
        assert!(pair[1] >= pair[0], "map went backwards: {pair:?}");
    }
}

#[test]
fn the_warp_map_puts_each_transient_on_its_division() {
    let fps = 172.265625;
    let times = fake(&[(0.02, 0.9), (0.27, 0.9)]);
    let p = plan(
        &times,
        QuantizeConfig {
            grid_secs: GRID,
            ..QuantizeConfig::default()
        },
    );
    let a = p.alignment(400, fps).expect("a map");
    for m in &p.moves {
        let frame = (m.from * fps).round() as usize;
        let landed = a.map[frame.min(a.map.len() - 1)] / fps;
        assert!(
            (landed - m.to).abs() < 0.01,
            "transient at {} landed at {landed}s, wanted {}",
            m.from,
            m.to
        );
    }
}

#[test]
fn no_moves_means_no_map_rather_than_an_identity_one() {
    // An identity map would be written as stretch markers that do
    // nothing, which is worse than not writing: the item then looks
    // edited.
    let p = plan(&[], QuantizeConfig::default());
    assert!(p.alignment(400, 172.265625).is_none());
    assert!(p.splits(1.0, SplitConfig::default()).is_empty());
}

// ── Detection ─────────────────────────────────────────────────────────

#[test]
fn a_real_kit_take_quantizes_onto_the_grid() {
    // End to end: detect, plan, and check every hit ends up within a
    // couple of milliseconds of its division.
    let played: Vec<f64> = (0..8)
        .map(|i| i as f64 * GRID + if i % 2 == 0 { 0.018 } else { -0.012 })
        .collect();
    let audio = kit(&played, 2.4);
    let t = found(&audio, DetectConfig::default());
    assert!(t.len() >= 7, "found {} of 8 hits", t.len());

    let p = plan(
        &t,
        QuantizeConfig {
            grid_secs: GRID,
            tolerance_secs: Some(0.05),
            ..QuantizeConfig::default()
        },
    );
    assert!(p.moves.len() >= 7);
    for m in &p.moves {
        assert!(
            (m.to - m.division).abs() < 1e-6,
            "hit from {} landed at {} not {}",
            m.from,
            m.to,
            m.division
        );
        // And each moved roughly the error it was played with.
        assert!(m.shift().abs() < 0.03, "moved {}s, too far", m.shift());
    }
}

#[test]
fn the_threshold_kills_bleed_without_touching_the_hits() {
    // A quiet hit 40 dB down — the shape of mic bleed.
    let mut audio = take(1.2);
    let mut seed = 0x9999u64;
    strike(&mut audio, 0.25, 0.8, 110.0, 0.10, &mut seed);
    strike(&mut audio, 0.50, 0.008, 110.0, 0.10, &mut seed);
    strike(&mut audio, 0.75, 0.8, 110.0, 0.10, &mut seed);

    let open = DetectConfig {
        gate: GateConfig {
            threshold_db: -80.0,
            ..GateConfig::default()
        },
        sensitivity: 1.0,
        ..DetectConfig::default()
    };
    let gated = DetectConfig {
        gate: GateConfig {
            threshold_db: -20.0,
            ..GateConfig::default()
        },
        ..open
    };
    let loud = found(&audio, gated);
    assert!(
        loud.iter().all(|t| (t.at - 0.50).abs() > 0.05),
        "the bleed survived the gate: {:?}",
        loud.iter().map(|t| t.at).collect::<Vec<_>>()
    );
    assert!(loud.len() >= 2, "the real hits must survive");
}

#[test]
fn crest_separates_a_struck_hit_from_a_swell_at_the_same_level() {
    // The case no threshold can answer: both are loud, only one is
    // struck.
    let mut audio = take(1.5);
    let mut seed = 0x4242u64;
    strike(&mut audio, 0.25, 0.8, 110.0, 0.10, &mut seed);
    swell(&mut audio, 0.60, 0.8, 0.35);
    strike(&mut audio, 1.10, 0.8, 110.0, 0.10, &mut seed);

    // A realistic threshold, near the material's noise floor. That is
    // not incidental: the crest test only means anything above the
    // threshold, because out of digital silence the slow envelope is
    // zero and the ratio degenerates into "how far above the threshold
    // are we". Left at -60 dB on a clean recording, every fade-in in
    // the take trips a high crest setting.
    let permissive = DetectConfig {
        sensitivity: 1.0,
        gate: GateConfig {
            crest_db: 0.0,
            threshold_db: -30.0,
            ..GateConfig::default()
        },
        ..DetectConfig::default()
    };
    let strict = DetectConfig {
        gate: GateConfig {
            crest_db: 9.0,
            ..permissive.gate
        },
        ..permissive
    };

    let all = found(&audio, permissive);
    let struck = found(&audio, strict);
    assert!(
        struck.len() < all.len(),
        "crest should reject something: {all:?}"
    );
    assert!(
        struck.iter().all(|t| (t.at - 0.60).abs() > 0.1),
        "the swell survived the crest gate"
    );
}

#[test]
fn sensitivity_admits_softer_hits_as_it_rises() {
    let mut audio = take(1.5);
    let mut seed = 0x1234u64;
    strike(&mut audio, 0.25, 0.9, 110.0, 0.10, &mut seed);
    strike(&mut audio, 0.60, 0.25, 110.0, 0.10, &mut seed);
    strike(&mut audio, 0.95, 0.9, 110.0, 0.10, &mut seed);

    let at = |sensitivity: f64| {
        found(
            &audio,
            DetectConfig {
                sensitivity,
                gate: GateConfig {
                    threshold_db: -80.0,
                    ..GateConfig::default()
                },
                ..DetectConfig::default()
            },
        )
        .len()
    };
    assert!(
        at(1.0) >= at(0.2),
        "higher sensitivity must not find fewer hits"
    );
    assert!(at(0.2) >= 1, "the loud hits survive any sensitivity");
}

#[test]
fn a_time_offset_moves_every_transient_by_the_same_amount() {
    let audio = kit(&[0.25, 0.5, 0.75], 1.2);
    let plain = found(&audio, DetectConfig::default());
    let nudged = found(
        &audio,
        DetectConfig {
            time_offset_secs: -0.004,
            ..DetectConfig::default()
        },
    );
    assert_eq!(plain.len(), nudged.len());
    for (a, b) in plain.iter().zip(&nudged) {
        assert!((a.at - b.at - 0.004).abs() < 1e-9);
    }
}

#[test]
fn a_filter_isolates_a_kick_from_a_ringing_snare() {
    // The reason filters exist: in the full-band signal the snare is the
    // loudest thing and the kick's transient hides under it.
    let mut audio = take(1.5);
    let mut seed = 0x7777u64;
    // A long, loud snare ring across the whole take.
    for i in 0..3 {
        strike(
            &mut audio,
            0.1 + i as f64 * 0.5,
            0.9,
            900.0,
            0.45,
            &mut seed,
        );
    }
    // Quiet kicks on the eighths between them.
    for i in 0..3 {
        strike(
            &mut audio,
            0.35 + i as f64 * 0.5,
            0.35,
            60.0,
            0.12,
            &mut seed,
        );
    }

    let low = DetectConfig {
        filters: Filters {
            low_pass_hz: Some(200.0),
            high_pass_hz: None,
            gain: 4.0,
        },
        sensitivity: 0.8,
        gate: GateConfig {
            threshold_db: -80.0,
            ..GateConfig::default()
        },
        ..DetectConfig::default()
    };
    let kicks = found(&audio, low);
    // Every kick found, and each near where it was played.
    for i in 0..3 {
        let want = 0.35 + i as f64 * 0.5;
        assert!(
            kicks.iter().any(|t| (t.at - want).abs() < 0.03),
            "kick at {want}s missing from {:?}",
            kicks.iter().map(|t| t.at).collect::<Vec<_>>()
        );
    }
}

#[test]
fn filtering_does_not_change_what_is_written() {
    // The filtered signal exists only to be measured. If detection ever
    // returned filtered audio, a quantize would replace the take with a
    // band-limited copy of itself.
    let audio = kit(&[0.25, 0.5], 1.0);
    let before = audio.clone();
    let _ = found(
        &audio,
        DetectConfig {
            filters: Filters {
                high_pass_hz: Some(100.0),
                low_pass_hz: Some(4000.0),
                gain: 2.0,
            },
            ..DetectConfig::default()
        },
    );
    assert_eq!(audio, before);
}

#[test]
fn the_two_scales_rank_hits_differently() {
    // A sharp click and a soft, full thump. Peak favours the click; RMS
    // favours the thump. If both scales gave the same answer the control
    // would be decoration.
    let mut audio = take(1.5);
    let mut seed = 0xabcdu64;
    // Bright and quick, like a hat.
    strike(&mut audio, 0.25, 0.9, 2000.0, 0.05, &mut seed);
    // Lower and full, like a kick — quieter at its peak but with far
    // more energy behind it.
    strike(&mut audio, 0.75, 0.5, 90.0, 0.30, &mut seed);

    let of = |scale| {
        found(
            &audio,
            DetectConfig {
                scale,
                normalize: true,
                sensitivity: 1.0,
                gate: GateConfig {
                    threshold_db: -80.0,
                    ..GateConfig::default()
                },
                ..DetectConfig::default()
            },
        )
    };
    let peak = of(Scale::Peak);
    let rms = of(Scale::Rms);
    let loudness_at = |ts: &[Transient], at: f64| {
        ts.iter()
            .find(|t| (t.at - at).abs() < 0.05)
            .map(|t| t.loudness)
    };

    let (pc, pt) = (loudness_at(&peak, 0.25), loudness_at(&peak, 0.75));
    let (rc, rt) = (loudness_at(&rms, 0.25), loudness_at(&rms, 0.75));
    if let (Some(pc), Some(pt), Some(rc), Some(rt)) = (pc, pt, rc, rt) {
        assert!(pc > pt, "peak should favour the click: {pc} vs {pt}");
        assert!(
            rt / rc > pt / pc,
            "RMS should favour the thump relatively more"
        );
    } else {
        panic!("both hits should be found on both scales");
    }
}

#[test]
fn transients_are_located_to_a_couple_of_milliseconds() {
    // The accuracy the whole feature rests on. The flux detector alone
    // is only good to one hop — 5.8 ms at the defaults — which is both
    // audible on a kit and the same order as the timing error being
    // corrected, so quantizing with it would replace one jitter with
    // another. Attack refinement is what closes that gap.
    let played = [0.137, 0.409, 0.688, 0.951];
    let audio = kit(&played, 1.4);
    let found = found(&audio, DetectConfig::default());
    assert_eq!(found.len(), played.len(), "found {found:?}");

    let hop_secs = 256.0 / SR;
    for (t, want) in found.iter().zip(&played) {
        let error = (t.at - want).abs();
        assert!(
            error < 0.003,
            "hit at {want}s located at {}s — {error}s out, which is {:.1} hops",
            t.at,
            error / hop_secs
        );
    }
}
