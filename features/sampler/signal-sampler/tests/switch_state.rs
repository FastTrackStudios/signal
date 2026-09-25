//! What a patch plays after a switch is its own definition plus what was
//! dialled on *it* — never something dialled on another patch.
//!
//! The rig keeps every patch's chain resident and switches between them, so
//! the patches share the track's FX slots. A knob value parked on a slot (as
//! daw's `FxParams::set` stores it, re-sent to the slot every block) reached
//! whatever chain played there next: after turning a knob on one patch's
//! delay, the next patch's delay played that value until its own knob was
//! moved — while the chain view, read from the definition, showed the right
//! one. These pin it down end to end on the offline rig:
//!
//! - a patch switched to after a knob moved on another sounds exactly as a
//!   rig that only ever played it;
//! - a patch switched back to keeps the knob moved on it.

use std::sync::Arc;

use signal_proto::block::BlockType;
use signal_sampler::{GuitarRig, ProfileRig, RigBlock, RigPatch, RigProfile};

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

fn max_diff(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f32::max)
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|s| s * s).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

fn pluck() -> Arc<Vec<f32>> {
    // A short burst then silence: the delay's repeats are what is heard.
    let n = SR as usize * 2;
    Arc::new(
        (0..n)
            .map(|i| if i < SR as usize / 20 { 0.3 * (std::f32::consts::TAU * 220.0 * i as f32 / SR as f32).sin() } else { 0.0 })
            .collect(),
    )
}

fn silence() -> Arc<Vec<f32>> {
    Arc::new(vec![0.0; SR as usize])
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

fn trim() -> RigBlock {
    RigBlock::effect(BlockType::Volume, "Trim").with_param("gain_db", "0")
}

fn profile(lead_delay: RigBlock, clean_delay: RigBlock) -> RigProfile {
    RigProfile::new("Live")
        .with_patch(RigPatch::new("Lead").with_block(trim()).with_block(lead_delay))
        .with_patch(RigPatch::new("Clean").with_block(trim()).with_block(clean_delay))
}

/// Silence long enough for any tail to die, then a pluck through the patch.
fn pluck_after_silence(rig: &GuitarRig) -> Vec<f32> {
    rig.start_test_signal(silence());
    heard(rig, 6.0);
    rig.start_test_signal(pluck());
    heard(rig, 1.5)
}

/// The id of the playing patch's block named `name`.
fn block_id(p: &ProfileRig, name: &str) -> String {
    let patch = p.active_patch().expect("a patch plays").clone();
    let reals: Vec<&RigBlock> = patch.chain.iter().filter(|b| b.has_backend()).collect();
    let ids = p.active_block_ids();
    reals
        .iter()
        .zip(ids)
        .find(|(b, _)| b.name == name)
        .map(|(_, id)| id)
        .expect("block on the playing patch")
}

#[test]
fn a_knob_on_one_patch_never_reaches_the_next() {
    let make = || profile(delay(300.0, 0.3), delay(500.0, 0.45));

    // Dial Lead's delay somewhere else entirely, then go to Clean.
    let mut p = rig(make());
    p.activate(0);
    heard(p.rig(), 0.2);
    let id = block_id(&p, "DLY 1");
    assert!(p.set_block_param(&id, "time", 120.0), "the knob reaches Lead's delay");
    assert!(p.set_block_param(&id, "feedback", 0.8));
    heard(p.rig(), 0.5);
    p.activate(1);
    let after = pluck_after_silence(p.rig());

    // A rig that only ever played Clean.
    let mut fresh = rig(make());
    fresh.activate(1);
    heard(fresh.rig(), 0.2);
    heard(fresh.rig(), 0.5);
    let want = pluck_after_silence(fresh.rig());

    assert!(rms(&want) > 1e-3, "the reference is heard");
    let d = max_diff(&after, &want);
    assert!(d < 1e-4, "Clean plays Lead's knob after the switch (max diff {d})");
}

#[test]
fn a_knob_stays_on_its_own_patch_across_switches() {
    // Lead as dialled: time 120 ms, feedback 0.5.
    let mut p = rig(profile(delay(300.0, 0.3), delay(500.0, 0.45)));
    p.activate(0);
    heard(p.rig(), 0.2);
    let id = block_id(&p, "DLY 1");
    p.set_block_param(&id, "time", 120.0);
    p.set_block_param(&id, "feedback", 0.5);
    heard(p.rig(), 0.5);
    p.activate(1);
    heard(p.rig(), 0.5);
    p.activate(0);
    let back = pluck_after_silence(p.rig());

    // A rig built with Lead already at those values.
    let mut want_rig = rig(profile(delay(120.0, 0.5), delay(500.0, 0.45)));
    want_rig.activate(0);
    heard(want_rig.rig(), 0.2);
    heard(want_rig.rig(), 0.5);
    let want = pluck_after_silence(want_rig.rig());

    let d = max_diff(&back, &want);
    assert!(d < 1e-3, "Lead lost the knob moved on it (max diff {d})");
}

/// What the view is told the playing chain holds (`live_state`) is what it
/// plays: the knob moved on it, kept across a switch away and back, and
/// dropped once a preset pick sets that param in its definition — where the
/// new value plays, not the old knob.
#[test]
fn the_live_state_is_what_the_chain_plays() {
    use signal_sampler::{LiveWrite, ReloadMode};
    let mut p = rig(profile(delay(300.0, 0.3), delay(500.0, 0.45)));
    p.activate(0);
    let id = block_id(&p, "DLY 1");
    p.set_block_param(&id, "feedback", 0.6);
    assert!(
        p.live_state().contains(&(id.clone(), LiveWrite::Param("feedback".into(), 0.6))),
        "the knob is in the chain's live state"
    );
    p.activate(1);
    assert!(
        !p.live_state().iter().any(|(_, w)| *w == LiveWrite::Param("feedback".into(), 0.6)),
        "another patch's chain does not carry it"
    );
    p.activate(0);
    assert!(p.live_state().contains(&(id.clone(), LiveWrite::Param("feedback".into(), 0.6))));

    // A preset pick sets Lead's delay feedback to 0.2 in its definition.
    let ticket = p.begin_reload(ReloadMode::Keep);
    let prepared = ticket.plan(profile(delay(300.0, 0.2), delay(500.0, 0.45)), None).prepare();
    p.commit_reload(prepared, None);
    assert!(
        !p.live_state().iter().any(|(_, w)| matches!(w, LiveWrite::Param(n, _) if n == "feedback")),
        "the old knob does not outlive the preset that set the param"
    );
    let now = pluck_after_silence(p.rig());
    let mut want_rig = rig(profile(delay(300.0, 0.2), delay(500.0, 0.45)));
    want_rig.activate(0);
    let want = pluck_after_silence(want_rig.rig());
    let d = max_diff(&now, &want);
    assert!(d < 1e-3, "the preset's value plays, not the old knob (max diff {d})");
}
