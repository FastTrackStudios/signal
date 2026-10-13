//! Measured applied-gain parity with the captured loud/quiet stimulus.
//! Fit uses PR 2/7, 3/7, 5/7, 1; PR 4/7 and 6/7 test interpolation.
use comp_dsp::la2a::La2a;

#[test]
fn captured_release_including_makeup_and_residual_compression() {
    let rows: Vec<(f64, usize, f64)> = include_str!("fixtures/la2a_release.csv")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            let values: Vec<f64> = line.split(',').map(|x| x.parse().unwrap()).collect();
            (values[0], (5000.0 + values[1]) as usize, values[2])
        })
        .collect();
    for rate in [48_000, 96_000] {
        let frames = rate / 1000;
        for group in rows.chunk_by(|a, b| a.0 == b.0) {
            let pr = group[0].0;
            let mut model = La2a::new(f64::from(rate));
            model.set_peak_reduction(pr);
            model.set_gain(0.2862548828125);
            let mut wanted = group.iter().peekable();
            let mut total = 0.0;
            let mut worst = 0.0f64;
            let mut count = 0;
            for ms in 0..9000 {
                let amplitude = 10.0f64.powf(if ms % 4500 < 500 {
                    -6.0 / 20.0
                } else {
                    -20.0 / 20.0
                });
                let mut input_power = 0.0;
                let mut output_power = 0.0;
                for n in 0..frames {
                    let input = (std::f64::consts::TAU * f64::from(n) / f64::from(frames)).sin()
                        * amplitude;
                    let output = model.process(input);
                    input_power += input * input;
                    output_power += output * output;
                }
                if wanted.peek().is_some_and(|row| row.1 == ms) {
                    let reference = wanted.next().unwrap().2;
                    let error = (10.0 * (output_power / input_power).log10() - reference).abs();
                    total += error;
                    worst = worst.max(error);
                    count += 1;
                }
            }
            assert_eq!(count, group.len());
            let mean = total / count as f64;
            assert!(
                mean < 0.70 && worst < 1.05,
                "PR={pr} SR={rate}: mean={mean:.3}, worst={worst:.3} dB"
            );
        }
    }
}
