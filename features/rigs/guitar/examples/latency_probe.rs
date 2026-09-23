//! Prove the rig's DSP adds no latency: an impulse through every patch.
//!
//!   cargo run --profile release-fast -p signal-guitar --example latency_probe \
//!     [-- <Profile>] [--per-block]
//!
//! Each patch is built on [`GuitarRig::open_offline`] — the project, slots,
//! chain installer and renderer the live rig plays through — settled on
//! silence, then fed one impulse. The output tap is armed on the same block
//! the impulse starts, so output sample `i` is input sample `i`: a chain with
//! no latency answers at the impulse's own index.
//!
//! Reported per patch: `onset` (first output sample within 40 dB of the
//! response's peak, minus the impulse index) and `peak` (the peak's lag). A
//! cab IR's mic distance or a filter's group delay moves `peak` by a few
//! samples; a block that buffers moves `onset`, by its block size. Anything
//! with a nonzero onset is listed. `--per-block` then plays each block of
//! each such patch on its own to name the one responsible.
//!
//! Nothing is written: the library is only read.

use std::sync::Arc;

use signal_guitar::library::RigLibrary;
use signal_guitar::measure::apply_chain_bypass;
use signal_guitar::nodes::profile_from_library;
use signal_sampler::rig::GuitarRig;
use signal_sampler::rig_profile::{ProfileRig, RigPatch, RigProfile};

const SR: u32 = 48_000;
/// Where the impulse sits in the probe signal — far enough in that any
/// pre-ringing (a linear-phase filter) would show as a negative onset.
const AT: usize = 4_800;
const LEN: usize = SR as usize;
/// Loud enough to open the gate and drive the amps like a pick attack.
const AMP: f32 = 0.5;

struct Probe {
    onset: Option<i64>,
    peak: i64,
    peak_db: f32,
}

fn probe(patch: &RigPatch) -> Result<Probe, String> {
    let rig = GuitarRig::open_offline(SR).map_err(|e| e.to_string())?;
    let mut prig = ProfileRig::new(rig);
    prig.set_level_match(false);
    let mut profile = RigProfile::new("probe");
    profile.patches.push(patch.clone());
    prig.load_profile(profile, None)?;
    if !prig.activate(0) {
        return Err("patch did not activate".into());
    }
    apply_chain_bypass(&prig);
    let rig = prig.rig();

    // Settle on silence: the gate closes, compressors release, tails die.
    rig.start_test_signal(Arc::new(vec![0.0; LEN]));
    rig.render_offline(LEN);

    let mut signal = vec![0.0f32; LEN];
    signal[AT] = AMP;
    rig.start_test_signal(Arc::new(signal));
    let (l, r) = rig.measure_output(LEN, std::time::Duration::ZERO);
    if l.len() < LEN {
        return Err(format!("short capture: {} of {LEN}", l.len()));
    }
    let y: Vec<f32> = l.iter().zip(&r).map(|(a, b)| a.abs().max(b.abs())).collect();
    let (peak_i, peak) = y
        .iter()
        .copied()
        .enumerate()
        .fold((0, 0.0f32), |m, (i, v)| if v > m.1 { (i, v) } else { m });
    let floor = peak * 0.01;
    let onset = if peak > 1e-6 {
        y.iter().position(|&v| v > floor).map(|i| i as i64 - AT as i64)
    } else {
        None
    };
    Ok(Probe {
        onset,
        peak: peak_i as i64 - AT as i64,
        peak_db: 20.0 * peak.max(1e-9).log10(),
    })
}

fn show(p: &Result<Probe, String>) -> String {
    match p {
        Ok(p) => match p.onset {
            Some(o) => format!(
                "onset {o:>5} smp ({:>6.2} ms)  peak {:>5} smp  {:>6.1} dBFS",
                o as f64 * 1000.0 / f64::from(SR),
                p.peak,
                p.peak_db
            ),
            None => format!("silent ({:.1} dBFS)", p.peak_db),
        },
        Err(e) => format!("error: {e}"),
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let per_block = args.iter().any(|a| a == "--per-block");
    let only = args.iter().find(|a| !a.starts_with("--"));

    signal_guitar::levelling::apply_nam_calibration();
    let lib = RigLibrary::load_or_bootstrap();
    let mut late = Vec::new();

    // Baseline: the first patch with every block bypassed — the engine's own
    // path, input → slots → output tap. Must be 0.
    if let Some(def) = lib.profiles.first() {
        let built = profile_from_library(def, &lib.drive_presets);
        if let Some(p) = built.patches.first() {
            let mut bare = p.clone();
            for b in &mut bare.chain {
                b.bypassed = true;
            }
            println!("{:<44} {}", "baseline (all blocks bypassed)", show(&probe(&bare)));
        }
    }

    for def in &lib.profiles {
        if only.is_some_and(|n| !def.name.eq_ignore_ascii_case(n)) {
            continue;
        }
        println!("\n== {} ==", def.name);
        let built = profile_from_library(def, &lib.drive_presets);
        for patch in &built.patches {
            let r = probe(patch);
            println!("{:<44} {}", patch.name, show(&r));
            if let Ok(Probe { onset: Some(o), .. }) = r
                && o != 0
            {
                late.push((def.name.clone(), patch.clone(), o));
            }
        }
    }

    if late.is_empty() {
        println!("\nevery patch answers on the impulse's own sample: no DSP latency");
        return;
    }
    println!("\n{} patch(es) with a nonzero onset", late.len());
    if !per_block {
        println!("rerun with --per-block to find the block");
        return;
    }
    for (profile, patch, onset) in &late {
        println!("\n-- {profile} / {} (onset {onset}) --", patch.name);
        for (i, block) in patch.chain.iter().enumerate() {
            if block.bypassed || !block.has_backend() {
                continue;
            }
            let mut solo = patch.clone();
            for (j, b) in solo.chain.iter_mut().enumerate() {
                b.bypassed = j != i;
            }
            println!(
                "  {:<40} {}",
                format!("{:?} {}", block.block_type, block.name),
                show(&probe(&solo))
            );
        }
    }
}
