//! Gapless patch switching and trails, end to end: the offline rig — the
//! same project, slots, renderer and output stage the live rig runs —
//! driven through switches, with what is *heard* (after the patch level and
//! the tails ringing under it) checked against rigs that never switched.
//!
//! What each test pins:
//! - a tail rings on after a switch exactly as it would have without one;
//! - a switch crossfades — no step, with or without time effects;
//! - rapid switching stays bounded (voices, peaks, steps, finite samples);
//! - switching back to a ringing patch picks its tail up where it was;
//! - bypassing a delay lets its repeats ring out;
//! - a tail keeps its own patch's level;
//! - under a concurrent switch storm, no block ever falls back to the raw
//!   input (the one-lock swap and the renderer's bounded spin).

use std::sync::Arc;

use signal_proto::block::BlockType;
use signal_sampler::{GuitarRig, ModelId, RigBlock};

const SR: u32 = 48_000;

fn rig() -> GuitarRig {
    GuitarRig::open_offline(SR).expect("offline rig")
}

/// Render `frames`, returning what is heard (left).
fn heard_frames(rig: &GuitarRig, frames: usize) -> Vec<f32> {
    rig.arm_heard_capture(frames);
    rig.render_offline(frames);
    rig.take_output_capture().0
}

fn heard(rig: &GuitarRig, secs: f64) -> Vec<f32> {
    heard_frames(rig, (secs * f64::from(SR)) as usize)
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|s| s * s).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

fn max_step(x: &[f32]) -> f32 {
    x.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max)
}

/// `a` against `b`, as the error's level below `b`'s (dB; lower is closer).
fn error_db(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    let err: Vec<f32> = (0..n).map(|i| a[i] - b[i]).collect();
    20.0 * (rms(&err).max(1e-12) / rms(&b[..n]).max(1e-12)).log10()
}

/// A 50 ms noise burst, then silence (long: the test signal loops).
fn burst() -> Arc<Vec<f32>> {
    let mut sig = vec![0.0f32; SR as usize * 30];
    let mut seed = 7u32;
    for s in sig.iter_mut().take(SR as usize / 20) {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *s = ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.5;
    }
    Arc::new(sig)
}

fn sine(hz: f32, amp: f32) -> Arc<Vec<f32>> {
    // A whole number of cycles, so the loop point is seamless.
    let period = (SR as f32 / hz).round() as usize;
    let len = period * (SR as usize * 10 / period);
    Arc::new(
        (0..len)
            .map(|i| amp * (std::f32::consts::TAU * i as f32 / period as f32).sin())
            .collect(),
    )
}

fn gain(db: f32) -> RigBlock {
    RigBlock::effect(BlockType::Volume, "Trim").with_param("gain_db", db.to_string())
}

fn verb(decay: f32) -> RigBlock {
    RigBlock::effect(BlockType::Reverb, "VERB 1")
        .with_param("mix", "1")
        .with_param("level", "-6")
        .with_param("algorithm", "1")
        .with_param("decay", decay.to_string())
}

fn delay(ms: f32, feedback: f32) -> RigBlock {
    RigBlock::effect(BlockType::Delay, "DLY 1")
        .with_param("mix", "1")
        .with_param("level", "-3")
        .with_param("style", "1")
        .with_param("tap_div_l", "7")
        .with_param("tap_div_r", "7")
        .with_param("time", ms.to_string())
        .with_param("feedback", feedback.to_string())
}

fn install(rig: &mut GuitarRig, blocks: &[RigBlock]) -> ModelId {
    rig.install_chain(blocks).expect("chain builds")
}

/// After a switch, the old patch's reverb rings on sample for sample as it
/// would have with no switch at all.
#[test]
fn a_tail_rings_on_as_if_nothing_had_switched() {
    let mut switched = rig();
    let a = install(&mut switched, &[verb(0.6)]);
    let b = install(&mut switched, &[gain(0.0)]);
    let mut reference = rig();
    let a_ref = install(&mut reference, &[verb(0.6)]);

    for (r, id) in [(&switched, a), (&reference, a_ref)] {
        r.set_active(Some(id));
        r.start_test_signal(burst());
        heard(r, 0.3);
    }
    switched.set_active(Some(b));
    let got = heard(&switched, 1.5);
    let want = heard(&reference, 1.5);
    // Past the 8 ms crossfade the stage plays the voice alone.
    let skip = SR as usize / 50;
    assert!(rms(&want[skip..]) > 1e-3, "the reference rings");
    let e = error_db(&got[skip..], &want[skip..]);
    assert!(e < -60.0, "the tail is the unswitched tail: error {e:.1} dB");
}

