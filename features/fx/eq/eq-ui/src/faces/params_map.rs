//! Panel control id → EQ parameter.
//!
//! Each hardware model has its own parameters because each is its own circuit,
//! so a panel control is exactly one parameter and this is the whole
//! translation. Ids are the parameter names without their model prefix
//! ("low_boost" on the Pultec panel is `pultec_low_boost_db`), which keeps the
//! layout tables readable and stops a control on one panel from resolving
//! against another model's parameter.

use audiocore_core::prelude::Param;
use nice_plug::prelude::ParamPtr;

use crate::params::FtsEqParams;

/// Resolve `model` + panel control id to the parameter behind it.
pub fn control_ptr(params: &FtsEqParams, model: i32, id: &str) -> Option<ParamPtr> {
    Some(match (model, id) {
        // ── Pultec EQP-1A ────────────────────────────────────────────────
        (1, "eq_in") => params.pultec_eq_in.as_ptr(),
        (1, "low_freq") => params.pultec_low_freq.as_ptr(),
        (1, "low_boost") => params.pultec_low_boost_db.as_ptr(),
        (1, "low_atten") => params.pultec_low_atten_db.as_ptr(),
        (1, "high_boost_freq") => params.pultec_high_boost_freq.as_ptr(),
        (1, "high_boost") => params.pultec_high_boost_db.as_ptr(),
        (1, "bandwidth") => params.pultec_bandwidth.as_ptr(),
        (1, "high_atten_freq") => params.pultec_high_atten_freq.as_ptr(),
        (1, "high_atten") => params.pultec_high_atten_db.as_ptr(),
        (1, "drive") => params.pultec_drive.as_ptr(),
        (1, "trim") => params.pultec_trim_db.as_ptr(),

        // ── Neve 1073 ────────────────────────────────────────────────────
        (2, "eq_in") => params.neve_eq_in.as_ptr(),
        (2, "phase") => params.neve_phase.as_ptr(),
        (2, "trim") => params.neve_trim_db.as_ptr(),
        (2, "drive") => params.neve_drive.as_ptr(),
        (2, "hpf") => params.neve_hpf.as_ptr(),
        (2, "low_freq") => params.neve_low_freq.as_ptr(),
        (2, "low_gain") => params.neve_low_gain_db.as_ptr(),
        (2, "mid_freq") => params.neve_mid_freq.as_ptr(),
        (2, "mid_gain") => params.neve_mid_gain_db.as_ptr(),
        (2, "high_gain") => params.neve_high_gain_db.as_ptr(),

        // ── API 550A ─────────────────────────────────────────────────────
        (3, "eq_in") => params.api_eq_in.as_ptr(),
        (3, "low_freq") => params.api_low_freq.as_ptr(),
        (3, "low_gain") => params.api_low_gain_db.as_ptr(),
        (3, "mid_freq") => params.api_mid_freq.as_ptr(),
        (3, "mid_gain") => params.api_mid_gain_db.as_ptr(),
        (3, "high_freq") => params.api_high_freq.as_ptr(),
        (3, "high_gain") => params.api_high_gain_db.as_ptr(),
        (3, "drive") => params.api_drive.as_ptr(),
        (3, "trim") => params.api_trim_db.as_ptr(),

        // ── SSL E and G ──────────────────────────────────────────────────
        // Both wear the same panel and share one parameter set; the model
        // value selects the curves, which is the whole difference between
        // them.
        (4 | 5, "eq_in") => params.ssl_eq_in.as_ptr(),
        (4 | 5, "hpf") => params.ssl_hpf_hz.as_ptr(),
        (4 | 5, "lpf") => params.ssl_lpf_hz.as_ptr(),
        (4 | 5, "lf_freq") => params.ssl_lf_freq_hz.as_ptr(),
        (4 | 5, "lf_gain") => params.ssl_lf_gain_db.as_ptr(),
        (4 | 5, "lmf_freq") => params.ssl_lmf_freq_hz.as_ptr(),
        (4 | 5, "lmf_gain") => params.ssl_lmf_gain_db.as_ptr(),
        (4 | 5, "hmf_freq") => params.ssl_hmf_freq_hz.as_ptr(),
        (4 | 5, "hmf_gain") => params.ssl_hmf_gain_db.as_ptr(),
        (4 | 5, "hf_freq") => params.ssl_hf_freq_hz.as_ptr(),
        (4 | 5, "hf_gain") => params.ssl_hf_gain_db.as_ptr(),
        (4 | 5, "drive") => params.ssl_drive.as_ptr(),
        (4 | 5, "trim") => params.ssl_trim_db.as_ptr(),

        _ => return None,
    })
}
