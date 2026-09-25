//! Gapless reloads, end to end: a [`ProfileRig`] on the offline rig — the
//! same project, slots, renderer and output stage the live rig runs —
//! reloaded mid-render the way an edit reloads it (a module or block preset
//! picked, a macro range saved), with what is *heard* checked.
//!
//! What each test pins:
//! - an edit to the playing patch's reverb crossfades in with no dropout
//!   and no click beyond the switch's own, and the old reverb's tail rings
//!   on exactly as if nothing had reloaded;
//! - an edit to a patch that is not playing leaves the output bit-identical
//!   (its chain untouched, nothing switched);
//! - the playing patch, stack cursors, a song's rotations and no-rotate
//!   flags survive a reload;
//! - only the newest reload commits;
//! - the locked part of a reload is short, the build runs off the lock, and
//!   a footswitch pressed during the build switches at once.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use signal_proto::block::BlockType;
use signal_sampler::{
    CommitStatus, GuitarRig, ProfileRig, ReloadMode, RigBlock, RigPatch, RigProfile,
};
use signal_sampler::rig_profile::RigStack;

const SR: u32 = 48_000;

fn rig(profile: RigProfile) -> ProfileRig {
    let mut p = ProfileRig::new(GuitarRig::open_offline(SR).expect("offline rig"));
    p.set_level_match(false);
    p.load_profile(profile, None).expect("profile loads");
    p
}

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

fn error_db(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    let err: Vec<f32> = (0..n).map(|i| a[i] - b[i]).collect();
    20.0 * (rms(&err).max(1e-12) / rms(&b[..n]).max(1e-12)).log10()
}

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

fn patch(name: &str, blocks: Vec<RigBlock>) -> RigPatch {
    blocks.into_iter().fold(RigPatch::new(name), RigPatch::with_block)
}

/// "Lead" (a long reverb, playing) and "Clean" (a delay).
fn two_patch(lead_decay: f32, clean_ms: f32) -> RigProfile {
    RigProfile::new("Worship")
        .with_patch(patch("Lead", lead(lead_decay)))
        .with_patch(patch("Clean", vec![gain(0.0), delay(clean_ms, 0.4)]))
}

fn lead(decay: f32) -> Vec<RigBlock> {
    vec![gain(6.0), verb(decay)]
}

/// Lead with a block more — a change no param write can make, so its chain
/// is built again.
fn lead_restructured(decay: f32) -> Vec<RigBlock> {
    vec![gain(6.0), verb(decay), gain(0.0).named("Out")]
}

/// [`two_patch`] with Lead restructured.
fn two_patch_rebuilt(lead_decay: f32, clean_ms: f32) -> RigProfile {
    let mut p = two_patch(lead_decay, clean_ms);
    p.patches[0].chain = lead_restructured(lead_decay);
    p
}

/// The playing patch's reverb preset changes mid-phrase: a sustained note
/// carries on through the commit with no window dropping out, and what is
/// heard is — sample for sample — what a footswitch between the old and the
/// new chain plays: the same 8 ms equal-power crossfade of the dry path,
/// the same old tail ringing on, the new reverb's input ramped in. No
/// sample step beyond the note's own.
#[test]
fn an_edit_to_the_playing_patch_does_not_drop_out() {
    let mut prig = rig(two_patch(0.8, 300.0));
    // The footswitch it must match: both chains installed, then a switch.
    let mut footswitch =
        rig(two_patch(0.8, 300.0).with_patch(patch("Lead'", lead_restructured(0.4))));
    for r in [&prig, &footswitch] {
        r.rig().start_test_signal(sine(110.0, 0.2));
        heard(r.rig(), 0.8);
    }
    let before = heard(prig.rig(), 0.2);
    heard(footswitch.rig(), 0.2);

    let report = prig.reload_profile(two_patch_rebuilt(0.4, 300.0), None);
    assert!(footswitch.activate_named("Lead'"));
    assert_eq!(report.status, CommitStatus::Committed);
    assert_eq!((report.built, report.reused), (1, 1), "only Lead rebuilds");
    assert!(report.switched, "Lead's new chain switched in");
    assert_eq!(prig.active_patch().map(|p| p.name.as_str()), Some("Lead"));
    assert_eq!(prig.rig().tail_voices(), 1, "the old reverb rings on as a tail");

    let around = heard(prig.rig(), 0.1);
    let around_fs = heard(footswitch.rig(), 0.1);
    heard(prig.rig(), 0.5);
    let after = heard(prig.rig(), 0.2);

    // No dropout: every 2.5 ms window through the commit holds its level.
    let win = (SR / 400) as usize;
    let steady = rms(&before).min(rms(&after));
    let quietest = around.chunks(win).map(rms).fold(f32::INFINITY, f32::min);
    // No click beyond a footswitch's: the largest sample step, against the
    // note's own and against the footswitch's through the same moment.
    let steady_step = max_step(&before).max(max_step(&after));
    let at_commit = max_step(&around);
    let at_footswitch = max_step(&around_fs);
    let vs_footswitch = error_db(&around, &around_fs);
    println!(
        "reload of the playing patch: build {:.2} ms (off-lock), commit {} µs; \
         quietest 2.5 ms window {:.4} vs steady {:.4} ({:+.1} dB); \
         largest step {:.5} (footswitch {:.5}, steady note {:.5}); \
         reload vs footswitch {vs_footswitch:.1} dB",
        report.build_ms,
        report.commit_us,
        quietest,
        steady,
        20.0 * (quietest / steady).log10(),
        at_commit,
        at_footswitch,
        steady_step,
    );
    assert!(quietest > steady * 0.7, "a window dropped out: {quietest} vs {steady}");
    assert!(
        at_commit < steady_step * 1.3,
        "largest step at the commit {at_commit} vs the note's own {steady_step}"
    );
    assert!(
        at_commit <= at_footswitch * 1.01,
        "largest step at the commit {at_commit} vs a footswitch's {at_footswitch}"
    );
    assert!(vs_footswitch < -100.0, "a reload is a footswitch: {vs_footswitch:.1} dB");
}

