//! Dial the compressor block presets in against the player's own guitar.
//!
//!   cargo run --profile release-fast -p signal-guitar --example comp_dial
//!
//! A compressor's threshold only means something relative to the signal it
//! sees, so each preset is specified by what it should *do* — a target gain
//! reduction, averaged over playing — and its threshold is searched for on
//! the offline rig:
//!
//! - a **pre** comp (in front of the amp) hears the DI reference, exactly as
//!   the live Pre Comp slot does;
//! - a **post** comp hears amp + cab outputs from representative presets
//!   (each levelled to the same loudness, so one threshold serves them all).
//!
//! Gain reduction is read as loudness in minus loudness out with no makeup.
//! Makeup is then set so engaging the comp leaves the level where it was: in
//! front of an amp that keeps the amp driven as hard as before (the pedal
//! gain-matched on bypass); after it, the patch levelling stays put.
//!
//! Prints the presets as `blocks.styx` entries; writes nothing.

use std::sync::Arc;

use signal_guitar::compose::snapshot_patch;
use signal_guitar::library::RigLibrary;
use signal_guitar::measure::apply_chain_bypass;
use signal_sampler::nam_calibrate::DiReference;
use signal_sampler::rig::GuitarRig;
use signal_sampler::rig_node::Param;
use signal_sampler::rig_profile::{ProfileRig, RigPatch, RigProfile};

const SR: u32 = 48_000;

#[derive(Clone, Copy)]
enum Pos {
    Pre,
    Post,
}

/// A preset by intent: its time constants and the squeeze it should apply.
struct Spec {
    name: &'static str,
    pos: Pos,
    /// Average gain reduction while playing, dB.
    gr: f64,
    ratio: f64,
    attack: f64,
    release: f64,
    knee: f64,
    mix: f64,
    /// 1 = FET, 2 = VCA, 3 = opto (0 = clean).
    style: f64,
}

const SPECS: &[Spec] = &[
    // In front of the amp.
    Spec { name: "Clean Sustain", pos: Pos::Pre, gr: 4.0, ratio: 3.0, attack: 25.0, release: 200.0, knee: 6.0, mix: 1.0, style: 2.0 },
    Spec { name: "Funk Squash", pos: Pos::Pre, gr: 10.0, ratio: 8.0, attack: 10.0, release: 110.0, knee: 3.0, mix: 0.8, style: 1.0 },
    Spec { name: "Country Squash", pos: Pos::Pre, gr: 8.0, ratio: 6.0, attack: 6.0, release: 150.0, knee: 3.0, mix: 0.9, style: 1.0 },
    Spec { name: "Swell Sustain", pos: Pos::Pre, gr: 12.0, ratio: 10.0, attack: 3.0, release: 600.0, knee: 6.0, mix: 1.0, style: 3.0 },
    Spec { name: "Drive Tighten", pos: Pos::Pre, gr: 2.0, ratio: 2.5, attack: 30.0, release: 150.0, knee: 6.0, mix: 1.0, style: 2.0 },
    // After amp + cab.
    Spec { name: "Live Glue", pos: Pos::Post, gr: 2.0, ratio: 2.0, attack: 20.0, release: 400.0, knee: 8.0, mix: 1.0, style: 3.0 },
    Spec { name: "Clean Punch", pos: Pos::Post, gr: 4.0, ratio: 4.0, attack: 4.0, release: 250.0, knee: 1.0, mix: 1.0, style: 1.0 },
    Spec { name: "Lead Sustain", pos: Pos::Post, gr: 5.0, ratio: 4.0, attack: 15.0, release: 250.0, knee: 4.0, mix: 1.0, style: 2.0 },
    Spec { name: "Rhythm Catch", pos: Pos::Post, gr: 1.0, ratio: 2.0, attack: 25.0, release: 130.0, knee: 6.0, mix: 1.0, style: 2.0 },
];

/// Sources for the post presets: the amps each is meant to sit behind.
fn post_sources(name: &str) -> &'static [(&'static str, &'static str)] {
    match name {
        "Lead Sustain" => &[("Marshall JCM800", "Lead"), ("Soldano SLO-100", "Lead"), ("Dumble ODS", "Lead")],
        "Rhythm Catch" => &[("EVH 5150 III", "Chug"), ("Mesa Dual Rectifier", "Red Modern"), ("Peavey 5150/6505", "Rhythm")],
        _ => &[("Fender Deluxe Reverb", "Clean"), ("Vox AC30", "Edge"), ("Marshall Plexi", "Crunch"), ("Fender Twin Reverb", "Clean")],
    }
}

fn set(b: &mut signal_sampler::RigBlock, name: &str, v: f64) {
    let v = format!("{v}");
    match b.params.iter_mut().find(|p| p.name == name) {
        Some(p) => p.value = v,
        None => b.params.push(Param { name: name.into(), value: v }),
    }
}

