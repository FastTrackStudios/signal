//! Does a block stay bounded? Drives one delay/reverb setup with 10 s of
//! noise-burst-and-silence and prints the peak per second. A block whose
//! peak grows after the input stops is running away.
//!
//! `cargo run --release -p fx-blocks --example stability_probe -- <case>`

use daw::plugin::{PluginEvents, PluginInstance};
use fx_blocks::{NativeDelay, NativeReverb};

const SR: f64 = 48_000.0;
const BLOCK: usize = 512;

fn delay(params: &[(&str, f64)]) -> Box<dyn PluginInstance> {
    let mut d = NativeDelay::new(SR);
    d.set_named("tempo_bpm", 139.0);
    for (n, v) in params {
        d.set_named(n, *v);
    }
    Box::new(d)
}

fn reverb(params: &[(&str, f64)]) -> Box<dyn PluginInstance> {
    let mut r = NativeReverb::new(SR);
    for (n, v) in params {
        r.set_named(n, *v);
    }
    Box::new(r)
}

fn main() {
    let case = std::env::args().nth(1).unwrap_or_default();
    let flute_echo: &[(&str, f64)] = &[("mix", 1.0), ("style", 6.0), ("tap_div_l", 1.0), ("tap_div_r", 1.0), ("time", 340.0),
        ("feedback", 0.55), ("level", -3.0), ("high_pass", 300.0), ("high_cut", 6000.0), ("interval", 27.0),
        ("slice", 2.0), ("blend", 1.0), ("mod_depth", 0.2), ("dry", 0.25)];
    let flute_wash: &[(&str, f64)] = &[("mix", 1.0), ("style", 0.0), ("tap_div_l", 8.0), ("tap_div_r", 8.0), ("time", 680.0),
        ("feedback", 0.7), ("level", -5.0), ("high_pass", 300.0), ("high_cut", 3500.0), ("mod_depth", 0.35)];
    let mut fx: Box<dyn PluginInstance> = match case.as_str() {
        "echo" => delay(flute_echo),
        "wash" => delay(flute_wash),
        "heaven" => reverb(&[("mix", 1.0), ("algorithm", 6.0), ("decay", 0.9), ("size", 1.0), ("level", -8.0),
            ("shim_amount", 0.5), ("shim_shift1", 12.0), ("shim_shift2", 12.0), ("shim_voice2", 1.0)]),
        "cathedral" => reverb(&[("mix", 1.0), ("algorithm", 1.0), ("variant", 1.0), ("decay", 0.8), ("size", 1.0), ("level", -6.0)]),
        c if c.starts_with("delay:") => {
            // delay:<style>:<feedback>
            let mut it = c.split(':').skip(1).map(|x| x.parse::<f64>().unwrap_or(0.0));
            let (style, fb) = (it.next().unwrap_or(0.0), it.next().unwrap_or(0.5));
            delay(&[("style", style), ("time", 250.0), ("feedback", fb), ("mix", 1.0), ("dry", 0.0)])
        }
        _ => {
            eprintln!("case: echo | wash | heaven | cathedral | delay:<style>:<fb>");
            return;
        }
    };
    fx.prepare(SR, BLOCK as u32).unwrap();
    let mut seed = 3u32;
    let per_sec = SR as usize / BLOCK;
    for sec in 0..12 {
        let mut peak = 0.0f32;
        for b in 0..per_sec {
            let x: Vec<f32> = (0..BLOCK)
                .map(|_| {
                    if sec == 0 && b < 5 {
                        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                        ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.5
                    } else {
                        0.0
                    }
                })
                .collect();
            let (mut l, mut r) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
            fx.process_block(&x, &x, &mut l, &mut r, &PluginEvents::EMPTY).unwrap();
            peak = l.iter().chain(&r).fold(peak, |m, s| if s.is_finite() { m.max(s.abs()) } else { f32::INFINITY });
        }
        println!("{case} {sec:>2}s  peak {peak:.4}");
    }
}