/// After the reload the old reverb's tail rings on sample for sample as it
/// would have with no reload at all (the new chain, fed silence, adds
/// nothing).
#[test]
fn the_old_tail_rings_on_through_a_reload() {
    let mut reloaded = rig(two_patch(0.7, 300.0));
    let reference = rig(two_patch(0.7, 300.0));
    for r in [&reloaded, &reference] {
        r.rig().start_test_signal(burst());
        heard(r.rig(), 0.3);
    }
    let report = reloaded.reload_profile(two_patch_rebuilt(0.3, 300.0), None);
    assert!(report.switched);
    let got = heard(reloaded.rig(), 1.5);
    let want = heard(reference.rig(), 1.5);
    let skip = SR as usize / 50; // past the 8 ms crossfade
    assert!(rms(&want[skip..]) > 1e-3, "the reference rings");
    let e = error_db(&got[skip..], &want[skip..]);
    println!("old tail after a reload vs never reloaded: error {e:.1} dB");
    assert!(e < -60.0, "the tail is the unreloaded tail: error {e:.1} dB");
}

/// An edit to a patch that is not playing: the output does not change by a
/// single bit through the commit — the playing chain is the same chain.
#[test]
fn an_edit_to_another_patch_leaves_the_output_untouched() {
    let mut reloaded = rig(two_patch(0.7, 300.0));
    let reference = rig(two_patch(0.7, 300.0));
    for r in [&reloaded, &reference] {
        r.rig().start_test_signal(sine(196.0, 0.2));
        heard(r.rig(), 0.5);
    }
    let a0 = heard(reloaded.rig(), 0.1);
    let b0 = heard(reference.rig(), 0.1);
    // A structural edit to Clean (rebuilt), then a settings-only one
    // (retuned): neither touches what plays.
    let mut restructured = two_patch(0.7, 300.0);
    restructured.patches[1].chain.push(gain(0.0).named("Out"));
    let report = reloaded.reload_profile(restructured.clone(), None);
    assert_eq!((report.built, report.reused, report.retired), (1, 1, 1));
    assert!(!report.switched, "nothing playing changed");
    let a_mid = heard(reloaded.rig(), 0.1);
    let b_mid = heard(reference.rig(), 0.1);
    restructured.patches[1].chain[1] = delay(450.0, 0.4);
    let report = reloaded.reload_profile(restructured, None);
    assert_eq!((report.built, report.retuned, report.retired), (0, 1, 0));
    assert!(!report.switched, "nothing playing changed");
    assert_eq!(reloaded.rig().tail_voices(), 0);
    let a1 = heard(reloaded.rig(), 0.5);
    let b1 = heard(reference.rig(), 0.5);
    let diff = a0
        .iter()
        .chain(&a_mid)
        .chain(&a1)
        .zip(b0.iter().chain(&b_mid).chain(&b1))
        .map(|(x, y)| (x - y).abs())
        .fold(0.0f32, f32::max);
    println!("non-active reload: largest difference from the unreloaded rig {diff:e}");
    assert!(diff == 0.0, "bit-identical through the commit: {diff:e}");
    // And the edited patch plays its new chain.
    assert!(reloaded.activate_named("Clean"));
}