/// A switch between two patches with time effects and different levels
/// steps no more than the music itself does.
#[test]
fn a_switch_with_time_effects_does_not_click() {
    let mut rig = rig();
    let a = install(&mut rig, &[gain(6.0), delay(120.0, 0.3)]);
    let b = install(&mut rig, &[gain(-6.0), verb(0.5)]);
    rig.set_active(Some(a));
    rig.start_test_signal(sine(110.0, 0.2));
    heard(&rig, 0.5);
    let before = heard(&rig, 0.2);
    rig.set_active(Some(b));
    let around = heard(&rig, 0.05);
    heard(&rig, 0.5);
    let after = heard(&rig, 0.2);
    let steady = max_step(&before).max(max_step(&after));
    let at_switch = max_step(&around);
    assert!(
        at_switch < steady * 1.3,
        "largest step at the switch {at_switch} vs steady {steady}"
    );
}

/// Forty switches 5 ms apart through five patches: every sample finite and
/// under full scale, the tails capped, no step out of proportion.
#[test]
fn rapid_switching_stays_bounded() {
    let mut rig = rig();
    let ids: Vec<ModelId> = (0..5)
        .map(|i| install(&mut rig, &[gain(-6.0 + 3.0 * i as f32), verb(0.7)]))
        .collect();
    rig.set_active(Some(ids[0]));
    rig.start_test_signal(sine(220.0, 0.2));
    heard(&rig, 0.3);
    let mut out = Vec::new();
    for k in 0..40 {
        rig.set_active(Some(ids[k % ids.len()]));
        out.extend(heard_frames(&rig, 240));
        assert!(rig.tail_voices() <= 4, "at most four tails ring");
    }
    out.extend(heard(&rig, 0.5));
    assert!(out.iter().all(|s| s.is_finite()), "no NaN or inf");
    let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(peak <= 1.0, "under full scale: {peak}");
    // A 220 Hz sine at up to +6 dB steps ~0.023 a sample; reverb adds some.
    assert!(max_step(&out) < 0.15, "largest step {}", max_step(&out));
}

/// A → B → A while A still rings: A's reverb carries on where it was, as
/// if A had never been left.
#[test]
fn switching_back_picks_the_tail_up() {
    let mut switched = rig();
    let a = install(&mut switched, &[verb(0.7)]);
    let b = install(&mut switched, &[gain(0.0)]);
    let mut reference = rig();
    let a_ref = install(&mut reference, &[verb(0.7)]);
    for (r, id) in [(&switched, a), (&reference, a_ref)] {
        r.set_active(Some(id));
        r.start_test_signal(burst());
        heard(r, 0.3);
    }
    switched.set_active(Some(b));
    heard(&switched, 0.2);
    heard(&reference, 0.2);
    switched.set_active(Some(a));
    let got = heard(&switched, 1.0);
    // B (no time effects) is gone once its fade is; A plays, not a tail.
    assert_eq!(switched.tail_voices(), 0, "nothing left ringing");
    let want = heard(&reference, 1.0);
    let skip = SR as usize / 50;
    let e = error_db(&got[skip..], &want[skip..]);
    assert!(e < -60.0, "A resumed with its own tail: error {e:.1} dB");
}

/// Bypassing a delay mid-phrase: the repeats already in it ring out.
#[test]
fn a_bypassed_delay_rings_out() {
    let mut bypassed = rig();
    let ids = ["dly".to_string()];
    let a = bypassed
        .install_chain_with_ids(&[delay(150.0, 0.6)], &ids)
        .expect("chain");
    let mut reference = rig();
    let a_ref = install(&mut reference, &[delay(150.0, 0.6)]);
    for (r, id) in [(&bypassed, a), (&reference, a_ref)] {
        r.set_active(Some(id));
        r.start_test_signal(burst());
        heard(r, 0.1);
    }
    assert!(bypassed.set_block_slot_bypass("dly", true));
    let got = heard(&bypassed, 1.0);
    let want = heard(&reference, 1.0);
    assert!(rms(&got[SR as usize / 4..]) > 1e-3, "repeats still sound");
    let skip = SR as usize / 50;
    let e = error_db(&got[skip..], &want[skip..]);
    assert!(e < -60.0, "the repeats are the unbypassed ones: error {e:.1} dB");
}

