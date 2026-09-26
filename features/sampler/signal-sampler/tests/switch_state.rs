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

// ── The reconciler, and random sequences ──────────────────────────────────

/// Nothing the rig does leaves the playing chain off what it is due to play
/// — so the reconciler, run after each step, finds nothing to correct. And a
/// write that goes around the patch rig (no live state recorded) is found
/// and put back.
#[test]
fn the_reconciler_corrects_a_stray_write_and_nothing_else() {
    use signal_sampler::ReloadMode;
    let mut p = rig(profile(delay(300.0, 0.3), delay(500.0, 0.45)));
    p.activate(0);
    assert!(p.reconcile().is_empty(), "a fresh chain is as built");
    let id = block_id(&p, "DLY 1");
    p.set_block_param(&id, "feedback", 0.6);
    p.set_block_param(&id, "tempo_bpm", 120.0);
    assert!(p.reconcile().is_empty(), "a knob through the patch rig is its live state");
    p.activate(1);
    assert!(p.reconcile().is_empty());
    p.activate(0);
    assert!(p.reconcile().is_empty(), "switching back finds it as it was left");
    let ticket = p.begin_reload(ReloadMode::Keep);
    let prepared = ticket.plan(profile(delay(250.0, 0.3), delay(500.0, 0.45)), None).prepare();
    p.commit_reload(prepared, None);
    assert!(p.reconcile().is_empty(), "a retune lands the chain where it is due");

    // Around the patch rig: the engine plays it, nothing records it.
    let id = block_id(&p, "DLY 1");
    assert!(p.rig().set_active_block_param(&id, "feedback", 0.95));
    let fixed = p.reconcile();
    assert_eq!(fixed.len(), 1, "the stray write is found: {fixed:?}");
    assert!(p.reconcile().is_empty(), "and corrected");
}

/// A small deterministic generator (no dependency, same sequence every run).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as u32
    }
    fn below(&mut self, n: usize) -> usize {
        self.next() as usize % n.max(1)
    }
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * (self.next() as f32 / u32::MAX as f32)
    }
}

/// A random live value for a param of the test chain (rounded, as a knob's
/// text round-trips).
fn random_write(g: &mut Lcg) -> (&'static str, &'static str, f32) {
    let v = |x: f32| (x * 100.0).round() / 100.0;
    match g.below(5) {
        0 => ("DLY 1", "time", v(g.range(60.0, 700.0))),
        1 => ("DLY 1", "feedback", v(g.range(0.0, 0.7))),
        2 => ("DLY 1", "level", v(g.range(-12.0, 0.0))),
        3 => ("Trim", "gain_db", v(g.range(-9.0, 3.0))),
        _ => ("Out", "gain_db", v(g.range(-6.0, 0.0))),
    }
}

fn random_patch(g: &mut Lcg, name: &str, extra: bool) -> RigPatch {
    let mut p = RigPatch::new(name)
        .with_block(RigBlock::effect(BlockType::Volume, "Trim").with_param("gain_db", g.range(-6.0, 0.0).round().to_string()))
        .with_block(delay((g.range(100.0, 600.0)).round(), (g.range(0.1, 0.6) * 10.0).round() / 10.0));
    if extra {
        p = p.with_block(RigBlock::effect(BlockType::Volume, "Pad").with_param("gain_db", "-1"));
    }
    p.with_block(RigBlock::effect(BlockType::Volume, "Out").with_param("gain_db", "0"))
}

/// The patch as it should play: its definition with its live state baked in.
fn resolved(patch: &RigPatch, live: &[(String, signal_sampler::LiveWrite)], ids: &[String]) -> RigPatch {
    let mut out = patch.clone();
    let reals: Vec<usize> = out.chain.iter().enumerate().filter(|(_, b)| b.has_backend()).map(|(i, _)| i).collect();
    for (id, w) in live {
        let signal_sampler::LiveWrite::Param(name, v) = w else { continue };
        let Some(pos) = ids.iter().position(|i| i == id) else { continue };
        let Some(&at) = reals.get(pos) else { continue };
        let b = &mut out.chain[at];
        let text = v.to_string();
        match b.params.iter_mut().find(|p| p.name == *name) {
            Some(p) => p.value = text,
            None => *b = b.clone().with_param(name, text),
        }
    }
    out
}

