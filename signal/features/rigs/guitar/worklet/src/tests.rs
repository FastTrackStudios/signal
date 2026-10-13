//! The worker protocol against a real NAM model, with a thread standing in
//! for the Web Worker: a remote model must sound exactly like the same model
//! inline — at the same time (lag zero) or exactly one quantum later (lag
//! one) — and a late worker must cost silence, never a stall.

use std::time::Duration;

use signal_sampler::RigBlock;
use signal_sampler::rig::prepare_chain;

use crate::remote::thread::ThreadLink;
use crate::remote::{Lag, Link};
use crate::runner::{BuildOptions, ChainRunner, build, plan_chain};

const Q: usize = 128;
const SR: u32 = 48_000;

fn model(name: &str) -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../default-config/models")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn amp() -> String {
    model("King of Tone both sides.nam")
}

/// The same block, built alone — what a worker runs.
fn worker_for(block: &RigBlock, delay: Duration) -> Box<dyn Link> {
    let built = prepare_chain(std::slice::from_ref(block), &["w".into()], SR).unwrap();
    let (_, boxed) = built.into_blocks().pop().unwrap();
    Box::new(ThreadLink::spawn(boxed, delay))
}

/// `remote` names the slots that go to a worker (cost over budget), the
/// rest are cheap enough to stay inline.
///
/// The budget is generous: tests run in parallel on debug builds, and the
/// budget is also how long the render loop waits for a worker — a tight one
/// would fail on scheduling noise, not on the protocol.
fn runner(blocks: &[RigBlock], remote: &[usize], delay: Duration) -> ChainRunner {
    runner_with_budget(blocks, remote, delay, 40_000)
}

fn runner_with_budget(
    blocks: &[RigBlock],
    remote: &[usize],
    delay: Duration,
    budget_us: u32,
) -> ChainRunner {
    let ids: Vec<String> = (0..blocks.len()).map(|i| format!("b{i}")).collect();
    let cost = |slot: usize, _: &RigBlock| {
        if remote.contains(&slot) {
            u32::MAX / 4
        } else {
            100
        }
    };
    let plan = plan_chain(blocks, &cost, budget_us);
    let mut link = |_: usize, b: &RigBlock| Ok(worker_for(b, delay));
    build(
        blocks,
        &ids,
        &plan,
        BuildOptions {
            sample_rate: SR,
            quantum: Q as u32,
            budget_us,
            link: &mut link,
        },
    )
    .unwrap()
}

fn guitar(quanta: usize) -> Vec<f32> {
    (0..quanta * Q)
        .map(|i| {
            let t = i as f32 / SR as f32;
            0.3 * (t * 2.0 * std::f32::consts::PI * 110.0).sin() * (-t * 3.0).exp()
        })
        .collect()
}

fn play(r: &mut ChainRunner, input: &[f32]) -> Vec<f32> {
    play_paced(r, input, Duration::ZERO)
}

/// As the audio device would: one quantum per `period`. A lag-one worker is
/// promised a whole quantum, which back-to-back calls would not give it.
fn play_paced(r: &mut ChainRunner, input: &[f32], period: Duration) -> Vec<f32> {
    let mut out = Vec::with_capacity(input.len());
    let (mut l, mut rr) = ([0.0; Q], [0.0; Q]);
    for q in input.chunks(Q) {
        r.process(q, &mut l, &mut rr);
        out.extend_from_slice(&l);
        std::thread::sleep(period);
    }
    out
}

/// A lone serial model over budget goes one quantum behind — and is then
/// exactly the inline model, 128 frames late.
#[test]
fn a_lagged_model_is_the_inline_model_one_quantum_late() {
    let blocks = [RigBlock::nam(amp()).named("Drive 1")];
    let input = guitar(12);

    let mut inline = runner(&blocks, &[], Duration::ZERO);
    assert_eq!(inline.added_latency(), 0);
    let want = play(&mut inline, &input);

    let mut lagged = runner(&blocks, &[0], Duration::ZERO);
    assert_eq!(
        lagged.plan().places[0],
        crate::plan::Place::Worker(Lag::One)
    );
    assert_eq!(lagged.added_latency(), Q as u32);
    let got = play_paced(&mut lagged, &input, Duration::from_millis(3));
    // (A lag-one worker is waited on for a quarter of the budget — 10 ms
    // here — on top of the quantum it already had.)

    assert!(
        got[..Q].iter().all(|&s| s == 0.0),
        "the first quantum is the lag"
    );
    assert_eq!(&got[Q..], &want[..want.len() - Q]);
    assert_eq!(lagged.misses(), 0);
}

