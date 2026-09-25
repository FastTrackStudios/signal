//! Settings-only edits and live state across reloads, end to end on the
//! offline rig:
//!
//! - an edit that changes only settings builds nothing and switches
//!   nothing — it is written to the running chains — and plays exactly what
//!   a chain built from the edited definition plays;
//! - an edit to a param that cannot be written live builds just that chain;
//! - a knob turned while a reload builds ends up on the committed chain;
//! - a patch's live state (macro positions, the boost, a delay's tempo) is on
//!   its rebuilt chain from its first audible sample — no step to the
//!   baseline and back.

use std::sync::Arc;

use signal_proto::block::BlockType;
use signal_sampler::{
    CommitStatus, GuitarRig, LiveWrite, ProfileRig, ReloadMode, RigBlock, RigPatch, RigProfile,
};

const SR: u32 = 48_000;

fn rig(profile: RigProfile) -> ProfileRig {
    let mut p = ProfileRig::new(GuitarRig::open_offline(SR).expect("offline rig"));
    p.set_level_match(false);
    p.load_profile(profile, None).expect("profile loads");
    p
}

fn heard(rig: &GuitarRig, secs: f64) -> Vec<f32> {
    let frames = (secs * f64::from(SR)) as usize;
    rig.arm_heard_capture(frames);
    rig.render_offline(frames);
    rig.take_output_capture().0
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|s| s * s).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

fn db(x: f32) -> f32 {
    20.0 * x.max(1e-12).log10()
}

fn max_diff(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f32::max)
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

fn silence() -> Arc<Vec<f32>> {
    Arc::new(vec![0.0; SR as usize])
}

