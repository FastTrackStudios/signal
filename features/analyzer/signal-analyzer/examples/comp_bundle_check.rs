//! Do the two compressors pass audio at all, and at what level?
//!
//! The converter's verification reported an infinite difference on every
//! Pro-C instance, which is what a transfer function says when one side is
//! silent — a number that tells you something is wrong and nothing about
//! which side or why. This asks the smaller question first: feed both
//! plugins the same noise and print what comes out.
//!
//! ```sh
//! cargo run --release -p signal-analyzer --example comp_bundle_check -- \
//!     --state <state.json> [--reference ~/.clap/yabridge/"FabFilter Pro-C 3.clap"]
//! ```

use signal_analyzer::eq_transfer::{self, Stimulus};
use signal_plugin_host::HostedPlugin;

const SR: f64 = 48_000.0;
const BLOCK: usize = 512;

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter()
        .position(|x| x == name)
        .and_then(|i| a.get(i + 1).cloned())
}

fn home(rest: &str) -> String {
    std::env::var("HOME").map_or_else(|_| rest.into(), |h| format!("{h}/{rest}"))
}

fn open(path: &str) -> Option<HostedPlugin> {
    match HostedPlugin::load(path) {
        Ok(Some(mut p)) => {
            p.prepare(SR, BLOCK as u32).ok()?;
            Some(p)
        }
        _ => {
            eprintln!("{path}: could not load");
            None
        }
    }
}

fn render(p: &mut HostedPlugin, l: &[f32], r: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let (mut ol, mut or) = (Vec::with_capacity(l.len()), Vec::with_capacity(l.len()));
    let mut pos = 0;
    while pos < l.len() {
        let n = BLOCK.min(l.len() - pos);
        let mut buf = vec![0.0f32; n * 2];
        for i in 0..n {
            buf[2 * i] = l[pos + i];
            buf[2 * i + 1] = r[pos + i];
        }
        if p.process_interleaved(&mut buf, &[], &[]).is_err() {
            break;
        }
        ol.extend((0..n).map(|i| buf[2 * i]));
        or.extend((0..n).map(|i| buf[2 * i + 1]));
        pos += n;
    }
    (ol, or)
}

fn rms_db(v: &[f32]) -> f64 {
    if v.is_empty() {
        return f64::NEG_INFINITY;
    }
    // Skip the warmup, as the transfer measurement does.
    let start = (eq_transfer::WARMUP_FRAMES * eq_transfer::FFT / 2).min(v.len() / 2);
    let tail = &v[start..];
    let e: f64 = tail.iter().map(|x| f64::from(*x) * f64::from(*x)).sum();
    10.0 * (e / tail.len().max(1) as f64).log10()
}

fn main() {
    let level = arg("--level")
        .and_then(|v| v.parse().ok())
        .unwrap_or(-18.8f64);
    let (l, r) = eq_transfer::stimulus(
        eq_transfer::frames_needed(),
        10.0f64.powf(level / 20.0),
        Stimulus::Flat,
        SR,
    );
    println!("stimulus  {:>8.2} dB rms", rms_db(&l));

    let ours_path = arg("--ours").unwrap_or_else(|| home(".clap/FTS Comp.clap"));
    if let Some(mut ours) = open(&ours_path) {
        match arg("--state") {
            None => println!("no --state; measuring the plugin's own default"),
            Some(path) => {
                let json = std::fs::read(&path).expect("read state json");
                // nih-plug's framing: a little-endian u64 length, then JSON.
                let mut blob = (json.len() as u64).to_le_bytes().to_vec();
                blob.extend_from_slice(&json);
                match ours.load_state(&blob) {
                    Ok(()) => println!("loaded {path}"),
                    Err(e) => println!("load_state refused it: {e:?}"),
                }
            }
        }
        let out = render(&mut ours, &l, &r);
        println!(
            "FTS Comp  {:>8.2} dB rms  ({} frames)",
            rms_db(&out.0),
            out.0.len()
        );
    }

    if let Some(path) = arg("--reference") {
        if let Some(mut reference) = open(&path) {
            let out = render(&mut reference, &l, &r);
            println!(
                "reference {:>8.2} dB rms  ({} frames)",
                rms_db(&out.0),
                out.0.len()
            );
        }
    }
}
