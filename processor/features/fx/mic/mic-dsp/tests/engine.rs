//! The engine against hand-built models: the measured laws the reference
//! follows (axis is a linear crossfade of anchors, rear trim acts on the
//! input's rear channel before the swap, proximity touches only the
//! gradient path, the curve sits after the linear response), checked on
//! data small enough to write here — the real models are measurements of
//! a commercial product and are not in the repository.

use mic_dsp::model::{AXES, PATTERNS};
use mic_dsp::{LATENCY, MicChain, MicModel, ModelError, Settings};

const TAPS: usize = 128;

/// A model whose anchor (pattern, axis) kernels are distinct short pulses:
/// P = δ[LATENCY]·(1 + axis) , G = δ[LATENCY + 3]·(0.5 + pattern/10).
fn model(curve: Option<[f64; 2]>) -> Result<MicModel, ModelError> {
    let mut b = Vec::new();
    b.extend_from_slice(b"MICM");
    for v in [6u32, 48_000, u32::try_from(TAPS).unwrap_or(0), 9, 5] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    for v in [10.0f64, 60.0, 100.0, 200.0] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    for _ in 0..PATTERNS {
        b.extend_from_slice(&100.0f64.to_le_bytes());
    }
    b.extend_from_slice(&0u32.to_le_bytes()); // corner law
    for v in [20.0f64, 500.0, 0.0, 0.0] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    for k in 0..4 {
        match (k, curve) {
            (0, Some([a2, a3])) => {
                b.extend_from_slice(&1u32.to_le_bytes());
                b.extend_from_slice(&2u32.to_le_bytes());
                b.extend_from_slice(&a2.to_le_bytes());
                b.extend_from_slice(&a3.to_le_bytes());
                b.extend_from_slice(&0u32.to_le_bytes()); // no sections after it
            }
            _ => b.extend_from_slice(&0u32.to_le_bytes()),
        }
    }
    b.extend_from_slice(&1u32.to_le_bytes()); // only the off set
    // stored whole: a head the full length, no tail modes
    b.extend_from_slice(&u32::try_from(TAPS).unwrap_or(0).to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes()); // no modes
    b.extend_from_slice(&0u32.to_le_bytes()); // no Laguerre functions
    let pulse = |at: usize, height: f32| (0..TAPS).map(move |i| if i == at { height } else { 0.0 });
    for p in 0..u8::try_from(PATTERNS).unwrap_or(0) {
        for a in 0..u8::try_from(AXES).unwrap_or(0) {
            let pk = pulse(LATENCY, 1.0 + f32::from(a));
            let gk = pulse(LATENCY.saturating_add(3), 0.5 + f32::from(p) / 10.0);
            for v in pk.chain(gk) {
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
    }
    // delay, shelf k, dual weights
    for v in core::iter::once(0.146_03).chain([1.0f64; 2 * PATTERNS]) {
        b.extend_from_slice(&v.to_le_bytes());
    }
    // low ends: the built-in corner moved by the switch; pressure blocks: none
    let block = |b: &mut Vec<u8>, hp: Option<f64>| {
        b.extend_from_slice(&1.0f64.to_le_bytes());
        b.extend_from_slice(&u32::from(hp.is_some()).to_le_bytes());
        if let Some(hz) = hp {
            b.extend_from_slice(&0u32.to_le_bytes());
            for v in [hz, 0.0, 0.0] {
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
    };
    for hz in [10.0, 60.0, 100.0, 200.0] {
        block(&mut b, Some(hz));
    }
    for _ in 0..PATTERNS {
        block(&mut b, None);
    }
    b.extend_from_slice(&0u32.to_le_bytes()); // no dual gradient block
    MicModel::from_bytes(&b)
}

/// A chain on a synthetic model; a model that fails to parse fails the test.
fn chain(m: Result<MicModel, ModelError>) -> Option<MicChain> {
    assert!(m.is_ok(), "synthetic model: {:?}", m.as_ref().err());
    m.ok().map(MicChain::new)
}

/// Response of a chain to an impulse on (front, rear), latency removed.
fn response(chain: &mut MicChain, front: f64, rear: f64, n: usize) -> Vec<f64> {
    chain.reset();
    (0..n.saturating_add(LATENCY))
        .map(|i| if i == 0 { chain.process(front, rear) } else { chain.process(0.0, 0.0) })
        .skip(LATENCY)
        .collect()
}

/// Sample `i` of a response (0 past the end).
fn at(y: &[f64], i: usize) -> f64 {
    y.get(i).copied().unwrap_or(0.0)
}

#[test]
fn neutral_settings_reproduce_the_anchor() {
    let Some(mut c) = chain(model(None)) else { return };
    c.apply(Settings { pattern: 2, ..Settings::default() }, true);
    let y = response(&mut c, 1.0, 0.0, 8);
    // front impulse: P(axis 0) = 1 at 0, G(pattern 2) = 0.7 at 3
    assert!((at(&y, 0) - 1.0).abs() < 1e-6, "{y:?}");
    assert!((at(&y, 3) - 0.7).abs() < 1e-6, "{y:?}");
    let y = response(&mut c, 0.0, 1.0, 8);
    // rear impulse: P − G
    assert!((at(&y, 0) - 1.0).abs() < 1e-6 && (at(&y, 3) + 0.7).abs() < 1e-6, "{y:?}");
}

#[test]
fn axis_is_a_linear_crossfade_of_the_anchors() {
    let Some(mut c) = chain(model(None)) else { return };
    // 67.5° is halfway between the 45° (P = 2) and 90° (P = 3) anchors.
    c.apply(Settings { axis_deg: 67.5, ..Settings::default() }, true);
    let y = response(&mut c, 1.0, 0.0, 4);
    assert!((at(&y, 0) - 2.5).abs() < 1e-6, "{y:?}");
}

#[test]
fn rear_trim_scales_the_rear_input_before_the_swap() {
    let Some(mut c) = chain(model(None)) else { return };
    c.apply(Settings { rear_trim_db: 6.0, swap: true, ..Settings::default() }, true);
    let g = 10f64.powf(6.0 / 20.0);
    // the input's rear channel, trimmed, becomes the front capsule
    let y = response(&mut c, 0.0, 1.0, 8);
    assert!((at(&y, 0) - g).abs() < 1e-6 && 0.9f64.mul_add(-g, at(&y, 3)).abs() < 1e-6, "{y:?}");
}

#[test]
fn proximity_leaves_the_pressure_path_alone() {
    let Some(mut c) = chain(model(None)) else { return };
    // Omni-like input (front = rear) has no gradient part at all.
    c.apply(Settings { proximity: 80.0, ..Settings::default() }, true);
    let near = response(&mut c, 1.0, 1.0, 64);
    c.apply(Settings { proximity: 0.0, ..Settings::default() }, true);
    let flat = response(&mut c, 1.0, 1.0, 64);
    let diff = near.iter().zip(&flat).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
    assert!(diff < 1e-12, "max diff {diff}");
}

#[test]
fn phase_and_output_scale_everything() {
    let Some(mut c) = chain(model(None)) else { return };
    c.apply(Settings { phase_invert: true, output_db: -6.0, ..Settings::default() }, true);
    let y = response(&mut c, 1.0, 0.0, 4);
    assert!((at(&y, 0) + 10f64.powf(-6.0 / 20.0)).abs() < 1e-6, "{y:?}");
}

#[test]
fn the_curve_adds_harmonics_and_nothing_at_small_levels() {
    let Some(mut c) = chain(model(Some([0.01, 0.0]))) else { return };
    c.apply(Settings::default(), true);
    // a2 = 0.01 on s = P∗front (P = δ): a 0.5 sample gives 0.5 + 0.01·0.25
    let y = response(&mut c, 0.5, 0.0, 4);
    assert!((at(&y, 0) - (0.5 + 0.0025)).abs() < 1e-6, "{y:?}");
    let y = response(&mut c, 1e-6, 0.0, 4);
    assert!((at(&y, 0) - 1e-6).abs() < 1e-12, "{y:?}");
}
