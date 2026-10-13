//! General preferences (Preferences > General).

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::ini::IniFile;
use crate::parse::*;
use crate::types::common::{HelpDisplay, ModalWindowPosition, StartupProjectMode};

/// Tooltip visibility settings.
///
/// Each field represents whether that category of tooltip is **shown**.
/// The default (all `true`) matches REAPER's factory behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet)]
pub struct TooltipSettings {
    /// Show tooltips for media items and envelopes. Default: `true`.
    pub items_and_envelopes: bool,
    /// Show tooltips for UI elements (buttons, knobs, etc.). Default: `true`.
    pub ui_elements: bool,
    /// Show envelope tooltips when hovering over them. Default: `true`.
    pub envelope_hover: bool,
}

impl Default for TooltipSettings {
    fn default() -> Self {
        Self {
            items_and_envelopes: true,
            ui_elements: true,
            envelope_hover: true,
        }
    }
}

impl TooltipSettings {
    /// Encode as the `tooltips` INI integer.
    ///
    /// Bits are **inverted**: a set bit means the feature is *off*.
    /// - bit 0 (`&1`) → items/envelopes tooltips disabled
    /// - bit 1 (`&2`) → UI element tooltips disabled
    /// - bit 2 (`&4`) → envelope-hover tooltips disabled
    pub fn to_u32(self) -> u32 {
        let mut v = 0u32;
        if !self.items_and_envelopes {
            v |= 1;
        }
        if !self.ui_elements {
            v |= 2;
        }
        if !self.envelope_hover {
            v |= 4;
        }
        v
    }

    /// Decode from the `tooltips` INI integer.
    pub fn from_u32(v: u32) -> Self {
        Self {
            items_and_envelopes: (v & 1) == 0,
            ui_elements: (v & 2) == 0,
            envelope_hover: (v & 4) == 0,
        }
    }
}

/// Which actions are tracked in REAPER's undo history.
///
/// Each field controls whether that category of change is included in the
/// undo history. The default matches REAPER's factory behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet)]
pub struct UndoFlags {
    /// Include item selection changes in undo history. Default: `true`.
    pub include_items: bool,
    /// Include time selection changes in undo history. Default: `true`.
    pub include_time: bool,
    /// When undo memory is full, discard the oldest state to keep the newest.
    /// Default: `false` (REAPER discards the newest state to keep the oldest).
    pub discard_oldest_when_full: bool,
}

impl Default for UndoFlags {
    fn default() -> Self {
        Self {
            include_items: true,
            include_time: true,
            discard_oldest_when_full: false,
        }
    }
}

impl UndoFlags {
    /// Encode as the `undomask` INI integer.
    pub fn to_u32(self) -> u32 {
        let mut v = 0u32;
        if self.include_items {
            v |= 1;
        }
        if self.include_time {
            v |= 2;
        }
        if self.discard_oldest_when_full {
            v |= 4;
        }
        v
    }

    /// Decode from the `undomask` INI integer.
    pub fn from_u32(v: u32) -> Self {
        Self {
            include_items: (v & 1) != 0,
            include_time: (v & 2) != 0,
            discard_oldest_when_full: (v & 4) != 0,
        }
    }
}

/// Controls automatic FX window resizing when switching between plug-ins.
///
/// Maps to REAPER's `fxresize` INI key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet)]
pub struct FxResizeFlags {
    /// Resize FX window upward (larger) to fit the new plug-in. Default: `true`.
    pub resize_up: bool,
    /// Resize FX window downward (smaller) to fit the new plug-in. Default: `true`.
    pub resize_down: bool,
}

impl Default for FxResizeFlags {
    fn default() -> Self {
        Self {
            resize_up: true,
            resize_down: true,
        }
    }
}

impl FxResizeFlags {
    /// Encode as the `fxresize` INI integer.
    pub fn to_u32(self) -> u32 {
        let mut v = 0u32;
        if self.resize_up {
            v |= 1;
        }
        if self.resize_down {
            v |= 2;
        }
        v
    }

    /// Decode from the `fxresize` INI integer.
    pub fn from_u32(v: u32) -> Self {
        Self {
            resize_up: (v & 1) != 0,
            resize_down: (v & 2) != 0,
        }
    }
}

/// Actions taken automatically when creating a new project.
///
/// Maps to REAPER's `newprojdo` INI key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet, Default)]
pub struct NewProjectFlags {
    /// Prompt to save the current project before creating a new one. Default: `false`.
    pub prompt_to_save: bool,
    /// Automatically open Project Settings when creating a new project. Default: `false`.
    pub open_properties: bool,
}

