//! Why a dual-amp blend comes out quieter than either of its amps.
//!
//!   cargo run --profile release-fast -p signal-guitar --example blend_probe
//!
//! For every Amp module snapshot with a second amp (`nam2`), build a preset
//! snapshot that plays it exactly as the rig does and render the DI through
//! three versions of the chain on the offline rig: both amps (the blend),
//! Amp L alone (Amp R + Cab R bypassed), Amp R alone (Amp L + Cab L
//! bypassed). Two equally loud amps summed at half each land 0 dB (coherent)
//! to −3 dB (uncorrelated) under one; anything much quieter is cancellation
//! or a branch lost, and the L/R correlation says which.

use std::sync::Arc;

use signal_guitar::compose::snapshot_patch;
use signal_guitar::library::RigLibrary;
use signal_guitar::measure::apply_chain_bypass;
use signal_sampler::nam_calibrate::DiReference;
use signal_sampler::rig::GuitarRig;
use signal_sampler::rig_profile::{ProfileRig, RigPatch, RigProfile};

const SR: u32 = 48_000;

fn render(patch: &RigPatch, di: &[f32]) -> Option<Vec<f32>> {
    let rig = GuitarRig::open_offline(SR).ok()?;
    let mut prig = ProfileRig::new(rig);
    prig.set_level_match(false);
    let mut profile = RigProfile::new("blend");
    profile.patches.push(patch.clone());
    prig.load_profile(profile, None).ok()?;
    if !prig.activate(0) {
        return None;
    }
    apply_chain_bypass(&prig);
    if std::env::var_os("DUMP_IDS").is_some() {
        let names: Vec<String> = patch
            .chain
            .iter()
            .filter(|b| b.has_backend())
            .map(|b| format!("{}{}", b.name, if b.bypassed { "(byp)" } else { "" }))
            .collect();
        println!("  chain(has_backend) {names:?}");
        println!("  live ids           {:?}", prig.active_block_ids());
        for b in patch.chain.iter().filter(|b| b.has_backend() && !b.bypassed) {
            println!("    {:<14} {:?} impl={:?} params={:?}", b.name, b.block_type, b.implementation(), b.params.iter().map(|p| format!("{}={}", p.name, p.value)).collect::<Vec<_>>());
        }
    }
    if std::env::var_os("DUMP_SLOTS").is_some() {
        for id in prig.active_block_ids() {
            let d = prig.with_active_block_instance(&id, |inst| {
                let ps = inst.params();
                let vals: Vec<String> = ps.iter().map(|p| format!("{}={:?}", p.name, inst.param_value(p.id))).collect();
                format!("{} {:?}", inst.descriptor().name, vals)
            });
            println!("    slot {id}: {d:?}");
        }
    }
    let rig = prig.rig();
    let samples = Arc::new(di.to_vec());
    rig.start_test_signal(samples.clone());
    rig.render_offline(SR as usize);
    rig.start_test_signal(samples);
    let (l, r) = rig.measure_output(di.len(), std::time::Duration::ZERO);
    Some(l.iter().zip(&r).map(|(a, b)| 0.5 * (a + b)).collect())
}

fn lufs(x: &[f32]) -> f64 {
    let v: Vec<f64> = x.iter().map(|&s| f64::from(s)).collect();
    signal_sampler::loudness::integrated_lufs(&v, f64::from(SR))
}

fn corr(a: &[f32], b: &[f32]) -> f64 {
    let (mut ab, mut aa, mut bb) = (0.0f64, 0.0f64, 0.0f64);
    for (&x, &y) in a.iter().zip(b) {
        let (x, y) = (f64::from(x), f64::from(y));
        ab += x * y;
        aa += x * x;
        bb += y * y;
    }
    ab / (aa * bb).sqrt().max(1e-30)
}

