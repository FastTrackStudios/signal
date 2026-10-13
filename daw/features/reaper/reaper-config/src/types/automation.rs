//! Automation preferences (Preferences > Automation).

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::ini::IniFile;
use crate::parse::*;
use crate::types::common::{AutoMuteMode, AutomationMode, EnvelopeTrimMode};

const S: &str = "REAPER";

/// Automation and envelope preferences.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct Automation {
    /// Automation return speed in seconds.
    /// Key: `autoreturntime`
    pub auto_return_time: Option<f64>,

    /// Automation transition time in seconds.
    /// Key: `autoreturntime_action`
    pub auto_return_time_action: Option<f64>,

    /// Auto-envelope creation settings bitfield.
    /// Key: `env_autoadd`
    pub envelope_auto_add: Option<u32>,

    /// Envelope display options bitfield.
    /// Key: `env_options`
    pub envelope_options: Option<u32>,

    /// When adding volume/pan envelopes, how to apply trim and reset it.
    /// Key: `envtrimadjmode`
    #[facet(opaque)]
    pub envelope_trim_adjust_mode: Option<EnvelopeTrimMode>,

    /// Automation mode to switch to after an envelope write pass.
    /// Key: `envwritepasschg`
    #[facet(opaque)]
    pub envelope_write_pass_change: Option<AutomationMode>,

    /// Pooled automation item edge transition in ms.
    /// Key: `pooledenvtranstime`
    pub pooled_envelope_transition_time: Option<f64>,

    /// When adding or showing envelopes, set focus to the new envelope.
    /// Key: `env_deffoc`
    pub envelope_default_focus: Option<bool>,

    /// Envelope overlay minimum height.
    /// Key: `env_ol_minh`
    pub envelope_overlay_min_height: Option<u32>,

    /// Envelope point reduction settings bitfield.
    /// Key: `env_reduce`
    pub envelope_reduce: Option<u32>,

    /// Envelope segment click behavior bitfield.
    /// Key: `envclicksegmode`
    pub envelope_click_segment_mode: Option<u32>,

    /// Envelope transition timing in ms.
    /// Key: `envtranstime`
    pub envelope_transition_time: Option<f64>,

    /// Pitch envelope range display.
    /// Key: `pitchenvrange`
    pub pitch_envelope_range: Option<u32>,

    /// Pooled automation item count.
    /// Key: `pooledenvs`
    pub pooled_envelopes: Option<u32>,

    /// Tempo envelope maximum value.
    /// Key: `tempoenvmax`
    pub tempo_envelope_max: Option<f64>,

    /// Tempo envelope minimum value.
    /// Key: `tempoenvmin`
    pub tempo_envelope_min: Option<f64>,

    /// Tempo envelope snapping.
    /// Key: `tempoenvsnap`
    pub tempo_envelope_snap: Option<u32>,

    /// Envelope attachment mode.
    /// Key: `envattach`
    pub envelope_attach: Option<u32>,

    /// Default automation mode for new tracks.
    /// Key: `defautomode`
    #[facet(opaque)]
    pub default_automation_mode: Option<AutomationMode>,

    /// Default envelope creation bitfield.
    /// Key: `defenvs`
    pub default_envelopes: Option<u32>,

    /// Volume envelope range display.
    /// Key: `volenvrange`
    pub volume_envelope_range: Option<u32>,

    // ── Mute / solo ──
    /// Automatic track muting behavior.
    /// Key: `automute`
    #[facet(opaque)]
    pub auto_mute: Option<AutoMuteMode>,

    /// Auto-mute behavior flags bitfield.
    /// Key: `automuteflags`
    pub auto_mute_flags: Option<u32>,

    /// Auto-mute threshold value in dB.
    /// Key: `automuteval`
    pub auto_mute_value: Option<f64>,

    /// Mute fade duration (ms x 10).
    /// Key: `mutefadems10`
    pub mute_fade_ms_10: Option<u32>,

    /// No-run mute setting.
    /// Key: `norunmute`
    pub no_run_mute: Option<bool>,

    /// Solo dim level (dB x 10).
    /// Key: `solodimdb10`
    pub solo_dim_db_10: Option<i32>,

    /// Solo dim automation display.
    /// Key: `solodimen`
    pub solo_dim_enabled: Option<bool>,

    /// Solo input protection options bitfield.
    /// Key: `soloip`
    pub solo_in_place: Option<u32>,
}

