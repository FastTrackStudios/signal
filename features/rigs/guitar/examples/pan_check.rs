//! How a patch sits between the sides: render it offline on the DI
//! reference (as `patch_bench` does) and print its left and right levels.
//!
//!   cargo run --release -p signal-guitar --example pan_check -- "Dry Chorus Clean L" [--secs 4]

use std::sync::Arc;

use signal_guitar::library::RigLibrary;
use signal_sampler::rig::GuitarRig;
use signal_sampler::rig_profile::{ProfileRig, RigProfile};

fn main() {
    let mut args = std::env::args().skip(1);
    let name = args.next().expect("a patch name");
    let secs: f64 = match (args.next().as_deref(), args.next()) {
        (Some("--secs"), Some(v)) => v.parse().unwrap_or(4.0),
        _ => 4.0,
    };
    let rate = 48_000u32;
    signal_guitar::levelling::apply_nam_calibration();
    let lib = RigLibrary::load_or_bootstrap();
    let def = &lib.profile;
    let mut built = signal_guitar::nodes::profile_from_library(def, &lib.drive_presets);
    let comp = RigLibrary::load_compositions();
    for patch in &mut built.patches {
        if let Some(d) = def.patches.iter().find(|d| d.name.eq_ignore_ascii_case(&patch.name)) {
            signal_guitar::macros::apply_positions(d, &comp, patch);
        }
    }
    let Some(patch) = built.patches.iter().find(|p| p.name.eq_ignore_ascii_case(&name)).cloned() else {
        eprintln!("no patch {name:?}");
        std::process::exit(2);
    };
    let order: Vec<&str> = patch.chain.iter().filter(|b| b.has_backend()).map(|b| b.name.as_str()).collect();
    println!("chain: {}", order.join(" → "));
    let mut prig = ProfileRig::new(GuitarRig::open_offline(rate).expect("offline rig"));
    prig.set_level_match(false);
    let mut profile = RigProfile::new("check");
    profile.patches = vec![patch];
    prig.load_profile(profile, None).expect("builds");
    prig.activate(0);
    signal_guitar::measure::apply_chain_bypass(&prig);
    let di = signal_sampler::nam_calibrate::DiReference::load_or_synthetic(f64::from(rate));
    prig.rig().start_test_signal(Arc::new(di.samples.iter().map(|&s| s as f32).collect()));
    prig.rig().render_offline(rate as usize / 2);
    let frames = (secs * f64::from(rate)) as usize;
    prig.rig().arm_heard_capture(frames);
    prig.rig().render_offline(frames);
    let (l, r) = prig.rig().take_output_capture();
    let rms = |x: &[f32]| (x.iter().map(|s| s * s).sum::<f32>() / x.len().max(1) as f32).sqrt();
    let db = |x: f32| 20.0 * x.max(1e-9).log10();
    let (lo, ro) = (rms(&l), rms(&r));
    println!("left {:+.1} dBFS · right {:+.1} dBFS · right is {:.1} dB under the left", db(lo), db(ro), db(lo) - db(ro));
}