impl NewProjectFlags {
    /// Encode as the `newprojdo` INI integer.
    pub fn to_u32(self) -> u32 {
        let mut v = 0u32;
        if self.prompt_to_save {
            v |= 1;
        }
        if self.open_properties {
            v |= 2;
        }
        v
    }

    /// Decode from the `newprojdo` INI integer.
    pub fn from_u32(v: u32) -> Self {
        Self {
            prompt_to_save: (v & 1) != 0,
            open_properties: (v & 2) != 0,
        }
    }
}

const S: &str = "REAPER";

/// General preferences.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct General {
    /// Maximum undo memory in MB. 0 disables undo and close-save prompts.
    /// Key: `undomaxmem`
    pub undo_max_mem: Option<u32>,

    /// Which changes are tracked in the undo history.
    /// Key: `undomask`
    pub undo_mask: Option<UndoFlags>,

    /// Whether to save undo states in project files.
    /// Key: `saveundostatesproj`
    pub save_undo_states_proj: Option<u32>,

    /// Which project to open on startup.
    /// Key: `loadlastproj`
    #[facet(opaque)]
    pub load_last_project: Option<StartupProjectMode>,

    /// Maximum number of recent files.
    /// Key: `maxrecent`
    pub max_recent: Option<u32>,

    /// Automatically check for new versions of REAPER on startup.
    /// Key: `verchk`
    pub version_check: Option<bool>,

    /// CPU usage limit percentage.
    /// Key: `cpuallowed`
    pub cpu_allowed: Option<u32>,

    /// CPU restriction mode bitfield.
    /// Key: `restrictcpu`
    pub restrict_cpu: Option<u32>,

    /// RAM limit warning threshold (64-bit).
    /// Key: `warnmaxram64`
    pub warn_max_ram_64: Option<u32>,

    /// UI scaling factor.
    /// Key: `uiscale`
    pub ui_scale: Option<f64>,

    /// Currently loaded theme (full path or `"<classic>"`).
    /// Key: `lastthemefn5`
    pub theme: Option<String>,

    /// Always-on-top window setting.
    /// Key: `aot`
    pub always_on_top: Option<bool>,

    /// Keyboard input priority setting.
    /// Key: `alwaysallowkb`
    pub always_allow_keyboard: Option<bool>,

    /// Auto-close track windows on track removal.
    /// Key: `autoclosetrackwnds`
    pub auto_close_track_windows: Option<bool>,

    /// Window behavior and docking options bitfield.
    /// Key: `windowflags`
    #[facet(opaque)]
    pub window_flags: Option<ModalWindowPosition>,

    /// Maximum recent FX list count.
    /// Key: `maxrecentfx`
    pub max_recent_fx: Option<u32>,

    /// Actions taken automatically when creating a new project.
    /// Key: `newprojdo`
    pub new_project_action: Option<NewProjectFlags>,

    /// Show last undo in menu bar.
    /// Key: `showlastundo`
    pub show_last_undo: Option<bool>,

    /// Action menu display options bitfield.
    /// Key: `actionmenu`
    pub action_menu: Option<u32>,

    /// Working set management toggle.
    /// Key: `workset_use`
    pub workset_use: Option<bool>,

    /// Working set minimum size.
    /// Key: `workset_min`
    pub workset_min: Option<u32>,

    /// Working set maximum size.
    /// Key: `workset_max`
    pub workset_max: Option<u32>,

    /// Large window frame styling.
    /// Key: `bigwndframes`
    pub big_window_frames: Option<bool>,

    /// Error message display settings bitfield.
    /// Key: `errnowarn`
    pub error_no_warn: Option<u32>,

    /// FX floating window focus behavior.
    /// Key: `fxfloat_focus`
    pub fx_float_focus: Option<u32>,

    /// Automatic FX window resize behavior.
    /// Key: `fxresize`
    pub fx_resize: Option<FxResizeFlags>,

    /// Tooltip display delay in ms.
    /// Key: `tooltipdelay`
    pub tooltip_delay: Option<u32>,

    /// Tooltip visibility settings.
    /// Key: `tooltips`
    pub tooltips: Option<TooltipSettings>,

    /// Help display and performance meter options bitfield.
    /// Key: `help`
    #[facet(opaque)]
    pub help: Option<HelpDisplay>,

    /// Envelope manager context menu settings.
    /// Key: `envmgropts`
    pub envelope_manager_options: Option<u32>,

    /// Peak gain display range.
    /// Key: `maxspeakgain`
    pub max_speaker_gain: Option<f64>,

    /// Auto-close keymap dialog.
    /// Key: `autoclosekeymap`
    pub auto_close_keymap: Option<bool>,

    /// Last properties page viewed.
    /// Key: `pspage_last`
    pub properties_page_last: Option<String>,

    /// macOS Cocoa middleware disable.
    /// Key: `osxnomiddlemancocoa`
    pub osx_no_middleman_cocoa: Option<u32>,

    /// Right-click emulation.
    /// Key: `rightclickemulate`
    pub right_click_emulate: Option<u32>,

    /// Storage max size.
    /// Key: `smmaxsz`
    pub storage_max_size: Option<u32>,

    /// Storage max size percentage.
    /// Key: `smmaxsz_pct`
    pub storage_max_size_percent: Option<u32>,

    /// Title bar region hide.
    /// Key: `titlebarreghide`
    pub title_bar_region_hide: Option<bool>,

    /// Time signature marker.
    /// Key: `tsmarker`
    pub time_sig_marker: Option<bool>,

    /// ReWire slave delay in samples.
    /// Key: `rewireslavedelay`
    pub rewire_slave_delay: Option<u32>,
}

