//! Performance / threading preferences (Preferences > Audio > Performance).

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::ini::IniFile;
use crate::parse::*;
use crate::types::common::VstBridgeMode;

/// Denormalization and plug-in security settings.
///
/// Maps to REAPER's `fxdenorm` INI key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet)]
pub struct FxDenormFlags {
    /// Reduce denormalization noise from plug-ins (recommended). Default: `true`.
    pub reduce_denormalization: bool,
    /// Terminate REAPER immediately on exit without waiting for audio threads. Default: `false`.
    pub terminate_immediately: bool,
}

impl Default for FxDenormFlags {
    fn default() -> Self {
        Self {
            reduce_denormalization: true,
            terminate_immediately: false,
        }
    }
}

impl FxDenormFlags {
    /// Encode as the `fxdenorm` INI integer.
    pub fn to_u32(self) -> u32 {
        let mut v = 0u32;
        if self.reduce_denormalization {
            v |= 1;
        }
        if self.terminate_immediately {
            v |= 2;
        }
        v
    }

    /// Decode from the `fxdenorm` INI integer.
    pub fn from_u32(v: u32) -> Self {
        Self {
            reduce_denormalization: (v & 1) != 0,
            terminate_immediately: (v & 2) != 0,
        }
    }
}

const S: &str = "REAPER";

/// Performance and threading preferences.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct Performance {
    /// Auto-detect number of processing threads.
    /// Key: `autonbworkerthreads`
    pub auto_worker_threads: Option<bool>,

    /// Audio processing thread count.
    /// Key: `workthreads`
    pub work_threads: Option<u32>,

    /// Thread priority level selection.
    /// Key: `threadpr`
    pub thread_priority: Option<u32>,

    /// Worker thread behavior mode.
    /// Key: `workbehvr`
    pub work_behavior: Option<u32>,

    /// Media buffer size in ms.
    /// Key: `workbufmsex`
    pub work_buffer_ms: Option<u32>,

    /// Media buffer with FX UI open in ms.
    /// Key: `workbuffxuims`
    pub work_buffer_fx_ui_ms: Option<u32>,

    /// Anticipative FX processing options bitfield.
    /// Key: `workrender`
    pub work_render: Option<u32>,

    /// Pre-buffer percentage (0-100).
    /// Key: `prebufperb`
    pub pre_buffer_percent: Option<u32>,

    /// Render-ahead length in ms.
    /// Key: `renderaheadlen`
    pub render_ahead_len: Option<u32>,

    /// Render-ahead secondary timing in ms.
    /// Key: `renderaheadlen2`
    pub render_ahead_len_2: Option<u32>,

    /// Live FX multiprocessing limit.
    /// Key: `syncsmpmax2`
    pub sync_smp_max: Option<u32>,

    /// Live FX multiprocessing toggle.
    /// Key: `syncsmpuse`
    pub sync_smp_use: Option<bool>,

    // ── FX / plugins ──
    /// Denormalization and plug-in security settings.
    /// Key: `fxdenorm`
    pub fx_denorm: Option<FxDenormFlags>,

    /// FX automation interpolation in ms.
    /// Key: `fxenvinterp`
    pub fx_envelope_interpolation: Option<f64>,

    /// VST plug-in bridging/firewall mode (64-bit).
    /// Key: `vstbr64`
    #[facet(opaque)]
    pub vst_bridge_64: Option<VstBridgeMode>,

    /// VST plug-in bridging/firewall mode (32-bit).
    /// Key: `vstbr32`
    #[facet(opaque)]
    pub vst_bridge_32: Option<VstBridgeMode>,

    /// VST full state saving.
    /// Key: `vstfullstate`
    pub vst_full_state: Option<u32>,

    /// VST plugin scanning options bitfield.
    /// Key: `vst_scan`
    pub vst_scan: Option<u32>,

    /// VST folder settings.
    /// Key: `vstfolder_settings`
    pub vst_folder_settings: Option<String>,

    /// ARA plugin support toggle.
    /// Key: `ara`
    pub ara: Option<bool>,

    /// LV2 plugin options.
    /// Key: `lv2_opts`
    pub lv2_opts: Option<u32>,

    /// Disable DirectX plugin scan.
    /// Key: `disabledxscan`
    pub disable_dx_scan: Option<bool>,

    /// Use DirectX plugins toggle.
    /// Key: `usedxplugs`
    pub use_dx_plugins: Option<bool>,

    /// ReWire slave mode toggle.
    /// Key: `rewireslave`
    pub rewire_slave: Option<bool>,

    /// ReWire enable toggle.
    /// Key: `userewire`
    pub use_rewire: Option<bool>,
}