impl Automation {
    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            auto_return_time: get_f64(ini, S, "autoreturntime"),
            auto_return_time_action: get_f64(ini, S, "autoreturntime_action"),
            envelope_auto_add: get_u32(ini, S, "env_autoadd"),
            envelope_options: get_u32(ini, S, "env_options"),
            envelope_trim_adjust_mode: get_enum_u32(ini, S, "envtrimadjmode"),
            envelope_write_pass_change: get_enum_u32(ini, S, "envwritepasschg"),
            pooled_envelope_transition_time: get_f64(ini, S, "pooledenvtranstime"),
            envelope_default_focus: get_bool(ini, S, "env_deffoc"),
            envelope_overlay_min_height: get_u32(ini, S, "env_ol_minh"),
            envelope_reduce: get_u32(ini, S, "env_reduce"),
            envelope_click_segment_mode: get_u32(ini, S, "envclicksegmode"),
            envelope_transition_time: get_f64(ini, S, "envtranstime"),
            pitch_envelope_range: get_u32(ini, S, "pitchenvrange"),
            pooled_envelopes: get_u32(ini, S, "pooledenvs"),
            tempo_envelope_max: get_f64(ini, S, "tempoenvmax"),
            tempo_envelope_min: get_f64(ini, S, "tempoenvmin"),
            tempo_envelope_snap: get_u32(ini, S, "tempoenvsnap"),
            envelope_attach: get_u32(ini, S, "envattach"),
            default_automation_mode: get_enum_u32(ini, S, "defautomode"),
            default_envelopes: get_u32(ini, S, "defenvs"),
            volume_envelope_range: get_u32(ini, S, "volenvrange"),
            auto_mute: get_enum_u32(ini, S, "automute"),
            auto_mute_flags: get_u32(ini, S, "automuteflags"),
            auto_mute_value: get_f64(ini, S, "automuteval"),
            mute_fade_ms_10: get_u32(ini, S, "mutefadems10"),
            no_run_mute: get_bool(ini, S, "norunmute"),
            solo_dim_db_10: get_i32(ini, S, "solodimdb10"),
            solo_dim_enabled: get_bool(ini, S, "solodimen"),
            solo_in_place: get_u32(ini, S, "soloip"),
        }
    }

    /// Returns a new [`Automation`] with REAPER factory default values.
    ///
    /// Fields whose factory value is an empty string are left as `None`.
    pub fn factory_defaults() -> Self {
        Self {
            auto_return_time: Some(0.1),        // autoreturntime=0.10000000000000
            auto_return_time_action: Some(0.1), // autoreturntime_action=0.10000000000000
            envelope_auto_add: Some(1),         // env_autoadd=1
            envelope_options: Some(0),          // env_options=0
            envelope_trim_adjust_mode: Some(EnvelopeTrimMode::Always), // envtrimadjmode=0
            envelope_write_pass_change: Some(AutomationMode::Write), // envwritepasschg=3
            pooled_envelope_transition_time: Some(0.012), // pooledenvtranstime=0.01200000000000
            envelope_default_focus: Some(false), // env_deffoc=0
            envelope_overlay_min_height: None,  // env_ol_minh=-41 (negative, unparseable as u32)
            envelope_reduce: Some(7),           // env_reduce=7
            envelope_click_segment_mode: Some(1), // envclicksegmode=1
            envelope_transition_time: Some(0.0005), // envtranstime=0.00050000000000
            pitch_envelope_range: Some(3),      // pitchenvrange=3
            pooled_envelopes: Some(0),          // pooledenvs=0
            tempo_envelope_max: Some(280.0),    // tempoenvmax=280
            tempo_envelope_min: Some(40.0),     // tempoenvmin=40
            tempo_envelope_snap: None,          // tempoenvsnap not in factory defaults
            envelope_attach: Some(3),           // envattach=3
            default_automation_mode: Some(AutomationMode::TrimRead), // defautomode=0
            default_envelopes: Some(0),         // defenvs=0
            volume_envelope_range: Some(2),     // volenvrange=2
            auto_mute: Some(AutoMuteMode::MuteAny), // automute=2
            auto_mute_flags: Some(0),           // automuteflags=0
            auto_mute_value: Some(7.94328235),  // automuteval=7.94328235000000
            mute_fade_ms_10: Some(50),          // mutefadems10=50
            no_run_mute: Some(true),            // norunmute=1
            solo_dim_db_10: Some(-180),         // solodimdb10=-180
            solo_dim_enabled: Some(false),      // solodimen=0
            solo_in_place: Some(1),             // soloip=1
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        set_opt_f64(ini, S, "autoreturntime", self.auto_return_time);
        set_opt_f64(
            ini,
            S,
            "autoreturntime_action",
            self.auto_return_time_action,
        );
        set_opt_u32(ini, S, "env_autoadd", self.envelope_auto_add);
        set_opt_u32(ini, S, "env_options", self.envelope_options);
        set_opt_enum_u32(ini, S, "envtrimadjmode", self.envelope_trim_adjust_mode);
        set_opt_enum_u32(ini, S, "envwritepasschg", self.envelope_write_pass_change);
        set_opt_f64(
            ini,
            S,
            "pooledenvtranstime",
            self.pooled_envelope_transition_time,
        );
        set_opt_bool(ini, S, "env_deffoc", self.envelope_default_focus);
        set_opt_u32(ini, S, "env_ol_minh", self.envelope_overlay_min_height);
        set_opt_u32(ini, S, "env_reduce", self.envelope_reduce);
        set_opt_u32(ini, S, "envclicksegmode", self.envelope_click_segment_mode);
        set_opt_f64(ini, S, "envtranstime", self.envelope_transition_time);
        set_opt_u32(ini, S, "pitchenvrange", self.pitch_envelope_range);
        set_opt_u32(ini, S, "pooledenvs", self.pooled_envelopes);
        set_opt_f64(ini, S, "tempoenvmax", self.tempo_envelope_max);
        set_opt_f64(ini, S, "tempoenvmin", self.tempo_envelope_min);
        set_opt_u32(ini, S, "tempoenvsnap", self.tempo_envelope_snap);
        set_opt_u32(ini, S, "envattach", self.envelope_attach);
        set_opt_enum_u32(ini, S, "defautomode", self.default_automation_mode);
        set_opt_u32(ini, S, "defenvs", self.default_envelopes);
        set_opt_u32(ini, S, "volenvrange", self.volume_envelope_range);
        set_opt_enum_u32(ini, S, "automute", self.auto_mute);
        set_opt_u32(ini, S, "automuteflags", self.auto_mute_flags);
        set_opt_f64(ini, S, "automuteval", self.auto_mute_value);
        set_opt_u32(ini, S, "mutefadems10", self.mute_fade_ms_10);
        set_opt_bool(ini, S, "norunmute", self.no_run_mute);
        set_opt_i32(ini, S, "solodimdb10", self.solo_dim_db_10);
        set_opt_bool(ini, S, "solodimen", self.solo_dim_enabled);
        set_opt_u32(ini, S, "soloip", self.solo_in_place);
    }
}