impl General {
    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            undo_max_mem: get_u32(ini, S, "undomaxmem"),
            undo_mask: get_u32(ini, S, "undomask").map(UndoFlags::from_u32),
            save_undo_states_proj: get_u32(ini, S, "saveundostatesproj"),
            load_last_project: get_enum_u32(ini, S, "loadlastproj"),
            max_recent: get_u32(ini, S, "maxrecent"),
            version_check: get_bool(ini, S, "verchk"),
            cpu_allowed: get_u32(ini, S, "cpuallowed"),
            restrict_cpu: get_u32(ini, S, "restrictcpu"),
            warn_max_ram_64: get_u32(ini, S, "warnmaxram64"),
            ui_scale: get_f64(ini, S, "uiscale"),
            theme: get_string(ini, S, "lastthemefn5"),
            always_on_top: get_bool(ini, S, "aot"),
            always_allow_keyboard: get_bool(ini, S, "alwaysallowkb"),
            auto_close_track_windows: get_bool(ini, S, "autoclosetrackwnds"),
            window_flags: get_enum_u32(ini, S, "windowflags"),
            max_recent_fx: get_u32(ini, S, "maxrecentfx"),
            new_project_action: get_u32(ini, S, "newprojdo").map(NewProjectFlags::from_u32),
            show_last_undo: get_bool(ini, S, "showlastundo"),
            action_menu: get_u32(ini, S, "actionmenu"),
            workset_use: get_bool(ini, S, "workset_use"),
            workset_min: get_u32(ini, S, "workset_min"),
            workset_max: get_u32(ini, S, "workset_max"),
            big_window_frames: get_bool(ini, S, "bigwndframes"),
            error_no_warn: get_u32(ini, S, "errnowarn"),
            fx_float_focus: get_u32(ini, S, "fxfloat_focus"),
            fx_resize: get_u32(ini, S, "fxresize").map(FxResizeFlags::from_u32),
            tooltip_delay: get_u32(ini, S, "tooltipdelay"),
            tooltips: get_u32(ini, S, "tooltips").map(TooltipSettings::from_u32),
            help: get_enum_u32(ini, S, "help"),
            envelope_manager_options: get_u32(ini, S, "envmgropts"),
            max_speaker_gain: get_f64(ini, S, "maxspeakgain"),
            auto_close_keymap: get_bool(ini, S, "autoclosekeymap"),
            properties_page_last: get_string(ini, S, "pspage_last"),
            osx_no_middleman_cocoa: get_u32(ini, S, "osxnomiddlemancocoa"),
            right_click_emulate: get_u32(ini, S, "rightclickemulate"),
            storage_max_size: get_u32(ini, S, "smmaxsz"),
            storage_max_size_percent: get_u32(ini, S, "smmaxsz_pct"),
            title_bar_region_hide: get_bool(ini, S, "titlebarreghide"),
            time_sig_marker: get_bool(ini, S, "tsmarker"),
            rewire_slave_delay: get_u32(ini, S, "rewireslavedelay"),
        }
    }

    /// Returns a new [`General`] with REAPER factory default values.
    ///
    /// Fields whose factory value is an empty string are left as `None`.
    pub fn factory_defaults() -> Self {
        Self {
            undo_max_mem: Some(256),               // undomaxmem=256
            undo_mask: Some(UndoFlags::default()), // undomask=3 (items + time)
            save_undo_states_proj: Some(0),        // saveundostatesproj=0
            load_last_project: Some(StartupProjectMode::LastProjectTabs), // loadlastproj=17
            max_recent: Some(50),                  // maxrecent=50
            version_check: Some(true),             // verchk=1
            cpu_allowed: None,                     // cpuallowed= (empty)
            restrict_cpu: None,                    // restrictcpu= (empty)
            warn_max_ram_64: Some(0),              // warnmaxram64=0
            ui_scale: Some(1.0),                   // uiscale=1.00000000000000
            theme: None,                           // lastthemefn5 — user-specific path, set to None
            always_on_top: Some(false),            // aot=0
            always_allow_keyboard: Some(false),    // alwaysallowkb=0
            auto_close_track_windows: Some(true),  // autoclosetrackwnds=1
            window_flags: Some(ModalWindowPosition::LastPosition), // windowflags=0
            max_recent_fx: Some(30),               // maxrecentfx=30
            new_project_action: Some(NewProjectFlags::default()), // newprojdo=0
            show_last_undo: Some(true),            // showlastundo=1
            action_menu: Some(1),                  // actionmenu=1
            workset_use: None,                     // workset_use= (empty) — Option<bool>
            workset_min: None,                     // workset_min= (empty)
            workset_max: None,                     // workset_max= (empty)
            big_window_frames: Some(false),        // bigwndframes=0
            error_no_warn: Some(37),               // errnowarn=37
            fx_float_focus: Some(137),             // fxfloat_focus=137
            fx_resize: Some(FxResizeFlags::default()), // fxresize=3 (both up and down)
            tooltip_delay: Some(200),              // tooltipdelay=200
            tooltips: Some(TooltipSettings::default()), // tooltips=0 (all on)
            help: Some(HelpDisplay::SelectedDetails), // help=3
            envelope_manager_options: Some(0),     // envmgropts=0
            max_speaker_gain: None,                // maxspeakgain not in factory defaults
            auto_close_keymap: None, // REAPER->autoclosekeymap= (empty) — Option<bool>
            properties_page_last: None, // REAPER->pspage_last= (empty)
            osx_no_middleman_cocoa: None, // osxnomiddlemancocoa not in factory defaults
            right_click_emulate: None, // rightclickemulate not in factory defaults
            storage_max_size: Some(6), // smmaxsz=6
            storage_max_size_percent: Some(12), // smmaxsz_pct=12
            title_bar_region_hide: None, // titlebarreghide not in factory defaults — Option<bool>
            time_sig_marker: None,   // tsmarker not in factory defaults — Option<bool>
            rewire_slave_delay: Some(0), // rewireslavedelay=0
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        set_opt_u32(ini, S, "undomaxmem", self.undo_max_mem);
        if let Some(f) = self.undo_mask {
            ini.set(S, "undomask", &f.to_u32().to_string());
        }
        set_opt_u32(ini, S, "saveundostatesproj", self.save_undo_states_proj);
        set_opt_enum_u32(ini, S, "loadlastproj", self.load_last_project);
        set_opt_u32(ini, S, "maxrecent", self.max_recent);
        set_opt_bool(ini, S, "verchk", self.version_check);
        set_opt_u32(ini, S, "cpuallowed", self.cpu_allowed);
        set_opt_u32(ini, S, "restrictcpu", self.restrict_cpu);
        set_opt_u32(ini, S, "warnmaxram64", self.warn_max_ram_64);
        set_opt_f64(ini, S, "uiscale", self.ui_scale);
        set_opt_string(ini, S, "lastthemefn5", &self.theme);
        set_opt_bool(ini, S, "aot", self.always_on_top);
        set_opt_bool(ini, S, "alwaysallowkb", self.always_allow_keyboard);
        set_opt_bool(ini, S, "autoclosetrackwnds", self.auto_close_track_windows);
        set_opt_enum_u32(ini, S, "windowflags", self.window_flags);
        set_opt_u32(ini, S, "maxrecentfx", self.max_recent_fx);
        if let Some(f) = self.new_project_action {
            ini.set(S, "newprojdo", &f.to_u32().to_string());
        }
        set_opt_bool(ini, S, "showlastundo", self.show_last_undo);
        set_opt_u32(ini, S, "actionmenu", self.action_menu);
        set_opt_bool(ini, S, "workset_use", self.workset_use);
        set_opt_u32(ini, S, "workset_min", self.workset_min);
        set_opt_u32(ini, S, "workset_max", self.workset_max);
        set_opt_bool(ini, S, "bigwndframes", self.big_window_frames);
        set_opt_u32(ini, S, "errnowarn", self.error_no_warn);
        set_opt_u32(ini, S, "fxfloat_focus", self.fx_float_focus);
        if let Some(f) = self.fx_resize {
            ini.set(S, "fxresize", &f.to_u32().to_string());
        }
        set_opt_u32(ini, S, "tooltipdelay", self.tooltip_delay);
        if let Some(t) = self.tooltips {
            ini.set(S, "tooltips", &t.to_u32().to_string());
        }
        set_opt_enum_u32(ini, S, "help", self.help);
        set_opt_u32(ini, S, "envmgropts", self.envelope_manager_options);
        set_opt_f64(ini, S, "maxspeakgain", self.max_speaker_gain);
        set_opt_bool(ini, S, "autoclosekeymap", self.auto_close_keymap);
        set_opt_string(ini, S, "pspage_last", &self.properties_page_last);
        set_opt_u32(ini, S, "osxnomiddlemancocoa", self.osx_no_middleman_cocoa);
        set_opt_u32(ini, S, "rightclickemulate", self.right_click_emulate);
        set_opt_u32(ini, S, "smmaxsz", self.storage_max_size);
        set_opt_u32(ini, S, "smmaxsz_pct", self.storage_max_size_percent);
        set_opt_bool(ini, S, "titlebarreghide", self.title_bar_region_hide);
        set_opt_bool(ini, S, "tsmarker", self.time_sig_marker);
        set_opt_u32(ini, S, "rewireslavedelay", self.rewire_slave_delay);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ini::IniFile;

    // ── FxResizeFlags ─────────────────────────────────────────────────────

    #[test]
    fn fx_resize_flags_roundtrip() {
        assert_eq!(
            FxResizeFlags::from_u32(0),
            FxResizeFlags {
                resize_up: false,
                resize_down: false
            }
        );
        assert_eq!(
            FxResizeFlags::from_u32(1),
            FxResizeFlags {
                resize_up: true,
                resize_down: false
            }
        );
        assert_eq!(
            FxResizeFlags::from_u32(2),
            FxResizeFlags {
                resize_up: false,
                resize_down: true
            }
        );
        assert_eq!(
            FxResizeFlags::from_u32(3),
            FxResizeFlags {
                resize_up: true,
                resize_down: true
            }
        );
        assert_eq!(FxResizeFlags::default().to_u32(), 3);
    }

    #[test]
    fn fx_resize_flags_parse_write() {
        let ini = IniFile::parse("[REAPER]\nfxresize=3\n");
        let g = General::from_ini(&ini);
        assert_eq!(
            g.fx_resize,
            Some(FxResizeFlags {
                resize_up: true,
                resize_down: true
            })
        );
        let mut out = IniFile::parse("[REAPER]\n");
        g.write_to(&mut out);
        assert_eq!(out.get("REAPER", "fxresize"), Some("3"));
    }

    // ── NewProjectFlags ───────────────────────────────────────────────────

    #[test]
    fn new_project_flags_roundtrip() {
        assert_eq!(
            NewProjectFlags::from_u32(0),
            NewProjectFlags {
                prompt_to_save: false,
                open_properties: false
            }
        );
        assert_eq!(
            NewProjectFlags::from_u32(1),
            NewProjectFlags {
                prompt_to_save: true,
                open_properties: false
            }
        );
        assert_eq!(
            NewProjectFlags::from_u32(2),
            NewProjectFlags {
                prompt_to_save: false,
                open_properties: true
            }
        );
        assert_eq!(
            NewProjectFlags::from_u32(3),
            NewProjectFlags {
                prompt_to_save: true,
                open_properties: true
            }
        );
        assert_eq!(NewProjectFlags::default().to_u32(), 0);
    }

    #[test]
    fn new_project_flags_parse_write() {
        let ini = IniFile::parse("[REAPER]\nnewprojdo=2\n");
        let g = General::from_ini(&ini);
        assert_eq!(
            g.new_project_action,
            Some(NewProjectFlags {
                prompt_to_save: false,
                open_properties: true
            })
        );
        let mut out = IniFile::parse("[REAPER]\n");
        g.write_to(&mut out);
        assert_eq!(out.get("REAPER", "newprojdo"), Some("2"));
    }
}