fn stacked(lead_decay: f32) -> RigProfile {
    let mut p = RigProfile::new("Stacks")
        .with_patch(patch("Clean", vec![gain(0.0)]))
        .with_patch(patch("Crunch", vec![gain(3.0)]))
        .with_patch(patch("Lead", vec![gain(6.0), verb(lead_decay)]))
        .with_patch(patch("Ambient", vec![gain(-3.0), verb(0.9)]));
    p.stacks = vec![
        RigStack::new("A", ["Clean", "Crunch", "Lead"]),
        RigStack::new("B", ["Ambient"]),
    ];
    p
}

/// The switcher's state survives a reload that rebuilds the playing patch:
/// the patch, every stack's cursor, a song's rotation and no-rotate flags.
#[test]
fn the_switcher_state_survives_a_reload() {
    let mut prig = rig(stacked(0.6));
    // Clean plays (the default); stack A pressed twice: → Crunch → Lead.
    for _ in 0..2 {
        assert!(prig.activate_stack(0));
    }
    assert_eq!(prig.active_patch().unwrap().name, "Lead");
    assert_eq!(prig.stack_position(0), 2);
    // A song tunes B to rotate Ambient → Crunch, sat on Crunch; B latches.
    prig.retune_stacks(&[("B".into(), vec!["Ambient".into(), "Crunch".into()])]);
    assert!(prig.point_stack_at("B", "Crunch"));
    prig.set_no_rotate(&[false, true]);

    // Edit Lead's reverb level (the playing patch; a setting) and reorder
    // the patches.
    let mut edited = stacked(0.6);
    for p in &mut edited.patches[2].chain[1].params {
        if p.name == "level" {
            p.value = "-9".into();
        }
    }
    edited.patches.rotate_left(1);
    let report = prig.reload_profile(edited, None);
    assert_eq!(report.status, CommitStatus::Committed);
    assert_eq!((report.built, report.retuned, report.reused), (0, 1, 3), "a settings edit");
    // And a structural one (Lead's decay rebuilds its reverb), same checks.
    let mut edited = stacked(0.3);
    edited.patches.rotate_left(1);
    let report = prig.reload_profile(edited, None);
    assert_eq!((report.built, report.reused), (1, 3), "Lead rebuilt");

    assert_eq!(prig.active_patch().unwrap().name, "Lead", "the playing patch plays on");
    assert_eq!(prig.stack_position(0), 2, "A's cursor on Lead");
    assert_eq!(prig.active_stack(), Some(0));
    assert_eq!(prig.stacks()[1].patches, ["Ambient", "Crunch"], "the song's rotation");
    assert_eq!(prig.stack_position(1), 1, "B's cursor on Crunch");
    // The session re-applies the song's tuning after a reload: the same
    // rotation, so no switch moves.
    prig.retune_stacks(&[("B".into(), vec!["Ambient".into(), "Crunch".into()])]);
    assert_eq!(prig.stack_position(1), 1, "re-tuning the same rotation keeps B's cursor");
    // B latches: pressing it lands on Crunch and stays there.
    assert!(prig.activate_stack(1));
    assert!(prig.activate_stack(1));
    assert_eq!(prig.active_patch().unwrap().name, "Crunch", "B still does not rotate");
    // The song goes: the profile's own rotation comes back.
    prig.retune_stacks(&[]);
    assert_eq!(prig.stacks()[1].patches, ["Ambient"]);
}

/// Two reloads racing: the one that began last wins; the other's chains
/// are handed back and nothing changes.
#[test]
fn only_the_newest_reload_commits() {
    let mut prig = rig(two_patch(0.6, 300.0));
    let first = prig.begin_reload(ReloadMode::Keep).plan(two_patch(0.2, 300.0), None);
    let second = prig.begin_reload(ReloadMode::Keep).plan(two_patch(0.4, 300.0), None);
    let (first, second) = (first.prepare(), second.prepare());
    let late = prig.commit_reload(second, None);
    assert_eq!(late.status, CommitStatus::Committed);
    let stale = prig.commit_reload(first, None);
    assert_eq!(stale.status, CommitStatus::Stale, "the older reload is discarded");
    assert_eq!(prig.rig().slots().len(), 2, "no chain from the stale reload installed");
    let decay = prig.active_patch().unwrap().chain[1].param_str("decay");
    assert_eq!(decay.as_deref(), Some("0.4"));
}

/// A profile *switch* (another profile) is gapless too: the landing patch
/// crossfades in and the old patch's tail rings out, no dropout.
#[test]
fn a_profile_switch_does_not_go_silent() {
    let mut prig = rig(two_patch(0.8, 300.0));
    prig.rig().start_test_signal(sine(110.0, 0.2));
    heard(prig.rig(), 0.5);
    let before = heard(prig.rig(), 0.2);
    let other = RigProfile::new("Other")
        .with_patch(patch("Warm", vec![gain(6.0), delay(250.0, 0.3)]));
    prig.load_profile(other, None).expect("loads");
    assert_eq!(prig.active_patch().unwrap().name, "Warm");
    assert_eq!(prig.rig().tail_voices(), 1, "the old patch rings out");
    let around = heard(prig.rig(), 0.1);
    let win = (SR / 400) as usize;
    let quietest = around.chunks(win).map(rms).fold(f32::INFINITY, f32::min);
    assert!(quietest > rms(&before) * 0.5, "no silence at the switch: {quietest}");
    assert_eq!(prig.rig().slots().len(), 1, "the old profile's chains retired");
}

