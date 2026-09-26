//! A live param write sets a block exactly as building it from the same
//! stored params does — for every built-in effect of the guitar chain and
//! every param each exposes.
//!
//! For each block type and each of its params: build the block from a
//! definition (every param at its default, as stored text), and again from
//! the definition with that one param changed; then write the change live to
//! the first, through the one conversion (`block_params::block_delta` →
//! `ResolvedWrite`), the path a param-only reload and a knob take. Both are
//! rendered on the same input:
//!
//! - **exact** — each block prepared after its params are set (as a build
//!   is), then a second of silence (so a param the effect glides has
//!   arrived): must then be bit-identical on the signal. This is the
//!   conversion, with nothing else in the way.
//! - **running** — the live-written block as it would be mid-song, not
//!   re-prepared: an effect that glides a param (a gain, a delay time) is
//!   allowed its glide, and must land on the built block's output after it.
//!
//! A param that cannot be written live must say so (`BlockDelta::Structural`
//! and listed in `block_params::STRUCTURAL`) rather than write something
//! else.

use signal_plugin_host::{PluginEvents, PluginInstance};
use signal_proto::block::BlockType;
use signal_sampler::block_params::{self, BlockDelta, ResolvedWrite};
use signal_sampler::rig::prepare_chain;
use signal_sampler::RigBlock;

const SR: u32 = 48_000;
const BLOCK: usize = 128;

/// The guitar chain's built-in effects, by the name the chain gives them.
const TYPES: &[(BlockType, &str)] = &[
    (BlockType::Eq, "EQ"),
    (BlockType::Compressor, "Comp"),
    (BlockType::Gate, "Gate"),
    (BlockType::Boost, "Boost"),
    (BlockType::Volume, "Volume"),
    (BlockType::Chorus, "Chorus"),
    (BlockType::Flanger, "Flanger"),
    (BlockType::Vibrato, "Vibrato"),
    (BlockType::Trem, "Trem"),
    (BlockType::Pitch, "Pitch"),
    (BlockType::Delay, "DLY 1"),
    (BlockType::Reverb, "VERB 1"),
];

fn input(frames: usize) -> Vec<f32> {
    let mut seed = 11u32;
    (0..frames)
        .map(|i| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.3;
            let burst = if i < SR as usize / 10 { noise } else { 0.0 };
            burst + 0.2 * (std::f32::consts::TAU * 196.0 * i as f32 / SR as f32).sin()
        })
        .collect()
}

fn render(bx: &mut Box<dyn PluginInstance>, x: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(x.len() * 2);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    for chunk in x.chunks(BLOCK) {
        let n = chunk.len();
        bx.process_block(chunk, chunk, &mut l[..n], &mut r[..n], &PluginEvents::default())
            .unwrap();
        out.extend_from_slice(&l[..n]);
        out.extend_from_slice(&r[..n]);
    }
    out
}

fn build(block: &RigBlock) -> Box<dyn PluginInstance> {
    let id = block.name.clone();
    let chain = prepare_chain(std::slice::from_ref(block), &[id], SR).expect("builds");
    chain.into_blocks().pop().expect("one block").1
}

fn fmt(v: f64) -> String {
    format!("{}", v as f32)
}

/// A value for `param` other than its default, inside its range.
fn changed(default: f64, min: f64, max: f64) -> f64 {
    let span = max - min;
    let up = default + 0.37 * span;
    let v = if up <= max { up } else { default - 0.37 * span };
    // Whole-number ranges (modes, engines, divisions) move by a whole step.
    if min.fract() == 0.0 && max.fract() == 0.0 && default.fract() == 0.0 && span >= 2.0 {
        v.round()
    } else {
        v
    }
}

/// A value a little away from `default`, inside the range.
fn nudged(default: f64, min: f64, max: f64) -> f64 {
    let span = max - min;
    let v = if default + 0.13 * span <= max { default + 0.13 * span } else { default - 0.13 * span };
    if min.fract() == 0.0 && max.fract() == 0.0 && default.fract() == 0.0 && span >= 2.0 {
        v.round().clamp(min, max)
    } else {
        v
    }
}

fn max_abs_diff(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f32::max)
}

fn error_db(a: &[f32], b: &[f32]) -> f32 {
    let rms = |x: &[f32]| (x.iter().map(|s| s * s).sum::<f32>() / x.len().max(1) as f32).sqrt();
    let err: Vec<f32> = a.iter().zip(b).map(|(x, y)| x - y).collect();
    20.0 * (rms(&err).max(1e-12) / rms(b).max(1e-12)).log10()
}