fn with_bypass(patch: &RigPatch, names: &[&str]) -> RigPatch {
    let mut p = patch.clone();
    for b in &mut p.chain {
        if names.iter().any(|n| b.name.eq_ignore_ascii_case(n)) {
            b.bypassed = true;
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
    let lib_drives = &lib.drive_presets;
    let base = lib.profiles.first().expect("a profile").clone();
    let di: Vec<f32> = DiReference::load_or_synthetic(f64::from(SR))
        .samples
        .iter()
        .map(|&s| s as f32)
        .collect();

    for preset in &comp.presets {
        for snap in &preset.snapshots {
            let blend = snap.modules.iter().any(|m| {
                comp.modules.iter().any(|mp| {
                    mp.name.eq_ignore_ascii_case(&m.preset)
                        && mp.snapshots.iter().any(|s| {
                            s.name.eq_ignore_ascii_case(&m.snapshot) && !s.nam2.is_empty()
                        })
                })
            });
            if !blend {
                if std::env::var_os("TRIM_CHECK").is_none() {
                    continue;
                }
            }
            let mut def = base.clone();
            let Some(p) = snapshot_patch(&base, &preset.name, &snap.name, "blend") else {
                continue;
            };
            def.patches = vec![p];
            let built = signal_guitar::nodes::profile_from_library(&def, lib_drives);
            let Some(patch) = built.patches.first() else { continue };
            // Raw: without the snapshot's level, so each branch reads as built.
            let mut patch = patch.clone();
            for b in &mut patch.chain {
                if b.name.eq_ignore_ascii_case("Patch Trim") {
                    for p in &mut b.params {
                        p.value = "0".into();
                    }
                }
            }
            let patch = &patch;

            println!("\n== {} · {} ==", preset.name, snap.name);
            for b in &patch.chain {
                if b.name.contains("Trim") || b.name.contains("Volume") {
                    println!("  {:<12} bypassed={:<5} params={:?}", b.name, b.bypassed, b.params);
                }
                if b.name.contains("Amp") || b.name.contains("Cab") {
                    let file = |p: &str| {
                        std::path::Path::new(p)
                            .file_name()
                            .map(|f| f.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    };
                    println!(
                        "  {:<6} bypassed={:<5} in={:+.1} out={:+.1} {}{}",
                        b.name,
                        b.bypassed,
                        b.input_trim_db,
                        b.output_trim_db,
                        file(&b.nam),
                        file(&b.ir)
                    );
                }
            }
            let (Some(both), Some(l), Some(r)) = (
                render(patch, &di),
                render(&with_bypass(patch, &["Amp R", "Cab R"]), &di),
                render(&with_bypass(patch, &["Amp L", "Cab L"]), &di),
            ) else {
                println!("  render failed");
                continue;
            };
            println!(
                "  blend {:6.1} LUFS   L alone {:6.1}   R alone {:6.1}   corr(L,R) {:+.2}   level_db {:+.1}",
                lufs(&both),
                lufs(&l),
                lufs(&r),
                corr(&l, &r),
                snap.level_db
            );
            if std::env::var_os("MINIMAL").is_some() {
                let keep = |names: &[&str], trim: &str| {
                    let mut p = patch.clone();
                    p.chain.retain(|b| names.iter().any(|n| b.name.eq_ignore_ascii_case(n)));
                    for b in &mut p.chain {
                        if b.name.eq_ignore_ascii_case("Patch Trim") {
                            b.params[0].value = trim.into();
                        }
                    }
                    p
                };
                let first = render(&keep(&["Amp L", "Amp R", "Patch Trim"], "-40"), &di).map(|x| lufs(&x));
                println!("  MIN first-render L+R trim-40 {first:?}");
                for (names, label) in [
                    (&["Amp L", "Amp R", "Patch Trim"][..], "L+R+trim"),
                    (&["Amp L", "Patch Trim"][..], "L+trim"),
                    (&["Amp R", "Patch Trim"][..], "R+trim"),
                ] {
                    let a = render(&keep(names, "0"), &di).map(|x| lufs(&x));
                    let b = render(&keep(names, "10"), &di).map(|x| lufs(&x));
                    let c = render(&keep(names, "-40"), &di).map(|x| lufs(&x));
                    println!("  MIN {label:<10} trim-40 {c:?}");
                    println!("  MIN {label:<10} trim0 {a:?}  trim10 {b:?}");
                }
                std::process::exit(0);
            }
            let mut hot = patch.clone();
            for b in &mut hot.chain {
                if b.name.eq_ignore_ascii_case("Patch Trim") {
                    for p in &mut b.params {
                        p.value = "10".into();
                    }
                }
            }
            if let Some(h) = render(&hot, &di) {
                let pk = |x: &[f32]| x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
                println!("  blend with Patch Trim +10: {:6.1} LUFS  peak {:.4} vs {:.4}  identical={}", lufs(&h), pk(&h), pk(&both), h == both);
            }
            if let Some(h) = render(&with_bypass(&hot, &["Amp R"]), &di) {
                println!("  L alone with Patch Trim +10: {:6.1} LUFS", lufs(&h));
            }
            if let Some(h) = render(&with_bypass(&hot, &["Post Comp", "Limiter"]), &di) {
                println!("  blend +10, no Post Comp/Limiter: {:6.1} LUFS", lufs(&h));
            }
            if let Some(h) = render(&with_bypass(patch, &["Post Comp", "Limiter"]), &di) {
                println!("  blend +0, no Post Comp/Limiter: {:6.1} LUFS", lufs(&h));
            }
            // The blend should be (L + R) / 2 of the solo renders.
            let ideal: Vec<f32> = l.iter().zip(&r).map(|(a, b)| 0.5 * (a + b)).collect();
            println!("  ideal (L+R)/2 {:6.1} LUFS", lufs(&ideal));
        }
    }
}