fn gain(name: &str, db: f32) -> RigBlock {
    RigBlock::effect(BlockType::Volume, name).with_param("gain_db", db.to_string())
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

/// `block` with `param` set to `value` (in place, as an edit leaves it).
fn set(mut block: RigBlock, param: &str, value: &str) -> RigBlock {
    match block.params.iter_mut().find(|p| p.name == param) {
        Some(p) => p.value = value.to_string(),
        None => block = block.with_param(param, value),
    }
    block
}

fn patch(name: &str, blocks: Vec<RigBlock>) -> RigPatch {
    blocks.into_iter().fold(RigPatch::new(name), RigPatch::with_block)
}

fn profile(lead: Vec<RigBlock>, clean: Vec<RigBlock>) -> RigProfile {
    RigProfile::new("Live")
        .with_patch(patch("Lead", lead))
        .with_patch(patch("Clean", clean))
}

/// Play silence for `secs`, then `sig` for `play` seconds: what is heard of
/// the playing. Both rigs of a comparison render the same silence first, so
/// every time-varying part of their chains (a modulated reverb) is at the
/// same place when the signal starts.
fn play_after(rig: &GuitarRig, secs: f64, sig: &Arc<Vec<f32>>, play: f64) -> Vec<f32> {
    rig.start_test_signal(silence());
    heard(rig, secs);
    rig.start_test_signal(sig.clone());
    heard(rig, play)
}

/// A preset pick that changes only settings — the trim, the reverb's level
/// and mix on the playing patch, the delay's time on the other — builds nothing,
/// switches nothing, and plays what the edited definition built fresh
/// plays.
#[test]
fn a_settings_edit_rebuilds_nothing_and_plays_as_a_rebuild() {
    let before = || profile(vec![gain("Trim", 6.0), verb(0.8)], vec![delay(300.0, 0.4)]);
    let mut edited = before();
    edited.patches[0].chain[1] = set(set(verb(0.8), "level", "-9"), "mix", "0.7");
    edited.patches[0].chain[0] = gain("Trim", 3.0);
    edited.patches[1].chain[0] = delay(450.0, 0.4);

    let mut reloaded = rig(before());
    let reference = rig(edited.clone());
    reloaded.rig().start_test_signal(silence());
    heard(reloaded.rig(), 0.2);
    let report = reloaded.reload_profile(edited, None);
    assert_eq!(report.status, CommitStatus::Committed);
    assert_eq!(
        (report.built, report.retuned, report.reused, report.retired),
        (0, 2, 0, 0),
        "both patches retuned, nothing built"
    );
    assert!(!report.switched, "no switch");
    assert_eq!(reloaded.rig().tail_voices(), 0, "no tail: no chain left");
    heard(reloaded.rig(), 0.3);
    heard(reference.rig(), 0.5);

    let sig = sine(196.0, 0.05);
    let got = play_after(reloaded.rig(), 0.0, &sig, 1.0);
    let want = play_after(reference.rig(), 0.0, &sig, 1.0);
    let d = max_diff(&got, &want);
    println!(
        "settings-only reload: commit {} µs, 0 builds, 0 switches; vs a chain built from the \
         edit: largest difference {d:e}",
        report.commit_us
    );
    assert!(d < 1e-6, "plays as a rebuild: {d:e}");
    // And Clean, not playing, was retuned too.
    assert!(reloaded.activate_named("Clean"));
}

/// A change to a param that cannot be written live (a compressor's meter
/// channel is build-time wiring) builds just that chain, gaplessly.
#[test]
fn a_structural_param_rebuilds_just_that_chain() {
    let comp = |meter: &str| {
        RigBlock::effect(BlockType::Compressor, "Comp")
            .with_param("threshold", "-20")
            .with_param("meter", meter)
    };
    let mut prig = rig(profile(vec![comp("0"), verb(0.5)], vec![delay(300.0, 0.4)]));
    let report = prig.reload_profile(profile(vec![comp("2"), verb(0.5)], vec![delay(300.0, 0.4)]), None);
    assert_eq!((report.built, report.retuned, report.reused), (1, 0, 1));
    assert!(report.switched, "the playing chain rebuilt, switched in gaplessly");
    // A threshold alone is a setting.
    let report = prig.reload_profile(
        profile(
            vec![set(comp("2"), "threshold", "-30"), verb(0.5)],
            vec![delay(300.0, 0.4)],
        ),
        None,
    );
    assert_eq!((report.built, report.retuned), (0, 1));
    assert!(!report.switched);
}

/// A knob turned between the ticket and the commit — while the chains
/// build — is on the committed chain, whether the reload rebuilds the patch
/// or retunes it, and even when the edit changed the same param.
#[test]
fn a_knob_turned_while_the_chains_build_is_kept() {
    let base = || profile(vec![gain("Trim", 6.0), verb(0.8)], vec![delay(300.0, 0.4)]);
    let sig = sine(196.0, 0.05);
    for rebuild in [true, false] {
        // The edit moves the reverb's level; the knob moves it again.
        let mut edited = base();
        edited.patches[0].chain[1] = set(verb(0.8), "level", "-12");
        if rebuild {
            edited.patches[0].chain.push(gain("Out", 0.0));
        }
        // What the knob leaves: the edit, with the knob's level.
        let mut want_def = edited.clone();
        want_def.patches[0].chain[1] = set(verb(0.8), "level", "-2");

        let mut prig = rig(base());
        prig.rig().start_test_signal(silence());
        heard(prig.rig(), 0.2);
        let ticket = prig.begin_reload(ReloadMode::Keep);
        let prepared = ticket.plan(edited, None).prepare();
        // The knob, on the old chain, while the new one built.
        assert!(prig.set_block_param("VERB 1", "level", -2.0));
        let report = prig.commit_reload(prepared, None);
        assert_eq!(report.status, CommitStatus::Committed);
        assert_eq!(report.built, usize::from(rebuild));
        assert!(report.carried >= 1, "the late knob was carried");

        let reference = rig(want_def);
        heard(prig.rig(), 0.3);
        reference.rig().start_test_signal(silence());
        // The chains the same age when the signal starts: a rebuilt one
        // is as old as the commit, a retuned one as the rig.
        heard(reference.rig(), if rebuild { 0.3 } else { 0.5 });
        let got = play_after(prig.rig(), 0.0, &sig, 1.0);
        let want = play_after(reference.rig(), 0.0, &sig, 1.0);
        let d = max_diff(&got, &want);
        println!(
            "knob during a {} reload: largest difference from a chain built with it {d:e}",
            if rebuild { "rebuilding" } else { "retuning" }
        );
        assert!(d < 1e-6, "the knob's level is on the committed chain: {d:e}");
    }
}

/// A macro holding the playing patch's trim 6 dB under what the definition
/// says: a reload that rebuilds the patch brings the new chain in at the
/// macro'd level from its first sample — the level through the switch never
/// steps to the baseline — and the macro's value computed over the new baseline winning over
/// the logged one where the session hands one in.
#[test]
fn macro_positions_are_on_a_rebuilt_chain_from_its_first_sample() {
    let lead = |out_db: f32| vec![gain("Trim", 6.0), gain("Out", out_db)];
    let mut prig = rig(profile(lead(0.0), vec![delay(300.0, 0.4)]));
    prig.rig().start_test_signal(sine(196.0, 0.2));
    heard(prig.rig(), 0.2);
    // The macro: Trim to 0 dB (a live write, not in the definition).
    assert!(prig.set_block_param("Trim", "gain_db", 0.0));
    heard(prig.rig(), 0.1);
    let before = rms(&heard(prig.rig(), 0.1));

    // The edit restructures Lead (a block more) and leaves Trim alone.
    let mut edited = profile(lead(0.0), vec![delay(300.0, 0.4)]);
    edited.patches[0].chain.push(gain("Pad", 0.0));
    let report = prig.reload_profile(edited, None);
    assert_eq!(report.built, 1);
    assert!(report.switched);
    let around = heard(prig.rig(), 0.05);
    let win = (SR / 400) as usize;
    let (lo, hi) = around
        .chunks(win)
        .map(rms)
        .fold((f32::INFINITY, 0.0f32), |(lo, hi), r| (lo.min(r), hi.max(r)));
    let after = rms(&heard(prig.rig(), 0.1));
    println!(
        "macro'd trim through a rebuild: {:+.2} dB before, windows {:+.2}..{:+.2} dB through \
         the switch, {:+.2} dB after (the baseline is +6 dB)",
        db(before),
        db(lo),
        db(hi),
        db(after)
    );
    assert!((db(after) - db(before)).abs() < 0.05, "the new chain plays the macro'd level");
    // Through the switch the old and new chains play the same note at the
    // same level, crossfaded equal-power: coherent, they sum to at most
    // +3 dB for the 8 ms of the fade (a footswitch between two identical
    // patches does the same). The baseline would be +6 dB, and stay.
    assert!(db(hi) - db(before) < 3.1, "never up to the baseline, even for a window");
    assert!(db(before) - db(lo) < 0.5, "no dip");

    // The session's overlay: a macro over a param the edit changed — Trim's
    // baseline goes to +3 dB, the macro puts it at -2 dB over that.
    let mut edited = profile(lead(0.0), vec![delay(300.0, 0.4)]);
    edited.patches[0].chain = vec![gain("Trim", 3.0), gain("Out", 0.0), gain("Pad", 0.0), gain("Pad2", 0.0)];
    let ticket = prig.begin_reload(ReloadMode::Keep);
    let mut prepared = ticket.plan(edited, None).prepare();
    assert!(prepared.changes("Lead"));
    prepared.set_overlay("Lead", vec![("Trim".into(), LiveWrite::Param("gain_db".into(), -2.0))]);
    let report = prig.commit_reload(prepared, None);
    assert_eq!(report.built, 1);
    heard(prig.rig(), 0.1);
    let with_overlay = rms(&heard(prig.rig(), 0.1));
    assert!(
        (db(with_overlay) - (db(before) - 2.0)).abs() < 0.05,
        "the overlay's -2 dB, not the edit's +3 or the old macro's 0: {:+.2} dB vs {:+.2}",
        db(with_overlay),
        db(before) - 2.0
    );
}

/// The boost pedal's level and a delay's tempo — live writes the definition
/// does not hold — are on a rebuilt chain as they were on the old one.
#[test]
fn boost_and_tempo_are_carried_onto_a_rebuilt_chain() {
    let boost = |drive: Option<&str>| {
        let b = RigBlock::effect(BlockType::Boost, "Boost");
        match drive {
            Some(d) => b.with_param("drive", d),
            None => b,
        }
    };
    let lead = |extra: bool| {
        let mut c = vec![boost(Some("0")), delay(300.0, 0.4)];
        if extra {
            c.push(gain("Out", 0.0));
        }
        c
    };
    let mut prig = rig(profile(lead(false), vec![delay(300.0, 0.4)]));
    prig.rig().start_test_signal(silence());
    heard(prig.rig(), 0.2);
    assert!(prig.set_block_param("Boost", "drive", 0.5));
    assert!(prig.set_block_param("DLY 1", "tempo_bpm", 132.0));
    heard(prig.rig(), 0.1);
    let report = prig.reload_profile(profile(lead(true), vec![delay(300.0, 0.4)]), None);
    assert_eq!(report.built, 1);
    assert!(report.carried >= 2, "boost and tempo carried: {}", report.carried);

    // What a chain built with that boost and that tempo plays.
    let mut want = profile(lead(true), vec![delay(300.0, 0.4)]);
    want.patches[0].chain[0] = boost(Some("0.5"));
    want.patches[0].chain[1] = delay(300.0, 0.4).with_param("tempo_bpm", "132");
    let reference = rig(want);
    // Both chains the same age when the signal starts (the new one was
    // built at the commit).
    heard(prig.rig(), 0.3);
    reference.rig().start_test_signal(silence());
    heard(reference.rig(), 0.3);
    // Quiet enough that the output stage's soft ceiling (which guards the
    // sum while a tail rings) never engages.
    let sig = sine(196.0, 0.05);
    let got = play_after(prig.rig(), 0.0, &sig, 1.0);
    let want = play_after(reference.rig(), 0.0, &sig, 1.0);
    let d = max_diff(&got, &want);
    println!("boost + tempo after a rebuild: largest difference from a chain built with them {d:e}");
    assert!(d < 1e-6, "boost and tempo carried: {d:e}");
}
