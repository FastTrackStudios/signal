//! What a patch sounds like at the rig's output — measured by the rig.
//!
//! There is one way a patch becomes sound: its chain is installed on the
//! [`GuitarRig`] track, the patch is activated, and switching it in applies
//! the drive compensation. The live rig does that against an audio device;
//! everything here does it against [`GuitarRig::open_offline`] — the same
//! project, track, slots, chain installer, block builder and renderer, only
//! pulled as fast as the CPU allows. Levelling used to render chains with a
//! runner of its own, which drifted from the live one by up to 18 dB (a
//! Diezel through a V30 levelled to target offline played far louder than
//! everything else live); there is nothing left here for the two to disagree
//! about.

use std::sync::Arc;

use signal_sampler::rig::GuitarRig;
use signal_sampler::rig_profile::{ProfileRig, RigPatch, RigProfile};

/// Name of this renderer in the levelling cache — a measurement from a
/// different renderer is a different measurement. Bump it when what happens
/// between "activate" and "measure" changes.
// v4: delays and reverbs run in parallel with the dry, and build with every
// param (algorithm, style, …) — same chains, different sound.
const ENGINE: &str = "rig-v4";

/// Settle time before a measurement: long enough for the compressors, gate
/// and NAM state from the previous input to have gone, and for the delays and
/// reverbs to be carrying this signal rather than silence.
const WARM_UP_SECS: f64 = 1.0;

/// Switch-in, step 1: bypass every block the active patch's chain marks
/// bypassed. The chain installer does not — a switch does, and a measurement
/// that skipped this rendered the patch with its tremolo, chorus and spare
/// drives all processing (Ambient measured 18 dB off). Both the live switch
/// (`GuitarRigBackend::resync_blocks`) and the offline measurement call this.
pub fn apply_chain_bypass(prig: &ProfileRig) {
    let Some(patch) = prig.active_patch() else {
        return;
    };
    let ids = prig.active_block_ids();
    for (block, id) in patch.chain.iter().filter(|b| b.has_backend()).zip(ids.iter()) {
        if block.bypassed {
            prig.rig().set_block_slot_bypass(id, true);
        }
    }
}

/// Integrated loudness (LUFS) of `patch` at the rig's output, playing the DI
/// reference — cached per chain, measured through an offline rig on a miss.
#[must_use]
pub fn patch_lufs(patch: &RigPatch, sample_rate: u32) -> Option<f32> {
    signal_sampler::patch_level::level_cached(&patch.chain, sample_rate, ENGINE, |di| {
        render(patch, sample_rate, di)
    })
    .filter(|l| l.is_finite())
    .map(|l| l as f32)
}

/// Load `patch` alone on an offline rig, switch it in exactly as the live rig
/// does, play the DI and measure what comes out.
fn render(
    patch: &RigPatch,
    sample_rate: u32,
    di: &signal_sampler::nam_calibrate::DiReference,
) -> Option<f64> {
    let rig = GuitarRig::open_offline(sample_rate)
        .map_err(|e| tracing::warn!(error = %e, "patch level: no offline rig"))
        .ok()?;
    let mut prig = ProfileRig::new(rig);
    // As the live rig opens it (session.rs `open_blocking`): one loudness
    // authority, the per-block calibration — no patch-level match on top.
    prig.set_level_match(false);
    let mut profile = RigProfile::new("level");
    profile.patches.push(patch.clone());
    if let Err(e) = prig.load_profile(profile, None) {
        tracing::warn!(patch = %patch.name, error = %e, "patch level: chain did not build");
        return None;
    }
    if !prig.activate(0) {
        return None;
    }
    apply_chain_bypass(&prig);

    let samples: Arc<Vec<f32>> = Arc::new(di.samples.iter().map(|&s| s as f32).collect());
    let frames = samples.len();
    let rig = prig.rig();
    rig.start_test_signal(samples.clone());
    rig.render_offline((WARM_UP_SECS * f64::from(sample_rate)) as usize);
    // From the top, so exactly one pass of the DI is measured.
    rig.start_test_signal(samples);
    let (l, r) = rig.measure_output(frames, std::time::Duration::ZERO);
    if l.len() < frames {
        tracing::warn!(patch = %patch.name, got = l.len(), frames, "patch level: short capture");
        return None;
    }
    let mixed: Vec<f64> = l
        .iter()
        .zip(&r)
        .map(|(&a, &b)| f64::midpoint(f64::from(a), f64::from(b)))
        .collect();
    let lufs = signal_sampler::loudness::integrated_lufs(&mixed, f64::from(sample_rate));
    let peak = l.iter().chain(&r).fold(0.0f32, |m, s| m.max(s.abs()));
    tracing::info!(
        patch = %patch.name,
        lufs,
        peak_dbfs = 20.0 * peak.max(1e-9).log10(),
        "patch level: measured on the rig"
    );
    Some(lufs)
}