/// Two amps: Amp R on a worker beside Amp L sounds exactly like both
/// inline, and adds nothing.
#[test]
fn amp_r_on_a_worker_blends_exactly_like_inline() {
    let blocks = [
        RigBlock::nam(amp()).named("Amp L"),
        RigBlock::nam(model("JHS Morning Glory V4 - Low Gain Blue.nam")).named("Amp R"),
    ];
    let input = guitar(12);
    let want = play(&mut runner(&blocks, &[], Duration::ZERO), &input);

    let mut remote = runner(&blocks, &[1], Duration::ZERO);
    assert_eq!(
        remote.plan().places[1],
        crate::plan::Place::Worker(Lag::Zero)
    );
    assert_eq!(remote.added_latency(), 0);
    let got = play(&mut remote, &input);
    assert_eq!(got, want);
    assert_eq!(remote.misses(), 0);
}

/// A worker that cannot keep up is waited on only so long: the quantum
/// plays silence, the miss is counted, and the render loop moves on.
#[test]
fn a_late_worker_costs_silence_not_a_stall() {
    let blocks = [
        RigBlock::nam(amp()).named("Amp L"),
        RigBlock::nam(amp()).named("Amp R"),
    ];
    // The worker takes 300 ms a quantum; the render loop waits 2 ms.
    let mut r = runner_with_budget(&blocks, &[1], Duration::from_millis(300), 2_000);
    let began = std::time::Instant::now();
    play(&mut r, &guitar(4));
    assert!(
        began.elapsed() < Duration::from_millis(200),
        "{:?}",
        began.elapsed()
    );
    assert!(r.misses() >= 3, "misses {}", r.misses());
}

/// Bypass and parameter routing reach the right block by id.
#[test]
fn bypass_and_params_find_their_block() {
    let blocks = [RigBlock::nam(amp()).named("Amp L")];
    let mut r = runner(&blocks, &[], Duration::ZERO);
    assert!(r.set_bypass("b0", true));
    assert!(!r.set_bypass("nope", true));
    let input = guitar(2);
    assert_eq!(
        play(&mut r, &input),
        input,
        "a bypassed chain is the dry guitar"
    );
    assert!(r.set_param("b0", 0, 0.5));
    let (i, o) = r.take_peaks();
    assert!(i > 0.0 && o > 0.0);
}

/// A bypassed model on a worker keeps hearing its input, so stomping it on
/// mid-stream is seamless: from that quantum on it sounds exactly like one
/// that was on all along — no silent quantum, no cold model.
#[test]
fn stomping_a_standby_model_on_is_seamless() {
    let input = guitar(12);
    let on = [RigBlock::nam(amp()).named("Drive 1")];
    let mut always = runner(&on, &[0], Duration::ZERO);
    let want = play_paced(&mut always, &input, Duration::from_millis(3));

    let mut off = on.clone();
    off[0].bypassed = true;
    let mut stomped = runner(&off, &[0], Duration::ZERO);
    assert_eq!(
        stomped.added_latency(),
        0,
        "a bypassed lagged model adds nothing"
    );
    let (first, rest) = input.split_at(6 * Q);
    let dry = play_paced(&mut stomped, first, Duration::from_millis(3));
    assert_eq!(dry, first, "bypassed: the dry guitar");
    assert!(stomped.set_bypass("b0", false));
    assert_eq!(stomped.added_latency(), Q as u32);
    let got = play_paced(&mut stomped, rest, Duration::from_millis(3));
    assert_eq!(got, &want[6 * Q..]);
    assert_eq!(stomped.misses(), 0);
}
