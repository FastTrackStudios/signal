//! Compare the model against extracted applied-gain traces with the real pulsing stimulus.
use comp_dsp::la2a::La2a;
fn main() {
    let fixture = include_str!("../tests/fixtures/la2a_release.csv");
    let rows: Vec<(f64, usize, f64)> = fixture
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            let values: Vec<f64> = line.split(',').map(|x| x.parse().unwrap()).collect();
            (values[0], (5000.0 + values[1]) as usize, values[2])
        })
        .collect();
    println!("Peak Reduction,mean error dB,worst error dB");
    for group in rows.chunk_by(|a, b| a.0 == b.0) {
        let pr = group[0].0;
        let mut unit = La2a::new(48000.0);
        unit.set_peak_reduction(pr);
        unit.set_gain(0.2862548828125);
        let mut sum = 0.0;
        let mut worst = 0.0f64;
        let mut count = 0;
        let mut wanted = group.iter().peekable();
        for ms in 0..9000 {
            let amp = 10.0f64.powf(if ms % 4500 < 500 {
                -6.0 / 20.0
            } else {
                -20.0 / 20.0
            });
            let mut input_energy = 0.0;
            let mut output_energy = 0.0;
            for n in 0..48 {
                let input = (std::f64::consts::TAU * n as f64 / 48.0).sin() * amp;
                let output = unit.process(input);
                input_energy += input * input;
                output_energy += output * output;
            }
            if wanted.peek().is_some_and(|row| row.1 == ms) {
                let row = wanted.next().unwrap();
                let actual = 10.0 * (output_energy / input_energy).log10();
                let error = (actual - row.2).abs();
                sum += error;
                worst = worst.max(error);
                count += 1;
            }
        }
        println!("{pr:.6},{:.4},{worst:.4}", sum / count as f64);
    }
}