/// The locked part of a one-patch reload is short; the build runs off the
/// lock; a footswitch pressed while it builds switches at once, and the
/// commit maps what it chose by name.
#[test]
fn a_footswitch_during_the_build_is_not_held_up() {
    // Enough patches that building them all takes a while.
    const PATCHES: usize = 48;
    let big = |d: f32| {
        let mut p = RigProfile::new("Big");
        for i in 0..PATCHES {
            let mut verb2 = verb(d);
            verb2.name = "VERB 2".into();
            p.patches.push(patch(
                &format!("P{i}"),
                vec![gain(0.0), delay(200.0 + i as f32, 0.3), verb(d), verb2],
            ));
        }
        p
    };
    // Every patch restructured: the build is the whole profile.
    let big_rebuilt = |d: f32| {
        let mut p = big(d);
        for q in &mut p.patches {
            q.chain.push(gain(0.0).named("Out"));
        }
        p
    };
    let shared = Mutex::new(rig(big(0.5)));

    // Every patch changes, so the build is the whole profile.
    let t = Instant::now();
    let ticket = shared.lock().unwrap().begin_reload(ReloadMode::Keep);
    let ticket_us = t.elapsed().as_micros();
    let done = std::sync::atomic::AtomicBool::new(false);
    let mut presses = Vec::new();
    let prepared = std::thread::scope(|s| {
        let presser = s.spawn(|| {
            let (mut worst_wait, mut worst) = (Duration::ZERO, Duration::ZERO);
            let mut k = 0;
            while !done.load(std::sync::atomic::Ordering::Relaxed) {
                let name = format!("P{}", k % PATCHES);
                let t = Instant::now();
                let mut prig = shared.lock().unwrap();
                worst_wait = worst_wait.max(t.elapsed());
                assert!(prig.activate_named(&name));
                worst = worst.max(t.elapsed());
                assert_eq!(prig.active_patch().unwrap().name, name, "switched at once");
                drop(prig);
                k += 1;
                std::thread::sleep(Duration::from_millis(1));
            }
            (worst_wait, worst, k)
        });
        let prepared = ticket.plan(big_rebuilt(0.3), None).prepare();
        done.store(true, std::sync::atomic::Ordering::Relaxed);
        presses.push(presser.join().unwrap());
        prepared
    });
    let (worst_wait, worst_press, n_presses) = presses[0];
    // The last press chose a patch; the commit must keep it.
    let chosen = shared.lock().unwrap().active_patch().unwrap().name.clone();
    let t = Instant::now();
    let report = shared.lock().unwrap().commit_reload(prepared, None);
    let full_commit = t.elapsed();
    assert_eq!(report.status, CommitStatus::Committed);
    assert_eq!(report.built, PATCHES);
    let prig = shared.into_inner().unwrap();
    assert_eq!(prig.active_patch().unwrap().name, chosen, "the footswitch's pick survives");

    // One patch changed: the common edit.
    let mut prig = prig;
    let mut one = big_rebuilt(0.3);
    one.patches[0].chain.push(gain(-1.0).named("Out 2"));
    prig.activate_named("P0");
    let one_patch = prig.reload_profile(one, None);
    assert_eq!((one_patch.built, one_patch.reused), (1, PATCHES - 1));
    println!(
        "{PATCHES} patches: ticket {ticket_us} µs; full build {:.1} ms off-lock with \
         {n_presses} footswitches pressed meanwhile: the longest wait for the lock {:.3} ms, \
         the slowest press {:.2} ms (lock + switch, CPU shared with the build); full commit \
         {:.2} ms; one-patch build {:.1} ms, commit {} µs",
        report.build_ms,
        worst_wait.as_secs_f64() * 1e3,
        worst_press.as_secs_f64() * 1e3,
        full_commit.as_secs_f64() * 1e3,
        one_patch.build_ms,
        one_patch.commit_us
    );
    assert!(n_presses > 0, "a footswitch was pressed during the build");
    // The build holds no lock: a press never waits for one.
    assert!(
        worst_wait < Duration::from_millis(5),
        "a footswitch waited on the build: {worst_wait:?}"
    );
    assert!(
        one_patch.commit_us < 5_000,
        "a one-patch commit is short: {} µs",
        one_patch.commit_us
    );
}
