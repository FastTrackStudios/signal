//! How hard each variation's Post Comp actually works, on every preset.
//!
//!   cargo run --profile release-fast -p signal-guitar --example comp_verify
//!
//! `comp_dial` sets each compressor preset against a few representative
//! amps; this checks it against all of them. Every variation of every preset
//! is built as the rig builds it (its drives, its Pre Comp, its amps), cut
//! after the Post Comp, and rendered with the Post Comp engaged and bypassed
//! (makeup 0): the difference is the gain reduction it applies on that amp.
//! One variation per core. Nothing is written.

use std::sync::Arc;

use signal_guitar::compose::snapshot_patch;
use signal_guitar::library::RigLibrary;
use signal_guitar::measure::apply_chain_bypass;
use signal_sampler::nam_calibrate::DiReference;
use signal_sampler::rig::GuitarRig;
use signal_sampler::rig_profile::{ProfileRig, RigPatch, RigProfile};

const SR: u32 = 48_000;

/// The range each post preset is meant to work in, dB of average GR.
fn intended(preset: &str) -> Option<(f64, f64)> {
    match preset {
        "Live Glue" => Some((0.5, 4.0)),
        "Clean Punch" => Some((2.0, 6.0)),
        "Lead Sustain" => Some((3.0, 7.0)),
        "Rhythm Catch" => Some((0.0, 2.5)),
        _ => None,
    }
}

fn render(patch: &RigPatch, di: &[f32]) -> Option<f64> {
    let rig = GuitarRig::open_offline(SR).ok()?;
    let mut prig = ProfileRig::new(rig);
    prig.set_level_match(false);
    let mut profile = RigProfile::new("verify");
    profile.patches.push(patch.clone());
    prig.load_profile(profile, None).ok()?;
    if !prig.activate(0) {
        return None;
    }
    apply_chain_bypass(&prig);
    let rig = prig.rig();
    let samples = Arc::new(di.to_vec());
    rig.start_test_signal(samples.clone());
    rig.render_offline(SR as usize / 2);
    rig.start_test_signal(samples);
    let (l, r) = rig.measure_output(di.len(), std::time::Duration::ZERO);
    let mono: Vec<f64> = l.iter().zip(&r).map(|(a, b)| f64::from(a + b) * 0.5).collect();
    Some(signal_sampler::loudness::integrated_lufs(&mono, f64::from(SR)))
}

/// The chain up to and including the Post Comp, with it engaged or not.
fn cut(patch: &RigPatch, post_on: bool) -> RigPatch {
    let mut p = patch.clone();
    let mut after = false;
    for b in &mut p.chain {
        if after {
            b.bypassed = true;
        } else if b.name.eq_ignore_ascii_case("Post Comp") {
            after = true;
            if !post_on {
                b.bypassed = true;
            }
            match b.params.iter_mut().find(|x| x.name == "makeup") {
                Some(x) => x.value = "0".into(),
                None => b.params.push(signal_sampler::rig_node::Param {
                    name: "makeup".into(),
                    value: "0".into(),
                }),
            }
        }
    }
    p
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();
    signal_guitar::levelling::apply_nam_calibration();
    let lib = RigLibrary::load_or_bootstrap();
    let comp = RigLibrary::load_compositions();
    let base = lib.profiles.first().expect("a profile").clone();
    let di: Vec<f32> = DiReference::load_or_synthetic(f64::from(SR))
        .samples
        .iter()
        .map(|&s| s as f32)
        .collect();

    let jobs: Vec<(String, String, String)> = comp
        .presets
        .iter()
        .flat_map(|p| {
            p.snapshots.iter().map(move |s| {
                let post = s
                    .blocks
                    .iter()
                    .find(|b| b.block.eq_ignore_ascii_case("Post Comp"))
                    .map(|b| b.preset.clone())
                    .unwrap_or_default();
                (p.name.clone(), s.name.clone(), post)
            })
        })
        .collect();

    let results = signal_guitar::levelling::par_map(&jobs, 0, |(preset, snap, post)| {
        let mut def = base.clone();
        def.patches = vec![snapshot_patch(&base, preset, snap, "verify")?];
        let patch = signal_guitar::nodes::profile_from_library(&def, &lib.drive_presets)
            .patches
            .into_iter()
            .next()?;
        let engaged = patch
            .chain
            .iter()
            .any(|b| b.name.eq_ignore_ascii_case("Post Comp") && !b.bypassed);
        if !engaged {
            return Some((0.0, false, post.clone()));
        }
        let off = render(&cut(&patch, false), &di)?;
        let on = render(&cut(&patch, true), &di)?;
        Some((off - on, true, post.clone()))
    });

    let mut outside = 0;
    for ((preset, snap, _), r) in jobs.iter().zip(&results) {
        let Some((gr, engaged, post)) = r else {
            println!("{preset:<22} {snap:<22} render failed");
            continue;
        };
        let flag = match intended(post) {
            Some((lo, hi)) if *engaged && (*gr < lo || *gr > hi) => {
                outside += 1;
                format!("  ← outside {lo}–{hi}")
            }
            _ if !engaged => "  (Post Comp not engaged)".into(),
            _ => String::new(),
        };
        println!("{preset:<22} {snap:<22} {post:<13} GR {gr:>4.1} dB{flag}");
    }
    println!("\n{} variations, {outside} outside their preset's range", jobs.len());
}
