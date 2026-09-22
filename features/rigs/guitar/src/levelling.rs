//! Patch levelling without a running rig.
//!
//! The rig's own pass (`Rig::level_patches`) measures the installed chains.
//! This one builds them from the profile on disk and renders them offline,
//! so a terminal — or an agent that just wrote a profile — can level every
//! patch before the app is ever opened.

use crate::library::RigLibrary;

/// Apply the rig's NAM level calibration (from its audio prefs) to every
/// chain built from now on — the live rig and offline levelling alike, so
/// what is measured is what is heard.
pub fn apply_nam_calibration() -> Option<f32> {
    let cal = signal_sampler::RigManager::load(crate::session::AUDIO_RIG_NAME)
        .audio
        .nam_calibration();
    signal_sampler::nam::set_interface_calibration_dbu(cal);
    cal
}
use crate::nodes::profile_from_library;

/// One patch's measurement.
#[derive(Clone, Debug)]
pub struct Levelled {
    pub patch: String,
    /// Integrated loudness of the raw patch; `None` when it did not render.
    pub lufs: Option<f32>,
    /// The calibration written to `level_db` (unchanged when `lufs` is `None`).
    pub level_db: f32,
}

/// Level every patch of the named profile (the active one when `None`) and,
/// unless `dry_run`, write each `level_db` back to its file.
///
/// Measured with the patch's calibration AND the player's trim zeroed: the
/// chain carries both on its trim block, and measuring through them would
/// make a second pass correct the first instead of repeating it. The
/// player's `trim_db` is never touched — it is set by ear, on top.
///
/// # Errors
///
/// When no profile has that name.
pub fn level_profile(
    name: Option<&str>,
    sample_rate: u32,
    dry_run: bool,
) -> Result<Vec<Levelled>, String> {
    apply_nam_calibration();
    let lib = RigLibrary::load_or_bootstrap();
    let mut def = match name {
        None => lib.profile.clone(),
        Some(n) => lib
            .profiles
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(n))
            .cloned()
            .ok_or_else(|| {
                let names: Vec<&str> = lib.profiles.iter().map(|p| p.name.as_str()).collect();
                format!("no profile named {n:?} (have: {})", names.join(", "))
            })?,
    };

    let mut raw = def.clone();
    for p in &mut raw.patches {
        p.level_db = 0.0;
        p.trim_db = 0.0;
    }
    let built = profile_from_library(&raw, &lib.drive_presets);

    let mut out = Vec::with_capacity(def.patches.len());
    for patch in &mut def.patches {
        let blocks: Vec<_> = built
            .patches
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(&patch.name))
            .map(|p| {
                p.chain
                    .iter()
                    .filter(|b| b.has_backend())
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let lufs = signal_sampler::patch_level::level_of(&blocks, sample_rate)
            .filter(|l| l.is_finite())
            .map(|l| l as f32);
        if let Some(l) = lufs {
            // Same clamp as the rig's pass: more than this is a patch built
            // wrong, and the makeup would only amplify noise.
            patch.level_db =
                ((signal_sampler::patch_level::TARGET_LUFS as f32) - l).clamp(-24.0, 24.0);
        }
        out.push(Levelled {
            patch: patch.name.clone(),
            lufs,
            level_db: patch.level_db,
        });
    }
    if !dry_run {
        RigLibrary::save_profile(&def);
    }
    Ok(out)
}