/// Random switches, knob moves, tempo writes, settings edits and rebuilds.
/// After every step the chain is where it is due (the reconciler finds
/// nothing); at the end, the playing patch sounds as a rig built from its
/// definition plus its live state.
#[test]
fn random_sequences_never_leave_a_patch_off_its_state() {
    use signal_sampler::ReloadMode;
    for seed in 1..=6u64 {
        let mut g = Lcg(seed);
        let n = 3;
        let mut extra = vec![false; n];
        let mut prof = RigProfile::new("Live");
        for i in 0..n {
            prof = prof.with_patch(random_patch(&mut g, &format!("P{i}"), false));
        }
        let mut p = rig(prof.clone());
        p.activate(0);
        let mut active = 0;
        for step in 0..40 {
            match g.below(10) {
                0..=2 => {
                    active = g.below(n);
                    p.activate(active);
                }
                3..=6 => {
                    let (block, param, v) = random_write(&mut g);
                    let id = block_id(&p, block);
                    p.set_block_param(&id, param, v);
                }
                7 => {
                    let id = block_id(&p, "DLY 1");
                    p.set_block_param(&id, "tempo_bpm", g.range(70.0, 160.0).round());
                }
                8 => {
                    // A preset pick: one patch's delay settings change.
                    let i = g.below(n);
                    let c = prof.patches[i].chain.iter().position(|b| b.name == "DLY 1").unwrap();
                    let t = g.range(100.0, 600.0).round().to_string();
                    let b = prof.patches[i].chain[c].clone();
                    prof.patches[i].chain[c] = set_param(b, "time", &t);
                    let ticket = p.begin_reload(ReloadMode::Keep);
                    let prepared = ticket.plan(prof.clone(), None).prepare();
                    p.commit_reload(prepared, None);
                }
                _ => {
                    // A structural edit: a block added or taken away.
                    let i = g.below(n);
                    extra[i] = !extra[i];
                    let keep = prof.patches[i].clone();
                    let mut fresh = random_patch(&mut g, &keep.name, extra[i]);
                    // Same settings as before for the blocks both have.
                    for b in &mut fresh.chain {
                        if let Some(old) = keep.chain.iter().find(|o| o.name == b.name) {
                            *b = old.clone();
                        }
                    }
                    prof.patches[i] = fresh;
                    let ticket = p.begin_reload(ReloadMode::Keep);
                    let prepared = ticket.plan(prof.clone(), None).prepare();
                    p.commit_reload(prepared, None);
                    p.activate(active);
                }
            }
            let fixed = p.reconcile();
            assert!(fixed.is_empty(), "seed {seed} step {step}: the chain drifted: {fixed:?}");
        }

        // The playing patch against one built as it should be.
        let playing = p.active_patch().expect("plays").clone();
        let want_patch = resolved(&playing, &p.live_state(), &p.active_block_ids());
        let got = pluck_after_silence(p.rig());
        let mut want_rig = rig(RigProfile::new("Want").with_patch(want_patch));
        want_rig.activate(0);
        let want = pluck_after_silence(want_rig.rig());
        let d = max_diff(&got, &want);
        assert!(d < 2e-3, "seed {seed}: {} plays off its state (max diff {d})", playing.name);
    }
}

/// `block` with `param` set (in place, as an edit leaves it).
fn set_param(mut block: RigBlock, param: &str, value: &str) -> RigBlock {
    match block.params.iter_mut().find(|p| p.name == param) {
        Some(p) => p.value = value.to_string(),
        None => block = block.with_param(param, value),
    }
    block
}
