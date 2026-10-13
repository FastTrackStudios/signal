//! Dividing an impulse response by a section undoes running it through
//! that section — what dual mode does to take a block out of a kernel.

use mic_dsp::section::Biquad;

/// A decaying pseudo-random response.
fn response(len: usize) -> Vec<f64> {
    let mut state = 0x2545_f491_u32;
    (0..len)
        .map(|n| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let x = f64::from(state) / f64::from(u32::MAX) - 0.5;
            x * (-dsp_core::count_to_f64(n) / 400.0).exp()
        })
        .collect()
}

fn worst_after_round_trip(section: Biquad) -> f64 {
    let original = response(32_768);
    let mut run = section;
    let mut h: Vec<f64> = original.iter().map(|&x| run.process(x)).collect();
    section.divide(&mut h);
    let peak = original.iter().fold(0.0f64, |m, x| m.max(x.abs()));
    original.iter().zip(&h).fold(0.0f64, |m, (a, b)| m.max((a - b).abs())) / peak
}

#[test]
fn division_undoes_each_section_shape() {
    let sr = 48_000.0;
    for (name, section) in [
        ("high-pass", Biquad::high_pass(30.0, sr)),
        ("second-order high-pass", Biquad::high_pass2(75.0, 0.71, sr)),
        ("shelf", Biquad::shelf(50.6, 160.0, sr)),
        ("peak", Biquad::peak_analog(146.0, 1.2, 1.25, sr)),
    ] {
        let worst = worst_after_round_trip(section);
        assert!(worst < 1e-6, "{name}: worst error {worst:e}");
    }
}
