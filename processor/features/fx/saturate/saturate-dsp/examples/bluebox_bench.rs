//! Time the Bluebox engine: ns per stereo frame at 48 kHz (4× oversampled)
//! and 96 kHz (2×) for a few settings, on 10 s of loud noise.
//!
//! ```sh
//! cargo run --release -p saturate-dsp --example bluebox_bench
//! ```

use std::time::Instant;

use saturate_dsp::bluebox::{Bluebox, BlueboxParams};

fn main() {
    let mut seed = 1u32;
    let noise: Vec<f64> = (0..480_000)
        .map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (f64::from(seed) / f64::from(u32::MAX) - 0.5) * 1.5
        })
        .collect();
    let t = Instant::now();
    let fx = Bluebox::new(48_000.0);
    println!("construction (curve tables) {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
    drop(fx);
    let base = BlueboxParams::default();
    for sr in [48_000.0, 96_000.0] {
    for (name, p) in [
        ("defaults", base),
        ("saturation in", BlueboxParams { saturation_in: true, saturation: 70.0, ..base }),
        ("sat + alt + air", BlueboxParams { saturation_in: true, alt_tube: true, air_in: true, ..base }),
    ] {
        let mut fx = Bluebox::new(sr);
        fx.set(p);
        let t = Instant::now();
        let mut acc = 0.0;
        for &x in &noise {
            let (l, r) = fx.process(x, -x);
            acc += l + r;
        }
        let ns = t.elapsed().as_secs_f64() * 1e9 / 480_000.0;
        println!("{:.0} kHz (OS {}x) {name:16} {ns:7.1} ns/frame  {:5.2} % of one core  (checksum {acc:.3e})", sr / 1e3, if sr < 50_000.0 { 4 } else { 2 }, ns * sr / 1e9 * 100.0);
    }
    }
}