#[test]
fn a_live_write_sets_every_builtin_effect_as_its_build_does() {
    let x = input(SR as usize / 2);
    let quiet = vec![0.0f32; SR as usize];
    let settle = 2 * (SR as usize / 5); // 200 ms, interleaved stereo
    let mut checked = 0;
    let mut structural = Vec::new();
    let mut failures = Vec::new();
    let mut worst_running = (f32::NEG_INFINITY, String::new());
    let mut gliding = Vec::new();
    let mut slowest = (0.0f64, String::new());
    let mut slow = Vec::new();
    let only = std::env::var("LIVE_PARAMS_ONLY").ok();
    for &(bt, name) in TYPES.iter().filter(|(_, n)| only.as_deref().is_none_or(|o| o == *n)) {
        let infos = signal_sampler::native::build_native(&RigBlock::of_type(bt), SR)
            .expect("registered")
            .params();
        // Every param away from its default, so a write that disturbs
        // another param (a mode resetting the rest) shows.
        let base = infos.iter().fold(RigBlock::effect(bt, name), |b, p| {
            b.with_param(p.name.clone(), fmt(nudged(p.default, p.min, p.max)))
        });
        for p in &infos {
            let from = nudged(p.default, p.min, p.max);
            let v = changed(from, p.min, p.max);
            if (v as f32) == (from as f32) {
                continue;
            }
            let mut edited = base.clone();
            for q in &mut edited.params {
                if q.name == p.name {
                    q.value = fmt(v);
                }
            }
            let write = match block_params::block_delta(&base, &edited) {
                BlockDelta::Live(w) => w,
                BlockDelta::Same => {
                    failures.push(format!("{name}.{}: seen as no change", p.name));
                    continue;
                }
                BlockDelta::Structural => {
                    structural.push(format!("{name}.{}", p.name));
                    continue;
                }
            };
            let resolved = ResolvedWrite::resolve(bt, &write).expect("resolves");

            // Exact: the conversion alone. Both then run a second of
            // silence — an effect that glides a param (a reverb's decay, a
            // gain) gets there — and are compared on the signal after.
            let mut built = build(&edited);
            built.prepare(f64::from(SR), 512).unwrap();
            let mut written = build(&base);
            resolved.apply(written.as_mut());
            written.prepare(f64::from(SR), 512).unwrap();
            render(&mut built, &quiet);
            render(&mut written, &quiet);
            let (want, got) = (render(&mut built, &x), render(&mut written, &x));
            let d = max_abs_diff(&got, &want);
            if d != 0.0 || got.iter().any(|s| !s.is_finite()) {
                failures.push(format!("{name}.{} = {v}: exact differs by {d:e}", p.name));
            }

            // Running: written mid-song, glides allowed — and timed: this
            // is the hold of the renderer's lock a live write costs.
            let mut built = build(&edited);
            let mut running = build(&base);
            render(&mut running, &x[..SR as usize / 10]);
            // The least of three writes (there, back, there again): the
            // write's own cost, not the scheduler's.
            let back = match block_params::block_delta(&edited, &base) {
                BlockDelta::Live(w) => ResolvedWrite::resolve(bt, &w),
                _ => None,
            };
            let mut us = f64::INFINITY;
            for w in [Some(&resolved), back.as_ref(), Some(&resolved)].into_iter().flatten() {
                let t = std::time::Instant::now();
                w.apply(running.as_mut());
                us = us.min(t.elapsed().as_secs_f64() * 1e6);
            }
            if us > slowest.0 {
                slowest = (us, format!("{name}.{}", p.name));
            }
            if us > 50.0 {
                slow.push(format!("{name}.{} {us:.0}µs", p.name));
            }
            let mut running = build(&base);
            resolved.apply(running.as_mut());
            let (want, got) = (render(&mut built, &x), render(&mut running, &x));
            let e = error_db(&got[settle..], &want[settle..]);
            if e > worst_running.0 {
                worst_running = (e, format!("{name}.{}", p.name));
            }
            if e > -60.0 {
                gliding.push(format!("{name}.{} {e:.0} dB", p.name));
            }
            checked += 1;
        }
    }
    println!(
        "{checked} params written live, bit-identical to their builds; structural: {structural:?}; \
         running (no re-prepare), worst after 200 ms: {:.1} dB ({})",
        worst_running.0, worst_running.1
    );
    println!("{} params glide when written mid-song (above -60 dB after 200 ms): {gliding:?}", gliding.len());
    println!("slowest live write (the renderer-lock hold): {:.1} µs ({})", slowest.0, slowest.1);
    println!("writes over 50 µs: {slow:?}");
    assert!(failures.is_empty(), "{} mismatches:\n{}", failures.len(), failures.join("\n"));
    assert!(checked > 100, "the sweep covered the params: {checked}");
}

/// A NAM block's trims — where a drive knob's constant-loudness
/// compensation lands — written live, against a block built with them.
#[test]
fn nam_trims_written_live_match_the_build() {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/assets/amp_a.nam"))
        .expect("test model");
    let make = |input_db: f32, output_db: f32| -> Box<dyn PluginInstance> {
        let mut nam = signal_sampler::nam::NamProcessor::from_bytes(
            &bytes,
            "amp_a.nam".to_string(),
            f64::from(SR),
            512,
        )
        .expect("loads");
        // What the build does with the block's trims.
        nam.input_gain_db = input_db;
        nam.output_gain_db = output_db;
        let mut b: Box<dyn PluginInstance> = Box::new(nam);
        b.prepare(f64::from(SR), 512).unwrap();
        b
    };
    let mut built = make(4.5, -3.25);
    let mut written = make(0.0, 0.0);
    let old = RigBlock::nam("amp_a.nam");
    let mut new = old.clone();
    new.input_trim_db = 4.5;
    new.output_trim_db = -3.25;
    let BlockDelta::Live(w) = block_params::block_delta(&old, &new) else {
        panic!("trims are live");
    };
    ResolvedWrite::resolve(BlockType::Amp, &w).unwrap().apply(written.as_mut());
    let x = input(SR as usize / 4);
    assert_eq!(render(&mut built, &x), render(&mut written, &x), "bit-identical");
}
