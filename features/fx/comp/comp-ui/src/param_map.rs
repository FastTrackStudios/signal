//! Core-parameter name → [`CompParams`] field.
//!
//! `comp-profiles` describes a hardware unit in terms of the *engine's* core
//! parameter names (`threshold_db`, `output_gain_db`, `fold`, …). Those names
//! come from the DSP chain, not from the plugin's param tree, and the two do
//! not spell everything the same way — the plugin exposes the parallel blend
//! as `mix` and the makeup trim as `makeup_db`. Faceplates need to write
//! engine params by core name, so the translation lives here, once.
//!
//! [`core_param_ptr`] returns `None` for a core param the plugin does not
//! expose (`multiband_amount` — the multiband stage still has no UI). Callers
//! skip those writes rather than failing: a profile mapping that mentions an
//! unexposed param should still drive the params that do exist.

use audiocore_core::prelude::Param;
use nice_plug::prelude::ParamPtr;

use crate::params::CompParams;

/// Resolve a `comp_profiles` core param name to the matching plugin param.
pub fn core_param_ptr(params: &CompParams, core_name: &str) -> Option<ParamPtr> {
    Some(match core_name {
        "threshold_db" => params.threshold_db.as_ptr(),
        "ratio" => params.ratio.as_ptr(),
        "attack_ms" => params.attack_ms.as_ptr(),
        "release_ms" => params.release_ms.as_ptr(),
        "knee_db" => params.knee_db.as_ptr(),
        "auto_makeup" => params.auto_makeup.as_ptr(),
        "feedback" => params.feedback.as_ptr(),
        // The engine calls the stereo detector link `channel_link`.
        "channel_link" => params.stereo_link.as_ptr(),
        "detector_rms_mix" => params.detector_rms_mix.as_ptr(),
        "inertia" => params.inertia.as_ptr(),
        "inertia_decay" => params.inertia_decay.as_ptr(),
        "ceiling" => params.ceiling.as_ptr(),
        "drive" => params.drive.as_ptr(),
        "character_mode" => params.character_mode.as_ptr(),
        // The engine's dry/wet is `fold`; the plugin exposes it as Mix.
        "fold" => params.mix.as_ptr(),
        "input_gain_db" => params.input_gain_db.as_ptr(),
        // Profiles call the post-compressor trim the output gain; the plugin
        // has always called it Makeup.
        "output_gain_db" => params.makeup_db.as_ptr(),
        "sidechain_freq" => params.sidechain_freq.as_ptr(),
        "sidechain_lowpass_freq" => params.sidechain_lowpass_freq.as_ptr(),
        "range_db" => params.range_db.as_ptr(),
        "expander_threshold_db" => params.expander_threshold_db.as_ptr(),
        "expander_ratio" => params.expander_ratio.as_ptr(),
        "upward_threshold_db" => params.upward_threshold_db.as_ptr(),
        "upward_ratio" => params.upward_ratio.as_ptr(),
        "hold_ms" => params.hold_ms.as_ptr(),
        "lookahead_ms" => params.lookahead_ms.as_ptr(),
        "style" => params.style.as_ptr(),
        "profile" => params.profile.as_ptr(),
        // Not exposed by this plugin (no crossover UI yet).
        "multiband_amount" => return None,
        _ => return None,
    })
}