impl Performance {
    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            auto_worker_threads: get_bool(ini, S, "autonbworkerthreads"),
            work_threads: get_u32(ini, S, "workthreads"),
            thread_priority: get_u32(ini, S, "threadpr"),
            work_behavior: get_u32(ini, S, "workbehvr"),
            work_buffer_ms: get_u32(ini, S, "workbufmsex"),
            work_buffer_fx_ui_ms: get_u32(ini, S, "workbuffxuims"),
            work_render: get_u32(ini, S, "workrender"),
            pre_buffer_percent: get_u32(ini, S, "prebufperb"),
            render_ahead_len: get_u32(ini, S, "renderaheadlen"),
            render_ahead_len_2: get_u32(ini, S, "renderaheadlen2"),
            sync_smp_max: get_u32(ini, S, "syncsmpmax2"),
            sync_smp_use: get_bool(ini, S, "syncsmpuse"),
            fx_denorm: get_u32(ini, S, "fxdenorm").map(FxDenormFlags::from_u32),
            fx_envelope_interpolation: get_f64(ini, S, "fxenvinterp"),
            vst_bridge_64: get_enum_u32(ini, S, "vstbr64"),
            vst_bridge_32: get_enum_u32(ini, S, "vstbr32"),
            vst_full_state: get_u32(ini, S, "vstfullstate"),
            vst_scan: get_u32(ini, S, "vst_scan"),
            vst_folder_settings: get_string(ini, S, "vstfolder_settings"),
            ara: get_bool(ini, S, "ara"),
            lv2_opts: get_u32(ini, S, "lv2_opts"),
            disable_dx_scan: get_bool(ini, S, "disabledxscan"),
            use_dx_plugins: get_bool(ini, S, "usedxplugs"),
            rewire_slave: get_bool(ini, S, "rewireslave"),
            use_rewire: get_bool(ini, S, "userewire"),
        }
    }

    /// Returns a new [`Performance`] with REAPER factory default values.
    ///
    /// Fields whose factory value is an empty string or negative integer
    /// (unparseable as `u32`) are left as `None`.
    pub fn factory_defaults() -> Self {
        Self {
            auto_worker_threads: Some(true),           // autonbworkerthreads=1
            work_threads: Some(32),                    // workthreads=32
            thread_priority: Some(4),                  // threadpr=4
            work_behavior: None, // workbehvr=-1 (negative, unparseable as u32)
            work_buffer_ms: Some(1200), // workbufmsex=1200
            work_buffer_fx_ui_ms: Some(200), // workbuffxuims=200
            work_render: Some(17), // workrender=17
            pre_buffer_percent: Some(100), // prebufperb=100
            render_ahead_len: Some(200), // renderaheadlen=200
            render_ahead_len_2: Some(100), // renderaheadlen2=100
            sync_smp_max: Some(32), // syncsmpmax2=32
            sync_smp_use: Some(true), // syncsmpuse=1
            fx_denorm: Some(FxDenormFlags::default()), // fxdenorm=1 (reduce_denormalization only)
            fx_envelope_interpolation: Some(0.0), // fxenvinterp=0.00000000000000
            vst_bridge_64: None, // vstbr64= (empty)
            vst_bridge_32: None, // vstbr32 not in factory defaults
            vst_full_state: Some(25413), // vstfullstate=25413
            vst_scan: None,      // vst_scan not in factory defaults
            vst_folder_settings: None, // vstfolder_settings not in factory defaults
            ara: Some(false),    // ara=0
            lv2_opts: Some(0),   // lv2_opts=0
            disable_dx_scan: Some(true), // disabledxscan=1
            use_dx_plugins: Some(true), // usedxplugs=1
            rewire_slave: Some(true), // rewireslave=1
            use_rewire: Some(true), // userewire=1
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        set_opt_bool(ini, S, "autonbworkerthreads", self.auto_worker_threads);
        set_opt_u32(ini, S, "workthreads", self.work_threads);
        set_opt_u32(ini, S, "threadpr", self.thread_priority);
        set_opt_u32(ini, S, "workbehvr", self.work_behavior);
        set_opt_u32(ini, S, "workbufmsex", self.work_buffer_ms);
        set_opt_u32(ini, S, "workbuffxuims", self.work_buffer_fx_ui_ms);
        set_opt_u32(ini, S, "workrender", self.work_render);
        set_opt_u32(ini, S, "prebufperb", self.pre_buffer_percent);
        set_opt_u32(ini, S, "renderaheadlen", self.render_ahead_len);
        set_opt_u32(ini, S, "renderaheadlen2", self.render_ahead_len_2);
        set_opt_u32(ini, S, "syncsmpmax2", self.sync_smp_max);
        set_opt_bool(ini, S, "syncsmpuse", self.sync_smp_use);
        if let Some(f) = self.fx_denorm {
            ini.set(S, "fxdenorm", &f.to_u32().to_string());
        }
        set_opt_f64(ini, S, "fxenvinterp", self.fx_envelope_interpolation);
        set_opt_enum_u32(ini, S, "vstbr64", self.vst_bridge_64);
        set_opt_enum_u32(ini, S, "vstbr32", self.vst_bridge_32);
        set_opt_u32(ini, S, "vstfullstate", self.vst_full_state);
        set_opt_u32(ini, S, "vst_scan", self.vst_scan);
        set_opt_string(ini, S, "vstfolder_settings", &self.vst_folder_settings);
        set_opt_bool(ini, S, "ara", self.ara);
        set_opt_u32(ini, S, "lv2_opts", self.lv2_opts);
        set_opt_bool(ini, S, "disabledxscan", self.disable_dx_scan);
        set_opt_bool(ini, S, "usedxplugs", self.use_dx_plugins);
        set_opt_bool(ini, S, "rewireslave", self.rewire_slave);
        set_opt_bool(ini, S, "userewire", self.use_rewire);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ini::IniFile;

    // ── FxDenormFlags ─────────────────────────────────────────────────────

    #[test]
    fn fx_denorm_flags_roundtrip() {
        assert_eq!(
            FxDenormFlags::from_u32(0),
            FxDenormFlags {
                reduce_denormalization: false,
                terminate_immediately: false
            }
        );
        assert_eq!(
            FxDenormFlags::from_u32(1),
            FxDenormFlags {
                reduce_denormalization: true,
                terminate_immediately: false
            }
        );
        assert_eq!(
            FxDenormFlags::from_u32(2),
            FxDenormFlags {
                reduce_denormalization: false,
                terminate_immediately: true
            }
        );
        assert_eq!(
            FxDenormFlags::from_u32(3),
            FxDenormFlags {
                reduce_denormalization: true,
                terminate_immediately: true
            }
        );
        assert_eq!(FxDenormFlags::default().to_u32(), 1);
    }

    #[test]
    fn fx_denorm_flags_parse_write() {
        let ini = IniFile::parse("[REAPER]\nfxdenorm=1\n");
        let p = Performance::from_ini(&ini);
        assert_eq!(
            p.fx_denorm,
            Some(FxDenormFlags {
                reduce_denormalization: true,
                terminate_immediately: false
            })
        );
        let mut out = IniFile::parse("[REAPER]\n");
        p.write_to(&mut out);
        assert_eq!(out.get("REAPER", "fxdenorm"), Some("1"));
    }
}
