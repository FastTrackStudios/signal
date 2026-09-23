//! Every patch of a profile, rendered with two inputs: the DI reference the
//! levelling uses, and a recording of real playing — to see whether the
//! patches keep their balance when the guitar is played the way it is.
//!
//!   cargo run --profile release-fast -p signal-guitar --example patch_compare -- \
//!     <Profile> <playing.wav>
//!
//! A clean amp follows its input dB for dB; a driven one compresses it. So a
//! set levelled on one input can drift apart on another — this measures by
//! how much. Nothing is written.

use std::sync::Arc;

use signal_guitar::library::RigLibrary;
use signal_guitar::measure::apply_chain_bypass;
use signal_sampler::nam_calibrate::DiReference;
use signal_sampler::rig::GuitarRig;
use signal_sampler::rig_profile::{ProfileRig, RigPatch, RigProfile};

const SR: u32 = 48_000;

fn render(patch: &RigPatch, input: &[f32]) -> Option<f64> {
    let rig = GuitarRig::open_offline(SR).ok()?;
    let mut prig = ProfileRig::new(rig);
    prig.set_level_match(false);
    let mut profile = RigProfile::new("compare");
    profile.patches.push(patch.clone());
    prig.load_profile(profile, None).ok()?;
    if !prig.activate(0) {
        return None;
    }
    apply_chain_bypass(&prig);
    let rig = prig.rig();
    let samples = Arc::new(input.to_vec());
    rig.start_test_signal(samples.clone());
    rig.render_offline(SR as usize);
    rig.start_test_signal(samples);
    let (l, r) = rig.measure_output(input.len(), std::time::Duration::ZERO);
    let mono: Vec<f64> = l.iter().zip(&r).map(|(a, b)| f64::from(a + b) * 0.5).collect();
    Some(signal_sampler::loudness::integrated_lufs(&mono, f64::from(SR)))
}

fn main() {
    tracing_subscriber::fmt().with_env_filter("warn").init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(name), Some(wav)) = (args.first(), args.get(1)) else {
        eprintln!("usage: patch_compare <Profile> <playing.wav>");
        std::process::exit(2);
    };
    signal_guitar::levelling::apply_nam_calibration();
    let lib = RigLibrary::load_or_bootstrap();
    let Some(def) = lib.profiles.iter().find(|p| p.name.eq_ignore_ascii_case(name)) else {
        eprintln!("no profile {name}");
        std::process::exit(2);
    };
    let built = signal_guitar::nodes::profile_from_library(def, &lib.drive_presets);
    let f32s = |d: DiReference| d.samples.iter().map(|&s| s as f32).collect::<Vec<f32>>();
    let di = f32s(DiReference::load_or_synthetic(f64::from(SR)));
    let playing = match DiReference::load(std::path::Path::new(wav), f64::from(SR)) {
        Ok(d) => f32s(d),
        Err(e) => {
            eprintln!("{wav}: {e}");
            std::process::exit(2);
        }
    };
    let rows = signal_guitar::levelling::par_map(&built.patches, 0, |p| {
        (render(p, &di), render(p, &playing))
    });
    let clean = rows.first().and_then(|r| r.0.zip(r.1));
    println!("{:<16} {:>10} {:>10}   vs first patch (ref / playing)", "patch", "DI ref", "playing");
    for (p, (a, b)) in built.patches.iter().zip(&rows) {
        let rel = match (clean, a, b) {
            (Some((c0, c1)), Some(a), Some(b)) => format!("{:+5.1} / {:+5.1}", a - c0, b - c1),
            _ => String::new(),
        };
        println!(
            "{:<16} {:>10} {:>10}   {rel}",
            p.name,
            a.map_or("—".into(), |v| format!("{v:.1}")),
            b.map_or("—".into(), |v| format!("{v:.1}")),
        );
    }
}