/// `patch` with only `keep` (by name) processing, and `comp` configured.
fn isolate(patch: &RigPatch, keep: &[&str], comp: &str, on: bool, s: &Spec, threshold: f64, mix: f64) -> RigPatch {
    let mut p = patch.clone();
    for b in &mut p.chain {
        if b.name.eq_ignore_ascii_case(comp) {
            b.bypassed = !on;
            set(b, "threshold", threshold);
            set(b, "ratio", s.ratio);
            set(b, "attack", s.attack);
            set(b, "release", s.release);
            set(b, "knee", s.knee);
            set(b, "style", s.style);
            set(b, "mix", mix);
            set(b, "makeup", 0.0);
        } else if !keep.iter().any(|k| b.name.eq_ignore_ascii_case(k)) {
            b.bypassed = true;
        }
    }
    p
}

fn render(patch: &RigPatch, di: &[f32]) -> Option<f64> {
    let rig = GuitarRig::open_offline(SR).ok()?;
    let mut prig = ProfileRig::new(rig);
    prig.set_level_match(false);
    let mut profile = RigProfile::new("dial");
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

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();
    signal_guitar::levelling::apply_nam_calibration();
    let lib = RigLibrary::load_or_bootstrap();
    let base = lib.profiles.first().expect("a profile").clone();
    let di: Vec<f32> = DiReference::load_or_synthetic(f64::from(SR))
        .samples
        .iter()
        .map(|&s| s as f32)
        .collect();
    let build = |preset: &str, snap: &str| -> Option<RigPatch> {
        let mut def = base.clone();
        def.patches = vec![snapshot_patch(&base, preset, snap, "dial")?];
        signal_guitar::nodes::profile_from_library(&def, &lib.drive_presets)
            .patches
            .into_iter()
            .next()
    };
    let di_peak = di.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    println!("DI reference: peak {:.1} dBFS", 20.0 * di_peak.max(1e-9).log10());

    // One preset per core: each dial is a chain of dependent renders (a
    // bisection), but the presets are independent of each other.
    let dialled = signal_guitar::levelling::par_map(SPECS, 0, |s| -> Option<(String, String)> {
        let (comp, keep, sources): (&str, &[&str], Vec<RigPatch>) = match s.pos {
            Pos::Pre => ("Pre Comp", &[], build("Fender Deluxe Reverb", "Clean").into_iter().collect()),
            Pos::Post => (
                "Post Comp",
                &["Amp L", "Cab L", "Amp R", "Cab R"],
                post_sources(s.name).iter().filter_map(|(p, v)| build(p, v)).collect(),
            ),
        };
        if sources.is_empty() {
            return Some((format!("{}: no source", s.name), String::new()));
        }
        // Mean over the sources of (bypassed − engaged) loudness.
        let dry: Vec<f64> = sources
            .iter()
            .filter_map(|p| render(&isolate(p, keep, comp, false, s, 0.0, 1.0), &di))
            .collect();
        let gr_at = |threshold: f64, mix: f64| -> f64 {
            let wet: Vec<f64> = sources
                .iter()
                .filter_map(|p| render(&isolate(p, keep, comp, true, s, threshold, mix), &di))
                .collect();
            dry.iter().zip(&wet).map(|(d, w)| d - w).sum::<f64>() / wet.len().max(1) as f64
        };
        // Gain reduction falls as the threshold rises: bisect.
        let (mut lo, mut hi) = (-60.0f64, 0.0f64);
        for _ in 0..9 {
            let mid = 0.5 * (lo + hi);
            if gr_at(mid, 1.0) > s.gr {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let threshold = (0.5 * (lo + hi) * 2.0).round() / 2.0;
        let gr = gr_at(threshold, 1.0);
        let makeup = ((gr_at(threshold, s.mix) * 2.0).round() / 2.0).max(0.0);
        let line = format!(
            "{:<15} {:<4} threshold {:>6.1} dB  GR {:>4.1} dB  makeup +{:.1} dB",
            s.name,
            match s.pos {
                Pos::Pre => "pre",
                Pos::Post => "post",
            },
            threshold,
            gr,
            makeup
        );
        Some((line, format!(
            "{{block_type compressor, name \"{}\", params ({{param threshold, value {threshold}}} {{param ratio, value {}}} {{param attack, value {}}} {{param release, value {}}} {{param knee, value {}}} {{param style, value {}}} {{param mix, value {}}} {{param makeup, value {makeup}}}), bypass false}}",
            s.name, s.ratio, s.attack, s.release, s.knee, s.style, s.mix
        )))
    });
    for (line, _) in dialled.iter().flatten() {
        println!("{line}");
    }
    println!("\n-- blocks.styx --");
    for (_, entry) in dialled.iter().flatten() {
        if !entry.is_empty() {
            println!("{entry}");
        }
    }
}
