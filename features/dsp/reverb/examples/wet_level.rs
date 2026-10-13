//! Each engine's wet level at the shared reference — the energy of a unit
//! impulse's response asked for a 2 s tail through `set_decay_seconds`, the
//! measure `tests/stability.rs::algorithms_share_one_output_level` holds them
//! to — and the `wet_calibration_db` that puts it at unity.
//!
//! Re-derive after an engine's level or its time conversion changes:
//! `cargo run --release -p reverb-dsp --example wet_level`, and copy the
//! printed constants into `AlgorithmType::wet_calibration_db`.

use audiocore_dsp::Processor as _;
use reverb::AlgorithmType;
use reverb::chain::ReverbChain;

const SR: f64 = 48_000.0;
const REFERENCE_T60: f64 = 2.0;

fn energy(algo: AlgorithmType) -> f64 {
    let config = audiocore_dsp::AudioConfig {
        sample_rate: SR,
        max_buffer_size: 512,
    };
    let mut c = ReverbChain::new();
    c.set_algorithm(algo);
    c.mix = 1.0;
    c.update(config);
    if c.decay_seconds_range().is_some() {
        c.set_decay_seconds(REFERENCE_T60);
        c.update(config);
    }
    let frames = (SR * REFERENCE_T60 * 3.0) as usize;
    let mut total = 0.0;
    let mut pos = 0;
    while pos < frames {
        let n = 512.min(frames - pos);
        let mut l = vec![0.0_f64; n];
        let mut r = vec![0.0_f64; n];
        if pos == 0 {
            l[0] = 1.0;
            r[0] = 1.0;
        }
        c.process(&mut l, &mut r);
        total += l.iter().map(|s| s * s).sum::<f64>();
        pos += n;
    }
    total
}

fn main() {
    for &algo in AlgorithmType::ALL {
        let error_db = 10.0 * energy(algo).log10();
        let new = algo.wet_calibration_db() - error_db;
        println!("{algo:?}\t{:+.2} dB off\tnow {:.2}\t→ {new:.2}", error_db, algo.wet_calibration_db());
    }
}
