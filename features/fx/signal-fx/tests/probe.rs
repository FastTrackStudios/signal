use signal_fx::NativeEq;
use signal_plugin_host::{PluginEvents, PluginInstance};
const SR: f64 = 48_000.0;
const BLOCK: usize = 512;

fn tone(f: f64, n: usize) -> Vec<f32> {
    let inc = std::f64::consts::TAU * f / SR;
    (0..n).map(|i| (0.3 * (inc * i as f64).sin()) as f32).collect()
}
fn rms(b: &[f32]) -> f64 {
    (b.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / b.len() as f64).sqrt()
}
fn resp(bands: &[&[(&str, f64)]], f: f64) -> f64 {
    let mut eq = NativeEq::new(SR);
    for b in bands { for (n, v) in *b { eq.set_named(n, *v); } }
    eq.prepare(SR, BLOCK as u32).unwrap();
    let input = tone(f, 24000);
    let ev = PluginEvents::default();
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < input.len() {
        let n = BLOCK.min(input.len() - pos);
        let l = &input[pos..pos+n];
        let (mut ol, mut or) = (vec![0.0f32; n], vec![0.0f32; n]);
        eq.process_block(l, l, &mut ol, &mut or, &ev).unwrap();
        out.extend_from_slice(&ol); pos += n;
    }
    let tail = &out[out.len()/2..];
    20.0 * (rms(tail) / rms(&input[input.len()/2..])).log10()
}

#[test]
fn probe() {
    let b1: &[(&str, f64)] = &[("b1_used",1.0),("b1_on",1.0),("b1_freq",6499.998),("b1_gain",1.0),("b1_q",1.0),("b1_shape",0.0),("b1_slope",2.0),("b1_placement",0.0)];
    let b2: &[(&str, f64)] = &[("b2_used",1.0),("b2_on",1.0),("b2_freq",9999.998),("b2_gain",3.0),("b2_q",0.3),("b2_shape",3.0),("b2_slope",2.0),("b2_placement",0.0)];
    let b3: &[(&str, f64)] = &[("b3_used",1.0),("b3_on",1.0),("b3_freq",60.0),("b3_gain",-3.0),("b3_q",0.3),("b3_shape",1.0),("b3_slope",2.0),("b3_placement",0.0)];
    let b4: &[(&str, f64)] = &[("b4_used",1.0),("b4_on",1.0),("b4_freq",30.0),("b4_gain",0.0),("b4_q",1.0),("b4_shape",2.0),("b4_slope",2.0),("b4_placement",0.0)];
    let _ = (b1, b3, b4);
    eprintln!("high shelf, 10 kHz, +3 dB — response at 1 kHz (should be ~0)");
    for q in [0.1f64, 0.2, 0.3, 0.5, 0.707, 1.0, 2.0, 4.0] {
        let mut line = format!("  Q {q:>5.3}");
        for slope in [1.0f64, 2.0, 3.0, 4.0] {
            let b: Vec<(&str, f64)> = vec![("b2_used",1.0),("b2_on",1.0),
                ("b2_freq",10000.0),("b2_gain",3.0),("b2_q",q),("b2_shape",3.0),
                ("b2_slope",slope)];
            line.push_str(&format!("  slope{slope:.0}:{:>8.2}", resp(&[&b], 1000.0)));
        }
        eprintln!("{line}");
    }
}
