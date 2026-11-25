//! What offline blocks cost: the `level` analysis of a four-minute vocal
//! (synthetic: `level::synth_vocal`), and each apply stage's realtime
//! cost, per block of 512 frames at 48 kHz.
//!
//! `cargo run --release -p signal-core --example offline_costs`

use std::time::Instant;

use signal_core::block::BlockProcessor;
use signal_core::graph::{self, BuildOptions};
use signal_core::host::NoHost;
use signal_core::offline::align::{self, AlignApply};
use signal_core::offline::level::{self, Amounts, Knobs, LevelApply, Segment};
use signal_core::offline::{Analysis, Playback, State};
use signal_core::state::{BlockSpec, Preset};
use signal_core::{BlockFactory, BuiltinFactory, Processor};

const SR: f64 = 48_000.0;
const BLOCK: usize = 512;

fn per_block(seconds: f64, frames: usize) -> (f64, f64) {
    let blocks = frames as f64 / BLOCK as f64;
    let us = seconds * 1e6 / blocks;
    // A 512-frame block is 10.67 ms of audio.
    (us, us / (BLOCK as f64 / SR * 1e6) * 100.0)
}

fn time_block(name: &str, b: &mut dyn BlockProcessor, input: &[f32]) {
    let (mut l, mut r) = (input.to_vec(), input.to_vec());
    let began = Instant::now();
    for (cl, cr) in l.chunks_mut(BLOCK).zip(r.chunks_mut(BLOCK)) {
        b.process(cl, cr);
    }
    let (us, pct) = per_block(began.elapsed().as_secs_f64(), input.len());
    println!("  {name:<40} {us:>7.2} µs per 512-frame block ({pct:.3} % of one core)");
}

fn main() {
    let seconds = 240.0;
    let vocal = level::synth_vocal(SR, seconds, 11);
    let input: Vec<f32> = vocal.iter().map(|s| *s as f32).collect();

    let began = Instant::now();
    let plan = level::plan(
        &[Segment {
            start_s: 0.0,
            samples: &vocal,
            sample_rate: SR,
        }],
        &Knobs::default(),
        &Amounts::default(),
        None,
    )
    .expect("a vocal");
    let analysis = began.elapsed();
    println!(
        "level analysis, a {seconds:.0} s vocal at 48 kHz: {:.0} ms",
        analysis.as_secs_f64() * 1e3
    );
    println!(
        "  lanes: gate {} points, breath {}, ride {}, de-ess {}",
        plan.lanes.gate.points().len(),
        plan.lanes.breath.points().len(),
        plan.lanes.ride.points().len(),
        plan.lanes.ess.points().len(),
    );

    println!("apply stages (one stereo block each):");
    time_block("level, at rest", &mut LevelApply::new(48_000.0), &input);
    let mut moving = LevelApply::new(48_000.0);
    moving.set_param(level::RIDE, -6.0);
    moving.set_param(level::ESS, -4.0);
    time_block("level, ramping, de-ess on", &mut moving, &input);
    time_block(
        "align, delay 37",
        &mut AlignApply::new(48_000.0, 37.0),
        &input,
    );
    let mut rotating = AlignApply::new(48_000.0, -12.0);
    rotating.set_param(align::INVERT, 1.0);
    rotating.set_param(align::AP1_HZ, 50.0);
    rotating.set_param(align::AP2_HZ, 120.0);
    rotating.set_param(align::AP0_HZ, 200.0);
    time_block(
        "align, advance 12, inverted, 3 all-passes",
        &mut rotating,
        &input,
    );

    // The whole path: a graph playing the four lanes itself (a host without
    // host-automation/1), splitting pieces at every breakpoint.
    let mut a = Analysis {
        state: State::Fresh,
        playback: Playback::Signal,
        ..Analysis::default()
    };
    for (id, c) in plan.lanes.by_param() {
        a.lanes.insert(id.to_owned(), c.clone());
    }
    let mut spec = BlockSpec::signal("level", "level");
    spec.analysis = Some(a);
    let mut preset = Preset::passthrough();
    preset.lanes[0].blocks = vec![spec];
    let factories: [&dyn BlockFactory; 1] = [&BuiltinFactory];
    let (g, _) = graph::build(
        &preset,
        &BuildOptions {
            sample_rate: 48_000.0,
            max_block: BLOCK,
            port: None,
            factories: &factories,
            host: &NoHost,
        },
    );
    let mut p = Processor::new();
    p.install(g);
    let (mut l, mut r) = (input.clone(), input.clone());
    let began = Instant::now();
    for (cl, cr) in l.chunks_mut(BLOCK).zip(r.chunks_mut(BLOCK)) {
        let n = cl.len();
        let mut io: [&mut [f32]; 2] = [cl, cr];
        p.process(&mut io, 2, n, &[], &[]);
    }
    let (us, pct) = per_block(began.elapsed().as_secs_f64(), input.len());
    println!(
        "  {:<40} {us:>7.2} µs per 512-frame block ({pct:.3} % of one core)",
        "level in a graph, Signal playing 4 lanes"
    );
    let (mut l, mut r) = (input.clone(), input);
    let mut empty = Processor::new();
    empty.install(
        graph::build(
            &Preset::passthrough(),
            &BuildOptions {
                sample_rate: 48_000.0,
                max_block: BLOCK,
                port: None,
                factories: &factories,
                host: &NoHost,
            },
        )
        .0,
    );
    let began = Instant::now();
    for (cl, cr) in l.chunks_mut(BLOCK).zip(r.chunks_mut(BLOCK)) {
        let n = cl.len();
        let mut io: [&mut [f32]; 2] = [cl, cr];
        empty.process(&mut io, 2, n, &[], &[]);
    }
    let (us, pct) = per_block(began.elapsed().as_secs_f64(), l.len());
    println!(
        "  {:<40} {us:>7.2} µs per 512-frame block ({pct:.3} % of one core)",
        "(an empty graph, for scale)"
    );
}