/// A tail keeps the level its patch played at, not the new patch's.
#[test]
fn a_tail_keeps_its_own_patch_level() {
    let mut switched = rig();
    let a = install(&mut switched, &[verb(0.6)]);
    let b = install(&mut switched, &[gain(0.0)]);
    let mut reference = rig();
    let a_ref = install(&mut reference, &[verb(0.6)]);
    for (r, id) in [(&switched, a), (&reference, a_ref)] {
        r.set_patch_trim_db(-6.0);
        r.set_active(Some(id));
        r.start_test_signal(burst());
        heard(r, 0.3);
    }
    // B plays 12 dB hotter; A's tail must not follow it.
    switched.set_patch_trim_db(6.0);
    switched.set_active(Some(b));
    let got = heard(&switched, 1.0);
    let want = heard(&reference, 1.0);
    let skip = SR as usize / 50;
    let e = error_db(&got[skip..], &want[skip..]);
    assert!(e < -60.0, "the tail at A's −6 dB: error {e:.1} dB");
}

/// An offline rig shared between a "control" and an "audio" thread, as the
/// live rig is between the UI and the device callback.
///
/// `GuitarRig` is not `Sync` only because of its live-device fields (the
/// duplex host, its stats), which an offline rig does not have (`None`).
/// Everything the two threads touch here — the swap state, the renderer,
/// the plugin map — is behind the rig's own mutexes, exactly as it is live.
struct Offline<'a>(&'a GuitarRig);
// SAFETY: see above — an offline rig's non-`Sync` fields are all `None`,
// and every field the two threads share is behind a `Mutex`.
unsafe impl Sync for Offline<'_> {}

/// One thread renders while another switches as fast as it can. Both
/// patches play the guitar 12 dB up, so a block that skipped the chain (the
/// raw input) would stand out 12 dB down. None may.
#[test]
fn a_switch_storm_never_drops_to_the_raw_input() {
    const BLOCK: usize = 128;
    const BLOCKS: usize = 3_000;
    let mut rig = rig();
    let a = install(&mut rig, &[gain(12.0)]);
    let b = install(&mut rig, &[gain(12.0), gain(0.0)]);
    rig.set_active(Some(a));
    rig.start_test_signal(sine(200.0, 0.05));
    heard(&rig, 0.1);
    rig.arm_heard_capture(BLOCK * BLOCKS);
    let done = std::sync::atomic::AtomicBool::new(false);
    let shared = Offline(&rig);
    let (shared, done) = (&shared, &done);
    std::thread::scope(|s| {
        s.spawn(move || {
            let mut k = 0usize;
            while !done.load(std::sync::atomic::Ordering::Relaxed) {
                shared.0.set_active(Some(if k % 2 == 0 { b } else { a }));
                k += 1;
                std::thread::yield_now();
            }
        });
        for _ in 0..BLOCKS {
            rig.render_offline(BLOCK);
        }
        done.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    let out = rig.take_output_capture().0;
    // 0.05 · 4 (+12 dB) as RMS; the raw input would read a quarter of it.
    let expected = 0.05 * 3.98 * std::f32::consts::FRAC_1_SQRT_2;
    let worst = out
        .chunks(BLOCK)
        .map(rms)
        .fold(f32::INFINITY, f32::min);
    assert!(
        worst > expected * 0.6,
        "every block through the chain: quietest {worst} vs {expected}"
    );
}

/// How long a block takes with four tails ringing (real reverbs) on top of
/// the playing patch — a budget check, not a correctness one; run it with
/// `--ignored --nocapture` in release to read the numbers.
#[test]
#[ignore = "timing: run in release with --ignored --nocapture"]
fn four_tails_fit_the_realtime_budget() {
    const BLOCK: usize = 128;
    let mut rig = rig();
    let ids: Vec<ModelId> = (0..5).map(|_| install(&mut rig, &[verb(0.9)])).collect();
    rig.start_test_signal(sine(220.0, 0.2));
    let time = |rig: &GuitarRig| {
        let t = std::time::Instant::now();
        for _ in 0..1_000 {
            rig.render_offline(BLOCK);
        }
        t.elapsed().as_secs_f64() / 1_000.0
    };
    rig.set_active(Some(ids[0]));
    let alone = time(&rig);
    for &id in &ids[1..] {
        rig.set_active(Some(id));
        rig.render_offline(BLOCK * 10);
    }
    assert_eq!(rig.tail_voices(), 4);
    let with_tails = time(&rig);
    let budget = BLOCK as f64 / f64::from(SR);
    println!(
        "block of {BLOCK}: alone {:.1} µs, with 4 tails {:.1} µs — {:.0}% of the {:.0} µs budget",
        alone * 1e6,
        with_tails * 1e6,
        with_tails / budget * 100.0,
        budget * 1e6
    );
    assert!(with_tails < budget, "four tails fit in real time");
}
